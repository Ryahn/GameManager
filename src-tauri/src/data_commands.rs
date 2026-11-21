//! Import from F95Checker, and library backup / restore.

use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::api::error::ApiError;
use crate::backup;
use crate::commands::AppState;
use crate::db::Db;
use crate::f95checker::{self, ImportSummary};

fn invalid(m: impl Into<String>) -> ApiError {
    ApiError::Invalid { message: m.into() }
}

/// Adds ids to the server library in 500-id chunks; returns (added, not in catalog).
async fn bookmark_all(state: &AppState, db: &Db, ids: &[i64]) -> Result<(usize, usize), ApiError> {
    if ids.is_empty() || !state.api.has_credential().await {
        return Ok((0, 0));
    }
    let r = state.api.add_bookmarks_bulk(ids).await?;
    for id in r.created.iter().chain(&r.already_tracked) {
        let _ = db.set_in_library(*id, true);
    }
    for id in &r.not_found {
        let _ = db.set_in_library(*id, false);
    }
    Ok((r.created.len(), r.not_found.len()))
}

#[tauri::command]
pub fn f95checker_default_path() -> Option<String> {
    f95checker::default_path().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn f95checker_import(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    db: State<'_, Arc<Db>>,
    path: String,
) -> Result<ImportSummary, ApiError> {
    let p = PathBuf::from(&path);
    if !p.is_file() {
        return Err(invalid("That file doesn't exist."));
    }
    let db2 = db.inner().clone();
    let (mut sum, ids) = tauri::async_runtime::spawn_blocking(move || -> Result<(ImportSummary, Vec<i64>), String> {
        let (games, skipped) = f95checker::read(&p)?;
        let mut sum = f95checker::apply(&db2, &games)?;
        sum.skipped_custom = skipped;
        Ok((sum, games.iter().map(|g| g.id).collect()))
    })
    .await
    .map_err(ApiError::internal)?
    .map_err(invalid)?;
    let (added, missing) = bookmark_all(&state, &db, &ids).await?;
    sum.added_to_library = added;
    sum.not_in_catalog = missing;
    crate::tray::refresh(&app);
    Ok(sum)
}

#[tauri::command]
pub async fn backup_export(db: State<'_, Arc<Db>>, path: String) -> Result<usize, ApiError> {
    let db = db.inner().clone();
    tauri::async_runtime::spawn_blocking(move || backup::export(&db, &PathBuf::from(path)))
        .await
        .map_err(ApiError::internal)?
        .map_err(invalid)
}

#[derive(Serialize)]
pub struct RestoreResult {
    games: usize,
    sessions_added: usize,
    added_to_library: usize,
}

#[tauri::command]
pub async fn backup_import(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    db: State<'_, Arc<Db>>,
    path: String,
) -> Result<RestoreResult, ApiError> {
    let db2 = db.inner().clone();
    let sum = tauri::async_runtime::spawn_blocking(move || backup::restore(&db2, &PathBuf::from(path)))
        .await
        .map_err(ApiError::internal)?
        .map_err(invalid)?;
    let (added, _) = bookmark_all(&state, &db, &sum.library_ids).await?;
    crate::tray::refresh(&app);
    Ok(RestoreResult {
        games: sum.games,
        sessions_added: sum.sessions_added,
        added_to_library: added,
    })
}
