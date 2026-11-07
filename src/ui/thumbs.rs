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
    let image = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let thumb = image.thumbnail(max_side, max_side).to_rgba8();
    let size = [thumb.width() as usize, thumb.height() as usize];
    Some(ColorImage::from_rgba_unmultiplied(size, thumb.as_raw()))
}
