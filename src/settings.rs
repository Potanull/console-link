use crate::types::VideoMode;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SettingsError {
    FutureSchema,
    Invalid(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    pub video_id: String,
    pub mode: Option<VideoMode>,
    pub audio_input_id: String,
    pub audio_output_id: String,
    pub gain: f32,
    pub muted: bool,
    pub sync_ms: i32,
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
            audio_input_id: String::new(),
            audio_output_id: String::new(),
            gain: 0.75,
            muted: false,
            sync_ms: 0,
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
        settings.sync_ms = settings.sync_ms.clamp(-500, 500);
        if settings.mode.as_ref().is_some_and(|m| {
            m.width == 0
                || m.height == 0
                || m.width > 8192
                || m.height > 8192
                || m.fps_num == 0
                || m.fps_den == 0
        }) {
            return Err(SettingsError::Invalid("Invalid video mode".into()));
        }
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
