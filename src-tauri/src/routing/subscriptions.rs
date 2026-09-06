use super::engine::{
    read_json, retry_time, IInferenceProvider, InferenceContext, InferenceDefinition,
    PreparedRequest, RouteFailure,
};
use crate::model::{number, timestamp, FieldOption, ProviderConfig, SettingField};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Url,
};
use serde_json::Value;
use std::time::Duration;
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub enum SubscriptionKind {
    OpenCodeGo,
    OllamaCloud,
}

pub struct SubscriptionProvider {
    kind: SubscriptionKind,
    base: Url,
}

impl SubscriptionKind {
    fn billing_mode(self) -> &'static str {
        match self {
            Self::OpenCodeGo => "go_included_only",
            Self::OllamaCloud => "ollama_legacy_limits",
        }
    }

    pub fn billing_field(self) -> SettingField {
        let (label, help) = match self {
            Self::OpenCodeGo => (
                "Go only: Use balance off, no BYOK",
                "Before enabling routing, turn off Use balance in the Go workspace and remove any bring-your-own-provider keys there. The Go API does not expose these billing settings. Confirm below only after checking them; the router also checks all three Go quota windows before each request.",
            ),
            Self::OllamaCloud => (
                "Legacy session/weekly plan, no extra credits",
                "Included-only routing currently supports the legacy session/weekly plan that stops at its limits. Do not select this for a monthly-credit plan or an account with extra usage credits. Ollama can spend those automatically. Disable routing before changing your plan or adding extra credits.",
            ),
        };
        SettingField {
            key: "routing_billing",
            label: "Subscription billing",
            kind: "select",
            help,
            placeholder: "",
            options: vec![
                FieldOption {
                    value: "unconfirmed",
                    label: "Usage only until billing is checked",
                },
                FieldOption {
                    value: self.billing_mode(),
                    label,
                },
            ],
        }
    }
}

impl SubscriptionProvider {
    pub fn new(kind: SubscriptionKind) -> Self {
        Self {
            kind,
            base: match kind {
                SubscriptionKind::OpenCodeGo => "https://opencode.ai/zen/go/v1/",
                SubscriptionKind::OllamaCloud => "https://ollama.com/v1/",
            }
            .parse()
            .expect("Constant subscription API URL"),
        }
    }

    fn key(&self, ctx: &InferenceContext<'_>) -> Result<Zeroizing<String>, String> {
        ctx.secrets
            .get(ctx.account_id, "api_key")?
            .filter(|key| !key.is_empty())
            .ok_or_else(|| "Save this account's API key before listing or routing models.".into())
    }

    async fn get(
        &self,
        ctx: &InferenceContext<'_>,
        path: &str,
        key: &str,
    ) -> Result<Value, RouteFailure> {
        let mut response = ctx
            .client
            .get(self.base.join(path).expect("Constant API path"))
            .bearer_auth(key)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|_| {
                unavailable(
                    "Subscription availability could not be checked.",
                    ctx.now + 15,
                    true,
                )
            })?;
        if !response.status().is_success() {
            let reason = match response.status().as_u16() {
                401 | 403 => "Subscription authorization failed. Check the saved account key.",
                429 => "Subscription rate limit reached.",
                _ => "Subscription availability could not be checked.",
            };
            return Err(unavailable(
                reason,
                retry_time(response.headers(), ctx.now),
                true,
            ));
        }
        read_json(&mut response, 4 * 1024 * 1024)
            .await
            .map_err(|_| {
                unavailable(
                    "Subscription returned an unsupported response.",
                    ctx.now + 30,
                    true,
                )
            })
    }

    async fn catalog(
        &self,
        ctx: &InferenceContext<'_>,
        key: &str,
    ) -> Result<Vec<String>, RouteFailure> {
        let value = self.get(ctx, "models", key).await?;
        let rows = value["data"].as_array().ok_or_else(|| {
            unavailable(
                "Subscription model catalog is unavailable.",
                ctx.now + 30,
                true,
            )
        })?;
        Ok(rows
            .iter()
            .filter_map(|row| row["id"].as_str())
            .filter(|id| super::config::valid_model(id))
            .take(4096)
            .map(str::to_owned)
            .collect())
    }
}

fn unavailable(reason: &'static str, retry_at: i64, account_wide: bool) -> RouteFailure {
    RouteFailure::Unavailable {
        reason,
        retry_at,
        account_wide,
    }
}

/// The provider-side billing settings are a prerequisite, not a quota estimate.
/// Go's endpoint does not reveal Use balance or BYOK settings; the owner must
/// confirm them. Ollama credit plans are deliberately not accepted yet.
#[async_trait]
impl IInferenceProvider for SubscriptionProvider {
    fn definition(&self) -> InferenceDefinition {
        InferenceDefinition { description: match self.kind {
            SubscriptionKind::OpenCodeGo => "OpenCode Go chat models using included allowances. Confirm subscription billing first. Every request checks five-hour, weekly and monthly quotas; depleted accounts are skipped until their reset.",
            SubscriptionKind::OllamaCloud => "Ollama Cloud chat models using a legacy session/weekly subscription. Requires its API key and confirmation that extra credits are unavailable. Provider limit responses move requests to the next eligible account.",
        } }
    }

    fn validate(&self, config: &ProviderConfig) -> Result<(), String> {
        if config.routing.enabled && config.field("routing_billing") != self.kind.billing_mode() {
            return Err("Check the provider's billing settings and select the included-only subscription option before enabling routing.".into());
        }
        Ok(())
    }

    async fn models(&self, ctx: &InferenceContext<'_>) -> Result<Vec<String>, String> {
        self.catalog(ctx, &self.key(ctx)?)
            .await
            .map_err(|failure| match failure {
                RouteFailure::Unavailable { reason, .. }
                | RouteFailure::Invalid(reason)
                | RouteFailure::Failed(reason) => reason.into(),
            })
    }

    async fn prepare(
        &self,
        ctx: &InferenceContext<'_>,
        request: &Value,
        upstream: &str,
    ) -> Result<PreparedRequest, RouteFailure> {
        if ctx.config.field("routing_billing") != self.kind.billing_mode() {
            return Err(unavailable(
                "Included-only billing has not been confirmed for this subscription.",
                ctx.now + 60,
                true,
            ));
        }
        let key = self.key(ctx).map_err(|_| {
            unavailable(
                "Save the subscription API key before routing.",
                ctx.now + 60,
                true,
            )
        })?;
        let catalog = self.catalog(ctx, &key).await?;
        if !catalog.iter().any(|id| id == upstream) {
            return Err(unavailable(
                "This model is not in the subscription catalog.",
                ctx.now + 60,
                false,
            ));
        }
        let mut headers = HeaderMap::new();
        headers.insert(
            "user-agent",
            HeaderValue::from_static(concat!("AI-Usage/", env!("CARGO_PKG_VERSION"))),
        );
        if matches!(self.kind, SubscriptionKind::OpenCodeGo) {
            check_go_allowance(&self.get(ctx, "usage", &key).await?, ctx.now)?;
            let session = ctx
                .session_id
                .filter(|id| super::engine::valid_session(id))
                .ok_or(RouteFailure::Invalid(
                    "OpenCode Go needs a valid request session.",
                ))?;
            headers.insert(
                "x-opencode-session",
                HeaderValue::from_str(session)
                    .map_err(|_| RouteFailure::Invalid("Invalid request session."))?,
            );
            headers.insert("x-opencode-client", HeaderValue::from_static("ai-usage"));
        }
        let mut body = request.clone();
        body["model"] = Value::String(upstream.into());
        Ok(PreparedRequest {
            url: self
                .base
                .join("chat/completions")
                .expect("Constant chat path"),
            body,
            key: Some(key),
            headers,
        })
    }
}

fn check_go_allowance(value: &Value, now: i64) -> Result<(), RouteFailure> {
    let invalid = || {
        unavailable(
            "All three Go allowances must be readable before routing.",
            now + 30,
            true,
        )
    };
    let mut blocked_until = None;
    for name in ["rolling", "weekly", "monthly"] {
        let window = &value["usage"][name];
        let percent = number(&window["percent"])
            .filter(|v| *v >= 0.0)
            .ok_or_else(invalid)?;
        let reset = timestamp(&window["resetsAt"]).ok_or_else(invalid)?;
        let status = window["status"]
            .as_str()
            .filter(|s| matches!(*s, "ok" | "rate-limited"))
            .ok_or_else(invalid)?;
        if status == "rate-limited" || percent >= 100.0 {
            let retry = if reset > now { reset } else { now + 30 };
            blocked_until = Some(blocked_until.map_or(retry, |old: i64| old.max(retry)));
        } else if reset <= now {
            // A past reset is not proof of newly available usage.
            return Err(invalid());
        }
    }
    if let Some(reset) = blocked_until {
        return Err(unavailable(
            "OpenCode Go included allowance is exhausted.",
            reset,
            true,
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
