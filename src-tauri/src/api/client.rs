//! Typed client for f95api.xyz.
//!
//! All catalog traffic goes through one `ApiClient`, which owns the credential
//! and a shared rate limiter. The credential never reaches the webview.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Method, StatusCode, header};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::sync::RwLock;

use super::error::ApiError;
use super::types::*;
use crate::config;
use crate::limiter::{LimitError, LimiterConfig, Priority, RateLimiter};

#[derive(Clone, PartialEq, Eq)]
pub enum Credential {
    ApiKey(String),
    Bearer(String),
}

impl std::fmt::Debug for Credential {
    // Never print secrets, even in debug logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Credential::ApiKey(_) => f.write_str("ApiKey(***)"),
            Credential::Bearer(_) => f.write_str("Bearer(***)"),
        }
    }
}

/// Called once when a Bearer token is rejected; returns a fresh access token.
pub type RefreshFn =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<String>> + Send>> + Send + Sync>;

pub struct ApiClient {
    http: reqwest::Client,
    base: String,
    limiter: Arc<RateLimiter>,
    cred: RwLock<Option<Credential>>,
    refresher: RwLock<Option<RefreshFn>>,
}

impl ApiClient {
    pub fn new(base: &str) -> Self {
        Self::with_limiter(
            base,
            Arc::new(RateLimiter::new(LimiterConfig {
                default_cooldown: Duration::from_secs(config::API_DEFAULT_COOLDOWN_SECS),
                ..LimiterConfig::per_minute(config::API_REQUESTS_PER_MINUTE)
            })),
        )
    }

    pub fn with_limiter(base: &str, limiter: Arc<RateLimiter>) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(config::USER_AGENT)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .expect("http client");
        Self {
            http,
            base: base.trim_end_matches('/').to_string(),
            limiter,
            cred: RwLock::new(None),
            refresher: RwLock::new(None),
        }
    }

    pub async fn set_credential(&self, cred: Option<Credential>) {
        *self.cred.write().await = cred;
    }

    pub async fn has_credential(&self) -> bool {
        self.cred.read().await.is_some()
    }

    pub async fn set_refresher(&self, f: Option<RefreshFn>) {
        *self.refresher.write().await = f;
    }

    fn apply(rb: reqwest::RequestBuilder, cred: &Credential) -> reqwest::RequestBuilder {
        match cred {
            Credential::ApiKey(k) => rb.header("X-API-Key", k),
            Credential::Bearer(t) => rb.header(header::AUTHORIZATION, format!("Bearer {t}")),
        }
    }

    async fn send_once(
        &self,
        method: &Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        cred: &Credential,
        priority: Priority,
    ) -> Result<(StatusCode, String), ApiError> {
        if let Err(LimitError::CoolingDown(d)) = self.limiter.acquire(priority).await {
            return Err(ApiError::RateLimited {
                retry_in_secs: d.as_secs().max(1),
            });
        }
        let mut rb = self
            .http
            .request(method.clone(), format!("{}{}", self.base, path))
            .query(query);
        if let Some(b) = body {
            rb = rb.json(b);
        }
        let resp = Self::apply(rb, cred).send().await?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            let d = self.limiter.report_rate_limited(retry_after).await;
            return Err(ApiError::RateLimited {
                retry_in_secs: d.as_secs().max(1),
            });
        }
        self.limiter.report_success().await;
        let text = resp.text().await?;
        Ok((status, text))
    }

    fn detail(text: &str) -> String {
        serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|v| v.get("detail").cloned())
            .map(|d| match d {
                Value::String(s) => s,
                other => other.to_string(),
            })
            .unwrap_or_else(|| text.chars().take(300).collect())
    }

    async fn request_with(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
        cred: Credential,
        priority: Priority,
    ) -> Result<String, ApiError> {
        let (mut status, mut text) = self
            .send_once(&method, path, query, body.as_ref(), &cred, priority)
            .await?;

        // One transparent refresh for expired SSO access tokens.
        if status == StatusCode::UNAUTHORIZED && matches!(cred, Credential::Bearer(_)) {
            let refresher = self.refresher.read().await.clone();
            if let Some(refresh) = refresher {
                if let Some(token) = refresh().await {
                    let fresh = Credential::Bearer(token);
                    *self.cred.write().await = Some(fresh.clone());
                    (status, text) = self
                        .send_once(&method, path, query, body.as_ref(), &fresh, priority)
                        .await?;
                }
            }
        }

        match status {
            s if s.is_success() => Ok(text),
            StatusCode::UNAUTHORIZED => Err(ApiError::Unauthorized {
                message: Self::detail(&text),
            }),
            StatusCode::FORBIDDEN => Err(ApiError::Forbidden {
                message: Self::detail(&text),
            }),
            StatusCode::NOT_FOUND => Err(ApiError::NotFound),
            StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => Err(ApiError::Invalid {
                message: Self::detail(&text),
            }),
            s => Err(ApiError::Server {
                status: s.as_u16(),
                message: Self::detail(&text),
            }),
        }
    }

    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
        priority: Priority,
    ) -> Result<T, ApiError> {
        let cred = self
            .cred
            .read()
            .await
            .clone()
            .ok_or(ApiError::NotSignedIn)?;
        let text = self
            .request_with(method, path, query, body, cred, priority)
            .await?;
        if text.trim().is_empty() {
            return serde_json::from_value(Value::Null).map_err(ApiError::internal);
        }
        serde_json::from_str(&text).map_err(|e| ApiError::Internal {
            message: format!("unexpected response from {path}: {e}"),
        })
    }

    // ---------------------------------------------------------------- auth

    /// Checks a credential without storing it. Uses the cheapest
    /// authenticated call (a one-item bookmark page).
    pub async fn validate(&self, cred: Credential) -> Result<(), ApiError> {
        self.request_with(
            Method::GET,
            "/v1/bookmarks",
            &[("limit", "1".into())],
            None,
            cred,
            Priority::Foreground,
        )
        .await
        .map(|_| ())
    }

    // ------------------------------------------------------------- catalog

    pub async fn latest(&self, q: &LatestQuery, p: Priority) -> Result<Vec<Handiwork>, ApiError> {
        self.request(Method::GET, "/v1/f95/latest", &q.to_pairs(), None, p)
            .await
    }

    pub async fn search(&self, q: &str, limit: u32) -> Result<SearchResponse, ApiError> {
        self.request(
            Method::GET,
            "/v1/search",
            &[
                ("q", q.to_string()),
                ("limit", limit.clamp(1, 50).to_string()),
            ],
            None,
            Priority::Foreground,
        )
        .await
    }

    pub async fn thread(&self, id: i64, p: Priority) -> Result<ThreadDetail, ApiError> {
        self.request(Method::GET, &format!("/v1/threads/{id}"), &[], None, p)
            .await
    }

    /// Batch lookup; splits into the API's 25-id pages.
    pub async fn threads_batch(
        &self,
        ids: &[i64],
        p: Priority,
    ) -> Result<Vec<ThreadSummary>, ApiError> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(25) {
            let joined = chunk
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let r: ThreadBatchResponse = self
                .request(
                    Method::GET,
                    "/v1/threads/batch",
                    &[("ids", joined)],
                    None,
                    p,
                )
                .await?;
            out.extend(r.items);
        }
        Ok(out)
    }

    // ----------------------------------------------------------- bookmarks

    pub async fn bookmarks(
        &self,
        updated_only: bool,
        limit: u32,
        offset: u32,
        p: Priority,
    ) -> Result<BookmarkListResponse, ApiError> {
        self.request(
            Method::GET,
            "/v1/bookmarks",
            &[
                ("updated_only", updated_only.to_string()),
                ("limit", limit.clamp(1, 200).to_string()),
                ("offset", offset.to_string()),
            ],
            None,
            p,
        )
        .await
    }

    pub async fn add_bookmark(&self, id: i64) -> Result<Bookmark, ApiError> {
        self.request(
            Method::POST,
            "/v1/bookmarks",
            &[],
            Some(json!({ "thread_id": id })),
            Priority::Foreground,
        )
        .await
    }

    /// Bulk add; splits into the API's 500-id cap.
    pub async fn add_bookmarks_bulk(&self, ids: &[i64]) -> Result<BulkBookmarkResponse, ApiError> {
        let mut out = BulkBookmarkResponse::default();
        for chunk in ids.chunks(500) {
            let r: BulkBookmarkResponse = self
                .request(
                    Method::POST,
                    "/v1/bookmarks/bulk",
                    &[],
                    Some(json!({ "thread_ids": chunk })),
                    Priority::Foreground,
                )
                .await?;
            out.created.extend(r.created);
            out.already_tracked.extend(r.already_tracked);
            out.not_found.extend(r.not_found);
        }
        Ok(out)
    }

    pub async fn remove_bookmark(&self, id: i64) -> Result<(), ApiError> {
        let _: Value = self
            .request(
                Method::DELETE,
                &format!("/v1/bookmarks/{id}"),
                &[],
                None,
                Priority::Foreground,
            )
            .await?;
        Ok(())
    }

    pub async fn ack(&self, id: i64) -> Result<Bookmark, ApiError> {
        self.request(
            Method::POST,
            &format!("/v1/bookmarks/{id}/ack"),
            &[],
            None,
            Priority::Foreground,
        )
        .await
    }

    pub async fn ack_all(&self) -> Result<Value, ApiError> {
        self.request(
            Method::POST,
            "/v1/bookmarks/ack-all",
            &[],
            None,
            Priority::Foreground,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client(server: &MockServer) -> ApiClient {
        let c = ApiClient::new(&server.uri());
        c.set_credential(Some(Credential::ApiKey("test-key".into())))
            .await;
        c
    }

    #[tokio::test]
    async fn sends_api_key_header_and_parses_latest() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/f95/latest"))
            .and(header("X-API-Key", "test-key"))
            .and(query_param("category", "games"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 1, "name": "Alpha", "version": "0.1", "unknown_field": true }
            ])))
            .expect(1)
            .mount(&server)
            .await;
        let c = client(&server).await;
        let items = c
            .latest(&LatestQuery::default(), Priority::Foreground)
            .await
            .unwrap();
        assert_eq!(items[0].name, "Alpha");
        assert_eq!(items[0].version.as_deref(), Some("0.1"));
    }

    #[tokio::test]
    async fn rate_limit_trips_circuit_and_fails_fast() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/search"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "120"))
            .expect(1) // the second call must never reach the server
            .mount(&server)
            .await;
        let c = client(&server).await;
        let e1 = c.search("x", 5).await.unwrap_err();
        assert_eq!(e1, ApiError::RateLimited { retry_in_secs: 120 });
        let e2 = c.search("x", 5).await.unwrap_err();
        assert!(matches!(e2, ApiError::RateLimited { retry_in_secs } if retry_in_secs > 100));
    }

    #[tokio::test]
    async fn maps_auth_errors_with_detail() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/bookmarks"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"detail": "Invalid API key"})),
            )
            .mount(&server)
            .await;
        let c = ApiClient::new(&server.uri());
        let e = c
            .validate(Credential::ApiKey("bad".into()))
            .await
            .unwrap_err();
        assert_eq!(
            e,
            ApiError::Unauthorized {
                message: "Invalid API key".into()
            }
        );
        assert!(
            !c.has_credential().await,
            "validate never stores the credential"
        );
    }

    #[tokio::test]
    async fn not_signed_in_without_credential() {
        let c = ApiClient::new("http://127.0.0.1:9");
        assert_eq!(c.search("x", 5).await.unwrap_err(), ApiError::NotSignedIn);
    }

    #[tokio::test]
    async fn bearer_refreshes_once_on_401() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/threads/7"))
            .and(header("authorization", "Bearer old"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/threads/7"))
            .and(header("authorization", "Bearer new"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"thread_id": 7, "title": "T"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let c = ApiClient::new(&server.uri());
        c.set_credential(Some(Credential::Bearer("old".into())))
            .await;
        c.set_refresher(Some(Arc::new(|| {
            Box::pin(async { Some("new".to_string()) })
        })))
        .await;
        let t = c.thread(7, Priority::Foreground).await.unwrap();
        assert_eq!(t.summary.title, "T");
    }

    #[tokio::test]
    async fn batch_splits_into_25s() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/threads/batch"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"items": [{"thread_id": 1}]})),
            )
            .expect(3)
            .mount(&server)
            .await;
        let c = client(&server).await;
        let ids: Vec<i64> = (1..=60).collect();
        let out = c.threads_batch(&ids, Priority::Background).await.unwrap();
        assert_eq!(out.len(), 3);
    }
}
