use crate::Cache;
use std::{ops::Deref, sync::OnceLock};
use twilight_http::Client;

pub static CONTEXT: Ref = Ref(OnceLock::new());

#[derive(Debug)]
pub struct Context {
    #[allow(dead_code)]
    pub cache: Cache,
    pub http: Client,
}

pub fn init(cache: Cache, http: Client) {
    let context = Context { cache, http };
    assert!(CONTEXT.0.set(context).is_ok());
}

pub struct Ref(OnceLock<Context>);

impl Deref for Ref {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        self.0.get().unwrap()
    }
}
