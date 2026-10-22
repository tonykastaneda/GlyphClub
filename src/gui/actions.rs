//! Every menu command and keyboard shortcut, and the native menu bar built
//! from them.

use gpui_kit::component::input;
use gpui_kit::*;

use crate::settings::{Settings, SortField, ViewMode};

actions!(
    glyphclub,
    [
        About,
        CheckForUpdates,
        OpenSettings,
        Quit,
        // Quits even while GlyphClub lives in the menu bar (where Quit only
        // hides it): the menu bar icon's Quit, and glyphclub-cli.
        QuitCompletely,
        HideApp,
        HideOthers,
        ShowAll,
        AddLibrary,
        SyncNow,
        ExportUserFonts,
        CloseWindow,
        Find,
        SelectAll,
        ClearSelection,
        ViewGrid,
        ViewList,
        ToggleInfo,
        ToggleSidebar,
        ToggleSampleBar,
        ZoomIn,
        ZoomOut,
        ToggleHideMissing,
        ExpandFamilies,
        EditPreviewText,
        ToggleDarkMode,
        ToggleFullScreen,
        SortByName,
        SortByDateAdded,
        SortByEnabled,
        SortByStarred,
        SortByStyles,
        SortByFormat,
        SortByTags,
        Activate,
        ActivateTemporarily,
        Deactivate,
        ToggleStar,
        CopyFontName,
        ExportSelected,
        RevealSelected,
        RemoveFromFontlist,
        DeleteSelected,
        Minimize,
        ZoomWindow,
        OpenGitHub,
    ]
);

/// Key context of the main window's root element.
pub const CONTEXT: &str = "GlyphClub";

/// Shortcuts are written with macOS's cmd; on Windows that's ctrl (gpui's
/// cmd there is the Windows key, which the menus showed as Win+O).
fn keys(mac: &str) -> String {
    if cfg!(target_os = "macos") {
        mac.to_string()
    } else {
        mac.replace("cmd", "ctrl")
    }
}

pub fn bind_keys(cx: &mut App) {
    let c = Some(CONTEXT);
    if cfg!(target_os = "macos") {
        cx.bind_keys([
            KeyBinding::new("cmd-h", HideApp, None),
            KeyBinding::new("alt-cmd-h", HideOthers, None),
            KeyBinding::new("ctrl-cmd-f", ToggleFullScreen, None),
        ]);
    } else {
        cx.bind_keys([KeyBinding::new("f11", ToggleFullScreen, None)]);
    }
    let k = |s: &str| keys(s);
    cx.bind_keys([
        KeyBinding::new(&k("cmd-q"), Quit, None),
        KeyBinding::new(&k("cmd-,"), OpenSettings, None),
        KeyBinding::new(&k("cmd-o"), AddLibrary, None),
        KeyBinding::new(&k("cmd-w"), CloseWindow, None),
        KeyBinding::new(&k("cmd-m"), Minimize, None),
        KeyBinding::new(&k("cmd-f"), Find, c),
        KeyBinding::new(&k("cmd-a"), SelectAll, c),
        KeyBinding::new("escape", ClearSelection, c),
        KeyBinding::new(&k("cmd-1"), ViewGrid, c),
        KeyBinding::new(&k("cmd-2"), ViewList, c),
        KeyBinding::new(&k("cmd-i"), ToggleInfo, c),
        KeyBinding::new(&k("cmd-alt-s"), ToggleSidebar, c),
        KeyBinding::new(&k("cmd-t"), ToggleSampleBar, c),
        KeyBinding::new(&k("cmd-="), ZoomIn, c),
        KeyBinding::new(&k("cmd-+"), ZoomIn, c),
        KeyBinding::new(&k("cmd--"), ZoomOut, c),
        KeyBinding::new(&k("cmd-r"), Activate, c),
        KeyBinding::new(&k("alt-cmd-r"), ActivateTemporarily, c),
        KeyBinding::new(&k("shift-cmd-r"), Deactivate, c),
        KeyBinding::new(&k("cmd-g"), ExpandFamilies, c),
        KeyBinding::new(&k("cmd-e"), EditPreviewText, c),
        KeyBinding::new(&k("cmd-d"), ToggleDarkMode, None),
        KeyBinding::new(&k("shift-cmd-c"), CopyFontName, c),
        KeyBinding::new(&k("cmd-backspace"), DeleteSelected, c),
        KeyBinding::new(&k("shift-cmd-f"), RevealSelected, c),
    ]);
}

pub fn sort_action(field: SortField) -> Box<dyn Action> {
    match field {
        SortField::Name | SortField::Category => Box::new(SortByName),
        SortField::DateAdded => Box::new(SortByDateAdded),
        SortField::Enabled => Box::new(SortByEnabled),
        SortField::Starred => Box::new(SortByStarred),
        SortField::Styles => Box::new(SortByStyles),
        SortField::Format => Box::new(SortByFormat),
        SortField::Tags => Box::new(SortByTags),
    }
}

/// Rebuilt whenever a checked item's state changes.
pub fn set_menus(settings: &Settings, info_open: bool, _sidebar_open: bool, cx: &mut App) {
    let sort_items: Vec<MenuItem> = SortField::MENU
        .iter()
        .map(|&(field, label)| {
            MenuItem::Action {
                name: label.into(),
                action: sort_action(field),
                os_action: None,
                checked: settings.sort_field == field,
                disabled: false,
            }
        })
        .collect();

    // macOS has its own app menu and Window menu; Windows puts Settings and
    // Exit under File and About under Help, as Windows apps do.
    let mac = cfg!(target_os = "macos");
    let mut menus = Vec::new();
    if mac {
        menus.push(Menu::new("GlyphClub").items([
            MenuItem::action("About GlyphClub", About),
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide GlyphClub", HideApp),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit GlyphClub", Quit),
        ]));
    }
    let mut file = vec![
        MenuItem::action("Add Library…", AddLibrary),
        MenuItem::action("Sync Now", SyncNow),
        MenuItem::separator(),
        MenuItem::action("Export User Fonts…", ExportUserFonts),
        MenuItem::separator(),
    ];
    if mac {
        file.push(MenuItem::action("Close Window", CloseWindow));
    } else {
        file.extend([MenuItem::action("Settings…", OpenSettings), MenuItem::separator(), MenuItem::action("Exit", Quit)]);
    }
    menus.push(Menu::new("File").items(file));
    menus.push(Menu::new("Edit").items([
        MenuItem::os_action("Undo", input::Undo, OsAction::Undo),
        MenuItem::os_action("Redo", input::Redo, OsAction::Redo),
        MenuItem::separator(),
        MenuItem::os_action("Cut", input::Cut, OsAction::Cut),
        MenuItem::os_action("Copy", input::Copy, OsAction::Copy),
        MenuItem::os_action("Paste", input::Paste, OsAction::Paste),
        MenuItem::action("Select All", SelectAll),
        MenuItem::separator(),
        MenuItem::action("Find", Find),
    ]));
    menus.push(Menu::new("View").items([
        MenuItem::action("as Grid", ViewGrid).checked(settings.view_mode == ViewMode::Grid),
        MenuItem::action("as List", ViewList).checked(settings.view_mode == ViewMode::List),
        MenuItem::separator(),
        MenuItem::action("Expand Font Families", ExpandFamilies).checked(settings.expand_families),
        MenuItem::submenu(Menu::new("Sort By").items(sort_items)),
        MenuItem::separator(),
        MenuItem::action("Edit Preview Text", EditPreviewText),
        MenuItem::action("Increase Size", ZoomIn),
        MenuItem::action("Decrease Size", ZoomOut),
        MenuItem::separator(),
        MenuItem::action("Toggle Info Pane", ToggleInfo).checked(info_open),
        MenuItem::action("Toggle Dark Mode", ToggleDarkMode),
        MenuItem::action("Enter Full Screen", ToggleFullScreen),
    ]));
    menus.push(Menu::new("Font").items([
        MenuItem::action("Activate", Activate),
        MenuItem::action("Activate Temporarily", ActivateTemporarily),
        MenuItem::action("Deactivate", Deactivate),
        MenuItem::separator(),
        MenuItem::action("Star / Unstar", ToggleStar),
        MenuItem::action("Copy Font Name", CopyFontName),
        MenuItem::separator(),
        MenuItem::action("Export…", ExportSelected),
        MenuItem::action(if mac { "Show in Finder" } else { "Show in Explorer" }, RevealSelected),
        MenuItem::separator(),
        MenuItem::action("Remove from Fontlist", RemoveFromFontlist),
        MenuItem::action("Delete from Library…", DeleteSelected),
    ]));
    if mac {
        menus.push(Menu::new("Window").items([
            MenuItem::action("Minimize", Minimize),
            MenuItem::action("Zoom", ZoomWindow),
        ]));
        menus.push(Menu::new("Help").items([MenuItem::action("GlyphClub on GitHub", OpenGitHub)]));
        cx.set_menus(menus);
    } else {
        menus.push(Menu::new("Help").items([
            MenuItem::action("About GlyphClub", About),
            MenuItem::action("Check for Updates…", CheckForUpdates),
            MenuItem::separator(),
            MenuItem::action("GlyphClub on GitHub", OpenGitHub),
        ]));
        // Windows has no global menu bar: the in-window one (AppMenuBar, in
        // the title bar strip) reads its menus from gpui-kit's GlobalState,
        // not from cx.set_menus.
        let owned = menus.into_iter().map(Menu::owned).collect();
        gpui_kit::component::GlobalState::global_mut(cx).set_app_menus(owned);
    }
}
