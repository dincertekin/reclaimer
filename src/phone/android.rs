//! An Android phone on a cable, in File Transfer mode (MTP). The phone
//! hands over its shared storage — camera, screenshots, downloads, WhatsApp
//! media — but not app-private data. Since Android 11, files moved to the
//! gallery's trash stay on the phone for 30 days as hidden
//! `.trashed-<time>-<name>` files, which MTP still lists.

use std::collections::HashMap;
use std::io::{self, Write};
use std::time::Duration;

use mtp_rs::{ByteRange, MtpDevice, ObjectHandle, ObjectInfo};

use super::{Phone, PhoneFile, PhoneKind, PhoneStatus, Sink};
use crate::fs::{Modified, Timestamp};
use crate::media;

const APPLE: u16 = 0x05AC; // iPhones also speak PTP; they're handled by `iphone_usb`

pub fn list() -> Vec<Phone> {
    let Ok(devices) = MtpDevice::list_devices() else { return Vec::new() };
    devices
        .into_iter()
        .filter(|d| d.vendor_id != APPLE)
        .map(|d| Phone {
            kind:   PhoneKind::Android,
            id:     match &d.serial_number {
                Some(serial) => format!("serial:{serial}"),
                None => format!("location:{}", d.location_id),
            },
            name:   d.product.clone().or_else(|| d.manufacturer.clone()).unwrap_or_else(|| "Android Phone".into()),
            status: PhoneStatus::Ready,
        })
        .collect()
}

/// What MTP sees, for `--diagnose`.
pub fn diagnose() -> String {
    match MtpDevice::list_devices() {
        Err(e) => format!("Android (MTP): listing failed ({e})"),
        Ok(devices) => {
            let mut out = format!("Android (MTP): {} device(s)", devices.len());
            for d in devices {
                out += &format!("\n  {:04x}:{:04x}  {:?} {:?}", d.vendor_id, d.product_id, d.manufacturer, d.product);
            }
            out
        }
    }
}

pub async fn recover(phone: &Phone, sink: &mut dyn Sink) -> io::Result<()> {
    sink.stage("Reading the phone\u{2026}");
    let device = open(phone).await?;
    let storages = device.storages().await.map_err(|e| explain(e, &phone.name))?;

    // List everything first, so progress has a total.
    let mut files: Vec<(usize, ObjectHandle, PhoneFile)> = Vec::new();
    for (index, storage) in storages.iter().enumerate() {
        let label = storage.info().description.clone();
        let objects = storage.list_objects_recursive(None).await.map_err(|e| explain(e, &phone.name))?;
        let folders: HashMap<ObjectHandle, &ObjectInfo> = objects.iter().filter(|o| o.is_folder()).map(|o| (o.handle, o)).collect();
        for object in objects.iter().filter(|o| !o.is_folder()) {
            let path = folder_path(&folders, object.parent);
            let Some((name, recently_deleted)) = classify(&path, &object.filename) else { continue };
            let folder = std::iter::once(label.as_str()).chain(path.iter().map(String::as_str)).collect::<Vec<_>>().join("/");
            files.push((index, object.handle, PhoneFile {
                name,
                folder: format!("/{folder}"),
                size: object.size,
                modified: object.modified.map(|d| Modified::Local(Timestamp { year: d.year, month: d.month, day: d.day, hour: d.hour, minute: d.minute })),
                recently_deleted,
            }));
        }
    }

    sink.listed(files.iter().map(|(_, _, f)| f.size).sum());
    for (index, handle, file) in files {
        let destination = sink.destination(&file)?;
        if download(&storages[index], handle, &destination).await.is_err() {
            let _ = std::fs::remove_file(&destination);
            continue; // e.g. deleted on the phone mid-copy
        }
        if !sink.saved(&file, destination) {
            break;
        }
    }
    let _ = device.close().await;
    Ok(())
}

/// Opens the phone. On macOS, Apple's camera service grabs phones the moment
/// they're plugged in; it steps aside when asked (and restarts by itself).
async fn open(phone: &Phone) -> io::Result<MtpDevice> {
    let mut attempt = 0;
    loop {
        let result = match phone.id.split_once(':') {
            Some(("serial", serial)) => MtpDevice::open_by_serial(serial).await,
            Some(("location", location)) => MtpDevice::open_by_location(location.parse().unwrap_or_default()).await,
            _ => MtpDevice::open_first().await,
        };
        match result {
            Ok(device) => return Ok(device),
            Err(e) if e.is_exclusive_access() && cfg!(target_os = "macos") && attempt < 5 => {
                let _ = std::process::Command::new("/usr/bin/pkill").args(["-x", "ptpcamerad"]).status();
                tokio::time::sleep(Duration::from_millis(150)).await;
                attempt += 1;
            }
            Err(e) => return Err(explain(e, &phone.name)),
        }
    }
}

async fn download(storage: &mtp_rs::Storage, handle: ObjectHandle, to: &std::path::Path) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(to)?);
    let mut stream = storage.download(handle, ByteRange::Full).await.map_err(io::Error::other)?;
    while let Some(chunk) = stream.next_chunk().await {
        out.write_all(&chunk.map_err(io::Error::other)?)?;
    }
    out.flush()
}

/// Folder names from the storage root down to `parent`.
fn folder_path(folders: &HashMap<ObjectHandle, &ObjectInfo>, mut parent: ObjectHandle) -> Vec<String> {
    let mut path = Vec::new();
    while let Some(folder) = folders.get(&parent) {
        if path.len() > 64 {
            break;
        }
        path.push(folder.filename.clone());
        parent = folder.parent;
    }
    path.reverse();
    path
}

/// Whether a file is worth copying, and as what: (name to save it as, in the
/// trash?). `path` is its folders from the storage root.
fn classify(path: &[String], name: &str) -> Option<(String, bool)> {
    let is = |i: usize, want: &str| path.get(i).is_some_and(|p| p.eq_ignore_ascii_case(want));
    if is(0, "Android") && (is(1, "data") || is(1, "obb")) {
        return None; // app-private files
    }
    // Samsung's gallery keeps its trash in a hidden `.Trash` folder.
    let in_trash_folder = path.iter().any(|p| p.eq_ignore_ascii_case(".Trash") || p.eq_ignore_ascii_case(".trashed"));

    // Android 11+: ".trashed-1712345678-IMG_1234.jpg" → "IMG_1234.jpg".
    let (name, trashed) = match name.strip_prefix(".trashed-") {
        Some(rest) => match rest.split_once('-') {
            Some((time, real)) if !time.is_empty() && time.bytes().all(|b| b.is_ascii_digit()) => (real, true),
            _ => (rest, true),
        },
        None => (name, in_trash_folder),
    };
    let hidden_folder = path.iter().any(|p| p.starts_with('.')) && !in_trash_folder;
    if name.starts_with('.') || hidden_folder || name.is_empty() {
        return None; // thumbnails, caches, .nomedia
    }
    let ext = std::path::Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    media::is_personal(&ext).then(|| (name.to_string(), trashed))
}

/// Turns MTP errors into something a person can act on.
fn explain(error: mtp_rs::Error, phone: &str) -> io::Error {
    let message = if error.is_exclusive_access() {
        format!("Another app is using {phone}. Quit Photos, Image Capture and Android File Transfer, then try again.")
    } else if error.is_permission_denied() {
        format!("This computer won\u{2019}t let Reclaimer open {phone}. On Linux, allow USB access for your user (see the README).")
    } else if error.is_disconnected() {
        format!("{phone} was disconnected.")
    } else {
        format!("Couldn\u{2019}t read {phone}. Unlock it and choose \u{201C}File Transfer\u{201D} in its USB notification. ({error})")
    };
    io::Error::other(message)
}

#[cfg(test)]
mod tests {
    use super::classify;

    fn path(p: &str) -> Vec<String> {
        p.split('/').filter(|s| !s.is_empty()).map(String::from).collect()
    }

    #[test]
    fn keeps_photos_videos_and_documents() {
        assert_eq!(classify(&path("DCIM/Camera"), "PXL_20261106.jpg"), Some(("PXL_20261106.jpg".into(), false)));
        assert_eq!(classify(&path("Movies"), "clip.MP4"), Some(("clip.MP4".into(), false)));
        assert_eq!(classify(&path("Download"), "ticket.pdf"), Some(("ticket.pdf".into(), false)));
        assert_eq!(classify(&path("Android/media/com.whatsapp/WhatsApp/Media/WhatsApp Images"), "IMG-1.jpg"), Some(("IMG-1.jpg".into(), false)));
    }

    #[test]
    fn finds_the_gallery_trash() {
        assert_eq!(classify(&path("DCIM/Camera"), ".trashed-1712345678-PXL_1.jpg"), Some(("PXL_1.jpg".into(), true)));
        assert_eq!(classify(&path("Android/.Trash/com.sec.android.gallery3d"), "20261101_101010.jpg"), Some(("20261101_101010.jpg".into(), true)));
    }

    #[test]
    fn skips_app_data_caches_and_other_files() {
        assert_eq!(classify(&path("Android/data/com.example/files"), "photo.jpg"), None);
        assert_eq!(classify(&path("DCIM/.thumbnails"), "123.jpg"), None);
        assert_eq!(classify(&path("Pictures"), ".nomedia"), None);
        assert_eq!(classify(&path("Download"), "setup.apk"), None);
    }
}
