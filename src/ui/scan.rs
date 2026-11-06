use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use eframe::egui;
use super::ReclaimerApp;
use super::types::{Category, DotState, FoundFile, Phase, ScanMsg};
use crate::scanner::{self, ScanEvent};
use crate::source::ImageFile;

impl ReclaimerApp {
    pub(super) fn start_scan(&mut self, ctx: egui::Context) {
        let Some(ref path) = self.image_path else { return };
        let image_path = path.clone();
        let output_dir = PathBuf::from("recovered");

        self.cancel_scan();
        self.phase = Phase::Scanning;
        self.found_files = Vec::new();
        self.selected_idx = None;
        self.scanned_sectors = 0;
        self.total_sectors = 0;
        self.found_count = 0;
        self.status_text = "Scanning sectors\u{2026}".into();
        self.dot_state = DotState::Scanning;

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_rx = Some(rx);
        self.scan_cancel = Some(Arc::clone(&cancel));

        std::thread::spawn(move || {
            let msg = match run_scan(&image_path, &output_dir, &cancel, &tx, &ctx) {
                Ok(())  => ScanMsg::Done,
                Err(e)  => ScanMsg::Error(e.to_string()),
            };
            let _ = tx.send(msg);
            ctx.request_repaint();
        });
    }

    /// Stops a running scan; its thread exits at the next block.
    pub(super) fn cancel_scan(&mut self) {
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.scan_rx = None;
    }
}

fn run_scan(
    image_path: &Path,
    output_dir: &Path,
    cancel:     &AtomicBool,
    tx:         &mpsc::Sender<ScanMsg>,
    ctx:        &egui::Context,
) -> io::Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let mut source = ImageFile::open(image_path)?;
    // A second handle for saving: the scan holds the first one.
    let mut saver = ImageFile::open(image_path)?;

    let mut found = 0u32;
    let mut save_error = None;

    scanner::scan(&mut source, cancel, |event| match event {
        ScanEvent::Progress { done, total } => {
            let _ = tx.send(ScanMsg::Progress { scanned: done / 512, total: total / 512, found });
            ctx.request_repaint();
        }
        ScanEvent::Found(item) => {
            let filename = format!("recovered_{}.{}", found + 1, item.kind.ext);
            match scanner::export(&mut saver, &item, &output_dir.join(&filename)) {
                Ok(()) => {
                    found += 1;
                    let _ = tx.send(ScanMsg::Found(FoundFile {
                        file_type:  item.kind.name.to_string(),
                        extension:  item.kind.ext.to_string(),
                        sector:     item.offset / 512,
                        offset:     item.offset,
                        size_bytes: item.len as usize,
                        filename,
                        category:   Category::from_ext(item.kind.ext),
                        complete:   item.complete,
                    }));
                    ctx.request_repaint();
                }
                Err(e) => {
                    // Can't save (disk full, permissions): stop rather than
                    // keep reporting files that aren't there.
                    save_error.get_or_insert(e);
                    cancel.store(true, Ordering::Relaxed);
                }
            }
        }
    })?;

    match save_error {
        Some(e) => Err(io::Error::new(e.kind(), format!("Could not save recovered file: {e}"))),
        None    => Ok(()),
    }
}
