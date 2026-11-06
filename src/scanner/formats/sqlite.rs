//! SQLite databases — messages, contacts, browser history — state their own
//! size in the header: page size × page count.

use std::io;
use super::{be16, be32, Carved, Kind, Window};

const KIND: Kind = Kind { name: "SQLite", ext: "sqlite" };
const MAGIC: &[u8] = b"SQLite format 3\0";

pub fn matches(h: &[u8]) -> bool {
    h.starts_with(MAGIC)
}

pub fn carve(w: &mut Window, start: u64, _limit: u64) -> io::Result<Option<Carved>> {
    let Some(h) = w.array::<100>(start)? else { return Ok(None) };
    let page_size = match be16(&h[16..18]) {
        1 => 65536, // 65536 does not fit in 16 bits, so it is stored as 1
        n => n as u64,
    };
    let page_count = be32(&h[28..32]) as u64;
    // The page count is only trustworthy when these two counters agree.
    let count_valid = be32(&h[24..28]) == be32(&h[92..96]);

    if !page_size.is_power_of_two() || !(512..=65536).contains(&page_size) || page_count == 0 || !count_valid {
        return Ok(None);
    }
    Ok(Some(Carved::complete(page_size * page_count, KIND)))
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    let mut db = vec![0u8; 3 * 512];
    db[..16].copy_from_slice(MAGIC);
    db[16..18].copy_from_slice(&512u16.to_be_bytes());
    db[24..28].copy_from_slice(&7u32.to_be_bytes());
    db[28..32].copy_from_slice(&3u32.to_be_bytes());
    db[92..96].copy_from_slice(&7u32.to_be_bytes());
    db
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn size_from_header() {
        let db = sample();
        let data = [db.clone(), vec![0xAA; 512]].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(db.len() as u64, KIND)));
    }

    #[test]
    fn stale_page_count_is_rejected() {
        let mut db = sample();
        db[92..96].copy_from_slice(&6u32.to_be_bytes());
        assert_eq!(carve_vec(db, carve), None);
    }
}
