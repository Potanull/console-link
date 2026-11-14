//! UI cache for public game covers and Discord avatars; never capture frames.
use console_link::config::artwork as config;
use crossbeam_channel::{Receiver, Sender, bounded};
use gpui::{RenderImage, Window};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

pub(super) struct Artwork {
    requests: Option<Sender<String>>,
    results: Receiver<(String, Option<image::RgbaImage>)>,
    cache: HashMap<String, Option<Arc<RenderImage>>>,
    retry: HashMap<String, Instant>,
    used: HashSet<String>,
    age: HashMap<String, u64>,
    frame: u64,
}
impl Artwork {
    pub fn new(enabled: bool) -> Self {
        let (tx, rx) = bounded::<String>(config::QUEUE_CAPACITY);
        let (out, results) = bounded(config::QUEUE_CAPACITY);
        if enabled {
            std::thread::spawn(move || {
                let client = oauth2::reqwest::blocking::Client::builder()
                    .redirect(oauth2::reqwest::redirect::Policy::none())
                    .connect_timeout(config::CONNECT_TIMEOUT)
                    .timeout(config::DOWNLOAD_TIMEOUT)
                    .build();
                while let Ok(url) = rx.recv() {
                    let image = client
                        .as_ref()
                        .ok()
                        .and_then(|c| console_link::artwork::download(c, &url));
                    if out.send((url, image)).is_err() {
                        break;
                    }
                }
            });
        }
        Self {
            requests: enabled.then_some(tx),
            results,
            cache: HashMap::new(),
            retry: HashMap::new(),
            used: HashSet::new(),
            age: HashMap::new(),
            frame: 0,
        }
    }
    pub fn begin_frame(&mut self) {
        self.used.clear();
        self.frame = self.frame.wrapping_add(1);
    }
    pub fn image(&mut self, url: &str, window: &mut Window) -> Option<Arc<RenderImage>> {
        self.used.insert(url.to_owned());
        self.age.insert(url.to_owned(), self.frame);
        while let Ok((key, image)) = self.results.try_recv() {
            // Discard stale/duplicate responses. A ready image is immutable for its URL.
            if !self.cache.contains_key(&key) || matches!(self.cache.get(&key), Some(Some(_))) {
                continue;
            }
            let image = image.map(|mut image| {
                for p in image.as_flat_samples_mut().samples.as_chunks_mut::<4>().0 {
                    p.swap(0, 2);
                }
                Arc::new(RenderImage::new(vec![image::Frame::new(image)]))
            });
            if image.is_none() {
                self.retry
                    .insert(key.clone(), Instant::now() + config::RETRY_DELAY);
            }
            self.cache.insert(key, image);
        }
        if self.retry.get(url).is_some_and(|t| Instant::now() >= *t) {
            self.retry.remove(url);
            self.cache.remove(url);
        }
        if !self.cache.contains_key(url) && console_link::artwork::allowed_url(url) {
            if self.cache.len() >= config::TEXTURE_CACHE_CAPACITY {
                let candidate = self
                    .cache
                    .keys()
                    .filter(|k| !self.used.contains(*k))
                    .min_by_key(|k| self.age.get(*k).copied().unwrap_or(0))
                    .cloned();
                let key = candidate?;
                self.age.remove(&key);
                self.retry.remove(&key);
                if let Some(Some(image)) = self.cache.remove(&key) {
                    // The current scene is rebuilt without this unused image. Wait
                    // through a complete frame before removing its previous atlas tile.
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |window, _| {
                            let _ = window.drop_image(image);
                        });
                    });
                }
            }
            if self
                .requests
                .as_ref()
                .is_some_and(|tx| tx.try_send(url.to_owned()).is_ok())
            {
                self.cache.insert(url.to_owned(), None);
            }
        }
        self.cache.get(url).cloned().flatten()
    }
    pub fn stop(&mut self) {
        self.requests.take();
        // The last scene may still reference these atlas tiles. The window's
        // renderer releases its own atlas during teardown; never evict it here.
        self.cache.clear();
    }
}
