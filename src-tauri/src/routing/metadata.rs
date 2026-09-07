//! Model limits are provider/account metadata, never inferred from a model name.
use super::engine::read_json;
use futures_util::{stream, StreamExt};
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::Mutex;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ModelLimits {
    pub context: Option<u64>,
    pub input: Option<u64>,
    pub output: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InferenceModel {
    pub id: String,
    pub limits: ModelLimits,
}
impl From<&str> for InferenceModel {
    fn from(id: &str) -> Self {
        Self {
            id: id.into(),
            limits: ModelLimits::default(),
        }
    }
}

fn tokens(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .filter(|value| (1..=9_007_199_254_740_991).contains(value))
}
impl ModelLimits {
    pub fn from_catalog(row: &Value) -> Self {
        Self {
            context: [
                tokens(&row["max_model_len"]),
                tokens(&row["context_length"]),
                tokens(&row["limit"]["context"]),
                tokens(&row["top_provider"]["context_length"]),
            ]
            .into_iter()
            .flatten()
            .min(),
            input: tokens(&row["limit"]["input"]).or_else(|| tokens(&row["max_input_tokens"])),
            output: [
                tokens(&row["limit"]["output"]),
                tokens(&row["max_output_tokens"]),
                tokens(&row["top_provider"]["max_completion_tokens"]),
            ]
            .into_iter()
            .flatten()
            .min(),
        }
        .bounded()
    }
    pub fn bounded(mut self) -> Self {
        for value in [&mut self.context, &mut self.input, &mut self.output] {
            *value = value.filter(|tokens| (1..=9_007_199_254_740_991).contains(tokens));
        }
        if let Some(context) = self.context {
            self.input = self.input.map(|value| value.min(context));
            self.output = self.output.map(|value| value.min(context));
        }
        self
    }
    pub fn fill_missing(self, fallback: Self) -> Self {
        Self {
            context: self.context.or(fallback.context),
            input: self.input.or(fallback.input),
            output: self.output.or(fallback.output),
        }
        .bounded()
    }
    /// Every enabled chain member must have a known bound. Unknown is not infinity.
    pub fn intersection(limits: &[Self]) -> Self {
        fn lowest(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
            values.collect::<Option<Vec<_>>>()?.into_iter().min()
        }
        Self {
            context: lowest(limits.iter().map(|limit| limit.context)),
            input: if limits.iter().any(|limit| limit.input.is_some()) {
                lowest(limits.iter().map(|limit| limit.input.or(limit.context)))
            } else {
                None
            },
            output: lowest(limits.iter().map(|limit| limit.output)),
        }
        .bounded()
    }
}

type ProviderLimits = BTreeMap<String, BTreeMap<String, ModelLimits>>;
struct Cached {
    attempted: i64,
    success: bool,
    data: Arc<ProviderLimits>,
}

/// OpenCode's own published catalog supplements ID-only subscription endpoints.
/// No credentials are sent here. Only exact provider/model matches are retained.
pub struct PublishedLimits {
    url: reqwest::Url,
    cache: Mutex<Option<Cached>>,
}
impl PublishedLimits {
    pub fn shared() -> Arc<Self> {
        static INSTANCE: OnceLock<Arc<PublishedLimits>> = OnceLock::new();
        INSTANCE
            .get_or_init(|| {
                Arc::new(Self::new(
                    "https://models.dev/api.json"
                        .parse()
                        .expect("Constant catalog URL"),
                ))
            })
            .clone()
    }
    pub(super) fn new(url: reqwest::Url) -> Self {
        Self {
            url,
            cache: Mutex::new(None),
        }
    }
    pub async fn read(&self, client: &reqwest::Client, now: i64) -> Arc<ProviderLimits> {
        let mut cache = self.cache.lock().await;
        if let Some(cached) = cache.as_ref().filter(|cached| {
            now >= cached.attempted
                && now - cached.attempted < if cached.success { 300 } else { 30 }
        }) {
            return cached.data.clone();
        }
        let result = async {
            let mut response = client
                .get(self.url.clone())
                .header(
                    "user-agent",
                    concat!("AI-Usage/", env!("CARGO_PKG_VERSION")),
                )
                .timeout(Duration::from_secs(8))
                .send()
                .await
                .ok()?;
            if !response.status().is_success() {
                return None;
            }
            let doc = read_json(&mut response, 16 * 1024 * 1024).await.ok()?;
            let mut data = ProviderLimits::new();
            for provider in ["opencode-go", "ollama-cloud"] {
                if let Some(models) = doc[provider]["models"].as_object() {
                    data.insert(
                        provider.into(),
                        models
                            .iter()
                            .filter(|(id, _)| super::config::valid_model(id))
                            .take(4096)
                            .map(|(id, row)| (id.clone(), ModelLimits::from_catalog(row)))
                            .collect(),
                    );
                }
            }
            if data.is_empty() {
                None
            } else {
                Some(data)
            }
        }
        .await;
        let success = result.is_some();
        let data = Arc::new(result.unwrap_or_default());
        // Expired metadata is not a current capacity claim when its refresh fails.
        *cache = Some(Cached {
            attempted: now,
            success,
            data: data.clone(),
        });
        data
    }
}

pub fn ollama_limits(value: &Value, local: bool) -> ModelLimits {
    let architecture = value["model_info"]["general.architecture"]
        .as_str()
        .unwrap_or("");
    let trained = tokens(&value["model_info"][format!("{architecture}.context_length")]);
    let context = if local {
        // A trained maximum does not reveal the server's VRAM-dependent default.
        // OpenAI-compatible clients cannot set num_ctx. Require a model parameter.
        value["parameters"]
            .as_str()
            .and_then(|parameters| {
                parameters
                    .lines()
                    .filter_map(|line| {
                        let mut words = line.split_whitespace();
                        if words.next()? != "num_ctx" {
                            return None;
                        }
                        let value = words.next()?.parse::<u64>().ok()?;
                        (value > 0 && value <= 9_007_199_254_740_991).then_some(value)
                    })
                    .next_back()
            })
            .map(|configured| trained.map_or(configured, |max| configured.min(max)))
    } else {
        trained
    };
    ModelLimits {
        context,
        ..Default::default()
    }
}

/// Read-only /api/show enrichment, bounded independently so names remain usable
/// when an optional metadata endpoint is slow or unavailable. Dropping cancels it.
pub async fn enrich_ollama(
    client: &reqwest::Client,
    base: &reqwest::Url,
    key: Option<&str>,
    models: &mut [InferenceModel],
    local: bool,
) {
    let url = base.join("api/show").expect("Validated Ollama base URL");
    let pending = models
        .iter()
        .enumerate()
        .map(|(index, model)| {
            let url = url.clone();
            let id = model.id.clone();
            async move {
                let mut request = client
                    .post(url)
                    .json(&json!({"model":id}))
                    .timeout(Duration::from_secs(3));
                if let Some(key) = key {
                    request = request.bearer_auth(key);
                }
                let limits = async {
                    let mut response = request.send().await.ok()?;
                    if !response.status().is_success() {
                        return None;
                    }
                    let value = read_json(&mut response, 4 * 1024 * 1024).await.ok()?;
                    if local && !super::providers::is_local_ollama(&value) {
                        return None;
                    }
                    Some(ollama_limits(&value, local))
                }
                .await;
                (index, limits)
            }
        })
        .collect::<Vec<_>>();
    let mut pending = stream::iter(pending).buffer_unordered(8);
    let deadline = tokio::time::sleep(Duration::from_secs(4));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            _ = &mut deadline => break,
            next = pending.next() => match next {
                Some((index, Some(limits))) => models[index].limits = limits.fill_missing(models[index].limits),
                Some((_, None)) => {},
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests;
