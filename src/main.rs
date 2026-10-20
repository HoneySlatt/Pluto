use zeroize::Zeroizing;

/// usage: t3-code-rust [--server URL] [--token-file PATH]
/// The token file is read once at startup and kept in memory only.
fn main() -> eframe::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut options = t3_code_rust::app::Options {
        server: None,
        token: None,
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
            _ => eprintln!("unknown argument: {arg}"),
        }
    }
    let viewport = eframe::egui::ViewportBuilder::default()
        .with_title("T3 Code")
        .with_app_id("t3-code-rust")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([720.0, 480.0]);
    eframe::run_native(
        "t3-code-rust",
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| Ok(Box::new(t3_code_rust::app::App::new(cc, options)))),
    )
}
