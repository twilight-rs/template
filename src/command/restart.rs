use crate::{CTX, ShardRestartKind};
use std::iter;
use twilight_model::{
    application::{
        command::{Command, CommandOptionChoice, CommandOptionChoiceValue, CommandType},
        interaction::application_command::{CommandData, CommandDataOption, CommandOptionValue},
    },
    gateway::payload::incoming::InteractionCreate,
    guild::Permissions,
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};
use twilight_util::builder::command::{BooleanBuilder, CommandBuilder, IntegerBuilder};

pub const NAME: &str = "restart";

pub fn command(shards: u32) -> Command {
    CommandBuilder::new(NAME, "Restart a shard", CommandType::ChatInput)
        .default_member_permissions(Permissions::empty())
        .option(
            IntegerBuilder::new("id", "Shard ID")
                .autocomplete(true)
                .max_value(shards as i64 - 1)
                .min_value(0)
                .required(true),
        )
        .option(BooleanBuilder::new(
            "resume",
            "Resume session? [default: false]",
        ))
        .build()
}

pub async fn autocomplete(
    interaction: Box<InteractionCreate>,
    mut data: Box<CommandData>,
) -> anyhow::Result<()> {
    let choice = |shard_id: u32| CommandOptionChoice {
        name: shard_id.to_string(),
        name_localizations: None,
        value: CommandOptionChoiceValue::Integer(shard_id.into()),
    };

    let mut options = data.options.drain(..);
    let CommandOptionValue::Focused(value, _) = options.next().unwrap().value else {
        unreachable!()
    };

    let choices = match value.parse() {
        Ok(shard_id) if shard_id == 0 => vec![choice(shard_id)],
        Ok(shard_id) => starts_with(shard_id, CTX.shards.len() as u32)
            .take(25)
            .map(choice)
            .collect(),
        Err(_) => (0..25.min(CTX.shards.len() as u32)).map(choice).collect(),
    };
    let data = InteractionResponseData {
        choices: Some(choices),
        ..Default::default()
    };

    let response = InteractionResponse {
        kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
        data: Some(data),
    };
    CTX.interaction()
        .create_response(interaction.id, &interaction.token, &response)
        .await?;

    Ok(())
}

pub async fn run(
    interaction: Box<InteractionCreate>,
    mut data: Box<CommandData>,
) -> anyhow::Result<()> {
    let CommandOptionValue::Integer(shard_id) = data.options.pop().unwrap().value else {
        unreachable!()
    };
    let kind = match data.options.pop() {
        Some(CommandDataOption {
            value: CommandOptionValue::Boolean(true),
            ..
        }) => ShardRestartKind::Resume,
        _ => ShardRestartKind::Normal,
    };

    let shard = CTX.shards.get(&(shard_id as u32)).unwrap().clone();
    let restart_result = shard.restart(kind);

    let response = if restart_result.is_forced() {
        tracing::debug!(shard.id = shard_id, "force restarting shard");
        let data = InteractionResponseData {
            content: Some("Force restarted shard".to_owned()),
            ..Default::default()
        };
        InteractionResponse {
            kind: InteractionResponseType::ChannelMessageWithSource,
            data: Some(data),
        }
    } else {
        tracing::debug!(shard.id = shard_id, ?kind, "restarting shard");
        InteractionResponse {
            kind: InteractionResponseType::DeferredChannelMessageWithSource,
            data: None,
        }
    };
    CTX.interaction()
        .create_response(interaction.id, &interaction.token, &response)
        .await?;
    if restart_result.is_forced() {
        return Ok(());
    }

    shard.restarted().await;
    let is_restarted = CTX.shards.get(&(shard_id as u32)).unwrap().is_valid();

    let content = if is_restarted {
        "Shard restarted"
    } else {
        "Bot shut down"
    };
    CTX.interaction()
        .update_response(&interaction.token)
        .content(Some(content))
        .await?;

    Ok(())
}

/// Creates an iterator which computes values up to `max` whose string
/// representanion starts with `value`.
///
/// Produces invalid values if `value` is 0.
///
/// # Example
///
/// ```
/// let values = starts_with(1, 100);
/// assert!(values.eq([1..2, 10..20].into_iter().flatten()));
///
/// let values = starts_with(10, 1000);
/// assert!(values.eq([10..11, 100..110].into_iter().flatten()));
///
/// let values = starts_with(1, 1000);
/// assert!(values.eq([1..2, 10..20, 100..200].into_iter().flatten()));
/// ```
fn starts_with(value: u32, max: u32) -> impl Iterator<Item = u32> {
    debug_assert_ne!(value, 0);

    iter::successors(Some(1_u32), |n| n.checked_mul(10))
        .take_while(move |&n| value * n < max)
        .flat_map(move |n| {
            let start = value * n;
            let end = ((value + 1) * n - 1).min(max);
            start..=end
        })
}
