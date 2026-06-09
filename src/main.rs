mod disk;
mod scanner;
mod ui;

use eframe::egui;

fn main() {
    println!("[reclaimer] starting up");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Reclaimer")
            .with_inner_size([900.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Reclaimer",
        options,
        Box::new(|cc| Ok(Box::new(ui::ReclaimerApp::new(cc)))),
    )
    .unwrap();
}
