//! Commands for the local library: launch targets, playtime, notes and the
//! folder scanner. Everything here works offline except catalog matching.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_opener::OpenerExt;

use crate::api::error::ApiError;
use crate::commands::AppState;
use crate::db::{Db, LocalGame, LocalPatch, PlaySession};
use crate::launch::Launcher;
use crate::library::{self, ScanFolder, detect};
use crate::limiter::Priority;

fn internal(e: String) -> ApiError {
    ApiError::Internal { message: e }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::Invalid {
        message: message.into(),
    }
}

fn fetch(db: &Db, id: i64) -> Result<LocalGame, ApiError> {
    db.get(id)
        .map_err(internal)?
        .ok_or_else(|| internal(format!("game {id} has no local data")))
}

#[tauri::command]
pub fn library_local(db: State<'_, Arc<Db>>) -> Result<Vec<LocalGame>, ApiError> {
    db.list().map_err(internal)
}

#[tauri::command]
pub fn game_local(db: State<'_, Arc<Db>>, id: i64) -> Result<Option<LocalGame>, ApiError> {
    db.get(id).map_err(internal)
}

#[tauri::command]
pub fn game_sessions(db: State<'_, Arc<Db>>, id: i64) -> Result<Vec<PlaySession>, ApiError> {
    db.sessions(id, 50).map_err(internal)
}

#[tauri::command]
pub fn game_update(
    db: State<'_, Arc<Db>>,
    id: i64,
    patch: LocalPatch,
) -> Result<LocalGame, ApiError> {
    db.update(id, patch).map_err(internal)?;
    fetch(&db, id)
}

fn check_launchable(path: &Path) -> Result<(), ApiError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if !detect::LAUNCHABLE_EXT.contains(&ext.as_str()) {
        return Err(invalid(
            "Pick the game's .exe (or index.html for browser games).",
        ));
    }
    if !path.is_file() {
        return Err(invalid("That file doesn't exist any more."));
    }
    Ok(())
}

#[tauri::command]
pub fn target_add(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    id: i64,
    path: String,
) -> Result<LocalGame, ApiError> {
    let p = PathBuf::from(&path);
    check_launchable(&p)?;
    db.add_target(id, &path, &detect::label_for(&p), "")
        .map_err(internal)?;
    let game = fetch(&db, id)?;
    if game.install_dir.is_none()
        && let Some(dir) = p.parent() {
            db.update(
                id,
                LocalPatch {
                    install_dir: Some(dir.to_string_lossy().into_owned()),
                    ..Default::default()
                },
            )
            .map_err(internal)?;
        }
    crate::tray::refresh(&app);
    fetch(&db, id)
}

#[tauri::command]
pub fn target_remove(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    target_id: i64,
) -> Result<Option<LocalGame>, ApiError> {
    let game = db.remove_target(target_id).map_err(internal)?;
    crate::tray::refresh(&app);
    match game {
        Some(id) => db.get(id).map_err(internal),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn target_set_default(
    db: State<'_, Arc<Db>>,
    target_id: i64,
) -> Result<Option<LocalGame>, ApiError> {
    db.set_default_target(target_id).map_err(internal)?;
    match db.target(target_id).map_err(internal)? {
        Some((id, _)) => db.get(id).map_err(internal),
        None => Ok(None),
    }
}

/// Launch candidates found in a folder the user picked.
#[tauri::command]
pub async fn folder_detect(dir: String) -> Result<Vec<detect::Candidate>, ApiError> {
    tauri::async_runtime::spawn_blocking(move || detect::candidates(Path::new(&dir)))
        .await
        .map_err(ApiError::internal)
}

/// Links a game to its folder: remembers the folder, adds the detected
/// executables (best one as default) and the version if known.
pub(crate) fn link_folder(db: &Db, id: i64, dir: &str, version: Option<String>) -> Result<usize, String> {
    let found = detect::candidates(Path::new(dir));
    for c in &found {
        db.add_target(id, &c.path, &c.label, "")?;
    }
    db.update(
        id,
        LocalPatch {
            install_dir: Some(dir.to_string()),
            installed_version: version,
            ..Default::default()
        },
    )?;
    Ok(found.len())
}

#[tauri::command]
pub async fn game_link_folder(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    id: i64,
    dir: String,
    version: Option<String>,
) -> Result<LocalGame, ApiError> {
    if !Path::new(&dir).is_dir() {
        return Err(invalid("That folder doesn't exist."));
    }
    let db2 = db.inner().clone();
    let d = dir.clone();
    let n = tauri::async_runtime::spawn_blocking(move || link_folder(&db2, id, &d, version))
        .await
        .map_err(ApiError::internal)?
        .map_err(internal)?;
    if n == 0 {
        return Err(invalid(
            "No game executable found in that folder. Use \"Add executable\" to pick it yourself.",
        ));
    }
    crate::tray::refresh(&app);
    fetch(&db, id)
}

#[tauri::command]
pub fn game_play(app: AppHandle, id: i64, target_id: Option<i64>) -> Result<(), ApiError> {
    crate::launch::play(&app, id, target_id).map_err(invalid)
}

#[tauri::command]
pub fn games_running(launcher: State<'_, Arc<Launcher>>) -> Vec<i64> {
    launcher.running_ids()
}

#[tauri::command]
pub fn game_open_folder(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    id: i64,
) -> Result<(), ApiError> {
    let g = fetch(&db, id)?;
    let path = g
        .targets
        .iter()
        .find(|t| t.is_default)
        .map(|t| t.path.clone())
        .or(g.install_dir.clone())
        .ok_or_else(|| invalid("This game has no folder yet."))?;
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|e| invalid(format!("Couldn't open the folder: {e}")))
}

// ----------------------------------------------------------------- scanner

#[derive(Default)]
pub struct ScanState {
    cancel: AtomicBool,
    busy: AtomicBool,
}

#[derive(Clone, Serialize)]
struct ScanProgress {
    phase: &'static str,
    done: usize,
    total: usize,
}

#[tauri::command]
pub fn library_scan_cancel(scan: State<'_, Arc<ScanState>>) {
    scan.cancel.store(true, Ordering::SeqCst);
}

/// Finds game folders under `dir` and suggests a catalog match for each.
/// Matching uses the background lane of the rate limiter, one search per
/// folder, and stops early (keeping what it has) if the API asks us to wait.
#[tauri::command]
pub async fn library_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    db: State<'_, Arc<Db>>,
    scan: State<'_, Arc<ScanState>>,
    dir: String,
) -> Result<Vec<ScanFolder>, ApiError> {
    if !Path::new(&dir).is_dir() {
        return Err(invalid("That folder doesn't exist."));
    }
    if scan.busy.swap(true, Ordering::SeqCst) {
        return Err(invalid("A scan is already running."));
    }
    scan.cancel.store(false, Ordering::SeqCst);
    let result = run_scan(&app, &state, &db, &scan, dir).await;
    scan.busy.store(false, Ordering::SeqCst);
    result
}

async fn run_scan(
    app: &AppHandle,
    state: &AppState,
    db: &Arc<Db>,
    scan: &ScanState,
    dir: String,
) -> Result<Vec<ScanFolder>, ApiError> {
    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            phase: "finding",
            done: 0,
            total: 0,
        },
    );
    let linked = db.linked_dirs().map_err(internal)?;
    let mut folders =
        tauri::async_runtime::spawn_blocking(move || library::scan(Path::new(&dir), &linked))
            .await
            .map_err(ApiError::internal)?;

    // Installed here but no longer in the library: offer it back as a match.
    for f in folders.iter_mut().filter(|f| f.linked_thread_id.is_some() && !f.linked_in_library) {
        let id = f.linked_thread_id.unwrap_or_default();
        if let Ok(Some(g)) = db.get(id) {
            let cached = g.cached.unwrap_or_default();
            f.suggestion = Some(library::MatchSuggestion {
                thread_id: id,
                title: if cached.title.is_empty() { format!("Thread {id}") } else { cached.title },
                creator: cached.creator,
                cover: cached.cover,
                confidence: 1.0,
                confident: true,
            });
        }
    }

    let total = folders
        .iter()
        .filter(|f| f.linked_thread_id.is_none())
        .count();
    let mut done = 0;
    for f in folders.iter_mut().filter(|f| f.linked_thread_id.is_none()) {
        if scan.cancel.load(Ordering::SeqCst) {
            break;
        }
        let _ = app.emit(
            "scan-progress",
            ScanProgress {
                phase: "matching",
                done,
                total,
            },
        );
        match state
            .api
            .search_with(&f.title_guess, 8, Priority::Background)
            .await
        {
            Ok(r) => f.suggestion = library::pick_match(&f.title_guess, &r.hits),
            // Keep the folders; the user can match the rest by hand.
            Err(ApiError::RateLimited { .. }) => break,
            Err(ApiError::NotSignedIn) | Err(ApiError::Unauthorized { .. }) => break,
            Err(e) => log::warn!("match for {:?} failed: {e}", f.title_guess),
        }
        done += 1;
    }
    let _ = app.emit(
        "scan-progress",
        ScanProgress {
            phase: "done",
            done,
            total,
        },
    );
    Ok(folders)
}

#[derive(Debug, Deserialize)]
pub struct ImportItem {
    pub thread_id: i64,
    pub path: String,
    pub version: Option<String>,
}

#[derive(Serialize)]
pub struct ImportResult {
    linked: usize,
    added_to_library: usize,
    failed: Vec<String>,
}

/// Links scanned folders to their threads and adds them to the library.
#[tauri::command]
pub async fn library_import(
    app: AppHandle,
    state: State<'_, AppState>,
    db: State<'_, Arc<Db>>,
    items: Vec<ImportItem>,
) -> Result<ImportResult, ApiError> {
    let mut linked = 0;
    let mut failed = Vec::new();
    let mut ids = Vec::new();
    for it in items.into_iter().filter(|i| i.thread_id > 0) {
        let db2 = db.inner().clone();
        let path = it.path.clone();
        let r = tauri::async_runtime::spawn_blocking(move || {
            link_folder(&db2, it.thread_id, &path, it.version)
        })
        .await
        .map_err(ApiError::internal)?;
        match r {
            Ok(_) => {
                linked += 1;
                ids.push(it.thread_id);
            }
            Err(e) => failed.push(format!("{}: {e}", it.path)),
        }
    }
    ids.sort_unstable();
    ids.dedup();
    let mut added = 0;
    if !ids.is_empty() {
        let r = state.api.add_bookmarks_bulk(&ids).await?;
        added = r.created.len();
        for id in r.created.iter().chain(&r.already_tracked) {
            let _ = db.set_in_library(*id, true);
        }
    }
    crate::tray::refresh(&app);
    Ok(ImportResult {
        linked,
        added_to_library: added,
        failed,
    })
}

/// Never sends a folder to the Recycle Bin that's a drive root, the games
/// folder itself or one of its parents.
fn safe_to_trash(dir: &Path, library: Option<&Path>) -> bool {
    let depth = dir.components().filter(|c| matches!(c, std::path::Component::Normal(_))).count();
    if depth < 2 || !dir.is_dir() {
        return false;
    }
    match library {
        Some(lib) => !lib.starts_with(dir),
        None => true,
    }
}

/// Removes a game from the library: stops tracking it on the server and
/// forgets its install on this PC. With `delete_files`, its folder goes to
/// the Recycle Bin (recoverable).
#[tauri::command]
pub async fn library_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    db: State<'_, Arc<Db>>,
    launcher: State<'_, Arc<Launcher>>,
    id: i64,
    delete_files: bool,
) -> Result<(), ApiError> {
    if launcher.is_running(id) {
        return Err(invalid("Close the game first."));
    }
    // Not bookmarked on the server is fine: removing is the goal.
    match state.api.remove_bookmark(id).await {
        Ok(()) | Err(ApiError::NotFound) => {}
        Err(e) => return Err(e),
    }
    let install_dir = db.get(id).map_err(internal)?.and_then(|g| g.install_dir);
    if delete_files
        && let Some(dir) = install_dir.as_deref() {
            let library = state.settings.get().await.library_dir;
            let path = PathBuf::from(dir);
            if !safe_to_trash(&path, library.as_deref().map(Path::new)) {
                return Err(invalid(format!(
                    "For safety, {dir} wasn't deleted. Remove it yourself if you want it gone."
                )));
            }
            tauri::async_runtime::spawn_blocking(move || trash::delete(&path))
                .await
                .map_err(ApiError::internal)?
                .map_err(|e| invalid(format!("Couldn't move the game to the Recycle Bin: {e}")))?;
        }
    db.unlink_install(id).map_err(internal)?;
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub fn sync_now(app: AppHandle) {
    crate::sync::wake(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_trashes_roots_or_the_games_folder() {
        let tmp = std::env::temp_dir().join(format!("f95gm-trash-{}", std::process::id()));
        let game = tmp.join("Dev").join("Game").join("1.0");
        std::fs::create_dir_all(&game).unwrap();
        assert!(safe_to_trash(&game, Some(&tmp)));
        assert!(!safe_to_trash(&tmp, Some(&tmp)), "the games folder itself");
        assert!(!safe_to_trash(tmp.parent().unwrap(), Some(&tmp)), "a parent of it");
        assert!(!safe_to_trash(Path::new("C:\\"), None), "a drive root");
        assert!(!safe_to_trash(&tmp.join("missing"), Some(&tmp)));
        let _ = std::fs::remove_dir_all(tmp);
    }
}
