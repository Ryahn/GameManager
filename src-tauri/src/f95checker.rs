//! Importing a library from F95Checker (github.com/WillyJL/F95Checker).
//!
//! Reads its `db.sqlite3` read-only (schema from `modules/db.py`):
//! - `games.installed` / `games.finished`: version strings ("" = no)
//! - `games.executables`: JSON list of paths, relative to
//!   `settings.default_exe_dir` (a JSON object keyed by OS, "1" = Windows)
//!   when inside it, otherwise absolute
//! - `games.labels`: JSON list of ids from the `labels` table
//! - negative ids are custom games with no F95 thread: skipped
//!
//! Nothing in the F95Checker database is changed. Close F95Checker first:
//! it saves every 30 seconds.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

use crate::api::types::ThreadSummary;
use crate::db::{Db, LocalPatch, PlayStatus};

#[derive(Debug, Clone, PartialEq)]
pub struct CheckerGame {
    pub id: i64,
    pub name: String,
    pub version: String,
    pub developer: String,
    pub url: String,
    pub image_url: String,
    pub installed: Option<String>,
    pub finished: Option<String>,
    pub executables: Vec<PathBuf>,
    pub rating: u8,
    pub notes: String,
    pub labels: Vec<String>,
    pub playtime_secs: i64,
    pub last_launched: Option<i64>,
    pub added_on: Option<i64>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct ImportSummary {
    pub games: usize,
    pub skipped_custom: usize,
    pub executables_linked: usize,
    pub executables_missing: usize,
    pub added_to_library: usize,
    pub not_in_catalog: usize,
}

/// `%APPDATA%\f95checker\db.sqlite3`
pub fn default_path() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var_os("APPDATA")?)
        .join("f95checker")
        .join("db.sqlite3");
    p.is_file().then_some(p)
}

fn version_flag(v: &str, current: &str) -> Option<String> {
    match v.trim() {
        "" | "False" | "0" => None,
        // Very old databases stored booleans.
        "True" | "1" => Some(current.to_string()).filter(|c| !c.is_empty() && c != "Unchecked"),
        other => Some(other.to_string()),
    }
}

fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    conn.prepare(&format!("SELECT {col} FROM {table} LIMIT 0")).is_ok()
}

/// Reads every real (non-custom) game. Returns `(games, skipped_custom)`.
pub fn read(path: &Path) -> Result<(Vec<CheckerGame>, usize), String> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("Couldn't open the F95Checker database: {e}"))?;
    if !has_column(&conn, "games", "id") {
        return Err("That file isn't an F95Checker database.".into());
    }

    // Base folder for relative executable paths (Windows key "1").
    let exe_base: Option<PathBuf> = has_column(&conn, "settings", "default_exe_dir")
        .then(|| {
            conn.query_row("SELECT default_exe_dir FROM settings LIMIT 1", [], |r| r.get::<_, String>(0))
                .ok()
        })
        .flatten()
        .and_then(|json| {
            let v: serde_json::Value = serde_json::from_str(&json).ok()?;
            match v {
                serde_json::Value::String(s) => Some(s),
                serde_json::Value::Object(m) => m
                    .get("1")
                    .or_else(|| m.get(""))
                    .and_then(|s| s.as_str())
                    .map(str::to_string),
                _ => None,
            }
        })
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from);

    let labels: HashMap<i64, String> = if has_column(&conn, "labels", "name") {
        let mut st = conn.prepare("SELECT id, name FROM labels").map_err(|e| e.to_string())?;
        st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .filter_map(Result::ok)
            .collect()
    } else {
        HashMap::new()
    };

    // Older databases may lack some columns; select what exists.
    let col = |c: &str, fallback: &str| -> String {
        if has_column(&conn, "games", c) { c.to_string() } else { format!("{fallback} AS {c}") }
    };
    let sql = format!(
        "SELECT id, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {}, {} FROM games",
        col("name", "''"),
        col("version", "''"),
        col("developer", "''"),
        col("url", "''"),
        col("image_url", "''"),
        col("installed", "''"),
        col("finished", "''"),
        col("executables", "'[]'"),
        col("rating", "0"),
        col("notes", "''"),
        col("labels", "'[]'"),
        col("playtime", "0"),
        col("last_launched", "0"),
        col("added_on", "0"),
    );
    let mut st = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let mut skipped = 0;
    let mut games = Vec::new();
    let rows = st
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                r.get::<_, Option<f64>>(9)?.unwrap_or_default(),
                r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                r.get::<_, Option<String>>(11)?.unwrap_or_default(),
                r.get::<_, Option<f64>>(12)?.unwrap_or_default(),
                r.get::<_, Option<i64>>(13)?.unwrap_or_default(),
                r.get::<_, Option<i64>>(14)?.unwrap_or_default(),
            ))
        })
        .map_err(|e| e.to_string())?;
    for row in rows {
        let (id, name, version, developer, url, image_url, installed, finished, exes, rating, notes, label_ids, playtime, last, added) =
            row.map_err(|e| e.to_string())?;
        if id <= 0 {
            skipped += 1;
            continue;
        }
        let executables = serde_json::from_str::<Vec<String>>(&exes)
            .unwrap_or_default()
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                let p = PathBuf::from(&s);
                match (&exe_base, p.is_absolute()) {
                    (Some(base), false) => base.join(p),
                    _ => p,
                }
            })
            .collect();
        let labels = serde_json::from_str::<Vec<i64>>(&label_ids)
            .unwrap_or_default()
            .iter()
            .filter_map(|l| labels.get(l).cloned())
            .collect();
        games.push(CheckerGame {
            id,
            installed: version_flag(&installed, &version),
            finished: version_flag(&finished, &version),
            name,
            version,
            developer,
            url,
            image_url,
            executables,
            rating: rating.clamp(0.0, 5.0).round() as u8,
            notes,
            labels,
            playtime_secs: playtime.max(0.0) as i64,
            last_launched: (last > 0).then_some(last),
            added_on: (added > 0).then_some(added),
        });
    }
    Ok((games, skipped))
}

/// Merges imported games into the local library without overwriting what
/// the user already set here (notes, rating, versions). Returns the summary
/// (without the server bookmark counts, which the caller fills in).
pub fn apply(db: &Db, games: &[CheckerGame]) -> Result<ImportSummary, String> {
    let mut sum = ImportSummary {
        games: games.len(),
        ..Default::default()
    };
    let threads: Vec<ThreadSummary> = games
        .iter()
        .map(|g| ThreadSummary {
            thread_id: g.id,
            title: g.name.clone(),
            creator: g.developer.clone(),
            version: g.version.clone(),
            url: g.url.clone(),
            cover: (!g.image_url.is_empty()).then(|| g.image_url.clone()),
            ..Default::default()
        })
        .collect();
    db.cache_threads(&threads, true)?;

    for g in games {
        let existing = db.get(g.id)?;
        let mut patch = LocalPatch::default();
        let ex = existing.as_ref();
        if ex.is_none_or(|e| e.notes.trim().is_empty()) && !g.notes.trim().is_empty() {
            patch.notes = Some(g.notes.clone());
        }
        if ex.is_none_or(|e| e.rating.is_none()) && g.rating > 0 {
            patch.rating = Some(g.rating);
        }
        if ex.is_none_or(|e| e.installed_version.is_none()) {
            patch.installed_version = g.installed.clone();
        }
        if ex.is_none_or(|e| e.finished_version.is_none()) {
            patch.finished_version = g.finished.clone();
        }
        if !g.labels.is_empty() {
            let mut labels = ex.map(|e| e.labels.clone()).unwrap_or_default();
            labels.extend(g.labels.iter().cloned());
            patch.labels = Some(labels);
        }
        if ex.is_none_or(|e| e.play_status == PlayStatus::NotStarted) {
            patch.play_status = Some(if g.finished.is_some() {
                PlayStatus::Finished
            } else if g.last_launched.is_some() {
                PlayStatus::Playing
            } else {
                PlayStatus::NotStarted
            });
        }
        db.update(g.id, patch)?;

        let mut first_dir = None;
        for exe in &g.executables {
            if exe.is_file() {
                let s = exe.to_string_lossy();
                db.add_target(g.id, &s, &crate::library::detect::label_for(exe), "")?;
                first_dir.get_or_insert_with(|| exe.parent().map(|p| p.to_string_lossy().into_owned()));
                sum.executables_linked += 1;
            } else {
                sum.executables_missing += 1;
            }
        }
        if let Some(Some(dir)) = first_dir
            && ex.is_none_or(|e| e.install_dir.is_none()) {
                db.update(
                    g.id,
                    LocalPatch {
                        install_dir: Some(dir),
                        ..Default::default()
                    },
                )?;
            }
        db.merge_stats(g.id, g.playtime_secs, g.last_launched, g.added_on)?;
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small database with F95Checker's table layout.
    fn fixture(dir: &Path) -> PathBuf {
        let path = dir.join("db.sqlite3");
        let _ = std::fs::remove_file(&path);
        let c = Connection::open(&path).unwrap();
        let base = dir.join("Games");
        std::fs::create_dir_all(base.join("Haven")).unwrap();
        std::fs::write(base.join("Haven").join("Haven.exe"), b"x").unwrap();
        let base_json = serde_json::json!({ "1": base.to_string_lossy() }).to_string();
        c.execute_batch(
            "CREATE TABLE settings (_ INTEGER PRIMARY KEY CHECK (_=0), default_exe_dir TEXT DEFAULT '{}');
             CREATE TABLE labels (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT DEFAULT '', color TEXT, position INTEGER);
             CREATE TABLE games (id INTEGER PRIMARY KEY, custom INTEGER, name TEXT, version TEXT, developer TEXT,
               url TEXT, image_url TEXT, installed TEXT, finished TEXT, executables TEXT, rating INTEGER,
               notes TEXT, labels TEXT, playtime REAL, last_launched INTEGER, added_on INTEGER, archived INTEGER);
             INSERT INTO labels (id, name) VALUES (1, 'Favourite'), (2, 'Later');",
        )
        .unwrap();
        c.execute("INSERT INTO settings (_, default_exe_dir) VALUES (0, ?1)", [&base_json]).unwrap();
        c.execute_batch(
            "INSERT INTO games VALUES (101, NULL, 'Haven', 'v0.9', 'Dev', 'https://f95zone.to/threads/101/', '',
                'v0.8', '', '[\"Haven/Haven.exe\", \"Gone/Gone.exe\"]', 4, 'route A', '[1]', 3600.5, 1700000000, 1690000000, 0);
             INSERT INTO games VALUES (102, NULL, 'Old', 'v1.0', 'Dev2', '', '', 'True', 'True', '[]', 0, '', '[2, 99]', 0, 0, 0, 0);
             INSERT INTO games VALUES (-1, 1, 'Custom game (-1)', '', '', '', '', '', '', '[]', 0, '', '[]', 0, 0, 0, 0);",
        )
        .unwrap();
        path
    }

    #[test]
    fn reads_games_labels_and_relative_executables() {
        let dir = std::env::temp_dir().join(format!("f95gm-f95c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (games, skipped) = read(&fixture(&dir)).unwrap();
        assert_eq!(skipped, 1, "custom game");
        let haven = games.iter().find(|g| g.id == 101).unwrap();
        assert_eq!(haven.installed.as_deref(), Some("v0.8"));
        assert_eq!(haven.finished, None);
        assert_eq!(haven.labels, vec!["Favourite"]);
        assert_eq!(haven.rating, 4);
        assert_eq!(haven.playtime_secs, 3600);
        assert!(haven.executables[0].ends_with("Haven/Haven.exe") || haven.executables[0].ends_with("Haven\\Haven.exe"));
        let old = games.iter().find(|g| g.id == 102).unwrap();
        assert_eq!(old.installed.as_deref(), Some("v1.0"), "legacy True means the current version");
        assert_eq!(old.labels, vec!["Later"], "unknown label ids are dropped");

        let db = Db::in_memory();
        db.update(101, LocalPatch { notes: Some("mine".into()), ..Default::default() }).unwrap();
        let sum = apply(&db, &games).unwrap();
        assert_eq!(sum.executables_linked, 1);
        assert_eq!(sum.executables_missing, 1);
        let g = db.get(101).unwrap().unwrap();
        assert_eq!(g.notes, "mine", "never overwrites local notes");
        assert_eq!(g.rating, Some(4));
        assert_eq!(g.installed_version.as_deref(), Some("v0.8"));
        assert_eq!(g.playtime_secs, 3600);
        assert_eq!(g.targets.len(), 1);
        assert!(g.install_dir.is_some());
        assert_eq!(db.get(102).unwrap().unwrap().play_status, PlayStatus::Finished);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rejects_other_files() {
        let dir = std::env::temp_dir().join(format!("f95gm-f95c-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.sqlite3");
        Connection::open(&p).unwrap().execute_batch("CREATE TABLE other (a INTEGER);").unwrap();
        assert!(read(&p).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
