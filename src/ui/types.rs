use std::cmp::Ordering;
use std::path::{Path, PathBuf};
pub use crate::media::Category;
use crate::fs::Timestamp;
use crate::phone::iphone::Backup;
use crate::phone::Phone;
use crate::source::drives::Drive;

/// A file recovered and saved during a scan.
#[derive(Clone, Debug)]
pub struct FoundFile {
    pub kind:      String,       // "JPEG", "MP4", ...
    pub extension: String,       // lowercase, without the dot
    pub offset:    Option<u64>,  // where it was found on the source (not for backups)
    pub size:      u64,
    pub filename:  String,
    pub path:      PathBuf,      // where the recovered copy was saved
    pub category:  Category,
    /// False when the file may be damaged: truncated, fragmented, or partly
    /// overwritten since it was deleted.
    pub complete:  bool,
    /// Where it was before deletion, when the filesystem remembers.
    pub folder:    Option<String>,
    pub modified:  Option<Timestamp>,
    /// In the iPhone's Recently Deleted album when it was backed up.
    pub recently_deleted: bool,
}

/// Which files the results grid shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    All,
    Category(Category),
    RecentlyDeleted,
}

impl Filter {
    pub fn includes(self, file: &FoundFile) -> bool {
        match self {
            Filter::All => true,
            Filter::Category(c) => file.category == c,
            Filter::RecentlyDeleted => file.recently_deleted,
        }
    }
}

/// What the results are ordered by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Date,
    Name,
    Kind,
    Size,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Date, SortKey::Name, SortKey::Kind, SortKey::Size];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Date => "Date Modified",
            SortKey::Name => "Name",
            SortKey::Kind => "Kind",
            SortKey::Size => "Size",
        }
    }

    /// How each direction reads for this key: (ascending, descending).
    pub fn directions(self) -> (&'static str, &'static str) {
        match self {
            SortKey::Date => ("Oldest First", "Newest First"),
            SortKey::Name | SortKey::Kind => ("A to Z", "Z to A"),
            SortKey::Size => ("Smallest First", "Largest First"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sort {
    pub key:        SortKey,
    pub descending: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort::by(SortKey::Date)
    }
}

impl Sort {
    /// A key in its natural direction: newest and largest first, names A to Z.
    pub fn by(key: SortKey) -> Self {
        Sort { key, descending: matches!(key, SortKey::Date | SortKey::Size) }
    }

    pub fn compare(self, a: &FoundFile, b: &FoundFile) -> Ordering {
        let order = match self.key {
            // Files without a date (found by content) always go last.
            SortKey::Date => match (a.modified, b.modified) {
                (Some(x), Some(y)) => x.cmp(&y),
                (Some(_), None) => return Ordering::Less,
                (None, Some(_)) => return Ordering::Greater,
                (None, None) => Ordering::Equal,
            },
            SortKey::Name => natural_cmp(&a.filename, &b.filename),
            SortKey::Kind => a.kind.to_lowercase().cmp(&b.kind.to_lowercase()).then_with(|| natural_cmp(&a.filename, &b.filename)),
            SortKey::Size => a.size.cmp(&b.size),
        };
        if self.descending { order.reverse() } else { order }
    }

    /// For remembering the choice between launches: "date-desc".
    pub fn to_setting(self) -> String {
        let key = match self.key {
            SortKey::Date => "date",
            SortKey::Name => "name",
            SortKey::Kind => "kind",
            SortKey::Size => "size",
        };
        format!("{key}-{}", if self.descending { "desc" } else { "asc" })
    }

    pub fn from_setting(text: &str) -> Option<Self> {
        let (key, direction) = text.split_once('-')?;
        let key = match key {
            "date" => SortKey::Date,
            "name" => SortKey::Name,
            "kind" => SortKey::Kind,
            "size" => SortKey::Size,
            _ => return None,
        };
        Some(Sort { key, descending: direction == "desc" })
    }
}

/// Compares names the way Finder does: numbers by value, so "IMG_2" comes
/// before "IMG_10", and letters ignoring case.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(&c) = it.peek().filter(|c| c.is_ascii_digit()) {
                        digits.push(c);
                        it.next();
                    }
                    digits
                };
                let (x, y) = (take(&mut a), take(&mut b));
                let (xt, yt) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = xt.len().cmp(&yt.len()).then_with(|| xt.cmp(yt)).then_with(|| x.len().cmp(&y.len()));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

/// What we scan: an image file, a whole drive, an iPhone backup, or a
/// phone on a cable.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Image(PathBuf),
    Drive(Drive),
    Backup(Backup),
    Phone(Phone),
}

impl Source {
    pub fn name(&self) -> String {
        match self {
            Source::Image(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Source::Drive(drive) => drive.name.clone(),
            Source::Backup(backup) => backup.device.clone(),
            Source::Phone(phone) => phone.name.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Choose,
    Scanning,
    Done,
}

pub enum ScanMsg {
    /// The scan opened its source and created its output folder.
    Started { folder: PathBuf },
    Progress { done: u64, total: u64 },
    Found(FoundFile),
    /// What the scan is doing, in words.
    Stage(&'static str),
    Done,
    Error(String),
}

/// What to call a file's kind, from its extension: "JPEG", "MOV", "File".
pub fn kind_name(ext: &str) -> String {
    match ext {
        "" => "File".into(),
        "jpg" | "jpeg" => "JPEG".into(),
        "sqlite" | "sqlite3" | "db" => "SQLite".into(),
        other => other.to_uppercase(),
    }
}

/// The extension of a file name, lowercase: "IMG_1.JPG" → "jpg".
pub fn extension_of(name: &str) -> String {
    Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// A name safe to use as one path component on every OS.
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']).trim_start();
    if trimmed.is_empty() || trimmed == ".." { "_".into() } else { trimmed.into() }
}

/// "7 Nov 2026"
pub fn fmt_day(t: Timestamp) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.get((t.month as usize).wrapping_sub(1)).unwrap_or(&"");
    format!("{} {month} {}", t.day, t.year)
}

/// "7 Nov 2026 at 14:02"
pub fn fmt_date(t: Timestamp) -> String {
    format!("{} at {:02}:{:02}", fmt_day(t), t.hour, t.minute)
}

/// Default home for recovered files: ~/Documents/Reclaimer.
pub fn default_output_dir() -> PathBuf {
    std::env::home_dir()
        .map(|home| home.join("Documents").join("Reclaimer"))
        .unwrap_or_else(|| PathBuf::from("Recovered"))
}

// ── Formatting ─────────────────────────────────────────────────────────────

/// Sizes the way Finder shows them: decimal units.
pub fn fmt_size(bytes: u64) -> String {
    let b = bytes as f64;
    match bytes {
        0..1_000                 => format!("{bytes} bytes"),
        1_000..1_000_000         => format!("{:.0} KB", b / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b / 1e6),
        1_000_000_000..1_000_000_000_000 => format!("{:.1} GB", b / 1e9),
        _                        => format!("{:.1} TB", b / 1e12),
    }
}

/// 1284 → "1,284"
pub fn fmt_count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", fmt_count(n), if n == 1 { one } else { many })
}

/// "about 3 min left"
pub fn fmt_remaining(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    match minutes {
        0     => "less than a minute left".into(),
        1..60 => format!("about {minutes} min left"),
        _     => format!("about {} hr {} min left", minutes / 60, minutes % 60),
    }
}

/// /Users/me/Documents → ~/Documents
pub fn pretty_path(path: &Path) -> String {
    match std::env::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        None => path.display().to_string(),
    }
}

/// What the platform calls its file manager.
pub const FILE_MANAGER: &str = if cfg!(target_os = "macos") {
    "Finder"
} else if cfg!(windows) {
    "Explorer"
} else {
    "Files"
};

/// Shows a file selected in the file manager.
pub fn reveal(path: &Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(windows)]
    let _ = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = open::that(path.parent().unwrap_or(path));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_finder() {
        assert_eq!(fmt_size(512), "512 bytes");
        assert_eq!(fmt_size(143_731), "144 KB");
        assert_eq!(fmt_size(6_333_041), "6.3 MB");
        assert_eq!(fmt_count(1284), "1,284");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1_000_000), "1,000,000");
        assert_eq!(plural(1, "file", "files"), "1 file");
        assert_eq!(fmt_remaining(3.0 * 60.0), "about 3 min left");
        assert_eq!(fmt_remaining(65.0 * 60.0), "about 1 hr 5 min left");
        assert_eq!(fmt_date(Timestamp { year: 2026, month: 11, day: 7, hour: 4, minute: 5 }), "7 Nov 2026 at 04:05");
        assert_eq!(kind_name("jpg"), "JPEG");
        assert_eq!(natural_cmp("IMG_2.jpg", "IMG_10.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("img_2.JPG", "IMG_2.jpg"), Ordering::Equal);
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
        assert_eq!(Sort::from_setting(&Sort::by(SortKey::Size).to_setting()), Some(Sort::by(SortKey::Size)));
        assert_eq!(extension_of("IMG_0001.JPG"), "jpg");
        assert_eq!(safe_name("a/b:c?. "), "a_b_c_");
        assert_eq!(safe_name(".."), "_");
    }
}
