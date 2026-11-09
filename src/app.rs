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
    store: SettingsStore,
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
}

impl ConsoleApp {
    pub fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        let (store, settings, warning) = SettingsStore::open();
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
            if let Err(error) = self.store.save(&self.settings) {
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
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(12.0); ui.heading(RichText::new("Console Link").size(30.0));
                ui.label(RichText::new("Консоль на твоем компьютере").color(Color32::from_gray(160))); ui.add_space(18.0);
                let running = self.active.is_some();
                ui.horizontal(|ui| { ui.colored_label(if running { Color32::from_rgb(99,210,155) } else { Color32::GRAY }, if running { "● Подключено" } else if self.opening { "● Подключение…" } else { "● Не подключено" }); if self.refreshing { ui.spinner(); } });
                if let Some(error) = &self.error { ui.add_space(8.0); ui.colored_label(Color32::from_rgb(255,140,130), error); }
                if let Some(notice) = &self.notice { ui.label(notice); }
                for warning in &self.devices.warnings { ui.colored_label(Color32::YELLOW, warning); }
                ui.add_space(14.0); ui.separator(); ui.add_space(8.0);
                ui.add_enabled_ui(!running && !self.opening, |ui| {
                    ui.strong("1. Карта захвата");
                    let selected = self.devices.video.iter().find(|d| d.id == self.settings.video_id).map(|d| d.name.as_str()).unwrap_or("Выбери устройство");
                    let previous = self.settings.video_id.clone();
                    egui::ComboBox::from_id_salt("video").selected_text(selected).width(420.0).show_ui(ui, |ui| { for device in &self.devices.video { ui.selectable_value(&mut self.settings.video_id, device.id.clone(), &device.name); } });
                    if previous != self.settings.video_id { self.settings.mode = None; }
                    if self.devices.video.is_empty() { ui.label("Подключи HDMI к консоли, USB карты к компьютеру и обнови список."); }
                    if let Some(device) = self.devices.video.iter().find(|d| d.id == self.settings.video_id) {
                        if self.settings.mode.is_none() { self.settings.mode = device.modes.iter().find(|m| m.width == 1920 && m.height == 1080 && m.fps_num as f64 / m.fps_den.max(1) as f64 >= 59.0).or_else(|| device.modes.first()).cloned(); }
                        egui::ComboBox::from_id_salt("mode").selected_text(self.settings.mode.as_ref().map(ToString::to_string).unwrap_or_else(|| "Нет доступных режимов".into())).width(420.0).show_ui(ui, |ui| { for mode in &device.modes { ui.selectable_value(&mut self.settings.mode, Some(mode.clone()), mode.to_string()); } });
                    }
                    if ui.button("Обновить устройства").clicked() { self.refresh(ctx.clone()); }
                    ui.add_space(14.0); ui.strong("2. Звук консоли");
                    audio_combo(ui, "audio-in", "Вход карты", &mut self.settings.audio_input_id, &self.devices.inputs);
                    audio_combo(ui, "audio-out", "Наушники / выход", &mut self.settings.audio_output_id, &self.devices.outputs);
                    ui.small("Выбери аудиовход карты, не микрофон. Для видео без звука оставь оба поля пустыми.");
                });
                ui.add_space(12.0);
                let gain_changed = ui.add(egui::Slider::new(&mut self.settings.gain, 0.0..=1.0).text("Громкость").custom_formatter(|v, _| format!("{:.0}%", v * 100.0))).changed();
                let mute_changed = ui.checkbox(&mut self.settings.muted, "Без звука").changed();
                let peak = self.active.as_ref().and_then(|a| a.audio.as_ref()).map(AudioEngine::peak).unwrap_or(0.0);
                ui.add(egui::ProgressBar::new(peak.clamp(0.0,1.0)).text("Уровень звука"));
                let offset_changed = ui.add(egui::Slider::new(&mut self.settings.sync_ms, -500..=500).text("Синхронизация, мс")).changed();
                ui.small("+ задерживает звук · - задерживает видео. Коррекция увеличивает задержку.");
                if let Some(audio) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
                    if gain_changed { audio.set_gain(self.settings.gain); }
                    if mute_changed { audio.set_muted(self.settings.muted); }
                    if offset_changed { audio.set_delay_ms(self.settings.sync_ms.max(0) as u32); }
                }
                if offset_changed { self.history.clear(); self.displayed = None; self.texture = None; }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if running || self.opening || self.reconnect.active() { if ui.button("Отключить").clicked() { self.stop(); } }
                    else if ui.add_enabled(!self.refreshing, egui::Button::new("Подключить консоль").min_size(egui::vec2(200.0,36.0))).clicked() { self.connect(ctx.clone()); }
                    if ui.add_enabled(self.displayed.is_some() && !self.screenshot_pending, egui::Button::new("Скриншот")).clicked() { self.screenshot(ctx.clone()); }
                });
                ui.add_space(12.0); ui.separator(); ui.add_space(8.0);
                ui.collapsing("Discord", |ui| {
                    ui.checkbox(&mut self.settings.discord_enabled, "Показывать игру в Discord");
                    ui.label("Название игры"); ui.add(egui::TextEdit::singleline(&mut self.settings.game).char_limit(128).hint_text("Например, The Legend of Zelda"));
                    ui.label("Discord Application ID"); ui.add(egui::TextEdit::singleline(&mut self.settings.discord_application_id).char_limit(22).hint_text("ID приложения из Discord Developer Portal"));
                    ui.small("Нужен один Application ID владельца приложения. Это публичный ID, не токен.");
                    ui.label(self.discord.status());
                    ui.hyperlink_to("Создать Discord Application", "https://discord.com/developers/applications");
                    ui.small("Для демонстрации выбери игровое окно в Discord и включи передачу звука. Громкость и mute действуют и на трансляцию.");
                });
                ui.collapsing("Скриншоты", |ui| { let mut directory = self.settings.screenshot_dir.to_string_lossy().into_owned(); if ui.text_edit_singleline(&mut directory).changed() { self.settings.screenshot_dir = directory.into(); } });
                ui.collapsing("Диагностика", |ui| {
                    ui.label(format!("Получено: {:.1} fps · очередь: {:.1} МБ", self.fps, self.history.bytes() as f64 / 1_048_576.0));
                    ui.label(format!("Пропуски: {} · audio underruns: {}", self.capture_dropped.load(Ordering::Relaxed) + self.history.dropped(), self.active.as_ref().and_then(|a| a.audio.as_ref()).map(AudioEngine::underruns).unwrap_or(0)));
                    ui.small("FPS измерен по доставленным кадрам. Задержка HDMI и работа Discord требуют отдельной проверки.");
                });
                ui.add_space(12.0); ui.small("Игровое окно: F11 - полный экран · F8 - скриншот · Esc - выйти из полного экрана");
            });
        });
        if before != serde_json::to_string(&self.settings).unwrap_or_default() {
            self.dirty = Some(Instant::now());
        }
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
    ui.label(label);
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
        .selected_text(name)
        .width(420.0)
        .show_ui(ui, |ui| {
            ui.selectable_value(selected, String::new(), "Не выбран");
            for device in devices {
                ui.selectable_value(selected, device.id.clone(), &device.name);
            }
        });
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
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
        if self.dirty.is_some() {
            let _ = self.store.save(&self.settings);
        }
    }
}
