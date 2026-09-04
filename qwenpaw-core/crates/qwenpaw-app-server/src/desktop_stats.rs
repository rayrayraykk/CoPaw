//! Real statistics contracts for the unchanged Console pages.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use axum::Json;
use axum::Router;
use axum::extract::Query;
use axum::extract::State;
use axum::routing::get;
use chrono::DateTime;
use chrono::Duration;
use chrono::Local;
use chrono::NaiveDate;
use chrono::Utc;
use qwenpaw_core::ThreadCheckpoint;
use qwenpaw_protocol::Item;
use qwenpaw_protocol::Turn;
use qwenpaw_storage::StoredTurnMetadata;
use qwenpaw_storage::StoredUsageRecord;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use serde_json::json;

use super::AppServer;

const DEFAULT_RANGE_DAYS: i64 = 30;
const CHANNEL: &str = "console";

pub(super) fn router() -> Router<AppServer> {
    Router::new()
        .route("/api/token-usage", get(token_usage))
        .route("/api/token-usage/details", get(token_usage_details))
        .route("/api/agent-stats", get(agent_stats))
        .route("/api/agent-stats/llm-tool-trend", get(llm_tool_trend))
}

#[derive(Debug, Default, Deserialize)]
struct StatsQuery {
    #[serde(default)]
    start_date: Option<String>,
    #[serde(default)]
    end_date: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    provider: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
struct UsageTotals {
    prompt_tokens: u64,
    completion_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    cache_eligible_input_tokens: u64,
    cache_observed_calls: u64,
    call_count: u64,
}

impl UsageTotals {
    fn add_call(&mut self, call: &qwenpaw_storage::StoredModelCall) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(call.prompt_tokens);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(call.completion_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(call.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(call.cache_write_tokens);
        self.cache_eligible_input_tokens = self
            .cache_eligible_input_tokens
            .saturating_add(call.cache_eligible_input_tokens);
        self.cache_observed_calls = self
            .cache_observed_calls
            .saturating_add(u64::from(call.cache_observed));
        self.call_count = self.call_count.saturating_add(1);
    }

    fn add_totals(&mut self, other: &Self) {
        self.prompt_tokens = self.prompt_tokens.saturating_add(other.prompt_tokens);
        self.completion_tokens = self
            .completion_tokens
            .saturating_add(other.completion_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(other.cache_read_tokens);
        self.cache_write_tokens = self
            .cache_write_tokens
            .saturating_add(other.cache_write_tokens);
        self.cache_eligible_input_tokens = self
            .cache_eligible_input_tokens
            .saturating_add(other.cache_eligible_input_tokens);
        self.cache_observed_calls = self
            .cache_observed_calls
            .saturating_add(other.cache_observed_calls);
        self.call_count = self.call_count.saturating_add(other.call_count);
    }
}

#[derive(Debug, Serialize)]
struct UsageRecord {
    date: String,
    provider_id: String,
    model: String,
    #[serde(flatten)]
    totals: UsageTotals,
    agent_id: String,
}

async fn token_usage_details(
    State(server): State<AppServer>,
    Query(query): Query<StatsQuery>,
) -> Json<Vec<UsageRecord>> {
    let (start, end) = resolved_range(&query);
    let usage = server.inner.core.usage_records().await;
    Json(usage_records(&usage, start, end, &query))
}

async fn token_usage(
    State(server): State<AppServer>,
    Query(query): Query<StatsQuery>,
) -> Json<Value> {
    let (start, end) = resolved_range(&query);
    let usage = server.inner.core.usage_records().await;
    let records = usage_records(&usage, start, end, &query);
    let mut total = UsageTotals::default();
    let mut by_model = BTreeMap::<String, Value>::new();
    let mut by_model_totals = BTreeMap::<String, UsageTotals>::new();
    let mut by_date = BTreeMap::<String, UsageTotals>::new();
    let mut model_identity = BTreeMap::<String, (String, String)>::new();
    for record in &records {
        total.add_totals(&record.totals);
        let key = if record.provider_id.is_empty() {
            record.model.clone()
        } else {
            format!("{}:{}", record.provider_id, record.model)
        };
        by_model_totals
            .entry(key.clone())
            .or_default()
            .add_totals(&record.totals);
        model_identity
            .entry(key)
            .or_insert_with(|| (record.provider_id.clone(), record.model.clone()));
        by_date
            .entry(record.date.clone())
            .or_default()
            .add_totals(&record.totals);
    }
    for (key, totals) in by_model_totals {
        let (provider_id, model) = model_identity
            .remove(&key)
            .unwrap_or_else(|| (String::new(), key.clone()));
        by_model.insert(
            key,
            json!({
                "provider_id": provider_id,
                "model": model,
                "prompt_tokens": totals.prompt_tokens,
                "completion_tokens": totals.completion_tokens,
                "cache_read_tokens": totals.cache_read_tokens,
                "cache_write_tokens": totals.cache_write_tokens,
                "cache_eligible_input_tokens": totals.cache_eligible_input_tokens,
                "cache_observed_calls": totals.cache_observed_calls,
                "call_count": totals.call_count
            }),
        );
    }
    Json(json!({
        "total_prompt_tokens": total.prompt_tokens,
        "total_completion_tokens": total.completion_tokens,
        "total_cache_read_tokens": total.cache_read_tokens,
        "total_cache_write_tokens": total.cache_write_tokens,
        "total_cache_eligible_input_tokens": total.cache_eligible_input_tokens,
        "cache_observed_calls": total.cache_observed_calls,
        "cache_hit_rate": cache_hit_rate(&total),
        "total_calls": total.call_count,
        "by_model": by_model,
        "by_date": by_date
    }))
}

fn usage_records(
    usage: &[StoredUsageRecord],
    start: NaiveDate,
    end: NaiveDate,
    query: &StatsQuery,
) -> Vec<UsageRecord> {
    let mut records = BTreeMap::<(String, String, String, String), UsageTotals>::new();
    for usage in usage {
        let Some(date) = date_from_timestamp(usage.recorded_at) else {
            continue;
        };
        let call = &usage.call;
        if date < start
            || date > end
            || query
                .model
                .as_ref()
                .is_some_and(|model| model != &call.model)
            || query
                .provider
                .as_ref()
                .is_some_and(|provider| provider != &call.provider_id)
        {
            continue;
        }
        records
            .entry((
                date.to_string(),
                usage.agent_id.clone(),
                call.provider_id.clone(),
                call.model.clone(),
            ))
            .or_default()
            .add_call(call);
    }
    records
        .into_iter()
        .map(
            |((date, agent_id, provider_id, model), totals)| UsageRecord {
                date,
                provider_id,
                model,
                totals,
                agent_id,
            },
        )
        .collect()
}

#[derive(Debug, Default, Serialize)]
struct DailyStats {
    date: String,
    chats: u64,
    active_sessions: u64,
    user_messages: u64,
    assistant_messages: u64,
    total_messages: u64,
    prompt_tokens: u64,
    completion_tokens: u64,
    llm_calls: u64,
    tool_calls: u64,
    agent_prompt_tokens: u64,
    agent_completion_tokens: u64,
    agent_llm_calls: u64,
    agent_cache_read_tokens: u64,
}

#[derive(Debug, Default)]
struct ChannelStats {
    sessions: BTreeSet<String>,
    user_messages: u64,
    assistant_messages: u64,
}

struct AgentAccumulator {
    by_date: BTreeMap<String, DailyStats>,
    active_threads: BTreeSet<String>,
    active_by_date: BTreeMap<String, BTreeSet<String>>,
    channel: ChannelStats,
    total_tool_calls: u64,
    global_usage: UsageTotals,
    agent_usage: UsageTotals,
    agent_llm_calls: u64,
}

impl AgentAccumulator {
    fn new(start: NaiveDate, end: NaiveDate) -> Self {
        Self {
            by_date: daily_range(start, end),
            active_threads: BTreeSet::new(),
            active_by_date: BTreeMap::new(),
            channel: ChannelStats::default(),
            total_tool_calls: 0,
            global_usage: UsageTotals::default(),
            agent_usage: UsageTotals::default(),
            agent_llm_calls: 0,
        }
    }

    fn add_global_usage(&mut self, usage: &StoredUsageRecord) {
        let Some(date) = date_from_timestamp(usage.recorded_at) else {
            return;
        };
        let Some(day) = self.by_date.get_mut(&date.to_string()) else {
            return;
        };
        self.global_usage.add_call(&usage.call);
        day.prompt_tokens = day.prompt_tokens.saturating_add(usage.call.prompt_tokens);
        day.completion_tokens = day
            .completion_tokens
            .saturating_add(usage.call.completion_tokens);
        day.llm_calls = day.llm_calls.saturating_add(1);
    }

    fn add_thread(&mut self, snapshot: &ThreadCheckpoint) {
        if let Some(created) = date_from_timestamp(snapshot.thread.created_at)
            && let Some(day) = self.by_date.get_mut(&created.to_string())
        {
            day.chats = day.chats.saturating_add(1);
        }
        for turn in &snapshot.turns {
            let Some(metadata) = snapshot
                .turn_metadata
                .iter()
                .find(|metadata| metadata.turn_id == turn.id)
            else {
                continue;
            };
            self.add_turn(&snapshot.thread.id, turn, metadata);
        }
    }

    fn add_turn(&mut self, thread_id: &str, turn: &Turn, metadata: &StoredTurnMetadata) {
        let Some(date) = date_from_timestamp(metadata.started_at) else {
            return;
        };
        let date_key = date.to_string();
        let Some(day) = self.by_date.get_mut(&date_key) else {
            return;
        };
        let user_messages =
            count_items(&turn.items, |item| matches!(item, Item::UserMessage { .. }));
        let assistant_messages = u64::try_from(metadata.model_calls.len()).unwrap_or(u64::MAX);
        let tool_calls = count_items(&turn.items, |item| matches!(item, Item::ToolCall { .. }));
        let message_count = user_messages.saturating_add(assistant_messages);
        if message_count > 0 {
            self.active_threads.insert(thread_id.to_owned());
            self.active_by_date
                .entry(date_key)
                .or_default()
                .insert(thread_id.to_owned());
            self.channel.sessions.insert(thread_id.to_owned());
        }
        day.user_messages = day.user_messages.saturating_add(user_messages);
        day.assistant_messages = day.assistant_messages.saturating_add(assistant_messages);
        day.total_messages = day.total_messages.saturating_add(message_count);
        day.tool_calls = day.tool_calls.saturating_add(tool_calls);
        self.channel.user_messages = self.channel.user_messages.saturating_add(user_messages);
        self.channel.assistant_messages = self
            .channel
            .assistant_messages
            .saturating_add(assistant_messages);
        self.total_tool_calls = self.total_tool_calls.saturating_add(tool_calls);
        let llm_calls = assistant_messages;
        self.agent_llm_calls = self.agent_llm_calls.saturating_add(llm_calls);
        day.agent_llm_calls = day.agent_llm_calls.saturating_add(llm_calls);
        for call in &metadata.model_calls {
            if call.usage_observed {
                self.agent_usage.add_call(call);
                day.agent_prompt_tokens =
                    day.agent_prompt_tokens.saturating_add(call.prompt_tokens);
                day.agent_completion_tokens = day
                    .agent_completion_tokens
                    .saturating_add(call.completion_tokens);
                day.agent_cache_read_tokens = day
                    .agent_cache_read_tokens
                    .saturating_add(call.cache_read_tokens);
            }
        }
    }

    fn into_value(mut self, start: NaiveDate, end: NaiveDate) -> Value {
        for (date, threads) in self.active_by_date {
            if let Some(day) = self.by_date.get_mut(&date) {
                day.active_sessions = u64::try_from(threads.len()).unwrap_or(u64::MAX);
            }
        }
        let total_user_messages = self
            .by_date
            .values()
            .fold(0_u64, |total, day| total.saturating_add(day.user_messages));
        let total_assistant_messages = self.by_date.values().fold(0_u64, |total, day| {
            total.saturating_add(day.assistant_messages)
        });
        let channel_stats = channel_stats_value(&self.channel);
        json!({
            "total_active_sessions": self.active_threads.len(),
            "total_messages": total_user_messages.saturating_add(total_assistant_messages),
            "total_user_messages": total_user_messages,
            "total_assistant_messages": total_assistant_messages,
            "total_prompt_tokens": self.global_usage.prompt_tokens,
            "total_completion_tokens": self.global_usage.completion_tokens,
            "total_llm_calls": self.global_usage.call_count,
            "total_tool_calls": self.total_tool_calls,
            "by_date": self.by_date.into_values().collect::<Vec<_>>(),
            "channel_stats": channel_stats,
            "start_date": start.to_string(),
            "end_date": end.to_string(),
            "agent_prompt_tokens": self.agent_usage.prompt_tokens,
            "agent_completion_tokens": self.agent_usage.completion_tokens,
            "agent_llm_calls": self.agent_llm_calls,
            "agent_cache_read_tokens": self.agent_usage.cache_read_tokens,
            "agent_cache_eligible_input_tokens": self.agent_usage.cache_eligible_input_tokens,
            "agent_cache_hit_rate": cache_hit_rate(&self.agent_usage)
        })
    }
}

async fn agent_stats(
    State(server): State<AppServer>,
    Query(query): Query<StatsQuery>,
) -> Json<Value> {
    let (start, end) = resolved_range(&query);
    let snapshots = server.inner.core.statistics_snapshots().await;
    let usage = server.inner.core.usage_records().await;
    Json(agent_stats_value(&snapshots, &usage, start, end))
}

fn agent_stats_value(
    snapshots: &[ThreadCheckpoint],
    usage_records: &[StoredUsageRecord],
    start: NaiveDate,
    end: NaiveDate,
) -> Value {
    let mut stats = AgentAccumulator::new(start, end);
    for usage in usage_records {
        stats.add_global_usage(usage);
    }
    for snapshot in snapshots {
        stats.add_thread(snapshot);
    }
    stats.into_value(start, end)
}

fn channel_stats_value(channel: &ChannelStats) -> Vec<Value> {
    if channel.sessions.is_empty() {
        Vec::new()
    } else {
        vec![json!({
            "channel": CHANNEL,
            "session_count": channel.sessions.len(),
            "user_messages": channel.user_messages,
            "assistant_messages": channel.assistant_messages,
            "total_messages": channel
                .user_messages
                .saturating_add(channel.assistant_messages)
        })]
    }
}

async fn llm_tool_trend(
    State(server): State<AppServer>,
    Query(query): Query<StatsQuery>,
) -> Json<Vec<Value>> {
    let (mut start, end) = resolved_range(&query);
    if end.signed_duration_since(start).num_days() >= 365 {
        start = end - Duration::days(364);
    }
    let snapshots = server.inner.core.statistics_snapshots().await;
    let summary = agent_stats_value(&snapshots, &[], start, end);
    let rows = summary["by_date"].as_array().map_or_else(Vec::new, |days| {
        days.iter()
            .map(|day| {
                json!({
                    "date": day["date"],
                    "agent_llm_calls": day["agent_llm_calls"],
                    "tool_calls": day["tool_calls"]
                })
            })
            .collect()
    });
    Json(rows)
}

fn resolved_range(query: &StatsQuery) -> (NaiveDate, NaiveDate) {
    let today = Local::now().date_naive();
    let mut end = query
        .end_date
        .as_deref()
        .and_then(parse_date)
        .unwrap_or(today);
    let mut start = query
        .start_date
        .as_deref()
        .and_then(parse_date)
        .unwrap_or_else(|| end - Duration::days(DEFAULT_RANGE_DAYS));
    if start > end {
        std::mem::swap(&mut start, &mut end);
    }
    (start, end)
}

fn parse_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn daily_range(start: NaiveDate, end: NaiveDate) -> BTreeMap<String, DailyStats> {
    let mut result = BTreeMap::new();
    let mut date = start;
    loop {
        let key = date.to_string();
        result.insert(
            key.clone(),
            DailyStats {
                date: key,
                ..DailyStats::default()
            },
        );
        if date >= end {
            break;
        }
        let Some(next) = date.succ_opt() else {
            break;
        };
        date = next;
    }
    result
}

fn date_from_timestamp(timestamp: i64) -> Option<NaiveDate> {
    DateTime::from_timestamp(timestamp, 0)
        .map(|value: DateTime<Utc>| value.with_timezone(&Local).date_naive())
}

fn count_items(items: &[Item], predicate: impl Fn(&Item) -> bool) -> u64 {
    u64::try_from(items.iter().filter(|item| predicate(item)).count()).unwrap_or(u64::MAX)
}

fn cache_hit_rate(totals: &UsageTotals) -> Option<f64> {
    const RATE_SCALE: u64 = 1_000_000;
    (totals.cache_eligible_input_tokens > 0).then(|| {
        let scaled = totals.cache_read_tokens.saturating_mul(RATE_SCALE)
            / totals.cache_eligible_input_tokens;
        f64::from(u32::try_from(scaled).unwrap_or(u32::MAX)) / 10_000.0
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swaps_reversed_ranges_and_defaults_invalid_dates() {
        let query = StatsQuery {
            start_date: Some(String::from("2026-09-05")),
            end_date: Some(String::from("2026-09-01")),
            ..StatsQuery::default()
        };
        assert_eq!(
            resolved_range(&query),
            (
                NaiveDate::from_ymd_opt(2026, 9, 1).expect("date should be valid"),
                NaiveDate::from_ymd_opt(2026, 9, 5).expect("date should be valid")
            )
        );
    }
}
