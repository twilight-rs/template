use futures_util::SinkExt as _;
use serde::de::DeserializeSeed as _;
use std::{error::Error, pin::pin};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    signal,
};
use tokio_stream::StreamExt as _;
use tokio_util::task::TaskTracker;
use tokio_websockets::{Message, WebSocketStream};
use twilight_model::gateway::event::{Event, GatewayEventDeserializer};

enum State {
    Active,
    Shutdown,
}

impl State {
    fn is_shutdown(&self) -> bool {
        matches!(self, Self::Shutdown)
    }
}

#[tracing::instrument(name = "dispatcher", skip_all)]
pub async fn run<Fut: Future<Output = ()> + Send + 'static>(
    mut event_handler: impl FnMut(Event) -> Fut,
    mut socket: WebSocketStream<impl AsyncRead + AsyncWrite + Unpin>,
) {
    let mut shutdown = pin!(signal::ctrl_c());
    let mut state = State::Active;
    let tracker = TaskTracker::new();

    loop {
        tokio::select! {
            _ = &mut shutdown, if !state.is_shutdown() => {
                if let Err(error) = socket.send(Message::close(None, "")).await {
                    tracing::warn!(error = &error as &dyn Error, "socket failed to send message");
                }
                state = State::Shutdown;
            }
            message = socket.next() => {
                match message {
                    Some(Ok(message)) => {
                        if message.is_text() {
                            let event = message.as_text().unwrap();
                            let Some(deserializer) = GatewayEventDeserializer::from_json(event) else {
                                tracing::warn!(event, "failed to deserialize event");
                                continue;
                            };
                            let mut json_deserializer = serde_json::Deserializer::from_str(event);
                            match deserializer.deserialize(&mut json_deserializer) {
                                Ok(event) => _ = tracker.spawn(event_handler(event.into())),
                                Err(error) => tracing::warn!(error = &error as &dyn Error, "failed to deserialize event"),
                            }
                        }
                    }
                    Some(Err(error)) => tracing::warn!(error = &error as &dyn Error, "socket failed to receive message"),
                    None => {
                        tracing::info!("gateway shut down");
                        break;
                    },
                }
            }
        }
    }

    if let Err(error) = socket.close().await {
        tracing::warn!(error = &error as &dyn Error, "socket failed to close");
    }

    tracker.close();
    tracing::info!("waiting for {} task(s) to finish", tracker.len());
    tracker.wait().await;
}
