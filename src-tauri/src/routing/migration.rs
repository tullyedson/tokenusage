use super::{
    config::{ModelPool, PoolMember, RouterSettings},
    legacy,
};
use crate::model::Settings;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub fn from_value(mut value: Value) -> Result<Settings, String> {
    let version = value["version"]
        .as_u64()
        .ok_or("Settings version is missing.")?;
    if version == 3 {
        return serde_json::from_value(value)
            .map_err(|_| "Saved settings are damaged. The original file was preserved.".into());
    }
    if !matches!(version, 1 | 2) {
        return Err("These settings were saved by a different app version. The original file was preserved.".into());
    }
    let router: legacy::RouterSettings = if value.get("routing").is_some() {
        serde_json::from_value(value["routing"].clone())
            .map_err(|_| "Legacy routing settings are invalid.")?
    } else {
        Default::default()
    };
    let accounts = value["providers"]
        .as_object_mut()
        .ok_or("Saved accounts are invalid.")?;
    let mut mappings = BTreeMap::new();
    for (id, account) in accounts.iter_mut() {
        let old: legacy::AccountRouting = if account.get("routing").is_some() {
            serde_json::from_value(account["routing"].clone())
                .map_err(|_| "Legacy account routing is invalid.")?
        } else {
            legacy::AccountRouting {
                enabled: true,
                models: vec![],
            }
        };
        if account.get("routing").is_some() {
            legacy::validate_account(&old)?;
        }
        account["routing"] = json!({"enabled":old.enabled});
        if let Some(fields) = account["fields"].as_object_mut() {
            fields.remove("routing_billing");
        }
        mappings.insert(id.clone(), old.models);
    }
    let known = mappings.keys().cloned().collect::<BTreeSet<_>>();
    legacy::validate(&router, &known)?;
    let order = router
        .account_order
        .iter()
        .chain(known.iter().filter(|id| !router.account_order.contains(id)))
        .collect::<Vec<_>>();
    let names = mappings
        .values()
        .flatten()
        .map(|m| m.model.clone())
        .chain(router.fallbacks.iter().map(|rule| rule.model.clone()))
        .collect::<BTreeSet<_>>();
    let pools = names
        .into_iter()
        .map(|name| {
            let mut members = Vec::new();
            let mut seen = BTreeSet::new();
            for model in legacy::model_order(&name, &router.fallbacks) {
                for id in &order {
                    for mapping in &mappings[*id] {
                        if mapping.model == model {
                            let member = PoolMember {
                                account_id: (*id).clone(),
                                model: mapping.upstream.clone(),
                            };
                            if seen.insert(member.clone()) {
                                members.push(member);
                            }
                        }
                    }
                }
            }
            ModelPool { name, members }
        })
        .collect();
    value["routing"] = serde_json::to_value(RouterSettings {
        enabled: router.enabled,
        port: router.port,
        pools,
    })
    .map_err(|_| "Could not migrate model pools.")?;
    value["version"] = json!(3);
    serde_json::from_value(value)
        .map_err(|_| "Saved settings are damaged. The original file was preserved.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_fallbacks_flatten_without_changing_account_order_or_credentials() {
        let value = json!({"version":2,"refreshMinutes":5,"providers":{
            "first":{"enabled":true,"fields":{"base_url":"http://127.0.0.1:8000","routing_billing":"unconfirmed"},"sessionGeneration":4,"revision":7,"routing":{"enabled":true,"models":[{"model":"writer","upstream":"glm"}]}},
            "second":{"enabled":true,"fields":{},"routing":{"enabled":false,"models":[{"model":"writer","upstream":"deepseek"},{"model":"offline","upstream":"qwen"}]}}
        },"routing":{"enabled":true,"port":43129,"accountOrder":["second","first"],"fallbacks":[{"model":"writer","alternatives":["offline"]}]}});
        let settings = from_value(value).unwrap();
        assert_eq!(settings.version, 3);
        let pool = settings
            .routing
            .pools
            .iter()
            .find(|p| p.name == "writer")
            .unwrap();
        assert_eq!(
            pool.members
                .iter()
                .map(|m| (m.account_id.as_str(), m.model.as_str()))
                .collect::<Vec<_>>(),
            [("second", "deepseek"), ("first", "glm"), ("second", "qwen")]
        );
        assert!(!settings.providers["second"].routing.enabled);
        assert_eq!(settings.providers["first"].session_generation, 4);
        assert_eq!(settings.providers["first"].revision, 7);
        let saved = serde_json::to_value(&settings).unwrap();
        assert!(saved["routing"].get("accountOrder").is_none());
        assert!(saved["routing"].get("fallbacks").is_none());
        assert!(saved["providers"]["first"]["routing"]
            .get("models")
            .is_none());
        assert!(saved["providers"]["first"]["fields"]
            .get("routing_billing")
            .is_none());
        assert_eq!(from_value(saved).unwrap().routing, settings.routing);
    }
}
