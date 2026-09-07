//! Numeric call measurements only. Request and response content never enters reports.
use serde::Serialize;
use serde_json::Value;

const MAX_COUNT: u64 = 9_007_199_254_740_991;

/// Compatible adapters request the standard numeric usage event. Preserve an
/// explicit caller opt-out and every other streaming option.
pub fn request_stream_usage(body: &mut Value) {
    if body["stream"] != true {
        return;
    }
    let Some(body) = body.as_object_mut() else {
        return;
    };
    let options = body
        .entry("stream_options")
        .or_insert_with(|| serde_json::json!({}));
    if let Some(options) = options.as_object_mut() {
        options.entry("include_usage").or_insert(Value::Bool(true));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input: Option<u64>,
    pub output: Option<u64>,
    pub total: Option<u64>,
    pub cached_input: Option<u64>,
    pub reasoning: Option<u64>,
}

fn count(value: &Value) -> Option<u64> {
    value.as_u64().filter(|value| *value <= MAX_COUNT)
}
impl TokenUsage {
    pub fn from_response(value: &Value) -> Option<Self> {
        let usage = value.get("usage")?.as_object()?;
        for name in ["prompt_tokens", "completion_tokens", "total_tokens"] {
            if usage
                .get(name)
                .is_some_and(|value| !value.is_null() && count(value).is_none())
            {
                return None;
            }
        }
        let input = usage.get("prompt_tokens").and_then(count);
        let output = usage.get("completion_tokens").and_then(count);
        let reported_total = usage.get("total_tokens").and_then(count);
        let sum = input
            .zip(output)
            .and_then(|(a, b)| a.checked_add(b))
            .filter(|sum| *sum <= MAX_COUNT);
        if reported_total
            .zip(sum)
            .is_some_and(|(reported, sum)| reported != sum)
            || reported_total.is_some_and(|total| {
                input.is_some_and(|input| input > total)
                    || output.is_some_and(|output| output > total)
            })
        {
            return None;
        }
        let total = reported_total.or(sum);
        if input.is_none() && output.is_none() && total.is_none() {
            return None;
        }
        let cached_input = count(&value["usage"]["prompt_tokens_details"]["cached_tokens"])
            .filter(|cached| input.is_some_and(|input| *cached <= input));
        let reasoning = count(&value["usage"]["completion_tokens_details"]["reasoning_tokens"])
            .filter(|reasoning| output.is_some_and(|output| *reasoning <= output));
        Some(Self {
            input,
            output,
            total,
            cached_input,
            reasoning,
        })
    }
}

#[derive(Clone)]
pub struct AllowanceWindow {
    pub id: &'static str,
    pub label: &'static str,
    pub used_percent: f64,
    pub resets_at: i64,
}
#[derive(Clone)]
pub struct AllowanceSnapshot {
    pub checked_at: i64,
    pub windows: Vec<AllowanceWindow>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowanceChange {
    pub label: &'static str,
    pub before_percent: f64,
    pub after_percent: f64,
    pub percentage_points: f64,
}
#[derive(Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AllowanceStatus {
    Pending,
    Observed,
    Unavailable,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllowanceObservation {
    pub status: AllowanceStatus,
    pub changes: Vec<AllowanceChange>,
}
impl AllowanceObservation {
    pub fn unavailable() -> Self {
        Self {
            status: AllowanceStatus::Unavailable,
            changes: vec![],
        }
    }
    pub fn compare(before: &AllowanceSnapshot, after: &AllowanceSnapshot) -> Self {
        if after.checked_at < before.checked_at {
            return Self::unavailable();
        }
        let changes = before
            .windows
            .iter()
            .take(8)
            .filter_map(|old| {
                let new = after.windows.iter().find(|new| new.id == old.id)?;
                // Provider reset timestamps may round to the nearest second. Never compare
                // across a reset or a decreasing/invalid counter.
                if old.resets_at.abs_diff(new.resets_at) > 2
                    || after.checked_at >= old.resets_at.min(new.resets_at)
                    || !old.used_percent.is_finite()
                    || !new.used_percent.is_finite()
                    || old.used_percent < 0.0
                    || new.used_percent < old.used_percent
                {
                    return None;
                }
                Some(AllowanceChange {
                    label: old.label,
                    before_percent: old.used_percent,
                    after_percent: new.used_percent,
                    percentage_points: new.used_percent - old.used_percent,
                })
            })
            .collect::<Vec<_>>();
        if changes.is_empty() {
            Self::unavailable()
        } else {
            Self {
                status: AllowanceStatus::Observed,
                changes,
            }
        }
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallMetrics {
    pub request_bytes: Option<u64>,
    pub response_bytes: Option<u64>,
    pub tokens: Option<TokenUsage>,
    pub context_limit: Option<u64>,
    pub context_used_percent: Option<f64>,
    pub allowance: Option<AllowanceObservation>,
}
impl CallMetrics {
    pub fn context_percentage(&mut self) {
        self.context_used_percent = self
            .tokens
            .and_then(|tokens| tokens.total)
            .zip(self.context_limit.filter(|limit| *limit > 0))
            .map(|(tokens, limit)| tokens as f64 / limit as f64 * 100.0);
    }
}

#[cfg(test)]
mod tests;
