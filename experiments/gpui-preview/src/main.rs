//! GPUI experiment: bounded source -> CPU BGRA conversion -> RenderImage.
mod source;

use console_link::capture::monotonic_ns;
use gpui::{
    AnyElement, App, Application, Bounds, ClickEvent, Context, FocusHandle, KeyBinding,
    RenderImage, SharedString, TitlebarOptions, Window, WindowBounds, WindowOptions, actions, div,
    img, prelude::*, px, rgb, size,
};
use image::{Frame, RgbaImage};
use source::{Source, SourceStats, VideoFrame};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

actions!(gpui_preview, [ToggleSettings, ToggleFullscreen, Quit]);

#[derive(Default)]
struct Metrics {
    ui_updates: u64,
    retired_images: u64,
    received_fps: f64,
    ui_fps: f64,
    queue_age_ms: f64,
    conversion_ms: f64,
    previous_received: u64,
    previous_ui: u64,
}

struct Preview {
    source: Source,
    image: Option<Arc<RenderImage>>,
    focus: FocusHandle,
    overlay: bool,
    borderless: bool,
    status: String,
    error: Option<String>,
    metrics: Metrics,
    started: Instant,
    rate_since: Instant,
    final_reported: bool,
}

impl Preview {
    fn new(source: Source, borderless: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus);
        let borderless_error = if borderless {
            configure_borderless(window).err()
        } else {
            None
        };
        print_window("created", window, borderless);
        let previous_received = source.stats().received;
        Self {
            source,
            image: None,
            focus,
            overlay: true,
            borderless,
            status: "Ожидание источника…".into(),
            error: borderless_error,
            metrics: Metrics {
                previous_received,
                ..Metrics::default()
            },
            started: Instant::now(),
            rate_since: Instant::now(),
            final_reported: false,
        }
    }

    fn toggle_settings(&mut self, _: &ToggleSettings, _: &mut Window, cx: &mut Context<Self>) {
        self.overlay = !self.overlay;
        cx.notify();
    }

    fn toggle_fullscreen(
        &mut self,
        _: &ToggleFullscreen,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.borderless {
            self.error = Some("В безрамочном опыте нативный fullscreen отключен. Запусти без --borderless для его проверки.".into());
        } else {
            window.toggle_fullscreen();
        }
        cx.notify();
    }

    fn update_frame(&mut self, window: &mut Window) {
        if let Some(status) = self.source.status() {
            match status {
                Ok(status) => self.status = status,
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(frame) = self.source.latest() {
            self.metrics.queue_age_ms =
                monotonic_ns().saturating_sub(frame.timestamp_ns) as f64 / 1_000_000.0;
            let started = Instant::now();
            if let Some(image) = render_image(frame) {
                if let Some(previous) = self.image.replace(image) {
                    // Arc drop alone does not evict the GPUI sprite atlas entry.
                    if let Err(error) = window.drop_image(previous) {
                        self.error = Some(format!("Не удалось освободить изображение: {error}"));
                    } else {
                        self.metrics.retired_images += 1;
                    }
                }
                self.metrics.ui_updates += 1;
                self.metrics.conversion_ms = started.elapsed().as_secs_f64() * 1000.0;
            } else {
                self.error = Some("Источник прислал неполный кадр.".into());
            }
        }
        let elapsed = self.rate_since.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            let stats = self.source.stats();
            self.metrics.received_fps = stats
                .received
                .saturating_sub(self.metrics.previous_received)
                as f64
                / elapsed;
            self.metrics.ui_fps = self
                .metrics
                .ui_updates
                .saturating_sub(self.metrics.previous_ui) as f64
                / elapsed;
            self.metrics.previous_received = stats.received;
            self.metrics.previous_ui = self.metrics.ui_updates;
            self.rate_since = Instant::now();
            self.print_metrics("METRICS", stats);
            print_window("sample", window, self.borderless);
        }
    }

    fn report_final(&mut self) {
        if !self.final_reported {
            self.print_metrics("FINAL", self.source.stats());
            self.final_reported = true;
        }
    }

    fn print_metrics(&self, kind: &str, stats: SourceStats) {
        eprintln!(
            "{kind} elapsed_s={:.2} received={} ui_updates={} source_dropped={} atlas_retired={} input_fps={:.2} ui_updates_fps={:.2} queue_age_ms={:.3} cpu_conversion_ms={:.3}",
            self.started.elapsed().as_secs_f64(),
            stats.received,
            self.metrics.ui_updates,
            stats.dropped,
            self.metrics.retired_images,
            self.metrics.received_fps,
            self.metrics.ui_fps,
            self.metrics.queue_age_ms,
            self.metrics.conversion_ms
        );
    }

    fn panel(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = if window.is_fullscreen() {
            "Полный экран"
        } else if self.borderless {
            "Без рамки · стартовый режим"
        } else {
            "Оконный"
        };
        div()
            .id("settings-panel")
            .overflow_y_scroll()
            .absolute()
            .right(px(20.))
            .top(px(20.))
            .bottom(px(20.))
            .w(px(340.))
            .p_5()
            .flex()
            .flex_col()
            .gap_3()
            .rounded_xl()
            .bg(rgb(0x111315))
            .border_1()
            .border_color(rgb(0x34383c))
            .shadow_lg()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x939aa3))
                            .child("CONSOLE LINK / GPUI"),
                    )
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(rgb(0x747d87))
                            .child("01"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(28.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Игровой экран"),
                    )
                    .child(
                        div().text_size(px(13.)).text_color(rgb(0x9199a3)).child(
                            "Экспериментальный интерфейс. Основное приложение не изменяется.",
                        ),
                    ),
            )
            .child(
                div()
                    .p_4()
                    .rounded_lg()
                    .border_1()
                    .border_color(rgb(0x2d3136))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(label("ИСТОЧНИК"))
                    .child(div().text_size(px(14.)).child(self.status.clone()))
                    .child(
                        div()
                            .text_size(px(12.))
                            .text_color(rgb(0x8b949f))
                            .child("CPU baseline · BGRA / RenderImage"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(metric(
                        "Вход, кадр/с",
                        format!("{:.1}", self.metrics.received_fps),
                    ))
                    .child(metric(
                        "UI updates/с",
                        format!("{:.1}", self.metrics.ui_fps),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(row("Режим окна", mode.to_owned()))
                    .child(row(
                        "Возраст до render()",
                        format!("{:.2} мс", self.metrics.queue_age_ms),
                    ))
                    .child(row(
                        "CPU-конвертация",
                        format!("{:.2} мс", self.metrics.conversion_ms),
                    ))
                    .child(row(
                        "Потери очереди",
                        self.source.stats().dropped.to_string(),
                    )),
            )
            .child(div().text_size(px(11.)).text_color(rgb(0x777f89)).child(
                "Это счетчики приложения. Present/display latency и задержка HDMI не измерены.",
            ))
            .child(div().flex_1())
            .child(if self.borderless {
                div()
                    .p_3()
                    .text_size(px(11.))
                    .text_color(rgb(0x858e98))
                    .child("Fullscreen проверяется отдельно, без --borderless.")
                    .into_any_element()
            } else {
                control(
                    "Полный экран · F11",
                    false,
                    cx.listener(|this, _, window, cx| {
                        this.toggle_fullscreen(&ToggleFullscreen, window, cx);
                        this.focus.focus(window);
                    }),
                )
            })
            .child(control(
                "Продолжить · Esc / Tab",
                true,
                cx.listener(|this, _, window, cx| {
                    this.overlay = false;
                    this.focus.focus(window);
                    cx.notify();
                }),
            ))
            .into_any_element()
    }
}

impl Render for Preview {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_frame(window);
        window.request_animation_frame();
        let mut root = div()
            .id("preview")
            .key_context("ConsoleLinkPreview")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::toggle_settings))
            .on_action(cx.listener(Self::toggle_fullscreen))
            .relative()
            .size_full()
            .overflow_hidden()
            .bg(rgb(0x070809))
            .text_color(rgb(0xf2f4f6));
        if let Some(image) = &self.image {
            root = root.child(
                div().absolute().inset_0().child(
                    img(Arc::clone(image))
                        .size_full()
                        .object_fit(gpui::ObjectFit::Contain),
                ),
            );
        }
        root = root.child(
            div()
                .absolute()
                .left(px(24.))
                .top(px(22.))
                .flex()
                .gap_3()
                .items_center()
                .child(div().size(px(7.)).rounded_full().bg(rgb(0xf3f5f7)))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child("CONSOLE LINK"),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0xa8b0ba))
                        .child("GPUI PREVIEW / CPU BASELINE"),
                ),
        );
        root = root.child(
            div()
                .absolute()
                .left(px(24.))
                .bottom(px(22.))
                .flex()
                .gap_3()
                .items_center()
                .child(control(
                    "Настройки · Tab",
                    false,
                    cx.listener(|this, _, window, cx| {
                        this.overlay = !this.overlay;
                        this.focus.focus(window);
                        cx.notify();
                    }),
                ))
                .child(div().text_size(px(12.)).child(format!(
                    "Вход {:.1} · UI {:.1} · очередь {}",
                    self.metrics.received_fps,
                    self.metrics.ui_fps,
                    self.source.stats().dropped
                ))),
        );
        if let Some(error) = &self.error {
            root = root.child(
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(60.))
                    .max_w(px(500.))
                    .p_4()
                    .rounded_lg()
                    .bg(rgb(0x212326))
                    .border_1()
                    .border_color(rgb(0x6b727b))
                    .child(error.clone()),
            );
        }
        if self.overlay {
            root = root.child(self.panel(window, cx));
        }
        root
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.report_final();
    }
}

#[cfg(target_os = "macos")]
fn configure_borderless(window: &Window) -> Result<(), String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSView, NSWindowStyleMask};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let _main_thread =
        MainThreadMarker::new().ok_or("Режим окна требует главного потока AppKit.")?;
    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("У окна отсутствует AppKit handle.".into());
    };
    // SAFETY: GPUI lends this live NSView; the main-thread check confines AppKit calls.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let native = view.window().ok_or("У окна отсутствует NSWindow.")?;
    let screen = native.screen().ok_or("У окна отсутствует экран.")?;
    // GPUI 0.2.2 titlebar=None retains NSTitled, which AppKit shifts below the menu bar.
    native.setStyleMask(NSWindowStyleMask::Borderless);
    native.setFrame_display(screen.frame(), true);
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn configure_borderless(_: &Window) -> Result<(), String> {
    Ok(())
}

fn print_window(phase: &str, window: &Window, borderless: bool) {
    eprintln!(
        "WINDOW phase={phase} bounds={:?} viewport={:?} scale={} fullscreen={} borderless={borderless}",
        window.bounds(),
        window.viewport_size(),
        window.scale_factor(),
        window.is_fullscreen()
    );
}

fn render_image(mut frame: VideoFrame) -> Option<Arc<RenderImage>> {
    if frame.width == 0 || frame.height == 0 {
        return None;
    }
    // RenderImage's bytes are BGRA despite image::RgbaImage's name.
    for pixel in frame.rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = RgbaImage::from_raw(frame.width, frame.height, frame.rgba)?;
    Some(Arc::new(RenderImage::new(vec![Frame::new(buffer)])))
}

fn label(text: &'static str) -> AnyElement {
    div()
        .text_size(px(10.))
        .text_color(rgb(0x858e98))
        .child(text)
        .into_any_element()
}

fn row(title: &'static str, value: String) -> AnyElement {
    div()
        .flex()
        .justify_between()
        .gap_3()
        .text_size(px(12.))
        .child(div().text_color(rgb(0x858e98)).child(title))
        .child(value)
        .into_any_element()
}

fn metric(title: &'static str, value: String) -> AnyElement {
    div()
        .flex_1()
        .p_4()
        .bg(rgb(0x1b1e22))
        .rounded_lg()
        .flex()
        .flex_col()
        .gap_2()
        .child(label(title))
        .child(
            div()
                .text_size(px(27.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(value),
        )
        .into_any_element()
}

fn control(
    label: &'static str,
    primary: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .id(label)
        .px_4()
        .py_3()
        .rounded_lg()
        .cursor_pointer()
        .bg(rgb(if primary { 0xf2f4f6 } else { 0x262a30 }))
        .text_color(rgb(if primary { 0x111315 } else { 0xe8edf2 }))
        .text_size(px(12.))
        .hover(|style| style.opacity(0.8))
        .child(label)
        .on_click(on_click)
        .into_any_element()
}

fn argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args();
    arguments.find(|argument| argument == name)?;
    Some(arguments.next().unwrap_or_else(|| {
        eprintln!("Аргумент {name} требует значение.");
        std::process::exit(2);
    }))
}

fn main() {
    if std::env::args().any(|argument| argument == "--list-devices") {
        match console_link::capture::enumerate() {
            Ok(devices) => {
                for (index, device) in devices.iter().enumerate() {
                    println!("{index}: {}", device.name);
                }
            }
            Err(error) => eprintln!("Не удалось получить список видеоустройств: {error}"),
        }
        return;
    }
    let capture = argument("--capture").map(|value| {
        value
            .parse::<usize>()
            .expect("--capture requires a video device index")
    });
    let smoke_seconds = argument("--smoke-seconds").map(|value| {
        value
            .parse::<u64>()
            .expect("--smoke-seconds requires integer seconds")
    });
    let borderless = std::env::args().any(|argument| argument == "--borderless");
    let fullscreen = std::env::args().any(|argument| argument == "--fullscreen");
    if borderless && fullscreen {
        eprintln!("--borderless и --fullscreen проверяются раздельно.");
        std::process::exit(2);
    }
    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("escape", ToggleSettings, None),
            KeyBinding::new("tab", ToggleSettings, None),
            KeyBinding::new("f11", ToggleFullscreen, None),
            KeyBinding::new("cmd-enter", ToggleFullscreen, None),
            KeyBinding::new("alt-enter", ToggleFullscreen, None),
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("ctrl-q", Quit, None),
        ]);
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        let normal = Bounds::centered(None, size(px(1240.), px(780.)), cx);
        let bounds = if borderless {
            cx.primary_display()
                .map(|display| display.bounds())
                .unwrap_or(normal)
        } else {
            normal
        };
        let source = capture
            .map(Source::capture)
            .unwrap_or_else(Source::synthetic);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(if fullscreen {
                        WindowBounds::Fullscreen(bounds)
                    } else {
                        WindowBounds::Windowed(bounds)
                    }),
                    titlebar: if borderless {
                        None
                    } else {
                        Some(TitlebarOptions {
                            title: Some(SharedString::from("Console Link · GPUI Preview")),
                            ..Default::default()
                        })
                    },
                    window_min_size: Some(size(px(760.), px(560.))),
                    ..Default::default()
                },
                move |window, cx| cx.new(|cx| Preview::new(source, borderless, window, cx)),
            )
            .expect("GPUI preview window could not open");
        if let Some(seconds) = smoke_seconds {
            // A render callback may stop while occluded; the smoke deadline must not.
            let timer = cx.background_executor().timer(Duration::from_secs(seconds));
            cx.spawn(async move |cx| {
                timer.await;
                let _ = window.update(cx, |view, window, _| {
                    view.report_final();
                    print_window("smoke-deadline", window, view.borderless);
                });
                let _ = cx.update(|cx| cx.quit());
            })
            .detach();
        }
        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_baseline_converts_rgba_to_gpui_bgra_without_losing_alpha() {
        let image = render_image(VideoFrame {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 90],
            timestamp_ns: 0,
        })
        .unwrap();
        assert_eq!(image.as_bytes(0).unwrap(), &[0, 0, 255, 255, 255, 0, 0, 90]);
        assert!(
            render_image(VideoFrame {
                width: 2,
                height: 1,
                rgba: vec![0; 7],
                timestamp_ns: 0,
            })
            .is_none()
        );
    }
}
