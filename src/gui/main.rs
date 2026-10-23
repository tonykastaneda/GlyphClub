//! GlyphClub's main GPUI application. The catalog, scanning, activation
//! and OS integrations are the shared core in `src/lib.rs`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod app;
mod bridge;
mod chrome;
mod detail;
mod drop_overlay;
mod export_wizard;
mod fit;
mod grid;
mod icon_view;
mod illustrator;
mod library;
mod list;
mod palette;
mod remote;
mod settings;
mod settings_window;
mod update;

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use glyphclub::catalog::Catalog;
use glyphclub::{login_item, tray};
use gpui_kit::*;

use crate::actions::*;
use crate::app::GlyphClub;
use crate::settings::{AppSettings, Settings};

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [Clock, Library, Monitor, Tag, Shapes, LayoutGrid, List, TextCursorInput, TextAlignStart, TextAlignCenter, TextAlignEnd, Download, ChevronsRight]
);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        // The drag-over cards (src/gui/drop_overlay.rs).
        let drop_card: Option<&'static [u8]> = match path {
            "drop/otf-dark.svg" => Some(include_bytes!("../../assets/drop/otf-dark.svg")),
            "drop/otf-light.svg" => Some(include_bytes!("../../assets/drop/otf-light.svg")),
            "drop/ttf-dark.svg" => Some(include_bytes!("../../assets/drop/ttf-dark.svg")),
            "drop/ttf-light.svg" => Some(include_bytes!("../../assets/drop/ttf-light.svg")),
            _ => None,
        };
        if let Some(bytes) = drop_card {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        if path == "branding/icon.png" {
            return Ok(Some(Cow::Borrowed(include_bytes!(
                "../../branding/glyphicon-macOS-Default-1024@1x.png"
            ))));
        }
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        Ok(paths)
    }
}

/// The main window, for the tray and the bridge to bring back.
struct MainWindow(WindowHandle<gpui_kit::base::Root>, WeakEntity<GlyphClub>);
impl Global for MainWindow {}

/// The menu bar icon, while Settings ▸ Show in the menu bar is on.
struct TrayIcon(Option<tray::Tray>);
impl Global for TrayIcon {}

fn sync_tray(settings: &Settings, cx: &mut App) {
    let want = settings.menu_bar;
    let has = cx.try_global::<TrayIcon>().is_some_and(|t| t.0.is_some());
    if want && !has {
        cx.set_global(TrayIcon(tray::Tray::new()));
    } else if !want && has {
        cx.set_global(TrayIcon(None));
    }
}

/// What a Settings change does beyond saving: the login item and the tray.
fn apply_settings_change(before: &Settings, after: &Settings, cx: &mut App) {
    if before.launch_on_startup != after.launch_on_startup {
        if let Err(e) = login_item::set_enabled(after.launch_on_startup) {
            eprintln!("login item: {e}");
        }
    }
    if before.menu_bar != after.menu_bar {
        sync_tray(after, cx);
    }
    if let Some(main) = cx.try_global::<MainWindow>().map(|m| m.1.clone()) {
        let _ = main.update(cx, |this, cx| {
            this.applied_appearance = None;
            this.refresh_menus(cx);
            this.refilter(cx);
        });
    }
}

fn show_main_window(cx: &mut App) {
    #[cfg(target_os = "macos")]
    set_in_dock(true);
    cx.activate(true);
    if let Some(handle) = cx.try_global::<MainWindow>().map(|m| m.0) {
        let _ = handle.update(cx, |_, window, _| {
            #[cfg(windows)]
            set_window_shown(window, true);
            window.activate_window()
        });
    }
}

/// Whether GlyphClub lives in the menu bar / system tray right now (the
/// setting is on and the icon is really there), so putting it away there
/// doesn't leave it unreachable.
fn in_tray(cx: &App) -> bool {
    cx.global::<AppSettings>().0.menu_bar && cx.try_global::<TrayIcon>().is_some_and(|t| t.0.is_some())
}

/// Puts GlyphClub away in the menu bar / system tray, still running:
/// macOS hides the app and takes it out of the Dock; Windows hides the main
/// window, which takes it off the taskbar (gpui's app-level hide does
/// nothing there).
fn hide_to_tray(window: &mut Window, cx: &mut App) {
    #[cfg(windows)]
    {
        let _ = cx;
        set_window_shown(window, false);
    }
    #[cfg(not(windows))]
    {
        let _ = window;
        cx.hide();
        #[cfg(target_os = "macos")]
        set_in_dock(false);
    }
}

/// macOS: whether GlyphClub has a Dock icon (a regular app) or lives only
/// in the menu bar (an accessory app).
#[cfg(target_os = "macos")]
pub(crate) fn set_in_dock(shown: bool) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let Some(mtm) = MainThreadMarker::new() else { return };
    let policy = if shown { NSApplicationActivationPolicy::Regular } else { NSApplicationActivationPolicy::Accessory };
    NSApplication::sharedApplication(mtm).setActivationPolicy(policy);
}

/// Cmd-W / Cmd-Q on the main window while GlyphClub lives in the menu bar
/// or tray: put it away there. Deferred, because the shortcut arrives while
/// the window is still handling the key press and can't be updated yet.
fn put_away_main_window(cx: &mut App) {
    cx.defer(|cx| {
        if let Some(handle) = cx.try_global::<MainWindow>().map(|m| m.0) {
            let _ = handle.update(cx, |_, window, cx| hide_to_tray(window, cx));
        }
    });
}

#[cfg(windows)]
pub(crate) fn set_window_shown(window: &Window, shown: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOW};
    let Ok(handle) = HasWindowHandle::window_handle(window) else { return };
    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
        let hwnd = HWND(win32.hwnd.get() as *mut core::ffi::c_void);
        unsafe {
            let _ = ShowWindow(hwnd, if shown { SW_SHOW } else { SW_HIDE });
        }
    }
}

/// Keep the branding icon for both bundled releases and bare development builds.
#[cfg(target_os = "macos")]
fn set_app_icon() {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else { return; };
    let data = NSData::with_bytes(include_bytes!("../../branding/glyphicon-macOS-Default-1024@1x.png"));
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)); }
    }
}

fn main() {
    // A second launch just brings the running one forward.
    if bridge::show_running_instance() {
        return;
    }

    let changed = Arc::new(AtomicBool::new(false));
    let mut catalog = {
        let changed = changed.clone();
        Catalog::open(move || changed.store(true, Ordering::Release)).expect("opening the font catalog")
    };
    // Temporary activations never outlive a session, even a crashed one.
    catalog.drop_leftover_temporary_activations();

    let settings = Settings::load();
    if settings.launch_on_startup {
        // Rewrites the login item to point at this build.
        login_item::refresh();
    }
    // The Illustrator plug-in starts GlyphClub with --background when a
    // document is missing fonts: show the window, but leave Illustrator in
    // front.
    let background = std::env::args().any(|a| a == "--background");
    bridge::record_app_path();
    let (bridge_tx, bridge_rx) = mpsc::channel();
    bridge::start(bridge_tx);

    let application = gpui_kit::application().with_assets(AppAssets);
    application.on_reopen(show_main_window);
    application.run(move |cx| {
        gpui_kit::init(cx);
        #[cfg(target_os = "macos")]
        set_app_icon();
        cx.set_global(AppSettings(settings));
        settings_window::set_apply_hook(apply_settings_change, cx);
        actions::bind_keys(cx);

        // Cmd-Q (File ▸ Exit on Windows) puts GlyphClub away in the menu
        // bar / tray while it lives there; the icon's own Quit really quits.
        cx.on_action(|_: &QuitCompletely, cx| cx.quit());
        cx.on_action(|_: &Quit, cx| {
            if !in_tray(cx) {
                return cx.quit();
            }
            put_away_main_window(cx);
        });
        cx.on_action(|_: &HideApp, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &OpenSettings, cx| settings_window::open(None, cx));
        cx.on_action(|_: &OpenGitHub, cx| cx.open_url(update::REPO_URL));
        // Cmd-W: the main window goes away to the menu bar / tray while
        // GlyphClub lives there (otherwise closing it quits, as before); any
        // other window (Settings) just closes. Deferred like Quit.
        cx.on_action(|_: &CloseWindow, cx| {
            let Some(active) = cx.active_window() else { return };
            let main = cx.try_global::<MainWindow>().map(|m| m.0);
            if main.is_some_and(|m| m.window_id() == active.window_id()) && in_tray(cx) {
                return put_away_main_window(cx);
            }
            cx.defer(move |cx| {
                let _ = active.update(cx, |_, window, _| window.remove_window());
            });
        });

        // Menu bar mode starts quietly, as just the icon.
        // Opening GlyphClub opens its window. Only a quiet start (login, or
        // the Illustrator plug-in: --background) stays in the menu bar /
        // tray while it lives there.
        let start_hidden = settings.menu_bar && background;
        sync_tray(&settings, cx);

        let bounds = Bounds::centered(None, size(px(1440.), px(940.)), cx);
        let changed_for_view = changed.clone();
        let view_slot: std::rc::Rc<std::cell::RefCell<Option<WeakEntity<GlyphClub>>>> = Default::default();
        let slot = view_slot.clone();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(480.), px(420.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("GlyphClub".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(18.), px(19.))),
                    }),
                    // Only the empty parts of our title-bar strips drag the
                    // window; controls up there must not.
                    app_owns_titlebar_drag: true,
                    show: !start_hidden,
                    focus: !start_hidden && !background,
                    app_id: Some("com.tonykastaneda.glyphclub".into()),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(|cx| GlyphClub::new(catalog, changed_for_view, window, cx));
                    view.update(cx, |this, cx| {
                        window.focus(&this.focus, cx);
                        this.refresh_menus(cx);
                    });
                    *slot.borrow_mut() = Some(view.downgrade());
                    cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
                },
            )
            .expect("opening the main window");
        let weak = view_slot.borrow_mut().take().expect("main view");
        cx.set_global(MainWindow(window, weak.clone()));

        // Closing the window quits, unless GlyphClub lives in the menu bar,
        // where it just hides.
        let _ = window.update(cx, |_, window, cx| {
            window.on_window_should_close(cx, |window, cx| {
                if in_tray(cx) {
                    hide_to_tray(window, cx);
                    false
                } else {
                    true
                }
            });
        });
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() || !cx.windows().iter().any(|w| w.downcast::<gpui_kit::base::Root>().is_some()) {
                cx.quit();
            }
        })
        .detach();

        // Bridge requests and tray clicks, drained on the main thread.
        cx.spawn(async move |cx| loop {
            cx.background_executor().timer(Duration::from_millis(100)).await;
            while let Ok(event) = bridge_rx.try_recv() {
                cx.update(|cx| {
                    let Some(handle) = cx.try_global::<MainWindow>().map(|m| m.0) else { return };
                    let weak = cx.global::<MainWindow>().1.clone();
                    let _ = handle.update(cx, |_, window, cx| {
                        let _ = weak.update(cx, |this, cx| this.handle_bridge(event, window, cx));
                    });
                });
            }
            let quit = cx.update(|cx| {
                let (mut open, mut quit) = (false, false);
                if let Some(Some(t)) = cx.try_global::<TrayIcon>().map(|t| t.0.as_ref()) {
                    while let Ok(event) = tray_icon::menu::MenuEvent::receiver().try_recv() {
                        match t.menu_action(&event) {
                            Some(tray::TrayAction::Open) => open = true,
                            Some(tray::TrayAction::Quit) => quit = true,
                            None => {}
                        }
                    }
                    open |= tray::drain_icon_clicks();
                }
                if open {
                    show_main_window(cx);
                }
                if quit {
                    cx.quit();
                }
                quit
            });
            if quit {
                return;
            }
        })
        .detach();

        cx.on_app_quit(move |cx| {
            let settings = cx.global::<AppSettings>().0;
            let _ = weak.update(cx, |this, _| this.deactivate_on_quit(&settings));
            bridge::stop();
            async {}
        })
        .detach();

        if !start_hidden && !background {
            cx.activate(true);
        }
        // Menu bar mode starts with no Dock icon either.
        #[cfg(target_os = "macos")]
        if start_hidden {
            set_in_dock(false);
        }
    });
}
