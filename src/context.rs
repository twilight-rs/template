use crate::ShardHandle;
use dashmap::DashMap;
use std::{ops::Deref, sync::OnceLock};
use twilight_http::Client;
use twilight_standby::Standby;

pub static CONTEXT: Ref = Ref(OnceLock::new());

#[derive(Debug)]
pub struct Context {
    pub http: Client,
    pub shard_handles: DashMap<u32, ShardHandle>,
    pub standby: Standby,
}

pub fn initialize(http: Client, shard_handles: DashMap<u32, ShardHandle>, standby: Standby) {
    let context = Context {
        http,
        shard_handles,
        standby,
    };
    assert!(CONTEXT.0.set(context).is_ok());
}

pub struct Ref(OnceLock<Context>);

impl Deref for Ref {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        self.0.get().unwrap()
    }
}
