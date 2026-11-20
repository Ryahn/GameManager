//! OAuth 2.0 Authorization Code + PKCE for a native app (RFC 7636, RFC 8252).
//!
//! The launcher is a *public* client: it has a client id but no secret. The
//! user signs in through their normal system browser; we listen on a one-shot
//! loopback port for the redirect and exchange the code with the PKCE verifier.

use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Deserialize)]
pub struct Discovery {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    #[serde(default)]
    pub revocation_endpoint: Option<String>,
}

#[derive(Clone, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub id_token: Option<String>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Tokens(***)")
    }
}

pub fn random_urlsafe(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// Returns `(verifier, S256 challenge)`.
pub fn pkce_pair() -> (String, String) {
    let verifier = random_urlsafe(48); // 64 chars, within RFC 7636's 43..=128
    (verifier.clone(), pkce_challenge(&verifier))
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub async fn discover(http: &reqwest::Client, issuer: &str) -> Result<Discovery, String> {
    let url = format!(
        "{}/.well-known/openid-configuration",
        issuer.trim_end_matches('/')
    );
    let resp = http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Could not reach the sign-in server: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "Sign-in server answered {} for its configuration",
            resp.status()
        ));
    }
    resp.json()
        .await
        .map_err(|e| format!("Bad sign-in configuration: {e}"))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Callback {
    Code {
        code: String,
        state: String,
    },
    Error {
        error: String,
        description: Option<String>,
    },
    /// Some other request (favicon, etc.) we should 404 and keep waiting.
    Ignore,
}

/// Parses the first line of an HTTP request, e.g.
/// `GET /callback?code=abc&state=xyz HTTP/1.1`.
pub fn parse_callback(request_line: &str) -> Callback {
    let mut parts = request_line.split_whitespace();
    let (Some("GET"), Some(target)) = (parts.next(), parts.next()) else {
        return Callback::Ignore;
    };
    let Ok(url) = url::Url::parse(&format!("http://127.0.0.1{target}")) else {
        return Callback::Ignore;
    };
    if url.path() != "/callback" {
        return Callback::Ignore;
    }
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if let Some(error) = q.get("error") {
        return Callback::Error {
            error: error.clone(),
            description: q.get("error_description").cloned(),
        };
    }
    match (q.get("code"), q.get("state")) {
        (Some(c), Some(s)) => Callback::Code {
            code: c.clone(),
            state: s.clone(),
        },
        _ => Callback::Ignore,
    }
}

fn html(title: &str, body: &str) -> String {
    let page = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"font-family:system-ui;background:#0e1014;color:#e8eaf0;display:grid;place-items:center;height:100vh;margin:0\">\
         <div style=\"text-align:center\"><h2>{title}</h2><p style=\"color:#9aa3b2\">{body}</p></div>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        page.len(),
        page
    )
}

/// Runs the whole browser sign-in. `open` is called with the authorize URL.
pub async fn sign_in(
    http: &reqwest::Client,
    issuer: &str,
    client_id: &str,
    scopes: &str,
    open: impl FnOnce(&str) -> Result<(), String>,
) -> Result<Tokens, String> {
    let disc = discover(http, issuer).await?;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}/callback");
    let (verifier, challenge) = pkce_pair();
    let state = random_urlsafe(24);

    let mut auth_url = url::Url::parse(&disc.authorization_endpoint).map_err(|e| e.to_string())?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("scope", scopes)
        .append_pair("state", &state)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
    open(auth_url.as_str())?;

    let wait = async {
        loop {
            let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 8192];
            let n = sock.read(&mut buf).await.unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let line = req.lines().next().unwrap_or_default();
            match parse_callback(line) {
                Callback::Ignore => {
                    let _ = sock
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .await;
                }
                Callback::Error { error, description } => {
                    let _ = sock
                        .write_all(
                            html(
                                "Sign-in cancelled",
                                "You can close this tab and return to the launcher.",
                            )
                            .as_bytes(),
                        )
                        .await;
                    return Err(description.unwrap_or(error));
                }
                Callback::Code { code, state: got } => {
                    if got != state {
                        let _ = sock
                            .write_all(
                                html("Sign-in failed", "Security check failed. Please try again.")
                                    .as_bytes(),
                            )
                            .await;
                        return Err(
                            "Sign-in response did not match this request (state mismatch).".into(),
                        );
                    }
                    let _ = sock
                        .write_all(
                            html(
                                "You're signed in",
                                "You can close this tab and return to F95 Game Manager.",
                            )
                            .as_bytes(),
                        )
                        .await;
                    return Ok(code);
                }
            }
        }
    };
    let code = tokio::time::timeout(SIGN_IN_TIMEOUT, wait)
        .await
        .map_err(|_| "Sign-in timed out. Please try again.".to_string())??;

    let resp = http
        .post(&disc.token_endpoint)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("client_id", client_id),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    token_response(resp).await
}

async fn token_response(resp: reqwest::Response) -> Result<Tokens, String> {
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Token request failed ({status}): {}",
            body.chars().take(200).collect::<String>()
        ));
    }
    resp.json().await.map_err(|e| e.to_string())
}

pub async fn refresh(
    http: &reqwest::Client,
    issuer: &str,
    client_id: &str,
    refresh_token: &str,
) -> Result<Tokens, String> {
    let disc = discover(http, issuer).await?;
    let resp = http
        .post(&disc.token_endpoint)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", client_id),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;
    token_response(resp).await
}

pub async fn revoke(http: &reqwest::Client, issuer: &str, client_id: &str, token: &str) {
    if let Ok(disc) = discover(http, issuer).await {
        if let Some(ep) = disc.revocation_endpoint {
            let _ = http
                .post(ep)
                .form(&[
                    ("token", token),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", client_id),
                ])
                .send()
                .await;
        }
    }
}

/// Reads a display name from the ID token payload. Display only: the API
/// validates the access token itself, so no signature check is needed here.
pub fn display_name(id_token: &str) -> Option<String> {
    let payload = id_token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    ["preferred_username", "nickname", "name"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|s| s.as_str()).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_matches_rfc7636_example() {
        // Appendix B of RFC 7636.
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn pkce_verifier_length_is_valid() {
        let (v, c) = pkce_pair();
        assert!((43..=128).contains(&v.len()));
        assert_eq!(c, pkce_challenge(&v));
        assert_ne!(pkce_pair().0, v, "verifiers are random");
    }

    #[test]
    fn parses_callback_variants() {
        assert_eq!(
            parse_callback("GET /callback?code=abc&state=s1 HTTP/1.1"),
            Callback::Code {
                code: "abc".into(),
                state: "s1".into()
            }
        );
        assert_eq!(
            parse_callback("GET /callback?error=access_denied&error_description=No HTTP/1.1"),
            Callback::Error {
                error: "access_denied".into(),
                description: Some("No".into())
            }
        );
        assert_eq!(
            parse_callback("GET /favicon.ico HTTP/1.1"),
            Callback::Ignore
        );
        assert_eq!(
            parse_callback("POST /callback?code=a&state=b HTTP/1.1"),
            Callback::Ignore
        );
        assert_eq!(
            parse_callback("GET /callback?code=only HTTP/1.1"),
            Callback::Ignore
        );
    }

    #[test]
    fn reads_display_name_from_id_token() {
        let payload = URL_SAFE_NO_PAD.encode(br#"{"preferred_username":"alice"}"#);
        assert_eq!(
            display_name(&format!("h.{payload}.s")),
            Some("alice".into())
        );
        assert_eq!(display_name("garbage"), None);
    }
}
