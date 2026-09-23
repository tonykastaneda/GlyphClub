//! The native macOS menu bar (an `NSMenu`, driven by `muda`) — same crate
//! and general shape as Amalith's own `native_menu.rs`, scaled down to
//! GlyphClub's much smaller command set. Windows has its own separate menu
//! plan and isn't touched here; see the `#[cfg(target_os = "macos")]` gate
//! on this module in `main.rs`.
//!
//! Every real item is registered into `items` as it's built (via `reg`)
//! and mapped back to a [`MenuAction`] when its click arrives on `muda`'s
//! global event channel — drained once per event-loop iteration in
//! `main.rs`'s `about_to_wait`. There's deliberately no "menu item that
//! renders but does nothing": every entry here is either a real `reg`'d
//! action or one of `muda`'s `PredefinedMenuItem`s, which macOS itself
//! wires up (About/Quit/Hide/Minimize/Zoom/etc.) with no channel event to
//! drain at all.

use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use objc2_foundation::{NSString, NSUserDefaults};

use crate::app::ViewMode;

/// AppKit auto-appends "Start Dictation…" and "Emoji & Symbols" to any
/// menu titled "Edit" — standard, expected behavior in most Mac apps, but
/// misleading here since our own Edit menu's one real item (Find) is a
/// hand-rolled field, not a real `NSTextView`, so neither of those would
/// actually insert anywhere. These two are the documented suppression
/// keys; there's no equivalent documented key for the third item AppKit
/// sometimes adds ("AutoFill" — password/text-replacement autofill,
/// normally tied to a real `NSTextField`), so that one may still show up.
pub fn suppress_system_edit_menu_items() {
    let defaults = NSUserDefaults::standardUserDefaults();
    defaults.setBool_forKey(true, &NSString::from_str("NSDisabledDictationMenuItem"));
    defaults.setBool_forKey(true, &NSString::from_str("NSDisabledCharacterPaletteMenuItem"));
}

#[derive(Clone, Copy)]
pub enum MenuAction {
    OpenAbout,
    CheckForUpdate,
    AddLibrary,
    Sync,
    FindFocus,
    SetView(ViewMode),
    ToggleInfo,
    ZoomIn,
    ZoomOut,
    Activate,
    ActivateTemporarily,
    Deactivate,
    ToggleFavorite,
    ExportSelected,
    RevealSelected,
    RemoveFromFontlist,
    DeleteSelected,
    OpenGitHub,
    OpenSettings,
    /// Routed through our own dispatch rather than `PredefinedMenuItem::
    /// quit` — quitting needs to uninstall every temporarily-activated
    /// font first (see `App::temp_active_ids`), which the predefined item
    /// terminates the process without giving us a chance to do.
    Quit,
}

pub struct NativeMenu {
    items: Vec<(muda::MenuId, MenuAction)>,
    view_checks: (CheckMenuItem, CheckMenuItem),
    // Kept alive for the process — dropping it tears the menu bar down.
    _menu: Menu,
}

impl NativeMenu {
    pub fn build(view_mode: ViewMode) -> Self {
        let sup = Some(Modifiers::SUPER);
        let sup_shift = Some(Modifiers::SUPER | Modifiers::SHIFT);
        let mk = |label: &str, mods, code| MenuItem::new(label, true, Some(Accelerator::new(mods, code)));

        let mut items: Vec<(muda::MenuId, MenuAction)> = Vec::new();
        fn reg<T: muda::IsMenuItem + Clone>(
            items: &mut Vec<(muda::MenuId, MenuAction)>,
            item: T,
            action: MenuAction,
        ) -> T {
            items.push((item.id().clone(), action));
            item
        }

        // A regular, `reg`'d item rather than `PredefinedMenuItem::about` —
        // opens our own in-app overlay (`ui::about`, consistent with how
        // Settings does modals here) instead of the native macOS About
        // panel.
        let about_i = reg(&mut items, MenuItem::new("About GlyphClub", true, None), MenuAction::OpenAbout);
        let quit = reg(&mut items, mk("Quit GlyphClub", sup, Code::KeyQ), MenuAction::Quit);
        let check_update_i =
            reg(&mut items, MenuItem::new("Check for Updates\u{2026}", true, None), MenuAction::CheckForUpdate);
        let settings_i = reg(&mut items, mk("Settings\u{2026}", sup, Code::Comma), MenuAction::OpenSettings);
        let app_menu = Submenu::with_items(
            "GlyphClub",
            true,
            &[
                &about_i,
                &PredefinedMenuItem::separator(),
                &check_update_i,
                &PredefinedMenuItem::separator(),
                &settings_i,
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::services(None),
                &PredefinedMenuItem::separator(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &PredefinedMenuItem::separator(),
                &quit,
            ],
        )
        .expect("app menu");

        let add_library_i = reg(&mut items, mk("Add Library\u{2026}", sup, Code::KeyO), MenuAction::AddLibrary);
        // No accelerator: Cmd+R is already the in-app font-activation
        // toggle (see `main.rs`'s `handle_key`) — giving Sync the same
        // key would collide with it.
        let sync_i = reg(&mut items, MenuItem::new("Sync Now", true, None), MenuAction::Sync);
        // Find lives here rather than its own Edit menu — a menu literally
        // titled "Edit" gets AppKit's Dictation/Emoji/AutoFill items
        // auto-injected into it unconditionally (no supported way to
        // suppress all three; see `suppress_system_edit_menu_items`'s doc
        // comment), which made no sense for an app with no other Edit
        // menu content and no real `NSTextView` for any of them to act on.
        let find_i = reg(&mut items, mk("Find", sup, Code::KeyF), MenuAction::FindFocus);
        let file_menu =
            Submenu::with_items("File", true, &[&add_library_i, &sync_i, &PredefinedMenuItem::separator(), &find_i])
                .expect("file menu");

        let grid_i = reg(
            &mut items,
            CheckMenuItem::new(
                "as Grid",
                true,
                matches!(view_mode, ViewMode::Grid),
                Some(Accelerator::new(sup, Code::Digit1)),
            ),
            MenuAction::SetView(ViewMode::Grid),
        );
        let list_i = reg(
            &mut items,
            CheckMenuItem::new(
                "as List",
                true,
                matches!(view_mode, ViewMode::List),
                Some(Accelerator::new(sup, Code::Digit2)),
            ),
            MenuAction::SetView(ViewMode::List),
        );
        let info_i = reg(&mut items, mk("Show Font Info", sup, Code::KeyI), MenuAction::ToggleInfo);
        let zoom_in_i = reg(&mut items, mk("Zoom In", sup, Code::Equal), MenuAction::ZoomIn);
        let zoom_out_i = reg(&mut items, mk("Zoom Out", sup, Code::Minus), MenuAction::ZoomOut);
        let view_menu = Submenu::with_items(
            "View",
            true,
            &[
                &grid_i,
                &list_i,
                &PredefinedMenuItem::separator(),
                &info_i,
                &PredefinedMenuItem::separator(),
                &zoom_in_i,
                &zoom_out_i,
            ],
        )
        .expect("view menu");

        let activate_i = reg(&mut items, mk("Activate", sup, Code::KeyR), MenuAction::Activate);
        let activate_temp_i = reg(
            &mut items,
            mk(
                "Activate Temporarily",
                Some(Modifiers::SUPER | Modifiers::ALT),
                Code::KeyR,
            ),
            MenuAction::ActivateTemporarily,
        );
        let deactivate_i = reg(&mut items, mk("Deactivate", sup_shift, Code::KeyR), MenuAction::Deactivate);
        let favorite_i = reg(
            &mut items,
            MenuItem::new("Mark Favorite", true, None),
            MenuAction::ToggleFavorite,
        );
        let export_i = reg(
            &mut items,
            MenuItem::new("Export\u{2026}", true, None),
            MenuAction::ExportSelected,
        );
        let reveal_i = reg(
            &mut items,
            MenuItem::new("Show in Finder", true, None),
            MenuAction::RevealSelected,
        );
        let remove_i = reg(
            &mut items,
            MenuItem::new("Remove from Fontlist", true, None),
            MenuAction::RemoveFromFontlist,
        );
        // The click here only opens the in-app confirmation overlay — the
        // actual delete is `MenuAction::DeleteSelected`'s job to hand off,
        // never to perform itself. See `ui::draw_delete_confirm`.
        let delete_i = reg(
            &mut items,
            mk("Delete from Library\u{2026}", sup, Code::Backspace),
            MenuAction::DeleteSelected,
        );
        let font_menu = Submenu::with_items(
            "Font",
            true,
            &[
                &activate_i,
                &activate_temp_i,
                &deactivate_i,
                &PredefinedMenuItem::separator(),
                &favorite_i,
                &PredefinedMenuItem::separator(),
                &export_i,
                &reveal_i,
                &PredefinedMenuItem::separator(),
                &remove_i,
                &delete_i,
            ],
        )
        .expect("font menu");

        let github_i = reg(
            &mut items,
            MenuItem::new("GlyphClub on GitHub", true, None),
            MenuAction::OpenGitHub,
        );
        let help_menu = Submenu::with_items("Help", true, &[&github_i]).expect("help menu");

        let menu = Menu::new();
        menu.append(&app_menu).expect("append app menu");
        menu.append(&file_menu).expect("append file menu");
        menu.append(&view_menu).expect("append view menu");
        menu.append(&font_menu).expect("append font menu");
        menu.append(&help_menu).expect("append help menu");
        menu.init_for_nsapp();

        Self {
            items,
            view_checks: (grid_i, list_i),
            _menu: menu,
        }
    }

    /// Ticks whichever of View ▸ as Grid / as List matches the live mode.
    pub fn sync_view(&self, view_mode: ViewMode) {
        self.view_checks.0.set_checked(matches!(view_mode, ViewMode::Grid));
        self.view_checks.1.set_checked(matches!(view_mode, ViewMode::List));
    }

    /// Every menu click queued since the last call.
    pub fn drain(&self) -> Vec<MenuAction> {
        let mut out = Vec::new();
        while let Ok(event) = muda::MenuEvent::receiver().try_recv() {
            if let Some((_, action)) = self.items.iter().find(|(id, _)| *id == event.id) {
                out.push(*action);
            }
        }
        out
    }
}
