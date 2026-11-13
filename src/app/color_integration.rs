use super::*;
use console_link::color::ColorSettings;

impl ConsoleApp {
    pub(super) fn color_preview(&mut self, ui: &mut egui::Ui) {
        theme::subtitle(
            ui,
            if self.color_original {
                "Исходное изображение · тестовая шкала SDR"
            } else {
                "Предпросмотр · тестовая шкала SDR"
            },
        );
        let width = ui.available_width().min(800.0);
        let size = egui::vec2(width, width * 9.0 / 16.0);
        let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        if self.color_gpu_available {
            let target = upscale_ui::viewport_size(rect, ui.ctx()).unwrap_or([512, 288]);
            let upscale = self.available_upscale_settings();
            let paint_error = upscale_gpu::paint_enhanced(
                ui,
                rect,
                self.color_test_pattern.clone(),
                self.settings.color,
                self.color_original,
                upscale,
                target,
            )
            .err();
            if paint_error.is_some() {
                self.upscale_error = paint_error;
                color_gpu::paint(
                    ui,
                    rect,
                    self.color_test_pattern.clone(),
                    self.settings.color,
                    self.color_original,
                );
            }
            let source = [
                self.color_test_pattern.width,
                self.color_test_pattern.height,
            ];
            let mode = if self.upscale_error.is_some() {
                console_link::upscale::UpscaleMode::Standard
            } else {
                upscale.effective_mode(source, target)
            };
            self.upscale_preview_status = Some(upscale_ui::DisplayStatus {
                source,
                target,
                mode,
            });
        } else {
            let texture = ui.ctx().load_texture(
                "color-test-pattern",
                egui::ColorImage::from_rgba_unmultiplied(
                    [
                        self.color_test_pattern.width as usize,
                        self.color_test_pattern.height as usize,
                    ],
                    &self.color_test_pattern.rgba,
                ),
                egui::TextureOptions::LINEAR,
            );
            ui.put(rect, egui::Image::new(&texture).fit_to_exact_size(size));
        }
    }
}

pub(super) fn test_pattern() -> Arc<VideoFrame> {
    let (width, height) = (512_u32, 288_u32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    let colors = [
        [255, 80, 70],
        [80, 230, 100],
        [70, 130, 255],
        [240, 220, 70],
        [220, 90, 230],
        [70, 220, 230],
        [190, 140, 105],
        [130, 130, 130],
    ];
    for y in 0..height {
        for x in 0..width {
            let gradient = (x * 255 / (width - 1)) as u8;
            let rgb = if y < height / 3 {
                [gradient; 3]
            } else if y < height * 2 / 3 {
                let color = colors[(x / (width / 8)) as usize];
                let brightness = 0.15 + 0.85 * (y - height / 3) as f32 / (height / 3) as f32;
                color.map(|c| (c as f32 * brightness) as u8)
            } else {
                let n = (x / (width / 16)) as u8;
                if x < width / 2 {
                    [n * 6; 3]
                } else {
                    [213 + (n - 8) * 6; 3]
                }
            };
            rgba.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    Arc::new(VideoFrame {
        width,
        height,
        rgba,
        timestamp_ns: 0,
    })
}

/// A screenshot uses the exact grading paired with the last submitted frame.
pub(super) fn effective_color(
    settings: ColorSettings,
    original: bool,
    gpu_available: bool,
) -> ColorSettings {
    if original || !gpu_available {
        ColorSettings::default()
    } else {
        let mut settings = settings;
        settings.normalize();
        settings
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_gpu_and_original_comparison_export_the_visible_original() {
        let settings = console_link::color::FACTORY_PRESETS[4].1;
        assert!(effective_color(settings, true, true).is_identity());
        assert!(effective_color(settings, false, false).is_identity());
        assert_eq!(effective_color(settings, false, true), settings);
        let frame = test_pattern();
        assert_eq!(frame.rgba.len(), (frame.width * frame.height * 4) as usize);
        assert!(
            frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .all(|pixel| pixel[3] == 255)
        );
    }
}

pub(super) fn save_screenshot(
    frame: &VideoFrame,
    settings: &ColorSettings,
    original: bool,
    directory: &std::path::Path,
) -> Result<String, String> {
    console_link::gpu::save_screenshot(frame, settings, original, directory)
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn original_and_processed_pngs_keep_dimensions_alpha_and_srgb_metadata() {
        let directory =
            std::env::temp_dir().join(format!("console-link-color-png-{}", std::process::id()));
        let frame = VideoFrame {
            width: 3,
            height: 1,
            rgba: vec![64, 100, 128, 0, 128, 90, 45, 137, 255, 0, 0, 255],
            timestamp_ns: 1,
        };
        let settings = console_link::color::FACTORY_PRESETS[4].1;
        let processed = save_screenshot(&frame, &settings, false, &directory).unwrap();
        let original = save_screenshot(&frame, &settings, true, &directory).unwrap();
        let mut expected = frame.rgba.clone();
        console_link::color::apply_rgba(&mut expected, &settings).unwrap();
        assert_ne!(expected, frame.rgba);
        for (path, expected) in [(processed, expected), (original, frame.rgba.clone())] {
            let decoder =
                png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
            let mut reader = decoder.read_info().unwrap();
            assert_eq!(
                reader.info().srgb,
                Some(png::SrgbRenderingIntent::Perceptual)
            );
            let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut bytes).unwrap();
            assert_eq!((info.width, info.height), (3, 1));
            assert_eq!(&bytes[..info.buffer_size()], expected);
            std::fs::remove_file(path).unwrap();
        }
        std::fs::remove_dir(directory).unwrap();
    }
    #[test]
    fn invalid_frame_is_rejected_before_creating_export_directory() {
        let directory = std::env::temp_dir().join(format!(
            "console-link-invalid-color-png-{}",
            std::process::id()
        ));
        let frame = VideoFrame {
            width: 3,
            height: 1,
            rgba: vec![0; 4],
            timestamp_ns: 1,
        };
        assert!(save_screenshot(&frame, &ColorSettings::default(), true, &directory).is_err());
        assert!(!directory.exists());
    }
}
