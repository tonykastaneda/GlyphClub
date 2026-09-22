//! Hides the native close/miniaturize/zoom buttons on macOS. This app
//! draws its own in the sidebar instead (`ui::sidebar`) and wires their
//! clicks straight to the equivalent window actions in `main.rs` —
//! positioning the *native* buttons turned out to have a hard floor
//! inside their own ~32px-tall native title-bar container, with no safe
//! way found to move them past it.

use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{sel, AnyThread, MainThreadMarker};
use objc2_app_kit::{NSApplication, NSImage, NSView, NSWindowButton};
use objc2_foundation::NSData;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;

/// Sets the real dock icon from `png_bytes` (the branding PNG, embedded via
/// `include_bytes!` in `main.rs`) — a bare `cargo run`/`cargo build` binary
/// has no `Info.plist`/`.icns` to pull one from otherwise, so without this
/// the dock just shows a generic placeholder. `NSApplication` will decode
/// the PNG itself; no separate image-decoding crate needed on macOS.
pub fn set_dock_icon(png_bytes: &[u8]) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(png_bytes);
    let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    unsafe { app.setApplicationIconImage(Some(&image)) };
}

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

/// Disables AppKit's default "drag the window by holding down anywhere
/// still covered by the nominal title-bar strip" behavior, unconditionally.
///
/// `main.rs`'s window attrs pair `titlebar_transparent`+`title_hidden` with
/// `fullsize_content_view` so the sidebar's own background can run to the
/// very top of the window (under the custom-drawn traffic lights) — but
/// the window is still `.titled`, just with its title bar made invisible,
/// not removed. For a still-`.titled` window, `-[NSView
/// mouseDownCanMoveWindow]`'s default implementation returns `YES` for a
/// non-opaque content view positioned under that nominal title-bar-height
/// strip — which is exactly winit's own Metal-backed content view here —
/// entirely independent of `NSWindow.movableByWindowBackground` (which
/// winit already leaves `false`). The toolbar's tile-size slider sits
/// partly inside that strip, so holding and dragging it was dragging the
/// whole window instead — confirmed by watching the window's on-screen
/// position shift under a synthetic press-drag on that control.
///
/// A first attempt tried to make this conditional (return `NO` only when
/// the cursor is over one of our own controls, `YES` otherwise) by reading
/// an `App::hit_regions`-derived flag from the swizzled method — but
/// logging showed AppKit only ever queries `mouseDownCanMoveWindow` once,
/// essentially at window-creation time, and caches the answer rather than
/// re-checking it on every mouse-down; a value that changes per-frame is
/// simply the wrong lever to pull here. This is unconditional instead, and
/// `main.rs`'s `MouseInput` handler manually re-implements "drag from
/// empty background" on top of it — see `WindowEvent::MouseInput`'s
/// `Pressed` arm, which calls the safe, public `Window::drag_window()`
/// (winit's own wrapper around `-[NSWindow performWindowDragWithEvent:]`,
/// the same call AppKit would otherwise have made itself) whenever a click
/// doesn't land on any registered hit region.
///
/// Winit owns the content view's actual class, so there's no Rust-level
/// subclass to override this on directly; registering a replacement
/// implementation for the one selector at the ObjC runtime level (the
/// standard fix for this specific AppKit interaction) is what this does.
pub fn disable_titlebar_drag(window: &Window) {
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
    let class = view.class() as *const AnyClass as *mut AnyClass;
    let types = c"c@:";
    unsafe {
        objc2::ffi::class_replaceMethod(
            class,
            sel!(mouseDownCanMoveWindow),
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool,
                Imp,
            >(no_move_window),
            types.as_ptr(),
        );
    }
}

unsafe extern "C-unwind" fn no_move_window(_this: *mut AnyObject, _cmd: Sel) -> Bool {
    Bool::NO
}
