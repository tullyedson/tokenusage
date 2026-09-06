use super::*;
use crate::model::{number, timestamp, SettingField, UsageMeter};

pub struct Suno;
impl IUsageProvider for Suno {
    fn definition(&self) -> ProviderDefinition {
        ProviderDefinition { id: "suno", name: "Suno", category: "music", initials: "Su", color: "#edb276", description: "Read subscription and total credits from your signed-in Suno account.", help_url: "https://suno.com/account", fields: vec![SettingField::number("allowance", "Total-credit reference allowance", "Optional. Used only for the total-credit bar when Suno does not supply its denominator. Include top-ups if that is the balance you want to compare.")] }
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        Some(BrowserSpec {
            url: "https://suno.com/account",
            hosts: &["suno.com", "www.suno.com"],
            script: include_str!("scripts/suno.js"),
        })
    }
    fn parse(&self, v: Value, config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = vec![];
        let reset = timestamp(&v["period_end"]).or_else(|| timestamp(&v["current_period_end"]));
        if let (Some(limit), Some(used)) =
            (number(&v["monthly_limit"]), number(&v["monthly_usage"]))
        {
            meters.push(UsageMeter::balance(
                "Monthly credits",
                limit - used,
                Some(limit),
                "credits",
                reset,
            )?);
        }
        if let Some(left) = number(&v["total_credits_left"]) {
            let mut meter =
                UsageMeter::balance("Total credits", left, config.allowance(), "credits", None)?;
            meter.note = Some(
                if config.allowance().is_some() {
                    "Includes top-ups; percentage uses your reference allowance"
                } else {
                    "Includes available top-ups; set a reference allowance for a percentage"
                }
                .into(),
            );
            meters.push(meter);
        }
        UsageSnapshot::new(meters)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn total_is_not_divided_by_monthly_when_topups_are_present() {
        let s = Suno.parse(serde_json::json!({"total_credits_left":3200,"monthly_limit":2500,"monthly_usage":1000}), &ProviderConfig::default()).unwrap();
        assert_eq!(s.meters[0].percent_left, Some(60.0));
        assert_eq!(s.meters[1].remaining, Some(3200.0));
        assert_eq!(s.meters[1].percent_left, None);
    }
}
