#![allow(dead_code)]
use reqwest::{Client, IntoUrl, Response, Result};
use twilight_model::{
    guild::Role,
    id::{Id, marker::RoleMarker},
};

#[derive(Debug)]
pub struct Cache(Client);

impl Cache {
    pub fn new() -> Self {
        Self(Client::new())
    }

    async fn get(&self, url: impl IntoUrl) -> Result<Response> {
        self.0.get(url).send().await
    }

    pub async fn role(&self, role_id: Id<RoleMarker>) -> Result<Role> {
        let url = format!("http://[::1]:3000/roles/{role_id}");
        self.get(url).await?.json().await
    }
}
