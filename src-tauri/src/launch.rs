//! Starting games and timing how long they run.
//!
//! Many games hand off to another process (Ren'Py's exe starts `pythonw`,
//! Unreal's bootstrap starts the shipping binary), so the session lasts while
//! *any* process whose executable lives inside the game folder is alive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::db::{self, Db, LaunchTarget};

const POLL: Duration = Duration::from_secs(5);
/// Sessions shorter than this are crashes or misclicks; not counted.
const MIN_SESSION_SECS: i64 = 10;

#[derive(Clone, Serialize)]
pub struct RunningEvent {
    pub thread_id: i64,
    pub running: bool,
    /// Length of the session that just ended.
    pub secs: Option<i64>,
}

#[derive(Default)]
pub struct Launcher {
    running: Mutex<HashMap<i64, i64>>,
}

impl Launcher {
    pub fn running_ids(&self) -> Vec<i64> {
        self.lock().keys().copied().collect()
    }

    pub fn is_running(&self, id: i64) -> bool {
        self.lock().contains_key(&id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<i64, i64>> {
        self.running.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

/// Splits user-entered arguments, honouring double quotes.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for c in s.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The folder whose processes count as "the game is running".
fn tracking_root(target: &Path, install_dir: Option<&str>) -> PathBuf {
    if let Some(dir) = install_dir {
        let dir = PathBuf::from(dir);
        if target.starts_with(&dir) {
            return dir;
        }
    }
    target.parent().map(Path::to_path_buf).unwrap_or_default()
}

fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn any_process_in(sys: &mut System, root: &str) -> bool {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    sys.processes()
        .values()
        .filter_map(|p| p.exe())
        .any(|exe| lower(exe).starts_with(root))
}

/// Starts a game with the chosen (or default) launch target.
pub fn play(app: &AppHandle, thread_id: i64, target_id: Option<i64>) -> Result<(), String> {
    let db = app.state::<Arc<Db>>().inner().clone();
    let game = db
        .get(thread_id)?
        .ok_or("Add the game's executable first.")?;
    let target = match target_id {
        Some(t) => game.targets.iter().find(|x| x.id == t),
        None => game
            .targets
            .iter()
            .find(|x| x.is_default)
            .or(game.targets.first()),
    }
    .ok_or("Add the game's executable first.")?;
    launch(
        app,
        thread_id,
        target,
        game.install_dir.as_deref(),
        game.installed_version.clone(),
    )
}

fn launch(
    app: &AppHandle,
    thread_id: i64,
    target: &LaunchTarget,
    install_dir: Option<&str>,
    version: Option<String>,
) -> Result<(), String> {
    let launcher = app.state::<Arc<Launcher>>().inner().clone();
    if launcher.is_running(thread_id) {
        return Err("This game is already running.".into());
    }
    let path = PathBuf::from(&target.path);
    if !path.is_file() {
        return Err(format!(
            "The game file is missing: {}. It may have been moved or deleted.",
            target.path
        ));
    }
    let db = app.state::<Arc<Db>>().inner().clone();
    let started = db::now();

    let child: Option<Child> = match ext(&path).as_str() {
        "exe" => {
            let mut cmd = Command::new(&path);
            cmd.args(split_args(&target.args));
            if let Some(dir) = path.parent() {
                cmd.current_dir(dir);
            }
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
                cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
            }
            Some(cmd.spawn().map_err(|e| match e.raw_os_error() {
                Some(740) => "This game needs administrator rights. Right-click the launcher and choose \"Run as administrator\", or change the game's compatibility settings.".to_string(),
                _ => format!("Couldn't start the game: {e}"),
            })?)
        }
        "html" | "htm" | "lnk" | "bat" => {
            app.opener()
                .open_path(target.path.clone(), None::<&str>)
                .map_err(|e| format!("Couldn't open the game: {e}"))?;
            None
        }
        _ => return Err("Only .exe, .html, .bat and shortcut files can be launched.".into()),
    };

    // Browser games and shortcuts can't be timed; just remember the launch.
    let Some(mut child) = child else {
        db.record_session(thread_id, started, 0, version.as_deref())?;
        let _ = app.emit(
            "game-running",
            RunningEvent {
                thread_id,
                running: false,
                secs: None,
            },
        );
        crate::tray::refresh(app);
        return Ok(());
    };

    launcher.lock().insert(thread_id, started);
    let _ = app.emit(
        "game-running",
        RunningEvent {
            thread_id,
            running: true,
            secs: None,
        },
    );

    // Trailing separator so `Games\Haven` doesn't also match `Games\Haven2`.
    // A game sitting at a drive root would match everything, so only the
    // launched process itself is tracked then.
    let root = lower(&tracking_root(&path, install_dir));
    let root = if root.trim_end_matches('\\').len() <= 3 {
        String::new()
    } else {
        format!("{}\\", root.trim_end_matches('\\'))
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let mut sys = System::new().ok();
        let mut misses = 0;
        loop {
            std::thread::sleep(POLL);
            let child_alive = matches!(child.try_wait(), Ok(None));
            if child_alive || (!root.is_empty() && sys.as_mut().is_some_and(|s| any_process_in(s, &root))) {
                misses = 0;
            } else {
                misses += 1;
                // Two quiet polls in a row: some games briefly have no
                // process while handing off from a launcher.
                if misses >= 2 {
                    break;
                }
            }
        }
        let ended = db::now() - POLL.as_secs() as i64;
        let secs = (ended - started).max(0);
        launcher.lock().remove(&thread_id);
        if secs >= MIN_SESSION_SECS
            && let Err(e) = db.record_session(thread_id, started, secs, version.as_deref()) {
                log::error!("saving play session failed: {e}");
            }
        let _ = app.emit(
            "game-running",
            RunningEvent {
                thread_id,
                running: false,
                secs: Some(secs),
            },
        );
        crate::tray::refresh(&app);
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_quoted_args() {
        assert_eq!(
            split_args(r#"-windowed  --lang "en us" x"#),
            vec!["-windowed", "--lang", "en us", "x"]
        );
        assert!(split_args("   ").is_empty());
    }

    #[test]
    fn tracking_root_prefers_install_dir() {
        let t = Path::new("C:/Games/Haven/bin/Haven.exe");
        assert_eq!(
            tracking_root(t, Some("C:/Games/Haven")),
            PathBuf::from("C:/Games/Haven")
        );
        assert_eq!(
            tracking_root(t, Some("D:/Other")),
            PathBuf::from("C:/Games/Haven/bin")
        );
    }
}
