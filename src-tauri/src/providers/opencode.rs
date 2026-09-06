use super::*;
use crate::{
    http,
    model::{number, timestamp, SettingField, UsageMeter},
};

pub struct OpenCode;
#[async_trait]
impl IUsageProvider for OpenCode {
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "opencode", name: "OpenCode", category: "llm", initials: "OC", color: "#e0dcd3", description: "OpenCode Go subscription allowances: five-hour, weekly and monthly usage. Zen pay-as-you-go credits are separate.", help_url: "https://opencode.ai/auth", fields: vec![SettingField::secret("api_key", "OpenCode API key", "Use a key from the workspace with your Go subscription. Saved in Windows Credential Manager; leave blank to keep the saved key.")] }
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
        let key = context
            .secrets
            .get("opencode", "api_key")?
            .ok_or("Save your OpenCode API key in Settings, then connect.")?;
        let value = http::get_usage("https://opencode.ai/zen/go/v1/usage", &key, &context.cancelled, "This key needs an OpenCode Go subscription in its workspace. Zen pay-as-you-go credits are separate from Go allowances.").await?;
        self.parse(value, config)
    }
    fn parse(&self, value: Value, _: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = Vec::new();
        for (key, label) in [
            ("rolling", "5-hour allowance"),
            ("weekly", "Weekly allowance"),
            ("monthly", "Monthly allowance"),
        ] {
            let window = &value["usage"][key];
            let used = number(&window["percent"])
                .ok_or("OpenCode did not return all three Go usage windows.")?;
            let reset = timestamp(&window["resetsAt"])
                .ok_or("OpenCode returned an unsupported reset time.")?;
            if !matches!(window["status"].as_str(), Some("ok" | "rate-limited")) {
                return Err("OpenCode returned an unsupported usage status.".into());
            }
            let mut meter = UsageMeter::used_percent(label, used, Some(reset))?;
            if window["status"] == "rate-limited" {
                meter.percent_left = Some(0.0);
                meter.note = Some("This allowance is currently rate limited by OpenCode.".into());
            }
            meters.push(meter);
        }
        let mut snapshot = UsageSnapshot::new(meters)?;
        snapshot.plan = Some("Go subscription".into());
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn response() -> Value {
        json!({"usage":{
            "rolling":{"status":"ok","percent":25,"resetsAt":"2026-09-07T05:00:00Z"},
            "weekly":{"status":"rate-limited","percent":100,"resetsAt":"2026-09-14T00:00:00Z"},
            "monthly":{"status":"ok","percent":60,"resetsAt":"2026-10-01T00:00:00Z"}
        }})
    }
    #[test]
    fn all_go_windows_report_remaining_percent_and_their_own_reset() {
        let snapshot = OpenCode
            .parse(response(), &ProviderConfig::default())
            .unwrap();
        assert_eq!(snapshot.meters.len(), 3);
        assert_eq!(
            snapshot
                .meters
                .iter()
                .map(|m| m.percent_left)
                .collect::<Vec<_>>(),
            vec![Some(75.0), Some(0.0), Some(40.0)]
        );
        assert!(snapshot.meters[0].resets_at < snapshot.meters[1].resets_at);
        assert!(snapshot.meters.iter().all(|m| m.limit.is_none()));
    }
    #[test]
    fn missing_windows_and_invalid_resets_are_errors_not_fresh_balances() {
        assert!(OpenCode
            .parse(json!({"usage":{}}), &ProviderConfig::default())
            .is_err());
        let mut value = response();
        value["usage"]["rolling"]["resetsAt"] = json!("bad");
        assert!(OpenCode.parse(value, &ProviderConfig::default()).is_err());
        let mut value = response();
        value["usage"]["rolling"]["percent"] = json!(-1);
        assert!(OpenCode.parse(value, &ProviderConfig::default()).is_err());
    }
}
