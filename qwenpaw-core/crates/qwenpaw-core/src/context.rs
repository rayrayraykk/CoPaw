use std::env;

use qwenpaw_storage::StoredMessage;

const DEFAULT_MAX_CONTEXT_MESSAGES: usize = 128;
const MIN_CONTEXT_MESSAGES: usize = 32;
const MAX_CONTEXT_MESSAGES: usize = 512;
const DEFAULT_MAX_CONTEXT_BYTES: usize = 4 * 1_048_576;
const MIN_CONTEXT_BYTES: usize = 65_536;
const MAX_CONTEXT_BYTES: usize = 64 * 1_048_576;
const TRUNCATION_MARKER: &str = "\n[context truncated]";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextLimits {
    max_messages: usize,
    max_bytes: usize,
}

impl ContextLimits {
    pub(crate) fn from_env() -> Self {
        Self {
            max_messages: read_limit(
                "QWENPAW_MAX_CONTEXT_MESSAGES",
                DEFAULT_MAX_CONTEXT_MESSAGES,
                MIN_CONTEXT_MESSAGES,
                MAX_CONTEXT_MESSAGES,
            ),
            max_bytes: read_limit(
                "QWENPAW_MAX_CONTEXT_BYTES",
                DEFAULT_MAX_CONTEXT_BYTES,
                MIN_CONTEXT_BYTES,
                MAX_CONTEXT_BYTES,
            ),
        }
    }
}

pub(crate) fn build_context(
    messages: &[StoredMessage],
    limits: ContextLimits,
) -> Result<Vec<StoredMessage>, ContextError> {
    let (system, conversation) = messages
        .first()
        .filter(|message| message.role == "system")
        .map_or((None, messages), |message| {
            (Some(message.clone()), &messages[1..])
        });
    let groups = group_by_user_turn(conversation);
    let system_size = system.as_ref().map_or(0, serialized_size);
    let mut remaining_messages = limits
        .max_messages
        .saturating_sub(usize::from(system.is_some()));
    let mut remaining_bytes = limits.max_bytes.saturating_sub(system_size);
    let mut selected = Vec::new();

    for (index, group) in groups.iter().rev().enumerate() {
        if group.len() > remaining_messages {
            continue;
        }
        let group_size = serialized_group_size(group);
        if index == 0 {
            let group = fit_latest_group(group, remaining_bytes);
            remaining_bytes = remaining_bytes.saturating_sub(serialized_group_size(&group));
            remaining_messages -= group.len();
            selected.push(group);
        } else if group_size <= remaining_bytes {
            remaining_bytes -= group_size;
            remaining_messages -= group.len();
            selected.push(group.clone());
        }
    }
    selected.reverse();

    let mut context = Vec::new();
    if let Some(system) = system {
        context.push(system);
    }
    context.extend(selected.into_iter().flatten());
    let actual_bytes = context.iter().map(serialized_size).sum();
    if actual_bytes > limits.max_bytes {
        return Err(ContextError::TooLarge {
            actual_bytes,
            max_bytes: limits.max_bytes,
        });
    }
    Ok(context)
}

fn group_by_user_turn(messages: &[StoredMessage]) -> Vec<Vec<StoredMessage>> {
    let mut groups = Vec::<Vec<StoredMessage>>::new();
    for message in messages {
        if message.role == "user" || groups.is_empty() {
            groups.push(Vec::new());
        }
        if let Some(group) = groups.last_mut() {
            group.push(message.clone());
        }
    }
    groups
}

fn fit_latest_group(group: &[StoredMessage], budget: usize) -> Vec<StoredMessage> {
    if serialized_group_size(group) <= budget {
        return group.to_vec();
    }
    let mut fitted = group.to_vec();
    let content_indices = fitted
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (!message.content.is_empty()).then_some(index))
        .collect::<Vec<_>>();
    for message in &mut fitted {
        message.content.clear();
    }
    let fixed_size = serialized_group_size(&fitted);
    let content_budget = budget.saturating_sub(fixed_size);
    let mut lower = 0;
    let mut upper = content_budget;
    while lower < upper {
        let candidate = lower + (upper - lower).div_ceil(2);
        set_content_budget(&mut fitted, group, &content_indices, candidate);
        if serialized_group_size(&fitted) <= budget {
            lower = candidate;
        } else {
            upper = candidate - 1;
        }
    }
    set_content_budget(&mut fitted, group, &content_indices, lower);
    fitted
}

fn set_content_budget(
    fitted: &mut [StoredMessage],
    original: &[StoredMessage],
    content_indices: &[usize],
    per_message: usize,
) {
    for &index in content_indices {
        fitted[index].content = truncate_to_bytes(&original[index].content, per_message);
    }
}

fn truncate_to_bytes(value: &str, budget: usize) -> String {
    if value.len() <= budget {
        return value.to_owned();
    }
    if budget == 0 {
        return String::new();
    }
    let marker_size = TRUNCATION_MARKER.len().min(budget);
    let mut boundary = budget - marker_size;
    while !value.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    let mut truncated = value[..boundary].to_owned();
    truncated.push_str(&TRUNCATION_MARKER[..marker_size]);
    truncated
}

fn serialized_group_size(group: &[StoredMessage]) -> usize {
    group.iter().map(serialized_size).sum()
}

fn serialized_size(message: &StoredMessage) -> usize {
    serde_json::to_vec(message).map_or(usize::MAX, |serialized| serialized.len())
}

fn read_limit(name: &str, default: usize, minimum: usize, maximum: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(minimum, maximum)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum ContextError {
    #[error(
        "latest conversation turn is {actual_bytes} bytes, exceeding the {max_bytes}-byte context limit"
    )]
    TooLarge {
        actual_bytes: usize,
        max_bytes: usize,
    },
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
