//! NTFS — Windows disks. Every file has a record in the Master File Table
//! (MFT): its name, parent folder, timestamps and a map of where its data
//! lives (or, for tiny files, the data itself). Deleting a file just marks
//! its record unused; until the record is reused, all of that survives —
//! including fragmented files, which the map describes exactly.

use std::collections::HashMap;
use std::io;

use super::{is_noise, join, le16, le32, le64, utf16, Allocation, DeletedFile, Modified, Timestamp, Volume};
use crate::source::{Piece, Window};

const IN_USE: u16 = 0x01;
const IS_DIRECTORY: u16 = 0x02;
const STANDARD_INFORMATION: u32 = 0x10;
const FILE_NAME: u32 = 0x30;
const DATA: u32 = 0x80;
const END: u32 = 0xFFFF_FFFF;
const ROOT: u64 = 5;
const BITMAP: u64 = 6;
const FIRST_USER_RECORD: u64 = 24; // 0–23 are NTFS's own files
const MAX_RECORDS: u64 = 1 << 24;
const MAX_BITMAP: u64 = 256 << 20;

pub fn detect(boot: &[u8; 512]) -> bool {
    &boot[3..11] == b"NTFS    " && boot[510..512] == [0x55, 0xAA]
}

/// One cluster run: where on disk (None = sparse, reads as zeros) and how
/// many clusters.
type Run = (Option<u64>, u64);

enum Data {
    Resident(Vec<u8>),
    Runs { runs: Vec<Run>, size: u64 },
}

struct Record {
    in_use:   bool,
    is_dir:   bool,
    sequence: u16,
    name:     Option<(String, u64, u16)>, // name, parent record, parent sequence
    modified: Option<i64>,
    data:     Option<Data>,
}

pub fn read(w: &mut Window, volume: u64, len: u64) -> io::Result<Volume> {
    let b = w.array::<512>(volume)?.ok_or(io::ErrorKind::UnexpectedEof)?;
    let sector = le16(&b, 11) as u64;
    let per_cluster = match b[13] {
        n if n > 0x80 => 1u64 << (256 - n as u32).min(31), // stored as a negative power of two
        n => n as u64,
    };
    let cluster = sector * per_cluster;
    if ![512, 1024, 2048, 4096].contains(&sector) || cluster == 0 {
        return Err(io::ErrorKind::InvalidData.into());
    }
    let record_size = match b[64] as i8 {
        n if n < 0 => 1u64 << (-n as u32).min(16),
        n => n as u64 * cluster,
    };
    let mft = Mft { volume, cluster, record_size };

    // Record 0 is the MFT itself: its data map says where all records live.
    let first = mft.record_at(w, volume + le64(&b, 48) * cluster)?.ok_or(io::ErrorKind::InvalidData)?;
    let Some(Data::Runs { runs, size }) = first.data else { return Err(io::ErrorKind::InvalidData.into()) };
    let extents = mft.extents(&runs);
    let count = (size / record_size).min(MAX_RECORDS);

    let mut folders: HashMap<u64, Record> = HashMap::new();
    let mut deleted: Vec<(u64, Record)> = Vec::new();
    let mut bitmap = Vec::new();
    for index in 0..count {
        let Some(offset) = locate(&extents, index * record_size) else { break };
        let Some(record) = mft.record_at(w, offset)? else { continue };
        if index == BITMAP
            && let Some(Data::Runs { runs, size }) = &record.data
        {
            bitmap = mft.read_runs(w, runs, (*size).min(MAX_BITMAP))?;
        }
        if record.is_dir {
            folders.insert(index, record);
        } else if !record.in_use && index >= FIRST_USER_RECORD && record.name.is_some() && record.data.is_some() {
            deleted.push((index, record));
        }
    }

    let total_clusters = len / cluster;
    let mut allocation = Allocation::new(volume..volume + len, volume, cluster, total_clusters as usize);
    for (i, byte) in bitmap.iter().enumerate() {
        for bit in 0..8 {
            if byte & (1 << bit) != 0 {
                allocation.mark(i * 8 + bit);
            }
        }
    }

    let files = deleted
        .into_iter()
        .filter_map(|(_, record)| {
            let (name, parent, parent_seq) = record.name?;
            if is_noise(&name) || name.starts_with('$') {
                return None;
            }
            let folder = folder_path(&folders, parent, parent_seq, 0);
            let modified = record.modified.map(Modified::Utc);
            Some(match record.data? {
                Data::Resident(bytes) => DeletedFile {
                    name, folder, size: bytes.len() as u64, pieces: vec![Piece::Bytes(bytes)], modified, intact: true,
                },
                Data::Runs { runs, size } => {
                    let intact = runs.iter().all(|&(lcn, n)| match lcn {
                        Some(lcn) => (lcn..lcn + n).all(|c| !allocation.is_used(c as usize)),
                        None => true,
                    });
                    DeletedFile { name, folder, size, pieces: mft.pieces(&runs, size), modified, intact }
                }
            })
        })
        .filter(|f| f.size > 0)
        .collect();

    Ok(Volume { filesystem: "NTFS", files, allocation: Some(allocation) })
}

/// The folder path of a parent record. A deleted parent's record may since
/// have been reused for something else; its sequence number tells us.
fn folder_path(folders: &HashMap<u64, Record>, index: u64, sequence: u16, depth: usize) -> String {
    if index == ROOT {
        return "/".into();
    }
    let same_folder = |r: &Record| r.sequence == sequence || (!r.in_use && r.sequence == sequence.wrapping_add(1));
    match folders.get(&index) {
        Some(folder) if depth < 64 && same_folder(folder) => match &folder.name {
            Some((name, parent, parent_seq)) => join(&folder_path(folders, *parent, *parent_seq, depth + 1), name),
            None => "/Unknown Folder".into(),
        },
        _ => "/Unknown Folder".into(),
    }
}

/// Byte offset of `pos` within the MFT, given its extents on disk.
fn locate(extents: &[(u64, u64)], mut pos: u64) -> Option<u64> {
    for &(start, len) in extents {
        if pos < len {
            return Some(start + pos);
        }
        pos -= len;
    }
    None
}

struct Mft {
    volume:      u64,
    cluster:     u64,
    record_size: u64,
}

impl Mft {
    fn extents(&self, runs: &[Run]) -> Vec<(u64, u64)> {
        runs.iter().filter_map(|&(lcn, n)| lcn.map(|l| (self.volume + l * self.cluster, n * self.cluster))).collect()
    }

    fn pieces(&self, runs: &[Run], size: u64) -> Vec<Piece> {
        let mut pieces = Vec::new();
        let mut remaining = size;
        for &(lcn, n) in runs {
            if remaining == 0 {
                break;
            }
            let len = (n * self.cluster).min(remaining);
            pieces.push(match lcn {
                Some(lcn) => Piece::Disk { offset: self.volume + lcn * self.cluster, len },
                None => Piece::Zeros(len),
            });
            remaining -= len;
        }
        pieces
    }

    fn read_runs(&self, w: &mut Window, runs: &[Run], size: u64) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        for piece in self.pieces(runs, size) {
            match piece {
                Piece::Disk { offset, len } => out.extend_from_slice(w.bytes(offset, len as usize)?),
                Piece::Zeros(len) => out.resize(out.len() + len as usize, 0),
                Piece::Bytes(bytes) => out.extend(bytes),
            }
        }
        Ok(out)
    }

    fn record_at(&self, w: &mut Window, offset: u64) -> io::Result<Option<Record>> {
        let mut bytes = w.bytes(offset, self.record_size as usize)?.to_vec();
        if bytes.len() < self.record_size as usize || !apply_fixups(&mut bytes) {
            return Ok(None);
        }
        Ok(parse_record(&bytes))
    }
}

/// Each 512-byte stretch of a record ends with a check value; the real bytes
/// are kept in the record's header. Swap them back (and catch torn writes).
fn apply_fixups(rec: &mut [u8]) -> bool {
    if &rec[..4] != b"FILE" {
        return false;
    }
    let (usa, count) = (le16(rec, 4) as usize, le16(rec, 6) as usize);
    if count == 0 || usa + 2 * count > rec.len() {
        return false;
    }
    let check = [rec[usa], rec[usa + 1]];
    for i in 1..count {
        let end = i * 512;
        if end > rec.len() || rec[end - 2..end] != check {
            return false;
        }
        rec[end - 2] = rec[usa + 2 * i];
        rec[end - 1] = rec[usa + 2 * i + 1];
    }
    true
}

fn parse_record(rec: &[u8]) -> Option<Record> {
    if le64(rec, 32) != 0 {
        return None; // an extension of another record
    }
    let flags = le16(rec, 22);
    let mut record = Record {
        in_use:   flags & IN_USE != 0,
        is_dir:   flags & IS_DIRECTORY != 0,
        sequence: le16(rec, 16),
        name:     None,
        modified: None,
        data:     None,
    };
    let mut best_namespace = u8::MAX;

    let mut pos = le16(rec, 20) as usize;
    while pos + 16 <= rec.len() {
        let kind = le32(rec, pos);
        let len = le32(rec, pos + 4) as usize;
        if kind == END || len < 16 || pos + len > rec.len() {
            break;
        }
        let attr = &rec[pos..pos + len];
        pos += len;

        let resident = attr[8] == 0;
        let named = attr[9] != 0;
        match kind {
            STANDARD_INFORMATION if resident => {
                if let Some(content) = resident_content(attr).filter(|c| c.len() >= 16) {
                    record.modified = Timestamp::filetime_to_unix(le64(content, 8));
                }
            }
            FILE_NAME if resident => {
                let Some(c) = resident_content(attr).filter(|c| c.len() >= 66) else { continue };
                let (name_len, namespace) = (c[64] as usize, c[65]);
                // Prefer the full Windows name over the 8.3 DOS alias.
                let rank = match namespace { 1 | 3 => 0, 0 => 1, _ => 2 };
                if rank < best_namespace && 66 + 2 * name_len <= c.len() {
                    best_namespace = rank;
                    let parent = le64(c, 0);
                    let name = utf16(&c[66..66 + 2 * name_len]);
                    record.name = Some((name, parent & 0xFFFF_FFFF_FFFF, (parent >> 48) as u16));
                }
            }
            DATA if !named => {
                record.data = if resident {
                    resident_content(attr).map(|c| Data::Resident(c.to_vec()))
                } else if attr.len() >= 64 {
                    let runlist = attr.get(le16(attr, 32) as usize..).unwrap_or(&[]);
                    Some(Data::Runs { runs: decode_runs(runlist), size: le64(attr, 48) })
                } else {
                    None
                };
            }
            _ => {}
        }
    }
    Some(record)
}

fn resident_content(attr: &[u8]) -> Option<&[u8]> {
    let (len, offset) = (le32(attr, 16) as usize, le16(attr, 20) as usize);
    attr.get(offset..offset + len)
}

/// The data map: each run is a header byte (sizes of the two fields), a
/// length in clusters, and a start relative to the previous run.
fn decode_runs(bytes: &[u8]) -> Vec<Run> {
    let mut runs = Vec::new();
    let (mut pos, mut lcn) = (0usize, 0i64);
    while let Some(&header) = bytes.get(pos) {
        let (len_size, off_size) = ((header & 0x0F) as usize, (header >> 4) as usize);
        if header == 0 || len_size == 0 || len_size > 8 || off_size > 8 || pos + 1 + len_size + off_size > bytes.len() {
            break;
        }
        pos += 1;
        let length = le_uint(&bytes[pos..pos + len_size]);
        pos += len_size;
        if off_size == 0 {
            runs.push((None, length)); // sparse
        } else {
            lcn += le_int(&bytes[pos..pos + off_size]);
            pos += off_size;
            if lcn < 0 {
                break;
            }
            runs.push((Some(lcn as u64), length));
        }
    }
    runs
}

fn le_uint(b: &[u8]) -> u64 {
    b.iter().rev().fold(0, |v, &x| (v << 8) | x as u64)
}

fn le_int(b: &[u8]) -> i64 {
    let shift = 64 - 8 * b.len() as u32;
    ((le_uint(b) << shift) as i64) >> shift // sign-extend
}

#[cfg(test)]
pub mod build {
    //! Writes a tiny NTFS volume for tests — macOS and Linux can't format one.
    use super::*;

    pub const CLUSTER: u64 = 4096;
    pub const RECORD: usize = 1024;

    pub fn encode_runs(runs: &[Run]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut prev = 0i64;
        for &(lcn, len) in runs {
            let len_bytes = minimal(len as i64, false);
            match lcn {
                None => {
                    out.push(len_bytes.len() as u8);
                    out.extend(len_bytes);
                }
                Some(lcn) => {
                    let delta = minimal(lcn as i64 - prev, true);
                    out.push(((delta.len() as u8) << 4) | len_bytes.len() as u8);
                    out.extend(len_bytes);
                    out.extend(delta);
                    prev = lcn as i64;
                }
            }
        }
        out.push(0);
        out
    }

    fn minimal(v: i64, signed: bool) -> Vec<u8> {
        let bytes = v.to_le_bytes();
        (1..=8)
            .map(|n| bytes[..n].to_vec())
            .find(|b| if signed { le_int(b) == v } else { le_uint(b) == v as u64 })
            .unwrap()
    }

    fn pad8(v: &mut Vec<u8>) {
        v.resize(v.len().next_multiple_of(8), 0);
    }

    pub fn resident(kind: u32, content: &[u8]) -> Vec<u8> {
        let mut a = vec![0u8; 24];
        a[..4].copy_from_slice(&kind.to_le_bytes());
        a[16..20].copy_from_slice(&(content.len() as u32).to_le_bytes());
        a[20..22].copy_from_slice(&24u16.to_le_bytes());
        a.extend_from_slice(content);
        pad8(&mut a);
        let len = a.len() as u32;
        a[4..8].copy_from_slice(&len.to_le_bytes());
        a
    }

    pub fn non_resident_data(runs: &[Run], size: u64) -> Vec<u8> {
        let mut a = vec![0u8; 64];
        a[..4].copy_from_slice(&DATA.to_le_bytes());
        a[8] = 1;
        a[32..34].copy_from_slice(&64u16.to_le_bytes());
        a[48..56].copy_from_slice(&size.to_le_bytes());
        a.extend(encode_runs(runs));
        pad8(&mut a);
        let len = a.len() as u32;
        a[4..8].copy_from_slice(&len.to_le_bytes());
        a
    }

    pub fn file_name(parent: u64, parent_seq: u16, name: &str) -> Vec<u8> {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut c = vec![0u8; 66];
        c[..8].copy_from_slice(&(parent | (parent_seq as u64) << 48).to_le_bytes());
        c[64] = units.len() as u8;
        c[65] = 1; // Win32 namespace
        units.iter().for_each(|u| c.extend(u.to_le_bytes()));
        resident(FILE_NAME, &c)
    }

    pub fn standard_information(modified_unix: i64) -> Vec<u8> {
        let ticks = (modified_unix as u64 + 11_644_473_600) * 10_000_000;
        let mut c = vec![0u8; 48];
        c[8..16].copy_from_slice(&ticks.to_le_bytes());
        resident(STANDARD_INFORMATION, &c)
    }

    /// A record with its fixups applied, as it would sit on disk.
    pub fn record(sequence: u16, flags: u16, attributes: &[Vec<u8>]) -> Vec<u8> {
        let mut r = vec![0u8; RECORD];
        r[..4].copy_from_slice(b"FILE");
        r[4..6].copy_from_slice(&0x30u16.to_le_bytes()); // update sequence array offset
        r[6..8].copy_from_slice(&3u16.to_le_bytes());    // 1 check value + 2 sectors
        r[16..18].copy_from_slice(&sequence.to_le_bytes());
        r[20..22].copy_from_slice(&0x38u16.to_le_bytes());
        r[22..24].copy_from_slice(&flags.to_le_bytes());
        let mut pos = 0x38;
        for a in attributes {
            r[pos..pos + a.len()].copy_from_slice(a);
            pos += a.len();
        }
        r[pos..pos + 4].copy_from_slice(&END.to_le_bytes());
        // Fixups: stash each sector's last two bytes, write the check value.
        let check = [0x01, 0x00];
        r[0x30..0x32].copy_from_slice(&check);
        for i in 1..=2 {
            let end = i * 512;
            let (a, b) = (r[end - 2], r[end - 1]);
            r[0x30 + 2 * i] = a;
            r[0x31 + 2 * i] = b;
            r[end - 2..end].copy_from_slice(&check);
        }
        r
    }

    pub fn boot_sector(total_clusters: u64, mft_lcn: u64) -> Vec<u8> {
        let mut b = vec![0u8; 512];
        b[..3].copy_from_slice(&[0xEB, 0x52, 0x90]);
        b[3..11].copy_from_slice(b"NTFS    ");
        b[11..13].copy_from_slice(&512u16.to_le_bytes());
        b[13] = (CLUSTER / 512) as u8;
        b[40..48].copy_from_slice(&(total_clusters * CLUSTER / 512).to_le_bytes());
        b[48..56].copy_from_slice(&mft_lcn.to_le_bytes());
        b[64] = (-10i8) as u8; // 2^10 = 1024-byte records
        b[510..512].copy_from_slice(&[0x55, 0xAA]);
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_round_trip_with_negative_and_sparse() {
        let runs: Vec<Run> = vec![(Some(1000), 16), (Some(40), 3), (None, 8), (Some(70_000), 1)];
        assert_eq!(decode_runs(&build::encode_runs(&runs)), runs);
    }

    #[test]
    fn fixups_restore_sector_ends() {
        let r = build::record(1, IN_USE, &[build::file_name(ROOT, 5, "a.txt")]);
        let mut bytes = r.clone();
        assert!(apply_fixups(&mut bytes));
        assert_eq!(&bytes[510..512], &[0, 0]);
        let mut torn = r;
        torn[1022] = 0x99; // second sector didn't finish writing
        assert!(!apply_fixups(&mut torn));
    }
}
