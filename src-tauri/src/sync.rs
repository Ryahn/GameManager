//! Background update checks.
//!
//! The server already re-checks every bookmarked thread every 30 minutes
//! (stealth updates included), so the launcher only reads the small
//! "updated" inbox on the same cadence. That's one or two requests per half
//! hour, in the background lane of the rate limiter.

use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::api::error::ApiError;
use crate::api::types::Bookmark;
use crate::commands::AppState;
use crate::db::Db;
use crate::limiter::Priority;

const FIRST_CHECK: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(30 * 60);

#[derive(Default)]
pub struct UpdateChecker {
    wake: Notify,
}

#[derive(Clone, Serialize)]
pub struct UpdatesEvent {
    pub count: usize,
}

pub fn wake(app: &AppHandle) {
    if let Some(s) = app.try_state::<Arc<UpdateChecker>>() {
        s.wake.notify_one();
    }
}

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let sync = app.state::<Arc<UpdateChecker>>().inner().clone();
        tokio::select! {
            _ = tokio::time::sleep(FIRST_CHECK) => {}
            _ = sync.wake.notified() => {}
        }
        loop {
            match check(&app).await {
                Ok(n) => log::info!("update check: {n} games with updates"),
                Err(ApiError::NotSignedIn) => {}
                Err(e) => log::warn!("update check failed: {e}"),
            }
            tokio::select! {
                _ = tokio::time::sleep(INTERVAL) => {}
                _ = sync.wake.notified() => {}
            }
        }
    });
}

/// Every bookmark the server flags as updated (paged, background priority).
pub async fn updated_bookmarks(state: &AppState) -> Result<Vec<Bookmark>, ApiError> {
    let mut all = Vec::new();
    let mut offset = 0;
    loop {
        let page = state
            .api
            .bookmarks(true, 200, offset, Priority::Background)
            .await?;
        let n = page.items.len() as u32;
        all.extend(page.items);
        offset += n;
        if n < 200 || offset as i64 >= page.total {
            break;
        }
    }
    Ok(all)
}

async fn check(app: &AppHandle) -> Result<usize, ApiError> {
    let state = app.state::<AppState>();
    if !state.api.has_credential().await {
        return Err(ApiError::NotSignedIn);
    }
    let items = updated_bookmarks(&state).await?;
    let db = app.state::<Arc<Db>>();
    let threads: Vec<_> = items.iter().filter_map(|b| b.thread.clone()).collect();
    if let Err(e) = db.cache_threads(&threads, true) {
        log::warn!("caching updated threads failed: {e}");
    }

    // A version the user chose to skip doesn't count as news.
    let skipped = |b: &Bookmark| {
        db.get(b.thread_id)
            .ok()
            .flatten()
            .and_then(|g| g.skipped_version)
            .is_some_and(|v| v == b.current_version)
    };
    let pending: Vec<&Bookmark> = items.iter().filter(|b| !skipped(b)).collect();

    crate::tray::set_update_count(app, pending.len());
    let _ = app.emit(
        "updates-changed",
        UpdatesEvent {
            count: pending.len(),
        },
    );

    if state.settings.get().await.notify_updates {
        let keys: Vec<(i64, String)> = pending
            .iter()
            .map(|b| (b.thread_id, b.current_version.clone()))
            .collect();
        let fresh = db.take_unnotified(&keys).map_err(ApiError::internal)?;
        if !fresh.is_empty() {
            let titles: Vec<String> = fresh
                .iter()
                .filter_map(|(id, _)| pending.iter().find(|b| b.thread_id == *id))
                .map(|b| {
                    b.thread
                        .as_ref()
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| format!("Thread {}", b.thread_id))
                })
                .collect();
            let title = if fresh.len() == 1 {
                "1 game has an update".to_string()
            } else {
                format!("{} games have updates", fresh.len())
            };
            crate::notify::send(app, &title, &crate::notify::summarize(&titles));
        }
    }
    Ok(pending.len())
}
