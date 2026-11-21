//! Library backup: one JSON file with everything the launcher keeps locally
//! (status, rating, notes, labels, versions, launch targets, playtime and
//! play sessions). Secrets are never included.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::db::{Db, LocalGame, LocalPatch, PlaySession};

const FORMAT: &str = "f95gm-backup";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct BackupGame {
    #[serde(flatten)]
    game: serde_json::Value,
    sessions: Vec<SessionOut>,
}

#[derive(Serialize, Deserialize)]
struct SessionOut {
    started_at: i64,
    secs: i64,
    version: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Backup {
    format: String,
    version: u32,
    exported_at: i64,
    app_version: String,
    games: Vec<BackupGame>,
}

pub fn export(db: &Db, path: &Path) -> Result<usize, String> {
    let games = db.all()?;
    let mut out = Vec::with_capacity(games.len());
    for g in &games {
        let sessions = db
            .sessions(g.thread_id, 100_000)?
            .into_iter()
            .map(|s: PlaySession| SessionOut {
                started_at: s.started_at,
                secs: s.secs,
                version: s.version,
            })
            .collect();
        out.push(BackupGame {
            game: serde_json::to_value(g).map_err(|e| e.to_string())?,
            sessions,
        });
    }
    let backup = Backup {
        format: FORMAT.into(),
        version: VERSION,
        exported_at: crate::db::now(),
        app_version: env!("CARGO_PKG_VERSION").into(),
        games: out,
    };
    let json = serde_json::to_string_pretty(&backup).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Couldn't write the backup: {e}"))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("Couldn't write the backup: {e}"))?;
    Ok(games.len())
}

#[derive(Debug, Default, Serialize, PartialEq)]
pub struct RestoreSummary {
    pub games: usize,
    pub sessions_added: usize,
    /// Thread ids marked as in the library in the backup.
    pub library_ids: Vec<i64>,
}

/// Merges a backup into the local library. Values already set here win;
/// empty ones are filled from the backup. Sessions are added once.
pub fn restore(db: &Db, path: &Path) -> Result<RestoreSummary, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let backup: Backup = serde_json::from_str(&text).map_err(|_| "That file isn't an F95 Game Manager backup.".to_string())?;
    if backup.format != FORMAT {
        return Err("That file isn't an F95 Game Manager backup.".into());
    }
    if backup.version > VERSION {
        return Err("This backup comes from a newer version of the app. Update first.".into());
    }
    let mut sum = RestoreSummary::default();
    for b in backup.games {
        let g: LocalGame = match serde_json::from_value(b.game) {
            Ok(g) => g,
            Err(e) => {
                log::warn!("skipping a game in the backup: {e}");
                continue;
            }
        };
        let id = g.thread_id;
        let ex = db.get(id)?;
        let ex = ex.as_ref();
        let keep = |mine: Option<&String>, theirs: &Option<String>| if mine.is_some() { None } else { theirs.clone() };
        let mut labels = ex.map(|e| e.labels.clone()).unwrap_or_default();
        labels.extend(g.labels.iter().cloned());
        db.update(
            id,
            LocalPatch {
                play_status: ex
                    .is_none_or(|e| e.play_status == crate::db::PlayStatus::NotStarted)
                    .then_some(g.play_status),
                rating: ex.is_none_or(|e| e.rating.is_none()).then_some(g.rating.unwrap_or(0)),
                notes: ex.is_none_or(|e| e.notes.trim().is_empty()).then(|| g.notes.clone()),
                labels: Some(labels),
                installed_version: keep(ex.and_then(|e| e.installed_version.as_ref()), &g.installed_version),
                finished_version: keep(ex.and_then(|e| e.finished_version.as_ref()), &g.finished_version),
                skipped_version: keep(ex.and_then(|e| e.skipped_version.as_ref()), &g.skipped_version),
                install_dir: keep(ex.and_then(|e| e.install_dir.as_ref()), &g.install_dir),
            },
        )?;
        for t in &g.targets {
            if Path::new(&t.path).is_file() {
                db.add_target(id, &t.path, &t.label, &t.args)?;
            }
        }
        let known: std::collections::HashSet<i64> = db.sessions(id, 100_000)?.iter().map(|s| s.started_at).collect();
        for s in b.sessions.iter().filter(|s| !known.contains(&s.started_at)) {
            db.insert_session(id, s.started_at, s.secs, s.version.as_deref())?;
            sum.sessions_added += 1;
        }
        db.merge_stats(id, g.playtime_secs, g.last_played_at, Some(g.added_at))?;
        if let Some(c) = &g.cached {
            db.cache_threads(
                &[crate::api::types::ThreadSummary {
                    thread_id: id,
                    title: c.title.clone(),
                    creator: c.creator.clone(),
                    version: c.version.clone(),
                    cover: c.cover.clone(),
                    url: c.url.clone(),
                    prefixes: c.prefixes.clone(),
                    last_promoted: c.last_promoted,
                    ..Default::default()
                }],
                false,
            )?;
        }
        if g.in_library {
            sum.library_ids.push(id);
        }
        sum.games += 1;
    }
    Ok(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_without_duplicating() {
        let dir = std::env::temp_dir().join(format!("f95gm-backup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("b.json");

        let a = Db::in_memory();
        a.cache_threads(
            &[crate::api::types::ThreadSummary {
                thread_id: 7,
                title: "Seven".into(),
                ..Default::default()
            }],
            true,
        )
        .unwrap();
        a.update(7, LocalPatch { notes: Some("n".into()), rating: Some(3), labels: Some(vec!["Fav".into()]), ..Default::default() })
            .unwrap();
        a.record_session(7, 1000, 120, Some("v1")).unwrap();
        assert_eq!(export(&a, &file).unwrap(), 1);

        let b = Db::in_memory();
        let s = restore(&b, &file).unwrap();
        assert_eq!((s.games, s.sessions_added, s.library_ids.clone()), (1, 1, vec![7]));
        let g = b.get(7).unwrap().unwrap();
        assert_eq!((g.notes.as_str(), g.rating, g.playtime_secs), ("n", Some(3), 120));
        assert_eq!(g.cached.unwrap().title, "Seven");
        // Restoring again changes nothing.
        let again = restore(&b, &file).unwrap();
        assert_eq!(again.sessions_added, 0);
        assert_eq!(b.get(7).unwrap().unwrap().playtime_secs, 120);
        assert_eq!(b.get(7).unwrap().unwrap().labels, vec!["Fav"]);

        std::fs::write(dir.join("x.json"), "{\"hello\": 1}").unwrap();
        assert!(restore(&b, &dir.join("x.json")).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
