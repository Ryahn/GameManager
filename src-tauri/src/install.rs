//! Unpacking downloaded archives and putting games in place.
//!
//! Archives are extracted into a staging folder inside the library (same
//! drive, so the final move is a rename). Entries that would land outside the
//! staging folder (`..`, absolute paths, symlinks) are skipped, never written.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub type InstallResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Zip,
    SevenZip,
    Rar,
}

/// Archive type from the file name. Multi-part sets are read from their
/// first part (`.part1.rar`, `.7z.001`, `.zip`).
pub fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_str()?.to_lowercase();
    if name.ends_with(".zip") || name.ends_with(".zip.001") {
        Some(ArchiveKind::Zip)
    } else if name.ends_with(".7z") || name.ends_with(".7z.001") {
        Some(ArchiveKind::SevenZip)
    } else if name.ends_with(".rar") {
        Some(ArchiveKind::Rar)
    } else {
        None
    }
}

/// A relative path that stays inside the destination.
fn is_safe_relative(p: &Path) -> bool {
    !p.as_os_str().is_empty()
        && p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

pub fn extract(archive: &Path, dest: &Path) -> InstallResult<usize> {
    fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let kind = archive_kind(archive).ok_or("This file isn't a zip, 7z or rar archive.")?;
    match kind {
        ArchiveKind::Zip => extract_zip(archive, dest),
        ArchiveKind::SevenZip => extract_7z(archive, dest),
        ArchiveKind::Rar => extract_rar(archive, dest),
    }
}

fn extract_zip(archive: &Path, dest: &Path) -> InstallResult<usize> {
    let file = fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Couldn't read the zip: {e}"))?;
    let mut written = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name().filter(|p| is_safe_relative(p)) else {
            log::warn!("skipping unsafe zip entry {:?}", entry.name());
            continue;
        };
        if entry.is_symlink() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut f = fs::File::create(&out).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut f).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

fn extract_7z(archive: &Path, dest: &Path) -> InstallResult<usize> {
    let mut written = 0;
    let root = dest.to_path_buf();
    sevenz_rust2::decompress_file_with_extract_fn(archive, dest, |entry, reader, out| {
        let rel = out.strip_prefix(&root).unwrap_or(out);
        if !is_safe_relative(rel) {
            log::warn!("skipping unsafe 7z entry {:?}", entry.name());
            io::copy(reader, &mut io::sink())?;
            return Ok(true);
        }
        if !entry.is_directory() {
            written += 1;
        }
        sevenz_rust2::default_entry_extract_fn(entry, reader, out)
    })
    .map_err(|e| format!("Couldn't read the 7z archive: {e}"))?;
    Ok(written)
}

fn extract_rar(archive: &Path, dest: &Path) -> InstallResult<usize> {
    let mut written = 0;
    let mut open = unrar::Archive::new(archive)
        .open_for_processing()
        .map_err(|e| format!("Couldn't read the rar archive: {e}"))?;
    while let Some(header) = open.read_header().map_err(|e| e.to_string())? {
        let entry = header.entry();
        let safe = is_safe_relative(&entry.filename);
        let is_file = entry.is_file();
        open = if safe {
            if is_file {
                written += 1;
            }
            header.extract_with_base(dest).map_err(|e| e.to_string())?
        } else {
            log::warn!("skipping unsafe rar entry {:?}", entry.filename);
            header.skip().map_err(|e| e.to_string())?
        };
    }
    Ok(written)
}

/// Archives usually wrap the game in one folder; returns that folder.
pub fn game_root(staging: &Path) -> PathBuf {
    let mut cur = staging.to_path_buf();
    for _ in 0..4 {
        let Ok(rd) = fs::read_dir(&cur) else { break };
        let entries: Vec<_> = rd
            .filter_map(Result::ok)
            .filter(|e| {
                let n = e.file_name().to_string_lossy().to_lowercase();
                n != "__macosx" && n != ".ds_store"
            })
            .collect();
        if entries.len() == 1 && entries[0].file_type().is_ok_and(|t| t.is_dir()) {
            cur = entries[0].path();
        } else {
            break;
        }
    }
    cur
}

/// A name that's safe as one Windows folder name.
pub fn folder_name(s: &str, fallback: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let mut name = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    name = name.trim_matches(|c: char| c == '.' || c == ' ').to_string();
    if name.chars().count() > 80 {
        name = name.chars().take(80).collect::<String>().trim_end().to_string();
    }
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3",
    ];
    if name.is_empty() || RESERVED.contains(&name.to_lowercase().as_str()) {
        fallback.to_string()
    } else {
        name
    }
}

/// `{library}/{creator}/{title}/{version}`, never overwriting a folder.
pub fn destination(library: &Path, creator: &str, title: &str, version: &str) -> PathBuf {
    let base = library
        .join(folder_name(creator, "Unknown creator"))
        .join(folder_name(title, "Untitled"))
        .join(folder_name(version.trim_start_matches(['v', 'V']), "current"));
    if !base.exists() {
        return base;
    }
    (2..1000)
        .map(|n| PathBuf::from(format!("{} ({n})", base.display())))
        .find(|p| !p.exists())
        .unwrap_or(base)
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<u64> {
    let mut n = 0;
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        let ft = e.file_type()?;
        let dest = to.join(e.file_name());
        if ft.is_dir() {
            n += copy_dir(&e.path(), &dest)?;
        } else if ft.is_file() {
            fs::copy(e.path(), &dest)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Moves a folder, falling back to copy + delete across drives.
pub fn move_dir(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)?;
    }
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir(from, to)?;
            fs::remove_dir_all(from)
        }
    }
}

/// Save folders that live inside the game folder, by engine.
const SAVE_DIRS: &[&str] = &["game/saves", "www/save", "save", "Saves"];

/// Copies saves from the previous version into the new one, when the new
/// one doesn't already have them. Ren'Py also keeps a copy under
/// %APPDATA%\RenPy, which carries over by itself.
pub fn carry_saves(old: &Path, new: &Path) -> Vec<String> {
    let mut copied = Vec::new();
    for rel in SAVE_DIRS {
        let src = old.join(rel);
        let dst = new.join(rel);
        let has_files = |p: &Path| fs::read_dir(p).is_ok_and(|mut r| r.next().is_some());
        if src.is_dir() && has_files(&src) && !has_files(&dst) && copy_dir(&src, &dst).is_ok() {
            copied.push(rel.to_string());
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f95gm-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let f = fs::File::create(path).unwrap();
        let mut z = zip::ZipWriter::new(f);
        let opts: zip::write::SimpleFileOptions = Default::default();
        for (name, data) in entries {
            z.start_file(*name, opts).unwrap();
            z.write_all(data).unwrap();
        }
        z.finish().unwrap();
    }

    #[test]
    fn extracts_zip_and_skips_escaping_entries() {
        let d = tmp("zip");
        let archive = d.join("game.zip");
        make_zip(
            &archive,
            &[
                ("Game-0.9-pc/Game.exe", b"exe"),
                ("Game-0.9-pc/game/script.rpy", b"x"),
                ("../evil.txt", b"nope"),
            ],
        );
        let staging = d.join("staging");
        let n = extract(&archive, &staging).unwrap();
        assert_eq!(n, 2);
        assert!(!d.join("evil.txt").exists());
        let root = game_root(&staging);
        assert!(root.ends_with("Game-0.9-pc"));
        assert!(root.join("Game.exe").is_file());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn rejects_non_archives() {
        assert!(archive_kind(Path::new("a/setup.exe")).is_none());
        assert_eq!(archive_kind(Path::new("A.PART1.RAR")), Some(ArchiveKind::Rar));
        assert_eq!(archive_kind(Path::new("x.7z.001")), Some(ArchiveKind::SevenZip));
        assert!(!is_safe_relative(Path::new("../x")));
        assert!(!is_safe_relative(Path::new("C:\\x")));
        assert!(is_safe_relative(Path::new("a/b.txt")));
    }

    #[test]
    fn folder_names_are_windows_safe() {
        assert_eq!(folder_name("What: A <Game>?", "x"), "What A Game");
        assert_eq!(folder_name("  ...  ", "fallback"), "fallback");
        assert_eq!(folder_name("CON", "fallback"), "fallback");
        let lib = tmp("dest");
        let first = destination(&lib, "Dev/Studio", "Game", "v0.9");
        assert!(first.ends_with("Dev Studio/Game/0.9") || first.ends_with("Dev Studio\\Game\\0.9"));
        fs::create_dir_all(&first).unwrap();
        let second = destination(&lib, "Dev/Studio", "Game", "v0.9");
        assert!(second.to_string_lossy().ends_with("0.9 (2)"));
        let _ = fs::remove_dir_all(lib);
    }

    #[test]
    fn carries_saves_forward() {
        let d = tmp("saves");
        let old = d.join("old");
        let new = d.join("new");
        fs::create_dir_all(old.join("game/saves")).unwrap();
        fs::write(old.join("game/saves/1-1-LT1.save"), b"s").unwrap();
        fs::create_dir_all(new.join("game")).unwrap();
        assert_eq!(carry_saves(&old, &new), vec!["game/saves"]);
        assert!(new.join("game/saves/1-1-LT1.save").is_file());
        // Never overwrites saves the new version already has.
        assert!(carry_saves(&old, &new).is_empty());
        let _ = fs::remove_dir_all(d);
    }
}
