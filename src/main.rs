mod command;
mod context;
mod dispatch;
mod resume;

pub use self::{
    context::CTX,
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
use twilight_model::id::{Id, marker::GuildMarker};

#[rustfmt::skip]
const ADMIN_GUILD_ID: Id<GuildMarker> = Id::new({{admin_guild_id}});
const EVENT_TYPES: EventTypeFlags = EventTypeFlags::INTERACTION_CREATE;
const INTENTS: Intents = Intents::empty();

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("TOKEN").context("reading `TOKEN`")?;

    let http = Client::new(token.clone());
    let app = async { anyhow::Ok(http.current_user_application().await?.model().await?) }
        .await
        .context("getting app")?;
    let info = async { anyhow::Ok(http.gateway().authed().await?.model().await?) }
        .await
        .context("getting info")?;

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

    async {
        http.interaction(app.id)
            .set_global_commands(&command::global_commands())
            .await?;
        http.interaction(app.id)
            .set_guild_commands(
                ADMIN_GUILD_ID,
                &command::admin_commands(shards.len() as u32),
            )
            .await?;
        anyhow::Ok(())
    }
    .await
    .context("putting commands")?;
    context::init(app.id, http, DashMap::with_capacity(shards.len()));

    let tasks = shards
        .into_iter()
        .map(|shard| tokio::spawn(dispatch::run(event_handler, shard, |_shard| ())))
        .collect::<Vec<_>>();

    signal::ctrl_c().await?;
    tracing::info!("shutting down; press CTRL-C to abort");

    let join_all_tasks = async {
        let mut resume_info = Vec::with_capacity(tasks.len());
        for task in tasks {
            resume_info.push(task.await?);
        }
        anyhow::Ok(resume_info)
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

async fn event_handler(event: Event, _state: ()) {
    async fn log_err(future: Instrumented<impl Future<Output = anyhow::Result<()>>>) {
        let mut future = pin!(future);
        if let Err(error) = future.as_mut().await {
            let _enter = future.span().enter();
            tracing::warn!(error = &*error, "failed to handle event");
        }
    }

    #[allow(clippy::single_match)]
    match event {
        Event::InteractionCreate(interaction) => {
            let span = tracing::info_span!("interaction", id = %interaction.id);
            log_err(command::handler(interaction).instrument(span)).await;
        }
        _ => {}
    }
}
