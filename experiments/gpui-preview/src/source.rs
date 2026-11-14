//! Disposable preview sources. No settings, accounts, or global runtime changes.
pub use console_link::capture::VideoFrame;
use console_link::capture::{self, CapturePreferences, FrameSink, choose_mode, send_latest};
use crossbeam_channel::{Receiver, Sender, bounded};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
pub struct SourceStats {
    pub received: u64,
    pub dropped: u64,
}

#[derive(Default)]
struct Counters {
    received: AtomicU64,
    dropped: AtomicU64,
    stopped: AtomicBool,
}

pub struct Source {
    frames: Receiver<VideoFrame>,
    statuses: Receiver<Result<String, String>>,
    stop: Sender<()>,
    counters: Arc<Counters>,
    worker: Option<JoinHandle<()>>,
}

impl Source {
    pub fn synthetic() -> Self {
        Self::spawn(None)
    }

    pub fn capture(device_index: usize) -> Self {
        Self::spawn(Some(device_index))
    }

    fn spawn(device_index: Option<usize>) -> Self {
        let (tx, frames) = bounded(1);
        let discard = frames.clone();
        let (status, statuses) = bounded(2);
        let (stop, stopped) = bounded(1);
        let counters = Arc::new(Counters::default());
        let counted = counters.clone();
        let sink: FrameSink = Arc::new(move |frame| {
            counted.received.fetch_add(1, Ordering::Relaxed);
            counted
                .dropped
                .fetch_add(send_latest(&tx, &discard, frame), Ordering::Relaxed);
        });
        let completed = counters.clone();
        let worker = thread::spawn(move || {
            if let Some(index) = device_index {
                run_capture(index, sink, status, stopped);
            } else {
                let _ = status.try_send(Ok("Synthetic · 1920 × 1080 · target 60 FPS".into()));
                run_synthetic(sink, stopped);
            }
            completed.stopped.store(true, Ordering::Release);
        });
        Self {
            frames,
            statuses,
            stop,
            counters,
            worker: Some(worker),
        }
    }

    pub fn latest(&self) -> Option<VideoFrame> {
        self.frames.try_iter().last()
    }

    pub fn status(&self) -> Option<Result<String, String>> {
        self.statuses.try_iter().last()
    }

    pub fn stats(&self) -> SourceStats {
        SourceStats {
            received: self.counters.received.load(Ordering::Relaxed),
            dropped: self.counters.dropped.load(Ordering::Relaxed),
        }
    }
}

impl Drop for Source {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
        // Native discovery/start can block inside an OS call. Never wait for it
        // on the UI thread. Cancellation is checked around initialization; the
        // worker drops its session when ready. Process exit releases OS handles.
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}

fn run_capture(
    index: usize,
    sink: FrameSink,
    status: Sender<Result<String, String>>,
    stop: Receiver<()>,
) {
    let result = (|| {
        let devices = capture::enumerate()?;
        let device = devices.get(index).ok_or_else(|| {
            format!(
                "Video device {index} unavailable ({} devices found)",
                devices.len()
            )
        })?;
        let mode = choose_mode(&device.modes, &CapturePreferences::default(), None)
            .ok_or_else(|| "No supported capture mode up to Full HD / 60 FPS".to_string())?;
        if !matches!(stop.try_recv(), Err(crossbeam_channel::TryRecvError::Empty)) {
            return Ok(());
        }
        let session = capture::start(&device.id, &mode, sink)?;
        if !matches!(stop.try_recv(), Err(crossbeam_channel::TryRecvError::Empty)) {
            return Ok(());
        }
        let _ = status.try_send(Ok(format!("{} · {mode}", device.name)));
        while let Err(crossbeam_channel::RecvTimeoutError::Timeout) =
            stop.recv_timeout(Duration::from_millis(100))
        {
            if let Some(error) = session.error() {
                return Err(error);
            }
        }
        drop(session);
        Ok(())
    })();
    if let Err(error) = result {
        let _ = status.try_send(Err(error));
    }
}

fn run_synthetic(sink: FrameSink, stop: Receiver<()>) {
    let width = 1920_u32;
    let height = 1080_u32;
    let mut base = vec![0_u8; (width * height * 4) as usize];
    let colors = [
        [235, 235, 235],
        [210, 210, 40],
        [40, 210, 210],
        [40, 200, 40],
        [210, 40, 210],
        [210, 40, 40],
        [40, 40, 210],
        [24, 24, 24],
    ];
    for y in 0..height {
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            let color = colors[(x * 8 / width) as usize];
            let scale = if y < height * 3 / 4 { 1.0 } else { 0.35 };
            base[offset..offset + 4].copy_from_slice(&[
                (color[0] as f32 * scale) as u8,
                (color[1] as f32 * scale) as u8,
                (color[2] as f32 * scale) as u8,
                255,
            ]);
        }
    }
    let epoch = Instant::now();
    let period = Duration::from_nanos(1_000_000_000 / 60);
    let mut deadline = epoch;
    loop {
        let wait = deadline.saturating_duration_since(Instant::now());
        match stop.recv_timeout(wait) {
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            _ => break,
        }
        let mut rgba = base.clone();
        // Position follows elapsed time, so skipped production deadlines do not
        // slow the animation or cause a catch-up burst.
        let x = ((epoch.elapsed().as_secs_f64() * 480.0) as u32) % width;
        for y in 0..height {
            let offset = ((y * width + x) * 4) as usize;
            rgba[offset..offset + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
        sink(VideoFrame {
            width,
            height,
            rgba,
            timestamp_ns: capture::monotonic_ns(),
        });
        deadline += period;
        if deadline < Instant::now() {
            deadline = Instant::now() + period;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_does_not_wait_for_blocked_native_initialization() {
        let (_frames_tx, frames) = bounded(1);
        let (_status_tx, statuses) = bounded(1);
        let (stop, _stopped) = bounded(1);
        let (release, blocked) = bounded::<()>(1);
        let (started, entered) = bounded(1);
        let worker = thread::spawn(move || {
            let _ = started.send(());
            let _ = blocked.recv_timeout(Duration::from_secs(2));
        });
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let source = Source {
            frames,
            statuses,
            stop,
            counters: Arc::new(Counters::default()),
            worker: Some(worker),
        };
        let began = Instant::now();
        drop(source);
        let elapsed = began.elapsed();
        let _ = release.send(());
        assert!(
            elapsed < Duration::from_secs(1),
            "UI waited {elapsed:?} for initialization"
        );
    }

    #[test]
    fn slow_consumer_keeps_only_latest_frame_and_drop_stops_producer() {
        let source = Source::synthetic();
        let deadline = Instant::now() + Duration::from_secs(5);
        while source.stats().received < 4 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let frame = source.latest().expect("synthetic frame");
        assert_eq!((frame.width, frame.height), (1920, 1080));
        assert_eq!(frame.rgba.len(), 1920 * 1080 * 4);
        assert!(source.stats().dropped > 0);
        assert!(source.frames.len() <= 1);
        let counter = source.counters.clone();
        drop(source);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !counter.stopped.load(Ordering::Acquire) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(counter.stopped.load(Ordering::Acquire));
        let stopped_at = counter.received.load(Ordering::Relaxed);
        thread::sleep(Duration::from_millis(40));
        assert_eq!(counter.received.load(Ordering::Relaxed), stopped_at);
    }
}
