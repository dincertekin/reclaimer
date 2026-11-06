use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use eframe::egui;
use super::ReclaimerApp;
use super::types::{Category, DotState, FoundFile, Phase, ScanMsg, Source};
use crate::scanner::{self, ScanEvent};
use crate::source::{drives, FileSource};

impl ReclaimerApp {
    pub(super) fn start_scan(&mut self, ctx: egui::Context) {
        let Some(source) = self.source.clone() else { return };
        let output_dir = self.output_dir.clone();

        self.cancel_scan();
        self.phase = Phase::Scanning;
        self.found_files = Vec::new();
        self.selected_idx = None;
        self.scanned_sectors = 0;
        self.total_sectors = 0;
        self.found_count = 0;
        self.status_text = match &source {
            Source::Drive(d) => format!("Waiting for permission to read {}\u{2026}", d.name),
            Source::Image(_) => "Scanning\u{2026}".into(),
        };
        self.dot_state = DotState::Scanning;

        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_rx = Some(rx);
        self.scan_cancel = Some(Arc::clone(&cancel));

        std::thread::spawn(move || {
            let msg = match run_scan(&source, &output_dir, &cancel, &tx, &ctx) {
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
    source:     &Source,
    output_dir: &Path,
    cancel:     &AtomicBool,
    tx:         &mpsc::Sender<ScanMsg>,
    ctx:        &egui::Context,
) -> io::Result<()> {
    // Writing to the drive being recovered could overwrite the very files
    // we are after. Check before touching the disk at all.
    if let Source::Drive(drive) = source
        && drive.holds(output_dir)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is on {}. Choose a folder on another drive \u{2014} saving there could overwrite the files you want back.",
                output_dir.display(),
                drive.name,
            ),
        ));
    }

    let mut reader = match source {
        Source::Image(path)  => FileSource::image(path)?,
        Source::Drive(drive) => drives::open(drive)?,
    };
    // A second handle for saving, as the scan holds the first. Cloning
    // (rather than reopening) means one permission prompt, not two.
    let mut saver = reader.try_clone()?;

    let folder = unique_folder(output_dir, &source.name());
    std::fs::create_dir_all(&folder)?;

    let mut found = 0u32;
    let mut save_error = None;

    scanner::scan(&mut reader, cancel, |event| match event {
        ScanEvent::Progress { done, total } => {
            let _ = tx.send(ScanMsg::Progress { scanned: done / 512, total: total / 512, found });
            ctx.request_repaint();
        }
        ScanEvent::Found(item) => {
            let filename = format!("recovered_{}.{}", found + 1, item.kind.ext);
            let path = folder.join(&filename);
            match scanner::export(&mut saver, &item, &path) {
                Ok(()) => {
                    found += 1;
                    let _ = tx.send(ScanMsg::Found(FoundFile {
                        file_type:  item.kind.name.to_string(),
                        extension:  item.kind.ext.to_string(),
                        sector:     item.offset / 512,
                        offset:     item.offset,
                        size_bytes: item.len as usize,
                        filename,
                        path,
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

/// `base/name`, or `base/name 2`, `base/name 3`, ... — the way Finder names
/// copies, so a new scan never overwrites an earlier one.
fn unique_folder(base: &Path, name: &str) -> PathBuf {
    let name = name.replace(['/', '\\', ':'], "-");
    (1..)
        .map(|n| if n == 1 { base.join(&name) } else { base.join(format!("{name} {n}")) })
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
}
