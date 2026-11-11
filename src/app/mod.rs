use crate::{argument_value, window_icon};
use console_link::config::{app as config, capture as capture_config};

mod artwork;
mod color_gpu;
mod color_integration;
mod color_page;
mod equalizer;
mod monitoring;
mod pages;
mod presentation;
mod session;
mod upscale_gpu;
mod upscale_ui;
mod widgets;

use widgets::{audio_combo, card_title, level_meter, message};
mod playback;
mod shortcuts;
mod theme;

use self::{
    playback::{FramePacer, GameFeedback, PlaybackState},
    shortcuts::Action,
    theme::Icon,
};
use console_link::{
    audio::{self, AudioConfig, AudioDeviceInfo, AudioEngine},
    capture::{
        self, CaptureSession, FrameHistory, FrameSink, RetryPolicy, VideoDevice, VideoFrame,
        choose_mode, monotonic_ns, nominal_fps, send_latest,
    },
    catalog::{self, Game},
    config::discord::DEFAULT_APPLICATION_ID,
    discord::{
        DiscordWorker, Presence,
        auth::{AuthStatus, DiscordAccount, DiscordAuth},
    },
    settings::{GameWindowMode, Settings, SettingsStore},
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
    Color,
    Discord,
    Settings,
}

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
    color_gpu_available: bool,
    color_selected: Option<usize>,
    color_name: String,
    color_original: bool,
    color_test_pattern: Arc<VideoFrame>,
    presented_image: Option<upscale_ui::PresentedImage>,
    upscale_comparison: bool,
    upscale_error: Option<String>,
    upscale_preview_status: Option<upscale_ui::DisplayStatus>,
    export_gpu: Option<(wgpu::Device, wgpu::Queue)>,
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
    pending_borderless: Option<Instant>,
    native_mode: Option<GameWindowMode>,
    snapshot_game: bool,
    snapshot_cycle: bool,
    snapshot_cycle_closed: bool,
    discord: DiscordWorker,
    discord_auth: Option<DiscordAuth>,
    auth_completion: u64,
    eq_selected: Option<usize>,
    eq_name: String,
    monitor: console_link::monitoring::Monitor,
    session_started: Option<Instant>,
    artwork: artwork::Artwork,
    game_query: String,
    game_results: Vec<Game>,
    game_metadata_name: String,
    game_metadata: Option<Game>,
    dirty: Option<Instant>,
    screenshot_pending: bool,
    capture_dropped: Arc<AtomicU64>,
    history_dropped_start: u64,
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
        let color_gpu_available = if let Some(state) = &context.wgpu_render_state {
            color_gpu::install(state);
            true
        } else {
            false
        };
        let icon = window_icon();
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
                Some("overlay" | "overlay-color" | "gameplay" | "volume")
            );
        let snapshot_game = snapshot_game || snapshot_cycle;
        let page = match argument_value("--ui-page").as_deref() {
            Some("overlay") => Page::Audio,
            Some("audio") => Page::Audio,
            Some("color" | "overlay-color") => Page::Color,
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
        let (tx, rx) = bounded(config::EVENT_QUEUE_CAPACITY);
        let (frames_tx, frames_rx) = bounded(config::FRAME_QUEUE_CAPACITY);
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
            history: FrameHistory::new(
                capture_config::HISTORY_BUDGET,
                capture_config::HISTORY_TOLERANCE_NS,
            ),
            displayed: None,
            texture: None,
            color_gpu_available,
            color_selected: None,
            color_name: String::new(),
            color_original: false,
            color_test_pattern: color_integration::test_pattern(),
            presented_image: None,
            upscale_comparison: false,
            upscale_error: None,
            upscale_preview_status: None,
            export_gpu: context
                .wgpu_render_state
                .as_ref()
                .map(|s| (s.device.clone(), s.queue.clone())),
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
            pending_borderless: None,
            native_mode: None,
            snapshot_game,
            snapshot_cycle,
            snapshot_cycle_closed: false,
            discord,
            discord_auth,
            auth_completion: 0,
            eq_selected: None,
            eq_name: String::new(),
            monitor: console_link::monitoring::Monitor::default(),
            session_started: None,
            artwork: artwork::Artwork::new(),
            game_query: String::new(),
            game_results: Vec::new(),
            game_metadata_name: String::new(),
            game_metadata: None,
            dirty: None,
            screenshot_pending: false,
            capture_dropped: Arc::new(AtomicU64::new(0)),
            history_dropped_start: 0,
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
        if app.snapshot_path.is_some()
            && let Some(game) = argument_value("--ui-game")
        {
            app.settings.game = game;
        }
        if snapshot_game {
            app.settings.game_window_mode = match argument_value("--ui-game-mode").as_deref() {
                Some("fullscreen") => GameWindowMode::Fullscreen,
                Some("borderless") => GameWindowMode::Borderless,
                _ => GameWindowMode::Windowed,
            };
            app.playback.overlay_open = matches!(
                argument_value("--ui-page").as_deref(),
                Some("overlay" | "overlay-color")
            );
        }
        if app.snapshot_path.is_some() {
            app.settings.upscale.mode = match argument_value("--ui-upscale-mode").as_deref() {
                Some("nis") => console_link::upscale::UpscaleMode::Nis,
                Some("fsr1") => console_link::upscale::UpscaleMode::Fsr1,
                Some("rtx") => console_link::upscale::UpscaleMode::RtxVideo,
                _ => console_link::upscale::UpscaleMode::Standard,
            };
            if let Some(name) = argument_value("--ui-color-preset")
                && let Some((_, preset)) = console_link::color::FACTORY_PRESETS
                    .iter()
                    .find(|(label, _)| *label == name)
            {
                app.settings.color = *preset;
            }
            if app.snapshot_game {
                app.displayed = Some(app.color_test_pattern.clone());
            }
        }
        app.refresh(context.egui_ctx.clone());
        app
    }
}

impl eframe::App for ConsoleApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
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
        if let Some(started) = self.session_started {
            let peak = self
                .active
                .as_ref()
                .and_then(|a| a.audio.as_ref())
                .map(AudioEngine::peak)
                .unwrap_or(0.0);
            self.monitor.record(started.elapsed(), self.fps, peak);
        }
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
        if let Some(requested) = self.pending_borderless {
            let geometry = if ctx.input(|input| input.viewport().fullscreen.unwrap_or(false)) {
                Ok(None)
            } else {
                crate::platform::window_geometry::monitor_rect(frame, ctx)
            };
            match geometry {
                Ok(Some(rect)) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(false));
                    self.pending_geometry = Some(WindowGeometry {
                        position: rect.min,
                        size: rect.size(),
                    });
                    self.pending_borderless = None;
                    ctx.request_repaint();
                }
                Ok(None) if requested.elapsed() < Duration::from_secs(10) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
                result => {
                    let error = result
                        .err()
                        .unwrap_or_else(|| "Не завершился переход из полного экрана".into());
                    self.error = Some(format!(
                        "Не удалось открыть безрамочный полный экран: {error}"
                    ));
                    self.playback.set_mode(GameWindowMode::Windowed);
                    self.apply_window_mode(ctx);
                }
            }
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
