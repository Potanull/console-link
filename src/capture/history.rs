use crate::capture::VideoFrame;
use crate::config::capture::MAX_SYNC_MS;
use std::collections::VecDeque;
use std::sync::Arc;

/// Bounded timestamped video history. Selection consumes only frames whose deadline is due.
pub struct FrameHistory {
    frames: VecDeque<Arc<VideoFrame>>,
    byte_limit: usize,
    retention_ns: u64,
    bytes: usize,
    dropped: u64,
    last_timestamp: Option<u64>,
}

impl FrameHistory {
    pub fn new(byte_limit: usize, retention_ns: u64) -> Self {
        Self {
            frames: VecDeque::new(),
            byte_limit,
            retention_ns,
            bytes: 0,
            dropped: 0,
            last_timestamp: None,
        }
    }

    pub fn set_retention_ns(&mut self, retention_ns: u64) {
        self.retention_ns = retention_ns;
    }

    pub fn push(&mut self, frame: Arc<VideoFrame>) {
        let expected = (frame.width as usize)
            .checked_mul(frame.height as usize)
            .and_then(|n| n.checked_mul(4));
        if frame.width == 0
            || frame.height == 0
            || expected != Some(frame.rgba.len())
            || frame.rgba.len() > self.byte_limit
            || self
                .last_timestamp
                .is_some_and(|last| frame.timestamp_ns < last)
        {
            self.dropped += 1;
            return;
        }
        self.last_timestamp = Some(frame.timestamp_ns);
        let cutoff = frame.timestamp_ns.saturating_sub(self.retention_ns);
        while self
            .frames
            .front()
            .is_some_and(|old| old.timestamp_ns < cutoff)
            || self.bytes.saturating_add(frame.rgba.len()) > self.byte_limit
        {
            if let Some(old) = self.frames.pop_front() {
                self.bytes -= old.rgba.len();
                self.dropped += 1;
            } else {
                break;
            }
        }
        self.bytes += frame.rgba.len();
        self.frames.push_back(frame);
    }

    pub fn select(&mut self, now_ns: u64, video_delay_ns: u64) -> Option<Arc<VideoFrame>> {
        let deadline = now_ns.checked_sub(video_delay_ns)?;
        let mut selected = None;
        while self
            .frames
            .front()
            .is_some_and(|f| f.timestamp_ns <= deadline)
        {
            let frame = self.frames.pop_front().expect("front was present");
            self.bytes -= frame.rgba.len();
            if selected.is_some() {
                self.dropped += 1;
            }
            selected = Some(frame);
        }
        selected
    }

    pub fn max_delay_ms(&self, frame_bytes: usize, fps: f64) -> u32 {
        if frame_bytes == 0 || !fps.is_finite() || fps <= 0.0 {
            return 0;
        }
        ((self.byte_limit / frame_bytes).saturating_sub(3) as f64 / fps * 1000.0)
            .clamp(0.0, f64::from(MAX_SYNC_MS)) as u32
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
        self.last_timestamp = None;
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
    pub fn dropped(&self) -> u64 {
        self.dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::VideoFrame;
    use std::sync::Arc;

    fn frame(ms: u64) -> Arc<VideoFrame> {
        Arc::new(VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
            timestamp_ns: ms * 1_000_000,
        })
    }

    #[test]
    fn negative_audio_offset_uses_history_at_sixty_fps() {
        let mut history = FrameHistory::new(1024, 600_000_000);
        for n in 0..=60 {
            history.push(frame(n * 1000 / 60));
        }
        let selected = history.select(1_000_000_000, 500_000_000).unwrap();
        assert_eq!(selected.timestamp_ns, 500_000_000);
    }

    #[test]
    fn selects_latest_due_not_a_future_frame() {
        let mut history = FrameHistory::new(1024, 600_000_000);
        history.push(frame(10));
        history.push(frame(20));
        history.push(frame(30));
        assert_eq!(
            history.select(25_000_000, 0).unwrap().timestamp_ns,
            20_000_000
        );
        assert!(history.select(26_000_000, 0).is_none());
        assert_eq!(
            history.select(30_000_000, 0).unwrap().timestamp_ns,
            30_000_000
        );
    }

    #[test]
    fn bounds_memory_and_rejects_invalid_frames() {
        let mut history = FrameHistory::new(8, 600_000_000);
        history.push(frame(1));
        history.push(frame(2));
        history.push(frame(3));
        assert_eq!(history.bytes(), 8);
        assert_eq!(history.dropped(), 1);
        let invalid = Arc::new(VideoFrame {
            width: 2,
            height: 2,
            rgba: vec![0],
            timestamp_ns: 4,
        });
        history.push(invalid);
        assert_eq!(history.bytes(), 8);
        assert_eq!(history.dropped(), 2);
    }

    #[test]
    fn rejects_out_of_order_and_clears_old_generation() {
        let mut history = FrameHistory::new(1024, 600_000_000);
        history.push(frame(20));
        history.push(frame(10));
        assert_eq!(history.dropped(), 1);
        history.clear();
        assert!(history.select(100_000_000, 0).is_none());
    }
}
