//! Public, compile-time configuration.
//!
//! Everything in this file ships inside the installer, so it must only contain
//! values that are safe for anyone to read: base URLs, the OAuth *public*
//! client id, and tuning constants. Never put a secret here (there is no OAuth
//! client secret: the launcher is a public client using PKCE).

/// Base URL of the catalog API.
pub const API_BASE: &str = match option_env!("F95GM_API_BASE") {
    Some(v) => v,
    None => "https://f95api.xyz",
};

/// Where users create an API key (Advanced sign-in path).
pub const API_KEYS_URL: &str = "https://f95api.xyz/panel/keys";

/// Authentik OIDC issuer for the launcher's application, e.g.
/// `https://auth.zonies.xyz/application/o/f95gamemanager/`.
/// Set at build time with `F95GM_OIDC_ISSUER`.
pub const OIDC_ISSUER: &str = match option_env!("F95GM_OIDC_ISSUER") {
    Some(v) => v,
    None => "https://auth.zonies.xyz/application/o/f95gamemanager/",
};

/// Public client id of the Authentik provider. Empty until the provider is
/// created; SSO is shown as "not available yet" while it is empty.
pub const OIDC_CLIENT_ID: &str = match option_env!("F95GM_OIDC_CLIENT_ID") {
    Some(v) => v,
    None => "",
};

pub const OIDC_SCOPES: &str = "openid profile offline_access";

/// The API allows 120 requests/minute per user. We stay well below it so that
/// a burst from the UI never trips the server-side limit.
pub const API_REQUESTS_PER_MINUTE: u32 = 90;

/// Cooldown used after a 429 when the server sends no `Retry-After`.
pub const API_DEFAULT_COOLDOWN_SECS: u64 = 60;

/// Cover/screenshot fetches go straight to F95 attachment hosts from the
/// user's IP, so they get their own, slower budget.
pub const IMAGE_REQUESTS_PER_SECOND: u32 = 2;

pub const USER_AGENT: &str = concat!("F95GameManager/", env!("CARGO_PKG_VERSION"));

/// Keyring service name used for the API key and the SSO refresh token.
pub const KEYRING_SERVICE: &str = "xyz.f95api.gamemanager";
