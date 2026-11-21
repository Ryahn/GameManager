//! Non-secret user settings, stored as JSON in the app data folder.
//! (Secrets go to the OS keyring; see `auth::store`.)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::auth::AuthMode;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub onboarding_complete: bool,
    pub auth_mode: Option<AuthMode>,
    pub username: Option<String>,
    pub library_dir: Option<String>,
    pub theme: String,
    pub accent: String,
    pub ui_scale: f32,
    pub reduce_motion: bool,
    pub blur_nsfw: bool,
    /// Closing the window keeps the app in the tray so update checks continue.
    pub close_to_tray: bool,
    pub notify_updates: bool,
    /// Set once the "still running in the tray" notice has been shown.
    pub tray_notice_shown: bool,
    /// Downloads at once (1-3; capped in `downloads::limits`).
    pub max_active_downloads: u8,
    /// Delete the downloaded archive once the game is installed.
    pub delete_archives: bool,
    /// Display names for file-host accounts (secrets are in the keyring).
    pub pixeldrain_user: Option<String>,
    pub mega_email: Option<String>,
    pub gofile_user: Option<String>,
    /// Global shortcut that hides every window, e.g. "Ctrl+Shift+H".
    pub boss_key: Option<String>,
    /// Linux/macOS: program that runs Windows games (umu-run, wine, …).
    /// None = detect automatically.
    pub wine_command: Option<String>,
    /// Look for launcher updates in the background (every 6 hours).
    pub auto_update_check: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            onboarding_complete: false,
            auth_mode: None,
            username: None,
            library_dir: None,
            theme: "dark".into(),
            accent: "violet".into(),
            ui_scale: 1.0,
            reduce_motion: false,
            blur_nsfw: false,
            close_to_tray: true,
            notify_updates: true,
            tray_notice_shown: false,
            max_active_downloads: crate::downloads::limits::DEFAULT_ACTIVE as u8,
            delete_archives: true,
            pixeldrain_user: None,
            mega_email: None,
            gofile_user: None,
            boss_key: None,
            wine_command: None,
            auto_update_check: true,
        }
    }
}

/// Fields the UI may change. Auth fields are set by the auth commands only.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub onboarding_complete: Option<bool>,
    pub library_dir: Option<String>,
    pub theme: Option<String>,
    pub accent: Option<String>,
    pub ui_scale: Option<f32>,
    pub reduce_motion: Option<bool>,
    pub blur_nsfw: Option<bool>,
    pub close_to_tray: Option<bool>,
    pub notify_updates: Option<bool>,
    pub max_active_downloads: Option<u8>,
    pub delete_archives: Option<bool>,
    /// Empty string = back to automatic.
    pub wine_command: Option<String>,
    pub auto_update_check: Option<bool>,
}

impl Settings {
    pub fn apply(&mut self, p: SettingsPatch) {
        if let Some(v) = p.onboarding_complete {
            self.onboarding_complete = v;
        }
        if let Some(v) = p.library_dir {
            self.library_dir = Some(v).filter(|s| !s.trim().is_empty());
        }
        if let Some(v) = p
            .theme
            .filter(|t| matches!(t.as_str(), "dark" | "light" | "system"))
        {
            self.theme = v;
        }
        if let Some(v) = p.accent {
            self.accent = v;
        }
        if let Some(v) = p.ui_scale {
            self.ui_scale = v.clamp(0.8, 1.5);
        }
        if let Some(v) = p.reduce_motion {
            self.reduce_motion = v;
        }
        if let Some(v) = p.blur_nsfw {
            self.blur_nsfw = v;
        }
        if let Some(v) = p.close_to_tray {
            self.close_to_tray = v;
        }
        if let Some(v) = p.notify_updates {
            self.notify_updates = v;
        }
        if let Some(v) = p.max_active_downloads {
            self.max_active_downloads = v.clamp(1, 3);
        }
        if let Some(v) = p.delete_archives {
            self.delete_archives = v;
        }
        if let Some(v) = p.auto_update_check {
            self.auto_update_check = v;
        }
        if let Some(v) = p.wine_command {
            self.wine_command = Some(v.trim().to_string()).filter(|s| !s.is_empty());
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
    pub value: Mutex<Settings>,
}

impl SettingsStore {
    pub fn load(dir: PathBuf) -> Self {
        let path = dir.join("settings.json");
        let value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        Self {
            path,
            value: Mutex::new(value),
        }
    }

    pub async fn get(&self) -> Settings {
        self.value.lock().await.clone()
    }

    pub async fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut s = self.value.lock().await;
        f(&mut s);
        let json = serde_json::to_string_pretty(&*s).map_err(|e| e.to_string())?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // Write-then-rename so a crash never leaves a half-written file.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
        Ok(s.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_clamps_and_validates() {
        let mut s = Settings::default();
        s.apply(SettingsPatch {
            ui_scale: Some(3.0),
            theme: Some("neon".into()),
            library_dir: Some("  ".into()),
            ..Default::default()
        });
        assert_eq!(s.ui_scale, 1.5);
        assert_eq!(s.theme, "dark");
        assert_eq!(s.library_dir, None);
    }

    #[tokio::test]
    async fn persists_round_trip() {
        let dir = std::env::temp_dir().join(format!("f95gm-test-{}", std::process::id()));
        let store = SettingsStore::load(dir.clone());
        store
            .update(|s| s.library_dir = Some("D:/Games".into()))
            .await
            .unwrap();
        let again = SettingsStore::load(dir.clone());
        assert_eq!(again.get().await.library_dir.as_deref(), Some("D:/Games"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
