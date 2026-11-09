pub mod audio;
pub mod discord;
pub mod discord_auth;
pub mod game_recognition;
pub mod native;
pub mod scheduler;
pub mod session;
pub mod settings;
pub mod types;
pub mod video_policy;

#[cfg(test)]
mod platform_configuration {
    #[test]
    fn native_graphics_backend_is_enabled() {
        let available = wgpu::Instance::enabled_backend_features();
        #[cfg(target_os = "macos")]
        assert!(available.contains(wgpu::Backends::METAL));
        #[cfg(target_os = "windows")]
        assert!(available.contains(wgpu::Backends::DX12));
    }

    #[test]
    fn embedded_window_icon_decodes() {
        let icon =
            eframe::icon_data::from_png_bytes(include_bytes!("../assets/window-icon.png")).unwrap();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
