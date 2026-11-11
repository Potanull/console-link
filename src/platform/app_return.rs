/// Protocol URLs only activate an existing window. They never authenticate a user.
pub fn activate_existing() {
    #[cfg(target_os = "windows")]
    windows_activate();
    // LaunchServices activates the registered .app on macOS. Login completion
    // also focuses its existing viewport independently of the browser protocol.
}

#[cfg(target_os = "windows")]
fn windows_activate() {
    use windows::Win32::{
        Foundation::{CloseHandle, HWND, LPARAM},
        System::Threading::{
            GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32,
            PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
        },
        UI::WindowsAndMessaging::{
            EnumWindows, GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE,
            SetForegroundWindow, ShowWindowAsync,
        },
    };
    struct Search {
        executable: String,
        own_pid: u32,
        window: Option<HWND>,
    }
    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> windows_core::BOOL {
        // SAFETY: EnumWindows calls synchronously with our live Search pointer.
        let search = unsafe { &mut *(data.0 as *mut Search) };
        if !unsafe { IsWindowVisible(window) }.as_bool() {
            return true.into();
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(window, Some(&mut pid));
        }
        if pid == search.own_pid {
            return true.into();
        }
        let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
        else {
            return true.into();
        };
        let mut path = vec![0_u16; 32768];
        let mut size = path.len() as u32;
        let read = unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                windows_core::PWSTR(path.as_mut_ptr()),
                &mut size,
            )
        };
        let _ = unsafe { CloseHandle(process) };
        if read.is_ok()
            && String::from_utf16_lossy(&path[..size as usize])
                .eq_ignore_ascii_case(&search.executable)
        {
            search.window = Some(window);
            return false.into();
        }
        true.into()
    }
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let mut search = Search {
        executable: executable.to_string_lossy().into_owned(),
        own_pid: unsafe { GetCurrentProcessId() },
        window: None,
    };
    // SAFETY: Search stays alive throughout the synchronous enumeration. Only
    // windows owned by the same executable path can be selected for activation.
    let _ = unsafe { EnumWindows(Some(visit), LPARAM((&mut search as *mut Search) as isize)) };
    if let Some(window) = search.window {
        unsafe {
            if IsIconic(window).as_bool() {
                let _ = ShowWindowAsync(window, SW_RESTORE);
            }
            let _ = SetForegroundWindow(window);
        }
    }
}
