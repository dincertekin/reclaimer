//! Physical drives: listing them, opening them read-only, and knowing which
//! folders live on them — so we never save recovered files onto the drive
//! we are recovering from.

use std::io;
use std::path::{Path, PathBuf};
use super::FileSource;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

// Compiled for tests everywhere: its parsing helpers are plain Rust.
#[cfg(any(target_os = "linux", test))]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use windows as platform;

#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    /// Device path: /dev/rdisk4, /dev/sdb, \\.\PhysicalDrive1
    pub path:         PathBuf,
    /// What people call it: the volume name when there is one, else the model.
    pub name:         String,
    pub model:        String,
    pub size:         u64,
    pub removable:    bool,
    /// Hardware-encrypted storage reads back as noise, so nothing can be
    /// recovered from it.
    pub encrypted:    bool,
    /// Where this drive's volumes are mounted.
    pub mount_points: Vec<PathBuf>,
}

impl Drive {
    /// Is `path` on this drive? Used to refuse saving onto the source.
    pub fn holds(&self, path: &Path) -> bool {
        let path = canonical_existing(path);
        self.mount_points.iter().any(|m| path.starts_with(canonical_existing(m)))
    }
}

/// All physical drives, internal first. Disk images mounted by the OS are left out.
pub fn list() -> io::Result<Vec<Drive>> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    {
        let mut drives = platform::list()?;
        drives.sort_by_key(|d| d.removable);
        Ok(drives)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Ok(Vec::new())
}

/// Opens a drive read-only, asking the OS for permission where it can.
pub fn open(drive: &Drive) -> io::Result<FileSource> {
    #[cfg(any(target_os = "macos", target_os = "linux", windows))]
    return platform::open(&drive.path);
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    Err(io::Error::new(io::ErrorKind::Unsupported, format!("Can't open {} on this system", drive.name)))
}

/// Canonical form of the deepest part of `path` that exists — the output
/// folder may not have been created yet.
fn canonical_existing(path: &Path) -> PathBuf {
    let canonical = path
        .ancestors()
        .find_map(|p| p.canonicalize().ok().map(|c| c.join(path.strip_prefix(p).unwrap_or(Path::new("")))))
        .unwrap_or_else(|| path.to_path_buf());
    strip_verbatim(canonical)
}

/// Windows canonical paths look like \\?\C:\Users; mount points look like C:\.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(rest) => PathBuf::from(rest),
        None => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(mounts: &[&str]) -> Drive {
        Drive {
            path: "/dev/test".into(), name: "Test".into(), model: String::new(),
            size: 0, removable: true, encrypted: false, mount_points: mounts.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn boot_drive_holds_everything() {
        let home = std::env::temp_dir().join("not-created-yet/deeper");
        assert!(drive(&["/"]).holds(&home));
    }

    #[test]
    fn other_drive_holds_only_its_own_folders() {
        let card = drive(&["/Volumes/SDCARD"]);
        assert!(card.holds(Path::new("/Volumes/SDCARD/DCIM")));
        assert!(!card.holds(Path::new("/Volumes/SDCARD2/DCIM")));
        assert!(!card.holds(&std::env::temp_dir()));
    }
}
