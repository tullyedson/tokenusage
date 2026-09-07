use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AccountRouting {
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}
fn enabled_by_default() -> bool {
    true
}
impl Default for AccountRouting {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PoolMember {
    pub account_id: String,
    pub model: String,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RouteMode {
    #[default]
    Failover,
    LoadDistribution,
}
impl RouteMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Failover => "failover",
            Self::LoadDistribution => "loadDistribution",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelPool {
    pub name: String,
    #[serde(default)]
    pub mode: RouteMode,
    pub members: Vec<PoolMember>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouterSettings {
    pub enabled: bool,
    pub port: u16,
    #[serde(default)]
    pub pools: Vec<ModelPool>,
}
impl Default for RouterSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            port: 43129,
            pools: vec![],
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

pub fn validate(settings: &RouterSettings, accounts: &BTreeSet<String>) -> Result<(), String> {
    if settings.port < 1024 {
        return Err("Choose a router port between 1024 and 65535.".into());
    }
    validate_pools(&settings.pools, accounts)
}
pub fn validate_pools(pools: &[ModelPool], accounts: &BTreeSet<String>) -> Result<(), String> {
    if pools.len() > 8192 {
        return Err("Use at most 8192 custom model pools.".into());
    }
    let mut names = BTreeSet::new();
    for pool in pools {
        if !valid_model(&pool.name) || !names.insert(&pool.name) {
            return Err("Pool names must be unique and use letters, numbers, dots, underscores, colons, slashes or hyphens, without spaces.".into());
        }
        if pool.members.len() > 8192 {
            return Err("Use at most 8192 entries in a model pool.".into());
        }
        let mut members = BTreeSet::new();
        for member in &pool.members {
            if !accounts.contains(&member.account_id)
                || !valid_model(&member.model)
                || !members.insert(member)
            {
                return Err("Each pool entry needs a known account and model ID, without duplicate account/model pairs.".into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn existing_pools_default_to_failover_and_modes_round_trip() {
        let old = serde_json::json!({"name":"sample", "members":[]});
        let mut pool: ModelPool = serde_json::from_value(old).unwrap();
        assert_eq!(pool.mode, RouteMode::Failover);
        pool.mode = RouteMode::LoadDistribution;
        let value = serde_json::to_value(&pool).unwrap();
        assert_eq!(value["mode"], "loadDistribution");
        assert_eq!(serde_json::from_value::<ModelPool>(value).unwrap(), pool);
        assert!(serde_json::from_value::<ModelPool>(
            serde_json::json!({"name":"sample", "members":[], "mode":"unknown"})
        )
        .is_err());
    }
    #[test]
    fn pools_use_ordered_account_model_pairs_and_unique_space_free_names() {
        let accounts = BTreeSet::from(["first".into(), "second".into()]);
        let mut pools = vec![ModelPool {
            mode: Default::default(),
            name: "flash-models".into(),
            members: vec![
                PoolMember {
                    account_id: "first".into(),
                    model: "glm-flash".into(),
                },
                PoolMember {
                    account_id: "second".into(),
                    model: "deepseek-flash".into(),
                },
            ],
        }];
        assert!(validate_pools(&pools, &accounts).is_ok());
        assert_eq!(pools[0].members[1].model, "deepseek-flash");
        pools[0].name = "flash models".into();
        assert!(validate_pools(&pools, &accounts).is_err());
        pools[0].name = "flash-models".into();
        let duplicate = pools[0].members[0].clone();
        pools[0].members.push(duplicate);
        assert!(validate_pools(&pools, &accounts).is_err());
        pools[0].members.pop();
        pools[0].members[1].account_id = "missing".into();
        assert!(validate_pools(&pools, &accounts).is_err());
        assert!(AccountRouting::default().enabled);
    }
}
