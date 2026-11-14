//! Native window modes and cursor idle handling, owned by the UI thread.
use console_link::settings::GameWindowMode;
use gpui::Window;
use std::{
    marker::PhantomData,
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum Transition {
    Entering,
    Leaving,
}

#[derive(Default)]
pub struct WindowModes {
    mode: GameWindowMode,
    applied: GameWindowMode,
    transition: Option<Transition>,
    original: Option<native::SavedWindow>,
    _main_thread: PhantomData<Rc<()>>,
}
impl WindowModes {
    pub fn mode(&self) -> GameWindowMode {
        self.mode
    }
    pub fn set_mode(&mut self, mode: GameWindowMode, window: &mut Window) -> Result<(), String> {
        self.mode = mode;
        self.drive(window)
    }
    pub fn poll(&mut self, window: &mut Window) -> Result<(), String> {
        // Follow native green-button/OS fullscreen changes as well as shortcuts.
        if self.transition.is_none() {
            if window.is_fullscreen() && self.applied != GameWindowMode::Fullscreen {
                self.applied = GameWindowMode::Fullscreen;
                self.mode = GameWindowMode::Fullscreen;
            } else if !window.is_fullscreen() && self.applied == GameWindowMode::Fullscreen {
                self.mode = GameWindowMode::Windowed;
                self.applied = GameWindowMode::Windowed;
                if let Some(saved) = &self.original {
                    native::restore(window, saved)?;
                }
            }
        }
        self.drive(window)
    }
    fn drive(&mut self, window: &mut Window) -> Result<(), String> {
        match self.transition {
            Some(Transition::Entering) if !window.is_fullscreen() => return Ok(()),
            Some(Transition::Entering) => {
                self.transition = None;
                self.applied = GameWindowMode::Fullscreen;
            }
            Some(Transition::Leaving) if window.is_fullscreen() || native::fullscreen(window)? => {
                return Ok(());
            }
            Some(Transition::Leaving) => {
                self.transition = None;
                if let Some(saved) = &self.original {
                    native::restore(window, saved)?;
                }
                self.applied = GameWindowMode::Windowed;
            }
            None => {}
        }
        if window.is_fullscreen() {
            if self.mode != GameWindowMode::Fullscreen {
                self.transition = Some(Transition::Leaving);
                window.toggle_fullscreen();
            }
            return Ok(());
        }
        if self.original.is_none()
            || (self.applied == GameWindowMode::Windowed && self.mode != GameWindowMode::Windowed)
        {
            self.original = Some(native::save(window)?);
        }
        let original = self.original.as_ref().expect("saved above");
        match self.mode {
            GameWindowMode::Fullscreen => {
                // AppKit cannot enter a native Space with the borderless style mask.
                if self.applied == GameWindowMode::Borderless {
                    native::restore(window, original)?;
                }
                self.transition = Some(Transition::Entering);
                window.toggle_fullscreen();
            }
            GameWindowMode::Borderless if self.applied != GameWindowMode::Borderless => {
                if let Err(error) = native::borderless(window) {
                    let _ = native::restore(window, original);
                    return Err(error);
                }
                self.applied = GameWindowMode::Borderless;
            }
            GameWindowMode::Windowed if self.applied != GameWindowMode::Windowed => {
                native::restore(window, original)?;
                self.applied = GameWindowMode::Windowed;
            }
            _ => {}
        }
        Ok(())
    }
}

pub struct CursorIdle {
    last_motion: Instant,
    eligible: bool,
    hidden: bool,
    #[cfg(target_os = "windows")]
    hide_calls: u32,
    _main_thread: PhantomData<Rc<()>>,
}
impl Default for CursorIdle {
    fn default() -> Self {
        Self {
            last_motion: Instant::now(),
            eligible: false,
            hidden: false,
            #[cfg(target_os = "windows")]
            hide_calls: 0,
            _main_thread: PhantomData,
        }
    }
}
impl CursorIdle {
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
    pub fn update(
        &mut self,
        moving: bool,
        pointer_in_game: bool,
        active: bool,
        window: &mut Window,
    ) {
        self.update_at(Instant::now(), moving, pointer_in_game, active, window);
    }
    pub fn update_at(
        &mut self,
        now: Instant,
        moving: bool,
        pointer_in_game: bool,
        active: bool,
        window: &mut Window,
    ) {
        let eligible =
            active && pointer_in_game && native::focused(window) && native::pointer_inside(window);
        let hidden = idle_hidden(
            &mut self.last_motion,
            &mut self.eligible,
            now,
            moving,
            eligible,
        );
        self.set_hidden(hidden);
    }
    fn set_hidden(&mut self, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            if objc2::MainThreadMarker::new().is_none() {
                return;
            }
            objc2_app_kit::NSCursor::setHiddenUntilMouseMoves(hidden);
        }
        #[cfg(target_os = "windows")]
        // SAFETY: UI-thread transitions balance every decrement on restore and teardown.
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::ShowCursor;
            if hidden {
                loop {
                    self.hide_calls += 1;
                    if ShowCursor(false) < 0 {
                        break;
                    }
                }
            } else {
                for _ in 0..self.hide_calls {
                    ShowCursor(true);
                }
                self.hide_calls = 0;
            }
        }
        self.hidden = hidden;
    }
}
impl Drop for CursorIdle {
    fn drop(&mut self) {
        self.set_hidden(false);
    }
}
fn idle_hidden(
    last_motion: &mut Instant,
    was_eligible: &mut bool,
    now: Instant,
    moving: bool,
    eligible: bool,
) -> bool {
    if moving || !eligible || !*was_eligible {
        *last_motion = now;
    }
    *was_eligible = eligible;
    eligible && now.saturating_duration_since(*last_motion) >= Duration::from_secs(4)
}

fn point_inside(x: f64, y: f64, left: f64, top: f64, right: f64, bottom: f64) -> bool {
    [x, y, left, top, right, bottom]
        .into_iter()
        .all(f64::is_finite)
        && x >= left
        && x < right
        && y >= top
        && y < bottom
}

#[cfg(target_os = "macos")]
mod native {
    use super::*;
    use objc2::{MainThreadMarker, rc::Retained};
    use objc2_app_kit::{NSView, NSWindow, NSWindowStyleMask};
    use objc2_foundation::NSRect;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    pub struct SavedWindow {
        style: NSWindowStyleMask,
        frame: NSRect,
    }
    fn window(window: &Window) -> Result<Retained<NSWindow>, String> {
        let _mtm = MainThreadMarker::new().ok_or("Режим окна требует главного потока AppKit.")?;
        let handle = HasWindowHandle::window_handle(window).map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("У окна отсутствует AppKit handle.".into());
        };
        // SAFETY: GPUI lends a live NSView, and the main-thread check confines AppKit calls.
        unsafe { handle.ns_view.cast::<NSView>().as_ref() }
            .window()
            .ok_or("У окна отсутствует NSWindow.".into())
    }
    pub fn save(w: &Window) -> Result<SavedWindow, String> {
        let w = window(w)?;
        Ok(SavedWindow {
            style: w.styleMask(),
            frame: w.frame(),
        })
    }
    pub fn restore(w: &Window, s: &SavedWindow) -> Result<(), String> {
        let native = window(w)?;
        native.setStyleMask(s.style);
        native.setFrame_display(s.frame, true);
        notify_viewport_after_update(w)
    }
    pub fn borderless(w: &Window) -> Result<(), String> {
        let native = window(w)?;
        let screen = native.screen().ok_or("У окна отсутствует монитор.")?;
        native.setStyleMask(NSWindowStyleMask::Borderless);
        native.setFrame_display(screen.frame(), true);
        native.makeKeyWindow();
        notify_viewport_after_update(w)
    }
    fn notify_viewport_after_update(w: &Window) -> Result<(), String> {
        let mtm = MainThreadMarker::new().ok_or("Режим окна требует главного потока AppKit.")?;
        let handle = HasWindowHandle::window_handle(w).map_err(|e| e.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("У окна отсутствует AppKit handle.".into());
        };
        // SAFETY: GPUI lends a live NSView. Retain it until the main-queue notification finishes.
        let view = unsafe { Retained::retain(handle.ns_view.cast::<NSView>().as_ptr()) }
            .ok_or("У окна отсутствует NSView.")?;
        let view = dispatch2::MainThreadBound::new(view, mtm);
        dispatch2::DispatchQueue::main().exec_async(move || {
            let Some(mtm) = MainThreadMarker::new() else {
                return;
            };
            let view = view.get(mtm);
            if view.window().is_some() {
                // GPUI's synchronous resize callback cannot update a Window still borrowed by its UI.
                view.viewDidChangeBackingProperties();
            }
        });
        Ok(())
    }
    pub fn fullscreen(w: &Window) -> Result<bool, String> {
        Ok(window(w)?
            .styleMask()
            .contains(NSWindowStyleMask::FullScreen))
    }
    pub fn focused(w: &Window) -> bool {
        window(w).is_ok_and(|w| w.isKeyWindow())
    }
    pub fn pointer_inside(w: &Window) -> bool {
        let Ok(w) = window(w) else {
            return false;
        };
        let Some(content) = w.contentView() else {
            return false;
        };
        // AppKit supplies current coordinates even when GPUI has not received a mouse event.
        let point = content.convertPoint_fromView(w.mouseLocationOutsideOfEventStream(), None);
        let bounds = content.bounds();
        point_inside(
            point.x,
            point.y,
            bounds.origin.x,
            bounds.origin.y,
            bounds.origin.x + bounds.size.width,
            bounds.origin.y + bounds.size.height,
        )
    }
}

#[cfg(target_os = "windows")]
mod native {
    use super::*;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::{
        Foundation::{ERROR_SUCCESS, GetLastError, HWND, SetLastError},
        Graphics::Gdi::{
            GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
        },
        UI::WindowsAndMessaging::*,
    };
    pub struct SavedWindow {
        style: isize,
        extended: isize,
        placement: WINDOWPLACEMENT,
    }
    fn hwnd(w: &Window) -> Result<HWND, String> {
        let handle = HasWindowHandle::window_handle(w).map_err(|e| e.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("У окна отсутствует Win32 handle.".into());
        };
        Ok(HWND(handle.hwnd.get() as *mut _))
    }
    pub fn save(w: &Window) -> Result<SavedWindow, String> {
        let w = hwnd(w)?;
        let mut placement = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        // SAFETY: borrowed live HWND and writable correctly-sized WINDOWPLACEMENT.
        unsafe {
            GetWindowPlacement(w, &mut placement).map_err(|e| e.to_string())?;
            Ok(SavedWindow {
                style: GetWindowLongPtrW(w, GWL_STYLE),
                extended: GetWindowLongPtrW(w, GWL_EXSTYLE),
                placement,
            })
        }
    }
    fn set_style(w: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) -> Result<(), String> {
        // SAFETY: the caller borrows a live HWND; resetting last-error disambiguates a zero previous style.
        unsafe {
            SetLastError(ERROR_SUCCESS);
            if SetWindowLongPtrW(w, index, value) == 0 && GetLastError() != ERROR_SUCCESS {
                return Err(windows::core::Error::from_thread().to_string());
            }
        }
        Ok(())
    }
    pub fn restore(w: &Window, s: &SavedWindow) -> Result<(), String> {
        let w = hwnd(w)?;
        // SAFETY: restore only style/placement previously read from this live HWND.
        unsafe {
            set_style(w, GWL_STYLE, s.style)?;
            set_style(w, GWL_EXSTYLE, s.extended)?;
            SetWindowPlacement(w, &s.placement).map_err(|e| e.to_string())?;
            SetWindowPos(
                w,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|e| e.to_string())?;
        }
        notify_viewport_after_update(w)
    }
    pub fn borderless(w: &Window) -> Result<(), String> {
        let w = hwnd(w)?;
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: valid live HWND, initialized monitor struct and current-monitor rectangle.
        unsafe {
            if !GetMonitorInfoW(MonitorFromWindow(w, MONITOR_DEFAULTTONEAREST), &mut monitor)
                .as_bool()
            {
                return Err("Не удалось получить геометрию монитора.".into());
            }
            let style = GetWindowLongPtrW(w, GWL_STYLE) as u32;
            set_style(
                w,
                GWL_STYLE,
                ((style & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0) as isize,
            )?;
            let r = monitor.rcMonitor;
            SetWindowPos(
                w,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_FRAMECHANGED | SWP_NOZORDER | SWP_NOACTIVATE,
            )
            .map_err(|e| e.to_string())?;
        }
        notify_viewport_after_update(w)
    }
    fn notify_viewport_after_update(w: HWND) -> Result<(), String> {
        use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
        let mut rect = RECT::default();
        // SAFETY: live HWND and writable client rectangle; posting defers GPUI's resize callback beyond its borrowed Window update.
        unsafe {
            GetClientRect(w, &mut rect).map_err(|e| e.to_string())?;
            let width = u16::try_from(rect.right - rect.left)
                .map_err(|_| "Ширина окна не помещается в WM_SIZE.")?;
            let height = u16::try_from(rect.bottom - rect.top)
                .map_err(|_| "Высота окна не помещается в WM_SIZE.")?;
            let state = if IsZoomed(w).as_bool() {
                SIZE_MAXIMIZED
            } else {
                SIZE_RESTORED
            };
            PostMessageW(
                Some(w),
                WM_SIZE,
                WPARAM(state as usize),
                LPARAM((u32::from(width) | (u32::from(height) << 16)) as isize),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub fn fullscreen(_: &Window) -> Result<bool, String> {
        Ok(false)
    }
    pub fn focused(w: &Window) -> bool {
        hwnd(w).is_ok_and(|w| unsafe { GetForegroundWindow() == w })
    }
    pub fn pointer_inside(w: &Window) -> bool {
        use windows::Win32::{
            Foundation::{POINT, RECT},
            Graphics::Gdi::ScreenToClient,
        };
        let Ok(w) = hwnd(w) else {
            return false;
        };
        let mut point = POINT::default();
        let mut rect = RECT::default();
        // SAFETY: live HWND and writable stack values; these calls only read current OS geometry.
        unsafe {
            if GetCursorPos(&mut point).is_err()
                || !ScreenToClient(w, &mut point).as_bool()
                || GetClientRect(w, &mut rect).is_err()
            {
                return false;
            }
        }
        point_inside(
            f64::from(point.x),
            f64::from(point.y),
            f64::from(rect.left),
            f64::from(rect.top),
            f64::from(rect.right),
            f64::from(rect.bottom),
        )
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod native {
    use super::*;
    pub struct SavedWindow;
    pub fn save(_: &Window) -> Result<SavedWindow, String> {
        Err("Режимы окна поддерживаются на macOS и Windows.".into())
    }
    pub fn restore(_: &Window, _: &SavedWindow) -> Result<(), String> {
        Ok(())
    }
    pub fn borderless(_: &Window) -> Result<(), String> {
        Err("Безрамочный режим недоступен.".into())
    }
    pub fn fullscreen(_: &Window) -> Result<bool, String> {
        Ok(false)
    }
    pub fn focused(_: &Window) -> bool {
        false
    }
    pub fn pointer_inside(_: &Window) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_pointer_bounds_reject_desktop_titlebar_and_invalid_geometry() {
        assert!(point_inside(10.0, 20.0, 0.0, 0.0, 800.0, 600.0));
        assert!(!point_inside(-1.0, 20.0, 0.0, 0.0, 800.0, 600.0));
        assert!(!point_inside(10.0, -20.0, 0.0, 0.0, 800.0, 600.0));
        assert!(!point_inside(800.0, 600.0, 0.0, 0.0, 800.0, 600.0));
        assert!(!point_inside(f64::NAN, 20.0, 0.0, 0.0, 800.0, 600.0));
    }
    #[test]
    fn leaving_native_bounds_resets_idle_even_with_stale_gpui_pointer() {
        let start = Instant::now();
        let mut last = start;
        let mut was_eligible = false;
        let stale_gpui_pointer_in_game = true;
        for (seconds, native_inside, expected_hidden) in [
            (0, true, false),
            (4, true, true),
            (5, false, false),
            (10, false, false),
            (11, true, false),
            (14, true, false),
            (15, true, true),
        ] {
            assert_eq!(
                idle_hidden(
                    &mut last,
                    &mut was_eligible,
                    start + Duration::from_secs(seconds),
                    false,
                    stale_gpui_pointer_in_game && native_inside
                ),
                expected_hidden
            );
        }
    }
    #[test]
    fn cursor_restores_on_motion_focus_overlay_and_restarts_idle_deadline() {
        let start = Instant::now();
        let mut last = start;
        let mut eligible = false;
        assert!(!idle_hidden(&mut last, &mut eligible, start, false, true));
        assert!(idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(4),
            false,
            true
        ));
        assert!(!idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(5),
            true,
            true
        ));
        assert!(idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(9),
            false,
            true
        ));
        assert!(!idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(10),
            false,
            false
        ));
        assert!(!idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(20),
            false,
            true
        ));
        assert!(idle_hidden(
            &mut last,
            &mut eligible,
            start + Duration::from_secs(24),
            false,
            true
        ));
    }
}
