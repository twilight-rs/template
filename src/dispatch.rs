use crate::{CONTEXT, EVENT_TYPES, ResumeInfo};
use std::{error::Error, pin::pin};
use tokio::{signal, sync::watch};
use tokio_util::task::TaskTracker;
use tracing::instrument::Instrumented;
use twilight_gateway::{CloseFrame, Event, Shard, ShardId, StreamExt as _};

pub struct ShardRestartResult(bool);

impl ShardRestartResult {
    pub fn is_forced(&self) -> bool {
        self.0
    }
}

/// Handle for a [`Shard`].
#[derive(Clone, Debug)]
pub struct ShardHandle(watch::Sender<Option<ShardRestartType>>);

impl ShardHandle {
    fn insert(shard_id: ShardId) -> watch::Receiver<Option<ShardRestartType>> {
        let (tx, rx) = watch::channel(None);
        CONTEXT
            .shard_handles
            .insert(shard_id.number(), ShardHandle(tx));

        rx
    }

    /// Whether or not the handle is valid.
    ///
    /// Handles are invalidated after their shard restarts or shuts down.
    pub fn is_valid(&self) -> bool {
        !self.0.is_closed()
    }

    /// Instructs the shard to restart, or force restart if called multiple times.
    pub fn restart(&self, kind: ShardRestartType) -> ShardRestartResult {
        ShardRestartResult(self.0.send_replace(Some(kind)).is_some())
    }

    /// Completes when the shard was restarted or shutdown.
    ///
    /// To check the result of whether the shard was restarted or shutdown, retrieve
    /// a fresh handle and check whether it's valid. It will be valid if the shard
    /// was restarted.
    pub async fn restarted(self) {
        self.0.closed().await;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ShardRestartType {
    Normal,
    Resume,
}

impl From<ShardRestartType> for CloseFrame<'_> {
    fn from(value: ShardRestartType) -> Self {
        match value {
            ShardRestartType::Normal => Self::NORMAL,
            ShardRestartType::Resume => Self::RESUME,
        }
    }
}

enum State {
    Active,
    RestartShard,
    Shutdown,
}

impl State {
    fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    fn is_restart_shard(&self) -> bool {
        matches!(self, Self::RestartShard)
    }

    fn is_shutdown(&self) -> bool {
        matches!(self, Self::Shutdown)
    }
}

pub struct Dispatcher<'a> {
    #[allow(dead_code)]
    pub shard: &'a Shard,
    tracker: &'a TaskTracker,
}

impl<'a> Dispatcher<'a> {
    fn new(shard: &'a Shard, tracker: &'a TaskTracker) -> Self {
        Self { shard, tracker }
    }

    pub fn dispatch(
        &self,
        future: Instrumented<impl Future<Output = anyhow::Result<()>> + Send + 'static>,
    ) {
        self.tracker.spawn(async move {
            let mut future = pin!(future);
            if let Err(error) = future.as_mut().await {
                let _enter = future.span().enter();
                tracing::warn!(error = &*error, "event handler failed");
            }
        });
    }
}

#[tracing::instrument(name = "dispatcher", fields(shard.id = shard.id().number()), skip_all)]
pub async fn run(mut shard: Shard, mut event_handler: impl FnMut(Dispatcher, Event)) -> ResumeInfo {
    let mut receiver = ShardHandle::insert(shard.id());
    let mut shutdown = pin!(signal::ctrl_c());
    let tracker = TaskTracker::new();

    loop {
        let mut state = State::Active;
        loop {
            tokio::select! {
                _ = &mut shutdown, if !state.is_shutdown() => {
                    if state.is_active() {
                        shard.close(CloseFrame::RESUME);
                    }
                    state = State::Shutdown;
                }
                _ = receiver.changed(), if !state.is_shutdown() => {
                    if state.is_restart_shard() {
                        break;
                    }
                    shard.close(receiver.borrow().unwrap().into());
                    state = State::RestartShard;
                }
                item = shard.next_event(EVENT_TYPES) => {
                    match item {
                        Some(Ok(Event::GatewayClose(_))) if !state.is_active() => break,
                        Some(Ok(event)) => event_handler(Dispatcher::new(&shard, &tracker), event),
                        Some(Err(error)) => {
                            tracing::warn!(error = &error as &dyn Error, "shard failed to receive an event");
                            continue;
                        }
                        None => break,
                    }
                }
            }
        }

        if state.is_restart_shard() {
            receiver = ShardHandle::insert(shard.id());
            shard = Shard::with_config(shard.id(), shard.config().clone());
        } else {
            tracker.close();
            tracker.wait().await;
            return ResumeInfo::from(&shard);
        }
    }
}
