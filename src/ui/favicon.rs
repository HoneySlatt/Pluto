//! Project favicons (`ProjectFavicon.tsx`, `createProjectFaviconUrlAtomFamily`
//! in `client-runtime/src/state/assets.ts`): `assets.createUrl` with a
//! `project-favicon` resource issues a signed URL, fetched over HTTP on a
//! background thread; the server's "missing" marker keeps the monogram.
//!
//! Only plain-HTTP servers (this computer) are fetched; favicons from an
//! HTTPS server stay monograms.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui::{self, Rect, Ui};
use serde_json::{Value, json};

use crate::app::App;
use crate::model::Project;

/// `PROJECT_FAVICON_FALLBACK_MARKER`.
const MISSING: &str = "project-favicon-missing";

#[derive(Clone)]
enum Favicon {
    Pending,
    Missing,
    Image { uri: String, bytes: Arc<[u8]> },
}

/// Favicons by project id, with the (cwd, favicon path) they were asked for.
fn cache() -> &'static Mutex<HashMap<String, (String, Favicon)>> {
    static CACHE: OnceLock<Mutex<HashMap<String, (String, Favicon)>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

fn key(project: &Project) -> String {
    format!(
        "{}\0{}",
        project.workspace_root,
        project.favicon_path.as_deref().unwrap_or("")
    )
}

/// The project's loaded favicon, if any.
fn image(project: &Project) -> Option<(String, Arc<[u8]>)> {
    let cache = cache().lock().ok()?;
    match cache.get(&project.id) {
        Some((key_, Favicon::Image { uri, bytes })) if *key_ == key(project) => {
            Some((uri.clone(), bytes.clone()))
        }
        _ => None,
    }
}

/// Paints the favicon into `rect` (`object-contain`); false when there is none.
pub fn paint(ui: &Ui, rect: Rect, project: &Project, tint: egui::Color32) -> bool {
    let Some((uri, bytes)) = image(project) else {
        return false;
    };
    egui::Image::from_bytes(uri, egui::load::Bytes::Shared(bytes))
        .fit_to_exact_size(rect.size())
        .maintain_aspect_ratio(true)
        .tint(tint)
        .paint_at(ui, rect);
    true
}

/// A plain HTTP/1.1 GET (`Connection: close`); the body on a 200.
fn http_get(url: &url::Url) -> Option<Vec<u8>> {
    if url.scheme() != "http" {
        return None;
    }
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    let port = url.port_or_known_default()?;
    let address = (host.as_str(), port).to_socket_addrs().ok()?.next()?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_owned(),
    };
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n"
    )
    .ok()?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).ok()?;
    let split = response.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = String::from_utf8_lossy(&response[..split]).to_lowercase();
    if !head.starts_with("http/1.1 200") && !head.starts_with("http/1.0 200") {
        return None;
    }
    let body = response[split + 4..].to_vec();
    if head.contains("transfer-encoding: chunked") {
        return Some(dechunk(&body));
    }
    Some(body)
}

/// Joins a chunked body.
fn dechunk(mut body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line) = body.windows(2).position(|w| w == b"\r\n") {
        let size =
            usize::from_str_radix(String::from_utf8_lossy(&body[..line]).trim(), 16).unwrap_or(0);
        if size == 0 || body.len() < line + 2 + size {
            break;
        }
        out.extend_from_slice(&body[line + 2..line + 2 + size]);
        body = &body[(line + 2 + size + 2).min(body.len())..];
    }
    out
}

impl App {
    /// Asks for the favicons of projects without a chosen icon (once per
    /// workspace root and favicon path).
    pub(crate) fn request_favicons(&mut self) {
        let projects = self.state.projects.clone();
        for project in projects.iter().filter(|p| p.project_icon.is_none()) {
            let wanted = key(project);
            let known = cache()
                .lock()
                .ok()
                .and_then(|c| c.get(&project.id).map(|(k, _)| *k == wanted))
                .unwrap_or(false);
            if known {
                continue;
            }
            if let Ok(mut cache) = cache().lock() {
                cache.insert(project.id.clone(), (wanted, Favicon::Pending));
            }
            let mut resource = json!({"_tag": "project-favicon", "cwd": project.workspace_root});
            if let Some(path) = &project.favicon_path {
                resource["path"] = json!(path);
            }
            self.send_request(
                format!("favicon:{}", project.id),
                "assets.createUrl",
                json!({"resource": resource}),
            );
        }
    }

    /// The signed URL for a project's favicon: fetch it, or keep the monogram.
    pub(crate) fn favicon_url(
        &mut self,
        project_id: &str,
        result: Result<Value, String>,
        ctx: &egui::Context,
    ) {
        let relative = result
            .ok()
            .and_then(|r| r["relativeUrl"].as_str().map(str::to_owned));
        let base = url::Url::parse(self.connect.server.trim()).ok();
        let url = relative
            .as_deref()
            .and_then(|r| base.as_ref()?.join(r).ok());
        let missing = url
            .as_ref()
            .and_then(|u| u.path_segments()?.next_back().map(|s| s == MISSING))
            .unwrap_or(true);
        let Some(url) = url.filter(|_| !missing) else {
            if let Ok(mut cache) = cache().lock()
                && let Some(entry) = cache.get_mut(project_id)
            {
                entry.1 = Favicon::Missing;
            }
            return;
        };
        let id = project_id.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let state = match http_get(&url) {
                Some(bytes) if !bytes.is_empty() => {
                    let name = url
                        .path_segments()
                        .and_then(|mut s| s.next_back().map(str::to_owned))
                        .unwrap_or_default();
                    Favicon::Image {
                        uri: format!("bytes://favicon/{id}/{name}"),
                        bytes: bytes.into(),
                    }
                }
                _ => Favicon::Missing,
            };
            if let Ok(mut cache) = cache().lock()
                && let Some(entry) = cache.get_mut(&id)
            {
                entry.1 = state;
            }
            ctx.request_repaint();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunked_bodies_join() {
        assert_eq!(dechunk(b"3\r\nabc\r\n2\r\nde\r\n0\r\n\r\n"), b"abcde");
    }
}
