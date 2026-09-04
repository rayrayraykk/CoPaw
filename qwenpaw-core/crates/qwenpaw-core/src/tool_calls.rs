use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use qwenpaw_tools::ToolOutput;
use serde_json::Value;
use serde_json::json;
use tokio::sync::Mutex;
use tokio::sync::Notify;
use tokio::sync::mpsc;
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

const COMPLETED_CACHE_TTL: Duration = Duration::from_secs(60);
const COMPLETED_CACHE_MAX: usize = 50;
const STREAM_CHANNEL_CAPACITY: usize = 4;
const MIN_BACKGROUND_WINDOW: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallSnapshot {
    pub tool_call_id: String,
    pub tool_name: String,
    pub thread_id: String,
    pub status: String,
    pub started_at: f64,
    pub elapsed: f64,
    pub offload_remaining: Option<f64>,
    pub kill_remaining: Option<f64>,
    pub end_state: Option<String>,
    pub force_cancelled: bool,
    pub max_internal_timeout_secs: Option<f64>,
    pub offload_reason: Option<String>,
    pub is_closed: bool,
    pub content: Vec<Value>,
}

#[derive(Debug)]
pub struct ToolCallSubscription {
    pub snapshot: ToolCallSnapshot,
    pub events: mpsc::Receiver<ToolCallStreamEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ToolCallStreamEvent {
    Chunk(Value),
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ToolCallControlError {
    #[error("tool call not found")]
    NotFound,
    #[error("tool call state does not allow this action")]
    Conflict,
    #[error("deadline extension is invalid")]
    InvalidDeadline,
}

#[derive(Clone)]
pub(crate) struct ToolCallCoordinator {
    inner: Arc<ToolCallCoordinatorInner>,
}

struct ToolCallCoordinatorInner {
    entries: Mutex<HashMap<String, ToolCallEntry>>,
    offload_on_deadline: AtomicBool,
    completed_ttl: Duration,
    completed_max: usize,
}

struct ToolCallEntry {
    tool_call_id: String,
    tool_name: String,
    thread_id: String,
    status: ToolCallStatus,
    started_at: f64,
    started: Instant,
    offload_deadline: Option<Instant>,
    kill_deadline: Option<Instant>,
    max_internal_timeout: Option<Duration>,
    offload_reason: Option<OffloadReason>,
    cancel_reason: Option<CancelReason>,
    end_state: Option<String>,
    force_cancelled: bool,
    content: Vec<Value>,
    completed_at: Option<Instant>,
    cancellation: CancellationToken,
    deadline_changed: Arc<Notify>,
    offloaded: watch::Sender<bool>,
    subscribers: Vec<mpsc::Sender<ToolCallStreamEvent>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCallStatus {
    Running,
    Offloaded,
    Completed,
}

impl ToolCallStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Offloaded => "offloaded",
            Self::Completed => "completed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OffloadReason {
    User,
    Timeout,
}

impl OffloadReason {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Timeout => "timeout",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolCancellationReason {
    User,
    Timeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CancelReason {
    User,
    Timeout,
}

#[derive(Clone)]
pub(crate) struct ToolCallLease {
    pub cancellation: CancellationToken,
    offloaded: watch::Receiver<bool>,
}

impl ToolCallCoordinator {
    pub(crate) fn new(offload_on_deadline: bool) -> Self {
        Self::with_limits(
            offload_on_deadline,
            COMPLETED_CACHE_TTL,
            COMPLETED_CACHE_MAX,
        )
    }

    fn with_limits(
        offload_on_deadline: bool,
        completed_ttl: Duration,
        completed_max: usize,
    ) -> Self {
        Self {
            inner: Arc::new(ToolCallCoordinatorInner {
                entries: Mutex::new(HashMap::new()),
                offload_on_deadline: AtomicBool::new(offload_on_deadline),
                completed_ttl,
                completed_max,
            }),
        }
    }

    pub(crate) fn set_offload_on_deadline(&self, enabled: bool) {
        self.inner
            .offload_on_deadline
            .store(enabled, Ordering::Release);
    }

    pub(crate) fn offload_on_deadline(&self) -> bool {
        self.inner.offload_on_deadline.load(Ordering::Acquire)
    }

    pub(crate) async fn begin(
        &self,
        thread_id: &str,
        tool_call_id: &str,
        tool_name: &str,
        timeout: Option<Duration>,
        max_internal_timeout: Option<Duration>,
        parent_cancellation: &CancellationToken,
    ) -> Result<ToolCallLease, ToolCallControlError> {
        let now = Instant::now();
        let offload_deadline = timeout.map(|duration| now + duration.div_f64(2.0));
        let kill_deadline = timeout.map(|duration| now + duration);
        let cancellation = parent_cancellation.child_token();
        let deadline_changed = Arc::new(Notify::new());
        let (offloaded, offloaded_receiver) = watch::channel(false);
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        if entries
            .get(tool_call_id)
            .is_some_and(|entry| entry.status != ToolCallStatus::Completed)
        {
            return Err(ToolCallControlError::Conflict);
        }
        entries.insert(
            tool_call_id.to_owned(),
            ToolCallEntry {
                tool_call_id: tool_call_id.to_owned(),
                tool_name: tool_name.to_owned(),
                thread_id: thread_id.to_owned(),
                status: ToolCallStatus::Running,
                started_at: unix_time_seconds(),
                started: now,
                offload_deadline,
                kill_deadline,
                max_internal_timeout,
                offload_reason: None,
                cancel_reason: None,
                end_state: None,
                force_cancelled: false,
                content: Vec::new(),
                completed_at: None,
                cancellation: cancellation.clone(),
                deadline_changed: Arc::clone(&deadline_changed),
                offloaded,
                subscribers: Vec::new(),
            },
        );
        drop(entries);
        self.spawn_deadline_monitor(tool_call_id.to_owned(), deadline_changed);
        Ok(ToolCallLease {
            cancellation,
            offloaded: offloaded_receiver,
        })
    }

    pub(crate) async fn list(&self, thread_id: &str) -> Vec<ToolCallSnapshot> {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let mut snapshots = entries
            .values()
            .filter(|entry| {
                entry.thread_id == thread_id && entry.status != ToolCallStatus::Completed
            })
            .map(|entry| snapshot(entry, now))
            .collect::<Vec<_>>();
        snapshots.sort_by(|left, right| {
            left.started_at
                .total_cmp(&right.started_at)
                .then_with(|| left.tool_call_id.cmp(&right.tool_call_id))
        });
        snapshots
    }

    pub(crate) async fn get(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let entry = scoped_entry(&entries, thread_id, tool_call_id)?;
        Ok(snapshot(entry, now))
    }

    pub(crate) async fn subscribe(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSubscription, ToolCallControlError> {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let entry = scoped_entry_mut(&mut entries, thread_id, tool_call_id)?;
        let snapshot = snapshot(entry, now);
        let (sender, events) = mpsc::channel(STREAM_CHANNEL_CAPACITY);
        if entry.status != ToolCallStatus::Completed {
            entry.subscribers.push(sender);
        }
        Ok(ToolCallSubscription { snapshot, events })
    }

    pub(crate) async fn request_offload(
        &self,
        thread_id: &str,
        tool_call_id: &str,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let entry = scoped_entry_mut(&mut entries, thread_id, tool_call_id)?;
        if entry.status != ToolCallStatus::Running || !ensure_background_window(entry, now) {
            return Err(ToolCallControlError::Conflict);
        }
        entry.status = ToolCallStatus::Offloaded;
        entry.offload_reason = Some(OffloadReason::User);
        entry.offload_deadline = None;
        let _ = entry.offloaded.send(true);
        entry.deadline_changed.notify_one();
        Ok(snapshot(entry, now))
    }

    pub(crate) async fn cancel(
        &self,
        thread_id: &str,
        tool_call_id: &str,
        force: bool,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let entry = scoped_entry_mut(&mut entries, thread_id, tool_call_id)?;
        if entry.status == ToolCallStatus::Completed {
            return Err(ToolCallControlError::Conflict);
        }
        entry.cancel_reason = Some(CancelReason::User);
        entry.force_cancelled |= force;
        entry.cancellation.cancel();
        entry.deadline_changed.notify_one();
        Ok(snapshot(entry, now))
    }

    pub(crate) async fn extend_deadline(
        &self,
        thread_id: &str,
        tool_call_id: &str,
        target: &str,
        seconds: Option<f64>,
        no_deadline: bool,
    ) -> Result<ToolCallSnapshot, ToolCallControlError> {
        if !no_deadline && seconds.is_none_or(|seconds| !seconds.is_finite() || seconds <= 0.0) {
            return Err(ToolCallControlError::InvalidDeadline);
        }
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        self.prune_completed_locked(&mut entries, now);
        let entry = scoped_entry_mut(&mut entries, thread_id, tool_call_id)?;
        match target {
            "offload" if entry.status == ToolCallStatus::Running => {
                if no_deadline {
                    entry.offload_deadline = None;
                } else if let Some(seconds) = seconds {
                    let base = entry.offload_deadline.unwrap_or(now).max(now);
                    entry.offload_deadline = Some(base + Duration::from_secs_f64(seconds));
                }
            }
            "kill" if entry.status != ToolCallStatus::Completed => {
                if no_deadline {
                    if entry.max_internal_timeout.is_some()
                        || entry.status == ToolCallStatus::Offloaded
                    {
                        return Err(ToolCallControlError::Conflict);
                    }
                    entry.kill_deadline = None;
                } else if let Some(seconds) = seconds {
                    let base = entry.kill_deadline.unwrap_or(now).max(now);
                    let next = base + Duration::from_secs_f64(seconds);
                    if entry
                        .max_internal_timeout
                        .is_some_and(|cap| next > entry.started + cap)
                    {
                        return Err(ToolCallControlError::Conflict);
                    }
                    entry.kill_deadline = Some(next);
                }
            }
            "offload" | "kill" => return Err(ToolCallControlError::Conflict),
            _ => return Err(ToolCallControlError::InvalidDeadline),
        }
        entry.deadline_changed.notify_one();
        Ok(snapshot(entry, now))
    }

    pub(crate) async fn finish(&self, tool_call_id: &str, output: &ToolOutput) {
        let now = Instant::now();
        let mut entries = self.inner.entries.lock().await;
        let Some(entry) = entries.get_mut(tool_call_id) else {
            return;
        };
        if entry.status == ToolCallStatus::Completed {
            return;
        }
        entry.status = ToolCallStatus::Completed;
        entry.offload_deadline = None;
        entry.kill_deadline = None;
        entry.end_state = Some(if entry.cancel_reason.is_some() {
            String::from("interrupted")
        } else if output.is_error {
            String::from("error")
        } else {
            String::from("success")
        });
        let block = json!({"type": "text", "text": output.content});
        entry.content = vec![block.clone()];
        entry.completed_at = Some(now);
        let subscribers = std::mem::take(&mut entry.subscribers);
        entry.deadline_changed.notify_one();
        for subscriber in subscribers {
            let _ = subscriber.try_send(ToolCallStreamEvent::Chunk(block.clone()));
            let _ = subscriber.try_send(ToolCallStreamEvent::Done);
        }
        self.prune_completed_locked(&mut entries, now);
    }

    pub(crate) async fn cancellation_reason(
        &self,
        tool_call_id: &str,
    ) -> Option<ToolCancellationReason> {
        self.inner
            .entries
            .lock()
            .await
            .get(tool_call_id)
            .and_then(|entry| entry.cancel_reason)
            .map(|reason| match reason {
                CancelReason::User => ToolCancellationReason::User,
                CancelReason::Timeout => ToolCancellationReason::Timeout,
            })
    }

    fn spawn_deadline_monitor(&self, tool_call_id: String, changed: Arc<Notify>) {
        let coordinator = self.clone();
        tokio::spawn(async move {
            loop {
                let deadline = {
                    let entries = coordinator.inner.entries.lock().await;
                    let Some(entry) = entries.get(&tool_call_id) else {
                        return;
                    };
                    if entry.status == ToolCallStatus::Completed {
                        return;
                    }
                    next_deadline(entry)
                };
                if let Some(deadline) = deadline {
                    tokio::select! {
                        () = tokio::time::sleep_until(deadline) => {}
                        () = changed.notified() => continue,
                    }
                } else {
                    changed.notified().await;
                    continue;
                }
                let now = Instant::now();
                let mut entries = coordinator.inner.entries.lock().await;
                let Some(entry) = entries.get_mut(&tool_call_id) else {
                    return;
                };
                if entry.status == ToolCallStatus::Completed {
                    return;
                }
                if entry.kill_deadline.is_some_and(|deadline| deadline <= now) {
                    entry.cancel_reason = Some(CancelReason::Timeout);
                    entry.cancellation.cancel();
                    return;
                }
                if entry
                    .offload_deadline
                    .is_some_and(|deadline| deadline <= now)
                {
                    entry.offload_deadline = None;
                    if coordinator
                        .inner
                        .offload_on_deadline
                        .load(Ordering::Acquire)
                    {
                        entry.status = ToolCallStatus::Offloaded;
                        entry.offload_reason = Some(OffloadReason::Timeout);
                        let _ = entry.offloaded.send(true);
                    }
                }
            }
        });
    }

    fn prune_completed_locked(&self, entries: &mut HashMap<String, ToolCallEntry>, now: Instant) {
        entries.retain(|_, entry| {
            entry.completed_at.is_none_or(|completed_at| {
                now.saturating_duration_since(completed_at) <= self.inner.completed_ttl
            })
        });
        let completed_count = entries
            .values()
            .filter(|entry| entry.status == ToolCallStatus::Completed)
            .count();
        if completed_count <= self.inner.completed_max {
            return;
        }
        let mut completed = entries
            .iter()
            .filter_map(|(call_id, entry)| {
                entry
                    .completed_at
                    .map(|completed_at| (call_id.clone(), completed_at))
            })
            .collect::<Vec<_>>();
        completed.sort_by_key(|(_, completed_at)| *completed_at);
        for (call_id, _) in completed
            .into_iter()
            .take(completed_count - self.inner.completed_max)
        {
            entries.remove(&call_id);
        }
    }
}

impl ToolCallLease {
    pub(crate) async fn wait_for_offload(&mut self) {
        while !*self.offloaded.borrow_and_update() {
            if self.offloaded.changed().await.is_err() {
                return;
            }
        }
    }
}

fn scoped_entry<'a>(
    entries: &'a HashMap<String, ToolCallEntry>,
    thread_id: &str,
    tool_call_id: &str,
) -> Result<&'a ToolCallEntry, ToolCallControlError> {
    entries
        .get(tool_call_id)
        .filter(|entry| entry.thread_id == thread_id)
        .ok_or(ToolCallControlError::NotFound)
}

fn scoped_entry_mut<'a>(
    entries: &'a mut HashMap<String, ToolCallEntry>,
    thread_id: &str,
    tool_call_id: &str,
) -> Result<&'a mut ToolCallEntry, ToolCallControlError> {
    entries
        .get_mut(tool_call_id)
        .filter(|entry| entry.thread_id == thread_id)
        .ok_or(ToolCallControlError::NotFound)
}

fn next_deadline(entry: &ToolCallEntry) -> Option<Instant> {
    match (entry.offload_deadline, entry.kill_deadline) {
        (Some(offload), Some(kill)) => Some(offload.min(kill)),
        (Some(offload), None) => Some(offload),
        (None, Some(kill)) => Some(kill),
        (None, None) => None,
    }
}

fn ensure_background_window(entry: &mut ToolCallEntry, now: Instant) -> bool {
    let Some(maximum) = entry.max_internal_timeout else {
        return entry
            .kill_deadline
            .is_some_and(|deadline| deadline > now + MIN_BACKGROUND_WINDOW);
    };
    let cap = entry.started + maximum;
    if cap <= now + MIN_BACKGROUND_WINDOW {
        return false;
    }
    if entry
        .kill_deadline
        .is_none_or(|deadline| deadline < now + MIN_BACKGROUND_WINDOW)
    {
        entry.kill_deadline = Some((now + MIN_BACKGROUND_WINDOW).min(cap));
    }
    true
}

fn snapshot(entry: &ToolCallEntry, now: Instant) -> ToolCallSnapshot {
    ToolCallSnapshot {
        tool_call_id: entry.tool_call_id.clone(),
        tool_name: entry.tool_name.clone(),
        thread_id: entry.thread_id.clone(),
        status: String::from(entry.status.as_str()),
        started_at: entry.started_at,
        elapsed: now.saturating_duration_since(entry.started).as_secs_f64(),
        offload_remaining: remaining(entry.offload_deadline, now),
        kill_remaining: remaining(entry.kill_deadline, now),
        end_state: entry.end_state.clone(),
        force_cancelled: entry.force_cancelled,
        max_internal_timeout_secs: entry.max_internal_timeout.map(|value| value.as_secs_f64()),
        offload_reason: entry
            .offload_reason
            .map(|reason| String::from(reason.as_str())),
        is_closed: entry.status == ToolCallStatus::Completed,
        content: entry.content.clone(),
    }
}

fn remaining(deadline: Option<Instant>, now: Instant) -> Option<f64> {
    deadline.map(|deadline| deadline.saturating_duration_since(now).as_secs_f64())
}

fn unix_time_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scopes_controls_and_streams_completed_output() {
        let coordinator = ToolCallCoordinator::new(false);
        let parent = CancellationToken::new();
        let _lease = coordinator
            .begin(
                "thread-1",
                "call-1",
                "shell",
                Some(Duration::from_secs(60)),
                Some(Duration::from_secs(600)),
                &parent,
            )
            .await
            .expect("tool call should begin");
        assert_eq!(coordinator.list("thread-1").await.len(), 1);
        assert_eq!(
            coordinator.get("thread-2", "call-1").await,
            Err(ToolCallControlError::NotFound)
        );
        let mut subscription = coordinator
            .subscribe("thread-1", "call-1")
            .await
            .expect("tool call should subscribe");
        coordinator
            .finish(
                "call-1",
                &ToolOutput {
                    content: String::from("finished"),
                    is_error: false,
                },
            )
            .await;
        assert_eq!(
            subscription.events.recv().await,
            Some(ToolCallStreamEvent::Chunk(
                json!({"type": "text", "text": "finished"})
            ))
        );
        assert_eq!(
            subscription.events.recv().await,
            Some(ToolCallStreamEvent::Done)
        );
        assert!(coordinator.list("thread-1").await.is_empty());
        assert_eq!(
            coordinator
                .get("thread-1", "call-1")
                .await
                .expect("completed call should remain cached")
                .end_state
                .as_deref(),
            Some("success")
        );
    }

    #[tokio::test]
    async fn enforces_offload_deadline_and_completion_cache_bounds() {
        let coordinator = ToolCallCoordinator::with_limits(true, Duration::from_millis(20), 1);
        let parent = CancellationToken::new();
        let mut lease = coordinator
            .begin(
                "thread",
                "auto",
                "shell",
                Some(Duration::from_millis(10)),
                Some(Duration::from_secs(60)),
                &parent,
            )
            .await
            .expect("auto-offloaded call should begin");
        tokio::time::timeout(Duration::from_secs(1), lease.wait_for_offload())
            .await
            .expect("call should auto-offload");
        assert_eq!(
            coordinator
                .get("thread", "auto")
                .await
                .expect("offloaded call should exist")
                .offload_reason
                .as_deref(),
            Some("timeout")
        );
        coordinator
            .finish(
                "auto",
                &ToolOutput {
                    content: String::from("first"),
                    is_error: false,
                },
            )
            .await;
        let lease = coordinator
            .begin("thread", "second", "read_file", None, None, &parent)
            .await
            .expect("second call should begin");
        coordinator
            .finish(
                "second",
                &ToolOutput {
                    content: String::from("second"),
                    is_error: false,
                },
            )
            .await;
        assert_eq!(
            coordinator.get("thread", "auto").await,
            Err(ToolCallControlError::NotFound)
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(
            coordinator.get("thread", "second").await,
            Err(ToolCallControlError::NotFound)
        );
        drop(lease);
    }

    #[tokio::test]
    async fn extends_and_cancels_one_call_without_cancelling_its_parent() {
        let coordinator = ToolCallCoordinator::new(false);
        let parent = CancellationToken::new();
        let lease = coordinator
            .begin(
                "thread",
                "call",
                "shell",
                Some(Duration::from_secs(60)),
                Some(Duration::from_secs(600)),
                &parent,
            )
            .await
            .expect("call should begin");
        let before = coordinator
            .get("thread", "call")
            .await
            .expect("call should exist")
            .kill_remaining
            .expect("kill deadline should exist");
        let after = coordinator
            .extend_deadline("thread", "call", "kill", Some(30.0), false)
            .await
            .expect("kill deadline should extend")
            .kill_remaining
            .expect("extended deadline should exist");
        assert!(after > before + 29.0);
        coordinator
            .cancel("thread", "call", false)
            .await
            .expect("call should cancel");
        lease.cancellation.cancelled().await;
        assert!(!parent.is_cancelled());
        assert_eq!(
            coordinator.cancellation_reason("call").await,
            Some(ToolCancellationReason::User)
        );
    }
}
