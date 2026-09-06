use super::*;
use crate::model::{number, timestamp, SettingField, UsageMeter};

pub struct Anthropic;
impl IUsageProvider for Anthropic {
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "anthropic", name: "Anthropic", category: "llm", initials: "An", color: "#dba185", description: "Claude Pro, Max and Team allowances. Sign in to Claude, then return here to refresh.", help_url: "https://claude.ai/settings/usage", fields: vec![SettingField::text("organization", "Organization ID", "Usually automatic. If you belong to multiple Claude organizations, choose the ID for the subscription to track.")] }
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        Some(BrowserSpec {
            url: "https://claude.ai/settings/usage",
            hosts: &["claude.ai"],
            script: include_str!("scripts/anthropic.js"),
        })
    }
    fn parse(&self, v: Value, _config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = vec![];
        for (key, label) in [
            ("five_hour", "5-hour allowance"),
            ("seven_day", "Weekly allowance"),
            ("seven_day_opus", "Weekly Opus"),
            ("seven_day_sonnet", "Weekly Sonnet"),
            ("seven_day_oauth_apps", "Weekly connected apps"),
            ("seven_day_cowork", "Weekly Cowork"),
        ] {
            if let Some(used) = number(&v[key]["utilization"]) {
                meters.push(UsageMeter::used_percent(
                    label,
                    used,
                    timestamp(&v[key]["resets_at"]),
                )?);
            }
        }
        if v["extra_usage"]["is_enabled"].as_bool() == Some(true) {
            if let (Some(limit), Some(used)) = (
                number(&v["extra_usage"]["monthly_limit"]),
                number(&v["extra_usage"]["used_credits"]),
            ) {
                let mut meter = UsageMeter::balance(
                    "Extra usage",
                    (limit - used) / 100.0,
                    Some(limit / 100.0),
                    "USD",
                    None,
                )?;
                meter.note = Some("Monthly extra-usage spending limit".into());
                meters.push(meter);
            }
        }
        UsageSnapshot::new(meters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinct_claude_windows_and_optional_fields() {
        let v = serde_json::json!({"five_hour":{"utilization":12,"resets_at":"2026-09-07T10:00:00Z"},"seven_day":{"utilization":80},"seven_day_sonnet":null});
        let s = Anthropic.parse(v, &ProviderConfig::default()).unwrap();
        assert_eq!(s.meters.len(), 2);
        assert_eq!(s.meters[0].percent_left, Some(88.0));
        assert!(s.meters[0].resets_at.is_some());
        assert!(Anthropic
            .parse(serde_json::json!({}), &ProviderConfig::default())
            .is_err());
    }
}
