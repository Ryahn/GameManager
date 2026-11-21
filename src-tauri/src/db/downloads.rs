//! Download jobs, the per-thread link cache and the rolling download log.

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Db, DbResult, err, now};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobSource {
    /// Fetched by the launcher from a host it understands.
    Direct,
    /// Downloaded by the F95 browser window after the user clicked through.
    Browser,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Downloading,
    Extracting,
    Installing,
    Done,
    Failed,
    /// Waiting on the user: a captcha, a host block, a missing folder.
    NeedsYou,
    Paused,
}

impl JobState {
    fn as_str(self) -> &'static str {
        match self {
            JobState::Queued => "queued",
            JobState::Downloading => "downloading",
            JobState::Extracting => "extracting",
            JobState::Installing => "installing",
            JobState::Done => "done",
            JobState::Failed => "failed",
            JobState::NeedsYou => "needs_you",
            JobState::Paused => "paused",
        }
    }
    fn parse(s: &str) -> Self {
        match s {
            "downloading" => JobState::Downloading,
            "extracting" => JobState::Extracting,
            "installing" => JobState::Installing,
            "done" => JobState::Done,
            "failed" => JobState::Failed,
            "needs_you" => JobState::NeedsYou,
            "paused" => JobState::Paused,
            _ => JobState::Queued,
        }
    }
    pub fn is_active(self) -> bool {
        matches!(
            self,
            JobState::Downloading | JobState::Extracting | JobState::Installing
        )
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DownloadJob {
    pub id: i64,
    pub thread_id: Option<i64>,
    pub title: String,
    pub creator: String,
    pub version: String,
    pub source: JobSource,
    pub host: String,
    /// The link the user picked (host page or direct file).
    pub page_url: String,
    pub file_name: Option<String>,
    /// The downloaded file, once there is one.
    pub path: Option<String>,
    pub state: JobState,
    pub bytes_done: i64,
    pub bytes_total: Option<i64>,
    pub error: Option<String>,
    pub attempts: i64,
    /// Unix time before which the job won't start (network retry backoff).
    pub retry_at: Option<i64>,
    pub install_dir: Option<String>,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    /// "games", "comics" or "animations": decides where it installs.
    pub category: String,
}

impl DownloadJob {
    pub fn new(source: JobSource, host: &str, page_url: &str) -> Self {
        Self {
            id: 0,
            thread_id: None,
            title: String::new(),
            creator: String::new(),
            version: String::new(),
            source,
            host: host.to_string(),
            page_url: page_url.to_string(),
            file_name: None,
            path: None,
            state: JobState::Queued,
            bytes_done: 0,
            bytes_total: None,
            error: None,
            attempts: 0,
            retry_at: None,
            install_dir: None,
            created_at: now(),
            finished_at: None,
            category: "games".into(),
        }
    }
}

const COLS: &str = "id, thread_id, title, creator, version, source, host, page_url, file_name, path,
    state, bytes_done, bytes_total, error, attempts, retry_at, install_dir, created_at, finished_at, category";

fn row(r: &rusqlite::Row) -> rusqlite::Result<DownloadJob> {
    Ok(DownloadJob {
        id: r.get(0)?,
        thread_id: r.get(1)?,
        title: r.get(2)?,
        creator: r.get(3)?,
        version: r.get(4)?,
        source: if r.get::<_, String>(5)? == "browser" {
            JobSource::Browser
        } else {
            JobSource::Direct
        },
        host: r.get(6)?,
        page_url: r.get(7)?,
        file_name: r.get(8)?,
        path: r.get(9)?,
        state: JobState::parse(&r.get::<_, String>(10)?),
        bytes_done: r.get(11)?,
        bytes_total: r.get(12)?,
        error: r.get(13)?,
        attempts: r.get(14)?,
        retry_at: r.get(15)?,
        install_dir: r.get(16)?,
        created_at: r.get(17)?,
        finished_at: r.get(18)?,
        category: r.get(19)?,
    })
}

impl Db {
    pub fn insert_job(&self, j: &DownloadJob) -> DbResult<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO download_jobs (thread_id, title, creator, version, source, host, page_url, file_name, path,
               state, bytes_done, bytes_total, error, attempts, retry_at, install_dir, created_at, finished_at, category)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
            params![
                j.thread_id,
                j.title,
                j.creator,
                j.version,
                if j.source == JobSource::Browser { "browser" } else { "direct" },
                j.host,
                j.page_url,
                j.file_name,
                j.path,
                j.state.as_str(),
                j.bytes_done,
                j.bytes_total,
                j.error,
                j.attempts,
                j.retry_at,
                j.install_dir,
                j.created_at,
                j.finished_at,
                j.category
            ],
        )
        .map_err(err)?;
        Ok(conn.last_insert_rowid())
    }

    pub fn save_job(&self, j: &DownloadJob) -> DbResult<()> {
        self.conn()
            .execute(
                "UPDATE download_jobs SET file_name = ?2, path = ?3, state = ?4, bytes_done = ?5, bytes_total = ?6,
                   error = ?7, attempts = ?8, retry_at = ?9, install_dir = ?10, finished_at = ?11, host = ?12
                 WHERE id = ?1",
                params![
                    j.id,
                    j.file_name,
                    j.path,
                    j.state.as_str(),
                    j.bytes_done,
                    j.bytes_total,
                    j.error,
                    j.attempts,
                    j.retry_at,
                    j.install_dir,
                    j.finished_at,
                    j.host
                ],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn jobs(&self) -> DbResult<Vec<DownloadJob>> {
        let conn = self.conn();
        let mut st = conn
            .prepare(&format!("SELECT {COLS} FROM download_jobs ORDER BY id"))
            .map_err(err)?;
        let rows = st.query_map([], row).map_err(err)?;
        rows.collect::<Result<_, _>>().map_err(err)
    }

    pub fn delete_job(&self, id: i64) -> DbResult<()> {
        self.conn()
            .execute("DELETE FROM download_jobs WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(err)
    }

    /// Records one download start for the rolling 24-hour count.
    pub fn log_start(&self, at: i64) -> DbResult<()> {
        let conn = self.conn();
        conn.execute("INSERT INTO download_log (started_at) VALUES (?1)", [at])
            .map_err(err)?;
        // Keep the table tiny.
        conn.execute(
            "DELETE FROM download_log WHERE started_at < ?1",
            [at - 7 * 86_400],
        )
        .map(|_| ())
        .map_err(err)
    }

    pub fn starts_since(&self, since: i64) -> DbResult<usize> {
        self.conn()
            .query_row(
                "SELECT COUNT(*) FROM download_log WHERE started_at >= ?1",
                [since],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(err)
    }

    // ------------------------------------------------------------ link cache

    pub fn cached_links(&self, thread_id: i64) -> DbResult<Option<(String, i64, String)>> {
        self.conn()
            .query_row(
                "SELECT version, fetched_at, groups FROM thread_links WHERE thread_id = ?1",
                [thread_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(err)
    }

    pub fn cache_links(&self, thread_id: i64, version: &str, groups_json: &str) -> DbResult<()> {
        self.conn()
            .execute(
                "INSERT INTO thread_links (thread_id, version, fetched_at, groups) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(thread_id) DO UPDATE SET version = excluded.version,
                   fetched_at = excluded.fetched_at, groups = excluded.groups",
                params![thread_id, version, now(), groups_json],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// Launch targets that point into `dir` (an old version's folder).
    pub fn remove_targets_under(&self, thread_id: i64, dir: &str) -> DbResult<()> {
        let conn = self.conn();
        let ids: Vec<(i64, String)> = {
            let mut st = conn
                .prepare("SELECT id, path FROM launch_targets WHERE thread_id = ?1")
                .map_err(err)?;
            st.query_map([thread_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(err)?
                .collect::<Result<_, _>>()
                .map_err(err)?
        };
        for (id, path) in ids {
            if crate::platform::is_under(&path, dir) {
                conn.execute("DELETE FROM launch_targets WHERE id = ?1", [id])
                    .map_err(err)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jobs_round_trip_and_log_counts() {
        let db = Db::in_memory();
        let mut j = DownloadJob::new(JobSource::Direct, "pixeldrain.com", "https://pixeldrain.com/u/x");
        j.thread_id = Some(7);
        j.id = db.insert_job(&j).unwrap();
        j.state = JobState::Downloading;
        j.bytes_done = 42;
        db.save_job(&j).unwrap();
        assert_eq!(db.jobs().unwrap(), vec![j.clone()]);
        db.delete_job(j.id).unwrap();
        assert!(db.jobs().unwrap().is_empty());

        db.log_start(1_000_000).unwrap();
        db.log_start(1_050_000).unwrap();
        assert_eq!(db.starts_since(1_000_001).unwrap(), 1);
    }

    #[test]
    fn link_cache_upserts() {
        let db = Db::in_memory();
        db.cache_links(5, "v1", "[]").unwrap();
        db.cache_links(5, "v2", "[1]").unwrap();
        let (v, _, g) = db.cached_links(5).unwrap().unwrap();
        assert_eq!((v.as_str(), g.as_str()), ("v2", "[1]"));
    }

    #[test]
    fn removes_only_targets_in_old_folder() {
        let db = Db::in_memory();
        // Paths in this OS's own form (Windows also accepts forward slashes).
        let (old, new, dir) = if cfg!(windows) {
            ("D:\\Games\\X\\0.1\\X.exe", "D:\\Games\\X\\0.10\\X.exe", "D:/Games/X/0.1")
        } else {
            ("/games/X/0.1/X.sh", "/games/X/0.10/X.sh", "/games/X/0.1/")
        };
        db.add_target(1, old, "x", "").unwrap();
        db.add_target(1, new, "x", "").unwrap();
        db.remove_targets_under(1, dir).unwrap();
        let t = db.get(1).unwrap().unwrap().targets;
        assert_eq!(t.len(), 1);
        assert!(t[0].path.contains("0.10"));
    }
}
