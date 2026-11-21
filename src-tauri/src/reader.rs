//! Comic/gallery reader backend.
//!
//! A "book" is what the reader opens: chapters of pages, built from however
//! the user's files are laid out:
//! - a folder of images (one chapter)
//! - chapter subfolders (Chapter 1/, Chapter 2/ …), one level or two deep
//! - comic archives (.cbz/.zip read in place; .cbr/.rar and .cb7/.7z are
//!   unpacked once into a small cache, since they can't be read page by
//!   page cheaply)
//! - a single archive file
//!
//! Pages are served to the webview through the `gmmedia://` scheme, which
//! only answers for files inside books that were opened (no other path on
//! disk is reachable).

use std::cmp::Ordering;
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::Serialize;
use tauri::http::{Response, StatusCode, header};

use crate::media::{is_comic_file, is_image};

/// Natural order: "page2" before "page10", case-insensitive.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek(), bi.peek()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().filter(|c| c.is_ascii_digit()) {
                        s.push(*c);
                        it.next();
                    }
                    s
                };
                let (na, nb) = (take(&mut ai), take(&mut bi));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_ascii_lowercase(), y.to_ascii_lowercase());
                if lx != ly {
                    return lx.cmp(&ly);
                }
                ai.next();
                bi.next();
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Page {
    /// Path for the `gmmedia://` scheme (the UI prefixes the scheme).
    pub src: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Chapter {
    pub name: String,
    pub pages: Vec<Page>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub struct Book {
    pub root: String,
    pub chapters: Vec<Chapter>,
    /// Files the reader can't show yet (PDF), to open with another app.
    pub other_files: Vec<String>,
}

// Paths travel base64url-encoded so no web engine can rewrite separators.
pub(crate) fn enc(s: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(s)
}

fn dec(s: &str) -> Option<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .ok()?;
    String::from_utf8(bytes).ok()
}

fn file_src(p: &Path) -> String {
    format!("/f/{}", enc(&p.to_string_lossy()))
}

fn zip_src(archive: &Path, entry: &str) -> String {
    format!("/z/{}/{}", enc(&archive.to_string_lossy()), enc(entry))
}

fn name_of(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn sorted_children(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (Vec::new(), Vec::new());
    };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in rd.filter_map(Result::ok) {
        let p = e.path();
        let n = name_of(&p);
        if n.starts_with('.') || n.eq_ignore_ascii_case("__macosx") {
            continue;
        }
        match e.file_type() {
            Ok(t) if t.is_dir() => dirs.push(p),
            Ok(t) if t.is_file() => files.push(p),
            _ => {}
        }
    }
    let by_name = |a: &PathBuf, b: &PathBuf| natural_cmp(&name_of(a), &name_of(b));
    dirs.sort_by(by_name);
    files.sort_by(by_name);
    (dirs, files)
}

fn image_pages(files: &[PathBuf]) -> Vec<Page> {
    files
        .iter()
        .filter(|p| is_image(p))
        .map(|p| Page {
            src: file_src(p),
            name: name_of(p),
        })
        .collect()
}

/// Chapters inside a zip: grouped by folder when the archive has several.
fn zip_chapters(archive: &Path, fallback_name: &str) -> Result<Vec<Chapter>, String> {
    let f = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut z =
        zip::ZipArchive::new(f).map_err(|e| format!("Couldn't read {}: {e}", name_of(archive)))?;
    let mut entries: Vec<String> = (0..z.len())
        .filter_map(|i| {
            z.by_index(i)
                .ok()
                .and_then(|e| Some((e.name().ok()?.to_string(), e.is_dir())))
        })
        .filter(|(n, dir)| !dir && is_image(Path::new(n)) && !n.starts_with("__MACOSX"))
        .map(|(n, _)| n)
        .collect();
    entries.sort_by(|a, b| natural_cmp(a, b));
    let mut chapters: Vec<Chapter> = Vec::new();
    for e in entries {
        let parent = Path::new(&e)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let name = if parent.is_empty() {
            fallback_name.to_string()
        } else {
            name_of(Path::new(&parent))
        };
        let page = Page {
            src: zip_src(archive, &e),
            name: name_of(Path::new(&e)),
        };
        match chapters.last_mut() {
            Some(c) if c.name == name => c.pages.push(page),
            _ => chapters.push(Chapter {
                name,
                pages: vec![page],
            }),
        }
    }
    Ok(chapters)
}

fn is_zip(p: &Path) -> bool {
    let e = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase();
    e == "cbz" || e == "zip"
}

fn is_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

/// Unpacks a .cbr/.rar/.cb7/.7z into the cache once (keyed by path, size
/// and modified time) and returns the folder.
fn unpacked(archive: &Path, cache: &Path) -> Result<PathBuf, String> {
    use sha2::{Digest, Sha256};
    let meta = std::fs::metadata(archive).map_err(|e| e.to_string())?;
    let mut h = Sha256::new();
    h.update(archive.to_string_lossy().as_bytes());
    h.update(meta.len().to_le_bytes());
    if let Ok(t) = meta.modified().and_then(|t| {
        t.duration_since(std::time::UNIX_EPOCH)
            .map_err(std::io::Error::other)
    }) {
        h.update(t.as_secs().to_le_bytes());
    }
    let key: String = h
        .finalize()
        .iter()
        .take(12)
        .map(|b| format!("{b:02x}"))
        .collect();
    let dir = cache.join(key);
    if dir.join(".done").is_file() {
        // Mark as recently used for the cache pruning.
        let _ = std::fs::write(dir.join(".done"), b"");
        return Ok(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    // The installer's extractor already refuses entries that escape.
    crate::install::extract(archive, &dir.join("pages"))?;
    std::fs::write(dir.join(".done"), b"").map_err(|e| e.to_string())?;
    prune_cache(cache, 8);
    Ok(dir)
}

/// Keeps the `keep` most recently used unpacked archives.
fn prune_cache(cache: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(cache) else {
        return;
    };
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .map(|p| {
            (
                std::fs::metadata(p.join(".done"))
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH),
                p,
            )
        })
        .collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.0));
    for (_, d) in dirs.into_iter().skip(keep) {
        let _ = std::fs::remove_dir_all(d);
    }
}

/// Pages of an unpacked folder, grouped by subfolder like a normal folder.
fn folder_chapters(
    dir: &Path,
    name: &str,
    depth: usize,
    out: &mut Vec<Chapter>,
    other: &mut Vec<String>,
    cache: &Path,
    roots: &mut Vec<PathBuf>,
) {
    let (dirs, files) = sorted_children(dir);
    let pages = image_pages(&files);
    if !pages.is_empty() {
        out.push(Chapter {
            name: name.to_string(),
            pages,
        });
    }
    for f in files.iter().filter(|p| is_comic_file(p)) {
        file_chapters(f, out, other, cache, roots);
    }
    if depth > 0 {
        for d in dirs {
            folder_chapters(&d, &name_of(&d), depth - 1, out, other, cache, roots);
        }
    }
}

fn file_chapters(
    f: &Path,
    out: &mut Vec<Chapter>,
    other: &mut Vec<String>,
    cache: &Path,
    roots: &mut Vec<PathBuf>,
) {
    let stem = f
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string();
    if is_pdf(f) {
        other.push(f.to_string_lossy().into_owned());
    } else if is_zip(f) {
        match zip_chapters(f, &stem) {
            Ok(mut ch) => out.append(&mut ch),
            Err(e) => log::warn!("{e}"),
        }
    } else {
        match unpacked(f, cache) {
            Ok(dir) => {
                roots.push(dir.clone());
                let mut inner = Vec::new();
                folder_chapters(
                    &dir.join("pages"),
                    &stem,
                    2,
                    &mut inner,
                    other,
                    cache,
                    roots,
                );
                // A single chapter from an archive takes the archive's name.
                if inner.len() == 1 {
                    inner[0].name = stem;
                }
                out.append(&mut inner);
            }
            Err(e) => log::warn!("couldn't open {}: {e}", f.display()),
        }
    }
}

/// Builds the book for a folder or file. Returns the book and the paths
/// the media scheme may serve from.
pub fn build(root: &Path, cache: &Path) -> Result<(Book, Vec<PathBuf>), String> {
    if !root.exists() {
        return Err("The folder isn't there any more. It may have been moved or deleted.".into());
    }
    let mut chapters = Vec::new();
    let mut other = Vec::new();
    let mut roots = vec![root.to_path_buf()];
    if root.is_file() {
        file_chapters(root, &mut chapters, &mut other, cache, &mut roots);
    } else {
        folder_chapters(
            root,
            &name_of(root),
            2,
            &mut chapters,
            &mut other,
            cache,
            &mut roots,
        );
    }
    Ok((
        Book {
            root: root.to_string_lossy().into_owned(),
            chapters,
            other_files: other,
        },
        roots,
    ))
}

// ------------------------------------------------------------- serving

/// Folders/files the reader opened; the scheme serves nothing else.
#[derive(Default)]
pub struct MediaRoots(RwLock<HashSet<PathBuf>>);

impl MediaRoots {
    pub fn allow(&self, paths: impl IntoIterator<Item = PathBuf>) {
        let mut set = self.0.write().unwrap_or_else(|p| p.into_inner());
        for p in paths {
            if let Ok(c) = std::fs::canonicalize(&p) {
                set.insert(c);
            }
        }
    }

    /// `p` (canonicalised, so `..` and links are resolved) is inside an
    /// allowed folder, or is an allowed file.
    pub fn permits(&self, p: &Path) -> Option<PathBuf> {
        let c = std::fs::canonicalize(p).ok()?;
        let set = self.0.read().unwrap_or_else(|p| p.into_inner());
        set.iter().any(|r| c == *r || c.starts_with(r)).then_some(c)
    }
}

const MAX_PAGE_BYTES: u64 = 64 * 1024 * 1024;

fn content_type(name: &str) -> &'static str {
    match Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "jxl" => "image/jxl",
        _ => "application/octet-stream",
    }
}

fn respond(status: StatusCode, ctype: &str, body: Vec<u8>) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, ctype)
        .header(header::CACHE_CONTROL, "max-age=3600")
        .body(body)
        .unwrap()
}

fn not_found() -> Response<Vec<u8>> {
    respond(StatusCode::NOT_FOUND, "text/plain", b"not found".to_vec())
}

/// The most a single video response carries; the player asks for more.
const VIDEO_CHUNK: u64 = 4 * 1024 * 1024;

fn video_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or_default().to_lowercase().as_str() {
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "ogv" => "video/ogg",
        // mp4, m4v and mov (H.264 in a QuickTime box plays as mp4)
        _ => "video/mp4",
    }
}

/// `bytes=a-b`, `bytes=a-` or `bytes=-n` as an inclusive range.
fn parse_range(header: &str, len: u64) -> Option<(u64, u64)> {
    let spec = header.trim().strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        return (n > 0 && len > 0).then(|| (len.saturating_sub(n), len - 1));
    }
    let start: u64 = a.parse().ok()?;
    if start >= len {
        return None;
    }
    let end = if b.is_empty() { len - 1 } else { b.parse::<u64>().ok()?.min(len - 1) };
    (end >= start).then_some((start, end))
}

/// A slice of a video file, so seeking never loads the whole file.
fn serve_video(p: &Path, range: Option<&str>) -> Response<Vec<u8>> {
    use std::io::{Seek, SeekFrom};
    let Ok(mut f) = std::fs::File::open(p) else { return not_found() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let (start, end) = match range.map(|h| parse_range(h, len)) {
        Some(Some(r)) => r,
        Some(None) => {
            return Response::builder()
                .status(StatusCode::RANGE_NOT_SATISFIABLE)
                .header(header::CONTENT_RANGE, format!("bytes */{len}"))
                .body(Vec::new())
                .unwrap();
        }
        None if len == 0 => return respond(StatusCode::OK, video_type(p), Vec::new()),
        None => (0, len - 1),
    };
    let end = end.min(start + VIDEO_CHUNK - 1);
    let mut buf = vec![0; (end - start + 1) as usize];
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_exact(&mut buf).is_err() {
        return not_found();
    }
    Response::builder()
        .status(StatusCode::PARTIAL_CONTENT)
        .header(header::CONTENT_TYPE, video_type(p))
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_RANGE, format!("bytes {start}-{end}/{len}"))
        .header(header::CONTENT_LENGTH, buf.len().to_string())
        .header(header::CACHE_CONTROL, "no-store")
        .body(buf)
        .unwrap()
}

/// Answers `/f/<path>` (an image), `/z/<archive>/<entry>` (an image in a
/// zip) and `/v/<path>` (a video, in ranges).
pub fn serve(roots: &MediaRoots, request_path: &str, range: Option<&str>) -> Response<Vec<u8>> {
    let mut parts = request_path.trim_start_matches('/').splitn(3, '/');
    let kind = parts.next().unwrap_or_default();
    let dec = |s: Option<&str>| s.and_then(dec);
    match kind {
        "f" => {
            let Some(path) = dec(parts.next()) else {
                return not_found();
            };
            let Some(p) = roots.permits(Path::new(&path)) else {
                return respond(StatusCode::FORBIDDEN, "text/plain", b"not allowed".to_vec());
            };
            if !is_image(&p)
                || std::fs::metadata(&p).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_PAGE_BYTES
            {
                return not_found();
            }
            match std::fs::read(&p) {
                Ok(bytes) => respond(StatusCode::OK, content_type(&p.to_string_lossy()), bytes),
                Err(_) => not_found(),
            }
        }
        "v" => {
            let Some(path) = dec(parts.next()) else {
                return not_found();
            };
            let Some(p) = roots.permits(Path::new(&path)) else {
                return respond(StatusCode::FORBIDDEN, "text/plain", b"not allowed".to_vec());
            };
            if !crate::media::is_video(&p) {
                return not_found();
            }
            serve_video(&p, range)
        }
        "z" => {
            let (Some(archive), Some(entry)) = (dec(parts.next()), dec(parts.next())) else {
                return not_found();
            };
            let Some(a) = roots.permits(Path::new(&archive)) else {
                return respond(StatusCode::FORBIDDEN, "text/plain", b"not allowed".to_vec());
            };
            let read = || -> Option<Vec<u8>> {
                let mut z = zip::ZipArchive::new(std::fs::File::open(&a).ok()?).ok()?;
                let mut e = z.by_name(&entry).ok()?;
                if e.size() > MAX_PAGE_BYTES || !is_image(Path::new(&entry)) {
                    return None;
                }
                let mut buf = Vec::with_capacity(e.size() as usize);
                e.read_to_end(&mut buf).ok()?;
                Some(buf)
            };
            match read() {
                Some(bytes) => respond(StatusCode::OK, content_type(&entry), bytes),
                None => not_found(),
            }
        }
        _ => not_found(),
    }
}

// ------------------------------------------------------------- commands

use std::sync::Arc;

use tauri::{AppHandle, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::api::error::ApiError;
use crate::db::{ContinueItem, Db, ReadingProgress};

fn internal(message: impl Into<String>) -> ApiError {
    ApiError::Internal {
        message: message.into(),
    }
}

fn invalid(message: impl Into<String>) -> ApiError {
    ApiError::Invalid {
        message: message.into(),
    }
}

#[derive(Debug, Serialize)]
pub struct OpenedBook {
    pub book: Book,
    pub progress: Option<ReadingProgress>,
}

/// Opens a comic from the library: builds its book and lets the media
/// scheme serve its pages.
#[tauri::command]
pub async fn media_book(
    app: AppHandle,
    db: State<'_, Arc<Db>>,
    roots: State<'_, Arc<MediaRoots>>,
    id: i64,
) -> Result<OpenedBook, ApiError> {
    let local = db.get(id).map_err(internal)?;
    let dir = local.and_then(|g| g.install_dir).ok_or_else(|| {
        invalid("This comic has no folder yet. Download it or choose its folder first.")
    })?;
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| internal(e.to_string()))?
        .join("reader");
    let (book, allowed) =
        tauri::async_runtime::spawn_blocking(move || build(Path::new(&dir), &cache))
            .await
            .map_err(|e| internal(e.to_string()))?
            .map_err(invalid)?;
    roots.allow(allowed);
    let progress = db.progress_get(id).map_err(internal)?;
    Ok(OpenedBook { book, progress })
}

#[tauri::command]
pub fn media_progress(
    db: State<'_, Arc<Db>>,
    id: i64,
) -> Result<Option<ReadingProgress>, ApiError> {
    db.progress_get(id).map_err(internal)
}

#[tauri::command]
pub fn media_progress_set(
    db: State<'_, Arc<Db>>,
    progress: ReadingProgress,
) -> Result<(), ApiError> {
    db.progress_set(&progress).map_err(internal)
}

#[tauri::command]
pub fn media_continue(
    db: State<'_, Arc<Db>>,
    limit: Option<i64>,
) -> Result<Vec<ContinueItem>, ApiError> {
    db.continue_reading(limit.unwrap_or(12).clamp(1, 50))
        .map_err(internal)
}

/// Opens a file the reader can't show (a PDF) in the default app. Only
/// files inside a book that was opened.
#[tauri::command]
pub fn media_open_external(
    app: AppHandle,
    roots: State<'_, Arc<MediaRoots>>,
    path: String,
) -> Result<(), ApiError> {
    let p = roots
        .permits(Path::new(&path))
        .ok_or_else(|| invalid("That file isn't part of an open comic."))?;
    app.opener()
        .open_path(p.to_string_lossy(), None::<&str>)
        .map_err(|e| invalid(format!("Couldn't open it: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f95gm-reader-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"img").unwrap();
    }

    #[test]
    fn sorts_naturally() {
        let mut v = vec![
            "page10.jpg",
            "Page2.jpg",
            "page1.jpg",
            "page02b.jpg",
            "cover.jpg",
        ];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(
            v,
            vec![
                "cover.jpg",
                "page1.jpg",
                "Page2.jpg",
                "page02b.jpg",
                "page10.jpg"
            ]
        );
    }

    #[test]
    fn builds_chapters_from_folders_and_zips() {
        let d = tmp("book");
        touch(&d.join("Chapter 10/01.jpg"));
        touch(&d.join("Chapter 2/02.png"));
        touch(&d.join("Chapter 2/01.png"));
        touch(&d.join("notes.txt"));
        touch(&d.join("extra.pdf"));
        let zip_path = d.join("Bonus.cbz");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let o: zip::write::SimpleFileOptions = Default::default();
            for n in ["p2.jpg", "p10.jpg", "p1.jpg", "readme.txt"] {
                z.start_file(n, o).unwrap();
                z.write_all(b"x").unwrap();
            }
            z.finish().unwrap();
        }
        let (book, roots) = build(&d, &d.join(".cache")).unwrap();
        let names: Vec<_> = book.chapters.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Bonus", "Chapter 2", "Chapter 10"]);
        assert_eq!(
            book.chapters[0]
                .pages
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["p1.jpg", "p2.jpg", "p10.jpg"]
        );
        assert_eq!(book.chapters[1].pages[0].name, "01.png");
        assert_eq!(book.other_files.len(), 1);
        assert!(roots.contains(&d));

        // Serving: allowed pages load, anything else is refused.
        let mr = MediaRoots::default();
        mr.allow(roots);
        let ok = serve(&mr, &book.chapters[1].pages[0].src, None);
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/png");
        let zipped = serve(&mr, &book.chapters[0].pages[0].src, None);
        assert_eq!(zipped.status(), StatusCode::OK);
        let outside =
            std::env::temp_dir().join(format!("f95gm-outside-{}.jpg", std::process::id()));
        fs::write(&outside, b"secret").unwrap();
        assert_eq!(
            serve(&mr, &file_src(&outside), None).status(),
            StatusCode::FORBIDDEN
        );
        let sneaky = format!(
            "/f/{}",
            enc(&d
                .join("..")
                .join(outside.file_name().unwrap())
                .to_string_lossy())
        );
        assert_ne!(
            serve(&mr, &sneaky, None).status(),
            StatusCode::OK,
            "no escaping with .."
        );
        assert_eq!(
            serve(&mr, &file_src(&d.join("notes.txt")), None).status(),
            StatusCode::NOT_FOUND,
            "only images"
        );
        let _ = fs::remove_file(outside);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn saves_progress() {
        let db = Db::in_memory();
        assert_eq!(db.progress_get(7).unwrap(), None);
        let mut p = ReadingProgress {
            thread_id: 7,
            chapter: 1,
            page: 12,
            pages_total: 40,
            mode: "double".into(),
            rtl: true,
            fit: "width".into(),
            updated_at: 0,
        };
        db.progress_set(&p).unwrap();
        p.page = 13;
        db.progress_set(&p).unwrap();
        let got = db.progress_get(7).unwrap().unwrap();
        assert_eq!((got.page, got.mode.as_str(), got.rtl), (13, "double", true));
        assert!(got.updated_at > 0);
    }

    #[test]
    fn single_archive_is_a_book() {
        let d = tmp("single");
        let zip_path = d.join("Vol 1.cbz");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
            let o: zip::write::SimpleFileOptions = Default::default();
            for n in ["Ch1/1.jpg", "Ch1/2.jpg", "Ch2/1.jpg"] {
                z.start_file(n, o).unwrap();
                z.write_all(b"x").unwrap();
            }
            z.finish().unwrap();
        }
        let (book, _) = build(&zip_path, &d.join(".cache")).unwrap();
        assert_eq!(
            book.chapters
                .iter()
                .map(|c| (c.name.as_str(), c.pages.len()))
                .collect::<Vec<_>>(),
            vec![("Ch1", 2), ("Ch2", 1)]
        );
        let _ = fs::remove_dir_all(d);
    }
}
