mod context;
mod forward;
mod resume;

pub(crate) use self::{context::CONTEXT, resume::Info as ResumeInfo};

use anyhow::Context as _;
use axum::{
    Json, Router,
    extract::Path,
    response::Result,
    routing::{any, get},
};
use std::{env, iter, time::Duration};
use tokio::{
    net::TcpListener,
    signal,
    sync::{Notify, broadcast},
};
use twilight_cache_inmemory::{InMemoryCache, ResourceType};
use twilight_gateway::{ConfigBuilder, Intents, queue::InMemoryQueue};
use twilight_http::Client;
use twilight_model::{
    guild::Role,
    id::{Id, marker::RoleMarker},
};

const SHARD_EVENT_BUFFER: usize = 16;
const INTENTS: Intents = Intents::GUILDS;
const RESOURCE_TYPES: ResourceType = ResourceType::ROLE;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    context::init(
        InMemoryCache::builder()
            .resource_types(RESOURCE_TYPES)
            .build(),
        Notify::new(),
    );

    let token = env::var("TOKEN").context("failed to get `TOKEN`")?;

    let http = Client::new(token.clone());
    let info = async { Ok::<_, anyhow::Error>(http.gateway().authed().await?.model().await?) }
        .await
        .context("failed to get info")?;

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

    let (event_tx, _) = broadcast::channel(shards.len() * SHARD_EVENT_BUFFER);
    let router = Router::new()
        .route(
            "/events",
            any({
                let weak = event_tx.downgrade();
                move |body| forward::socket(body, weak)
            }),
        )
        .route("/roles/{id}", get(get_role));

    tokio::spawn({
        let listener = TcpListener::bind("[::1]:3000").await?;
        async { axum::serve(listener, router).await.unwrap() }
    });
    let tasks = iter::repeat_n(event_tx, shards.len())
        .zip(shards)
        .map(|(event_tx, shard)| tokio::spawn(forward::shard(event_tx, shard)))
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
        .context("failed to save resume info")?;

    Ok(())
}

async fn get_role(Path(role_id): Path<Id<RoleMarker>>) -> Result<Json<Role>> {
    let role = CONTEXT.cache.role(role_id).ok_or("not found")?;
    Ok(Json(role.value().resource().clone()))
}
