use crate::{CONTEXT, ResumeInfo};
use anyhow::anyhow;
use axum::{
    body::Bytes,
    extract::{WebSocketUpgrade, ws::Message as SocketMessage},
    response::Response,
};
use serde::de::DeserializeSeed as _;
use std::{collections::VecDeque, error::Error, pin::pin};
use tokio::{
    signal,
    sync::broadcast::{self, error::RecvError},
};
use tokio_stream::StreamExt as _;
use twilight_gateway::{CloseFrame, Event, EventTypeFlags, Message as ShardMessage, Shard};
use twilight_model::gateway::{OpCode, event::GatewayEventDeserializer};

const BUFFER_LIMIT: usize = 1000;
const EVENT_TYPES: EventTypeFlags = EventTypeFlags::INTERACTION_CREATE
    .union(EventTypeFlags::READY)
    .union(EventTypeFlags::ROLE_CREATE)
    .union(EventTypeFlags::ROLE_DELETE)
    .union(EventTypeFlags::ROLE_UPDATE);

fn parse(input: &str) -> anyhow::Result<Option<Event>> {
    let deserializer =
        GatewayEventDeserializer::from_json(input).ok_or_else(|| anyhow!("missing opcode"))?;
    let opcode = OpCode::from(deserializer.op()).ok_or_else(|| anyhow!("unknown opcode"))?;
    let event_type = EventTypeFlags::try_from((opcode, deserializer.event_type()))
        .map_err(|_| anyhow!("missing event type"))?;

    Ok(EVENT_TYPES
        .contains(event_type)
        .then(|| {
            let mut json_deserializer = serde_json::Deserializer::from_str(input);
            deserializer.deserialize(&mut json_deserializer)
        })
        .transpose()?
        .map(Into::into))
}

enum ShardState {
    Active,
    Shutdown,
}

impl ShardState {
    fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    fn is_shutdown(&self) -> bool {
        matches!(self, Self::Shutdown)
    }
}

#[tracing::instrument(fields(id = shard.id().number()), skip_all)]
pub async fn shard(event_tx: broadcast::Sender<Bytes>, mut shard: Shard) -> ResumeInfo {
    let mut notified = None;
    let mut buffer = VecDeque::new();
    let mut shutdown = pin!(signal::ctrl_c());
    let mut state = ShardState::Active;

    loop {
        tokio::select! {
            _ = notified.as_mut().unwrap(), if notified.is_some() => {
                notified = None;
                loop {
                    let inner = CONTEXT.notify.notified();
                    match event_tx.send(buffer.pop_front().unwrap()) {
                        Ok(_) if buffer.is_empty() => break,
                        Ok(_) => {}
                        Err(error) => {
                            notified = Some(Box::pin(inner));
                            buffer.push_front(error.0);
                            break;
                        }
                    }
                }
            }
            _ = &mut shutdown, if !state.is_shutdown() => {
                if state.is_active() {
                    shard.close(CloseFrame::RESUME);
                }
                state = ShardState::Shutdown;
            }
            event = shard.next() => {
                match event {
                    Some(Ok(ShardMessage::Close(_))) if !state.is_active() => break,
                    Some(Ok(ShardMessage::Close(_))) => {}
                    Some(Ok(ShardMessage::Text(json))) => {
                        match parse(&json) {
                            Ok(Some(event)) => {
                                CONTEXT.cache.update(&event);

                                let inner = notified.is_none().then(|| CONTEXT.notify.notified());
                                if let Err(error) = event_tx.send(json.into()) {
                                    if let Some(inner) = inner {
                                        notified = Some(Box::pin(inner));
                                    }

                                    if buffer.len() == BUFFER_LIMIT {
                                        buffer.pop_front();
                                    }
                                    buffer.push_back(error.0);
                                }
                            }
                            Ok(_) => {}
                            Err(error) => tracing::warn!(error = &*error, "failed to deserialize event"),
                        }
                    }
                    Some(Err(error)) => tracing::warn!(error = &error as &dyn Error, "shard failed to receive event"),
                    None => break,
                }
            }
        }
    }

    return ResumeInfo::from(&shard);
}

#[tracing::instrument(skip_all)]
pub async fn socket(ws: WebSocketUpgrade, weak: broadcast::WeakSender<Bytes>) -> Response {
    ws.on_upgrade(async move |mut socket| {
        if let Some(mut event_rx) = weak.upgrade().map(|tx| tx.subscribe()) {
            tracing::info!("worker connected");
            CONTEXT.notify.notify_waiters();

            loop {
                tokio::select! {
                    message = socket.recv() => {
                        match message {
                            Some(Ok(SocketMessage::Close(_))) => return,
                            Some(Err(error)) => tracing::warn!(error = &error as &dyn Error, "socket failed to receive event"),
                            None => return,
                            _ => {}
                        }
                    }
                    event = event_rx.recv() => {
                        match event {
                            Ok(event) => {
                                if let Err(error) = socket.send(SocketMessage::Text(event.try_into().unwrap())).await {
                                    tracing::warn!(error = &error as &dyn Error, "socket failed to send event");
                                    return;
                                }
                            }
                            Err(RecvError::Closed) => return,
                            Err(RecvError::Lagged(count)) => tracing::warn!("socket lagged {count} events"),
                        }
                    }
                }
            }
        }

        // Drive socket to completion.
        while socket.recv().await.is_some() {}
    })
}
