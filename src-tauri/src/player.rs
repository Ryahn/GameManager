//! Video player backend: playlists built from however the user's folders
//! are laid out, and where each video was left off.
//!
//! Videos are served through the same `gmmedia://` scheme as comic pages
//! (in byte ranges, so seeking doesn't load whole files), and only from
//! folders the player opened.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::api::error::ApiError;
use crate::db::{ContinueWatch, Db, WatchProgress};
use crate::media::is_video;
use crate::reader::{MediaRoots, enc, sorted_children};

/// Formats the webview can usually play. Anything else (avi, wmv, flv,
/// mpg, ts) goes straight to the user's own video player.
const IN_APP: &[&str] = &["mp4", "m4v", "webm", "mov", "mkv", "ogv"];

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Video {
    /// Path for the `gmmedia://` scheme.
    pub src: String,
    pub name: String,
    /// Path relative to the playlist's folder, `/`-separated; the key for
    /// saved positions, so they survive the folder being moved.
    pub file: String,
    /// Full path, for opening in another player.
    pub path: String,
    pub size: u64,
    pub playable: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Group {
    pub name: String,
    pub videos: Vec<Video>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Playlist {
    pub root: String,
    pub groups: Vec<Group>,
}

fn video(root: &Path, p: &Path) -> Video {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let rel = p.strip_prefix(root).unwrap_or(p);
    let file = if rel.as_os_str().is_empty() {
        p.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    } else {
        rel.to_string_lossy().replace('\\', "/")
    };
    Video {
        src: format!("/v/{}", enc(&p.to_string_lossy())),
        name: p
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        file,
        path: p.to_string_lossy().into_owned(),
        size: std::fs::metadata(p).map(|m| m.len()).unwrap_or(0),
        playable: IN_APP.contains(&ext.as_str()),
    }
}

fn walk(root: &Path, dir: &Path, name: String, depth: usize, out: &mut Vec<Group>) {
    let (dirs, files) = sorted_children(dir);
    let videos: Vec<Video> = files
        .iter()
        .filter(|p| is_video(p))
        .map(|p| video(root, p))
        .collect();
    if !videos.is_empty() {
        out.push(Group { name, videos });
    }
    if depth > 0 {
        for d in dirs {
            let n = d
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            walk(root, &d, n, depth - 1, out);
        }
    }
}

/// Every video in a folder (three levels deep, each subfolder its own
/// group), or a single video file.
pub fn build(root: &Path) -> Result<Playlist, String> {
    if !root.exists() {
        return Err("The folder isn't there any more. It may have been moved or deleted.".into());
    }
    let mut groups = Vec::new();
    if root.is_file() {
        if is_video(root) {
            let base = root.parent().unwrap_or(root);
            let v = video(base, root);
            groups.push(Group {
                name: v.name.clone(),
                videos: vec![v],
            });
        }
    } else {
        let name = root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        walk(root, root, name, 3, &mut groups);
    }
    Ok(Playlist {
        root: root.to_string_lossy().into_owned(),
        groups,
    })
}

// ------------------------------------------------------------- commands

fn internal(message: impl Into<String>) -> ApiError {
    ApiError::Internal {
        message: message.into(),
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::Invalid {
        message: message.into(),
    }
}

#[derive(Debug, Serialize)]
pub struct OpenedPlaylist {
    pub playlist: Playlist,
    /// Saved positions, most recently watched first.
    pub progress: Vec<WatchProgress>,
}

#[tauri::command]
pub async fn media_playlist(
    db: State<'_, Arc<Db>>,
    roots: State<'_, Arc<MediaRoots>>,
    id: i64,
) -> Result<OpenedPlaylist, ApiError> {
    let dir = db
        .get(id)
        .map_err(internal)?
        .and_then(|g| g.install_dir)
        .ok_or_else(|| {
            invalid("This has no folder yet. Download it or choose its folder first.")
        })?;
    let root = PathBuf::from(&dir);
    let playlist = tauri::async_runtime::spawn_blocking(move || build(&root))
        .await
        .map_err(|e| internal(e.to_string()))?
        .map_err(invalid)?;
    roots.allow([PathBuf::from(dir)]);
    let progress = db.watch_get(id).map_err(internal)?;
    Ok(OpenedPlaylist { playlist, progress })
}

#[tauri::command]
pub fn media_watch_set(
    db: State<'_, Arc<Db>>,
    id: i64,
    file: String,
    position: f64,
    duration: f64,
) -> Result<(), ApiError> {
    if !position.is_finite() || !duration.is_finite() || position < 0.0 || duration < 0.0 {
        return Err(invalid("Bad position."));
    }
    db.watch_set(id, &file, position, duration)
        .map_err(internal)
}

#[tauri::command]
pub fn media_continue_watching(
    db: State<'_, Arc<Db>>,
    limit: Option<i64>,
) -> Result<Vec<ContinueWatch>, ApiError> {
    db.continue_watching(limit.unwrap_or(12).clamp(1, 50))
        .map_err(internal)
}

/// The latest saved spot for each title, for the game page's button.
#[tauri::command]
pub fn media_watch_last(
    db: State<'_, Arc<Db>>,
    id: i64,
) -> Result<Option<WatchProgress>, ApiError> {
    Ok(db.watch_get(id).map_err(internal)?.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::{MediaRoots, serve};
    use std::fs;
    use tauri::http::{StatusCode, header};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f95gm-player-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn builds_groups_in_natural_order() {
        let d = tmp("list");
        for f in [
            "Season 1/Ep 10.mp4",
            "Season 1/Ep 2.mp4",
            "Season 2/Ep 1.webm",
            "intro.mkv",
            "old.avi",
            "notes.txt",
        ] {
            let p = d.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, b"0123456789").unwrap();
        }
        let pl = build(&d).unwrap();
        let shape: Vec<(String, Vec<String>)> = pl
            .groups
            .iter()
            .map(|g| {
                (
                    g.name.clone(),
                    g.videos.iter().map(|v| v.file.clone()).collect(),
                )
            })
            .collect();
        assert_eq!(shape[0].1, vec!["intro.mkv", "old.avi"]);
        assert_eq!(
            shape[1],
            (
                "Season 1".into(),
                vec!["Season 1/Ep 2.mp4".into(), "Season 1/Ep 10.mp4".into()]
            )
        );
        assert_eq!(shape[2].0, "Season 2");
        assert!(
            !pl.groups[0].videos[1].playable,
            "avi opens in another player"
        );
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn serves_videos_in_ranges() {
        let d = tmp("range");
        let file = d.join("clip.mp4");
        fs::write(&file, b"0123456789").unwrap();
        let pl = build(&d).unwrap();
        let src = &pl.groups[0].videos[0].src;
        let roots = MediaRoots::default();
        assert_eq!(
            serve(&roots, src, None).status(),
            StatusCode::FORBIDDEN,
            "not opened yet"
        );
        roots.allow([d.clone()]);

        let r = serve(&roots, src, Some("bytes=2-5"));
        assert_eq!(r.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(r.body(), b"2345");
        assert_eq!(r.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(serve(&roots, src, Some("bytes=7-")).body(), b"789");
        assert_eq!(serve(&roots, src, Some("bytes=-3")).body(), b"789");
        assert_eq!(serve(&roots, src, None).body(), b"0123456789");
        assert_eq!(
            serve(&roots, src, Some("bytes=20-")).status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn remembers_positions() {
        let db = Db::in_memory();
        db.watch_set(3, "Season 1/Ep 2.mp4", 61.5, 1200.0).unwrap();
        db.watch_set(3, "Season 1/Ep 10.mp4", 5.0, 900.0).unwrap();
        db.watch_set(3, "Season 1/Ep 2.mp4", 75.0, 1200.0).unwrap();
        let all = db.watch_get(3).unwrap();
        assert_eq!(all.len(), 2);
        let ep2 = all.iter().find(|w| w.file == "Season 1/Ep 2.mp4").unwrap();
        assert_eq!(ep2.position, 75.0);
    }
}
