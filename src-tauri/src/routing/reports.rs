//! Bounded, in-memory routing metadata. Never accept request/response bodies or keys.
use serde::Serialize;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

const HISTORY_LIMIT: usize = 100;
const ATTEMPT_LIMIT: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RequestStatus {
    Routing,
    Waiting,
    Checking,
    Connecting,
    Streaming,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteTarget {
    pub account_id: String,
    pub account_label: String,
    pub provider_id: String,
    pub model: String,
    pub position: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteAttempt {
    pub target: RouteTarget,
    pub outcome: &'static str,
    pub reason: &'static str,
    pub retry_at: Option<i64>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestReport {
    pub id: String,
    pub pool: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration_ms: u64,
    pub streaming: bool,
    pub status: RequestStatus,
    /// Candidate being checked or provider currently serving the request.
    pub target: Option<RouteTarget>,
    pub attempts: Vec<RouteAttempt>,
    pub omitted_attempts: usize,
    pub fallback_count: usize,
    pub http_status: Option<u16>,
    pub message: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingReport {
    pub active: Vec<RequestReport>,
    pub recent: Vec<RequestReport>,
    pub history_limit: usize,
    pub attempt_limit: usize,
}

struct ActiveReport {
    report: RequestReport,
    started: Instant,
}
#[derive(Default)]
struct ReportState {
    sequence: u64,
    active: BTreeMap<String, ActiveReport>,
    recent: VecDeque<RequestReport>,
}
#[derive(Default)]
pub struct RoutingReports(Mutex<ReportState>);
impl RoutingReports {
    fn lock(&self) -> MutexGuard<'_, ReportState> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
    pub fn snapshot(&self) -> RoutingReport {
        let state = self.lock();
        let mut active: Vec<_> = state
            .active
            .values()
            .map(|entry| {
                let mut report = entry.report.clone();
                report.duration_ms = elapsed(entry.started);
                report
            })
            .collect();
        active.sort_by(|a, b| {
            b.started_at
                .cmp(&a.started_at)
                .then_with(|| b.id.cmp(&a.id))
        });
        RoutingReport {
            active,
            recent: state.recent.iter().cloned().collect(),
            history_limit: HISTORY_LIMIT,
            attempt_limit: ATTEMPT_LIMIT,
        }
    }
    pub fn clear_history(&self) {
        self.lock().recent.clear();
    }
    pub fn begin(self: &Arc<Self>, pool: String, streaming: bool) -> RequestTrace {
        let mut state = self.lock();
        state.sequence += 1;
        let id = state.sequence.to_string();
        state.active.insert(
            id.clone(),
            ActiveReport {
                started: Instant::now(),
                report: RequestReport {
                    id: id.clone(),
                    pool,
                    started_at: chrono::Utc::now().timestamp(),
                    finished_at: None,
                    duration_ms: 0,
                    streaming,
                    status: RequestStatus::Routing,
                    target: None,
                    attempts: vec![],
                    omitted_attempts: 0,
                    fallback_count: 0,
                    http_status: None,
                    message: "Finding the requested model pool.",
                },
            },
        );
        RequestTrace {
            reports: self.clone(),
            id,
            finished: false,
        }
    }
}

/// Ownership follows the request, then moves to its streaming worker. Dropping a
/// cancelled HTTP future also finalizes its report without spawning cleanup work.
pub struct RequestTrace {
    reports: Arc<RoutingReports>,
    id: String,
    finished: bool,
}
impl RequestTrace {
    pub fn id(&self) -> &str {
        &self.id
    }
    fn update(&self, change: impl FnOnce(&mut RequestReport)) {
        if let Some(active) = self.reports.lock().active.get_mut(&self.id) {
            change(&mut active.report);
        }
    }
    pub fn progress(&self, target: RouteTarget, status: RequestStatus, message: &'static str) {
        self.update(|report| {
            report.fallback_count = target.position.saturating_sub(1);
            report.target = Some(target);
            report.status = status;
            report.message = message;
        });
    }
    pub fn attempt(
        &self,
        target: RouteTarget,
        outcome: &'static str,
        reason: &'static str,
        retry_at: Option<i64>,
    ) {
        self.update(|report| {
            report.fallback_count = target.position.saturating_sub(1);
            if outcome == "skipped" {
                report.target = None;
            }
            if report.attempts.len() == ATTEMPT_LIMIT {
                report.attempts.remove(0);
                report.omitted_attempts += 1;
            }
            report.attempts.push(RouteAttempt {
                target,
                outcome,
                reason,
                retry_at,
            });
        });
    }
    pub fn finish(
        mut self,
        status: RequestStatus,
        http_status: Option<u16>,
        message: &'static str,
    ) {
        self.complete(status, http_status, message);
    }
    fn complete(&mut self, status: RequestStatus, http_status: Option<u16>, message: &'static str) {
        let mut state = self.reports.lock();
        if let Some(mut active) = state.active.remove(&self.id) {
            active.report.status = status;
            active.report.http_status = http_status;
            active.report.message = message;
            active.report.finished_at = Some(chrono::Utc::now().timestamp());
            active.report.duration_ms = elapsed(active.started);
            state.recent.push_front(active.report);
            state.recent.truncate(HISTORY_LIMIT);
        }
        self.finished = true;
    }
}
impl Drop for RequestTrace {
    fn drop(&mut self) {
        if !self.finished {
            self.complete(
                RequestStatus::Cancelled,
                None,
                "The client disconnected or the request was cancelled.",
            );
        }
    }
}
fn elapsed(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_is_bounded_and_clearing_never_loses_active_requests() {
        let reports = Arc::new(RoutingReports::default());
        let active = reports.begin("still-streaming".into(), true);
        for _ in 0..120 {
            reports.begin("example-pool".into(), false).finish(
                RequestStatus::Completed,
                Some(200),
                "Completed.",
            );
        }
        assert_eq!(reports.snapshot().recent.len(), 100);
        assert_eq!(reports.snapshot().active.len(), 1);
        reports.clear_history();
        assert!(reports.snapshot().recent.is_empty());
        assert_eq!(reports.snapshot().active[0].pool, "still-streaming");
        drop(active);
        assert!(reports.snapshot().active.is_empty());
        assert_eq!(
            reports.snapshot().recent[0].status,
            RequestStatus::Cancelled
        );
    }
    #[test]
    fn long_pools_keep_latest_attempts_and_the_exact_position() {
        let reports = Arc::new(RoutingReports::default());
        let trace = reports.begin("example-pool".into(), false);
        for position in 1..=80 {
            trace.attempt(
                RouteTarget {
                    account_id: "example".into(),
                    account_label: "Example".into(),
                    provider_id: "fixture".into(),
                    model: "small".into(),
                    position,
                },
                "skipped",
                "Allowance exhausted.",
                Some(500),
            );
        }
        let snapshot = reports.snapshot();
        assert_eq!(snapshot.active[0].attempts.len(), 64);
        assert_eq!(snapshot.active[0].omitted_attempts, 16);
        assert_eq!(snapshot.active[0].fallback_count, 79);
        assert_eq!(snapshot.active[0].attempts[0].target.position, 17);
        assert!(snapshot.active[0].target.is_none());
    }
}
