//! Persist / load bindings to `Documents/clickassist.json`.

use std::path::PathBuf;
use std::{fs, io};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_Documents, KF_FLAG_DEFAULT, SHGetKnownFolderPath};

use crate::bindings::Binding;

/// Top-level persisted configuration.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub bindings: Vec<Binding>,
}

/// Return the path to the config file: `Documents/clickassist.json`.
pub fn config_path() -> PathBuf {
    let mut path = documents_folder();
    path.push("clickassist.json");
    path
}

fn documents_folder() -> PathBuf {
    let mut pwstr: windows_sys::core::PWSTR = std::ptr::null_mut();
    unsafe {
        let hr = SHGetKnownFolderPath(
            &FOLDERID_Documents,
            KF_FLAG_DEFAULT as u32,
            std::ptr::null_mut(), // null token = current user
            &mut pwstr,
        );
        if hr != 0 {
            return PathBuf::from(".");
        }
        let len = (0..).take_while(|&i| *pwstr.offset(i) != 0).count();
        let slice = std::slice::from_raw_parts(pwstr, len);
        let result = String::from_utf16_lossy(slice);
        CoTaskMemFree(pwstr as *mut std::ffi::c_void);
        PathBuf::from(result)
    }
}

/// Load config from disk. Returns default (empty) on any error.
pub fn load() -> Config {
    let path = config_path();
    match fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => Config::default(),
    }
}

/// Save config to disk atomically.
pub fn save(cfg: &Config) -> io::Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cfg)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, &json)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}
