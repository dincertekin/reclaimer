pub struct FileSignature {
    pub name:          &'static str,
    pub magic:         &'static [u8],
    pub offset:        usize,       // byte offset within the sector where the magic starts
    pub extension:     &'static str,
    pub end_marker:    Option<&'static [u8]>,
    pub max_size_bytes: usize,
}

pub const SIGNATURES: &[FileSignature] = &[
    FileSignature {
        name:          "JPEG",
        magic:         &[0xFF, 0xD8, 0xFF, 0xE0],
        offset:        0,
        extension:     "jpg",
        end_marker:    Some(&[0xFF, 0xD9]),
        max_size_bytes: 10 * 1024 * 1024,
    },
    FileSignature {
        name:          "PNG",
        magic:         &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        offset:        0,
        extension:     "png",
        // IEND chunk: length(0) + "IEND" + CRC
        end_marker:    Some(&[0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82]),
        max_size_bytes: 10 * 1024 * 1024,
    },
    FileSignature {
        name:          "MP4",
        // The "ftyp" box starts at byte 4; bytes 0-3 are the box length.
        magic:         &[0x66, 0x74, 0x79, 0x70],
        offset:        4,
        extension:     "mp4",
        end_marker:    None,
        max_size_bytes: 500 * 1024 * 1024,
    },
    FileSignature {
        name:          "PDF",
        magic:         &[0x25, 0x50, 0x44, 0x46],
        offset:        0,
        extension:     "pdf",
        end_marker:    Some(&[0x25, 0x25, 0x45, 0x4F, 0x46]),
        max_size_bytes: 50 * 1024 * 1024,
    },
];
