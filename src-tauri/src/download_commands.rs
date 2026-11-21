//! Commands for the F95 window, thread links and the download queue.

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use url::Url;

use crate::api::error::ApiError;
use crate::commands::AppState;
use crate::db::Db;
use crate::downloads::{Downloads, Snapshot, hosts, limits};
use crate::f95::{self, GameCtx, Link, SessionStatus, ThreadLinks};

#[tauri::command]
pub async fn f95_status(app: AppHandle, force: bool) -> Result<Option<SessionStatus>, ApiError> {
    if !force {
        return Ok(app.state_f95().status());
    }
    f95::session_status(&app, true).await.map(Some)
}

/// Opens the F95 window (sign-in page by default).
///
/// Async on purpose: on Windows, creating a window from a synchronous
/// command deadlocks the main thread (white window that can't be closed).
#[tauri::command]
pub async fn f95_open(app: AppHandle, url: Option<String>) -> Result<(), ApiError> {
    f95::open_browser(&app, url.as_deref().unwrap_or(f95::LOGIN_URL), None)
}

#[tauri::command]
pub async fn links_get(
    app: AppHandle,
    thread_id: i64,
    version: String,
    force: bool,
) -> Result<ThreadLinks, ApiError> {
    f95::thread_links(&app, thread_id, &version, force).await
}

#[tauri::command]
pub async fn download_preflight(
    downloads: State<'_, Arc<Downloads>>,
    db: State<'_, Arc<Db>>,
    hosts: Vec<String>,
) -> Result<limits::Preflight, ApiError> {
    let daily = db
        .starts_since(crate::db::now() - 86_400)
        .map_err(ApiError::internal)?;
    Ok(limits::preflight(&downloads.queued_per_host(), &hosts, daily))
}

#[derive(Serialize)]
pub struct StartOutcome {
    /// Queued for the launcher to fetch.
    pub job_id: Option<i64>,
    /// Opened in the F95 window for the user to click through.
    pub opened_browser: bool,
}

/// Starts a download for one link. Links the launcher can fetch go in the
/// queue; masked links and other hosts open in the F95 window, where
/// whatever the user downloads is tracked for this game.
#[tauri::command]
pub async fn download_start(
    app: AppHandle,
    state: State<'_, AppState>,
    downloads: State<'_, Arc<Downloads>>,
    db: State<'_, Arc<Db>>,
    game: GameCtx,
    link: Link,
    confirm: Option<String>,
) -> Result<StartOutcome, ApiError> {
    let settings = state.settings.get().await;
    downloads.set_folders(&settings);
    let kind = crate::media::Kind::parse(&game.category);
    if downloads.folder(kind).is_none() {
        let what = match kind {
            crate::media::Kind::Games => "games",
            crate::media::Kind::Comics => "comics",
            crate::media::Kind::Animations => "animations",
        };
        return Err(ApiError::Invalid {
            message: format!("Choose a folder for {what} in Settings first. Downloads are installed there."),
        });
    }

    let url = Url::parse(&link.url).map_err(|_| ApiError::Invalid {
        message: "That link isn't valid.".into(),
    })?;
    let creds = tauri::async_runtime::spawn_blocking(hosts::HostCreds::load)
        .await
        .unwrap_or_default();
    let host = (!link.masked).then(|| hosts::supported(&url, &creds)).flatten();

    let daily = db
        .starts_since(crate::db::now() - 86_400)
        .map_err(ApiError::internal)?;
    let pre = limits::preflight(
        &downloads.queued_per_host(),
        &[host.clone().unwrap_or_else(|| link.host.clone())],
        daily,
    );
    if pre.needs_confirm && !limits::confirmed(confirm.as_deref()) {
        return Err(ApiError::ConfirmRequired {
            daily,
            limit: limits::DAILY_SOFT_LIMIT,
        });
    }

    match host {
        Some(host) => {
            let id = downloads
                .enqueue(&app, &game, url.as_str(), &host)
                .map_err(|message| ApiError::Invalid { message })?;
            Ok(StartOutcome {
                job_id: Some(id),
                opened_browser: false,
            })
        }
        None => {
            f95::open_browser(&app, url.as_str(), Some(game))?;
            Ok(StartOutcome {
                job_id: None,
                opened_browser: true,
            })
        }
    }
}

#[tauri::command]
pub async fn downloads_list(
    state: State<'_, AppState>,
    downloads: State<'_, Arc<Downloads>>,
    db: State<'_, Arc<Db>>,
) -> Result<Snapshot, ApiError> {
    let s = state.settings.get().await;
    Ok(downloads.snapshot(&db, limits::clamp_active(s.max_active_downloads as usize)))
}

#[tauri::command]
pub fn download_pause(app: AppHandle, downloads: State<'_, Arc<Downloads>>, id: i64) {
    downloads.pause(&app, id);
}

#[tauri::command]
pub fn download_resume(app: AppHandle, downloads: State<'_, Arc<Downloads>>, id: i64) {
    downloads.resume(&app, id);
}

#[tauri::command]
pub fn download_remove(app: AppHandle, downloads: State<'_, Arc<Downloads>>, id: i64) {
    downloads.remove(&app, id);
}

#[tauri::command]
pub fn downloads_clear_finished(app: AppHandle, downloads: State<'_, Arc<Downloads>>) {
    downloads.clear_finished(&app);
}

#[tauri::command]
pub fn download_open_folder(
    app: AppHandle,
    downloads: State<'_, Arc<Downloads>>,
    id: i64,
) -> Result<(), ApiError> {
    let p = downloads.path_of(id).ok_or_else(|| ApiError::Invalid {
        message: "There's nothing on disk for this download yet.".into(),
    })?;
    app.opener()
        .reveal_item_in_dir(&p)
        .map_err(|e| ApiError::Invalid {
            message: format!("Couldn't open the folder: {e}"),
        })
}

/// Opens a "needs you" job's page in the F95 window for this game.
/// Async for the same reason as `f95_open`.
#[tauri::command]
pub async fn download_open_in_browser(
    app: AppHandle,
    downloads: State<'_, Arc<Downloads>>,
    id: i64,
) -> Result<(), ApiError> {
    let job = downloads.job(id).ok_or(ApiError::NotFound)?;
    let ctx = job.thread_id.map(|thread_id| GameCtx {
        thread_id,
        title: job.title.clone(),
        creator: job.creator.clone(),
        version: job.version.clone(),
        category: job.category.clone(),
    });
    f95::open_browser(&app, &job.page_url, ctx)?;
    downloads.remove(&app, id);
    Ok(())
}

trait StateF95 {
    fn state_f95(&self) -> Arc<f95::F95>;
}

impl StateF95 for AppHandle {
    fn state_f95(&self) -> Arc<f95::F95> {
        use tauri::Manager;
        self.state::<Arc<f95::F95>>().inner().clone()
    }
}

// ------------------------------------------------------------ JDownloader

/// JDownloader's documented Click'n'Load listener on this PC.
const CNL: &str = "http://127.0.0.1:9666";

/// Whether the user's own JDownloader is running with Click'n'Load on.
#[tauri::command]
pub async fn jdownloader_status(state: State<'_, AppState>) -> Result<bool, ApiError> {
    let ok = state
        .http
        .get(format!("{CNL}/jdcheck.js"))
        .timeout(std::time::Duration::from_millis(800))
        .send()
        .await
        .ok()
        .filter(|r| r.status().is_success());
    Ok(match ok {
        Some(r) => r.text().await.unwrap_or_default().contains("jdownloader=true"),
        None => false,
    })
}

/// Hands a host link to the user's JDownloader (Click'n'Load,
/// `POST /flash/add`). JDownloader then applies its own host handling; the
/// file isn't installed by the launcher. Counts toward the daily total.
#[tauri::command]
pub async fn jdownloader_send(
    state: State<'_, AppState>,
    downloads: State<'_, Arc<Downloads>>,
    db: State<'_, Arc<Db>>,
    game: GameCtx,
    link: Link,
    confirm: Option<String>,
) -> Result<(), ApiError> {
    if link.masked {
        return Err(ApiError::Invalid {
            message: "Masked F95 links need the captcha in the F95 window first.".into(),
        });
    }
    let url = Url::parse(&link.url).map_err(|_| ApiError::Invalid {
        message: "That link isn't valid.".into(),
    })?;
    let daily = db
        .starts_since(crate::db::now() - 86_400)
        .map_err(ApiError::internal)?;
    let pre = limits::preflight(&downloads.queued_per_host(), std::slice::from_ref(&link.host), daily);
    if pre.needs_confirm && !limits::confirmed(confirm.as_deref()) {
        return Err(ApiError::ConfirmRequired {
            daily,
            limit: limits::DAILY_SOFT_LIMIT,
        });
    }
    // The wiki form names the field "urls"; the developer reference says
    // "url". Sending both is harmless.
    let source = format!("F95 Game Manager: {} {}", game.title, game.version);
    let resp = state
        .http
        .post(format!("{CNL}/flash/add"))
        .form(&[
            ("urls", url.as_str()),
            ("url", url.as_str()),
            ("source", source.as_str()),
            ("passwords", ""),
        ])
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|_| ApiError::Invalid {
            message: "JDownloader isn't answering. Start it and turn on Click'n'Load, then try again.".into(),
        })?;
    if !resp.status().is_success() {
        return Err(ApiError::Invalid {
            message: format!("JDownloader refused the link ({}).", resp.status()),
        });
    }
    db.log_start(crate::db::now()).map_err(ApiError::internal)?;
    Ok(())
}
