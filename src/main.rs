// Release builds on Windows are GUI apps: no console window behind them.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod source;
mod fs;
mod media;
mod phone;
mod scanner;
mod sqlite;
mod ui;

fn main() -> eframe::Result<()> {
    // `Reclaimer --diagnose`: print what Reclaimer can see, for bug reports.
    if std::env::args().nth(1).as_deref() == Some("--diagnose") {
        println!("{}", phone::diagnose());
        match source::drives::list() {
            Ok(drives) => drives.iter().for_each(|d| println!("Drive: {} ({}, {} bytes)", d.name, d.path.display(), d.size)),
            Err(e) => println!("Drives: listing failed ({e})"),
        }
        return Ok(());
    }

    let icon = load_icon();

    // On macOS the title bar is transparent, so the window is one white
    // surface with just the traffic lights — like Apple's own apps.
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Reclaimer")
        .with_inner_size([1080.0, 720.0])
        .with_min_inner_size([760.0, 560.0])
        .with_fullsize_content_view(true)
        .with_titlebar_shown(false)
        .with_title_shown(false)
        // Linux: matches reclaimer.desktop, so the taskbar shows our icon.
        .with_app_id("reclaimer");

    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Reclaimer",
        options,
        Box::new(|cc| {
            // `reclaimer card.img` opens that image, ready to scan.
            let image = std::env::args_os().nth(1).map(std::path::PathBuf::from);
            Ok(Box::new(ui::ReclaimerApp::new(cc, image)))
        }),
    )
}

fn load_icon() -> Option<eframe::egui::IconData> {
    let bytes = include_bytes!("../icons/icon.png");
    let img   = image::load_from_memory(bytes).ok()?.into_rgba8();
    let (w, h) = img.dimensions();
    Some(eframe::egui::IconData {
        rgba:   img.into_raw(),
        width:  w,
        height: h,
    })
}
