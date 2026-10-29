//! Pictures for Compose: Lucide markup as path data, the providers' marks
//! and the server's SVG favicons rendered to PNG (resvg), raster images
//! passed through for Android to decode.

/// The path data of each element of an icon's markup (`path`, `circle`,
/// `rect`, `line`, `polyline`, `polygon`, `ellipse`), in its 24px box.
pub fn icon_paths(markup: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = markup;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start + 1..start + end];
        rest = &rest[start + end + 1..];
        let tag = tag.trim().trim_end_matches('/').trim();
        let (name, attributes) = tag.split_once(char::is_whitespace).unwrap_or((tag, ""));
        let get = |key: &str| attribute(attributes, key);
        let num = |key: &str| get(key).and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(0.0);
        let path = match name {
            "path" => get("d"),
            "circle" => Some(ellipse(num("cx"), num("cy"), num("r"), num("r"))),
            "ellipse" => Some(ellipse(num("cx"), num("cy"), num("rx"), num("ry"))),
            "line" => Some(format!(
                "M{} {}L{} {}",
                num("x1"),
                num("y1"),
                num("x2"),
                num("y2")
            )),
            "polyline" | "polygon" => get("points").map(|points| {
                let numbers: Vec<&str> = points
                    .split(|c: char| c == ',' || c.is_whitespace())
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut d = String::new();
                for (index, pair) in numbers.chunks(2).enumerate() {
                    if let [x, y] = pair {
                        d.push_str(&format!("{}{x} {y}", if index == 0 { 'M' } else { 'L' }));
                    }
                }
                if name == "polygon" {
                    d.push('Z');
                }
                d
            }),
            "rect" => {
                let (x, y, w, h) = (num("x"), num("y"), num("width"), num("height"));
                let rx = get("rx").map(|_| num("rx"));
                let ry = get("ry").map(|_| num("ry"));
                let (rx, ry) = match (rx, ry) {
                    (Some(rx), Some(ry)) => (rx, ry),
                    (Some(r), None) | (None, Some(r)) => (r, r),
                    (None, None) => (0.0, 0.0),
                };
                let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
                Some(if rx > 0.0 || ry > 0.0 {
                    format!(
                        "M{} {}H{}A{rx} {ry} 0 0 1 {} {}V{}A{rx} {ry} 0 0 1 {} {}H{}A{rx} {ry} 0 0 1 {x} {}V{}A{rx} {ry} 0 0 1 {} {y}Z",
                        x + rx,
                        y,
                        x + w - rx,
                        x + w,
                        y + ry,
                        y + h - ry,
                        x + w - rx,
                        y + h,
                        x + rx,
                        y + h - ry,
                        y + ry,
                        x + rx,
                    )
                } else {
                    format!("M{x} {y}H{}V{}H{x}Z", x + w, y + h)
                })
            }
            _ => None,
        };
        if let Some(path) = path {
            out.push(path);
        }
    }
    out
}

fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> String {
    format!(
        "M{} {cy}A{rx} {ry} 0 1 0 {} {cy}A{rx} {ry} 0 1 0 {} {cy}Z",
        cx - rx,
        cx + rx,
        cx - rx
    )
}

fn attribute(attributes: &str, key: &str) -> Option<String> {
    let mut rest = attributes;
    while let Some(at) = rest.find(key) {
        let before = rest[..at].chars().next_back();
        let after = &rest[at + key.len()..];
        if before.is_none_or(char::is_whitespace)
            && let Some(value) = after.trim_start().strip_prefix('=')
        {
            let value = value.trim_start();
            let quote = value.chars().next()?;
            let body = &value[quote.len_utf8()..];
            return body.find(quote).map(|end| body[..end].to_owned());
        }
        rest = after;
    }
    None
}

/// What Android can decode as is, or an SVG rendered to PNG; `None` for
/// anything that is not an image.
pub fn raster(bytes: &[u8]) -> Option<Vec<u8>> {
    let raster = bytes.starts_with(b"\x89PNG")
        || bytes.starts_with(&[0xff, 0xd8])
        || bytes.starts_with(b"GIF8")
        || (bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(&[0, 0, 1, 0]);
    if raster {
        return Some(bytes.to_vec());
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]).to_lowercase();
    head.contains("<svg").then(|| svg_png(bytes, 192)).flatten()
}

/// An SVG rendered `px` wide (or tall, the longer side) as PNG.
pub fn svg_png(bytes: &[u8], px: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = px as f32 / size.width().max(size.height());
    let width = (size.width() * scale).ceil().max(1.0) as u32;
    let height = (size.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap.encode_png().ok()
}

/// A provider's mark (`catalog::mark`) as SVG.
pub fn mark(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "claude" => include_bytes!("../assets/marks/claude.svg"),
        "openai" => include_bytes!("../assets/marks/openai.svg"),
        "opencode" => include_bytes!("../assets/marks/opencode.svg"),
        "provider-copilot" => include_bytes!("../assets/marks/provider-copilot.svg"),
        "provider-cursor" => include_bytes!("../assets/marks/provider-cursor.svg"),
        "provider-gemini" => include_bytes!("../assets/marks/provider-gemini.svg"),
        "provider-grok" => include_bytes!("../assets/marks/provider-grok.svg"),
        "provider-kimi-dark" => include_bytes!("../assets/marks/provider-kimi-dark.svg"),
        "provider-mistral-vibe" => include_bytes!("../assets/marks/provider-mistral-vibe.svg"),
        "provider-ollama" => include_bytes!("../assets/marks/provider-ollama.svg"),
        "provider-pi" => include_bytes!("../assets/marks/provider-pi.svg"),
        "provider-zcode" => include_bytes!("../assets/marks/provider-zcode.svg"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elements_become_paths() {
        let paths = icon_paths(
            "<circle cx=\"12\" cy=\"12\" r=\"10\" /> <line x1=\"12\" x2=\"12\" y1=\"8\" y2=\"12\" /><rect width=\"20\" height=\"5\" x=\"2\" y=\"3\" rx=\"1\" /><path d=\"M4 8v11\"/>",
        );
        assert_eq!(paths.len(), 4);
        assert_eq!(paths[0], "M2 12A10 10 0 1 0 22 12A10 10 0 1 0 2 12Z");
        assert_eq!(paths[1], "M12 8L12 12");
        assert!(paths[2].starts_with("M3 3H21A1 1"));
        assert_eq!(paths[3], "M4 8v11");
    }

    #[test]
    fn every_mark_renders() {
        for name in [
            "claude",
            "openai",
            "opencode",
            "provider-copilot",
            "provider-cursor",
            "provider-gemini",
            "provider-grok",
            "provider-kimi-dark",
            "provider-mistral-vibe",
            "provider-ollama",
            "provider-pi",
            "provider-zcode",
        ] {
            let png = svg_png(mark(name).unwrap(), 64).unwrap_or_default();
            assert!(png.starts_with(b"\x89PNG"), "{name}");
        }
        assert!(raster(b"not an image").is_none());
    }
}
