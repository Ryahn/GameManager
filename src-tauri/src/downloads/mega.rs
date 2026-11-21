//! MEGA downloads through MEGA's API (via the `mega` crate): the file is
//! fetched encrypted and decrypted locally with the key from the link, and
//! its checksum is verified at the end.
//!
//! Signing in is optional and only raises the transfer quota. The password
//! is used once; only the session string is kept (Windows Credential
//! Manager).

use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

use super::transfer::{Progress, TransferError, safe_file_name};

const STOPPED: &str = "f95gm-stopped";

/// Turns the old `#!id!key` / `#F!id!key` and mega.co.nz forms into the
/// `https://mega.nz/file/id#key` form. `None` if it isn't a MEGA link with
/// a key (without the key the file can't be decrypted).
pub fn normalize(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.trim_start_matches("www.");
    if host != "mega.nz" && host != "mega.co.nz" && host != "mega.io" {
        return None;
    }
    let frag = u.fragment().unwrap_or("");
    let path = u.path();
    let (kind, id, key) = if let Some(rest) = frag.strip_prefix("F!") {
        let (id, key) = rest.split_once('!')?;
        ("folder", id, key)
    } else if let Some(rest) = frag.strip_prefix('!') {
        let (id, key) = rest.split_once('!')?;
        ("file", id, key)
    } else if let Some(id) = path.strip_prefix("/file/") {
        ("file", id.trim_end_matches('/'), frag)
    } else {
        let id = path.strip_prefix("/folder/")?;
        ("folder", id.split('/').next()?, frag.split('/').next().unwrap_or(""))
    };
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    (ok(id) && ok(key)).then(|| format!("https://mega.nz/{kind}/{id}#{key}"))
}

fn map_err(e: mega::Error) -> TransferError {
    use mega::ErrorCode as C;
    match e {
        mega::Error::MegaError { code } => match code {
            C::EOVERQUOTA | C::EGOINGOVERQUOTA | C::ERATELIMIT | C::ETOOMANY | C::ETOOMANYCONNECTIONS => {
                TransferError::HostLimited("MEGA".into())
            }
            C::ENOENT | C::EBLOCKED | C::EACCESS => TransferError::Fatal("The file was removed from MEGA.".into()),
            C::EKEY => TransferError::Fatal("The MEGA link's key is wrong or missing.".into()),
            C::EAGAIN | C::ETEMPUNAVAIL | C::EINTERNAL => TransferError::Network(format!("MEGA is busy ({code})")),
            other => TransferError::Fatal(format!("MEGA said: {other}")),
        },
        mega::Error::ReqwestError { source } => match source.status().map(|s| s.as_u16()) {
            // 509: transfer quota used up for this IP/account.
            Some(509) | Some(429) | Some(403) => TransferError::HostLimited("MEGA".into()),
            _ => TransferError::Network(source.to_string()),
        },
        mega::Error::IoError { source } if source.to_string() == STOPPED => TransferError::Stopped,
        mega::Error::IoError { source } => TransferError::Fatal(format!("Couldn't write the file: {source}")),
        mega::Error::CondensedMacMismatch => TransferError::Network("the download was corrupted".into()),
        mega::Error::MaxRetriesReached => TransferError::Network("MEGA didn't answer".into()),
        mega::Error::InvalidPublicUrlFormat | mega::Error::InvalidUrlFormat => {
            TransferError::Fatal("That MEGA link isn't valid.".into())
        }
        other => TransferError::Fatal(other.to_string()),
    }
}

/// Counts bytes and stops the transfer when asked.
struct Counting<F: FnMut(u64)> {
    inner: Compat<tokio::fs::File>,
    stop: Arc<AtomicBool>,
    on_bytes: F,
}

impl<F: FnMut(u64) + Unpin> futures_io::AsyncWrite for Counting<F> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        if self.stop.load(Ordering::SeqCst) {
            return Poll::Ready(Err(io::Error::other(STOPPED)));
        }
        let this = &mut *self;
        match Pin::new(&mut this.inner).poll_write(cx, buf) {
            Poll::Ready(Ok(n)) => {
                (this.on_bytes)(n as u64);
                Poll::Ready(Ok(n))
            }
            other => other,
        }
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_close(cx)
    }
}

pub async fn client(http: &reqwest::Client, session: Option<&str>) -> Result<mega::Client, TransferError> {
    let mut client = mega::Client::builder()
        .max_retries(3)
        .min_retry_delay(Duration::from_secs(2))
        .max_retry_delay(Duration::from_secs(30))
        .build(http.clone())
        .map_err(map_err)?;
    if let Some(s) = session {
        // An expired session just means downloading as a guest.
        if let Err(e) = client.resume_session(s).await {
            log::warn!("MEGA session couldn't be resumed: {e}");
        }
    }
    Ok(client)
}

/// Downloads every file behind a MEGA file or folder link into `dir`.
/// Returns the files, first archive first.
pub async fn download(
    http: &reqwest::Client,
    link: &str,
    session: Option<&str>,
    dir: &Path,
    stop: Arc<AtomicBool>,
    mut on_progress: impl FnMut(Progress) + Send,
) -> Result<Vec<PathBuf>, TransferError> {
    let url = normalize(link).ok_or_else(|| TransferError::Fatal("That MEGA link has no key, so it can't be decrypted.".into()))?;
    let client = client(http, session).await?;
    let nodes = client.fetch_public_nodes(&url).await.map_err(map_err)?;
    let files: Vec<&mega::Node> = nodes.iter().filter(|n| n.kind() == mega::NodeKind::File).collect();
    if files.is_empty() {
        return Err(TransferError::Fatal("That MEGA link has no files.".into()));
    }
    let total: u64 = files.iter().map(|n| n.size()).sum();
    tokio::fs::create_dir_all(dir)
        .await
        .map_err(|e| TransferError::Fatal(format!("Couldn't create the download folder: {e}")))?;

    let mut done_before = 0u64;
    let mut out = Vec::new();
    for node in files {
        let name = safe_file_name(node.name());
        let dest = dir.join(&name);
        // Already finished on an earlier attempt.
        if tokio::fs::metadata(&dest).await.is_ok_and(|m| m.len() == node.size()) {
            done_before += node.size();
            out.push(dest);
            continue;
        }
        let part = dir.join(format!("{name}.part"));
        let file = tokio::fs::File::create(&part)
            .await
            .map_err(|e| TransferError::Fatal(format!("Couldn't write the file: {e}")))?;
        let mut done = 0u64;
        let mut last = Instant::now() - Duration::from_secs(1);
        let writer = Counting {
            inner: file.compat_write(),
            stop: stop.clone(),
            on_bytes: |n| {
                done += n;
                if last.elapsed() >= Duration::from_millis(500) {
                    last = Instant::now();
                    on_progress(Progress {
                        done: done_before + done,
                        total: Some(total),
                    });
                }
            },
        };
        client.download_node(node, writer).await.map_err(map_err)?;
        done_before += node.size();
        let _ = tokio::fs::remove_file(&dest).await;
        tokio::fs::rename(&part, &dest)
            .await
            .map_err(|e| TransferError::Fatal(format!("Couldn't finish the file: {e}")))?;
        out.push(dest);
    }
    on_progress(Progress {
        done: total,
        total: Some(total),
    });
    // Install from the archive (the first part of a multi-part set).
    out.sort_by_key(|p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let is_first = n.ends_with(".part1.rar") || n.ends_with(".part01.rar") || n.ends_with(".7z.001") || n.ends_with(".zip.001");
        let is_archive = crate::install::archive_kind(p).is_some();
        (!is_first, !is_archive, n)
    });
    Ok(out)
}

/// Signs in once and returns the session string to keep.
pub async fn sign_in(http: &reqwest::Client, email: &str, password: &str, mfa: Option<&str>) -> Result<String, String> {
    let mut client = client(http, None).await.map_err(|e| e.to_string())?;
    match client.login(email, password, mfa).await {
        Ok(()) => client.serialize_session().await.map_err(|e| e.to_string()),
        Err(mega::Error::MegaError { code: mega::ErrorCode::EMFAREQUIRED }) => Err("mfa_required".into()),
        Err(mega::Error::MegaError { code: mega::ErrorCode::ENOENT }) => Err("That email and password didn't work.".into()),
        Err(mega::Error::MegaError { code: mega::ErrorCode::ETOOMANY }) => {
            Err("Too many sign-in attempts. Wait a while and try again.".into())
        }
        Err(e) => Err(format!("Couldn't sign in to MEGA: {e}")),
    }
}

/// Ends the stored session on MEGA's side (best effort).
pub async fn sign_out(http: &reqwest::Client, session: &str) {
    if let Ok(mut c) = client(http, Some(session)).await
        && c.has_user_session() {
            let _ = c.logout().await;
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_mega_links() {
        assert_eq!(
            normalize("https://mega.nz/file/AbC123#KeY_-9").as_deref(),
            Some("https://mega.nz/file/AbC123#KeY_-9")
        );
        assert_eq!(
            normalize("https://mega.nz/#!AbC123!KeY9").as_deref(),
            Some("https://mega.nz/file/AbC123#KeY9")
        );
        assert_eq!(
            normalize("https://mega.co.nz/#F!Fold1!K2").as_deref(),
            Some("https://mega.nz/folder/Fold1#K2")
        );
        assert_eq!(
            normalize("https://mega.nz/folder/Fold1#K2/file/x").as_deref(),
            Some("https://mega.nz/folder/Fold1#K2")
        );
        assert!(normalize("https://mega.nz/file/AbC123").is_none(), "no key, can't decrypt");
        assert!(normalize("https://example.com/file/a#b").is_none());
    }

    #[test]
    fn quota_and_rate_errors_are_host_limits() {
        for code in [mega::ErrorCode::EOVERQUOTA, mega::ErrorCode::ERATELIMIT, mega::ErrorCode::ETOOMANY] {
            assert_eq!(map_err(mega::Error::MegaError { code }), TransferError::HostLimited("MEGA".into()));
        }
        assert!(matches!(
            map_err(mega::Error::MegaError { code: mega::ErrorCode::ENOENT }),
            TransferError::Fatal(_)
        ));
        assert_eq!(
            map_err(mega::Error::IoError { source: io::Error::other(STOPPED) }),
            TransferError::Stopped
        );
    }
}
