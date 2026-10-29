//! Mermaid diagrams in answers (`MarkdownMermaidCodeBlock`,
//! `chat/MermaidDiagram.tsx`), drawn natively: `merman` lays the source out
//! as resvg-safe SVG off the UI thread, and `resvg` rasterizes it at twice
//! its size with the app's Geist fonts. Settled renders stay cached (64, by
//! theme and source).

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui;

#[derive(Clone)]
pub(crate) enum Diagram {
    Rendering,
    Ready {
        image: Arc<egui::ColorImage>,
        size: egui::Vec2,
    },
    Failed(String),
}

type RenderCache = (HashMap<u64, Diagram>, Vec<u64>);

fn cache() -> &'static Mutex<RenderCache> {
    static CACHE: OnceLock<Mutex<RenderCache>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The diagram's texture, uploaded once.
pub(crate) fn texture(
    ctx: &egui::Context,
    key: u64,
    image: &egui::ColorImage,
) -> egui::TextureHandle {
    let cache_id = egui::Id::new("mermaid-textures");
    if let Some(texture) = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<HashMap<u64, egui::TextureHandle>>(cache_id)
            .get(&key)
            .cloned()
    }) {
        return texture;
    }
    // load_texture also reads the Context; do not hold its data lock here.
    let texture = ctx.load_texture(
        format!("mermaid-{key:x}"),
        image.clone(),
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| {
        let textures = data.get_temp_mut_or_default::<HashMap<u64, egui::TextureHandle>>(cache_id);
        if textures.len() >= 64 {
            textures.clear();
        }
        textures.insert(key, texture.clone());
    });
    texture
}

/// The SVG's own size, from its `viewBox`.
fn svg_size(svg: &str) -> Option<egui::Vec2> {
    let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
    let end = svg[start..].find('"')? + start;
    let values: Vec<f32> = svg[start..end]
        .split([' ', ','])
        .filter_map(|v| v.parse().ok())
        .collect();
    match values[..] {
        [_, _, w, h] if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => {
            Some(egui::vec2(w, h))
        }
        _ => None,
    }
}

/// The fonts the diagrams' text is set in.
fn fonts() -> Arc<resvg::usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<resvg::usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = resvg::usvg::fontdb::Database::new();
            for font in [
                &include_bytes!("../../assets/fonts/Geist-Regular.otf")[..],
                &include_bytes!("../../assets/fonts/Geist-Medium.otf")[..],
                &include_bytes!("../../assets/fonts/Geist-SemiBold.otf")[..],
                &include_bytes!("../../assets/fonts/Geist-Bold.otf")[..],
                &include_bytes!("../../assets/fonts/GeistMono-Regular.otf")[..],
            ] {
                db.load_font_data(font.to_vec());
            }
            db.set_sans_serif_family("Geist");
            db.set_serif_family("Geist");
            db.set_monospace_family("Geist Mono");
            Arc::new(db)
        })
        .clone()
}

/// The SVG as pixels at twice its size (sharp on a 2× screen), 2400px on either
/// axis at most.
fn rasterize(svg: &str) -> Result<egui::ColorImage, String> {
    use resvg::{tiny_skia, usvg};
    let options = usvg::Options {
        fontdb: fonts(),
        font_family: "Geist".into(),
        // Like upstream's sanitizer, diagrams never load external images.
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| e.to_string())?;
    let size = tree.size();
    let scale = (2400.0 / size.width().max(size.height())).min(2.0);
    let (w, h) = (
        (size.width() * scale).ceil() as u32,
        (size.height() * scale).ceil() as u32,
    );
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1)).ok_or("empty diagram")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(egui::ColorImage::from_rgba_premultiplied(
        [pixmap.width() as usize, pixmap.height() as usize],
        pixmap.data(),
    ))
}

fn render(source: &str, dark: bool) -> Result<(String, egui::Vec2), String> {
    use merman::svg::SvgPipeline;
    use merman::{
        Engine, MermaidConfig, OperationControl, RenderOutput, RenderRequest, Renderer, SvgRequest,
    };
    let config = MermaidConfig::from_value(serde_json::json!({
        "theme": if dark { "dark" } else { "default" },
        "securityLevel": "strict",
        "fontFamily": "Geist",
        "secure": ["secure", "securityLevel", "htmlLabels", "themeCSS", "maxTextSize", "maxEdges"],
        "htmlLabels": false,
        "flowchart": {"htmlLabels": false},
    }));
    let renderer = Renderer::new().with_engine(Engine::new().with_site_config(config));
    let request = SvgRequest {
        pipeline: Some(SvgPipeline::resvg_safe()),
        ..SvgRequest::default()
    };
    let control = OperationControl::new().with_deadline(std::time::Duration::from_secs(5));
    match renderer.render(RenderRequest::svg(source, control, request)) {
        Ok(RenderOutput::Svg(Some(svg))) => {
            let svg = svg.into_parts().0;
            let size = svg_size(&svg).unwrap_or(egui::vec2(480.0, 270.0));
            Ok((svg, size))
        }
        Ok(_) => Err("no Mermaid diagram detected".into()),
        Err(error) => Err(error.to_string()),
    }
}

/// Keep pending entries stable, and cap settled pixels as well as entry count.
fn evict(cache: &mut RenderCache, limit: usize) {
    let mut bytes: usize = cache
        .0
        .values()
        .map(|entry| match entry {
            Diagram::Ready { image, .. } => image.pixels.len() * 4,
            _ => 0,
        })
        .sum();
    while cache.0.len() > limit || bytes > 128 * 1024 * 1024 {
        let Some(index) = cache
            .1
            .iter()
            .position(|key| !matches!(cache.0.get(key), Some(Diagram::Rendering)))
        else {
            break;
        };
        let key = cache.1.remove(index);
        if let Some(Diagram::Ready { image, .. }) = cache.0.remove(&key) {
            bytes -= image.pixels.len() * 4;
        }
    }
}

/// Expand the pixels already rendered; no file, URL or second render needed.
pub(crate) fn expand(ctx: &egui::Context, key: u64, image: &egui::ColorImage) {
    let pixels: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
    if let Some(image) =
        image::RgbaImage::from_raw(image.width() as u32, image.height() as u32, pixels)
    {
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::new("expanded-mermaid"),
                (format!("mermaid-{key:x}"), Arc::new(image)),
            )
        });
    }
}

/// The diagram for `source`, rendered in the background the first time.
pub(crate) fn diagram(ctx: &egui::Context, source: &str, dark: bool) -> (u64, Diagram) {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (source.trim(), dark).hash(&mut hasher);
    let key = hasher.finish();
    {
        let Ok(mut known) = cache().lock() else {
            return (key, Diagram::Rendering);
        };
        if let Some(found) = known.0.get(&key) {
            return (key, found.clone());
        }
        // Streaming answers stay as code. Still bound the work when a history
        // page contains many finished diagrams: only two render at a time.
        if known
            .0
            .values()
            .filter(|v| matches!(v, Diagram::Rendering))
            .count()
            >= 2
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return (key, Diagram::Rendering);
        }
        evict(&mut known, 63);
        known.0.insert(key, Diagram::Rendering);
        known.1.push(key);
    }
    let source = source.trim().to_owned();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let state = match render(&source, dark).and_then(|(svg, size)| Ok((rasterize(&svg)?, size)))
        {
            Ok((image, size)) => Diagram::Ready {
                image: Arc::new(image),
                size,
            },
            Err(error) => Diagram::Failed(error),
        };
        if let Ok(mut cache) = cache().lock() {
            cache.0.insert(key, state);
            evict(&mut cache, 64);
        }
        ctx.request_repaint();
    });
    (key, Diagram::Rendering)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flowcharts_render_to_svg() {
        let (svg, size) = render("flowchart LR\n  A[Start] --> B[Done]", true).unwrap();
        assert!(svg.contains("<svg"));
        assert!(size.x > 0.0 && size.y > 0.0);
        assert!(!svg.contains("foreignObject"));
        let image = rasterize(&svg).unwrap();
        assert!(image.size[0] > 0);
        assert!(render("not a diagram at all", true).is_err());
        assert_eq!(
            svg_size(r#"<svg viewBox="0 0 120.5 40">"#),
            Some(egui::vec2(120.5, 40.0))
        );
    }
    #[test]
    fn tall_diagrams_and_invalid_sizes_are_bounded() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="100000"><rect width="10" height="100000" fill="red"/></svg>"#;
        let image = rasterize(svg).unwrap();
        assert!(image.width() <= 2400 && image.height() <= 2400);
        assert_eq!(svg_size(r#"<svg viewBox="0 0 inf 40">"#), None);
        assert!(render("flowchart LR\n A[unclosed", true).is_err());
        let (svg, _) = render(
            "sequenceDiagram\n User->>Pluto: hello\n Pluto-->>User: done",
            false,
        )
        .unwrap();
        assert!(rasterize(&svg).unwrap().pixels.iter().any(|p| p.a() > 0));
    }

    #[test]
    fn cache_does_not_evict_pending_renders() {
        let mut cache = RenderCache::default();
        cache.0.insert(1, Diagram::Rendering);
        cache.0.insert(2, Diagram::Failed("invalid".into()));
        cache.1.extend([1, 2]);
        evict(&mut cache, 1);
        assert!(matches!(cache.0.get(&1), Some(Diagram::Rendering)));
        assert_eq!(cache.1, vec![1]);
    }

    #[test]
    fn diagram_ui_streams_as_code_then_expands_and_toggles() {
        use egui_kittest::Harness;
        use std::sync::atomic::{AtomicBool, Ordering};
        let streaming = Arc::new(AtomicBool::new(true));
        let live = streaming.clone();
        let mut harness = Harness::builder().with_size(egui::vec2(720.0, 700.0)).build_ui(move |ui| {
            if ui.ctx().cumulative_frame_nr() == 0 {
                crate::theme::install(ui.ctx());
                ui.ctx().add_plugin(crate::ui::widgets::CopyCleanup::default());
                return;
            }
            let mut highlighter = crate::highlight::Highlighter::new(ui.ctx().clone());
            let mut markdown = crate::ui::markdown::Markdown::new(&mut highlighter, egui::Id::new("mermaid-test"));
            markdown.streaming = live.load(Ordering::Relaxed);
            markdown.show(ui, "```mermaid\nflowchart LR\n A[Client] -->|WebSocket| B[Server]\n B --> C{Provider}\n C --> D[Turn]\n```\n\n```mermaid\nsequenceDiagram\n User->>Pluto: send\n Pluto->>Server: dispatch\n Server-->>Pluto: events\n```", 680.0);
        });
        let has_text = |h: &Harness<'_, ()>, wanted: &str| {
            h.output().shapes.iter().any(|s| match &s.shape {
                egui::Shape::Text(t) => t.galley.text().contains(wanted),
                _ => false,
            })
        };
        harness.run_steps(3);
        assert!(has_text(&harness, "flowchart"));
        streaming.store(false, Ordering::Relaxed);
        // Background layout/rasterization has a real deadline; UI steps do
        // not wait for it. No network or provider session is involved.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            harness.run_steps(2);
            if !has_text(&harness, "Rendering diagram") {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(!has_text(&harness, "Unable to render diagram"));
        assert!(!has_text(&harness, "flowchart"));
        if let Ok(dir) = std::env::var("PLUTO_MERMAID_SNAPSHOT_DIR") {
            harness
                .render()
                .unwrap()
                .save(std::path::Path::new(&dir).join("mermaid.png"))
                .unwrap();
        }
        let click = |h: &mut Harness<'_, ()>, pos| {
            h.event(egui::Event::PointerMoved(pos));
            h.run_steps(1);
            for pressed in [true, false] {
                h.event(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            h.run_steps(2);
        };
        click(&mut harness, egui::pos2(340.0, 95.0));
        assert!(
            harness
                .ctx
                .data(|d| d
                    .get_temp::<(String, Arc<image::RgbaImage>)>(egui::Id::new("expanded-mermaid")))
                .is_some()
        );
        click(&mut harness, egui::pos2(644.0, 26.0));
        assert!(has_text(&harness, "flowchart"));
    }
}
