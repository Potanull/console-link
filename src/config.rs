//! Build-time defaults and operational limits. User preferences live in `settings`.

pub mod app {
    use std::time::Duration;

    pub const TITLE: &str = "Console Link";
    pub const REPOSITORY_URL: &str = "https://github.com/Potanull/console-link";
    pub const WINDOW_SIZE: [f32; 2] = [1200.0, 800.0];
    pub const MIN_WINDOW_SIZE: [f32; 2] = [720.0, 600.0];
    pub const MAX_WINDOW_SIZE: [f32; 2] = [1920.0, 1440.0];
    pub const EVENT_QUEUE_CAPACITY: usize = 16;
    pub const FRAME_QUEUE_CAPACITY: usize = 3;
    pub const SETTINGS_SAVE_DELAY: Duration = Duration::from_millis(700);
    pub const SIGNAL_TIMEOUT: Duration = Duration::from_secs(4);
}

pub mod capture {
    pub const HISTORY_BUDGET: usize = 320 * 1024 * 1024;
    pub const HISTORY_TOLERANCE_NS: u64 = 50_000_000;
    pub const MAX_FRAME_BYTES: usize = 256 * 1024 * 1024;
    pub const MAX_SYNC_MS: i32 = 500;
    pub const MAX_WIDTH: u32 = 1920;
    pub const MAX_HEIGHT: u32 = 1080;
    pub const MAX_FPS: u32 = 60;
    pub const MAX_DEVICE_DIMENSION: u32 = 8192;
    pub const RETRY_INITIAL_SECS: u64 = 2;
    pub const RETRY_MAX_SECS: u64 = 30;
}

pub mod audio {
    use std::time::Duration;

    pub const DEFAULT_GAIN: f32 = 0.75;
    pub const START_TIMEOUT: Duration = Duration::from_secs(15);
}

pub mod discord {
    use std::time::Duration;

    pub const DEFAULT_APPLICATION_ID: &str = "1569416573180051537";
    pub const REDIRECT_URI: &str = "http://127.0.0.1:45961/discord/callback";
    pub const CALLBACK_ADDRESS: &str = "127.0.0.1:45961";
    pub const AUTHORIZE_URL: &str = "https://discord.com/oauth2/authorize";
    pub const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
    pub const PROFILE_URL: &str = "https://discord.com/api/v10/users/@me";
    pub const REVOKE_URL: &str = "https://discord.com/api/oauth2/token/revoke";
    pub const ICON_URL: &str =
        "https://raw.githubusercontent.com/Potanull/console-link/main/assets/window-icon.png";
    pub const MAX_PACKET: usize = 64 * 1024;
    pub const MAX_HEADER_BYTES: usize = 8192;
    pub const MAX_RESPONSE_BYTES: u64 = 64 * 1024;
    pub const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
    pub const IO_TIMEOUT: Duration = Duration::from_millis(500);
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
    pub const HTTP_TIMEOUT: Duration = Duration::from_secs(8);
    pub const PUBLISH_INTERVAL: Duration = Duration::from_secs(15);
    pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(5);
}

pub mod artwork {
    use std::time::Duration;

    pub const QUEUE_CAPACITY: usize = 4;
    pub const TEXTURE_CACHE_CAPACITY: usize = 24;
    pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(3);
    pub const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(8);
    pub const RETRY_DELAY: Duration = Duration::from_secs(30);
    pub const MAX_DOWNLOAD_BYTES: u64 = 2_000_000;
    pub const MAX_IMAGE_DIMENSION: u32 = 2048;
    pub const MAX_IMAGE_ALLOCATION: u64 = 32 * 1024 * 1024;
    pub const THUMBNAIL_SIZE: u32 = 512;
}
