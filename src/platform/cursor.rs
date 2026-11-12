//! Native cursor visibility paired with the application's idle-cursor state.

use std::{marker::PhantomData, rc::Rc};

/// Keep AppKit cursor calls on the UI thread and restore visibility on teardown.
/// Other platforms retain the requested state; egui controls their native cursor.
#[derive(Default)]
pub struct CursorVisibility {
    hidden: bool,
    _main_thread_only: PhantomData<Rc<()>>,
}

impl CursorVisibility {
    /// Request visibility only on state transitions. This does not use AppKit's
    /// global hide/unhide counter, which can become unbalanced across windows.
    pub fn set_hidden(&mut self, hidden: bool) {
        if self.hidden == hidden {
            return;
        }
        #[cfg(target_os = "macos")]
        {
            // The owning application invokes this from its UI thread. Refuse
            // a native call if an alternate caller violates that contract.
            let Some(_main_thread) = objc2::MainThreadMarker::new() else {
                return;
            };
            objc2_app_kit::NSCursor::setHiddenUntilMouseMoves(hidden);
        }
        self.hidden = hidden;
    }

    /// Last requested state, not a query of the system's visible cursor.
    pub fn is_hidden(&self) -> bool {
        self.hidden
    }
}

impl Drop for CursorVisibility {
    fn drop(&mut self) {
        self.set_hidden(false);
    }
}
