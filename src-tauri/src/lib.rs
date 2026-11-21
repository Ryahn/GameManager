mod api;
mod auth;
mod commands;
mod config;
mod db;
mod download_commands;
mod downloads;
mod f95;
mod host_accounts;
mod images;
mod install;
mod launch;
mod library;
mod limiter;
mod local_commands;
mod notify;
mod settings;
mod sync;
mod tray;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Manager, WindowEvent};

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
        .plugin(tauri_plugin_window_state::Builder::default().build())
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
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let cache_dir = app.path().app_cache_dir()?;

            let http = reqwest::Client::builder()
                .user_agent(config::USER_AGENT)
                .timeout(Duration::from_secs(30))
                .build()?;
            let api = Arc::new(ApiClient::new(config::API_BASE));
            let settings = Arc::new(SettingsStore::load(data_dir.clone()));
            let db = Arc::new(db::Db::open(&data_dir)?);
            let db_for_downloads = db.clone();
            let settings_for_downloads = settings.clone();

            // Restore the previous session before the UI asks for auth status.
            // This only reads the keyring; no network.
            let mode = tauri::async_runtime::block_on(settings.get()).auth_mode;
            tauri::async_runtime::block_on(auth::restore(&api, &http, mode));

            app.manage(ImageCache::new(cache_dir.join("images")));
            app.manage(db);
            app.manage(Arc::new(launch::Launcher::default()));
            app.manage(Arc::new(sync::UpdateChecker::default()));
            app.manage(Arc::new(local_commands::ScanState::default()));
            let downloads = downloads::Downloads::load(&db_for_downloads);
            downloads.set_library(tauri::async_runtime::block_on(settings_for_downloads.get()).library_dir);
            app.manage(Arc::new(downloads));
            app.manage(Arc::new(f95::F95::default()));
            app.manage(AppState {
                api,
                http,
                settings,
            });

            tray::init(app.handle())?;
            sync::start(app.handle().clone());
            downloads::start(app.handle().clone());
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
            local_commands::library_scan,
            local_commands::library_scan_cancel,
            local_commands::library_import,
            local_commands::sync_now,
            local_commands::library_remove,
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
