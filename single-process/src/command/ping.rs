use crate::{APPLICATION_ID, CONTEXT};
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::application_command::CommandData,
    },
    channel::message::MessageFlags,
    gateway::payload::incoming::InteractionCreate,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};
use twilight_util::builder::command::CommandBuilder;

pub const NAME: &str = "ping";

pub fn command() -> Command {
    CommandBuilder::new(NAME, "Ping the bot", CommandType::ChatInput).build()
}

pub async fn autocomplete(
    _event: Box<InteractionCreate>,
    _data: Box<CommandData>,
) -> anyhow::Result<()> {
    Ok(())
}

pub async fn run(event: Box<InteractionCreate>, _data: Box<CommandData>) -> anyhow::Result<()> {
    let data = InteractionResponseData {
        content: Some("Pong!".to_owned()),
        flags: Some(MessageFlags::EPHEMERAL),
        ..Default::default()
    };

    let response = InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(data),
    };
    CONTEXT
        .http
        .interaction(APPLICATION_ID)
        .create_response(event.id, &event.token, &response)
        .await?;

    Ok(())
}
