mod disk;
mod scanner;
mod ui;

fn main() -> eframe::Result<()> {
    let icon = load_icon();

    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Reclaimer")
        .with_inner_size([1100.0, 700.0])
        .with_min_inner_size([800.0, 500.0]);

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
        Box::new(|cc| Ok(Box::new(ui::ReclaimerApp::new(cc)))),
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
