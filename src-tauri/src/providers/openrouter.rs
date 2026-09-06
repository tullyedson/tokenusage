use super::*;
use crate::{
    http,
    model::{number, FieldOption, SettingField, UsageMeter},
};

pub struct OpenRouter;
#[async_trait]
impl IUsageProvider for OpenRouter {
    fn definition(&self) -> ProviderDefinition {
        let mut connection = SettingField::text("connection", "Usage source", "Account credits require an OpenRouter management key. A standard key reports only its own spending allowance.");
        connection.kind = "select";
        connection.options = vec![
            FieldOption {
                value: "credits",
                label: "Account credits (management key)",
            },
            FieldOption {
                value: "key",
                label: "This key's allowance (standard key)",
            },
        ];
        ProviderDefinition { id: "openrouter", name: "OpenRouter", category: "llm", initials: "OR", color: "#afa8f4", description: "Account credit balance or a single API key's remaining spending allowance.", help_url: "https://openrouter.ai/settings/keys", fields: vec![connection, SettingField::secret("api_key", "OpenRouter key", "Saved in Windows Credential Manager. Leave blank to keep the saved key. Usage checks never generate tokens.")] }
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
            .get("openrouter", "api_key")?
            .ok_or("Save an OpenRouter key in Settings, then connect.")?;
        let url = if config.field("connection") == "key" {
            "https://openrouter.ai/api/v1/key"
        } else {
            "https://openrouter.ai/api/v1/credits"
        };
        let value = http::get_usage(url, &key, &context.cancelled, "This key cannot read the selected usage source. Account credits require a management key; use This key's allowance with a standard key.").await?;
        self.parse(value, config)
    }
    fn parse(&self, value: Value, config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let data = value
            .get("data")
            .filter(|data| data.is_object())
            .ok_or("OpenRouter returned an unsupported usage response.")?;
        let meter = if config.field("connection") == "key" {
            let limit_value = data
                .get("limit")
                .ok_or("OpenRouter did not return this key's spending limit.")?;
            if limit_value.is_null() {
                if number(&data["usage"]).filter(|v| *v >= 0.0).is_none() {
                    return Err("OpenRouter did not return key usage.".into());
                }
                UsageMeter { label: "Key spending allowance".into(), remaining: None, limit: None, percent_left: None, unit: "USD".into(), resets_at: None, note: Some("This key has no spending cap. Its account credit balance is not included in this reading.".into()) }
            } else {
                let limit = number(limit_value)
                    .filter(|v| *v >= 0.0)
                    .ok_or("OpenRouter returned an invalid key limit.")?;
                let left = number(&data["limit_remaining"])
                    .ok_or("OpenRouter did not return the key's remaining allowance.")?;
                let reset = data.get("limit_reset").and_then(Value::as_str);
                let label = match reset {
                    Some("daily") => "Daily key allowance",
                    Some("weekly") => "Weekly key allowance",
                    Some("monthly") => "Monthly key allowance",
                    _ => "Key spending allowance",
                };
                let mut meter = UsageMeter::balance(label, left, Some(limit), "USD", None)?;
                meter.note = Some(
                    match reset {
                        Some("daily") => "This key only. Resets at midnight UTC.",
                        Some("weekly") => "This key only. Resets Monday at midnight UTC.",
                        Some("monthly") => {
                            "This key only. Resets on the first day of the month at midnight UTC."
                        }
                        _ => "This key's spending cap, separate from the account's credit balance.",
                    }
                    .into(),
                );
                meter
            }
        } else {
            let total = number(&data["total_credits"])
                .filter(|v| *v >= 0.0)
                .ok_or("OpenRouter did not return total account credits.")?;
            let used = number(&data["total_usage"])
                .filter(|v| *v >= 0.0)
                .ok_or("OpenRouter did not return total account usage.")?;
            let mut meter =
                UsageMeter::balance("Account credits", total - used, Some(total), "USD", None)?;
            meter.note = Some("Percentage compares the remaining balance with total purchased credits; this is not a monthly allowance.".into());
            meter
        };
        let mut snapshot = UsageSnapshot::new(vec![meter])?;
        snapshot.plan = Some(
            if config.field("connection") == "key" {
                "API key allowance"
            } else {
                "Account credits"
            }
            .into(),
        );
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn key_config() -> ProviderConfig {
        ProviderConfig {
            fields: std::collections::BTreeMap::from([("connection".into(), "key".into())]),
            ..Default::default()
        }
    }
    #[test]
    fn account_balance_uses_purchased_credits_and_excludes_identity() {
        let snapshot = OpenRouter.parse(json!({"data":{"total_credits":100,"total_usage":25,"label":"fictional-private-key"}}), &ProviderConfig::default()).unwrap();
        assert_eq!(snapshot.meters[0].remaining, Some(75.0));
        assert_eq!(snapshot.meters[0].percent_left, Some(75.0));
        assert!(!serde_json::to_string(&snapshot)
            .unwrap()
            .contains("fictional"));
        assert!(snapshot.meters[0].resets_at.is_none());
    }
    #[test]
    fn resetting_key_uses_authoritative_remaining_instead_of_lifetime_usage() {
        let snapshot = OpenRouter.parse(json!({"data":{"limit":50,"limit_remaining":40,"limit_reset":"monthly","usage":1000,"usage_monthly":10}}), &key_config()).unwrap();
        assert_eq!(snapshot.meters[0].percent_left, Some(80.0));
        assert_eq!(snapshot.meters[0].label, "Monthly key allowance");
    }
    #[test]
    fn uncapped_keys_zero_credits_and_overages_do_not_invent_allowances() {
        let unlimited = OpenRouter
            .parse(
                json!({"data":{"limit":null,"limit_remaining":null,"usage":12}}),
                &key_config(),
            )
            .unwrap();
        assert_eq!(unlimited.meters[0].percent_left, None);
        assert_eq!(unlimited.meters[0].remaining, None);
        let empty = OpenRouter
            .parse(
                json!({"data":{"total_credits":0,"total_usage":0}}),
                &ProviderConfig::default(),
            )
            .unwrap();
        assert_eq!(empty.meters[0].percent_left, None);
        let over = OpenRouter
            .parse(
                json!({"data":{"limit":5,"limit_remaining":-1}}),
                &key_config(),
            )
            .unwrap();
        assert_eq!(over.meters[0].percent_left, Some(0.0));
        assert!(OpenRouter
            .parse(json!({"data":{"limit":50}}), &key_config())
            .is_err());
        assert!(OpenRouter
            .parse(
                json!({"data":{"total_credits":10}}),
                &ProviderConfig::default()
            )
            .is_err());
    }
}
