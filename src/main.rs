#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use console_link::config::app as config;

mod app;
mod platform;

fn main() -> eframe::Result {
    // A protocol invocation only returns focus to an existing login. It never
    // carries an authorization code and must not start a second media process.
    if std::env::args()
        .any(|arg| arg == "--return-from-discord" || arg == "console-link://auth/complete")
    {
        platform::app_return::activate_existing();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--list-devices") {
        match console_link::capture::enumerate() {
            Ok(devices) => {
                for device in devices {
                    println!("{}", device.name);
                    for mode in device.modes {
                        println!("  {mode}");
                    }
                }
            }
            Err(error) => eprintln!("Video: {error}"),
        }
        for (label, result) in [
            ("Audio inputs", console_link::audio::input_devices()),
            ("Audio outputs", console_link::audio::output_devices()),
        ] {
            println!("{label}:");
            match result {
                Ok(devices) => {
                    for device in devices {
                        println!("  {}", device.name);
                    }
                }
                Err(error) => eprintln!("  {error}"),
            }
        }
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(config::TITLE)
            .with_icon(window_icon())
            .with_inner_size([
                dimension_argument("--ui-width", 0),
                dimension_argument("--ui-height", 1),
            ])
            .with_min_inner_size(config::MIN_WINDOW_SIZE),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        config::TITLE,
        options,
        Box::new(|context| Ok(Box::new(app::ConsoleApp::new(context)))),
    )
}

fn window_icon() -> eframe::egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/window-icon.png"
    )))
    .expect("embedded application icon is valid PNG")
}

fn argument_value(key: &str) -> Option<String> {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == key)
        .map(|args| args[1].clone())
}

fn dimension_argument(key: &str, axis: usize) -> f32 {
    argument_value(key)
        .and_then(|value| value.parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(config::MIN_WINDOW_SIZE[axis], config::MAX_WINDOW_SIZE[axis]))
        .unwrap_or(config::WINDOW_SIZE[axis])
}
