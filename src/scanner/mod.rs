use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub struct FileSignature {
    pub name: &'static str,
    pub magic: &'static [u8],
    pub offset: usize,
    pub extension: &'static str,
    pub end_marker: Option<&'static [u8]>,
    pub max_size_bytes: usize,
}

// Known file signatures for detecting file types
pub const SIGNATURES: &[FileSignature] = &[
    FileSignature {
        name: "JPEG",
        magic: &[0xFF, 0xD8, 0xFF, 0xE0],
        offset: 0,
        extension: "jpg",
        end_marker: Some(&[0xFF, 0xD9]),
        max_size_bytes: 10 * 1024 * 1024, // 10 MB fallback limit
    },
    FileSignature {
        name: "PNG",
        magic: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        offset: 0,
        extension: "png",
        end_marker: Some(&[
            0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]),
        max_size_bytes: 10 * 1024 * 1024, // 10 MB fallback limit
    },
    FileSignature {
        name: "MP4",
        magic: &[0x66, 0x74, 0x79, 0x70],
        offset: 4,
        extension: "mp4",
        end_marker: None,
        max_size_bytes: 500 * 1024 * 1024, // 500 MB limit
    },
    FileSignature {
        name: "PDF",
        magic: &[0x25, 0x50, 0x44, 0x46],
        offset: 0,
        extension: "pdf",
        end_marker: Some(&[0x25, 0x25, 0x45, 0x4F, 0x46]),
        max_size_bytes: 50 * 1024 * 1024, // 50 MB fallback limit
    },
];

pub fn detect_signature(sector: &[u8]) -> Option<&'static FileSignature> {
    for sig in SIGNATURES {
        let start = sig.offset;
        let end = sig.offset + sig.magic.len();
        if sector.len() >= end && &sector[start..end] == sig.magic {
            return Some(sig);
        }
    }
    None
}

pub fn extract_file(
    first_sector: &[u8],
    sig: &'static FileSignature,
    image: &mut crate::disk::DiskImage,
    sector_number: u64,
    output_dir: &Path,
    file_index: u32,
) -> io::Result<String> {
    let mut data: Vec<u8> = Vec::new();
    data.extend_from_slice(first_sector);
    let mut current_sector = sector_number + 1;

    loop {
        if data.len() >= sig.max_size_bytes {
            println!(
                "Warning: hit size limit for {}, saving what we have.",
                sig.name
            );
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

    if let Some(marker) = sig.end_marker {
        if let Some(end_pos) = find_subsequence_last(&data, marker) {
            data.truncate(end_pos + marker.len());
        }
    }

    let filename = format!("recovered_{}.{}", file_index, sig.extension);
    let output_path = output_dir.join(&filename);
    let mut file = fs::File::create(&output_path)?;
    file.write_all(&data)?;

    Ok(filename)
}

#[allow(dead_code)]
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn find_subsequence_last(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}
