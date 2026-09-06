//! Native inference adapters. No provider credentials or prompts are logged.
use super::engine::*;
use crate::model::ProviderConfig;
use async_trait::async_trait;
use reqwest::Url;
use serde_json::{json, Value};
use std::net::IpAddr;

#[derive(Clone, Copy)]
pub enum HttpProvider {
    OllamaLocal,
    VllmLocal,
    OpenRouterFree,
}

pub fn local_base(config: &ProviderConfig, default: &str) -> Result<Url, String> {
    let value = config.field("base_url");
    let mut url = Url::parse(if value.is_empty() { default } else { value })
        .map_err(|_| "Enter a valid local server URL.")?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/" | "/v1" | "/v1/")
    {
        return Err(
            "Use a local HTTP(S) server root or /v1 URL without credentials, query or fragment."
                .into(),
        );
    }
    if url.host_str() == Some("localhost") {
        url.set_host(Some("127.0.0.1"))
            .map_err(|_| "Invalid local host.")?;
    }
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches('[')
        .trim_end_matches(']');
    let allowed = match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => ip.is_loopback() || ip.is_private(),
        Ok(IpAddr::V6(ip)) => ip.is_loopback() || ip.is_unique_local(),
        Err(_) => false,
    };
    if !allowed {
        return Err(
            "Local models require localhost, a loopback IP, or a private LAN IP address.".into(),
        );
    }
    url.set_path("/");
    Ok(url)
}

impl HttpProvider {
    fn base(&self, config: &ProviderConfig) -> Result<Url, String> {
        match self {
            Self::OllamaLocal => local_base(config, "http://127.0.0.1:11434"),
            Self::VllmLocal => local_base(config, "http://127.0.0.1:8000"),
            Self::OpenRouterFree => {
                Url::parse("https://openrouter.ai/api/").map_err(|_| "Invalid provider URL.".into())
            }
        }
    }
    fn key(
        &self,
        ctx: &InferenceContext<'_>,
    ) -> Result<Option<zeroize::Zeroizing<String>>, String> {
        let key = ctx.secrets.get(ctx.account_id, "api_key")?;
        if matches!(self, Self::OpenRouterFree) && key.is_none() {
            return Err("Save a standard OpenRouter API key first.".into());
        }
        Ok(key)
    }
    async fn catalog(&self, ctx: &InferenceContext<'_>) -> Result<Value, String> {
        let path = if matches!(self, Self::OllamaLocal) {
            "api/tags"
        } else {
            "v1/models"
        };
        let mut request = ctx
            .client
            .get(
                self.base(ctx.config)?
                    .join(path)
                    .map_err(|_| "Invalid model endpoint.")?,
            )
            .timeout(std::time::Duration::from_secs(10));
        if let Some(key) = self.key(ctx)? {
            request = request.bearer_auth(key.as_str());
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Could not reach the model server.")?;
        if !response.status().is_success() {
            return Err("Could not list models. Check the server URL and key.".into());
        }
        read_json(&mut response, 4 * 1024 * 1024).await
    }
}

pub fn is_local_ollama(row: &Value) -> bool {
    row.get("remote_model").is_none_or(Value::is_null)
        && row.get("remote_host").is_none_or(Value::is_null)
        && row["details"]["parameter_size"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && row["details"]["quantization_level"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
        && !row["name"].as_str().unwrap_or("").contains("cloud")
}

pub fn is_free_model(row: &Value) -> bool {
    row["id"]
        .as_str()
        .is_some_and(|id| id.ends_with(":free") && !id.starts_with("openrouter/"))
        && row["pricing"].as_object().is_some_and(|prices| {
            ["prompt", "completion"]
                .iter()
                .all(|key| crate::model::number(&row["pricing"][key]) == Some(0.0))
                && prices
                    .values()
                    .all(|p| p.is_null() || crate::model::number(p) == Some(0.0))
        })
}

#[async_trait]
impl IInferenceProvider for HttpProvider {
    fn definition(&self) -> InferenceDefinition {
        InferenceDefinition { description: match self {
            Self::OllamaLocal => "Local Ollama models only. Cloud-backed models are excluded. Supports chat completions and streaming.",
            Self::VllmLocal => "Your local or LAN vLLM server. Supports chat completions and streaming using the model IDs served by vLLM.",
            Self::OpenRouterFree => "Standard API key required. Only explicit :free models with zero catalog prices are eligible. Paid plugins and provider-side model fallbacks are blocked. Free usage is checked by the provider on each request.",
        } }
    }
    fn validate(&self, config: &ProviderConfig) -> Result<(), String> {
        self.base(config)?;
        if matches!(self, Self::OpenRouterFree) && config.routing.enabled {
            if config.field("connection") != "key" {
                return Err(
                    "Routing needs the standard-key connection, not a management key.".into(),
                );
            }
            if config
                .routing
                .models
                .iter()
                .any(|m| !m.upstream.ends_with(":free") || m.upstream.starts_with("openrouter/"))
            {
                return Err("OpenRouter routing accepts explicit :free model IDs only.".into());
            }
        }
        Ok(())
    }
    async fn models(&self, ctx: &InferenceContext<'_>) -> Result<Vec<String>, String> {
        self.validate(ctx.config)?;
        let catalog = self.catalog(ctx).await?;
        let (key, name) = if matches!(self, Self::OllamaLocal) {
            ("models", "name")
        } else {
            ("data", "id")
        };
        let rows = catalog[key]
            .as_array()
            .ok_or("The server returned an unsupported model catalog.")?;
        Ok(rows
            .iter()
            .filter(|row| match self {
                Self::OllamaLocal => is_local_ollama(row),
                Self::OpenRouterFree => is_free_model(row),
                Self::VllmLocal => true,
            })
            .filter_map(|row| {
                row[name]
                    .as_str()
                    .filter(|id| super::config::valid_model(id))
                    .map(str::to_string)
            })
            .take(4096)
            .collect())
    }
    async fn prepare(
        &self,
        ctx: &InferenceContext<'_>,
        request: &Value,
        upstream: &str,
    ) -> Result<PreparedRequest, RouteFailure> {
        self.validate(ctx.config)
            .map_err(|_| RouteFailure::Invalid("The account routing settings are invalid."))?;
        let models = self
            .models(ctx)
            .await
            .map_err(|_| RouteFailure::Unavailable {
                reason: "Model availability could not be verified.",
                retry_at: chrono::Utc::now().timestamp() + 15,
                account_wide: true,
            })?;
        if !models.iter().any(|id| id == upstream) {
            return Err(RouteFailure::Unavailable {
                reason: "Model is unavailable or does not meet the local/free-only policy.",
                retry_at: chrono::Utc::now().timestamp() + 30,
                account_wide: false,
            });
        }
        // A local alias can point at a remote model. Verify metadata immediately before
        // generation, instead of relying solely on its user-chosen name or cached tags.
        if matches!(self, Self::OllamaLocal) {
            let url = self
                .base(ctx.config)
                .map_err(|_| RouteFailure::Invalid("Invalid Ollama URL."))?
                .join("api/show")
                .map_err(|_| RouteFailure::Invalid("Invalid Ollama URL."))?;
            let mut call = ctx
                .client
                .post(url)
                .json(&json!({"model": upstream}))
                .timeout(std::time::Duration::from_secs(10));
            if let Some(key) = self
                .key(ctx)
                .map_err(|_| RouteFailure::Invalid("Could not read the saved key."))?
            {
                call = call.bearer_auth(key.as_str());
            }
            let mut response = call
                .send()
                .await
                .map_err(|_| RouteFailure::Failed("Could not verify local model metadata."))?;
            if !response.status().is_success() {
                return Err(RouteFailure::Failed(
                    "Could not verify local model metadata.",
                ));
            }
            let metadata = read_json(&mut response, 4 * 1024 * 1024)
                .await
                .map_err(|_| RouteFailure::Failed("Could not verify local model metadata."))?;
            if !is_local_ollama(&metadata) {
                return Err(RouteFailure::Unavailable {
                    reason: "This Ollama model is not confirmed local.",
                    retry_at: chrono::Utc::now().timestamp() + 30,
                    account_wide: false,
                });
            }
        }
        let mut body = request.clone();
        body["model"] = json!(upstream);
        if matches!(self, Self::OpenRouterFree) {
            body["provider"] = json!({"max_price":{"prompt":0,"completion":0,"request":0,"image":0},"allow_fallbacks":false});
        }
        Ok(PreparedRequest {
            headers: Default::default(),
            url: self
                .base(ctx.config)
                .map_err(|_| RouteFailure::Invalid("Invalid server URL."))?
                .join("v1/chat/completions")
                .map_err(|_| RouteFailure::Invalid("Invalid chat endpoint."))?,
            body,
            key: self
                .key(ctx)
                .map_err(|_| RouteFailure::Invalid("Save the account's API key before routing."))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn local_urls_reject_public_hosts_credentials_and_arbitrary_paths() {
        for url in [
            "https://example.com",
            "http://169.254.169.254",
            "http://127.0.0.1/admin",
            "http://key@127.0.0.1",
            "http://127.0.0.1?key=secret",
        ] {
            let config = ProviderConfig {
                fields: [("base_url".into(), url.into())].into(),
                ..Default::default()
            };
            assert!(local_base(&config, "").is_err(), "{url}");
        }
        for url in [
            "http://localhost:11434/v1",
            "http://192.168.1.20:8000",
            "http://[::1]:8000",
        ] {
            let config = ProviderConfig {
                fields: [("base_url".into(), url.into())].into(),
                ..Default::default()
            };
            assert!(local_base(&config, "").is_ok(), "{url}");
        }
    }
    #[test]
    fn free_and_local_claims_are_verified_from_metadata() {
        assert!(is_free_model(
            &json!({"id":"example/small:free","pricing":{"prompt":"0","completion":"0"}})
        ));
        assert!(!is_free_model(
            &json!({"id":"example/small:free","pricing":{"prompt":"0.1","completion":"0"}})
        ));
        assert!(!is_free_model(
            &json!({"id":"openrouter/auto:free","pricing":{"prompt":"0","completion":"0"}})
        ));
        assert!(!is_local_ollama(
            &json!({"name":"innocent-alias","remote_model":"remote","details":{"parameter_size":"1B","quantization_level":"Q4"}})
        ));
        assert!(!is_local_ollama(&json!({"name":"unknown"})));
    }
}
