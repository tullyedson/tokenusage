use crate::model::Settings;
use std::{fs, io::Write, path::Path};

pub fn load(path: &Path) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let data = fs::read(path).map_err(|_| "Could not read saved settings.")?;
    let settings: Settings = serde_json::from_slice(&data)
        .map_err(|_| "Saved settings are damaged. The original file was preserved.")?;
    if settings.version != 1 {
        return Err("These settings were saved by a different app version. The original file was preserved.".into());
    }
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
