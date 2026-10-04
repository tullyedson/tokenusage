use super::*;
use crate::model::{number, timestamp, FieldOption, SettingField, UsageMeter};
use std::collections::BTreeSet;

pub struct Google;

fn source(config: &ProviderConfig) -> Result<&str, String> {
    match config.field("connection") {
        "" | "gemini" => Ok("gemini"),
        "antigravity" => Ok("antigravity"),
        _ => Err("Choose a supported Google usage source in Settings.".into()),
    }
}

#[async_trait]
impl IUsageProvider for Google {
    fn definition(&self) -> ProviderDefinition {
        let mut connection = SettingField::text("connection", "Usage source", "Google products have separate allowances. Add a connection for each product you use. Antigravity follows the account signed in to its desktop app; website connections have their own sign-in.");
        connection.kind = "select";
        connection.options = vec![
            FieldOption {
                value: "gemini",
                label: "Gemini app (Google sign-in)",
            },
            FieldOption {
                value: "antigravity",
                label: "Antigravity (signed-in desktop app)",
            },
        ];
        ProviderDefinition {
            id: "google", name: "Google AI Ultra", category: "llm", initials: "G", color: "#8ab4f8",
            description: "Google AI subscription allowances, reported separately for each product. Connect the Google account with your Ultra plan. Usage monitoring only.",
            show_in_usage: true, help_url: "https://support.google.com/googleone/answer/16286513", fields: vec![connection],
        }
    }
    fn browser_spec(&self) -> Option<BrowserSpec> {
        Some(BrowserSpec {
            url: "https://gemini.google.com/usage?hl=en",
            hosts: &["gemini.google.com"],
            script: include_str!("scripts/google-gemini.js"),
        })
    }
    async fn connect(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<ConnectionOutcome, String> {
        if source(config)? == "antigravity" {
            return Ok(ConnectionOutcome::Ready);
        }
        context
            .browser
            .sign_in(
                &context.account_id,
                self.browser_spec()
                    .ok_or("Missing Google website connection.")?,
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
        let value = if source(config)? == "antigravity" {
            super::google_antigravity::read(&context.cancelled).await?
        } else {
            context
                .browser
                .read(
                    &context.account_id,
                    self.browser_spec()
                        .ok_or("Missing Google website connection.")?,
                    config,
                    &context.cancelled,
                )
                .await?
        };
        self.parse(value, config)
    }
    fn parse(&self, value: Value, config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        match source(config)? {
            "antigravity" => parse_antigravity(&value),
            _ => parse_gemini(&value),
        }
    }
}

fn fraction(value: &Value) -> Option<f64> {
    number(value).filter(|v| (0.0..=1.0).contains(v))
}
fn reset(value: &Value) -> Option<i64> {
    timestamp(value).filter(|v| (1..=253_402_300_799).contains(v))
}
fn safe_label(value: &Value) -> Option<String> {
    value
        .as_str()
        .filter(|s| {
            !s.trim().is_empty() && s.chars().count() <= 100 && !s.chars().any(char::is_control)
        })
        .map(str::to_string)
}

fn parse_gemini(value: &Value) -> Result<UsageSnapshot, String> {
    if value["source"] != "gemini" {
        return Err("Gemini returned an unsupported usage response.".into());
    }
    let mut meters = vec![];
    let mut seen = BTreeSet::new();
    for window in value["windows"]
        .as_array()
        .ok_or("Gemini did not return usage windows.")?
    {
        let label = match window["kind"].as_str() {
            Some("five_hour") => "Gemini app - 5-hour allowance",
            Some("weekly") => "Gemini app - weekly allowance",
            _ => continue,
        };
        if !seen.insert(label) {
            return Err("Gemini returned duplicate usage windows.".into());
        }
        let used = fraction(&window["usedFraction"])
            .ok_or("Gemini returned an invalid usage fraction.")?;
        meters.push(UsageMeter::used_percent(
            label,
            used * 100.0,
            reset(&window["resetsAt"]),
        )?);
    }
    if meters.is_empty() {
        return Err("No Gemini app usage allowances were returned.".into());
    }
    if let Some(amount) = number(&value["credits"]).filter(|n| *n >= 0.0) {
        let mut meter = UsageMeter::balance("Google AI credits", amount, None, "credits", None)?;
        meter.note = Some(
            "Separate extra-usage balance reported by Gemini, not your included Gemini allowance."
                .into(),
        );
        meters.push(meter);
    }
    let mut result = UsageSnapshot::new(meters)?;
    result.plan = match value["plan"].as_str() {
        Some(plan @ ("Google AI Ultra" | "Google AI Pro" | "Google AI Plus")) => Some(plan.into()),
        _ => None,
    };
    result.note = Some("Gemini app limits. Add a separate Antigravity connection to track its included allowances.".into());
    Ok(result)
}

fn parse_antigravity(value: &Value) -> Result<UsageSnapshot, String> {
    let groups = value["response"]["groups"]
        .as_array()
        .ok_or("Antigravity did not return quota groups. Update Antigravity and try again.")?;
    let mut meters = vec![];
    let mut seen = BTreeSet::new();
    for group in groups {
        let name = safe_label(&group["displayName"])
            .ok_or("Antigravity returned an unnamed quota group.")?;
        for bucket in group["buckets"]
            .as_array()
            .ok_or("Antigravity returned an invalid quota group.")?
        {
            if bucket["disabled"].as_bool() == Some(true) {
                continue;
            }
            let id = safe_label(&bucket["bucketId"])
                .ok_or("Antigravity returned an unnamed quota window.")?;
            if !seen.insert((name.clone(), id)) {
                return Err("Antigravity returned duplicate quota windows.".into());
            }
            let cadence = match bucket["window"].as_str() {
                Some("weekly") => "weekly allowance".into(),
                Some("5h") => "5-hour allowance".into(),
                _ => safe_label(&bucket["displayName"])
                    .ok_or("Antigravity returned an unnamed quota window.")?,
            };
            // Current desktop responses are flat; earlier versions wrap the
            // same fraction in remaining. Never default a missing fraction to 0.
            let left = fraction(
                bucket
                    .get("remainingFraction")
                    .unwrap_or(&bucket["remaining"]["remainingFraction"]),
            );
            let Some(left) = left else {
                return Err(
                    "Antigravity did not report a valid remaining fraction for every quota window."
                        .into(),
                );
            };
            meters.push(UsageMeter::used_percent(
                format!("Antigravity - {name} - {cadence}"),
                (1.0 - left) * 100.0,
                reset(&bucket["resetTime"]),
            )?);
        }
    }
    let mut result = UsageSnapshot::new(meters)?;
    result.note = Some("Included allowances of the account currently signed in to Antigravity. These are separate from Gemini app limits and Google AI credits.".into());
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn gemini_windows_credits_and_actual_plan_stay_separate() {
        let snapshot = Google
            .parse(
                json!({"source":"gemini","plan":"Google AI Ultra","windows":[
            {"kind":"five_hour","usedFraction":0.25,"resetsAt":1791200000},
            {"kind":"weekly","usedFraction":1,"resetsAt":1791500000}],"credits":530}),
                &ProviderConfig::default(),
            )
            .unwrap();
        assert_eq!(snapshot.meters.len(), 3);
        assert_eq!(snapshot.meters[0].percent_left, Some(75.0));
        assert_eq!(snapshot.meters[1].percent_left, Some(0.0));
        assert_eq!(snapshot.meters[0].resets_at, Some(1791200000));
        assert_eq!(snapshot.meters[2].remaining, Some(530.0));
        assert_eq!(snapshot.meters[2].percent_left, None);
        assert_eq!(snapshot.meters[2].resets_at, None);
        assert_eq!(snapshot.plan.as_deref(), Some("Google AI Ultra"));
    }
    #[test]
    fn missing_or_invalid_values_are_never_full_allowances() {
        for value in [
            json!({}),
            json!({"source":"gemini","windows":[]}),
            json!({"source":"gemini","windows":[{"kind":"weekly"}]}),
            json!({"source":"gemini","windows":[{"kind":"weekly","usedFraction":1.1}]}),
        ] {
            assert!(Google.parse(value, &ProviderConfig::default()).is_err());
        }
        let snapshot = Google
            .parse(
                json!({"source":"gemini","windows":[{"kind":"weekly","usedFraction":0}]}),
                &ProviderConfig::default(),
            )
            .unwrap();
        assert_eq!(snapshot.meters[0].percent_left, Some(100.0));
        assert_eq!(snapshot.meters[0].resets_at, None);
        assert!(snapshot.plan.is_none());
    }
    #[test]
    fn antigravity_reports_each_group_window_with_reset() {
        let value = json!({"response":{"groups":[{"displayName":"Example models","buckets":[
            {"bucketId":"example-5h","window":"5h","remainingFraction":0.8,"resetTime":"2026-10-04T22:00:00Z"},
            {"bucketId":"example-weekly","window":"weekly","remaining":{"remainingFraction":0.2}}]}]}});
        let snapshot = parse_antigravity(&value).unwrap();
        assert_eq!(snapshot.meters.len(), 2);
        assert!((snapshot.meters[0].percent_left.unwrap() - 80.0).abs() < 0.001);
        assert!((snapshot.meters[1].percent_left.unwrap() - 20.0).abs() < 0.001);
        assert!(snapshot.meters[0].resets_at.is_some());
        assert!(snapshot.meters[1].resets_at.is_none());
        assert!(snapshot.plan.is_none());
        assert!(parse_antigravity(&json!({"response":{"groups":[{"displayName":"Example","buckets":[{"bucketId":"missing","window":"weekly"}]}]}})).is_err());
        assert!(parse_antigravity(&json!({"response":{"groups":[{"displayName":"Example","buckets":[{"bucketId":"invalid","window":"weekly","remainingFraction":2,"remaining":{"remainingFraction":1}}]}]}})).is_err());
        assert_eq!(reset(&json!(253_402_300_800i64)), None);
    }
    #[test]
    fn metadata_exposes_only_nonsecret_connections_and_no_inference() {
        let provider = Google;
        assert!(provider.inference().is_none());
        assert_eq!(provider.definition().category, "llm");
        assert_eq!(provider.definition().fields[0].kind, "select");
        let mut config = ProviderConfig::default();
        config.fields.insert("connection".into(), "unknown".into());
        assert!(provider.parse(json!({}), &config).is_err());
    }
}
