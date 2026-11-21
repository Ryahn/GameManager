//! Finding games already on disk and matching them to catalog threads.

pub mod detect;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::api::types::SearchHit;
use detect::{Candidate, Engine};

/// A folder that looks like one installed game.
#[derive(Debug, Clone, Serialize)]
pub struct ScanFolder {
    pub path: String,
    pub folder_name: String,
    pub title_guess: String,
    pub version_guess: Option<String>,
    pub engine: Engine,
    pub targets: Vec<Candidate>,
    /// This folder is already linked to this thread on this PC.
    pub linked_thread_id: Option<i64>,
    /// …and that game is also in the library (nothing to do).
    pub linked_in_library: bool,
    /// Best catalog match, if the search found one.
    pub suggestion: Option<MatchSuggestion>,
    /// Comics/animations: how many images or videos were found.
    pub media_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchSuggestion {
    pub thread_id: i64,
    pub title: String,
    pub creator: String,
    pub cover: Option<String>,
    /// 0..=1 title similarity.
    pub confidence: f64,
    /// Good enough to pre-tick in the review list.
    pub confident: bool,
}

pub const CONFIDENT: f64 = 0.8;
const MAX_FOLDERS: usize = 1500;

fn folder(path: PathBuf, targets: Vec<Candidate>) -> ScanFolder {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let (mut title, mut version) = detect::parse_folder_name(&name);
    // `Library/Title/0.9` layouts: the title lives one level up.
    if (title.chars().all(|c| c.is_ascii_digit() || c == '.' || c.is_whitespace())
        || title.eq_ignore_ascii_case("game"))
        && let Some(parent) = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
            version = version.or_else(|| Some(name.trim_start_matches(['v', 'V']).to_string()));
            title = detect::parse_folder_name(parent).0;
        }
    ScanFolder {
        engine: targets.first().map(|c| c.engine).unwrap_or(Engine::Other),
        path: path.to_string_lossy().into_owned(),
        folder_name: name,
        title_guess: title,
        version_guess: version,
        targets,
        linked_thread_id: None,
        linked_in_library: false,
        suggestion: None,
        media_count: 0,
    }
}

fn subdirs(p: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(p) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| !n.starts_with('.') && !n.starts_with('$'))
        })
        .collect();
    v.sort();
    v
}

/// Lists game folders under `root`. Handles a flat folder of games, a
/// `Creator/Title` or `Title/Version` layout, or `root` being one game.
pub fn scan(root: &Path, linked: &[(i64, String, bool)]) -> Vec<ScanFolder> {
    let mut out = Vec::new();
    let here = detect::candidates(root);
    let children = subdirs(root);
    // `root` is itself a game when it has an exe and its subfolders don't.
    if !here.is_empty() && children.iter().all(|c| detect::candidates(c).is_empty()) {
        out.push(folder(root.to_path_buf(), here));
    } else {
        for child in children {
            if out.len() >= MAX_FOLDERS {
                break;
            }
            let c = detect::candidates(&child);
            let grandkids = subdirs(&child);
            let nested: Vec<_> = grandkids
                .into_iter()
                .map(|g| {
                    let c = detect::candidates(&g);
                    (g, c)
                })
                .filter(|(_, c)| !c.is_empty())
                .collect();
            // Several games inside: a creator or versions folder.
            let is_container = nested.len() > 1;
            if is_container {
                out.extend(nested.into_iter().map(|(g, c)| folder(g, c)));
            } else if !c.is_empty() {
                out.push(folder(child, c));
            }
        }
    }
    for f in &mut out {
        if let Some((id, _, in_library)) = linked.iter().find(|(_, dir, _)| same_path(dir, &f.path)) {
            f.linked_thread_id = Some(*id);
            f.linked_in_library = *in_library;
        }
    }
    out
}

/// One comic or animation found on disk: a folder of images/videos, or a
/// single comic file (.cbz, .pdf) or video.
fn media_item(path: PathBuf, count: usize) -> ScanFolder {
    let mut f = folder(path, Vec::new());
    if f.title_guess.is_empty() || std::path::Path::new(&f.path).is_file() {
        let stem = std::path::Path::new(&f.path)
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let (t, v) = detect::parse_folder_name(&stem);
        f.title_guess = t;
        f.version_guess = v;
    }
    // Comics often use a plain "v2" / "[v2]" that game folders don't.
    if f.version_guess.is_none() {
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        let re = RE.get_or_init(|| regex::Regex::new(r"(?i)\s*[\[(]?\bv(\d+)[\])]?\s*$").expect("regex"));
        if let Some(c) = re.captures(&f.title_guess) {
            f.version_guess = Some(c[1].to_string());
            f.title_guess = f.title_guess[..c.get(0).map_or(0, |m| m.start())].trim().to_string();
        }
    }
    f.media_count = count;
    f
}

fn has_media_below(dir: &Path, kind: crate::media::Kind, depth: usize) -> usize {
    let here = crate::media::count_direct(dir, kind);
    if here > 0 || depth == 0 {
        return here;
    }
    subdirs(dir).iter().map(|d| has_media_below(d, kind, depth - 1)).sum()
}

/// Finds comics or animations under `root`, using whatever layout the user
/// already has: `Title/…`, `Creator/Title/…`, or loose files. A folder whose
/// subfolders are chapters/episodes/volumes counts as one work.
pub fn scan_media(root: &Path, kind: crate::media::Kind, linked: &[(i64, String, bool)]) -> Vec<ScanFolder> {
    use crate::media::{count_direct, is_comic_file, is_part_name, is_video, Kind};
    let min_images = 3;
    let is_item_file = |p: &Path| match kind {
        Kind::Comics => is_comic_file(p),
        Kind::Animations => is_video(p),
        Kind::Games => false,
    };
    let counts_as_work = |dir: &Path| -> Option<usize> {
        let direct = count_direct(dir, kind);
        let enough = if kind == Kind::Comics { direct >= min_images } else { direct >= 1 };
        if enough {
            return Some(direct);
        }
        // Chapters/episodes in subfolders.
        let parts: Vec<_> = subdirs(dir)
            .into_iter()
            .filter(|d| d.file_name().and_then(|n| n.to_str()).is_some_and(is_part_name))
            .collect();
        let in_parts: usize = parts.iter().map(|d| has_media_below(d, kind, 1)).sum();
        (!parts.is_empty() && in_parts > 0).then_some(in_parts)
    };

    let mut out = Vec::new();
    // Loose files directly in the root.
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut files: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| is_item_file(p))
            .collect();
        files.sort();
        // A root that is itself one work (a folder of pages) is one item.
        if kind == Kind::Comics && count_direct(root, kind) >= min_images && files.iter().all(|p| !is_comic_file(p)) {
            out.push(media_item(root.to_path_buf(), count_direct(root, kind)));
        } else {
            out.extend(files.into_iter().map(|p| media_item(p, 1)));
        }
    }
    for child in subdirs(root) {
        if out.len() >= MAX_FOLDERS {
            break;
        }
        if let Some(n) = counts_as_work(&child) {
            out.push(media_item(child, n));
            continue;
        }
        // A creator (or series) folder: each work inside it.
        for grand in subdirs(&child) {
            if let Some(n) = counts_as_work(&grand) {
                out.push(media_item(grand, n));
            }
        }
        if let Ok(rd) = std::fs::read_dir(&child) {
            for e in rd.filter_map(Result::ok) {
                let p = e.path();
                if e.file_type().is_ok_and(|t| t.is_file()) && is_item_file(&p) && kind != Kind::Animations {
                    out.push(media_item(p, 1));
                }
            }
        }
    }
    for f in &mut out {
        if let Some((id, _, in_library)) = linked.iter().find(|(_, dir, _)| same_path(dir, &f.path)) {
            f.linked_thread_id = Some(*id);
            f.linked_in_library = *in_library;
        }
    }
    out
}

pub fn same_path(a: &str, b: &str) -> bool {
    crate::platform::norm_path(a) == crate::platform::norm_path(b)
}

/// Picks the best search hit for a folder's title guess.
pub fn pick_match(title: &str, hits: &[SearchHit], kind: crate::media::Kind) -> Option<MatchSuggestion> {
    hits.iter()
        .filter(|h| h.category.is_empty() || crate::media::Kind::parse(&h.category) == kind)
        .map(|h| (h, detect::title_similarity(title, &h.title)))
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .filter(|(_, s)| *s >= 0.45)
        .map(|(h, s)| MatchSuggestion {
            thread_id: h.thread_id,
            title: h.title.clone(),
            creator: h.creator.clone(),
            cover: h.cover.clone(),
            confidence: (s * 100.0).round() / 100.0,
            confident: s >= CONFIDENT,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"x").unwrap();
    }

    #[test]
    fn scans_flat_and_nested_layouts() {
        let root = std::env::temp_dir().join(format!("f95gm-scan-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        touch(&root.join("SummerNights-0.9-pc/SummerNights.exe"));
        fs::create_dir_all(root.join("SummerNights-0.9-pc/renpy")).unwrap();
        touch(&root.join("Velvet Dev/Crimson Haven/Crimson Haven.exe"));
        touch(&root.join("Velvet Dev/Echoes/Echoes.exe"));
        touch(&root.join("Starlight/0.4/Game.exe"));
        fs::create_dir_all(root.join("Starlight/0.4/www")).unwrap();
        touch(&root.join("Starlight/0.5/Game.exe"));
        fs::create_dir_all(root.join("Empty")).unwrap();

        let linked = vec![(42, root.join("Velvet Dev/Echoes").to_string_lossy().into_owned(), true)];
        let found = scan(&root, &linked);
        let titles: Vec<_> = found.iter().map(|f| f.title_guess.as_str()).collect();
        assert_eq!(found.len(), 5, "{titles:?}");
        assert!(titles.contains(&"Summer Nights"));
        assert!(titles.contains(&"Crimson Haven"));
        let star: Vec<_> = found.iter().filter(|f| f.title_guess == "Starlight").collect();
        assert_eq!(star.len(), 2);
        assert!(star.iter().any(|f| f.version_guess.as_deref() == Some("0.5")));
        let echoes = found.iter().find(|f| f.title_guess == "Echoes").unwrap();
        assert_eq!(echoes.linked_thread_id, Some(42));
        assert!(echoes.linked_in_library);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn finds_comics_and_animations_as_laid_out() {
        let root = std::env::temp_dir().join(format!("f95gm-scanmedia-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for p in ["Summer Days/001.jpg", "Summer Days/002.jpg", "Summer Days/003.png"] {
            touch(&root.join(p));
        }
        touch(&root.join("Night Out [v2].cbz"));
        touch(&root.join("Velvet Dev/Haven Comic/Chapter 1/01.jpg"));
        touch(&root.join("Velvet Dev/Haven Comic/Chapter 2/01.jpg"));
        touch(&root.join("Velvet Dev/Other Comic/p1.webp"));
        touch(&root.join("Velvet Dev/Other Comic/p2.webp"));
        touch(&root.join("Velvet Dev/Other Comic/p3.webp"));
        touch(&root.join("Notes/readme.txt"));
        let found = scan_media(&root, crate::media::Kind::Comics, &[]);
        let titles: Vec<_> = found.iter().map(|f| f.title_guess.as_str()).collect();
        assert_eq!(found.len(), 4, "{titles:?}");
        for t in ["Summer Days", "Night Out", "Haven Comic", "Other Comic"] {
            assert!(titles.contains(&t), "{t} in {titles:?}");
        }
        assert_eq!(found.iter().find(|f| f.title_guess == "Haven Comic").unwrap().media_count, 2);

        let anim = root.join("anim");
        touch(&anim.join("Studio X/Show/Episode 1/ep1.mp4"));
        touch(&anim.join("Studio X/Show/Episode 2/ep2.mp4"));
        touch(&anim.join("Clip.webm"));
        let found = scan_media(&anim, crate::media::Kind::Animations, &[]);
        let titles: Vec<_> = found.iter().map(|f| f.title_guess.as_str()).collect();
        assert_eq!(found.len(), 2, "{titles:?}");
        assert!(titles.contains(&"Show") && titles.contains(&"Clip"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn single_game_root() {
        let root = std::env::temp_dir().join(format!("f95gm-scan1-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        touch(&root.join("Haven.exe"));
        touch(&root.join("Haven_Data/x.dll"));
        let found = scan(&root, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].engine, Engine::Unity);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn picks_confident_match() {
        let hit = |id, t: &str| SearchHit {
            thread_id: id,
            title: t.into(),
            category: "games".into(),
            ..Default::default()
        };
        let hits = vec![hit(1, "Summer Academy"), hit(2, "Summer Nights"), hit(3, "Nights")];
        let m = pick_match("Summer Nights", &hits, crate::media::Kind::Games).unwrap();
        assert_eq!(m.thread_id, 2);
        assert!(m.confidence >= CONFIDENT);
        assert!(pick_match("Totally Different", &hits, crate::media::Kind::Games).is_none());
        assert!(pick_match("Summer Nights", &hits, crate::media::Kind::Comics).is_none(), "only the right type");
    }
}
