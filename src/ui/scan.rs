use std::path::PathBuf;
use std::sync::mpsc;
use eframe::egui;
use super::ReclaimerApp;
use super::types::{Category, DotState, FoundFile, Phase, ScanMsg};

impl ReclaimerApp {
    pub(super) fn start_scan(&mut self, ctx: egui::Context) {
        let Some(ref path) = self.image_path else { return };
        let image_path = path.clone();
        let output_dir = PathBuf::from("recovered");

        self.phase = Phase::Scanning;
        self.found_files = Vec::new();
        self.selected_idx = None;
        self.scanned_sectors = 0;
        self.total_sectors = 0;
        self.found_count = 0;
        self.status_text = "Scanning sectors\u{2026}".into();
        self.dot_state = DotState::Scanning;

        let (tx, rx) = mpsc::channel();
        self.scan_rx = Some(rx);

        std::thread::spawn(move || {
            run_scan(image_path, output_dir, tx, ctx);
        });
    }
}

fn run_scan(
    image_path: PathBuf,
    output_dir: PathBuf,
    tx: mpsc::Sender<ScanMsg>,
    ctx: egui::Context,
) {
    use crate::{disk, scanner};

    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        let _ = tx.send(ScanMsg::Error(e.to_string()));
        return;
    }

    let mut image = match disk::DiskImage::open(&image_path) {
        Ok(img) => img,
        Err(e) => {
            let _ = tx.send(ScanMsg::Error(format!("Failed to open image: {e}")));
            return;
        }
    };

    let total_sectors = std::fs::metadata(&image_path)
        .map(|m| m.len() / image.sector_size)
        .unwrap_or(0);

    let mut sector = 0u64;
    let mut found = 0u32;

    loop {
        match image.read_sector(sector) {
            Ok(data) => {
                if let Some(sig) = scanner::detect_signature(&data) {
                    found += 1;
                    if let Ok(filename) =
                        scanner::extract_file(&data, sig, &mut image, sector, &output_dir, found)
                    {
                        let path = output_dir.join(&filename);
                        let size = std::fs::metadata(&path)
                            .map(|m| m.len() as usize)
                            .unwrap_or(0);
                        let _ = tx.send(ScanMsg::Found(FoundFile {
                            file_type: sig.name.to_string(),
                            extension: sig.extension.to_string(),
                            sector,
                            offset: sector * 512,
                            size_bytes: size,
                            filename,
                            category: Category::from_ext(sig.extension),
                        }));
                        ctx.request_repaint();
                    }
                }

                sector += 1;

                if sector.is_multiple_of(50) {
                    let _ = tx.send(ScanMsg::Progress {
                        scanned: sector,
                        total: total_sectors,
                        found,
                    });
                    ctx.request_repaint();
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                let _ = tx.send(ScanMsg::Error(format!("Read error at sector {sector}: {e}")));
                return;
            }
        }
    }

    let _ = tx.send(ScanMsg::Progress { scanned: sector, total: total_sectors, found });
    let _ = tx.send(ScanMsg::Done);
    ctx.request_repaint();
}
