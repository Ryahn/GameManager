//! Secrets live in the OS credential store (Windows Credential Manager),
//! never in settings files, the database, or the webview.

use crate::config::KEYRING_SERVICE;

pub const API_KEY: &str = "api_key";
pub const REFRESH_TOKEN: &str = "sso_refresh_token";
/// The user's own Pixeldrain API key.
pub const PIXELDRAIN_KEY: &str = "pixeldrain_api_key";
/// A MEGA session (the password itself is never stored).
pub const MEGA_SESSION: &str = "mega_session";
/// The user's own Gofile (Premium) account token.
pub const GOFILE_TOKEN: &str = "gofile_token";

fn entry(name: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| e.to_string())
}

pub fn get(name: &str) -> Option<String> {
    entry(name)
        .ok()?
        .get_password()
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn set(name: &str, value: &str) -> Result<(), String> {
    entry(name)?.set_password(value).map_err(|e| e.to_string())
}

pub fn delete(name: &str) {
    if let Ok(e) = entry(name) {
        let _ = e.delete_credential();
    }
}
