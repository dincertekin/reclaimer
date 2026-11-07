//! A small, read-only SQLite reader — just enough to read whole tables.
//!
//! Phones keep their indexes in SQLite: an iPhone backup's file list
//! (Manifest.db), the Photos library (Photos.sqlite), Messages, WhatsApp.
//! A database file is a sequence of fixed-size pages; each table is a B-tree
//! of pages whose leaves hold rows, and a row too big for its page continues
//! on a chain of overflow pages. Format: https://www.sqlite.org/fileformat.html

use std::io;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self { Value::Int(n) => Some(*n), _ => None }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self { Value::Text(s) => Some(s), _ => None }
    }

    pub fn as_blob(&self) -> Option<&[u8]> {
        match self { Value::Blob(b) => Some(b), _ => None }
    }
}

/// A table's rows, with its column names to look values up by.
pub struct Table {
    pub columns: Vec<String>,
    pub rows:    Vec<Vec<Value>>,
}

impl Table {
    pub fn column(&self, name: &str) -> Option<usize> {
        self.columns.iter().position(|c| c.eq_ignore_ascii_case(name))
    }
}

pub struct Database {
    bytes:     Vec<u8>,
    page_size: usize,
    usable:    usize, // page size minus space reserved at the end of each page
}

const MAX_DEPTH: usize = 64;

fn bad(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("damaged database: {what}"))
}

impl Database {
    pub fn open(bytes: Vec<u8>) -> io::Result<Self> {
        if bytes.len() < 100 || !bytes.starts_with(b"SQLite format 3\0") {
            return Err(bad("not an SQLite file"));
        }
        let page_size = match u16::from_be_bytes([bytes[16], bytes[17]]) {
            1 => 65_536,
            n => n as usize,
        };
        if !page_size.is_power_of_two() || page_size < 512 {
            return Err(bad("page size"));
        }
        let usable = page_size - bytes[20] as usize;
        Ok(Self { bytes, page_size, usable })
    }

    /// Every row of the named table. Older rows written before an
    /// `ALTER TABLE ... ADD COLUMN` are padded with `Null`.
    pub fn table(&self, name: &str) -> io::Result<Table> {
        // Page 1 holds the schema: rows of (type, name, tbl_name, rootpage, sql).
        let mut schema = Vec::new();
        self.walk(1, 0, &mut |_, row| schema.push(row))?;
        let entry = schema
            .iter()
            .find(|r| r.first().and_then(Value::as_text) == Some("table") && r.get(1).and_then(Value::as_text).is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("no table {name}")))?;
        let root = entry.get(3).and_then(Value::as_int).ok_or_else(|| bad("root page"))? as u32;
        let columns = entry.get(4).and_then(Value::as_text).map(column_names).unwrap_or_default();
        let rowid_column = entry.get(4).and_then(Value::as_text).and_then(rowid_alias);

        let mut rows = Vec::new();
        self.walk(root, 0, &mut |rowid, mut row| {
            row.resize(columns.len().max(row.len()), Value::Null);
            // An INTEGER PRIMARY KEY column is stored as NULL; its value is the rowid.
            if let Some(i) = rowid_column
                && row.get(i) == Some(&Value::Null)
            {
                row[i] = Value::Int(rowid);
            }
            rows.push(row);
        })?;
        Ok(Table { columns, rows })
    }

    fn page(&self, number: u32) -> io::Result<&[u8]> {
        let start = (number as usize).checked_sub(1).ok_or_else(|| bad("page 0"))? * self.page_size;
        self.bytes.get(start..start + self.page_size).ok_or_else(|| bad("page past end of file"))
    }

    /// Visits every row of the table B-tree rooted at `number`.
    fn walk(&self, number: u32, depth: usize, visit: &mut dyn FnMut(i64, Vec<Value>)) -> io::Result<()> {
        if depth > MAX_DEPTH {
            return Err(bad("tree too deep"));
        }
        let page = self.page(number)?;
        let header = if number == 1 { 100 } else { 0 }; // page 1 starts with the file header
        let kind = page[header];
        let cells = u16::from_be_bytes([page[header + 3], page[header + 4]]) as usize;
        let interior = kind == 0x05;
        if kind != 0x05 && kind != 0x0D {
            return Err(bad("not a table page"));
        }
        let pointers = header + if interior { 12 } else { 8 };

        for i in 0..cells {
            let at = pointers + 2 * i;
            let cell = u16::from_be_bytes([page[at], page[at + 1]]) as usize;
            if interior {
                // Interior cell: child page, then a rowid key we don't need.
                let child = u32::from_be_bytes(page[cell..cell + 4].try_into().unwrap());
                self.walk(child, depth + 1, visit)?;
            } else {
                let (payload_len, n1) = varint(&page[cell..]);
                let (rowid, n2) = varint(&page[cell + n1..]);
                let payload = self.payload(page, cell + n1 + n2, payload_len as usize)?;
                visit(rowid as i64, record(&payload)?);
            }
        }
        if interior {
            let right = u32::from_be_bytes(page[header + 8..header + 12].try_into().unwrap());
            self.walk(right, depth + 1, visit)?;
        }
        Ok(())
    }

    /// A row's bytes: the part stored in the page, plus overflow pages.
    fn payload(&self, page: &[u8], at: usize, len: usize) -> io::Result<Vec<u8>> {
        let u = self.usable;
        let max_local = u - 35;
        let local = if len <= max_local {
            len
        } else {
            let min_local = (u - 12) * 32 / 255 - 23;
            let k = min_local + (len - min_local) % (u - 4);
            if k <= max_local { k } else { min_local }
        };
        let mut out = page.get(at..at + local).ok_or_else(|| bad("cell past end of page"))?.to_vec();
        if local < len {
            let mut next = u32::from_be_bytes(page[at + local..at + local + 4].try_into().unwrap());
            while out.len() < len {
                if next == 0 {
                    return Err(bad("overflow chain ends early"));
                }
                let overflow = self.page(next)?;
                next = u32::from_be_bytes(overflow[..4].try_into().unwrap());
                let take = (len - out.len()).min(u - 4);
                out.extend_from_slice(&overflow[4..4 + take]);
            }
        }
        Ok(out)
    }
}

/// SQLite's variable-length integer: up to 9 bytes, 7 bits each (8 in the last).
fn varint(b: &[u8]) -> (u64, usize) {
    let mut v = 0u64;
    for (i, &byte) in b.iter().take(9).enumerate() {
        if i == 8 {
            return ((v << 8) | byte as u64, 9);
        }
        v = (v << 7) | (byte & 0x7F) as u64;
        if byte & 0x80 == 0 {
            return (v, i + 1);
        }
    }
    (v, b.len().min(9))
}

/// A row: a header of type codes, then the values they describe.
fn record(payload: &[u8]) -> io::Result<Vec<Value>> {
    let (header_len, mut pos) = varint(payload);
    let header_len = header_len as usize;
    let mut body = header_len;
    let mut values = Vec::new();
    while pos < header_len {
        let (kind, n) = varint(&payload[pos..]);
        pos += n;
        let size = match kind {
            0 | 8 | 9 => 0,
            1 => 1, 2 => 2, 3 => 3, 4 => 4, 5 => 6, 6 | 7 => 8,
            k if k >= 12 => ((k - 12) / 2) as usize,
            _ => return Err(bad("reserved type code")),
        };
        let bytes = payload.get(body..body + size).ok_or_else(|| bad("value past end of row"))?;
        body += size;
        let int = || {
            let shift = 64 - 8 * size as u32;
            ((bytes.iter().fold(0u64, |v, &b| (v << 8) | b as u64) << shift) as i64) >> shift // sign-extend
        };
        values.push(match kind {
            0 => Value::Null,
            1..=6 => Value::Int(int()),
            7 => Value::Real(f64::from_be_bytes(bytes.try_into().unwrap())),
            8 => Value::Int(0),
            9 => Value::Int(1),
            k if k % 2 == 0 => Value::Blob(bytes.to_vec()),
            _ => Value::Text(String::from_utf8_lossy(bytes).into_owned()),
        });
    }
    Ok(values)
}

/// Column names from a CREATE TABLE statement.
fn column_names(sql: &str) -> Vec<String> {
    let (Some(open), Some(close)) = (sql.find('('), sql.rfind(')')) else { return Vec::new() };
    let mut columns = Vec::new();
    let (mut depth, mut start) = (0, open + 1);
    let body = &sql[..close];
    for (i, c) in body.char_indices().skip_while(|&(i, _)| i <= open) {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                columns.push(&body[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    columns.push(&body[start..]);
    const CONSTRAINTS: [&str; 5] = ["PRIMARY", "UNIQUE", "CHECK", "FOREIGN", "CONSTRAINT"];
    columns
        .iter()
        .filter_map(|def| def.split(|c: char| c.is_whitespace() || c == '(').find(|t| !t.is_empty()))
        .map(|name| name.trim_matches(['"', '`', '[', ']', '\'']).to_string())
        .filter(|name| !CONSTRAINTS.iter().any(|k| name.eq_ignore_ascii_case(k)))
        .collect()
}

/// Which column, if any, is an alias for the rowid (`x INTEGER PRIMARY KEY`).
fn rowid_alias(sql: &str) -> Option<usize> {
    let names = column_names(sql);
    let upper = sql.to_uppercase();
    names.iter().position(|name| {
        let pattern = format!("{} INTEGER PRIMARY KEY", name.to_uppercase());
        upper.contains(&pattern)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/sqlite-test.db")).unwrap();
        Database::open(bytes).unwrap()
    }

    #[test]
    fn reads_every_row_of_a_multi_level_table() {
        let table = test_db().table("items").unwrap();
        assert_eq!(table.columns, ["id", "name", "size", "ratio", "data", "note"]);
        assert_eq!(table.rows.len(), 2000);
        let (id, name, size, ratio) = (0, table.column("name").unwrap(), 2, 3);
        let row = &table.rows[41]; // rows come back in rowid order
        assert_eq!(row[id], Value::Int(42));
        assert_eq!(row[name], Value::Text("item 42".into()));
        assert_eq!(row[size], Value::Int(41_500));
        assert_eq!(row[ratio], Value::Real(10.5));
    }

    #[test]
    fn values_of_every_kind() {
        let table = test_db().table("items").unwrap();
        let row = &table.rows[6];
        assert_eq!(row[1], Value::Text("naïve ✓".into()));
        assert_eq!(row[2], Value::Null);
        assert_eq!(row[3], Value::Real(-1.5));
        assert_eq!(row[5], Value::Null, "added after this row was written");
        assert_eq!(table.rows[1999][5], Value::Text("later".into()));
    }

    #[test]
    fn follows_overflow_pages() {
        let table = test_db().table("items").unwrap();
        let expected: Vec<u8> = (0..20).flat_map(|_| 0..=255u8).collect();
        assert_eq!(table.rows[999][4], Value::Blob(expected));
    }

    #[test]
    fn varints_and_schema_parsing() {
        assert_eq!(varint(&[0x7F]), (127, 1));
        assert_eq!(varint(&[0x81, 0x00]), (128, 2));
        assert_eq!(varint(&[0xFF; 9]), (u64::MAX, 9));
        let sql = "CREATE TABLE Files (fileID TEXT PRIMARY KEY, domain TEXT, flags INTEGER, file BLOB, UNIQUE(fileID))";
        assert_eq!(column_names(sql), ["fileID", "domain", "flags", "file"]);
        assert_eq!(rowid_alias("CREATE TABLE t (z_pk INTEGER PRIMARY KEY, a)"), Some(0));
        assert_eq!(rowid_alias(sql), None);
    }

    #[test]
    fn rejects_other_files() {
        assert!(Database::open(b"not a database".repeat(10)).is_err());
        assert!(test_db().table("missing").is_err());
    }
}
