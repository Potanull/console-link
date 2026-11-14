//! Bounded public artwork decoding shared by desktop frontends.
use crate::config::artwork as config;
use std::io::Read;
pub fn allowed_url(url: &str) -> bool {
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

pub fn download(client: &oauth2::reqwest::blocking::Client, url: &str) -> Option<image::RgbaImage> {
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
    Some(image)
}
