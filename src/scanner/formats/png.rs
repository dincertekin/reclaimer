//! PNG: 8-byte signature, then chunks of length + type + data + CRC,
//! ending with the IEND chunk.

use std::io;
use super::{be32, Carved, Kind, Window};

const KIND: Kind = Kind { name: "PNG", ext: "png" };
const SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

pub fn matches(h: &[u8]) -> bool {
    h.starts_with(&SIGNATURE)
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    let mut pos = start + SIGNATURE.len() as u64;
    let mut chunks = 0;

    while pos < limit {
        let Some(head) = w.array::<8>(pos)? else { break };
        let len  = be32(&head[..4]) as u64;
        let kind = &head[4..8];
        if !kind.iter().all(u8::is_ascii_alphabetic) || len > 0x7FFF_FFFF {
            break;
        }
        if chunks == 0 && kind != b"IHDR" {
            return Ok(None); // the spec requires IHDR first
        }
        chunks += 1;
        pos += 12 + len;
        if kind == b"IEND" {
            return Ok(Some(Carved::complete(pos - start, KIND)));
        }
    }
    Ok((chunks > 0).then(|| Carved::partial(pos - start, KIND)))
}

#[cfg(test)]
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    [&(data.len() as u32).to_be_bytes()[..], kind, data, &[0; 4]].concat()
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    [SIGNATURE.to_vec(), chunk(b"IHDR", &[0; 13]), chunk(b"IDAT", &[1, 2, 3]), chunk(b"IEND", &[])].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn ends_after_iend() {
        let png = sample();
        let data = [png.clone(), vec![0xAA; 64]].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(png.len() as u64, KIND)));
    }

    #[test]
    fn rejects_missing_ihdr() {
        let data = [SIGNATURE.to_vec(), chunk(b"IDAT", &[1])].concat();
        assert_eq!(carve_vec(data, carve), None);
    }
}
