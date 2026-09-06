use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Default, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountRouting {
    pub enabled: bool,
    pub models: Vec<ModelMapping>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelMapping {
    pub model: String,
    pub upstream: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FallbackRule {
    pub model: String,
    pub alternatives: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouterSettings {
    pub enabled: bool,
    pub port: u16,
    pub account_order: Vec<String>,
    pub fallbacks: Vec<FallbackRule>,
}

impl Default for RouterSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 43129,
            account_order: vec![],
            fallbacks: vec![],
        }
    }
}

pub fn valid_model(model: &str) -> bool {
    !model.is_empty()
        && model.len() <= 200
        && model
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
}

pub fn valid_account(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 80
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_-".contains(&b))
}

pub fn validate_account(account: &AccountRouting) -> Result<(), String> {
    if account.models.len() > 128 {
        return Err("Use at most 128 model mappings per account.".into());
    }
    let mut seen = BTreeSet::new();
    for mapping in &account.models {
        if !valid_model(&mapping.model)
            || !valid_model(&mapping.upstream)
            || !seen.insert(&mapping.model)
        {
            return Err(
                "Model mappings need valid, unique client model names and an upstream model ID."
                    .into(),
            );
        }
    }
    if account.enabled && account.models.is_empty() {
        return Err("Add at least one model before enabling account routing.".into());
    }
    Ok(())
}

pub fn validate(settings: &RouterSettings, accounts: &BTreeSet<String>) -> Result<(), String> {
    if settings.port < 1024 {
        return Err("Choose a router port between 1024 and 65535.".into());
    }
    let mut seen = BTreeSet::new();
    for id in &settings.account_order {
        if !accounts.contains(id) || !seen.insert(id) {
            return Err("Account order contains an unknown or duplicate account.".into());
        }
    }
    if settings.fallbacks.len() > 128 {
        return Err("Use at most 128 fallback rules.".into());
    }
    let mut rules = BTreeMap::new();
    for rule in &settings.fallbacks {
        if !valid_model(&rule.model)
            || rules.insert(&rule.model, &rule.alternatives).is_some()
            || rule.alternatives.len() > 16
        {
            return Err(
                "Fallback rules need unique model names and at most 16 alternatives.".into(),
            );
        }
        let mut names = BTreeSet::new();
        for model in &rule.alternatives {
            if !valid_model(model) || !names.insert(model) {
                return Err("A fallback contains an invalid or duplicate model.".into());
            }
        }
    }
    fn visit<'a>(
        node: &'a str,
        rules: &BTreeMap<&'a String, &'a Vec<String>>,
        stack: &mut BTreeSet<&'a str>,
        done: &mut BTreeSet<&'a str>,
    ) -> Result<(), String> {
        if done.contains(node) {
            return Ok(());
        }
        if !stack.insert(node) {
            return Err("Model fallbacks contain a cycle. Remove the circular mapping.".into());
        }
        if let Some(next) = rules
            .iter()
            .find_map(|(key, value)| (key.as_str() == node).then_some(*value))
        {
            for child in next {
                visit(child, rules, stack, done)?;
            }
        }
        stack.remove(node);
        done.insert(node);
        Ok(())
    }
    let mut done = BTreeSet::new();
    for node in rules.keys() {
        visit(node, &rules, &mut BTreeSet::new(), &mut done)?;
    }
    Ok(())
}

// Breadth first: X, then X's explicit alternatives in order, then their alternatives.
pub fn model_order(requested: &str, rules: &[FallbackRule]) -> Vec<String> {
    let mut result = vec![requested.to_string()];
    let mut seen = BTreeSet::from([requested.to_string()]);
    let mut cursor = 0;
    while cursor < result.len() {
        if let Some(rule) = rules.iter().find(|r| r.model == result[cursor]) {
            for name in &rule.alternatives {
                if seen.insert(name.clone()) {
                    result.push(name.clone());
                }
            }
        }
        cursor += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mappings_are_ordered_deduplicated_and_cycles_are_rejected() {
        let mut config = RouterSettings {
            fallbacks: vec![
                FallbackRule {
                    model: "x".into(),
                    alternatives: vec!["y".into(), "z".into()],
                },
                FallbackRule {
                    model: "y".into(),
                    alternatives: vec!["z".into()],
                },
            ],
            ..Default::default()
        };
        assert!(validate(&config, &BTreeSet::new()).is_ok());
        assert_eq!(model_order("x", &config.fallbacks), vec!["x", "y", "z"]);
        config.fallbacks[1].alternatives.push("x".into());
        assert!(validate(&config, &BTreeSet::new()).is_err());
    }
}
