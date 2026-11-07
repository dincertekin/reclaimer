//! Where the volumes are. Most drives have a partition table — MBR (older,
//! still common on SD cards) or GPT (modern) — but small cards and floppies
//! are often formatted whole, with the filesystem starting at byte 0.

use std::io;
use super::{le32, le64, is_boot_sector};
use crate::source::Window;

const EXTENDED: [u8; 3] = [0x05, 0x0F, 0x85];

/// (offset, length) in bytes of each volume.
pub fn find(w: &mut Window) -> io::Result<Vec<(u64, u64)>> {
    let size = w.size();
    let Some(first) = w.array::<512>(0)? else { return Ok(Vec::new()) };
    if is_boot_sector(&first) {
        return Ok(vec![(0, size)]); // no partition table
    }
    if first[510..512] != [0x55, 0xAA] {
        return Ok(Vec::new());
    }

    let entries: Vec<(u8, u64, u64)> = (0..4)
        .map(|i| {
            let e = &first[446 + 16 * i..462 + 16 * i];
            (e[4], le32(e, 8) as u64, le32(e, 12) as u64)
        })
        .filter(|&(kind, _, sectors)| kind != 0 && sectors > 0)
        .collect();

    // A GPT disk carries a "protective" MBR with one entry of type 0xEE.
    if entries.iter().any(|&(kind, ..)| kind == 0xEE) {
        return gpt(w);
    }

    let mut volumes = Vec::new();
    for (kind, start, sectors) in entries {
        if EXTENDED.contains(&kind) {
            logical(w, start, &mut volumes)?;
        } else {
            volumes.push((start * 512, sectors * 512));
        }
    }
    Ok(volumes)
}

fn gpt(w: &mut Window) -> io::Result<Vec<(u64, u64)>> {
    // The header sits in the second logical block: 512 bytes in, or 4096 on
    // drives with 4K sectors.
    for block in [512u64, 4096] {
        let Some(header) = w.array::<92>(block)? else { continue };
        if &header[..8] != b"EFI PART" {
            continue;
        }
        let table = le64(&header, 72) * block;
        let count = le32(&header, 80).min(256) as u64;
        let entry_size = le32(&header, 84).max(128) as u64;
        let bytes = w.bytes(table, (count * entry_size) as usize)?.to_vec();
        let volumes = bytes
            .chunks_exact(entry_size as usize)
            .filter(|e| e[..16].iter().any(|&b| b != 0)) // unused entries are all zero
            .filter_map(|e| {
                let (first, last) = (le64(e, 32), le64(e, 40));
                (last >= first).then(|| (first * block, (last - first + 1) * block))
            })
            .collect();
        return Ok(volumes);
    }
    Ok(Vec::new())
}

/// Logical partitions inside an MBR extended partition: a chain of extended
/// boot records, each describing one volume and pointing to the next.
fn logical(w: &mut Window, extended: u64, volumes: &mut Vec<(u64, u64)>) -> io::Result<()> {
    let mut ebr = extended;
    for _ in 0..128 {
        let Some(sector) = w.array::<512>(ebr * 512)? else { break };
        if sector[510..512] != [0x55, 0xAA] {
            break;
        }
        let (this, next) = (&sector[446..462], &sector[462..478]);
        if this[4] != 0 && le32(this, 12) > 0 {
            volumes.push(((ebr + le32(this, 8) as u64) * 512, le32(this, 12) as u64 * 512));
        }
        if next[4] == 0 {
            break;
        }
        ebr = extended + le32(next, 8) as u64;
    }
    Ok(())
}
