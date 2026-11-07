//! FAT12 / FAT16 / FAT32 — SD cards, USB sticks, cameras.
//!
//! Directories are lists of 32-byte entries. Deleting a file overwrites the
//! first byte of its entry with 0xE5 and frees its clusters in the FAT, but
//! the rest — name, size, first cluster, date — stays. The FAT chain is gone,
//! so we assume the data is contiguous from the first cluster, which it
//! usually is on cards written by cameras and phones.

use std::collections::HashSet;
use std::io;

use super::{is_noise, join, le16, le32, utf16, Allocation, DeletedFile, Modified, Timestamp, Volume};
use crate::source::{Piece, Window};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Fat12,
    Fat16,
    Fat32,
}

struct Layout {
    kind:     Kind,
    cluster:  u64,
    fat:      u64,           // byte offset of the first FAT
    fat_len:  u64,
    root:     Root,
    data:     u64,           // byte offset of cluster 2
    clusters: u32,           // number of data clusters
}

enum Root {
    Fixed { offset: u64, len: u64 }, // FAT12/16: a fixed area before the data
    Cluster(u32),                    // FAT32: an ordinary cluster chain
}

const DIRECTORY: u8 = 0x10;
const VOLUME_LABEL: u8 = 0x08;
const LONG_NAME: u8 = 0x0F;
const DELETED: u8 = 0xE5;
const MAX_DEPTH: usize = 32;

pub fn detect(boot: &[u8; 512]) -> bool {
    layout(boot, 0).is_some()
}

fn layout(b: &[u8; 512], volume: u64) -> Option<Layout> {
    let jump = (b[0] == 0xEB && b[2] == 0x90) || b[0] == 0xE9;
    if !jump || b[510..512] != [0x55, 0xAA] {
        return None;
    }
    let sector = le16(b, 11) as u64;
    let per_cluster = b[13] as u64;
    let reserved = le16(b, 14) as u64;
    let fats = b[16] as u64;
    let root_entries = le16(b, 17) as u64;
    let total = match le16(b, 19) { 0 => le32(b, 32) as u64, n => n as u64 };
    let fat_size = match le16(b, 22) { 0 => le32(b, 36) as u64, n => n as u64 };

    if ![512, 1024, 2048, 4096].contains(&sector)
        || !per_cluster.is_power_of_two()
        || reserved == 0
        || !(1..=2).contains(&fats)
        || fat_size == 0
    {
        return None;
    }
    let root_sectors = (root_entries * 32).div_ceil(sector);
    let first_data = reserved + fats * fat_size + root_sectors;
    let clusters = total.checked_sub(first_data)? / per_cluster;
    let kind = if le16(b, 22) == 0 {
        Kind::Fat32
    } else if clusters < 4085 {
        Kind::Fat12
    } else {
        Kind::Fat16
    };
    let fat = volume + reserved * sector;
    let root = match kind {
        Kind::Fat32 => Root::Cluster(le32(b, 44)),
        _ => Root::Fixed { offset: fat + fats * fat_size * sector, len: root_sectors * sector },
    };
    Some(Layout {
        kind,
        cluster: per_cluster * sector,
        fat,
        fat_len: fat_size * sector,
        root,
        data: volume + first_data * sector,
        clusters: clusters.min(0x0FFF_FFF0) as u32,
    })
}

pub fn read(w: &mut Window, volume: u64, len: u64) -> io::Result<Volume> {
    let boot = w.array::<512>(volume)?.ok_or(io::ErrorKind::UnexpectedEof)?;
    let layout = layout(&boot, volume).ok_or(io::ErrorKind::InvalidData)?;
    let table = w.bytes(layout.fat, layout.fat_len as usize)?.to_vec();
    let mut reader = Reader { w, layout, table, files: Vec::new(), seen: HashSet::new() };

    let root = match reader.layout.root {
        Root::Fixed { offset, len } => reader.w.bytes(offset, len as usize)?.to_vec(),
        Root::Cluster(first) => {
            reader.seen.insert(first);
            reader.chain(first)?
        }
    };
    reader.directory(&root, "/", false, 0)?;

    let mut allocation = Allocation::new(volume..volume + len, reader.layout.data, reader.layout.cluster, reader.layout.clusters as usize);
    for i in 0..reader.layout.clusters {
        if reader.next(i + 2) != 0 {
            allocation.mark(i as usize);
        }
    }
    let filesystem = match reader.layout.kind {
        Kind::Fat12 => "FAT12",
        Kind::Fat16 => "FAT16",
        Kind::Fat32 => "FAT32",
    };
    Ok(Volume { filesystem, files: reader.files, allocation: Some(allocation) })
}

struct Reader<'w, 'a> {
    w:      &'w mut Window<'a>,
    layout: Layout,
    table:  Vec<u8>,       // the first FAT, in memory
    files:  Vec<DeletedFile>,
    seen:   HashSet<u32>,  // directories already read (guards against loops)
}

impl Reader<'_, '_> {
    /// The FAT entry for `cluster`: the next cluster in its chain, 0 if free.
    fn next(&self, cluster: u32) -> u32 {
        let t = &self.table;
        let n = cluster as usize;
        let get16 = |at: usize| t.get(at..at + 2).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as u32);
        match self.layout.kind {
            Kind::Fat12 => {
                let v = get16(n + n / 2);
                if n.is_multiple_of(2) { v & 0xFFF } else { v >> 4 }
            }
            Kind::Fat16 => get16(n * 2),
            Kind::Fat32 => t.get(n * 4..n * 4 + 4).map_or(0, |b| le32(b, 0) & 0x0FFF_FFFF),
        }
    }

    fn valid(&self, cluster: u32) -> bool {
        (2..self.layout.clusters + 2).contains(&cluster)
    }

    fn offset(&self, cluster: u32) -> u64 {
        self.layout.data + (cluster as u64 - 2) * self.layout.cluster
    }

    /// The contents of a live cluster chain.
    fn chain(&mut self, mut cluster: u32) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        for _ in 0..self.layout.clusters {
            if !self.valid(cluster) {
                break;
            }
            let offset = self.offset(cluster);
            out.extend_from_slice(self.w.bytes(offset, self.layout.cluster as usize)?);
            cluster = self.next(cluster);
        }
        Ok(out)
    }

    fn directory(&mut self, bytes: &[u8], folder: &str, deleted_parent: bool, depth: usize) -> io::Result<()> {
        let mut long_parts: Vec<&[u8]> = Vec::new();
        for e in bytes.chunks_exact(32) {
            if e[0] == 0x00 {
                break; // end of directory
            }
            let attr = e[11];
            if attr == LONG_NAME {
                long_parts.push(e);
                continue;
            }
            let parts = std::mem::take(&mut long_parts);
            if attr & VOLUME_LABEL != 0 {
                continue;
            }
            let deleted = e[0] == DELETED || deleted_parent;
            let name = name(e, &parts);
            if name == "." || name == ".." {
                continue;
            }

            let high = if self.layout.kind == Kind::Fat32 { (le16(e, 20) as u32) << 16 } else { 0 };
            let first = high | le16(e, 26) as u32;
            let size = le32(e, 28) as u64;

            if attr & DIRECTORY != 0 {
                if depth >= MAX_DEPTH || !self.valid(first) || !self.seen.insert(first) {
                    continue;
                }
                let contents = if deleted { self.deleted_directory(first)? } else { self.chain(first)? };
                self.directory(&contents, &join(folder, &name), deleted, depth + 1)?;
            } else if deleted && size > 0 && self.valid(first) && !is_noise(&name) {
                self.deleted_file(name, folder, first, size, e);
            }
        }
        Ok(())
    }

    /// A deleted folder's entries: its chain is gone, so read the clusters
    /// that follow its first one — as long as that cluster still looks like
    /// the same folder (it starts with the "." entry).
    fn deleted_directory(&mut self, first: u32) -> io::Result<Vec<u8>> {
        let offset = self.offset(first);
        let len = (self.layout.cluster * 16).min(256 * 1024);
        let bytes = self.w.bytes(offset, len as usize)?.to_vec();
        // Some writers put long-name entries even before ".", so skip those.
        let dot = bytes.chunks_exact(32).find(|e| e[11] != LONG_NAME);
        Ok(if dot.is_some_and(|e| &e[..11] == b".          ") { bytes } else { Vec::new() })
    }

    fn deleted_file(&mut self, name: String, folder: &str, first: u32, size: u64, entry: &[u8]) {
        let count = size.div_ceil(self.layout.cluster) as u32;
        let last = first as u64 + count as u64;
        let fits = last <= self.layout.clusters as u64 + 2;
        // Intact if every cluster it needs is still free.
        let free = fits && (first..first + count).all(|c| self.next(c) == 0);
        self.files.push(DeletedFile {
            name,
            folder: folder.to_string(),
            size,
            pieces: vec![Piece::Disk { offset: self.offset(first), len: size }],
            modified: Timestamp::from_dos(le16(entry, 24), le16(entry, 22)).map(Modified::Local),
            intact: free,
        });
    }
}

/// The best name for an entry: its long name if one survives, otherwise the
/// 8.3 short name (with "_" for a first letter lost to deletion).
fn name(entry: &[u8], long_parts: &[&[u8]]) -> String {
    let short = short_name(entry);
    if long_parts.is_empty() {
        return short;
    }
    // Long name pieces are stored last-first, 13 UTF-16 characters each.
    let mut units = Vec::new();
    for part in long_parts.iter().rev() {
        units.extend_from_slice(&part[1..11]);
        units.extend_from_slice(&part[14..26]);
        units.extend_from_slice(&part[28..32]);
    }
    let long = utf16(&units);

    // The pieces carry a checksum of the short name. Deletion overwrote the
    // short name's first byte, so try it with the long name's first letter.
    let mut raw: [u8; 11] = entry[..11].try_into().unwrap();
    if raw[0] == DELETED {
        raw[0] = short_initial(&long);
    }
    let sum = raw.iter().fold(0u8, |s, &b| s.rotate_right(1).wrapping_add(b));
    if !long.is_empty() && long_parts.iter().all(|p| p[13] == sum) { long } else { short }
}

/// The first character of the short name FAT derives from a long name:
/// leading dots and spaces are dropped, letters uppercased, and characters
/// a short name can't hold become "_".
fn short_initial(long: &str) -> u8 {
    match long.trim_start_matches(['.', ' ']).chars().next() {
        Some(c) if c.is_ascii_alphanumeric() || "$%'-_@~`!(){}^#&".contains(c) => c.to_ascii_uppercase() as u8,
        _ => b'_',
    }
}

fn short_name(e: &[u8]) -> String {
    let text = |bytes: &[u8], lower: bool| -> String {
        bytes
            .iter()
            .map(|&b| match b {
                b' ' => None,
                0x21..=0x7E => Some(if lower { b.to_ascii_lowercase() } else { b } as char),
                _ => Some('_'),
            })
            .map_while(|c| c)
            .collect()
    };
    let mut base = text(&e[..8], e[12] & 0x08 != 0);
    if e[0] == DELETED {
        base.replace_range(..1, "_");
    } else if e[0] == 0x05 {
        base.replace_range(..1, "\u{E5}"); // a real first character 0xE5
    }
    let ext = text(&e[8..11], e[12] & 0x10 != 0);
    if ext.is_empty() { base } else { format!("{base}.{ext}") }
}
