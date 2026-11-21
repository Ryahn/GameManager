//! What differs between Windows, Linux (Steam Deck included) and macOS:
//! path comparison, what a runnable game file looks like, and how Windows
//! games are run elsewhere (Proton via umu-launcher, Wine, or CrossOver).

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub fn os() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Valve's handheld (SteamOS sets `ID=steamos`; the Deck also exports
/// `SteamDeck=1` in Game Mode).
pub fn is_steam_deck() -> bool {
    if !cfg!(target_os = "linux") {
        return false;
    }
    std::env::var("SteamDeck").is_ok_and(|v| v == "1")
        || std::fs::read_to_string("/etc/os-release").is_ok_and(|s| {
            s.lines()
                .any(|l| l.trim() == "ID=steamos" || l.trim() == "VARIANT_ID=steamdeck")
        })
}

// ------------------------------------------------------------------ paths

/// A path in a form that can be compared: Windows paths are
/// case-insensitive and accept both slashes; others are compared exactly.
pub fn norm_path(p: &str) -> String {
    if cfg!(windows) {
        p.replace('/', "\\").trim_end_matches('\\').to_lowercase()
    } else {
        let t = p.trim_end_matches('/');
        if t.is_empty() { "/".into() } else { t.to_string() }
    }
}

pub const SEP: char = if cfg!(windows) { '\\' } else { '/' };

/// `child` is `root` or inside it (never `root2` when `root` is asked).
pub fn is_under(child: &str, root: &str) -> bool {
    let (c, r) = (norm_path(child), norm_path(root));
    c == r || c.starts_with(&format!("{r}{SEP}"))
}

/// Too close to the top of the disk to treat as "the game's folder":
/// tracking or deleting it would sweep in unrelated things.
pub fn is_too_shallow(p: &Path) -> bool {
    p.components()
        .filter(|c| matches!(c, std::path::Component::Normal(_)))
        .count()
        < 2
}

// ------------------------------------------------------------ game files

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A Windows program: run directly on Windows, through a compatibility
    /// layer elsewhere.
    WindowsExe,
    /// A program for this OS (Linux binary or script, macOS app).
    Native,
    /// Opened with the default app (browser games, shortcuts).
    Open,
}

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn is_elf(p: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    std::fs::File::open(p)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == [0x7f, b'E', b'L', b'F']
}

/// How to start `p`, or `None` if it isn't something we launch.
pub fn kind(p: &Path) -> Option<Kind> {
    let e = ext(p);
    match e.as_str() {
        "exe" => Some(Kind::WindowsExe),
        "html" | "htm" => Some(Kind::Open),
        "bat" | "lnk" if cfg!(windows) => Some(Kind::Open),
        "app" if cfg!(target_os = "macos") && p.is_dir() => Some(Kind::Native),
        "command" if cfg!(target_os = "macos") => Some(Kind::Native),
        "sh" | "x86_64" | "x86" | "appimage" if cfg!(target_os = "linux") => Some(Kind::Native),
        "" if cfg!(target_os = "linux") && p.is_file() && is_elf(p) => Some(Kind::Native),
        _ => None,
    }
}

/// File extensions to offer in "Pick the game's executable" dialogs.
pub fn pick_extensions() -> Vec<&'static str> {
    match os() {
        "windows" => vec!["exe", "html", "htm", "bat", "lnk"],
        "macos" => vec!["app", "command", "exe", "html", "htm"],
        _ => vec!["sh", "x86_64", "x86", "AppImage", "exe", "html", "htm"],
    }
}

/// Makes Linux/macOS game binaries and scripts runnable (archives made on
/// Windows drop the executable bit).
#[cfg(unix)]
pub fn make_executable(p: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(m) = std::fs::metadata(p) {
        let mut perm = m.permissions();
        if perm.mode() & 0o111 == 0 {
            perm.set_mode(perm.mode() | 0o755);
            let _ = std::fs::set_permissions(p, perm);
        }
    }
}

#[cfg(not(unix))]
pub fn make_executable(_p: &Path) {}

// ----------------------------------------------- running Windows games

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerKind {
    /// umu-launcher: Proton outside Steam (the Steam Deck's usual choice).
    Umu,
    Wine,
    CrossOver,
    Custom,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Runner {
    pub kind: RunnerKind,
    pub program: PathBuf,
}

/// The program set in Settings, if any (kept here so sync code can read it).
fn custom_runner() -> &'static Mutex<Option<String>> {
    static CELL: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(None))
}

pub fn set_custom_runner(v: Option<String>) {
    *custom_runner().lock().unwrap_or_else(|p| p.into_inner()) = v.filter(|s| !s.trim().is_empty());
}

fn which(name: &str) -> Option<PathBuf> {
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .chain(
            // Common places that aren't always on PATH for GUI apps.
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".local/bin")),
        )
        .map(|d| d.join(name))
        .find(|p| p.is_file())
}

/// How Windows games run on this machine: the program from Settings, else
/// umu-launcher, else Wine (CrossOver's Wine on macOS).
pub fn find_runner() -> Option<Runner> {
    if cfg!(windows) {
        return None;
    }
    if let Some(c) = custom_runner().lock().unwrap_or_else(|p| p.into_inner()).clone() {
        let p = PathBuf::from(&c);
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let kind = if name.starts_with("umu") { RunnerKind::Umu } else { RunnerKind::Custom };
        return Some(Runner { kind, program: p });
    }
    if let Some(p) = which("umu-run") {
        return Some(Runner { kind: RunnerKind::Umu, program: p });
    }
    if cfg!(target_os = "macos") {
        let cx = PathBuf::from("/Applications/CrossOver.app/Contents/SharedSupport/CrossOver/bin/wine");
        if cx.is_file() {
            return Some(Runner { kind: RunnerKind::CrossOver, program: cx });
        }
    }
    which("wine")
        .or_else(|| which("wine64"))
        .map(|p| Runner { kind: RunnerKind::Wine, program: p })
}

/// A command that runs `exe` through `runner`.
pub fn runner_command(runner: &Runner, exe: &Path) -> std::process::Command {
    let mut cmd = std::process::Command::new(&runner.program);
    if runner.kind == RunnerKind::Umu {
        // umu-launcher needs a game id; "umu-default" applies no
        // per-game fixes. GE-Proton is downloaded once if missing, unless
        // the user chose a Proton build themselves.
        if std::env::var_os("GAMEID").is_none() {
            cmd.env("GAMEID", "umu-default");
        }
        if std::env::var_os("PROTONPATH").is_none() {
            cmd.env("PROTONPATH", "GE-Proton");
        }
    }
    cmd.arg(exe);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_paths_for_this_os() {
        if cfg!(windows) {
            assert!(is_under("C:/Games/Haven/x.exe", "c:\\games\\haven"));
            assert!(!is_under("C:\\Games\\Haven2\\x.exe", "C:\\Games\\Haven"));
        } else {
            assert!(is_under("/home/deck/Games/Haven/x.sh", "/home/deck/Games/Haven/"));
            assert!(!is_under("/home/deck/Games/Haven2/x.sh", "/home/deck/Games/Haven"));
            assert!(!is_under("/home/deck/games/haven/x.sh", "/home/deck/Games/Haven"), "case matters off Windows");
        }
        assert!(is_too_shallow(Path::new(if cfg!(windows) { "C:\\Games" } else { "/home" })));
        assert!(!is_too_shallow(Path::new(if cfg!(windows) { "C:\\Games\\Haven" } else { "/home/deck" })));
    }

    #[test]
    fn knows_what_it_can_launch() {
        assert_eq!(kind(Path::new("Game.exe")), Some(Kind::WindowsExe));
        assert_eq!(kind(Path::new("index.html")), Some(Kind::Open));
        assert_eq!(kind(Path::new("readme.txt")), None);
        if cfg!(target_os = "linux") {
            assert_eq!(kind(Path::new("Game.sh")), Some(Kind::Native));
            assert_eq!(kind(Path::new("Game.x86_64")), Some(Kind::Native));
        } else {
            assert_eq!(kind(Path::new("Game.sh")), None);
        }
    }

    #[test]
    fn umu_gets_a_game_id_and_proton() {
        let r = Runner { kind: RunnerKind::Umu, program: PathBuf::from("/usr/bin/umu-run") };
        let cmd = runner_command(&r, Path::new("/games/Haven/Haven.exe"));
        let envs: Vec<_> = cmd.get_envs().map(|(k, v)| (k.to_string_lossy().into_owned(), v.map(|v| v.to_string_lossy().into_owned()))).collect();
        if std::env::var_os("GAMEID").is_none() {
            assert!(envs.contains(&("GAMEID".into(), Some("umu-default".into()))));
        }
        assert_eq!(cmd.get_args().last().unwrap(), "/games/Haven/Haven.exe");
    }
}
