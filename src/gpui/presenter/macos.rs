use super::SurfaceSlot;
use gpui::Window;
use objc2::{MainThreadMarker, MainThreadOnly, rc::Retained};
use objc2_app_kit::{NSAutoresizingMaskOptions, NSView, NSWindowOrderingMode};
use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use std::ptr::NonNull;

/// Retained siblings. The GPUI view remains above video and owns all input.
pub(super) struct NativeHost {
    video: Retained<NSView>,
    overlay: Retained<NSView>,
    surface: SurfaceSlot,
}

impl NativeHost {
    pub(super) fn attach(window: &Window, surface: SurfaceSlot) -> Result<Self, String> {
        let main_thread =
            MainThreadMarker::new().ok_or("Видеоповерхность требует главного потока AppKit.")?;
        let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return Err("GPUI не предоставил AppKit view.".into());
        };
        // SAFETY: GPUI lends a live NSView for this call on the main thread.
        // Retaining it keeps the reference valid through native resize/drop.
        let overlay = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
        let overlay = unsafe { Retained::retain(overlay as *const NSView as *mut NSView) }
            .ok_or("Не удалось удержать GPUI view.")?;
        // SAFETY: the retained GPUI view is attached to the live window here;
        // no event dispatch occurs between borrowing and retaining its parent.
        let parent = unsafe { overlay.superview() }.ok_or("GPUI view еще не добавлен в окно.")?;
        let video = NSView::initWithFrame(NSView::alloc(main_thread), overlay.frame());
        video.setAutoresizingMask(
            NSAutoresizingMaskOptions::ViewWidthSizable
                | NSAutoresizingMaskOptions::ViewHeightSizable,
        );
        video.setWantsLayer(true);
        parent.addSubview_positioned_relativeTo(
            &video,
            NSWindowOrderingMode::Below,
            Some(&overlay),
        );
        Ok(Self {
            video,
            overlay,
            surface,
        })
    }

    pub(super) fn resize(&self) -> Result<(), String> {
        MainThreadMarker::new()
            .ok_or("Изменение видеоповерхности требует главного потока AppKit.")?;
        let frame = self.overlay.frame();
        if self.video.frame() != frame {
            self.video.setFrame(frame);
        }
        Ok(())
    }

    pub(super) fn handles(&self) -> (RawDisplayHandle, RawWindowHandle) {
        (
            RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            RawWindowHandle::AppKit(AppKitWindowHandle::new(NonNull::from(&*self.video).cast())),
        )
    }
}

impl Drop for NativeHost {
    fn drop(&mut self) {
        // The presenter drops its Metal surface before reaching this field.
        // NSView is main-thread-only, so this owner cannot move to a worker.
        self.surface.borrow_mut().take();
        self.video.removeFromSuperview();
    }
}
