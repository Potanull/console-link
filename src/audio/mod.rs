//! Native CoreAudio/WASAPI monitoring with a bounded stereo frame ring.
//!
//! CPAL 0.16 exposes device names rather than stable cross-platform identifiers.
//! Names are matched exactly and duplicates are rejected instead of guessing.
//! Linear interpolation handles differing sample rates; a bounded occupancy
//! feedback loop compensates small input/output clock drift. This is not source
//! PTS synchronization and does not measure the capture card's HDMI latency.

pub mod equalizer;
use crate::config::{audio as config, capture::MAX_SYNC_MS};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use equalizer::{Equalizer, PreparedEqualizer};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

#[derive(Clone, Debug)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct AudioConfig {
    pub input_id: String,
    pub output_id: String,
    pub gain: f32,
    pub muted: bool,
    pub delay_ms: u32,
    pub eq_enabled: bool,
    pub eq_gains: [f32; 10],
}

pub fn input_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    device_infos(true)
}

pub fn output_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    device_infos(false)
}

fn device_infos(input: bool) -> Result<Vec<AudioDeviceInfo>, String> {
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|error| format!("Could not list audio devices: {error}"))?;
    let mut infos = Vec::new();
    for device in devices {
        let name = device
            .name()
            .map_err(|error| format!("Could not read audio device name: {error}"))?;
        infos.push(AudioDeviceInfo {
            id: name.clone(),
            name,
        });
    }
    infos.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(infos)
}

fn selected_device(id: &str, input: bool) -> Result<cpal::Device, String> {
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|error| format!("Could not list audio devices: {error}"))?;
    select_unique(
        id,
        input,
        devices.filter_map(|device| device.name().ok().map(|name| (name, device))),
    )
}

fn select_unique<T>(
    id: &str,
    input: bool,
    devices: impl IntoIterator<Item = (String, T)>,
) -> Result<T, String> {
    let mut found = None;
    for (name, device) in devices {
        if name == id {
            if found.is_some() {
                return Err(format!(
                    "Several audio devices are named '{id}'. Rename or disconnect a duplicate device; no device was selected automatically."
                ));
            }
            found = Some(device);
        }
    }
    found.ok_or_else(|| {
        format!(
            "Selected audio {} '{id}' is unavailable. Choose an available device.",
            if input { "input" } else { "output" }
        )
    })
}

struct Controls {
    gain: AtomicU32,
    muted: AtomicBool,
    delay_ms: AtomicU32,
    peak: AtomicU32,
    underruns: AtomicU64,
    overruns: AtomicU64,
    error: AtomicU32,
    output_rate: AtomicU32,
    eq_tx: crossbeam_channel::Sender<PreparedEqualizer>,
    eq_rx: crossbeam_channel::Receiver<PreparedEqualizer>,
}

impl Controls {
    fn new(gain: f32, muted: bool, delay_ms: u32) -> Self {
        let (eq_tx, eq_rx) = crossbeam_channel::bounded(1);
        Self {
            output_rate: AtomicU32::new(48000),
            eq_tx,
            eq_rx,
            gain: AtomicU32::new(normalized_gain(gain).to_bits()),
            muted: AtomicBool::new(muted),
            delay_ms: AtomicU32::new(delay_ms.min(MAX_SYNC_MS as u32)),
            peak: AtomicU32::new(0.0_f32.to_bits()),
            underruns: AtomicU64::new(0),
            overruns: AtomicU64::new(0),
            error: AtomicU32::new(0),
        }
    }
}

fn normalized_gain(gain: f32) -> f32 {
    if gain.is_finite() {
        gain.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn finite_sample(sample: f32) -> f32 {
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

/// SPSC ring. Only the input callback writes `write`; only output advances `read`.
/// Packed atomic stereo samples avoid unsafe shared memory. Acquire/release on
/// cursors publishes complete frames and prevents reuse before consumption.
struct FrameRing {
    frames: Box<[AtomicU64]>,
    write: AtomicUsize,
    read: AtomicUsize,
}

impl FrameRing {
    fn new(capacity: usize) -> Self {
        Self {
            frames: (0..capacity.max(2)).map(|_| AtomicU64::new(0)).collect(),
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    fn push(&self, frame: [f32; 2]) -> bool {
        let write = self.write.load(Ordering::Relaxed);
        let read = self.read.load(Ordering::Acquire);
        if write.wrapping_sub(read) >= self.frames.len() {
            return false;
        }
        let packed = u64::from(frame[0].to_bits()) | (u64::from(frame[1].to_bits()) << 32);
        self.frames[write % self.frames.len()].store(packed, Ordering::Relaxed);
        self.write.store(write.wrapping_add(1), Ordering::Release);
        true
    }

    fn available(&self) -> usize {
        self.write
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Relaxed))
    }

    fn peek(&self, offset: usize) -> Option<[f32; 2]> {
        let read = self.read.load(Ordering::Relaxed);
        if self.write.load(Ordering::Acquire).wrapping_sub(read) <= offset {
            return None;
        }
        let packed =
            self.frames[read.wrapping_add(offset) % self.frames.len()].load(Ordering::Relaxed);
        Some([
            f32::from_bits(packed as u32),
            f32::from_bits((packed >> 32) as u32),
        ])
    }

    fn consume(&self, count: usize) {
        let count = count.min(self.available());
        self.read.fetch_add(count, Ordering::Release);
    }
}

struct Renderer {
    input_rate: u32,
    output_rate: u32,
    ring: Arc<FrameRing>,
    controls: Arc<Controls>,
    fraction: f64,
    step: f64,
    ready: bool,
    delay_ms: u32,
    buffer_peak: f32,
    buffer_underrun: bool,
    drift: f64,
    last_overruns: u64,
    equalizer: Equalizer,
}

impl Renderer {
    fn new(
        input_rate: u32,
        output_rate: u32,
        ring: Arc<FrameRing>,
        controls: Arc<Controls>,
    ) -> Self {
        let delay_ms = controls.delay_ms.load(Ordering::Relaxed);
        Self {
            input_rate,
            output_rate,
            ring,
            controls,
            fraction: 0.0,
            step: f64::from(input_rate) / f64::from(output_rate),
            ready: false,
            delay_ms,
            buffer_peak: 0.0,
            buffer_underrun: false,
            drift: 0.0,
            last_overruns: 0,
            equalizer: Equalizer::new(output_rate),
        }
    }

    fn begin_buffer(&mut self, output_frames: usize) {
        if let Ok(prepared) = self.controls.eq_rx.try_recv() {
            self.equalizer.apply(prepared);
        }
        self.buffer_peak = 0.0;
        self.buffer_underrun = false;
        let delay = self.controls.delay_ms.load(Ordering::Relaxed);
        let delay_frames = (u64::from(self.input_rate) * u64::from(delay) / 1000) as usize;
        let jitter_frames = (u64::from(self.input_rate) * 20 / 1000) as usize + 2;
        // A native callback can exceed the 20ms jitter baseline. Retain enough
        // input for the entire requested output buffer, including interpolation
        // at the current resampling position, instead of discarding valid audio.
        let callback_frames = if output_frames == 0 {
            0
        } else {
            (self.fraction + (output_frames - 1) as f64 * self.step).ceil() as usize + 1
        };
        let target = delay_frames.saturating_add(jitter_frames.max(callback_frames));
        let overruns = self.controls.overruns.load(Ordering::Relaxed);
        if overruns != self.last_overruns {
            // A stalled output must not resume with up to 750ms of stale audio.
            self.ring
                .consume(self.ring.available().saturating_sub(target));
            self.fraction = 0.0;
            self.drift = 0.0;
            self.last_overruns = overruns;
        }
        if delay != self.delay_ms {
            if delay > self.delay_ms {
                self.ready = false;
            } else {
                self.ring
                    .consume(self.ring.available().saturating_sub(target));
                self.fraction = 0.0;
            }
            self.delay_ms = delay;
            self.drift = 0.0;
        }
        if !self.ready && self.ring.available() >= target {
            self.ring
                .consume(self.ring.available().saturating_sub(target));
            self.ready = true;
            self.fraction = 0.0;
        }
        // Compare occupancy at the same boundary each callback. The target is
        // already a whole callback; adding half a callback would add latency.
        let error_seconds =
            (self.ring.available() as f64 - target as f64) / f64::from(self.input_rate);
        let wanted = (error_seconds * 0.05).clamp(-0.0025, 0.0025);
        self.drift += (wanted - self.drift) * 0.02;
        self.step = f64::from(self.input_rate) / f64::from(self.output_rate) * (1.0 + self.drift);
    }

    fn frame(&mut self) -> [f32; 2] {
        if self.controls.error.load(Ordering::Relaxed) != 0 || !self.ready {
            self.note_underrun();
            return [0.0; 2];
        }
        let Some(a) = self.ring.peek(0) else {
            self.ready = false;
            self.note_underrun();
            return [0.0; 2];
        };
        let b = if self.fraction == 0.0 {
            // Exact-rate playback needs no look-ahead sample for this frame.
            a
        } else if let Some(next) = self.ring.peek(1) {
            next
        } else {
            self.ready = false;
            self.note_underrun();
            return [0.0; 2];
        };
        let gain = if self.controls.muted.load(Ordering::Relaxed) {
            0.0
        } else {
            f32::from_bits(self.controls.gain.load(Ordering::Relaxed))
        };
        let interpolated = std::array::from_fn(|channel| {
            finite_sample(a[channel] + (b[channel] - a[channel]) * self.fraction as f32)
        });
        let mut result = self.equalizer.process(interpolated);
        for sample in &mut result {
            *sample *= gain;
            self.buffer_peak = self.buffer_peak.max(sample.abs());
        }
        self.fraction += self.step;
        let consume = self.fraction.floor() as usize;
        self.ring.consume(consume);
        self.fraction -= consume as f64;
        result
    }

    fn note_underrun(&mut self) {
        if !self.buffer_underrun {
            self.controls.underruns.fetch_add(1, Ordering::Relaxed);
            self.buffer_underrun = true;
        }
    }

    fn end_buffer(&self) {
        self.controls
            .peak
            .store(self.buffer_peak.to_bits(), Ordering::Relaxed);
    }
}

struct NativeStreams {
    input: cpal::Stream,
    output: cpal::Stream,
}

/// Send handle; CPAL streams stay on their owner thread because macOS streams
/// are not Send. Call `start` on a startup worker: device initialization and its
/// bounded handshake can block. Drop sends a stop signal and never joins on UI.
pub struct AudioEngine {
    stop: std::sync::mpsc::Sender<()>,
    _worker: std::thread::JoinHandle<()>,
    controls: Arc<Controls>,
}

impl AudioEngine {
    pub fn start(config: &AudioConfig) -> Result<Self, String> {
        let config = config.clone();
        let controls = Arc::new(Controls::new(config.gain, config.muted, config.delay_ms));
        let owner_controls = controls.clone();
        let (stop, stop_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("console-audio-owner".into())
            .spawn(move || match Self::open_streams(&config, owner_controls) {
                Ok(streams) => {
                    if ready_tx.send(Ok(())).is_ok() {
                        let _ = stop_rx.recv();
                    }
                    let _ = streams.output.pause();
                    let _ = streams.input.pause();
                }
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                }
            })
            .map_err(|error| format!("Cannot start audio owner thread: {error}"))?;
        match ready_rx.recv_timeout(config::START_TIMEOUT) {
            Ok(Ok(())) => Ok(Self { stop, _worker: worker, controls }),
            Ok(Err(error)) => Err(error),
            Err(_) => Err("Audio device initialization timed out. Check permissions, device connection, and whether another application is using it.".into()),
        }
    }

    fn open_streams(
        config: &AudioConfig,
        controls: Arc<Controls>,
    ) -> Result<NativeStreams, String> {
        let input_device = selected_device(&config.input_id, true)?;
        let output_device = selected_device(&config.output_id, false)?;
        let input_config = input_device.default_input_config().map_err(|error| format!("Cannot open selected audio input: {error}. Check microphone permission and whether the device is busy."))?;
        let output_config = output_device
            .default_output_config()
            .map_err(|error| format!("Cannot open selected audio output: {error}"))?;
        let input_rate = input_config.sample_rate().0;
        let output_rate = output_config.sample_rate().0;
        for (rate, channels) in [
            (input_rate, input_config.channels()),
            (output_rate, output_config.channels()),
        ] {
            if !(8000..=192000).contains(&rate) || !(1..=32).contains(&channels) {
                return Err(
                    "Unsupported audio configuration: expected 8-192 kHz and 1-32 channels.".into(),
                );
            }
        }
        controls.output_rate.store(output_rate, Ordering::Relaxed);
        let _ = controls.eq_tx.try_send(Equalizer::prepare(
            output_rate,
            config.eq_enabled,
            config.eq_gains,
        ));
        let ring = Arc::new(FrameRing::new((input_rate as usize * 3 / 4).max(4096)));
        let input = build_input(&input_device, &input_config, ring.clone(), controls.clone())?;
        let output = build_output(
            &output_device,
            &output_config,
            input_rate,
            ring,
            controls.clone(),
        )?;
        input
            .play()
            .map_err(|error| format!("Cannot start selected audio input: {error}"))?;
        output
            .play()
            .map_err(|error| format!("Cannot start selected audio output: {error}"))?;
        Ok(NativeStreams { input, output })
    }

    pub fn set_equalizer(&self, enabled: bool, gains: [f32; 10]) {
        let value = Equalizer::prepare(
            self.controls.output_rate.load(Ordering::Relaxed),
            enabled,
            gains,
        );
        if let Err(crossbeam_channel::TrySendError::Full(value)) =
            self.controls.eq_tx.try_send(value)
        {
            let _ = self.controls.eq_rx.try_recv();
            let _ = self.controls.eq_tx.try_send(value);
        }
    }
    pub fn overruns(&self) -> u64 {
        self.controls.overruns.load(Ordering::Relaxed)
    }
    pub fn set_gain(&self, gain: f32) {
        self.controls
            .gain
            .store(normalized_gain(gain).to_bits(), Ordering::Relaxed);
    }
    pub fn set_muted(&self, muted: bool) {
        self.controls.muted.store(muted, Ordering::Relaxed);
    }
    /// Adds 0-500ms to the larger of the 20ms jitter baseline and one native
    /// callback's input demand. Native device buffers add their own latency;
    /// this is not a measured total A/V delay.
    pub fn set_delay_ms(&self, delay_ms: u32) {
        self.controls
            .delay_ms
            .store(delay_ms.min(MAX_SYNC_MS as u32), Ordering::Relaxed);
    }
    pub fn peak(&self) -> f32 {
        f32::from_bits(self.controls.peak.load(Ordering::Relaxed))
    }
    pub fn underruns(&self) -> u64 {
        self.controls.underruns.load(Ordering::Relaxed)
    }
    pub fn error(&self) -> Option<String> {
        match self.controls.error.load(Ordering::Relaxed) {
            1 => Some("Audio input stopped or disconnected. Reconnect the selected capture-card input and start again.".into()),
            2 => Some("Selected audio output stopped or disconnected. Choose an available output; playback did not switch to speakers.".into()),
            _ => None,
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        self.controls.muted.store(true, Ordering::Relaxed);
        let _ = self.stop.send(());
    }
}

fn build_input(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    ring: Arc<FrameRing>,
    controls: Arc<Controls>,
) -> Result<cpal::Stream, String> {
    macro_rules! build {
        ($sample:ty) => {
            input_stream::<$sample>(device, &config.config(), ring, controls)
        };
    }
    match config.sample_format() {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        format => Err(format!("Unsupported input sample format: {format}")),
    }
}

fn input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ring: Arc<FrameRing>,
    controls: Arc<Controls>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels as usize;
    let errors = controls.clone();
    device
        .build_input_stream(
            config,
            move |data: &[T], _| {
                if controls.error.load(Ordering::Relaxed) != 0 {
                    return;
                }
                for frame in data.chunks_exact(channels) {
                    let left = finite_sample(f32::from_sample(frame[0]));
                    let right = if channels == 1 {
                        left
                    } else {
                        finite_sample(f32::from_sample(frame[1]))
                    };
                    if !ring.push([left, right]) {
                        controls.overruns.fetch_add(1, Ordering::Relaxed);
                    }
                }
            },
            move |_| {
                errors.error.store(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| format!("Cannot build selected audio input stream: {error}"))
}

fn build_output(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    input_rate: u32,
    ring: Arc<FrameRing>,
    controls: Arc<Controls>,
) -> Result<cpal::Stream, String> {
    macro_rules! build {
        ($sample:ty) => {
            output_stream::<$sample>(device, &config.config(), input_rate, ring, controls)
        };
    }
    match config.sample_format() {
        SampleFormat::I8 => build!(i8),
        SampleFormat::I16 => build!(i16),
        SampleFormat::I32 => build!(i32),
        SampleFormat::I64 => build!(i64),
        SampleFormat::U8 => build!(u8),
        SampleFormat::U16 => build!(u16),
        SampleFormat::U32 => build!(u32),
        SampleFormat::U64 => build!(u64),
        SampleFormat::F32 => build!(f32),
        SampleFormat::F64 => build!(f64),
        format => Err(format!("Unsupported output sample format: {format}")),
    }
}

fn output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    input_rate: u32,
    ring: Arc<FrameRing>,
    controls: Arc<Controls>,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let errors = controls.clone();
    let mut renderer = Renderer::new(input_rate, config.sample_rate.0, ring, controls);
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _| {
                renderer.begin_buffer(data.len() / channels);
                for frame in data.chunks_exact_mut(channels) {
                    let stereo = renderer.frame();
                    for (channel, sample) in frame.iter_mut().enumerate() {
                        let value = if channels == 1 {
                            (stereo[0] + stereo[1]) * 0.5
                        } else if channel < 2 {
                            stereo[channel]
                        } else {
                            0.0
                        };
                        *sample = T::from_sample(value);
                    }
                }
                renderer.end_buffer();
            },
            move |_| {
                errors.error.store(2, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|error| format!("Cannot build selected audio output stream: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_can_be_moved_from_startup_worker_to_ui() {
        fn assert_send<T: Send>() {}
        assert_send::<AudioEngine>();
    }

    #[test]
    fn ring_is_bounded_and_preserves_unconsumed_frames() {
        let ring = FrameRing::new(3);
        assert!(ring.push([0.1, 0.2]));
        assert!(ring.push([0.3, 0.4]));
        assert!(ring.push([0.5, 0.6]));
        assert!(!ring.push([0.7, 0.8]));
        assert_eq!(ring.peek(0), Some([0.1, 0.2]));
        ring.consume(2);
        assert!(ring.push([0.7, 0.8]));
        assert_eq!(ring.available(), 2);
        assert_eq!(ring.peek(0), Some([0.5, 0.6]));
        assert_eq!(ring.peek(1), Some([0.7, 0.8]));
        assert_eq!(ring.peek(2), None);
    }

    #[test]
    fn rendering_applies_gain_and_mute_before_meter() {
        let ring = std::sync::Arc::new(FrameRing::new(128));
        for _ in 0..64 {
            ring.push([0.8, -0.4]);
        }
        let controls = std::sync::Arc::new(Controls::new(0.5, false, 0));
        let mut renderer = Renderer::new(1000, 1000, ring, controls.clone());
        renderer.begin_buffer(4);
        assert_eq!(renderer.frame(), [0.4, -0.2]);
        renderer.end_buffer();
        assert_eq!(
            f32::from_bits(controls.peak.load(std::sync::atomic::Ordering::Relaxed)),
            0.4
        );
        controls
            .muted
            .store(true, std::sync::atomic::Ordering::Relaxed);
        renderer.begin_buffer(4);
        assert_eq!(renderer.frame(), [0.0, 0.0]);
        renderer.end_buffer();
        assert_eq!(
            f32::from_bits(controls.peak.load(std::sync::atomic::Ordering::Relaxed)),
            0.0
        );
    }

    #[test]
    fn positive_delay_waits_for_requested_history() {
        let ring = std::sync::Arc::new(FrameRing::new(1024));
        let controls = std::sync::Arc::new(Controls::new(1.0, false, 100));
        let mut renderer = Renderer::new(1000, 1000, ring.clone(), controls);
        for _ in 0..100 {
            ring.push([0.5, 0.5]);
        }
        renderer.begin_buffer(10);
        assert_eq!(renderer.frame(), [0.0, 0.0]);
        assert_eq!(ring.available(), 100);
        for _ in 0..40 {
            ring.push([0.5, 0.5]);
        }
        renderer.begin_buffer(10);
        assert_eq!(renderer.frame(), [0.5, 0.5]);
    }

    #[test]
    fn rate_conversion_interpolates_without_changing_pitch() {
        let ring = std::sync::Arc::new(FrameRing::new(128));
        for n in 0..60 {
            ring.push([n as f32 / 100.0; 2]);
        }
        let controls = std::sync::Arc::new(Controls::new(1.0, false, 0));
        let mut renderer = Renderer::new(1000, 2000, ring, controls);
        renderer.begin_buffer(10);
        let a = renderer.frame()[0];
        let b = renderer.frame()[0];
        let c = renderer.frame()[0];
        assert!((b - a - 0.005).abs() < 0.0001);
        assert!((c - b - 0.005).abs() < 0.0001);
    }

    #[test]
    fn underrun_is_silent_and_counted_without_repeating_old_audio() {
        let ring = std::sync::Arc::new(FrameRing::new(128));
        let controls = std::sync::Arc::new(Controls::new(1.0, false, 0));
        let mut renderer = Renderer::new(1000, 1000, ring, controls.clone());
        renderer.begin_buffer(10);
        assert_eq!(renderer.frame(), [0.0, 0.0]);
        assert_eq!(
            controls
                .underruns
                .load(std::sync::atomic::Ordering::Relaxed),
            1
        );
    }

    #[test]
    fn nonfinite_samples_and_controls_are_sanitized() {
        assert_eq!(finite_sample(f32::NAN), 0.0);
        assert_eq!(finite_sample(f32::INFINITY), 0.0);
        assert_eq!(finite_sample(-2.0), -1.0);
        let controls = Controls::new(f32::NAN, false, 999);
        assert_eq!(
            f32::from_bits(controls.gain.load(std::sync::atomic::Ordering::Relaxed)),
            0.0
        );
        assert_eq!(
            controls.delay_ms.load(std::sync::atomic::Ordering::Relaxed),
            500
        );
    }

    #[test]
    fn slow_input_clock_difference_does_not_grow_unbounded_backlog() {
        let ring = Arc::new(FrameRing::new(1024));
        let controls = Arc::new(Controls::new(1.0, false, 0));
        let mut renderer = Renderer::new(1000, 1000, ring.clone(), controls.clone());
        for _ in 0..30 {
            ring.push([0.25; 2]);
        }
        let mut incoming = 0.0_f64;
        for _ in 0..30000 {
            incoming += 10.005; // Input clock runs 500 ppm fast for five minutes.
            while incoming >= 1.0 {
                assert!(ring.push([0.25; 2]));
                incoming -= 1.0;
            }
            renderer.begin_buffer(10);
            for _ in 0..10 {
                renderer.frame();
            }
            renderer.end_buffer();
        }
        assert!(ring.available() < 80, "backlog: {}", ring.available());
        assert_eq!(controls.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn decreasing_delay_discards_old_history_at_callback_boundary() {
        let ring = Arc::new(FrameRing::new(1024));
        let controls = Arc::new(Controls::new(1.0, false, 100));
        let mut renderer = Renderer::new(1000, 1000, ring.clone(), controls.clone());
        for _ in 0..150 {
            ring.push([0.1; 2]);
        }
        renderer.begin_buffer(10);
        assert_eq!(renderer.frame(), [0.1; 2]);
        controls.delay_ms.store(0, Ordering::Relaxed);
        renderer.begin_buffer(10);
        assert!(ring.available() <= 22);
    }

    #[test]
    fn overflowing_input_recovers_to_target_instead_of_playing_stale_backlog() {
        let ring = Arc::new(FrameRing::new(1024));
        let controls = Arc::new(Controls::new(1.0, false, 0));
        let mut renderer = Renderer::new(1000, 1000, ring.clone(), controls.clone());
        for _ in 0..30 {
            ring.push([0.1; 2]);
        }
        renderer.begin_buffer(10);
        renderer.frame();
        while ring.push([0.5; 2]) {}
        controls.overruns.store(1, Ordering::Relaxed);
        renderer.begin_buffer(10);
        assert!(
            ring.available() <= 22,
            "stale backlog: {}",
            ring.available()
        );
    }

    #[test]
    fn ambiguous_output_name_never_selects_first_device() {
        let result = select_unique(
            "Speakers",
            false,
            [("Speakers".into(), 1), ("Speakers".into(), 2)],
        );
        assert!(result.unwrap_err().contains("Several audio devices"));
    }

    #[test]
    fn missing_output_does_not_fall_back_to_another_device() {
        let result = select_unique("Headphones", false, [("Speakers".into(), 1)]);
        assert!(
            result
                .unwrap_err()
                .contains("output 'Headphones' is unavailable")
        );
    }

    #[test]
    fn concurrent_callbacks_preserve_frame_pairs_through_wraparound() {
        let ring = Arc::new(FrameRing::new(31));
        let writer = ring.clone();
        let producer = std::thread::spawn(move || {
            for sequence in 0..50000 {
                let frame = [sequence as f32, -(sequence as f32)];
                while !writer.push(frame) {
                    std::thread::yield_now();
                }
            }
        });
        for sequence in 0..50000 {
            let frame = loop {
                if let Some(frame) = ring.peek(0) {
                    break frame;
                }
                std::thread::yield_now();
            };
            assert_eq!(frame, [sequence as f32, -(sequence as f32)]);
            ring.consume(1);
        }
        producer.join().unwrap();
        assert_eq!(ring.available(), 0);
    }

    #[test]
    fn thirty_ms_callback_keeps_all_available_input_and_does_not_underrun() {
        let ring = Arc::new(FrameRing::new(48000));
        let controls = Arc::new(Controls::new(1.0, false, 0));
        let mut renderer = Renderer::new(48000, 48000, ring.clone(), controls.clone());
        for _ in 0..1440 {
            assert!(ring.push([0.25; 2]));
        }
        renderer.begin_buffer(1440);
        for frame in 0..1440 {
            assert_eq!(renderer.frame(), [0.25; 2], "silent frame {frame}");
        }
        assert_eq!(controls.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn long_callback_resamples_without_losing_requested_delay() {
        let ring = Arc::new(FrameRing::new(44100));
        let controls = Arc::new(Controls::new(1.0, false, 100));
        let mut renderer = Renderer::new(44100, 48000, ring.clone(), controls.clone());
        // A 30ms output callback needs 1324 input frames for interpolation;
        // the remaining 4410 input frames supply the requested 100ms delay.
        for _ in 0..5734 {
            assert!(ring.push([0.25; 2]));
        }
        renderer.begin_buffer(1440);
        for _ in 0..1440 {
            assert_eq!(renderer.frame(), [0.25; 2]);
        }
        assert!((4410..=4412).contains(&ring.available()));
        assert_eq!(controls.underruns.load(Ordering::Relaxed), 0);
    }
}

#[cfg(test)]
mod equalizer_pipeline_regressions {
    use super::*;
    #[test]
    fn prepared_equalizer_is_applied_by_renderer_before_output_meter() {
        let ring = Arc::new(FrameRing::new(50000));
        for n in 0..48001 {
            let value = (std::f32::consts::TAU * 1000.0 * n as f32 / 48000.0).sin() * 0.1;
            assert!(ring.push([value, -value]));
        }
        let controls = Arc::new(Controls::new(0.5, false, 0));
        let mut gains = [0.0; 10];
        gains[5] = -12.0;
        controls
            .eq_tx
            .try_send(Equalizer::prepare(48000, true, gains))
            .ok()
            .unwrap();
        let mut renderer = Renderer::new(48000, 48000, ring, controls.clone());
        renderer.begin_buffer(48000);
        let mut power = 0.0;
        for n in 0..48000 {
            let frame = renderer.frame();
            assert!((frame[0] + frame[1]).abs() < 0.000001);
            if n == 24000 {
                renderer.buffer_peak = 0.0;
            }
            if n >= 24000 {
                power += f64::from(frame[0]).powi(2);
            }
        }
        renderer.end_buffer();
        let rms = (power / 24000.0).sqrt();
        assert!((rms - 0.1 * 0.5 * 10.0_f64.powf(-12.0 / 20.0) / 2.0_f64.sqrt()).abs() < 0.0001);
        let peak = f32::from_bits(controls.peak.load(Ordering::Relaxed));
        assert!((peak - 0.1 * 0.5 * 10.0_f32.powf(-12.0 / 20.0)).abs() < 0.0001);
    }
}
