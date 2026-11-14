//! UI-independent application state. Call `poll` from the UI timer, and `changed`
//! after editing settings. Native capture and network operations run on workers.
mod nintendo;
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
    pub fn ready(&self) -> bool {
        let s = &self.settings;
        !self.opening
            && !self.refreshing
            && self
                .devices
                .video
                .iter()
                .any(|d| d.id == s.video_id && s.mode.as_ref().is_some_and(|m| d.modes.contains(m)))
            && ((s.audio_input_id.is_empty() && s.audio_output_id.is_empty())
                || (!self.refreshing_audio
                    && self.devices.inputs.iter().any(|d| d.id == s.audio_input_id)
                    && self
                        .devices
                        .outputs
                        .iter()
                        .any(|d| d.id == s.audio_output_id)))
    }
    pub fn refresh(&mut self) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        self.devices.warnings.clear();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Event::Video(capture::enumerate()));
        });
        if !self.refreshing_audio {
            self.refreshing_audio = true;
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Event::Audio(
                    audio::input_devices(),
                    audio::output_devices(),
                ));
            });
        }
    }
    pub fn refresh_capture_mode(&mut self) {
        if self.active() || self.opening {
            return;
        }
        if let Some(d) = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)
        {
            let chosen = choose_mode(
                &d.modes,
                &self.settings.capture_preferences,
                self.settings.mode.as_ref(),
            );
            if chosen != self.settings.mode {
                self.settings.mode = chosen;
                self.dirty = Some(Instant::now());
            }
        }
    }
    /// Resolution and target FPS are independent UI choices; capture keeps an advertised native rate.
    pub fn select_resolution(&mut self, width: u32, height: u32) {
        self.settings.capture_preferences.automatic = false;
        self.settings.mode = self.mode_for_resolution(width, height);
        self.changed();
    }
    fn mode_for_resolution(&self, width: u32, height: u32) -> Option<capture::VideoMode> {
        let device = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)?;
        let modes: Vec<_> = device
            .modes
            .iter()
            .filter(|m| m.width == width && m.height == height)
            .cloned()
            .collect();
        choose_mode(
            &modes,
            &capture::CapturePreferences {
                automatic: true,
                preferred_fps: self
                    .settings
                    .capture_preferences
                    .preferred_fps
                    .clamp(1, capture_config::MAX_FPS),
            },
            None,
        )
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
    pub fn apply_audio(&self) {
        if let Some(a) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
            a.set_gain(self.settings.gain);
            a.set_muted(self.settings.muted);
            a.set_delay_ms(self.settings.sync_ms.max(0) as u32);
            a.set_equalizer(
                self.settings.equalizer.enabled,
                self.settings.equalizer.gains,
            );
        }
    }
    pub fn connect(&mut self) {
        self.refresh_capture_mode();
        if !self.ready() {
            self.error = Some("Выбери доступную карту, режим и оба устройства звука, либо убери оба аудиоустройства.".into());
            return;
        }
        let mode = self.settings.mode.clone().expect("ready validates mode");
        let recovering = self.reconnect.active();
        self.stop();
        self.recovery_attempt = recovering;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        self.opening = true;
        self.error = None;
        self.notice = None;
        self.capture_dropped.store(0, Ordering::Relaxed);
        self.history_dropped_start = self.history.dropped();
        let settings = self.settings.clone();
        let tx = self.tx.clone();
        let frames = self.frames_tx.clone();
        let discard = self.frames_rx.clone();
        let dropped = self.capture_dropped.clone();
        std::thread::spawn(move || {
            let frame_cancelled = cancelled.clone();
            let sink: FrameSink = Arc::new(move |frame| {
                if !frame_cancelled.load(Ordering::Acquire) {
                    dropped.fetch_add(
                        send_latest(&frames, &discard, (generation, frame)),
                        Ordering::Relaxed,
                    );
                }
            });
            let result = (|| {
                let video = capture::start(&settings.video_id, &mode, sink)?;
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                let audio = if settings.audio_input_id.is_empty() {
                    None
                } else {
                    Some(AudioEngine::start(&AudioConfig {
                        input_id: settings.audio_input_id,
                        output_id: settings.audio_output_id,
                        gain: settings.gain,
                        muted: settings.muted,
                        delay_ms: settings.sync_ms.max(0) as u32,
                        eq_enabled: settings.equalizer.enabled,
                        eq_gains: settings.equalizer.gains,
                    })?)
                };
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                Ok(Active {
                    video,
                    audio,
                    cancelled: cancelled.clone(),
                })
            })();
            let _ = tx.send(Event::Started(generation, result));
        });
    }
    pub fn stop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.generation = self.generation.wrapping_add(1);
        if let Some(active) = self.active.take() {
            std::thread::spawn(move || drop(active));
        }
        self.opening = false;
        self.started = None;
        self.monitor.reset();
        self.history.clear();
        self.displayed = None;
        self.next_frame_deadline = None;
        self.reconnect.cancel();
        self.retry_refresh = false;
        self.recovery_attempt = false;
        while self.frames_rx.try_recv().is_ok() {}
        if let Some(d) = &self.discord {
            d.set(None);
        }
    }
    fn disconnected(&mut self, error: String) {
        self.stop();
        self.error = Some(error);
        self.notice =
            Some("Ожидаю выбранную карту. Отключить отменяет повторное подключение.".into());
        self.reconnect.arm(Instant::now());
    }
    fn current_generation(&self, generation: u64) -> bool {
        generation == self.generation && !self.cancelled.load(Ordering::Acquire)
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
        let fps = self.settings.capture_preferences.preferred_fps.clamp(1, 60);
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
            && (!self.refreshing_audio || self.settings.audio_input_id.is_empty())
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
    pub fn game_name(&self) -> &str {
        if self.settings.nintendo_enabled {
            self.nintendo.game.as_deref().unwrap_or("")
        } else {
            &self.settings.game
        }
    }
    pub fn game_started_at(&self) -> i64 {
        if self.settings.nintendo_enabled {
            self.nintendo.started_at
        } else {
            self.started_unix
        }
    }
    pub fn current_game(&mut self) -> Option<Game> {
        if self.metadata_name != self.game_name() {
            self.metadata_name = self.game_name().to_owned();
            self.metadata = catalog::find_game(self.game_name());
        }
        let mut game = self.metadata.clone();
        if self.settings.nintendo_enabled && !self.game_name().is_empty() {
            let entry = game.get_or_insert_with(|| Game {
                id: String::new(),
                name: self.game_name().into(),
                aliases: vec![],
                cover_url: None,
                game_url: None,
            });
            if let Some(cover) = self
                .nintendo
                .friends
                .iter()
                .find(|f| f.id == self.settings.nintendo_friend_id)
                .and_then(|f| f.cover_url.clone())
            {
                entry.cover_url = Some(cover);
            }
        }
        game
    }
    pub fn cover_url(&mut self) -> Option<String> {
        self.current_game().and_then(|g| g.cover_url)
    }
    pub fn search_games(&self, query: &str) -> Vec<Game> {
        catalog::search_games(query, 20)
    }
    pub fn choose_game(&mut self, game: String) {
        let before = self.game_name().to_owned();
        let previous_started = self.game_started_at();
        self.settings.nintendo_enabled = false;
        self.settings.game = game.chars().filter(|c| !c.is_control()).take(128).collect();
        self.started_unix = if before == self.game_name() {
            previous_started
        } else {
            unix_now()
        };
        self.previous = self.settings.clone();
        self.dirty = Some(Instant::now());
    }
    fn presence(&mut self, linked: bool) -> Option<Presence> {
        if !(linked
            && self.settings.discord_enabled
            && (self.active() || self.settings.nintendo_enabled)
            && !self.game_name().is_empty())
        {
            return None;
        }
        let game = self.current_game();
        Some(Presence {
            application_id: DEFAULT_APPLICATION_ID.into(),
            game: self
                .game_name()
                .chars()
                .filter(|c| !c.is_control())
                .take(128)
                .collect(),
            started_at: self.game_started_at(),
            cover_url: game.as_ref().and_then(|g| g.cover_url.clone()),
            game_url: game.and_then(|g| g.game_url),
        })
    }
    pub fn discord_status(&self) -> AuthStatus {
        self.auth
            .as_ref()
            .map_or(AuthStatus::Disconnected, DiscordAuth::status)
    }
    pub fn discord_account(&self) -> Option<DiscordAccount> {
        self.auth.as_ref().and_then(DiscordAuth::account)
    }
    pub fn presence_status(&self) -> String {
        self.discord
            .as_ref()
            .map_or_else(|| "Отключено".into(), DiscordWorker::status)
    }
    pub fn discord_login(&mut self) {
        if self.persistent {
            if self.auth.is_none() {
                self.auth = Some(DiscordAuth::new(DEFAULT_APPLICATION_ID.into()));
                self.auth_completion = 0;
            }
            if let Some(a) = &self.auth {
                a.connect();
            }
        }
    }
    pub fn discord_logout(&mut self) {
        if let Some(a) = &self.auth {
            a.disconnect();
        }
        self.settings.discord_enabled = false;
        self.changed();
        if let Some(d) = &self.discord {
            d.set(None);
        }
    }
    /// Cancels the pending login without deleting the saved account from the vault.
    pub fn discord_cancel(&mut self) {
        self.auth = None;
        self.auth_completion = 0;
        if let Some(d) = &self.discord {
            d.set(None);
            d.require_account(None);
        }
    }
    pub fn nintendo_friends(&self) -> &[crate::nintendo::Friend] {
        &self.nintendo.friends
    }
    pub fn nintendo_error(&self) -> Option<&str> {
        self.nintendo.error.as_deref()
    }
    pub fn nintendo_pending(&self) -> bool {
        self.nintendo.pending()
    }
    pub fn nintendo_refresh(&mut self) {
        self.nintendo.refresh();
    }
    pub fn select_profile(&mut self, id: String) {
        self.settings.nintendo_friend_id = id;
        self.changed();
    }
    pub fn components_status(&self) -> &ComponentStatus {
        &self.nintendo.status
    }
    pub fn check_components(&mut self) {
        if self.persistent {
            self.nintendo.check_components();
        }
    }
    pub fn install_components(&mut self) {
        if self.persistent {
            self.nintendo.install();
        }
    }
    pub fn cancel_install(&mut self) {
        self.nintendo.cancel();
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
mod tests {
    use super::*;
    fn resolution_runtime() -> Runtime {
        let mut r = Runtime::new(false);
        r.settings.video_id = "card".into();
        r.devices.video.push(VideoDevice {
            id: "card".into(),
            name: "Card".into(),
            modes: vec![
                capture::VideoMode {
                    width: 1920,
                    height: 1080,
                    fps_num: 30,
                    fps_den: 1,
                },
                capture::VideoMode {
                    width: 1920,
                    height: 1080,
                    fps_num: 50,
                    fps_den: 1,
                },
                capture::VideoMode {
                    width: 1280,
                    height: 720,
                    fps_num: 60000,
                    fps_den: 1001,
                },
                capture::VideoMode {
                    width: 1280,
                    height: 720,
                    fps_num: 30000,
                    fps_den: 1001,
                },
            ],
        });
        r
    }
    #[test]
    fn resolution_choice_uses_advertised_rate_and_fps_changes_keep_resolution() {
        let mut r = resolution_runtime();
        r.select_resolution(1920, 1080);
        assert!(!r.settings.capture_preferences.automatic);
        assert_eq!(r.settings.mode.as_ref().unwrap().fps_num, 50);
        r.settings.capture_preferences.preferred_fps = 30;
        r.changed();
        assert_eq!(r.settings.mode.as_ref().unwrap().fps_num, 30);
        r.settings.capture_preferences.preferred_fps = 60;
        r.changed();
        let mode = r.settings.mode.as_ref().unwrap();
        assert_eq!(
            (mode.width, mode.height, mode.fps_num, mode.fps_den),
            (1920, 1080, 50, 1)
        );
        assert!(r.devices.video[0].modes.contains(mode));
        r.select_resolution(3840, 2160);
        assert!(r.settings.mode.is_none());
    }
    #[test]
    fn manual_resolution_target_changes_preserve_native_fraction() {
        let mut r = resolution_runtime();
        r.select_resolution(1280, 720);
        let mode = r.settings.mode.as_ref().unwrap();
        assert_eq!((mode.fps_num, mode.fps_den), (60000, 1001));
        r.settings.capture_preferences.preferred_fps = 30;
        r.changed();
        let mode = r.settings.mode.as_ref().unwrap();
        assert_eq!(
            (mode.width, mode.height, mode.fps_num, mode.fps_den),
            (1280, 720, 30000, 1001)
        );
        assert!(r.devices.video[0].modes.contains(mode));
    }
    #[test]
    fn ten_ms_polls_keep_sixty_fps_phase_and_skip_stalled_slots() {
        let mut deadline = None;
        let mut count = 0;
        for ms in (0..1000u128).step_by(10) {
            let now = ms * 1_000_000 * 60;
            if deadline.is_none_or(|d| now >= d) {
                count += 1;
                deadline = Some(next_deadline(deadline, now));
            }
        }
        assert_eq!(count, 60);
        let stalled = 10_000_000_000u128 * 60;
        assert!(next_deadline(deadline, stalled) > stalled);
    }
    #[test]
    fn isolated_runtime_and_late_capture_events_do_not_restart_session() {
        let mut r = Runtime::new(false);
        assert!(r.store.is_none() && r.auth.is_none() && r.discord.is_none());
        r.cancelled = Arc::new(AtomicBool::new(false));
        r.generation = 7;
        r.opening = true;
        r.stop();
        r.tx.send(Event::Started(7, Err("late error".into())))
            .unwrap();
        r.frames_tx
            .send((
                7,
                VideoFrame {
                    width: 1,
                    height: 1,
                    rgba: vec![0; 4],
                    timestamp_ns: monotonic_ns(),
                },
            ))
            .unwrap();
        assert!(r.poll().is_none());
        assert!(!r.active() && !r.opening);
        assert!(r.error.is_none());
        assert_eq!(r.history.bytes(), 0);
    }
    #[test]
    fn presence_requires_account_and_current_game_and_obeys_disable() {
        let mut r = Runtime::new(false);
        r.settings.discord_enabled = true;
        r.settings.nintendo_enabled = true;
        r.nintendo.game = Some("Current game".into());
        r.nintendo.started_at = 123;
        assert!(r.presence(false).is_none());
        let p = r.presence(true).unwrap();
        assert_eq!(p.game, "Current game");
        assert_eq!(p.started_at, 123);
        r.nintendo.game = None;
        assert!(r.presence(true).is_none());
        r.settings.discord_enabled = false;
        assert!(r.presence(true).is_none());
    }
    #[test]
    fn choosing_same_game_preserves_timer() {
        let mut r = Runtime::new(false);
        r.choose_game("Test".into());
        r.started_unix = 123;
        r.choose_game("Test".into());
        assert_eq!(r.game_started_at(), 123);
        r.choose_game("Other".into());
        assert_ne!(r.game_started_at(), 123);
    }
    #[test]
    fn same_title_manual_override_keeps_nintendo_elapsed_time() {
        let mut r = Runtime::new(false);
        r.settings.nintendo_enabled = true;
        r.nintendo.game = Some("Current game".into());
        r.nintendo.started_at = 123;
        r.choose_game("Current game".into());
        assert!(!r.settings.nintendo_enabled);
        assert_eq!(r.game_started_at(), 123);
    }
    #[test]
    fn close_reaps_queued_native_session_off_ui_thread() {
        struct BlockedDrop {
            entered: std::sync::mpsc::Sender<()>,
            release: std::sync::mpsc::Receiver<()>,
        }
        impl CaptureSession for BlockedDrop {}
        impl Drop for BlockedDrop {
            fn drop(&mut self) {
                let _ = self.entered.send(());
                let _ = self.release.recv();
            }
        }
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let mut r = Runtime::new(false);
        r.tx.send(Event::Started(
            0,
            Ok(Active {
                video: Box::new(BlockedDrop {
                    entered: entered_tx,
                    release: release_rx,
                }),
                audio: None,
                cancelled: Arc::new(AtomicBool::new(false)),
            }),
        ))
        .unwrap();
        r.close();
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        release_tx.send(()).unwrap();
    }
    #[test]
    fn screenshot_roundtrip_preserves_alpha_and_srgb_metadata() {
        let directory =
            std::env::temp_dir().join(format!("console-link-runtime-png-{}", std::process::id()));
        let frame = VideoFrame {
            width: 2,
            height: 1,
            rgba: vec![25, 100, 200, 0, 255, 80, 0, 137],
            timestamp_ns: 0,
        };
        let path = save_screenshot(&frame, &directory).unwrap();
        let decoder =
            png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
        let mut reader = decoder.read_info().unwrap();
        assert_eq!(
            reader.info().srgb,
            Some(png::SrgbRenderingIntent::Perceptual)
        );
        let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut bytes).unwrap();
        assert_eq!(&bytes[..info.buffer_size()], &frame.rgba);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn screenshot_rejects_invalid_frame_before_creating_directory() {
        let path =
            std::env::temp_dir().join(format!("console-link-invalid-frame-{}", std::process::id()));
        assert!(
            save_screenshot(
                &VideoFrame {
                    width: 2,
                    height: 2,
                    rgba: vec![0; 4],
                    timestamp_ns: 0
                },
                &path
            )
            .is_err()
        );
        assert!(!path.exists());
    }
}
