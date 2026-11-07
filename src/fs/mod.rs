//! Filesystems. When a file is deleted, its directory record usually
//! survives, only marked as deleted — with the file's name, folder, size,
//! date and where its data lives. Reading those records recovers files with
//! their real names, fragmented ones included, which carving cannot do.
//!
//! `analyze` finds the volumes on a source (via the partition table) and asks
//! each supported filesystem for its deleted files.

mod exfat;
mod fat;
mod ntfs;
mod partitions;

use std::io;
use crate::source::{BlockSource, Piece, Window};

/// A deleted file, as its filesystem remembers it.
#[derive(Clone, Debug, PartialEq)]
pub struct DeletedFile {
    pub name:     String,
    /// Folder it was in, e.g. "/DCIM/100CANON"; "/" for the top level.
    pub folder:   String,
    pub size:     u64,
    pub pieces:   Vec<Piece>,
    pub modified: Option<Modified>,
    /// False when some of its space has since been given to other files, so
    /// the contents may be partly overwritten.
    pub intact:   bool,
}

impl DeletedFile {
    /// Where the file's data starts on the source, if it is on disk.
    pub fn start(&self) -> Option<u64> {
        match self.pieces.first() {
            Some(Piece::Disk { offset, .. }) => Some(*offset),
            _ => None,
        }
    }
}

/// When a file was last changed. FAT records wall-clock time with no time
/// zone; exFAT (with its offset) and NTFS record an exact moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modified {
    Local(Timestamp),
    Utc(i64), // seconds since 1970
}

impl Modified {
    /// The date as the user's clock would show it.
    pub fn local(self) -> Timestamp {
        match self {
            Modified::Local(t) => t,
            Modified::Utc(secs) => Timestamp::from_unix(secs + local_utc_offset()),
        }
    }
}

// Field order matters: the derived ordering compares year first, then month...
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp {
    pub year:   u16,
    pub month:  u8,
    pub day:    u8,
    pub hour:   u8,
    pub minute: u8,
}

impl Timestamp {
    /// FAT and exFAT pack dates as days/months/years-since-1980 bit fields.
    fn from_dos(date: u16, time: u16) -> Option<Self> {
        let t = Timestamp {
            year:   1980 + (date >> 9),
            month:  ((date >> 5) & 0x0F) as u8,
            day:    (date & 0x1F) as u8,
            hour:   (time >> 11) as u8,
            minute: ((time >> 5) & 0x3F) as u8,
        };
        ((1..=12).contains(&t.month) && (1..=31).contains(&t.day) && t.hour < 24 && t.minute < 60).then_some(t)
    }

    /// NTFS counts 100-nanosecond ticks since 1601 (UTC).
    fn filetime_to_unix(ticks: u64) -> Option<i64> {
        Some((ticks / 10_000_000).checked_sub(11_644_473_600)? as i64)
    }

    fn from_unix(unix: i64) -> Self {
        let (days, secs) = (unix.div_euclid(86_400), unix.rem_euclid(86_400));
        // Days since 1970 → civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u8;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u8;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as u16;
        Timestamp { year, month, day, hour: (secs / 3600) as u8, minute: (secs / 60 % 60) as u8 }
    }

    /// Seconds since 1970, reading this as a UTC time.
    pub(crate) fn to_unix(self) -> i64 {
        // Civil date → days since 1970 (Howard Hinnant's algorithm).
        let (m, d) = (self.month as i64, self.day as i64);
        let y = self.year as i64 - i64::from(m <= 2);
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        let days = era * 146_097 + doe - 719_468;
        days * 86_400 + self.hour as i64 * 3600 + self.minute as i64 * 60
    }
}

/// This computer's current offset from UTC, in seconds.
pub fn local_utc_offset() -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: localtime_r only writes into the `tm` we pass it.
        unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&now, &mut tm).is_null() { 0 } else { tm.tm_gmtoff as i64 }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
        // SAFETY: fills the struct we pass; all-zero is a valid starting value.
        let mut tz: TIME_ZONE_INFORMATION = unsafe { std::mem::zeroed() };
        let mode = unsafe { GetTimeZoneInformation(&mut tz) };
        let daylight = if mode == 2 { tz.DaylightBias } else { tz.StandardBias };
        -((tz.Bias + daylight) as i64) * 60 // Bias is minutes to add to local time to get UTC
    }
    #[cfg(not(any(unix, windows)))]
    0
}

/// Which parts of a volume belong to existing files. Carving skips them:
/// a photo found there is one the user still has, not one they lost.
#[derive(Clone, Debug)]
pub struct Allocation {
    volume:  std::ops::Range<u64>,
    data:    u64,      // where cluster numbering starts (metadata before it)
    cluster: u64,
    used:    Vec<u64>, // one bit per cluster
}

impl Allocation {
    fn new(volume: std::ops::Range<u64>, data: u64, cluster: u64, count: usize) -> Self {
        Self { volume, data, cluster, used: vec![0; count.div_ceil(64)] }
    }

    fn mark(&mut self, index: usize) {
        if let Some(word) = self.used.get_mut(index / 64) {
            *word |= 1 << (index % 64);
        }
    }

    fn is_used(&self, index: usize) -> bool {
        self.used.get(index / 64).is_some_and(|w| w & (1 << (index % 64)) != 0)
    }

    /// Is this byte of the source part of a file that still exists?
    pub fn contains(&self, offset: u64) -> bool {
        if !self.volume.contains(&offset) {
            return false;
        }
        if offset < self.data {
            return true; // the filesystem's own tables
        }
        self.is_used(((offset - self.data) / self.cluster) as usize)
    }
}

pub struct Volume {
    #[allow(dead_code)] // read by tests; handy when debugging a drive
    pub filesystem: &'static str,
    pub files:      Vec<DeletedFile>,
    pub allocation: Option<Allocation>,
}

/// Every volume on the source with a filesystem we can read. A damaged
/// volume is skipped rather than failing the whole scan.
pub fn analyze(src: &mut dyn BlockSource) -> io::Result<Vec<Volume>> {
    let mut w = Window::new(src);
    let mut volumes = Vec::new();
    for (offset, len) in partitions::find(&mut w)? {
        let Some(boot) = w.array::<512>(offset)? else { continue };
        let result = if fat::detect(&boot) {
            fat::read(&mut w, offset, len)
        } else if exfat::detect(&boot) {
            exfat::read(&mut w, offset, len)
        } else if ntfs::detect(&boot) {
            ntfs::read(&mut w, offset, len)
        } else {
            continue;
        };
        if let Ok(volume) = result {
            volumes.push(volume);
        }
    }
    Ok(volumes)
}

/// Does this sector start a filesystem we know?
fn is_boot_sector(sector: &[u8; 512]) -> bool {
    fat::detect(sector) || exfat::detect(sector) || ntfs::detect(sector)
}

fn le16(b: &[u8], at: usize) -> u16 { u16::from_le_bytes([b[at], b[at + 1]]) }
fn le32(b: &[u8], at: usize) -> u32 { u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) }
fn le64(b: &[u8], at: usize) -> u64 { u64::from_le_bytes(b[at..at + 8].try_into().unwrap()) }

/// UTF-16LE text, ending at the first NUL (or 0xFFFF padding).
fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0 && u != 0xFFFF)
        .collect();
    String::from_utf16_lossy(&units)
}

fn join(folder: &str, name: &str) -> String {
    if folder == "/" { format!("/{name}") } else { format!("{folder}/{name}") }
}

/// macOS metadata twins ("._photo.jpg") are not files anyone wants back.
fn is_noise(name: &str) -> bool {
    name.starts_with("._") || name == ".DS_Store"
}

#[cfg(test)]
mod tests;
