mod ping;

use twilight_model::{
    application::{
        command::Command,
        interaction::{InteractionData, InteractionType},
    },
    gateway::payload::incoming::InteractionCreate,
};

pub fn commands() -> [Command; 1] {
    [ping::command()]
}

#[derive(Clone, Copy, Debug)]
enum Kind {
    Ping,
}

impl From<&str> for Kind {
    fn from(name: &str) -> Self {
        match name {
            ping::NAME => Kind::Ping,
            _ => panic!("unknown command name: '{name}'"),
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
                Kind::Ping => ping::autocomplete(event, data).await?,
            }
        }
        InteractionType::ApplicationCommand => {
            let InteractionData::ApplicationCommand(data) = event.data.take().unwrap() else {
                unreachable!();
            };
            let kind = data.name.as_str().into();

            match kind {
                Kind::Ping => ping::run(event, data).await?,
            }
        }
        _ => {}
    }

    Ok(())
}
