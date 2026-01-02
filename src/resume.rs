use serde::{Deserialize, Serialize};
use tokio::fs;
use twilight_gateway::{Config, ConfigBuilder, Session, Shard, ShardId};

const INFO_FILE: &str = "resume-info.json";

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
pub async fn save(info: &[Info]) -> anyhow::Result<()> {
    if !info.iter().all(Info::is_none) {
        let contents = serde_json::to_vec(&info)?;
        fs::write(INFO_FILE, contents).await?;
    }

    Ok(())
}

/// Restores shard resumption information from the file system.
pub async fn restore(config: Config, shards: u32) -> Vec<Shard> {
    let info = async {
        let contents = fs::read(INFO_FILE).await?;
        Ok::<_, anyhow::Error>(serde_json::from_slice::<Vec<Info>>(&contents)?)
    }
    .await;

    let shard_ids = (0..shards).map(|shard| ShardId::new(shard, shards));

    // A session may only successfully be resumed if it retains its shard ID,
    // but Discord may have recommend a different shard count (producing
    // different shard IDs).
    let shards: Vec<_> = if let Ok(info) = info
        && info.len() == shards as usize
    {
        tracing::info!("resuming previous gateway sessions");
        shard_ids
            .zip(info)
            .map(|(shard_id, info)| {
                let mut builder = ConfigBuilder::from(config.clone());

                if let Some(resume_url) = info.resume_url {
                    builder = builder.resume_url(resume_url);
                }
                if let Some(session) = info.session {
                    builder = builder.session(session);
                }

                Shard::with_config(shard_id, builder.build())
            })
            .collect()
    } else {
        shard_ids
            .map(|shard_id| Shard::with_config(shard_id, config.clone()))
            .collect()
    };

    // Resumed or not, the saved resume info is now stale.
    _ = fs::remove_file(INFO_FILE).await;

    shards
}
