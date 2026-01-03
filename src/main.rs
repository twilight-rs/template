mod context;

use crate::context::CONTEXT;
use anyhow::Context;
use std::{env, error::Error, pin::pin};
use tokio::signal;
use tokio_util::task::TaskTracker;
use twilight_gateway::{CloseFrame, Config, Event, EventTypeFlags, Intents, Shard, StreamExt as _};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::MessageCreate;

const EVENT_TYPES: EventTypeFlags = EventTypeFlags::MESSAGE_CREATE;
const INTENTS: Intents = Intents::GUILD_MESSAGES.union(Intents::MESSAGE_CONTENT);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("TOKEN").context("reading `TOKEN`")?;

    let config = Config::new(token.clone(), INTENTS);
    let http = Client::new(token);
    let shards = twilight_gateway::create_recommended(&http, config, |_, builder| builder.build())
        .await
        .context("creating shards")?;
    context::initialize(http);

    let tasks = shards
        .into_iter()
        .map(|shard| tokio::spawn(dispatcher(shard)))
        .collect::<Vec<_>>();

    signal::ctrl_c().await?;
    tracing::info!("shutting down; press CTRL-C to abort");

    let join_all_tasks = async {
        for task in tasks {
            task.await?;
        }
        Ok::<_, anyhow::Error>(())
    };
    tokio::select! {
        _ = signal::ctrl_c() => {},
        _ = join_all_tasks => {},
    };

    Ok(())
}

#[tracing::instrument(fields(shard = %shard.id(), skip_all))]
async fn dispatcher(mut shard: Shard) {
    let mut is_shutdown = false;
    let tracker = TaskTracker::new();
    let mut shutdown_fut = pin!(signal::ctrl_c());

    loop {
        tokio::select! {
            _ = &mut shutdown_fut, if !is_shutdown => {
                shard.close(CloseFrame::NORMAL);
                is_shutdown = true;
            },
            Some(item) = shard.next_event(EVENT_TYPES) => {
                let event = match item {
                    Ok(event) => event,
                    Err(error) => {
                        tracing::warn!(error = &error as &dyn Error, "error receiving event");
                        continue;
                    }
                };
                let kind = event.kind();

                let handler = match event {
                    Event::GatewayClose(_) if is_shutdown => break,
                    Event::MessageCreate(event) => message(event),
                    _ => continue,
                };

                tracker.spawn(async move {
                    if let Err(error) = handler.await {
                        tracing::warn!(error = &*error, type = ?kind, "error handling event");
                    }
                });
            }
        }
    }

    tracker.close();
    tracker.wait().await;
}

#[tracing::instrument(fields(id = %event.id), skip_all)]
async fn message(event: Box<MessageCreate>) -> anyhow::Result<()> {
    #[allow(clippy::single_match)]
    match &*event.content {
        "!ping" => {
            tracing::debug!(channel_id = %event.channel_id, "received ping");
            CONTEXT
                .http
                .create_message(event.channel_id)
                .content("Pong!")
                .await?;
        }
        _ => {}
    }

    Ok(())
}
