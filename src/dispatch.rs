use crate::{CONTEXT, ConfigBuilderExt as _, EVENT_TYPES, ResumeInfo};
use std::{error::Error, pin::pin};
use tokio::{signal, sync::watch};
use tokio_util::task::TaskTracker;
use twilight_gateway::{CloseFrame, ConfigBuilder, Event, Shard, ShardId, StreamExt as _};

#[derive(Clone, Copy, Debug)]
pub enum ShardRestartResult {
    ForcedRestart,
    Restarted,
}

impl ShardRestartResult {
    pub fn is_forced(self) -> bool {
        matches!(self, Self::ForcedRestart)
    }
}

/// Handle for a [`Shard`].
#[derive(Clone, Debug)]
pub struct ShardHandle(watch::Sender<Option<ShardRestartKind>>);

impl ShardHandle {
    fn insert(shard_id: ShardId) -> watch::Receiver<Option<ShardRestartKind>> {
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
    pub fn restart(&self, kind: ShardRestartKind) -> ShardRestartResult {
        match self.0.send_replace(Some(kind)) {
            Some(_) => ShardRestartResult::ForcedRestart,
            None => ShardRestartResult::Restarted,
        }
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
pub enum ShardRestartKind {
    Normal,
    Resume,
}

impl From<ShardRestartKind> for CloseFrame<'_> {
    fn from(value: ShardRestartKind) -> Self {
        match value {
            ShardRestartKind::Normal => Self::NORMAL,
            ShardRestartKind::Resume => Self::RESUME,
        }
    }
}

enum State {
    Active,
    Restart,
    Shutdown,
}

impl State {
    fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }

    fn is_restart(&self) -> bool {
        matches!(self, Self::Restart)
    }

    fn is_shutdown(&self) -> bool {
        matches!(self, Self::Shutdown)
    }
}

#[tracing::instrument(name = "dispatcher", fields(shard.id = shard.id().number()), skip_all)]
pub async fn run<Fut: Future<Output = ()> + Send + 'static, S>(
    mut event_handler: impl FnMut(Event, S) -> Fut,
    mut shard: Shard,
    mut shard_state_provider: impl FnMut(&mut Shard) -> S,
) -> ResumeInfo {
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
                    if state.is_restart() {
                        break;
                    }
                    shard.close(receiver.borrow().unwrap().into());
                    state = State::Restart;
                }
                event = shard.next_event(EVENT_TYPES) => {
                    match event {
                        Some(Ok(Event::GatewayClose(_))) if !state.is_active() => break,
                        Some(Ok(event)) => _ = tracker.spawn(event_handler(event, shard_state_provider(&mut shard))),
                        Some(Err(error)) => tracing::warn!(error = &error as &dyn Error, "shard failed to receive event"),
                        None => break,
                    }
                }
            }
        }

        let resume_info = ResumeInfo::from(&shard);
        if state.is_restart() {
            receiver = ShardHandle::insert(shard.id());
            let builder = ConfigBuilder::from(shard.config().clone()).resume_info(resume_info);
            shard = Shard::with_config(shard.id(), builder.build());
        } else {
            tracker.close();
            tracing::info!("waiting for {} task(s) to finish", tracker.len());
            tracker.wait().await;
            return resume_info;
        }
    }
}
