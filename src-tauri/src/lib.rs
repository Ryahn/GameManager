mod api;
mod auth;
mod backup;
mod commands;
mod config;
mod data_commands;
mod db;
mod download_commands;
mod downloads;
mod f95;
mod f95checker;
mod host_accounts;
mod images;
mod install;
mod launch;
mod library;
mod limiter;
mod local_commands;
mod media;
mod notify;
mod platform;
mod player;
mod reader;
mod settings;
mod sync;
mod system;
mod tray;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, WindowEvent};
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_global_shortcut::ShortcutState;
use tauri_plugin_window_state::StateFlags;

use api::client::ApiClient;
use commands::AppState;
use images::ImageCache;
use settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main(app);
        }))
        // Links (f95gm://…) reach the running instance through this too.
        .plugin(tauri_plugin_deep_link::init())
        // Remember size/position, but decide visibility ourselves (start
        // hidden when launched minimized at Windows sign-in).
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::all() & !StateFlags::VISIBLE)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        system::boss_key_pressed(app);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("gmimg", |ctx, request, responder| {
            let cache = ctx.app_handle().state::<Arc<ImageCache>>().inner().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(cache.handle(&path).await);
            });
        })
        .register_asynchronous_uri_scheme_protocol("gmmedia", |ctx, request, responder| {
            let roots = ctx.app_handle().state::<Arc<reader::MediaRoots>>().inner().clone();
            let path = request.uri().path().to_string();
            let range = request.headers().get("range").and_then(|v| v.to_str().ok()).map(str::to_string);
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(reader::serve(&roots, &path, range.as_deref()));
            });
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;

            let http = reqwest::Client::builder()
                .user_agent(config::USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()?;
            let api = Arc::new(ApiClient::new(config::API_BASE));
            auth::store::set_fallback_dir(data_dir.clone());
            let settings = Arc::new(SettingsStore::load(data_dir.clone()));
            let db = Arc::new(db::Db::open(&data_dir)?);
            let db_for_downloads = db.clone();
            let settings_for_downloads = settings.clone();

            // Restore the previous session before the UI asks for auth status.
            // This only reads the keyring; no network.
            let mode = tauri::async_runtime::block_on(settings.get()).auth_mode;
            tauri::async_runtime::block_on(auth::restore(&api, &http, mode));

            app.manage(ImageCache::new(cache_dir.join("images")));
            app.manage(Arc::new(reader::MediaRoots::default()));
            app.manage(db);
            app.manage(Arc::new(launch::Launcher::default()));
            app.manage(Arc::new(sync::UpdateChecker::default()));
            app.manage(Arc::new(local_commands::ScanState::default()));
            let downloads = downloads::Downloads::load(&db_for_downloads);
            downloads.set_folders(&tauri::async_runtime::block_on(settings_for_downloads.get()));
            app.manage(Arc::new(downloads));
            app.manage(Arc::new(f95::F95::default()));
            let boss_key = tauri::async_runtime::block_on(settings.get()).boss_key;
            platform::set_custom_runner(tauri::async_runtime::block_on(settings.get()).wine_command);
            app.manage(AppState {
                api,
                http,
                settings,
            });

            // PIN lock starts locked; boss key from settings.
            let lock = Arc::new(system::Lock::default());
            app.manage(lock);
            system::lock_now(app.handle());
            if let Err(e) = system::register_boss_key(app.handle(), boss_key.as_deref()) {
                log::warn!("boss key: {e}");
            }

            // f95gm:// links: from a running app, and the one that started it.
            app.manage(system::PendingLink::default());
            // Windows: the installer registers f95gm:// (dev builds do it
            // here). Linux: AppImages can't, so always register at start.
            #[cfg(any(debug_assertions, target_os = "linux"))]
            if let Err(e) = app.deep_link().register_all() {
                log::warn!("registering f95gm:// for development failed: {e}");
            }
            let link_app = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                for url in event.urls() {
                    system::handle_link(&link_app, &url);
                }
            });
            if let Ok(Some(urls)) = app.deep_link().get_current() {
                for url in urls {
                    system::handle_link(app.handle(), &url);
                }
            }

            // Started with Windows: stay in the tray.
            if !std::env::args().any(|a| a == "--minimized") {
                tray::show_main(app.handle());
            }

            tray::init(app.handle())?;
            sync::start(app.handle().clone());
            downloads::start(app.handle().clone());
            app.manage(Arc::new(system::Updates::default()));
            system::start_update_checks(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the launcher in the tray (if enabled)
            // so update checks and playtime tracking keep running.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != "main" {
                    return;
                }
                let app = window.app_handle();
                let state = app.state::<AppState>();
                let s = tauri::async_runtime::block_on(state.settings.get());
                if !s.close_to_tray {
                    return;
                }
                api.prevent_close();
                let _ = window.hide();
                if !s.tray_notice_shown {
                    notify::send(
                        app,
                        "Still running in the tray",
                        "F95 Game Manager keeps checking for updates. Right-click the tray icon to quit.",
                    );
                    let _ = tauri::async_runtime::block_on(
                        state.settings.update(|s| s.tray_notice_shown = true),
                    );
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_update,
            commands::auth_status,
            commands::auth_use_api_key,
            commands::auth_sso_sign_in,
            commands::auth_sign_out,
            commands::catalog_latest,
            commands::catalog_search,
            commands::catalog_thread,
            commands::bookmarks_list,
            commands::bookmarks_all,
            commands::bookmarks_add,
            commands::bookmarks_remove,
            commands::bookmarks_ack,
            commands::bookmarks_ack_all,
            local_commands::library_local,
            local_commands::game_local,
            local_commands::game_sessions,
            local_commands::game_update,
            local_commands::target_add,
            local_commands::target_remove,
            local_commands::target_set_default,
            local_commands::folder_detect,
            local_commands::game_link_folder,
            local_commands::game_play,
            local_commands::games_running,
            local_commands::game_open_folder,
            reader::media_book,
            reader::media_progress,
            reader::media_progress_set,
            reader::media_continue,
            reader::media_open_external,
            player::media_playlist,
            player::media_watch_set,
            player::media_watch_last,
            player::media_continue_watching,
            local_commands::library_scan,
            local_commands::library_scan_cancel,
            local_commands::library_import,
            local_commands::sync_now,
            local_commands::library_remove,
            data_commands::f95checker_default_path,
            data_commands::f95checker_import,
            data_commands::backup_export,
            data_commands::backup_import,
            system::system_info,
            system::compat_status,
            system::deep_link_take,
            system::pin_set,
            system::pin_clear,
            system::pin_unlock,
            system::lock_app,
            system::boss_key_set,
            system::autostart_set,
            system::update_check,
            system::update_install,
            system::update_status,
            download_commands::f95_status,
            download_commands::f95_open,
            download_commands::links_get,
            download_commands::download_preflight,
            download_commands::download_start,
            download_commands::downloads_list,
            download_commands::download_pause,
            download_commands::download_resume,
            download_commands::download_remove,
            download_commands::downloads_clear_finished,
            download_commands::download_open_folder,
            download_commands::download_open_in_browser,
            host_accounts::host_accounts,
            host_accounts::pixeldrain_set_key,
            host_accounts::pixeldrain_clear,
            host_accounts::mega_sign_in,
            host_accounts::mega_sign_out,
            host_accounts::gofile_set_token,
            host_accounts::gofile_clear,
            download_commands::jdownloader_status,
            download_commands::jdownloader_send,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
