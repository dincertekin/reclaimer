//! Where bytes come from. A `BlockSource` is anything we can read at an
//! arbitrary offset — a disk image file or a physical drive. The scanner only
//! ever talks to this trait.

pub mod drives;
mod window;

pub use window::Window;

use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
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

/// One stretch of a file being recovered. Files carved by content are one
/// `Disk` piece; files found through a filesystem may be fragmented, sparse,
/// or (tiny NTFS files) stored inline in the filesystem's own records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Piece {
    Disk { offset: u64, len: u64 },
    Zeros(u64),
    Bytes(Vec<u8>),
}

/// Writes the pieces, in order, to a new file at `path`.
pub fn save(src: &mut dyn BlockSource, pieces: &[Piece], path: &Path) -> io::Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    let mut window = Window::new(src);
    for piece in pieces {
        match piece {
            Piece::Disk { offset, len } => {
                let (mut pos, end) = (*offset, offset + len);
                while pos < end {
                    let chunk = window.bytes(pos, (end - pos).min(1 << 20) as usize)?;
                    if chunk.is_empty() {
                        return Err(io::ErrorKind::UnexpectedEof.into());
                    }
                    out.write_all(chunk)?;
                    pos += chunk.len() as u64;
                }
            }
            Piece::Zeros(len) => io::copy(&mut io::repeat(0).take(*len), &mut out).map(|_| ())?,
            Piece::Bytes(bytes) => out.write_all(bytes)?,
        }
    }
    out.flush()
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
