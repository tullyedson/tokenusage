use super::*;
use crate::model::{number, timestamp, SettingField, UsageMeter};

pub struct Ollama;
impl IUsageProvider for Ollama {
    fn inference(&self) -> Option<Arc<dyn crate::routing::engine::IInferenceProvider>> {
        use crate::routing::subscriptions::{SubscriptionKind, SubscriptionProvider};
        Some(Arc::new(SubscriptionProvider::new(
            SubscriptionKind::OllamaCloud,
        )))
    }
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "ollama", name: "Ollama Cloud", category: "llm", initials: "Ol", color: "#d2d8e0", description: "Read included monthly credits and any hourly, session or weekly allowances shown in Ollama settings.", help_url: "https://ollama.com/settings", fields: vec![SettingField::secret("api_key", "Ollama API key (routing)", "For model routing only. The website usage reader still uses your sign-in. Save a key for the same account; leave blank to preserve it.")] }
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn preserves_independent_reset_times_in_usage_meters() {
        let snapshot = Ollama
            .parse(
                json!({ "windows": [
                    { "label": "Monthly usage", "used": 6, "limit": 20, "resetsAt": "2026-10-12T09:15:00-05:00" },
                    { "label": "Session usage", "usedPercent": 35, "resetsAt": "2026-09-08T19:00:00Z" },
                    { "label": "Weekly usage", "usedPercent": 74, "resetsAt": "2026-09-12T16:00:00Z" }
                ] }),
                &ProviderConfig::default(),
            )
            .unwrap();
        assert_eq!(snapshot.meters.len(), 3);
        assert_eq!(snapshot.meters[0].resets_at, Some(1_791_814_500));
        assert_eq!(snapshot.meters[1].resets_at, Some(1_788_894_000));
        assert_eq!(snapshot.meters[2].resets_at, Some(1_789_228_800));
        assert_eq!(snapshot.meters[0].remaining, Some(14.0));
        assert_eq!(snapshot.meters[1].percent_left, Some(65.0));
        let payload = serde_json::to_value(snapshot).unwrap();
        assert_eq!(payload["meters"][2]["resetsAt"], 1_789_228_800_i64);
    }

    #[test]
    fn missing_or_invalid_resets_do_not_discard_valid_usage_or_invent_a_date() {
        let snapshot = Ollama
            .parse(
                json!({ "windows": [
                    { "label": "Hourly usage", "usedPercent": 10, "resetsAt": null },
                    { "label": "Weekly usage", "usedPercent": 70, "resetsAt": "invalid" }
                ] }),
                &ProviderConfig::default(),
            )
            .unwrap();
        assert_eq!(snapshot.meters.len(), 2);
        assert!(snapshot
            .meters
            .iter()
            .all(|meter| meter.resets_at.is_none()));
        assert_eq!(snapshot.meters[0].percent_left, Some(90.0));
        assert_eq!(snapshot.meters[1].percent_left, Some(30.0));
    }
}
