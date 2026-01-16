use std::{ops::Deref, sync::OnceLock};
use tokio::sync::Notify;
use twilight_cache_inmemory::InMemoryCache;

pub static CONTEXT: Ref = Ref(OnceLock::new());

#[derive(Debug)]
pub struct Context {
    pub cache: InMemoryCache,
    pub notify: Notify,
}

pub fn init(cache: InMemoryCache, notify: Notify) {
    let context = Context { cache, notify };
    assert!(CONTEXT.0.set(context).is_ok());
}

pub struct Ref(OnceLock<Context>);

impl Deref for Ref {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        self.0.get().unwrap()
    }
}
