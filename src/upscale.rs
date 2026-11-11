use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UpscaleMode {
    #[default]
    Standard,
    Nis,
    Fsr1,
    RtxVideo,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpscaleSettings {
    pub mode: UpscaleMode,
    pub nis_sharpness: f32,
    pub fsr_sharpness: f32,
}
impl Default for UpscaleSettings {
    fn default() -> Self {
        Self {
            mode: UpscaleMode::Standard,
            nis_sharpness: 0.3,
            fsr_sharpness: 0.3,
        }
    }
}
impl UpscaleSettings {
    pub fn normalize(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_defaults_normalization_and_geometry() {
        let mut value: UpscaleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(value, UpscaleSettings::default());
        value.mode = UpscaleMode::Nis;
        value.nis_sharpness = f32::NAN;
        value.fsr_sharpness = 9.0;
        value.normalize();
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
