//! An iPhone on a cable. The computer's USB multiplexer (usbmuxd: part of
//! macOS; installed with Apple Devices / iTunes on Windows; a package on
//! Linux) carries connections to services on the phone. After the phone
//! trusts this computer, its AFC service serves the media folder: every
//! photo and video — those in Recently Deleted included, since they stay in
//! /DCIM until purged — plus the Photos library that says which are trashed.

use std::collections::HashSet;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use idevice::afc::opcode::AfcFopenMode;
use idevice::afc::AfcClient;
use idevice::lockdown::LockdownClient;
use idevice::provider::UsbmuxdProvider;
use idevice::usbmuxd::{Connection, UsbmuxdAddr, UsbmuxdConnection, UsbmuxdDevice};
use idevice::{IdeviceError, IdeviceService};

use super::{iphone::recently_deleted_in, Phone, PhoneFile, PhoneKind, PhoneStatus, Sink};
use crate::fs::Modified;

const LABEL: &str = "Reclaimer";
const MEDIA: &str = "/DCIM";
const PHOTOS_LIBRARY: &str = "/PhotoData/Photos.sqlite";
const TRUST_TIMEOUT: Duration = Duration::from_secs(120);
const CHUNK: usize = 1 << 20;

pub async fn list() -> Vec<Phone> {
    let Ok(mut mux) = UsbmuxdConnection::default().await else { return Vec::new() };
    let Ok(devices) = mux.get_devices().await else { return Vec::new() };
    let mut phones = Vec::new();
    for device in devices.into_iter().filter(|d| matches!(d.connection_type, Connection::Usb)) {
        let trusted = mux.get_pair_record(&device.udid).await.is_ok();
        phones.push(Phone {
            kind:   PhoneKind::IPhone,
            name:   device_name(&device).await.unwrap_or_else(|| "iPhone".into()),
            id:     device.udid,
            status: if trusted { PhoneStatus::Ready } else { PhoneStatus::NeedsTrust },
        });
    }
    phones
}

/// What the iPhone connection service sees, for `--diagnose`.
pub async fn diagnose() -> String {
    match UsbmuxdConnection::default().await {
        Err(e) => format!("iPhone service (usbmuxd): not reachable ({e})"),
        Ok(mut mux) => match mux.get_devices().await {
            Err(e) => format!("iPhone service (usbmuxd): reachable, but listing failed ({e})"),
            Ok(devices) => {
                let mut out = format!("iPhone service (usbmuxd): reachable, {} device(s)", devices.len());
                for d in devices {
                    let trusted = mux.get_pair_record(&d.udid).await.is_ok();
                    out += &format!("\n  {:?}  {}  trusted: {trusted}", d.connection_type, d.udid);
                }
                out
            }
        },
    }
}

/// The name the owner gave the phone ("Dinçer's iPhone"). Readable before
/// pairing.
async fn device_name(device: &UsbmuxdDevice) -> Option<String> {
    let provider = provider(device);
    let mut lockdown = LockdownClient::connect(&provider).await.ok()?;
    let value = lockdown.get_value(Some("DeviceName"), None).await.ok()?;
    value.as_string().map(str::to_string)
}

fn provider(device: &UsbmuxdDevice) -> UsbmuxdProvider {
    device.to_provider(UsbmuxdAddr::default(), LABEL)
}

pub async fn recover(phone: &Phone, sink: &mut dyn Sink) -> io::Result<()> {
    let mut mux = UsbmuxdConnection::default().await.map_err(|_| unreachable_mux())?;
    let device = mux
        .get_device(&phone.id)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::NotFound, format!("{} was disconnected.", phone.name)))?;

    if mux.get_pair_record(&device.udid).await.is_err() {
        sink.stage("Unlock your iPhone and tap Trust\u{2026}");
        pair(&mut mux, &device).await?;
    }

    sink.stage("Reading the iPhone\u{2026}");
    let provider = provider(&device);
    let mut afc = AfcClient::connect(&provider).await.map_err(|e| explain(e, &phone.name))?;

    let trashed = photos_library(&mut afc).await;
    let mut files = Vec::new();
    walk(&mut afc, MEDIA, &mut files).await.map_err(|e| explain(e, &phone.name))?;
    let files: Vec<(String, PhoneFile)> = files
        .into_iter()
        .map(|(path, size, modified)| {
            let (folder, name) = path.rsplit_once('/').unwrap_or(("", &path));
            let inside = folder.strip_prefix(MEDIA).unwrap_or(folder);
            let file = PhoneFile {
                name: name.to_string(),
                folder: format!("/Camera Roll{inside}"),
                size,
                modified,
                recently_deleted: trashed.contains(path.trim_start_matches('/')),
            };
            (path, file)
        })
        .collect();

    sink.listed(files.iter().map(|(_, f)| f.size).sum());
    for (path, file) in files {
        let destination = sink.destination(&file)?;
        if download(&mut afc, &path, file.size, &destination).await.is_err() {
            let _ = std::fs::remove_file(&destination);
            continue; // e.g. deleted on the phone mid-copy
        }
        if !sink.saved(&file, destination) {
            break;
        }
    }
    Ok(())
}

/// Asks the phone to trust this computer, waiting while it shows the
/// "Trust This Computer?" alert, then saves the pairing with the system so
/// it's remembered (and shared with Finder / Apple Devices).
async fn pair(mux: &mut UsbmuxdConnection, device: &UsbmuxdDevice) -> io::Result<()> {
    let buid = mux.get_buid().await.map_err(|_| unreachable_mux())?;
    let host_id = host_id();
    let started = Instant::now();
    loop {
        let mut lockdown = LockdownClient::connect(&provider(device)).await.map_err(|e| explain(e, "the iPhone"))?;
        match lockdown.pair(host_id.clone(), buid.clone(), Some(LABEL)).await {
            Ok(record) => {
                let bytes = record.serialize().map_err(|e| explain(e, "the iPhone"))?;
                return mux.save_pair_record(&device.udid, bytes).await.map_err(|e| explain(e, "the iPhone"));
            }
            Err(IdeviceError::PairingDialogResponsePending | IdeviceError::PasswordProtected) if started.elapsed() < TRUST_TIMEOUT => {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            Err(e) => return Err(explain(e, "the iPhone")),
        }
    }
}

/// Recursively lists files under `dir`: (path, size, modified).
async fn walk(afc: &mut AfcClient, dir: &str, out: &mut Vec<(String, u64, Option<Modified>)>) -> Result<(), IdeviceError> {
    let mut pending = vec![dir.to_string()];
    while let Some(dir) = pending.pop() {
        for name in afc.list_dir(&dir).await? {
            if name == "." || name == ".." || name.starts_with('.') {
                continue;
            }
            let path = format!("{dir}/{name}");
            let Ok(info) = afc.get_file_info(&path).await else { continue };
            match info.st_ifmt.as_str() {
                "S_IFDIR" => pending.push(path),
                "S_IFREG" => out.push((path, info.size as u64, Some(Modified::Utc(info.modified.and_utc().timestamp())))),
                _ => {}
            }
        }
    }
    Ok(())
}

/// Which photos are in Recently Deleted, from the phone's Photos library
/// (and its write-ahead log, where the latest changes live).
async fn photos_library(afc: &mut AfcClient) -> HashSet<String> {
    let Some(db) = read_whole(afc, PHOTOS_LIBRARY).await else { return HashSet::new() };
    let wal = read_whole(afc, &format!("{PHOTOS_LIBRARY}-wal")).await;
    recently_deleted_in(db, wal.as_deref())
}

async fn read_whole(afc: &mut AfcClient, path: &str) -> Option<Vec<u8>> {
    let mut file = afc.open(path, AfcFopenMode::RdOnly).await.ok()?;
    let bytes = file.read_entire().await.ok();
    let _ = file.close().await;
    bytes
}

async fn download(afc: &mut AfcClient, path: &str, size: u64, to: &std::path::Path) -> io::Result<()> {
    let mut out = io::BufWriter::new(std::fs::File::create(to)?);
    let mut file = afc.open(path, AfcFopenMode::RdOnly).await.map_err(io::Error::other)?;
    let mut written = 0u64;
    while written < size {
        let chunk = file.read_n(CHUNK).await.map_err(io::Error::other)?;
        if chunk.is_empty() {
            break;
        }
        out.write_all(&chunk)?;
        written += chunk.len() as u64;
    }
    let _ = file.close().await;
    out.flush()
}

/// A random identifier for this computer, in the UUID form iOS expects.
fn host_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut bytes = [0u8; 16];
    for half in bytes.chunks_mut(8) {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
        half.copy_from_slice(&h.finish().to_le_bytes());
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &hex[..8], &hex[8..12], &hex[12..16], &hex[16..20], &hex[20..])
}

fn unreachable_mux() -> io::Error {
    let hint = if cfg!(windows) {
        "Install Apple Devices (or iTunes) from the Microsoft Store so Windows can talk to iPhones."
    } else if cfg!(target_os = "linux") {
        "Install and start usbmuxd so Linux can talk to iPhones."
    } else {
        "macOS's iPhone connection service isn't responding. Unplug the iPhone and plug it back in."
    };
    io::Error::new(io::ErrorKind::NotConnected, hint)
}

/// Turns protocol errors into something a person can act on.
fn explain(error: IdeviceError, phone: &str) -> io::Error {
    let (kind, message) = match error {
        IdeviceError::PasswordProtected => (io::ErrorKind::PermissionDenied, format!("Unlock {phone} and try again.")),
        IdeviceError::UserDeniedPairing => (io::ErrorKind::PermissionDenied, format!("{phone} didn\u{2019}t trust this computer. Unplug it, plug it back in, and tap Trust.")),
        IdeviceError::PairingDialogResponsePending => (io::ErrorKind::TimedOut, format!("{phone} is still waiting for you to tap Trust.")),
        other => (io::ErrorKind::Other, format!("Couldn\u{2019}t read {phone}: {other}")),
    };
    io::Error::new(kind, message)
}

#[cfg(test)]
mod tests {
    #[test]
    fn host_ids_look_like_uuids() {
        let id = super::host_id();
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
        assert_ne!(id, super::host_id());
    }
}

