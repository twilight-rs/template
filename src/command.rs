mod ping;
mod restart;

use twilight_model::{
    application::{
        command::Command,
        interaction::{InteractionData, InteractionType},
    },
    gateway::payload::incoming::InteractionCreate,
};

pub fn admin_commands(shards: u32) -> [Command; 1] {
    [restart::command(shards)]
}

pub fn global_commands() -> [Command; 1] {
    [ping::command()]
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
