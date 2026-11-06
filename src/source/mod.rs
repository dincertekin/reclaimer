//! Where bytes come from. A `BlockSource` is anything we can read at an
//! arbitrary offset — a disk image file or a physical drive. The scanner only
//! ever talks to this trait.

pub mod drives;

use std::fs::File;
use std::io;
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

/// A source backed by an open file: a disk image or a raw device.
pub struct FileSource {
    file:       File,
    size:       u64,
    block_size: u64,
}

impl FileSource {
    /// Opens a disk image file (.img, .dd, .raw, ...).
    pub fn image(path: &Path) -> io::Result<Self> {
        let file = File::open(path)?;
        let size = file.metadata()?.len();
        Ok(Self { file, size, block_size: 512 })
    }

    /// Wraps an already-opened device. Devices can't report their own size
    /// through file metadata, so the platform code supplies it.
    pub(crate) fn device(file: File, size: u64, block_size: u64) -> Self {
        Self { file, size, block_size: block_size.max(512) }
    }

    /// A second handle to the same source. Reads are positional, so the two
    /// never disturb each other — and a drive needs only one permission prompt.
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self { file: self.file.try_clone()?, ..*self })
    }
}

impl BlockSource for FileSource {
    fn size(&self) -> u64 {
        self.size
    }

    fn block_size(&self) -> u64 {
        self.block_size
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
        // A single read may return less than asked even mid-file, so loop
        // until the buffer is full or we reach the end.
        let mut filled = 0;
        while filled < buf.len() {
            match read_at(&self.file, &mut buf[filled..], offset + filled as u64) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        Ok(filled)
    }
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
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
