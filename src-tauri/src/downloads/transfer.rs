//! Streams one file to disk, resuming a `.part` file when one exists.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::{StatusCode, header};
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransferError {
    /// The host said "too many" (429/403/quota page). Back off from it.
    #[error("{0} asked us to slow down")]
    HostLimited(String),
    /// Connection problems: worth retrying later.
    #[error("network problem: {0}")]
    Network(String),
    /// Won't work by retrying (file gone, disk full, bad link).
    #[error("{0}")]
    Fatal(String),
    /// The host sent a web page instead of the file: someone has to press
    /// its download button (in the F95 window).
    #[error("the host wants you to download from its page")]
    NeedsBrowser,
    #[error("stopped")]
    Stopped,
}

pub struct Progress {
    pub done: u64,
    pub total: Option<u64>,
}

fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

/// File name from Content-Disposition, if the host sent one.
fn disposition_name(h: &header::HeaderMap) -> Option<String> {
    let v = h.get(header::CONTENT_DISPOSITION)?.to_str().ok()?;
    if let Some(i) = v.find("filename*=") {
        let raw = v[i + 10..].split(';').next()?.trim();
        let raw = raw.split("''").nth(1).unwrap_or(raw);
        return urlencoding::decode(raw.trim_matches('"')).ok().map(|s| s.into_owned());
    }
    let i = v.find("filename=")?;
    Some(v[i + 9..].split(';').next()?.trim().trim_matches('"').to_string())
}

/// Strips anything that could make the name a path.
pub fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let cleaned: String = base
        .chars()
        .map(|c| if "<>:\"|?*".contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let cleaned = cleaned.trim().trim_matches('.').to_string();
    if cleaned.is_empty() { "download".into() } else { cleaned }
}

/// Downloads `url` into `dir`. Returns the finished file's path.
#[allow(clippy::too_many_arguments)]
pub async fn download(
    http: &reqwest::Client,
    url: &str,
    dir: &Path,
    file_name: Option<&str>,
    expected_size: Option<u64>,
    host_label: &str,
    auth: &super::hosts::Auth,
    stop: Arc<AtomicBool>,
    mut on_progress: impl FnMut(Progress),
) -> Result<PathBuf, TransferError> {
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| TransferError::Fatal(format!("Couldn't create the download folder: {e}")))?;

    // Resume: whatever `.part` exists for a known name.
    let known = file_name.map(safe_file_name);
    let mut offset = 0u64;
    if let Some(n) = &known
        && let Ok(m) = tokio::fs::metadata(part_path(&dir.join(n))).await {
            offset = m.len();
        }

    let mut req = http.get(url);
    match auth {
        super::hosts::Auth::None => {}
        super::hosts::Auth::Basic(key) => req = req.basic_auth("", Some(key)),
        super::hosts::Auth::Gofile(token) => {
            req = req
                .bearer_auth(token)
                .header(header::COOKIE, format!("accountToken={token}"));
        }
    }
    if offset > 0 {
        req = req.header(header::RANGE, format!("bytes={offset}-"));
    }
    let resp = req.send().await.map_err(|e| TransferError::Network(e.to_string()))?;
    let status = resp.status();
    match status {
        StatusCode::TOO_MANY_REQUESTS | StatusCode::FORBIDDEN => {
            return Err(TransferError::HostLimited(host_label.to_string()));
        }
        StatusCode::NOT_FOUND | StatusCode::GONE => {
            return Err(TransferError::Fatal("The file isn't on the host any more.".into()));
        }
        StatusCode::RANGE_NOT_SATISFIABLE => offset = 0,
        s if s.is_server_error() => return Err(TransferError::Network(format!("the host answered {s}"))),
        s if !s.is_success() => return Err(TransferError::Fatal(format!("the host answered {s}"))),
        _ => {}
    }
    // A web page instead of the file: the link is a download page (or a
    // captcha). Not a reason to back off from the host; a person finishes it.
    let is_html = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if is_html {
        return Err(TransferError::NeedsBrowser);
    }

    let resumed = status == StatusCode::PARTIAL_CONTENT && offset > 0;
    if !resumed {
        offset = 0;
    }
    let name = known
        .or_else(|| disposition_name(resp.headers()).map(|n| safe_file_name(&n)))
        .unwrap_or_else(|| "download".into());
    let dest = dir.join(&name);
    let part = part_path(&dest);
    let total = resp.content_length().map(|n| n + offset).or(expected_size);

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(&part)
        .await
        .map_err(|e| TransferError::Fatal(format!("Couldn't write the file: {e}")))?;

    let mut done = offset;
    let mut last = Instant::now() - Duration::from_secs(1);
    let mut resp = resp;
    loop {
        if stop.load(Ordering::SeqCst) {
            let _ = file.flush().await;
            return Err(TransferError::Stopped);
        }
        let chunk = match tokio::time::timeout(Duration::from_secs(60), resp.chunk()).await {
            Err(_) => return Err(TransferError::Network("the connection stalled".into())),
            Ok(Err(e)) => return Err(TransferError::Network(e.to_string())),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(c))) => c,
        };
        file.write_all(&chunk).await.map_err(|e| {
            TransferError::Fatal(if e.raw_os_error() == Some(112) {
                "The disk is full.".into()
            } else {
                format!("Couldn't write the file: {e}")
            })
        })?;
        done += chunk.len() as u64;
        if last.elapsed() >= Duration::from_millis(500) {
            last = Instant::now();
            on_progress(Progress { done, total });
        }
    }
    file.flush().await.map_err(|e| TransferError::Fatal(e.to_string()))?;
    drop(file);
    on_progress(Progress { done, total });

    if let Some(t) = total
        && done != t {
            return Err(TransferError::Network(format!(
                "the download ended early ({done} of {t} bytes)"
            )));
        }
    let _ = tokio::fs::remove_file(&dest).await;
    tokio::fs::rename(&part, &dest)
        .await
        .map_err(|e| TransferError::Fatal(format!("Couldn't finish the file: {e}")))?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header as h, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("f95gm-dl-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn file_names_cannot_escape() {
        assert_eq!(safe_file_name("../../evil.exe"), "evil.exe");
        assert_eq!(safe_file_name("C:\\Windows\\x.zip"), "x.zip");
        assert_eq!(safe_file_name("a:b?.zip"), "a_b_.zip");
        assert_eq!(safe_file_name(".."), "download");
    }

    #[tokio::test]
    async fn downloads_and_resumes() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/f.zip"))
            .and(h("range", "bytes=4-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(b"56789".to_vec()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tmp("resume");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("f.zip.part"), b"1234").unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let out = download(
            &reqwest::Client::new(),
            &format!("{}/f.zip", server.uri()),
            &dir,
            Some("f.zip"),
            None,
            "test",
            &super::super::hosts::Auth::None,
            stop,
            |_| {},
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"123456789");
        assert!(!dir.join("f.zip.part").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[tokio::test]
    async fn host_pushback_is_a_limit_and_html_needs_the_browser() {
        let server = MockServer::start().await;
        Mock::given(path("/busy"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;
        Mock::given(path("/page"))
            .respond_with(ResponseTemplate::new(200).set_body_raw("<html>captcha</html>", "text/html"))
            .mount(&server)
            .await;
        let http = reqwest::Client::new();
        for (p, want) in [("busy", TransferError::HostLimited("Host".into())), ("page", TransferError::NeedsBrowser)] {
            let e = download(
                &http,
                &format!("{}/{p}", server.uri()),
                &tmp(p),
                None,
                None,
                "Host",
                &super::super::hosts::Auth::None,
                Arc::new(AtomicBool::new(false)),
                |_| {},
            )
            .await
            .unwrap_err();
            assert_eq!(e, want);
        }
    }
}
