//! The user's own file-host accounts (Settings → File host accounts).
//!
//! Secrets go to Windows Credential Manager. The settings file only keeps a
//! display name so the UI can say "Signed in as …" without a request.

use serde::Serialize;
use tauri::State;

use crate::api::error::ApiError;
use crate::auth::store;
use crate::commands::AppState;
use crate::downloads::mega;

#[derive(Serialize)]
pub struct HostAccounts {
    pub pixeldrain: Option<String>,
    pub mega: Option<String>,
    pub gofile: Option<String>,
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::Invalid {
        message: message.into(),
    }
}

#[tauri::command]
pub async fn host_accounts(state: State<'_, AppState>) -> Result<HostAccounts, ApiError> {
    let s = state.settings.get().await;
    Ok(HostAccounts {
        pixeldrain: s.pixeldrain_user.filter(|_| store::get(store::PIXELDRAIN_KEY).is_some()),
        mega: s.mega_email.filter(|_| store::get(store::MEGA_SESSION).is_some()),
        gofile: s.gofile_user.filter(|_| store::get(store::GOFILE_TOKEN).is_some()),
    })
}

/// Checks a Gofile token with `GET /accounts/getid` and stores it. Only
/// Premium accounts can list shared folders through the API, so other tiers
/// are refused with an explanation.
#[tauri::command]
pub async fn gofile_set_token(state: State<'_, AppState>, token: String) -> Result<HostAccounts, ApiError> {
    let token = token.trim().to_string();
    if token.len() < 8 || token.chars().any(char::is_whitespace) {
        return Err(invalid("That doesn't look like a Gofile API token."));
    }
    let resp = state
        .http
        .get("https://api.gofile.io/accounts/getid")
        .bearer_auth(&token)
        .send()
        .await
        .map_err(ApiError::from)?;
    let v: serde_json::Value = resp.json().await.map_err(ApiError::from)?;
    if v.get("status").and_then(|s| s.as_str()) != Some("ok") {
        return Err(invalid("Gofile didn't accept that token. Copy it from the API page on gofile.io while signed in."));
    }
    let data = v.get("data").cloned().unwrap_or_default();
    let tier = data.get("tier").and_then(|t| t.as_str()).unwrap_or("").to_lowercase();
    if !tier.is_empty() && tier != "premium" {
        return Err(invalid(
            "This Gofile account isn't Premium. Gofile only lets apps download through its API with a Premium account; without one, Gofile links open in the F95 window.",
        ));
    }
    let name = data
        .get("email")
        .and_then(|e| e.as_str())
        .filter(|e| !e.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| "Gofile Premium".into());
    store::set(store::GOFILE_TOKEN, &token).map_err(ApiError::internal)?;
    state
        .settings
        .update(|s| s.gofile_user = Some(name))
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}

#[tauri::command]
pub async fn gofile_clear(state: State<'_, AppState>) -> Result<HostAccounts, ApiError> {
    store::delete(store::GOFILE_TOKEN);
    state
        .settings
        .update(|s| s.gofile_user = None)
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}

/// Checks a Pixeldrain API key against `GET /api/user`, then stores it.
#[tauri::command]
pub async fn pixeldrain_set_key(state: State<'_, AppState>, key: String) -> Result<HostAccounts, ApiError> {
    let key = key.trim().to_string();
    if key.len() < 8 || key.chars().any(char::is_whitespace) {
        return Err(invalid("That doesn't look like a Pixeldrain API key."));
    }
    let resp = state
        .http
        .get("https://pixeldrain.com/api/user")
        .basic_auth("", Some(&key))
        .send()
        .await
        .map_err(ApiError::from)?;
    match resp.status().as_u16() {
        200 => {}
        401 | 403 => return Err(invalid("Pixeldrain didn't accept that key.")),
        s => {
            return Err(ApiError::Server {
                status: s,
                message: "Pixeldrain couldn't check the key right now.".into(),
            });
        }
    }
    let name = resp
        .json::<serde_json::Value>()
        .await
        .ok()
        .and_then(|v| v.get("username").and_then(|u| u.as_str()).map(str::to_string))
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| "Pixeldrain account".into());
    store::set(store::PIXELDRAIN_KEY, &key).map_err(ApiError::internal)?;
    state
        .settings
        .update(|s| s.pixeldrain_user = Some(name))
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}

#[tauri::command]
pub async fn pixeldrain_clear(state: State<'_, AppState>) -> Result<HostAccounts, ApiError> {
    store::delete(store::PIXELDRAIN_KEY);
    state
        .settings
        .update(|s| s.pixeldrain_user = None)
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}

/// Signs in to MEGA. The password is used for this request only; the
/// resulting session is what's stored.
#[tauri::command]
pub async fn mega_sign_in(
    state: State<'_, AppState>,
    email: String,
    password: String,
    mfa: Option<String>,
) -> Result<HostAccounts, ApiError> {
    let email = email.trim().to_lowercase();
    if !email.contains('@') || password.is_empty() {
        return Err(invalid("Enter your MEGA email and password."));
    }
    let mfa = mfa.map(|c| c.trim().to_string()).filter(|c| !c.is_empty());
    let session = mega::sign_in(&state.http, &email, &password, mfa.as_deref())
        .await
        .map_err(|m| {
            if m == "mfa_required" {
                ApiError::Invalid {
                    message: "mfa_required".into(),
                }
            } else {
                ApiError::Unauthorized { message: m }
            }
        })?;
    drop(password);
    store::set(store::MEGA_SESSION, &session).map_err(ApiError::internal)?;
    state
        .settings
        .update(|s| s.mega_email = Some(email))
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}

#[tauri::command]
pub async fn mega_sign_out(state: State<'_, AppState>) -> Result<HostAccounts, ApiError> {
    if let Some(session) = store::get(store::MEGA_SESSION) {
        mega::sign_out(&state.http, &session).await;
    }
    store::delete(store::MEGA_SESSION);
    state
        .settings
        .update(|s| s.mega_email = None)
        .await
        .map_err(ApiError::internal)?;
    host_accounts(state).await
}
