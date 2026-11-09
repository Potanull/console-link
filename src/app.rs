use crate::{
    playback::{self, FramePacer, GameFeedback, PlaybackState},
    shortcuts::{self, Action},
    theme::{self, Icon},
};
use console_link::{
    audio::{self, AudioConfig, AudioDeviceInfo, AudioEngine},
    discord::{DiscordWorker, Presence},
    discord_auth::{AuthStatus, DEFAULT_APPLICATION_ID, DiscordAuth},
    native,
    scheduler::FrameHistory,
    session::{RetryPolicy, send_latest},
    settings::{GameWindowMode, Settings, SettingsStore},
    types::{CaptureSession, FrameSink, VideoDevice, VideoFrame, monotonic_ns},
    video_policy::{choose_mode, nominal_fps},
};
use crossbeam_channel::{Receiver, Sender, bounded};
use eframe::egui::{self, Color32, RichText};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Session,
    Audio,
    Discord,
    Settings,
}

const HISTORY_BUDGET: usize = 320 * 1024 * 1024;

#[derive(Clone, Copy)]
struct WindowGeometry {
    position: egui::Pos2,
    size: egui::Vec2,
}
impl WindowGeometry {
    fn current(ctx: &egui::Context) -> Option<Self> {
        ctx.input(|input| {
            let viewport = input.viewport();
            Some(Self {
                position: viewport.outer_rect?.min,
                size: viewport.inner_rect?.size(),
            })
        })
    }
}

struct Devices {
    video: Vec<VideoDevice>,
    inputs: Vec<AudioDeviceInfo>,
    outputs: Vec<AudioDeviceInfo>,
    warnings: Vec<String>,
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
    Devices(Result<Devices, String>),
    Started(u64, Result<Active, String>),
    Screenshot(Result<String, String>),
}

pub struct ConsoleApp {
    settings: Settings,
    store: Option<SettingsStore>,
    devices: Devices,
    tx: Sender<Event>,
    rx: Receiver<Event>,
    frames_tx: Sender<(u64, VideoFrame)>,
    frames_rx: Receiver<(u64, VideoFrame)>,
    active: Option<Active>,
    cancelled: Arc<AtomicBool>,
    generation: u64,
    opening: bool,
    refreshing: bool,
    history: FrameHistory,
    displayed: Option<Arc<VideoFrame>>,
    texture: Option<egui::TextureHandle>,
    notice: Option<String>,
    error: Option<String>,
    last_frame: Instant,
    started: Instant,
    started_unix: i64,
    playback: PlaybackState,
    frame_pacer: FramePacer,
    feedback: GameFeedback,
    menu_geometry: Option<WindowGeometry>,
    game_geometry: Option<WindowGeometry>,
    pending_geometry: Option<WindowGeometry>,
    native_mode: Option<GameWindowMode>,
    snapshot_game: bool,
    snapshot_cycle: bool,
    snapshot_cycle_closed: bool,
    discord: DiscordWorker,
    discord_auth: Option<DiscordAuth>,
    dirty: Option<Instant>,
    screenshot_pending: bool,
    capture_dropped: Arc<AtomicU64>,
    received: u64,
    rate_since: Instant,
    fps: f64,
    reconnect: RetryPolicy,
    retry_refresh: bool,
    recovery_attempt: bool,
    page: Page,
    brand: egui::TextureHandle,
    snapshot_path: Option<std::path::PathBuf>,
    snapshot_requested: bool,
}

impl ConsoleApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        theme::install(&context.egui_ctx);
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/window-icon.png"))
            .expect("embedded icon");
        let brand = context.egui_ctx.load_texture(
            "console-link-brand",
            egui::ColorImage::from_rgba_unmultiplied(
                [icon.width as usize, icon.height as usize],
                &icon.rgba,
            ),
            egui::TextureOptions::LINEAR,
        );
        let snapshot_path = argument_value("--ui-snapshot").map(std::path::PathBuf::from);
        let snapshot_cycle =
            snapshot_path.is_some() && std::env::args().any(|arg| arg == "--ui-game-cycle");
        let snapshot_game = snapshot_path.is_some()
            && matches!(
                argument_value("--ui-page").as_deref(),
                Some("overlay" | "gameplay" | "volume")
            );
        let snapshot_game = snapshot_game || snapshot_cycle;
        let page = match argument_value("--ui-page").as_deref() {
            Some("overlay") => Page::Audio,
            Some("audio") => Page::Audio,
            Some("discord") => Page::Discord,
            Some("settings") => Page::Settings,
            _ => Page::Session,
        };
        let (store, settings, warning) = if snapshot_path.is_some() {
            (None, Settings::default(), None)
        } else {
            let (store, settings, warning) = SettingsStore::open();
            (Some(store), settings, warning)
        };
        let (tx, rx) = bounded(16);
        let (frames_tx, frames_rx) = bounded(3);
        let discord = DiscordWorker::new();
        discord.require_account(None);
        let discord_auth = snapshot_path
            .is_none()
            .then(|| DiscordAuth::new(DEFAULT_APPLICATION_ID.to_owned()));
        let mut app = Self {
            settings,
            store,
            devices: Devices {
                video: vec![],
                inputs: vec![],
                outputs: vec![],
                warnings: vec![],
            },
            tx,
            rx,
            frames_tx,
            frames_rx,
            active: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            generation: 0,
            opening: false,
            refreshing: false,
            history: FrameHistory::new(HISTORY_BUDGET, 50_000_000),
            displayed: None,
            texture: None,
            notice: warning,
            error: None,
            last_frame: Instant::now(),
            started: Instant::now(),
            started_unix: 0,
            playback: PlaybackState::default(),
            frame_pacer: FramePacer::default(),
            feedback: GameFeedback::default(),
            menu_geometry: None,
            game_geometry: None,
            pending_geometry: None,
            native_mode: None,
            snapshot_game,
            snapshot_cycle,
            snapshot_cycle_closed: false,
            discord,
            discord_auth,
            dirty: None,
            screenshot_pending: false,
            capture_dropped: Arc::new(AtomicU64::new(0)),
            received: 0,
            rate_since: Instant::now(),
            fps: 0.0,
            reconnect: RetryPolicy::default(),
            retry_refresh: false,
            recovery_attempt: false,
            page,
            brand,
            snapshot_path,
            snapshot_requested: false,
        };
        if argument_value("--ui-page").as_deref() == Some("volume") && app.snapshot_path.is_some() {
            app.feedback.volume_changed(0.0);
        }
        if snapshot_game {
            app.settings.game_window_mode = match argument_value("--ui-game-mode").as_deref() {
                Some("fullscreen") => GameWindowMode::Fullscreen,
                Some("borderless") => GameWindowMode::Borderless,
                _ => GameWindowMode::Windowed,
            };
            app.playback.overlay_open = argument_value("--ui-page").as_deref() == Some("overlay");
        }
        app.refresh(context.egui_ctx.clone());
        app
    }

    fn refresh(&mut self, ctx: egui::Context) {
        if self.refreshing {
            return;
        }
        self.refreshing = true;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let mut devices = Devices {
                video: vec![],
                inputs: vec![],
                outputs: vec![],
                warnings: vec![],
            };
            match native::enumerate() {
                Ok(video) => devices.video = video,
                Err(e) => devices.warnings.push(e),
            }
            match audio::input_devices() {
                Ok(inputs) => devices.inputs = inputs,
                Err(e) => devices.warnings.push(e),
            }
            match audio::output_devices() {
                Ok(outputs) => devices.outputs = outputs,
                Err(e) => devices.warnings.push(e),
            }
            let _ = tx.send(Event::Devices(Ok(devices)));
            ctx.request_repaint();
        });
    }

    fn refresh_capture_mode(&mut self) {
        if self.active.is_some() || self.opening {
            return;
        }
        if let Some(device) = self
            .devices
            .video
            .iter()
            .find(|device| device.id == self.settings.video_id)
        {
            let chosen = choose_mode(
                &device.modes,
                &self.settings.capture_preferences,
                self.settings.mode.as_ref(),
            );
            if chosen != self.settings.mode {
                self.settings.mode = chosen;
                self.dirty = Some(Instant::now());
            }
        }
    }

    fn connect(&mut self, ctx: egui::Context) {
        self.refresh_capture_mode();
        let Some(device) = self
            .devices
            .video
            .iter()
            .find(|d| d.id == self.settings.video_id)
        else {
            self.error = Some("Выбери карту захвата.".into());
            return;
        };
        let Some(mode) = self
            .settings
            .mode
            .clone()
            .filter(|m| device.modes.contains(m))
        else {
            self.error = Some("Выбери доступный режим карты.".into());
            return;
        };
        if self.settings.audio_input_id.is_empty() != self.settings.audio_output_id.is_empty() {
            self.error = Some("Для звука выбери и вход карты, и аудиовыход. Для видео без звука убери оба выбора.".into());
            return;
        }
        let recovering = self.reconnect.active();
        self.stop();
        self.recovery_attempt = recovering;
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::clone(&self.cancelled);
        self.opening = true;
        self.error = None;
        self.notice = None;
        self.capture_dropped.store(0, Ordering::Relaxed);
        let frame_tx = self.frames_tx.clone();
        let discard_frames = self.frames_rx.clone();
        let tx = self.tx.clone();
        let settings = self.settings.clone();
        let dropped = Arc::clone(&self.capture_dropped);
        std::thread::spawn(move || {
            let frame_cancelled = Arc::clone(&cancelled);
            let paint = ctx.clone();
            let sink: FrameSink = Arc::new(move |frame| {
                if frame_cancelled.load(Ordering::Acquire) {
                    return;
                }
                dropped.fetch_add(
                    send_latest(&frame_tx, &discard_frames, (generation, frame)),
                    Ordering::Relaxed,
                );
                paint.request_repaint();
            });
            let result = (|| {
                let video = native::start(&settings.video_id, &mode, sink)?;
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
                    })?)
                };
                if cancelled.load(Ordering::Acquire) {
                    return Err("Подключение отменено.".into());
                }
                Ok(Active {
                    video,
                    audio,
                    cancelled: Arc::clone(&cancelled),
                })
            })();
            let _ = tx.send(Event::Started(generation, result));
            ctx.request_repaint();
        });
    }

    fn stop(&mut self) {
        self.frame_pacer.reset();
        self.reconnect.cancel();
        self.retry_refresh = false;
        self.recovery_attempt = false;
        self.cancelled.store(true, Ordering::Release);
        if let Some(active) = self.active.take() {
            std::thread::spawn(move || drop(active));
        }
        self.opening = false;
        self.history.clear();
        self.displayed = None;
        self.texture = None;
        while self.frames_rx.try_recv().is_ok() {}
        self.discord.set(None);
    }

    fn disconnected(&mut self, error: String) {
        self.stop();
        self.error = Some(error);
        self.notice = Some(
            "Ожидаю выбранную карту и устройства звука. Отключить отменяет повторное подключение."
                .into(),
        );
        self.reconnect.arm(Instant::now());
    }

    fn events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::Devices(result) => {
                    self.refreshing = false;
                    match result {
                        Ok(devices) => {
                            self.devices = devices;
                            self.refresh_capture_mode();
                            if !self.settings.video_id.is_empty()
                                && !self
                                    .devices
                                    .video
                                    .iter()
                                    .any(|d| d.id == self.settings.video_id)
                            {
                                self.notice = Some(
                                    "Сохраненная карта не найдена. Подключи ее и обнови список."
                                        .into(),
                                );
                            }
                        }
                        Err(e) => self.error = Some(e),
                    }
                    if self.retry_refresh && self.reconnect.active() {
                        self.retry_refresh = false;
                        let video_ready = self.devices.video.iter().any(|d| {
                            d.id == self.settings.video_id
                                && self
                                    .settings
                                    .mode
                                    .as_ref()
                                    .is_some_and(|m| d.modes.contains(m))
                        });
                        let audio_ready = self.settings.audio_input_id.is_empty()
                            || (self
                                .devices
                                .inputs
                                .iter()
                                .any(|d| d.id == self.settings.audio_input_id)
                                && self
                                    .devices
                                    .outputs
                                    .iter()
                                    .any(|d| d.id == self.settings.audio_output_id));
                        if video_ready && audio_ready {
                            self.connect(ctx.clone());
                        }
                    }
                }
                Event::Started(generation, result) => {
                    if generation != self.generation || self.cancelled.load(Ordering::Acquire) {
                        if let Ok(active) = result {
                            std::thread::spawn(move || drop(active));
                        }
                        continue;
                    }
                    self.opening = false;
                    match result {
                        Ok(active) => {
                            if let Some(audio) = &active.audio {
                                audio.set_gain(self.settings.gain);
                                audio.set_muted(self.settings.muted);
                                audio.set_delay_ms(self.settings.sync_ms.max(0) as u32);
                            }
                            self.reconnect.cancel();
                            self.recovery_attempt = false;
                            self.active = Some(active);
                            self.started = Instant::now();
                            self.last_frame = Instant::now();
                            self.started_unix = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64;
                            self.rate_since = Instant::now();
                            self.received = 0;
                        }
                        Err(e) => {
                            self.cancelled.store(true, Ordering::Release);
                            self.error = Some(e);
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
                        Ok(path) => self.notice = Some(format!("Скриншот сохранен: {path}")),
                        Err(e) => self.error = Some(format!("Не удалось сохранить скриншот: {e}")),
                    }
                }
            }
        }
        let delay_ns = (-self.settings.sync_ms.min(0)) as u64 * 1_000_000;
        self.history.set_retention_ns(delay_ns + 60_000_000);
        while let Ok((generation, frame)) = self.frames_rx.try_recv() {
            if generation == self.generation && !self.cancelled.load(Ordering::Acquire) {
                let fps = self
                    .settings
                    .mode
                    .as_ref()
                    .map(|m| m.fps_num as f64 / m.fps_den.max(1) as f64)
                    .unwrap_or(60.0);
                let max_delay = self.history.max_delay_ms(frame.rgba.len(), fps) as i32;
                if self.settings.sync_ms < -max_delay {
                    self.settings.sync_ms = -max_delay;
                    self.notice = Some(format!(
                        "Задержка видео ограничена {max_delay} мс по доступному бюджету памяти."
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
        let now_ns = monotonic_ns();
        if self.active.is_some()
            && self
                .frame_pacer
                .ready(now_ns, self.settings.capture_preferences.preferred_fps)
            && let Some(frame) = self.history.select(now_ns, delay_ns)
        {
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [frame.width as usize, frame.height as usize],
                &frame.rgba,
            );
            if let Some(texture) = &mut self.texture {
                texture.set(image, egui::TextureOptions::LINEAR);
            } else {
                self.texture =
                    Some(ctx.load_texture("console-frame", image, egui::TextureOptions::LINEAR));
            }
            self.frame_pacer.presented(now_ns);
            self.displayed = Some(frame);
        }
        let stream_error = self.active.as_ref().and_then(|active| {
            active
                .video
                .error()
                .or_else(|| active.audio.as_ref().and_then(AudioEngine::error))
        });
        if let Some(error) = stream_error {
            self.disconnected(error);
        } else if self.active.is_some() && self.last_frame.elapsed() > Duration::from_secs(4) {
            self.disconnected(
                "Кадры не поступают. Проверь USB, HDMI и выбранную карту. Подключение остановлено."
                    .into(),
            );
        }
        if self.reconnect.due(Instant::now()) && !self.opening && !self.refreshing {
            self.reconnect.failed(Instant::now());
            self.retry_refresh = true;
            self.refresh(ctx.clone());
        }
        if self
            .dirty
            .is_some_and(|since| since.elapsed() > Duration::from_millis(700))
        {
            self.dirty = None;
            if let Some(store) = &self.store
                && let Err(error) = store.save(&self.settings)
            {
                self.notice = Some(error);
            }
        }
        let linked_account = self.discord_auth.as_ref().and_then(DiscordAuth::account);
        self.discord
            .require_account(linked_account.as_ref().map(|account| account.id.clone()));
        let presence =
            if self.active.is_some() && self.settings.discord_enabled && linked_account.is_some() {
                Some(Presence {
                    application_id: DEFAULT_APPLICATION_ID.to_owned(),
                    game: self.settings.game.clone(),
                    started_at: self.started_unix,
                })
            } else {
                None
            };
        self.discord.set(presence);
    }

    fn screenshot(&mut self, ctx: egui::Context) {
        if self.screenshot_pending {
            return;
        }
        let Some(frame) = self.displayed.clone() else {
            return;
        };
        self.screenshot_pending = true;
        let directory = self.settings.screenshot_dir.clone();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<String, String> {
                std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
                let time = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();
                let path = directory.join(format!("console-{time}.png"));
                let file = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map_err(|e| e.to_string())?;
                let mut encoder =
                    png::Encoder::new(std::io::BufWriter::new(file), frame.width, frame.height);
                encoder.set_color(png::ColorType::Rgba);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
                writer
                    .write_image_data(&frame.rgba)
                    .map_err(|e| e.to_string())?;
                writer.finish().map_err(|e| e.to_string())?;
                Ok(path.to_string_lossy().into_owned())
            })();
            let _ = tx.send(Event::Screenshot(result));
            ctx.request_repaint();
        });
    }

    fn controls(&mut self, ctx: &egui::Context) {
        let before = serde_json::to_string(&self.settings).unwrap_or_default();
        let compact = ctx.content_rect().width() < 900.0;
        let running = self.active.is_some();
        let status = if running {
            "Подключено"
        } else if self.opening {
            "Подключение"
        } else if self.reconnect.active() {
            "Ожидание карты"
        } else {
            "Не подключено"
        };

        egui::SidePanel::left("navigation")
            .exact_width(if compact { 74.0 } else { 204.0 })
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(theme::SIDEBAR)
                    .inner_margin(egui::Margin::symmetric(14, 24)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.brand).fit_to_exact_size(egui::vec2(42.0, 42.0)));
                    if !compact {
                        ui.vertical(|ui| {
                            ui.add_space(4.0);
                            ui.label(RichText::new("Console Link").size(15.0).strong());
                            theme::label(ui, "ИГРА БЛИЖЕ");
                        });
                    }
                });
                ui.add_space(35.0);
                for (page, icon, label) in [
                    (Page::Session, Icon::Session, "Подключение"),
                    (Page::Audio, Icon::Audio, "Звук"),
                    (Page::Discord, Icon::Discord, "Discord"),
                    (Page::Settings, Icon::Settings, "Настройки"),
                ] {
                    if theme::nav(ui, icon, label, self.page == page, compact).clicked() {
                        self.page = page;
                    }
                    ui.add_space(4.0);
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.label(
                        RichText::new(if compact {
                            format!("v{}", env!("CARGO_PKG_VERSION"))
                        } else {
                            format!("Console Link  {}", env!("CARGO_PKG_VERSION"))
                        })
                        .size(10.5)
                        .color(theme::SUBTLE),
                    );
                    if !compact {
                        theme::label(ui, "Только ты и твоя игра.");
                        ui.add_space(10.0);
                    }
                });
            });

        egui::TopBottomPanel::bottom("session-actions")
            .exact_height(83.0)
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(28, 18))
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let stop = running || self.opening || self.reconnect.active();
                    let valid_video = self.devices.video.iter().any(|d| {
                        d.id == self.settings.video_id
                            && self
                                .settings
                                .mode
                                .as_ref()
                                .is_some_and(|m| d.modes.contains(m))
                    });
                    let valid_audio = (self.settings.audio_input_id.is_empty()
                        && self.settings.audio_output_id.is_empty())
                        || (self
                            .devices
                            .inputs
                            .iter()
                            .any(|d| d.id == self.settings.audio_input_id)
                            && self
                                .devices
                                .outputs
                                .iter()
                                .any(|d| d.id == self.settings.audio_output_id));
                    let can_connect = valid_video && valid_audio && !self.refreshing;
                    ui.add_enabled_ui(stop || can_connect, |ui| {
                        if theme::action(
                            ui,
                            if stop { Icon::Stop } else { Icon::Play },
                            if stop {
                                "Отключить"
                            } else {
                                "Подключить консоль"
                            },
                            true,
                            210.0,
                        )
                        .clicked()
                        {
                            if stop {
                                self.stop();
                            } else {
                                self.connect(ctx.clone());
                            }
                        }
                    });
                    ui.add_space(5.0);
                    ui.add_enabled_ui(self.displayed.is_some() && !self.screenshot_pending, |ui| {
                        if theme::action(ui, Icon::Camera, "Скриншот", false, 132.0).clicked()
                        {
                            self.screenshot(ctx.clone());
                        }
                    });
                    if !compact {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(if running {
                                    "F1 открывает боковое меню"
                                } else if self.opening {
                                    "Готовим изображение и звук…"
                                } else {
                                    if !valid_video {
                                        "Выбери карту, чтобы начать"
                                    } else if !valid_audio {
                                        "Выбери аудиовход и выход"
                                    } else {
                                        "Все готово к подключению"
                                    }
                                })
                                .size(11.0)
                                .color(theme::MUTED),
                            );
                        });
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(28, 25)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    theme::label(ui, "ТВОЕ ИГРОВОЕ ПРОСТРАНСТВО");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        theme::status(ui, status, running);
                    });
                });
                ui.add_space(15.0);
                if compact {
                    ui.spacing_mut().scroll.floating = false;
                    ui.spacing_mut().scroll.bar_width = 4.0;
                    ui.spacing_mut().scroll.bar_inner_margin = 8.0;
                }
                egui::ScrollArea::vertical()
                    .scroll_bar_visibility(if compact {
                        egui::scroll_area::ScrollBarVisibility::AlwaysVisible
                    } else {
                        egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded
                    })
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.page {
                            Page::Session => self.session_page(ui, ctx),
                            Page::Audio => self.audio_page(ui),
                            Page::Discord => self.discord_page(ui),
                            Page::Settings => self.settings_page(ui, ctx),
                        }
                        if let Some(error) = &self.error {
                            ui.add_space(14.0);
                            message(ui, "Не удалось подключиться", error, true);
                        }
                        if let Some(notice) = &self.notice {
                            ui.add_space(10.0);
                            message(ui, "Обрати внимание", notice, false);
                        }
                        for warning in &self.devices.warnings {
                            ui.add_space(10.0);
                            message(ui, "Устройство недоступно", warning, false);
                        }
                    });
            });
        if before != serde_json::to_string(&self.settings).unwrap_or_default() {
            self.dirty = Some(Instant::now());
        }
    }

    fn session_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::section_heading(ui, "Подключение", "Все готово для новой игровой сессии.");
        let running = self.active.is_some();
        if ui.ctx().content_rect().height() >= 680.0 {
            theme::card().inner_margin(22).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.set_min_height(95.0);
                ui.spacing_mut().item_spacing.y = 5.0;
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(&self.brand).fit_to_exact_size(egui::vec2(82.0, 82.0)));
                    ui.add_space(17.0);
                    ui.vertical(|ui| {
                        ui.set_width(ui.available_width());
                        ui.add_space(3.0);
                        ui.label(
                            RichText::new(if running {
                                "Хорошей игры."
                            } else if self.opening {
                                "Еще немного…"
                            } else {
                                "Твоя консоль. Твой экран."
                            })
                            .size(if ui.available_width() < 350.0 {
                                22.0
                            } else {
                                26.0
                            })
                            .strong(),
                        );
                        ui.add_space(5.0);
                        theme::subtitle(
                            ui,
                            if running {
                                "F1 открывает настройки поверх игры. Делись окном в Discord или OBS."
                            } else {
                                "Подключи карту захвата и играй на экране компьютера."
                            },
                        );
                        ui.add_space(13.0);
                        ui.horizontal(|ui| {
                            theme::icon(ui, Icon::Plug, 14.0, theme::MUTED);
                            theme::label(ui, "КОНСОЛЬ");
                            theme::label(ui, "-  HDMI  -");
                            theme::label(ui, "КАРТА");
                            theme::label(ui, "-  USB  -");
                            theme::label(ui, "КОМПЬЮТЕР");
                        });
                    });
                });
            });
            ui.add_space(17.0);
        }
        let locked = running || self.opening;
        ui.add_enabled_ui(!locked, |ui| {
            if ui.available_width() >= 650.0 {
                let width = (ui.available_width() - 16.0) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        self.video_card(ui, ctx);
                    });
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.set_width(width);
                        self.audio_devices_card(ui);
                    });
                });
            } else {
                self.video_card(ui, ctx);
                ui.add_space(14.0);
                self.audio_devices_card(ui);
            }
        });
        ui.add_space(17.0);
        ui.horizontal(|ui| {
            theme::icon(ui, Icon::Session, 15.0, theme::SUBTLE);
            ui.label(
                RichText::new("F1  меню     F11  полный экран     F8  скриншот     M  звук")
                    .size(11.0)
                    .color(theme::SUBTLE),
            );
        });
    }

    fn video_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::card().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.set_min_height(254.0);
            card_title(
                ui,
                Icon::Session,
                "Изображение",
                "Источник и качество захвата",
            );
            theme::label(ui, "КАРТА ЗАХВАТА");
            let selected = self
                .devices
                .video
                .iter()
                .find(|d| d.id == self.settings.video_id)
                .map(|d| d.name.as_str())
                .unwrap_or("Выбери устройство");
            let previous = self.settings.video_id.clone();
            egui::ComboBox::from_id_salt("video")
                .truncate()
                .selected_text(selected)
                .width(ui.available_width())
                .show_ui(ui, |ui| {
                    for device in &self.devices.video {
                        ui.selectable_value(
                            &mut self.settings.video_id,
                            device.id.clone(),
                            &device.name,
                        );
                    }
                });
            if previous != self.settings.video_id {
                self.settings.mode = None;
            }
            ui.add_space(9.0);
            theme::label(ui, "РЕЖИМ ЗАХВАТА");
            self.refresh_capture_mode();
            if let Some(mode) = &self.settings.mode {
                ui.label(RichText::new(mode.to_string()).size(15.0));
                if self.settings.capture_preferences.automatic {
                    theme::subtitle(
                        ui,
                        &format!(
                            "Автоматически · цель {} FPS",
                            self.settings.capture_preferences.preferred_fps
                        ),
                    );
                }
                if nominal_fps(mode) != self.settings.capture_preferences.preferred_fps {
                    theme::subtitle(
                        ui,
                        &format!(
                            "Режим карты: {} FPS. Ограничение показа: {} FPS.",
                            nominal_fps(mode),
                            self.settings.capture_preferences.preferred_fps
                        ),
                    );
                }
            } else {
                theme::subtitle(ui, "Выбери карту. Качество настроится автоматически.");
            }
            if ui.small_button("Изменить в настройках").clicked() {
                self.page = Page::Settings;
            }
            ui.add_space(13.0);
            ui.horizontal(|ui| {
                theme::icon(ui, Icon::Refresh, 13.0, theme::MUTED);
                if self.refreshing {
                    ui.spinner();
                    theme::label(ui, "Ищем устройства…");
                } else if ui
                    .add(egui::Button::new("Обновить список").small().frame(false))
                    .clicked()
                {
                    self.refresh(ctx.clone());
                }
            });
            if self.devices.video.is_empty() {
                theme::subtitle(ui, "Пока не видим карту. Проверь USB-подключение.");
            }
        });
    }

    fn audio_devices_card(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.set_min_height(254.0);
            card_title(ui, Icon::Audio, "Звук", "От консоли к твоим наушникам");
            audio_combo(
                ui,
                "audio-in",
                "АУДИОВХОД КАРТЫ",
                &mut self.settings.audio_input_id,
                &self.devices.inputs,
            );
            ui.add_space(9.0);
            audio_combo(
                ui,
                "audio-out",
                "КУДА ВЫВОДИТЬ ЗВУК",
                &mut self.settings.audio_output_id,
                &self.devices.outputs,
            );
            ui.add_space(13.0);
            theme::subtitle(ui, "Для игры без звука оставь оба поля пустыми.");
        });
    }

    fn audio_page(&mut self, ui: &mut egui::Ui) {
        theme::section_heading(ui, "Звук", "Комфортная громкость и точная синхронизация.");
        let previous_gain = self.settings.gain;
        let previous_mute = self.settings.muted;
        let previous_offset = self.settings.sync_ms;
        theme::card().inner_margin(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            ui.spacing_mut().interact_size.y = 28.0;
            ui.visuals_mut().selection.bg_fill = theme::TEXT;
            ui.horizontal(|ui| {
                theme::icon(ui, Icon::Audio, 20.0, theme::TEXT);
                ui.add_space(3.0);
                ui.label(RichText::new("Громкость консоли").size(17.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("{:.0}%", self.settings.gain * 100.0)).size(25.0),
                    );
                });
            });
            ui.add_space(12.0);
            ui.style_mut().spacing.slider_width = ui.available_width();
            ui.add(egui::Slider::new(&mut self.settings.gain, 0.0..=1.0).show_value(false));
            ui.add_space(9.0);
            theme::toggle(ui, &mut self.settings.muted, "Выключить звук консоли");
            ui.add_space(12.0);
            let peak = self
                .active
                .as_ref()
                .and_then(|a| a.audio.as_ref())
                .map(AudioEngine::peak)
                .unwrap_or(0.0);
            level_meter(ui, peak);
            ui.add_space(9.0);
            theme::subtitle(
                ui,
                "Громкость и выключение звука действуют и на демонстрацию в Discord.",
            );
        });
        ui.add_space(12.0);
        theme::card().inner_margin(18).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            ui.spacing_mut().interact_size.y = 28.0;
            ui.visuals_mut().selection.bg_fill = theme::TEXT;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Синхронизация").size(17.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("{:+} мс", self.settings.sync_ms)).size(20.0));
                });
            });
            ui.add_space(8.0);
            theme::subtitle(
                ui,
                "Подстрой звук, если он опережает изображение или отстает.",
            );
            ui.add_space(12.0);
            ui.style_mut().spacing.slider_width = ui.available_width();
            ui.add(egui::Slider::new(&mut self.settings.sync_ms, -500..=500).show_value(false));
            ui.horizontal(|ui| {
                theme::label(ui, "Задержать изображение");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    theme::label(ui, "Задержать звук");
                });
            });
            ui.add_space(8.0);
            if ui
                .add(egui::Button::new("Сбросить коррекцию").small())
                .clicked()
            {
                self.settings.sync_ms = 0;
            }
            ui.add_space(0.0);
            theme::subtitle(ui, "Коррекция добавляет задержку выбранному потоку.");
        });
        if let Some(audio) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
            if previous_gain != self.settings.gain {
                audio.set_gain(self.settings.gain);
            }
            if previous_mute != self.settings.muted {
                audio.set_muted(self.settings.muted);
            }
            if previous_offset != self.settings.sync_ms {
                audio.set_delay_ms(self.settings.sync_ms.max(0) as u32);
            }
        }
        if previous_offset != self.settings.sync_ms {
            self.history.clear();
            self.displayed = None;
            self.texture = None;
        }
    }

    fn discord_page(&mut self, ui: &mut egui::Ui) {
        theme::section_heading(ui, "Discord", "Покажи друзьям, во что играешь.");
        let auth_status = self
            .discord_auth
            .as_ref()
            .map(DiscordAuth::status)
            .unwrap_or(AuthStatus::Disconnected);
        theme::card().show(ui, |ui| {
            card_title(
                ui,
                Icon::Discord,
                "Твой аккаунт",
                "Вход через Discord в браузере",
            );
            match &auth_status {
                AuthStatus::Connected(account) => {
                    ui.label(RichText::new(&account.display_name).size(20.0).strong());
                    theme::subtitle(ui, "Аккаунт подключен");
                    ui.add_space(12.0);
                    if ui.button("Отключить Discord").clicked() {
                        if let Some(auth) = &self.discord_auth {
                            auth.disconnect();
                        }
                        self.discord.require_account(None);
                        self.discord.set(None);
                    }
                }
                AuthStatus::Connecting => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label("Ожидаем подтверждение Discord…");
                    });
                    theme::subtitle(ui, "Если открылся браузер, подтверди вход и вернись сюда.");
                    if ui.button("Отменить подключение").clicked() {
                        if let Some(auth) = &self.discord_auth {
                            auth.disconnect();
                        }
                        self.discord.require_account(None);
                        self.discord.set(None);
                    }
                }
                AuthStatus::Disconnected | AuthStatus::Error(_) => {
                    if let AuthStatus::Error(message) = &auth_status {
                        ui.colored_label(Color32::from_rgb(232, 157, 149), message);
                        ui.add_space(8.0);
                    }
                    theme::subtitle(
                        ui,
                        "Разрешение нужно только для основного профиля. Пароль вводится в Discord.",
                    );
                    ui.add_space(12.0);
                    if ui
                        .add_enabled(
                            self.discord_auth.is_some(),
                            egui::Button::new("Подключить Discord")
                                .min_size(egui::vec2(190.0, 38.0)),
                        )
                        .clicked()
                        && let Some(auth) = &self.discord_auth
                    {
                        auth.connect();
                    }
                    if matches!(auth_status, AuthStatus::Error(_))
                        && ui.button("Забыть сохраненный вход").clicked()
                    {
                        if let Some(auth) = &self.discord_auth {
                            auth.disconnect();
                        }
                        self.discord.require_account(None);
                        self.discord.set(None);
                    }
                }
            }
        });
        ui.add_space(17.0);
        theme::card().show(ui, |ui| {
            card_title(
                ui,
                Icon::Discord,
                "Игровая активность",
                "Название игры в твоем профиле",
            );
            theme::toggle(
                ui,
                &mut self.settings.discord_enabled,
                "Показывать мою игру",
            );
            ui.add_space(14.0);
            theme::label(ui, "ВО ЧТО ИГРАЕШЬ");
            ui.add(
                egui::TextEdit::singleline(&mut self.settings.game)
                    .char_limit(128)
                    .hint_text("Например, The Legend of Zelda")
                    .desired_width(ui.available_width())
                    .margin(egui::vec2(12.0, 11.0)),
            );
            ui.add_space(12.0);
            let raw = self.discord.status();
            let status = if !matches!(auth_status, AuthStatus::Connected(_)) {
                "Войди в Discord, чтобы отправлять статус."
            } else if !self.settings.discord_enabled {
                "Показ активности выключен."
            } else if self.active.is_none() {
                "Статус появится после подключения консоли."
            } else if self.settings.game.trim().is_empty() {
                "Укажи название игры для статуса."
            } else if raw.starts_with("Activity sent") {
                "Активность отправлена в Discord."
            } else if raw.starts_with("Desktop Discord account differs") {
                "В настольном Discord другой аккаунт. Войди в тот же аккаунт, что подключен здесь."
            } else {
                "Ожидаем настольный Discord. Проверь, что передача активности включена."
            };
            theme::subtitle(ui, status);
            ui.add_space(10.0);
            theme::subtitle(
                ui,
                "Для статуса должен быть запущен настольный Discord с тем же аккаунтом.",
            );
        });
        ui.add_space(17.0);
        theme::card().show(ui,|ui| {
            ui.label(RichText::new("Поделиться геймплеем").size(17.0).strong());ui.add_space(8.0);
            theme::subtitle(ui,"В звонке Discord выбери игровое окно Console Link и включи передачу звука. На Mac разреши Discord запись экрана и системного аудио.");
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::section_heading(ui, "Настройки", "Все важное уже под рукой.");
        self.capture_settings(ui);
        ui.add_space(18.0);
        self.window_settings(ui, ctx);
        ui.add_space(18.0);
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Скриншоты").size(17.0).strong());
            ui.add_space(7.0);
            theme::subtitle(
                ui,
                "Снимки сохраняются в PNG, без рамки окна и элементов управления.",
            );
            ui.add_space(16.0);
            theme::label(ui, "ПАПКА ДЛЯ СОХРАНЕНИЯ");
            let mut directory = self.settings.screenshot_dir.to_string_lossy().into_owned();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut directory)
                        .desired_width(ui.available_width())
                        .margin(egui::vec2(12.0, 10.0)),
                )
                .changed()
            {
                self.settings.screenshot_dir = directory.into();
            }
        });
        ui.add_space(18.0);
        theme::card().show(ui,|ui| {
            ui.label(RichText::new("Об приложении").size(17.0).strong());ui.add_space(8.0);
            theme::subtitle(ui,&format!("Console Link {} · Rust · macOS и Windows",env!("CARGO_PKG_VERSION")));ui.add_space(11.0);
            ui.hyperlink_to("Исходный код и загрузки ↗","https://github.com/Potanull/console-link");ui.add_space(12.0);
            ui.collapsing("Диагностика подключения",|ui|{
                ui.label(format!("Получено: {:.1} кадров/с",self.fps));
                ui.label(format!("Очередь видео: {:.1} МБ",self.history.bytes() as f64/1_048_576.0));
                ui.label(format!("Пропущенные кадры: {}",self.capture_dropped.load(Ordering::Relaxed)+self.history.dropped()));
                ui.label(format!("Паузы аудиобуфера: {}",self.active.as_ref().and_then(|a|a.audio.as_ref()).map(AudioEngine::underruns).unwrap_or(0)));
                theme::subtitle(ui,"Показатели относятся к приложению. Полную задержку карты нужно измерять отдельно.");
                if ui.add_enabled(self.active.is_none()&&!self.opening,egui::Button::new("Обновить устройства")).clicked(){self.refresh(ctx.clone());}
            });
        });
    }

    fn capture_settings(&mut self, ui: &mut egui::Ui) {
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Качество изображения").size(17.0).strong());
            ui.add_space(10.0);
            let locked = self.active.is_some() || self.opening;
            ui.add_enabled_ui(!locked, |ui| {
                theme::toggle(ui, &mut self.settings.capture_preferences.automatic, "Автоматическое разрешение");
                ui.add_space(8.0); theme::label(ui, "ЦЕЛЕВОЙ FPS");
                egui::ComboBox::from_id_salt("preferred-fps").selected_text(format!("{} FPS", self.settings.capture_preferences.preferred_fps)).width(ui.available_width()).show_ui(ui, |ui| {
                    for fps in [60, 50, 30, 25, 24] { ui.selectable_value(&mut self.settings.capture_preferences.preferred_fps, fps, format!("{fps} FPS")); }
                });
                if !self.settings.capture_preferences.automatic {
                    if let Some(device) = self.devices.video.iter().find(|d| d.id == self.settings.video_id) {
                        egui::ComboBox::from_id_salt("manual-mode").selected_text(self.settings.mode.as_ref().map(ToString::to_string).unwrap_or_else(|| "Выбери ручной режим".into())).width(ui.available_width()).show_ui(ui, |ui| {
                            for mode in &device.modes { ui.selectable_value(&mut self.settings.mode, Some(mode.clone()), mode.to_string()); }
                        });
                    } else { theme::subtitle(ui, "Сначала выбери карту на странице подключения."); }
                }
            });
            ui.add_space(8.0);
            theme::subtitle(ui, if locked { "Изменить режим захвата можно после завершения сеанса." } else { "По умолчанию Full HD (1920 × 1080), затем ближайший доступный FPS. Если Full HD нет, выберем доступный режим карты." });
        });
        self.refresh_capture_mode();
    }

    fn window_settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::card().show(ui, |ui| {
            ui.label(RichText::new("Окно игры").size(17.0).strong());
            ui.add_space(10.0);
            let before = self.settings.game_window_mode;
            egui::ComboBox::from_id_salt("game-window-mode").selected_text(playback::mode_label(before)).width(ui.available_width()).show_ui(ui, |ui| {
                for mode in [GameWindowMode::Windowed, GameWindowMode::Borderless, GameWindowMode::Fullscreen] { ui.selectable_value(&mut self.settings.game_window_mode, mode, playback::mode_label(mode)); }
            });
            if before != self.settings.game_window_mode && self.playback.playing { self.playback.set_mode(self.settings.game_window_mode); self.apply_window_mode(ctx); }
            theme::subtitle(ui, "Окно без рамки сохраняет размер; его можно перемещать за изображение, когда меню закрыто.");
            ui.add_space(14.0); ui.strong("Горячие клавиши");
            for (key, action) in shortcuts::HELP { ui.horizontal_wrapped(|ui| { ui.label(RichText::new(*key).strong()); theme::label(ui, action); }); }
            theme::subtitle(ui, "Клавиши работают, когда окно приложения в фокусе. M и +/- - только при закрытом меню.");
        });
    }

    fn synchronize_presentation(&mut self, ctx: &egui::Context) {
        let wants_game = self.active.is_some() || self.snapshot_game;
        if wants_game && !self.playback.playing {
            self.menu_geometry = WindowGeometry::current(ctx);
            self.game_geometry.get_or_insert(WindowGeometry {
                position: self
                    .menu_geometry
                    .map(|g| g.position)
                    .unwrap_or(egui::pos2(80.0, 80.0)),
                size: if self.snapshot_game {
                    self.menu_geometry
                        .map(|geometry| geometry.size)
                        .unwrap_or(egui::vec2(1040.0, 760.0))
                } else {
                    egui::vec2(960.0, 540.0)
                },
            });
            self.playback.begin(self.settings.game_window_mode);
            if self.snapshot_game && argument_value("--ui-page").as_deref() == Some("overlay") {
                self.playback.overlay_open = true;
            }
            self.native_mode = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("Console Link - Game".into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                480.0, 270.0,
            )));
            self.apply_window_mode(ctx);
        } else if !wants_game && self.playback.playing {
            self.playback.finish();
            self.page = Page::Session;
            self.native_mode = None;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Title("Console Link".into()));
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(egui::vec2(
                720.0, 600.0,
            )));
            self.pending_geometry = self.menu_geometry;
        }
        if self.pending_geometry.is_some()
            && !ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
        {
            let geometry = self
                .pending_geometry
                .take()
                .expect("checked pending geometry");
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(geometry.position));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(geometry.size));
        }
    }

    fn apply_window_mode(&mut self, ctx: &egui::Context) {
        let mode = self.playback.mode;
        if self.native_mode == Some(mode) {
            return;
        }
        if self
            .native_mode
            .is_some_and(|mode| mode != GameWindowMode::Fullscreen)
        {
            self.game_geometry = WindowGeometry::current(ctx).or(self.game_geometry);
        }
        if mode == GameWindowMode::Fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(
                mode == GameWindowMode::Windowed,
            ));
            self.pending_geometry = self.game_geometry;
        }
        self.native_mode = Some(mode);
        self.settings.game_window_mode = mode;
        self.dirty = Some(Instant::now());
    }

    fn game_shortcuts(&mut self, ctx: &egui::Context) {
        for action in shortcuts::pressed(ctx) {
            if self.playback.overlay_open
                && matches!(action, Action::Mute | Action::Louder | Action::Quieter)
            {
                continue;
            }
            match action {
                Action::Overlay => self.playback.toggle_overlay(),
                Action::Escape => {
                    if self.playback.escape() {
                        self.apply_window_mode(ctx);
                    }
                }
                Action::Fullscreen => {
                    self.playback.toggle_fullscreen();
                    self.apply_window_mode(ctx);
                }
                Action::CycleMode => {
                    self.playback.cycle_mode();
                    self.apply_window_mode(ctx);
                }
                Action::Screenshot => self.screenshot(ctx.clone()),
                Action::Stop => {
                    self.stop();
                    break;
                }
                Action::Mute => {
                    self.settings.muted = !self.settings.muted;
                    self.sync_audio_controls();
                }
                Action::Louder => {
                    self.settings.gain = (self.settings.gain + 0.05).min(1.0);
                    self.sync_audio_controls();
                }
                Action::Quieter => {
                    self.settings.gain = (self.settings.gain - 0.05).max(0.0);
                    self.sync_audio_controls();
                }
            }
        }
    }

    fn sync_audio_controls(&mut self) {
        if let Some(audio) = self
            .active
            .as_ref()
            .and_then(|active| active.audio.as_ref())
        {
            audio.set_gain(self.settings.gain);
            audio.set_muted(self.settings.muted);
        }
        self.dirty = Some(Instant::now());
    }

    fn volume_feedback(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("volume-feedback"))
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -60.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::BACKGROUND)
                    .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                    .corner_radius(16)
                    .inner_margin(18)
                    .show(ui, |ui| {
                        ui.set_width(230.0);
                        ui.horizontal(|ui| {
                            theme::icon(ui, Icon::Audio, 20.0, theme::TEXT);
                            ui.label(
                                RichText::new(if self.settings.muted {
                                    "Звук выключен".into()
                                } else {
                                    format!("Громкость  {:.0}%", self.settings.gain * 100.0)
                                })
                                .size(17.0)
                                .strong(),
                            );
                        });
                        ui.add_space(10.0);
                        let (track, _) =
                            ui.allocate_exact_size(egui::vec2(230.0, 6.0), egui::Sense::hover());
                        ui.painter().rect_filled(track, 3, theme::BORDER);
                        let gain = if self.settings.muted {
                            0.0
                        } else {
                            self.settings.gain
                        };
                        let end = track.left() + track.width() * gain;
                        if gain > 0.0 {
                            ui.painter().rect_filled(
                                egui::Rect::from_min_max(
                                    track.min,
                                    egui::pos2(end, track.bottom()),
                                ),
                                3,
                                theme::TEXT,
                            );
                        }
                        ui.painter().circle_filled(
                            egui::pos2(end, track.center().y),
                            5.0,
                            theme::TEXT,
                        );
                    });
            });
    }

    fn game_scene(&mut self, ctx: &egui::Context) {
        let before = serde_json::to_string(&self.settings).unwrap_or_default();
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| {
                if let Some(texture) = &self.texture {
                    let available = ui.available_size();
                    let original = texture.size_vec2();
                    let size = original * (available.x / original.x).min(available.y / original.y);
                    let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
                    ui.put(rect, egui::Image::new(texture).fit_to_exact_size(size));
                }
                if self.playback.mode == GameWindowMode::Borderless && !self.playback.overlay_open {
                    let response = ui.interact(
                        ui.max_rect(),
                        ui.id().with("game-drag"),
                        egui::Sense::drag(),
                    );
                    if response.drag_started() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                }
            });
        let progress = ctx.animate_bool_with_time(
            egui::Id::new("game-drawer-animation"),
            self.playback.overlay_open,
            0.18,
        );
        if progress > 0.0 {
            let bounds = ctx.content_rect();
            let width = (bounds.width() * 0.5)
                .clamp(360.0, 460.0)
                .min(bounds.width() - 24.0);
            let position = egui::pos2(
                bounds.right() - width * progress - 12.0,
                bounds.top() + 12.0,
            );
            egui::Area::new(egui::Id::new("game-settings-drawer"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .constrain(false)
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(theme::BACKGROUND)
                        .stroke(egui::Stroke::new(1.0_f32, theme::BORDER))
                        .corner_radius(16)
                        .inner_margin(18)
                        .show(ui, |ui| {
                            ui.set_width((width - 36.0).max(1.0));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Console Link").size(18.0).strong());
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.button("Закрыть · F1").clicked() {
                                            self.playback.overlay_open = false;
                                        }
                                    },
                                );
                            });
                            ui.add_space(10.0);
                            ui.scope(|ui| {
                                ui.spacing_mut().button_padding = egui::vec2(8.0, 6.0);
                                ui.spacing_mut().item_spacing.x = 4.0;
                                ui.horizontal_wrapped(|ui| {
                                    for (page, label) in [
                                        (Page::Session, "Сеанс"),
                                        (Page::Audio, "Звук"),
                                        (Page::Discord, "Discord"),
                                        (Page::Settings, "Настройки"),
                                    ] {
                                        if ui
                                            .selectable_label(
                                                self.page == page,
                                                RichText::new(label).size(12.0),
                                            )
                                            .clicked()
                                        {
                                            self.page = page;
                                        }
                                    }
                                });
                            });
                            ui.add_space(10.0);
                            ui.spacing_mut().scroll.floating = false;
                            ui.spacing_mut().scroll.bar_width = 3.0;
                            let scroll_height =
                                (bounds.bottom() - 30.0 - ui.cursor().top() - 62.0).max(20.0);
                            egui::ScrollArea::vertical()
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                                )
                                .id_salt("drawer-scroll")
                                .max_height(scroll_height)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    match self.page {
                                        Page::Session => {
                                            ui.label(
                                                RichText::new("Сеанс игры").size(21.0).strong(),
                                            );
                                            if let Some(mode) = &self.settings.mode {
                                                theme::subtitle(ui, &mode.to_string());
                                            }
                                            self.window_settings(ui, ctx);
                                        }
                                        Page::Audio => self.audio_page(ui),
                                        Page::Discord => self.discord_page(ui),
                                        Page::Settings => self.settings_page(ui, ctx),
                                    }
                                    if let Some(error) = &self.error {
                                        theme::subtitle(ui, error);
                                    }
                                });
                            ui.add_space(10.0);
                            if theme::action(
                                ui,
                                Icon::Stop,
                                "Завершить сеанс",
                                false,
                                ui.available_width(),
                            )
                            .clicked()
                            {
                                self.stop();
                            }
                        });
                });
        } else if self.started.elapsed() < Duration::from_secs(5) || self.snapshot_game {
            egui::Area::new(egui::Id::new("game-hotkey-hint"))
                .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(Color32::from_black_alpha(190))
                        .corner_radius(9)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new("F1  меню     F11  полный экран     F8  скриншот")
                                    .size(12.0)
                                    .color(theme::TEXT),
                            );
                        });
                });
        }
        if before != serde_json::to_string(&self.settings).unwrap_or_default() {
            self.dirty = Some(Instant::now());
        }
    }
}

fn audio_combo(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    selected: &mut String,
    devices: &[AudioDeviceInfo],
) {
    theme::label(ui, label);
    let name = devices
        .iter()
        .find(|d| d.id == *selected)
        .map(|d| d.name.as_str())
        .unwrap_or(if selected.is_empty() {
            "Не выбран"
        } else {
            "Устройство недоступно"
        });
    egui::ComboBox::from_id_salt(id)
        .truncate()
        .selected_text(name)
        .width(ui.available_width())
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, String::new(), "Без звука / не выбран");
            for device in devices {
                ui.selectable_value(selected, device.id.clone(), &device.name);
            }
        });
}

fn card_title(ui: &mut egui::Ui, icon: Icon, title: &str, description: &str) {
    ui.horizontal(|ui| {
        theme::icon(ui, icon, 19.0, theme::TEXT);
        ui.add_space(3.0);
        ui.label(RichText::new(title).size(16.0).strong());
    });
    theme::subtitle(ui, description);
    ui.add_space(8.0);
}

fn message(ui: &mut egui::Ui, title: &str, text: &str, error: bool) {
    egui::Frame::new()
        .fill(if error {
            Color32::from_rgb(47, 32, 34)
        } else {
            theme::SURFACE
        })
        .stroke(egui::Stroke::new(
            1.0_f32,
            if error {
                Color32::from_rgb(94, 57, 61)
            } else {
                theme::BORDER
            },
        ))
        .corner_radius(10)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.label(RichText::new(title).size(13.0).strong());
            ui.add_space(4.0);
            theme::subtitle(ui, text);
        });
}

fn level_meter(ui: &mut egui::Ui, peak: f32) {
    theme::label(ui, "УРОВЕНЬ СИГНАЛА");
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::hover());
    let cells = 40;
    let gap = 3.0;
    let width = (rect.width() - gap * (cells - 1) as f32) / cells as f32;
    for n in 0..cells {
        let active = (n as f32) / (cells as f32) < peak.clamp(0.0, 1.0);
        let r = egui::Rect::from_min_size(
            rect.min + egui::vec2(n as f32 * (width + gap), 0.0),
            egui::vec2(width, rect.height()),
        );
        ui.painter().rect_filled(
            r,
            2,
            if active {
                theme::TEXT
            } else {
                Color32::from_rgb(49, 52, 58)
            },
        );
    }
}

fn argument_value(key: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if self.snapshot_cycle
            && !self.snapshot_cycle_closed
            && self.started.elapsed() > Duration::from_millis(1300)
        {
            // Exercise the same Close input handled for a native game-window close, without a capture device.
            ctx.input_mut(|input| {
                if let Some(viewport) = input.raw.viewports.get_mut(&egui::ViewportId::ROOT) {
                    viewport.events.push(egui::ViewportEvent::Close);
                }
            });
            self.snapshot_cycle_closed = true;
        }
        if let Some(path) = &self.snapshot_path {
            let image = ctx.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Screenshot {
                        viewport_id, image, ..
                    } if *viewport_id == egui::ViewportId::ROOT => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = image {
                let result = (|| -> Result<(), Box<dyn std::error::Error>> {
                    if let Some(parent) = path
                        .parent()
                        .filter(|parent| !parent.as_os_str().is_empty())
                    {
                        std::fs::create_dir_all(parent)?;
                    }
                    let file = std::fs::File::create(path)?;
                    let mut encoder = png::Encoder::new(
                        std::io::BufWriter::new(file),
                        image.width() as u32,
                        image.height() as u32,
                    );
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let bytes: Vec<u8> = image
                        .pixels
                        .iter()
                        .flat_map(|pixel| pixel.to_array())
                        .collect();
                    let mut writer = encoder.write_header()?;
                    writer.write_image_data(&bytes)?;
                    writer.finish()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    eprintln!("UI snapshot failed: {error}");
                }
                self.snapshot_game = false;
                self.playback.finish();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if !self.snapshot_requested
                && self.started.elapsed()
                    > Duration::from_millis(if self.snapshot_cycle { 3600 } else { 1200 })
            {
                if self.snapshot_cycle && (self.playback.playing || self.snapshot_game) {
                    eprintln!("Game-to-menu presentation cycle failed");
                    std::process::exit(2);
                }
                self.snapshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            if self.started.elapsed() > Duration::from_secs(10) {
                eprintln!("UI snapshot timed out");
                self.snapshot_game = false;
                self.playback.finish();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        if std::env::args().any(|argument| argument == "--smoke-test")
            && self.started.elapsed() > Duration::from_secs(3)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.playback.playing && ctx.input(|input| input.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.stop();
            self.snapshot_game = false;
        }
        let previous_volume = (self.settings.gain, self.settings.muted);
        self.events(ctx);
        self.refresh_capture_mode();
        self.synchronize_presentation(ctx);
        if self.active.is_some() {
            self.game_shortcuts(ctx);
            self.synchronize_presentation(ctx);
        }
        if self.active.is_some() || self.snapshot_game {
            self.game_scene(ctx);
        } else {
            self.controls(ctx);
        }
        let now = ctx.input(|input| input.time);
        if previous_volume != (self.settings.gain, self.settings.muted) {
            self.feedback.volume_changed(now);
        }
        if self.feedback.volume_visible(now) {
            self.volume_feedback(ctx);
        }
        let (focused, moved) = ctx.input(|input| {
            (
            input.focused,
            input.events.iter().any(|event| matches!(event, egui::Event::PointerMoved(_))
                || matches!(event, egui::Event::MouseMoved(delta) if *delta != egui::Vec2::ZERO)),
        )
        });
        if self.feedback.hide_cursor(
            now,
            self.active.is_some() || self.snapshot_game,
            self.playback.overlay_open,
            focused,
            moved,
        ) {
            ctx.set_cursor_icon(egui::CursorIcon::None);
        }
        ctx.request_repaint_after(if self.active.is_some() {
            self.frame_pacer.repaint_after(
                monotonic_ns(),
                self.settings.capture_preferences.preferred_fps,
            )
        } else {
            Duration::from_millis(100)
        });
    }
}

impl Drop for ConsoleApp {
    fn drop(&mut self) {
        self.stop();
        if self.dirty.is_some()
            && let Some(store) = &self.store
        {
            let _ = store.save(&self.settings);
        }
    }
}
