//! macOS app-level plumbing for Menu Bar mode (see `tray.rs`): showing or
//! hiding GlyphClub's Dock icon, and noticing when someone opens GlyphClub
//! again (Finder, Spotlight, the Dock) while it's running with no window.

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{define_class, msg_send, sel, AllocAnyThread, DefinedClass};
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
use objc2_foundation::MainThreadMarker;
use winit::event_loop::EventLoopProxy;

use crate::AppEvent;

/// With a window open GlyphClub is a normal app (Dock icon, menu bar);
/// with only the menu bar icon left it drops out of the Dock and ⌘-Tab.
pub fn set_in_dock(in_dock: bool, icon_png: &[u8]) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let policy = if in_dock {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    app.setActivationPolicy(policy);
    if in_dock {
        // A policy switch resets the Dock tile to the generic icon.
        crate::traffic_lights::set_dock_icon(icon_png);
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

define_class!(
    // Handles the "reopen" Apple Event macOS sends a running app when it's
    // opened again. winit leaves it to AppKit's default, which does nothing
    // for an app with no visible window.
    #[unsafe(super(NSObject))]
    #[name = "GlyphClubReopenHandler"]
    #[ivars = EventLoopProxy<AppEvent>]
    struct ReopenHandler;

    impl ReopenHandler {
        #[unsafe(method(handleReopen:withReplyEvent:))]
        fn handle_reopen(&self, _event: &NSObject, _reply: &NSObject) {
            let _ = self.ivars().send_event(AppEvent::ShowWindow);
        }
    }
);

impl ReopenHandler {
    fn new(proxy: EventLoopProxy<AppEvent>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(proxy);
        unsafe { msg_send![super(this), init] }
    }
}

/// Installs the reopen handler. Call once the app has finished launching
/// (`resumed`): AppKit installs its own handler while launching and would
/// replace this one.
pub fn install_reopen_handler(proxy: EventLoopProxy<AppEvent>) {
    // 'aevt' / 'rapp' — kCoreEventClass / kAEReopenApplication.
    const CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
    const REOPEN_APPLICATION: u32 = u32::from_be_bytes(*b"rapp");
    let handler = ReopenHandler::new(proxy);
    unsafe {
        let class = objc2::class!(NSAppleEventManager);
        let manager: *mut NSObject = msg_send![class, sharedAppleEventManager];
        let _: () = msg_send![
            manager,
            setEventHandler: &*handler,
            andSelector: sel!(handleReopen:withReplyEvent:),
            forEventClass: CORE_EVENT_CLASS,
            andEventID: REOPEN_APPLICATION
        ];
    }
    // The event manager doesn't retain its handlers; this lives as long as
    // the app does.
    std::mem::forget(handler);
}
