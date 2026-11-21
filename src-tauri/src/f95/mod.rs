//! The user's own F95 session, in built-in browser windows.
//!
//! - The launcher never sees the F95 password: the user signs in on
//!   f95zone.to in a normal web page, and the cookies stay in the WebView.
//! - F95 pages never get access to the app: these windows are not in any
//!   capability, so they can't call Rust commands.
//! - Threads are read one at a time, at least ~3-7 s apart, only when the
//!   user asks for links. The API does all update checking.
//! - Captchas, Cloudflare checks and sign-in are always done by the user in
//!   the visible window; nothing here clicks through them.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

use crate::api::error::ApiError;
use crate::db::Db;
use crate::downloads::{Downloads, hosts};
use crate::limiter::{LimitError, LimiterConfig, Priority, RateLimiter};

const PARSER: &str = include_str!("op_parser.js");
const WORKER: &str = "f95-worker";
pub const BROWSER: &str = "f95";
pub const LOGIN_URL: &str = "https://f95zone.to/login/";
/// Links picked in the F95 window count for this game for this long.
const CONTEXT_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameCtx {
    pub thread_id: i64,
    pub title: String,
    pub creator: String,
    pub version: String,
    /// "games" (default), "comics" or "animations".
    #[serde(default)]
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Link {
    pub host: String,
    pub url: String,
    pub label: String,
    pub masked: bool,
    /// Decided by `downloads::hosts::supported` with the user's accounts:
    /// true = the app downloads it, false = it opens in the F95 window.
    #[serde(default)]
    pub in_app: bool,
}

/// Marks which links the app can download itself (recomputed every time,
/// so adding an account in Settings applies to cached links too).
async fn mark_in_app(mut groups: Vec<LinkGroup>) -> Vec<LinkGroup> {
    let creds = tauri::async_runtime::spawn_blocking(hosts::HostCreds::load)
        .await
        .unwrap_or_default();
    for l in groups.iter_mut().flat_map(|g| g.links.iter_mut()) {
        l.in_app = !l.masked
            && Url::parse(&l.url)
                .ok()
                .and_then(|u| hosts::supported(&u, &creds))
                .is_some();
    }
    groups
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkGroup {
    pub label: String,
    #[serde(default)]
    pub os: Vec<String>,
    pub links: Vec<Link>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadLinks {
    pub thread_id: i64,
    pub version: String,
    pub fetched_at: i64,
    pub from_cache: bool,
    pub groups: Vec<LinkGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    pub checked_at: i64,
    pub logged_in: bool,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PageResult {
    Ok {
        logged_in: bool,
        username: Option<String>,
        groups: Vec<LinkGroup>,
    },
    Challenge,
    RateLimited,
    LoginRequired,
    NotFound {
        #[serde(default)]
        logged_in: bool,
        #[serde(default)]
        username: Option<String>,
    },
}

pub struct F95 {
    pacer: RateLimiter,
    fetch_lock: tokio::sync::Mutex<()>,
    ctx: Mutex<Option<(GameCtx, Instant)>>,
    status: Mutex<Option<SessionStatus>>,
}

impl Default for F95 {
    fn default() -> Self {
        Self {
            // ~5 s ± 2 s between page reads (3 s floor), at most 8 a minute;
            // a "slow down" page stops reads for 60 s, doubling to 15 min.
            pacer: RateLimiter::new(LimiterConfig {
                capacity: 8,
                min_gap: Duration::from_secs(3),
                jitter: Duration::from_secs(4),
                ..LimiterConfig::per_minute(8)
            }),
            fetch_lock: tokio::sync::Mutex::new(()),
            ctx: Mutex::new(None),
            status: Mutex::new(None),
        }
    }
}

fn is_f95(u: &Url) -> bool {
    u.host_str()
        .is_some_and(|h| h == "f95zone.to" || h.ends_with(".f95zone.to"))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl F95 {
    pub fn set_context(&self, ctx: GameCtx) {
        *lock(&self.ctx) = Some((ctx, Instant::now()));
    }

    pub fn context(&self) -> Option<GameCtx> {
        lock(&self.ctx)
            .as_ref()
            .filter(|(_, at)| at.elapsed() < CONTEXT_TTL)
            .map(|(c, _)| c.clone())
    }

    pub fn status(&self) -> Option<SessionStatus> {
        lock(&self.status).clone()
    }

    fn set_status(&self, logged_in: bool, username: Option<String>) {
        *lock(&self.status) = Some(SessionStatus {
            checked_at: crate::db::now(),
            logged_in,
            username,
        });
    }
}

fn needs_browser(reason: &str, url: &str) -> ApiError {
    ApiError::NeedsBrowser {
        reason: reason.into(),
        url: url.into(),
    }
}

/// Loads one F95 page in a hidden window and runs the parser on it.
async fn read_page(app: &AppHandle, url: &str) -> Result<PageResult, ApiError> {
    let f95 = app.state::<Arc<F95>>().inner().clone();
    let _one_at_a_time = f95.fetch_lock.lock().await;
    if let Err(LimitError::CoolingDown(d)) = f95.pacer.acquire(Priority::Foreground).await {
        return Err(ApiError::RateLimited {
            retry_in_secs: d.as_secs().max(1),
        });
    }

    if let Some(w) = app.get_webview_window(WORKER) {
        let _ = w.destroy();
    }
    let target = Url::parse(url).map_err(ApiError::internal)?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let loaded = tx.clone();
    let window = WebviewWindowBuilder::new(app, WORKER, WebviewUrl::External(target))
        .title("F95 reader")
        .visible(false)
        .focused(false)
        .skip_taskbar(true)
        // Stay on F95 (and Cloudflare's check); never wander elsewhere.
        .on_navigation(|u| {
            is_f95(u)
                || u.host_str() == Some("challenges.cloudflare.com")
                || u.as_str() == "about:blank"
        })
        .on_new_window(|_, _| NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_page_load(move |_, p| {
            if p.event() == PageLoadEvent::Finished && is_f95(p.url())
                && let Some(tx) = lock(&loaded).take() {
                    let _ = tx.send(());
                }
        })
        .build()
        .map_err(ApiError::internal)?;

    let result = async {
        tokio::time::timeout(Duration::from_secs(45), rx)
            .await
            .map_err(|_| ApiError::Network {
                message: "F95 took too long to answer.".into(),
            })?
            .map_err(ApiError::internal)?;

        let (jtx, jrx) = tokio::sync::oneshot::channel::<String>();
        let jtx = Mutex::new(Some(jtx));
        let script = format!("(function(){{ {PARSER}\n return f95gmParse(document); }})()");
        window
            .eval_with_callback(script, move |json| {
                if let Some(tx) = lock(&jtx).take() {
                    let _ = tx.send(json);
                }
            })
            .map_err(ApiError::internal)?;
        let json = tokio::time::timeout(Duration::from_secs(15), jrx)
            .await
            .map_err(|_| ApiError::internal("reading the page timed out"))?
            .map_err(ApiError::internal)?;
        serde_json::from_str::<PageResult>(&json)
            .map_err(|e| ApiError::internal(format!("couldn't read the F95 page: {e}")))
    }
    .await;
    let _ = window.destroy();
    drop(tx);

    if let Ok(PageResult::RateLimited) = result {
        let d = f95.pacer.report_rate_limited(None).await;
        return Err(ApiError::RateLimited {
            retry_in_secs: d.as_secs().max(1),
        });
    }
    result
}

/// Download links from a thread's first post. Cached per version, so asking
/// again for the same version never reloads the page.
pub async fn thread_links(
    app: &AppHandle,
    thread_id: i64,
    version: &str,
    force: bool,
) -> Result<ThreadLinks, ApiError> {
    let db = app.state::<Arc<Db>>().inner().clone();
    if !force
        && let Some((v, at, groups)) = db.cached_links(thread_id).map_err(ApiError::internal)?
            && v == version
                && let Ok(groups) = serde_json::from_str(&groups) {
                    return Ok(ThreadLinks {
                        thread_id,
                        version: v,
                        fetched_at: at,
                        from_cache: true,
                        groups: mark_in_app(groups).await,
                    });
                }
    let url = format!("https://f95zone.to/threads/{thread_id}/");
    let f95 = app.state::<Arc<F95>>().inner().clone();
    match read_page(app, &url).await? {
        PageResult::Ok {
            logged_in,
            username,
            groups,
        } => {
            f95.set_status(logged_in, username);
            let _ = app.emit("f95-status", f95.status());
            if groups.is_empty() && !logged_in {
                return Err(needs_browser("login", LOGIN_URL));
            }
            let json = serde_json::to_string(&groups).map_err(ApiError::internal)?;
            db.cache_links(thread_id, version, &json)
                .map_err(ApiError::internal)?;
            Ok(ThreadLinks {
                thread_id,
                version: version.to_string(),
                fetched_at: crate::db::now(),
                from_cache: false,
                groups: mark_in_app(groups).await,
            })
        }
        PageResult::Challenge => Err(needs_browser("challenge", &url)),
        PageResult::LoginRequired => Err(needs_browser("login", LOGIN_URL)),
        PageResult::NotFound { .. } => Err(ApiError::NotFound),
        PageResult::RateLimited => unreachable!("handled in read_page"),
    }
}

/// Whether the user is signed in to F95 in the launcher's browser.
pub async fn session_status(app: &AppHandle, force: bool) -> Result<SessionStatus, ApiError> {
    let f95 = app.state::<Arc<F95>>().inner().clone();
    if !force
        && let Some(s) = f95.status() {
            return Ok(s);
        }
    match read_page(app, "https://f95zone.to/").await? {
        PageResult::Ok {
            logged_in,
            username,
            ..
        }
        | PageResult::NotFound {
            logged_in,
            username,
        } => f95.set_status(logged_in, username),
        PageResult::LoginRequired => f95.set_status(false, None),
        PageResult::Challenge => return Err(needs_browser("challenge", "https://f95zone.to/")),
        PageResult::RateLimited => unreachable!("handled in read_page"),
    }
    let s = f95.status().expect("just set");
    let _ = app.emit("f95-status", Some(&s));
    Ok(s)
}

/// A link the launcher can fetch itself turns into a queued download
/// instead of a page in the browser window.
fn capture(app: &AppHandle, u: &Url) -> bool {
    let Some(host) = hosts::supported(u, &hosts::HostCreds::load()) else {
        return false;
    };
    let f95 = app.state::<Arc<F95>>();
    let Some(ctx) = f95.context() else {
        return false;
    };
    let downloads = app.state::<Arc<Downloads>>();
    match downloads.enqueue(app, &ctx, u.as_str(), &host) {
        Ok(_) => {
            let _ = app.emit("download-captured", &ctx.title);
            true
        }
        Err(e) => {
            log::warn!("couldn't queue {u}: {e}");
            false
        }
    }
}

/// Shows the visible F95 window at `url`. Links clicked there for `ctx`
/// (after any captcha) are added to the download queue.
///
/// Must not be called from a synchronous command or an event handler: on
/// Windows, building a window there deadlocks (see Tauri's
/// `WebviewWindowBuilder::build` docs).
pub fn open_browser(app: &AppHandle, url: &str, ctx: Option<GameCtx>) -> Result<(), ApiError> {
    let target = Url::parse(url).map_err(|_| ApiError::Invalid {
        message: "That link isn't valid.".into(),
    })?;
    if !matches!(target.scheme(), "https" | "http") {
        return Err(ApiError::Invalid {
            message: "Only web links can be opened.".into(),
        });
    }
    if let Some(c) = ctx {
        app.state::<Arc<F95>>().set_context(c);
    }
    if let Some(w) = app.get_webview_window(BROWSER) {
        w.navigate(target).map_err(ApiError::internal)?;
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        return Ok(());
    }

    let nav_app = app.clone();
    let popup_app = app.clone();
    let dl_app = app.clone();
    WebviewWindowBuilder::new(app, BROWSER, WebviewUrl::External(target))
        .title("F95zone · F95 Game Manager")
        .inner_size(1120.0, 820.0)
        .min_inner_size(720.0, 520.0)
        .center()
        .on_navigation(move |u| !capture(&nav_app, u))
        // Popups (download buttons, ads) open in this same window instead.
        .on_new_window(move |u, _| {
            if !capture(&popup_app, &u) && matches!(u.scheme(), "https" | "http") {
                let app = popup_app.clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(w) = app.get_webview_window(BROWSER) {
                        let _ = w.navigate(u);
                    }
                });
            }
            NewWindowResponse::Deny
        })
        .on_download(move |_, event| {
            let downloads = dl_app.state::<Arc<Downloads>>().inner().clone();
            match event {
                DownloadEvent::Requested { url, destination } => {
                    let ctx = dl_app.state::<Arc<F95>>().context();
                    let name = destination
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned());
                    if let Some(path) = downloads.add_browser_job(&dl_app, ctx, url.as_str(), name) {
                        *destination = path;
                    }
                    true
                }
                DownloadEvent::Finished { url, path, success } => {
                    downloads.browser_finished(&dl_app, url.as_str(), path, success);
                    true
                }
                _ => true,
            }
        })
        .build()
        .map_err(ApiError::internal)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_page_results() {
        let ok: PageResult = serde_json::from_str(
            r#"{"kind":"ok","logged_in":true,"username":"Me","groups":[{"label":"Win","os":["windows"],"links":[{"host":"mega.nz","url":"https://f95zone.to/masked/mega.nz/1/x","label":"MEGA","masked":true}]}]}"#,
        )
        .unwrap();
        assert!(matches!(ok, PageResult::Ok { logged_in: true, ref groups, .. } if groups[0].links[0].masked));
        assert!(matches!(
            serde_json::from_str::<PageResult>(r#"{"kind":"challenge"}"#).unwrap(),
            PageResult::Challenge
        ));
        assert!(matches!(
            serde_json::from_str::<PageResult>(r#"{"kind":"not_found","logged_in":false,"username":null}"#).unwrap(),
            PageResult::NotFound { logged_in: false, .. }
        ));
    }

    #[test]
    fn only_f95_hosts_count() {
        assert!(is_f95(&Url::parse("https://f95zone.to/threads/1/").unwrap()));
        assert!(is_f95(&Url::parse("https://attachments.f95zone.to/x").unwrap()));
        assert!(!is_f95(&Url::parse("https://f95zone.to.evil.com/").unwrap()));
    }

    #[test]
    fn context_expires() {
        let f = F95::default();
        assert!(f.context().is_none());
        f.set_context(GameCtx {
            thread_id: 1,
            title: "T".into(),
            creator: "C".into(),
            version: "v1".into(),
            category: String::new(),
        });
        assert_eq!(f.context().unwrap().thread_id, 1);
        if let Some(old) = Instant::now().checked_sub(CONTEXT_TTL) {
            *lock(&f.ctx) = Some((f.context().unwrap(), old));
            assert!(f.context().is_none());
        }
    }
}
