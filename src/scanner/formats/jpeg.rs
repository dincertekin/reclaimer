//! JPEG is a chain of segments: `FF xx` marker + 2-byte length + payload.
//! We hop from marker to marker. Hopping (rather than searching for `FF D9`)
//! skips over embedded EXIF thumbnails, which carry their own `FF D9`.

use std::io;
use super::{be16, Carved, Kind, Window};

const KIND: Kind = Kind { name: "JPEG", ext: "jpg" };

pub fn matches(h: &[u8]) -> bool {
    // SOI (FF D8) followed by any segment marker: covers JFIF (E0),
    // EXIF (E1, most camera and phone photos), raw DQT (DB) and others.
    h.len() >= 4 && h[..3] == [0xFF, 0xD8, 0xFF] && (0xC0..0xFF).contains(&h[3])
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    let mut pos = start + 2;
    let mut in_image = false; // set once we pass the first scan (SOS)

    while pos < limit {
        let Some([ff, marker]) = w.array(pos)? else { break };
        if ff != 0xFF {
            break;
        }
        match marker {
            0xFF => pos += 1, // fill byte
            0xD9 => return Ok(Some(Carved::complete(pos + 2 - start, KIND))),
            0xD8 => break,    // a new image starts: ours was cut short
            0x01 | 0xD0..=0xD7 => pos += 2, // markers without a payload
            0xC0..=0xFE => {
                let Some(len) = w.array::<2>(pos + 2)? else { break };
                let len = be16(&len) as u64;
                if len < 2 {
                    break;
                }
                pos += 2 + len;
                if marker == 0xDA {
                    in_image = true;
                    pos = skip_entropy_data(w, pos, limit)?;
                }
            }
            _ => break,
        }
    }

    // Structure broke off. Past the first scan we have real pixels — keep them.
    Ok(in_image.then(|| Carved::partial(pos - start, KIND)))
}

/// Compressed image data follows SOS with no length field. It ends at the
/// first `FF` that is not a stuffed `FF 00` or a restart marker `FF D0-D7`.
fn skip_entropy_data(w: &mut Window, mut pos: u64, limit: u64) -> io::Result<u64> {
    while pos < limit {
        let chunk = w.bytes(pos, 64 * 1024)?;
        if chunk.len() < 2 {
            return Ok(pos + chunk.len() as u64);
        }
        let mut i = 0;
        while i + 1 < chunk.len() {
            if chunk[i] != 0xFF {
                i += 1;
                continue;
            }
            match chunk[i + 1] {
                0x00 | 0xD0..=0xD7 => i += 2,
                0xFF => i += 1,
                _ => return Ok(pos + i as u64),
            }
        }
        pos += i as u64;
    }
    Ok(pos)
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8];
    j.extend([0xFF, 0xE0, 0x00, 0x10]);
    j.extend(b"JFIF\0");
    j.extend([1, 1, 0, 0, 1, 0, 1, 0, 0]);
    // EXIF segment carrying a thumbnail with its own SOI/EOI
    j.extend([0xFF, 0xE1, 0x00, 0x0A]);
    j.extend(b"Exif");
    j.extend([0xFF, 0xD8, 0xFF, 0xD9]);
    // Scan header, then entropy data with a stuffed byte and a restart marker
    j.extend([0xFF, 0xDA, 0x00, 0x04, 0x01, 0x00]);
    j.extend([0x12, 0xFF, 0x00, 0x34, 0xFF, 0xD0, 0x56]);
    j.extend([0xFF, 0xD9]);
    j
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn ends_at_real_eoi_not_thumbnail_or_later_ff_d9() {
        let jpg = sample();
        let mut data = jpg.clone();
        data.extend([0u8; 100]);
        data.extend([0xFF, 0xD9]);
        assert!(matches(&data));
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(jpg.len() as u64, KIND)));
    }

    #[test]
    fn truncated_file_is_partial() {
        let mut jpg = sample();
        jpg.truncate(jpg.len() - 2);
        let carved = carve_vec(jpg, carve).unwrap();
        assert!(!carved.complete);
    }

    #[test]
    fn garbage_after_header_is_rejected() {
        assert_eq!(carve_vec(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x00, 0, 0], carve), None);
    }

    #[test]
    fn real_photo_does_not_swallow_the_next_one() {
        // test.img holds two real photos; put them back to back, no gap.
        let image  = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/test.img")).unwrap();
        let first  = image[1024..1024 + 94_978].to_vec();
        let second = image[102_400..102_400 + 42_355].to_vec();
        let carved = carve_vec([first.clone(), second].concat(), carve).unwrap();
        assert_eq!(carved, Carved::complete(first.len() as u64, KIND));
    }
}
