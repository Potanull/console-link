//! UI cache for public game covers and Discord avatars; never capture frames.
use console_link::config::artwork as config;
use crossbeam_channel::{Receiver, Sender, bounded};
use gpui::{RenderImage, Window};
use std::{collections::HashMap, sync::Arc, time::Instant};

pub(super) struct Artwork {
    requests: Option<Sender<String>>,
    results: Receiver<(String, Option<image::RgbaImage>)>,
    cache: HashMap<String, Option<Arc<RenderImage>>>,
    retry: HashMap<String, Instant>,
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
        }
    }
    pub fn image(&mut self, url: &str, window: &mut Window) -> Option<Arc<RenderImage>> {
        while let Ok((key, image)) = self.results.try_recv() {
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
            if let Some(Some(old)) = self.cache.insert(key, image) {
                let _ = window.drop_image(old);
            }
        }
        if self.retry.get(url).is_some_and(|t| Instant::now() >= *t) {
            self.retry.remove(url);
            self.cache.remove(url);
        }
        if !self.cache.contains_key(url) && console_link::artwork::allowed_url(url) {
            if self.cache.len() >= config::TEXTURE_CACHE_CAPACITY {
                for (_, image) in self.cache.drain() {
                    if let Some(image) = image {
                        let _ = window.drop_image(image);
                    }
                }
                self.retry.clear();
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
    pub fn clear(&mut self, window: &mut Window) {
        for (_, image) in self.cache.drain() {
            if let Some(image) = image {
                let _ = window.drop_image(image);
            }
        }
    }
}
