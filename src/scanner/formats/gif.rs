//! GIF: header, optional colour table, then a sequence of blocks — images
//! (0x2C) and extensions (0x21) — closed by the trailer byte 0x3B.

use std::io;
use super::{Carved, Kind, Window};

const KIND: Kind = Kind { name: "GIF", ext: "gif" };

pub fn matches(h: &[u8]) -> bool {
    h.starts_with(b"GIF87a") || h.starts_with(b"GIF89a")
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    let Some(screen) = w.array::<7>(start + 6)? else { return Ok(None) };
    let mut pos = start + 13 + color_table_len(screen[4]);
    let mut images = 0;

    while pos < limit {
        let Some([block]) = w.array(pos)? else { break };
        match block {
            0x3B => return Ok(Some(Carved::complete(pos + 1 - start, KIND))),
            0x21 => pos = skip_sub_blocks(w, pos + 2, limit)?,
            0x2C => {
                let Some(desc) = w.array::<10>(pos)? else { break };
                // descriptor + local colour table + LZW code size byte
                pos += 10 + color_table_len(desc[9]) + 1;
                pos = skip_sub_blocks(w, pos, limit)?;
                images += 1;
            }
            _ => break,
        }
    }
    Ok((images > 0).then(|| Carved::partial(pos - start, KIND)))
}

fn color_table_len(flags: u8) -> u64 {
    if flags & 0x80 != 0 { 3 << ((flags & 0x07) + 1) } else { 0 }
}

/// Data is stored as length-prefixed sub-blocks ending with a zero length.
fn skip_sub_blocks(w: &mut Window, mut pos: u64, limit: u64) -> io::Result<u64> {
    while pos < limit {
        let Some([len]) = w.array(pos)? else { break };
        pos += 1;
        if len == 0 {
            break;
        }
        pos += len as u64;
    }
    Ok(pos)
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    let mut g = b"GIF89a".to_vec();
    g.extend([1, 0, 1, 0, 0x80, 0, 0]);            // 1x1, 2-colour global table
    g.extend([0; 6]);
    g.extend([0x21, 0xF9, 4, 0, 0, 0, 0, 0]);       // graphic control extension
    g.extend([0x2C, 0, 0, 0, 0, 1, 0, 1, 0, 0x00]); // image descriptor
    g.extend([2, 2, 0x44, 0x01, 0]);                // LZW data
    g.push(0x3B);
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn ends_at_trailer() {
        let gif = sample();
        let data = [gif.clone(), vec![0x3B; 16]].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(gif.len() as u64, KIND)));
    }
}
