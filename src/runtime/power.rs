//! Session-scoped idle-sleep prevention. These requests do not change the user's
//! power settings or prevent explicit sleep, lid-close sleep, or shutdown.

pub(super) use native::PlaybackWakeLock;

#[cfg(target_os = "macos")]
mod native {
    use objc2_core_foundation::CFString;

    #[link(name = "IOKit", kind = "framework")]
    unsafe extern "C" {
        fn IOPMAssertionCreateWithName(
            assertion_type: &CFString,
            level: u32,
            name: &CFString,
            assertion_id: &mut u32,
        ) -> i32;
        fn IOPMAssertionRelease(assertion_id: u32) -> i32;
    }

    struct Assertion(u32);

    impl Assertion {
        fn new(kind: &str) -> Result<Self, String> {
            let kind = CFString::from_str(kind);
            let reason = CFString::from_str("Console Link: active console playback");
            let mut id = 0;
            // SAFETY: Both CFStrings and the output ID remain valid for the call.
            // IOKit retains the assertion until release or process termination.
            let status = unsafe { IOPMAssertionCreateWithName(&kind, 255, &reason, &mut id) };
            if status != 0 {
                return Err(format!("IOPMAssertionCreateWithName: {status:#x}"));
            }
            Ok(Self(id))
        }
    }

    impl Drop for Assertion {
        fn drop(&mut self) {
            // SAFETY: This ID belongs to this guard and is released exactly once.
            unsafe { IOPMAssertionRelease(self.0) };
        }
    }

    pub(in crate::runtime) struct PlaybackWakeLock {
        _display: Assertion,
        _system: Assertion,
    }

    impl PlaybackWakeLock {
        pub(in crate::runtime) fn new() -> Result<Self, String> {
            Ok(Self {
                _display: Assertion::new("PreventUserIdleDisplaySleep")?,
                _system: Assertion::new("PreventUserIdleSystemSleep")?,
            })
        }
    }
}

#[cfg(target_os = "windows")]
mod native {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
    use windows::{
        Win32::{
            Foundation::HANDLE,
            System::{
                Power::{
                    PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
                    PowerSetRequest,
                },
                Threading::{
                    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
                },
            },
        },
        core::PWSTR,
    };

    // A power-request handle can be released on any thread, unlike
    // SetThreadExecutionState. Closing it clears every request it owns.
    pub(in crate::runtime) struct PlaybackWakeLock {
        _request: OwnedHandle,
    }

    impl PlaybackWakeLock {
        pub(in crate::runtime) fn new() -> Result<Self, String> {
            let mut reason: Vec<u16> = "Console Link: active console playback\0"
                .encode_utf16()
                .collect();
            let context = REASON_CONTEXT {
                Version: 0, // POWER_REQUEST_CONTEXT_VERSION
                Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
                Reason: REASON_CONTEXT_0 {
                    SimpleReasonString: PWSTR(reason.as_mut_ptr()),
                },
            };
            // SAFETY: The context and its terminated UTF-16 reason are valid for
            // the call. PowerCreateRequest copies the reason into its object.
            let handle = unsafe { PowerCreateRequest(&context) }.map_err(|e| e.to_string())?;
            // SAFETY: PowerCreateRequest returned a new owned, valid handle.
            let request = unsafe { OwnedHandle::from_raw_handle(handle.0) };
            let handle = HANDLE(request.as_raw_handle());
            // SAFETY: Both requests use a live handle owned by `request`. Any
            // error drops it, also clearing a previously successful request.
            unsafe {
                PowerSetRequest(handle, PowerRequestDisplayRequired)
                    .and_then(|()| PowerSetRequest(handle, PowerRequestSystemRequired))
            }
            .map_err(|e| e.to_string())?;
            Ok(Self { _request: request })
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod native {
    pub(in crate::runtime) struct PlaybackWakeLock;

    impl PlaybackWakeLock {
        pub(in crate::runtime) fn new() -> Result<Self, String> {
            Err("Idle-sleep prevention is unsupported on this platform".into())
        }
    }
}
