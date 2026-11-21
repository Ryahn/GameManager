//! Desktop integration: start with Windows, `f95gm://` links, boss key,
//! PIN lock and signed self-updates.

use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rand::RngCore;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::api::error::ApiError;
use crate::auth::store;
use crate::commands::AppState;

fn invalid(m: impl Into<String>) -> ApiError {
    ApiError::Invalid { message: m.into() }
}

// ------------------------------------------------------------------ links

/// `f95gm://add/<thread id>` or `f95gm://game/<thread id>`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeepLink {
    pub action: String,
    pub thread_id: i64,
}

pub fn parse_link(url: &url::Url) -> Option<DeepLink> {
    if url.scheme() != "f95gm" {
        return None;
    }
    // `f95gm://add/123` parses with host "add"; also accept `f95gm:add/123`.
    let mut parts: Vec<&str> = url.host_str().into_iter().collect();
    parts.extend(url.path().split('/').filter(|s| !s.is_empty()));
    let (action, id) = (parts.first()?, parts.get(1)?);
    let action = match *action {
        "add" => "add",
        "game" | "open" | "thread" => "open",
        _ => return None,
    };
    let thread_id: i64 = id.parse().ok().filter(|n| *n > 0 && *n < 100_000_000)?;
    Some(DeepLink {
        action: action.into(),
        thread_id,
    })
}

#[derive(Default)]
pub struct PendingLink(Mutex<Option<DeepLink>>);

/// Handles a link from the browser (extension button, site link). Adding a
/// game is harmless and undoable, so it happens without asking; the window
/// then shows the game.
pub fn handle_link(app: &AppHandle, url: &url::Url) {
    let Some(link) = parse_link(url) else {
        log::warn!("ignored link {url}");
        return;
    };
    crate::tray::show_main(app);
    *app.state::<PendingLink>().0.lock().unwrap_or_else(|p| p.into_inner()) = Some(link.clone());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut added = false;
        if link.action == "add" {
            let state = app.state::<AppState>();
            if state.api.has_credential().await
                && let Ok(b) = state.api.add_bookmark(link.thread_id).await {
                    added = true;
                    if let Some(t) = &b.thread {
                        let _ = app.state::<Arc<crate::db::Db>>().cache_threads(std::slice::from_ref(t), true);
                    }
                }
        }
        let _ = app.emit(
            "deep-link",
            serde_json::json!({ "action": link.action, "thread_id": link.thread_id, "added": added }),
        );
    });
}

/// A link that opened the app before the UI was listening.
#[tauri::command]
pub fn deep_link_take(pending: State<'_, PendingLink>) -> Option<DeepLink> {
    pending.0.lock().unwrap_or_else(|p| p.into_inner()).take()
}

// ------------------------------------------------------------------- lock

const PIN_KEY: &str = "pin_hash";

#[derive(Default)]
pub struct Lock {
    locked: AtomicBool,
    failures: Mutex<(u32, Option<Instant>)>,
}

impl Lock {
    pub fn is_locked(&self) -> bool {
        self.locked.load(Ordering::SeqCst)
    }
}

pub fn pin_enabled() -> bool {
    store::get(PIN_KEY).is_some()
}

fn hash_pin(salt: &str, pin: &str) -> String {
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(b":");
    h.update(pin.as_bytes());
    format!("{:x}", h.finalize())
}

fn check_pin(pin: &str) -> bool {
    let Some(stored) = store::get(PIN_KEY) else { return true };
    let Some((salt, hash)) = stored.split_once('$') else { return false };
    // Constant-time-ish compare; this is a privacy screen, not encryption.
    let got = hash_pin(salt, pin);
    got.len() == hash.len() && got.bytes().zip(hash.bytes()).fold(0, |a, (x, y)| a | (x ^ y)) == 0
}

fn valid_pin(pin: &str) -> bool {
    (4..=8).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit())
}

pub fn lock_now(app: &AppHandle) {
    if pin_enabled() {
        app.state::<Arc<Lock>>().locked.store(true, Ordering::SeqCst);
        let _ = app.emit("lock-changed", true);
        crate::tray::refresh(app);
    }
}

#[derive(Serialize)]
pub struct SystemInfo {
    autostart: bool,
    boss_key: Option<String>,
    pin_enabled: bool,
    locked: bool,
    updater_configured: bool,
}

#[tauri::command]
pub async fn system_info(app: AppHandle, state: State<'_, AppState>) -> Result<SystemInfo, ApiError> {
    Ok(SystemInfo {
        autostart: app.autolaunch().is_enabled().unwrap_or(false),
        boss_key: state.settings.get().await.boss_key,
        pin_enabled: pin_enabled(),
        locked: app.state::<Arc<Lock>>().is_locked(),
        updater_configured: updater_configured(),
    })
}

#[tauri::command]
pub fn pin_set(current: Option<String>, pin: String) -> Result<(), ApiError> {
    if pin_enabled() && !current.as_deref().is_some_and(check_pin) {
        return Err(invalid("Your current PIN isn't right."));
    }
    if !valid_pin(&pin) {
        return Err(invalid("Use 4 to 8 digits."));
    }
    let mut salt = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut salt);
    let salt: String = salt.iter().map(|b| format!("{b:02x}")).collect();
    store::set(PIN_KEY, &format!("{salt}${}", hash_pin(&salt, &pin))).map_err(ApiError::internal)
}

#[tauri::command]
pub fn pin_clear(app: AppHandle, current: String) -> Result<(), ApiError> {
    if !check_pin(&current) {
        return Err(invalid("That PIN isn't right."));
    }
    store::delete(PIN_KEY);
    app.state::<Arc<Lock>>().locked.store(false, Ordering::SeqCst);
    let _ = app.emit("lock-changed", false);
    Ok(())
}

/// Unlocks with the PIN. After 5 wrong tries, waits 30 s between tries.
#[tauri::command]
pub fn pin_unlock(app: AppHandle, lock: State<'_, Arc<Lock>>, pin: String) -> Result<bool, ApiError> {
    let mut f = lock.failures.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(until) = f.1
        && Instant::now() < until {
            let secs = (until - Instant::now()).as_secs().max(1);
            return Err(invalid(format!("Too many wrong tries. Wait {secs} seconds.")));
        }
    if check_pin(&pin) {
        *f = (0, None);
        lock.locked.store(false, Ordering::SeqCst);
        let _ = app.emit("lock-changed", false);
        crate::tray::refresh(&app);
        Ok(true)
    } else {
        f.0 += 1;
        if f.0 >= 5 {
            f.1 = Some(Instant::now() + Duration::from_secs(30));
        }
        Ok(false)
    }
}

#[tauri::command]
pub fn lock_app(app: AppHandle) {
    lock_now(&app);
}

// --------------------------------------------------------------- boss key

/// Hides every launcher window at once (and locks, with a PIN). Pressing
/// it again brings the main window back.
pub fn boss_key_pressed(app: &AppHandle) {
    let windows = app.webview_windows();
    let any_visible = windows.values().any(|w| w.is_visible().unwrap_or(false));
    if any_visible {
        for w in windows.values() {
            let _ = w.hide();
        }
        lock_now(app);
    } else {
        crate::tray::show_main(app);
    }
}

pub fn register_boss_key(app: &AppHandle, accel: Option<&str>) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    if let Some(a) = accel.filter(|a| !a.is_empty()) {
        let sc = Shortcut::from_str(a).map_err(|e| format!("That shortcut isn't valid: {e}"))?;
        gs.register(sc)
            .map_err(|e| format!("Couldn't use that shortcut (another app may have it): {e}"))?;
    }
    Ok(())
}

#[tauri::command]
pub async fn boss_key_set(app: AppHandle, state: State<'_, AppState>, shortcut: Option<String>) -> Result<(), ApiError> {
    let shortcut = shortcut.filter(|s| !s.trim().is_empty());
    register_boss_key(&app, shortcut.as_deref()).map_err(invalid)?;
    state
        .settings
        .update(|s| s.boss_key = shortcut)
        .await
        .map(|_| ())
        .map_err(ApiError::internal)
}

// -------------------------------------------------------------- autostart

#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool, ApiError> {
    let al = app.autolaunch();
    let r = if enabled { al.enable() } else { al.disable() };
    r.map_err(|e| invalid(format!("Windows didn't accept the change: {e}")))?;
    Ok(al.is_enabled().unwrap_or(enabled))
}

// ---------------------------------------------------------------- updates

/// Public values, set when building a release. Without both, the app
/// never checks for updates. The private signing key never ships: it only
/// lives in the release pipeline (TAURI_SIGNING_PRIVATE_KEY).
const UPDATER_PUBKEY: Option<&str> = option_env!("F95GM_UPDATER_PUBKEY");
const UPDATER_URL: Option<&str> = option_env!("F95GM_UPDATE_URL");

pub fn updater_configured() -> bool {
    UPDATER_PUBKEY.is_some_and(|k| !k.is_empty()) && UPDATER_URL.is_some_and(|u| !u.is_empty())
}

fn updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, ApiError> {
    use tauri_plugin_updater::UpdaterExt;
    let (Some(key), Some(url)) = (UPDATER_PUBKEY, UPDATER_URL) else {
        return Err(invalid("Automatic updates aren't set up in this build."));
    };
    let url = url::Url::parse(url).map_err(ApiError::internal)?;
    app.updater_builder()
        .pubkey(key)
        .endpoints(vec![url])
        .map_err(ApiError::internal)?
        .build()
        .map_err(ApiError::internal)
}

#[derive(Serialize)]
pub struct UpdateInfo {
    version: String,
    notes: Option<String>,
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> Result<Option<UpdateInfo>, ApiError> {
    let up = updater(&app)?
        .check()
        .await
        .map_err(|e| ApiError::Network { message: e.to_string() })?;
    Ok(up.map(|u| UpdateInfo {
        version: u.version.clone(),
        notes: u.body.clone(),
    }))
}

/// Downloads, verifies the signature, installs and restarts.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), ApiError> {
    let Some(update) = updater(&app)?
        .check()
        .await
        .map_err(|e| ApiError::Network { message: e.to_string() })?
    else {
        return Err(invalid("You're already on the latest version."));
    };
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| invalid(format!("The update couldn't be installed: {e}")))?;
    app.restart();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links() {
        let p = |s: &str| parse_link(&url::Url::parse(s).unwrap());
        assert_eq!(p("f95gm://add/12345"), Some(DeepLink { action: "add".into(), thread_id: 12345 }));
        assert_eq!(p("f95gm://game/7/").unwrap().action, "open");
        assert_eq!(p("f95gm:add/9").unwrap().thread_id, 9);
        assert!(p("f95gm://delete/1").is_none());
        assert!(p("f95gm://add/-5").is_none());
        assert!(p("f95gm://add/abc").is_none());
        assert!(p("https://add/1").is_none());
    }

    #[test]
    fn pins_hash_with_salt() {
        assert!(valid_pin("1234") && valid_pin("12345678"));
        assert!(!valid_pin("123") && !valid_pin("12a4") && !valid_pin("123456789"));
        assert_ne!(hash_pin("a", "1234"), hash_pin("b", "1234"));
        assert_eq!(hash_pin("a", "1234"), hash_pin("a", "1234"));
    }
}
