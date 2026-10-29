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
//!
//! Other files (`ChatFileAttachment`) are uploaded when attached, as
//! upstream's `attachmentUploadQueue.ts` does: `attachments.createUploadUrl`,
//! then a POST of the bytes to the signed URL, and sent as `file`
//! attachments the agent reads by path. Their chips are Pluto's own drawing;
//! upstream's upload progress, retry and file viewer are not ported.

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

/// The decoded images kept at most (their RGBA bytes): past it, the least
/// recently stored go, and come back from their source when shown again.
const IMAGE_BUDGET: usize = 384 * 1024 * 1024;

/// Stores a load's outcome; decoded images count against the budget.
fn store(key: String, state: Loaded) {
    static ORDER: Mutex<std::collections::VecDeque<(String, usize)>> =
        Mutex::new(std::collections::VecDeque::new());
    let Ok(mut cache) = cache().lock() else {
        return;
    };
    let size = match &state {
        Loaded::Image(image) => image.as_raw().len(),
        _ => 0,
    };
    cache.insert(key.clone(), state);
    if size == 0 {
        return;
    }
    let Ok(mut order) = ORDER.lock() else {
        return;
    };
    order.retain(|(k, _)| *k != key);
    order.push_back((key, size));
    let mut total: usize = order.iter().map(|(_, s)| s).sum();
    while total > IMAGE_BUDGET && order.len() > 1 {
        if let Some((old, old_size)) = order.pop_front() {
            cache.remove(&old);
            total -= old_size;
        }
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
            // Its base64 is a third larger than the image.
            data.split_once(',')
                .filter(|(_, payload)| payload.len() <= MAX_DECODED_BYTES / 3 * 4 + 4)
                .and_then(|(meta, payload)| {
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
        store(key, state);
        ctx.request_repaint();
    });
}

/// The largest image file decoded (a markdown image, a download, a
/// `data:` URL).
const MAX_DECODED_BYTES: usize = 40 * 1024 * 1024;

/// Decodes image bytes (or an SVG, rasterized at its own size) into
/// premultiplied RGBA.
pub(crate) fn decode(bytes: &[u8]) -> Option<image::RgbaImage> {
    // An image this large (or a decompression bomb) is not drawn.
    if bytes.len() > MAX_DECODED_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let Ok(decoded) = reader.decode() else {
        let svg = egui_extras::image::load_svg_bytes(bytes, &Default::default()).ok()?;
        let [width, height] = svg.size;
        let pixels = svg.pixels.iter().flat_map(|p| p.to_array()).collect();
        return image::RgbaImage::from_raw(width as u32, height as u32, pixels);
    };
    let mut image = decoded.to_rgba8();
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

/// Decode a discovered native-thread image on a background thread. Its key
/// changes when Codex finishes writing it, so a partial write can retry.
pub(crate) fn generated_image(
    generated: &crate::generated_images::GeneratedImage,
    ctx: &egui::Context,
) -> Option<Arc<image::RgbaImage>> {
    let key = generated.key();
    match cache().lock().ok()?.get(&key) {
        Some(Loaded::Image(image)) => return Some(image.clone()),
        Some(_) => return None,
        None => {}
    }
    cache().lock().ok()?.insert(key.clone(), Loaded::Pending);
    let path = generated.path.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let found = std::fs::read(&path).ok();
        if let Ok(mut origins) = origins().lock() {
            origins.files.insert(key.clone(), path);
        }
        let state = found
            .and_then(|bytes| decode(&bytes))
            .map_or(Loaded::Missing, |image| Loaded::Image(Arc::new(image)));
        store(key, state);
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
            store(id.to_owned(), Loaded::Missing);
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
            store(id, state);
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

/// `PROVIDER_SEND_TURN_MAX_FILE_BYTES`.
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;

/// A file attached to the composer (`ComposerFileAttachment`): uploaded to
/// the server as soon as it is attached, sent as a `file` attachment the
/// agent reads by path.
pub(crate) struct ComposerFile {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) mime_type: String,
    pub(crate) size: u64,
    pub(crate) path: std::path::PathBuf,
    /// `None` while uploading; the stored attachment id, or why it failed.
    pub(crate) upload: Option<Result<String, String>>,
}

/// What an attach brought: an image shown inline, or any other file.
pub(crate) enum Attached {
    Image(ComposerImage),
    File(ComposerFile),
}

/// Attachments read off the UI thread, or why one was refused.
pub(crate) type Incoming = Arc<Mutex<Vec<Result<Attached, String>>>>;

/// Finished file uploads: the file's id, and the stored attachment id or
/// why the upload failed.
pub(crate) type Uploads = Arc<Mutex<Vec<(String, Result<String, String>)>>>;

/// A file's mime type, by extension; `application/octet-stream` otherwise.
fn file_mime(name: &str) -> &'static str {
    let extension = name.rsplit('.').next().unwrap_or_default().to_lowercase();
    match extension.as_str() {
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "xml" => "application/xml",
        "yaml" | "yml" => "application/yaml",
        "rtf" => "application/rtf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "odt" => "application/vnd.oasis.opendocument.text",
        "ods" => "application/vnd.oasis.opendocument.spreadsheet",
        "odp" => "application/vnd.oasis.opendocument.presentation",
        "epub" => "application/epub+zip",
        "zip" => "application/zip",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "heic" => "image/heic",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        _ => "application/octet-stream",
    }
}

/// A file for the composer: any readable, non-empty file within the cap.
fn composer_file(name: &str, path: &std::path::Path) -> Result<ComposerFile, String> {
    let size = std::fs::metadata(path)
        .map_err(|_| format!("'{name}' could not be read."))?
        .len();
    if size == 0 {
        return Err(format!("'{name}' is empty."));
    }
    if size > MAX_FILE_BYTES {
        return Err(format!("'{name}' is too large to attach (50 MB at most)."));
    }
    Ok(ComposerFile {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_owned(),
        mime_type: file_mime(name).to_owned(),
        size,
        path: path.to_owned(),
        upload: None,
    })
}

/// "1.2 MB", "340 KB", "12 B".
pub(crate) fn format_size(bytes: u64) -> String {
    match bytes {
        0..1_000 => format!("{bytes} B"),
        1_000..1_000_000 => format!("{:.0} KB", bytes as f64 / 1_000.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_000_000.0),
    }
}

/// A file attachment's short type: its extension, else "File".
pub(crate) fn file_kind(name: &str) -> String {
    match name.rsplit_once('.') {
        Some((_, extension)) if (1..=5).contains(&extension.len()) => extension.to_uppercase(),
        _ => "File".to_owned(),
    }
}

/// A file chip (`ComposerFileAttachmentChip`): the file's icon tile, its
/// name and a detail line.
pub(crate) const FILE_CHIP: Vec2 = Vec2::new(220.0, 52.0);

pub(crate) fn paint_file_chip(
    ui: &Ui,
    rect: Rect,
    name: &str,
    detail: &str,
    detail_color: egui::Color32,
) {
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(12),
        crate::theme::background(),
        egui::Stroke::new(1.0, crate::theme::alpha(crate::theme::border(), 0.8)),
        egui::StrokeKind::Inside,
    );
    let tile = Rect::from_min_size(
        egui::Pos2::new(rect.left() + 8.0, rect.center().y - 18.0),
        Vec2::splat(36.0),
    );
    ui.painter().rect_filled(
        tile,
        egui::CornerRadius::same(8),
        crate::theme::alpha(crate::theme::primary(), 0.85),
    );
    crate::theme::paint_icon(ui, "file-text", tile, 18.0, egui::Color32::WHITE);
    let text_left = tile.right() + 10.0;
    let text_right = rect.right() - 28.0;
    crate::ui::widgets::text_in(
        ui,
        Rect::from_min_max(
            egui::Pos2::new(text_left, rect.top() + 7.0),
            egui::Pos2::new(text_right, rect.top() + 27.0),
        ),
        name,
        crate::theme::font(13.0, crate::theme::Weight::Medium),
        crate::theme::foreground(),
    );
    crate::ui::widgets::text_in(
        ui,
        Rect::from_min_max(
            egui::Pos2::new(text_left, rect.top() + 27.0),
            egui::Pos2::new(text_right, rect.bottom() - 7.0),
        ),
        detail,
        crate::theme::font(12.0, crate::theme::Weight::Regular),
        detail_color,
    );
}

/// The file chips' rows within `width`: chips per row and the block's size.
pub(crate) fn file_grid(count: usize, width: f32) -> (usize, Vec2) {
    if count == 0 {
        return (1, Vec2::ZERO);
    }
    let per_row = (((width + 8.0) / (FILE_CHIP.x + 8.0)).floor() as usize).max(1);
    let rows = count.div_ceil(per_row) as f32;
    let columns = count.min(per_row) as f32;
    (
        per_row,
        Vec2::new(
            columns * (FILE_CHIP.x + 8.0) - 8.0,
            rows * (FILE_CHIP.y + 8.0) - 8.0,
        ),
    )
}

/// The file attachments of a message.
pub(crate) fn files(attachments: &[Value]) -> Vec<&Value> {
    attachments.iter().filter(|a| a["type"] == "file").collect()
}

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
            composer_image(&format!("image.{extension}"), &mime, bytes).map(Attached::Image)
        });
        if let Ok(mut incoming) = incoming.lock() {
            incoming.push(image);
        }
        ctx.request_repaint();
    });
}

/// Reads dropped files on a background thread.
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
        let picked = rfd::FileDialog::new().pick_files();
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

/// Reads files on a background thread: supported images within the image
/// cap show inline, anything else goes as a file.
fn add_files(incoming: &Incoming, ctx: &egui::Context, paths: Vec<std::path::PathBuf>) {
    let incoming = incoming.clone();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "file".to_owned());
            let image = match mime_for_name(&name) {
                Some(mime)
                    if std::fs::metadata(&path)
                        .is_ok_and(|m| m.len() <= MAX_IMAGE_BYTES as u64) =>
                {
                    match std::fs::read(&path) {
                        Ok(bytes) => composer_image(&name, mime, bytes).map(Attached::Image),
                        Err(_) => Err(format!("'{name}' could not be read as an image.")),
                    }
                }
                _ => composer_file(&name, &path).map(Attached::File),
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
    /// Moves read attachments into the composer, starting file uploads and
    /// reporting refusals, then records finished uploads.
    pub(crate) fn take_incoming_images(&mut self) {
        let arrived: Vec<_> = match self.incoming_images.lock() {
            Ok(mut incoming) => incoming.drain(..).collect(),
            Err(_) => return,
        };
        for attached in arrived {
            match attached {
                Ok(_)
                    if self.composer_images.len() + self.composer_files.len()
                        >= MAX_ATTACHMENTS =>
                {
                    self.notify(
                        crate::ui::toasts::Kind::Error,
                        format!("You can attach up to {MAX_ATTACHMENTS} files per message."),
                    );
                }
                Ok(Attached::Image(image)) => self.composer_images.push(image),
                Ok(Attached::File(file)) => {
                    // `attachments.createUploadUrl`, then the bytes to it.
                    self.send_request(
                        format!("file-upload:{}", file.id),
                        "attachments.createUploadUrl",
                        json!({"type": "file", "name": file.name, "mimeType": file.mime_type,
                            "sizeBytes": file.size}),
                    );
                    self.composer_files.push(file);
                }
                Err(error) => self.notify(crate::ui::toasts::Kind::Error, error),
            }
        }
        let finished: Vec<_> = match self.file_uploads.lock() {
            Ok(mut uploads) => uploads.drain(..).collect(),
            Err(_) => return,
        };
        for (id, result) in finished {
            match self.composer_files.iter_mut().find(|file| file.id == id) {
                Some(file) => file.upload = Some(result),
                // Removed while it uploaded.
                None => {
                    if let Ok(attachment) = result {
                        self.delete_upload(&attachment);
                    }
                }
            }
        }
    }

    /// An upload URL arrived: send the file's bytes to it off the UI thread.
    pub(crate) fn file_upload_result(&mut self, key: &str, result: &Result<Value, String>) -> bool {
        if key.starts_with("file-delete:") {
            return true;
        }
        let Some(id) = key.strip_prefix("file-upload:") else {
            return false;
        };
        let base = url::Url::parse(self.connect.server.trim()).ok();
        let target = result.as_ref().map_err(Clone::clone).and_then(|value| {
            let attachment = value["attachmentId"]
                .as_str()
                .ok_or("No upload was created.")?;
            let url = value["relativeUrl"]
                .as_str()
                .and_then(|relative| base?.join(relative).ok())
                .ok_or("The upload URL is invalid.")?;
            Ok((attachment.to_owned(), url))
        });
        let Some(file) = self.composer_files.iter_mut().find(|file| file.id == id) else {
            if let Ok((attachment, _)) = target {
                self.delete_upload(&attachment);
            }
            return true;
        };
        let (attachment, url) = match target {
            Ok(target) => target,
            Err(error) => {
                file.upload = Some(Err(error));
                return true;
            }
        };
        let (id, path, mime) = (file.id.clone(), file.path.clone(), file.mime_type.clone());
        let uploads = self.file_uploads.clone();
        let ctx = self.egui_ctx.clone();
        std::thread::spawn(move || {
            let sent = std::fs::read(&path)
                .map_err(|_| "The file could not be read.".to_owned())
                .and_then(|bytes| crate::http::post(&url, &mime, &bytes));
            if let Ok(mut uploads) = uploads.lock() {
                uploads.push((id, sent.map(|()| attachment)));
            }
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
        true
    }

    /// Drops a pending upload the message will not send.
    pub(crate) fn delete_upload(&mut self, attachment: &str) {
        self.send_request(
            format!("file-delete:{attachment}"),
            "attachments.delete",
            json!({"attachmentId": attachment}),
        );
    }

    /// The composer's files as `file` attachments, or why the message has
    /// to wait.
    pub(crate) fn file_attachments(&self) -> Result<Vec<Value>, String> {
        self.composer_files
            .iter()
            .map(|file| match &file.upload {
                None => Err("Wait for the files to finish uploading.".to_owned()),
                Some(Err(error)) => Err(format!(
                    "'{}' could not be uploaded: {error} Remove it and attach it again.",
                    file.name
                )),
                Some(Ok(id)) => Ok(json!({"type": "file", "id": id, "name": file.name,
                    "mimeType": file.mime_type, "sizeBytes": file.size})),
            })
            .collect()
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
    fn files_carry_their_type_and_size() {
        assert_eq!(file_mime("Rapport.PDF"), "application/pdf");
        assert_eq!(file_mime("notes"), "application/octet-stream");
        assert_eq!(file_kind("Rapport.pdf"), "PDF");
        assert_eq!(file_kind("backup.snapshot"), "File");
        assert_eq!(file_kind("README"), "File");
        assert_eq!(format_size(12), "12 B");
        assert_eq!(format_size(340_000), "340 KB");
        assert_eq!(format_size(1_250_000), "1.2 MB");
        let (per_row, size) = file_grid(3, 460.0);
        assert_eq!(per_row, 2);
        assert_eq!(size, Vec2::new(448.0, 112.0));
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
