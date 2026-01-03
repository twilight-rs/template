mod context;
mod resume;

use crate::context::CONTEXT;
use anyhow::Context as _;
use std::{env, error::Error, pin::pin, time::Duration};
use tokio::signal;
use tokio_util::task::TaskTracker;
use twilight_gateway::{
    CloseFrame, ConfigBuilder, Event, EventTypeFlags, Intents, Shard, StreamExt as _,
    queue::InMemoryQueue,
};
use twilight_http::Client;
use twilight_model::gateway::payload::incoming::MessageCreate;

const EVENT_TYPES: EventTypeFlags = EventTypeFlags::MESSAGE_CREATE;
const INTENTS: Intents = Intents::GUILD_MESSAGES.union(Intents::MESSAGE_CONTENT);

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("TOKEN").context("reading `TOKEN`")?;

    let http = Client::new(token.clone());
    let info = async { Ok::<_, anyhow::Error>(http.gateway().authed().await?.model().await?) }
        .await
        .context("getting info")?;
    context::initialize(http);

    // The queue defaults are static and may be incorrect for large or newly
    // restarted bots.
    let queue = InMemoryQueue::new(
        info.session_start_limit.max_concurrency,
        info.session_start_limit.remaining,
        Duration::from_millis(info.session_start_limit.reset_after),
        info.session_start_limit.total,
    );
    let config = ConfigBuilder::new(token, INTENTS).queue(queue).build();

    let shards = resume::restore(config, info.shards).await;

    let tasks = shards
        .into_iter()
        .map(|shard| tokio::spawn(dispatcher(shard)))
        .collect::<Vec<_>>();

    signal::ctrl_c().await?;
    tracing::info!("shutting down; press CTRL-C to abort");

    let join_all_tasks = async {
        let mut resume_info = Vec::new();
        for task in tasks {
            resume_info.push(task.await?);
        }
        Ok::<_, anyhow::Error>(resume_info)
    };
    let resume_info = tokio::select! {
        _ = signal::ctrl_c() => Vec::new(),
        resume_info = join_all_tasks => resume_info?,
    };

    // Save shard information to be restored.
    resume::save(&resume_info)
        .await
        .context("saving resume info")?;

    Ok(())
}

#[tracing::instrument(fields(shard = %shard.id(), skip_all))]
async fn dispatcher(mut shard: Shard) -> resume::Info {
    let mut is_shutdown = false;
    let tracker = TaskTracker::new();
    let mut shutdown_fut = pin!(signal::ctrl_c());

    loop {
        tokio::select! {
            _ = &mut shutdown_fut, if !is_shutdown => {
                shard.close(CloseFrame::RESUME);
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

    resume::Info::from(&shard)
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
