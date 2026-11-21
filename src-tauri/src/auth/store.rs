//! Secrets live in the OS credential store (Windows Credential Manager,
//! macOS Keychain, Secret Service on Linux), never in settings files, the
//! database, or the webview.
//!
//! Off Windows there may be no credential store at all (Steam Deck in Game
//! Mode has no Secret Service). Then secrets go to `credentials.json` in the
//! app data folder, readable only by the user (0600), and a warning is
//! logged.

use crate::config::KEYRING_SERVICE;

pub const API_KEY: &str = "api_key";
pub const REFRESH_TOKEN: &str = "sso_refresh_token";
/// The user's own Pixeldrain API key.
pub const PIXELDRAIN_KEY: &str = "pixeldrain_api_key";
/// A MEGA session (the password itself is never stored).
pub const MEGA_SESSION: &str = "mega_session";
/// The user's own Gofile (Premium) account token.
pub const GOFILE_TOKEN: &str = "gofile_token";

use std::path::PathBuf;
use std::sync::OnceLock;

static FALLBACK_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_fallback_dir(dir: PathBuf) {
    let _ = FALLBACK_DIR.set(dir);
}

fn fallback_path() -> Option<PathBuf> {
    if cfg!(windows) {
        return None;
    }
    FALLBACK_DIR.get().map(|d| d.join("credentials.json"))
}

fn fallback_read() -> serde_json::Map<String, serde_json::Value> {
    fallback_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn fallback_write(map: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    let p = fallback_path().ok_or("no credential store available")?;
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(map).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
    }
    std::fs::rename(&tmp, &p).map_err(|e| e.to_string())
}

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| e.to_string())
}

pub fn get(name: &str) -> Option<String> {
    entry(name)
        .ok()
        .and_then(|e| e.get_password().ok())
        .filter(|s| !s.is_empty())
        .or_else(|| {
            fallback_read()
                .get(name)
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .filter(|s| !s.is_empty())
        })
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    match entry(name).and_then(|e| e.set_password(value).map_err(|e| e.to_string())) {
        Ok(()) => Ok(()),
        Err(e) if fallback_path().is_some() => {
            log::warn!("credential store unavailable ({e}); using the protected file instead");
            let mut map = fallback_read();
            map.insert(name.to_string(), serde_json::Value::String(value.to_string()));
            fallback_write(&map)
        }
        Err(e) => Err(e),
    }
}

pub fn delete(name: &str) {
    if let Ok(e) = entry(name) {
        let _ = e.delete_credential();
    }
    let mut map = fallback_read();
    if map.remove(name).is_some() {
        let _ = fallback_write(&map);
    }
}
