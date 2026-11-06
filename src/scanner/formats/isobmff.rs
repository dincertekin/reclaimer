//! ISO base media files — MP4, MOV, HEIC, AVIF, 3GP — are a flat list of
//! "boxes": 4-byte size + 4-byte type. We hop box to box; the file ends where
//! the next thing is not a box. The brand in the first box (`ftyp`) says
//! which flavour it is.

use std::io;
use super::{be32, Carved, Kind, Window};

const MP4:  Kind = Kind { name: "MP4",  ext: "mp4" };
const MOV:  Kind = Kind { name: "MOV",  ext: "mov" };
const HEIC: Kind = Kind { name: "HEIC", ext: "heic" };
const AVIF: Kind = Kind { name: "AVIF", ext: "avif" };
const M4A:  Kind = Kind { name: "M4A",  ext: "m4a" };
const GP3:  Kind = Kind { name: "3GP",  ext: "3gp" };

/// Box types that may appear at the top level of a file.
const TOP_LEVEL: &[&[u8; 4]] = &[
    b"moov", b"mdat", b"free", b"skip", b"wide", b"meta", b"uuid", b"moof",
    b"mfra", b"styp", b"sidx", b"ssix", b"prft", b"pdin", b"emsg", b"pnot",
];

pub fn matches(h: &[u8]) -> bool {
    h.len() >= 12
        && &h[4..8] == b"ftyp"
        && (8..=1024).contains(&be32(&h[..4]))
        && h[8..12].iter().all(|c| c.is_ascii_alphanumeric() || *c == b' ')
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    let Some(head) = w.array::<12>(start)? else { return Ok(None) };
    let kind = kind_for_brand(&head[8..12]);
    let mut pos = start;
    let (mut has_index, mut has_media) = (false, false);

    while pos < limit {
        let Some(b) = w.array::<8>(pos)? else { break };
        let box_type = &b[4..8];
        // A second ftyp means the next file has begun.
        if pos > start && (box_type == b"ftyp" || !TOP_LEVEL.iter().any(|t| t[..] == *box_type)) {
            break;
        }
        let size = match be32(&b[..4]) {
            0 => {
                // "extends to end of file" — we cannot know where that is.
                pos = limit;
                has_media = false;
                break;
            }
            1 => match w.array::<8>(pos + 8)? {
                Some(large) => u64::from_be_bytes(large),
                None => break,
            },
            n => n as u64,
        };
        if size < 8 {
            break;
        }
        match box_type {
            b"moov" | b"meta" => has_index = true,
            b"mdat" => has_media = true,
            _ => {}
        }
        pos = pos.saturating_add(size);
    }

    if !has_index && !has_media {
        return Ok(None); // just an ftyp box: nothing to recover
    }
    let len = pos.min(limit) - start;
    let complete = has_index && has_media && pos <= limit;
    Ok(Some(Carved { len, complete, kind }))
}

fn kind_for_brand(brand: &[u8]) -> Kind {
    match brand {
        b"qt  " => MOV,
        b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => HEIC,
        b"avif" | b"avis" => AVIF,
        b"M4A " => M4A,
        b"3gp4" | b"3gp5" | b"3gp6" | b"3g2a" => GP3,
        _ => MP4,
    }
}

#[cfg(test)]
fn bx(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    [&((payload.len() + 8) as u32).to_be_bytes()[..], kind, payload].concat()
}

#[cfg(test)]
pub fn sample_mp4() -> Vec<u8> {
    [bx(b"ftyp", b"isom\0\0\0\0isom"), bx(b"moov", &[0; 8]), bx(b"mdat", &[7; 32])].concat()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn mp4_ends_after_last_box() {
        let mp4 = sample_mp4();
        let data = [mp4.clone(), vec![0; 64]].concat();
        assert!(matches(&data));
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(mp4.len() as u64, MP4)));
    }

    #[test]
    fn stops_at_next_file() {
        let mp4 = sample_mp4();
        let carved = carve_vec([mp4.clone(), mp4.clone()].concat(), carve).unwrap();
        assert_eq!(carved.len, mp4.len() as u64);
    }

    #[test]
    fn heic_brand_and_64bit_box_size() {
        let mut mdat = 1u32.to_be_bytes().to_vec();
        mdat.extend(b"mdat");
        mdat.extend(20u64.to_be_bytes());
        mdat.extend([9; 4]);
        let heic = [bx(b"ftyp", b"heic\0\0\0\0mif1heic"), bx(b"meta", &[0; 12]), mdat].concat();
        let carved = carve_vec([heic.clone(), vec![0; 32]].concat(), carve).unwrap();
        assert_eq!(carved, Carved::complete(heic.len() as u64, HEIC));
    }

    #[test]
    fn mdat_running_past_the_end_is_partial() {
        let mut mp4 = [bx(b"ftyp", b"isom\0\0\0\0isom"), bx(b"moov", &[0; 8])].concat();
        mp4.extend(1000u32.to_be_bytes());
        mp4.extend(b"mdat");
        mp4.extend([0; 16]);
        let carved = carve_vec(mp4.clone(), carve).unwrap();
        assert!(!carved.complete);
        assert_eq!(carved.len, mp4.len() as u64);
    }
}
