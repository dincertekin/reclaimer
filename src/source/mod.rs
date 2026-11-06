//! Where bytes come from. A `BlockSource` is anything we can read at an
//! arbitrary offset: a disk image today, a physical drive later. The scanner
//! only ever talks to this trait, so new sources need no scanner changes.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

pub trait BlockSource {
    /// Total size in bytes.
    fn size(&self) -> u64;

    /// Smallest readable unit. Raw devices reject reads that are not aligned
    /// to it, so callers align every read to this.
    fn block_size(&self) -> u64 {
        512
    }

    /// Fills `buf` from `offset`. Returns fewer bytes than requested only at
    /// the end of the source.
    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize>;
}

/// A raw disk image file (.img, .dd, .raw, ...).
pub struct ImageFile {
    file: File,
    size: u64,
}

impl ImageFile {
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size })
    }
}

impl BlockSource for ImageFile {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        // A single read() may return less than asked even mid-file, so loop
        // until the buffer is full or we hit the end.
        let mut filled = 0;
        while filled < buf.len() {
            match self.file.read(&mut buf[filled..]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(filled)
    }
}

/// In-memory source, so tests can build disk images on the fly.
#[cfg(test)]
impl BlockSource for Vec<u8> {
    fn size(&self) -> u64 {
        self.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        let start = (offset as usize).min(self.len());
        let n = buf.len().min(self.len() - start);
        buf[..n].copy_from_slice(&self[start..start + n]);
        Ok(n)
    }
}
