//! Windows: drives are \\.\PhysicalDrive0, 1, 2, ... We ask each one for its
//! size and model, then ask each drive letter which physical drive it is on.

use std::fs::{File, OpenOptions};
use std::io;
use std::mem;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Storage::FileSystem::{
    BusTypeMmc, BusTypeSd, BusTypeUsb, GetLogicalDrives, GetVolumeInformationW, FILE_SHARE_READ,
    FILE_SHARE_WRITE, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, DISK_GEOMETRY_EX, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX,
    IOCTL_STORAGE_QUERY_PROPERTY, STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
};

use super::Drive;
use crate::source::FileSource;

const MAX_DRIVES: u32 = 64;

pub fn list() -> io::Result<Vec<Drive>> {
    let letters = drive_letters();
    let mut drives = Vec::new();
    for number in 0..MAX_DRIVES {
        let path = PathBuf::from(format!(r"\\.\PhysicalDrive{number}"));
        // Access 0 = query only: works without administrator rights.
        let Ok(handle) = open_handle(&path, 0) else { continue };
        let Ok((size, _)) = geometry(&handle) else { continue };
        let (model, removable) = describe(&handle).unwrap_or_default();

        let mount_points: Vec<PathBuf> = letters
            .iter()
            .filter(|(_, disks)| disks.contains(&number))
            .map(|(letter, _)| PathBuf::from(format!("{letter}:\\")))
            .collect();
        let name = mount_points
            .iter()
            .find_map(|m| volume_label(m))
            .unwrap_or_else(|| if model.is_empty() { format!("Disk {number}") } else { model.clone() });

        drives.push(Drive { path, name, model, size, removable, mount_points });
    }
    Ok(drives)
}

pub fn open(path: &Path) -> io::Result<FileSource> {
    const GENERIC_READ: u32 = 0x8000_0000;
    let file = open_handle(path, GENERIC_READ).map_err(|e| match e.kind() {
        io::ErrorKind::PermissionDenied => io::Error::new(
            e.kind(),
            "Reading a drive needs administrator rights. Right-click Reclaimer and choose \"Run as administrator\".",
        ),
        _ => e,
    })?;
    let (size, block_size) = geometry(&file)?;
    Ok(FileSource::device(file, size, block_size))
}

fn open_handle(path: &Path, access: u32) -> io::Result<File> {
    OpenOptions::new()
        .access_mode(access)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .open(path)
}

/// Sends an ioctl (with an optional property query) and reads its output into `T`.
fn query<T>(file: &File, code: u32, input: Option<&STORAGE_PROPERTY_QUERY>, out: &mut T) -> io::Result<()> {
    let (in_ptr, in_len) = match input {
        Some(q) => (q as *const STORAGE_PROPERTY_QUERY as *const _, mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32),
        None => (ptr::null(), 0),
    };
    let mut returned = 0u32;
    // SAFETY: the buffers are valid for the sizes given; the call is synchronous.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(), code, in_ptr, in_len,
            (out as *mut T).cast(), mem::size_of::<T>() as u32,
            &mut returned, ptr::null_mut(),
        )
    };
    if ok == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// (size in bytes, bytes per sector)
fn geometry(file: &File) -> io::Result<(u64, u64)> {
    // SAFETY: DISK_GEOMETRY_EX is plain data; all-zero is a valid value.
    let mut geo: DISK_GEOMETRY_EX = unsafe { mem::zeroed() };
    query(file, IOCTL_DISK_GET_DRIVE_GEOMETRY_EX, None, &mut geo)?;
    Ok((geo.DiskSize as u64, geo.Geometry.BytesPerSector as u64))
}

/// (model, removable)
fn describe(file: &File) -> io::Result<(String, bool)> {
    let request = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };
    // The descriptor is followed by the strings it points to.
    let mut buf = [0u64; 128];
    query(file, IOCTL_STORAGE_QUERY_PROPERTY, Some(&request), &mut buf)?;
    let bytes: &[u8] = bytemuck_bytes(&buf);
    // SAFETY: the buffer is 8-byte aligned and larger than the descriptor.
    let desc: STORAGE_DEVICE_DESCRIPTOR = unsafe { ptr::read(buf.as_ptr().cast()) };

    let text = |offset: u32| -> String {
        let start = offset as usize;
        if start == 0 || start >= bytes.len() {
            return String::new();
        }
        let end = bytes[start..].iter().position(|&b| b == 0).map_or(bytes.len(), |n| start + n);
        String::from_utf8_lossy(&bytes[start..end]).trim().to_string()
    };
    let model = [text(desc.VendorIdOffset), text(desc.ProductIdOffset)].join(" ").trim().to_string();
    let removable = desc.RemovableMedia || [BusTypeUsb, BusTypeSd, BusTypeMmc].contains(&desc.BusType);
    Ok((model, removable))
}

/// Each mounted drive letter and the physical drive numbers it spans.
fn drive_letters() -> Vec<(char, Vec<u32>)> {
    // Buffer for VOLUME_DISK_EXTENTS: a u32 count, then 24-byte extents
    // (u32 disk number, padding, i64 offset, i64 length) starting at byte 8.
    let mut letters = Vec::new();
    // SAFETY: no arguments; returns a bitmask of drive letters.
    let mask = unsafe { GetLogicalDrives() };
    for (i, letter) in ('A'..='Z').enumerate() {
        if mask & (1 << i) == 0 {
            continue;
        }
        let Ok(volume) = open_handle(Path::new(&format!(r"\\.\{letter}:")), 0) else { continue };
        let mut buf = [0u64; 1 + 3 * 16];
        if query(&volume, IOCTL_VOLUME_GET_VOLUME_DISK_EXTENTS, None, &mut buf).is_err() {
            continue;
        }
        let bytes = bytemuck_bytes(&buf);
        let count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
        let disks = (0..count.min(16))
            .map(|n| u32::from_le_bytes(bytes[8 + 24 * n..12 + 24 * n].try_into().unwrap()))
            .collect();
        letters.push((letter, disks));
    }
    letters
}

fn volume_label(root: &Path) -> Option<String> {
    let root: Vec<u16> = root.as_os_str().to_string_lossy().encode_utf16().chain([0]).collect();
    let mut label = [0u16; 261];
    // SAFETY: `root` is NUL-terminated and `label` is sized as passed.
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(), label.as_mut_ptr(), label.len() as u32,
            ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), 0,
        )
    };
    let len = label.iter().position(|&c| c == 0).unwrap_or(0);
    (ok != 0 && len > 0).then(|| String::from_utf16_lossy(&label[..len]))
}

fn bytemuck_bytes(words: &[u64]) -> &[u8] {
    // SAFETY: any u64 slice is a valid byte slice of 8× the length.
    unsafe { std::slice::from_raw_parts(words.as_ptr().cast(), mem::size_of_val(words)) }
}
