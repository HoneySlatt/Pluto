use zeroize::Zeroizing;

/// usage: pluto [--server URL] [--token-file PATH] [pluto://…]
/// The token file is read once at startup and kept in memory only. A
/// prompt link goes to the Pluto already running, if any.
fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut links = Vec::new();
    // Client settings (local-storage settings upstream) persist beside the
    // user's other app config. Nothing secret is stored there.
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".config")));
    let mut options = pluto::app::Options {
        server: None,
        token: None,
        client_settings_path: config_home.map(|d| d.join("pluto").join("client-settings.json")),
        block_turn_starts: false,
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--server" => options.server = args.next(),
            "--token-file" => {
                let path = args.next().unwrap_or_default();
                match std::fs::read_to_string(&path) {
                    Ok(token) => options.token = Some(Zeroizing::new(token.trim().to_owned())),
                    Err(error) => eprintln!("could not read token file: {error}"),
                }
            }
            link if link.starts_with("pluto:") => links.push(link.to_owned()),
            _ => eprintln!("unknown argument: {arg}"),
        }
    }
    if !links.is_empty()
        && links
            .iter()
            .all(|link| pluto::ui::prompt_links::forward(link))
    {
        return Ok(());
    }
    // The window comes back as it was left.
    let (size, maximized) = pluto::app::saved_window(options.client_settings_path.as_deref())
        .unwrap_or(([1440.0, 900.0], false));
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Pluto")
        .with_app_id("pluto")
        .with_inner_size(size)
        .with_maximized(maximized)
        .with_min_inner_size([720.0, 480.0]);
    eframe::run_native(
        "pluto",
        eframe::NativeOptions {
            viewport,
            // Frames take little GPU work: one frame queued instead of two
            // keeps the screen a frame closer to the input.
            wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
                surface: eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY,
                ..Default::default()
            },
            ..Default::default()
        },
        Box::new(move |cc| {
            let mut app = pluto::app::App::new(cc, options);
            app.queue_links(links);
            Ok(Box::new(app))
        }),
    )
}
