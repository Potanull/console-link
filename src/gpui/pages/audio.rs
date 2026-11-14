use super::*;

impl Desktop {
    pub(super) fn audio_page(&self, cx: &Context<Self>) -> AnyElement {
        let peak = self.runtime.peak().clamp(0., 1.);
        let mut volume=card("Звук консоли","Громкость и отключение звука применяются сразу.")
            .child(self.device_picker(Selection::AudioInput,"Аудиовход карты",cx))
            .child(self.device_picker(Selection::AudioOutput,"Выход звука",cx))
            .child(numeric(cx,"gain","Громкость, %",self.runtime.settings.gain*100.,0.,100.,|s,v|s.runtime.settings.gain=v/100.))
            .child(switch(cx,"mute","Выключить звук",self.runtime.settings.muted,|s,v|s.runtime.settings.muted=v))
            .child(widgets::row("Уровень сигнала",format!("{:.0}%",peak*100.)))
            .child(div().h(px(5.)).flex_shrink_0().rounded_full().bg(rgb(theme::HOVER)).child(div().h_full().w(relative(peak)).rounded_full().bg(rgb(theme::TEXT))))
            .child(numeric(cx,"sync","Коррекция синхронизации, мс",self.runtime.settings.sync_ms as f32,-500.,500.,|s,v|s.runtime.settings.sync_ms=v.round() as i32))
            .child(hint("Плюс задерживает звук, минус задерживает видео. HDMI-задержка самой карты не меняется."));
        volume = volume.child(action_button(
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
            .w_full()
            .gap_6()
            .child(volume)
            .child(self.equalizer_card(cx))
            .into_any_element()
    }

    pub(super) fn equalizer_card(&self, cx: &Context<Self>) -> AnyElement {
        let mut choices: Vec<_> = EQ_FACTORY
            .iter()
            .enumerate()
            .map(|(i, (name, gains))| {
                (
                    format!("factory:{i}"),
                    (*name).to_owned(),
                    self.eq_selected.is_none() && self.runtime.settings.equalizer.gains == *gains,
                )
            })
            .collect();
        choices.extend(
            self.runtime
                .settings
                .equalizer_presets
                .iter()
                .enumerate()
                .map(|(i, preset)| {
                    (
                        format!("user:{i}"),
                        preset.name.clone(),
                        self.eq_selected == Some(i)
                            && self.runtime.settings.equalizer.gains == preset.gains,
                    )
                }),
        );
        let selected = choices
            .iter()
            .find(|(_, _, selected)| *selected)
            .map(|(_, label, _)| label.clone())
            .unwrap_or_else(|| "Свои настройки".into());
        let presets = self.select_box(
            Selection::Equalizer,
            "eq-presets",
            selected,
            choices,
            cx,
            |s, key, _, cx| {
                if let Some((kind, index)) = preset_key(&key) {
                    match kind {
                        "factory" => {
                            if let Some((_, gains)) = EQ_FACTORY.get(index) {
                                s.runtime.settings.equalizer.enabled = true;
                                s.runtime.settings.equalizer.gains = *gains;
                                s.eq_selected = None;
                                s.eq_input.update(cx, |f, cx| f.set_value("", cx));
                            }
                        }
                        "user" => {
                            if let Some(p) =
                                s.runtime.settings.equalizer_presets.get(index).cloned()
                            {
                                s.runtime.settings.equalizer.gains = p.gains;
                                s.runtime.settings.equalizer.enabled = true;
                                s.eq_selected = Some(index);
                                s.eq_input.update(cx, |f, cx| f.set_value(p.name, cx));
                            }
                        }
                        _ => {}
                    }
                    s.runtime.changed();
                }
            },
        );
        let mut bands = div().flex().w_full().flex_shrink_0().gap_2().py_3();
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
                            .text_size(px(12.))
                            .text_color(rgb(theme::SECONDARY))
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
                            .text_size(px(12.))
                            .text_color(rgb(theme::SECONDARY))
                            .child(freq),
                    ),
            );
        }
        card("Эквалайзер", "10 полос · частота в Гц · усиление в дБ")
            .child(switch(
                cx,
                "eq-on",
                "Обрабатывать звук консоли",
                self.runtime.settings.equalizer.enabled,
                |s, v| s.runtime.settings.equalizer.enabled = v,
            ))
            .child(setting_control("Пресет", presets))
            .child(bands)
            .child(hint(
                "Автоматический запас громкости защищает от перегрузки при усилении полос.",
            ))
            .child(caption("Название пресета"))
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

    pub(super) fn save_eq_preset(&mut self, update: bool, cx: &mut Context<Self>) {
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
}
