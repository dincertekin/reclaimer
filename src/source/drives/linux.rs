//! Linux: the kernel describes every block device under /sys/block, and
//! /proc/self/mounts says where each one is mounted.

use std::fs::{self, File};
use std::io::{self, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::Drive;
use crate::source::FileSource;

/// Kernel devices that are not physical drives.
const SKIP_PREFIXES: &[&str] = &["loop", "ram", "zram", "dm-", "md", "sr", "nbd", "fd"];

pub fn list() -> io::Result<Vec<Drive>> {
    let mounts = fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let mounts = parse_mounts(&mounts);

    let mut drives = Vec::new();
    for entry in fs::read_dir("/sys/block")? {
        let disk = entry?.file_name().to_string_lossy().into_owned();
        if SKIP_PREFIXES.iter().any(|p| disk.starts_with(p)) {
            continue;
        }
        let sys = Path::new("/sys/block").join(&disk);
        let size = read_number(&sys.join("size")) * 512; // always 512-byte units
        if size == 0 {
            continue; // e.g. a card reader with no card in it
        }

        let mount_points: Vec<PathBuf> = mounts
            .iter()
            .filter(|(device, _)| physical_disks(device).contains(&disk))
            .map(|(_, mount)| mount.clone())
            .collect();

        let model = [read_text(&sys.join("device/vendor")), read_text(&sys.join("device/model"))]
            .join(" ")
            .trim()
            .to_string();
        let model = if model.is_empty() { disk.clone() } else { model };
        // The folder name it's mounted under (/media/me/SDCARD → SDCARD).
        let name = mount_points
            .iter()
            .find(|m| m.as_path() != Path::new("/"))
            .and_then(|m| m.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| model.clone());
        // USB drives often report removable = 0, so check the bus too.
        let on_usb = fs::canonicalize(&sys).is_ok_and(|p| p.to_string_lossy().contains("/usb"));

        drives.push(Drive {
            path: PathBuf::from("/dev").join(&disk),
            name,
            model,
            size,
            removable: read_number(&sys.join("removable")) == 1 || on_usb,
            encrypted: false,
            mount_points,
        });
    }
    Ok(drives)
}

pub fn open(path: &Path) -> io::Result<FileSource> {
    let mut file = File::open(path).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => io::Error::new(
            e.kind(),
            format!("Reading {} needs root. Run Reclaimer with sudo, or add yourself to the 'disk' group.", path.display()),
        ),
        _ => e,
    })?;
    let size = file.seek(SeekFrom::End(0))?;
    let disk = path.file_name().unwrap_or_default();
    let block_size = read_number(&Path::new("/sys/class/block").join(disk).join("queue/logical_block_size"));
    Ok(FileSource::device(file, size, block_size.max(512)))
}

/// `(device, mount point)` for every mounted block device.
fn parse_mounts(text: &str) -> Vec<(String, PathBuf)> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let device = fields.next()?;
            let mount = fields.next()?;
            device.starts_with("/dev/").then(|| (device.to_string(), PathBuf::from(unescape(mount))))
        })
        .collect()
}

/// /proc/mounts writes spaces and tabs as octal escapes: `\040`, `\011`.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes.get(i + 1..i + 4).and_then(|d| std::str::from_utf8(d).ok()).and_then(|d| u8::from_str_radix(d, 8).ok());
        match (bytes[i], octal) {
            (b'\\', Some(byte)) => { out.push(byte); i += 4; }
            (byte, _) => { out.push(byte); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The physical disks a block device lives on. A partition (sdb1) lives on
/// its parent (sdb); an encrypted or LVM volume (dm-0) on whatever its
/// "slaves" live on.
fn physical_disks(device: &str) -> Vec<String> {
    let name = fs::canonicalize(device)
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| device.trim_start_matches("/dev/").to_string());
    disks_under(&name, 0)
}

fn disks_under(name: &str, depth: u8) -> Vec<String> {
    let class = Path::new("/sys/class/block").join(name);
    if depth > 8 || !class.exists() {
        return Vec::new();
    }
    if let Ok(slaves) = fs::read_dir(class.join("slaves")) {
        let found: Vec<String> = slaves
            .flatten()
            .flat_map(|s| disks_under(&s.file_name().to_string_lossy(), depth + 1))
            .collect();
        if !found.is_empty() {
            return found;
        }
    }
    if class.join("partition").exists() {
        // /sys/devices/.../block/sdb/sdb1 → sdb
        let parent = fs::canonicalize(&class).ok().and_then(|p| p.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()));
        return parent.into_iter().collect();
    }
    vec![name.to_string()]
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path).map(|s| s.trim().to_string()).unwrap_or_default()
}

fn read_number(path: &Path) -> u64 {
    read_text(path).parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_mounts_with_spaces() {
        let text = "/dev/sdb1 /media/me/MY\\040CARD vfat rw 0 0\nproc /proc proc rw 0 0\n";
        assert_eq!(parse_mounts(text), vec![("/dev/sdb1".to_string(), PathBuf::from("/media/me/MY CARD"))]);
    }
}
