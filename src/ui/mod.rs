//! Two screens, one thing at a time:
//!   Choose  — pick a drive or disk image, press Scan.
//!   Results — watch files appear while scanning, then browse them.

use std::io;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32};

mod choose;
mod icons;
mod results;
mod scan;
mod theme;
mod thumbs;
mod types;
#[cfg(test)]
mod tests;

use crate::phone::iphone::{self, Backup};
use crate::phone::{self as phones, Phone};
use crate::source::drives::{self, Drive};

/// What the background listing found: drives, phones on a cable, and
/// iPhone backups.
struct Listing {
    drives:  io::Result<Vec<Drive>>,
    phones:  Vec<Phone>,
    backups: io::Result<Vec<Backup>>,
}
use thumbs::Thumbnails;
use types::*;

const OUTPUT_DIR_KEY: &str = "output_dir";
const SORT_KEY: &str = "sort";
/// Drives are re-listed this often while choosing, so a card plugged in
/// shows up by itself.
const DRIVE_REFRESH: Duration = Duration::from_secs(3);

/// What a cached results view was computed from.
#[derive(PartialEq)]
struct ViewKey {
    scan:   u64,
    files:  usize,
    filter: Filter,
    search: String,
    sort:   Sort,
}

pub struct ReclaimerApp {
    // Choosing
    source:        Option<Source>,
    drives:        Vec<Drive>,
    phones:        Vec<Phone>,
    backups:       Vec<Backup>,
    /// macOS hides iPhone backups until the app has Full Disk Access.
    backups_locked: bool,
    drives_rx:     Option<mpsc::Receiver<Listing>>,
    drives_listed: Option<Instant>,
    output_dir:    PathBuf,
    error:         Option<String>,

    // Scanning
    phase:         Phase,
    found:         Vec<FoundFile>,
    progress:      (u64, u64), // bytes done, total
    stage:         &'static str,
    started:       Instant,
    stopped:       bool,
    /// Counts scans, so a new scan's results never reuse the last one's view.
    scan_number:   u64,
    scan_folder:   Option<PathBuf>,
    scan_rx:       Option<mpsc::Receiver<ScanMsg>>,
    scan_cancel:   Option<Arc<AtomicBool>>,

    // Browsing results
    filter:        Filter,
    search:        String,
    sort:          Sort,
    /// The last filtered-and-sorted list, reused until something changes.
    view:          std::cell::RefCell<Option<(ViewKey, Vec<usize>)>>,
    selected:      Option<usize>,
    thumbs:        Thumbnails,

    // Native dialogs run on their own threads so the window stays live.
    image_rx:      Option<mpsc::Receiver<Option<PathBuf>>>,
    folder_rx:     Option<mpsc::Receiver<Option<PathBuf>>>,
}

impl ReclaimerApp {
    /// `image`: a disk image to start with, e.g. from the command line.
    pub fn new(cc: &eframe::CreationContext<'_>, image: Option<PathBuf>) -> Self {
        theme::install(&cc.egui_ctx);
        let output_dir = cc.storage
            .and_then(|s| eframe::get_value(s, OUTPUT_DIR_KEY))
            .unwrap_or_else(default_output_dir);
        Self {
            source:        image.map(Source::Image),
            drives:        Vec::new(),
            phones:        Vec::new(),
            backups:       Vec::new(),
            backups_locked: false,
            drives_rx:     None,
            drives_listed: None,
            output_dir,
            error:         None,
            phase:         Phase::Choose,
            found:         Vec::new(),
            progress:      (0, 0),
            stage:         "",
            started:       Instant::now(),
            stopped:       false,
            scan_number:   0,
            scan_folder:   None,
            scan_rx:       None,
            scan_cancel:   None,
            filter:        Filter::All,
            search:        String::new(),
            sort:          cc.storage
                .and_then(|s| eframe::get_value::<String>(s, SORT_KEY))
                .and_then(|s| Sort::from_setting(&s))
                .unwrap_or_default(),
            view:          Default::default(),
            selected:      None,
            thumbs:        Thumbnails::new(&cc.egui_ctx),
            image_rx:      None,
            folder_rx:     None,
        }
    }

    // ── Actions ────────────────────────────────────────────────────────────

    fn choose_image(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.image_rx = Some(rx);
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Choose a Disk Image")
                .add_filter("Disk Images", &["img", "dd", "raw", "iso", "bin", "dmg"])
                .add_filter("All Files", &["*"])
                .pick_file();
            let _ = tx.send(path);
        });
    }

    fn choose_output_folder(&mut self) {
        let (tx, rx) = mpsc::channel();
        self.folder_rx = Some(rx);
        let start = self.output_dir.clone();
        std::thread::spawn(move || {
            let folder = rfd::FileDialog::new()
                .set_title("Save Recovered Files To")
                .set_directory(start)
                .pick_folder();
            let _ = tx.send(folder);
        });
    }

    /// Listing drives shells out to the OS, so it runs off the UI thread.
    /// iPhone backups are listed alongside.
    fn list_drives(&mut self, ctx: &egui::Context) {
        if self.drives_rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.drives_rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let backups = iphone::find_backups(&iphone::backup_folders());
            let _ = tx.send(Listing { drives: drives::list(), phones: phones::connected(), backups });
            ctx.request_repaint();
        });
    }

    fn set_source(&mut self, source: Source) {
        self.source = Some(source);
        self.error = None;
    }

    /// Back to the Choose screen, keeping the chosen source.
    fn new_scan(&mut self) {
        self.cancel_scan();
        self.scan_number += 1;
        self.phase = Phase::Choose;
        self.found.clear();
        self.thumbs.clear();
        self.selected = None;
        self.filter = Filter::All;
        self.search.clear();
        self.error = None;
        self.drives_listed = None; // refresh now
    }

    fn open_file(&self, idx: usize) {
        if let Some(file) = self.found.get(idx) {
            let _ = open::that(&file.path);
        }
    }

    // ── Queries ────────────────────────────────────────────────────────────

    fn count(&self, filter: Filter) -> usize {
        self.found.iter().filter(|f| filter.includes(f)).count()
    }

    /// Indices of the files that pass the filter and search, in sort order.
    /// Computed once and reused until the files or the choices change.
    fn visible_files(&self) -> Vec<usize> {
        let key = ViewKey { scan: self.scan_number, files: self.found.len(), filter: self.filter, search: self.search.clone(), sort: self.sort };
        if let Some((cached, ids)) = self.view.borrow().as_ref()
            && *cached == key
        {
            return ids.clone();
        }
        let query = self.search.trim().to_lowercase();
        let mut ids: Vec<usize> = self
            .found
            .iter()
            .enumerate()
            .filter(|(_, f)| self.filter.includes(f))
            .filter(|(_, f)| {
                query.is_empty()
                    || f.filename.to_lowercase().contains(&query)
                    || f.kind.to_lowercase().contains(&query)
                    || f.folder.as_ref().is_some_and(|d| d.to_lowercase().contains(&query))
            })
            .map(|(i, _)| i)
            .collect();
        // Ties keep the order files were found in.
        ids.sort_by(|&a, &b| self.sort.compare(&self.found[a], &self.found[b]).then(a.cmp(&b)));
        *self.view.borrow_mut() = Some((key, ids.clone()));
        ids
    }

    // ── Background results ─────────────────────────────────────────────────

    fn poll(&mut self, ctx: &egui::Context) {
        self.thumbs.poll(ctx);

        if self.phase == Phase::Choose {
            let dropped = ctx.input(|i| i.raw.dropped_files.first().and_then(|f| f.path.clone()));
            if let Some(path) = dropped {
                self.set_source(Source::Image(path));
            }
            let stale = self.drives_listed.is_none_or(|t| t.elapsed() >= DRIVE_REFRESH);
            if stale && self.drives_rx.is_none() {
                self.list_drives(ctx);
            }
            ctx.request_repaint_after(DRIVE_REFRESH);
        }

        if let Some(rx) = &self.image_rx
            && let Ok(choice) = rx.try_recv()
        {
            self.image_rx = None;
            if let Some(path) = choice {
                self.set_source(Source::Image(path));
            }
        }

        if let Some(rx) = &self.folder_rx
            && let Ok(choice) = rx.try_recv()
        {
            self.folder_rx = None;
            if let Some(folder) = choice {
                self.output_dir = folder;
                self.error = None;
            }
        }

        if let Some(rx) = &self.drives_rx
            && let Ok(listing) = rx.try_recv()
        {
            self.drives_rx = None;
            self.drives_listed = Some(Instant::now());
            // A failed listing keeps the last good one: images still work.
            if let Ok(list) = listing.drives {
                // Keep the chosen drive in sync; forget it if it was unplugged.
                if let Some(Source::Drive(chosen)) = &self.source {
                    let current = list.iter().find(|d| d.path == chosen.path).cloned();
                    self.source = current.map(Source::Drive);
                }
                self.drives = list;
            }
            // Same for a phone that was unplugged.
            if let Some(Source::Phone(chosen)) = &self.source {
                let current = listing.phones.iter().find(|p| p.id == chosen.id).cloned();
                self.source = current.map(Source::Phone);
            }
            // A phone that shows up is almost certainly what the user came
            // for: choose it if nothing else is chosen.
            let new_phone = listing.phones.iter().find(|p| !self.phones.iter().any(|old| old.id == p.id)).cloned();
            if self.source.is_none()
                && let Some(phone) = new_phone
            {
                self.source = Some(Source::Phone(phone));
            }
            self.phones = listing.phones;
            match listing.backups {
                Ok(list) => {
                    self.backups = list;
                    self.backups_locked = false;
                }
                Err(e) => self.backups_locked = e.kind() == io::ErrorKind::PermissionDenied,
            }
        }

        let messages: Vec<ScanMsg> = match &self.scan_rx {
            Some(rx) => rx.try_iter().collect(),
            None => Vec::new(),
        };
        for msg in messages {
            match msg {
                ScanMsg::Started { folder } => self.scan_folder = Some(folder),
                ScanMsg::Progress { done, total } => self.progress = (done, total),
                ScanMsg::Found(file) => self.found.push(file),
                ScanMsg::Stage(stage) => self.stage = stage,
                ScanMsg::Done => {
                    self.phase = Phase::Done;
                    self.scan_rx = None;
                    self.scan_cancel = None;
                }
                ScanMsg::Error(message) => {
                    self.scan_rx = None;
                    self.scan_cancel = None;
                    self.error = Some(message);
                    // Nothing to show? Go back so the problem can be fixed.
                    self.phase = if self.found.is_empty() { Phase::Choose } else { Phase::Done };
                }
            }
        }

        if self.phase == Phase::Scanning {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }
}

impl eframe::App for ReclaimerApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, OUTPUT_DIR_KEY, &self.output_dir);
        eframe::set_value(storage, SORT_KEY, &self.sort.to_setting());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);

        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selected = None;
        }

        match self.phase {
            Phase::Choose => self.choose_screen(ui),
            Phase::Scanning | Phase::Done => self.results_screen(ui),
        }

        let window = ctx.content_rect();

        // No drag handling here: macOS's own (transparent) title bar already
        // moves the window. Asking it to drag as well made the two race and
        // log "Window move completed without beginning".

        if self.phase == Phase::Choose && ctx.input(|i| !i.raw.hovered_files.is_empty()) {
            let painter = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop")));
            painter.rect_filled(window, 0.0, Color32::from_white_alpha(235));
            painter.rect_stroke(
                window.shrink(16.0),
                egui::CornerRadius::same(theme::RADIUS + 4),
                egui::Stroke::new(2.0, theme::BLACK),
                egui::StrokeKind::Inside,
            );
            painter.text(window.center(), egui::Align2::CENTER_CENTER, "Drop to scan this disk image", theme::semibold(20.0), theme::BLACK);
        }
    }
}
