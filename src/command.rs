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
enum Type {
    Ping,
    Restart,
}

impl From<&str> for Type {
    fn from(value: &str) -> Self {
        match value {
            ping::NAME => Type::Ping,
            restart::NAME => Type::Restart,
            _ => panic!("unknown command name: '{value}'"),
        }
    }
}

pub async fn interaction(mut event: Box<InteractionCreate>) -> anyhow::Result<()> {
    match event.kind {
        InteractionType::ApplicationCommandAutocomplete => {
            let InteractionData::ApplicationCommand(data) = event.data.take().unwrap() else {
                unreachable!();
            };
            let kind = data.name.as_str().into();

            match kind {
                Type::Ping => ping::autocomplete(event, data).await?,
                Type::Restart => restart::autocomplete(event, data).await?,
            }
        }
        InteractionType::ApplicationCommand => {
            let InteractionData::ApplicationCommand(data) = event.data.take().unwrap() else {
                unreachable!();
            };
            let kind = data.name.as_str().into();

            match kind {
                Type::Ping => ping::run(event, data).await?,
                Type::Restart => restart::run(event, data).await?,
            }
        }
        _ => {}
    }

    Ok(())
}
