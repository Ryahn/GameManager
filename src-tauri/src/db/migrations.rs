//! Schema migrations, tracked with `PRAGMA user_version`. The database file is
//! copied aside before any migration runs; the newest three copies are kept.

use std::path::Path;

use rusqlite::Connection;

const MIGRATIONS: &[&str] = &[
    // v1: local library
    "CREATE TABLE games (
        thread_id     INTEGER PRIMARY KEY,
        title         TEXT NOT NULL DEFAULT '',
        creator       TEXT NOT NULL DEFAULT '',
        version       TEXT NOT NULL DEFAULT '',
        cover         TEXT,
        url           TEXT NOT NULL DEFAULT '',
        prefixes      TEXT NOT NULL DEFAULT '[]',
        last_promoted INTEGER,
        cached_at     INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE user_games (
        thread_id           INTEGER PRIMARY KEY,
        in_library          INTEGER NOT NULL DEFAULT 0,
        play_status         TEXT NOT NULL DEFAULT 'not_started',
        rating              INTEGER,
        notes               TEXT NOT NULL DEFAULT '',
        labels              TEXT NOT NULL DEFAULT '[]',
        installed_version   TEXT,
        finished_version    TEXT,
        last_played_version TEXT,
        skipped_version     TEXT,
        install_dir         TEXT,
        added_at            INTEGER NOT NULL DEFAULT 0,
        last_played_at      INTEGER,
        playtime_secs       INTEGER NOT NULL DEFAULT 0
     );
     CREATE TABLE launch_targets (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        thread_id  INTEGER NOT NULL REFERENCES user_games(thread_id) ON DELETE CASCADE,
        path       TEXT NOT NULL,
        label      TEXT NOT NULL DEFAULT '',
        args       TEXT NOT NULL DEFAULT '',
        is_default INTEGER NOT NULL DEFAULT 0
     );
     CREATE INDEX launch_targets_game ON launch_targets(thread_id);
     CREATE TABLE play_sessions (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        thread_id  INTEGER NOT NULL REFERENCES user_games(thread_id) ON DELETE CASCADE,
        started_at INTEGER NOT NULL,
        ended_at   INTEGER NOT NULL,
        secs       INTEGER NOT NULL,
        version    TEXT
     );
     CREATE INDEX play_sessions_game ON play_sessions(thread_id, started_at);
     CREATE TABLE notified_updates (
        thread_id   INTEGER NOT NULL,
        version     TEXT NOT NULL,
        notified_at INTEGER NOT NULL,
        PRIMARY KEY (thread_id, version)
     );",
    // v2: downloads
    "CREATE TABLE thread_links (
        thread_id  INTEGER PRIMARY KEY,
        version    TEXT NOT NULL,
        fetched_at INTEGER NOT NULL,
        groups     TEXT NOT NULL
     );
     CREATE TABLE download_jobs (
        id          INTEGER PRIMARY KEY AUTOINCREMENT,
        thread_id   INTEGER,
        title       TEXT NOT NULL DEFAULT '',
        creator     TEXT NOT NULL DEFAULT '',
        version     TEXT NOT NULL DEFAULT '',
        source      TEXT NOT NULL,
        host        TEXT NOT NULL DEFAULT '',
        page_url    TEXT NOT NULL DEFAULT '',
        file_name   TEXT,
        path        TEXT,
        state       TEXT NOT NULL,
        bytes_done  INTEGER NOT NULL DEFAULT 0,
        bytes_total INTEGER,
        error       TEXT,
        attempts    INTEGER NOT NULL DEFAULT 0,
        retry_at    INTEGER,
        install_dir TEXT,
        created_at  INTEGER NOT NULL,
        finished_at INTEGER
     );
     CREATE TABLE download_log (started_at INTEGER NOT NULL);
     CREATE INDEX download_log_time ON download_log(started_at);",
    // v3: comics and animations
    "ALTER TABLE games ADD COLUMN category TEXT NOT NULL DEFAULT 'games';
     ALTER TABLE download_jobs ADD COLUMN category TEXT NOT NULL DEFAULT 'games';",
];

pub fn run(conn: &mut Connection, path: Option<&Path>) -> Result<(), String> {
    let current: usize = conn
        .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())? as usize;
    if current >= MIGRATIONS.len() {
        return Ok(());
    }
    if current > 0
        && let Some(p) = path {
            backup(conn, p, current)?;
        }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current) {
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch(sql)
            .map_err(|e| format!("migration {} failed: {e}", i + 1))?;
        tx.pragma_update(None, "user_version", (i + 1) as i64)
            .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn backup(conn: &Connection, path: &Path, version: usize) -> Result<(), String> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let stamp = crate::db::now();
    let dest = dir.join(format!("library.v{version}.{stamp}.bak.db"));
    // VACUUM INTO writes a consistent copy even with WAL pages pending.
    conn.execute("VACUUM INTO ?1", [dest.to_string_lossy()])
        .map_err(|e| format!("backup before migration failed: {e}"))?;

    let mut backups: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("library.v") && n.ends_with(".bak.db"))
        })
        .collect();
    backups.sort_by_key(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    while backups.len() > 3 {
        let _ = std::fs::remove_file(backups.remove(0));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_once() {
        let mut c = Connection::open_in_memory().unwrap();
        run(&mut c, None).unwrap();
        run(&mut c, None).unwrap();
        let v: i64 = c
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
    }
}
