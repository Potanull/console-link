use serde::{Deserialize, Serialize};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

pub fn monotonic_ns() -> u64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH
        .get_or_init(Instant::now)
        .elapsed()
        .as_nanos()
        .min(u64::MAX as u128) as u64
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoMode {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
}

impl std::fmt::Display for VideoMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let fps = self.fps_num as f64 / self.fps_den.max(1) as f64;
        write!(f, "{} × {} · {:.2} fps", self.width, self.height, fps)
    }
}

#[derive(Clone, Debug)]
pub struct VideoDevice {
    pub id: String,
    pub name: String,
    pub modes: Vec<VideoMode>,
}

#[derive(Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Monotonic host timestamp at capture delivery, not HDMI-to-display time.
    pub timestamp_ns: u64,
}

pub type FrameSink = Arc<dyn Fn(VideoFrame) + Send + Sync>;

/// Owns an active native capture session; dropping it stops capture.
pub trait CaptureSession: Send {
    fn error(&self) -> Option<String> {
        None
    }
}
