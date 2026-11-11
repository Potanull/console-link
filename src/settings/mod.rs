use crate::capture::{CapturePreferences, VideoMode};
use crate::color::{ColorPreset, ColorSettings};
use crate::config::{audio, capture};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SettingsError {
    FutureSchema,
    Invalid(String),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameWindowMode {
    #[default]
    Windowed,
    Borderless,
    Fullscreen,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqualizerSettings {
    pub enabled: bool,
    pub gains: [f32; 10],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EqualizerPreset {
    pub name: String,
    pub gains: [f32; 10],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub video_id: String,
    pub mode: Option<VideoMode>,
    pub capture_preferences: CapturePreferences,
    pub game_window_mode: GameWindowMode,
    pub audio_input_id: String,
    pub audio_output_id: String,
    pub gain: f32,
    pub muted: bool,
    pub sync_ms: i32,
    pub color: ColorSettings,
    pub color_presets: Vec<ColorPreset>,
    pub equalizer: EqualizerSettings,
    pub equalizer_presets: Vec<EqualizerPreset>,
    pub game: String,
    pub discord_enabled: bool,
    pub discord_application_id: String,
    pub screenshot_dir: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        let screenshot_dir = directories::UserDirs::new()
            .and_then(|dirs| dirs.picture_dir().map(|p| p.join("Console Link")))
            .unwrap_or_else(|| PathBuf::from("screenshots"));
        Self {
            schema_version: 1,
            video_id: String::new(),
            mode: None,
            capture_preferences: CapturePreferences::default(),
            game_window_mode: GameWindowMode::default(),
            audio_input_id: String::new(),
            audio_output_id: String::new(),
            gain: audio::DEFAULT_GAIN,
            muted: false,
            sync_ms: 0,
            color: ColorSettings::default(),
            color_presets: Vec::new(),
            equalizer: EqualizerSettings::default(),
            equalizer_presets: Vec::new(),
            game: String::new(),
            discord_enabled: false,
            discord_application_id: String::new(),
            screenshot_dir,
        }
    }
}

impl Settings {
    pub fn parse(json: &str) -> Result<Self, SettingsError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| SettingsError::Invalid(e.to_string()))?;
        let version = value
            .get("schema_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| SettingsError::Invalid("Missing settings schema".into()))?;
        if version > 1 {
            return Err(SettingsError::FutureSchema);
        }
        if version != 1 {
            return Err(SettingsError::Invalid("Unsupported settings schema".into()));
        }
        let mut settings: Self =
            serde_json::from_value(value).map_err(|e| SettingsError::Invalid(e.to_string()))?;
        if !settings.gain.is_finite() {
            return Err(SettingsError::Invalid("Non-finite volume".into()));
        }
        settings.gain = settings.gain.clamp(0.0, 1.0);
        settings.sync_ms = settings
            .sync_ms
            .clamp(-capture::MAX_SYNC_MS, capture::MAX_SYNC_MS);
        settings.capture_preferences.preferred_fps = settings
            .capture_preferences
            .preferred_fps
            .clamp(1, capture::MAX_FPS);
        if settings.mode.as_ref().is_some_and(|m| {
            m.width == 0
                || m.height == 0
                || m.width > capture::MAX_DEVICE_DIMENSION
                || m.height > capture::MAX_DEVICE_DIMENSION
                || m.fps_num == 0
                || m.fps_den == 0
        }) {
            return Err(SettingsError::Invalid("Invalid video mode".into()));
        }
        settings.equalizer.gains = crate::audio::equalizer::clean_gains(settings.equalizer.gains);
        settings.equalizer_presets.truncate(64);
        let mut names = std::collections::HashSet::new();
        settings.equalizer_presets.retain_mut(|preset| {
            preset.name = preset
                .name
                .trim()
                .chars()
                .filter(|c| !c.is_control())
                .take(48)
                .collect();
            preset.gains = crate::audio::equalizer::clean_gains(preset.gains);
            !preset.name.is_empty() && names.insert(preset.name.to_lowercase())
        });
        settings.color.normalize();
        settings.color_presets.truncate(64);
        let mut names = std::collections::HashSet::new();
        settings.color_presets.retain_mut(|preset| {
            preset.name = crate::color::preset_name(&preset.name);
            preset.settings.normalize();
            !preset.name.is_empty() && names.insert(preset.name.to_lowercase())
        });
        settings.game = settings.game.chars().take(128).collect();
        Ok(settings)
    }
}

pub struct SettingsStore {
    path: PathBuf,
    writable: bool,
}

impl SettingsStore {
    pub fn open() -> (Self, Settings, Option<String>) {
        let Some(dirs) = directories::ProjectDirs::from("io", "Potanull", "ConsoleLink") else {
            return (
                Self {
                    path: PathBuf::new(),
                    writable: false,
                },
                Settings::default(),
                Some("Settings directory is unavailable; changes will not be saved.".into()),
            );
        };
        Self::open_path(dirs.config_dir().join("settings.json"))
    }

    fn open_path(path: PathBuf) -> (Self, Settings, Option<String>) {
        match fs::read_to_string(&path) {
            Ok(json) => match Settings::parse(&json) {
                Ok(settings) => (Self { path, writable: true }, settings, None),
                Err(SettingsError::FutureSchema) => (Self { path, writable: false }, Settings::default(), Some("Settings belong to a newer version. Original file preserved; saving is disabled.".into())),
                Err(SettingsError::Invalid(error)) => {
                    let backup = path.with_extension(format!("invalid-{}.json", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
                    let preserved = fs::rename(&path, backup).is_ok();
                    (Self { path, writable: preserved }, Settings::default(), Some(format!("Could not load settings: {error}. Original file preserved.")))
                }
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (Self { path, writable: true }, Settings::default(), None),
            Err(error) => (Self { path, writable: false }, Settings::default(), Some(format!("Could not read settings: {error}. Saving is disabled."))),
        }
    }

    pub fn save(&self, settings: &Settings) -> Result<(), String> {
        if !self.writable {
            return Err("Settings file is protected; changes are not being saved.".into());
        }
        let parent = self.path.parent().ok_or("Invalid settings path")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        atomic_write(
            &self.path,
            &serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "json.{}.{}.tmp",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_settings_gain_automatic_sixty_fps_and_windowed_default() {
        let settings = Settings::parse(
            r#"{"schema_version":1,"mode":{"width":1920,"height":1080,"fps_num":30,"fps_den":1}}"#,
        )
        .unwrap();
        assert!(settings.capture_preferences.automatic);
        assert_eq!(settings.capture_preferences.preferred_fps, 60);
        assert_eq!(settings.game_window_mode, GameWindowMode::Windowed);
    }

    #[test]
    fn capture_preference_limits_and_window_mode_survive_roundtrip() {
        let settings = Settings::parse(r#"{"schema_version":1,"capture_preferences":{"automatic":false,"preferred_fps":900},"game_window_mode":"borderless"}"#).unwrap();
        assert!(!settings.capture_preferences.automatic);
        assert_eq!(settings.capture_preferences.preferred_fps, 60);
        assert_eq!(settings.game_window_mode, GameWindowMode::Borderless);
        let decoded = Settings::parse(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(decoded.capture_preferences, settings.capture_preferences);
        assert_eq!(decoded.game_window_mode, settings.game_window_mode);
    }

    #[test]
    fn rejects_future_schema_without_overwriting() {
        assert!(matches!(
            Settings::parse(r#"{"schema_version":999}"#),
            Err(SettingsError::FutureSchema)
        ));
    }

    #[test]
    fn validates_values_and_preserves_rational_fps() {
        let settings = Settings::parse(r#"{"schema_version":1,"gain":3.0,"sync_ms":-900,"mode":{"width":1920,"height":1080,"fps_num":60000,"fps_den":1001}}"#).unwrap();
        assert_eq!(settings.gain, 1.0);
        assert_eq!(settings.sync_ms, -500);
        assert_eq!(settings.mode.unwrap().fps_den, 1001);
    }

    #[test]
    fn corrupt_or_invalid_mode_is_rejected() {
        assert!(Settings::parse("not json").is_err());
        assert!(Settings::parse(r#"{"schema_version":1,"mode":{"width":1920,"height":1080,"fps_num":60,"fps_den":0}}"#).is_err());
    }

    #[test]
    fn future_file_cannot_be_overwritten() {
        let path =
            std::env::temp_dir().join(format!("console-link-future-{}.json", std::process::id()));
        let original = r#"{"schema_version":999}"#;
        fs::write(&path, original).unwrap();
        let (store, settings, _) = SettingsStore::open_path(path.clone());
        assert!(store.save(&settings).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn repeated_atomic_save_replaces_complete_file() {
        let path =
            std::env::temp_dir().join(format!("console-link-save-{}.json", std::process::id()));
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod equalizer_regressions {
    use super::*;
    #[test]
    fn legacy_settings_default_to_bypassed_eq_and_discard_ocr_flags() {
        let settings = Settings::parse(r#"{"schema_version":1,"recognition_enabled":true,"game_manual_override":false,"game":"Mario Kart 8 Deluxe"}"#).unwrap();
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(value["equalizer"]["enabled"], false);
        assert!(value.get("recognition_enabled").is_none());
        assert_eq!(settings.game, "Mario Kart 8 Deluxe");
    }
    #[test]
    fn equalizer_and_user_presets_roundtrip_and_clamp() {
        let settings = Settings::parse(r#"{"schema_version":1,"equalizer":{"enabled":true,"gains":[99,-99,0,0,0,0,0,0,0,0]},"equalizer_presets":[{"name":"Custom","gains":[1,2,3,4,5,6,7,8,9,10]}]}"#).unwrap();
        let value = serde_json::to_value(&settings).unwrap();
        assert_eq!(value["equalizer"]["gains"][0], 12.0);
        assert_eq!(value["equalizer"]["gains"][1], -12.0);
        let roundtrip = Settings::parse(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(serde_json::to_value(roundtrip).unwrap(), value);
    }
}

#[cfg(test)]
mod preset_storage_regressions {
    use super::*;
    #[test]
    fn edited_presets_survive_atomic_save_reload_and_delete() {
        let path =
            std::env::temp_dir().join(format!("console-link-presets-{}.json", std::process::id()));
        let (store, mut settings, _) = SettingsStore::open_path(path.clone());
        settings.equalizer.enabled = true;
        settings.equalizer.gains = [2.0; 10];
        settings.equalizer_presets = vec![EqualizerPreset {
            name: "Мой пресет".into(),
            gains: [2.0; 10],
        }];
        store.save(&settings).unwrap();
        let (store, mut settings, warning) = SettingsStore::open_path(path.clone());
        assert!(warning.is_none());
        assert!(settings.equalizer.enabled);
        assert_eq!(settings.equalizer_presets[0].gains, [2.0; 10]);
        settings.equalizer_presets[0].name = "Новое имя".into();
        settings.equalizer_presets[0].gains[5] = -3.0;
        store.save(&settings).unwrap();
        let (store, mut settings, _) = SettingsStore::open_path(path.clone());
        assert_eq!(settings.equalizer_presets[0].name, "Новое имя");
        assert_eq!(settings.equalizer_presets[0].gains[5], -3.0);
        settings.equalizer_presets.clear();
        store.save(&settings).unwrap();
        let (_, settings, _) = SettingsStore::open_path(path.clone());
        assert!(settings.equalizer_presets.is_empty());
        fs::remove_file(path).unwrap();
    }
}

#[cfg(test)]
mod overlay_settings_regressions {
    use super::*;
    #[test]
    fn old_share_flag_is_ignored_and_typed_comparison_tracks_preset_edits() {
        let settings =
            Settings::parse(r#"{"schema_version":1,"show_share_prompt":true,"gain":0.4}"#).unwrap();
        assert_eq!(settings.gain, 0.4);
        assert!(
            serde_json::to_value(&settings)
                .unwrap()
                .get("show_share_prompt")
                .is_none()
        );
        let mut edited = settings.clone();
        assert_eq!(edited, settings);
        edited.equalizer_presets.push(EqualizerPreset {
            name: "Мой".into(),
            gains: [1.0; 10],
        });
        assert_ne!(edited, settings);
    }
}

#[cfg(test)]
mod color_persistence_regressions {
    use super::*;
    #[test]
    fn legacy_files_keep_original_colors_and_color_presets_roundtrip() {
        let mut settings =
            Settings::parse(r#"{"schema_version":1,"gain":0.4,"game":"Saved title"}"#).unwrap();
        assert!(settings.color.is_identity());
        assert!(settings.color_presets.is_empty());
        assert_eq!(settings.game, "Saved title");
        settings.color = crate::color::FACTORY_PRESETS[4].1;
        settings.color_presets.push(ColorPreset {
            name: "Мой цвет".into(),
            settings: settings.color,
        });
        let decoded = Settings::parse(&serde_json::to_string(&settings).unwrap()).unwrap();
        assert_eq!(settings, decoded);
    }
    #[test]
    fn preset_names_and_parameters_are_sanitized_without_resetting_old_options() {
        let settings=Settings::parse(r#"{"schema_version":1,"gain":0.4,"color":{"enabled":true,"exposure":99,"hdr":99},"color_presets":[{"name":"  My  ","settings":{"contrast":99}},{"name":"my","settings":{}},{"name":" ","settings":{}}]}"#).unwrap();
        assert_eq!(settings.gain, 0.4);
        assert_eq!(settings.color.exposure, 2.0);
        assert_eq!(settings.color.hdr, 1.0);
        assert_eq!(settings.color_presets.len(), 1);
        assert_eq!(settings.color_presets[0].name, "My");
        assert_eq!(settings.color_presets[0].settings.contrast, 1.5);
    }
}

#[cfg(test)]
mod color_atomic_storage_regressions {
    use super::*;
    #[test]
    fn color_settings_and_edited_presets_survive_save_reload_and_delete() {
        let path = std::env::temp_dir().join(format!(
            "console-link-color-presets-{}.json",
            std::process::id()
        ));
        let (store, mut settings, _) = SettingsStore::open_path(path.clone());
        settings.color = crate::color::FACTORY_PRESETS[4].1;
        settings.color_presets = vec![ColorPreset {
            name: "Мой цвет".into(),
            settings: settings.color,
        }];
        store.save(&settings).unwrap();
        let (store, mut settings, warning) = SettingsStore::open_path(path.clone());
        assert!(warning.is_none());
        assert_eq!(settings.color, crate::color::FACTORY_PRESETS[4].1);
        settings.color_presets[0].name = "Новое имя".into();
        settings.color_presets[0].settings.exposure = -0.5;
        store.save(&settings).unwrap();
        let (store, mut settings, _) = SettingsStore::open_path(path.clone());
        assert_eq!(settings.color_presets[0].name, "Новое имя");
        assert_eq!(settings.color_presets[0].settings.exposure, -0.5);
        settings.color_presets.clear();
        store.save(&settings).unwrap();
        let (_, settings, _) = SettingsStore::open_path(path.clone());
        assert!(settings.color_presets.is_empty());
        std::fs::remove_file(path).unwrap();
    }
}
