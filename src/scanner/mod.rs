//! Signature carving. Walk the source one block at a time; wherever a known
//! file header sits at the start of a block, let that format's carver find
//! where the file ends, report it, and jump past it.
//!
//! Finding and saving are separate steps: `scan` only reports what is there,
//! `export` writes a found file out.

mod formats;
mod window;

pub use formats::Kind;

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::source::BlockSource;
use formats::{FORMATS, HEADER_LEN};
use window::Window;

/// Progress is reported roughly every this many bytes.
const PROGRESS_EVERY: u64 = 8 << 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub offset:   u64,
    pub len:      u64,
    pub kind:     Kind,
    pub complete: bool,
}

pub enum ScanEvent {
    Progress { done: u64, total: u64 },
    Found(Found),
}

/// Scans the whole source. Returns early, without error, when `cancel` is set.
pub fn scan(
    src:      &mut dyn BlockSource,
    cancel:   &AtomicBool,
    mut emit: impl FnMut(ScanEvent),
) -> io::Result<()> {
    let total = src.size();
    let step  = src.block_size();
    let mut window   = Window::new(src);
    let mut pos      = 0;
    let mut reported = 0;

    while pos < total {
        if cancel.load(Ordering::Relaxed) {
            return Ok(());
        }

        // Copy the header out so the window is free for the carver to use.
        let mut header = [0u8; HEADER_LEN];
        let bytes = window.bytes(pos, HEADER_LEN)?;
        let n = bytes.len();
        header[..n].copy_from_slice(bytes);

        let mut next = pos + step;
        for format in FORMATS.iter().filter(|f| (f.matches)(&header[..n])) {
            let limit = pos.saturating_add(format.max_size).min(total);
            let Some(carved) = (format.carve)(&mut window, pos, limit)? else { continue };
            let len = carved.len.min(limit - pos);
            if len == 0 {
                continue;
            }
            emit(ScanEvent::Found(Found {
                offset:   pos,
                len,
                kind:     carved.kind,
                complete: carved.complete && len == carved.len,
            }));
            // Files start on block boundaries, so the next one can't begin
            // before the block after this file's last byte.
            next = (pos + len).next_multiple_of(step);
            break;
        }
        pos = next;

        if pos - reported >= PROGRESS_EVERY || pos >= total {
            reported = pos;
            emit(ScanEvent::Progress { done: pos.min(total), total });
        }
    }
    Ok(())
}

/// Copies a found file's bytes from the source into `path`.
pub fn export(src: &mut dyn BlockSource, item: &Found, path: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    let mut window = Window::new(src);
    let end = item.offset + item.len;
    let mut pos = item.offset;
    while pos < end {
        let chunk = window.bytes(pos, (end - pos).min(1 << 20) as usize)?;
        if chunk.is_empty() {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        out.write_all(chunk)?;
        pos += chunk.len() as u64;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use formats::samples;

    fn scan_all(mut image: Vec<u8>) -> Vec<Found> {
        let mut found = Vec::new();
        scan(&mut image, &AtomicBool::new(false), |e| {
            if let ScanEvent::Found(f) = e {
                found.push(f);
            }
        })
        .unwrap();
        found
    }

    fn bytes_of<'a>(image: &'a [u8], f: &Found) -> &'a [u8] {
        &image[f.offset as usize..(f.offset + f.len) as usize]
    }

    /// FNV-1a: a tiny checksum, enough to tell recovered bytes changed.
    fn checksum(bytes: &[u8]) -> u64 {
        bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
    }

    #[test]
    fn test_img_recovers_the_original_photos_exactly() {
        let image = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/test.img")).unwrap();
        let found = scan_all(image.clone());

        // Offset, size and checksum of the two photos written into test.img.
        let expected = [(1024, 94_978, 0xe466_e673_c9b6_1576), (102_400, 42_355, 0x067f_c741_409e_0f5b)];
        assert_eq!(found.len(), expected.len());
        for (f, (offset, len, sum)) in found.iter().zip(expected) {
            assert!(f.complete);
            assert_eq!((f.offset, f.len), (offset, len));
            assert_eq!(checksum(bytes_of(&image, f)), sum, "photo at {offset} differs");
        }
    }

    #[test]
    fn finds_every_format_among_junk() {
        let files = [
            samples::jpeg(), samples::png(), samples::gif(), samples::mp4(),
            samples::pdf(), samples::zip(), samples::sqlite(),
        ];
        let mut image = vec![0xAA; 512];
        for file in &files {
            image.extend(file);
            image.resize(image.len().next_multiple_of(512) + 1024, 0xAA);
        }

        let found = scan_all(image.clone());
        let names: Vec<_> = found.iter().map(|f| f.kind.name).collect();
        assert_eq!(names, ["JPEG", "PNG", "GIF", "MP4", "PDF", "ZIP", "SQLite"]);
        for (f, file) in found.iter().zip(&files) {
            assert!(f.complete, "{} incomplete", f.kind.name);
            assert_eq!(bytes_of(&image, f), file.as_slice(), "{} differs", f.kind.name);
        }
    }

    #[test]
    fn cancelled_scan_reports_nothing() {
        let mut image = samples::jpeg();
        let mut events = 0;
        scan(&mut image, &AtomicBool::new(true), |_| events += 1).unwrap();
        assert_eq!(events, 0);
    }

    #[test]
    fn export_writes_exact_bytes() {
        let jpg = samples::jpeg();
        let mut image = [vec![0; 512], jpg.clone(), vec![0; 512]].concat();
        let item = Found { offset: 512, len: jpg.len() as u64, kind: Kind { name: "JPEG", ext: "jpg" }, complete: true };
        let path = std::env::temp_dir().join(format!("reclaimer-export-{}.jpg", std::process::id()));

        export(&mut image, &item, &path).unwrap();
        let written = std::fs::read(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(written, jpg);
    }
}
