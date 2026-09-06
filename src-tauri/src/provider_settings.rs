use crate::{credentials::SecretChanges, model::ProviderDefinition};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub fn validate(
    definition: &ProviderDefinition,
    fields: &BTreeMap<String, String>,
    secrets: BTreeMap<String, String>,
) -> Result<SecretChanges, String> {
    let secrets: BTreeMap<_, _> = secrets
        .into_iter()
        .map(|(key, value)| (key, Zeroizing::new(value)))
        .collect();
    if fields.len() + secrets.len() > 20 {
        return Err("Too many settings fields.".into());
    }
    for (key, value) in fields {
        let field = definition
            .fields
            .iter()
            .find(|f| f.key == key)
            .ok_or("Unknown provider setting.")?;
        if field.kind == "secret" {
            return Err("Keys must use the secure credential field.".into());
        }
        valid_text(value)?;
        if field.kind == "number"
            && !value.is_empty()
            && !value
                .parse::<f64>()
                .ok()
                .is_some_and(|v| v.is_finite() && v > 0.0)
        {
            return Err(format!("{} must be a positive number.", field.label));
        }
        if field.kind == "select"
            && !value.is_empty()
            && !field.options.iter().any(|o| o.value == value)
        {
            return Err("Choose a valid connection.".into());
        }
    }
    let mut changes = SecretChanges::new();
    for (key, value) in secrets {
        if !definition
            .fields
            .iter()
            .any(|f| f.key == key && f.kind == "secret")
        {
            return Err("Unknown credential field.".into());
        }
        valid_text(&value)?;
        if !value.is_empty() {
            if value.bytes().any(|c| !c.is_ascii_graphic()) {
                return Err("The key contains whitespace or unsupported characters.".into());
            }
            changes.insert(key, Some(value));
        }
    }
    Ok(changes)
}

fn valid_text(value: &str) -> Result<(), String> {
    if value.len() > 2048 || value.chars().any(|c| c.is_control()) {
        return Err("A setting contains invalid text.".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SettingField;
    fn definition() -> ProviderDefinition {
        ProviderDefinition {
            id: "test",
            name: "Test",
            category: "llm",
            initials: "T",
            color: "#fff",
            description: "",
            help_url: "",
            fields: vec![
                SettingField::secret("api_key", "API key", ""),
                SettingField::text("workspace", "Workspace", ""),
            ],
        }
    }
    #[test]
    fn secret_payload_cannot_enter_serialized_settings() {
        let key = BTreeMap::from([("api_key".into(), "fictional-only".into())]);
        assert!(validate(&definition(), &key, BTreeMap::new()).is_err());
        let fields = BTreeMap::from([("workspace".into(), "example".into())]);
        let changes = validate(&definition(), &fields, key).unwrap();
        assert_eq!(changes.len(), 1);
        assert!(!serde_json::to_string(&fields)
            .unwrap()
            .contains("fictional"));
        assert!(!serde_json::to_string(&fields).unwrap().contains("api_key"));
    }
    #[test]
    fn blank_key_preserves_saved_value_and_unknown_or_invalid_keys_fail() {
        assert!(validate(
            &definition(),
            &BTreeMap::new(),
            BTreeMap::from([("api_key".into(), "".into())])
        )
        .unwrap()
        .is_empty());
        for (field, value) in [
            ("workspace", "secret"),
            ("unknown", "secret"),
            ("api_key", "bad\nkey"),
        ] {
            assert!(validate(
                &definition(),
                &BTreeMap::new(),
                BTreeMap::from([(field.into(), value.into())])
            )
            .is_err());
        }
    }
}
