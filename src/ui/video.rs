//! Video playback (`media/MediaVideoPlayer.tsx`, `packages/shared/video.ts`)
//! through `ffprobe` and `ffmpeg`: the size and length, a first frame for
//! the poster, then frames piped as RGBA while a second `ffmpeg` plays the
//! sound to PulseAudio. The controls (play, pause, seek bar, time, mute)
//! are drawn here. Markdown videos play in place (30rem at most); a user's
//! video attachment shows its poster with a play disc and opens large.
//!
//! Not ported: fullscreen, the media menu ("Save video"), and the
//! pause-when-hidden rule (a player not drawn for half a second stops).

use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Color32, ColorImage, CornerRadius, Pos2, Rect, Sense, TextureHandle, TextureOptions, Ui,
    Vec2,
};

use crate::theme::{self, Weight, font};
use crate::ui::widgets;

/// `videoMimeType`: a `video/*` type, or a video extension behind a
/// generic or missing type.
pub(crate) fn is_video(name: &str, mime: Option<&str>) -> bool {
    let mime = mime.unwrap_or_default().trim().to_lowercase();
    if mime.starts_with("video/") {
        return true;
    }
    let generic = matches!(
        mime.as_str(),
        "" | "application/octet-stream" | "binary/octet-stream" | "application/unknown"
    );
    generic && is_video_path(name)
}

pub(crate) fn is_video_path(path: &str) -> bool {
    let path = path.split(['?', '#']).next().unwrap_or(path).to_lowercase();
    ["avi", "m4v", "mkv", "mov", "mp4", "ogv", "webm"]
        .iter()
        .any(|ext| path.ends_with(&format!(".{ext}")))
}

#[derive(Default)]
struct Probe {
    size: Option<(u32, u32)>,
    duration: Option<f64>,
    poster: Option<ColorImage>,
    failed: Option<String>,
    done: bool,
}

/// The latest frame from the pipe, numbered.
#[derive(Default)]
struct Frames {
    image: Option<ColorImage>,
    count: u64,
    ended: bool,
}

struct Playback {
    video: Child,
    audio: Option<Child>,
    frames: Arc<Mutex<Frames>>,
    from: f64,
    started: Instant,
}

impl Drop for Playback {
    fn drop(&mut self) {
        let _ = self.video.kill();
        let _ = self.video.wait();
        if let Some(audio) = &mut self.audio {
            let _ = audio.kill();
            let _ = audio.wait();
        }
    }
}

#[derive(Default)]
struct Player {
    /// A path or an http(s) URL; `None` while its signed URL is asked for.
    source: Option<String>,
    unavailable: bool,
    probe: Option<Arc<Mutex<Probe>>>,
    texture: Option<TextureHandle>,
    shown: u64,
    playing: Option<Playback>,
    position: f64,
    muted: bool,
    drawn: Option<Instant>,
}

fn players() -> &'static Mutex<HashMap<String, Player>> {
    static PLAYERS: OnceLock<Mutex<HashMap<String, Player>>> = OnceLock::new();
    PLAYERS.get_or_init(Default::default)
}

/// Whether `key` already knows where its video is (or that it has none).
pub(crate) fn known(key: &str) -> bool {
    players().lock().is_ok_and(|p| {
        p.get(key)
            .is_some_and(|p| p.source.is_some() || p.unavailable)
    })
}

/// Where `key`'s video is: a path, a URL, or none (unavailable).
pub(crate) fn set_source(key: &str, source: Option<String>) {
    if let Ok(mut players) = players().lock() {
        let player = players.entry(key.to_owned()).or_default();
        player.unavailable = source.is_none();
        player.source = source;
    }
}

/// Marks `key` as asked for, so its URL is requested once.
pub(crate) fn mark_requested(key: &str) {
    if let Ok(mut players) = players().lock() {
        players.entry(key.to_owned()).or_default();
    }
}

pub(crate) fn requested(key: &str) -> bool {
    players().lock().is_ok_and(|p| p.contains_key(key))
}

/// Stops players nobody drew for half a second.
pub(crate) fn sweep() {
    if let Ok(mut players) = players().lock() {
        for player in players.values_mut() {
            let stale = player
                .drawn
                .is_none_or(|at| at.elapsed() > Duration::from_millis(500));
            if stale && let Some(playing) = player.playing.take() {
                player.position = playing.from + playing.started.elapsed().as_secs_f64();
            }
        }
    }
}

/// `ffprobe`'s size and length, and a frame near the start for the poster.
fn start_probe(source: String, ctx: egui::Context) -> Arc<Mutex<Probe>> {
    let probe = Arc::new(Mutex::new(Probe::default()));
    let shared = probe.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<Probe, String> {
            let output = Command::new("ffprobe")
                .args([
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_entries",
                    "stream=width,height:format=duration",
                    "-of",
                    "json",
                    &source,
                ])
                .stdin(Stdio::null())
                .output()
                .map_err(|_| "ffprobe is not installed".to_owned())?;
            let value: serde_json::Value =
                serde_json::from_slice(&output.stdout).map_err(|_| "not a video".to_owned())?;
            let stream = &value["streams"][0];
            let (w, h) = (stream["width"].as_u64(), stream["height"].as_u64());
            let (Some(w), Some(h)) = (w, h) else {
                return Err("not a video".into());
            };
            let duration = value["format"]["duration"]
                .as_str()
                .and_then(|d| d.parse::<f64>().ok());
            let (w, h) = (w as u32, h as u32);
            let (fw, fh) = frame_size((w, h), 960);
            let at = duration.map_or(0.1, |d| (d / 2.0).min(0.1));
            let poster = Command::new("ffmpeg")
                .args(["-v", "error", "-ss", &format!("{at:.3}"), "-i", &source])
                .args(["-frames:v", "1", "-vf", &format!("scale={fw}:{fh}")])
                .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
                .stdin(Stdio::null())
                .output()
                .ok()
                .filter(|o| o.stdout.len() == (fw * fh * 4) as usize)
                .map(|o| ColorImage::from_rgba_unmultiplied([fw as usize, fh as usize], &o.stdout));
            Ok(Probe {
                size: Some((w, h)),
                duration,
                poster,
                failed: None,
                done: true,
            })
        })();
        let probe = result.unwrap_or_else(|error| Probe {
            failed: Some(error),
            done: true,
            ..Default::default()
        });
        if let Ok(mut shared) = shared.lock() {
            *shared = probe;
        }
        ctx.request_repaint();
    });
    probe
}

/// Frames at most `max` wide, even sides (what `scale` wants).
fn frame_size((w, h): (u32, u32), max: u32) -> (u32, u32) {
    let fw = w.min(max).max(2);
    let fh = ((h as f64 * fw as f64 / w.max(1) as f64).round() as u32).max(2);
    (fw & !1, fh & !1)
}

fn start_playback(
    source: &str,
    size: (u32, u32),
    from: f64,
    muted: bool,
    ctx: &egui::Context,
) -> Option<Playback> {
    let (fw, fh) = frame_size(size, 960);
    let mut video = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-re",
            "-ss",
            &format!("{from:.3}"),
            "-i",
            source,
        ])
        .args(["-an", "-vf", &format!("scale={fw}:{fh}")])
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let audio = (!muted)
        .then(|| {
            Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-re",
                    "-ss",
                    &format!("{from:.3}"),
                    "-i",
                    source,
                ])
                .args(["-vn", "-f", "pulse", "Pluto"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()
        })
        .flatten();
    let frames = Arc::new(Mutex::new(Frames::default()));
    let shared = frames.clone();
    let mut stdout = video.stdout.take()?;
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let mut buffer = vec![0u8; (fw * fh * 4) as usize];
        loop {
            if stdout.read_exact(&mut buffer).is_err() {
                if let Ok(mut frames) = shared.lock() {
                    frames.ended = true;
                }
                ctx.request_repaint();
                return;
            }
            let image = ColorImage::from_rgba_unmultiplied([fw as usize, fh as usize], &buffer);
            if let Ok(mut frames) = shared.lock() {
                frames.image = Some(image);
                frames.count += 1;
            }
            ctx.request_repaint();
        }
    });
    Some(Playback {
        video,
        audio,
        frames,
        from,
        started: Instant::now(),
    })
}

fn clock(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    format!("{}:{:02}", total / 60, total % 60)
}

/// The video's natural size once probed.
pub(crate) fn natural_size(key: &str) -> Option<(u32, u32)> {
    let players = players().lock().ok()?;
    let probe = players.get(key)?.probe.clone()?;
    let probe = probe.lock().ok()?;
    probe.size
}

/// Draws `key`'s player in `rect`. `thumbnail`: the poster and a play disc
/// only, whose click is returned.
pub(crate) fn player(ui: &mut Ui, key: &str, rect: Rect, label: &str, thumbnail: bool) -> bool {
    let Ok(mut players) = players().lock() else {
        return false;
    };
    let player = players.entry(key.to_owned()).or_default();
    player.drawn = Some(Instant::now());
    let painter = ui.painter().clone();
    let radius = CornerRadius::same(8);
    if player.unavailable {
        unavailable(ui, rect, label);
        return false;
    }
    let Some(source) = player.source.clone() else {
        painter.rect_filled(rect, radius, theme::alpha(theme::muted(), 0.6));
        return false;
    };
    let probe = player
        .probe
        .get_or_insert_with(|| start_probe(source.clone(), ui.ctx().clone()))
        .clone();
    let (size, duration, failed) = {
        let mut probe = match probe.lock() {
            Ok(probe) => probe,
            Err(_) => return false,
        };
        if !probe.done {
            drop(probe);
            painter.rect_filled(rect, radius, theme::alpha(theme::muted(), 0.6));
            return false;
        }
        if let Some(poster) = probe.poster.take() {
            player.texture = Some(ui.ctx().load_texture(
                format!("video-{key}"),
                poster,
                TextureOptions::LINEAR,
            ));
        }
        (probe.size, probe.duration, probe.failed.clone())
    };
    if let Some(error) = failed {
        drop(players);
        unavailable(ui, rect, &format!("{label} · {error}"));
        return false;
    }
    // A new frame from the pipe.
    let mut ended = false;
    if let Some(playing) = &player.playing
        && let Ok(mut frames) = playing.frames.lock()
    {
        ended = frames.ended;
        if frames.count != player.shown
            && let Some(image) = frames.image.take()
        {
            player.shown = frames.count;
            match &mut player.texture {
                Some(texture) => texture.set(image, TextureOptions::LINEAR),
                None => {
                    player.texture = Some(ui.ctx().load_texture(
                        format!("video-{key}"),
                        image,
                        TextureOptions::LINEAR,
                    ))
                }
            }
        }
    }
    let position = match &player.playing {
        Some(playing) => playing.from + playing.started.elapsed().as_secs_f64(),
        None => player.position,
    };
    if ended || duration.is_some_and(|d| position >= d) {
        player.playing = None;
        player.position = 0.0;
    }
    painter.rect_filled(rect, radius, Color32::BLACK);
    if let Some(texture) = &player.texture {
        let image = texture.size_vec2();
        let scale = (rect.width() / image.x).min(rect.height() / image.y);
        let shown = Rect::from_center_size(rect.center(), image * scale);
        painter.image(
            texture.id(),
            shown,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    let response = ui.interact(rect, ui.id().with(("video", key)), Sense::click());
    let disc = |center: Pos2| {
        painter.circle_filled(center, 16.0, Color32::from_black_alpha(128));
        theme::paint_icon(
            ui,
            "play-fill",
            Rect::from_center_size(center + Vec2::new(1.0, 0.0), Vec2::splat(16.0)),
            16.0,
            Color32::WHITE,
        );
    };
    if thumbnail {
        disc(rect.center());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Play {label}"))
        });
        return response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked();
    }
    let playing = player.playing.is_some();
    if playing {
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    } else {
        disc(rect.center());
    }
    // The controls: play/pause, the bar, the time, mute.
    let bar = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - 32.0), rect.max);
    painter.rect_filled(
        bar,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: 8,
            se: 8,
        },
        Color32::from_black_alpha(140),
    );
    let toggle = Rect::from_min_size(bar.min + Vec2::new(6.0, 4.0), Vec2::splat(24.0));
    let toggle_response = widgets::icon_button(
        ui,
        toggle,
        ("video-toggle", key),
        if playing { "pause" } else { "play-fill" },
        14.0,
        Color32::WHITE,
        if playing { "Pause" } else { "Play" },
    );
    let mute = Rect::from_min_size(
        Pos2::new(bar.right() - 30.0, bar.top() + 4.0),
        Vec2::splat(24.0),
    );
    let mute_response = widgets::icon_button(
        ui,
        mute,
        ("video-mute", key),
        if player.muted { "volume-x" } else { "volume-2" },
        14.0,
        Color32::WHITE,
        if player.muted { "Unmute" } else { "Mute" },
    );
    let time = format!(
        "{} / {}",
        clock(position),
        duration.map_or_else(|| "–:––".to_owned(), clock)
    );
    let time_font = font(11.0, Weight::Regular);
    let time_width = widgets::tabular_width(ui, &time, time_font.clone());
    widgets::tabular_text(
        ui,
        Pos2::new(mute.left() - 6.0 - time_width, bar.center().y - 8.0),
        &time,
        time_font,
        Color32::WHITE,
        16.0,
    );
    let track = Rect::from_min_max(
        Pos2::new(toggle.right() + 8.0, bar.center().y - 2.0),
        Pos2::new(mute.left() - 14.0 - time_width, bar.center().y + 2.0),
    );
    painter.rect_filled(track, CornerRadius::same(2), Color32::from_white_alpha(70));
    if let Some(duration) = duration.filter(|d| *d > 0.0) {
        let done = (position / duration).clamp(0.0, 1.0) as f32;
        painter.rect_filled(
            Rect::from_min_max(
                track.min,
                Pos2::new(track.left() + track.width() * done, track.bottom()),
            ),
            CornerRadius::same(2),
            Color32::WHITE,
        );
    }
    let seek = ui.interact(
        track.expand2(Vec2::new(0.0, 8.0)),
        ui.id().with(("video-seek", key)),
        Sense::click(),
    );
    let mut restart = None;
    if let (true, Some(pointer), Some(duration)) =
        (seek.clicked(), seek.interact_pointer_pos(), duration)
    {
        let at = ((pointer.x - track.left()) / track.width()).clamp(0.0, 1.0) as f64 * duration;
        restart = Some((at, playing));
    } else if toggle_response.clicked() || (response.clicked() && !mute_response.hovered()) {
        restart = Some((position, !playing));
    } else if mute_response.clicked() {
        player.muted = !player.muted;
        if playing {
            restart = Some((position, true));
        }
    }
    if let Some((at, play)) = restart {
        player.playing = None;
        player.position = at;
        if play && let Some(size) = size {
            player.playing = start_playback(&source, size, at, player.muted, ui.ctx());
        }
    }
    false
}

/// `MediaVideoPlayer`'s failed state.
fn unavailable(ui: &Ui, rect: Rect, label: &str) {
    ui.painter().rect(
        rect,
        CornerRadius::same(8),
        theme::alpha(theme::muted(), 0.4),
        egui::Stroke::new(1.0, theme::alpha(theme::border(), 0.4)),
        egui::StrokeKind::Inside,
    );
    let text = if label.trim().is_empty() {
        "Video unavailable".to_owned()
    } else {
        format!("Video unavailable · {label}")
    };
    let line = Rect::from_center_size(rect.center(), Vec2::new(rect.width() - 32.0, 20.0));
    theme::paint_icon(
        ui,
        "triangle-alert",
        Rect::from_min_size(line.min, Vec2::new(14.0, 20.0)),
        14.0,
        theme::muted_foreground(),
    );
    widgets::text_in(
        ui,
        Rect::from_min_max(Pos2::new(line.left() + 20.0, line.top()), line.max),
        &text,
        font(14.0, Weight::Regular),
        theme::muted_foreground(),
    );
}

impl crate::app::App {
    /// A video attachment's player key, asking for its signed URL once.
    pub(crate) fn attachment_video(&mut self, attachment: &serde_json::Value) -> Option<String> {
        let id = attachment["id"].as_str()?;
        let key = format!("att-video:{id}");
        if !requested(&key) {
            mark_requested(&key);
            self.send_request(
                format!("video-url:{key}"),
                "assets.createUrl",
                serde_json::json!({"resource": {
                    "_tag": "attachment",
                    "attachmentId": id,
                    "fileName": attachment["name"],
                    "mimeType": attachment["mimeType"],
                }}),
            );
        }
        Some(key)
    }

    /// The video opened large (`ExpandedVideo`): Escape or the backdrop
    /// closes it.
    pub(crate) fn video_dialog(&mut self, ui: &mut Ui, screen: Rect) {
        let Some((key, name)) = self.video_overlay.clone() else {
            return;
        };
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.video_overlay = None;
            return;
        }
        let layer = egui::LayerId::new(egui::Order::Foreground, egui::Id::new("video-dialog"));
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(screen));
        child
            .painter()
            .rect_filled(screen, 0.0, Color32::from_black_alpha(200));
        let backdrop = child.interact(screen, egui::Id::new("video-backdrop"), Sense::click());
        let bounds = screen.shrink2(Vec2::new(screen.width() * 0.1, screen.height() * 0.1));
        let (w, h) = natural_size(&key).unwrap_or((16, 9));
        let scale = (bounds.width() / w as f32).min(bounds.height() / h as f32);
        let rect = Rect::from_center_size(bounds.center(), Vec2::new(w as f32, h as f32) * scale);
        player(&mut child, &key, rect, &name, false);
        let close = Rect::from_min_size(
            Pos2::new(screen.right() - 48.0, screen.top() + 16.0),
            Vec2::splat(32.0),
        );
        let closed = widgets::icon_button(
            &mut child,
            close,
            "video-close",
            "x",
            16.0,
            Color32::WHITE,
            "Close",
        )
        .clicked();
        let outside = backdrop.clicked()
            && backdrop
                .interact_pointer_pos()
                .is_some_and(|p| !rect.contains(p));
        if closed || outside {
            self.video_overlay = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn videos_by_type_then_extension() {
        assert!(is_video("clip.bin", Some("video/mp4")));
        assert!(is_video("clip.webm", Some("application/octet-stream")));
        assert!(is_video("clip.MOV", None));
        assert!(!is_video("clip.mp4", Some("image/png")));
        assert!(!is_video("notes.txt", None));
        assert!(is_video_path("https://a.dev/v.mp4?sig=1"));
        assert_eq!(frame_size((1920, 1080), 960), (960, 540));
        assert_eq!(frame_size((641, 361), 960), (640, 360));
    }
}
