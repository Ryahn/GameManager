//! File hosts the launcher can download from by itself.
//!
//! Only documented, official ways to fetch a file are used:
//! - Pixeldrain through its public API, with the user's own API key when
//!   they've added one (Settings), which avoids captcha prompts.
//! - MEGA through MEGA's API (see `mega.rs`), optionally signed in.
//! - Gofile through its API (https://gofile.io/api): listing a shared
//!   folder (`GET /contents/{id}`) is Premium-only, so this only applies
//!   when the user has added their own Premium token.
//! - Plain links to an archive file.
//!
//! Everything else (Uploadhaven, Buzzheavier, MediaFire, Google Drive,
//! masked F95 links, anything behind a captcha) goes through the F95
//! browser window, where the user clicks the host's own download button.

use serde::Deserialize;
use url::Url;

/// How to authenticate the file request itself.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum Auth {
    #[default]
    None,
    /// HTTP Basic with an empty user (Pixeldrain API keys).
    Basic(String),
    /// Gofile account token: Bearer header plus the site's accountToken cookie.
    Gofile(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteFile {
    pub url: String,
    pub name: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Resolved {
    /// One or more files to fetch over HTTP (a folder may hold several,
    /// e.g. a multi-part archive).
    Files { files: Vec<RemoteFile>, auth: Auth },
    /// A MEGA file or folder link (normalised, with its key).
    Mega { url: String },
    /// The host wants a person (captcha, login, Premium, rate limit page).
    NeedsBrowser(String),
}

/// Credentials the user added in Settings.
#[derive(Debug, Clone, Default)]
pub struct HostCreds {
    pub pixeldrain_key: Option<String>,
    pub mega_session: Option<String>,
    pub gofile_token: Option<String>,
}

impl HostCreds {
    pub fn load() -> Self {
        use crate::auth::store;
        Self {
            pixeldrain_key: store::get(store::PIXELDRAIN_KEY),
            mega_session: store::get(store::MEGA_SESSION),
            gofile_token: store::get(store::GOFILE_TOKEN),
        }
    }
}

const ARCHIVE_EXT: &[&str] = &[".zip", ".7z", ".rar", ".zip.001", ".7z.001"];

/// File hosts whose links are web pages (download buttons, timers,
/// captchas), even when the URL ends in a file name, e.g.
/// `datanodes.to/<id>/Game-1.0-pc.zip`. Always handled in the F95 window.
/// Covers F95's approved hosts that don't serve raw files.
const PAGE_HOSTS: &[&str] = &[
    "akirabox.com", "akirabox.to", "apkadmin.com", "buzzheavier.com", "bzzhr.co", "bzzhr.to",
    "clonr.co", "cyberfile.me", "datanodes.to", "delafil.se", "download.gg", "dropmefiles.com",
    "drive.google.com", "docs.google.com", "drive.usercontent.google.com", "filemail.com",
    "files.dp.ua", "files.fm", "fromsmash.com", "gofile.io", "hexload.com", "hexupload.net",
    "krakenfiles.com", "mediafire.com", "mixdrop.ag", "mixdrop.co", "proton.me", "sendgb.com",
    "terminal.lc", "transfert.free.fr", "uploadhaven.com", "uploadnow.io", "vikingfile.com",
    "wdho.ru", "we.tl", "wetransfer.com", "workupload.com", "yourfilestore.com", "1fichier.com",
    "rapidgator.net", "nitroflare.com",
];

fn is_page_host(host: &str) -> bool {
    let host = host.trim_start_matches("www.");
    // Bunkr moves between many mirror domains (bunkr.cr, bunkr.si, bunkrr.su …).
    host.starts_with("bunkr")
        || host.contains(".bunkr")
        || PAGE_HOSTS
            .iter()
            .any(|h| host == *h || host.ends_with(&format!(".{h}")))
}
const GOFILE_API: &str = "https://api.gofile.io";

fn pixeldrain_id(u: &Url) -> Option<String> {
    let host = u.host_str()?.trim_start_matches("www.");
    if host != "pixeldrain.com" && host != "pixeldra.in" {
        return None;
    }
    let mut seg = u.path_segments()?;
    match (seg.next(), seg.next()) {
        (Some("u"), Some(id)) if is_id(id) => Some(id.to_string()),
        (Some("api"), Some("file")) => seg.next().filter(|id| is_id(id)).map(str::to_string),
        _ => None,
    }
}

/// `https://gofile.io/d/{code}` → `code`.
fn gofile_code(u: &Url) -> Option<String> {
    let host = u.host_str()?.trim_start_matches("www.");
    if host != "gofile.io" {
        return None;
    }
    let mut seg = u.path_segments()?;
    match (seg.next(), seg.next()) {
        (Some("d"), Some(code)) if !code.is_empty() && code.len() <= 40 && code.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => {
            Some(code.to_string())
        }
        _ => None,
    }
}

fn is_id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_direct_file(u: &Url) -> bool {
    let p = u.path().to_lowercase();
    ARCHIVE_EXT.iter().any(|e| p.ends_with(e))
}

/// The host name if the launcher can fetch this link itself with the
/// credentials the user has.
pub fn supported(u: &Url, creds: &HostCreds) -> Option<String> {
    if !matches!(u.scheme(), "https" | "http") {
        return None;
    }
    let host = u.host_str()?.trim_start_matches("www.").to_lowercase();
    if host.ends_with("f95zone.to") {
        return None;
    }
    if pixeldrain_id(u).is_some() {
        return Some("pixeldrain.com".into());
    }
    if super::mega::normalize(u.as_str()).is_some() {
        return Some("mega.nz".into());
    }
    if gofile_code(u).is_some() {
        return creds.gofile_token.as_ref().map(|_| "gofile.io".into());
    }
    if is_page_host(&host) {
        return None;
    }
    is_direct_file(u).then_some(host)
}

fn file_name_from_url(u: &Url) -> Option<String> {
    u.path_segments()?
        .next_back()
        .filter(|s| !s.is_empty())
        .and_then(|s| urlencoding::decode(s).ok())
        .map(|s| s.into_owned())
}

#[derive(Deserialize)]
struct GofileEnvelope {
    status: String,
    #[serde(default)]
    data: Option<GofileContent>,
}

#[derive(Deserialize)]
struct GofileContent {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    children: Option<std::collections::BTreeMap<String, GofileContent>>,
}

/// Lists a shared Gofile file or folder with the user's Premium token.
async fn gofile_files(
    http: &reqwest::Client,
    api: &str,
    code: &str,
    token: &str,
    page: &str,
) -> Result<Resolved, super::transfer::TransferError> {
    use super::transfer::TransferError as E;
    let resp = http
        .get(format!("{api}/contents/{code}"))
        .query(&[("pageSize", "100")])
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| E::Network(e.to_string()))?;
    if resp.status().is_server_error() {
        return Err(E::Network(format!("Gofile answered {}", resp.status())));
    }
    // Gofile puts the outcome in "status", not always in the HTTP code.
    let env: GofileEnvelope = resp.json().await.map_err(|e| E::Network(e.to_string()))?;
    match env.status.as_str() {
        "ok" => {}
        "error-rateLimit" => return Err(E::HostLimited("Gofile".into())),
        "error-notFound" => return Err(E::Fatal("The file was removed from Gofile.".into())),
        "error-token" => {
            return Err(E::Fatal("Gofile rejected your token. Update it in Settings → File host accounts.".into()));
        }
        // Not Premium, password-protected, or anything new: let the user
        // finish on the website instead.
        _ => return Ok(Resolved::NeedsBrowser(page.to_string())),
    }
    let content = env.data.ok_or_else(|| E::Network("Gofile sent an empty answer".into()))?;
    let as_file = |c: GofileContent| {
        c.link.map(|url| RemoteFile {
            url,
            name: c.name,
            size: c.size,
        })
    };
    let files: Vec<RemoteFile> = if content.kind == "folder" {
        content
            .children
            .unwrap_or_default()
            .into_values()
            .filter(|c| c.kind == "file")
            .filter_map(as_file)
            .collect()
    } else {
        as_file(content).into_iter().collect()
    };
    if files.is_empty() {
        return Err(E::Fatal("That Gofile link has no files.".into()));
    }
    Ok(Resolved::Files {
        files,
        auth: Auth::Gofile(token.to_string()),
    })
}

/// Turns a link into something downloadable, asking the host when needed.
pub async fn resolve(
    http: &reqwest::Client,
    page: &str,
    creds: &HostCreds,
) -> Result<Resolved, super::transfer::TransferError> {
    use super::transfer::TransferError as E;
    let u = Url::parse(page).map_err(|_| E::Fatal("That link isn't valid.".into()))?;

    if let Some(url) = super::mega::normalize(page) {
        return Ok(Resolved::Mega { url });
    }

    if let Some(code) = gofile_code(&u) {
        return match &creds.gofile_token {
            Some(token) => gofile_files(http, GOFILE_API, &code, token, page).await,
            None => Ok(Resolved::NeedsBrowser(page.to_string())),
        };
    }

    if let Some(id) = pixeldrain_id(&u) {
        // Public API: https://pixeldrain.com/api/file/{id}/info
        let mut req = http.get(format!("https://pixeldrain.com/api/file/{id}/info"));
        if let Some(key) = &creds.pixeldrain_key {
            req = req.basic_auth("", Some(key));
        }
        let info = req.send().await.map_err(|e| E::Network(e.to_string()))?;
        match info.status().as_u16() {
            404 => return Err(E::Fatal("The file was removed from Pixeldrain.".into())),
            429 | 403 => return Err(E::HostLimited("Pixeldrain".into())),
            s if s >= 500 => return Err(E::Network(format!("Pixeldrain answered {s}"))),
            _ => {}
        }
        let v: serde_json::Value = info.json().await.map_err(|e| E::Network(e.to_string()))?;
        // A non-empty "availability" means Pixeldrain wants a captcha first
        // (heavy traffic on the file, or a virus scan notice).
        let blocked = v
            .get("availability")
            .and_then(|a| a.as_str())
            .is_some_and(|a| !a.is_empty())
            || v.get("can_download").and_then(|c| c.as_bool()) == Some(false);
        if blocked {
            return Ok(Resolved::NeedsBrowser(format!("https://pixeldrain.com/u/{id}")));
        }
        return Ok(Resolved::Files {
            files: vec![RemoteFile {
                url: format!("https://pixeldrain.com/api/file/{id}?download"),
                name: v.get("name").and_then(|n| n.as_str()).map(str::to_string),
                size: v.get("size").and_then(|s| s.as_u64()),
            }],
            auth: creds.pixeldrain_key.clone().map(Auth::Basic).unwrap_or_default(),
        });
    }

    let host = u.host_str().unwrap_or_default().to_lowercase();
    if is_direct_file(&u) && !is_page_host(&host) {
        return Ok(Resolved::Files {
            files: vec![RemoteFile {
                url: u.to_string(),
                name: file_name_from_url(&u),
                size: None,
            }],
            auth: Auth::None,
        });
    }
    Ok(Resolved::NeedsBrowser(page.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn u(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn knows_which_links_it_can_fetch() {
        let none = HostCreds::default();
        let gofile = HostCreds {
            gofile_token: Some("t".into()),
            ..Default::default()
        };
        assert_eq!(supported(&u("https://pixeldrain.com/u/AbC123"), &none).as_deref(), Some("pixeldrain.com"));
        assert_eq!(
            supported(&u("https://pixeldrain.com/api/file/AbC123?download"), &none).as_deref(),
            Some("pixeldrain.com")
        );
        assert_eq!(
            supported(&u("https://cdn.example.com/files/Game-0.9-pc.zip"), &none).as_deref(),
            Some("cdn.example.com")
        );
        assert!(supported(&u("https://pixeldrain.com/l/list123"), &none).is_none(), "lists need the browser");
        assert_eq!(supported(&u("https://mega.nz/file/abc#key"), &none).as_deref(), Some("mega.nz"));
        assert!(supported(&u("https://mega.nz/file/abc"), &none).is_none(), "MEGA without its key");
        assert!(supported(&u("https://gofile.io/d/abc"), &none).is_none(), "Gofile needs Premium");
        assert_eq!(supported(&u("https://gofile.io/d/abc"), &gofile).as_deref(), Some("gofile.io"));
        assert!(supported(&u("https://f95zone.to/masked/pixeldrain.com/1/x"), &none).is_none());
        assert!(supported(&u("ftp://example.com/a.zip"), &none).is_none());
    }

    #[test]
    fn page_hosts_go_to_the_browser_even_with_a_file_name() {
        let none = HostCreds::default();
        for link in [
            "https://datanodes.to/a1b2c3/Game-0.5-pc.zip",
            "https://www.datanodes.to/a1b2c3/Game.7z",
            "https://buzzheavier.com/abc/Game.zip",
            "https://bzzhr.to/abc",
            "https://bunkr.cr/f/Game.zip",
            "https://get.bunkrr.su/file/Game.zip",
            "https://www.mediafire.com/file/k3y/Game-1.0.zip/file",
            "https://cyberfile.me/x1/Game.rar",
            "https://krakenfiles.com/view/abc/file.html",
            "https://workupload.com/file/abc",
        ] {
            assert!(supported(&u(link), &none).is_none(), "{link}");
        }
        // Raw file hosts and developers' own servers still download in the app.
        assert_eq!(supported(&u("https://files.catbox.moe/ab12cd.zip"), &none).as_deref(), Some("files.catbox.moe"));
        assert_eq!(
            supported(&u("https://github.com/dev/game/releases/download/v1/Game.zip"), &none).as_deref(),
            Some("github.com")
        );
    }

    #[tokio::test]
    async fn page_host_files_resolve_to_the_browser() {
        let http = reqwest::Client::new();
        let r = resolve(&http, "https://datanodes.to/a1b2c3/Game.zip", &HostCreds::default()).await.unwrap();
        assert_eq!(r, Resolved::NeedsBrowser("https://datanodes.to/a1b2c3/Game.zip".into()));
    }

    #[tokio::test]
    async fn direct_files_resolve_without_a_request() {
        let http = reqwest::Client::new();
        let none = HostCreds::default();
        let r = resolve(&http, "https://cdn.example.com/a/Game%20v1.zip", &none).await.unwrap();
        assert_eq!(
            r,
            Resolved::Files {
                files: vec![RemoteFile {
                    url: "https://cdn.example.com/a/Game%20v1.zip".into(),
                    name: Some("Game v1.zip".into()),
                    size: None,
                }],
                auth: Auth::None,
            }
        );
        let r = resolve(&http, "https://gofile.io/d/x", &none).await.unwrap();
        assert!(matches!(r, Resolved::NeedsBrowser(_)), "no token: browser");
        let r = resolve(&http, "https://mega.nz/#!Ab1!Key2", &none).await.unwrap();
        assert_eq!(r, Resolved::Mega { url: "https://mega.nz/file/Ab1#Key2".into() });
    }

    #[tokio::test]
    async fn gofile_folder_lists_files_with_premium_token() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/contents/x7k2p9"))
            .and(header("authorization", "Bearer tok"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "ok",
                "data": {
                    "type": "folder", "name": "Game",
                    "children": {
                        "a": {"type": "file", "name": "Game.part1.rar", "size": 10, "link": "https://store-1.gofile.io/download/web/a/Game.part1.rar"},
                        "b": {"type": "file", "name": "Game.part2.rar", "size": 5, "link": "https://store-1.gofile.io/download/web/b/Game.part2.rar"},
                        "c": {"type": "folder", "name": "extras"}
                    }
                }
            })))
            .mount(&server)
            .await;
        let r = gofile_files(&reqwest::Client::new(), &server.uri(), "x7k2p9", "tok", "https://gofile.io/d/x7k2p9")
            .await
            .unwrap();
        let Resolved::Files { files, auth } = r else { panic!("expected files") };
        assert_eq!(auth, Auth::Gofile("tok".into()));
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].name.as_deref(), Some("Game.part1.rar"));
    }

    #[tokio::test]
    async fn gofile_errors_map_to_actions() {
        let server = MockServer::start().await;
        for (code, status) in [("np", "error-notPremium"), ("rl", "error-rateLimit"), ("nf", "error-notFound")] {
            Mock::given(path(format!("/contents/{code}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"status": status})))
                .mount(&server)
                .await;
        }
        let http = reqwest::Client::new();
        let page = "https://gofile.io/d/np";
        assert!(matches!(
            gofile_files(&http, &server.uri(), "np", "t", page).await.unwrap(),
            Resolved::NeedsBrowser(_)
        ));
        assert_eq!(
            gofile_files(&http, &server.uri(), "rl", "t", page).await.unwrap_err(),
            super::super::transfer::TransferError::HostLimited("Gofile".into())
        );
        assert!(matches!(
            gofile_files(&http, &server.uri(), "nf", "t", page).await.unwrap_err(),
            super::super::transfer::TransferError::Fatal(_)
        ));
    }
}
