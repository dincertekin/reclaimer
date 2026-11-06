//! One module per file format. Each knows two things:
//! - `matches`: does a file of this type start here? (cheap, header bytes only)
//! - `carve`: where does it end? Found by walking the format's own structure,
//!   never by guessing — that is what keeps two neighbouring files from being
//!   glued together.
//!
//! To add a format: write a module with those two functions and add one line
//! to `FORMATS`.

use std::io;
use super::window::Window;

mod gif;
mod isobmff;
mod jpeg;
mod pdf;
mod png;
mod sqlite;
mod zip;

/// What a carved file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub name: &'static str,
    pub ext:  &'static str,
}

/// Result of a successful carve.
#[derive(Debug, PartialEq, Eq)]
pub struct Carved {
    pub len:      u64,
    /// False when the end of the file was never reached (truncated or
    /// fragmented). The bytes are still worth saving — a partial photo
    /// often opens fine.
    pub complete: bool,
    pub kind:     Kind,
}

impl Carved {
    fn complete(len: u64, kind: Kind) -> Self {
        Self { len, complete: true, kind }
    }

    fn partial(len: u64, kind: Kind) -> Self {
        Self { len, complete: false, kind }
    }
}

/// `carve(window, start, limit)`: `start` is where the header was found,
/// `limit` the furthest offset the file may reach. `Ok(None)` means the
/// header was a false positive.
pub type CarveFn = fn(&mut Window, u64, u64) -> io::Result<Option<Carved>>;

pub struct Format {
    pub matches:  fn(&[u8]) -> bool,
    pub carve:    CarveFn,
    pub max_size: u64,
}

/// How many bytes `matches` gets to look at.
pub const HEADER_LEN: usize = 32;

const MB: u64 = 1 << 20;
const GB: u64 = 1 << 30;

pub const FORMATS: &[Format] = &[
    Format { matches: jpeg::matches,    carve: jpeg::carve,    max_size: 64 * MB },
    Format { matches: png::matches,     carve: png::carve,     max_size: 64 * MB },
    Format { matches: gif::matches,     carve: gif::carve,     max_size: 32 * MB },
    Format { matches: isobmff::matches, carve: isobmff::carve, max_size: 8 * GB },
    Format { matches: pdf::matches,     carve: pdf::carve,     max_size: 256 * MB },
    Format { matches: zip::matches,     carve: zip::carve,     max_size: GB },
    Format { matches: sqlite::matches,  carve: sqlite::carve,  max_size: 4 * GB },
];

fn be16(b: &[u8]) -> u16 { u16::from_be_bytes([b[0], b[1]]) }
fn be32(b: &[u8]) -> u32 { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) }
fn le16(b: &[u8]) -> u16 { u16::from_le_bytes([b[0], b[1]]) }
fn le32(b: &[u8]) -> u32 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

/// Runs a carver over an in-memory buffer starting at offset 0.
#[cfg(test)]
fn carve_vec(mut data: Vec<u8>, carve: CarveFn) -> Option<Carved> {
    let limit = data.len() as u64;
    let mut w = Window::new(&mut data);
    carve(&mut w, 0, limit).unwrap()
}

/// Test fixtures: one small, valid file per format.
#[cfg(test)]
pub mod samples {
    pub use super::gif::sample as gif;
    pub use super::isobmff::sample_mp4 as mp4;
    pub use super::jpeg::sample as jpeg;
    pub use super::pdf::sample as pdf;
    pub use super::png::sample as png;
    pub use super::sqlite::sample as sqlite;
    pub use super::zip::sample as zip;
}
