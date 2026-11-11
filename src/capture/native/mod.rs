#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{enumerate, start};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "windows")]
pub use windows::{enumerate, start};

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn enumerate() -> Result<Vec<crate::capture::VideoDevice>, String> {
    Err("Console Link supports macOS and Windows 11.".into())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn start(
    _: &str,
    _: &crate::capture::VideoMode,
    _: crate::capture::FrameSink,
) -> Result<Box<dyn crate::capture::CaptureSession>, String> {
    Err("Console Link supports macOS and Windows 11.".into())
}
