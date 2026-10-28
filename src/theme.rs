use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

// Dark theme tokens of the upstream web client (T3 Code v0.0.45, stock dark
// palette, no contrast boost), converted from oklch/color-mix to sRGB and
// checked against rendered reference screenshots. Geometry is in CSS pixels.
//
// An environment theme (`t3 theme set`, published over the config stream as
// `environmentThemesUpdated`) replaces the palette at runtime; the stock dark
// palette below is the default and stays pixel-exact.
pub const DIFF_ADDITION: Color32 = Color32::from_rgb(0, 212, 146); // --diff-addition-foreground
pub const DIFF_DELETION: Color32 = Color32::from_rgb(255, 100, 103); // --diff-deletion-foreground
const BLUE_400_DIFF: Color32 = Color32::from_rgb(81, 162, 255);
const ORANGE_400: Color32 = Color32::from_rgb(255, 137, 4);

static BLUE_ORANGE_DIFFS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The Diff colors setting (`:root[data-diff-color-scheme="blue-orange"]`).
pub fn set_blue_orange_diffs(on: bool) {
    BLUE_ORANGE_DIFFS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn blue_orange_diffs() -> bool {
    BLUE_ORANGE_DIFFS.load(std::sync::atomic::Ordering::Relaxed)
}

/// `--diff-addition`: change counts and stats.
pub fn diff_addition() -> Color32 {
    if blue_orange_diffs() {
        BLUE_400_DIFF
    } else {
        Color32::from_rgb(0, 188, 125)
    }
}

/// `--diff-deletion`.
pub fn diff_deletion() -> Color32 {
    if blue_orange_diffs() {
        ORANGE_400
    } else {
        Color32::from_rgb(251, 65, 74)
    }
}

/// `--diff-addition-foreground`.
pub fn diff_addition_foreground() -> Color32 {
    if blue_orange_diffs() {
        BLUE_400_DIFF
    } else {
        DIFF_ADDITION
    }
}

/// `--diff-deletion-foreground`.
pub fn diff_deletion_foreground() -> Color32 {
    if blue_orange_diffs() {
        ORANGE_400
    } else {
        DIFF_DELETION
    }
}
pub const SKY_400: Color32 = Color32::from_rgb(0, 188, 255);
pub const INDIGO_300: Color32 = Color32::from_rgb(163, 179, 255);
pub const RED_300: Color32 = Color32::from_rgb(255, 162, 162);
pub const EMERALD_300: Color32 = Color32::from_rgb(94, 233, 181);
pub const BLUE_400: Color32 = Color32::from_rgb(81, 162, 255); // --info-foreground
pub const BLUE_500: Color32 = Color32::from_rgb(43, 127, 255); // --info

/// `color-mix(in oklab, #fff N%, transparent)` on the stock palette.
const fn white(alpha: f32) -> Color32 {
    let a = (alpha * 255.0 + 0.5) as u8;
    Color32::from_rgba_premultiplied(a, a, a, a)
}

/// The app's color tokens (`--background`, `--foreground`, …).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// An environment theme replaced the stock dark palette.
    pub custom: bool,
    /// A dark canvas (code takes `pierre-dark`, else `pierre-light`).
    pub dark: bool,
    pub background: Color32,
    pub sidebar: Color32,
    pub card: Color32,
    pub popover: Color32,
    pub foreground: Color32,
    pub sidebar_foreground: Color32,
    pub muted_foreground: Color32,
    pub secondary_label: Color32,
    pub sidebar_muted: Color32,
    pub sidebar_icon: Color32,
    pub sidebar_border: Color32,
    pub primary: Color32,
    pub primary_hover: Color32,
    pub primary_foreground: Color32,
    pub warning: Color32,
    pub warning_foreground: Color32,
    pub error_foreground: Color32,
    pub border: Color32,
    pub input: Color32,
    pub accent: Color32,
    pub muted: Color32,
    pub message_surface: Color32,
    pub control_surface: Color32,
    pub row_active: Color32,
    pub code_foreground: Color32,
    /// The base of `overlay` tints: white on the stock palette, the text
    /// color in a theme (so light themes darken where dark ones lighten).
    pub overlay: Color32,
}

/// The stock dark palette.
pub const DARK: Palette = Palette {
    custom: false,
    dark: true,
    background: Color32::from_rgb(10, 10, 10),
    sidebar: Color32::from_rgb(4, 4, 4), // rendered sidebar surface
    card: Color32::from_rgb(17, 17, 17),
    popover: Color32::from_rgb(16, 16, 16), // dropdown glass over the app
    foreground: Color32::from_rgb(245, 245, 245),
    sidebar_foreground: Color32::from_rgb(241, 243, 247),
    muted_foreground: Color32::from_rgb(129, 129, 129),
    secondary_label: Color32::from_rgb(129, 129, 129),
    sidebar_muted: Color32::from_rgb(163, 163, 163), // muted text inside the sidebar
    sidebar_icon: Color32::from_rgb(84, 84, 84),     // --sidebar-icon-color
    sidebar_border: Color32::from_rgb(29, 29, 29),   // white 7.8% over the app
    primary: Color32::from_rgb(52, 107, 241),
    primary_hover: Color32::from_rgb(48, 97, 218),
    primary_foreground: Color32::WHITE,
    warning: Color32::from_rgb(254, 154, 0),
    warning_foreground: Color32::from_rgb(255, 185, 0),
    error_foreground: Color32::from_rgb(255, 100, 103),
    border: white(0.06),
    input: white(0.08),
    accent: white(0.04), // --accent, --sidebar-row-hover
    muted: white(0.03),  // --muted, --secondary
    message_surface: white(0.04),
    control_surface: white(0.0256), // `--input` at 32%: outline buttons, code blocks
    row_active: Color32::from_rgba_premultiplied(27, 27, 27, 28), // rgba(241,243,247,.11)
    code_foreground: Color32::from_rgb(250, 250, 250),
    overlay: Color32::WHITE,
};

static PALETTE: std::sync::RwLock<Palette> = std::sync::RwLock::new(DARK);

pub fn palette() -> Palette {
    *PALETTE.read().unwrap_or_else(|e| e.into_inner())
}

/// Installs `palette`; returns whether it changed.
pub fn set_palette(palette: Palette) -> bool {
    let mut current = PALETTE.write().unwrap_or_else(|e| e.into_inner());
    let changed = *current != palette;
    *current = palette;
    changed
}

/// `#rrggbb` or `#rrggbbaa`.
/// `oklch(L C H)` (L as a fraction or a percentage) in sRGB, clamped to
/// the gamut.
fn parse_oklch(value: &str) -> Option<Color32> {
    let inner = value.trim().strip_prefix("oklch(")?.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split_whitespace().collect();
    let [l, c, h] = parts.get(..3)? else {
        return None;
    };
    let l: f32 = match l.strip_suffix('%') {
        Some(percent) => percent.parse::<f32>().ok()? / 100.0,
        None => l.parse().ok()?,
    };
    let c: f32 = c.parse().ok()?;
    let h: f32 = h.trim_end_matches("deg").parse().ok()?;
    let (a, b) = (c * h.to_radians().cos(), c * h.to_radians().sin());
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l3, m3, s3) = (l_.powi(3), m_.powi(3), s_.powi(3));
    let linear = [
        4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3,
        -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3,
        -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3,
    ];
    let encode = |v: f32| {
        let v = v.clamp(0.0, 1.0);
        let srgb = if v <= 0.003_130_8 {
            12.92 * v
        } else {
            1.055 * v.powf(1.0 / 2.4) - 0.055
        };
        (srgb * 255.0).round() as u8
    };
    Some(Color32::from_rgb(
        encode(linear[0]),
        encode(linear[1]),
        encode(linear[2]),
    ))
}

/// A theme color: `#rrggbb[aa]` or `oklch(…)`.
fn parse_hex(value: &serde_json::Value) -> Option<Color32> {
    let text = value.as_str()?;
    if text.trim_start().starts_with("oklch(") {
        return parse_oklch(text);
    }
    let hex = text.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(
            byte(0)?,
            byte(2)?,
            byte(4)?,
            byte(6)?,
        )),
        _ => None,
    }
}

/// The built-in themes' dark variants (`BUILT_IN_THEMES` of
/// `packages/shared/src/themePalettes.ts`, `getThemeColorsForAppearance`).
pub fn built_in_theme(id: &str) -> Option<serde_json::Value> {
    static THEMES: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
    THEMES
        .get_or_init(|| {
            serde_json::from_str(include_str!("../assets/themes/built-in-dark.json"))
                .unwrap_or_default()
        })
        .iter()
        .find(|theme| theme["id"] == id)
        .cloned()
}

impl Palette {
    /// The tokens of a published environment theme (`EnvironmentTheme`),
    /// mapped like the web client's `html[data-theme-id]` rule: canvas to
    /// `--background`, text to `--foreground`, surface to `--card`, …
    /// Missing roles fall back to the canvas, text or accent.
    pub fn from_theme(theme: &serde_json::Value) -> Option<Palette> {
        let colors = &theme["colors"];
        let canvas = parse_hex(&colors["canvas"]).or_else(|| parse_hex(&theme["canvas"]))?;
        let accent = parse_hex(&colors["accent"]).or_else(|| parse_hex(&theme["accent"]))?;
        let dark = theme["appearance"] != "light";
        let text = parse_hex(&colors["text"]).unwrap_or(if dark {
            DARK.foreground
        } else {
            Color32::from_rgb(10, 10, 10)
        });
        let pick = |key: &str, fallback: Color32| parse_hex(&colors[key]).unwrap_or(fallback);
        let muted_text = pick("mutedForeground", mix(canvas, text, 0.55));
        let surface = pick("surface", canvas);
        let input = pick("input", mix(canvas, text, 0.2));
        let primary = pick("messageAction", accent);
        Some(Palette {
            custom: true,
            dark: luminance(canvas) < 0.179,
            background: canvas,
            sidebar: pick("sidebar", canvas),
            card: surface,
            popover: pick("surfaceOverlay", surface),
            foreground: text,
            sidebar_foreground: pick("sidebarForeground", text),
            muted_foreground: muted_text,
            secondary_label: pick("secondaryLabel", muted_text),
            sidebar_muted: pick("sidebarMutedForeground", muted_text),
            sidebar_icon: pick("iconMuted", muted_text),
            sidebar_border: pick("sidebarBorder", pick("border", input)),
            primary,
            primary_hover: pick("messageActionHover", primary),
            primary_foreground: pick("messageActionForeground", canvas),
            warning: pick("warning", DARK.warning),
            warning_foreground: pick("warningForeground", DARK.warning_foreground),
            error_foreground: pick("errorForeground", DARK.error_foreground),
            border: pick("border", input),
            input,
            accent: pick("accentSurface", mix(canvas, text, 0.08)),
            muted: pick("muted", mix(canvas, text, 0.05)),
            message_surface: pick("messageSurface", surface),
            control_surface: alpha(input, 0.32),
            row_active: pick("sidebarRowActive", mix(canvas, text, 0.11)),
            code_foreground: pick("codeForeground", text),
            overlay: text,
        })
    }
}

/// WCAG relative luminance (`themeRelativeLuminance`).
fn luminance(color: Color32) -> f32 {
    let linear = |c: u8| {
        let c = f32::from(c) / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(color.r()) + 0.7152 * linear(color.g()) + 0.0722 * linear(color.b())
}

/// `a` mixed toward `b` by `t` (sRGB, opaque).
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let channel = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(
        channel(a.r(), b.r()),
        channel(a.g(), b.g()),
        channel(a.b(), b.b()),
    )
}

pub fn background() -> Color32 {
    palette().background
}
pub fn sidebar() -> Color32 {
    palette().sidebar
}
pub fn card() -> Color32 {
    palette().card
}
pub fn popover() -> Color32 {
    palette().popover
}
pub fn foreground() -> Color32 {
    palette().foreground
}
pub fn sidebar_foreground() -> Color32 {
    palette().sidebar_foreground
}
pub fn muted_foreground() -> Color32 {
    palette().muted_foreground
}
pub fn secondary_label() -> Color32 {
    palette().secondary_label
}
pub fn sidebar_muted() -> Color32 {
    palette().sidebar_muted
}
pub fn sidebar_icon() -> Color32 {
    palette().sidebar_icon
}
pub fn sidebar_border() -> Color32 {
    palette().sidebar_border
}
pub fn primary() -> Color32 {
    palette().primary
}
pub fn primary_hover() -> Color32 {
    palette().primary_hover
}
pub fn primary_foreground() -> Color32 {
    palette().primary_foreground
}
pub fn warning() -> Color32 {
    palette().warning
}
pub fn warning_foreground() -> Color32 {
    palette().warning_foreground
}
pub fn error_foreground() -> Color32 {
    palette().error_foreground
}
pub fn border() -> Color32 {
    palette().border
}
pub fn input() -> Color32 {
    palette().input
}
pub fn accent() -> Color32 {
    palette().accent
}
pub fn muted() -> Color32 {
    palette().muted
}
pub fn message_surface() -> Color32 {
    palette().message_surface
}
pub fn control_surface() -> Color32 {
    palette().control_surface
}
pub fn row_active() -> Color32 {
    palette().row_active
}
pub fn code_foreground() -> Color32 {
    palette().code_foreground
}

/// A translucent tint: `#fff` at `alpha` on the stock palette (the upstream
/// `white/N` utilities), the theme's text color at `alpha` in a theme.
pub fn overlay(alpha: f32) -> Color32 {
    let palette = palette();
    if palette.custom {
        let base = palette.overlay;
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (alpha * 255.0 + 0.5) as u8)
    } else {
        white(alpha)
    }
}

/// An opaque surface measured on the stock palette as gray `level` (white
/// over the #0a0a0a background); in a theme, the same share of the overlay
/// color over the theme's background.
pub fn shade(level: u8) -> Color32 {
    let palette = palette();
    if palette.custom {
        mix(
            palette.background,
            palette.overlay,
            (f32::from(level) - 10.0) / 245.0,
        )
    } else {
        Color32::from_gray(level)
    }
}

/// `color` on the stock palette, `themed` in an environment theme.
pub fn stock_or(color: Color32, themed: impl FnOnce(&Palette) -> Color32) -> Color32 {
    let palette = palette();
    if palette.custom {
        themed(&palette)
    } else {
        color
    }
}

pub fn alpha(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity)
}

pub const SIDEBAR_WIDTH: f32 = 256.0; // --sidebar-width: 16rem
pub const TOPBAR_HEIGHT: f32 = 52.0; // --workspace-topbar-height
pub const CHAT_WIDTH: f32 = 768.0; // --chat-max-width: 48rem
pub const CONTROL_RADIUS: u8 = 8; // --control-radius: .5rem
pub const RADIUS: u8 = 10; // --radius: .625rem
pub const WORKSPACE_GUTTER: f32 = 20.0; // --workspace-gutter: 1.25rem

/// Project icon palette (Tailwind 400 shades used by the dark theme).
pub const PROJECT_COLORS: [Color32; 18] = [
    Color32::from_rgb(153, 161, 175), // gray
    Color32::from_rgb(255, 100, 103), // red
    Color32::from_rgb(255, 137, 4),   // orange
    Color32::from_rgb(255, 185, 0),   // amber
    Color32::from_rgb(253, 199, 0),   // yellow
    Color32::from_rgb(154, 230, 0),   // lime
    Color32::from_rgb(5, 223, 114),   // green
    Color32::from_rgb(0, 212, 146),   // emerald
    Color32::from_rgb(0, 213, 190),   // teal
    Color32::from_rgb(0, 211, 242),   // cyan
    Color32::from_rgb(0, 188, 255),   // sky
    Color32::from_rgb(81, 162, 255),  // blue
    Color32::from_rgb(124, 134, 255), // indigo
    Color32::from_rgb(166, 132, 255), // violet
    Color32::from_rgb(194, 122, 255), // purple
    Color32::from_rgb(237, 106, 255), // fuchsia
    Color32::from_rgb(251, 100, 182), // pink
    Color32::from_rgb(255, 99, 126),  // rose
];

pub const PROJECT_COLOR_NAMES: [&str; 18] = [
    "gray", "red", "orange", "amber", "yellow", "lime", "green", "emerald", "teal", "cyan", "sky",
    "blue", "indigo", "violet", "purple", "fuchsia", "pink", "rose",
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

/// Geist at a CSS weight. egui has no font weights, so each weight is its own family.
pub fn font(size: f32, weight: Weight) -> FontId {
    let family = match weight {
        Weight::Regular => FontFamily::Proportional,
        Weight::Medium => FontFamily::Name("medium".into()),
        Weight::SemiBold => FontFamily::Name("semibold".into()),
        Weight::Bold => FontFamily::Name("bold".into()),
    };
    FontId::new(size, family)
}

/// Sizes from Settings → Appearance: code, prompt and terminal text stay
/// absolute pixels while the interface size zooms everything else
/// (`applyAppearanceFontVariables`).
#[derive(Clone, Copy, PartialEq)]
pub struct TypeSizes {
    pub code: f32,
    pub prompt: f32,
    pub terminal: f32,
    /// The interface zoom (interface size / 16).
    pub zoom: f32,
}

impl Default for TypeSizes {
    fn default() -> Self {
        Self {
            code: 13.0,
            prompt: 14.0,
            terminal: 12.0,
            zoom: 1.0,
        }
    }
}

fn type_sizes_cell() -> &'static std::sync::RwLock<TypeSizes> {
    static SIZES: std::sync::OnceLock<std::sync::RwLock<TypeSizes>> = std::sync::OnceLock::new();
    SIZES.get_or_init(Default::default)
}

pub fn set_type_sizes(sizes: TypeSizes) {
    if let Ok(mut cell) = type_sizes_cell().write() {
        *cell = sizes;
    }
}

pub fn type_sizes() -> TypeSizes {
    type_sizes_cell().read().map(|s| *s).unwrap_or_default()
}

/// The code font (`--font-size-code`) for code blocks, diffs and files.
pub fn code_font() -> FontId {
    let sizes = type_sizes();
    FontId::monospace(sizes.code / sizes.zoom)
}

/// The prompt editor's font (`--font-composer`, `--font-size-prompt`).
pub fn composer_font() -> FontId {
    let sizes = type_sizes();
    FontId::new(
        sizes.prompt / sizes.zoom,
        FontFamily::Name("composer".into()),
    )
}

/// The terminal's font, regular or bold.
pub fn terminal_font(bold: bool) -> FontId {
    let sizes = type_sizes();
    let family = if bold { "terminal-bold" } else { "terminal" };
    FontId::new(sizes.terminal / sizes.zoom, FontFamily::Name(family.into()))
}

/// A code line height given at the default 13px code size, scaled with it.
pub fn code_line(default: f32) -> f32 {
    let sizes = type_sizes();
    default * sizes.code / 13.0 / sizes.zoom
}

/// The prompt's line height (`leading-relaxed`).
pub fn composer_line() -> f32 {
    let sizes = type_sizes();
    sizes.prompt * 1.625 / sizes.zoom
}

/// The terminal's cell height: Ghostty's measured 18px at 12 and 13px
/// (the font's line height, about 1.385em), never under 18px.
pub fn terminal_cell_height() -> f32 {
    let sizes = type_sizes();
    (sizes.terminal * 18.0 / 13.0).round().max(18.0) / sizes.zoom
}

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-bold".into()))
}

/// Carries the current palette into egui's own visuals (scroll handles,
/// selection, text cursor, panels).
pub fn apply_palette(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        // Scroll handles take these fills (no egui buttons or sliders are used).
        style.visuals.widgets.inactive.bg_fill = overlay(0.078); // #ffffff14
        style.visuals.widgets.hovered.bg_fill = overlay(0.122); // --app-scrollbar-thumb-hover
        style.visuals.widgets.active.bg_fill = overlay(0.122);
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
        }
        style.visuals.panel_fill = background();
        style.visuals.window_fill = card();
        style.visuals.override_text_color = None;
        style.visuals.window_stroke = Stroke::new(1.0, border());
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, border());
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, foreground());
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, foreground());
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, foreground());
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, foreground());
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        style.visuals.widgets.hovered.weak_bg_fill = accent();
        style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        style.visuals.widgets.active.weak_bg_fill = accent();
        style.visuals.widgets.active.bg_stroke = Stroke::NONE;
        style.visuals.selection.bg_fill = alpha(primary(), 0.35);
        style.visuals.selection.stroke = Stroke::new(1.0, foreground());
        style.visuals.text_cursor.stroke = Stroke::new(1.5, foreground());
        style.visuals.hyperlink_color = primary();
        style.visuals.extreme_bg_color = background();
        style.visuals.code_bg_color = muted();
    });
}

/// The bytes of `SymbolsNerdFontMono-Regular.ttf` from the usual font
/// directories, if installed.
/// System fonts covering common symbols terminals and tools print (check
/// marks, crosses, triangles, arrows), found once through fontconfig.
fn symbol_fonts() -> &'static [std::sync::Arc<Vec<u8>>] {
    static FONTS: std::sync::OnceLock<Vec<std::sync::Arc<Vec<u8>>>> = std::sync::OnceLock::new();
    FONTS.get_or_init(|| {
        // The lookups run side by side: each `fc-match` takes ~12ms.
        let lookups: Vec<_> = ["2713", "2717", "25b3", "2192", "2014"]
            .iter()
            .filter_map(|code| {
                std::process::Command::new("fc-match")
                    .args(["-f", "%{file}", &format!(":charset={code}")])
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok()
            })
            .collect();
        let mut files: Vec<String> = Vec::new();
        for lookup in lookups {
            let file = lookup
                .wait_with_output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .filter(|f| !f.is_empty());
            if let Some(file) = file
                && !files.contains(&file)
            {
                files.push(file);
            }
        }
        files
            .iter()
            .filter_map(|f| std::fs::read(f).ok())
            .map(std::sync::Arc::new)
            .collect()
    })
}

fn installed_symbols_font() -> Option<Vec<u8>> {
    const NAME: &str = "SymbolsNerdFontMono-Regular.ttf";
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let user = std::env::var("USER").unwrap_or_default();
    let mut roots: Vec<std::path::PathBuf> = vec![
        "/usr/share/fonts".into(),
        "/usr/local/share/fonts".into(),
        "/run/current-system/sw/share/fonts".into(),
        format!("/etc/profiles/per-user/{user}/share/fonts").into(),
    ];
    if let Some(home) = home {
        roots.push(home.join(".local/share/fonts"));
        roots.push(home.join(".fonts"));
        roots.push(home.join(".nix-profile/share/fonts"));
    }
    fn find(dir: &std::path::Path, depth: usize) -> Option<std::path::PathBuf> {
        let entries = std::fs::read_dir(dir).ok()?;
        let mut dirs = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().is_some_and(|n| n == NAME) {
                return Some(path);
            }
            if depth > 0 && path.is_dir() {
                dirs.push(path);
            }
        }
        dirs.into_iter().find_map(|d| find(&d, depth - 1))
    }
    roots
        .iter()
        .find_map(|root| find(root, 4))
        .and_then(|path| std::fs::read(path).ok())
}

/// Fonts chosen in Settings → Appearance (`appearanceFonts.ts`), each
/// prepended to its default family so Geist and the fallbacks keep covering
/// missing glyphs: the interface by weight (regular, medium, semibold, bold),
/// the prompt, code (regular, bold) and the terminal (regular, bold).
#[derive(Clone, Default, PartialEq)]
pub struct CustomFonts {
    pub sans: [Option<std::sync::Arc<Vec<u8>>>; 4],
    pub composer: Option<std::sync::Arc<Vec<u8>>>,
    pub code: [Option<std::sync::Arc<Vec<u8>>>; 2],
    pub terminal: [Option<std::sync::Arc<Vec<u8>>>; 2],
}

pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
    install_fonts(ctx, &CustomFonts::default());
    install_styles(ctx);
}

/// Installs Geist and the fallbacks, with the custom fonts in front.
pub fn install_fonts(ctx: &egui::Context, custom: &CustomFonts) {
    let mut fonts = FontDefinitions::empty();
    let faces: [(&str, &'static [u8]); 6] = [
        ("Geist", include_bytes!("../assets/fonts/Geist-Regular.otf")),
        (
            "Geist Medium",
            include_bytes!("../assets/fonts/Geist-Medium.otf"),
        ),
        (
            "Geist SemiBold",
            include_bytes!("../assets/fonts/Geist-SemiBold.otf"),
        ),
        (
            "Geist Bold",
            include_bytes!("../assets/fonts/Geist-Bold.otf"),
        ),
        (
            "Geist Mono",
            include_bytes!("../assets/fonts/GeistMono-Regular.otf"),
        ),
        (
            "Geist Mono Bold",
            include_bytes!("../assets/fonts/GeistMono-Bold.otf"),
        ),
    ];
    for (name, bytes) in faces {
        fonts
            .font_data
            .insert(name.into(), FontData::from_static(bytes).into());
    }
    // egui's bundled fonts stay as fallbacks for glyphs Geist lacks.
    let defaults = FontDefinitions::default();
    let mut fallback = Vec::new();
    // Prompt symbols and devicons (the terminal's glyph fallback): a locally
    // installed Symbols Nerd Font, as upstream prefers one before its bundled copy.
    if let Some(bytes) = installed_symbols_font() {
        fonts.font_data.insert(
            "Symbols Nerd Font Mono".into(),
            FontData::from_owned(bytes).into(),
        );
        fallback.push("Symbols Nerd Font Mono".into());
    }
    // Symbols Geist lacks (✓ △ ✗ …), from the system fonts that cover them.
    for (index, bytes) in symbol_fonts().iter().enumerate() {
        let name = format!("System Symbols {index}");
        fonts.font_data.insert(
            name.clone(),
            FontData::from_owned(bytes.as_ref().clone()).into(),
        );
        fallback.push(name);
    }
    for (name, data) in defaults.font_data {
        fallback.push(name.clone());
        fonts.font_data.insert(name, data);
    }
    let mut custom_names: Vec<(String, std::sync::Arc<Vec<u8>>)> = Vec::new();
    let mut custom_name = |slot: &Option<std::sync::Arc<Vec<u8>>>, name: &str| {
        slot.as_ref().map(|bytes| {
            custom_names.push((name.to_owned(), bytes.clone()));
            name.to_owned()
        })
    };
    let sans: Vec<Option<String>> = custom
        .sans
        .iter()
        .enumerate()
        .map(|(i, slot)| custom_name(slot, &format!("custom-sans-{i}")))
        .collect();
    let composer = custom_name(&custom.composer, "custom-composer");
    let code: Vec<Option<String>> = custom
        .code
        .iter()
        .enumerate()
        .map(|(i, slot)| custom_name(slot, &format!("custom-code-{i}")))
        .collect();
    let terminal: Vec<Option<String>> = custom
        .terminal
        .iter()
        .enumerate()
        .map(|(i, slot)| custom_name(slot, &format!("custom-terminal-{i}")))
        .collect();
    for (name, bytes) in custom_names {
        fonts
            .font_data
            .insert(name, FontData::from_owned(bytes.as_ref().clone()).into());
    }
    let chain = |front: &[&Option<String>], primary: &str| {
        let mut list: Vec<String> = front.iter().filter_map(|f| (*f).clone()).collect();
        list.push(primary.to_owned());
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, chain(&[&sans[0]], "Geist"));
    fonts
        .families
        .insert(FontFamily::Monospace, chain(&[&code[0]], "Geist Mono"));
    fonts.families.insert(
        FontFamily::Name("medium".into()),
        chain(&[&sans[1], &sans[0]], "Geist Medium"),
    );
    fonts.families.insert(
        FontFamily::Name("semibold".into()),
        chain(&[&sans[2], &sans[0]], "Geist SemiBold"),
    );
    fonts.families.insert(
        FontFamily::Name("bold".into()),
        chain(&[&sans[3], &sans[0]], "Geist Bold"),
    );
    fonts.families.insert(
        FontFamily::Name("mono-bold".into()),
        chain(&[&code[1], &code[0]], "Geist Mono Bold"),
    );
    // The prompt follows the interface font unless it has its own.
    fonts.families.insert(
        FontFamily::Name("composer".into()),
        chain(&[&composer, &sans[0]], "Geist"),
    );
    fonts.families.insert(
        FontFamily::Name("terminal".into()),
        chain(&[&terminal[0]], "Geist Mono"),
    );
    fonts.families.insert(
        FontFamily::Name("terminal-bold".into()),
        chain(&[&terminal[1], &terminal[0]], "Geist Mono Bold"),
    );
    ctx.set_fonts(fonts);
}

fn install_styles(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
        // Raw glyph coverage, without egui's dark-mode weight boost, which
        // fattens and blurs glyphs next to Chromium's text (measured at 1x
        // and 1.5x against the reference).
        style.visuals.text_options.color_transfer_function =
            egui::epaint::FontColorTransferFunction::Off;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(12.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.0));
        style
            .text_styles
            .insert(TextStyle::Heading, font(30.0, Weight::SemiBold));
        style.spacing.item_spacing = egui::vec2(0.0, 0.0);
        style.spacing.button_padding = egui::vec2(0.0, 0.0);
        // Chromium's classic scrollbars: a 6px rounded thumb (`--app-scrollbar-thumb`)
        // that stays visible, no track. Views that must keep the strip clear
        // reserve it themselves.
        style.spacing.scroll = egui::style::ScrollStyle {
            bar_width: 6.0, // --app-scrollbar-width
            floating: true,
            floating_width: 6.0,
            floating_allocated_width: 0.0,
            bar_inner_margin: 0.0,
            bar_outer_margin: 0.0,
            foreground_color: false,
            dormant_background_opacity: 0.0,
            active_background_opacity: 0.0,
            interact_background_opacity: 0.0,
            dormant_handle_opacity: 1.0,
            active_handle_opacity: 1.0,
            interact_handle_opacity: 1.0,
            ..egui::style::ScrollStyle::floating()
        };
        style.interaction.selectable_labels = false;
    });
    apply_palette(ctx);
}

pub fn icon(name: &str) -> egui::ImageSource<'static> {
    if name == "provider-antigravity" {
        // A raster mark (upstream embeds the same PNG in its SVG).
        return egui::include_image!("../assets/icons/provider-antigravity.png");
    }
    macro_rules! icons {
        ($($name:literal),* $(,)?) => {
            match name {
                $($name => egui::include_image!(concat!("../assets/icons/", $name, ".svg")),)*
                _ => egui::include_image!("../assets/icons/circle-alert.svg"),
            }
        };
    }
    icons!(
        "panel-left-close",
        "panel-left-open",
        "search",
        "folder",
        "folder-plus",
        "square-pen",
        "settings",
        "git-pull-request-arrow",
        "chart-no-axes-column",
        "chevron-down",
        "chevron-right",
        "chevron-up",
        "clock",
        "check",
        "maximize-2",
        "minimize-2",
        "panel-bottom",
        "panel-right",
        "plus",
        "cloud-upload",
        "copy",
        "text-wrap",
        "lock-open",
        "lock",
        "paperclip",
        "arrow-up",
        "git-branch",
        "terminal",
        "x",
        "circle-alert",
        "circle-check",
        "pin",
        "wrench",
        "file-text",
        "square-arrow-out-up-right",
        "folder-closed",
        "ellipsis",
        "arrow-left",
        "loader-circle",
        "openai",
        "zed",
        "hammer",
        "eye",
        "code-xml",
        "table-2",
        "pencil-ruler",
        "list-todo",
        "package",
        "skill-chip",
        "circle",
        "globe",
        "pen-line",
        "sparkles",
        "git-commit-horizontal",
        "cloud-download",
        "info",
        "lang-rust",
        "lang-typescript",
        "lang-javascript",
        "lang-react",
        "lang-python",
        "lang-bash",
        "lang-json",
        "lang-markdown",
        "lang-css",
        "lang-html",
        "lang-go",
        "lang-c",
        "lang-cpp",
        "lang-ruby",
        "lang-swift",
        "lang-zig",
        "lang-yml",
        "lang-database",
        "lang-vue",
        "lang-svelte",
        "lang-sass",
        "lang-graphql",
        "lang-docker",
        "lang-svg",
        "lang-image",
        "shield",
        "chevron-left",
        "triangle-alert",
        "send-arrow",
        "stop-square",
        "message-circle",
        "brain",
        "zap",
        "shield-question-mark",
        "message-circle-question-mark",
        "circle-dashed",
        "claude",
        "star",
        "star-filled",
        "file-diff",
        "chevrons-up-down",
        "chevrons-down-up",
        "rows-3",
        "columns-2",
        "pilcrow",
        "folder-tree",
        "timer",
        "alarm-clock",
        "alarm-clock-off",
        "undo-2",
        "git-branch-plus",
        "earth",
        "square-terminal",
        "files",
        "link-2",
        "bot",
        "smartphone",
        "settings-2",
        "trending-up",
        "trending-down",
        "gauge",
        "file-braces",
        "archive-x",
        "keyboard",
        "snap-shot",
        "blocks",
        "hard-drive",
        "layers",
        "rotate-ccw",
        "diffs-symbol-added",
        "diffs-symbol-deleted",
        "diffs-symbol-modified",
        "diffs-symbol-moved",
        "pin-off",
        "pencil",
        "quote",
        "radio-tower",
        "refresh-cw",
        "hash",
        "archive",
        "trash",
        "mail-open",
        "message-square-plus",
        "message-square",
        "message-square-dashed",
        "file-search",
        "image",
        "text-search",
        "folder-git-2",
        "palette",
        "monitor",
        "tablet",
        "server",
        "cloud",
        "laptop",
        "download",
        "link",
        "github",
        "github-mark",
        "box",
        "arrow-down",
        "opencode",
        "minus",
        "chevrons-left-right-ellipsis",
        "vcs-git",
        "vcs-jj",
        "sc-github",
        "sc-gitlab",
        "sc-azure-devops",
        "sc-bitbucket",
        "sc-forgejo",
        "ellipsis-vertical",
        "provider-cursor",
        "provider-grok",
        "provider-kimi-dark",
        "provider-kimi-light",
        "provider-mistral-vibe",
        "provider-zcode",
        "circle-arrow-up",
        "pipette",
        "calendar",
        "arrow-right",
        "square",
        "sliders-horizontal",
        "square-split-vertical",
        "square-split-horizontal",
        "circle-slash",
        "eye-off",
        "calendar-arrow-up",
        "calendar-arrow-down",
        "user-lock",
        "list-checks",
        "plug-2",
        "list-filter",
        "arrow-down-up",
        "users",
        "user-round",
        "user-round-x",
        "user-check",
        "circle-dot",
        "circle-x",
        "git-merge",
        "git-pull-request-closed",
        "git-pull-request-draft",
        "trash-2",
        "provider-copilot",
        "provider-gemini",
        "provider-acp",
        "provider-pi",
        "external-link",
        "panels-top-left",
        "play",
        "flask-conical",
        "bug",
        "corner-up-right",
        "circle-dot-dashed",
    )
}

pub fn tinted_icon(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::new(icon(name))
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(tint)
}

/// Paints an icon centered in `rect`.
pub fn paint_icon(ui: &egui::Ui, name: &str, rect: egui::Rect, size: f32, tint: Color32) {
    // OpenCode's 32x40 mark keeps its aspect inside the square slot.
    let width = if name == "opencode" { size * 0.8 } else { size };
    let rect = egui::Rect::from_center_size(rect.center(), egui::vec2(width, size));
    tinted_icon(name, size, tint).paint_at(ui, rect);
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    #[test]
    fn environment_themes_map_to_tokens() {
        let theme = serde_json::json!({
            "id": "amane", "appearance": "light", "canvas": "#F8E9EE", "accent": "#E0486B",
            "colors": {"canvas": "#F8E9EE", "text": "#3B2730", "surface": "#F0DCE3",
                       "border": "#E6C6D1", "sidebar": "#ECD4DB", "messageAction": "#E0486B"}
        });
        let palette = Palette::from_theme(&theme).expect("palette");
        assert!(palette.custom && !palette.dark);
        assert_eq!(palette.background, Color32::from_rgb(0xf8, 0xe9, 0xee));
        assert_eq!(palette.foreground, Color32::from_rgb(0x3b, 0x27, 0x30));
        assert_eq!(palette.card, Color32::from_rgb(0xf0, 0xdc, 0xe3));
        assert_eq!(palette.primary, Color32::from_rgb(0xe0, 0x48, 0x6b));
        // Missing roles fall back: the sidebar text takes the text color.
        assert_eq!(palette.sidebar_foreground, palette.foreground);
        assert!(Palette::from_theme(&serde_json::json!({"colors": {}})).is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn oklch_colors_convert_to_srgb() {
        let rgb = |v: &str| parse_hex(&json!(v)).map(|c| (c.r(), c.g(), c.b()));
        assert_eq!(rgb("oklch(1 0 0)"), Some((255, 255, 255)));
        assert_eq!(rgb("oklch(0 0 0)"), Some((0, 0, 0)));
        assert_eq!(rgb("oklch(0.627955 0.257683 29.2339)"), Some((255, 0, 0)));
        assert_eq!(rgb("oklch(62.7955% 0.257683 29.2339)"), Some((255, 0, 0)));
        assert_eq!(rgb("#0a0b0c"), Some((10, 11, 12)));
        assert!(rgb("oklch(bad)").is_none());
    }

    #[test]
    fn built_in_themes_have_dark_palettes() {
        for id in ["t3-chat", "grove", "ocean", "ember", "iris"] {
            let theme = built_in_theme(id).expect(id);
            let palette = Palette::from_theme(&theme).expect(id);
            assert!(palette.dark, "{id} dark variant");
        }
        assert!(built_in_theme("t3-code").is_none());
    }
}
