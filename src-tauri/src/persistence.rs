use crate::model::Settings;
use std::{fs, io::Write, path::Path};

pub fn load(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = fs::read(path).map_err(|_| "Could not read saved settings.")?;
    let value = serde_json::from_slice(&data)
        .map_err(|_| "Saved settings are damaged. The original file was preserved.")?;
    let settings = crate::routing::migration::from_value(value)?;
    if settings.providers.len() > 64
        || settings.providers.iter().any(|(id, config)| {
            !crate::routing::config::valid_account(id)
                || (!config.provider_type.is_empty()
                    && !crate::routing::config::valid_account(&config.provider_type))
        })
    {
        return Err("Saved accounts are invalid. The original file was preserved.".into());
    }
    crate::routing::config::validate(
        &settings.routing,
        &settings.providers.keys().cloned().collect(),
    )?;
    if !(1..=60).contains(&settings.refresh_minutes) {
        return Err("Saved refresh interval is invalid. The original file was preserved.".into());
    }
    Ok(settings)
}

pub fn save(path: &Path, settings: &Settings) -> Result<(), String> {
    let data = serde_json::to_vec_pretty(settings).map_err(|_| "Could not prepare settings.")?;
    let temp = path.with_extension("json.pending");
    let mut file = fs::File::create(&temp).map_err(|_| "Could not write settings.")?;
    file.write_all(&data)
        .and_then(|_| file.sync_all())
        .map_err(|_| "Could not finish writing settings.")?;
    drop(file);
    replace(&temp, path)
        .map_err(|_| "Could not commit settings. Previous settings were preserved.".to_string())
}

#[cfg(windows)]
fn replace(from: &Path, to: &Path) -> Result<(), ()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        },
    };
    let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // Both buffers are NUL-terminated and live until the Windows call returns.
    unsafe {
        MoveFileExW(
            PCWSTR(a.as_ptr()),
            PCWSTR(b.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|_| ())
}

#[cfg(not(windows))]
fn replace(from: &Path, to: &Path) -> Result<(), ()> {
    fs::rename(from, to).map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_one_migrates_without_renaming_accounts_or_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = br#"{"version":1,"refreshMinutes":5,"providers":{"openrouter":{"enabled":true,"fields":{"connection":"key"},"sessionGeneration":7,"revision":2}}}"#;
        fs::write(&path, original).unwrap();
        let settings = load(&path).unwrap();
        assert_eq!(settings.version, 3);
        assert!(!settings.routing.enabled);
        let account = &settings.providers["openrouter"];
        assert_eq!(account.provider_type("openrouter"), "openrouter");
        assert_eq!(account.session_generation, 7);
        assert_eq!(account.revision, 2);
        assert!(account.routing.enabled);
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn settings_survive_replacement_and_bad_data_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save(&path, &Settings::default()).unwrap();
        let next = Settings {
            refresh_minutes: 12,
            ..Settings::default()
        };
        save(&path, &next).unwrap();
        assert_eq!(load(&path).unwrap().refresh_minutes, 12);
        fs::write(&path, b"broken").unwrap();
        assert!(load(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"broken");
    }
}
