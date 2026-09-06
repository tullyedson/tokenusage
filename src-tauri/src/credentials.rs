//! Native-only secret boundary. Values never become configuration or bootstrap DTOs.
use std::collections::BTreeMap;
use zeroize::Zeroizing;

pub trait ISecretStore: Send + Sync {
    fn get(&self, provider: &str, field: &str) -> Result<Option<Zeroizing<String>>, String>;
    fn set(&self, provider: &str, field: &str, value: &str) -> Result<(), String>;
    fn delete(&self, provider: &str, field: &str) -> Result<(), String>;
}

pub struct WindowsCredentialStore;

#[cfg(windows)]
mod native {
    use super::*;
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::ERROR_NOT_FOUND,
            Security::Credentials::{
                CredDeleteW, CredFree, CredReadW, CredWriteW, CREDENTIALW,
                CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
            },
        },
    };
    use zeroize::Zeroize;

    fn target(provider: &str, field: &str) -> Result<Vec<u16>, String> {
        if [provider, field].iter().any(|part| {
            part.is_empty()
                || part.len() > 80
                || !part
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        }) {
            return Err("Invalid credential identifier.".into());
        }
        Ok(format!("com.aiusagetray.desktop/{provider}/{field}")
            .encode_utf16()
            .chain(Some(0))
            .collect())
    }

    struct OwnedCredential(*mut CREDENTIALW);
    impl Drop for OwnedCredential {
        fn drop(&mut self) {
            // CredReadW owns this allocation until CredFree. Clear its plaintext first.
            unsafe {
                let credential = &*self.0;
                if credential.CredentialBlobSize > 0 && !credential.CredentialBlob.is_null() {
                    std::slice::from_raw_parts_mut(
                        credential.CredentialBlob,
                        credential.CredentialBlobSize as usize,
                    )
                    .zeroize();
                }
                CredFree(self.0.cast());
            }
        }
    }

    impl ISecretStore for WindowsCredentialStore {
        fn get(&self, provider: &str, field: &str) -> Result<Option<Zeroizing<String>>, String> {
            let target = target(provider, field)?;
            let mut pointer = std::ptr::null_mut();
            match unsafe {
                CredReadW(
                    PCWSTR(target.as_ptr()),
                    CRED_TYPE_GENERIC,
                    None,
                    &mut pointer,
                )
            } {
                Ok(()) => (),
                Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => return Ok(None),
                Err(_) => {
                    return Err("Windows Credential Manager could not read the saved key.".into())
                }
            }
            if pointer.is_null() {
                return Err("Windows returned an invalid credential.".into());
            }
            let owned = OwnedCredential(pointer);
            let credential = unsafe { &*owned.0 };
            if credential.CredentialBlobSize == 0 {
                return Ok(None);
            }
            if credential.CredentialBlobSize > 2048 || credential.CredentialBlob.is_null() {
                return Err("The saved key has an unsupported format. Save it again.".into());
            }
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                )
            };
            let value = std::str::from_utf8(bytes)
                .map_err(|_| "The saved key has an unsupported format. Save it again.")?;
            Ok(Some(Zeroizing::new(value.to_owned())))
        }
        fn set(&self, provider: &str, field: &str, value: &str) -> Result<(), String> {
            let mut target = target(provider, field)?;
            if value.is_empty() || value.len() > 2048 {
                return Err("The key has an invalid length.".into());
            }
            let mut username: Vec<u16> = "AI Usage".encode_utf16().chain(Some(0)).collect();
            let mut bytes = Zeroizing::new(value.as_bytes().to_vec());
            let credential = CREDENTIALW {
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(target.as_mut_ptr()),
                CredentialBlobSize: bytes.len() as u32,
                CredentialBlob: bytes.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                UserName: PWSTR(username.as_mut_ptr()),
                ..Default::default()
            };
            unsafe { CredWriteW(&credential, 0) }
                .map_err(|_| "Windows Credential Manager could not save the key.".into())
        }
        fn delete(&self, provider: &str, field: &str) -> Result<(), String> {
            let target = target(provider, field)?;
            match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
                Ok(()) => Ok(()),
                Err(error) if error.code() == ERROR_NOT_FOUND.to_hresult() => Ok(()),
                Err(_) => Err("Windows Credential Manager could not remove the saved key.".into()),
            }
        }
    }
}

#[cfg(not(windows))]
impl ISecretStore for WindowsCredentialStore {
    fn get(&self, _: &str, _: &str) -> Result<Option<Zeroizing<String>>, String> {
        Err("Key storage requires Windows.".into())
    }
    fn set(&self, _: &str, _: &str, _: &str) -> Result<(), String> {
        Err("Key storage requires Windows.".into())
    }
    fn delete(&self, _: &str, _: &str) -> Result<(), String> {
        Err("Key storage requires Windows.".into())
    }
}

pub type SecretChanges = BTreeMap<String, Option<Zeroizing<String>>>;

/// Roll back credential edits if either the vault write or atomic settings save fails.
pub fn update_with<T>(
    store: &dyn ISecretStore,
    provider: &str,
    changes: &SecretChanges,
    save: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let previous: SecretChanges = changes
        .keys()
        .map(|key| Ok((key.clone(), store.get(provider, key)?)))
        .collect::<Result<_, String>>()?;
    let apply = |key: &str, value: &Option<Zeroizing<String>>| match value {
        Some(value) => store.set(provider, key, value),
        None => store.delete(provider, key),
    };
    let mut applied = Vec::new();
    let result = (|| {
        for (key, value) in changes {
            apply(key, value)?;
            applied.push(key);
        }
        save()
    })();
    if result.is_err() {
        let mut restored = true;
        for key in applied.into_iter().rev() {
            restored &= apply(key, &previous[key]).is_ok();
        }
        if !restored {
            return Err("Settings were not saved and the previous key could not be restored. Save the key again or use Forget.".into());
        }
    }
    result
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::sync::Mutex;
    #[derive(Default)]
    pub struct MemoryStore(pub Mutex<BTreeMap<(String, String), String>>);
    impl ISecretStore for MemoryStore {
        fn get(&self, provider: &str, field: &str) -> Result<Option<Zeroizing<String>>, String> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(provider.into(), field.into()))
                .cloned()
                .map(Zeroizing::new))
        }
        fn set(&self, provider: &str, field: &str, value: &str) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .insert((provider.into(), field.into()), value.into());
            Ok(())
        }
        fn delete(&self, provider: &str, field: &str) -> Result<(), String> {
            self.0
                .lock()
                .unwrap()
                .remove(&(provider.into(), field.into()));
            Ok(())
        }
    }
    #[test]
    fn failed_settings_save_restores_replaced_and_new_keys() {
        let store = MemoryStore::default();
        store.set("test", "existing", "fictional-old").unwrap();
        let changes = BTreeMap::from([
            (
                "existing".into(),
                Some(Zeroizing::new("fictional-new".into())),
            ),
            ("new".into(), Some(Zeroizing::new("fictional-new".into()))),
        ]);
        assert!(update_with(&store, "test", &changes, || Err::<(), _>(
            "Disk full".into()
        ))
        .is_err());
        assert_eq!(
            store.get("test", "existing").unwrap().unwrap().as_str(),
            "fictional-old"
        );
        assert!(store.get("test", "new").unwrap().is_none());
    }
    #[test]
    fn forget_deletes_only_the_selected_providers_keys() {
        let store = MemoryStore::default();
        store.set("test", "api_key", "fictional").unwrap();
        store.set("other", "api_key", "fictional").unwrap();
        update_with(
            &store,
            "test",
            &BTreeMap::from([("api_key".into(), None)]),
            || Ok(()),
        )
        .unwrap();
        assert!(store.get("test", "api_key").unwrap().is_none());
        assert!(store.get("other", "api_key").unwrap().is_some());
    }

    #[cfg(windows)]
    #[test]
    #[ignore = "Writes and removes a unique fictional Windows credential; no provider account is used."]
    fn windows_vault_round_trip_with_isolated_temporary_key() {
        let id = format!(
            "test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let store = WindowsCredentialStore;
        assert!(store.get(&id, "api_key").unwrap().is_none());
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = WindowsCredentialStore.delete(&self.0, "api_key");
            }
        }
        let _cleanup = Cleanup(id.clone());
        store.set(&id, "api_key", "fictional-vault-test").unwrap();
        assert_eq!(
            store.get(&id, "api_key").unwrap().unwrap().as_str(),
            "fictional-vault-test"
        );
        store.set(&id, "api_key", "fictional-replacement").unwrap();
        assert_eq!(
            store.get(&id, "api_key").unwrap().unwrap().as_str(),
            "fictional-replacement"
        );
        store.delete(&id, "api_key").unwrap();
        assert!(store.get(&id, "api_key").unwrap().is_none());
    }
}
