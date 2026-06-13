mod ping;
mod restart;

use crate::{ADMIN_GUILD_ID, CTX};
use twilight_model::{
    application::interaction::{InteractionData, InteractionType},
    gateway::payload::incoming::InteractionCreate,
};

pub async fn register() -> anyhow::Result<()> {
    CTX.interaction()
        .set_global_commands(&[ping::command()])
        .await?;
    CTX.interaction()
        .set_guild_commands(
            ADMIN_GUILD_ID,
            &[restart::command(CTX.shards.capacity() as u32)],
        )
        .await?;
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Ping,
    Restart,
}

impl From<&str> for Kind {
    fn from(name: &str) -> Self {
        match name {
            ping::NAME => Kind::Ping,
            restart::NAME => Kind::Restart,
            _ => panic!("unknown command name: '{name}'"),
        }
    }
}

pub async fn handler(mut interaction: Box<InteractionCreate>) -> anyhow::Result<()> {
    match interaction.kind {
        InteractionType::ApplicationCommandAutocomplete => {
            let InteractionData::ApplicationCommand(data) = interaction.data.take().unwrap() else {
                unreachable!();
            };
            let kind = data.name.as_str().into();

            match kind {
                Kind::Ping => ping::autocomplete(interaction, data).await?,
                Kind::Restart => restart::autocomplete(interaction, data).await?,
            }
        }
        InteractionType::ApplicationCommand => {
            let InteractionData::ApplicationCommand(data) = interaction.data.take().unwrap() else {
                unreachable!();
            };
            let kind = data.name.as_str().into();

            match kind {
                Kind::Ping => ping::run(interaction, data).await?,
                Kind::Restart => restart::run(interaction, data).await?,
            }
        }
        _ => {}
    }

    Ok(())
}
