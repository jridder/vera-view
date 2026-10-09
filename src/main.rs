// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod find;
#[cfg(target_os = "macos")]
mod macos;
mod viewer;

use eframe::egui;

const USAGE: &str = "Usage: vera-view [FILE.vsdx] [--page N] [--view X,Y,ZOOM%] [--find TEXT] [--screenshot OUT.png]";

fn main() -> eframe::Result {
    let mut args = viewer::Args::default();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--page" => args.page = it.next().and_then(|n| n.parse::<usize>().ok()).unwrap_or(1).saturating_sub(1),
            "--screenshot" => args.screenshot = it.next().map(Into::into),
            "--find" => args.find = it.next(),
            "--about" => args.about = true,
            "--notices" => args.notices = true,
            "--view" => {
                let v: Vec<f32> = it.next().unwrap_or_default().split(',').filter_map(|n| n.trim().parse().ok()).collect();
                if let [x, y, zoom] = v[..] {
                    args.view = Some([x, y, zoom]);
                }
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            // Ignore other options, such as the "-psn_…" that older macOS versions pass to
            // apps launched from Finder.
            other if other.starts_with('-') => {}
            _ => args.file = Some(arg.into()),
        }
    }

    // On macOS, files opened from Finder arrive as Apple Events, not arguments.
    #[cfg(target_os = "macos")]
    macos::install();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vera View")
            // Matches vera-view.desktop, so Linux desktops show the right icon and grouping.
            .with_app_id("vera-view")
            .with_inner_size([1280.0, 840.0])
            .with_min_inner_size([480.0, 320.0])
            .with_drag_and_drop(true)
            .with_icon(window_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "Vera View",
        options,
        Box::new(|cc| Ok(Box::new(viewer::ViewerApp::new(cc, args)))),
    )
}

fn window_icon() -> egui::IconData {
    let img = image::load_from_memory(include_bytes!("../assets/icon-256.png"))
        .expect("bundled icon is a valid PNG")
        .to_rgba8();
    egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
}
