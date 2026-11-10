use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

// Dark theme tokens of the upstream web client (T3 Code v0.0.45, stock dark
// palette, no contrast boost), converted from oklch/color-mix to sRGB and
// checked against rendered reference screenshots. Geometry is in CSS pixels.
//
// Pluto: the client draws `pluto_stock()`, the violet look of its logo, set
// once by `install`. `DARK` holds the upstream tokens until then.
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
pub const EMERALD_500: Color32 = Color32::from_rgb(0, 188, 125); // --success

/// `color-mix(in oklab, #fff N%, transparent)` on the stock palette.
const fn white(alpha: f32) -> Color32 {
    let a = (alpha * 255.0 + 0.5) as u8;
    Color32::from_rgba_premultiplied(a, a, a, a)
}

/// The app's color tokens (`--background`, `--foreground`, …).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Pluto's own look, not the upstream `DARK` tokens.
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
    /// `--error`: red-500 mixed 90% with white.
    pub error: Color32,
    pub error_foreground: Color32,
    pub border: Color32,
    pub input: Color32,
    pub accent: Color32,
    pub muted: Color32,
    pub message_surface: Color32,
    pub control_surface: Color32,
    pub row_active: Color32,
    pub code_foreground: Color32,
    /// The base of `overlay` tints: white on `DARK`, the text color on
    /// Pluto's look.
    pub overlay: Color32,
    /// Pluto: the logo keeps its own violets (`"logo": "brand"`) instead of
    /// taking the palette's colors.
    pub brand_logo: bool,
    /// Pluto: the violet glow behind the draft hero and the gradient on the
    /// send button (`"glow": true`, `pluto_stock()` only).
    pub glow: bool,
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
    error: Color32::from_rgb(251, 65, 74),
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
    brand_logo: false,
    glow: false,
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

/// Pluto's own look, the only one the client draws: near-black tinted toward
/// the logo, its violet as the accent, the logo in its own colors and the
/// glow. Set once at start-up (`install`).
pub fn pluto_stock() -> Palette {
    Palette {
        custom: true,
        dark: true,
        background: Color32::from_rgb(0x0b, 0x0a, 0x0f),
        sidebar: Color32::from_rgb(0x07, 0x06, 0x0a),
        card: Color32::from_rgb(0x12, 0x10, 0x18),
        popover: Color32::from_rgb(0x16, 0x14, 0x1d),
        foreground: Color32::from_rgb(0xf3, 0xf1, 0xf8),
        sidebar_foreground: Color32::from_rgb(0xf1, 0xef, 0xf7),
        muted_foreground: Color32::from_rgb(0x8f, 0x8a, 0x9c),
        secondary_label: Color32::from_rgb(0x8f, 0x8a, 0x9c),
        sidebar_muted: Color32::from_rgb(0xa6, 0xa1, 0xb3),
        sidebar_icon: Color32::from_rgb(0x62, 0x5d, 0x70),
        sidebar_border: Color32::from_rgb(0x1c, 0x1a, 0x23),
        primary: Color32::from_rgb(0xa7, 0x8b, 0xfa),
        primary_hover: Color32::from_rgb(0xc4, 0xb5, 0xfd),
        primary_foreground: Color32::from_rgb(0x1a, 0x14, 0x26),
        warning: DARK.warning,
        warning_foreground: DARK.warning_foreground,
        error: DARK.error,
        error_foreground: DARK.error_foreground,
        border: Color32::from_rgb(0x1b, 0x1a, 0x20),
        input: Color32::from_rgb(0x26, 0x25, 0x2c),
        accent: Color32::from_rgb(0x16, 0x15, 0x1b),
        muted: Color32::from_rgb(0x14, 0x13, 0x19),
        message_surface: Color32::from_rgb(0x19, 0x17, 0x1d),
        // `input` at 32%.
        control_surface: Color32::from_rgba_premultiplied(0x0c, 0x0c, 0x0e, 0x52),
        row_active: Color32::from_rgb(0x1f, 0x1d, 0x29),
        code_foreground: Color32::from_rgb(0xf3, 0xf1, 0xf8),
        overlay: Color32::from_rgb(0xf3, 0xf1, 0xf8),
        brand_logo: true,
        glow: true,
    }
}

/// The pixel logo's colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LogoColors {
    pub ring: Color32,
    pub planet: Color32,
    pub band: Color32,
    pub front: Color32,
    pub highlight: Color32,
    pub moon: Color32,
}

impl LogoColors {
    /// The logo in `palette`: its own violets for a brand theme, else the
    /// planet in the text color and the rings in the accent mixed toward the
    /// canvas, which keeps them apart from the planet when the accent is
    /// white (Carbonfox).
    pub fn for_palette(palette: &Palette) -> LogoColors {
        if palette.brand_logo {
            let violet = Color32::from_rgb(0xa7, 0x8b, 0xfa);
            return LogoColors {
                ring: violet,
                planet: Color32::from_rgb(0xed, 0xe9, 0xfe),
                band: violet,
                front: Color32::from_rgb(0x6d, 0x28, 0xd9),
                highlight: Color32::WHITE,
                moon: Color32::from_rgb(0xf5, 0xf3, 0xff),
            };
        }
        let ring = mix(palette.primary, palette.background, 0.3);
        LogoColors {
            ring,
            planet: palette.foreground,
            band: ring,
            front: mix(palette.primary, palette.background, 0.6),
            highlight: palette.foreground,
            moon: palette.foreground,
        }
    }
}

pub fn logo_colors() -> LogoColors {
    LogoColors::for_palette(&palette())
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
pub fn error() -> Color32 {
    palette().error
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

/// A translucent tint: `#fff` at `alpha` on `DARK` (the upstream `white/N`
/// utilities), the text color at `alpha` on Pluto's look.
pub fn overlay(alpha: f32) -> Color32 {
    let palette = palette();
    if palette.custom {
        let base = palette.overlay;
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), (alpha * 255.0 + 0.5) as u8)
    } else {
        white(alpha)
    }
}

/// An opaque surface measured on `DARK` as gray `level` (white over the
/// #0a0a0a background); on Pluto's look, the same share of the overlay color
/// over its background.
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

/// `color` on `DARK`, `themed` on Pluto's look.
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

/// Sizes from Settings → Appearance: code and terminal text stay absolute
/// pixels while the interface size zooms everything else
/// (`applyAppearanceFontVariables`).
#[derive(Clone, Copy, PartialEq)]
pub struct TypeSizes {
    pub code: f32,
    /// The interface zoom (interface size / 16).
    pub zoom: f32,
}

impl Default for TypeSizes {
    fn default() -> Self {
        Self {
            code: 13.0,
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

/// The prompt text's size, in absolute pixels.
const PROMPT_SIZE: f32 = 14.0;

/// The prompt editor's font: the interface font at the prompt size.
pub fn composer_font() -> FontId {
    FontId::new(PROMPT_SIZE / type_sizes().zoom, FontFamily::Proportional)
}

/// The terminal's font, regular or bold: the code font at the code size.
pub fn terminal_font(bold: bool) -> FontId {
    let sizes = type_sizes();
    let family = if bold {
        FontFamily::Name("mono-bold".into())
    } else {
        FontFamily::Monospace
    };
    FontId::new(sizes.code / sizes.zoom, family)
}

/// A code line height given at the default 13px code size, scaled with it.
pub fn code_line(default: f32) -> f32 {
    let sizes = type_sizes();
    default * sizes.code / 13.0 / sizes.zoom
}

/// The prompt's line height (`leading-relaxed`).
pub fn composer_line() -> f32 {
    PROMPT_SIZE * 1.625 / type_sizes().zoom
}

/// The terminal's cell height: Ghostty's measured 18px at 12 and 13px
/// (the font's line height, about 1.385em), never under 18px.
pub fn terminal_cell_height() -> f32 {
    let sizes = type_sizes();
    (sizes.code * 18.0 / 13.0).round().max(18.0) / sizes.zoom
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
/// missing glyphs: the interface by weight (regular, medium, semibold, bold)
/// and code (regular, bold).
#[derive(Clone, Default, PartialEq)]
pub struct CustomFonts {
    pub sans: [Option<std::sync::Arc<Vec<u8>>>; 4],
    pub code: [Option<std::sync::Arc<Vec<u8>>>; 2],
}

pub fn install(ctx: &egui::Context) {
    set_palette(pluto_stock());
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
    let code: Vec<Option<String>> = custom
        .code
        .iter()
        .enumerate()
        .map(|(i, slot)| custom_name(slot, &format!("custom-code-{i}")))
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
        "workflow",
        "unlink-2",
        "arrow-up-right",
        "pause",
        "volume-2",
        "volume-x",
        "rotate-cw",
        "browser-app",
        "arrow-up-left",
        "bookmark",
        "shield-check",
        "messages-square",
        "file",
        "target",
        "wifi-off",
        "shield-question",
        "redo-2",
        "octagon-alert",
        "message-square-warning",
        "lightbulb",
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
        "folder-git",
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
        "sc-forgejo",
        "ellipsis-vertical",
        "provider-cursor",
        "provider-grok",
        "provider-deepseek",
        "provider-kimi-dark",
        "provider-kimi-light",
        "provider-mistral-vibe",
        "provider-zcode",
        "provider-ollama",
        "circle-arrow-up",
        "pipette",
        "calendar",
        "arrow-right",
        "square",
        "sliders-horizontal",
        "square-split-vertical",
        "square-split-horizontal",
        "eye-off",
        "calendar-arrow-up",
        "calendar-arrow-down",
        "user-lock",
        "list-checks",
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
        "grip-vertical",
        "list-ordered",
        "list-plus",
        "play-fill",
        "arrow-right-left",
        "git-fork",
        "square-menu",
        "corner-left-up",
        "calendar-clock",
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
    fn pluto_stock_is_a_violet_theme_with_the_glow() {
        let stock = pluto_stock();
        assert!(stock.custom && stock.dark && stock.brand_logo && stock.glow);
        assert_eq!(stock.primary, Color32::from_rgb(0xa7, 0x8b, 0xfa));
        assert_eq!(
            stock.primary_foreground,
            Color32::from_rgb(0x1a, 0x14, 0x26)
        );
        assert_eq!(
            stock.control_surface,
            alpha(stock.input, 0.32),
            "the control surface is the input color at 32%"
        );
    }

    #[test]
    fn the_brand_logo_keeps_its_own_violets() {
        assert_eq!(
            LogoColors::for_palette(&pluto_stock()).front,
            Color32::from_rgb(0x6d, 0x28, 0xd9)
        );
    }

    #[test]
    fn a_white_accent_keeps_the_logo_rings_apart_from_the_planet() {
        let palette = Palette {
            brand_logo: false,
            background: Color32::from_rgb(0x16, 0x16, 0x16),
            foreground: Color32::from_rgb(0xf2, 0xf4, 0xf8),
            primary: Color32::WHITE,
            ..pluto_stock()
        };
        let logo = LogoColors::for_palette(&palette);
        assert_ne!(logo.ring, logo.planet);
        assert_eq!(logo.ring, logo.band);
        assert!(
            logo.front.r() < logo.ring.r(),
            "the front ring is the darker one"
        );
    }
}
