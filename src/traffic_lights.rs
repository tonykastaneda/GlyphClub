//! Hides the native close/miniaturize/zoom buttons on macOS. This app
//! draws its own in the sidebar instead (`ui::sidebar`) and wires their
//! clicks straight to the equivalent window actions in `main.rs` —
//! positioning the *native* buttons turned out to have a hard floor
//! inside their own ~32px-tall native title-bar container, with no safe
//! way found to move them past it.

use objc2_app_kit::{NSView, NSWindowButton};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

pub fn hide_native_buttons(window: &Window) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return;
    };
    let ns_view_ptr = appkit.ns_view.as_ptr() as *mut NSView;
    if ns_view_ptr.is_null() {
        return;
    }
    let view = unsafe { &*ns_view_ptr };
    let Some(ns_window) = view.window() else {
        return;
    };

    for kind in [
        NSWindowButton::CloseButton,
        NSWindowButton::MiniaturizeButton,
        NSWindowButton::ZoomButton,
    ] {
        if let Some(btn) = ns_window.standardWindowButton(kind) {
            btn.setHidden(true);
        }
    }
}
