use super::{Desktop, Page, widgets};
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
}

const EQ_FACTORY: [(&str, [f32; 10]); 5] = [
    ("Нейтральный", [0.; 10]),
    ("Бас", [4., 3., 2., 0., -1., -1., 0., 0., 0., 0.]),
    ("Речь", [-6., -4., -2., -1., 0., 2., 3., 2., 0., -2.]),
    ("Ночной", [-8., -6., -4., -2., 0., 1., 2., 0., -2., -4.]),
    ("Игровой", [-4., -3., -2., -1., 0., 1., 3., 2., 1., -1.]),
];

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

    fn device_picker(
        &self,
        which: Selection,
        label: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let locked = self.runtime.active() || self.runtime.opening;
        let selected = match which {
            Selection::Video => self
                .runtime
                .devices
                .video
                .iter()
                .find(|d| d.id == self.runtime.settings.video_id)
                .map(|d| d.name.clone()),
            Selection::AudioInput => self
                .runtime
                .devices
                .inputs
                .iter()
                .find(|d| d.id == self.runtime.settings.audio_input_id)
                .map(|d| d.name.clone()),
            Selection::AudioOutput => self
                .runtime
                .devices
                .outputs
                .iter()
                .find(|d| d.id == self.runtime.settings.audio_output_id)
                .map(|d| d.name.clone()),
            Selection::Resolution => self.runtime.settings.mode.as_ref().map(resolution_label),
        }
        .unwrap_or_else(|| "Не выбран".into());
        let id = match which {
            Selection::Video => "video-picker",
            Selection::AudioInput => "input-picker",
            Selection::AudioOutput => "output-picker",
            Selection::Resolution => "mode-picker",
        };
        let mut content = column().gap_2().child(caption(label));
        if locked {
            content = content.child(text(selected));
        } else {
            content = content.child(widgets::button(
                id,
                selected,
                false,
                cx.listener(move |this, _, _, cx| {
                    this.selection = if this.selection == Some(which) {
                        None
                    } else {
                        Some(which)
                    };
                    cx.notify();
                }),
            ));
        }
        if self.selection == Some(which) && !locked {
            let mut choices: Vec<(String, String)> = match which {
                Selection::Video => self
                    .runtime
                    .devices
                    .video
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
                Selection::AudioInput => self
                    .runtime
                    .devices
                    .inputs
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
                Selection::AudioOutput => self
                    .runtime
                    .devices
                    .outputs
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone()))
                    .collect(),
                Selection::Resolution => self
                    .runtime
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
            };
            if which != Selection::Resolution {
                choices.insert(0, (String::new(), "Не выбран".into()));
            }
            let mut list = column()
                .id((id, usize::MAX))
                .gap_1()
                .max_h(px(220.))
                .overflow_y_scroll()
                .p_2()
                .rounded_md()
                .bg(rgb(0x1e1f22));
            for (i, (value, name)) in choices.into_iter().enumerate() {
                list = list.child(widgets::button(
                    (id, i),
                    name,
                    false,
                    cx.listener(move |this, _, _, cx| {
                        match which {
                            Selection::Video => {
                                this.runtime.settings.video_id = value.clone();
                                this.runtime.settings.mode = None;
                            }
                            Selection::AudioInput => {
                                this.runtime.settings.audio_input_id = value.clone()
                            }
                            Selection::AudioOutput => {
                                this.runtime.settings.audio_output_id = value.clone()
                            }
                            Selection::Resolution => {
                                if let Some((width, height)) =
                                    value.split_once(':').and_then(|(width, height)| {
                                        Some((width.parse().ok()?, height.parse().ok()?))
                                    })
                                {
                                    this.runtime.select_resolution(width, height);
                                }
                            }
                        }
                        this.selection = None;
                        if which != Selection::Resolution {
                            this.runtime.changed();
                        }
                        cx.notify();
                    }),
                ));
            }
            content = content.child(list);
        }
        content.into_any_element()
    }

    fn session_page(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let mode = self
            .runtime
            .settings
            .mode
            .as_ref()
            .map(resolution_label)
            .unwrap_or_else(|| "1920 × 1080 · автоматически".into());
        let mut source = card(
            "Подключение консоли",
            "Подключи HDMI от консоли к карте захвата, затем карту к компьютеру.",
        )
        .child(self.device_picker(Selection::Video, "Карта захвата", cx))
        .child(widgets::row("Разрешение", mode))
        .child(hint(
            "По умолчанию выбираем Full HD и ближайшую поддерживаемую частоту до 60 FPS.",
        ));
        if self.runtime.opening {
            source = source
                .child(text("Подключаем консоль…"))
                .child(widgets::button(
                    "cancel-connect",
                    "Отменить",
                    false,
                    cx.listener(|this, _, window, cx| this.stop(window, cx)),
                ));
        } else if self.runtime.active() {
            source = source.child(widgets::button(
                "stop-session",
                "Завершить сеанс",
                false,
                cx.listener(|this, _, window, cx| this.stop(window, cx)),
            ));
        } else {
            if self.runtime.ready() {
                source = source.child(widgets::button(
                    "start-session",
                    "Подключить консоль",
                    true,
                    cx.listener(|this, _, window, cx| this.connect(window, cx)),
                ));
            } else {
                source = source.child(hint(if self.runtime.refreshing {
                    "Ищем карты захвата…"
                } else {
                    "Выбери карту и доступный режим. Для звука нужны и аудиовход, и выход."
                }));
            }
            if !self.runtime.refreshing {
                source = source.child(widgets::button(
                    "refresh-video",
                    "Обновить устройства",
                    false,
                    cx.listener(|this, _, _, cx| {
                        this.runtime.refresh();
                        cx.notify();
                    }),
                ));
            }
        }
        let mut body = column().gap_4().child(source).child(
            card(
                "Звук консоли",
                "Управляй выходом и громкостью отдельно от остальных приложений.",
            )
            .child(self.device_picker(Selection::AudioInput, "Аудиовход карты", cx))
            .child(self.device_picker(Selection::AudioOutput, "Куда выводить звук", cx))
            .child(hint(if self.runtime.active() {
                "Чтобы сменить устройства, заверши сеанс."
            } else {
                "Для подключения без звука оставь оба устройства не выбранными."
            })),
        );
        if !self.runtime.devices.warnings.is_empty() {
            body = body.child(
                card("Устройства требуют внимания", "")
                    .children(self.runtime.devices.warnings.iter().cloned().map(hint)),
            );
        }
        body.into_any_element()
    }

    fn audio_page(&self, cx: &Context<Self>) -> AnyElement {
        let peak = self.runtime.peak().clamp(0., 1.);
        let mut volume=card("Звук консоли","Громкость и mute применяются сразу.")
            .child(self.device_picker(Selection::AudioInput,"Аудиовход карты",cx))
            .child(self.device_picker(Selection::AudioOutput,"Аудиовыход",cx))
            .child(numeric(cx,"gain","Громкость, %",self.runtime.settings.gain*100.,0.,100.,|s,v|s.runtime.settings.gain=v/100.))
            .child(switch(cx,"mute","Выключить звук",self.runtime.settings.muted,|s,v|s.runtime.settings.muted=v))
            .child(widgets::row("Уровень сигнала",format!("{:.0}%",peak*100.)))
            .child(div().h(px(5.)).rounded_full().bg(rgb(0x4e5058)).child(div().h_full().w(relative(peak)).rounded_full().bg(rgb(0xf2f3f5))))
            .child(numeric(cx,"sync","Коррекция синхронизации, мс",self.runtime.settings.sync_ms as f32,-500.,500.,|s,v|s.runtime.settings.sync_ms=v.round() as i32))
            .child(hint("Плюс задерживает звук, минус задерживает видео. HDMI-задержка самой карты не меняется."));
        volume = volume.child(widgets::button(
            "reset-sync",
            "Сбросить синхронизацию",
            false,
            cx.listener(|s, _, _, cx| {
                s.runtime.settings.sync_ms = 0;
                s.runtime.changed();
                cx.notify();
            }),
        ));
        column()
            .gap_4()
            .child(volume)
            .child(self.equalizer_card(cx))
            .into_any_element()
    }

    fn equalizer_card(&self, cx: &Context<Self>) -> AnyElement {
        let mut presets = div().flex().flex_wrap().gap_2();
        for (i, (name, gains)) in EQ_FACTORY.into_iter().enumerate() {
            presets = presets.child(widgets::button(
                ("eq-factory", i),
                name,
                self.eq_selected.is_none() && self.runtime.settings.equalizer.gains == gains,
                cx.listener(move |s, _, _, cx| {
                    s.runtime.settings.equalizer.enabled = true;
                    s.runtime.settings.equalizer.gains = gains;
                    s.eq_selected = None;
                    s.eq_input.update(cx, |f, cx| f.set_value("", cx));
                    s.runtime.changed();
                    cx.notify();
                }),
            ));
        }
        let mut bands = div().flex().w_full().gap_2().py_3();
        for (i, freq) in [
            "31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
        ]
        .into_iter()
        .enumerate()
        {
            let weak = cx.entity().downgrade();
            let value = self.runtime.settings.equalizer.gains[i];
            bands = bands.child(
                column()
                    .flex_1()
                    .min_w(px(0.))
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0xb5bac1))
                            .child(format!("{value:+.0}")),
                    )
                    .child(widgets::vertical_slider(
                        ("eq-band", i),
                        value,
                        -12.,
                        12.,
                        move |v, _, app| {
                            let _ = weak.update(app, |s, cx| {
                                s.runtime.settings.equalizer.gains[i] = v;
                                s.runtime.changed();
                                cx.notify();
                            });
                        },
                    ))
                    .child(
                        div()
                            .text_size(px(10.))
                            .text_color(rgb(0xb5bac1))
                            .child(freq),
                    ),
            );
        }
        let mut users = column().gap_2();
        for (i, preset) in self.runtime.settings.equalizer_presets.iter().enumerate() {
            users = users.child(widgets::button(
                ("eq-user", i),
                preset.name.clone(),
                self.eq_selected == Some(i),
                cx.listener(move |s, _, _, cx| {
                    if let Some(p) = s.runtime.settings.equalizer_presets.get(i).cloned() {
                        s.runtime.settings.equalizer.gains = p.gains;
                        s.runtime.settings.equalizer.enabled = true;
                        s.eq_selected = Some(i);
                        s.eq_input.update(cx, |f, cx| f.set_value(p.name, cx));
                        s.runtime.changed();
                        cx.notify();
                    }
                }),
            ));
        }
        card("Эквалайзер", "10 полос · частота в Гц · усиление в дБ")
            .child(switch(
                cx,
                "eq-on",
                "Обрабатывать звук консоли",
                self.runtime.settings.equalizer.enabled,
                |s, v| s.runtime.settings.equalizer.enabled = v,
            ))
            .child(presets)
            .child(bands)
            .child(hint(
                "Автоматический запас громкости защищает от перегрузки при усилении полос.",
            ))
            .child(users)
            .child(self.eq_input.clone())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(widgets::button(
                        "eq-save",
                        "Сохранить копию",
                        false,
                        cx.listener(|s, _, _, cx| s.save_eq_preset(false, cx)),
                    ))
                    .child(widgets::button(
                        "eq-update",
                        "Обновить / переименовать",
                        false,
                        cx.listener(|s, _, _, cx| s.save_eq_preset(true, cx)),
                    ))
                    .child(widgets::button(
                        "eq-delete",
                        "Удалить выбранный",
                        false,
                        cx.listener(|s, _, _, cx| {
                            if let Some(i) = s
                                .eq_selected
                                .take()
                                .filter(|i| *i < s.runtime.settings.equalizer_presets.len())
                            {
                                s.runtime.settings.equalizer_presets.remove(i);
                                s.eq_input.update(cx, |f, cx| f.set_value("", cx));
                                s.runtime.changed();
                                cx.notify();
                            }
                        }),
                    )),
            )
            .into_any_element()
    }

    fn save_eq_preset(&mut self, update: bool, cx: &mut Context<Self>) {
        let name = color::preset_name(self.eq_input.read(cx).value());
        let selected = if update { self.eq_selected } else { None };
        if name.is_empty()
            || self
                .runtime
                .settings
                .equalizer_presets
                .iter()
                .enumerate()
                .any(|(i, p)| Some(i) != selected && p.name.to_lowercase() == name.to_lowercase())
        {
            self.runtime.error = Some("Дай пресету уникальное название.".into());
            cx.notify();
            return;
        }
        let preset = EqualizerPreset {
            name,
            gains: self.runtime.settings.equalizer.gains,
        };
        if update {
            if let Some(i) = selected.filter(|i| *i < self.runtime.settings.equalizer_presets.len())
            {
                self.runtime.settings.equalizer_presets[i] = preset;
            } else {
                self.runtime.error = Some("Сначала выбери свой пресет.".into());
                cx.notify();
                return;
            }
        } else if self.runtime.settings.equalizer_presets.len() < 64 {
            self.runtime.settings.equalizer_presets.push(preset);
            self.eq_selected = Some(self.runtime.settings.equalizer_presets.len() - 1);
        } else {
            self.runtime.error = Some("Можно сохранить не больше 64 пресетов.".into());
            cx.notify();
            return;
        }
        self.runtime.error = None;
        self.runtime.notice = Some("Пресет эквалайзера сохранен.".into());
        self.runtime.changed();
        cx.notify();
    }

    fn image_page(&self, cx: &Context<Self>) -> AnyElement {
        let color = self.runtime.settings.color;
        let mut factory = div().flex().flex_wrap().gap_2();
        for (i, (name, settings)) in color::FACTORY_PRESETS.into_iter().enumerate() {
            factory = factory.child(widgets::button(
                ("color-factory", i),
                name,
                self.color_selected.is_none() && self.runtime.settings.color == settings,
                cx.listener(move |s, _, _, cx| {
                    s.runtime.settings.color = settings;
                    s.color_selected = None;
                    s.preset_input.update(cx, |f, cx| f.set_value("", cx));
                    s.runtime.changed();
                    cx.notify();
                }),
            ));
        }
        let mut grading = card(
            "Цвет изображения",
            "Все изменения применяются к изображению консоли.",
        )
        .child(switch(
            cx,
            "color-on",
            "Цветокоррекция",
            color.enabled,
            |s, v| s.runtime.settings.color.enabled = v,
        ))
        .child(factory)
        .child(numeric(
            cx,
            "exposure",
            "Экспозиция, EV",
            color.exposure,
            -2.,
            2.,
            |s, v| s.runtime.settings.color.exposure = v,
        ))
        .child(numeric(
            cx,
            "contrast",
            "Контраст",
            color.contrast,
            0.5,
            1.5,
            |s, v| s.runtime.settings.color.contrast = v,
        ))
        .child(numeric(
            cx,
            "saturation",
            "Насыщенность",
            color.saturation,
            0.,
            2.,
            |s, v| s.runtime.settings.color.saturation = v,
        ))
        .child(numeric(
            cx,
            "temperature",
            "Температура",
            color.temperature,
            -1.,
            1.,
            |s, v| s.runtime.settings.color.temperature = v,
        ))
        .child(numeric(
            cx,
            "shadows",
            "Тени",
            color.shadows,
            -1.,
            1.,
            |s, v| s.runtime.settings.color.shadows = v,
        ))
        .child(numeric(
            cx,
            "highlights",
            "Светлые участки",
            color.highlights,
            -1.,
            1.,
            |s, v| s.runtime.settings.color.highlights = v,
        ))
        .child(numeric(
            cx,
            "hdr",
            "HDR-эффект",
            color.hdr,
            0.,
            1.,
            |s, v| s.runtime.settings.color.hdr = v,
        ))
        .child(hint(
            "HDR-эффект меняет тональную кривую SDR. Приложение не выводит настоящий HDR.",
        ))
        .child(ui_switch(
            cx,
            "original",
            "Показать оригинал без цветокоррекции",
            self.original,
            |s, v| s.original = v,
        ))
        .child(widgets::button(
            "color-reset",
            "Сбросить цвет",
            false,
            cx.listener(|s, _, _, cx| {
                s.runtime.settings.color = ColorSettings::default();
                s.color_selected = None;
                s.runtime.changed();
                cx.notify();
            }),
        ));
        for (i, preset) in self.runtime.settings.color_presets.iter().enumerate() {
            grading = grading.child(widgets::button(
                ("color-user", i),
                preset.name.clone(),
                self.color_selected == Some(i),
                cx.listener(move |s, _, _, cx| {
                    if let Some(p) = s.runtime.settings.color_presets.get(i).cloned() {
                        s.runtime.settings.color = p.settings;
                        s.color_selected = Some(i);
                        s.preset_input.update(cx, |f, cx| f.set_value(p.name, cx));
                        s.runtime.changed();
                        cx.notify();
                    }
                }),
            ));
        }
        grading = grading.child(self.preset_input.clone()).child(
            div()
                .flex()
                .flex_wrap()
                .gap_2()
                .child(widgets::button(
                    "color-save",
                    "Сохранить копию",
                    false,
                    cx.listener(|s, _, _, cx| s.save_color_preset(false, cx)),
                ))
                .child(widgets::button(
                    "color-update",
                    "Обновить / переименовать",
                    false,
                    cx.listener(|s, _, _, cx| s.save_color_preset(true, cx)),
                ))
                .child(widgets::button(
                    "color-delete",
                    "Удалить выбранный",
                    false,
                    cx.listener(|s, _, _, cx| {
                        if let Some(i) = s
                            .color_selected
                            .take()
                            .filter(|i| *i < s.runtime.settings.color_presets.len())
                        {
                            s.runtime.settings.color_presets.remove(i);
                            s.preset_input.update(cx, |f, cx| f.set_value("", cx));
                            s.runtime.changed();
                            cx.notify();
                        }
                    }),
                )),
        );
        let upscale = self.runtime.settings.upscale;
        let mut modes = div().flex().flex_wrap().gap_2();
        for (i, (mode, label)) in [
            (UpscaleMode::Standard, "Стандартный"),
            (UpscaleMode::Nis, "NVIDIA NIS"),
            (UpscaleMode::Fsr1, "AMD FSR 1"),
            (UpscaleMode::RtxVideo, "RTX Video"),
        ]
        .into_iter()
        .enumerate()
        {
            modes = modes.child(widgets::button(
                ("upscale-mode", i),
                label,
                upscale.mode == mode,
                cx.listener(move |s, _, _, cx| {
                    s.runtime.settings.upscale.mode = mode;
                    s.runtime.changed();
                    cx.notify();
                }),
            ));
        }
        let mut enhancements = card(
            "Масштабирование",
            "Улучшение применяется при увеличении изображения.",
        )
        .child(modes);
        match upscale.mode {
            UpscaleMode::Nis => enhancements=enhancements.child(numeric(cx,"nis-sharp","Резкость NIS",upscale.nis_sharpness,0.,1.,|s,v|s.runtime.settings.upscale.nis_sharpness=v)),
            UpscaleMode::Fsr1 => enhancements=enhancements.child(numeric(cx,"fsr-sharp","Резкость FSR",upscale.fsr_sharpness,0.,1.,|s,v|s.runtime.settings.upscale.fsr_sharpness=v)),
            UpscaleMode::RtxVideo => enhancements=enhancements.child(numeric(cx,"rtx-quality","Качество RTX",upscale.rtx_quality as f32,1.,4.,|s,v|s.runtime.settings.upscale.rtx_quality=v.round() as u32))
                .child(hint("RTX Video требует Windows, совместимую NVIDIA RTX и установленный runtime. При недоступности используется стандартный режим.")),
            UpscaleMode::Standard => {}
        }
        enhancements = enhancements.child(ui_switch(
            cx,
            "upscale-compare",
            "Сравнить со стандартным масштабированием",
            self.comparison,
            |s, v| s.comparison = v,
        ));
        column()
            .gap_4()
            .child(grading)
            .child(enhancements)
            .into_any_element()
    }

    fn save_color_preset(&mut self, update: bool, cx: &mut Context<Self>) {
        let name = color::preset_name(self.preset_input.read(cx).value());
        let selected = if update { self.color_selected } else { None };
        if name.is_empty()
            || self
                .runtime
                .settings
                .color_presets
                .iter()
                .enumerate()
                .any(|(i, p)| Some(i) != selected && p.name.to_lowercase() == name.to_lowercase())
        {
            self.runtime.error = Some("Дай пресету уникальное название.".into());
            cx.notify();
            return;
        }
        let preset = ColorPreset {
            name,
            settings: self.runtime.settings.color,
        };
        if update {
            if let Some(i) = selected.filter(|i| *i < self.runtime.settings.color_presets.len()) {
                self.runtime.settings.color_presets[i] = preset;
            } else {
                self.runtime.error = Some("Сначала выбери свой пресет.".into());
                cx.notify();
                return;
            }
        } else if self.runtime.settings.color_presets.len() < 64 {
            self.runtime.settings.color_presets.push(preset);
            self.color_selected = Some(self.runtime.settings.color_presets.len() - 1);
        } else {
            self.runtime.error = Some("Можно сохранить не больше 64 пресетов.".into());
            cx.notify();
            return;
        }
        self.runtime.error = None;
        self.runtime.notice = Some("Цветовой пресет сохранен.".into());
        self.runtime.changed();
        cx.notify();
    }

    fn discord_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let account = self.runtime.discord_status();
        let linked = matches!(&account, AuthStatus::Connected(_));
        let mut identity = card("Discord", "Твой профиль, название игры и обложка.");
        match account {
            AuthStatus::Connected(account) => {
                let mut profile = div().flex().items_center().gap_4();
                if let Some(avatar) = self.artwork_image(account.avatar_url, window, cx) {
                    profile = profile.child(
                        img(avatar)
                            .size(px(56.))
                            .rounded_full()
                            .object_fit(gpui::ObjectFit::Cover),
                    );
                } else {
                    profile = profile.child(
                        div()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(56.))
                            .rounded_full()
                            .bg(rgb(0x5865f2))
                            .child("D"),
                    );
                }
                identity = identity
                    .child(
                        profile.child(
                            column()
                                .gap_1()
                                .child(text(account.display_name))
                                .child(hint("Аккаунт Discord подключен")),
                        ),
                    )
                    .child(widgets::button(
                        "discord-logout",
                        "Отключить аккаунт",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_logout();
                            cx.notify();
                        }),
                    ));
            }
            AuthStatus::Connecting => {
                identity = identity
                    .child(text("Подтверди вход в Discord"))
                    .child(hint("После подтверждения вернись в приложение."))
                    .child(widgets::button(
                        "discord-cancel",
                        "Отменить вход",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_cancel();
                            cx.notify();
                        }),
                    ))
            }
            status => {
                if let AuthStatus::Error(error) = status {
                    identity = identity.child(hint(error));
                }
                identity = identity
                    .child(widgets::button(
                        "discord-login",
                        "Подключить Discord",
                        true,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_login();
                            cx.notify();
                        }),
                    ))
                    .child(widgets::button(
                        "discord-forget",
                        "Забыть сохраненный вход",
                        false,
                        cx.listener(|s, _, _, cx| {
                            s.runtime.discord_logout();
                            cx.notify();
                        }),
                    ));
            }
        }
        let game_name = self.runtime.game_name().to_owned();
        let has_game = !game_name.is_empty();
        let cover = self.runtime.cover_url();
        let mut game = div().flex().items_center().gap_4();
        if let Some(image) = self.artwork_image(cover, window, cx) {
            game = game.child(
                img(image)
                    .size(px(76.))
                    .rounded_lg()
                    .object_fit(gpui::ObjectFit::Cover),
            );
        }
        game = game.child(
            column()
                .gap_1()
                .child(text(if game_name.is_empty() {
                    "Игра пока не определена".to_owned()
                } else {
                    game_name
                }))
                .child(hint("Nintendo Switch")),
        );
        if has_game && (self.runtime.active() || self.runtime.settings.nintendo_enabled) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |t| t.as_secs() as i64);
            let minutes = now.saturating_sub(self.runtime.game_started_at()).max(0) / 60;
            game = game.child(hint(format!("В игре {minutes} мин")));
        }
        let presence = if !self.runtime.settings.discord_enabled {
            "Показ активности выключен.".to_owned()
        } else if !linked {
            "Подключи аккаунт для игрового статуса.".to_owned()
        } else if self.runtime.settings.nintendo_enabled && !has_game {
            "Ожидаем текущую игру из Nintendo.".to_owned()
        } else if !self.runtime.active() && !self.runtime.settings.nintendo_enabled {
            "Статус появится при подключении консоли.".to_owned()
        } else {
            friendly_presence(&self.runtime.presence_status())
        };
        let activity=card("Сейчас играешь","Название обновляется независимо от изображения карты захвата.")
            .child(game)
            .child(switch(cx,"discord-enabled","Показывать игру в Discord",self.runtime.settings.discord_enabled,|s,v|s.runtime.settings.discord_enabled=v))
            .child(hint(presence))
            .child(hint("Для демонстрации выбери игровое окно Console Link в настольном Discord. Передача изображения и звука управляется Discord."));
        let mut nintendo = card(
            "Определение игры Nintendo",
            "Получаем текущую игру выбранного профиля Nintendo.",
        )
        .child(switch(
            cx,
            "nintendo-enabled",
            "Определять игру автоматически",
            self.runtime.settings.nintendo_enabled,
            |s, v| s.runtime.settings.nintendo_enabled = v,
        ));
        let components = self.runtime.components_status();
        let component_text = match components {
            ComponentStatus::Unknown => "Проверка компонентов еще не выполнена.".to_owned(),
            ComponentStatus::Missing => "Компоненты Nintendo не установлены.".to_owned(),
            ComponentStatus::Installing(stage) => stage.label().to_owned(),
            ComponentStatus::Ready => "Компоненты Nintendo установлены.".to_owned(),
            ComponentStatus::Failed(error) => error.clone(),
            ComponentStatus::Cancelled => "Установка отменена.".to_owned(),
        };
        nintendo = nintendo.child(hint(component_text));
        if matches!(components, ComponentStatus::Installing(_)) {
            nintendo = nintendo.child(widgets::button(
                "nintendo-cancel-install",
                "Отменить установку",
                false,
                cx.listener(|s, _, _, cx| {
                    s.runtime.cancel_install();
                    cx.notify();
                }),
            ));
        } else {
            nintendo = nintendo
                .child(widgets::button(
                    "nintendo-install",
                    "Установить / восстановить компоненты",
                    false,
                    cx.listener(|s, _, _, cx| {
                        s.runtime.install_components();
                        cx.notify();
                    }),
                ))
                .child(widgets::button(
                    "nintendo-check",
                    "Проверить компоненты",
                    false,
                    cx.listener(|s, _, _, cx| {
                        s.runtime.check_components();
                        cx.notify();
                    }),
                ));
        }
        nintendo=nintendo.child(hint("Node.js и nxapi устанавливаются только для Console Link. Системный Node не меняется. Вход в Nintendo настраивается отдельно."));
        for (i, friend) in self.runtime.nintendo_friends().iter().enumerate() {
            let id = friend.id.clone();
            nintendo = nintendo.child(widgets::button(
                ("nintendo-profile", i),
                friend.name.clone(),
                self.runtime.settings.nintendo_friend_id == id,
                cx.listener(move |s, _, _, cx| {
                    s.runtime.select_profile(id.clone());
                    cx.notify();
                }),
            ));
        }
        if self.runtime.nintendo_pending() {
            nintendo = nintendo.child(hint("Проверяем текущую игру…"));
        }
        if let Some(error) = self.runtime.nintendo_error() {
            nintendo = nintendo.child(hint(error.to_owned()));
        }
        nintendo = nintendo
            .child(widgets::button(
                "nintendo-refresh",
                "Обновить статус Nintendo",
                false,
                cx.listener(|s, _, _, cx| {
                    s.runtime.nintendo_refresh();
                    cx.notify();
                }),
            ))
            .child(hint(
                "Обновление раз в минуту. Если Nintendo не сообщает игру, статус очищается.",
            ));
        let mut manual = card(
            "Выбрать название вручную",
            "Ручной выбор отключает автоматическое определение Nintendo.",
        )
        .child(self.game_input.clone());
        if self.game_query.chars().count() >= 2 {
            for (i, entry) in self
                .runtime
                .search_games(&self.game_query)
                .into_iter()
                .take(6)
                .enumerate()
            {
                let name = entry.name;
                manual = manual.child(widgets::button(
                    ("game-result", i),
                    name.clone(),
                    false,
                    cx.listener(move |s, _, window, cx| s.choose_game(name.clone(), window, cx)),
                ));
            }
        }
        if !self.game_query.trim().is_empty() {
            manual = manual.child(widgets::button(
                "manual-game",
                "Использовать введенное название",
                true,
                cx.listener(|s, _, window, cx| {
                    let name = s.game_query.trim().to_owned();
                    s.choose_game(name, window, cx);
                }),
            ));
        }
        column()
            .gap_4()
            .child(identity)
            .child(activity)
            .child(nintendo)
            .child(manual)
            .into_any_element()
    }

    fn settings_page(&self, cx: &Context<Self>) -> AnyElement {
        let locked = self.runtime.active() || self.runtime.opening;
        let mut capture = card(
            "Изображение с карты",
            "По умолчанию Full HD и частота до 60 FPS.",
        );
        if locked {
            capture = capture.child(hint(
                "Изменить режим захвата можно после завершения сеанса.",
            ));
        } else {
            capture = capture.child(switch(
                cx,
                "automatic-capture",
                "Автоматическое разрешение",
                self.runtime.settings.capture_preferences.automatic,
                |s, v| s.runtime.settings.capture_preferences.automatic = v,
            ));
            let mut rates = div().flex().flex_wrap().gap_2();
            for fps in [60, 50, 30, 25, 24] {
                rates = rates.child(widgets::button(
                    ("target-fps", fps as usize),
                    format!("{fps} FPS"),
                    self.runtime.settings.capture_preferences.preferred_fps == fps,
                    cx.listener(move |s, _, _, cx| {
                        s.runtime.settings.capture_preferences.preferred_fps = fps;
                        s.runtime.changed();
                        cx.notify();
                    }),
                ));
            }
            capture = capture.child(caption("Целевая частота")).child(rates);
            if !self.runtime.settings.capture_preferences.automatic {
                capture =
                    capture.child(self.device_picker(Selection::Resolution, "Разрешение", cx));
            }
        }
        capture=capture.child(hint("Если Full HD или нужный FPS недоступны, используем поддерживаемый режим карты. Частота карты не подменяется выбранным числом."));
        let mut modes = div().flex().flex_wrap().gap_2();
        for (i, (mode, label)) in [
            (GameWindowMode::Windowed, "Оконный"),
            (GameWindowMode::Borderless, "Без границ"),
            (GameWindowMode::Fullscreen, "Полный экран"),
        ]
        .into_iter()
        .enumerate()
        {
            modes = modes.child(widgets::button(
                ("window-mode", i),
                label,
                self.runtime.settings.game_window_mode == mode,
                cx.listener(move |s, _, window, cx| s.set_mode(mode, window, cx)),
            ));
        }
        let mut keys = column().gap_1();
        for (key, description) in [
            ("F1 / Tab", "Боковое меню"),
            ("F11 / Alt+Enter / ⌘Enter", "Полный экран"),
            ("F10", "Сменить режим окна"),
            ("F8 / Ctrl+Shift+S / ⌘⇧S", "Скриншот"),
            ("M", "Включить или выключить звук"),
            ("+ / -", "Громкость на 5%"),
            ("Esc", "Закрыть меню / выйти из полного экрана"),
            ("Ctrl+Shift+Enter / ⌘⇧Enter", "Завершить сеанс"),
        ] {
            keys = keys.child(widgets::row(key, description));
        }
        let window=card("Окно игры","Без границ занимает текущий монитор и остается обычным окном рабочего стола.").child(modes).child(keys)
            .child(hint("Клавиши работают в активном окне. M и +/- не мешают вводу текста. Курсор скрывается через 4 секунды без движения."));
        let screenshot = card("Скриншоты", "PNG без рамки окна и элементов управления.")
            .child(caption("Папка для сохранения"))
            .child(self.path_input.clone())
            .child(widgets::button(
                "screenshot-now",
                "Сделать скриншот",
                false,
                cx.listener(|s, _, window, cx| s.screenshot(window, cx)),
            ));
        column()
            .gap_4()
            .child(capture)
            .child(window)
            .child(screenshot)
            .child(self.monitor_card(cx))
            .into_any_element()
    }

    fn monitor_card(&self, cx: &Context<Self>) -> AnyElement {
        let elapsed = self.runtime.session_elapsed().as_secs();
        let audio = self
            .runtime
            .audio_counters()
            .map(|(under, over)| format!("{under} / {over}"))
            .unwrap_or_else(|| "Не подключено".into());
        let mut about=card("Об приложении",&format!("Console Link {} · Rust · GPUI · macOS и Windows",env!("CARGO_PKG_VERSION")))
            .child(widgets::row("Сеанс",format!("{:02}:{:02}:{:02}",elapsed/3600,elapsed/60%60,elapsed%60)))
            .child(widgets::row("Кадры, принятые интерфейсом/с",format!("{:.1}",self.runtime.fps)))
            .child(widgets::row("Память истории видео",format!("{:.1} МБ",self.runtime.history_bytes() as f64/1048576.)))
            .child(widgets::row("Пропущенные кадры",self.runtime.dropped_frames().to_string()))
            .child(widgets::row("Аудиобуфер: паузы / переполнения",audio))
            .child(widgets::row("Синхронизация",format!("{:+} мс",self.runtime.settings.sync_ms)))
            .child(hint("История за 60 секунд, шаг 0,5 с. Это счетчики приложения, не полная задержка от HDMI до экрана."));
        for (audio, title) in [
            (false, "Кадры интерфейса · FPS"),
            (true, "Пик выходного звука · %"),
        ] {
            let maximum = if audio {
                100.
            } else {
                self.runtime
                    .monitor
                    .samples
                    .iter()
                    .map(|s| s.fps)
                    .fold(60., f32::max)
            };
            let mut bars = div()
                .flex()
                .items_end()
                .w_full()
                .h(px(64.))
                .gap(px(1.))
                .rounded_md()
                .bg(rgb(0x1e1f22))
                .px_2()
                .py_2();
            for sample in &self.runtime.monitor.samples {
                let value = if audio {
                    sample.peak * 100.
                } else {
                    sample.fps
                };
                bars = bars.child(
                    div()
                        .flex_1()
                        .min_w(px(1.))
                        .h(relative((value / maximum).clamp(0., 1.)))
                        .rounded_sm()
                        .bg(rgb(0x80848e)),
                );
            }
            about = about.child(caption(title)).child(bars);
        }
        about
            .child(widgets::button(
                "repository",
                "Исходный код и загрузки",
                false,
                cx.listener(|_, _, _, _| {
                    let _ = webbrowser::open("https://github.com/Potanull/console-link");
                }),
            ))
            .into_any_element()
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

fn column() -> gpui::Div {
    div().flex().flex_col().min_w(px(0.))
}
fn card(title: &str, description: &str) -> gpui::Div {
    column()
        .w_full()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x383a40))
        .bg(rgb(0x2b2d31))
        .child(
            div()
                .text_size(px(16.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0xf2f3f5))
                .child(title.to_owned()),
        )
        .when(!description.is_empty(), |s| {
            s.child(hint(description.to_owned()))
        })
}
fn text(value: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(14.))
        .text_color(rgb(0xf2f3f5))
        .child(value.into())
        .into_any_element()
}
fn caption(value: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(rgb(0xb5bac1))
        .child(value.into())
        .into_any_element()
}
fn hint(value: impl Into<SharedString>) -> AnyElement {
    div()
        .text_size(px(13.))
        .text_color(rgb(0xb5bac1))
        .child(value.into())
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
    column()
        .gap_1()
        .child(widgets::row(label, format!("{value:.2}")))
        .child(widgets::slider(id, value, min, max, move |v, _, app| {
            let _ = weak.update(app, |s, cx| {
                change(s, v);
                s.runtime.changed();
                cx.notify();
            });
        }))
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
        assert_eq!(available_resolutions(&modes), [(1920, 1080), (1280, 720)]);
        assert_eq!(resolution_label(&modes[0]), "1920 × 1080");
        assert_eq!(resolution_label(&modes[0]), resolution_label(&modes[1]));
    }
}
