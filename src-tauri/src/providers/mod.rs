//! To add a provider: implement IUsageProvider in one module and register it below.
//! The UI builds settings and usage cards entirely from this public contract.
mod anthropic;
mod higgsfield;
mod local;
mod ollama;
mod openai;
mod opencode;
mod openrouter;
mod suno;

use crate::{
    browser::BrowserSession,
    credentials::ISecretStore,
    model::{ProviderConfig, ProviderDefinition, UsageSnapshot},
};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::{atomic::AtomicBool, Arc};

pub struct FetchContext {
    pub account_id: String,
    pub browser: BrowserSession,
    pub cancelled: Arc<AtomicBool>,
    pub secrets: Arc<dyn ISecretStore>,
}

pub enum ConnectionOutcome {
    BrowserOpened,
    Ready,
}

#[derive(Clone)]
pub struct BrowserSpec {
    pub url: &'static str,
    pub hosts: &'static [&'static str],
    pub script: &'static str,
}

#[async_trait]
pub trait IUsageProvider: Send + Sync {
    fn definition(&self) -> ProviderDefinition;
    fn inference(&self) -> Option<Arc<dyn crate::routing::engine::IInferenceProvider>> {
        None
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        None
    }
    fn parse(&self, value: Value, config: &ProviderConfig) -> Result<UsageSnapshot, String>;
    async fn connect(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<ConnectionOutcome, String> {
        context
            .browser
            .sign_in(
                &context.account_id,
                self.browser_spec()
                    .ok_or("This provider needs an API key connection.")?,
                config,
            )
            .await?;
        Ok(ConnectionOutcome::BrowserOpened)
    }
    async fn fetch(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<UsageSnapshot, String> {
        let value = context
            .browser
            .read(
                &context.account_id,
                self.browser_spec()
                    .ok_or("This provider needs an API key connection.")?,
                config,
                &context.cancelled,
            )
            .await?;
        self.parse(value, config)
    }
}

pub fn registry() -> Vec<Arc<dyn IUsageProvider>> {
    vec![
        Arc::new(openai::OpenAi),
        Arc::new(anthropic::Anthropic),
        Arc::new(ollama::Ollama),
        Arc::new(openrouter::OpenRouter),
        Arc::new(opencode::OpenCode),
        Arc::new(suno::Suno),
        Arc::new(higgsfield::Higgsfield),
        Arc::new(local::LocalModels(
            crate::routing::providers::HttpProvider::OllamaLocal,
        )),
        Arc::new(local::LocalModels(
            crate::routing::providers::HttpProvider::VllmLocal,
        )),
    ]
}
