mod api;
mod auth;
mod commands;
mod config;
mod images;
mod limiter;
mod settings;

use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use api::client::ApiClient;
use commands::AppState;
use images::ImageCache;
use settings::SettingsStore;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch focuses the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
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
            let settings = Arc::new(SettingsStore::load(data_dir));

            // Restore the previous session before the UI asks for auth status.
            let mode = tauri::async_runtime::block_on(settings.get()).auth_mode;
            tauri::async_runtime::block_on(auth::restore(&api, &http, mode));

            app.manage(ImageCache::new(cache_dir.join("images")));
            app.manage(AppState {
                api,
                http,
                settings,
            });
            Ok(())
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
            commands::bookmarks_add,
            commands::bookmarks_remove,
            commands::bookmarks_ack,
            commands::bookmarks_ack_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
