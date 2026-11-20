//! Cover/screenshot cache served to the webview through the `gmimg://` scheme.
//!
//! Covers live on F95 attachment hosts and are fetched from the user's IP, so
//! we (1) only allow F95 image hosts, (2) pace fetches with their own limiter,
//! and (3) cache every image on disk so it is downloaded once.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tauri::http::{Response, StatusCode, header};

use crate::config;
use crate::limiter::{LimiterConfig, Priority, RateLimiter};

const ALLOWED_HOST_SUFFIXES: &[&str] = &["f95zone.to"];
const MAX_IMAGE_BYTES: usize = 15 * 1024 * 1024;

pub struct ImageCache {
    dir: PathBuf,
    http: reqwest::Client,
    limiter: RateLimiter,
}

pub fn is_allowed(url: &url::Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some_and(|h| {
            ALLOWED_HOST_SUFFIXES
                .iter()
                .any(|s| h == *s || h.ends_with(&format!(".{s}")))
        })
}

fn content_type(bytes: &[u8]) -> &'static str {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        _ => "application/octet-stream",
    }
}

impl ImageCache {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let http = reqwest::Client::builder()
            .user_agent(config::USER_AGENT)
            .timeout(Duration::from_secs(60))
            .build()
            .expect("http client");
        Arc::new(Self {
            dir,
            http,
            limiter: RateLimiter::new(LimiterConfig {
                background_share: 1.0,
                ..LimiterConfig::per_second(config::IMAGE_REQUESTS_PER_SECOND)
            }),
        })
    }

    fn path_for(&self, url: &str) -> PathBuf {
        let hash = hex(&Sha256::digest(url.as_bytes()));
        self.dir.join(&hash[..2]).join(hash)
    }

    async fn load(&self, src: &str) -> Result<Vec<u8>, (StatusCode, String)> {
        let url = url::Url::parse(src).map_err(|_| (StatusCode::BAD_REQUEST, "bad url".into()))?;
        if !is_allowed(&url) {
            return Err((StatusCode::FORBIDDEN, "host not allowed".into()));
        }
        let path = self.path_for(src);
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(bytes);
        }
        self.limiter
            .acquire(Priority::Foreground)
            .await
            .map_err(|e| (StatusCode::TOO_MANY_REQUESTS, e.to_string()))?;
        let resp = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
        if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            self.limiter.report_rate_limited(None).await;
            return Err((StatusCode::TOO_MANY_REQUESTS, "rate limited".into()));
        }
        if !resp.status().is_success() {
            return Err((
                StatusCode::BAD_GATEWAY,
                format!("upstream {}", resp.status()),
            ));
        }
        self.limiter.report_success().await;
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err((StatusCode::BAD_GATEWAY, "image too large".into()));
        }
        if let Some(parent) = path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        let _ = tokio::fs::write(&path, &bytes).await;
        Ok(bytes.to_vec())
    }

    /// Handles `gmimg://localhost/<url-encoded source>`.
    pub async fn handle(&self, request_path: &str) -> Response<Vec<u8>> {
        let encoded = request_path.trim_start_matches('/');
        let src = urlencoding::decode(encoded)
            .map(|s| s.into_owned())
            .unwrap_or_default();
        match self.load(&src).await {
            Ok(bytes) => Response::builder()
                .header(header::CONTENT_TYPE, content_type(&bytes))
                .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                .body(bytes)
                .unwrap(),
            Err((status, msg)) => Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(msg.into_bytes())
                .unwrap(),
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_f95_https_hosts_are_allowed() {
        let ok = |s: &str| is_allowed(&url::Url::parse(s).unwrap());
        assert!(ok("https://attachments.f95zone.to/2024/01/1.png"));
        assert!(ok("https://f95zone.to/data/x.jpg"));
        assert!(!ok("http://attachments.f95zone.to/1.png"));
        assert!(!ok("https://evilf95zone.to/1.png"));
        assert!(!ok("https://example.com/1.png"));
        assert!(!ok("https://127.0.0.1/1.png"));
    }

    #[test]
    fn sniffs_content_type() {
        assert_eq!(content_type(&[0x89, b'P', b'N', b'G', 0]), "image/png");
        assert_eq!(content_type(&[0xFF, 0xD8, 0xFF]), "image/jpeg");
        assert_eq!(content_type(b"RIFF1234WEBPVP8"), "image/webp");
    }
}
