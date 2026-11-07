use std::path::{Path, PathBuf};
use crate::fs::Timestamp;
use crate::source::drives::Drive;

/// A file recovered and saved during a scan.
#[derive(Clone, Debug)]
pub struct FoundFile {
    pub kind:      String,       // "JPEG", "MP4", ...
    pub extension: String,       // lowercase, without the dot
    pub offset:    u64,          // where it was found on the source
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Photos,
    Media,
    Documents,
    Other,
}

impl Category {
    pub const ALL: [Category; 4] = [Category::Photos, Category::Media, Category::Documents, Category::Other];

    pub fn from_ext(ext: &str) -> Self {
        match ext {
            "jpg" | "jpeg" | "png" | "gif" | "heic" | "heif" | "avif" | "webp" | "bmp" | "tif" | "tiff"
            | "dng" | "cr2" | "cr3" | "nef" | "arw" | "raf" | "orf" | "rw2" => Category::Photos,
            "mp4" | "mov" | "m4v" | "avi" | "mkv" | "wmv" | "3gp" | "mts" | "m2ts"
            | "m4a" | "mp3" | "wav" | "aac" | "flac" | "ogg" | "wma" => Category::Media,
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf"
            | "txt" | "md" | "csv" | "pages" | "numbers" | "key" | "epub" => Category::Documents,
            _ => Category::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Category::Photos    => "Photos",
            Category::Media     => "Videos & Audio",
            Category::Documents => "Documents",
            Category::Other     => "Other",
        }
    }
}

/// What we scan: an image file or a whole drive.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Image(PathBuf),
    Drive(Drive),
}

impl Source {
    pub fn name(&self) -> String {
        match self {
            Source::Image(path) => path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            Source::Drive(drive) => drive.name.clone(),
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

/// "7 Nov 2026 at 14:02"
pub fn fmt_date(t: Timestamp) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = MONTHS.get(t.month as usize - 1).unwrap_or(&"");
    format!("{} {month} {} at {:02}:{:02}", t.day, t.year, t.hour, t.minute)
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
        assert_eq!(extension_of("IMG_0001.JPG"), "jpg");
        assert_eq!(safe_name("a/b:c?. "), "a_b_c_");
        assert_eq!(safe_name(".."), "_");
    }
}
