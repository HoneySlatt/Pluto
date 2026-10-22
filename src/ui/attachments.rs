//! Image attachments of user messages (`UserTimelineRow` in
//! `chat/MessagesTimeline.tsx`, `selectMessageImageResources` in
//! `session-logic.ts`): `assets.createUrl` with an `attachment` resource
//! issues a signed URL, fetched (HTTP or HTTPS) on a background thread like the
//! project favicons, then decoded and drawn cropped to its cell
//! (`object-cover`), downscaled on the CPU to the drawn pixel size (off the
//! UI thread for large images).
//! The preview opens in `lightbox`.
//!
//! Also the composer's images (`addComposerAttachments`): pasted from the
//! clipboard (read with `wl-paste`, or `xclip` on X11, since egui only
//! pastes text), dropped, or picked with the attach button (the desktop
//! portal's file chooser), and sent inline as data URLs.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, CornerRadius, Rect, TextureHandle, Ui, Vec2};
use serde_json::{Value, json};

use crate::app::App;
use crate::http::get as http_get;
use crate::ui::emoji::premultiply;

#[derive(Clone)]
enum Loaded {
    Pending,
    Missing,
    Image(Arc<image::RgbaImage>),
}

/// Decoded images by attachment id.
fn cache() -> &'static Mutex<HashMap<String, Loaded>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Loaded>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Where each loaded image came from (signed, direct or `data:` URL), its
/// pixel size when the server reported it, and a local file holding it.
#[derive(Default)]
struct Origins {
    urls: HashMap<String, String>,
    sizes: HashMap<String, (u32, u32)>,
    files: HashMap<String, std::path::PathBuf>,
}

fn origins() -> &'static Mutex<Origins> {
    static ORIGINS: OnceLock<Mutex<Origins>> = OnceLock::new();
    ORIGINS.get_or_init(Default::default)
}

/// A cached image's load state; None until something asked for it.
pub(crate) enum MediaState {
    Loading,
    Failed,
    Ready(Arc<image::RgbaImage>),
}

pub(crate) fn media_state(key: &str) -> Option<MediaState> {
    cache().lock().ok()?.get(key).map(|loaded| match loaded {
        Loaded::Pending => MediaState::Loading,
        Loaded::Missing => MediaState::Failed,
        Loaded::Image(image) => MediaState::Ready(image.clone()),
    })
}

pub(crate) fn cached_image(key: &str) -> Option<Arc<image::RgbaImage>> {
    match media_state(key)? {
        MediaState::Ready(image) => Some(image),
        _ => None,
    }
}

/// Marks an image as asked for, before its request goes out.
pub(crate) fn mark_loading(key: &str) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_owned(), Loaded::Pending);
    }
}

pub(crate) fn loaded_url(key: &str) -> Option<String> {
    origins().lock().ok()?.urls.get(key).cloned()
}

/// The pixel size the server read from the file header (`imageDimensions`).
pub(crate) fn image_size(key: &str) -> Option<(u32, u32)> {
    origins().lock().ok()?.sizes.get(key).copied()
}

pub(crate) fn local_file(key: &str) -> Option<std::path::PathBuf> {
    origins().lock().ok()?.files.get(key).cloned()
}

/// Loads an image the client can reach itself (`https:`, `//`, `data:`)
/// on a background thread.
pub(crate) fn load_direct(key: &str, uri: &str, ctx: &egui::Context) {
    mark_loading(key);
    let (key, uri, ctx) = (key.to_owned(), uri.to_owned(), ctx.clone());
    std::thread::spawn(move || {
        let bytes = if let Some(data) = uri.strip_prefix("data:") {
            data.split_once(',').and_then(|(meta, payload)| {
                use base64::Engine;
                meta.ends_with(";base64")
                    .then(|| {
                        base64::engine::general_purpose::STANDARD
                            .decode(payload.trim())
                            .ok()
                    })
                    .flatten()
            })
        } else {
            let absolute = if uri.starts_with("//") {
                format!("https:{uri}")
            } else {
                uri.clone()
            };
            url::Url::parse(&absolute)
                .ok()
                .and_then(|url| http_get(&url))
        };
        let state = bytes
            .and_then(|bytes| decode(&bytes))
            .map_or(Loaded::Missing, |image| Loaded::Image(Arc::new(image)));
        if let Ok(mut origins) = origins().lock() {
            origins.urls.insert(key.clone(), uri);
        }
        if let Ok(mut cache) = cache().lock() {
            cache.insert(key, state);
        }
        ctx.request_repaint();
    });
}

/// Decodes image bytes into premultiplied RGBA.
pub(crate) fn decode(bytes: &[u8]) -> Option<image::RgbaImage> {
    let mut image = image::load_from_memory(bytes).ok()?.to_rgba8();
    premultiply(&mut image);
    Some(image)
}

/// `source` cropped to `aspect` (width / height) around its center, then
/// resized to `width`x`height` pixels (`object-cover`).
fn cover(source: &image::RgbaImage, width: u32, height: u32) -> image::RgbaImage {
    let (sw, sh) = (source.width() as f32, source.height() as f32);
    let aspect = width as f32 / height as f32;
    let (cw, ch) = if sw / sh > aspect {
        (sh * aspect, sh)
    } else {
        (sw, sw / aspect)
    };
    let x = ((sw - cw) / 2.0).round() as u32;
    let y = ((sh - ch) / 2.0).round() as u32;
    let cropped = image::imageops::crop_imm(
        source,
        x,
        y,
        (cw.round() as u32).max(1),
        (ch.round() as u32).max(1),
    )
    .to_image();
    image::imageops::resize(
        &cropped,
        width.max(1),
        height.max(1),
        image::imageops::FilterType::Triangle,
    )
}

type SizeKey = (String, u32, u32);

enum Sized {
    Ready(TextureHandle, std::time::Instant),
    Pending,
}

#[derive(Default)]
struct Textures {
    sizes: HashMap<SizeKey, Sized>,
    /// Images resized off the UI thread, waiting for upload.
    done: Arc<Mutex<Vec<(SizeKey, image::RgbaImage)>>>,
}

/// A texture of `key` at `width`x`height` pixels, made once per size by
/// `make` from `source`. Large sources are resized on a background thread
/// (a 12-megapixel screenshot takes tens of milliseconds): meanwhile
/// another size of the same key stands in, else nothing. Sizes unused for a
/// minute are dropped.
pub(crate) fn sized_texture(
    ui: &Ui,
    key: &str,
    width: u32,
    height: u32,
    source: &Arc<image::RgbaImage>,
    make: fn(&image::RgbaImage, u32, u32) -> image::RgbaImage,
) -> Option<TextureHandle> {
    const INLINE_PIXELS: u64 = 512 * 512;
    const IDLE: std::time::Duration = std::time::Duration::from_secs(60);
    static TEXTURES: OnceLock<Mutex<Textures>> = OnceLock::new();
    let mut textures = TEXTURES.get_or_init(Default::default).lock().ok()?;
    let now = std::time::Instant::now();
    let upload = |key: &SizeKey, image: &image::RgbaImage| {
        ui.ctx().load_texture(
            format!("sized:{}:{}x{}", key.0, key.1, key.2),
            egui::ColorImage::from_rgba_premultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        )
    };
    let done = std::mem::take(&mut *textures.done.lock().ok()?);
    for (size, image) in done {
        let texture = upload(&size, &image);
        textures.sizes.insert(size, Sized::Ready(texture, now));
    }
    let size = (key.to_owned(), width, height);
    if let Some(Sized::Ready(texture, used)) = textures.sizes.get_mut(&size) {
        *used = now;
        return Some(texture.clone());
    }
    if !textures.sizes.contains_key(&size) {
        textures.sizes.retain(|_, sized| match sized {
            Sized::Ready(_, used) => now.duration_since(*used) < IDLE,
            Sized::Pending => true,
        });
        let pixels = u64::from(source.width()) * u64::from(source.height());
        if pixels <= INLINE_PIXELS {
            let texture = upload(&size, &make(source, width, height));
            textures
                .sizes
                .insert(size, Sized::Ready(texture.clone(), now));
            return Some(texture);
        }
        textures.sizes.insert(size.clone(), Sized::Pending);
        let (source, done, ctx) = (source.clone(), textures.done.clone(), ui.ctx().clone());
        std::thread::spawn(move || {
            let image = make(&source, width, height);
            if let Ok(mut done) = done.lock() {
                done.push((size, image));
            }
            ctx.request_repaint();
        });
    }
    // Another size of the image, scaled, until this one is ready.
    textures
        .sizes
        .iter()
        .filter(|((other, ..), _)| other == key)
        .filter_map(|((_, w, h), sized)| match sized {
            Sized::Ready(texture, _) => Some((w.abs_diff(width) + h.abs_diff(height), texture)),
            Sized::Pending => None,
        })
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, texture)| texture.clone())
}

/// Paints `image` into `rect`, cropped to fill it, with rounded corners.
pub(crate) fn paint_cover(
    ui: &Ui,
    rect: Rect,
    key: &str,
    image: &Arc<image::RgbaImage>,
    corner_radius: CornerRadius,
) {
    let ppp = ui.ctx().pixels_per_point();
    let width = (rect.width() * ppp).round() as u32;
    let height = (rect.height() * ppp).round() as u32;
    let Some(texture) = sized_texture(ui, key, width, height, image, cover) else {
        return;
    };
    egui::Image::new((texture.id(), rect.size()))
        .corner_radius(corner_radius)
        .paint_at(ui, rect);
}

/// Paints `image` fitted inside `rect` and centered (`object-contain`),
/// one texel per physical pixel.
pub(crate) fn paint_contain(
    ui: &Ui,
    rect: Rect,
    key: &str,
    image: &Arc<image::RgbaImage>,
    tint: egui::Color32,
) {
    let ppp = ui.ctx().pixels_per_point();
    let scale = (rect.width() / image.width().max(1) as f32)
        .min(rect.height() / image.height().max(1) as f32)
        * ppp;
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);
    let Some(texture) = sized_texture(ui, key, width, height, image, |image, width, height| {
        image::imageops::resize(image, width, height, image::imageops::FilterType::Triangle)
    }) else {
        return;
    };
    let size = egui::vec2(width as f32, height as f32);
    let origin = rect.center().to_vec2() * ppp - size / 2.0;
    let min = egui::pos2(origin.x.round(), origin.y.round()) / ppp;
    egui::Image::new((texture.id(), size / ppp))
        .tint(tint)
        .paint_at(ui, Rect::from_min_size(min, size / ppp));
}

/// An image Codex generated, read on a background thread from the file it
/// saves for each image generation call
/// (`$CODEX_HOME/generated_images/<thread>/<call id>.png`, CODEX_HOME
/// defaulting to ~/.codex). The server strips the picture from activities,
/// so only a client on the server's machine can show it.
pub(crate) fn generated_image(
    tool_call_id: &str,
    ctx: &egui::Context,
) -> Option<Arc<image::RgbaImage>> {
    let key = format!("generated:{tool_call_id}");
    match cache().lock().ok()?.get(&key) {
        Some(Loaded::Image(image)) => return Some(image.clone()),
        Some(_) => return None,
        None => {}
    }
    cache().lock().ok()?.insert(key.clone(), Loaded::Pending);
    let file = format!("{tool_call_id}.png");
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let home = std::env::var_os("CODEX_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::home_dir().map(|home| home.join(".codex")));
        let found = home
            .and_then(|home| std::fs::read_dir(home.join("generated_images")).ok())
            .into_iter()
            .flatten()
            .flatten()
            .find_map(|dir| {
                let path = dir.path().join(&file);
                std::fs::read(&path).ok().map(|bytes| (path, bytes))
            });
        if let (Some((path, _)), Ok(mut origins)) = (&found, origins().lock()) {
            origins.files.insert(key.clone(), path.clone());
        }
        let state = found
            .and_then(|(_, bytes)| decode(&bytes))
            .map_or(Loaded::Missing, |image| Loaded::Image(Arc::new(image)));
        if let Ok(mut cache) = cache().lock() {
            cache.insert(key, state);
        }
        ctx.request_repaint();
    });
    None
}

/// The message's image attachments (`isImageAttachment`).
pub(crate) fn images(attachments: &[Value]) -> Vec<&Value> {
    attachments
        .iter()
        .filter(|a| a["type"] == "image")
        .collect()
}

/// The size of the images grid at most `max_width` wide: two columns of
/// 4:3 cells (`grid max-w-[210px] grid-cols-2 gap-2`, `aspect-[4/3]`).
pub(crate) fn grid_size(count: usize, max_width: f32) -> (f32, Vec2) {
    let width = max_width.min(210.0);
    let cell = Vec2::new((width - 8.0) / 2.0, (width - 8.0) / 2.0 * 3.0 / 4.0);
    let rows = count.div_ceil(2) as f32;
    (rows * cell.y + (rows - 1.0).max(0.0) * 8.0, cell)
}

impl App {
    /// The attachment's decoded image, asking for it the first time.
    pub(crate) fn attachment_image(&mut self, attachment: &Value) -> Option<Arc<image::RgbaImage>> {
        let id = attachment["id"].as_str()?;
        let known = cache().lock().ok()?.get(id).cloned();
        match known {
            Some(Loaded::Image(image)) => Some(image),
            Some(_) => None,
            None => {
                cache().lock().ok()?.insert(id.to_owned(), Loaded::Pending);
                self.send_request(
                    format!("attachment:{id}"),
                    "assets.createUrl",
                    json!({"resource": {"_tag": "attachment", "attachmentId": id}}),
                );
                None
            }
        }
    }

    /// The signed URL of an attachment: fetch and decode it off the UI thread.
    pub(crate) fn attachment_url(
        &mut self,
        id: &str,
        result: Result<Value, String>,
        ctx: &egui::Context,
    ) {
        let base = url::Url::parse(self.connect.server.trim()).ok();
        if let (Ok(r), Ok(mut origins)) = (&result, origins().lock())
            && let (Some(width), Some(height)) = (
                r["imageDimensions"]["width"].as_u64(),
                r["imageDimensions"]["height"].as_u64(),
            )
        {
            origins
                .sizes
                .insert(id.to_owned(), (width as u32, height as u32));
        }
        let url = result.ok().and_then(|r| {
            let relative = r["relativeUrl"].as_str()?.to_owned();
            base?.join(&relative).ok()
        });
        let Some(url) = url else {
            if let Ok(mut cache) = cache().lock() {
                cache.insert(id.to_owned(), Loaded::Missing);
            }
            return;
        };
        if let Ok(mut origins) = origins().lock() {
            origins.urls.insert(id.to_owned(), url.to_string());
        }
        let id = id.to_owned();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let state = http_get(&url)
                .and_then(|bytes| decode(&bytes))
                .map_or(Loaded::Missing, |image| Loaded::Image(Arc::new(image)));
            if let Ok(mut cache) = cache().lock() {
                cache.insert(id, state);
            }
            ctx.request_repaint();
        });
    }
}

/// `PROVIDER_SEND_TURN_SUPPORTED_IMAGE_MIME_TYPES`.
const SUPPORTED_IMAGE_TYPES: [&str; 4] = ["image/gif", "image/jpeg", "image/png", "image/webp"];
/// `PROVIDER_SEND_TURN_MAX_IMAGE_BYTES`.
const MAX_IMAGE_BYTES: usize = 10 * 1024 * 1024;
/// `PROVIDER_SEND_TURN_MAX_ATTACHMENTS`.
const MAX_ATTACHMENTS: usize = 100;

/// An image attached to the composer (`ComposerImageAttachment`).
pub(crate) struct ComposerImage {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) bytes: Arc<Vec<u8>>,
    pub(crate) preview: Arc<image::RgbaImage>,
}

/// Images read off the UI thread, or why one was refused.
pub(crate) type Incoming = Arc<Mutex<Vec<Result<ComposerImage, String>>>>;

/// The supported image type of a file name, by extension.
fn mime_for_name(name: &str) -> Option<&'static str> {
    let extension = name.rsplit('.').next()?.to_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Checks and decodes an image for the composer (`addComposerAttachments`):
/// a supported type within the wire cap. Larger images are refused, where
/// upstream downscales them to fit.
fn composer_image(name: &str, mime_type: &str, bytes: Vec<u8>) -> Result<ComposerImage, String> {
    if !SUPPORTED_IMAGE_TYPES.contains(&mime_type) {
        return Err(format!(
            "'{name}' is not a supported image type. Attach GIF, JPEG, PNG, or WebP images."
        ));
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!("'{name}' is too large to attach."));
    }
    let mut preview =
        decode(&bytes).ok_or_else(|| format!("'{name}' could not be read as an image."))?;
    // The thumbnail never needs more than a few hundred pixels.
    if preview.width().max(preview.height()) > 256 {
        let scale = 256.0 / preview.width().max(preview.height()) as f32;
        preview = image::imageops::resize(
            &preview,
            ((preview.width() as f32 * scale).round() as u32).max(1),
            ((preview.height() as f32 * scale).round() as u32).max(1),
            image::imageops::FilterType::Triangle,
        );
    }
    Ok(ComposerImage {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_owned(),
        mime_type: mime_type.to_owned(),
        bytes: Arc::new(bytes),
        preview: Arc::new(preview),
    })
}

/// The clipboard's image as (mime type, bytes): `wl-paste` on Wayland,
/// `xclip` on X11. None when it holds no image, or text as well.
fn clipboard_image() -> Option<Result<(String, Vec<u8>), String>> {
    let run = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| output.stdout)
    };
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    let types = if wayland {
        run("wl-paste", &["--list-types"])
    } else {
        run("xclip", &["-selection", "clipboard", "-t", "TARGETS", "-o"])
    }?;
    let types = String::from_utf8_lossy(&types).to_string();
    let types: Vec<&str> = types.lines().map(str::trim).collect();
    // egui already pasted the clipboard's text (upstream lets an image win).
    if types
        .iter()
        .any(|t| t.starts_with("text/plain") || *t == "UTF8_STRING")
    {
        return None;
    }
    let Some(mime) = SUPPORTED_IMAGE_TYPES
        .iter()
        .find(|mime| types.contains(mime))
        .copied()
    else {
        return types.iter().any(|t| t.starts_with("image/")).then(|| {
            Err(
                "'image' is not a supported image type. Attach GIF, JPEG, PNG, or WebP images."
                    .to_owned(),
            )
        });
    };
    let bytes = if wayland {
        run("wl-paste", &["--no-newline", "--type", mime])
    } else {
        run("xclip", &["-selection", "clipboard", "-t", mime, "-o"])
    }?;
    Some(Ok((mime.to_owned(), bytes)))
}

/// Reads the clipboard's image on a background thread (a paste the text
/// clipboard could not serve).
pub(crate) fn paste_clipboard_image(incoming: &Incoming, ctx: &egui::Context) {
    let incoming = incoming.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let Some(read) = clipboard_image() else {
            return;
        };
        // Clipboard images are named like the browser names them.
        let image = read.and_then(|(mime, bytes)| {
            let extension = mime.trim_start_matches("image/").replace("jpeg", "jpg");
            composer_image(&format!("image.{extension}"), &mime, bytes)
        });
        if let Ok(mut incoming) = incoming.lock() {
            incoming.push(image);
        }
        ctx.request_repaint();
    });
}

/// Reads dropped image files on a background thread.
pub(crate) fn add_dropped_files(
    incoming: &Incoming,
    ctx: &egui::Context,
    files: Vec<egui::DroppedFileHandle>,
) {
    let paths = files.iter().map(|f| f.path().to_path_buf()).collect();
    add_files(incoming, ctx, paths);
}

/// The attach button (`<input type="file" multiple>`): the portal's file
/// chooser, off the UI thread, then the chosen files like dropped ones.
pub(crate) fn pick_files(incoming: &Incoming, ctx: &egui::Context) {
    let incoming = incoming.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let picked = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
            .add_filter("All files", &["*"])
            .pick_files();
        match picked {
            Some(paths) => add_files(&incoming, &ctx, paths),
            // A cancel is `Some([])`; None means no chooser could open
            // (rfd loads libdbus at runtime to reach the portal).
            None => {
                if let Ok(mut incoming) = incoming.lock() {
                    incoming.push(Err(
                        "The file chooser could not open: no desktop portal (libdbus) was found."
                            .into(),
                    ));
                }
                ctx.request_repaint();
            }
        }
    });
}

/// Reads image files on a background thread.
fn add_files(incoming: &Incoming, ctx: &egui::Context, paths: Vec<std::path::PathBuf>) {
    let incoming = incoming.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_owned());
            let image = match (std::fs::read(&path).ok(), mime_for_name(&name)) {
                (Some(bytes), Some(mime)) => composer_image(&name, mime, bytes),
                (None, _) => Err(format!("'{name}' could not be read as an image.")),
                (Some(_), None) => Err(format!(
                    "'{name}' is not a supported image type. Attach GIF, JPEG, PNG, or WebP images."
                )),
            };
            if let Ok(mut incoming) = incoming.lock() {
                incoming.push(image);
            }
        }
        ctx.request_repaint();
    });
}

/// The composer images as `UploadChatImageAttachment`s (inline data URLs).
pub(crate) fn upload_payload(images: &[ComposerImage]) -> Vec<Value> {
    use base64::Engine;
    images
        .iter()
        .map(|image| {
            let data = base64::engine::general_purpose::STANDARD.encode(image.bytes.as_slice());
            json!({
                "type": "image",
                "name": image.name,
                "mimeType": image.mime_type,
                "sizeBytes": image.bytes.len(),
                "dataUrl": format!("data:{};base64,{data}", image.mime_type),
            })
        })
        .collect()
}

impl App {
    /// Moves read images into the composer, reporting refusals.
    pub(crate) fn take_incoming_images(&mut self) {
        let arrived: Vec<_> = match self.incoming_images.lock() {
            Ok(mut incoming) => incoming.drain(..).collect(),
            Err(_) => return,
        };
        for image in arrived {
            match image {
                Ok(_) if self.composer_images.len() >= MAX_ATTACHMENTS => {
                    self.status = Some(format!(
                        "You can attach up to {MAX_ATTACHMENTS} files per message."
                    ));
                }
                Ok(image) => self.composer_images.push(image),
                Err(error) => self.status = Some(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uploads_are_data_urls() {
        let image = ComposerImage {
            id: "a".into(),
            name: "image.png".into(),
            mime_type: "image/png".into(),
            bytes: Arc::new(vec![1, 2, 3]),
            preview: Arc::new(image::RgbaImage::new(1, 1)),
        };
        let payload = upload_payload(&[image]);
        assert_eq!(payload[0]["dataUrl"], "data:image/png;base64,AQID");
        assert_eq!(payload[0]["sizeBytes"], 3);
        assert_eq!(mime_for_name("Shot.JPG"), Some("image/jpeg"));
        assert_eq!(mime_for_name("photo.heic"), None);
    }

    #[test]
    fn grids_follow_upstream() {
        let (height, cell) = grid_size(1, 500.0);
        assert_eq!(cell, Vec2::new(101.0, 75.75));
        assert_eq!(height, 75.75);
        let (height, _) = grid_size(3, 500.0);
        assert_eq!(height, 75.75 * 2.0 + 8.0);
    }

    #[test]
    fn cover_crops_the_long_side() {
        let source = image::RgbaImage::from_pixel(400, 100, image::Rgba([1, 2, 3, 255]));
        let out = cover(&source, 40, 30);
        assert_eq!((out.width(), out.height()), (40, 30));
    }
}
