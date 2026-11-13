#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[path = "platform/app_return.rs"]
mod app_return;
#[path = "gpui/mod.rs"]
mod desktop;

fn main() {
    if std::env::args().any(|a| a == "--return-from-discord" || a == "console-link://auth/complete")
    {
        app_return::activate_existing();
        return;
    }
    if let Some(directory) = argument("--install-nintendo-runtime") {
        let directory = std::path::PathBuf::from(directory);
        if !directory.is_absolute() {
            eprintln!("Nintendo install check requires an absolute destination.");
            std::process::exit(1);
        }
        let cancel = console_link::nintendo::installation::Cancellation::default();
        if let Err(error) = console_link::nintendo::installation::install(
            &directory,
            |stage| eprintln!("{}", stage.label()),
            &cancel,
        ) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        println!("Nintendo components verified.");
        return;
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
            Err(e) => eprintln!("Video: {e}"),
        }
        for (label, result) in [
            ("Audio inputs", console_link::audio::input_devices()),
            ("Audio outputs", console_link::audio::output_devices()),
        ] {
            println!("{label}:");
            match result {
                Ok(devices) => {
                    for d in devices {
                        println!("  {}", d.name);
                    }
                }
                Err(e) => eprintln!("{e}"),
            }
        }
        return;
    }
    desktop::run();
}

pub(crate) fn argument(name: &str) -> Option<String> {
    let args: Vec<_> = std::env::args().collect();
    args.windows(2).find(|a| a[0] == name).map(|a| a[1].clone())
}
