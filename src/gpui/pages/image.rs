use super::*;

impl Desktop {
    pub(super) fn image_page(&self, cx: &Context<Self>) -> AnyElement {
        let color = self.runtime.settings.color;
        let mut choices: Vec<_> = color::FACTORY_PRESETS
            .iter()
            .enumerate()
            .map(|(i, (name, settings))| {
                (
                    format!("factory:{i}"),
                    (*name).to_owned(),
                    self.color_selected.is_none() && color == *settings,
                )
            })
            .collect();
        choices.extend(self.runtime.settings.color_presets.iter().enumerate().map(
            |(i, preset)| {
                (
                    format!("user:{i}"),
                    preset.name.clone(),
                    self.color_selected == Some(i) && color == preset.settings,
                )
            },
        ));
        let selected = choices
            .iter()
            .find(|(_, _, selected)| *selected)
            .map(|(_, label, _)| label.clone())
            .unwrap_or_else(|| "Свои настройки".into());
        let presets = self.select_box(
            Selection::ColorPreset,
            "color-presets",
            selected,
            choices,
            cx,
            |s, key, _, cx| {
                if let Some((kind, index)) = preset_key(&key) {
                    match kind {
                        "factory" => {
                            if let Some((_, settings)) = color::FACTORY_PRESETS.get(index) {
                                s.runtime.settings.color = *settings;
                                s.color_selected = None;
                                s.preset_input.update(cx, |f, cx| f.set_value("", cx));
                            }
                        }
                        "user" => {
                            if let Some(p) = s.runtime.settings.color_presets.get(index).cloned() {
                                s.runtime.settings.color = p.settings;
                                s.color_selected = Some(index);
                                s.preset_input.update(cx, |f, cx| f.set_value(p.name, cx));
                            }
                        }
                        _ => {}
                    }
                    s.runtime.changed();
                }
            },
        );
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
        .child(setting_control("Пресет", presets))
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
        .child(action_button(
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
        grading = grading
            .child(caption("Название пресета"))
            .child(self.preset_input.clone())
            .child(
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
        let modes = [
            ("standard", UpscaleMode::Standard, "Стандартный"),
            ("nis", UpscaleMode::Nis, "NVIDIA NIS"),
            ("fsr1", UpscaleMode::Fsr1, "AMD FSR 1"),
            ("rtx", UpscaleMode::RtxVideo, "RTX Video"),
        ];
        let selected = modes
            .iter()
            .find(|(_, mode, _)| *mode == upscale.mode)
            .map(|(_, _, label)| *label)
            .unwrap_or("Стандартный");
        let mode_picker = self.select_box(
            Selection::Upscale,
            "upscale-mode",
            selected.to_owned(),
            modes
                .iter()
                .map(|(key, mode, label)| {
                    (
                        (*key).to_owned(),
                        (*label).to_owned(),
                        *mode == upscale.mode,
                    )
                })
                .collect(),
            cx,
            move |s, key, _, _| {
                if let Some((_, mode, _)) = modes.iter().find(|(value, _, _)| *value == key) {
                    s.runtime.settings.upscale.mode = *mode;
                    s.runtime.changed();
                }
            },
        );
        use console_link::upscale::OutputResolution;
        let outputs = [
            ("auto", OutputResolution::Auto, "Авто · размер окна"),
            ("qhd", OutputResolution::Qhd, "2K · 2560 × 1440"),
            ("uhd", OutputResolution::Uhd, "4K · 3840 × 2160"),
        ];
        let selected_output = outputs
            .iter()
            .find(|(_, value, _)| *value == upscale.output_resolution)
            .unwrap()
            .2;
        let output_picker = self.select_box(
            Selection::OutputResolution,
            "output-resolution",
            selected_output.into(),
            outputs
                .iter()
                .map(|(key, value, label)| {
                    (
                        (*key).into(),
                        (*label).into(),
                        *value == upscale.output_resolution,
                    )
                })
                .collect(),
            cx,
            move |s, key, _, _| {
                if let Some((_, value, _)) = outputs.iter().find(|(id, _, _)| *id == key) {
                    s.runtime.settings.upscale.output_resolution = *value;
                    s.runtime.changed();
                }
            },
        );
        let mut enhancements = card(
            "Масштабирование",
            "Улучшение применяется при увеличении изображения.",
        )
        .child(setting_control("Режим", mode_picker))
        .child(setting_control("Разрешение обработки", output_picker))
        .child(hint("2K и 4K задают размер обработки NIS, FSR или RTX, сохраняя пропорции. Исходное разрешение карты не меняется. Авто снижает нагрузку в небольшом окне."));
        match upscale.mode {
            UpscaleMode::Nis => enhancements=enhancements.child(numeric(cx,"nis-sharp","Резкость NIS",upscale.nis_sharpness,0.,1.,|s,v|s.runtime.settings.upscale.nis_sharpness=v)),
            UpscaleMode::Fsr1 => enhancements=enhancements.child(numeric(cx,"fsr-sharp","Резкость FSR",upscale.fsr_sharpness,0.,1.,|s,v|s.runtime.settings.upscale.fsr_sharpness=v)),
            UpscaleMode::RtxVideo => enhancements=enhancements.child(setting_control("Качество RTX", self.select_box(
                Selection::RtxQuality, "rtx-quality", console_link::upscale::rtx_quality_label(upscale.rtx_quality).into(),
                (1..=4).map(|level| (level.to_string(), console_link::upscale::rtx_quality_label(level).into(), level == upscale.rtx_quality)).collect(), cx,
                |s, value, _, _| { if let Ok(level) = value.parse::<u32>() { s.runtime.settings.upscale.rtx_quality=level.clamp(1,4); s.runtime.changed(); } }
            )))
                .child(hint("1 — меньше нагрузка, 4 — максимальная обработка. Это уровень алгоритма, а не процент качества исходника."))
                .child(hint("RTX Video требует Windows, совместимую NVIDIA RTX и установленный runtime. При недоступности используется стандартный режим.")),
            UpscaleMode::Standard => {}
        }
        if let Some(recipe) = &self.recipe {
            enhancements = enhancements.child(hint(format!(
                "Источник: {} × {} → обработка: {} × {} · {:?}",
                recipe.frame.width,
                recipe.frame.height,
                recipe.target[0],
                recipe.target[1],
                recipe.settings.mode
            )));
        }
        enhancements = enhancements.child(ui_switch(
            cx,
            "upscale-compare",
            "Сравнить со стандартным масштабированием",
            self.comparison,
            |s, v| s.comparison = v,
        ));
        column()
            .w_full()
            .gap_6()
            .child(grading)
            .child(enhancements)
            .into_any_element()
    }

    pub(super) fn save_color_preset(&mut self, update: bool, cx: &mut Context<Self>) {
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
}
