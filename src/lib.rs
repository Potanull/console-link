pub mod audio;
pub mod capture;
pub mod catalog;
pub mod color;
pub mod config;
pub mod discord;
pub mod monitoring;
pub mod settings;
pub mod upscale;

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
        let icon = eframe::icon_data::from_png_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/window-icon.png"
        )))
        .unwrap();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
    }
}
pub mod nintendo;
