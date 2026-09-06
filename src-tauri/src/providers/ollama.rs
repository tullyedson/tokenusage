use super::*;
use crate::model::{number, timestamp, UsageMeter};

pub struct Ollama;
impl IUsageProvider for Ollama {
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "ollama", name: "Ollama Cloud", category: "llm", initials: "Ol", color: "#d2d8e0", description: "Read included monthly credits and any hourly, session or weekly allowances shown in Ollama settings.", help_url: "https://ollama.com/settings", fields: vec![] }
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        Some(BrowserSpec {
            url: "https://ollama.com/settings",
            hosts: &["ollama.com"],
            script: include_str!("scripts/ollama.js"),
        })
    }
    fn parse(&self, v: Value, _config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = vec![];
        for row in v["windows"]
            .as_array()
            .ok_or("Ollama usage data is unavailable.")?
        {
            let label = row["label"]
                .as_str()
                .ok_or("Ollama returned an unnamed allowance.")?;
            let reset = timestamp(&row["resetsAt"]);
            if let (Some(used), Some(limit)) = (number(&row["used"]), number(&row["limit"])) {
                meters.push(UsageMeter::balance(
                    label,
                    limit - used,
                    Some(limit),
                    "USD",
                    reset,
                )?);
            } else if let Some(used) = number(&row["usedPercent"]) {
                meters.push(UsageMeter::used_percent(label, used, reset)?);
            }
        }
        UsageSnapshot::new(meters)
    }
}
