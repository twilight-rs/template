use crate::{APPLICATION_ID, CONTEXT, ShardRestartType};
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
            "Resume ression? [default: false]",
        ))
        .build()
}

pub async fn autocomplete(
    event: Box<InteractionCreate>,
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
        Ok(shard_id) => starts_with(shard_id, CONTEXT.shard_handles.len() as u32 - 1)
            .take(25)
            .map(choice)
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    let data = InteractionResponseData {
        choices: Some(choices),
        ..Default::default()
    };

    let response = InteractionResponse {
        kind: InteractionResponseType::ApplicationCommandAutocompleteResult,
        data: Some(data),
    };
    CONTEXT
        .http
        .interaction(APPLICATION_ID)
        .create_response(event.id, &event.token, &response)
        .await?;

    Ok(())
}

pub async fn run(event: Box<InteractionCreate>, mut data: Box<CommandData>) -> anyhow::Result<()> {
    let mut options = data.options.drain(..);
    let CommandOptionValue::Integer(shard_id) = options.next().unwrap().value else {
        unreachable!()
    };
    let kind = match options.next() {
        Some(CommandDataOption {
            name: _,
            value: CommandOptionValue::Boolean(resume),
        }) => match resume {
            true => ShardRestartType::Resume,
            false => ShardRestartType::Normal,
        },
        None => ShardRestartType::Normal,
        Some(_) => unreachable!(),
    };

    let shard_handle = CONTEXT
        .shard_handles
        .get(&(shard_id as u32))
        .unwrap()
        .clone();
    let restart_result = shard_handle.restart(kind);

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
        tracing::debug!(shard.id = shard_id, type = ?kind, "restarting shard");
        InteractionResponse {
            kind: InteractionResponseType::DeferredChannelMessageWithSource,
            data: None,
        }
    };
    CONTEXT
        .http
        .interaction(APPLICATION_ID)
        .create_response(event.id, &event.token, &response)
        .await?;
    if restart_result.is_forced() {
        return Ok(());
    }

    shard_handle.restarted().await;
    let is_restarted = CONTEXT
        .shard_handles
        .get(&(shard_id as u32))
        .unwrap()
        .is_valid();

    let content = if is_restarted {
        "Shard restarted"
    } else {
        "Bot shut down"
    };
    CONTEXT
        .http
        .interaction(APPLICATION_ID)
        .update_response(&event.token)
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
/// let values = starts_with(5, 100);
/// assert!(values.eq([5..6, 50..60].into_iter().flatten()));
///
/// let values = starts_with(50, 1000);
/// assert!(values.eq([50..51, 500..510].into_iter().flatten()));
///
/// let values = starts_with(5, 1000);
/// assert!(values.eq([5..6, 50..60, 500..600].into_iter().flatten()));
/// ```
fn starts_with(value: u32, max: u32) -> impl Iterator<Item = u32> {
    debug_assert_ne!(value, 0);

    iter::successors(Some(1_u32), |n| n.checked_mul(10))
        .take_while(move |&n| value * n <= max)
        .flat_map(move |n| {
            let start = value * n;
            let end = ((value + 1) * n - 1).min(max);
            start..=end
        })
}
