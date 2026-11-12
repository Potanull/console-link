use super::*;
use console_link::upscale::{UpscaleMode, UpscaleSettings};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_viewport_respects_dpi_and_rejects_invalid_size() {
        assert_eq!(
            physical_size(egui::vec2(1280.0, 720.0), 2.0),
            Some([2560, 1440])
        );
        assert_eq!(
            physical_size(egui::vec2(853.3, 480.0), 1.5),
            Some([1280, 720])
        );
        assert_eq!(physical_size(egui::vec2(0.0, 720.0), 1.0), None);
        assert_eq!(physical_size(egui::vec2(f32::NAN, 720.0), 1.0), None);
    }
    #[test]
    fn comparison_keeps_requested_settings() {
        let settings = UpscaleSettings {
            mode: UpscaleMode::Nis,
            ..Default::default()
        };
        assert_eq!(
            effective_settings(settings, true).mode,
            UpscaleMode::Standard
        );
        assert_eq!(settings.mode, UpscaleMode::Nis);
        assert_eq!(effective_settings(settings, false).mode, UpscaleMode::Nis);
        let rtx = UpscaleSettings {
            mode: UpscaleMode::RtxVideo,
            ..settings
        };
        assert_eq!(effective_settings(rtx, false).mode, UpscaleMode::RtxVideo);
    }
}

pub(super) fn physical_size(size: egui::Vec2, pixels_per_point: f32) -> Option<[u32; 2]> {
    let size = size * pixels_per_point;
    if !size.x.is_finite()
        || !size.y.is_finite()
        || size.x < 1.0
        || size.y < 1.0
        || size.x > 16384.0
        || size.y > 16384.0
    {
        return None;
    }
    Some([size.x.round() as u32, size.y.round() as u32])
}

pub(super) fn effective_settings(
    mut settings: UpscaleSettings,
    comparison: bool,
) -> UpscaleSettings {
    settings.normalize();
    if comparison {
        settings.mode = UpscaleMode::Standard;
    }
    settings
}

fn mode_name(mode: UpscaleMode) -> &'static str {
    match mode {
        UpscaleMode::Standard => "Обычный",
        UpscaleMode::Nis => "NVIDIA Image Scaling",
        UpscaleMode::Fsr1 => "AMD FSR 1",
        UpscaleMode::RtxVideo => "RTX Video Super Resolution",
    }
}

#[derive(Clone)]
pub(super) struct PresentedImage {
    pub frame: Arc<VideoFrame>,
    pub color: console_link::color::ColorSettings,
    pub upscale: UpscaleSettings,
    pub target: [u32; 2],
}

impl ConsoleApp {
    pub(super) fn available_upscale_settings(&mut self) -> UpscaleSettings {
        let mut settings = effective_settings(self.settings.upscale, self.upscale_comparison);
        self.upscale_error = None;
        if settings.mode != UpscaleMode::Standard {
            let result = self
                .export_gpu
                .as_ref()
                .ok_or_else(|| "GPU недоступен".to_owned())
                .and_then(|state| {
                    upscale_gpu::ensure(state, settings.mode)?;
                    if settings.mode == UpscaleMode::RtxVideo {
                        upscale_gpu::rtx_status(state)?;
                    }
                    Ok(())
                });
            if let Err(error) = result {
                self.upscale_error = Some(format!(
                    "{} недоступен: {error}. Используется обычное масштабирование.",
                    mode_name(settings.mode)
                ));
                settings.mode = UpscaleMode::Standard;
            }
        }
        settings
    }

    pub(super) fn upscale_controls(&mut self, ui: &mut egui::Ui, compact: bool) {
        let before = self.settings.upscale;
        theme::card().inner_margin(if compact {12} else {18}).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Апскейл изображения").size(17.0).strong());
            ui.add_enabled_ui(self.color_gpu_available, |ui| {
                egui::ComboBox::from_id_salt("upscale-mode").width(ui.available_width().min(300.0))
                    .selected_text(mode_name(self.settings.upscale.mode)).show_ui(ui, |ui| {
                        for mode in [UpscaleMode::Standard, UpscaleMode::Nis, UpscaleMode::Fsr1, UpscaleMode::RtxVideo] {
                            ui.add_enabled_ui(mode != UpscaleMode::RtxVideo || cfg!(target_os = "windows"), |ui| {
                                ui.selectable_value(&mut self.settings.upscale.mode, mode, mode_name(mode))
                                    .on_disabled_hover_text("RTX Video требует Windows и видеокарту NVIDIA RTX.");
                            });
                        }
                    });
                let sharpness = match self.settings.upscale.mode {
                    UpscaleMode::Nis => Some(&mut self.settings.upscale.nis_sharpness),
                    UpscaleMode::Fsr1 => Some(&mut self.settings.upscale.fsr_sharpness),
                    _ => None,
                };
                if let Some(sharpness) = sharpness {
                    ui.label("Резкость");
                    ui.spacing_mut().slider_width = (ui.available_width() - 65.0).max(40.0);
                    ui.add(egui::Slider::new(sharpness, 0.0..=1.0).fixed_decimals(2));

                }
                if self.settings.upscale.mode == UpscaleMode::RtxVideo {
                    egui::ComboBox::from_id_salt("rtx-quality")
                        .selected_text(match self.settings.upscale.rtx_quality { 1 => "Быстрое", 3 => "Высокое", 4 => "Максимальное", _ => "Сбалансированное" })
                        .show_ui(ui, |ui| {
                            for (value, label) in [(1, "Быстрое"), (2, "Сбалансированное"), (3, "Высокое"), (4, "Максимальное")] {
                                ui.selectable_value(&mut self.settings.upscale.rtx_quality, value, label);
                            }
                        });
                    theme::subtitle(ui, "Более высокое качество увеличивает нагрузку на GPU. Для плавной игры начни со сбалансированного.");
                }
                if self.settings.upscale.mode != UpscaleMode::Standard {
                    theme::toggle(ui, &mut self.upscale_comparison, "Сравнить с обычным масштабированием");
                }
            });
            if self.upscale_comparison {
                theme::subtitle(ui, "Сравнение: обычный фильтр. Цветокоррекция сохранена.");
            }
            let status = if self.playback.playing || self.snapshot_game {
                self.presented_image.as_ref().map(|image| DisplayStatus {
                    source:[image.frame.width,image.frame.height],target:image.target,
                    mode:image.upscale.effective_mode([image.frame.width,image.frame.height],image.target),
                })
            } else { self.upscale_preview_status };
            if let Some(status) = status {
                theme::subtitle(ui, &format!("{} × {} → {} × {} · {}", status.source[0],status.source[1],status.target[0],status.target[1],mode_name(status.mode)));
                if status.mode == UpscaleMode::Standard && self.settings.upscale.mode != UpscaleMode::Standard && !self.upscale_comparison && (status.target[0] <= status.source[0] || status.target[1] <= status.source[1]) {
                    theme::subtitle(ui,"Улучшение включается при размере больше исходного кадра. Во время игры увеличь окно или включи полный экран.");
                }
            } else {
                theme::subtitle(ui,"Размер результата определяется окном и масштабом экрана. Для Full HD → 1440p включи полный экран на 2K-мониторе.");
            }
            if let Some(error) = &self.upscale_error { theme::subtitle(ui,error); }
            theme::subtitle(ui,"NIS и FSR улучшают контуры. Детали нативного высокого разрешения не гарантируются.");
        });
        self.settings.upscale.normalize();
        if self.settings.upscale != before {
            self.dirty = Some(Instant::now());
            self.upscale_comparison = false;
            ui.ctx().request_repaint();
        }
        ui.add_space(10.0);
    }
}

#[cfg(test)]
mod recipe_tests {
    use super::*;
    #[test]
    fn exported_recipe_retains_frame_mode_color_and_size_after_ui_changes() {
        let frame = Arc::new(VideoFrame {
            width: 2,
            height: 2,
            rgba: vec![128; 16],
            timestamp_ns: 7,
        });
        let mut presented = PresentedImage {
            frame: frame.clone(),
            color: console_link::color::FACTORY_PRESETS[4].1,
            upscale: UpscaleSettings {
                mode: UpscaleMode::Nis,
                nis_sharpness: 0.6,
                ..Default::default()
            },
            target: [2560, 1440],
        };
        let pending = presented.clone();
        presented.upscale.mode = UpscaleMode::Fsr1;
        presented.target = [3840, 2160];
        presented.color = Default::default();
        presented.frame = Arc::new(VideoFrame {
            width: 1,
            height: 1,
            rgba: vec![0; 4],
            timestamp_ns: 8,
        });
        assert_eq!(pending.target, [2560, 1440]);
        assert_eq!(pending.upscale.mode, UpscaleMode::Nis);
        assert_eq!(pending.upscale.nis_sharpness, 0.6);
        assert!(!pending.color.is_identity());
        assert!(Arc::ptr_eq(&pending.frame, &frame));
    }
}

pub(super) fn viewport_size(rect: egui::Rect, ctx: &egui::Context) -> Option<[u32; 2]> {
    let ppp = ctx.pixels_per_point();
    let screen = physical_size(ctx.viewport_rect().size(), ppp)?;
    physical_viewport(rect, ppp, screen)
}
fn physical_viewport(rect: egui::Rect, ppp: f32, screen: [u32; 2]) -> Option<[u32; 2]> {
    physical_size(rect.size(), ppp)?;
    if !rect.is_finite() {
        return None;
    }
    let viewport = egui::epaint::ViewportInPixels::from_points(&rect, ppp, screen);
    (viewport.width_px > 0 && viewport.height_px > 0)
        .then_some([viewport.width_px as u32, viewport.height_px as u32])
}
#[cfg(test)]
mod viewport_tests {
    use super::*;
    #[test]
    fn viewport_uses_egui_endpoint_rounding_and_screen_bounds() {
        let rect = egui::Rect::from_min_max(egui::pos2(0.4, 0.4), egui::pos2(10.6, 6.6));
        assert_eq!(physical_viewport(rect, 1.0, [100, 100]), Some([11, 7]));
        assert_eq!(physical_viewport(rect, 1.0, [8, 5]), Some([8, 5]));
    }
}

#[derive(Clone, Copy)]
pub(super) struct DisplayStatus {
    pub source: [u32; 2],
    pub target: [u32; 2],
    pub mode: UpscaleMode,
}
