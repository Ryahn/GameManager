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

use crate::platform::{self, Kind};
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

/// Any process running from inside `root`, or started with a path inside it
/// (Windows games under Wine/Proton show up as the Wine loader with the
/// game's .exe as an argument).
fn any_process_in(sys: &mut System, root: &str) -> bool {
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing()
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cmd(UpdateKind::OnlyIfNotSet),
    );
    sys.processes().values().any(|p| {
        p.exe().is_some_and(|e| platform::is_under(&e.to_string_lossy(), root))
            || p.cmd().iter().any(|a| platform::is_under(&a.to_string_lossy(), root))
    })
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
    if !path.exists() {
        return Err(format!(
            "The game file is missing: {}. It may have been moved or deleted.",
            target.path
        ));
    }
    let db = app.state::<Arc<Db>>().inner().clone();
    let started = db::now();

    let kind = platform::kind(&path).ok_or("The launcher can't start this kind of file.")?;
    let child: Option<Child> = match kind {
        Kind::WindowsExe if !cfg!(windows) => {
            let runner = platform::find_runner().ok_or(
                "To play Windows games here, install umu-launcher (Proton) or Wine, or choose the program in Settings → Windows games.",
            )?;
            let mut cmd = platform::runner_command(&runner, &path);
            cmd.args(split_args(&target.args));
            if let Some(dir) = path.parent() {
                cmd.current_dir(dir);
            }
            Some(cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", runner.program.display()))?)
        }
        Kind::Native if cfg!(target_os = "macos") && path.is_dir() => {
            // `open -W` waits until the app quits, so the session is timed.
            let mut cmd = Command::new("open");
            cmd.arg("-W").arg(&path);
            let args = split_args(&target.args);
            if !args.is_empty() {
                cmd.arg("--args").args(args);
            }
            Some(cmd.spawn().map_err(|e| format!("Couldn't start the game: {e}"))?)
        }
        Kind::Native => {
            platform::make_executable(&path);
            let is_script = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sh"));
            let mut cmd = if is_script {
                let mut c = Command::new("sh");
                c.arg(&path);
                c
            } else {
                Command::new(&path)
            };
            cmd.args(split_args(&target.args));
            if let Some(dir) = path.parent() {
                cmd.current_dir(dir);
            }
            Some(cmd.spawn().map_err(|e| format!("Couldn't start the game: {e}"))?)
        }
        Kind::WindowsExe => {
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
        Kind::Open => {
            app.opener()
                .open_path(target.path.clone(), None::<&str>)
                .map_err(|e| format!("Couldn't open the game: {e}"))?;
            None
        }
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
    let root_path = tracking_root(&path, install_dir);
    let root = if platform::is_too_shallow(&root_path) {
        String::new()
    } else {
        root_path.to_string_lossy().into_owned()
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
