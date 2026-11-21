//! Comics and animations: the catalog's other content types. They share the
//! library, bookmarks, update tracking and downloads with games; what's
//! different is what counts as "installed" (a folder or file of images or
//! videos, not an executable) and how a folder of them is recognised.

use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Games,
    Comics,
    Animations,
}

impl Kind {
    /// From the catalog's category ("games", "comics", "animations").
    /// Anything unknown is treated as a game.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "comics" | "comic" => Kind::Comics,
            "animations" | "animation" => Kind::Animations,
            _ => Kind::Games,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Games => "games",
            Kind::Comics => "comics",
            Kind::Animations => "animations",
        }
    }

    /// Default subfolder of the games folder when no folder is set for
    /// this type.
    pub fn default_subfolder(self) -> Option<&'static str> {
        match self {
            Kind::Games => None,
            Kind::Comics => Some("Comics"),
            Kind::Animations => Some("Animations"),
        }
    }
}

const IMAGE_EXT: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "avif", "bmp", "jxl"];
const COMIC_FILE_EXT: &[&str] = &["cbz", "cbr", "cb7", "pdf", "zip", "rar", "7z"];
const VIDEO_EXT: &[&str] = &["mp4", "m4v", "webm", "mkv", "mov", "avi", "wmv", "flv", "mpg", "mpeg", "ts"];

fn ext(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

pub fn is_image(p: &Path) -> bool {
    IMAGE_EXT.contains(&ext(p).as_str())
}

pub fn is_video(p: &Path) -> bool {
    VIDEO_EXT.contains(&ext(p).as_str())
}

/// A single file that is a whole comic (archive or PDF).
pub fn is_comic_file(p: &Path) -> bool {
    COMIC_FILE_EXT.contains(&ext(p).as_str())
}

/// The folder where downloads of `kind` go: its own folder if set,
/// otherwise a subfolder of the games folder.
pub fn folder_for(kind: Kind, games: Option<&str>, comics: Option<&str>, animations: Option<&str>) -> Option<PathBuf> {
    let own = match kind {
        Kind::Games => games,
        Kind::Comics => comics,
        Kind::Animations => animations,
    };
    if let Some(dir) = own.filter(|d| !d.trim().is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let base = PathBuf::from(games.filter(|d| !d.trim().is_empty())?);
    Some(match kind.default_subfolder() {
        Some(sub) => base.join(sub),
        None => base,
    })
}

/// How many media files of `kind` are directly inside `dir`.
pub fn count_direct(dir: &Path, kind: Kind) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| match kind {
            Kind::Comics => is_image(p) || is_comic_file(p),
            Kind::Animations => is_video(p),
            Kind::Games => false,
        })
        .count()
}

/// Folder names that mean "part of one work" rather than "a separate work":
/// Chapter 1, Ch.02, Episode 3, Vol 1, Season 2, Part 4, #5, 01 …
pub fn is_part_name(name: &str) -> bool {
    let n = name.trim().to_lowercase();
    let starts = ["ch", "chapter", "ep", "episode", "vol", "volume", "part", "season", "issue", "book", "#", "s0", "s1"];
    n.chars().next().is_some_and(|c| c.is_ascii_digit())
        || starts.iter().any(|s| {
            n.strip_prefix(s)
                .is_some_and(|rest| rest.trim_start_matches(['.', ' ', '_', '-']).starts_with(|c: char| c.is_ascii_digit()))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kinds() {
        assert_eq!(Kind::parse("comics"), Kind::Comics);
        assert_eq!(Kind::parse("Animations"), Kind::Animations);
        assert_eq!(Kind::parse(""), Kind::Games);
        assert_eq!(Kind::parse("mods"), Kind::Games);
    }

    #[test]
    fn picks_folders() {
        assert_eq!(folder_for(Kind::Games, Some("G"), None, None), Some(PathBuf::from("G")));
        assert_eq!(folder_for(Kind::Comics, Some("G"), None, None), Some(PathBuf::from("G").join("Comics")));
        assert_eq!(folder_for(Kind::Comics, Some("G"), Some("C"), None), Some(PathBuf::from("C")));
        assert_eq!(folder_for(Kind::Animations, None, None, Some("A")), Some(PathBuf::from("A")));
        assert_eq!(folder_for(Kind::Animations, None, None, None), None);
    }

    #[test]
    fn recognises_files_and_parts() {
        assert!(is_image(Path::new("p01.JPG")) && is_video(Path::new("e1.mp4")) && is_comic_file(Path::new("x.cbz")));
        assert!(!is_image(Path::new("x.txt")));
        for part in ["Chapter 1", "ch.02", "Episode 3", "Vol_1", "Season 2", "01", "#5", "Part-4"] {
            assert!(is_part_name(part), "{part}");
        }
        for work in ["Summer Nights", "Velvet Dev", "Chapters of Love"] {
            assert!(!is_part_name(work), "{work}");
        }
    }
}
