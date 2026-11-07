use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use eframe::egui;
use super::ReclaimerApp;
use std::time::Instant;
use std::collections::HashSet;
use std::io::Read;
use super::types::{extension_of, kind_name, safe_name, Category, Filter, FoundFile, Phase, ScanMsg, Source};
use crate::fs::{self, DeletedFile};
use crate::phone::iphone::{self, Backup};
use crate::phone::{self, Phone, PhoneFile};
use crate::scanner::{self, ScanEvent};
use crate::source::{drives, BlockSource, FileSource};

impl ReclaimerApp {
    pub(super) fn start_scan(&mut self, ctx: egui::Context) {
        let Some(source) = self.source.clone() else { return };
        let output_dir = self.output_dir.clone();

        self.cancel_scan();
        self.phase = Phase::Scanning;
        self.found.clear();
        self.thumbs.clear();
        self.selected = None;
        self.filter = Filter::All;
        self.search.clear();
        self.progress = (0, 0);
        self.started = Instant::now();
        self.stopped = false;
        self.scan_folder = None;
        self.error = None;
        self.stage = "";

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

    /// Stop button: end the scan but keep what was found.
    pub(super) fn stop_scan(&mut self) {
        self.cancel_scan();
        self.phase = Phase::Done;
        self.stopped = true;
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
        Source::Image(path)   => FileSource::image(path)?,
        Source::Drive(drive)  => drives::open(drive)?,
        Source::Backup(backup) => return scan_backup(backup, output_dir, cancel, tx, ctx),
        Source::Phone(device)  => return scan_phone(device, output_dir, cancel, tx, ctx),
    };
    // A second handle for saving, as the scan holds the first. Cloning
    // (rather than reopening) means one permission prompt, not two.
    let mut saver = reader.try_clone()?;

    let folder = unique_folder(output_dir, &source.name());
    std::fs::create_dir_all(&folder)?;
    let _ = tx.send(ScanMsg::Started { folder: folder.clone() });
    // The clock starts once permission is granted, so the estimate is honest.
    let _ = tx.send(ScanMsg::Progress { done: 0, total: reader.size() });

    // 1. Ask the filesystem: deleted files with their names and folders.
    let _ = tx.send(ScanMsg::Stage("Looking for deleted files\u{2026}"));
    ctx.request_repaint();
    let volumes = fs::analyze(&mut reader).unwrap_or_default();
    let mut named_starts = HashSet::new();
    for file in volumes.iter().flat_map(|v| &v.files) {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let found = save_named(&mut saver, file, &folder)
            .map_err(|e| io::Error::new(e.kind(), format!("Could not save recovered file: {e}")))?;
        named_starts.extend(file.start());
        let _ = tx.send(ScanMsg::Found(found));
        ctx.request_repaint();
    }

    // 2. Search every block for files the filesystem has forgotten —
    // skipping ones already recovered by name, and space that belongs to
    // files that still exist (those aren't lost).
    let _ = tx.send(ScanMsg::Stage("Searching every block\u{2026}"));
    let in_use = |offset: u64| volumes.iter().filter_map(|v| v.allocation.as_ref()).any(|a| a.contains(offset));
    let mut found = 0u32;
    let mut save_error = None;

    scanner::scan(&mut reader, cancel, |event| match event {
        ScanEvent::Progress { done, total } => {
            let _ = tx.send(ScanMsg::Progress { done, total });
            ctx.request_repaint();
        }
        ScanEvent::Found(item) if named_starts.contains(&item.offset) || in_use(item.offset) => {}
        ScanEvent::Found(item) => {
            let filename = format!("recovered_{}.{}", found + 1, item.kind.ext);
            let path = folder.join(&filename);
            match scanner::export(&mut saver, &item, &path) {
                Ok(()) => {
                    found += 1;
                    let _ = tx.send(ScanMsg::Found(FoundFile {
                        kind:      item.kind.name.to_string(),
                        extension: item.kind.ext.to_string(),
                        offset:    Some(item.offset),
                        size:      item.len,
                        filename,
                        path,
                        category:  Category::from_ext(item.kind.ext),
                        complete:  item.complete,
                        folder:    None,
                        modified:  None,
                        recently_deleted: false,
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

/// Saves a file the filesystem told us about, recreating its folder inside
/// the scan folder: `<scan>/DCIM/IMG_0001.JPG`.
fn save_named(src: &mut FileSource, file: &DeletedFile, scan_folder: &Path) -> io::Result<FoundFile> {
    let dir = file
        .folder
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(scan_folder.to_path_buf(), |dir, part| dir.join(safe_name(part)));
    std::fs::create_dir_all(&dir)?;
    let path = unique_file(&dir, &safe_name(&file.name));
    crate::source::save(src, &file.pieces, &path)?;

    let extension = extension_of(&file.name);
    let looks_right = content_matches_name(&path, &extension)?;

    Ok(FoundFile {
        kind:     kind_name(&extension),
        category: Category::from_ext(&extension),
        offset:   file.start(),
        size:     file.size,
        filename: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        path,
        complete: file.intact && looks_right,
        folder:   Some(file.folder.clone()),
        modified: file.modified.map(|m| m.local()),
        recently_deleted: false,
        extension,
    })
}

/// Copies the photos and attachments out of an iPhone backup.
fn scan_backup(
    backup:     &Backup,
    output_dir: &Path,
    cancel:     &AtomicBool,
    tx:         &mpsc::Sender<ScanMsg>,
    ctx:        &egui::Context,
) -> io::Result<()> {
    let _ = tx.send(ScanMsg::Stage("Reading the backup\u{2026}"));
    ctx.request_repaint();
    let files = iphone::files(backup)?;

    let folder = unique_folder(output_dir, &backup.device);
    std::fs::create_dir_all(&folder)?;
    let _ = tx.send(ScanMsg::Started { folder: folder.clone() });
    let total = files.iter().map(|f| f.size).sum::<u64>().max(1);
    let mut done = 0;

    for file in &files {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let dir = file.folder.split('/').filter(|p| !p.is_empty()).fold(folder.clone(), |d, p| d.join(safe_name(p)));
        std::fs::create_dir_all(&dir)?;
        let path = unique_file(&dir, &safe_name(&file.name));
        // An interrupted backup can lack some files: skip those, keep going.
        if std::fs::copy(&file.stored, &path).is_err() {
            continue;
        }
        let extension = extension_of(&file.name);
        let _ = tx.send(ScanMsg::Found(FoundFile {
            kind:      kind_name(&extension),
            category:  Category::from_ext(&extension),
            offset:    None,
            size:      file.size,
            filename:  path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            complete:  content_matches_name(&path, &extension)?,
            path,
            folder:    Some(file.folder.clone()),
            modified:  file.modified.map(|m| m.local()),
            recently_deleted: file.recently_deleted,
            extension,
        }));
        done += file.size;
        let _ = tx.send(ScanMsg::Progress { done, total });
        ctx.request_repaint();
    }
    Ok(())
}

/// Copies a phone's files over the cable.
fn scan_phone(
    device:     &Phone,
    output_dir: &Path,
    cancel:     &AtomicBool,
    tx:         &mpsc::Sender<ScanMsg>,
    ctx:        &egui::Context,
) -> io::Result<()> {
    let folder = unique_folder(output_dir, &device.name);
    std::fs::create_dir_all(&folder)?;
    let _ = tx.send(ScanMsg::Started { folder: folder.clone() });
    let mut sink = ToFolder { folder, tx, ctx, cancel, total: 1, done: 0 };
    phone::recover(device, &mut sink)
}

/// Saves phone files under the scan folder, mirroring where they lived.
pub(super) struct ToFolder<'a> {
    pub(super) folder: PathBuf,
    pub(super) tx:     &'a mpsc::Sender<ScanMsg>,
    pub(super) ctx:    &'a egui::Context,
    pub(super) cancel: &'a AtomicBool,
    pub(super) total:  u64,
    pub(super) done:   u64,
}

impl phone::Sink for ToFolder<'_> {
    fn stage(&mut self, text: &'static str) {
        let _ = self.tx.send(ScanMsg::Stage(text));
        self.ctx.request_repaint();
    }

    fn listed(&mut self, bytes: u64) {
        self.total = bytes.max(1);
    }

    fn destination(&mut self, file: &PhoneFile) -> io::Result<PathBuf> {
        let dir = file.folder.split('/').filter(|p| !p.is_empty()).fold(self.folder.clone(), |d, p| d.join(safe_name(p)));
        std::fs::create_dir_all(&dir)?;
        Ok(unique_file(&dir, &safe_name(&file.name)))
    }

    fn saved(&mut self, file: &PhoneFile, path: PathBuf) -> bool {
        let extension = extension_of(&file.name);
        let _ = self.tx.send(ScanMsg::Found(FoundFile {
            kind:      kind_name(&extension),
            category:  Category::from_ext(&extension),
            offset:    None,
            size:      file.size,
            filename:  path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            complete:  content_matches_name(&path, &extension).unwrap_or(true),
            path,
            folder:    Some(file.folder.clone()),
            modified:  file.modified.map(|m| m.local()),
            recently_deleted: file.recently_deleted,
            extension,
        }));
        self.done += file.size;
        let _ = self.tx.send(ScanMsg::Progress { done: self.done, total: self.total });
        self.ctx.request_repaint();
        !self.cancel.load(Ordering::Relaxed)
    }
}

/// A "photo.jpg" that no longer starts like a JPEG was overwritten.
fn content_matches_name(path: &Path, extension: &str) -> io::Result<bool> {
    let mut header = [0u8; 32];
    let n = std::fs::File::open(path)?.read(&mut header)?;
    Ok(scanner::looks_like(extension, &header[..n]) != Some(false))
}

/// `dir/name.ext`, or `dir/name 2.ext`, ... if that is taken.
fn unique_file(dir: &Path, name: &str) -> PathBuf {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    (1..)
        .map(|n| if n == 1 { dir.join(name) } else { dir.join(format!("{stem} {n}{ext}")) })
        .find(|p| !p.exists())
        .expect("unbounded range always yields a free name")
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
