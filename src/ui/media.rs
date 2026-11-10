//! Images in the chat: where a markdown image's bytes come from
//! (`client-runtime/markdownImages.ts` `classifyMarkdownImageSource`), the
//! location a menu copies (`mediaReference.ts`), and the right-click menu of
//! an image (`media/MediaActions.tsx`): copy its path or URL, open it in the
//! file viewer, save it, copy it. Saving and copying run off the UI thread;
//! copying puts a PNG on the clipboard with `wl-copy` (Wayland) or `xclip`
//! (X11), as pasting reads it.
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use base64::Engine;
use eframe::egui::{self, Pos2, Ui};

use crate::app::App;
use crate::ui::context_menu::{self, Item};
pub(crate) use pluto_core::media::{ImageSource, classify, percent_decode, segments};

/// The authored location a menu copies, never the URL used to load it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Reference {
    File {
        path: String,
        relative: Option<String>,
    },
    Url(String),
}

/// What an image's menu acts on (`MediaActionSource`).
#[derive(Clone, Debug)]
pub(crate) struct Source {
    pub name: String,
    pub reference: Option<Reference>,
    /// The decoded image's key in the attachments cache.
    pub key: String,
    /// A local file holding the bytes, when the client can read it.
    pub local_file: Option<PathBuf>,
}

impl Source {
    /// A source with nothing but its name and image (`ExpandedImageDialog`).
    pub(crate) fn plain(name: &str, key: &str) -> Self {
        Self {
            name: name.to_owned(),
            reference: None,
            key: key.to_owned(),
            local_file: None,
        }
    }

    /// The hover text: the file path or URL, else the name.
    pub(crate) fn tooltip(&self) -> &str {
        match &self.reference {
            Some(Reference::File { path, .. }) => path,
            Some(Reference::Url(url)) => url,
            None => &self.name,
        }
    }

    /// `mediaReferenceFileName`, else the name, as a saved file's name.
    fn file_name(&self) -> String {
        let from_reference = match &self.reference {
            Some(Reference::File { path, .. }) => {
                path.rsplit(['/', '\\']).next().map(str::to_owned)
            }
            Some(Reference::Url(url)) => url::Url::parse(url).ok().and_then(|url| {
                url.path_segments()?
                    .next_back()
                    .filter(|s| !s.is_empty())
                    .map(percent_decode)
            }),
            None => None,
        };
        from_reference
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| self.name.clone())
    }
}

/// `mediaFileReference`: the path, and its workspace-relative form when it
/// lies inside the workspace (compared lexically).
pub(crate) fn file_reference(path: &str, workspace_root: Option<&str>) -> Reference {
    let relative = segments(path)
        .zip(workspace_root.and_then(segments))
        .filter(|(target, workspace)| {
            target.len() > workspace.len() && target.starts_with(workspace)
        })
        .map(|(target, workspace)| target[workspace.len()..].join("/"));
    Reference::File {
        path: path.to_owned(),
        relative,
    }
}

/// `mediaUrlReference`: http(s) URLs only.
pub(crate) fn url_reference(url: &str) -> Option<Reference> {
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("//"))
        .then(|| Reference::Url(url.to_owned()))
}

/// Results of saves and copies, shown as the status toast.
static NOTICES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn notify(text: String, ctx: &egui::Context) {
    if let Ok(mut notices) = NOTICES.lock() {
        notices.push(text);
    }
    ctx.request_repaint();
}

/// A download (`anchor.download`): the desktop portal's save dialog, then
/// the text written where it points, the outcome as the status toast.
pub(crate) fn save_text_file(name: String, contents: String, ctx: &egui::Context) {
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let Some(path) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
            return;
        };
        notify(
            match std::fs::write(&path, contents) {
                Ok(()) => format!("Saved to {}", path.display()),
                Err(error) => format!("Could not save {name}: {error}"),
            },
            &ctx,
        );
    });
}

/// The image's original bytes: its local file, else the URL it loaded from.
fn read_bytes(source: &Source) -> Result<Vec<u8>, String> {
    if let Some(file) = &source.local_file {
        return std::fs::read(file).map_err(|e| e.to_string());
    }
    let url = crate::ui::attachments::loaded_url(&source.key)
        .ok_or("This media is unavailable. Try reopening the preview.")?;
    if let Some(data) = url.strip_prefix("data:") {
        let (meta, payload) = data.split_once(',').ok_or("The image data is invalid.")?;
        return if meta.ends_with(";base64") {
            base64::engine::general_purpose::STANDARD
                .decode(payload.trim())
                .map_err(|e| e.to_string())
        } else {
            Ok(percent_decode(payload).into_bytes())
        };
    }
    let parsed = url::Url::parse(&url).map_err(|e| e.to_string())?;
    crate::http::get(&parsed).ok_or_else(|| "The image could not be downloaded.".to_owned())
}

/// The decoded image as PNG bytes (the cache holds premultiplied pixels).
fn png(image: &image::RgbaImage) -> Result<Vec<u8>, String> {
    let mut straight = image.clone();
    for pixel in straight.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a > 0 && a < 255 {
            let scale = |c: u8| ((c as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8;
            pixel.0 = [scale(r), scale(g), scale(b), a];
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    straight
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

fn copy_png(bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let (program, args): (&str, &[&str]) = if wayland {
        ("wl-copy", &["--type", "image/png"])
    } else {
        (
            "xclip",
            &["-selection", "clipboard", "-t", "image/png", "-i"],
        )
    };
    let mut child = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| format!("{program} is not installed."))?;
    child
        .stdin
        .take()
        .ok_or("The clipboard did not accept the image.")?
        .write_all(bytes)
        .map_err(|e| e.to_string())?;
    child
        .wait()
        .ok()
        .filter(|status| status.success())
        .map(|_| ())
        .ok_or_else(|| "The clipboard did not accept the image.".to_owned())
}

/// `mediaMimeTypeFromExtension` for images.
pub(crate) fn is_image_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp", ".svg", ".avif", ".ico",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

impl App {
    /// The hosts of the sources of a run's answer (ADR-0022): those its
    /// tool calls carry (`pluto_core::media::source_hosts`), read again
    /// when the state changes; empty for an answer outside a run.
    pub(crate) fn answer_sources(&mut self, run_id: Option<&str>) -> Arc<HashSet<String>> {
        let (Some(run_id), Some(detail)) = (run_id, self.current_detail()) else {
            return Arc::default();
        };
        if self.answer_sources.0 != self.state.revision {
            self.answer_sources = (self.state.revision, HashMap::new());
        }
        self.answer_sources
            .1
            .entry(run_id.to_owned())
            .or_insert_with(|| {
                Arc::new(pluto_core::media::source_hosts(
                    detail
                        .turn_items
                        .iter()
                        .filter(|item| item.run_id.as_deref() == Some(run_id))
                        .map(|item| item.as_ref()),
                ))
            })
            .clone()
    }
}

impl App {
    /// Opens a host image in the expanded preview once it has loaded
    /// (`openMarkdownMedia`).
    pub(crate) fn open_media_file(&mut self, path: &str) {
        let Some(thread) = self.selected.clone() else {
            return;
        };
        let key = format!("md-file:{thread}:{path}");
        if crate::ui::attachments::media_state(&key).is_none() {
            crate::ui::attachments::mark_loading(&key);
            self.send_request(
                format!("attachment:{key}"),
                "assets.createUrl",
                serde_json::json!({"resource": {"_tag": "media-file", "threadId": thread, "path": path}}),
            );
        }
        let name = path.rsplit('/').next().unwrap_or(path).to_owned();
        let reference = file_reference(path, self.workspace_cwd().as_deref());
        self.media_pending = Some(Source {
            name,
            reference: Some(reference),
            key,
            local_file: None,
        });
    }

    /// The image a file link asked for, shown once it has loaded.
    pub(crate) fn poll_media_open(&mut self) {
        use crate::ui::attachments::MediaState;
        let Some(source) = &self.media_pending else {
            return;
        };
        match crate::ui::attachments::media_state(&source.key) {
            Some(MediaState::Ready(image)) => {
                let source = self.media_pending.take().expect("pending media");
                self.open_lightbox(
                    vec![crate::ui::lightbox::Item {
                        name: source.name.clone(),
                        key: source.key.clone(),
                        image,
                        source: Some(source),
                    }],
                    0,
                );
            }
            Some(MediaState::Failed) => {
                self.media_pending = None;
                self.notify(crate::ui::toasts::Kind::Error, "Unable to open file");
            }
            _ => {}
        }
    }

    /// Opens an image's menu at `at`.
    pub(crate) fn open_media_menu(&mut self, source: Source, at: Pos2) {
        self.media_menu = Some((source, context_menu::State::new(at)));
    }

    /// The open image menu, and the toasts of finished saves and copies.
    pub(crate) fn media_menu(&mut self, ui: &mut Ui) {
        if let Some(notice) = NOTICES.lock().ok().and_then(|mut n| n.pop()) {
            let kind = if notice.starts_with("Could not") {
                crate::ui::toasts::Kind::Error
            } else {
                crate::ui::toasts::Kind::Success
            };
            self.notify(kind, notice);
        }
        let Some((source, state)) = self.media_menu.take() else {
            return;
        };
        let loaded = crate::ui::attachments::cached_image(&source.key);
        let available = loaded.is_some() || source.local_file.is_some();
        let mut items = Vec::new();
        let mut relative = None;
        match &source.reference {
            Some(Reference::File {
                relative: rel_path, ..
            }) => {
                items.push(Item::new("copy-full-path", "Copy full path"));
                if let Some(rel_path) = rel_path {
                    items.push(Item::new("copy-relative-path", "Copy relative path"));
                    relative = Some(rel_path.clone());
                }
            }
            Some(Reference::Url(_)) => items.push(Item::new("copy-url", "Copy URL")),
            None => {}
        }
        let can_open = relative.is_some() && self.selected.is_some();
        if can_open {
            items.push(Item::new("open-file", "Open in file viewer"));
        }
        let mut save = Item::new("save", "Save image");
        save.disabled = !available;
        items.push(save);
        let mut copy = Item::new("copy-image", "Copy image");
        copy.disabled = loaded.is_none();
        items.push(copy);

        let mut open = Some(state);
        let clicked = context_menu::show(ui, &mut open, &items);
        if let Some(state) = open {
            self.media_menu = Some((source.clone(), state));
        }
        let ctx = ui.ctx().clone();
        match clicked.as_deref() {
            Some("copy-full-path") => {
                if let Some(Reference::File { path, .. }) = &source.reference {
                    ctx.copy_text(path.clone());
                    self.notify(crate::ui::toasts::Kind::Success, "Path copied");
                }
            }
            Some("copy-relative-path") => {
                if let Some(relative) = relative {
                    ctx.copy_text(relative);
                    self.notify(crate::ui::toasts::Kind::Success, "Path copied");
                }
            }
            Some("copy-url") => {
                if let Some(Reference::Url(url)) = &source.reference {
                    ctx.copy_text(url.clone());
                    self.notify(crate::ui::toasts::Kind::Success, "URL copied");
                }
            }
            Some("open-file") => {
                if let Some(relative) = relative {
                    self.lightbox = None;
                    self.open_file(relative);
                }
            }
            Some("save") => {
                std::thread::spawn(move || {
                    let Some(path) = rfd::FileDialog::new()
                        .set_file_name(source.file_name())
                        .save_file()
                    else {
                        return;
                    };
                    let saved = read_bytes(&source)
                        .and_then(|bytes| std::fs::write(&path, bytes).map_err(|e| e.to_string()));
                    notify(
                        match saved {
                            Ok(()) => format!("Image saved to {}", path.display()),
                            Err(error) => format!("Could not save image: {error}"),
                        },
                        &ctx,
                    );
                });
            }
            Some("copy-image") => {
                if let Some(image) = loaded {
                    std::thread::spawn(move || {
                        let copied = png(&image).and_then(|bytes| copy_png(&bytes));
                        notify(
                            match copied {
                                Ok(()) => "Image copied".into(),
                                Err(error) => format!("Could not copy image: {error}"),
                            },
                            &ctx,
                        );
                    });
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_markdown_image_sources() {
        let root = Some("/work/app");
        assert_eq!(
            classify("https://example.com/a.png", root),
            ImageSource::Direct("https://example.com/a.png".into())
        );
        assert_eq!(
            classify("/tmp/shot.png", root),
            ImageSource::File("/tmp/shot.png".into())
        );
        assert_eq!(
            classify("<docs/My%20Shot.png>", root),
            ImageSource::File("/work/app/docs/My Shot.png".into())
        );
        assert_eq!(
            classify("file:///tmp/a%20b.png", root),
            ImageSource::File("/tmp/a b.png".into())
        );
        assert_eq!(classify("docs/a.png", None), ImageSource::Blocked);
        assert_eq!(classify("~/a.png", root), ImageSource::Blocked);
        assert_eq!(classify("mailto:x", root), ImageSource::Blocked);
        assert_eq!(classify("#top", root), ImageSource::Blocked);
        // A relative path stays in the workspace.
        assert_eq!(
            classify("docs/../a.png", root),
            ImageSource::File("/work/app/docs/../a.png".into())
        );
        assert_eq!(classify("../../.ssh/id_rsa", root), ImageSource::Blocked);
    }

    #[test]
    fn copied_images_are_straight_alpha_pngs() {
        let mut image = image::RgbaImage::from_pixel(2, 1, image::Rgba([200, 100, 50, 128]));
        crate::ui::emoji::premultiply(&mut image);
        let bytes = png(&image).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        let [r, g, b, a] = decoded.get_pixel(0, 0).0;
        assert_eq!(a, 128);
        let near = |c: u8, want: i32| (c as i32 - want).abs() <= 2;
        assert!(
            near(r, 200) && near(g, 100) && near(b, 50),
            "{:?}",
            (r, g, b)
        );
    }

    #[test]
    fn file_references_know_their_workspace_path() {
        assert_eq!(
            file_reference("/work/app/docs/../img/a.png", Some("/work/app/")),
            Reference::File {
                path: "/work/app/docs/../img/a.png".into(),
                relative: Some("img/a.png".into()),
            }
        );
        assert_eq!(
            file_reference("/tmp/a.png", Some("/work/app")),
            Reference::File {
                path: "/tmp/a.png".into(),
                relative: None,
            }
        );
        assert_eq!(url_reference("data:image/png;base64,xx"), None);
    }
}
