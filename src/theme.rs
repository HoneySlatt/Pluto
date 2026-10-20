use eframe::egui::{
    self, Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle,
};

// Dark theme tokens of the upstream web client (T3 Code v0.0.45, stock dark
// palette, no contrast boost), converted from oklch/color-mix to sRGB and
// checked against rendered reference screenshots. Geometry is in CSS pixels.
pub const BACKGROUND: Color32 = Color32::from_rgb(10, 10, 10); // --background
pub const SIDEBAR: Color32 = Color32::from_rgb(4, 4, 4); // rendered sidebar surface
pub const CARD: Color32 = Color32::from_rgb(17, 17, 17); // --card
pub const FOREGROUND: Color32 = Color32::from_rgb(245, 245, 245); // --foreground
pub const SIDEBAR_FOREGROUND: Color32 = Color32::from_rgb(241, 243, 247);
pub const MUTED_FOREGROUND: Color32 = Color32::from_rgb(129, 129, 129); // --muted-foreground
pub const SECONDARY_LABEL: Color32 = MUTED_FOREGROUND; // --secondary-label
pub const SIDEBAR_MUTED: Color32 = Color32::from_rgb(163, 163, 163); // muted text inside the sidebar
pub const SIDEBAR_ICON: Color32 = Color32::from_rgb(84, 84, 84); // --sidebar-icon-color
pub const PRIMARY: Color32 = Color32::from_rgb(52, 107, 241); // --primary
pub const DIFF_ADDITION: Color32 = Color32::from_rgb(0, 212, 146); // --diff-addition-foreground
pub const DIFF_DELETION: Color32 = Color32::from_rgb(255, 100, 103); // --diff-deletion-foreground
pub const WARNING_FOREGROUND: Color32 = Color32::from_rgb(255, 185, 0); // --warning-foreground
pub const ERROR_FOREGROUND: Color32 = Color32::from_rgb(255, 100, 103); // --destructive-foreground
pub const SKY_400: Color32 = Color32::from_rgb(0, 188, 255);
pub const INDIGO_300: Color32 = Color32::from_rgb(163, 179, 255);
pub const RED_300: Color32 = Color32::from_rgb(255, 162, 162);
pub const EMERALD_300: Color32 = Color32::from_rgb(94, 233, 181);
pub const BLUE_400: Color32 = Color32::from_rgb(81, 162, 255); // --info-foreground
pub const BLUE_500: Color32 = Color32::from_rgb(43, 127, 255); // --info

/// `color-mix(in oklab, #fff N%, transparent)` tokens.
pub const fn white(alpha: f32) -> Color32 {
    let a = (alpha * 255.0 + 0.5) as u8;
    Color32::from_rgba_premultiplied(a, a, a, a)
}

pub fn alpha(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity)
}

pub const BORDER: Color32 = white(0.06); // --border
pub const INPUT: Color32 = white(0.08); // --input
pub const ACCENT: Color32 = white(0.04); // --accent, --sidebar-row-hover
pub const MUTED: Color32 = white(0.03); // --muted, --secondary
pub const MESSAGE_SURFACE: Color32 = white(0.04); // --message-surface
pub const CONTROL_SURFACE: Color32 = white(0.0256); // outline buttons, code blocks
pub const ROW_ACTIVE: Color32 = Color32::from_rgba_premultiplied(27, 27, 27, 28); // rgba(241,243,247,.11)

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

pub fn mono(size: f32) -> FontId {
    FontId::monospace(size)
}

pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("mono-bold".into()))
}

pub fn install(ctx: &egui::Context) {
    egui_extras::install_image_loaders(ctx);
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
    for (name, data) in defaults.font_data {
        fallback.push(name.clone());
        fonts.font_data.insert(name, data);
    }
    let family = |primary: &str| {
        let mut list = vec![primary.to_owned()];
        list.extend(fallback.iter().cloned());
        list
    };
    fonts
        .families
        .insert(FontFamily::Proportional, family("Geist"));
    fonts
        .families
        .insert(FontFamily::Monospace, family("Geist Mono"));
    fonts
        .families
        .insert(FontFamily::Name("medium".into()), family("Geist Medium"));
    fonts.families.insert(
        FontFamily::Name("semibold".into()),
        family("Geist SemiBold"),
    );
    fonts
        .families
        .insert(FontFamily::Name("bold".into()), family("Geist Bold"));
    fonts.families.insert(
        FontFamily::Name("mono-bold".into()),
        family("Geist Mono Bold"),
    );
    ctx.set_fonts(fonts);
    ctx.set_visuals(egui::Visuals::dark());
    ctx.all_styles_mut(|style| {
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
        // Scroll handles take these fills (no egui buttons or sliders are used).
        style.visuals.widgets.inactive.bg_fill = white(0.078); // #ffffff14
        style.visuals.widgets.hovered.bg_fill = white(0.122); // --app-scrollbar-thumb-hover
        style.visuals.widgets.active.bg_fill = white(0.122);
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
        }
        style.visuals.panel_fill = BACKGROUND;
        style.visuals.window_fill = CARD;
        style.visuals.override_text_color = None;
        style.visuals.window_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, FOREGROUND);
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, FOREGROUND);
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, FOREGROUND);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, FOREGROUND);
        style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
        style.visuals.widgets.hovered.weak_bg_fill = ACCENT;
        style.visuals.widgets.hovered.bg_stroke = Stroke::NONE;
        style.visuals.widgets.active.weak_bg_fill = ACCENT;
        style.visuals.widgets.active.bg_stroke = Stroke::NONE;
        style.visuals.selection.bg_fill = alpha(PRIMARY, 0.35);
        style.visuals.selection.stroke = Stroke::new(1.0, FOREGROUND);
        style.visuals.text_cursor.stroke = Stroke::new(1.5, FOREGROUND);
        style.visuals.hyperlink_color = PRIMARY;
        style.visuals.extreme_bg_color = BACKGROUND;
        style.visuals.code_bg_color = MUTED;
        style.interaction.selectable_labels = false;
    });
}

pub fn icon(name: &str) -> egui::ImageSource<'static> {
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
        "t3-wordmark",
        "openai",
        "zed",
        "hammer",
        "eye",
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
        "refresh-cw",
        "hash",
        "archive",
        "trash",
        "mail-open",
        "message-square-plus",
        "message-square",
        "message-square-dashed",
        "file-search",
        "text-search",
        "folder-git-2",
        "palette",
        "monitor",
        "link",
        "github",
        "arrow-down",
        "opencode",
    )
}

pub fn tinted_icon(name: &str, size: f32, tint: Color32) -> egui::Image<'static> {
    egui::Image::new(icon(name))
        .fit_to_exact_size(egui::vec2(size, size))
        .tint(tint)
}

/// Paints an icon centered in `rect`.
pub fn paint_icon(ui: &egui::Ui, name: &str, rect: egui::Rect, size: f32, tint: Color32) {
    let rect = egui::Rect::from_center_size(rect.center(), egui::vec2(size, size));
    tinted_icon(name, size, tint).paint_at(ui, rect);
}
