use serde::Serialize;

/// Errors surfaced to the UI. Serialized as `{ kind, ... }` so the frontend
/// can show a human message and the next step for each case.
#[derive(Debug, Clone, Serialize, thiserror::Error, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApiError {
    #[error("not signed in")]
    NotSignedIn,
    #[error("unauthorized: {message}")]
    Unauthorized { message: String },
    #[error("forbidden: {message}")]
    Forbidden { message: String },
    #[error("not found")]
    NotFound,
    #[error("rate limited, retry in {retry_in_secs}s")]
    RateLimited { retry_in_secs: u64 },
    #[error("network error: {message}")]
    Network { message: String },
    #[error("server error {status}: {message}")]
    Server { status: u16, message: String },
    #[error("invalid request: {message}")]
    Invalid { message: String },
    #[error("{message}")]
    Internal { message: String },
}

impl ApiError {
    pub fn internal(e: impl std::fmt::Display) -> Self {
        ApiError::Internal {
            message: e.to_string(),
        }
    }
}

impl From<reqwest::Error> for ApiError {
    fn from(e: reqwest::Error) -> Self {
        if e.is_decode() {
            ApiError::Internal {
                message: format!("unexpected response: {e}"),
            }
        } else {
            ApiError::Network {
                message: e.to_string(),
            }
        }
    }
}
