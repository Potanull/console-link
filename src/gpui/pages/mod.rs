use super::{Desktop, Page, theme, widgets};
use console_link::{
    color::{self, ColorPreset, ColorSettings},
    discord::auth::AuthStatus,
    runtime::ComponentStatus,
    settings::{EqualizerPreset, GameWindowMode},
    upscale::UpscaleMode,
};
use gpui::{
    AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px, relative, rgb,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selection {
    Video,
    AudioInput,
    AudioOutput,
    Resolution,
    Equalizer,
    ColorPreset,
    Upscale,
    FrameRate,
    RtxQuality,
    OutputResolution,
    NintendoProfile,
}

const EQ_FACTORY: [(&str, [f32; 10]); 5] = [
    ("Нейтральный", [0.; 10]),
    ("Бас", [4., 3., 2., 0., -1., -1., 0., 0., 0., 0.]),
    ("Речь", [-6., -4., -2., -1., 0., 2., 3., 2., 0., -2.]),
    ("Ночной", [-8., -6., -4., -2., 0., 1., 2., 0., -2., -4.]),
    ("Игровой", [-4., -3., -2., -1., 0., 1., 3., 2., 1., -1.]),
];

mod audio;
mod discord;
mod image;
mod session;
mod settings;

impl Desktop {
    pub(super) fn page_content(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.page {
            Page::Session => self.session_page(window, cx),
            Page::Audio => self.audio_page(cx),
            Page::Image => self.image_page(cx),
            Page::Discord => self.discord_page(window, cx),
            Page::Settings => self.settings_page(cx),
        }
    }

    fn select_box(
        &self,
        which: Selection,
        id: &'static str,
        selected: String,
        choices: Vec<(String, String, bool)>,
        cx: &Context<Self>,
        change: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let selected_entity = cx.entity().downgrade();
        let dismissed_entity = cx.entity().downgrade();
        widgets::select(
            id,
            selected,
            self.selection == Some(which),
            choices,
            cx.listener(move |this, _, _, cx| {
                this.selection = (this.selection != Some(which)).then_some(which);
                cx.notify();
            }),
            move |value, window, app| {
                let _ = selected_entity.update(app, |this, cx| {
                    this.selection = None;
                    change(this, value, window, cx);
                    cx.notify();
                });
            },
            move |_, app| {
                let _ = dismissed_entity.update(app, |this, cx| {
                    this.selection = None;
                    cx.notify();
                });
            },
        )
    }

    fn device_picker(
        &self,
        which: Selection,
        label: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let locked = self.runtime.active() || self.runtime.opening;
        let (id, selected_key, mut choices): (_, _, Vec<(String, String)>) = match which {
            Selection::Video => (
                "video-picker",
                self.runtime.settings.video_id.clone(),
                self.runtime
                    .devices
                    .video
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
            ),
            Selection::AudioInput => (
                "input-picker",
                self.runtime.settings.audio_input_id.clone(),
                self.runtime
                    .devices
                    .inputs
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
            ),
            Selection::AudioOutput => (
                "output-picker",
                self.runtime.settings.audio_output_id.clone(),
                self.runtime
                    .devices
                    .outputs
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
            ),
            Selection::Resolution => (
                "mode-picker",
                self.runtime
                    .settings
                    .mode
                    .as_ref()
                    .map(|m| format!("{}:{}", m.width, m.height))
                    .unwrap_or_default(),
                self.runtime
                    .devices
                    .video
                    .iter()
                    .find(|d| d.id == self.runtime.settings.video_id)
                    .map(|d| {
                        available_resolutions(&d.modes)
                            .into_iter()
                            .map(|(width, height)| {
                                (format!("{width}:{height}"), format!("{width} × {height}"))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            _ => unreachable!("device_picker only accepts device or resolution selections"),
        };
        if which != Selection::Resolution {
            choices.insert(0, (String::new(), "Не выбран".into()));
        }
        let selected = choices
            .iter()
            .find(|(key, _)| *key == selected_key)
            .map(|(_, label)| label.clone())
            .unwrap_or_else(|| "Не выбран".into());
        let control = if locked {
            text(selected)
        } else {
            self.select_box(
                which,
                id,
                selected,
                choices
                    .into_iter()
                    .map(|(key, label)| {
                        let selected = key == selected_key;
                        (key, label, selected)
                    })
                    .collect(),
                cx,
                move |this, value, _, _| {
                    match which {
                        Selection::Video => {
                            this.runtime.settings.video_id = value;
                            this.runtime.settings.mode = None;
                        }
                        Selection::AudioInput => this.runtime.settings.audio_input_id = value,
                        Selection::AudioOutput => this.runtime.settings.audio_output_id = value,
                        Selection::Resolution => {
                            if let Some((width, height)) =
                                value.split_once(':').and_then(|(width, height)| {
                                    Some((width.parse().ok()?, height.parse().ok()?))
                                })
                            {
                                this.runtime.select_resolution(width, height);
                            }
                        }
                        _ => unreachable!(
                            "device_picker only accepts device or resolution selections"
                        ),
                    }
                    if which != Selection::Resolution {
                        this.runtime.changed();
                    }
                },
            )
        };
        setting_control(label, control)
    }
}

fn resolution_label(mode: &console_link::capture::VideoMode) -> String {
    format!("{} × {}", mode.width, mode.height)
}

fn available_resolutions(modes: &[console_link::capture::VideoMode]) -> Vec<(u32, u32)> {
    let mut resolutions = Vec::new();
    for mode in modes {
        if console_link::capture::choose_mode(std::slice::from_ref(mode), &Default::default(), None)
            .is_none()
        {
            continue;
        }
        let resolution = (mode.width, mode.height);
        if !resolutions.contains(&resolution) {
            resolutions.push(resolution);
        }
    }
    resolutions
}

fn preset_key(key: &str) -> Option<(&str, usize)> {
    let (kind, index) = key.split_once(':')?;
    Some((kind, index.parse().ok()?))
}

fn setting_control(label: &'static str, control: AnyElement) -> AnyElement {
    div()
        .flex()
        .w_full()
        .flex_shrink_0()
        .items_center()
        .gap_4()
        .min_h(px(44.))
        .child(div().w(px(128.)).flex_shrink_0().child(caption(label)))
        .child(div().min_w_0().flex_1().child(control))
        .into_any_element()
}

fn column() -> gpui::Div {
    div().flex().flex_col().flex_shrink_0().min_w_0()
}
fn card(title: &str, description: &str) -> gpui::Div {
    column()
        .w_full()
        .gap_4()
        .p_5()
        .rounded_2xl()
        .bg(rgb(theme::SURFACE))
        .child(
            column()
                .w_full()
                .gap_2()
                .pb_1()
                .child(
                    div()
                        .text_size(px(17.))
                        .line_height(relative(1.3))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme::TEXT))
                        .child(title.to_owned()),
                )
                .when(!description.is_empty(), |s| {
                    s.child(hint(description.to_owned()))
                }),
        )
}
fn text(value: impl Into<SharedString>) -> AnyElement {
    div()
        .min_w_0()
        .flex_shrink_0()
        .text_size(px(14.))
        .line_height(relative(1.5))
        .text_color(rgb(theme::TEXT))
        .child(value.into())
        .into_any_element()
}
fn caption(value: impl Into<SharedString>) -> AnyElement {
    div()
        .min_w_0()
        .flex_shrink_0()
        .text_size(px(13.))
        .line_height(relative(1.4))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(theme::SECONDARY))
        .child(value.into())
        .into_any_element()
}
fn hint(value: impl Into<SharedString>) -> AnyElement {
    div()
        .min_w_0()
        .flex_shrink_0()
        .text_size(px(13.))
        .line_height(relative(1.5))
        .text_color(rgb(theme::SECONDARY))
        .child(value.into())
        .into_any_element()
}

fn action_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .flex()
        .min_w_0()
        .flex_shrink_0()
        .child(widgets::button(id, label, primary, on_click))
        .into_any_element()
}

fn switch(
    cx: &Context<Desktop>,
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    change: impl Fn(&mut Desktop, bool) + 'static,
) -> AnyElement {
    let weak = cx.entity().downgrade();
    widgets::toggle(id, label, checked, move |v, _, app| {
        let _ = weak.update(app, |s, cx| {
            change(s, v);
            s.runtime.changed();
            cx.notify();
        });
    })
}
fn ui_switch(
    cx: &Context<Desktop>,
    id: impl Into<ElementId>,
    label: &'static str,
    checked: bool,
    change: impl Fn(&mut Desktop, bool) + 'static,
) -> AnyElement {
    let weak = cx.entity().downgrade();
    widgets::toggle(id, label, checked, move |v, _, app| {
        let _ = weak.update(app, |s, cx| {
            change(s, v);
            cx.notify();
        });
    })
}

fn numeric(
    cx: &Context<Desktop>,
    id: impl Into<ElementId>,
    label: &'static str,
    value: f32,
    min: f32,
    max: f32,
    change: impl Fn(&mut Desktop, f32) + 'static,
) -> AnyElement {
    let weak = cx.entity().downgrade();
    let formatted = if label.ends_with('%') || label.ends_with("мс") || label == "Качество RTX"
    {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    div()
        .flex()
        .w_full()
        .min_w_0()
        .flex_shrink_0()
        .items_center()
        .gap_3()
        .min_h(px(40.))
        .child(div().flex_1().min_w_0().child(text(label)))
        .child(
            div()
                .w(relative(0.4))
                .min_w(px(110.))
                .flex_shrink_0()
                .child(widgets::slider(id, value, min, max, move |v, _, app| {
                    let _ = weak.update(app, |s, cx| {
                        change(s, v);
                        s.runtime.changed();
                        cx.notify();
                    });
                })),
        )
        .child(
            div()
                .w(px(56.))
                .flex_shrink_0()
                .py_1()
                .rounded_md()
                .bg(rgb(theme::ELEVATED))
                .text_size(px(13.))
                .text_color(rgb(theme::TEXT))
                .text_center()
                .whitespace_nowrap()
                .child(formatted),
        )
        .into_any_element()
}

fn friendly_presence(raw: &str) -> String {
    if raw.starts_with("Activity sent") {
        "Название игры и карточка подтверждены Discord."
    } else if raw.starts_with("Activity accepted but") {
        "Discord пока показывает имя приложения. Обнови настольный клиент Discord."
    } else if raw.starts_with("Activity accepted by") {
        "Активность принята; отображаемое название еще не подтверждено."
    } else if raw.starts_with("Desktop Discord account differs") {
        "В настольном Discord открыт другой аккаунт."
    } else if raw.starts_with("Waiting for desktop Discord") {
        "Ожидаем настольный Discord. Включи показ игровой активности в его настройках."
    } else if raw == "Off" {
        "Нет активного игрового статуса."
    } else {
        raw
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use console_link::capture::VideoMode;

    #[test]
    fn resolution_choices_do_not_duplicate_frame_rates() {
        let modes = [
            VideoMode {
                width: 1920,
                height: 1080,
                fps_num: 60,
                fps_den: 1,
            },
            VideoMode {
                width: 1920,
                height: 1080,
                fps_num: 50,
                fps_den: 1,
            },
            VideoMode {
                width: 1280,
                height: 720,
                fps_num: 60,
                fps_den: 1,
            },
            VideoMode {
                width: 1280,
                height: 720,
                fps_num: 30,
                fps_den: 1,
            },
        ];
        let mut modes = modes.to_vec();
        modes.push(VideoMode {
            width: 3840,
            height: 2160,
            fps_num: 60,
            fps_den: 1,
        });
        modes.push(VideoMode {
            width: 1600,
            height: 900,
            fps_num: 120,
            fps_den: 1,
        });
        assert_eq!(
            available_resolutions(&modes),
            [(1920, 1080), (1280, 720), (3840, 2160), (1600, 900)]
        );
        assert_eq!(resolution_label(&modes[0]), "1920 × 1080");
        assert_eq!(resolution_label(&modes[0]), resolution_label(&modes[1]));
    }
}
