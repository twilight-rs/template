use crate::CTX;
use twilight_model::{
    application::{
        command::{Command, CommandType},
        interaction::application_command::CommandData,
    },
    channel::message::MessageFlags,
    gateway::payload::incoming::InteractionCreate,
};
use twilight_util::builder::{
    command::CommandBuilder, interaction_response::ChannelMessageBuilder,
};

pub const NAME: &str = "ping";

pub fn command() -> Command {
    CommandBuilder::new(NAME, "Ping the bot", CommandType::ChatInput).build()
}

pub async fn autocomplete(
    _interaction: Box<InteractionCreate>,
    _data: Box<CommandData>,
) -> anyhow::Result<()> {
    Ok(())
}

pub async fn run(
    interaction: Box<InteractionCreate>,
    _data: Box<CommandData>,
) -> anyhow::Result<()> {
    let response = ChannelMessageBuilder::new()
        .content("Pong!")
        .flags(MessageFlags::EPHEMERAL)
        .build();
    CTX.interaction()
        .create_response(interaction.id, &interaction.token, &response)
        .await?;

    Ok(())
}
