use console_link::config::artwork as config;
use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui;
use std::{collections::HashMap, io::Read};

/// Remote artwork only. Capture frames and OAuth credentials never enter this worker.
pub struct Artwork {
    requests: Sender<String>,
    results: Receiver<(String, Option<egui::ColorImage>)>,
    textures: HashMap<String, Option<egui::TextureHandle>>,
    retry_at: HashMap<String, std::time::Instant>,
}

impl Artwork {
    pub fn new() -> Self {
        let (requests, rx) = bounded::<String>(config::QUEUE_CAPACITY);
        let (tx, results) = bounded(config::QUEUE_CAPACITY);
        std::thread::spawn(move || {
            let client = oauth2::reqwest::blocking::Client::builder()
                .redirect(oauth2::reqwest::redirect::Policy::none())
                .connect_timeout(config::CONNECT_TIMEOUT)
                .timeout(config::DOWNLOAD_TIMEOUT)
                .build();
            while let Ok(url) = rx.recv() {
                let result = client
                    .as_ref()
                    .ok()
                    .and_then(|client| download(client, &url));
                if tx.send((url, result)).is_err() {
                    break;
                }
            }
        });
        Self {
            requests,
            results,
            textures: HashMap::new(),
            retry_at: HashMap::new(),
        }
    }

    pub fn image(&mut self, ctx: &egui::Context, url: &str) -> Option<egui::TextureHandle> {
        while let Ok((url, image)) = self.results.try_recv() {
            if image.is_none() {
                self.retry_at
                    .insert(url.clone(), std::time::Instant::now() + config::RETRY_DELAY);
            } else {
                self.retry_at.remove(&url);
            }
            self.textures.insert(
                url.clone(),
                image.map(|image| ctx.load_texture(url, image, egui::TextureOptions::LINEAR)),
            );
        }
        if self
            .retry_at
            .get(url)
            .is_some_and(|retry| std::time::Instant::now() >= *retry)
        {
            self.retry_at.remove(url);
            self.textures.remove(url);
        }
        if !self.textures.contains_key(url) && allowed_url(url) {
            // Bound GPU memory and queued work when browsing a large game catalog.
            if self.textures.len() >= config::TEXTURE_CACHE_CAPACITY {
                self.textures.clear();
                self.retry_at.clear();
            }
            if self.requests.try_send(url.to_owned()).is_ok() {
                self.textures.insert(url.to_owned(), None);
            }
        }
        self.textures.get(url).cloned().flatten()
    }
}

fn allowed_url(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|url| {
        url.scheme() == "https"
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
            && matches!(
                url.host_str(),
                Some(
                    "cdn.discordapp.com"
                        | "img-eshop.cdn.nintendo.net"
                        | "atum-img-lp1.cdn.nintendo.net"
                        | "raw.githubusercontent.com"
                )
            )
    })
}

fn download(client: &oauth2::reqwest::blocking::Client, url: &str) -> Option<egui::ColorImage> {
    if !allowed_url(url) {
        return None;
    }
    let response = client.get(url).send().ok()?.error_for_status().ok()?;
    if response
        .content_length()
        .is_some_and(|size| size > config::MAX_DOWNLOAD_BYTES)
    {
        return None;
    }
    let mut bytes = Vec::new();
    response
        .take(config::MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > config::MAX_DOWNLOAD_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(config::MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(config::MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(config::MAX_IMAGE_ALLOCATION);
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .thumbnail(config::THUMBNAIL_SIZE, config::THUMBNAIL_SIZE)
        .to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [image.width() as usize, image.height() as usize],
        &image,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_artwork_retries_after_backoff() {
        let (requests, rx) = bounded(config::QUEUE_CAPACITY);
        let (tx, results) = bounded(config::QUEUE_CAPACITY);
        let mut artwork = Artwork {
            requests,
            results,
            textures: HashMap::new(),
            retry_at: HashMap::new(),
        };
        let ctx = egui::Context::default();
        let url = "https://cdn.discordapp.com/embed/avatars/0.png";
        assert!(artwork.image(&ctx, url).is_none());
        assert_eq!(rx.try_recv().unwrap(), url);
        tx.send((url.into(), None)).unwrap();
        artwork.image(&ctx, url);
        assert!(rx.try_recv().is_err());
        artwork.retry_at.insert(
            url.into(),
            std::time::Instant::now() - std::time::Duration::from_secs(1),
        );
        artwork.image(&ctx, url);
        assert_eq!(rx.try_recv().unwrap(), url);
    }
    #[test]
    fn artwork_accepts_only_expected_public_https_hosts() {
        assert!(allowed_url(
            "https://atum-img-lp1.cdn.nintendo.net/example.jpg"
        ));
        assert!(allowed_url(
            "https://cdn.discordapp.com/embed/avatars/0.png"
        ));
        for url in [
            "http://cdn.discordapp.com/a.png",
            "https://localhost/a",
            "https://cdn.discordapp.com.attacker.net/a",
            "https://user@cdn.discordapp.com/a",
        ] {
            assert!(!allowed_url(url));
        }
    }
}
