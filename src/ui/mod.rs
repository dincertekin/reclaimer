use crate::scanner;
use eframe::egui;
use std::path::PathBuf;

pub struct FoundFile {
    pub file_type: &'static str,
    pub sector: u64,
    pub offset: u64,
    pub size_bytes: usize,
    pub saved_path: Option<PathBuf>,
}

/// egui rebuilds the UI every frame, so all state lives here.
pub struct ReclaimerApp {
    image_path: String,
    found_files: Vec<FoundFile>,
    selected_index: Option<usize>,
    total_sectors: u64,
    scanned_sectors: u64,
    scanning: bool,
    status: String,
}

impl ReclaimerApp {
    pub fn new(_cc: &eframe::CreationContext) -> Self {
        println!("[ui] app created");
        Self {
            image_path: String::new(),
            found_files: Vec::new(),
            selected_index: None,
            total_sectors: 0,
            scanned_sectors: 0,
            scanning: false,
            status: "Open a disk image to begin.".to_string(),
        }
    }

    fn run_scan(&mut self) {
        self.found_files.clear();
        self.selected_index = None;
        self.scanned_sectors = 0;
        self.scanning = true;
        self.status = "Scanning...".to_string();

        let path = std::path::Path::new(&self.image_path);
        let output_dir = std::path::Path::new("recovered");

        let mut image = match crate::disk::DiskImage::open(path) {
            Ok(img) => img,
            Err(e) => {
                self.status = format!("Failed to open image: {}", e);
                self.scanning = false;
                eprintln!("[scan] failed to open: {}", e);
                return;
            }
        };

        if let Ok(meta) = std::fs::metadata(path) {
            self.total_sectors = meta.len() / image.sector_size;
        }

        let mut sector_number = 0u64;
        let mut found_count = 0u32;

        loop {
            match image.read_sector(sector_number) {
                Ok(sector) => {
                    if let Some(sig) = scanner::detect_signature(&sector) {
                        found_count += 1;
                        println!(
                            "[scan] found {} at sector {} (offset {})",
                            sig.name,
                            sector_number,
                            sector_number * 512
                        );

                        match scanner::extract_file(
                            &sector,
                            sig,
                            &mut image,
                            sector_number,
                            output_dir,
                            found_count,
                        ) {
                            Ok(filename) => {
                                println!("[scan] saved as {}", filename);
                                let saved_path = output_dir.join(&filename);
                                let size = std::fs::metadata(&saved_path)
                                    .map(|m| m.len() as usize)
                                    .unwrap_or(0);
                                self.found_files.push(FoundFile {
                                    file_type: sig.name,
                                    sector: sector_number,
                                    offset: sector_number * 512,
                                    size_bytes: size,
                                    saved_path: Some(saved_path),
                                });
                            }
                            Err(e) => {
                                eprintln!("[scan] extract failed: {}", e);
                            }
                        }
                    }
                    self.scanned_sectors += 1;
                    sector_number += 1;
                }

                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    println!("[scan] complete. {} files found.", found_count);
                    self.status = format!("Scan complete. {} files found.", found_count);
                    break;
                }

                Err(e) => {
                    eprintln!("[scan] read error at sector {}: {}", sector_number, e);
                    self.status = format!("Read error: {}", e);
                    break;
                }
            }
        }

        self.scanning = false;
    }
}

impl eframe::App for ReclaimerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Title
        ui.heading("Reclaimer");
        ui.separator();

        // Toolbar row
        ui.horizontal(|ui| {
            ui.label("Image:");
            ui.text_edit_singleline(&mut self.image_path);

            if ui.button("Open").clicked() {
                println!("[ui] open button clicked (file picker coming soon)");
            }

            let scan_btn = ui.add_enabled(
                !self.scanning && !self.image_path.is_empty(),
                egui::Button::new("Scan"),
            );

            if scan_btn.clicked() {
                println!("[ui] scan started on: {}", self.image_path);
                self.run_scan();
            }
        });

        ui.add_space(8.0);

        // Progress bar
        if self.total_sectors > 0 {
            let progress = self.scanned_sectors as f32 / self.total_sectors as f32;
            ui.add(egui::ProgressBar::new(progress).show_percentage());
            ui.add_space(4.0);
        }

        // Stats row
        ui.horizontal(|ui| {
            ui.label(format!("Sectors scanned: {}", self.scanned_sectors));
            ui.separator();
            ui.label(format!("Files found: {}", self.found_files.len()));
            ui.separator();
            let total_size: usize = self.found_files.iter().map(|f| f.size_bytes).sum();
            ui.label(format!("Total size: {} KB", total_size / 1024));
        });

        ui.separator();

        // Results list and detail panel side by side
        let available = ui.available_size();
        ui.horizontal(|ui| {
            // Left panel: results list
            let left_width = available.x * 0.6;
            ui.vertical(|ui| {
                ui.set_width(left_width);
                ui.label("Found files:");
                egui::ScrollArea::vertical()
                    .max_height(available.y - 120.0)
                    .show(ui, |ui| {
                        for (i, file) in self.found_files.iter().enumerate() {
                            let label = format!(
                                "[{}] {} — sector {} — {} bytes",
                                i + 1,
                                file.file_type,
                                file.sector,
                                file.size_bytes,
                            );
                            let selected = self.selected_index == Some(i);
                            if ui.selectable_label(selected, &label).clicked() {
                                self.selected_index = Some(i);
                                println!("[ui] selected file {}", i + 1);
                            }
                        }
                    });
            });

            ui.separator();

            // Right panel: details
            ui.vertical(|ui| {
                ui.label("Details:");
                if let Some(idx) = self.selected_index {
                    if let Some(file) = self.found_files.get(idx) {
                        ui.label(format!("Type:   {}", file.file_type));
                        ui.label(format!("Sector: {}", file.sector));
                        ui.label(format!("Offset: {} bytes", file.offset));
                        ui.label(format!("Size:   {} bytes", file.size_bytes));
                        let status = match &file.saved_path {
                            Some(p) => format!("Saved: {}", p.display()),
                            None => "Not recovered yet.".to_string(),
                        };
                        ui.label(status);
                    }
                } else {
                    ui.label("Select a file to see details.");
                }
            });
        });

        // Status bar at the bottom
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.label(&self.status);
        });
    }
}
