//! The download queue.
//!
//! One scheduler loop decides what may start, using the limits in
//! `limits.rs`: a few downloads at a time, one per host, 30-60 s between
//! starts on the same host, long cooldowns when a host pushes back, nothing
//! new while a game is running. Every state change is saved, so the queue
//! survives restarts and partial files resume.

pub mod hosts;
pub mod limits;
pub mod mega;
pub mod transfer;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Notify;

use crate::commands::AppState;
use crate::db::{self, Db, DownloadJob, JobSource, JobState};
use crate::f95::GameCtx;
use crate::launch::Launcher;
use limits::HostGates;
use transfer::TransferError;

#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    #[serde(flatten)]
    pub job: DownloadJob,
    /// For queued jobs: unix time the job may start (host pacing/backoff).
    pub starts_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub jobs: Vec<JobView>,
    pub daily: usize,
    pub daily_limit: usize,
    pub paused_for_game: bool,
    pub max_active: usize,
}

pub struct Downloads {
    jobs: Mutex<Vec<DownloadJob>>,
    gates: Mutex<HostGates>,
    stops: Mutex<HashMap<i64, Arc<AtomicBool>>>,
    wake: Notify,
    http: reqwest::Client,
    paused_for_game: AtomicBool,
    /// The games folder, refreshed from settings every tick (so sync code
    /// such as the browser's download hook can read it).
    library: Mutex<Option<PathBuf>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p)
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter_map(|e| e.metadata().ok())
                .filter(|m| m.is_file())
                .map(|m| m.len())
                .sum()
        })
        .unwrap_or(0)
}

fn time_hm(unix: i64) -> String {
    let mins = (unix - db::now()).max(0) / 60;
    if mins < 1 {
        "a minute".into()
    } else if mins < 60 {
        format!("{mins} minutes")
    } else {
        format!("{} h {} min", mins / 60, mins % 60)
    }
}

impl Downloads {
    pub fn load(db: &Db) -> Self {
        let mut jobs = db.jobs().unwrap_or_default();
        // The app closed mid-way: direct downloads resume from their .part
        // file; browser downloads can't be picked up again.
        for j in &mut jobs {
            if j.state.is_active() {
                if j.source == JobSource::Browser && j.state == JobState::Downloading {
                    j.state = JobState::Failed;
                    j.error = Some("The app closed during this download. Start it again from the game page.".into());
                } else {
                    j.state = JobState::Queued;
                }
                let _ = db.save_job(j);
            }
        }
        let http = reqwest::Client::builder()
            .user_agent(crate::config::USER_AGENT)
            .connect_timeout(Duration::from_secs(20))
            .build()
            .expect("http client");
        Self {
            jobs: Mutex::new(jobs),
            gates: Mutex::new(HostGates::default()),
            stops: Mutex::new(HashMap::new()),
            wake: Notify::new(),
            http,
            paused_for_game: AtomicBool::new(false),
            library: Mutex::new(None),
        }
    }

    pub fn set_library(&self, dir: Option<String>) {
        *lock(&self.library) = dir.map(PathBuf::from).filter(|p| p.is_dir());
    }

    pub fn library(&self) -> Option<PathBuf> {
        lock(&self.library).clone()
    }

    pub fn snapshot(&self, db: &Db, max_active: usize) -> Snapshot {
        let gates = lock(&self.gates);
        let jobs = lock(&self.jobs)
            .iter()
            .rev()
            .map(|j| {
                let starts_at = (j.state == JobState::Queued).then(|| {
                    let gate = gates.check(&j.host, db::now() as u64).err().unwrap_or(0) as i64;
                    gate.max(j.retry_at.unwrap_or(0))
                });
                JobView {
                    job: j.clone(),
                    starts_at: starts_at.filter(|t| *t > db::now()),
                }
            })
            .collect();
        Snapshot {
            jobs,
            daily: db.starts_since(db::now() - 86_400).unwrap_or(0),
            daily_limit: limits::DAILY_SOFT_LIMIT,
            paused_for_game: self.paused_for_game.load(Ordering::SeqCst),
            max_active,
        }
    }

    fn emit(&self, app: &AppHandle) {
        let _ = app.emit("downloads-changed", ());
    }

    pub fn job(&self, id: i64) -> Option<DownloadJob> {
        self.get(id)
    }

    fn get(&self, id: i64) -> Option<DownloadJob> {
        lock(&self.jobs).iter().find(|j| j.id == id).cloned()
    }

    /// Changes a job, saves it and tells the UI.
    fn update(&self, app: &AppHandle, id: i64, f: impl FnOnce(&mut DownloadJob)) -> Option<DownloadJob> {
        let job = {
            let mut jobs = lock(&self.jobs);
            let j = jobs.iter_mut().find(|j| j.id == id)?;
            f(j);
            j.clone()
        };
        let _ = app.state::<Arc<Db>>().save_job(&job);
        self.emit(app);
        Some(job)
    }

    /// Jobs waiting to start, per host (for the mass-download warning).
    pub fn queued_per_host(&self) -> HashMap<String, usize> {
        let mut m = HashMap::new();
        for j in lock(&self.jobs).iter().filter(|j| j.state == JobState::Queued) {
            *m.entry(j.host.clone()).or_default() += 1;
        }
        m
    }

    /// Queues a download the launcher can fetch itself.
    pub fn enqueue(&self, app: &AppHandle, ctx: &GameCtx, page_url: &str, host: &str) -> Result<i64, String> {
        {
            let jobs = lock(&self.jobs);
            let dup = jobs.iter().any(|j| {
                j.page_url == page_url && !matches!(j.state, JobState::Done | JobState::Failed)
            });
            if dup {
                return Err("This download is already in the queue.".into());
            }
        }
        let mut job = DownloadJob::new(JobSource::Direct, host, page_url);
        job.thread_id = Some(ctx.thread_id);
        job.title = ctx.title.clone();
        job.creator = ctx.creator.clone();
        job.version = ctx.version.clone();
        let db = app.state::<Arc<Db>>();
        job.id = db.insert_job(&job)?;
        let id = job.id;
        lock(&self.jobs).push(job);
        self.emit(app);
        self.wake.notify_one();
        Ok(id)
    }

    /// The F95 window started a download: track it and choose where it goes.
    pub fn add_browser_job(
        &self,
        app: &AppHandle,
        ctx: Option<GameCtx>,
        url: &str,
        name: Option<String>,
    ) -> Option<PathBuf> {
        let library = self.library()?;
        let db = app.state::<Arc<Db>>();
        let host = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.trim_start_matches("www.").to_string()))
            .unwrap_or_default();
        let mut job = DownloadJob::new(JobSource::Browser, &host, url);
        if let Some(c) = &ctx {
            job.thread_id = Some(c.thread_id);
            job.title = c.title.clone();
            job.creator = c.creator.clone();
            job.version = c.version.clone();
        } else {
            job.title = name.clone().unwrap_or_else(|| "Download".into());
        }
        job.state = JobState::Downloading;
        let file = transfer::safe_file_name(name.as_deref().unwrap_or("download"));
        job.file_name = Some(file.clone());
        job.id = db.insert_job(&job).ok()?;
        let dest = downloads_dir(&library, job.id).join(&file);
        let _ = std::fs::create_dir_all(dest.parent()?);
        job.path = Some(dest.to_string_lossy().into_owned());
        let _ = db.save_job(&job);
        let _ = db.log_start(db::now());
        lock(&self.jobs).push(job);
        self.emit(app);
        Some(dest)
    }

    pub fn browser_finished(&self, app: &AppHandle, url: &str, path: Option<PathBuf>, success: bool) {
        let id = lock(&self.jobs)
            .iter()
            .rev()
            .find(|j| j.source == JobSource::Browser && j.page_url == url && j.state == JobState::Downloading)
            .map(|j| j.id);
        let Some(id) = id else { return };
        if !success {
            self.update(app, id, |j| {
                j.state = JobState::Failed;
                j.error = Some("The download didn't finish in the F95 window.".into());
            });
            return;
        }
        let job = self.update(app, id, |j| {
            if let Some(p) = &path {
                j.path = Some(p.to_string_lossy().into_owned());
            }
            if let Some(p) = j.path.as_ref().map(PathBuf::from) {
                let size = std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(j.bytes_done);
                j.bytes_done = size;
                j.bytes_total = Some(size);
            }
        });
        if job.is_some() {
            let app = app.clone();
            tauri::async_runtime::spawn(async move { install_job(&app, id).await });
        }
    }

    // ---------------------------------------------------------- user actions

    fn stop(&self, id: i64) {
        if let Some(s) = lock(&self.stops).get(&id) {
            s.store(true, Ordering::SeqCst);
        }
    }

    pub fn pause(&self, app: &AppHandle, id: i64) {
        self.update(app, id, |j| {
            if matches!(j.state, JobState::Queued | JobState::Downloading) && j.source == JobSource::Direct {
                j.state = JobState::Paused;
            }
        });
        self.stop(id);
    }

    pub fn resume(&self, app: &AppHandle, id: i64) {
        self.update(app, id, |j| {
            if matches!(j.state, JobState::Paused | JobState::NeedsYou | JobState::Failed) {
                j.state = JobState::Queued;
                j.error = None;
                j.retry_at = None;
                j.attempts = 0;
            }
        });
        self.wake.notify_one();
    }

    /// Removes a job and its partial files. Installed games are untouched.
    pub fn remove(&self, app: &AppHandle, id: i64) {
        self.stop(id);
        let job = {
            let mut jobs = lock(&self.jobs);
            let pos = jobs.iter().position(|j| j.id == id);
            pos.map(|p| jobs.remove(p))
        };
        if let Some(job) = job {
            if let Some(lib) = self.library()
                && (job.state != JobState::Done || job.install_dir.is_some()) {
                    let _ = std::fs::remove_dir_all(downloads_dir(&lib, job.id));
                }
            let _ = app.state::<Arc<Db>>().delete_job(id);
        }
        self.emit(app);
    }

    pub fn clear_finished(&self, app: &AppHandle) {
        let ids: Vec<i64> = lock(&self.jobs)
            .iter()
            .filter(|j| j.state == JobState::Done)
            .map(|j| j.id)
            .collect();
        let db = app.state::<Arc<Db>>();
        lock(&self.jobs).retain(|j| j.state != JobState::Done);
        for id in ids {
            let _ = db.delete_job(id);
        }
        self.emit(app);
    }

    pub fn path_of(&self, id: i64) -> Option<String> {
        self.get(id).and_then(|j| j.install_dir.or(j.path))
    }
}

fn downloads_dir(library: &Path, id: i64) -> PathBuf {
    library.join(".downloads").join(id.to_string())
}

// ----------------------------------------------------------------- scheduler

pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let dl = app.state::<Arc<Downloads>>().inner().clone();
        loop {
            tick(&app, &dl).await;
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                _ = dl.wake.notified() => {}
            }
        }
    });
}

async fn tick(app: &AppHandle, dl: &Arc<Downloads>) {
    let settings = app.state::<AppState>().settings.get().await;
    dl.set_library(settings.library_dir.clone());
    let max_active = limits::clamp_active(settings.max_active_downloads as usize);
    let game_running = !app.state::<Arc<Launcher>>().running_ids().is_empty();
    let was = dl.paused_for_game.swap(game_running, Ordering::SeqCst);
    if was != game_running {
        dl.emit(app);
    }

    // Browser downloads: progress is the file growing on disk.
    let browsing: Vec<(i64, PathBuf)> = lock(&dl.jobs)
        .iter()
        .filter(|j| j.source == JobSource::Browser && j.state == JobState::Downloading)
        .filter_map(|j| j.path.as_ref().map(|p| (j.id, PathBuf::from(p))))
        .collect();
    for (id, p) in browsing {
        let size = p.parent().map(dir_size).unwrap_or(0) as i64;
        let changed = lock(&dl.jobs).iter().any(|j| j.id == id && j.bytes_done != size);
        if changed {
            if let Some(j) = lock(&dl.jobs).iter_mut().find(|j| j.id == id) {
                j.bytes_done = size;
            }
            dl.emit(app);
        }
    }

    // A game started: stop direct downloads; they resume afterwards.
    if game_running {
        let active: Vec<i64> = lock(&dl.jobs)
            .iter()
            .filter(|j| j.source == JobSource::Direct && j.state == JobState::Downloading)
            .map(|j| j.id)
            .collect();
        for id in active {
            dl.update(app, id, |j| j.state = JobState::Queued);
            dl.stop(id);
        }
        return;
    }

    let now = db::now();
    let mut to_start = Vec::new();
    let mut to_install = Vec::new();
    {
        let jobs = lock(&dl.jobs);
        let mut active = jobs
            .iter()
            .filter(|j| j.state == JobState::Downloading && j.source == JobSource::Direct)
            .count();
        let mut busy_hosts: HashSet<String> = jobs
            .iter()
            .filter(|j| j.state == JobState::Downloading)
            .map(|j| j.host.clone())
            .collect();
        let mut gates = lock(&dl.gates);
        for j in jobs.iter().filter(|j| j.state == JobState::Queued) {
            if j.retry_at.is_some_and(|t| t > now) {
                continue;
            }
            // Already downloaded (the app closed while installing).
            if j.path.as_ref().is_some_and(|p| Path::new(p).is_file()) {
                to_install.push(j.id);
                continue;
            }
            if j.source != JobSource::Direct || active >= max_active {
                continue;
            }
            if busy_hosts.len() >= max_active || busy_hosts.contains(&j.host) {
                continue;
            }
            if gates.check(&j.host, now as u64).is_err() {
                continue;
            }
            gates.started(&j.host, now as u64);
            busy_hosts.insert(j.host.clone());
            active += 1;
            to_start.push((j.id, j.attempts == 0 && j.bytes_done == 0));
        }
    }
    for id in to_install {
        dl.update(app, id, |j| j.state = JobState::Extracting);
        let app = app.clone();
        tauri::async_runtime::spawn(async move { install_job(&app, id).await });
    }
    for (id, first) in to_start {
        if first {
            let _ = app.state::<Arc<Db>>().log_start(now);
        }
        dl.update(app, id, |j| {
            j.state = JobState::Downloading;
            j.error = None;
        });
        let app = app.clone();
        let dl = dl.clone();
        tauri::async_runtime::spawn(async move { run_direct(&app, &dl, id).await });
    }
}

async fn run_direct(app: &AppHandle, dl: &Arc<Downloads>, id: i64) {
    let Some(job) = dl.get(id) else { return };
    let Some(library) = dl.library() else {
        dl.update(app, id, |j| {
            j.state = JobState::NeedsYou;
            j.error = Some("Choose your games folder in Settings, then retry.".into());
        });
        return;
    };
    let stop = Arc::new(AtomicBool::new(false));
    lock(&dl.stops).insert(id, stop.clone());

    let host_label = match job.host.as_str() {
        "pixeldrain.com" => "Pixeldrain".to_string(),
        "mega.nz" => "MEGA".to_string(),
        h => h.to_string(),
    };
    // Read fresh each time so a key added in Settings applies right away.
    let creds = tauri::async_runtime::spawn_blocking(hosts::HostCreds::load)
        .await
        .unwrap_or_default();
    let progress = {
        let app = app.clone();
        let dl = dl.clone();
        move |p: transfer::Progress| {
            if let Some(j) = lock(&dl.jobs).iter_mut().find(|j| j.id == id) {
                j.bytes_done = p.done as i64;
                j.bytes_total = p.total.map(|t| t as i64).or(j.bytes_total);
            }
            dl.emit(&app);
        }
    };
    let result = async {
        let dir = downloads_dir(&library, id);
        match hosts::resolve(&dl.http, &job.page_url, &creds).await? {
            hosts::Resolved::NeedsBrowser(page) => Ok(Err(page)),
            hosts::Resolved::Mega { url } => {
                let files = mega::download(&dl.http, &url, creds.mega_session.as_deref(), &dir, stop.clone(), progress).await?;
                Ok(Ok(files.into_iter().next().expect("at least one file")))
            }
            hosts::Resolved::Files { files, auth } => {
                let total: Option<u64> = files.iter().map(|f| f.size).sum();
                let single = files.len() == 1;
                dl.update(app, id, |j| {
                    if j.file_name.is_none() {
                        j.file_name = files[0].name.clone();
                    }
                    j.bytes_total = total.map(|s| s as i64);
                });
                // Several files (a multi-part archive in a folder): fetch
                // them one after another and report combined progress.
                let mut paths = Vec::with_capacity(files.len());
                let mut before = 0u64;
                for f in &files {
                    let offset = before;
                    let path = transfer::download(
                        &dl.http,
                        &f.url,
                        &dir,
                        f.name.as_deref().or(if single { job.file_name.as_deref() } else { None }),
                        f.size,
                        &host_label,
                        &auth,
                        stop.clone(),
                        |p| {
                            progress(transfer::Progress {
                                done: offset + p.done,
                                total: total.or(p.total.map(|t| t + offset)),
                            })
                        },
                    )
                    .await?;
                    before += f.size.unwrap_or_else(|| std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0));
                    paths.push(path);
                }
                Ok(Ok(first_archive(paths)))
            }
        }
    }
    .await;
    lock(&dl.stops).remove(&id);

    match result {
        Ok(Ok(path)) => {
            lock(&dl.gates).succeeded(&job.host);
            dl.update(app, id, |j| {
                j.path = Some(path.to_string_lossy().into_owned());
                j.file_name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                j.state = JobState::Extracting;
            });
            install_job(app, id).await;
        }
        Ok(Err(page)) => {
            dl.update(app, id, |j| {
                j.state = JobState::NeedsYou;
                j.page_url = page;
                j.error = Some(format!(
                    "{host_label} wants you to confirm this download yourself. Open it in the F95 window."
                ));
            });
        }
        Err(TransferError::HostLimited(h)) => {
            let until = {
                let mut g = lock(&dl.gates);
                g.penalize(&job.host, db::now() as u64);
                g.cooldown_until(&job.host).unwrap_or(0) as i64
            };
            dl.update(app, id, |j| {
                j.state = JobState::NeedsYou;
                j.retry_at = Some(until);
                j.error = Some(format!(
                    "{h} asked us to slow down. Downloads from it are paused for {} to protect your IP. Retry after that, or open it in the F95 window.",
                    time_hm(until)
                ));
            });
        }
        Err(TransferError::Network(m)) => {
            dl.update(app, id, |j| {
                j.attempts += 1;
                match limits::RETRY_BACKOFF.get(j.attempts as usize - 1) {
                    Some(secs) => {
                        j.state = JobState::Queued;
                        j.retry_at = Some(db::now() + *secs as i64);
                        j.error = Some(format!("Connection problem ({m}). Retrying automatically."));
                    }
                    None => {
                        j.state = JobState::Failed;
                        j.error = Some(format!("Connection problem: {m}"));
                    }
                }
            });
        }
        Err(TransferError::Fatal(m)) => {
            dl.update(app, id, |j| {
                j.state = JobState::Failed;
                j.error = Some(m);
            });
        }
        Err(TransferError::NeedsBrowser) => {
            dl.update(app, id, |j| {
                j.state = JobState::NeedsYou;
                j.error = Some(format!(
                    "{host_label} sent its download page instead of the file. Open it in the F95 window and press its download button."
                ));
            });
        }
        // Paused by the user or by a game starting: state already set.
        Err(TransferError::Stopped) => {}
    }
}

/// The file to install from: the first part of a multi-part set, else the
/// first archive, else the first file.
fn first_archive(mut paths: Vec<PathBuf>) -> PathBuf {
    paths.sort_by_key(|p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let first_part = n.ends_with(".part1.rar") || n.ends_with(".part01.rar") || n.ends_with(".7z.001") || n.ends_with(".zip.001");
        (!first_part, crate::install::archive_kind(p).is_none(), n)
    });
    paths.remove(0)
}

/// Extracts and installs a finished download, carrying saves forward.
async fn install_job(app: &AppHandle, id: i64) {
    let dl = app.state::<Arc<Downloads>>().inner().clone();
    let Some(job) = dl.get(id) else { return };
    let Some(library) = dl.library() else {
        dl.update(app, id, |j| {
            j.state = JobState::NeedsYou;
            j.error = Some("Choose your games folder in Settings, then retry.".into());
        });
        return;
    };
    let Some(archive) = job.path.clone().map(PathBuf::from) else { return };
    let settings = app.state::<AppState>().settings.get().await;
    let db = app.state::<Arc<Db>>().inner().clone();

    if crate::install::archive_kind(&archive).is_none() {
        dl.update(app, id, |j| {
            j.state = JobState::Done;
            j.finished_at = Some(db::now());
            j.error = Some("Saved, but not installed: it isn't a zip, 7z or rar file. Open the folder to use it.".into());
        });
        return;
    }

    dl.update(app, id, |j| j.state = JobState::Extracting);
    let staging = library.join(".staging").join(id.to_string());
    let job2 = job.clone();
    let lib2 = library.clone();
    let db2 = db.clone();
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(PathBuf, Vec<String>), String> {
        let _ = std::fs::remove_dir_all(&staging);
        let n = crate::install::extract(&archive, &staging)?;
        if n == 0 {
            return Err("The archive was empty.".into());
        }
        let root = crate::install::game_root(&staging);
        let dest = match job2.thread_id {
            Some(_) => crate::install::destination(
                &lib2,
                if job2.creator.is_empty() { "Unknown creator" } else { &job2.creator },
                &job2.title,
                &job2.version,
            ),
            None => crate::install::destination(&lib2, "Unsorted", &job2.title, "download"),
        };
        crate::install::move_dir(&root, &dest).map_err(|e| format!("Couldn't move the game into place: {e}"))?;
        let _ = std::fs::remove_dir_all(&staging);

        let mut saves = Vec::new();
        if let Some(tid) = job2.thread_id {
            let old = db2.get(tid)?.and_then(|g| g.install_dir);
            if let Some(old) = old.filter(|o| !crate::library::same_path(o, &dest.to_string_lossy())) {
                saves = crate::install::carry_saves(Path::new(&old), &dest);
                db2.remove_targets_under(tid, &old)?;
            }
            crate::local_commands::link_folder(&db2, tid, &dest.to_string_lossy(), Some(job2.version.clone()))?;
            db2.set_in_library(tid, true)?;
        }
        Ok((dest, saves))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);

    match result {
        Ok((dest, saves)) => {
            if settings.delete_archives {
                let _ = std::fs::remove_dir_all(downloads_dir(&library, id));
            }
            let note = (!saves.is_empty()).then(|| "Your saves were copied from the previous version.".to_string());
            dl.update(app, id, |j| {
                j.state = JobState::Done;
                j.install_dir = Some(dest.to_string_lossy().into_owned());
                j.finished_at = Some(db::now());
                j.error = note;
                if settings.delete_archives {
                    j.path = None;
                }
            });
            if let Some(tid) = job.thread_id {
                // A downloaded game belongs in the library (and gets update
                // checks). Bookmarking is idempotent on the server.
                let state = app.state::<AppState>();
                match state.api.add_bookmark(tid).await {
                    Ok(b) => {
                        if let Some(t) = &b.thread {
                            let _ = db.cache_threads(std::slice::from_ref(t), true);
                        }
                        let _ = app.emit("updates-changed", ());
                    }
                    Err(e) => log::warn!("couldn't add {tid} to the library: {e}"),
                }
                let _ = app.emit("game-installed", tid);
            }
            crate::tray::refresh(app);
            crate::notify::send(
                app,
                "Ready to play",
                &format!("{} {} is installed.", job.title, job.version),
            );
        }
        Err(e) => {
            dl.update(app, id, |j| {
                j.state = JobState::Failed;
                j.error = Some(format!("Couldn't install: {e}"));
            });
        }
    }
}
