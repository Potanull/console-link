#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod playback;
mod shortcuts;
mod theme;
mod window_geometry;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--list-devices") {
        match console_link::native::enumerate() {
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
            .with_title("Console Link")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/window-icon.png"))
                    .expect("embedded application icon is valid PNG"),
            )
            .with_inner_size([
                dimension_argument("--ui-width", 1040.0, 720.0, 1920.0),
                dimension_argument("--ui-height", 760.0, 600.0, 1440.0),
            ])
            .with_min_inner_size([720.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "Console Link",
        options,
        Box::new(|context| Ok(Box::new(app::ConsoleApp::new(context)))),
    )
}

fn dimension_argument(key: &str, default: f32, min: f32, max: f32) -> f32 {
    std::env::args()
        .collect::<Vec<_>>()
        .windows(2)
        .find(|args| args[0] == key)
        .and_then(|args| args[1].parse::<f32>().ok())
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(min, max))
        .unwrap_or(default)
}
