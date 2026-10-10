//! Thumbnails (Design/spec/browse.md §6): made only for what is requested (the tiles on screen), newest request
//! first, in a small background pool; cached on disk as 320 px JPEG keyed by path, size and modification time, so
//! revisiting is instant.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

const WIDTH: u32 = 320;
const QUALITY: u8 = 80;

#[derive(Clone, Debug, PartialEq)]
pub enum Thumb {
    Ready(PathBuf),
    /// Not an image Waxbill can decode (a video), or the file is missing.
    None,
}

#[derive(Default)]
struct Queue {
    /// Requested media, newest last (taken from the end: what is on screen now first).
    waiting: Vec<PathBuf>,
    queued: HashSet<PathBuf>,
    done: HashMap<PathBuf, Thumb>,
    /// Finished since the view last looked.
    fresh: bool,
}

pub struct Thumbnails {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

impl Thumbnails {
    pub fn new() -> Thumbnails {
        let dir = cache_dir().join("thumbnails");
        let _ = std::fs::create_dir_all(&dir);
        let queue: Arc<(Mutex<Queue>, Condvar)> = Default::default();
        let workers = std::thread::available_parallelism()
            .map(|n| n.get().clamp(2, 6))
            .unwrap_or(4);
        for _ in 0..workers {
            let queue = queue.clone();
            let dir = dir.clone();
            std::thread::spawn(move || {
                loop {
                    let media = {
                        let (lock, ready) = &*queue;
                        let mut q = lock.lock().unwrap();
                        while q.waiting.is_empty() {
                            q = ready.wait(q).unwrap();
                        }
                        q.waiting.pop().unwrap()
                    };
                    let thumb = make(&media, &dir);
                    let mut q = queue.0.lock().unwrap();
                    q.queued.remove(&media);
                    q.done.insert(media, thumb);
                    q.fresh = true;
                }
            });
        }
        Thumbnails { queue }
    }

    /// The thumbnail if it is ready; otherwise it is requested (again, moving it to the front).
    pub fn get(&self, media: &Path) -> Option<Thumb> {
        let (lock, ready) = &*self.queue;
        let mut q = lock.lock().unwrap();
        if let Some(thumb) = q.done.get(media) {
            return Some(thumb.clone());
        }
        if !q.queued.insert(media.to_path_buf()) {
            q.waiting.retain(|p| p != media);
        }
        q.waiting.push(media.to_path_buf());
        ready.notify_one();
        None
    }

    /// Forget requests that are no longer on screen.
    pub fn keep_only(&self, wanted: &HashSet<PathBuf>) {
        let mut q = self.queue.0.lock().unwrap();
        q.waiting.retain(|p| wanted.contains(p));
        let waiting: HashSet<PathBuf> = q.waiting.iter().cloned().collect();
        q.queued.retain(|p| waiting.contains(p));
    }

    /// Whether thumbnails finished since the last call (the view then redraws).
    pub fn take_fresh(&self) -> bool {
        std::mem::take(&mut self.queue.0.lock().unwrap().fresh)
    }
}

fn make(media: &Path, dir: &Path) -> Thumb {
    let Ok(meta) = std::fs::metadata(media) else {
        return Thumb::None;
    };
    let is_image = media
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "jpg" | "jpeg" | "png"));
    if !is_image {
        return Thumb::None;
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    media.hash(&mut hasher);
    meta.len().hash(&mut hasher);
    meta.modified().ok().hash(&mut hasher);
    let cached = dir.join(format!("{:016x}.jpg", hasher.finish()));
    if cached.exists() {
        return Thumb::Ready(cached);
    }
    let Ok(image) = image::open(media) else {
        return Thumb::None;
    };
    let thumb = image.thumbnail(WIDTH, WIDTH * 3 / 4).to_rgb8();
    let temp = cached.with_extension("tmp");
    let written = std::fs::File::create(&temp)
        .map_err(|e| e.to_string())
        .and_then(|file| {
            let mut writer = std::io::BufWriter::new(file);
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, QUALITY)
                .encode_image(&thumb)
                .map_err(|e| e.to_string())
        })
        .and_then(|()| std::fs::rename(&temp, &cached).map_err(|e| e.to_string()));
    match written {
        Ok(()) => Thumb::Ready(cached),
        Err(_) => {
            let _ = std::fs::remove_file(&temp);
            Thumb::None
        }
    }
}

/// Waxbill's cache directory (thumbnails can always be made again).
pub fn cache_dir() -> PathBuf {
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir)
        .join("io.github.wsyxbcl.waxbill")
}
