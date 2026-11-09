#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{enumerate, start};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{enumerate, start};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn enumerate() -> Result<Vec<crate::types::VideoDevice>, String> {
    Err("Console Link supports macOS and Windows 11.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start(
    _: &str,
    _: &crate::types::VideoMode,
    _: crate::types::FrameSink,
) -> Result<Box<dyn crate::types::CaptureSession>, String> {
    Err("Console Link supports macOS and Windows 11.".into())
}
