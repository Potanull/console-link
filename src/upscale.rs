use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpscaleMode {
    #[default]
    Standard,
    Nis,
    Fsr1,
    RtxVideo,
}

/// Processing resolution, independent of the capture card's advertised modes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OutputResolution {
    #[default]
    Auto,
    Qhd,
    Uhd,
}
impl OutputResolution {
    pub fn target(self, viewport: [u32; 2], source: [u32; 2]) -> [u32; 2] {
        let bounds = match self {
            Self::Auto => return viewport,
            Self::Qhd => [2560, 1440],
            Self::Uhd => [3840, 2160],
        };
        if source.contains(&0) {
            return viewport;
        }
        let scale = (bounds[0] as f64 / source[0] as f64).min(bounds[1] as f64 / source[1] as f64);
        [
            (source[0] as f64 * scale).round().max(1.) as u32,
            (source[1] as f64 * scale).round().max(1.) as u32,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpscaleSettings {
    pub mode: UpscaleMode,
    pub output_resolution: OutputResolution,
    pub nis_sharpness: f32,
    pub fsr_sharpness: f32,
    pub rtx_quality: u32,
}
impl Default for UpscaleSettings {
    fn default() -> Self {
        Self {
            mode: UpscaleMode::Standard,
            output_resolution: OutputResolution::Auto,
            nis_sharpness: 0.3,
            fsr_sharpness: 0.3,
            rtx_quality: 2,
        }
    }
}
impl UpscaleSettings {
    pub fn normalize(&mut self) {
        self.rtx_quality = self.rtx_quality.clamp(1, 4);
        for value in [&mut self.nis_sharpness, &mut self.fsr_sharpness] {
            *value = if value.is_finite() {
                value.clamp(0.0, 1.0)
            } else {
                0.3
            };
        }
    }
    /// Native size, mixed-axis resizing and downscaling use ordinary sampling.
    pub fn effective_mode(self, source: [u32; 2], target: [u32; 2]) -> UpscaleMode {
        if source.contains(&0) || target[0] <= source[0] || target[1] <= source[1] {
            UpscaleMode::Standard
        } else {
            self.mode
        }
    }
}

/// NVIDIA SDK quality levels, not a subjective image-quality percentage.
pub fn rtx_quality_label(level: u32) -> &'static str {
    match level {
        1 => "1 · Производительность",
        3 => "3 · Высокое",
        4 => "4 · Максимальное",
        _ => "2 · Сбалансированное",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_resolution_preserves_aspect_ratio_and_legacy_default() {
        assert_eq!(
            OutputResolution::Auto.target([1111, 625], [1920, 1080]),
            [1111, 625]
        );
        assert_eq!(
            OutputResolution::Qhd.target([1, 1], [1920, 1080]),
            [2560, 1440]
        );
        assert_eq!(
            OutputResolution::Uhd.target([1, 1], [1920, 1080]),
            [3840, 2160]
        );
        assert_eq!(
            OutputResolution::Uhd.target([1, 1], [1440, 1080]),
            [2880, 2160]
        );
        assert_eq!(OutputResolution::Qhd.target([800, 600], [0, 0]), [800, 600]);
    }
    #[test]
    fn legacy_defaults_normalization_and_geometry() {
        let mut value: UpscaleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(value, UpscaleSettings::default());
        value.mode = UpscaleMode::Nis;
        value.nis_sharpness = f32::NAN;
        value.fsr_sharpness = 9.0;
        value.rtx_quality = 99;
        value.normalize();
        assert_eq!(value.rtx_quality, 4);
        assert_eq!(value.nis_sharpness, 0.3);
        assert_eq!(value.fsr_sharpness, 1.0);
        for target in [[1920, 1080], [1280, 720], [2560, 1080], [1920, 1440]] {
            assert_eq!(
                value.effective_mode([1920, 1080], target),
                UpscaleMode::Standard
            );
        }
        assert_eq!(
            value.effective_mode([1920, 1080], [2560, 1440]),
            UpscaleMode::Nis
        );
        assert_eq!(
            value.effective_mode([0, 0], [2560, 1440]),
            UpscaleMode::Standard
        );
        assert!(serde_json::from_str::<UpscaleSettings>("{\"mode\":\"Unknown\"}").is_err());
    }
}
