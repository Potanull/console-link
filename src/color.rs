//! SDR sRGB grading. The HDR control is an artistic shoulder, not HDR output.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorSettings {
    pub enabled: bool,
    pub exposure: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub temperature: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub hdr: f32,
}

const NEUTRAL: ColorSettings = ColorSettings {
    enabled: false,
    exposure: 0.0,
    contrast: 1.0,
    saturation: 1.0,
    temperature: 0.0,
    shadows: 0.0,
    highlights: 0.0,
    hdr: 0.0,
};
impl Default for ColorSettings {
    fn default() -> Self {
        NEUTRAL
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColorPreset {
    pub name: String,
    pub settings: ColorSettings,
}

pub const FACTORY_PRESETS: [(&str, ColorSettings); 6] = [
    ("Нейтральный", NEUTRAL),
    (
        "Естественный",
        ColorSettings {
            enabled: true,
            contrast: 1.03,
            saturation: 1.04,
            temperature: 0.02,
            shadows: 0.08,
            highlights: -0.06,
            hdr: 0.03,
            ..NEUTRAL
        },
    ),
    (
        "Яркий",
        ColorSettings {
            enabled: true,
            exposure: 0.05,
            contrast: 1.10,
            saturation: 1.18,
            shadows: 0.05,
            highlights: -0.08,
            hdr: 0.10,
            ..NEUTRAL
        },
    ),
    (
        "Кино",
        ColorSettings {
            enabled: true,
            exposure: -0.05,
            contrast: 1.08,
            saturation: 0.90,
            temperature: 0.10,
            shadows: 0.06,
            highlights: -0.16,
            hdr: 0.06,
        },
    ),
    (
        "HDR-эффект",
        ColorSettings {
            enabled: true,
            contrast: 1.07,
            saturation: 1.10,
            shadows: 0.20,
            highlights: -0.20,
            hdr: 0.65,
            ..NEUTRAL
        },
    ),
    (
        "Детали в тенях",
        ColorSettings {
            enabled: true,
            shadows: 0.45,
            highlights: -0.08,
            hdr: 0.15,
            ..NEUTRAL
        },
    ),
];

pub fn preset_name(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(48)
        .collect()
}

fn bounded(value: f32, default: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        default
    }
}

impl ColorSettings {
    pub fn normalize(&mut self) {
        self.exposure = bounded(self.exposure, 0.0, -2.0, 2.0);
        self.contrast = bounded(self.contrast, 1.0, 0.5, 1.5);
        self.saturation = bounded(self.saturation, 1.0, 0.0, 2.0);
        self.temperature = bounded(self.temperature, 0.0, -1.0, 1.0);
        self.shadows = bounded(self.shadows, 0.0, -1.0, 1.0);
        self.highlights = bounded(self.highlights, 0.0, -1.0, 1.0);
        self.hdr = bounded(self.hdr, 0.0, 0.0, 1.0);
    }
    pub fn is_identity(&self) -> bool {
        let mut p = *self;
        p.normalize();
        !p.enabled
            || Self {
                enabled: false,
                ..p
            } == NEUTRAL
    }
    pub fn uniform_values(&self, original: bool, target_is_srgb: bool) -> [f32; 12] {
        let mut p = *self;
        p.normalize();
        [
            p.exposure,
            p.contrast,
            p.saturation,
            p.temperature,
            p.shadows,
            p.highlights,
            p.hdr,
            if !original && !p.is_identity() {
                1.0
            } else {
                0.0
            },
            if target_is_srgb { 1.0 } else { 0.0 },
            0.0,
            0.0,
            0.0,
        ]
    }
}

fn decode(value: f32) -> f32 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}
fn encode(value: f32) -> f32 {
    if value <= 0.0031308 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    }
}
fn luminance(rgb: [f32; 3]) -> f32 {
    rgb[0] * 0.2126 + rgb[1] * 0.7152 + rgb[2] * 0.0722
}

struct Prepared {
    params: ColorSettings,
    balance: [f32; 3],
    bypass: bool,
}
impl Prepared {
    fn new(mut params: ColorSettings) -> Self {
        params.normalize();
        let exposure = 2.0_f32.powf(params.exposure);
        Self {
            params,
            balance: [
                exposure * 2.0_f32.powf(params.temperature * 0.12),
                exposure,
                exposure * 2.0_f32.powf(-params.temperature * 0.12),
            ],
            bypass: params.is_identity(),
        }
    }
    fn transform(&self, input: [f32; 3]) -> [f32; 3] {
        let input = input.map(|x| bounded(x, 0.0, 0.0, 1.0));
        if self.bypass {
            return input;
        }
        let p = self.params;
        let mut rgb = std::array::from_fn(|i| decode(input[i]) * self.balance[i]);
        let y = luminance(rgb);
        let shadows = (1.0 - (y / 0.5).clamp(0.0, 1.0)).powi(2);
        let highlights = ((y - 0.25) / 0.75).clamp(0.0, 1.0).powi(2);
        let tonal_gain = 2.0_f32.powf(p.shadows * 2.0 * shadows + p.highlights * 1.5 * highlights);
        let shoulder = 0.6 * p.hdr;
        rgb = rgb.map(|x| {
            let x = x * tonal_gain;
            x * (1.0 + shoulder) / (1.0 + shoulder * x)
        });
        let y = luminance(rgb);
        rgb.map(|x| {
            let saturated = (y + (x - y) * p.saturation).max(0.0);
            ((encode(saturated) - 0.5) * p.contrast + 0.5).clamp(0.0, 1.0)
        })
    }
}

pub fn transform_rgb(rgb: [f32; 3], settings: &ColorSettings) -> [f32; 3] {
    Prepared::new(*settings).transform(rgb)
}

/// Screenshot-only transform. Playback uses the equivalent fragment shader.
pub fn apply_rgba(bytes: &mut [u8], settings: &ColorSettings) -> Result<(), String> {
    if !bytes.len().is_multiple_of(4) {
        return Err("Incomplete RGBA pixel".into());
    }
    let prepared = Prepared::new(*settings);
    if prepared.bypass {
        return Ok(());
    }
    for pixel in bytes.as_chunks_mut::<4>().0 {
        let rgb = prepared.transform([
            f32::from(pixel[0]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[2]) / 255.0,
        ]);
        for i in 0..3 {
            pixel[i] = (rgb[i] * 255.0).round() as u8;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn enabled() -> ColorSettings {
        ColorSettings {
            enabled: true,
            ..Default::default()
        }
    }
    #[test]
    fn neutral_and_disabled_preserve_bytes_and_alpha_exactly() {
        for mut settings in [ColorSettings::default(), enabled()] {
            let mut bytes = vec![0, 1, 2, 0, 10, 40, 90, 127, 255, 254, 253, 255];
            let original = bytes.clone();
            apply_rgba(&mut bytes, &settings).unwrap();
            assert_eq!(bytes, original);
            settings.enabled = false;
            settings.exposure = 2.0;
            apply_rgba(&mut bytes, &settings).unwrap();
            assert_eq!(bytes, original);
        }
    }
    #[test]
    fn exposure_is_linear_light_not_gamma_space_brightness() {
        let settings = ColorSettings {
            exposure: 1.0,
            ..enabled()
        };
        let result = transform_rgb([0.5; 3], &settings);
        assert!((result[0] - 0.6858361).abs() < 0.00002, "{result:?}");
    }
    #[test]
    fn saturation_temperature_shadows_and_highlights_have_expected_direction() {
        let gray = transform_rgb(
            [0.8, 0.3, 0.1],
            &ColorSettings {
                saturation: 0.0,
                ..enabled()
            },
        );
        assert!((gray[0] - gray[1]).abs() < 0.000001 && (gray[1] - gray[2]).abs() < 0.000001);
        let warm = transform_rgb(
            [0.5; 3],
            &ColorSettings {
                temperature: 1.0,
                ..enabled()
            },
        );
        assert!(warm[0] > 0.5 && warm[2] < 0.5);
        let shadows = transform_rgb(
            [0.1; 3],
            &ColorSettings {
                shadows: 1.0,
                ..enabled()
            },
        );
        assert!(shadows[0] > 0.1);
        let highlights = transform_rgb(
            [0.9; 3],
            &ColorSettings {
                highlights: -1.0,
                ..enabled()
            },
        );
        assert!(highlights[0] < 0.9);
    }
    #[test]
    fn parameters_normalize_and_export_rejects_partial_pixels() {
        let mut settings = ColorSettings {
            enabled: true,
            exposure: f32::NAN,
            contrast: 99.0,
            saturation: -99.0,
            temperature: f32::INFINITY,
            shadows: 9.0,
            highlights: -9.0,
            hdr: 9.0,
        };
        settings.normalize();
        assert_eq!(settings.exposure, 0.0);
        assert_eq!(settings.contrast, 1.5);
        assert_eq!(settings.saturation, 0.0);
        assert_eq!(settings.temperature, 0.0);
        assert_eq!(settings.shadows, 1.0);
        assert_eq!(settings.highlights, -1.0);
        assert_eq!(settings.hdr, 1.0);
        assert!(apply_rgba(&mut [0; 3], &settings).is_err());
        let mut bytes = [20, 40, 60, 137];
        apply_rgba(&mut bytes, &settings).unwrap();
        assert_eq!(bytes[3], 137);
    }
    #[test]
    fn all_presets_are_bounded_and_neutral_is_unmodified() {
        assert_eq!(FACTORY_PRESETS.len(), 6);
        assert!(FACTORY_PRESETS[0].1.is_identity());
        assert!(FACTORY_PRESETS[1..].iter().all(|(_, s)| !s.is_identity()));
        for (_, settings) in FACTORY_PRESETS {
            for n in 0..1000 {
                let rgb = [
                    (n % 10) as f32 / 9.0,
                    (n / 10 % 10) as f32 / 9.0,
                    (n / 100) as f32 / 9.0,
                ];
                assert!(
                    transform_rgb(rgb, &settings)
                        .iter()
                        .all(|c| c.is_finite() && *c >= 0.0 && *c <= 1.0)
                );
            }
        }
    }
    #[test]
    fn before_toggle_bypasses_shader_without_mutating_settings() {
        let settings = ColorSettings {
            exposure: 1.0,
            ..enabled()
        };
        let before = settings;
        assert_eq!(settings.uniform_values(true, false)[7], 0.0);
        assert_eq!(settings.uniform_values(false, false)[7], 1.0);
        assert_eq!(settings, before);
    }
}
