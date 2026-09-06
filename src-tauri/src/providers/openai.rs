use super::*;
use crate::{
    codex,
    model::{number, timestamp, FieldOption, SettingField, UsageMeter},
};

pub struct OpenAi;
#[async_trait]
impl IUsageProvider for OpenAi {
    fn definition(&self) -> ProviderDefinition {
        let mut connection = SettingField::text("connection", "Connection", "Codex reads your existing signed-in Codex installation. Website sign-in keeps a separate ChatGPT session in this app.");
        connection.kind = "select";
        connection.options = vec![
            FieldOption {
                value: "browser",
                label: "Sign in to ChatGPT",
            },
            FieldOption {
                value: "codex",
                label: "Use signed-in Codex",
            },
        ];
        ProviderDefinition { id: "openai", name: "OpenAI", category: "llm", initials: "OA", color: "#87e4b0", description: "Codex allowances included with your ChatGPT subscription. Other ChatGPT model caps are not exposed by this usage source.", help_url: "https://chatgpt.com/codex/settings/usage", fields: vec![connection, SettingField::text("executable", "Codex executable", "Only for the Codex connection. Leave blank to find the installed Codex app or CLI automatically."), SettingField::text("account_id", "ChatGPT account ID", "Optional, for selecting a specific workspace with the website connection.")] }
    }
    fn browser_spec(&self) -> BrowserSpec {
        BrowserSpec {
            url: "https://chatgpt.com/codex/settings/usage",
            hosts: &["chatgpt.com"],
            script: include_str!("scripts/openai.js"),
        }
    }
    async fn connect(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<ConnectionOutcome, String> {
        if config.field("connection") == "codex" {
            Ok(ConnectionOutcome::Ready)
        } else {
            context
                .browser
                .sign_in("openai", self.browser_spec(), config)
                .await?;
            Ok(ConnectionOutcome::BrowserOpened)
        }
    }
    async fn fetch(
        &self,
        context: &FetchContext,
        config: &ProviderConfig,
    ) -> Result<UsageSnapshot, String> {
        let value = if config.field("connection") == "codex" {
            codex::read_limits(config.field("executable"), &context.cancelled).await?
        } else {
            context
                .browser
                .read("openai", self.browser_spec(), config, &context.cancelled)
                .await?
        };
        self.parse(value, config)
    }
    fn parse(&self, v: Value, _config: &ProviderConfig) -> Result<UsageSnapshot, String> {
        let mut meters = vec![];
        let mut plan = None;
        if let Some(buckets) = v["rateLimitsByLimitId"]
            .as_object()
            .filter(|m| !m.is_empty())
        {
            for (id, bucket) in buckets {
                append_codex(&mut meters, &mut plan, id, bucket)?;
            }
        } else if v["rateLimits"].is_object() {
            append_codex(&mut meters, &mut plan, "Codex", &v["rateLimits"])?;
        } else {
            append_web(&mut meters, "Codex", &v["rate_limit"])?;
            if let Some(buckets) = v["additional_rate_limits"].as_array() {
                for bucket in buckets {
                    let name = bucket["limit_name"]
                        .as_str()
                        .or_else(|| bucket["metered_feature"].as_str())
                        .unwrap_or("Additional allowance");
                    append_web(&mut meters, name, &bucket["rate_limit"])?;
                }
            }
            plan = v["plan_type"].as_str().map(str::to_string);
            append_credits(&mut meters, &v["credits"])?;
        }
        let mut snapshot = UsageSnapshot::new(meters)?;
        snapshot.plan = plan;
        snapshot.note = Some("Codex subscription usage".into());
        Ok(snapshot)
    }
}

fn window_name(minutes: Option<f64>, fallback: &str) -> String {
    match minutes {
        Some(m) if m >= 10080.0 && (m % 10080.0).abs() < 0.01 => {
            format!("{}-week allowance", m / 10080.0)
        }
        Some(m) if m >= 1440.0 && (m % 1440.0).abs() < 0.01 => {
            format!("{}-day allowance", m / 1440.0)
        }
        Some(m) if m >= 60.0 && (m % 60.0).abs() < 0.01 => format!("{}-hour allowance", m / 60.0),
        Some(m) => format!("{m}-minute allowance"),
        None => fallback.into(),
    }
}

fn append_codex(
    meters: &mut Vec<UsageMeter>,
    plan: &mut Option<String>,
    id: &str,
    bucket: &Value,
) -> Result<(), String> {
    let name = bucket["limitName"].as_str().unwrap_or(id);
    for (key, fallback) in [
        ("primary", "Primary allowance"),
        ("secondary", "Secondary allowance"),
    ] {
        let window = &bucket[key];
        if let Some(used) = number(&window["usedPercent"]) {
            meters.push(UsageMeter::used_percent(
                format!(
                    "{} · {}",
                    name,
                    window_name(number(&window["windowDurationMins"]), fallback)
                ),
                used,
                timestamp(&window["resetsAt"]),
            )?);
        }
    }
    *plan = plan
        .take()
        .or_else(|| bucket["planType"].as_str().map(str::to_string));
    append_credits(meters, &bucket["credits"])
}

fn append_web(meters: &mut Vec<UsageMeter>, name: &str, bucket: &Value) -> Result<(), String> {
    for (key, fallback) in [
        ("primary_window", "Primary allowance"),
        ("secondary_window", "Secondary allowance"),
    ] {
        let window = &bucket[key];
        if let Some(used) = number(&window["used_percent"]) {
            meters.push(UsageMeter::used_percent(
                format!(
                    "{} · {}",
                    name,
                    window_name(
                        number(&window["limit_window_seconds"]).map(|s| s / 60.0),
                        fallback
                    )
                ),
                used,
                timestamp(&window["reset_at"]),
            )?);
        }
    }
    Ok(())
}

fn append_credits(meters: &mut Vec<UsageMeter>, value: &Value) -> Result<(), String> {
    if let Some(balance) = number(&value["balance"]) {
        meters.push(UsageMeter::balance(
            "Additional credits",
            balance,
            None,
            "credits",
            None,
        )?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "Requires an installed, signed-in Codex account and network access; reads usage only."]
    async fn signed_in_codex_returns_usage() {
        let value = codex::read_limits("", &AtomicBool::new(false))
            .await
            .expect("Codex usage read failed");
        let snapshot = OpenAi
            .parse(value, &ProviderConfig::default())
            .expect("Codex returned unsupported usage");
        assert!(!snapshot.meters.is_empty());
        assert!(snapshot
            .meters
            .iter()
            .all(|m| m.percent_left.is_none_or(|p| (0.0..=100.0).contains(&p))));
    }
    #[test]
    fn all_buckets_take_precedence_over_legacy() {
        let s = OpenAi.parse(serde_json::json!({"rateLimits":{"primary":{"usedPercent":99}},"rateLimitsByLimitId":{"coding":{"primary":{"usedPercent":20,"windowDurationMins":300},"secondary":{"usedPercent":60,"windowDurationMins":10080}},"review":{"primary":{"usedPercent":5}}}}), &ProviderConfig::default()).unwrap();
        assert_eq!(s.meters.len(), 3);
        assert_eq!(s.meters[0].percent_left, Some(80.0));
    }
    #[test]
    fn website_contract_is_supported() {
        let s = OpenAi.parse(serde_json::json!({"rate_limit":{"primary_window":{"used_percent":25,"limit_window_seconds":18000,"reset_at":1800000000}},"plan_type":"pro"}), &ProviderConfig::default()).unwrap();
        assert_eq!(s.meters[0].percent_left, Some(75.0));
        assert_eq!(s.plan.as_deref(), Some("pro"));
    }
}
