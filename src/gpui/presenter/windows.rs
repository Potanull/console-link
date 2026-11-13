use super::SurfaceSlot;
use gpui::Window;
use raw_window_handle::{
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::{cell::Cell, ffi::c_void, num::NonZeroIsize, rc::Rc, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BLACK_BRUSH, GetStockObject, HBRUSH},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GWLP_HINSTANCE,
            GWLP_USERDATA, GetClientRect, GetWindowLongPtrW, GetWindowThreadProcessId,
            HTTRANSPARENT, MA_NOACTIVATE, RegisterClassW, SWP_NOACTIVATE, SWP_NOZORDER,
            SetWindowLongPtrW, SetWindowPos, WM_DESTROY, WM_MOUSEACTIVATE, WM_NCDESTROY,
            WM_NCHITTEST, WNDCLASSW, WS_CHILD, WS_DISABLED, WS_EX_NOACTIVATE,
            WS_EX_NOREDIRECTIONBITMAP, WS_EX_TRANSPARENT, WS_VISIBLE,
        },
    },
    core::w,
};

struct ChildState {
    surface: SurfaceSlot,
    alive: Rc<Cell<bool>>,
}

/// This child owns no input. GPUI's DirectComposition visual tree is drawn
/// above child HWNDs (CreateTargetForHwnd(parent, TRUE) in GPUI 0.2.2).
pub(super) struct NativeHost {
    parent: HWND,
    video: HWND,
    alive: Rc<Cell<bool>>,
    surface: SurfaceSlot,
}

impl NativeHost {
    pub(super) fn attach(window: &Window, surface: SurfaceSlot) -> Result<Self, String> {
        if std::env::var("GPUI_DISABLE_DIRECT_COMPOSITION")
            .is_ok_and(|value| value == "1" || value == "true")
        {
            return Err("Видеовывод требует DirectComposition. Убери GPUI_DISABLE_DIRECT_COMPOSITION и перезапусти приложение.".into());
        }
        let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Err("GPUI не предоставил Win32 window handle.".into());
        };
        let parent = HWND(handle.hwnd.get() as *mut c_void);
        // SAFETY: GPUI lends this live HWND. All operations stay on its owning
        // UI thread; the disabled child cannot take input from the GPUI parent.
        unsafe {
            if GetWindowThreadProcessId(parent, None) != GetCurrentThreadId() {
                return Err("Видеоповерхность требует потока окна GPUI.".into());
            }
            if GetWindowLongPtrW(parent, GWL_EXSTYLE) & WS_EX_NOREDIRECTIONBITMAP.0 as isize == 0 {
                return Err(
                    "Окно GPUI не использует DirectComposition; видеоповерхность не создана."
                        .into(),
                );
            }
            let instance = HINSTANCE(GetWindowLongPtrW(parent, GWLP_HINSTANCE) as *mut c_void);
            register_class(instance)?;
            let mut rect = RECT::default();
            GetClientRect(parent, &mut rect).map_err(|error| error.to_string())?;
            let video = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                w!("ConsoleLinkWgpuVideoHost"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_DISABLED,
                0,
                0,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                Some(parent),
                None,
                Some(instance),
                None,
            )
            .map_err(|error| format!("Не удалось создать окно видео: {error}"))?;
            let alive = Rc::new(Cell::new(true));
            let state = Box::new(ChildState {
                surface: Rc::clone(&surface),
                alive: Rc::clone(&alive),
            });
            let state = Box::into_raw(state);
            SetWindowLongPtrW(video, GWLP_USERDATA, state as isize);
            if GetWindowLongPtrW(video, GWLP_USERDATA) != state as isize {
                drop(Box::from_raw(state));
                let _ = DestroyWindow(video);
                return Err("Не удалось установить обработчик жизненного цикла окна видео.".into());
            }
            Ok(Self {
                parent,
                video,
                alive,
                surface,
            })
        }
    }

    pub(super) fn resize(&self) -> Result<(), String> {
        if !self.alive.get() {
            return Err("Окно видео уже закрыто.".into());
        }
        // SAFETY: this main-thread-only owner checks the child's lifetime above.
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(self.parent, &mut rect).map_err(|error| error.to_string())?;
            SetWindowPos(
                self.video,
                None,
                0,
                0,
                (rect.right - rect.left).max(1),
                (rect.bottom - rect.top).max(1),
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
            .map_err(|error| error.to_string())
        }
    }

    pub(super) fn handles(&self) -> (RawDisplayHandle, RawWindowHandle) {
        (
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            RawWindowHandle::Win32(Win32WindowHandle::new(
                NonZeroIsize::new(self.video.0 as isize)
                    .expect("CreateWindowExW returned a non-null HWND"),
            )),
        )
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        // Drop the swap chain while the child HWND still exists. WM_DESTROY
        // also clears this slot when Windows destroys the parent first.
        self.surface.borrow_mut().take();
        if self.alive.get() {
            // SAFETY: Rc makes this owner !Send. DestroyWindow dispatches the
            // window procedure on this same UI thread and releases its boxed state.
            let _ = unsafe { DestroyWindow(self.video) };
        }
    }
}

fn register_class(instance: HINSTANCE) -> Result<(), String> {
    static REGISTERED: OnceLock<Result<(), String>> = OnceLock::new();
    REGISTERED
        .get_or_init(|| {
            // SAFETY: class data is copied by RegisterClassW. Its constant name and
            // Rust window procedure remain valid for this process's lifetime.
            unsafe {
                let class = WNDCLASSW {
                    lpfnWndProc: Some(child_proc),
                    hInstance: instance,
                    lpszClassName: w!("ConsoleLinkWgpuVideoHost"),
                    hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                    ..Default::default()
                };
                if RegisterClassW(&class) == 0 {
                    Err(format!(
                        "Не удалось зарегистрировать окно видео: {}",
                        windows::core::Error::from_thread()
                    ))
                } else {
                    Ok(())
                }
            }
        })
        .clone()
}

unsafe extern "system" fn child_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: installed only on our private class. After creation GWLP_USERDATA holds a live
    // Box<ChildState>. Its allocation is released exactly once at WM_NCDESTROY.
    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut ChildState;
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let state = unsafe { &*state_ptr };
    if message == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    if message == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if message == WM_DESTROY {
        state.alive.set(false);
        // WM_DESTROY occurs before the HWND is invalidated at WM_NCDESTROY.
        // No GPUI/OS event dispatch may be called from a video draw closure.
        state.surface.borrow_mut().take();
    }
    if message == WM_NCDESTROY {
        // SAFETY: clear the pointer before releasing its allocation; subsequent
        // native messages can no longer observe this private state.
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(state_ptr));
        }
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
