use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub initials: &'static str,
    pub color: &'static str,
    pub description: &'static str,
    pub help_url: &'static str,
    pub fields: Vec<SettingField>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingField {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: &'static str,
    pub help: &'static str,
    pub placeholder: &'static str,
    pub options: Vec<FieldOption>,
}

#[derive(Clone, Serialize)]
pub struct FieldOption {
    pub value: &'static str,
    pub label: &'static str,
}

impl SettingField {
    pub fn text(key: &'static str, label: &'static str, help: &'static str) -> Self {
        Self {
            key,
            label,
            kind: "text",
            help,
            placeholder: "Optional",
            options: vec![],
        }
    }
    pub fn number(key: &'static str, label: &'static str, help: &'static str) -> Self {
        Self {
            kind: "number",
            ..Self::text(key, label, help)
        }
    }
    pub fn secret(key: &'static str, label: &'static str, help: &'static str) -> Self {
        Self {
            kind: "secret",
            placeholder: "Paste a key",
            ..Self::text(key, label, help)
        }
    }
}

#[derive(Clone, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default)]
    pub provider_type: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub routing: crate::routing::config::AccountRouting,
    pub enabled: bool,
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    #[serde(default)]
    pub session_generation: u64,
    #[serde(default)]
    pub revision: u64,
}

impl ProviderConfig {
    pub fn provider_type<'a>(&'a self, account_id: &'a str) -> &'a str {
        if self.provider_type.is_empty() {
            account_id
        } else {
            &self.provider_type
        }
    }
    pub fn field(&self, key: &str) -> &str {
        self.fields.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn allowance(&self) -> Option<f64> {
        self.field("allowance")
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v > 0.0)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub refresh_minutes: u64,
    pub providers: BTreeMap<String, ProviderConfig>,
    #[serde(default)]
    pub routing: crate::routing::config::RouterSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 2,
            refresh_minutes: 5,
            providers: BTreeMap::new(),
            routing: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageMeter {
    pub label: String,
    pub remaining: Option<f64>,
    pub limit: Option<f64>,
    pub percent_left: Option<f64>,
    pub unit: String,
    pub resets_at: Option<i64>,
    pub note: Option<String>,
}

impl UsageMeter {
    pub fn used_percent(
        label: impl Into<String>,
        used: f64,
        reset: Option<i64>,
    ) -> Result<Self, String> {
        if !used.is_finite() || used < 0.0 {
            return Err("The provider returned an invalid usage percentage.".into());
        }
        let left = (100.0 - used).clamp(0.0, 100.0);
        Ok(Self {
            label: label.into(),
            remaining: None,
            limit: None,
            percent_left: Some(left),
            unit: "%".into(),
            resets_at: reset,
            note: None,
        })
    }
    pub fn balance(
        label: impl Into<String>,
        left: f64,
        limit: Option<f64>,
        unit: &str,
        reset: Option<i64>,
    ) -> Result<Self, String> {
        if !left.is_finite() || limit.is_some_and(|v| !v.is_finite() || v < 0.0) {
            return Err("The provider returned an invalid balance.".into());
        }
        let percent = limit
            .filter(|v| *v > 0.0)
            .map(|v| (left / v * 100.0).clamp(0.0, 100.0));
        Ok(Self {
            label: label.into(),
            remaining: Some(left.max(0.0)),
            limit,
            percent_left: percent,
            unit: unit.into(),
            resets_at: reset,
            note: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub meters: Vec<UsageMeter>,
    pub plan: Option<String>,
    pub note: Option<String>,
}

impl UsageSnapshot {
    pub fn new(meters: Vec<UsageMeter>) -> Result<Self, String> {
        if meters.is_empty() {
            return Err(
                "No usage allowances were returned. Sign in or check the provider's usage page."
                    .into(),
            );
        }
        if meters.len() > 32 {
            return Err("The provider returned too many usage meters.".into());
        }
        Ok(Self {
            meters,
            plan: None,
            note: None,
        })
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReport {
    pub provider_id: String,
    pub snapshot: Option<UsageSnapshot>,
    pub updated_at: Option<i64>,
    pub attempted_at: Option<i64>,
    pub error: Option<String>,
    pub refreshing: bool,
}

impl ProviderReport {
    pub fn empty(id: &str) -> Self {
        Self {
            provider_id: id.into(),
            snapshot: None,
            updated_at: None,
            attempted_at: None,
            error: None,
            refreshing: false,
        }
    }
}

pub fn number(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse::<f64>().ok())
        .filter(|v| v.is_finite())
}

pub fn timestamp(value: &serde_json::Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        chrono::DateTime::parse_from_rfc3339(value.as_str()?)
            .ok()
            .map(|v| v.timestamp())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percent_means_remaining_and_overages_do_not_wrap() {
        assert_eq!(
            UsageMeter::used_percent("Weekly", 24.0, None)
                .unwrap()
                .percent_left,
            Some(76.0)
        );
        assert_eq!(
            UsageMeter::used_percent("Weekly", 110.0, None)
                .unwrap()
                .percent_left,
            Some(0.0)
        );
        assert!(UsageMeter::used_percent("Bad", f64::NAN, None).is_err());
    }
    #[test]
    fn unknown_or_zero_denominator_never_invents_a_percent() {
        assert_eq!(
            UsageMeter::balance("Credits", 40.0, None, "credits", None)
                .unwrap()
                .percent_left,
            None
        );
        assert_eq!(
            UsageMeter::balance("Credits", 0.0, Some(0.0), "credits", None)
                .unwrap()
                .percent_left,
            None
        );
        assert_eq!(
            UsageMeter::balance("Credits", 120.0, Some(100.0), "credits", None)
                .unwrap()
                .remaining,
            Some(120.0)
        );
    }
}
