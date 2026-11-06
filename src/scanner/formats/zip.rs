//! ZIP — and everything built on it: DOCX, XLSX, PPTX. A ZIP ends with an
//! "end of central directory" record (`PK 05 06`) that points back at the
//! directory. We accept the first record whose pointer lands on our start,
//! which skips the records of ZIPs nested inside this one.

use std::io;
use super::{contains, le16, le32, Carved, Kind, Window};

const ZIP:  Kind = Kind { name: "ZIP",  ext: "zip" };
const DOCX: Kind = Kind { name: "DOCX", ext: "docx" };
const XLSX: Kind = Kind { name: "XLSX", ext: "xlsx" };
const PPTX: Kind = Kind { name: "PPTX", ext: "pptx" };

const LOCAL_HEADER: &[u8] = b"PK\x03\x04";
const END_RECORD:   &[u8] = b"PK\x05\x06";

pub fn matches(h: &[u8]) -> bool {
    h.starts_with(LOCAL_HEADER)
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    let mut pos = start + LOCAL_HEADER.len() as u64;
    while let Some((at, _)) = w.find_any(pos, limit, &[END_RECORD])? {
        let Some(rec) = w.array::<22>(at)? else { break };
        let dir_size    = le32(&rec[12..16]) as u64;
        let dir_offset  = le32(&rec[16..20]) as u64;
        let comment_len = le16(&rec[20..22]) as u64;
        if start + dir_offset + dir_size == at {
            let kind = classify(w, start + dir_offset, dir_size)?;
            return Ok(Some(Carved::complete(at + 22 + comment_len - start, kind)));
        }
        pos = at + 1;
    }
    Ok(None)
}

/// Office files are ZIPs with telltale folder names in the directory.
fn classify(w: &mut Window, dir_start: u64, dir_size: u64) -> io::Result<Kind> {
    let names = w.bytes(dir_start, dir_size.min(1 << 20) as usize)?;
    Ok(if contains(names, b"word/") {
        DOCX
    } else if contains(names, b"xl/") {
        XLSX
    } else if contains(names, b"ppt/") {
        PPTX
    } else {
        ZIP
    })
}

#[cfg(test)]
fn build(name: &[u8], data: &[u8]) -> Vec<u8> {
    let name_len = (name.len() as u16).to_le_bytes();
    let mut z = LOCAL_HEADER.to_vec();
    z.extend([0; 22]);
    z.extend(name_len);
    z.extend([0; 2]);
    z.extend(name);
    z.extend(data);

    let dir_offset = z.len() as u32;
    let mut dir = b"PK\x01\x02".to_vec();
    dir.extend([0; 24]);
    dir.extend(name_len);
    dir.extend([0; 16]);
    dir.extend(name);
    z.extend(&dir);

    z.extend(END_RECORD);
    z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
    z.extend((dir.len() as u32).to_le_bytes());
    z.extend(dir_offset.to_le_bytes());
    z.extend([0, 0]);
    z
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    build(b"hello.txt", b"hello")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn ends_after_end_record() {
        let zip = sample();
        let data = [zip.clone(), vec![0; 64]].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(zip.len() as u64, ZIP)));
    }

    #[test]
    fn recognises_docx() {
        let carved = carve_vec(build(b"word/document.xml", b"<w/>"), carve).unwrap();
        assert_eq!(carved.kind, DOCX);
    }

    #[test]
    fn skips_end_record_of_nested_zip() {
        let outer = build(b"inner.zip", &sample());
        assert_eq!(carve_vec(outer.clone(), carve).unwrap().len, outer.len() as u64);
    }
}
