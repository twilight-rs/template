mod command;
mod context;
mod dispatch;
mod resume;

pub(crate) use self::{
    context::CONTEXT,
    dispatch::{ShardHandle, ShardRestartKind},
    resume::{ConfigBuilderExt, Info as ResumeInfo},
};

use anyhow::Context as _;
use dashmap::DashMap;
use std::{env, pin::pin, time::Duration};
use tokio::signal;
use tracing::{Instrument as _, instrument::Instrumented};
use twilight_gateway::{ConfigBuilder, Event, EventTypeFlags, Intents, queue::InMemoryQueue};
use twilight_http::Client;
use twilight_model::id::{
    Id,
    marker::{ApplicationMarker, GuildMarker},
};

#[rustfmt::skip]
const ADMIN_GUILD_ID: Id<GuildMarker> = Id::new({{admin_guild_id}});
#[rustfmt::skip]
const APPLICATION_ID: Id<ApplicationMarker> = Id::new({{application_id}});
const EVENT_TYPES: EventTypeFlags = EventTypeFlags::INTERACTION_CREATE;
const INTENTS: Intents = Intents::empty();

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("TOKEN").context("reading `TOKEN`")?;

    let http = Client::new(token.clone());
    let info = async { Ok::<_, anyhow::Error>(http.gateway().authed().await?.model().await?) }
        .await
        .context("getting info")?;
    async {
        http.interaction(APPLICATION_ID)
            .set_global_commands(&command::global_commands())
            .await?;
        http.interaction(APPLICATION_ID)
            .set_guild_commands(ADMIN_GUILD_ID, &command::admin_commands(info.shards))
            .await?;
        Ok::<_, anyhow::Error>(())
    }
    .await
    .context("putting commands")?;
    let shard_handles = DashMap::new();
    context::initialize(http, shard_handles);

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
        .map(|shard| tokio::spawn(dispatch::run(shard, event_handler)))
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

fn event_handler(dispatcher: dispatch::Dispatcher, event: Event) {
    async fn log_err(future: Instrumented<impl Future<Output = anyhow::Result<()>>>) {
        let mut future = pin!(future);
        if let Err(error) = future.as_mut().await {
            let _enter = future.span().enter();
            tracing::warn!(error = &*error, "event handler failed");
        }
    }

    #[allow(clippy::single_match)]
    match event {
        Event::InteractionCreate(event) => {
            let span = tracing::info_span!(parent: None, "interaction", id = %event.id);
            dispatcher.dispatch(log_err(command::interaction(event).instrument(span)))
        }
        _ => {}
    }
}
