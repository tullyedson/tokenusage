use super::*;
use crate::model::{number, timestamp, SettingField, UsageMeter};

pub struct Higgsfield;
impl IUsageProvider for Higgsfield {
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "higgsfield", name: "Higgsfield", category: "media", initials: "Hi", color: "#b8a3ef", description: "Read the subscription wallet for the workspace selected in your Higgsfield account.", help_url: "https://higgsfield.ai/me/settings/subscription", fields: vec![SettingField::number("allowance", "Total-credit reference allowance", "Optional denominator for the total-credit bar. Subscription credits use the allowance reported by Higgsfield.")] }
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        Some(BrowserSpec {
            url: "https://higgsfield.ai/me/settings/subscription",
            hosts: &["higgsfield.ai"],
            script: include_str!("scripts/higgsfield.js"),
        })
    }
    fn parse(&self, v: Value, config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = vec![];
        let reset = timestamp(&v["next_credit_allocation_date"]);
        if let Some(left) = number(&v["subscription_balance"]) {
            meters.push(UsageMeter::balance(
                "Subscription credits",
                left,
                number(&v["total_credits"]),
                "credits",
                reset,
            )?);
        }
        if let Some(left) = number(&v["credits_balance"]) {
            let mut meter =
                UsageMeter::balance("Total wallet", left, config.allowance(), "credits", None)?;
            meter.note = Some("Wallet balance, including purchased credits".into());
            meters.push(meter);
        }
        if let Some(left) = number(&v["on_demand_credits"]).filter(|v| *v > 0.0) {
            meters.push(UsageMeter::balance(
                "Auto-refill credits",
                left,
                None,
                "credits",
                None,
            )?);
        }
        UsageSnapshot::new(meters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wallet_keeps_separate_allowances_and_actual_balance() {
        let s = Higgsfield.parse(serde_json::json!({"subscription_balance":400,"total_credits":1000,"credits_balance":600,"on_demand_credits":200}), &ProviderConfig::default()).unwrap();
        assert_eq!(s.meters.len(), 3);
        assert_eq!(s.meters[0].percent_left, Some(40.0));
        assert_eq!(s.meters[1].percent_left, None);
    }
}
