//! Project favicons (`ProjectFavicon.tsx`, `createProjectFaviconUrlAtomFamily`
//! in `client-runtime/src/state/assets.ts`): `assets.createUrl` with a
//! `project-favicon` resource issues a signed URL, fetched (HTTP or HTTPS,
//! `crate::http`) on a background thread; the server's "missing" marker
//! keeps the monogram. Raster favicons are downscaled on the CPU and
//! fitted into the icon box (`object-contain`); SVGs are rasterized by egui.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

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
    Image {
        uri: String,
        bytes: Arc<[u8]>,
        /// The decoded raster image (None for SVG).
        raster: Option<Arc<image::RgbaImage>>,
    },
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

type Loaded = (String, Arc<[u8]>, Option<Arc<image::RgbaImage>>);

/// The project's loaded favicon, if any.
fn image(project: &Project) -> Option<Loaded> {
    let cache = cache().lock().ok()?;
    match cache.get(&project.id) {
        Some((key_, Favicon::Image { uri, bytes, raster })) if *key_ == key(project) => {
            Some((uri.clone(), bytes.clone(), raster.clone()))
        }
        _ => None,
    }
}

/// Paints the favicon into `rect` (`object-contain`); false when there is none.
pub fn paint(ui: &Ui, rect: Rect, project: &Project, tint: egui::Color32) -> bool {
    let Some((uri, bytes, raster)) = image(project) else {
        return false;
    };
    if let Some(raster) = raster {
        crate::ui::attachments::paint_contain(ui, rect, &uri, &raster, tint);
        return true;
    }
    egui::Image::from_bytes(uri, egui::load::Bytes::Shared(bytes))
        .fit_to_exact_size(rect.size())
        .maintain_aspect_ratio(true)
        .tint(tint)
        .paint_at(ui, rect);
    true
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
            let state = match crate::http::get(&url) {
                Some(bytes) if !bytes.is_empty() => {
                    let name = url
                        .path_segments()
                        .and_then(|mut s| s.next_back().map(str::to_owned))
                        .unwrap_or_default();
                    Favicon::Image {
                        uri: format!("bytes://favicon/{id}/{name}"),
                        raster: crate::ui::attachments::decode(&bytes).map(Arc::new),
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
