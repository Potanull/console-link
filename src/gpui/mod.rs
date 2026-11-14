//! GPUI desktop frontend. Domain state and GPU passes are shared with the legacy frontend.
mod artwork;
mod assets;
mod pages;
mod presenter;
mod widgets;
mod window;

use artwork::Artwork;
use console_link::{
    capture::VideoFrame,
    gpu::{VideoRenderer, fit_viewport},
    runtime::Runtime,
    settings::GameWindowMode,
};
use gpui::{prelude::*, *};
use presenter::Presenter;
use std::{
    borrow::Cow,
    cell::Cell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use widgets::{TextChanged, TextField, button};
use window::{CursorIdle, WindowModes};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Page {
    Session,
    Audio,
    Image,
    Discord,
    Settings,
}
actions!(
    console_desktop,
    [
        ToggleOverlay,
        Escape,
        Fullscreen,
        CycleMode,
        Screenshot,
        Mute,
        Louder,
        Quieter,
        Stop,
        Quit
    ]
);
#[derive(Clone)]
struct Recipe {
    frame: Arc<VideoFrame>,
    color: console_link::color::ColorSettings,
    settings: console_link::upscale::UpscaleSettings,
    target: [u32; 2],
    original: bool,
}
pub(super) struct Desktop {
    pub(super) runtime: Runtime,
    pub(super) page: Page,
    pub(super) game_input: Entity<TextField>,
    pub(super) preset_input: Entity<TextField>,
    pub(super) eq_input: Entity<TextField>,
    pub(super) path_input: Entity<TextField>,
    pub(super) game_query: String,
    selection: Option<pages::Selection>,
    pub(super) color_selected: Option<usize>,
    pub(super) eq_selected: Option<usize>,
    pub(super) original: bool,
    pub(super) comparison: bool,
    artwork: Artwork,
    brand: Arc<RenderImage>,
    calibration: Arc<VideoFrame>,
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
    presenter: Option<Presenter>,
    renderer: Option<VideoRenderer>,
    frame: Option<Arc<VideoFrame>>,
    recipe: Option<Recipe>,
    export: Option<std::sync::mpsc::Receiver<Result<Arc<VideoFrame>, String>>>,
    video_bounds: Rc<Cell<Bounds<Pixels>>>,
    modes: WindowModes,
    fullscreen_return: GameWindowMode,
    cursor: CursorIdle,
    pointer: Point<Pixels>,
    playing: bool,
    overlay: bool,
    feedback: Option<Instant>,
    last_volume: (f32, bool),
    gpu_error: Option<String>,
    synthetic: bool,
    synthetic_started: Instant,
    synthetic_index: u64,
    redraw: bool,
    ui_updated: Instant,
    closed: bool,
    present_count: u64,
    smoke_cycle: bool,
    smoke_stage: usize,
    last_trace: Instant,
}

impl Desktop {
    fn new(persistent: bool, synthetic: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let runtime = Runtime::new(persistent);
        let brand = brand_image();
        let calibration = synthetic_frame(0);
        let startup_page = crate::argument("--ui-page");
        let page = match startup_page.as_deref() {
            Some("audio") => Page::Audio,
            Some("color" | "image") => Page::Image,
            Some("discord") => Page::Discord,
            Some("settings") => Page::Settings,
            _ => Page::Session,
        };
        let game_query = runtime.settings.game.clone();
        let last_volume = (runtime.settings.gain, runtime.settings.muted);
        let game_input = cx.new(|cx| TextField::new("Название игры или поиск", 128, cx));
        game_input.update(cx, |v, cx| v.set_value(game_query.clone(), cx));
        let path_input = cx.new(|cx| TextField::new("Папка скриншотов", 1024, cx));
        path_input.update(cx, |v, cx| {
            v.set_value(
                runtime
                    .settings
                    .screenshot_dir
                    .to_string_lossy()
                    .into_owned(),
                cx,
            )
        });
        let preset_input = cx.new(|cx| TextField::new("Название профиля изображения", 48, cx));
        let eq_input = cx.new(|cx| TextField::new("Название пресета эквалайзера", 48, cx));
        let subscriptions = vec![
            cx.subscribe(&game_input, |this, _, event: &TextChanged, cx| {
                this.game_query = event.0.clone();
                cx.notify();
            }),
            cx.subscribe(&path_input, |this, _, event: &TextChanged, cx| {
                this.runtime.settings.screenshot_dir = std::path::PathBuf::from(&event.0);
                this.runtime.changed();
                cx.notify();
            }),
        ];
        let focus = cx.focus_handle();
        window.focus(&focus);
        let weak = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.playing || this.runtime.opening {
                    this.stop(window, cx);
                    false
                } else {
                    this.shutdown(window, cx);
                    true
                }
            })
            .unwrap_or(true)
        });
        let (presenter, renderer, gpu_error) = match Presenter::new(window) {
            Ok(p) => match VideoRenderer::new(p.device(), p.format()) {
                Ok(r) => (Some(p), Some(r), None),
                Err(e) => (Some(p), None, Some(e)),
            },
            Err(e) => (None, None, Some(e)),
        };
        let mut this = Self {
            runtime,
            page,
            game_input,
            preset_input,
            eq_input,
            path_input,
            game_query,
            selection: None,
            color_selected: None,
            eq_selected: None,
            original: false,
            comparison: false,
            artwork: Artwork::new(persistent),
            brand,
            calibration: calibration.clone(),
            focus,
            _subscriptions: subscriptions,
            presenter,
            renderer,
            frame: Some(calibration),
            recipe: None,
            export: None,
            video_bounds: Rc::new(Cell::new(Bounds::default())),
            modes: WindowModes::default(),
            fullscreen_return: GameWindowMode::Windowed,
            cursor: CursorIdle::default(),
            pointer: point(px(-1.), px(-1.)),
            playing: synthetic,
            overlay: synthetic && startup_page.is_some(),
            feedback: None,
            last_volume,
            gpu_error,
            synthetic,
            synthetic_started: Instant::now(),
            synthetic_index: 0,
            redraw: true,
            ui_updated: Instant::now(),
            closed: false,
            present_count: 0,
            smoke_cycle: std::env::args().any(|a| a == "--smoke-cycle"),
            smoke_stage: 0,
            last_trace: Instant::now(),
        };
        if synthetic
            && let Some(mode) = crate::argument("--ui-window-mode").and_then(|s| match s.as_str() {
                "borderless" => Some(GameWindowMode::Borderless),
                "fullscreen" => Some(GameWindowMode::Fullscreen),
                "windowed" => Some(GameWindowMode::Windowed),
                _ => None,
            })
        {
            this.runtime.settings.game_window_mode = mode;
            if let Err(e) = this.modes.set_mode(mode, window) {
                this.runtime.error = Some(e);
            }
        }
        this
    }
    fn tick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.closed {
            return;
        }
        if self.smoke_cycle {
            let seconds = self.synthetic_started.elapsed().as_secs();
            let stages = [
                (1, GameWindowMode::Borderless),
                (3, GameWindowMode::Fullscreen),
                (6, GameWindowMode::Windowed),
            ];
            if self.smoke_stage < stages.len() && seconds >= stages[self.smoke_stage].0 {
                self.set_mode(stages[self.smoke_stage].1, window, cx);
                self.smoke_stage += 1;
            } else if self.smoke_stage == 3 && seconds >= 8 {
                self.overlay = true;
                self.page = Page::Audio;
                self.smoke_stage += 1;
                cx.notify();
            } else if self.smoke_stage == 4 && seconds >= 10 {
                self.stop(window, cx);
                self.page = Page::Session;
                self.smoke_stage += 1;
            }
        }
        if self.last_trace.elapsed() >= Duration::from_secs(1)
            && std::env::args().any(|a| a == "--smoke-trace" || a == "--smoke-cycle")
        {
            self.last_trace = Instant::now();
            eprintln!(
                "GPUI_FRAME presents={} mode={:?} fullscreen={} bounds={:?} viewport={:?} cursor_hidden={}",
                self.present_count,
                self.modes.mode(),
                window.is_fullscreen(),
                window.bounds(),
                window.viewport_size(),
                self.cursor.is_hidden()
            );
        }
        if let Some(frame) = self.runtime.poll() {
            self.frame = Some(frame);
            self.redraw = true;
        }
        if self.synthetic {
            let index = (self.synthetic_started.elapsed().as_secs_f64() * 60.) as u64;
            if self.frame.is_none() || index != self.synthetic_index {
                self.synthetic_index = index;
                self.frame = Some(synthetic_frame(index));
                self.redraw = true;
            }
        }
        let volume = (self.runtime.settings.gain, self.runtime.settings.muted);
        if volume != self.last_volume {
            self.last_volume = volume;
            self.feedback = Some(Instant::now());
        }
        let playing = self.runtime.active() || self.synthetic;
        let title = if playing && !self.runtime.game_name().is_empty() {
            format!("{} · Console Link", self.runtime.game_name())
        } else {
            "Console Link".into()
        };
        if window.window_title() != title {
            window.set_window_title(&title);
        }

        if playing != self.playing {
            self.playing = playing;
            if !playing {
                self.recipe = None;
            }
            self.overlay = false;
            let mode = if playing {
                self.runtime.settings.game_window_mode
            } else {
                GameWindowMode::Windowed
            };
            if let Err(e) = self.modes.set_mode(mode, window) {
                self.runtime.error = Some(e);
            }
            self.redraw = true;
            cx.notify();
        }
        let previous_mode = self.modes.mode();
        if let Err(e) = self.modes.poll(window) {
            self.runtime.error = Some(e);
        }
        if previous_mode != GameWindowMode::Fullscreen
            && self.modes.mode() == GameWindowMode::Fullscreen
        {
            self.fullscreen_return = previous_mode;
        }
        let pointer = window.mouse_position();
        let moving = pointer != self.pointer;
        self.pointer = pointer;
        let in_game = self.playing
            && !self.overlay
            && window.is_window_hovered()
            && pointer.x >= px(0.)
            && pointer.y >= px(0.)
            && pointer.x < window.viewport_size().width
            && pointer.y < window.viewport_size().height;
        self.cursor
            .update(moving, in_game, window.is_window_active(), window);
        if let Some(rx) = &self.export {
            match rx.try_recv() {
                Ok(result) => {
                    self.export = None;
                    match result {
                        Ok(frame) => self.runtime.save_frame_screenshot(frame),
                        Err(e) => self.runtime.error = Some(e),
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.export = None;
                    self.runtime.error = Some("Экспорт изображения прерван.".into());
                }
                _ => {}
            }
        }
        if self.redraw || playing {
            self.draw_video(window);
        }
        if self.ui_updated.elapsed() >= Duration::from_millis(100) {
            self.ui_updated = Instant::now();
            cx.notify();
        }
    }
    fn draw_video(&mut self, window: &mut Window) {
        let Some(presenter) = self.presenter.as_mut() else {
            return;
        };
        let before = presenter.size();
        if let Err(e) = presenter.resize(window) {
            self.gpu_error = Some(e);
            return;
        }
        let size = presenter.size();
        if before != size {
            self.redraw = true;
        }
        if !self.redraw {
            return;
        }
        let bounds = if self.playing {
            Bounds::new(point(px(0.), px(0.)), window.viewport_size())
        } else {
            self.video_bounds.get()
        };
        let scale = window.scale_factor();
        let source = self
            .frame
            .as_ref()
            .map_or([1920, 1080], |f| [f.width, f.height]);
        let rect = [
            f32::from(bounds.origin.x) * scale,
            f32::from(bounds.origin.y) * scale,
            f32::from(bounds.size.width) * scale,
            f32::from(bounds.size.height) * scale,
        ];
        let viewport = fit_viewport(rect, size, source).unwrap_or([0., 0., 1., 1.]);
        let frame = if !self.playing {
            (self.page == Page::Image).then(|| self.calibration.clone())
        } else if !self.synthetic && self.runtime.displayed().is_none() {
            None
        } else {
            self.frame.clone()
        };
        let color = self.runtime.settings.color;
        let mut settings = self.runtime.settings.upscale;
        if self.comparison {
            settings.mode = console_link::upscale::UpscaleMode::Standard;
        }
        let original = self.original;
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };
        renderer.set_background(if self.playing {
            [0, 0, 0]
        } else {
            [16, 16, 16]
        });
        match presenter.present(|device, queue, view, encoder| {
            renderer.draw(
                device,
                queue,
                view,
                encoder,
                frame.as_ref(),
                color,
                original,
                settings,
                viewport,
            )
        }) {
            Ok(true) => {
                self.redraw = false;
                self.present_count += 1;
                if let Some(frame) = frame.filter(|_| self.playing) {
                    self.recipe = Some(Recipe {
                        frame,
                        color,
                        settings: renderer.effective_settings(),
                        target: [viewport[2] as u32, viewport[3] as u32],
                        original,
                    });
                }
                self.gpu_error = renderer.warning().map(str::to_owned);
            }
            Ok(false) => {}
            Err(e) => self.gpu_error = Some(e),
        }
    }
    pub(super) fn connect(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.presenter.is_none() || self.renderer.is_none() {
            self.runtime.error = Some(
                self.gpu_error
                    .clone()
                    .unwrap_or_else(|| "Видеовывод GPU недоступен.".into()),
            );
        } else {
            self.runtime.connect();
        }
        cx.notify();
    }
    pub(super) fn stop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.synthetic = false;
        self.runtime.stop();
        self.playing = false;
        self.recipe = None;
        self.overlay = false;
        let _ = self.modes.set_mode(GameWindowMode::Windowed, window);
        self.redraw = true;
        cx.notify();
    }
    pub(super) fn choose_game(&mut self, name: String, _: &mut Window, cx: &mut Context<Self>) {
        self.runtime.choose_game(name.clone());
        self.game_query = name.clone();
        self.game_input.update(cx, |v, cx| v.set_value(name, cx));
        cx.notify();
    }
    pub(super) fn set_mode(
        &mut self,
        mode: GameWindowMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mode == GameWindowMode::Fullscreen && self.modes.mode() != GameWindowMode::Fullscreen {
            self.fullscreen_return = self.modes.mode();
        }
        self.runtime.settings.game_window_mode = mode;
        self.runtime.changed();
        if self.playing
            && let Err(e) = self.modes.set_mode(mode, window)
        {
            self.runtime.error = Some(e);
        }
        self.redraw = true;
        cx.notify();
    }
    pub(super) fn artwork_image(
        &mut self,
        url: Option<String>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Arc<RenderImage>> {
        url.and_then(|url| self.artwork.image(&url, window))
    }
    pub(super) fn screenshot(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if !self.playing || !(self.runtime.active() || self.synthetic) {
            self.runtime.notice = Some("Сначала подключи консоль.".into());
            cx.notify();
            return;
        }
        if self.export.is_some() || self.runtime.screenshot_pending() {
            return;
        }
        let Some(recipe) = self.recipe.clone() else {
            self.runtime.notice = Some("Сначала подключи консоль.".into());
            cx.notify();
            return;
        };
        let Some(presenter) = &self.presenter else {
            return;
        };
        let device = presenter.device().clone();
        let queue = presenter.queue().clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.export = Some(rx);
        std::thread::spawn(move || {
            let result = console_link::gpu::upscale::render_export(
                &device,
                &queue,
                recipe.frame.clone(),
                if recipe.original {
                    Default::default()
                } else {
                    recipe.color
                },
                recipe.settings,
                recipe.target,
            )
            .map(|rgba| {
                Arc::new(VideoFrame {
                    width: recipe.target[0],
                    height: recipe.target[1],
                    rgba,
                    timestamp_ns: recipe.frame.timestamp_ns,
                })
            });
            let _ = tx.send(result);
        });
        cx.notify();
    }
    fn shutdown(&mut self, _window: &mut Window, _: &mut Context<Self>) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.cursor = CursorIdle::default();
        self.artwork.stop();
        self.renderer = None;
        self.presenter = None;
        self.runtime.close();
        eprintln!(
            "GPUI_SHUTDOWN presents={} gpu_readback_playback=0",
            self.present_count
        );
    }
    fn volume(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.runtime.settings.gain = (self.runtime.settings.gain + delta).clamp(0., 1.);
        self.runtime.changed();
        self.feedback = Some(Instant::now());
        cx.notify();
    }
    fn toggle_overlay(&mut self, _: &ToggleOverlay, window: &mut Window, cx: &mut Context<Self>) {
        if self.playing {
            self.overlay = !self.overlay;
            self.focus.focus(window);
            cx.notify();
        }
    }
    fn escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay {
            self.overlay = false;
        } else if self.playing && self.modes.mode() != GameWindowMode::Windowed {
            self.set_mode(GameWindowMode::Windowed, window, cx);
        }
        self.focus.focus(window);
        cx.notify();
    }
    fn fullscreen(&mut self, _: &Fullscreen, window: &mut Window, cx: &mut Context<Self>) {
        let mode = if self.modes.mode() == GameWindowMode::Fullscreen {
            self.fullscreen_return
        } else {
            GameWindowMode::Fullscreen
        };
        self.set_mode(mode, window, cx);
    }
    fn cycle(&mut self, _: &CycleMode, window: &mut Window, cx: &mut Context<Self>) {
        let mode = match self.modes.mode() {
            GameWindowMode::Windowed => GameWindowMode::Borderless,
            GameWindowMode::Borderless => GameWindowMode::Fullscreen,
            GameWindowMode::Fullscreen => GameWindowMode::Windowed,
        };
        self.set_mode(mode, window, cx);
    }
    fn take_screenshot(&mut self, _: &Screenshot, w: &mut Window, cx: &mut Context<Self>) {
        self.screenshot(w, cx);
    }
    fn mute(&mut self, _: &Mute, _: &mut Window, cx: &mut Context<Self>) {
        self.runtime.settings.muted = !self.runtime.settings.muted;
        self.runtime.changed();
        self.feedback = Some(Instant::now());
        cx.notify();
    }
    fn louder(&mut self, _: &Louder, _: &mut Window, cx: &mut Context<Self>) {
        self.volume(0.05, cx);
    }
    fn quieter(&mut self, _: &Quieter, _: &mut Window, cx: &mut Context<Self>) {
        self.volume(-0.05, cx);
    }
    fn stop_action(&mut self, _: &Stop, w: &mut Window, cx: &mut Context<Self>) {
        self.stop(w, cx);
    }
    fn quit(&mut self, _: &Quit, w: &mut Window, cx: &mut Context<Self>) {
        self.shutdown(w, cx);
        cx.quit();
    }
    fn video_slot(&self, height: Option<f32>) -> AnyElement {
        let cell = self.video_bounds.clone();
        let mut slot = div()
            .relative()
            .w_full()
            .min_h(px(220.))
            .rounded_lg()
            .overflow_hidden();
        slot = if let Some(height) = height {
            slot.h(px(height))
        } else {
            slot.h_full()
        };
        slot = slot.child(
            canvas(
                move |bounds, _, _| {
                    cell.set(bounds);
                },
                |_, _, _, _| {},
            )
            .size_full(),
        );
        if !self.playing && self.page == Page::Session {
            slot = slot.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_3()
                    .child(img(self.brand.clone()).size(px(96.)).rounded_2xl())
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Твоя консоль. Твой экран."),
                    )
                    .child(
                        div()
                            .text_size(px(14.))
                            .text_color(rgb(0xb5b5b5))
                            .child("Выбери карту справа и подключи консоль"),
                    ),
            );
        }
        slot.into_any_element()
    }
    fn navigation(
        &mut self,
        compact: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut nav = div()
            .w(px(if compact { 76. } else { 224. }))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(0x171717));
        let mut brand = div()
            .h(px(72.))
            .flex_shrink_0()
            .px_3()
            .flex()
            .gap_3()
            .items_center()
            .bg(rgb(0x101010))
            .child(img(self.brand.clone()).size(px(48.)).rounded_xl());
        if compact {
            brand = brand.justify_center();
        }
        if !compact {
            brand = brand.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_size(px(15.))
                            .child("Console Link"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x949494))
                            .child("Игровое пространство"),
                    ),
            );
        }
        nav = nav.child(brand).child(div().h(px(16.)));
        for (page, icon, label) in [
            (Page::Session, "ui/console.svg", "Подключение"),
            (Page::Audio, "ui/audio.svg", "Звук"),
            (Page::Image, "ui/image.svg", "Изображение"),
            (Page::Discord, "ui/chat.svg", "Discord"),
            (Page::Settings, "ui/settings.svg", "Настройки"),
        ] {
            let selected = self.page == page;
            let mut item = div()
                .id(("nav", page as usize))
                .mx_2()
                .mb_1()
                .h(px(44.))
                .px_3()
                .rounded_full()
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .bg(rgb(if selected { 0x292929 } else { 0x171717 }))
                .text_color(rgb(if selected { 0xffffff } else { 0xb5b5b5 }))
                .hover(|s| s.bg(rgb(0x242424)).text_color(rgb(0xffffff)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = page;
                    this.selection = None;
                    if page == Page::Discord {
                        this.runtime.check_components();
                    }
                    cx.notify();
                }))
                .child(svg().path(icon).size(px(24.)));
            if compact {
                item = item.justify_center().px_0();
            }
            if !compact {
                item = item.child(
                    div()
                        .text_size(px(14.))
                        .font_weight(FontWeight::MEDIUM)
                        .child(label),
                );
            }
            nav = nav.child(item);
        }
        nav = nav.child(div().flex_1());
        let account = self.runtime.discord_account();
        let mut footer = div()
            .h(px(64.))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .bg(rgb(0x101010));
        if compact {
            footer = footer.justify_center();
        }
        let avatar = account.as_ref().and_then(|a| a.avatar_url.clone());
        if let Some(image) = self.artwork_image(avatar, window, cx) {
            footer = footer.child(img(image).size(px(32.)).rounded_full());
        } else {
            footer = footer.child(
                div()
                    .size(px(32.))
                    .rounded_full()
                    .bg(rgb(0xf2f2f2))
                    .text_color(rgb(0x101010))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(svg().path("ui/console.svg").size(px(18.))),
            );
        }
        if !compact {
            footer = footer.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(
                                account.map_or_else(|| "Console Link".into(), |a| a.display_name),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0x949494))
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            );
        }
        nav.child(footer).into_any_element()
    }
    fn page_header(&self) -> AnyElement {
        let (icon, title) = match self.page {
            Page::Session => ("ui/console.svg", "Консоль"),
            Page::Audio => ("ui/audio.svg", "Звук"),
            Page::Image => ("ui/image.svg", "Изображение"),
            Page::Discord => ("ui/chat.svg", "Discord"),
            Page::Settings => ("ui/settings.svg", "Настройки"),
        };
        div()
            .h(px(56.))
            .flex_shrink_0()
            .px_6()
            .flex()
            .items_center()
            .gap_3()
            .bg(rgb(0x101010))
            .border_b_1()
            .border_color(rgb(0x242424))
            .child(svg().path(icon).size(px(22.)).text_color(rgb(0x949494)))
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .into_any_element()
    }
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.redraw = true;
        self.artwork.begin_frame();
        let mut root = div()
            .id("console-link")
            .key_context("ConsoleDesktop")
            .track_focus(&self.focus)
            .relative()
            .size_full()
            .overflow_hidden()
            .text_color(rgb(0xeeeeee))
            .font_family("Inter")
            .on_action(cx.listener(Self::toggle_overlay))
            .on_action(cx.listener(Self::escape))
            .on_action(cx.listener(Self::fullscreen))
            .on_action(cx.listener(Self::cycle))
            .on_action(cx.listener(Self::take_screenshot))
            .on_action(cx.listener(Self::mute))
            .on_action(cx.listener(Self::louder))
            .on_action(cx.listener(Self::quieter))
            .on_action(cx.listener(Self::stop_action))
            .on_action(cx.listener(Self::quit));
        if self.presenter.is_none() || self.renderer.is_none() {
            root = root.bg(rgb(0x090909));
        }
        if !self.playing || self.overlay {
            let compact = self.playing
                || self.page == Page::Session
                || f32::from(window.viewport_size().width) < 1050.;
            let navigation = self.navigation(compact, window, cx);
            let message = self
                .runtime
                .error
                .clone()
                .or_else(|| self.gpu_error.clone())
                .or_else(|| self.runtime.notice.clone());
            let header = self.page_header();
            let mut body = div().flex_1().min_h_0().min_w_0().flex().flex_col();
            if !self.playing && self.page == Page::Session {
                let mut setup = div()
                    .id("connection-panel")
                    .w(px(320.))
                    .flex_shrink_0()
                    .h_full()
                    .overflow_y_scroll()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .bg(rgb(0x171717));
                setup = setup.child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0xb5b5b5))
                        .child("ПОДКЛЮЧЕНИЕ"),
                );
                if let Some(message) = message {
                    setup = setup.child(
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(0x101010))
                            .text_size(px(12.))
                            .child(message),
                    );
                }
                setup = setup.child(self.page_content(window, cx));
                let stage = div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_6()
                    .child(self.video_slot(None));
                body = body.child(div().size_full().flex().child(stage).child(setup));
            } else {
                let mut scroll = div()
                    .id("page-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_4();
                if !self.playing && self.page == Page::Image {
                    let width = (f32::from(window.viewport_size().width)
                        - if compact { 76. } else { 224. }
                        - 48.)
                        .max(320.);
                    let height =
                        (width * 9. / 16.).min(f32::from(window.viewport_size().height) * 0.58);
                    scroll = scroll.child(self.video_slot(Some(height)));
                }
                if let Some(message) = message {
                    scroll = scroll.child(
                        div()
                            .p_3()
                            .rounded_md()
                            .bg(rgb(0x171717))
                            .text_size(px(13.))
                            .child(message),
                    );
                }
                scroll = scroll.child(self.page_content(window, cx));
                if self.playing {
                    scroll = scroll.child(button(
                        "resume",
                        "Вернуться к игре · Esc",
                        true,
                        cx.listener(|this, _, window, cx| {
                            this.overlay = false;
                            this.focus.focus(window);
                            cx.notify();
                        }),
                    ));
                }
                body = body.child(scroll);
            }
            let content = div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(header)
                .child(body);
            let layout = div().flex().size_full().child(navigation).child(content);
            root = if self.playing {
                root.child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom_0()
                        .w(px(f32::from(window.viewport_size().width).min(660.)))
                        .bg(rgb(0x101010))
                        .shadow_xl()
                        .child(layout),
                )
            } else {
                root.child(layout)
            };
        }
        if self.playing && !self.overlay && !self.cursor.is_hidden() {
            root = root.child(div().absolute().top(px(16.)).right(px(20.)).child(button(
                "overlay",
                "Меню · F1",
                false,
                cx.listener(|this, _, window, cx| {
                    this.overlay = true;
                    this.focus.focus(window);
                    cx.notify();
                }),
            )));
        }
        if self.playing
            && self
                .feedback
                .is_some_and(|t| t.elapsed() < Duration::from_millis(1200))
        {
            let volume = if self.runtime.settings.muted {
                0.
            } else {
                self.runtime.settings.gain
            };
            root = root.child(
                div()
                    .absolute()
                    .bottom(px(40.))
                    .left(relative(0.38))
                    .w(px(260.))
                    .p_4()
                    .rounded_xl()
                    .bg(rgba(0x181818ee))
                    .child(div().mb_2().child(if self.runtime.settings.muted {
                        "Звук выключен".into()
                    } else {
                        format!("Громкость {}%", (volume * 100.).round())
                    }))
                    .child(
                        div().h(px(4.)).rounded_full().bg(rgb(0x494949)).child(
                            div()
                                .h_full()
                                .w(relative(volume))
                                .rounded_full()
                                .bg(rgb(0xffffff)),
                        ),
                    ),
            );
        }
        root
    }
}

fn brand_image() -> Arc<RenderImage> {
    let mut image = image::load_from_memory(include_bytes!("../../assets/window-icon.png"))
        .expect("embedded application icon")
        .to_rgba8();
    for p in image.as_flat_samples_mut().samples.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(image)]))
}

fn synthetic_frame(index: u64) -> Arc<VideoFrame> {
    // Also serves as the inactive image-calibration card. No camera is opened.
    let (w, h) = (1920usize, 1080usize);
    let mut rgba = vec![0; w * h * 4];
    let colors = [
        [225, 225, 225],
        [200, 200, 40],
        [40, 200, 200],
        [40, 180, 40],
        [200, 40, 200],
        [200, 40, 40],
        [40, 40, 200],
        [20, 20, 20],
    ];
    for row in rgba.chunks_exact_mut(w * 4) {
        for (x, p) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let c = colors[x * 8 / w];
            p.copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
        let x = (index as usize * 8) % w;
        row[x * 4..x * 4 + 4].copy_from_slice(&[255, 255, 255, 255]);
    }
    Arc::new(VideoFrame {
        width: w as u32,
        height: h as u32,
        rgba,
        timestamp_ns: console_link::capture::monotonic_ns(),
    })
}

pub fn run() {
    let synthetic = std::env::args().any(|a| a == "--synthetic");
    let smoke = crate::argument("--smoke-seconds").and_then(|s| s.parse::<u64>().ok());
    let persistent = smoke.is_none() && !synthetic && !std::env::args().any(|a| a == "--isolated");
    Application::new()
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            widgets::install(cx);
            let _ = cx
                .text_system()
                .add_fonts(vec![Cow::Borrowed(include_bytes!(
                    "../../assets/fonts/InterVariable.ttf"
                ))]);
            let context = Some("ConsoleDesktop");
            let gameplay = Some("ConsoleDesktop && !ConsoleTextField");
            cx.bind_keys([
                KeyBinding::new("f1", ToggleOverlay, context),
                KeyBinding::new("escape", Escape, context),
                KeyBinding::new("f11", Fullscreen, context),
                KeyBinding::new("cmd-enter", Fullscreen, context),
                KeyBinding::new("alt-enter", Fullscreen, context),
                KeyBinding::new("f10", CycleMode, context),
                KeyBinding::new("f8", Screenshot, context),
                KeyBinding::new("cmd-shift-s", Screenshot, context),
                KeyBinding::new("ctrl-shift-s", Screenshot, context),
                KeyBinding::new("m", Mute, gameplay),
                KeyBinding::new("+", Louder, gameplay),
                KeyBinding::new("=", Louder, gameplay),
                KeyBinding::new("-", Quieter, gameplay),
                KeyBinding::new("cmd-shift-enter", Stop, context),
                KeyBinding::new("ctrl-shift-enter", Stop, context),
                KeyBinding::new("cmd-q", Quit, context),
                KeyBinding::new("ctrl-q", Quit, context),
            ]);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
            let handle = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        window_background: WindowBackgroundAppearance::Transparent,
                        titlebar: Some(TitlebarOptions {
                            title: Some("Console Link".into()),
                            ..Default::default()
                        }),
                        window_min_size: Some(size(px(760.), px(600.))),
                        ..Default::default()
                    },
                    move |window, cx| cx.new(|cx| Desktop::new(persistent, synthetic, window, cx)),
                )
                .expect("Console Link window");
            cx.on_app_quit(move |cx| {
                let _ = handle.update(cx, |this, window, cx| this.shutdown(window, cx));
                async {}
            })
            .detach();
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(8))
                        .await;
                    let active = handle.update(cx, |this, window, cx| {
                        this.tick(window, cx);
                        !this.closed
                    });
                    if !matches!(active, Ok(true)) {
                        break;
                    }
                }
            })
            .detach();
            if let Some(seconds) = smoke {
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_secs(seconds))
                        .await;
                    let _ = handle.update(cx, |this, window, cx| this.shutdown(window, cx));
                    let _ = cx.update(|cx| cx.quit());
                })
                .detach();
            }
            cx.activate(true);
        });
}
