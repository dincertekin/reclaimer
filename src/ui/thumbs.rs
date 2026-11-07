//! Photo thumbnails, decoded on background threads so scrolling never
//! stutters. Only tiles on screen ask for one, and the cache is capped, so
//! memory stays flat even with thousands of results.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};

const WORKERS: usize = 3;
const MAX_CACHED: usize = 400;

type Key = (PathBuf, u32);

enum Entry {
    Pending,
    Ready(TextureHandle),
    Failed, // not decodable (partial file, HEIC, ...): the tile shows an icon
}

pub struct Thumbnails {
    cache: HashMap<Key, Entry>,
    order: VecDeque<Key>, // oldest first, for eviction
    jobs:  Sender<Key>,
    done:  Receiver<(Key, Option<ColorImage>)>,
}

impl Thumbnails {
    pub fn new(ctx: &Context) -> Self {
        let (jobs, job_rx) = mpsc::channel::<Key>();
        let (done_tx, done) = mpsc::channel();
        let job_rx = Arc::new(Mutex::new(job_rx));
        for _ in 0..WORKERS {
            let (job_rx, done_tx, ctx) = (Arc::clone(&job_rx), done_tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                // Take the next job; the lock is released before decoding.
                while let Ok(key) = { let rx = job_rx.lock().unwrap(); rx.recv() } {
                    let image = decode(&key.0, key.1);
                    if done_tx.send((key, image)).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self { cache: HashMap::new(), order: VecDeque::new(), jobs, done }
    }

    /// Uploads finished thumbnails. Call once per frame.
    pub fn poll(&mut self, ctx: &Context) {
        while let Ok((key, image)) = self.done.try_recv() {
            if let Some(entry) = self.cache.get_mut(&key) {
                *entry = match image {
                    Some(image) => Entry::Ready(ctx.load_texture(key.0.to_string_lossy(), image, TextureOptions::LINEAR)),
                    None => Entry::Failed,
                };
            }
        }
    }

    /// The thumbnail if ready; otherwise queues it and returns `None`.
    pub fn get(&mut self, path: &Path, max_side: u32) -> Option<&TextureHandle> {
        let key = (path.to_path_buf(), max_side);
        if !self.cache.contains_key(&key) {
            let _ = self.jobs.send(key.clone());
            self.cache.insert(key.clone(), Entry::Pending);
            self.order.push_back(key.clone());
            while self.order.len() > MAX_CACHED {
                if let Some(old) = self.order.pop_front() {
                    self.cache.remove(&old);
                }
            }
        }
        match self.cache.get(&key) {
            Some(Entry::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// True when nothing is waiting to be decoded.
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        !self.cache.values().any(|e| matches!(e, Entry::Pending))
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.order.clear();
    }
}

fn decode(path: &Path, max_side: u32) -> Option<ColorImage> {
    // JPEG, PNG, GIF in pure Rust; HEIC (most iPhone photos), AVIF and
    // camera RAW through the operating system's own decoder.
    decode_portable(path, max_side).or_else(|| system::decode(path, max_side))
}

fn decode_portable(path: &Path, max_side: u32) -> Option<ColorImage> {
    let image = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let thumb = image.thumbnail(max_side, max_side).to_rgba8();
    let size = [thumb.width() as usize, thumb.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, thumb.as_raw()))
}

/// The system's image decoders, for formats there's no good pure-Rust
/// decoder for.
mod system {
    use std::path::Path;
    #[cfg(not(windows))]
    use std::path::PathBuf;
    use eframe::egui::ColorImage;

    /// A fresh temporary file name for a converted thumbnail.
    #[cfg(not(windows))]
    fn temp_jpeg() -> PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("reclaimer-thumb-{}-{n}.jpg", std::process::id()))
    }

    /// Runs a converter that writes a JPEG, then reads that back.
    #[cfg(not(windows))]
    fn via_jpeg(max_side: u32, convert: impl FnOnce(&Path) -> bool) -> Option<ColorImage> {
        let out = temp_jpeg();
        let image = convert(&out).then(|| super::decode_portable(&out, max_side)).flatten();
        let _ = std::fs::remove_file(&out);
        image
    }

    /// macOS: `sips` reads HEIC, AVIF and camera RAW.
    #[cfg(target_os = "macos")]
    pub fn decode(path: &Path, max_side: u32) -> Option<ColorImage> {
        via_jpeg(max_side, |out| {
            std::process::Command::new("/usr/bin/sips")
                .args(["-s", "format", "jpeg", "-Z", &max_side.to_string()])
                .arg(path)
                .arg("--out")
                .arg(out)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    }

    /// Linux: libheif's `heif-convert`, when installed.
    #[cfg(all(unix, not(target_os = "macos")))]
    pub fn decode(path: &Path, max_side: u32) -> Option<ColorImage> {
        let ext = path.extension()?.to_string_lossy().to_lowercase();
        if !matches!(ext.as_str(), "heic" | "heif" | "avif") {
            return None;
        }
        via_jpeg(max_side, |out| {
            std::process::Command::new("heif-convert")
                .arg(path)
                .arg(out)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        })
    }

    /// Windows: WIC, the system imaging library (HEIC needs Microsoft's free
    /// "HEIF Image Extensions", which most PCs have).
    #[cfg(windows)]
    pub fn decode(path: &Path, max_side: u32) -> Option<ColorImage> {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Foundation::GENERIC_READ;
        use windows::Win32::Graphics::Imaging::*;
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};

        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: plain COM calls on objects we create and own; `wide` is
        // NUL-terminated and outlives the call that reads it.
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED); // per thread; fine to repeat
            let factory: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
            let decoder = factory.CreateDecoderFromFilename(PCWSTR(wide.as_ptr()), None, GENERIC_READ, WICDecodeMetadataCacheOnDemand).ok()?;
            let frame = decoder.GetFrame(0).ok()?;
            let (mut w, mut h) = (0u32, 0u32);
            frame.GetSize(&mut w, &mut h).ok()?;
            if w == 0 || h == 0 {
                return None;
            }
            let scale = (max_side as f64 / w.max(h) as f64).min(1.0);
            let (tw, th) = (((w as f64 * scale).round() as u32).max(1), ((h as f64 * scale).round() as u32).max(1));

            let scaler = factory.CreateBitmapScaler().ok()?;
            scaler.Initialize(&frame, tw, th, WICBitmapInterpolationModeFant).ok()?;
            let converter = factory.CreateFormatConverter().ok()?;
            converter
                .Initialize(&scaler, &GUID_WICPixelFormat32bppRGBA, WICBitmapDitherTypeNone, None::<&IWICPalette>, 0.0, WICBitmapPaletteTypeCustom)
                .ok()?;
            let mut pixels = vec![0u8; (tw * th * 4) as usize];
            converter.CopyPixels(std::ptr::null(), tw * 4, &mut pixels).ok()?;
            Some(ColorImage::from_rgba_unmultiplied([tw as usize, th as usize], &pixels))
        }
    }

    #[cfg(not(any(unix, windows)))]
    pub fn decode(_path: &Path, _max_side: u32) -> Option<ColorImage> {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(target_os = "macos")]
    fn heic_photos_get_thumbnails() {
        let path = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/photo.heic"));
        let thumb = super::decode(path, 320).expect("HEIC thumbnail");
        assert_eq!(thumb.size[0].max(thumb.size[1]), 320);
    }
}
