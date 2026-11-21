//! Finds the thing to run inside a game folder and names it in plain words.
//!
//! Engines leave fingerprints next to their executable:
//! - Ren'Py: `renpy/` (and `game/`, `lib/`) beside `Name.exe` / `Name-32.exe`
//! - RPG Maker: `Game.exe` with `www/` (MV), `data/` + `js/` (MZ) or `*.rgss*`
//! - Unity: `Name.exe` with `Name_Data/`
//! - Unreal: `Engine/` folder, real binary under `Name/Binaries/Win64`
//! - Godot: `Name.exe` with `Name.pck`
//! - HTML: `index.html`

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Engine {
    RenPy,
    RpgMaker,
    Unity,
    Unreal,
    Godot,
    Html,
    Other,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Candidate {
    pub path: String,
    pub label: String,
    pub engine: Engine,
    /// Higher is a better default.
    #[serde(skip)]
    pub score: i32,
}

/// Executables that ship with games but are never the game.
const NOT_GAMES: &[&str] = &[
    "unitycrashhandler",
    "crashhandler",
    "crashpad_handler",
    "crashreportclient",
    "unins",
    "uninstall",
    "notification_helper",
    "dxwebsetup",
    "dxsetup",
    "vcredist",
    "vc_redist",
    "directx",
    "python",
    "pythonw",
    "zsync",
    "zsyncmake",
    "ue4prereqsetup",
    "ueprereqsetup",
    "nwjc",
    "payload",
    "7z",
    "winrar",
    "setup",
    "config",
    "launcherconfig",
];

/// Folders that never hold the main executable.
const SKIP_DIRS: &[&str] = &[
    "renpy",
    "lib",
    "game",
    "www",
    "js",
    "audio",
    "img",
    "movies",
    "save",
    "saves",
    "locales",
    "swiftshader",
    "monobleedingedge",
    "__macosx",
    "engine",
    "redist",
    "_commonredist",
    "data",
];

pub const LAUNCHABLE_EXT: &[&str] = &["exe", "html", "htm", "bat", "lnk"];

fn lower_name(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn stem(p: &Path) -> String {
    p.file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

fn is_helper(stem_lower: &str) -> bool {
    NOT_GAMES
        .iter()
        .any(|n| stem_lower == *n || stem_lower.starts_with(n))
}

fn engine_of(exe: &Path) -> Engine {
    let dir = exe.parent().unwrap_or(Path::new("."));
    let s = stem(exe);
    let base = s.trim_end_matches("-32").trim_end_matches("-64");
    if dir.join("renpy").is_dir() {
        return Engine::RenPy;
    }
    if dir.join(format!("{base}_Data")).is_dir() || dir.join(format!("{s}_Data")).is_dir() {
        return Engine::Unity;
    }
    if dir.join(format!("{s}.pck")).is_file() {
        return Engine::Godot;
    }
    if dir.join("Engine").is_dir() {
        return Engine::Unreal;
    }
    let rpgm_files = std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(Result::ok).any(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n.ends_with(".rgss3a") || n.ends_with(".rgss2a") || n.ends_with(".rgssad")
            })
        })
        .unwrap_or(false);
    if dir.join("www").is_dir()
        || rpgm_files
        || (s.eq_ignore_ascii_case("game") && dir.join("js").is_dir())
    {
        return Engine::RpgMaker;
    }
    Engine::Other
}

/// Turns `My_Game-32` into "My Game (32-bit)".
fn friendly(exe: &Path, engine: Engine) -> String {
    let s = stem(exe);
    let (base, suffix) = if let Some(b) = s.strip_suffix("-32") {
        (b.to_string(), " (32-bit)")
    } else {
        (s.clone(), "")
    };
    if matches!(engine, Engine::Html) {
        return "Play in browser".into();
    }
    if base.eq_ignore_ascii_case("game") || base.eq_ignore_ascii_case("nw") {
        return format!("Play{suffix}");
    }
    let pretty = base.replace(['_', '.'], " ");
    format!("{}{}", pretty.trim(), suffix)
}

fn score(exe: &Path, engine: Engine, depth: usize, folder_hint: &str) -> i32 {
    let s = stem(exe).to_lowercase();
    let mut score = 100 - (depth as i32) * 20;
    if engine != Engine::Other {
        score += 30;
    }
    if s.ends_with("-32") || s.ends_with("x86") || s.contains("32bit") {
        score -= 15;
    }
    if s.contains("launcher") || s.contains("debug") || s.contains("dedicated") {
        score -= 25;
    }
    if s.ends_with("-shipping") {
        score += 10;
    }
    let hint = normalize(folder_hint);
    if !hint.is_empty() && normalize(&s).starts_with(&hint[..hint.len().min(6)]) {
        score += 10;
    }
    if exe
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
    {
        score -= 10;
    }
    score
}

fn walk(dir: &Path, depth: usize, max: usize, out: &mut Vec<(PathBuf, usize)>, budget: &mut usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for e in rd.filter_map(Result::ok) {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let p = e.path();
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            let n = lower_name(&p);
            if !SKIP_DIRS.contains(&n.as_str()) && !n.ends_with("_data") && !n.starts_with('.') {
                subdirs.push(p);
            }
        } else if ft.is_file() {
            let ext = p
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let is_exe = ext == "exe" && !is_helper(&stem(&p).to_lowercase());
            let is_index = (ext == "html" || ext == "htm") && lower_name(&p).starts_with("index.");
            if is_exe || is_index {
                out.push((p, depth));
            }
        }
    }
    if depth < max {
        for d in subdirs {
            walk(&d, depth + 1, max, out, budget);
        }
    }
}

/// Launch candidates in `dir`, best first.
pub fn candidates(dir: &Path) -> Vec<Candidate> {
    let mut found = Vec::new();
    // Caps work on huge or odd folders (e.g. a whole drive picked by mistake).
    let mut budget = 4000;
    walk(dir, 0, 3, &mut found, &mut budget);

    // Unreal: prefer the real shipping binary over the bootstrap.
    let hint = dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let mut out: Vec<Candidate> = found
        .into_iter()
        .map(|(p, depth)| {
            let engine = if p
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
            {
                Engine::Html
            } else {
                engine_of(&p)
            };
            Candidate {
                label: friendly(&p, engine),
                score: score(&p, engine, depth, &hint),
                path: p.to_string_lossy().into_owned(),
                engine,
            }
        })
        .collect();
    // An HTML game's index file is only interesting when there is no exe.
    if out.iter().any(|c| c.engine != Engine::Html) {
        out.retain(|c| c.engine != Engine::Html);
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then(a.path.len().cmp(&b.path.len())));
    out.truncate(8);
    out
}

pub fn label_for(path: &Path) -> String {
    let engine = if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
    {
        Engine::Html
    } else {
        engine_of(path)
    };
    friendly(path, engine)
}

// ------------------------------------------------------------ name parsing

/// Lowercase letters and digits only: "Summer's Night!" -> "summersnight".
pub fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn version_re() -> &'static regex::Regex {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(
            r"(?ix)
            [\s_\-\.\[\(]*
            (?:v(?:er(?:sion)?)?[\s_\.]?)?
            (?:
              (\d+(?:[\._]\d+)+[a-z]?)
              |
              (\d+[a-z]?)[\s_\-]+(?:pc|win|windows|linux|mac)\b
            )
            (?:[\s_\-\.]*[a-z]*\d*)?
            [\]\)]?
            ",
        )
        .expect("version regex")
    })
}

const NOISE: &[&str] = &[
    "pc", "win", "windows", "linux", "mac", "market", "android", "64bit", "32bit", "x64", "x86",
    "public", "final", "full", "patched", "uncensored", "compressed", "repack", "steam", "gog",
    "release", "build", "beta", "alpha", "demo", "eng", "english", "multi",
];

/// Guesses `(title, version)` from a folder name such as
/// `SummerNights-0.9.1-pc`, `Summer_Nights_v0.9.1` or `[Dev] Summer Nights [v0.9]`.
pub fn parse_folder_name(name: &str) -> (String, Option<String>) {
    let mut s = name.to_string();
    // Leading "[Creator]" tags.
    while s.starts_with('[') {
        match s.find(']') {
            Some(i) => s = s[i + 1..].trim_start().to_string(),
            None => break,
        }
    }
    let mut version = None;
    if let Some(m) = version_re()
        .captures(&s)
        .and_then(|c| c.get(1).or_else(|| c.get(2)))
    {
        // Only treat it as a version if something title-like comes before it.
        if m.start() > 2 {
            version = Some(m.as_str().replace('_', "."));
            let cut = version_re().find(&s).map(|f| f.start()).unwrap_or(m.start());
            s.truncate(cut);
        }
    }

    // Split CamelCase and separators into words.
    let mut spaced = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if matches!(c, '_' | '-' | '.' | '[' | ']' | '(' | ')') {
            spaced.push(' ');
            continue;
        }
        if i > 0 && c.is_uppercase() && chars[i - 1].is_lowercase() {
            spaced.push(' ');
        }
        spaced.push(c);
    }
    let words: Vec<&str> = spaced
        .split_whitespace()
        .filter(|w| !NOISE.contains(&w.to_lowercase().as_str()))
        .collect();
    let title = if words.is_empty() {
        name.trim().to_string()
    } else {
        words.join(" ")
    };
    (title, version)
}

/// 0..=1 similarity between two titles, tolerant of punctuation, word order
/// and subtitles ("Echoes" vs "Echoes: Reborn" scores well, unrelated titles
/// score near 0).
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let (na, nb) = (normalize(a), normalize(b));
    if na.is_empty() || nb.is_empty() {
        return 0.0;
    }
    if na == nb {
        return 1.0;
    }
    let words = |s: &str| -> std::collections::BTreeSet<String> {
        s.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect()
    };
    let (wa, wb) = (words(a), words(b));
    let inter = wa.intersection(&wb).count() as f64;
    let union = wa.union(&wb).count().max(1) as f64;
    let jaccard = inter / union;
    // "SummerNights" vs "Summer Nights": compare squashed strings too.
    let (short, long) = if na.len() <= nb.len() { (&na, &nb) } else { (&nb, &na) };
    let prefix = if long.starts_with(short.as_str()) {
        short.len() as f64 / long.len() as f64 * 0.95
    } else {
        0.0
    };
    jaccard.max(prefix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f95gm-detect-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn renpy_prefers_64bit_and_skips_python() {
        let d = tmp("renpy");
        let g = d.join("SummerNights-0.9-pc");
        touch(&g.join("SummerNights.exe"));
        touch(&g.join("SummerNights-32.exe"));
        touch(&g.join("lib/py3-windows-x86_64/pythonw.exe"));
        fs::create_dir_all(g.join("renpy")).unwrap();
        let c = candidates(&g);
        assert_eq!(c.len(), 2);
        assert!(c[0].path.ends_with("SummerNights.exe"));
        assert_eq!(c[0].engine, Engine::RenPy);
        assert_eq!(c[1].label, "SummerNights (32-bit)");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn rpgm_unity_and_helpers() {
        let d = tmp("mixed");
        let r = d.join("rpg");
        touch(&r.join("Game.exe"));
        fs::create_dir_all(r.join("www")).unwrap();
        let c = candidates(&r);
        assert_eq!(c[0].engine, Engine::RpgMaker);
        assert_eq!(c[0].label, "Play");

        let u = d.join("unity");
        touch(&u.join("Echoes.exe"));
        touch(&u.join("UnityCrashHandler64.exe"));
        fs::create_dir_all(u.join("Echoes_Data")).unwrap();
        let c = candidates(&u);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].engine, Engine::Unity);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn html_only_when_no_exe() {
        let d = tmp("html");
        touch(&d.join("index.html"));
        let c = candidates(&d);
        assert_eq!(c[0].engine, Engine::Html);
        touch(&d.join("nw.exe"));
        assert!(candidates(&d).iter().all(|c| c.engine != Engine::Html));
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn parses_common_folder_names() {
        assert_eq!(
            parse_folder_name("SummerNights-0.9.1-pc"),
            ("Summer Nights".into(), Some("0.9.1".into()))
        );
        assert_eq!(
            parse_folder_name("Summer_Nights_v0.9.1b"),
            ("Summer Nights".into(), Some("0.9.1b".into()))
        );
        assert_eq!(
            parse_folder_name("[Velvet Dev] Crimson Haven [v0.12]"),
            ("Crimson Haven".into(), Some("0.12".into()))
        );
        assert_eq!(
            parse_folder_name("Echoes Reborn Ep 2 Final"),
            ("Echoes Reborn Ep 2".into(), None)
        );
        assert_eq!(parse_folder_name("Haven").0, "Haven");
    }

    #[test]
    fn similarity_ranks_sensibly() {
        assert_eq!(title_similarity("Summer Nights", "summer-nights!"), 1.0);
        assert!(title_similarity("SummerNights", "Summer Nights: Reborn") > 0.55);
        assert!(title_similarity("Crimson Haven", "Haven Crimson") > 0.9);
        assert!(title_similarity("Crimson Haven", "Starlight Academy") < 0.1);
    }
}
