//! A read-ahead buffer over a `BlockSource`. Carvers ask for small pieces at
//! arbitrary offsets; the window turns that into a few large, block-aligned
//! reads — millions of 512-byte syscalls become a handful of 4 MB ones.

use std::io;
use crate::source::BlockSource;

const WINDOW_SIZE: usize = 4 << 20;
const SEARCH_CHUNK: usize = 1 << 20;

pub struct Window<'a> {
    src:   &'a mut dyn BlockSource,
    size:  u64,
    start: u64,     // source offset of buf[0]
    buf:   Vec<u8>,
}

impl<'a> Window<'a> {
    pub fn new(src: &'a mut dyn BlockSource) -> Self {
        let size = src.size();
        Self { src, size, start: 0, buf: Vec::new() }
    }

    /// Up to `n` bytes at `offset`. Shorter only at the end of the source.
    pub fn bytes(&mut self, offset: u64, n: usize) -> io::Result<&[u8]> {
        if offset >= self.size {
            return Ok(&[]);
        }
        let end = offset.saturating_add(n as u64).min(self.size);
        if offset < self.start || end > self.start + self.buf.len() as u64 {
            self.fill(offset, (end - offset) as usize)?;
        }
        let from = (offset - self.start) as usize;
        let to   = ((end - self.start) as usize).min(self.buf.len());
        Ok(&self.buf[from.min(to)..to])
    }

    /// Exactly `N` bytes at `offset`, or `None` if the source ends first.
    pub fn array<const N: usize>(&mut self, offset: u64) -> io::Result<Option<[u8; N]>> {
        Ok(self.bytes(offset, N)?.try_into().ok())
    }

    /// Earliest occurrence of any needle starting in `from..to`.
    /// Returns its offset and which needle matched.
    pub fn find_any(&mut self, from: u64, to: u64, needles: &[&[u8]]) -> io::Result<Option<(u64, usize)>> {
        // Chunks overlap by (longest needle - 1) so a match straddling two
        // chunks is still seen.
        let overlap = needles.iter().map(|n| n.len()).max().unwrap_or(1) - 1;
        let mut pos = from;
        while pos < to {
            let want  = ((to - pos) as usize).min(SEARCH_CHUNK) + overlap;
            let chunk = self.bytes(pos, want)?;
            let best  = needles
                .iter()
                .enumerate()
                .filter_map(|(i, needle)| find(chunk, needle).map(|p| (p, i)))
                .min();
            if let Some((p, i)) = best {
                let at = pos + p as u64;
                return Ok((at < to).then_some((at, i)));
            }
            if chunk.len() < want || chunk.len() <= overlap {
                break; // end of source
            }
            pos += (chunk.len() - overlap) as u64;
        }
        Ok(None)
    }

    fn fill(&mut self, offset: u64, n: usize) -> io::Result<()> {
        let block     = self.src.block_size();
        let aligned   = offset - offset % block;
        let need      = (offset - aligned) as usize + n;
        // Never ask a device for blocks past its end.
        let remaining = (self.size - aligned).next_multiple_of(block) as usize;
        let want      = need.max(WINDOW_SIZE).next_multiple_of(block as usize).min(remaining);
        self.buf.resize(want, 0);
        let got = match self.src.read_at(aligned, &mut self.buf) {
            Ok(got) => got,
            Err(e)  => self.read_around_bad_blocks(aligned, block as usize, e)?,
        };
        self.buf.truncate(got);
        self.start = aligned;
        Ok(())
    }

    /// Failing drives have blocks that can't be read. Read block by block
    /// and leave those zeroed, so one bad spot doesn't end the whole scan.
    /// Only if nothing at all is readable (drive unplugged) is it an error.
    fn read_around_bad_blocks(&mut self, offset: u64, block: usize, first_error: io::Error) -> io::Result<usize> {
        let mut readable = false;
        for (i, chunk) in self.buf.chunks_mut(block).enumerate() {
            match self.src.read_at(offset + (i * block) as u64, chunk) {
                Ok(n) => {
                    readable = true;
                    chunk[n..].fill(0);
                }
                Err(_) => chunk.fill(0),
            }
        }
        if readable { Ok(self.buf.len()) } else { Err(first_error) }
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_across_refills_and_stops_at_end() {
        let mut data: Vec<u8> = (0..WINDOW_SIZE * 2 + 100).map(|i| i as u8).collect();
        let total = data.len() as u64;
        let mut w = Window::new(&mut data);

        let edge = WINDOW_SIZE as u64 - 2;
        assert_eq!(w.bytes(edge, 4).unwrap(), &[edge as u8, (edge + 1) as u8, (edge + 2) as u8, (edge + 3) as u8]);
        assert_eq!(w.bytes(total - 2, 10).unwrap().len(), 2);
        assert!(w.bytes(total, 10).unwrap().is_empty());
        assert_eq!(w.array::<4>(total - 3).unwrap(), None);
    }

    /// Fails any read that touches `bad`.
    struct Flaky { data: Vec<u8>, bad: std::ops::Range<u64> }

    impl BlockSource for Flaky {
        fn size(&self) -> u64 { self.data.len() as u64 }
        fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> io::Result<usize> {
            if offset < self.bad.end && offset + buf.len() as u64 > self.bad.start {
                return Err(io::Error::other("bad sector"));
            }
            self.data.read_at(offset, buf)
        }
    }

    #[test]
    fn bad_blocks_read_as_zeros_and_the_rest_survives() {
        let mut src = Flaky { data: vec![7; 4096], bad: 1024..1536 };
        let mut w = Window::new(&mut src);
        assert_eq!(w.bytes(1000, 4).unwrap(), &[7, 7, 7, 7]);
        assert_eq!(w.bytes(1024, 4).unwrap(), &[0, 0, 0, 0]);
        assert_eq!(w.bytes(1536, 4).unwrap(), &[7, 7, 7, 7]);
    }

    #[test]
    fn unreadable_source_is_an_error() {
        let mut src = Flaky { data: vec![7; 4096], bad: 0..4096 };
        assert!(Window::new(&mut src).bytes(0, 4).is_err());
    }

    #[test]
    fn finds_needles_across_chunk_boundaries() {
        let mut data = vec![0u8; SEARCH_CHUNK * 3];
        let at = SEARCH_CHUNK - 2;
        data[at..at + 5].copy_from_slice(b"%%EOF");
        let total = data.len() as u64;
        let mut w = Window::new(&mut data);

        assert_eq!(w.find_any(0, total, &[b"nope!", b"%%EOF"]).unwrap(), Some((at as u64, 1)));
        assert_eq!(w.find_any(0, at as u64, &[b"%%EOF"]).unwrap(), None);
    }
}
