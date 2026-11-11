use super::*;
use console_link::color::{ColorPreset, ColorSettings, FACTORY_PRESETS};

impl ConsoleApp {
    pub(super) fn color_page(&mut self, ui: &mut egui::Ui) {
        if self.playback.playing {
            self.color_controls(ui, true);
            return;
        }
        theme::section_heading(
            ui,
            "Изображение",
            "Цвет и настроение картинки — без перезапуска захвата.",
        );
        if ui.available_width() >= 800.0 {
            let width = ui.available_width();
            let preview_width = (width * 0.38).clamp(260.0, 350.0);
            let controls_width = width - preview_width - 18.0;
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(preview_width);
                    self.color_preview(ui);
                    theme::subtitle(ui,"Сгенерированная шкала SDR. Подключи консоль, чтобы настроить изображение во время игры через F1.");
                });
                ui.add_space(8.0);

                ui.vertical(|ui| {ui.set_width(controls_width);self.color_controls(ui,false);});
            });
        } else {
            self.color_controls(ui, false);
            ui.add_space(12.0);
            self.color_preview(ui);
            theme::subtitle(
                ui,
                "Сгенерированная шкала SDR · это не изображение с консоли",
            );
        }
    }

    fn color_controls(&mut self, ui: &mut egui::Ui, compact: bool) {
        self.upscale_controls(ui, compact);
        if !self.color_gpu_available {
            theme::subtitle(
                ui,
                "Обработка цвета недоступна: GPU-рендерер не активен. Показывается оригинал.",
            );
        }
        let before = self.settings.color;
        let presets_before = self.settings.color_presets.clone();
        if self
            .color_selected
            .is_some_and(|i| i >= self.settings.color_presets.len())
        {
            self.color_selected = None;
        }
        ui.add_enabled_ui(self.color_gpu_available, |ui| {
            theme::card().inner_margin(if compact { 12 } else { 18 }).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing.y=if compact {4.0} else {6.0};
                ui.spacing_mut().interact_size.y=if compact {24.0} else {28.0};
                ui.spacing_mut().button_padding=if compact {egui::vec2(8.0,4.0)} else {egui::vec2(10.0,5.0)};
                ui.label(RichText::new("Цветокоррекция").size(17.0).strong());
                theme::toggle(ui, &mut self.settings.color.enabled, "Обрабатывать изображение");
                theme::toggle(ui, &mut self.color_original, "Сравнить: показывать оригинал");
                if self.color_original {
                    theme::subtitle(ui, "Сейчас виден оригинал. Настройки обработки сохранены.");
                }
                self.color_preset_picker(ui);
                ui.add_space(6.0);
                color_sliders(ui, &mut self.settings.color);
                self.settings.color.normalize();
                theme::subtitle(ui, "HDR-эффект — художественная обработка SDR, не настоящий HDR и не восстановление деталей.");
                if ui.button("Сбросить цвет").clicked() {
                    self.settings.color = ColorSettings::default();
                    self.color_original = false;
                    self.color_selected = None;
                    self.color_name.clear();
                }
                if compact {
                    ui.collapsing("Мои пресеты", |ui| self.color_presets_editor(ui));
                } else {
                    ui.add_space(5.0);
                    self.color_presets_editor(ui);
                }
            });
        });
        if before != self.settings.color || presets_before != self.settings.color_presets {
            self.dirty = Some(Instant::now());
        }
        ui.add_space(8.0);
        if self.screenshot_pending {
            theme::subtitle(ui, "Скриншот сохраняется…");
        }
        ui.horizontal_wrapped(|ui| {
            let source = self.displayed.is_some() && !self.screenshot_pending;
            if ui
                .add_enabled(source, egui::Button::new("Скриншот текущего вида"))
                .clicked()
            {
                self.screenshot_with_color(ui.ctx().clone(), false);
            }
            if ui
                .add_enabled(source, egui::Button::new("Сохранить оригинал"))
                .clicked()
            {
                self.screenshot_with_color(ui.ctx().clone(), true);
            }
        });
        theme::subtitle(
            ui,
            if self.displayed.is_some() {
                "F8 сохраняет текущий вид: с обработкой или оригинал при сравнении."
            } else {
                "Скриншоты доступны после получения кадра с карты захвата."
            },
        );
    }

    fn color_preset_picker(&mut self, ui: &mut egui::Ui) {
        let label = self
            .color_selected
            .and_then(|i| self.settings.color_presets.get(i))
            .map(|p| p.name.as_str())
            .or_else(|| {
                FACTORY_PRESETS
                    .iter()
                    .find(|(_, settings)| *settings == self.settings.color)
                    .map(|(name, _)| *name)
            })
            .unwrap_or("Свои настройки")
            .to_owned();
        let mut selected = None;
        let mut factory = None;
        egui::ComboBox::from_id_salt("color-presets")
            .truncate()
            .selected_text(label)
            .width(ui.available_width())
            .show_ui(ui, |ui| {
                for (name, settings) in FACTORY_PRESETS {
                    if ui
                        .selectable_label(
                            self.color_selected.is_none() && self.settings.color == settings,
                            name,
                        )
                        .clicked()
                    {
                        factory = Some(settings);
                    }
                }
                if !self.settings.color_presets.is_empty() {
                    ui.separator();
                }
                for (i, preset) in self.settings.color_presets.iter().enumerate() {
                    if ui
                        .selectable_label(self.color_selected == Some(i), &preset.name)
                        .clicked()
                    {
                        selected = Some(i);
                    }
                }
            });
        if let Some(settings) = factory {
            self.settings.color = settings;
            self.color_selected = None;
            self.color_name.clear();
            self.color_original = false;
        }
        if let Some(i) = selected {
            let preset = &self.settings.color_presets[i];
            self.settings.color = preset.settings;
            self.color_name = preset.name.clone();
            self.color_selected = Some(i);
            self.color_original = false;
        }
    }

    fn color_presets_editor(&mut self, ui: &mut egui::Ui) {
        ui.add(
            egui::TextEdit::singleline(&mut self.color_name)
                .hint_text("Название своего пресета")
                .char_limit(48)
                .desired_width(ui.available_width()),
        );
        let name = color_preset_name(&self.color_name);
        let duplicate = self
            .settings
            .color_presets
            .iter()
            .enumerate()
            .any(|(i, p)| {
                Some(i) != self.color_selected && p.name.to_lowercase() == name.to_lowercase()
            });
        let new_duplicate = self
            .settings
            .color_presets
            .iter()
            .any(|p| p.name.to_lowercase() == name.to_lowercase());
        let valid = !name.is_empty() && !duplicate;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    valid && !new_duplicate && self.settings.color_presets.len() < 64,
                    egui::Button::new("Сохранить копию"),
                )
                .clicked()
            {
                self.settings.color_presets.push(ColorPreset {
                    name: name.clone(),
                    settings: self.settings.color,
                });
                self.color_selected = Some(self.settings.color_presets.len() - 1);
                self.color_name = name.clone();
            }
            if ui
                .add_enabled(
                    valid && self.color_selected.is_some(),
                    egui::Button::new("Обновить / переименовать"),
                )
                .clicked()
                && let Some(i) = self.color_selected
            {
                self.settings.color_presets[i] = ColorPreset {
                    name: name.clone(),
                    settings: self.settings.color,
                };
                self.color_name = name.clone();
            }
            if ui
                .add_enabled(self.color_selected.is_some(), egui::Button::new("Удалить"))
                .clicked()
            {
                if let Some(i) = self.color_selected.take() {
                    self.settings.color_presets.remove(i);
                }
                self.color_name.clear();
            }
        });
        if duplicate || (new_duplicate && self.color_selected.is_none()) {
            theme::subtitle(ui, "Пресет с таким названием уже существует.");
        }
        if self.settings.color_presets.len() >= 64 {
            theme::subtitle(
                ui,
                "Сохранено 64 пресета. Удали один, чтобы сохранить новый.",
            );
        }
    }
}

fn color_preset_name(raw: &str) -> String {
    console_link::color::preset_name(raw)
}

fn color_sliders(ui: &mut egui::Ui, color: &mut ColorSettings) {
    // egui's slider_width excludes its padded numeric editor.
    let label_font = egui::TextStyle::Body.resolve(ui.style());
    let label_width = [
        "Экспозиция",
        "Контраст",
        "Насыщенность",
        "Температура",
        "Тени",
        "Света",
        "HDR-эффект",
    ]
    .iter()
    .map(|label| {
        ui.painter()
            .layout_no_wrap((*label).to_owned(), label_font.clone(), theme::TEXT)
            .size()
            .x
    })
    .fold(0.0_f32, f32::max);
    let value_font = ui.style().drag_value_text_style.resolve(ui.style());
    let value_width = (ui
        .painter()
        .layout_no_wrap("-2.00 EV".to_owned(), value_font, theme::TEXT)
        .size()
        .x
        + 2.0 * ui.spacing().button_padding.x)
        .max(ui.spacing().interact_size.x);
    let slider_width = (ui.available_width() - label_width - value_width - 18.0).max(24.0);
    egui::Grid::new("color-sliders")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 5.0))
        .show(ui, |ui| {
            ui.spacing_mut().slider_width = slider_width;
            for (label, value, range, suffix) in [
                ("Экспозиция", &mut color.exposure, -2.0..=2.0, " EV"),
                ("Контраст", &mut color.contrast, 0.5..=1.5, ""),
                ("Насыщенность", &mut color.saturation, 0.0..=2.0, ""),
                ("Температура", &mut color.temperature, -1.0..=1.0, ""),
                ("Тени", &mut color.shadows, -1.0..=1.0, ""),
                ("Света", &mut color.highlights, -1.0..=1.0, ""),
                ("HDR-эффект", &mut color.hdr, 0.0..=1.0, ""),
            ] {
                ui.label(label);
                ui.add(
                    egui::Slider::new(value, range)
                        .step_by(0.01)
                        .fixed_decimals(2)
                        .suffix(suffix),
                );
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_name_strips_controls_trims_and_caps_unicode_characters() {
        assert_eq!(color_preset_name("  Тёплый\n\t вечер  "), "Тёплый вечер");
        assert_eq!(color_preset_name(&"я".repeat(60)).chars().count(), 48);
        assert!(color_preset_name(" \n\r\t ").is_empty());
    }

    #[test]
    fn controls_fit_narrow_drawer_and_main_page() {
        for width in [300.0, 380.0, 560.0] {
            let ctx = egui::Context::default();
            theme::install(&ctx);
            let mut color = ColorSettings {
                exposure: -2.0,
                temperature: -1.0,
                shadows: -1.0,
                highlights: -1.0,
                ..Default::default()
            };
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| color_sliders(ui, &mut color));
                },
            );
            let labels = [
                "Экспозиция",
                "Контраст",
                "Насыщенность",
                "Температура",
                "Тени",
                "Света",
                "HDR-эффект",
            ];
            let mut rows = Vec::new();
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    if labels.contains(&text.galley.job.text.as_str()) {
                        rows.push(text.pos);
                    }
                    assert!(
                        text.pos.x + text.galley.size().x <= width,
                        "control overflows at {width}: {}",
                        text.galley.job.text
                    );
                }
            }
            assert_eq!(rows.len(), 7);
            assert!(rows.windows(2).all(|pair| pair[0].y < pair[1].y));
        }
    }
}
