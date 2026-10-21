//! GlyphClub's gpui front end. Replaces the hand-drawn winit/vello UI in
//! `src/main.rs`; the catalog, scanning, activation and OS integrations
//! underneath are the shared core in `src/lib.rs`.

mod actions;
mod app;
mod bridge;
mod chrome;
mod detail;
mod fit;
mod grid;
mod illustrator;
mod library;
mod list;
mod palette;
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
    [Clock, Library, Monitor, Tag, LayoutGrid, List, TextCursorInput, TextAlignStart, TextAlignCenter, TextAlignEnd]
);

struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
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
    cx.activate(true);
    if let Some(handle) = cx.try_global::<MainWindow>().map(|m| m.0) {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
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
    let (bridge_tx, bridge_rx) = mpsc::channel();
    bridge::start(bridge_tx);

    let application = gpui_kit::application().with_assets(AppAssets);
    application.on_reopen(show_main_window);
    application.run(move |cx| {
        gpui_kit::init(cx);
        cx.set_global(AppSettings(settings));
        settings_window::set_apply_hook(apply_settings_change, cx);
        actions::bind_keys(cx);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_action(|_: &HideApp, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &OpenSettings, cx| settings_window::open(None, cx));
        cx.on_action(|_: &OpenGitHub, cx| cx.open_url(update::REPO_URL));
        cx.on_action(|_: &CloseWindow, cx| {
            if let Some(window) = cx.active_window() {
                let _ = window.update(cx, |_, window, _| window.remove_window());
            }
        });

        // Menu bar mode starts quietly, as just the icon.
        let start_hidden = settings.menu_bar;
        sync_tray(&settings, cx);

        let bounds = Bounds::centered(None, size(px(1440.), px(940.)), cx);
        let changed_for_view = changed.clone();
        let view_slot: std::rc::Rc<std::cell::RefCell<Option<WeakEntity<GlyphClub>>>> = Default::default();
        let slot = view_slot.clone();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(760.), px(480.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("GlyphClub".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(point(px(18.), px(19.))),
                    }),
                    // Only the empty parts of our title-bar strips drag the
                    // window; controls up there must not.
                    app_owns_titlebar_drag: true,
                    show: !start_hidden,
                    focus: !start_hidden,
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
            window.on_window_should_close(cx, |_, cx| {
                if cx.global::<AppSettings>().0.menu_bar {
                    cx.hide();
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
            let _ = weak.update(cx, |this, _| this.deactivate_temporary());
            bridge::stop();
            async {}
        })
        .detach();

        if !start_hidden {
            cx.activate(true);
        }
    });
}
