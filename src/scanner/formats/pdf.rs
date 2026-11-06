//! PDF ends with `%%EOF` — but edited PDFs append updates, each with its own
//! `%%EOF`, so we keep looking a little further after the first one.
//! Linearized ("fast web view") PDFs state their exact length up front, and
//! we use that when present.

use std::io;
use super::{contains, Carved, Kind, Window};

const KIND: Kind = Kind { name: "PDF", ext: "pdf" };
const HEADER: &[u8] = b"%PDF-";
const EOF_MARK: &[u8] = b"%%EOF";
/// How far past an `%%EOF` we look for an appended update.
const UPDATE_WINDOW: u64 = 8 << 20;

pub fn matches(h: &[u8]) -> bool {
    h.starts_with(HEADER)
}

pub fn carve(w: &mut Window, start: u64, limit: u64) -> io::Result<Option<Carved>> {
    if let Some(len) = linearized_len(w, start)?
        && start + len <= limit
        && contains(w.bytes(start + len - len.min(32), len.min(32) as usize)?, EOF_MARK)
    {
        return Ok(Some(Carved::complete(len, KIND)));
    }

    let mut end = None;
    let mut pos = start + HEADER.len() as u64;
    let mut search_to = limit;
    // Stop at the next PDF header: it belongs to another file.
    while let Some((at, 0)) = w.find_any(pos, search_to, &[EOF_MARK, HEADER])? {
        let after = at + EOF_MARK.len() as u64;
        end = Some(after);
        pos = after;
        search_to = limit.min(after + UPDATE_WINDOW);
    }

    let Some(end) = end else { return Ok(None) };
    let end = end + line_ending_len(w, end)?;
    Ok(Some(Carved::complete(end - start, KIND)))
}

/// Reads `/L <length>` from the linearization dictionary at the top of the file.
fn linearized_len(w: &mut Window, start: u64) -> io::Result<Option<u64>> {
    let head = w.bytes(start, 1024)?;
    if !contains(head, b"/Linearized") {
        return Ok(None);
    }
    let Some(key) = head.windows(3).position(|k| k[0] == b'/' && k[1] == b'L' && k[2].is_ascii_whitespace()) else {
        return Ok(None);
    };
    let digits: String = head[key + 2..]
        .iter()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .map(|&c| c as char)
        .collect();
    Ok(digits.parse().ok().filter(|&len| len > 0))
}

fn line_ending_len(w: &mut Window, at: u64) -> io::Result<u64> {
    let tail = w.bytes(at, 2)?;
    Ok(tail.iter().take_while(|c| matches!(c, b'\r' | b'\n')).count() as u64)
}

#[cfg(test)]
pub fn sample() -> Vec<u8> {
    b"%PDF-1.4\n1 0 obj <<>> endobj\ntrailer <<>>\n%%EOF\n2 0 obj <<>> endobj\n%%EOF\n".to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::formats::carve_vec;

    #[test]
    fn includes_appended_updates() {
        let pdf = sample();
        let data = [pdf.clone(), vec![0; 100]].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(pdf.len() as u64, KIND)));
    }

    #[test]
    fn stops_at_next_pdf() {
        let pdf = sample();
        let carved = carve_vec([pdf.clone(), pdf.clone()].concat(), carve).unwrap();
        assert_eq!(carved.len, pdf.len() as u64);
    }

    #[test]
    fn uses_linearized_length() {
        let body = |len: usize| format!("%PDF-1.5\n1 0 obj <</Linearized 1 /L {len:06} >> endobj\n%%EOF\nxref\n%%EOF\n");
        let pdf = body(body(0).len()).into_bytes();
        let data = [pdf.clone(), b"junk %%EOF\n".to_vec()].concat();
        assert_eq!(carve_vec(data, carve), Some(Carved::complete(pdf.len() as u64, KIND)));
    }

    #[test]
    fn no_eof_is_rejected() {
        assert_eq!(carve_vec(b"%PDF-1.4\n garbage".to_vec(), carve), None);
    }
}
