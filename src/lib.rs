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
        let icon = image::load_from_memory(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/window-icon.png"
        )))
        .unwrap();
        assert_eq!((icon.width(), icon.height()), (256, 256));
        let rgba = icon.to_rgba8();
        assert_eq!(rgba.len(), 256 * 256 * 4);
        for (x, y) in [(0, 0), (255, 0), (0, 255), (255, 255), (24, 24)] {
            assert_eq!(
                rgba.get_pixel(x, y)[3],
                0,
                "Rounded icon corner must be transparent"
            );
        }
        assert!(rgba.get_pixel(128, 128)[3] >= 250);
    }
}
pub mod nintendo;

pub mod runtime;

pub mod artwork;
pub mod gpu;
