//! Typed commands the frontend may call. The webview never sees credentials;
//! it only asks Rust to do things on the user's behalf.

use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

use crate::api::client::{ApiClient, Credential};
use crate::api::error::ApiError;
use crate::api::types::*;
use crate::auth::{self, AuthMode, AuthStatus, oidc, store};
use crate::config;
use crate::limiter::Priority;
use crate::settings::{Settings, SettingsPatch, SettingsStore};

pub struct AppState {
    pub api: Arc<ApiClient>,
    pub http: reqwest::Client,
    pub settings: Arc<SettingsStore>,
}

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    sso_configured: bool,
    api_keys_url: &'static str,
}

#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        sso_configured: auth::sso_configured(),
        api_keys_url: config::API_KEYS_URL,
    }
}

// ---------------------------------------------------------------- settings

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<Settings, ApiError> {
    Ok(state.settings.get().await)
}

#[tauri::command]
pub async fn settings_update(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<Settings, ApiError> {
    state
        .settings
        .update(|s| s.apply(patch))
        .await
        .map_err(ApiError::internal)
}

// -------------------------------------------------------------------- auth

#[tauri::command]
pub async fn auth_status(state: State<'_, AppState>) -> Result<AuthStatus, ApiError> {
    let s = state.settings.get().await;
    let signed_in = state.api.has_credential().await;
    Ok(AuthStatus {
        signed_in,
        mode: if signed_in { s.auth_mode } else { None },
        username: if signed_in { s.username } else { None },
    })
}

#[tauri::command]
pub async fn auth_use_api_key(
    state: State<'_, AppState>,
    key: String,
) -> Result<AuthStatus, ApiError> {
    let key = key.trim().to_string();
    if key.len() < 16 || key.chars().any(char::is_whitespace) {
        return Err(ApiError::Invalid {
            message: "That doesn't look like an API key. Copy the full key from the panel.".into(),
        });
    }
    let cred = Credential::ApiKey(key.clone());
    state.api.validate(cred.clone()).await?;
    store::set(store::API_KEY, &key).map_err(ApiError::internal)?;
    store::delete(store::REFRESH_TOKEN);
    state.api.set_refresher(None).await;
    state.api.set_credential(Some(cred)).await;
    state
        .settings
        .update(|s| {
            s.auth_mode = Some(AuthMode::ApiKey);
            s.username = None;
        })
        .await
        .map_err(ApiError::internal)?;
    auth_status(state).await
}

#[tauri::command]
pub async fn auth_sso_sign_in(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<AuthStatus, ApiError> {
    if !auth::sso_configured() {
        return Err(ApiError::Invalid {
            message: "Sign-in with F95 isn't available in this build yet. Use an API key instead."
                .into(),
        });
    }
    let opener = app.clone();
    let tokens = oidc::sign_in(
        &state.http,
        config::OIDC_ISSUER,
        config::OIDC_CLIENT_ID,
        config::OIDC_SCOPES,
        |url| {
            opener
                .opener()
                .open_url(url, None::<&str>)
                .map_err(|e| e.to_string())
        },
    )
    .await
    .map_err(|message| ApiError::Unauthorized { message })?;

    let cred = Credential::Bearer(tokens.access_token.clone());
    state.api.validate(cred.clone()).await?;
    if let Some(rt) = &tokens.refresh_token {
        store::set(store::REFRESH_TOKEN, rt).map_err(ApiError::internal)?;
    }
    store::delete(store::API_KEY);
    state.api.set_credential(Some(cred)).await;
    state
        .api
        .set_refresher(Some(auth::sso_refresher(state.http.clone())))
        .await;
    let username = tokens.id_token.as_deref().and_then(oidc::display_name);
    state
        .settings
        .update(|s| {
            s.auth_mode = Some(AuthMode::Sso);
            s.username = username;
        })
        .await
        .map_err(ApiError::internal)?;
    auth_status(state).await
}

#[tauri::command]
pub async fn auth_sign_out(state: State<'_, AppState>) -> Result<AuthStatus, ApiError> {
    if let Some(rt) = store::get(store::REFRESH_TOKEN) {
        oidc::revoke(
            &state.http,
            config::OIDC_ISSUER,
            config::OIDC_CLIENT_ID,
            &rt,
        )
        .await;
    }
    store::delete(store::REFRESH_TOKEN);
    store::delete(store::API_KEY);
    state.api.set_credential(None).await;
    state.api.set_refresher(None).await;
    state
        .settings
        .update(|s| {
            s.auth_mode = None;
            s.username = None;
        })
        .await
        .map_err(ApiError::internal)?;
    auth_status(state).await
}

// ----------------------------------------------------------------- catalog

#[tauri::command]
pub async fn catalog_latest(
    state: State<'_, AppState>,
    query: LatestQuery,
) -> Result<Vec<Handiwork>, ApiError> {
    state.api.latest(&query, Priority::Foreground).await
}

#[tauri::command]
pub async fn catalog_search(
    state: State<'_, AppState>,
    q: String,
    limit: Option<u32>,
) -> Result<SearchResponse, ApiError> {
    if q.trim().is_empty() {
        return Ok(SearchResponse::default());
    }
    state.api.search(q.trim(), limit.unwrap_or(20)).await
}

#[tauri::command]
pub async fn catalog_thread(state: State<'_, AppState>, id: i64) -> Result<ThreadDetail, ApiError> {
    state.api.thread(id, Priority::Foreground).await
}

// --------------------------------------------------------------- bookmarks

#[tauri::command]
pub async fn bookmarks_list(
    state: State<'_, AppState>,
    updated_only: bool,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<BookmarkListResponse, ApiError> {
    state
        .api
        .bookmarks(
            updated_only,
            limit.unwrap_or(200),
            offset.unwrap_or(0),
            Priority::Foreground,
        )
        .await
}

#[tauri::command]
pub async fn bookmarks_add(state: State<'_, AppState>, id: i64) -> Result<Bookmark, ApiError> {
    state.api.add_bookmark(id).await
}

#[tauri::command]
pub async fn bookmarks_remove(state: State<'_, AppState>, id: i64) -> Result<(), ApiError> {
    state.api.remove_bookmark(id).await
}

#[tauri::command]
pub async fn bookmarks_ack(state: State<'_, AppState>, id: i64) -> Result<Bookmark, ApiError> {
    state.api.ack(id).await
}

#[tauri::command]
pub async fn bookmarks_ack_all(state: State<'_, AppState>) -> Result<serde_json::Value, ApiError> {
    state.api.ack_all().await
}
