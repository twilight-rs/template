use crate::ShardHandle;
use dashmap::DashMap;
use std::{ops::Deref, sync::OnceLock};
use twilight_http::Client;

pub static CONTEXT: Ref = Ref(OnceLock::new());

#[derive(Debug)]
pub struct Context {
    pub http: Client,
    pub shard_handles: DashMap<u32, ShardHandle>,
}

pub fn initialize(http: Client, shard_handles: DashMap<u32, ShardHandle>) {
    let context = Context {
        http,
        shard_handles,
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
