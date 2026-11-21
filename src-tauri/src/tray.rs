//! System tray: reopen the window, quick-launch recent games, check for
//! updates, quit. The menu is rebuilt whenever play history changes.

use std::sync::Arc;

use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::db::Db;

const TRAY_ID: &str = "main";

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let recent = app
        .try_state::<Arc<Db>>()
        .and_then(|db| db.recently_played(5).ok())
        .unwrap_or_default();

    let open = MenuItem::with_id(app, "open", "Open F95 Game Manager", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let header = MenuItem::with_id(
        app,
        "recent-header",
        if recent.is_empty() { "No games played yet" } else { "Play" },
        false,
        None::<&str>,
    )?;
    let games: Vec<MenuItem<Wry>> = recent
        .iter()
        .map(|(id, title)| {
            MenuItem::with_id(app, format!("play:{id}"), truncate(title, 40), true, None::<&str>)
        })
        .collect::<tauri::Result<_>>()?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let check = MenuItem::with_id(app, "check", "Check for updates", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let mut items: Vec<&dyn IsMenuItem<Wry>> = vec![&open, &sep1, &header];
    items.extend(games.iter().map(|g| g as &dyn IsMenuItem<Wry>));
    items.extend([&sep2 as &dyn IsMenuItem<Wry>, &check, &quit]);
    Menu::with_items(app, &items)
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("F95 Game Manager")
        .menu(&build_menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id().as_ref();
            match id {
                "open" => show_main(app),
                "check" => crate::sync::wake(app),
                "quit" => app.exit(0),
                _ => {
                    if let Some(game) = id.strip_prefix("play:").and_then(|s| s.parse().ok())
                        && let Err(e) = crate::launch::play(app, game, None) {
                            crate::notify::send(app, "Couldn't start the game", &e);
                        }
                }
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Rebuilds the menu (recently played list).
pub fn refresh(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        match build_menu(app) {
            Ok(menu) => {
                let _ = tray.set_menu(Some(menu));
            }
            Err(e) => log::warn!("tray menu rebuild failed: {e}"),
        }
    }
}

pub fn set_update_count(app: &AppHandle, n: usize) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tip = match n {
            0 => "F95 Game Manager".to_string(),
            1 => "F95 Game Manager · 1 update".to_string(),
            n => format!("F95 Game Manager · {n} updates"),
        };
        let _ = tray.set_tooltip(Some(tip));
    }
}
