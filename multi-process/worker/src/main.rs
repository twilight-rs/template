mod cache;
mod command;
mod context;
mod dispatch;

pub(crate) use self::{cache::Cache, context::CONTEXT};

use anyhow::Context as _;
use std::{env, pin::pin};
use tokio::signal;
use tokio_websockets::{ClientBuilder, Limits};
use tracing::{Instrument as _, instrument::Instrumented};
use twilight_http::Client;
use twilight_model::{
    gateway::event::Event,
    id::{Id, marker::ApplicationMarker},
};

#[rustfmt::skip]
const APPLICATION_ID: Id<ApplicationMarker> = Id::new({{application_id}});

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let token = env::var("TOKEN").context("failed to get `TOKEN`")?;

    let http = Client::new(token.clone());
    http.interaction(APPLICATION_ID)
        .set_global_commands(&command::commands())
        .await
        .context("failed to put commands")?;
    context::init(Cache::new(), http);

    let task = tokio::spawn({
        let (socket, _) = ClientBuilder::new()
            .limits(Limits::unlimited())
            .uri("ws://[::1]:3000/events")?
            .connect()
            .await?;
        dispatch::run(event_handler, socket)
    });

    signal::ctrl_c().await?;
    tracing::info!("shutting down; press CTRL-C to abort");

    tokio::select! {
        _ = signal::ctrl_c() => {},
        result = task => result?,
    }

    Ok(())
}

async fn event_handler(event: Event) {
    async fn log_err(future: Instrumented<impl Future<Output = anyhow::Result<()>>>) {
        let mut future = pin!(future);
        if let Err(error) = future.as_mut().await {
            let _enter = future.span().enter();
            tracing::warn!(error = &*error, "failed to handle event");
        }
    }

    #[allow(clippy::single_match)]
    match event {
        Event::InteractionCreate(event) => {
            let span = tracing::info_span!("interaction", id = %event.id);
            log_err(command::interaction(event).instrument(span)).await;
        }
        _ => {}
    }
}
