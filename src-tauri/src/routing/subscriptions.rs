use super::engine::{
    read_json, retry_time, IInferenceProvider, InferenceContext, InferenceDefinition,
    PreparedRequest, RouteFailure,
};
use super::metadata::{enrich_ollama, InferenceModel, ModelLimits, PublishedLimits};
use crate::model::{number, timestamp, ProviderConfig};
use async_trait::async_trait;
use reqwest::{
    header::{HeaderMap, HeaderValue},
    Url,
};
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use zeroize::Zeroizing;

#[derive(Clone, Copy)]
pub enum SubscriptionKind {
    OpenCodeGo,
    OllamaCloud,
}

pub struct SubscriptionProvider {
    kind: SubscriptionKind,
    base: Url,
    metadata: Arc<PublishedLimits>,
}

impl SubscriptionProvider {
    pub fn new(kind: SubscriptionKind) -> Self {
        Self {
            kind,
            metadata: PublishedLimits::shared(),
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
    ) -> Result<Vec<InferenceModel>, RouteFailure> {
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
            .filter_map(|row| {
                row["id"]
                    .as_str()
                    .filter(|id| super::config::valid_model(id))
                    .map(|id| InferenceModel {
                        id: id.into(),
                        limits: ModelLimits::from_catalog(row),
                    })
            })
            .take(4096)
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

/// Plan-only is the default. Provider-side paid overages must remain disabled;
/// these APIs do not expose a per-request switch that can enforce that setting.
#[async_trait]
impl IInferenceProvider for SubscriptionProvider {
    fn definition(&self) -> InferenceDefinition {
        InferenceDefinition { description: match self.kind {
            SubscriptionKind::OpenCodeGo => "OpenCode Go chat models using included allowances. Keep Use balance off and remove workspace BYOK. Every request checks five-hour, weekly and monthly quotas; depleted accounts are skipped until their reset.",
            SubscriptionKind::OllamaCloud => "Ollama Cloud chat models using a legacy session/weekly subscription. Use a plan that stops at its limits, without extra credits or automatic top-ups. Provider limit responses move requests to the next eligible account.",
        } }
    }

    fn validate(&self, _config: &ProviderConfig) -> Result<(), String> {
        Ok(())
    }

    async fn models(&self, ctx: &InferenceContext<'_>) -> Result<Vec<InferenceModel>, String> {
        let key = self.key(ctx)?;
        let (catalog, published) = tokio::join!(
            self.catalog(ctx, &key),
            self.metadata.read(ctx.client, ctx.now)
        );
        let mut models = catalog.map_err(|failure| match failure {
            RouteFailure::Unavailable { reason, .. }
            | RouteFailure::Invalid(reason)
            | RouteFailure::Failed(reason) => reason.to_owned(),
        })?;
        let provider = match self.kind {
            SubscriptionKind::OpenCodeGo => "opencode-go",
            SubscriptionKind::OllamaCloud => "ollama-cloud",
        };
        for model in &mut models {
            if let Some(limits) = published
                .get(provider)
                .and_then(|models| models.get(&model.id))
            {
                model.limits = model.limits.fill_missing(*limits);
            }
        }
        if matches!(self.kind, SubscriptionKind::OllamaCloud) {
            let base = self.base.join("../").expect("Constant Ollama root");
            enrich_ollama(ctx.client, &base, Some(&key), &mut models, false).await;
        }
        Ok(models)
    }

    async fn prepare(
        &self,
        ctx: &InferenceContext<'_>,
        request: &Value,
        upstream: &str,
    ) -> Result<PreparedRequest, RouteFailure> {
        let key = self.key(ctx).map_err(|_| {
            unavailable(
                "Save the subscription API key before routing.",
                ctx.now + 60,
                true,
            )
        })?;
        let catalog = self.catalog(ctx, &key).await?;
        if !catalog.iter().any(|model| model.id == upstream) {
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
