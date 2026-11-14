//! UI-independent application state. Call `poll` from the UI timer, and `changed`
//! after editing settings. Native capture and network operations run on workers.
mod devices;
mod nintendo;
mod presence;
mod session;
pub use nintendo::ComponentStatus;

use crate::{
    audio::{self, AudioConfig, AudioDeviceInfo, AudioEngine},
    capture::{
        self, CaptureSession, FrameHistory, FrameSink, RetryPolicy, VideoDevice, VideoFrame,
        choose_mode, monotonic_ns, send_latest,
    },
    catalog::{self, Game},
    config::{app as config, capture as capture_config, discord::DEFAULT_APPLICATION_ID},
    discord::{
        DiscordWorker, Presence,
        auth::{AuthStatus, DiscordAccount, DiscordAuth},
    },
    monitoring::Monitor,
    settings::{Settings, SettingsStore},
};
use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Default)]
pub struct Devices {
    pub video: Vec<VideoDevice>,
    pub inputs: Vec<AudioDeviceInfo>,
    pub outputs: Vec<AudioDeviceInfo>,
    pub warnings: Vec<String>,
}
struct Active {
    video: Box<dyn CaptureSession>,
    audio: Option<AudioEngine>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Active {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}
enum Event {
    Video(Result<Vec<VideoDevice>, String>),
    Audio(
        Result<Vec<AudioDeviceInfo>, String>,
        Result<Vec<AudioDeviceInfo>, String>,
    ),
    Started(u64, Result<Active, String>),
    Screenshot(Result<String, String>),
}

pub struct Runtime {
    pub settings: Settings,
    pub devices: Devices,
    pub notice: Option<String>,
    pub error: Option<String>,
    pub opening: bool,
    pub refreshing: bool,
    pub refreshing_audio: bool,
    pub fps: f64,
    pub monitor: Monitor,
    store: Option<SettingsStore>,
    persistent: bool,
    dirty: Option<Instant>,
    previous: Settings,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    frames_tx: Sender<(u64, VideoFrame)>,
    frames_rx: Receiver<(u64, VideoFrame)>,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    active: Option<Active>,
    history: FrameHistory,
    displayed: Option<Arc<VideoFrame>>,
    capture_dropped: Arc<AtomicU64>,
    history_dropped_start: u64,
    received: u64,
    rate_since: Instant,
    last_frame: Instant,
    next_frame_deadline: Option<u128>,
    pacing_fps: u32,
    started: Option<Instant>,
    started_unix: i64,
    reconnect: RetryPolicy,
    retry_refresh: bool,
    recovery_attempt: bool,
    discord: Option<DiscordWorker>,
    auth: Option<DiscordAuth>,
    auth_completion: u64,
    nintendo: nintendo::Nintendo,
    screenshot_pending: bool,
    metadata_name: String,
    metadata: Option<Game>,
}

pub(super) fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
impl Runtime {
    /// `false` never opens user settings/keychain, starts IPC or initiates network traffic.
    pub fn new(persistent: bool) -> Self {
        let (store, settings, notice) = if persistent {
            let (s, v, n) = SettingsStore::open();
            (Some(s), v, n)
        } else {
            (None, Settings::default(), None)
        };
        let (tx, rx) = unbounded();
        let (frames_tx, frames_rx) = bounded(config::FRAME_QUEUE_CAPACITY);
        let now = Instant::now();
        let mut runtime = Self {
            previous: settings.clone(),
            settings,
            store,
            notice,
            persistent,
            error: None,
            devices: Devices::default(),
            opening: false,
            refreshing: false,
            refreshing_audio: false,
            fps: 0.0,
            monitor: Monitor::default(),
            dirty: None,
            tx,
            rx,
            frames_tx,
            frames_rx,
            generation: 0,
            cancelled: Arc::new(AtomicBool::new(true)),
            active: None,
            history: FrameHistory::new(
                capture_config::HISTORY_BUDGET,
                capture_config::HISTORY_TOLERANCE_NS,
            ),
            displayed: None,
            capture_dropped: Arc::new(AtomicU64::new(0)),
            history_dropped_start: 0,
            received: 0,
            rate_since: now,
            last_frame: now,
            next_frame_deadline: None,
            pacing_fps: 0,
            started: None,
            started_unix: unix_now(),
            reconnect: RetryPolicy::default(),
            retry_refresh: false,
            recovery_attempt: false,
            discord: persistent.then(DiscordWorker::new),
            auth: None,
            auth_completion: 0,
            nintendo: nintendo::Nintendo::default(),
            screenshot_pending: false,
            metadata_name: String::new(),
            metadata: None,
        };
        if persistent {
            runtime.auth = Some(DiscordAuth::new(DEFAULT_APPLICATION_ID.into()));
            runtime.refresh();
        }
        runtime
    }
    pub fn active(&self) -> bool {
        self.active.is_some()
    }
    pub fn reconnecting(&self) -> bool {
        self.reconnect.active()
    }
    pub fn displayed(&self) -> Option<Arc<VideoFrame>> {
        self.displayed.clone()
    }
    pub fn peak(&self) -> f32 {
        self.active
            .as_ref()
            .and_then(|a| a.audio.as_ref())
            .map_or(0.0, AudioEngine::peak)
    }
    pub fn capture_dropped(&self) -> u64 {
        self.capture_dropped.load(Ordering::Relaxed)
    }
    pub fn history_dropped(&self) -> u64 {
        self.history
            .dropped()
            .saturating_sub(self.history_dropped_start)
    }
    pub fn history_bytes(&self) -> usize {
        self.history.bytes()
    }
    pub fn dropped_frames(&self) -> u64 {
        self.capture_dropped() + self.history_dropped()
    }
    pub fn audio_counters(&self) -> Option<(u64, u64)> {
        self.active
            .as_ref()
            .and_then(|a| a.audio.as_ref())
            .map(|a| (a.underruns(), a.overruns()))
    }
    pub fn capture_statistics(&self) -> Option<capture::CaptureStatistics> {
        self.active.as_ref().and_then(|a| a.video.statistics())
    }
    pub fn session_elapsed(&self) -> Duration {
        self.started.map_or(Duration::ZERO, |t| t.elapsed())
    }
    pub fn changed(&mut self) {
        self.settings.gain = if self.settings.gain.is_finite() {
            self.settings.gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.settings.sync_ms = self
            .settings
            .sync_ms
            .clamp(-capture_config::MAX_SYNC_MS, capture_config::MAX_SYNC_MS);
        self.settings.capture_preferences.preferred_fps = self
            .settings
            .capture_preferences
            .preferred_fps
            .clamp(1, capture_config::MAX_FPS);
        if !self.settings.capture_preferences.automatic
            && self.previous.capture_preferences.preferred_fps
                != self.settings.capture_preferences.preferred_fps
            && let Some(mode) = &self.settings.mode
        {
            self.settings.mode = self.mode_for_resolution(mode.width, mode.height);
        }
        self.settings.equalizer.gains =
            audio::equalizer::clean_gains(self.settings.equalizer.gains);
        self.settings.color.normalize();
        self.settings.upscale.normalize();
        if self.previous.sync_ms != self.settings.sync_ms {
            self.history.clear();
            self.displayed = None;
        }
        if self.previous.game != self.settings.game {
            self.started_unix = unix_now();
        }
        if self.previous.nintendo_enabled != self.settings.nintendo_enabled
            || self.previous.nintendo_friend_id != self.settings.nintendo_friend_id
        {
            self.nintendo.poll(
                self.settings.nintendo_enabled,
                &mut self.settings.nintendo_friend_id,
                false,
            );
        }
        self.refresh_capture_mode();
        self.apply_audio();
        self.dirty = Some(Instant::now());
        self.previous = self.settings.clone();
    }
    pub fn poll(&mut self) -> Option<Arc<VideoFrame>> {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Video(result) => {
                    self.refreshing = false;
                    self.devices.video = result.unwrap_or_else(|e| {
                        self.devices.warnings.push(e);
                        vec![]
                    });
                    self.refresh_capture_mode();
                }
                Event::Audio(inputs, outputs) => {
                    self.refreshing_audio = false;
                    self.devices.inputs = inputs.unwrap_or_else(|e| {
                        self.devices.warnings.push(e);
                        vec![]
                    });
                    self.devices.outputs = outputs.unwrap_or_else(|e| {
                        self.devices.warnings.push(e);
                        vec![]
                    });
                }
                Event::Started(generation, result) => {
                    if !self.current_generation(generation) {
                        if let Ok(active) = result {
                            std::thread::spawn(move || drop(active));
                        }
                        continue;
                    }
                    self.opening = false;
                    match result {
                        Ok(active) => {
                            self.active = Some(active);
                            self.apply_audio();
                            self.started = Some(Instant::now());
                            self.started_unix = unix_now();
                            self.last_frame = Instant::now();
                            self.rate_since = Instant::now();
                            self.received = 0;
                            self.fps = 0.0;
                            self.reconnect.cancel();
                            self.recovery_attempt = false;
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.cancelled.store(true, Ordering::Release);
                            self.history.clear();
                            if self.recovery_attempt {
                                self.reconnect.arm(Instant::now());
                                self.reconnect.failed(Instant::now());
                            }
                        }
                    }
                }
                Event::Screenshot(result) => {
                    self.screenshot_pending = false;
                    match result {
                        Ok(p) => self.notice = Some(format!("Скриншот сохранен: {p}")),
                        Err(e) => self.error = Some(format!("Не удалось сохранить скриншот: {e}")),
                    }
                }
            }
        }
        if self.retry_refresh && self.reconnect.active() {
            if self.ready() {
                self.retry_refresh = false;
                self.connect();
            } else if !self.refreshing && !self.refreshing_audio {
                self.retry_refresh = false;
            }
        }
        let delay = (-self.settings.sync_ms.min(0)) as u64 * 1_000_000;
        self.history.set_retention_ns(delay + 60_000_000);
        while let Ok((generation, frame)) = self.frames_rx.try_recv() {
            if self.current_generation(generation) {
                let fps = self
                    .settings
                    .mode
                    .as_ref()
                    .map_or(60.0, |m| m.fps_num as f64 / m.fps_den.max(1) as f64);
                let max_delay = self.history.max_delay_ms(frame.rgba.len(), fps) as i32;
                if self.settings.sync_ms < -max_delay {
                    self.settings.sync_ms = -max_delay;
                    self.notice = Some(format!(
                        "Задержка видео ограничена {max_delay} мс по бюджету памяти."
                    ));
                    self.dirty = Some(Instant::now());
                }
                self.history.push(Arc::new(frame));
                self.last_frame = Instant::now();
                self.received += 1;
            }
        }
        if self.rate_since.elapsed() >= Duration::from_secs(1) {
            self.fps = self.received as f64 / self.rate_since.elapsed().as_secs_f64();
            self.received = 0;
            self.rate_since = Instant::now();
        }
        let now = monotonic_ns();
        let fps = self
            .settings
            .capture_preferences
            .preferred_fps
            .clamp(1, capture_config::MAX_FPS);
        if self.pacing_fps != fps {
            self.next_frame_deadline = None;
            self.pacing_fps = fps;
        }
        let now_scaled = u128::from(now) * u128::from(fps);
        let mut new_frame = None;
        if self.active()
            && self
                .next_frame_deadline
                .is_none_or(|deadline| now_scaled + 500_000 * u128::from(fps) >= deadline)
            && let Some(frame) = self.history.select(now, delay)
        {
            // Preserve clock phase across 10 ms polls instead of reducing 60 FPS to 50.
            self.next_frame_deadline = Some(next_deadline(self.next_frame_deadline, now_scaled));
            self.displayed = Some(frame.clone());
            new_frame = Some(frame);
        }
        let stream_error = self.active.as_ref().and_then(|a| {
            a.video
                .error()
                .or_else(|| a.audio.as_ref().and_then(AudioEngine::error))
        });
        if let Some(e) = stream_error {
            self.disconnected(e);
            new_frame = None;
        } else if self.active() && self.last_frame.elapsed() > config::SIGNAL_TIMEOUT {
            self.disconnected("Кадры не поступают. Проверь USB и HDMI.".into());
            new_frame = None;
        }
        if self.reconnect.due(Instant::now())
            && !self.opening
            && !self.refreshing
            && (!self.settings.audio_enabled
                || !self.refreshing_audio
                || self.settings.audio_input_id.is_empty())
        {
            self.reconnect.failed(Instant::now());
            self.retry_refresh = true;
            self.refresh();
        }
        if let Some(started) = self.started {
            self.monitor
                .record(started.elapsed(), self.fps, self.peak());
        }
        if self.nintendo.poll(
            self.settings.nintendo_enabled,
            &mut self.settings.nintendo_friend_id,
            self.persistent,
        ) {
            self.dirty = Some(Instant::now());
        }
        if let Some(auth) = &self.auth {
            let generation = auth.completion_generation();
            if generation != self.auth_completion {
                self.auth_completion = generation;
                self.settings.discord_enabled = true;
                self.dirty = Some(Instant::now());
            }
        }
        let account = self.discord_account();
        let presence = self.presence(account.is_some());
        if let Some(d) = &self.discord {
            d.require_account(account.map(|a| a.id));
            d.set(presence);
        }
        if self
            .dirty
            .is_some_and(|t| t.elapsed() >= config::SETTINGS_SAVE_DELAY)
        {
            self.flush_settings();
        }
        new_frame
    }
    pub fn screenshot_pending(&self) -> bool {
        self.screenshot_pending
    }
    /// Saves the exact supplied RGBA image. GPU callers pass their processed export.
    pub fn save_frame_screenshot(&mut self, frame: Arc<VideoFrame>) {
        if self.screenshot_pending {
            return;
        }
        self.screenshot_pending = true;
        let directory = self.settings.screenshot_dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Screenshot(save_screenshot(&frame, &directory)));
        });
    }
    pub fn flush_settings(&mut self) {
        if self.dirty.take().is_some()
            && let Some(store) = &self.store
            && let Err(e) = store.save(&self.settings)
        {
            self.notice = Some(e);
        }
    }
    pub fn close(&mut self) {
        self.stop();
        self.nintendo.cancel();
        self.discord_cancel();
        self.flush_settings();
        // A completed startup may already be queued; destroy native sessions off the UI thread.
        let (_, empty) = unbounded();
        let pending = std::mem::replace(&mut self.rx, empty);
        std::thread::spawn(move || {
            for event in pending {
                drop(event);
            }
        });
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        self.close();
    }
}

fn next_deadline(previous: Option<u128>, now_scaled: u128) -> u128 {
    match previous {
        Some(deadline) => {
            deadline + (now_scaled.saturating_sub(deadline) / 1_000_000_000 + 1) * 1_000_000_000
        }
        None => now_scaled + 1_000_000_000,
    }
}

pub(crate) fn save_screenshot(
    frame: &VideoFrame,
    directory: &std::path::Path,
) -> Result<String, String> {
    if frame.width == 0
        || frame.height == 0
        || (frame.width as usize)
            .checked_mul(frame.height as usize)
            .and_then(|p| p.checked_mul(4))
            != Some(frame.rgba.len())
    {
        return Err("Некорректный кадр.".into());
    }
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path: PathBuf = directory.join(format!("Console-Link-{stamp}.png"));
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer
        .write_image_data(&frame.rgba)
        .map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

#[cfg(test)]
mod tests;
