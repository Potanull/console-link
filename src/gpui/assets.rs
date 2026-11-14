//! Embedded interface assets. No file or network access is needed for navigation.
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;
pub(super) struct Assets;
impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "ui/console.svg" => include_bytes!("../../assets/ui/console.svg"),
            "ui/audio.svg" => include_bytes!("../../assets/ui/audio.svg"),
            "ui/image.svg" => include_bytes!("../../assets/ui/image.svg"),
            "ui/chat.svg" => include_bytes!("../../assets/ui/chat.svg"),
            "ui/settings.svg" => include_bytes!("../../assets/ui/settings.svg"),
            "ui/chevron-down.svg" => include_bytes!("../../assets/ui/chevron-down.svg"),
            "ui/check.svg" => include_bytes!("../../assets/ui/check.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok([
            "ui/console.svg",
            "ui/audio.svg",
            "ui/image.svg",
            "ui/chat.svg",
            "ui/settings.svg",
            "ui/chevron-down.svg",
            "ui/check.svg",
        ]
        .into_iter()
        .filter(|p| p.starts_with(path))
        .map(SharedString::from)
        .collect())
    }
}
