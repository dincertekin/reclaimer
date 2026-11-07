//! iPhone backups made by Finder (macOS) or iTunes / Apple Devices (Windows).
//!
//! A backup is one folder per device. Its files are stored under their
//! SHA-1 names (`ab/ab12…`); `Manifest.db`, an SQLite database, maps each
//! one back to its domain ("CameraRollDomain") and path on the phone
//! ("Media/DCIM/100APPLE/IMG_0001.HEIC"). The Photos library database in
//! the backup says which photos are in Recently Deleted.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use plist::{Dictionary, Value as Plist};

use crate::fs::Modified;
use crate::sqlite::{Database, Table};

#[derive(Clone, Debug, PartialEq)]
pub struct Backup {
    pub path:      PathBuf,
    pub device:    String,
    pub model:     String,
    pub date:      Option<Modified>,
    /// Encrypted backups need their password, which we can't ask for yet.
    pub encrypted: bool,
}

/// A file in a backup worth recovering.
#[derive(Clone, Debug, PartialEq)]
pub struct BackupFile {
    /// Where the backup keeps it.
    pub stored:           PathBuf,
    pub name:             String,
    /// Where it lives conceptually: "/Camera Roll/100APPLE", "/Messages", ...
    pub folder:           String,
    pub size:             u64,
    pub modified:         Option<Modified>,
    pub recently_deleted: bool,
}

/// Where each platform's backup software keeps backups.
pub fn backup_folders() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else { return Vec::new() };
    if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/MobileSync/Backup")]
    } else if cfg!(windows) {
        let mut folders = vec![home.join("Apple").join("MobileSync").join("Backup")]; // Microsoft Store version
        if let Some(appdata) = std::env::var_os("APPDATA") {
            folders.push(PathBuf::from(appdata).join("Apple Computer").join("MobileSync").join("Backup"));
        }
        folders
    } else {
        Vec::new()
    }
}

/// All backups in `folders`, newest first. macOS protects its backup folder:
/// without Full Disk Access this fails with `PermissionDenied`.
pub fn find_backups(folders: &[PathBuf]) -> io::Result<Vec<Backup>> {
    let mut backups = Vec::new();
    for folder in folders {
        let entries = match std::fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        backups.extend(entries.flatten().filter_map(|e| read_backup(&e.path())));
    }
    backups.sort_by_key(|b| std::cmp::Reverse(b.date.map(|d| d.local().to_unix())));
    Ok(backups)
}

fn read_backup(dir: &Path) -> Option<Backup> {
    let info = Plist::from_file(dir.join("Info.plist")).ok()?.into_dictionary()?;
    let manifest = Plist::from_file(dir.join("Manifest.plist")).ok().and_then(Plist::into_dictionary);
    let text = |d: &Dictionary, key: &str| d.get(key).and_then(Plist::as_string).map(str::to_string);
    let date = info.get("Last Backup Date").and_then(Plist::as_date).and_then(|d| {
        let since = std::time::SystemTime::from(d).duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(Modified::Utc(since.as_secs() as i64))
    });
    Some(Backup {
        path:      dir.to_path_buf(),
        device:    text(&info, "Device Name").or_else(|| text(&info, "Display Name")).unwrap_or_else(|| "iPhone".into()),
        model:     text(&info, "Product Name").unwrap_or_default(),
        date,
        encrypted: manifest.as_ref().and_then(|m| m.get("IsEncrypted")).and_then(Plist::as_boolean).unwrap_or(false),
    })
}

/// What we recover, by domain and path prefix, and the folder it goes in.
const KINDS: &[(&str, &str, &str)] = &[
    ("CameraRollDomain", "Media/DCIM/", "/Camera Roll"),
    ("MediaDomain", "Library/SMS/Attachments/", "/Messages"),
    ("AppDomainGroup-group.net.whatsapp.WhatsApp.shared", "Message/Media/", "/WhatsApp"),
];
const PHOTOS_LIBRARY: &str = "Media/PhotoData/Photos.sqlite";

/// The photos, videos and attachments in a backup.
pub fn files(backup: &Backup) -> io::Result<Vec<BackupFile>> {
    if backup.encrypted {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("The backup of {} is encrypted. Reading encrypted backups isn\u{2019}t supported yet.", backup.device),
        ));
    }
    let manifest = Database::open(std::fs::read(backup.path.join("Manifest.db"))?)?.table("Files")?;
    let [id, domain, path, flags, meta] = ["fileID", "domain", "relativePath", "flags", "file"]
        .map(|c| manifest.column(c).ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("Manifest.db has no {c} column"))));
    let (id, domain, path, flags, meta) = (id?, domain?, path?, flags?, meta?);
    let stored = |file_id: &str| backup.path.join(file_id.get(..2).unwrap_or("")).join(file_id);

    // Which Camera Roll files are in Recently Deleted.
    let stored_copy = |wanted: &str| {
        manifest
            .rows
            .iter()
            .find(|r| r[domain].as_text() == Some("CameraRollDomain") && r[path].as_text() == Some(wanted))
            .and_then(|r| r[id].as_text())
            .and_then(|file_id| std::fs::read(stored(file_id)).ok())
    };
    let wal = stored_copy(&format!("{PHOTOS_LIBRARY}-wal"));
    let trashed = stored_copy(PHOTOS_LIBRARY).map(|db| recently_deleted_in(db, wal.as_deref())).unwrap_or_default();

    let mut files = Vec::new();
    for row in &manifest.rows {
        let (Some(file_id), Some(row_domain), Some(row_path)) = (row[id].as_text(), row[domain].as_text(), row[path].as_text()) else { continue };
        if row[flags].as_int() != Some(1) {
            continue; // folders and links
        }
        let Some(&(_, prefix, folder)) = KINDS.iter().find(|(d, p, _)| *d == row_domain && row_path.starts_with(p)) else { continue };
        let name = row_path.rsplit('/').next().unwrap_or(row_path).to_string();
        if name.starts_with('.') {
            continue;
        }
        // Camera Roll keeps its subfolders (100APPLE, ...); attachments are flattened.
        let folder = match row_domain {
            "CameraRollDomain" => {
                let inside = row_path[prefix.len()..].rsplit_once('/').map_or("", |(dir, _)| dir);
                if inside.is_empty() { folder.to_string() } else { format!("{folder}/{inside}") }
            }
            _ => folder.to_string(),
        };
        let file = stored(file_id);
        let (size, modified) = row[meta].as_blob().map(file_metadata).unwrap_or_default();
        let size = size.or_else(|| std::fs::metadata(&file).ok().map(|m| m.len())).unwrap_or(0);
        files.push(BackupFile {
            recently_deleted: row_domain == "CameraRollDomain" && trashed.contains(row_path.trim_start_matches("Media/")),
            stored: file,
            name,
            folder,
            size,
            modified,
        });
    }
    Ok(files)
}

/// Size and modification time from a file's archived metadata (an
/// NSKeyedArchiver property list whose second object is the file record).
fn file_metadata(blob: &[u8]) -> (Option<u64>, Option<Modified>) {
    let record = Plist::from_reader(io::Cursor::new(blob))
        .ok()
        .and_then(Plist::into_dictionary)
        .and_then(|d| d.get("$objects").and_then(Plist::as_array).and_then(|o| o.get(1)).and_then(Plist::as_dictionary).cloned());
    let Some(record) = record else { return (None, None) };
    let number = |key: &str| record.get(key).and_then(|v| v.as_unsigned_integer().or_else(|| v.as_signed_integer().map(|n| n as u64)));
    (number("Size"), number("LastModified").map(|t| Modified::Utc(t as i64)))
}

/// Paths ("DCIM/100APPLE/IMG_0002.JPG") of photos in Recently Deleted,
/// from the Photos library database and its write-ahead log.
pub(crate) fn recently_deleted_in(photos_library: Vec<u8>, wal: Option<&[u8]>) -> HashSet<String> {
    let table = Database::open_with_wal(photos_library, wal)
        .ok()
        // The table was called ZGENERICASSET before iOS 14.
        .and_then(|db| db.table("ZASSET").or_else(|_| db.table("ZGENERICASSET")).ok());
    let Some(table) = table else { return HashSet::new() };
    trashed_paths(&table)
}

fn trashed_paths(table: &Table) -> HashSet<String> {
    let (Some(state), Some(dir), Some(name)) = (table.column("ZTRASHEDSTATE"), table.column("ZDIRECTORY"), table.column("ZFILENAME")) else {
        return HashSet::new();
    };
    table
        .rows
        .iter()
        .filter(|r| r[state].as_int() == Some(1))
        .filter_map(|r| Some(format!("{}/{}", r[dir].as_text()?, r[name].as_text()?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/iphone-backups"))
    }

    #[test]
    fn finds_backups_and_notes_encrypted_ones() {
        let backups = find_backups(&[fixtures(), fixtures().join("missing")]).unwrap();
        assert_eq!(backups.len(), 2);
        assert!(backups.iter().all(|b| b.device == "Test iPhone" && b.model == "iPhone 15"));
        assert_eq!(backups.iter().filter(|b| b.encrypted).count(), 1);
        let date = backups[0].date.unwrap();
        assert_eq!(date, Modified::Utc(1_793_736_900)); // 3 Nov 2026 20:15 UTC
    }

    #[test]
    fn lists_photos_attachments_and_recently_deleted() {
        let backup = read_backup(&fixtures().join("00008110-000A1B2C3D4E5F6A")).unwrap();
        let mut files = files(&backup).unwrap();
        files.sort_by(|a, b| (&a.folder, &a.name).cmp(&(&b.folder, &b.name)));
        let summary: Vec<_> = files.iter().map(|f| (f.folder.as_str(), f.name.as_str(), f.size, f.recently_deleted)).collect();
        assert_eq!(summary, [
            ("/Camera Roll/100APPLE", "IMG_0001.JPG", 94_978, false),
            ("/Camera Roll/100APPLE", "IMG_0002.JPG", 42_355, true),
            ("/Messages", "Menu.pdf", 48, false),
            ("/WhatsApp", "9f8e2d1c.jpg", 42_355, false),
        ]);
        assert!(files.iter().all(|f| f.stored.exists() && f.modified == Some(Modified::Utc(1_793_990_100))));
    }

    #[test]
    fn encrypted_backups_say_so() {
        let backup = read_backup(&fixtures().join("00008110-000F00DFEEDBEEF01")).unwrap();
        let error = files(&backup).unwrap_err();
        assert!(error.to_string().contains("encrypted"));
    }
}
