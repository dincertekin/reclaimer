//! exFAT — modern SD cards (64 GB and up), USB drives, cameras.
//!
//! Each file is a small set of 32-byte directory entries: a file entry
//! (attributes, dates), a stream entry (first cluster, size, whether the data
//! is contiguous) and name entries (the full UTF-16 name). Deleting a file
//! clears one "in use" bit in each entry and frees its clusters in the
//! allocation bitmap — everything else, including the full name, survives.

use std::collections::HashSet;
use std::io;

use super::{is_noise, join, le16, le32, le64, utf16, Allocation, DeletedFile, Modified, Timestamp, Volume};
use crate::source::{Piece, Window};

const IN_USE: u8 = 0x80;
const FILE: u8 = 0x05;
const STREAM: u8 = 0x40;
const NAME: u8 = 0x41;
const BITMAP: u8 = 0x81;
const DIRECTORY: u16 = 0x10;
const NO_FAT_CHAIN: u8 = 0x02;
const MAX_DEPTH: usize = 32;
const MAX_DIR_BYTES: u64 = 4 << 20;

pub fn detect(boot: &[u8; 512]) -> bool {
    &boot[3..11] == b"EXFAT   " && boot[510..512] == [0x55, 0xAA]
}

struct Reader<'w, 'a> {
    w:        &'w mut Window<'a>,
    cluster:  u64,
    heap:     u64,      // byte offset of cluster 2
    clusters: u32,
    fat:      Vec<u8>,
    bitmap:   Vec<u8>,  // one bit per cluster, set = in use
    files:    Vec<DeletedFile>,
    seen:     HashSet<u32>,
}

pub fn read(w: &mut Window, volume: u64, len: u64) -> io::Result<Volume> {
    let b = w.array::<512>(volume)?.ok_or(io::ErrorKind::UnexpectedEof)?;
    let sector = 1u64 << b[108].min(12);
    let cluster = sector << b[109].min(25 - b[108].min(12));
    let clusters = le32(&b, 92);
    let fat = w.bytes(volume + le32(&b, 80) as u64 * sector, clusters as usize * 4 + 8)?.to_vec();
    let mut reader = Reader {
        w,
        cluster,
        heap: volume + le32(&b, 88) as u64 * sector,
        clusters,
        fat,
        bitmap: Vec::new(),
        files: Vec::new(),
        seen: HashSet::new(),
    };

    let root_cluster = le32(&b, 96);
    reader.seen.insert(root_cluster);
    let root = reader.chain(root_cluster, MAX_DIR_BYTES)?;
    reader.load_bitmap(&root)?;
    reader.directory(&root, "/", false, 0)?;

    let mut allocation = Allocation::new(volume..volume + len, reader.heap, cluster, clusters as usize);
    for i in 0..clusters as usize {
        if reader.in_use(i as u32 + 2) {
            allocation.mark(i);
        }
    }
    Ok(Volume { filesystem: "exFAT", files: reader.files, allocation: Some(allocation) })
}

impl Reader<'_, '_> {
    fn valid(&self, c: u32) -> bool {
        (2..self.clusters + 2).contains(&c)
    }

    fn offset(&self, c: u32) -> u64 {
        self.heap + (c as u64 - 2) * self.cluster
    }

    fn next(&self, c: u32) -> u32 {
        let at = c as usize * 4;
        self.fat.get(at..at + 4).map_or(0, |b| le32(b, 0))
    }

    fn in_use(&self, c: u32) -> bool {
        let i = (c - 2) as usize;
        self.bitmap.get(i / 8).is_some_and(|byte| byte & (1 << (i % 8)) != 0)
    }

    /// Clusters of a chain, following the FAT.
    fn clusters_of(&self, mut c: u32, len: u64) -> Option<Vec<u32>> {
        let needed = len.div_ceil(self.cluster).max(1) as usize;
        let mut out = Vec::with_capacity(needed);
        while out.len() < needed {
            if !self.valid(c) {
                return None;
            }
            out.push(c);
            c = self.next(c);
        }
        Some(out)
    }

    fn chain(&mut self, first: u32, len: u64) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        let mut c = first;
        while self.valid(c) && (out.len() as u64) < len {
            let offset = self.offset(c);
            out.extend_from_slice(self.w.bytes(offset, self.cluster as usize)?);
            c = self.next(c);
        }
        Ok(out)
    }

    /// The allocation bitmap is itself described by an entry in the root.
    fn load_bitmap(&mut self, root: &[u8]) -> io::Result<()> {
        if let Some(e) = root.chunks_exact(32).find(|e| e[0] == BITMAP) {
            let (first, len) = (le32(e, 20), le64(e, 24));
            self.bitmap = self.chain(first, len)?;
            self.bitmap.truncate(len as usize);
        }
        Ok(())
    }

    fn directory(&mut self, bytes: &[u8], folder: &str, deleted_parent: bool, depth: usize) -> io::Result<()> {
        let entries: Vec<&[u8]> = bytes.chunks_exact(32).collect();
        let mut i = 0;
        while i < entries.len() {
            let e = entries[i];
            if e[0] == 0x00 {
                break; // end of directory
            }
            if e[0] & !IN_USE != FILE {
                i += 1;
                continue;
            }
            let secondary = (e[1] as usize).min(entries.len() - i - 1);
            let set = &entries[i + 1..=i + secondary];
            i += 1 + secondary;

            let Some(stream) = set.first().filter(|s| s[0] & !IN_USE == STREAM) else { continue };
            let name_len = stream[3] as usize;
            let name_bytes: Vec<u8> = set[1..].iter().filter(|s| s[0] & !IN_USE == NAME).flat_map(|s| s[2..32].to_vec()).collect();
            let name: String = utf16(&name_bytes).chars().take(name_len).collect();
            if name.is_empty() {
                continue;
            }

            let deleted = e[0] & IN_USE == 0 || deleted_parent;
            let contiguous = stream[1] & NO_FAT_CHAIN != 0;
            let (first, len) = (le32(stream, 20), le64(stream, 24));
            let modified = modified(e);

            if le16(e, 4) & DIRECTORY != 0 {
                if depth >= MAX_DEPTH || !self.valid(first) || !self.seen.insert(first) {
                    continue;
                }
                let len = len.min(MAX_DIR_BYTES);
                let contents = if contiguous || deleted {
                    self.w.bytes(self.offset(first), len as usize)?.to_vec()
                } else {
                    self.chain(first, len)?
                };
                self.directory(&contents, &join(folder, &name), deleted, depth + 1)?;
            } else if deleted && len > 0 && self.valid(first) && !is_noise(&name) {
                self.deleted_file(name, folder, first, len, contiguous, modified);
            }
        }
        Ok(())
    }

    fn deleted_file(&mut self, name: String, folder: &str, first: u32, len: u64, contiguous: bool, modified: Option<Modified>) {
        // Fragmented files keep their FAT chain after deletion (only the
        // bitmap is cleared). Contiguous ones never had one.
        let count = len.div_ceil(self.cluster) as u32;
        let clusters = if contiguous { None } else { self.clusters_of(first, len) }
            .unwrap_or_else(|| (first..first.saturating_add(count)).collect());
        let fits = clusters.iter().all(|&c| self.valid(c));
        let intact = fits && clusters.iter().all(|&c| !self.in_use(c));

        // Merge runs of consecutive clusters into single pieces.
        let mut pieces: Vec<Piece> = Vec::new();
        let mut remaining = len;
        for &c in clusters.iter().filter(|&&c| self.valid(c)) {
            let take = remaining.min(self.cluster);
            let offset = self.offset(c);
            match pieces.last_mut() {
                Some(Piece::Disk { offset: start, len }) if *start + *len == offset => *len += take,
                _ => pieces.push(Piece::Disk { offset, len: take }),
            }
            remaining -= take;
        }
        self.files.push(DeletedFile { name, folder: folder.to_string(), size: len, pieces, modified, intact });
    }
}

/// Last-modified time. When the entry records its UTC offset (in 15-minute
/// steps), it is an exact moment; otherwise wall-clock time.
fn modified(file_entry: &[u8]) -> Option<Modified> {
    let stamp = le32(file_entry, 12);
    let local = Timestamp::from_dos((stamp >> 16) as u16, stamp as u16)?;
    let offset = file_entry[23];
    if offset & 0x80 == 0 {
        return Some(Modified::Local(local));
    }
    let quarters = ((offset << 1) as i8 >> 1) as i64; // 7-bit two's complement
    Some(Modified::Utc(local.to_unix() - quarters * 15 * 60))
}
