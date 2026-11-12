//! Video capture, mode selection, frame history, and session delivery.

mod history;
mod native;
#[cfg(any(target_os = "macos", target_os = "windows", test))]
mod pixels;
mod policy;
mod session;
mod types;

pub use history::FrameHistory;
pub use native::{enumerate, start};
pub use policy::{CapturePreferences, choose_mode, nominal_fps};
pub use session::{RetryPolicy, send_latest};
pub use types::{
    CaptureSession, CaptureStatistics, FrameSink, VideoDevice, VideoFrame, VideoMode, monotonic_ns,
};
