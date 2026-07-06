mod signatures;
pub use signatures::{FileSignature, SIGNATURES};

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub fn detect_signature(sector: &[u8]) -> Option<&'static FileSignature> {
    for sig in SIGNATURES {
        let start = sig.offset;
        let end   = sig.offset + sig.magic.len();
        if sector.len() >= end && &sector[start..end] == sig.magic {
            return Some(sig);
        }
    }
    None
}

pub fn extract_file(
    first_sector:  &[u8],
    sig:           &'static FileSignature,
    image:         &mut crate::disk::DiskImage,
    sector_number: u64,
    output_dir:    &Path,
    file_index:    u32,
) -> io::Result<String> {
    let mut data = Vec::from(first_sector);
    let mut current_sector = sector_number + 1;

    loop {
        if data.len() >= sig.max_size_bytes {
            eprintln!("Warning: size limit reached for {}, truncating.", sig.name);
            break;
        }
        match image.read_sector(current_sector) {
            Ok(sector) => {
                data.extend_from_slice(&sector);
                current_sector += 1;
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }

    // Use the LAST occurrence of the end marker, not the first. JPEG's FF D9
    // can appear inside the file before the real end, which would truncate it.
    if let Some(marker) = sig.end_marker
        && let Some(end_pos) = find_last(&data, marker)
    {
        data.truncate(end_pos + marker.len());
    }

    let filename    = format!("recovered_{}.{}", file_index, sig.extension);
    let output_path = output_dir.join(&filename);
    fs::File::create(&output_path)?.write_all(&data)?;

    Ok(filename)
}

fn find_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|window| window == needle)
}
