//! Local SQLite library: per-game state that belongs to this machine
//! (installed version, launch targets, playtime, notes, labels) plus a cache
//! of catalog fields so the library still works offline.
//!
//! The server stays the source of truth for *which* games are tracked
//! (bookmarks) and for update detection. This database only decorates them.

mod migrations;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::api::types::ThreadSummary;

pub type DbResult<T> = Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PlayStatus {
    #[default]
    NotStarted,
    Playing,
    OnHold,
    Finished,
    Dropped,
}

impl PlayStatus {
    fn as_str(self) -> &'static str {
        match self {
            PlayStatus::NotStarted => "not_started",
            PlayStatus::Playing => "playing",
            PlayStatus::OnHold => "on_hold",
            PlayStatus::Finished => "finished",
            PlayStatus::Dropped => "dropped",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "playing" => PlayStatus::Playing,
            "on_hold" => PlayStatus::OnHold,
            "finished" => PlayStatus::Finished,
            "dropped" => PlayStatus::Dropped,
            _ => PlayStatus::NotStarted,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LaunchTarget {
    pub id: i64,
    pub path: String,
    pub label: String,
    pub args: String,
    pub is_default: bool,
}

/// Catalog fields cached locally so the library renders offline.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct CachedGame {
    pub title: String,
    pub creator: String,
    pub version: String,
    pub cover: Option<String>,
    pub url: String,
    pub prefixes: Vec<String>,
    pub last_promoted: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LocalGame {
    pub thread_id: i64,
    pub in_library: bool,
    pub play_status: PlayStatus,
    pub rating: Option<u8>,
    pub notes: String,
    pub labels: Vec<String>,
    pub installed_version: Option<String>,
    pub finished_version: Option<String>,
    pub last_played_version: Option<String>,
    pub skipped_version: Option<String>,
    pub install_dir: Option<String>,
    pub added_at: i64,
    pub last_played_at: Option<i64>,
    pub playtime_secs: i64,
    pub targets: Vec<LaunchTarget>,
    pub cached: Option<CachedGame>,
}

/// Fields the user edits on the game page. `Some(None)`-style clearing is
/// done with empty strings, which are stored as NULL.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LocalPatch {
    pub play_status: Option<PlayStatus>,
    /// 0 clears the rating.
    pub rating: Option<u8>,
    pub notes: Option<String>,
    pub labels: Option<Vec<String>>,
    pub installed_version: Option<String>,
    pub finished_version: Option<String>,
    pub skipped_version: Option<String>,
    pub install_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PlaySession {
    pub started_at: i64,
    pub secs: i64,
    pub version: Option<String>,
}

pub struct Db {
    conn: Mutex<Connection>,
}

fn blank_to_null(v: String) -> Option<String> {
    let t = v.trim();
    (!t.is_empty()).then(|| t.to_string())
}

impl Db {
    /// Opens (or creates) `library.db` in `dir`, backing it up before any
    /// schema migration.
    pub fn open(dir: &Path) -> DbResult<Self> {
        std::fs::create_dir_all(dir).map_err(err)?;
        let path = dir.join("library.db");
        let conn = Connection::open(&path).map_err(err)?;
        Self::init(conn, Some(path))
    }

    #[cfg(test)]
    pub fn in_memory() -> Self {
        Self::init(Connection::open_in_memory().unwrap(), None).unwrap()
    }

    fn init(mut conn: Connection, path: Option<PathBuf>) -> DbResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL").map_err(err)?;
        conn.pragma_update(None, "foreign_keys", "ON").map_err(err)?;
        conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(err)?;
        migrations::run(&mut conn, path.as_deref())?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the lock leaves the connection usable.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    // ------------------------------------------------------------ catalog cache

    /// Caches catalog fields and makes sure a local row exists.
    pub fn cache_threads(&self, threads: &[ThreadSummary], in_library: bool) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        {
            let mut cache = tx
                .prepare(
                    "INSERT INTO games (thread_id, title, creator, version, cover, url, prefixes, last_promoted, cached_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                     ON CONFLICT(thread_id) DO UPDATE SET title=excluded.title, creator=excluded.creator,
                       version=excluded.version, cover=excluded.cover, url=excluded.url,
                       prefixes=excluded.prefixes, last_promoted=excluded.last_promoted, cached_at=excluded.cached_at",
                )
                .map_err(err)?;
            let mut user = tx
                .prepare(
                    "INSERT INTO user_games (thread_id, in_library, added_at) VALUES (?1, ?2, ?3)
                     ON CONFLICT(thread_id) DO UPDATE SET in_library = MAX(in_library, excluded.in_library)",
                )
                .map_err(err)?;
            let ts = now();
            for t in threads.iter().filter(|t| t.thread_id > 0) {
                let creator = if t.creator.is_empty() { &t.developer } else { &t.creator };
                cache
                    .execute(params![
                        t.thread_id,
                        t.title,
                        creator,
                        t.version,
                        t.cover,
                        t.url,
                        serde_json::to_string(&t.prefixes).unwrap_or_default(),
                        t.last_promoted,
                        ts
                    ])
                    .map_err(err)?;
                user.execute(params![t.thread_id, in_library, ts]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    /// After a full bookmark sync: exactly these ids are in the library.
    pub fn set_library_membership(&self, ids: &[i64]) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        tx.execute("UPDATE user_games SET in_library = 0", [])
            .map_err(err)?;
        {
            let mut up = tx
                .prepare(
                    "INSERT INTO user_games (thread_id, in_library, added_at) VALUES (?1, 1, ?2)
                     ON CONFLICT(thread_id) DO UPDATE SET in_library = 1",
                )
                .map_err(err)?;
            let ts = now();
            for id in ids {
                up.execute(params![id, ts]).map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    pub fn set_in_library(&self, id: i64, in_library: bool) -> DbResult<()> {
        self.conn()
            .execute(
                "INSERT INTO user_games (thread_id, in_library, added_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(thread_id) DO UPDATE SET in_library = excluded.in_library",
                params![id, in_library, now()],
            )
            .map(|_| ())
            .map_err(err)
    }

    // ---------------------------------------------------------------- reads

    fn targets_for(conn: &Connection, id: i64) -> DbResult<Vec<LaunchTarget>> {
        let mut st = conn
            .prepare_cached(
                "SELECT id, path, label, args, is_default FROM launch_targets
                 WHERE thread_id = ?1 ORDER BY is_default DESC, id",
            )
            .map_err(err)?;
        let rows = st
            .query_map([id], |r| {
                Ok(LaunchTarget {
                    id: r.get(0)?,
                    path: r.get(1)?,
                    label: r.get(2)?,
                    args: r.get(3)?,
                    is_default: r.get(4)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    const SELECT: &'static str = "SELECT u.thread_id, u.in_library, u.play_status, u.rating, u.notes, u.labels,
            u.installed_version, u.finished_version, u.last_played_version, u.skipped_version,
            u.install_dir, u.added_at, u.last_played_at, u.playtime_secs,
            g.title, g.creator, g.version, g.cover, g.url, g.prefixes, g.last_promoted
         FROM user_games u LEFT JOIN games g ON g.thread_id = u.thread_id";

    fn row(r: &rusqlite::Row) -> rusqlite::Result<LocalGame> {
        let title: Option<String> = r.get(14)?;
        let cached = match title {
            Some(title) => Some(CachedGame {
                title,
                creator: r.get::<_, Option<String>>(15)?.unwrap_or_default(),
                version: r.get::<_, Option<String>>(16)?.unwrap_or_default(),
                cover: r.get(17)?,
                url: r.get::<_, Option<String>>(18)?.unwrap_or_default(),
                prefixes: r
                    .get::<_, Option<String>>(19)?
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
                last_promoted: r.get(20)?,
            }),
            None => None,
        };
        Ok(LocalGame {
            thread_id: r.get(0)?,
            in_library: r.get(1)?,
            play_status: PlayStatus::parse(&r.get::<_, String>(2)?),
            rating: r.get(3)?,
            notes: r.get(4)?,
            labels: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
            installed_version: r.get(6)?,
            finished_version: r.get(7)?,
            last_played_version: r.get(8)?,
            skipped_version: r.get(9)?,
            install_dir: r.get(10)?,
            added_at: r.get(11)?,
            last_played_at: r.get(12)?,
            playtime_secs: r.get(13)?,
            targets: Vec::new(),
            cached,
        })
    }

    /// Every game with local state: library members plus anything installed.
    pub fn list(&self) -> DbResult<Vec<LocalGame>> {
        let conn = self.conn();
        let sql = format!(
            "{} WHERE u.in_library = 1 OR u.install_dir IS NOT NULL
               OR EXISTS (SELECT 1 FROM launch_targets t WHERE t.thread_id = u.thread_id)",
            Self::SELECT
        );
        let mut st = conn.prepare(&sql).map_err(err)?;
        let mut games: Vec<LocalGame> = st
            .query_map([], Self::row)
            .map_err(err)?
            .collect::<Result<_, _>>()
            .map_err(err)?;
        for g in &mut games {
            g.targets = Self::targets_for(&conn, g.thread_id)?;
        }
        Ok(games)
    }

    pub fn get(&self, id: i64) -> DbResult<Option<LocalGame>> {
        let conn = self.conn();
        let sql = format!("{} WHERE u.thread_id = ?1", Self::SELECT);
        let game = conn
            .query_row(&sql, [id], Self::row)
            .optional()
            .map_err(err)?;
        match game {
            Some(mut g) => {
                g.targets = Self::targets_for(&conn, id)?;
                Ok(Some(g))
            }
            None => Ok(None),
        }
    }

    fn ensure_row(conn: &Connection, id: i64) -> DbResult<()> {
        conn.execute(
            "INSERT OR IGNORE INTO user_games (thread_id, in_library, added_at) VALUES (?1, 0, ?2)",
            params![id, now()],
        )
        .map(|_| ())
        .map_err(err)
    }

    // --------------------------------------------------------------- writes

    pub fn update(&self, id: i64, p: LocalPatch) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        Self::ensure_row(&tx, id)?;
        if let Some(v) = p.play_status {
            tx.execute(
                "UPDATE user_games SET play_status = ?2 WHERE thread_id = ?1",
                params![id, v.as_str()],
            )
            .map_err(err)?;
        }
        if let Some(v) = p.rating {
            let v = (v > 0).then_some(v.min(5));
            tx.execute(
                "UPDATE user_games SET rating = ?2 WHERE thread_id = ?1",
                params![id, v],
            )
            .map_err(err)?;
        }
        if let Some(v) = p.notes {
            let v: String = v.chars().take(20_000).collect();
            tx.execute(
                "UPDATE user_games SET notes = ?2 WHERE thread_id = ?1",
                params![id, v],
            )
            .map_err(err)?;
        }
        if let Some(v) = p.labels {
            let mut labels: Vec<String> = v
                .into_iter()
                .map(|l| l.trim().chars().take(40).collect::<String>())
                .filter(|l| !l.is_empty())
                .collect();
            labels.sort_by_key(|l| l.to_lowercase());
            labels.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
            labels.truncate(20);
            tx.execute(
                "UPDATE user_games SET labels = ?2 WHERE thread_id = ?1",
                params![id, serde_json::to_string(&labels).unwrap_or_default()],
            )
            .map_err(err)?;
        }
        for (col, v) in [
            ("installed_version", p.installed_version),
            ("finished_version", p.finished_version),
            ("skipped_version", p.skipped_version),
            ("install_dir", p.install_dir),
        ] {
            if let Some(v) = v {
                tx.execute(
                    &format!("UPDATE user_games SET {col} = ?2 WHERE thread_id = ?1"),
                    params![id, blank_to_null(v)],
                )
                .map_err(err)?;
            }
        }
        tx.commit().map_err(err)
    }

    pub fn add_target(&self, id: i64, path: &str, label: &str, args: &str) -> DbResult<i64> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        Self::ensure_row(&tx, id)?;
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM launch_targets WHERE thread_id = ?1 AND path = ?2 COLLATE NOCASE",
                params![id, path],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(tid) = existing {
            return Ok(tid);
        }
        let has_default: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM launch_targets WHERE thread_id = ?1 AND is_default = 1)",
                [id],
                |r| r.get(0),
            )
            .map_err(err)?;
        tx.execute(
            "INSERT INTO launch_targets (thread_id, path, label, args, is_default) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, path, label, args, !has_default],
        )
        .map_err(err)?;
        let tid = tx.last_insert_rowid();
        tx.commit().map_err(err)?;
        Ok(tid)
    }

    /// Returns the game the target belonged to.
    pub fn remove_target(&self, target_id: i64) -> DbResult<Option<i64>> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        let row: Option<(i64, bool)> = tx
            .query_row(
                "SELECT thread_id, is_default FROM launch_targets WHERE id = ?1",
                [target_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(err)?;
        let Some((game, was_default)) = row else {
            return Ok(None);
        };
        tx.execute("DELETE FROM launch_targets WHERE id = ?1", [target_id])
            .map_err(err)?;
        if was_default {
            tx.execute(
                "UPDATE launch_targets SET is_default = 1 WHERE id =
                   (SELECT id FROM launch_targets WHERE thread_id = ?1 ORDER BY id LIMIT 1)",
                [game],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        Ok(Some(game))
    }

    pub fn set_default_target(&self, target_id: i64) -> DbResult<()> {
        self.conn()
            .execute(
                "UPDATE launch_targets SET is_default = (id = ?1)
                 WHERE thread_id = (SELECT thread_id FROM launch_targets WHERE id = ?1)",
                [target_id],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn target(&self, target_id: i64) -> DbResult<Option<(i64, LaunchTarget)>> {
        self.conn()
            .query_row(
                "SELECT thread_id, id, path, label, args, is_default FROM launch_targets WHERE id = ?1",
                [target_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        LaunchTarget {
                            id: r.get(1)?,
                            path: r.get(2)?,
                            label: r.get(3)?,
                            args: r.get(4)?,
                            is_default: r.get(5)?,
                        },
                    ))
                },
            )
            .optional()
            .map_err(err)
    }

    // ------------------------------------------------------------- playtime

    pub fn record_session(
        &self,
        id: i64,
        started_at: i64,
        secs: i64,
        version: Option<&str>,
    ) -> DbResult<()> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        Self::ensure_row(&tx, id)?;
        let ended = started_at + secs;
        tx.execute(
            "INSERT INTO play_sessions (thread_id, started_at, ended_at, secs, version) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, started_at, ended, secs, version],
        )
        .map_err(err)?;
        tx.execute(
            "UPDATE user_games SET playtime_secs = playtime_secs + ?2, last_played_at = ?3,
               last_played_version = COALESCE(?4, last_played_version),
               play_status = CASE WHEN play_status = 'not_started' THEN 'playing' ELSE play_status END
             WHERE thread_id = ?1",
            params![id, secs, ended, version],
        )
        .map_err(err)?;
        tx.commit().map_err(err)
    }

    pub fn sessions(&self, id: i64, limit: u32) -> DbResult<Vec<PlaySession>> {
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT started_at, secs, version FROM play_sessions WHERE thread_id = ?1
                 ORDER BY started_at DESC LIMIT ?2",
            )
            .map_err(err)?;
        let rows = st
            .query_map(params![id, limit], |r| {
                Ok(PlaySession {
                    started_at: r.get(0)?,
                    secs: r.get(1)?,
                    version: r.get(2)?,
                })
            })
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    /// Most recently played games that can still be launched, for the tray.
    pub fn recently_played(&self, limit: u32) -> DbResult<Vec<(i64, String)>> {
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT u.thread_id, COALESCE(g.title, 'Thread ' || u.thread_id) FROM user_games u
                 LEFT JOIN games g ON g.thread_id = u.thread_id
                 WHERE u.last_played_at IS NOT NULL
                   AND EXISTS (SELECT 1 FROM launch_targets t WHERE t.thread_id = u.thread_id)
                 ORDER BY u.last_played_at DESC LIMIT ?1",
            )
            .map_err(err)?;
        let rows = st
            .query_map([limit], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    // ------------------------------------------------------- notifications

    /// Returns the (id, version) pairs not notified before, and remembers them.
    pub fn take_unnotified(&self, items: &[(i64, String)]) -> DbResult<Vec<(i64, String)>> {
        let mut conn = self.conn();
        let tx = conn.transaction().map_err(err)?;
        let mut fresh = Vec::new();
        {
            let mut ins = tx
                .prepare(
                    "INSERT OR IGNORE INTO notified_updates (thread_id, version, notified_at) VALUES (?1, ?2, ?3)",
                )
                .map_err(err)?;
            let ts = now();
            for (id, v) in items {
                if ins.execute(params![id, v, ts]).map_err(err)? > 0 {
                    fresh.push((*id, v.clone()));
                }
            }
        }
        tx.commit().map_err(err)?;
        Ok(fresh)
    }

    /// Ids of games whose install folder is already known (for the scanner).
    pub fn linked_dirs(&self) -> DbResult<Vec<(i64, String)>> {
        let conn = self.conn();
        let mut st = conn
            .prepare(
                "SELECT thread_id, install_dir FROM user_games WHERE install_dir IS NOT NULL",
            )
            .map_err(err)?;
        let rows = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: i64, title: &str) -> ThreadSummary {
        ThreadSummary {
            thread_id: id,
            title: title.into(),
            creator: "Dev".into(),
            version: "v0.2".into(),
            ..Default::default()
        }
    }

    #[test]
    fn caches_and_tracks_membership() {
        let db = Db::in_memory();
        db.cache_threads(&[thread(1, "A"), thread(2, "B")], true)
            .unwrap();
        assert_eq!(db.list().unwrap().len(), 2);
        db.set_library_membership(&[2]).unwrap();
        let list = db.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].thread_id, 2);
        assert_eq!(list[0].cached.as_ref().unwrap().title, "B");
    }

    #[test]
    fn installed_games_stay_listed_after_leaving_library() {
        let db = Db::in_memory();
        db.cache_threads(&[thread(1, "A")], true).unwrap();
        db.add_target(1, "C:/Games/A/A.exe", "A", "").unwrap();
        db.set_library_membership(&[]).unwrap();
        let list = db.list().unwrap();
        assert_eq!(list.len(), 1);
        assert!(!list[0].in_library);
    }

    #[test]
    fn patch_validates_and_clears() {
        let db = Db::in_memory();
        db.update(
            5,
            LocalPatch {
                rating: Some(9),
                labels: Some(vec![" Fav ".into(), "fav".into(), "".into(), "Later".into()]),
                installed_version: Some("0.3".into()),
                play_status: Some(PlayStatus::Finished),
                ..Default::default()
            },
        )
        .unwrap();
        let g = db.get(5).unwrap().unwrap();
        assert_eq!(g.rating, Some(5));
        assert_eq!(g.labels, vec!["Fav", "Later"]);
        assert_eq!(g.installed_version.as_deref(), Some("0.3"));
        assert_eq!(g.play_status, PlayStatus::Finished);

        db.update(
            5,
            LocalPatch {
                rating: Some(0),
                installed_version: Some("  ".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let g = db.get(5).unwrap().unwrap();
        assert_eq!(g.rating, None);
        assert_eq!(g.installed_version, None);
    }

    #[test]
    fn targets_keep_one_default() {
        let db = Db::in_memory();
        let a = db.add_target(1, "C:/g/a.exe", "a", "").unwrap();
        let b = db.add_target(1, "C:/g/b.exe", "b", "").unwrap();
        assert_eq!(db.add_target(1, "c:/G/A.EXE", "dup", "").unwrap(), a);
        let t = db.get(1).unwrap().unwrap().targets;
        assert!(t[0].is_default && t[0].id == a && !t[1].is_default);
        db.set_default_target(b).unwrap();
        assert_eq!(db.get(1).unwrap().unwrap().targets[0].id, b);
        db.remove_target(b).unwrap();
        let t = db.get(1).unwrap().unwrap().targets;
        assert_eq!(t.len(), 1);
        assert!(t[0].is_default);
    }

    #[test]
    fn sessions_add_playtime() {
        let db = Db::in_memory();
        db.add_target(1, "C:/g/a.exe", "a", "").unwrap();
        db.record_session(1, 1000, 600, Some("0.2")).unwrap();
        db.record_session(1, 5000, 60, None).unwrap();
        let g = db.get(1).unwrap().unwrap();
        assert_eq!(g.playtime_secs, 660);
        assert_eq!(g.last_played_at, Some(5060));
        assert_eq!(g.last_played_version.as_deref(), Some("0.2"));
        assert_eq!(g.play_status, PlayStatus::Playing);
        assert_eq!(db.sessions(1, 10).unwrap().len(), 2);
        assert_eq!(db.recently_played(5).unwrap(), vec![(1, "Thread 1".into())]);
    }

    #[test]
    fn notifies_each_version_once() {
        let db = Db::in_memory();
        let first = db
            .take_unnotified(&[(1, "v2".into()), (2, "v5".into())])
            .unwrap();
        assert_eq!(first.len(), 2);
        let again = db
            .take_unnotified(&[(1, "v2".into()), (2, "v6".into())])
            .unwrap();
        assert_eq!(again, vec![(2, "v6".into())]);
    }
}
