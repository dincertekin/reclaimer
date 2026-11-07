//! Phones. Neither iPhones nor modern Android phones let a computer read
//! their raw storage, so recovery means copying everything the phone will
//! hand over — including what's in its trash:
//!
//! - iPhone backups on this computer (Finder / iTunes)           `iphone`
//! - an iPhone on a cable: the camera roll, Recently Deleted too  `iphone_usb`
//! - an Android phone on a cable (File Transfer mode): photos,
//!   videos, documents, and the gallery's trash                  `android`

pub mod android;
pub mod iphone;
pub mod iphone_usb;

use std::io;
use std::path::PathBuf;

use crate::fs::Modified;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneKind {
    IPhone,
    Android,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoneStatus {
    Ready,
    /// An iPhone that hasn't trusted this computer yet; the scan will ask.
    NeedsTrust,
}

/// A phone connected by cable.
#[derive(Clone, Debug, PartialEq)]
pub struct Phone {
    pub kind:   PhoneKind,
    /// iPhone UDID, or Android serial number / USB location.
    pub id:     String,
    pub name:   String,
    pub status: PhoneStatus,
}

/// A file on the phone worth copying.
#[derive(Clone, Debug, PartialEq)]
pub struct PhoneFile {
    pub name:             String,
    /// Where it lives on the phone, e.g. "/Camera Roll/100APPLE".
    pub folder:           String,
    pub size:             u64,
    pub modified:         Option<Modified>,
    pub recently_deleted: bool,
}

/// Where recovered files go and who hears about them. The scan thread
/// implements this; the phone code just walks the phone.
pub trait Sink {
    fn stage(&mut self, text: &'static str);
    /// Everything that will be copied has been listed: `bytes` in total.
    fn listed(&mut self, bytes: u64);
    /// Where to write this file.
    fn destination(&mut self, file: &PhoneFile) -> io::Result<PathBuf>;
    /// The file is written. Return false to stop.
    fn saved(&mut self, file: &PhoneFile, path: PathBuf) -> bool;
}

/// Phones connected right now. A phone that can't be reached (no Apple
/// software on Windows, Android in "charging only" mode) is simply absent.
pub fn connected() -> Vec<Phone> {
    let mut phones = runtime().map(|rt| rt.block_on(iphone_usb::list())).unwrap_or_default();
    phones.extend(android::list());
    phones
}

/// A plain-text report of what Reclaimer can see: `Reclaimer --diagnose`.
pub fn diagnose() -> String {
    let iphone = runtime().map(|rt| rt.block_on(iphone_usb::diagnose())).unwrap_or_else(|e| e.to_string());
    format!("{iphone}\n{}\nPhones shown on the Choose screen: {:?}", android::diagnose(), connected())
}

/// Copies the phone's photos, videos and documents through `sink`.
pub fn recover(phone: &Phone, sink: &mut dyn Sink) -> io::Result<()> {
    let rt = runtime()?;
    match phone.kind {
        PhoneKind::IPhone => rt.block_on(iphone_usb::recover(phone, sink)),
        PhoneKind::Android => rt.block_on(android::recover(phone, sink)),
    }
}

/// Both phone libraries are async; a small single-threaded runtime is all
/// they need.
fn runtime() -> io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread().enable_all().build()
}
