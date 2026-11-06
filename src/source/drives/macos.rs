//! macOS: `diskutil` describes the disks; `/dev/rdiskN` reads them. Reading a
//! raw disk needs administrator rights, which we get the way Apple intends:
//! `authopen` shows the system password prompt and hands us an open file.

use std::fs::File;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use plist::{Dictionary, Value};
use super::Drive;
use crate::source::FileSource;

pub fn list() -> io::Result<Vec<Drive>> {
    let listing = diskutil(&["list", "-plist"])?;
    let entries: Vec<&Dictionary> = array(&listing, "AllDisksAndPartitions")
        .iter()
        .filter_map(Value::as_dictionary)
        .collect();

    let mut drives = Vec::new();
    // APFS containers are virtual disks that live on a physical one; they
    // are covered by the physical disk they sit on.
    for disk in entries.iter().filter(|e| !e.contains_key("APFSPhysicalStores")) {
        let id = string(disk, "DeviceIdentifier");
        let info = diskutil(&["info", "-plist", id])?;
        if string(&info, "VirtualOrPhysical") == "Virtual" {
            continue; // a mounted disk image
        }

        // Volumes: the disk's own partitions plus those of any APFS
        // container stored on it.
        let mut volumes: Vec<&Dictionary> = array(disk, "Partitions").iter().filter_map(Value::as_dictionary).collect();
        let prefix = format!("{id}s");
        for container in &entries {
            let on_this_disk = array(container, "APFSPhysicalStores")
                .iter()
                .filter_map(Value::as_dictionary)
                .any(|s| string(s, "DeviceIdentifier").starts_with(&prefix));
            if on_this_disk {
                volumes.extend(array(container, "APFSVolumes").iter().filter_map(Value::as_dictionary));
            }
        }

        let mounted: Vec<(&str, &str)> = volumes
            .iter()
            .filter(|v| v.contains_key("MountPoint"))
            .map(|v| (string(v, "MountPoint"), string(v, "VolumeName")))
            .collect();
        let model = string(&info, "MediaName").trim().to_string();
        // The startup volume's name for the internal disk, else the first
        // user-visible volume, else the hardware model.
        let name = mounted
            .iter()
            .find(|(m, _)| *m == "/")
            .or_else(|| mounted.iter().find(|(m, _)| m.starts_with("/Volumes/")))
            .map(|(_, n)| n.to_string())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| model.clone());

        drives.push(Drive {
            path:         PathBuf::from(format!("/dev/r{id}")),
            name,
            model,
            size:         integer(&info, "TotalSize"),
            removable:    !boolean(&info, "Internal"),
            mount_points: mounted.iter().map(|(m, _)| PathBuf::from(m)).collect(),
        });
    }
    Ok(drives)
}

pub fn open(path: &Path) -> io::Result<FileSource> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => open_with_authorization(path)?,
        Err(e) => return Err(e),
    };
    let (size, block_size) = geometry(&file)?;
    Ok(FileSource::device(file, size, block_size))
}

fn geometry(file: &File) -> io::Result<(u64, u64)> {
    // From <sys/disk.h>: _IOR('d', 24, uint32_t) and _IOR('d', 25, uint64_t)
    const DKIOCGETBLOCKSIZE:  libc::c_ulong = 0x4004_6418;
    const DKIOCGETBLOCKCOUNT: libc::c_ulong = 0x4008_6419;

    let mut block_size: u32 = 0;
    let mut block_count: u64 = 0;
    // SAFETY: both ioctls write exactly one value of the pointed-to type.
    unsafe {
        if libc::ioctl(file.as_raw_fd(), DKIOCGETBLOCKSIZE, &mut block_size) != 0
            || libc::ioctl(file.as_raw_fd(), DKIOCGETBLOCKCOUNT, &mut block_count) != 0
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok((block_count * block_size as u64, block_size as u64))
}

/// Runs `authopen`, which asks for an administrator password and passes the
/// opened file back to us over a Unix socket.
fn open_with_authorization(path: &Path) -> io::Result<File> {
    let mut fds = [0; 2];
    // SAFETY: socketpair fills `fds` with two new descriptors on success.
    if unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: we own both descriptors from here on.
    let (ours, theirs) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };

    let mut child = Command::new("/usr/libexec/authopen")
        .arg("-stdoutpipe")
        .args(["-o", &libc::O_RDONLY.to_string()])
        .arg(path)
        .stdout(Stdio::from(theirs)) // moved: our copy closes when Command drops
        .spawn()?;

    let received = receive_fd(&ours);
    let status = child.wait()?;
    match received? {
        Some(fd) if status.success() => Ok(File::from(fd)),
        _ => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Reading a drive needs your administrator password.",
        )),
    }
}

/// Receives one file descriptor sent with SCM_RIGHTS. `None` if the sender
/// closed the socket without sending one (the user cancelled).
fn receive_fd(socket: &OwnedFd) -> io::Result<Option<OwnedFd>> {
    let mut data = [0u8; 64];
    let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
    // u64 storage keeps the control buffer aligned for cmsghdr.
    let mut control = [0u64; 8];

    // SAFETY: msghdr is plain data; every pointer we set outlives recvmsg,
    // and the CMSG macros only walk within `control`.
    unsafe {
        let mut msg: libc::msghdr = mem::zeroed();
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = control.as_mut_ptr().cast();
        msg.msg_controllen = mem::size_of_val(&control) as _;

        if libc::recvmsg(socket.as_raw_fd(), &mut msg, 0) < 0 {
            return Err(io::Error::last_os_error());
        }
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() || (*cmsg).cmsg_level != libc::SOL_SOCKET || (*cmsg).cmsg_type != libc::SCM_RIGHTS {
            return Ok(None);
        }
        let fd = std::ptr::read_unaligned(libc::CMSG_DATA(cmsg) as *const libc::c_int);
        Ok(Some(OwnedFd::from_raw_fd(fd)))
    }
}

fn diskutil(args: &[&str]) -> io::Result<Dictionary> {
    let output = Command::new("/usr/sbin/diskutil").args(args).output()?;
    if !output.status.success() {
        return Err(io::Error::other(format!("diskutil {} failed", args.join(" "))));
    }
    Value::from_reader_xml(&output.stdout[..])
        .ok()
        .and_then(Value::into_dictionary)
        .ok_or_else(|| io::Error::other("diskutil returned unexpected output"))
}

fn array<'a>(dict: &'a Dictionary, key: &str) -> &'a [Value] {
    dict.get(key).and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[])
}

fn string<'a>(dict: &'a Dictionary, key: &str) -> &'a str {
    dict.get(key).and_then(Value::as_string).unwrap_or("")
}

fn integer(dict: &Dictionary, key: &str) -> u64 {
    dict.get(key).and_then(Value::as_unsigned_integer).unwrap_or(0)
}

fn boolean(dict: &Dictionary, key: &str) -> bool {
    dict.get(key).and_then(Value::as_boolean).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_this_macs_internal_disk() {
        let drives = super::list().unwrap();
        let internal = drives.iter().find(|d| !d.removable).expect("no internal disk");
        assert!(internal.size > 0);
        assert!(internal.mount_points.iter().any(|m| m == std::path::Path::new("/")));
        eprintln!("{drives:#?}");
    }
}
