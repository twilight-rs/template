use either::Either;
use serde::{Deserialize, Serialize};
use std::{fs, iter};
use twilight_gateway::{Config, ConfigBuilder, Session, Shard, ShardId};

const INFO_FILE: &str = "resume-info.json";

pub trait ConfigBuilderExt {
    fn resume_info(self, resume_info: Info) -> Self;
}

impl ConfigBuilderExt for ConfigBuilder {
    fn resume_info(mut self, resume_info: Info) -> Self {
        if let Some(resume_url) = resume_info.resume_url {
            self = self.resume_url(resume_url);
        }
        if let Some(session) = resume_info.session {
            self = self.session(session);
        }

        self
    }
}

/// [`Shard`] session resumption information.
#[derive(Debug, Deserialize, Serialize)]
pub struct Info {
    resume_url: Option<String>,
    session: Option<Session>,
}

impl Info {
    fn is_none(&self) -> bool {
        self.resume_url.is_none() && self.session.is_none()
    }
}

impl From<&Shard> for Info {
    fn from(value: &Shard) -> Self {
        Self {
            resume_url: value.resume_url().map(ToOwned::to_owned),
            session: value.session().cloned(),
        }
    }
}

/// Saves shard resumption information to the file system.
pub fn save(info: &[Info]) -> anyhow::Result<()> {
    if !info.iter().all(Info::is_none) {
        let contents = serde_json::to_vec(&info)?;
        fs::write(INFO_FILE, contents)?;
    }

    Ok(())
}

/// Restores shard resumption information from the file system.
pub fn restore(config: Config, recommended_shards: u32) -> impl ExactSizeIterator<Item = Shard> {
    let info = (|| {
        let contents = fs::read(INFO_FILE)?;
        anyhow::Ok(serde_json::from_slice::<Box<[_]>>(&contents)?)
    })();

    // The recommended shard count targets 1000 guilds per shard (out of a maximum
    // of 2500), so it might be different from the previous shard count.
    let configs = if let Ok(info) = info
        && recommended_shards / 2 <= info.len() as u32
    {
        tracing::info!("resuming previous gateway sessions");
        Either::Left(
            iter::repeat_n(config, info.len())
                .zip(info)
                .map(|(config, info)| ConfigBuilder::from(config).resume_info(info).build()),
        )
    } else {
        Either::Right(iter::repeat_n(config, recommended_shards as usize))
    };

    // Resumed or not, the saved resume info is now stale.
    _ = fs::remove_file(INFO_FILE);

    let total = configs.len() as u32;
    configs
        .zip((0..total).map(move |id| ShardId::new(id, total)))
        .map(|(config, shard_id)| Shard::with_config(shard_id, config))
}
