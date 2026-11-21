//! Sign-in state: either a pasted API key (Advanced) or Authentik SSO.

pub mod oidc;
pub mod store;

use std::sync::Arc;

use serde::Serialize;

use crate::api::client::{ApiClient, Credential, RefreshFn};
use crate::config;

#[derive(Debug, Clone, Copy, Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    ApiKey,
    Sso,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthStatus {
    pub signed_in: bool,
    pub mode: Option<AuthMode>,
    pub username: Option<String>,
}

pub fn sso_configured() -> bool {
    !config::OIDC_CLIENT_ID.is_empty()
}

/// Builds the refresher the API client calls when a Bearer token expires.
/// Rotated refresh tokens are written back to the keyring.
pub fn sso_refresher(http: reqwest::Client) -> RefreshFn {
    Arc::new(move || {
        let http = http.clone();
        Box::pin(async move {
            let rt = store::get(store::REFRESH_TOKEN)?;
            let t = oidc::refresh(&http, config::OIDC_ISSUER, config::OIDC_CLIENT_ID, &rt)
                .await
                .ok()?;
            if let Some(new_rt) = &t.refresh_token {
                let _ = store::set(store::REFRESH_TOKEN, new_rt);
            }
            Some(t.access_token)
        })
    })
}

/// Restores the credential saved by a previous session, if any.
pub async fn restore(api: &ApiClient, http: &reqwest::Client, mode: Option<AuthMode>) -> bool {
    match mode {
        Some(AuthMode::ApiKey) => match store::get(store::API_KEY) {
            Some(k) => {
                api.set_credential(Some(Credential::ApiKey(k))).await;
                true
            }
            None => false,
        },
        // No network here: the first API call fetches an access token, so
        // start-up stays instant (and works offline with the local library).
        Some(AuthMode::Sso) if sso_configured() && store::get(store::REFRESH_TOKEN).is_some() => {
            api.set_credential(Some(Credential::Bearer(String::new())))
                .await;
            api.set_refresher(Some(sso_refresher(http.clone()))).await;
            true
        }
        _ => false,
    }
}
