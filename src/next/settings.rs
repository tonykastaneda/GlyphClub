//! Persisted preferences, in the same `settings.json` (and format) the old
//! front end uses, so switching between them keeps your choices.

use std::path::PathBuf;

use gpui_kit::Global;
use serde::{Deserialize, Serialize};

fn settings_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("glyphclub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("settings.json")
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Appearance {
    SystemDefault,
    Light,
    Dark,
}

/// Same variant names as the old front end's `app::SortField`, which is
/// what's in existing settings files.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum SortField {
    Enabled,
    Name,
    Tags,
    DateAdded,
    Starred,
    Styles,
    Format,
    Category,
}

impl SortField {
    /// View ▸ Sort By, in menu order.
    pub const MENU: [(SortField, &'static str); 7] = [
        (SortField::Name, "Name"),
        (SortField::DateAdded, "Date Added"),
        (SortField::Enabled, "Activated"),
        (SortField::Starred, "Starred"),
        (SortField::Styles, "Styles"),
        (SortField::Format, "Format"),
        (SortField::Tags, "Tags"),
    ];

    /// The direction a fresh pick starts in: active, starred, newest and
    /// most-styles first; A–Z otherwise.
    pub fn default_ascending(self) -> bool {
        !matches!(
            self,
            SortField::Enabled | SortField::DateAdded | SortField::Starred | SortField::Styles
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ViewMode {
    Grid,
    List,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub launch_on_startup: bool,
    /// A menu bar icon; GlyphClub then starts as just that icon and closing
    /// the window hides it there instead of quitting.
    pub menu_bar: bool,
    pub global_shortcut_enabled: bool,
    pub sidebar_show_all: bool,
    pub sidebar_show_system: bool,
    pub sidebar_show_starred: bool,
    pub sidebar_show_recent: bool,
    pub appearance: Appearance,
    pub sort_field: SortField,
    pub sort_ascending: bool,
    // Added by the gpui front end.
    pub view_mode: ViewMode,
    pub preview_size: f32,
    pub hide_missing: bool,
    pub show_sample_bar: bool,
    pub auto_activate_illustrator: bool,
    pub sidebar_width: f32,
    pub detail_width: f32,
    /// View ▸ Expand Font Families: every style gets its own tile/row.
    pub expand_families: bool,
    // Settings > Import (dropping font files on the window).
    pub import_skip_duplicates: bool,
    pub import_activate: bool,
    // Settings > Auto Activation (the Illustrator plug-in).
    pub activate_entire_family: bool,
    pub silent_mode: bool,
    pub deactivate_auto_on_quit: bool,
    pub deactivate_manual_on_quit: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            menu_bar: false,
            global_shortcut_enabled: false,
            sidebar_show_all: true,
            sidebar_show_system: true,
            sidebar_show_starred: true,
            sidebar_show_recent: true,
            appearance: Appearance::SystemDefault,
            sort_field: SortField::DateAdded,
            sort_ascending: false,
            view_mode: ViewMode::Grid,
            preview_size: 64.,
            hide_missing: false,
            show_sample_bar: true,
            auto_activate_illustrator: true,
            sidebar_width: crate::chrome::SIDEBAR_MIN_W,
            detail_width: crate::detail::DETAIL_MIN_W,
            expand_families: false,
            import_skip_duplicates: false,
            import_activate: true,
            activate_entire_family: true,
            silent_mode: false,
            deactivate_auto_on_quit: true,
            deactivate_manual_on_quit: false,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(settings_path(), json);
        }
    }
}

/// The live settings, shared by the main window, Settings window and menus.
pub struct AppSettings(pub Settings);

impl Global for AppSettings {}
