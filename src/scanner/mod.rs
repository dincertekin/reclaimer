pub struct FileSignature {
    pub name: &'static str,
    pub magic: &'static [u8],
    pub offset: usize,
    #[allow(dead_code)]
    pub extension: &'static str,
}

pub const SIGNATURES: &[FileSignature] = &[
    FileSignature {
        name: "JPEG",
        magic: &[0xFF, 0xD8, 0xFF, 0xE0],
        offset: 0,
        extension: "jpg",
    },
    FileSignature {
        name: "PNG",
        magic: &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        offset: 0,
        extension: "png",
    },
    FileSignature {
        name: "MP4",
        magic: &[0x66, 0x74, 0x79, 0x70],
        offset: 4,
        extension: "mp4",
    },
    FileSignature {
        name: "PDF",
        magic: &[0x25, 0x50, 0x44, 0x46],
        offset: 0,
        extension: "pdf",
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
