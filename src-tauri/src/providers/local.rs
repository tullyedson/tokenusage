use super::*;
use crate::{
    model::{SettingField, UsageMeter},
    routing::{
        engine::{IInferenceProvider, InferenceContext},
        providers::HttpProvider,
    },
};

pub struct LocalModels(pub HttpProvider);
#[async_trait]
impl IUsageProvider for LocalModels {
    fn definition(&self) -> ProviderDefinition {
        let (id, name, initials, help, url) = match self.0 {
            HttpProvider::OllamaLocal => (
                "ollama-local",
                "Ollama (local)",
                "Ol",
                "https://docs.ollama.com/api/openai-compatibility",
                "http://127.0.0.1:11434",
            ),
            _ => (
                "vllm-local",
                "vLLM (local)",
                "vL",
                "https://docs.vllm.ai/en/latest/serving/online_serving/openai_compatible_server/",
                "http://127.0.0.1:8000",
            ),
        };
        let mut base = SettingField::text(
            "base_url",
            "Server URL",
            "Use localhost, a private LAN IP or a .local hostname. The server must already be running.",
        );
        base.placeholder = url;
        ProviderDefinition { id, name, category: "llm", initials, color: "#82bbec", description: "Models running on your own hardware. There is no subscription quota. Create model pools on Models after connecting.", help_url: help, fields: vec![base, SettingField::secret("api_key", "Server API key", "Optional for local Ollama. Use your vLLM server key if authentication is enabled.")] }
    }
    fn inference(&self) -> Option<Arc<dyn IInferenceProvider>> {
        Some(Arc::new(self.0))
    }
    async fn connect(
        &self,
        _: &FetchContext,
        _: &ProviderConfig,
    ) -> Result<ConnectionOutcome, String> {
        Ok(ConnectionOutcome::Ready)
    }
    async fn fetch(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<UsageSnapshot, String> {
        let client = crate::routing::network::client_builder()
            .build()
            .map_err(|_| "Could not initialize local server connection.")?;
        let inference_context = InferenceContext {
            account_id: &context.account_id,
            config,
            secrets: context.secrets.as_ref(),
            client: &client,
            now: chrono::Utc::now().timestamp(),
            session_id: None,
        };
        let names = tokio::select! {
            result = self.0.models(&inference_context) => result?,
            _ = async { while !context.cancelled.load(std::sync::atomic::Ordering::Acquire) { tokio::time::sleep(std::time::Duration::from_millis(100)).await; } } => return Err("Refresh cancelled.".into()),
        };
        let mut snapshot = self.parse(serde_json::json!(names.len()), config)?;
        snapshot.note = Some(format!("{} local model(s) available.", names.len()));
        Ok(snapshot)
    }
    fn parse(&self, _: Value, _: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut snapshot = UsageSnapshot::new(vec![UsageMeter {
            label: "Local capacity".into(),
            remaining: None,
            limit: None,
            percent_left: None,
            unit: "local".into(),
            resets_at: None,
            note: Some(
                "No subscription allowance. Availability depends on your server and hardware."
                    .into(),
            ),
        }])?;
        snapshot.plan = Some("Local models".into());
        Ok(snapshot)
    }
}
