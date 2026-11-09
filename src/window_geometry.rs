use eframe::egui;

/// Full current-monitor bounds in egui desktop coordinates, including system bars.
/// Returns `None` while macOS is still leaving native fullscreen.
pub fn monitor_rect(
    frame: &eframe::Frame,
    ctx: &egui::Context,
) -> Result<Option<egui::Rect>, String> {
    platform_monitor_rect(frame, ctx)
}

#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn scaled_rect(x: f64, y: f64, width: f64, height: f64, scale: f64) -> Result<egui::Rect, String> {
    if ![x, y, width, height, scale].into_iter().all(f64::is_finite)
        || width <= 0.0
        || height <= 0.0
        || scale <= 0.0
    {
        return Err("Некорректная геометрия монитора".into());
    }
    let rect = egui::Rect::from_min_size(
        egui::pos2((x / scale) as f32, (y / scale) as f32),
        egui::vec2((width / scale) as f32, (height / scale) as f32),
    );
    if !rect.is_finite() {
        return Err("Геометрия монитора выходит за допустимые границы".into());
    }
    Ok(rect)
}

#[cfg(any(target_os = "macos", test))]
fn appkit_rect(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    primary_top: f64,
    zoom: f64,
) -> Result<egui::Rect, String> {
    // AppKit has a bottom-left origin; winit anchors its top-left origin to the
    // primary display, even when another display contains the active window.
    scaled_rect(x, primary_top - y - height, width, height, zoom)
}

#[cfg(target_os = "macos")]
fn platform_monitor_rect(
    frame: &eframe::Frame,
    ctx: &egui::Context,
) -> Result<Option<egui::Rect>, String> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSScreen, NSView, NSWindowStyleMask};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let mtm = MainThreadMarker::new().ok_or("Геометрию окна нужно читать в главном потоке")?;
    let handle = frame.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err("Окно не использует AppKit".into());
    };
    // SAFETY: eframe borrows a live NSView through this handle for the duration
    // of the call; MainThreadMarker above verifies AppKit thread confinement.
    let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    let window = view.window().ok_or("У окна отсутствует NSWindow")?;
    if window.styleMask().contains(NSWindowStyleMask::FullScreen) {
        return Ok(None);
    }
    let screen = window.screen().ok_or("У окна отсутствует монитор")?;
    // The first NSScreen is the primary display, unlike mainScreen(), which
    // follows the focused window and would break secondary-monitor positions.
    let screens = NSScreen::screens(mtm);
    let primary = screens.firstObject().ok_or("Не найден основной монитор")?;
    let primary_frame = primary.frame();
    let rect = screen.frame();
    appkit_rect(
        rect.origin.x,
        rect.origin.y,
        rect.size.width,
        rect.size.height,
        primary_frame.origin.y + primary_frame.size.height,
        f64::from(ctx.zoom_factor()),
    )
    .map(Some)
}

#[cfg(target_os = "windows")]
fn platform_monitor_rect(
    frame: &eframe::Frame,
    ctx: &egui::Context,
) -> Result<Option<egui::Rect>, String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::HWND,
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
    };

    let handle = frame.window_handle().map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("Окно не использует Win32".into());
    };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: the borrowed handle refers to the live eframe window, and info is
    // a correctly sized, writable MONITORINFO. These APIs only read OS state.
    unsafe {
        let monitor =
            MonitorFromWindow(HWND(handle.hwnd.get() as *mut _), MONITOR_DEFAULTTONEAREST);
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err("Не удалось получить геометрию монитора".into());
        }
    }
    let rect = info.rcMonitor;
    scaled_rect(
        f64::from(rect.left),
        f64::from(rect.top),
        f64::from(rect.right) - f64::from(rect.left),
        f64::from(rect.bottom) - f64::from(rect.top),
        f64::from(ctx.pixels_per_point()),
    )
    .map(Some)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_monitor_rect(
    _: &eframe::Frame,
    _: &egui::Context,
) -> Result<Option<egui::Rect>, String> {
    Err("Геометрия монитора поддерживается только в macOS и Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appkit_primary_and_secondary_coordinates_preserve_display_layout() {
        assert_eq!(
            appkit_rect(0.0, 0.0, 1920.0, 1080.0, 1080.0, 1.0).unwrap(),
            egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1920.0, 1080.0))
        );
        let left = appkit_rect(-1280.0, 0.0, 1280.0, 1024.0, 1080.0, 1.0).unwrap();
        assert_eq!(left.min, egui::pos2(-1280.0, 56.0));
        let above = appkit_rect(0.0, 1080.0, 1920.0, 1200.0, 1080.0, 1.25).unwrap();
        assert_eq!(above.min, egui::pos2(0.0, -960.0));
        assert_eq!(above.size(), egui::vec2(1536.0, 960.0));
    }

    #[test]
    fn physical_monitor_coordinates_include_negative_origins_and_dpi_scale() {
        let rect = scaled_rect(-2560.0, -1440.0, 2560.0, 1440.0, 2.0).unwrap();
        assert_eq!(rect.min, egui::pos2(-1280.0, -720.0));
        assert_eq!(rect.size(), egui::vec2(1280.0, 720.0));
        assert!(scaled_rect(0.0, 0.0, 100.0, 100.0, 0.0).is_err());
        assert!(scaled_rect(0.0, 0.0, f64::NAN, 100.0, 1.0).is_err());
    }
}
