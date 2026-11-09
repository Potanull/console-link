use crate::theme::{self, Icon};
use console_link::{
    audio::{self, AudioConfig, AudioDeviceInfo, AudioEngine},
    discord::{DiscordWorker, Presence},
    native,
    scheduler::FrameHistory,
    session::{RetryPolicy, send_latest},
    settings::{Settings, SettingsStore},
    types::{CaptureSession, FrameSink, VideoDevice, VideoFrame, monotonic_ns},
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
    fullscreen: bool,
    discord: DiscordWorker,
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
        let page = match argument_value("--ui-page").as_deref() {
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
            fullscreen: false,
            discord: DiscordWorker::new(),
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

    fn connect(&mut self, ctx: egui::Context) {
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
        self.fullscreen = false;
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
        if self.active.is_some()
            && let Some(frame) = self.history.select(monotonic_ns(), delay_ns)
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
        let presence = if self.active.is_some() && self.settings.discord_enabled {
            Some(Presence {
                application_id: self.settings.discord_application_id.clone(),
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
                                    "Изображение в отдельном окне"
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
                                "Игра открыта в отдельном окне. Делись им в Discord или OBS."
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
                RichText::new(
                    "F11  полный экран     F8  скриншот     Esc  выйти из полного экрана",
                )
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
            theme::label(ui, "КАЧЕСТВО ИЗОБРАЖЕНИЯ");
            if let Some(device) = self
                .devices
                .video
                .iter()
                .find(|d| d.id == self.settings.video_id)
            {
                if self.settings.mode.is_none() {
                    self.settings.mode = device
                        .modes
                        .iter()
                        .find(|m| {
                            m.width == 1920
                                && m.height == 1080
                                && m.fps_num as f64 / m.fps_den.max(1) as f64 >= 59.0
                        })
                        .or_else(|| device.modes.first())
                        .cloned();
                }
                egui::ComboBox::from_id_salt("mode")
                    .truncate()
                    .selected_text(
                        self.settings
                            .mode
                            .as_ref()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "Нет доступных режимов".into()),
                    )
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for mode in &device.modes {
                            ui.selectable_value(
                                &mut self.settings.mode,
                                Some(mode.clone()),
                                mode.to_string(),
                            );
                        }
                    });
            } else {
                ui.add_enabled(
                    false,
                    egui::Button::new("Сначала выбери карту")
                        .min_size(egui::vec2(ui.available_width(), 36.0)),
                );
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
        theme::card().show(ui,|ui| {
            card_title(ui,Icon::Discord,"Твоя игровая активность","Название игры в профиле Discord");
            theme::toggle(ui,&mut self.settings.discord_enabled,"Показывать мою игру");ui.add_space(18.0);
            theme::label(ui,"ВО ЧТО ИГРАЕШЬ");
            ui.add(egui::TextEdit::singleline(&mut self.settings.game).char_limit(128).hint_text("Например, The Legend of Zelda").desired_width(ui.available_width()).margin(egui::vec2(12.0,11.0)));
            ui.add_space(14.0);
            let raw=self.discord.status();let status=if raw=="Off" {"Активность выключена"} else if raw.starts_with("Activity sent") {"Активность отправлена в Discord"} else if raw.starts_with("Waiting") {"Ожидаем настольный Discord"} else {"Настрой подключение и укажи игру"};
            theme::subtitle(ui,status);
            ui.add_space(17.0);ui.separator();ui.add_space(13.0);
            ui.collapsing("Первое подключение к Discord",|ui| {
                theme::subtitle(ui,"Создай приложение в Discord Developer Portal и скопируй его Application ID. Это публичный идентификатор, не токен.");
                ui.add_space(9.0);ui.add(egui::TextEdit::singleline(&mut self.settings.discord_application_id).char_limit(22).hint_text("Application ID").desired_width(ui.available_width()).margin(egui::vec2(12.0,10.0)));
                ui.add_space(8.0);ui.hyperlink_to("Открыть Developer Portal ↗","https://discord.com/developers/applications");
            });
        });
        ui.add_space(17.0);
        theme::card().show(ui,|ui| {
            ui.label(RichText::new("Поделиться геймплеем").size(17.0).strong());ui.add_space(8.0);
            theme::subtitle(ui,"В звонке Discord выбери игровое окно Console Link и включи передачу звука. На Mac разреши Discord запись экрана и системного аудио.");
        });
    }

    fn settings_page(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        theme::section_heading(ui, "Настройки", "Все важное уже под рукой.");
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

    fn game_window(&mut self, ctx: &egui::Context) {
        if self.active.is_none() {
            return;
        }
        let texture = self.texture.clone();
        let mut close = false;
        let mut screenshot = false;
        let mut fullscreen = self.fullscreen;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("console-game"),
            egui::ViewportBuilder::default()
                .with_title("Console Link - Game")
                .with_inner_size([960.0, 540.0])
                .with_min_inner_size([320.0, 180.0]),
            |ctx, _| {
                close = ctx.input(|i| i.viewport().close_requested());
                if ctx.input(|i| i.key_pressed(egui::Key::F11)) {
                    fullscreen = !fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(fullscreen));
                }
                if fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    fullscreen = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
                }
                screenshot = ctx.input(|i| i.key_pressed(egui::Key::F8));
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::BLACK))
                    .show(ctx, |ui| {
                        if let Some(texture) = &texture {
                            let available = ui.available_size();
                            let original = texture.size_vec2();
                            let scale = (available.x / original.x).min(available.y / original.y);
                            let size = original * scale;
                            let rect = egui::Rect::from_center_size(ui.max_rect().center(), size);
                            ui.put(rect, egui::Image::new(texture).fit_to_exact_size(size));
                        }
                    });
            },
        );
        self.fullscreen = fullscreen;
        if screenshot {
            self.screenshot(ctx.clone());
        }
        if close {
            self.stop();
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
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
            if !self.snapshot_requested && self.started.elapsed() > Duration::from_millis(1200) {
                self.snapshot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            if self.started.elapsed() > Duration::from_secs(10) {
                eprintln!("UI snapshot timed out");
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
        self.events(ctx);
        self.controls(ctx);
        self.game_window(ctx);
        ctx.request_repaint_after(Duration::from_millis(if self.active.is_some() {
            16
        } else {
            100
        }));
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
