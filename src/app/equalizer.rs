use super::*;
use console_link::settings::EqualizerPreset;

const FACTORY: [(&str, [f32; 10]); 5] = [
    ("Нейтральный", [0.0; 10]),
    ("Бас", [4.0, 3.0, 2.0, 0.0, -1.0, -1.0, 0.0, 0.0, 0.0, 0.0]),
    (
        "Речь",
        [-6.0, -4.0, -2.0, -1.0, 0.0, 2.0, 3.0, 2.0, 0.0, -2.0],
    ),
    (
        "Ночной",
        [-8.0, -6.0, -4.0, -2.0, 0.0, 1.0, 2.0, 0.0, -2.0, -4.0],
    ),
    (
        "Игровой",
        [-4.0, -3.0, -2.0, -1.0, 0.0, 1.0, 3.0, 2.0, 1.0, -1.0],
    ),
];

impl ConsoleApp {
    pub(super) fn equalizer_card(&mut self, ui: &mut egui::Ui) {
        let before = self.settings.equalizer.clone();
        let compact = self.playback.playing;
        theme::card().inner_margin(if compact { 12 } else { 18 }).show(ui, |ui| {
            if compact { ui.spacing_mut().item_spacing.y = 4.0; ui.spacing_mut().interact_size.y = 24.0; }
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Эквалайзер").size(17.0).strong());
            theme::toggle(ui, &mut self.settings.equalizer.enabled, "Обрабатывать звук консоли");
            let selected = self.eq_selected.and_then(|i| self.settings.equalizer_presets.get(i));
            let label = selected.map(|p| p.name.as_str()).or_else(|| FACTORY.iter().find(|(_,gains)| *gains == self.settings.equalizer.gains).map(|(name,_)| *name)).unwrap_or("Свои настройки");
            let mut factory = None;
            let mut user = None;
            egui::ComboBox::from_id_salt("eq-presets").selected_text(label).width(ui.available_width()).show_ui(ui, |ui| {
                for (name, gains) in FACTORY {
                    if ui.selectable_label(self.eq_selected.is_none() && self.settings.equalizer.gains == gains, name).clicked() { factory = Some(gains); }
                }
                if !self.settings.equalizer_presets.is_empty() { ui.separator(); }
                for (i, preset) in self.settings.equalizer_presets.iter().enumerate() {
                    if ui.selectable_label(self.eq_selected == Some(i), &preset.name).clicked() { user = Some(i); }
                }
            });
            if let Some(gains) = factory {
                self.settings.equalizer.gains = gains;
                self.settings.equalizer.enabled = true;
                self.eq_selected = None;
                self.eq_name.clear();
            }
            if let Some(i) = user {
                let preset = &self.settings.equalizer_presets[i];
                self.settings.equalizer.gains = preset.gains;
                self.settings.equalizer.enabled = true;
                self.eq_name = preset.name.clone();
                self.eq_selected = Some(i);
            }
            ui.add_space(10.0);
            equalizer_bands(ui, &mut self.settings.equalizer.gains, compact);
            if !compact {
            theme::subtitle(ui, "Частота, Гц · усиление, дБ · автоматический запас громкости");
            theme::subtitle(ui, "Запас громкости защищает от перегрузки при усилении полос. «Ночной» смягчает бас и верхние частоты.");
            }
            ui.add_space(8.0);
            if compact {
                ui.collapsing("Мои пресеты", |ui| self.equalizer_presets_editor(ui));
            } else { self.equalizer_presets_editor(ui); }

        });
        if before != self.settings.equalizer {
            self.dirty = Some(Instant::now());
            if let Some(audio) = self.active.as_ref().and_then(|a| a.audio.as_ref()) {
                audio.set_equalizer(
                    self.settings.equalizer.enabled,
                    self.settings.equalizer.gains,
                );
            }
        }
    }
    fn equalizer_presets_editor(&mut self, ui: &mut egui::Ui) {
        ui.add(
            egui::TextEdit::singleline(&mut self.eq_name)
                .hint_text("Название своего пресета")
                .char_limit(48)
                .desired_width(ui.available_width()),
        );
        let name = console_link::color::preset_name(&self.eq_name);
        let duplicate = self
            .settings
            .equalizer_presets
            .iter()
            .enumerate()
            .any(|(i, p)| {
                Some(i) != self.eq_selected && p.name.to_lowercase() == name.to_lowercase()
            });
        let valid = !name.is_empty() && !duplicate;
        ui.horizontal_wrapped(|ui| {
            let new_duplicate = self
                .settings
                .equalizer_presets
                .iter()
                .any(|p| p.name.to_lowercase() == name.to_lowercase());
            if ui
                .add_enabled(
                    valid && !new_duplicate && self.settings.equalizer_presets.len() < 64,
                    egui::Button::new("Сохранить копию"),
                )
                .clicked()
            {
                self.settings.equalizer_presets.push(EqualizerPreset {
                    name: name.clone(),
                    gains: self.settings.equalizer.gains,
                });
                self.eq_selected = Some(self.settings.equalizer_presets.len() - 1);
                self.dirty = Some(Instant::now());
            }
            if ui
                .add_enabled(
                    valid && self.eq_selected.is_some(),
                    egui::Button::new("Обновить / переименовать"),
                )
                .clicked()
            {
                if let Some(i) = self.eq_selected {
                    self.settings.equalizer_presets[i] = EqualizerPreset {
                        name: name.clone(),
                        gains: self.settings.equalizer.gains,
                    };
                }
                self.dirty = Some(Instant::now());
            }
            if ui
                .add_enabled(self.eq_selected.is_some(), egui::Button::new("Удалить"))
                .clicked()
            {
                if let Some(i) = self.eq_selected.take() {
                    self.settings.equalizer_presets.remove(i);
                }
                self.eq_name.clear();
                self.dirty = Some(Instant::now());
            }
            if ui.button("Сбросить полосы").clicked() {
                self.settings.equalizer.gains = [0.0; 10];
            }
        });
        if duplicate {
            theme::subtitle(ui, "Пресет с таким названием уже существует.");
        }
    }
}

fn equalizer_bands(ui: &mut egui::Ui, gains: &mut [f32; 10], compact: bool) {
    let labels = [
        "31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
    ];
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = if compact { 3.0 } else { 8.0 };
        ui.columns(10, |uis| {
            for (i, ui) in uis.iter_mut().enumerate() {
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{:+.1}", gains[i])).size(if compact {
                                11.0
                            } else {
                                13.0
                            }),
                        )
                        .wrap_mode(egui::TextWrapMode::Extend),
                    );
                    ui.spacing_mut().slider_width = if compact { 74.0 } else { 110.0 };
                    ui.add(
                        egui::Slider::new(&mut gains[i], -12.0..=12.0)
                            .vertical()
                            .show_value(false)
                            .step_by(0.5),
                    )
                    .on_hover_text(format!("{} Гц · ±12 дБ", labels[i]));
                    ui.label(RichText::new(labels[i]).size(if compact { 11.0 } else { 13.0 }));
                });
            }
        });
    });
}

#[cfg(test)]
mod band_layout_regressions {
    use super::*;
    #[test]
    fn all_ten_bands_stay_on_one_line_in_main_and_overlay() {
        for (width, compact) in [
            (340.0, true),
            (460.0, true),
            (560.0, false),
            (1000.0, false),
        ] {
            let ctx = egui::Context::default();
            theme::install(&ctx);
            let mut gains = [-12.0, -11.5, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0, 11.5, 12.0];
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 500.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| equalizer_bands(ui, &mut gains, compact));
                },
            );
            let labels = [
                "31", "63", "125", "250", "500", "1k", "2k", "4k", "8k", "16k",
            ];
            let positions = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if labels.contains(&text.galley.job.text.as_str()) => {
                        Some(text.pos)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(positions.len(), 10);
            assert!(
                positions.iter().all(|p| (p.y - positions[0].y).abs() < 1.0),
                "width={width}, {positions:?}"
            );
            assert!(positions.windows(2).all(|p| p[0].x < p[1].x));
        }
    }
}
