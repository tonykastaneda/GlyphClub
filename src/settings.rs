//! Persisted app preferences — a small JSON file next to the catalog
//! database (same `dirs`-based base directory `catalog.rs` uses), separate
//! from it since these are UI/app-level toggles rather than font-library
//! data `Store` owns. See `ui::settings` for the panel that edits these.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

fn settings_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("glyphclub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("settings.json")
}

/// `ui::sidebar::draw` filters its nav list by the four `sidebar_show_*`
/// flags here rather than hard-coding all of them as always shown.
#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// UI-present and persisted, but not yet wired to a real macOS login
    /// item / Windows startup registration — see `ui::settings`'s panel
    /// copy, which says so rather than implying it already works.
    pub launch_on_startup: bool,
    /// Same caveat as `launch_on_startup`: persisted, but closing the
    /// window still quits rather than backgrounding behind a menu-bar
    /// icon — that's real additional platform work, not yet done.
    pub run_in_background: bool,
    /// Same caveat again: no global hotkey is actually registered with the
    /// OS yet, just the toggle + a name recorded for later.
    pub global_shortcut_enabled: bool,
    pub sidebar_show_all: bool,
    pub sidebar_show_system: bool,
    pub sidebar_show_starred: bool,
    pub sidebar_show_recent: bool,
    pub appearance: Appearance,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            launch_on_startup: false,
            run_in_background: false,
            global_shortcut_enabled: false,
            sidebar_show_all: true,
            sidebar_show_system: true,
            sidebar_show_starred: true,
            sidebar_show_recent: true,
            appearance: Appearance::SystemDefault,
        }
    }
}

/// The Settings panel's Appearance dropdown — see `ui::settings`. Applied
/// via `crate::theme::set_mode`, resolved to a concrete `ThemeMode` by
/// `Appearance::resolve`: `SystemDefault` tracks whatever `main.rs` last
/// saw from `WindowEvent::ThemeChanged`/`Window::theme()`, `Light`/`Dark`
/// pin it regardless of the OS setting.
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Appearance {
    SystemDefault,
    Light,
    Dark,
}

impl Appearance {
    pub fn label(self) -> &'static str {
        match self {
            Appearance::SystemDefault => "System Default",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }

    /// `system_is_dark` is the OS's own current preference — irrelevant
    /// for `Light`/`Dark`, which override it outright.
    pub fn resolve(self, system_is_dark: bool) -> crate::theme::ThemeMode {
        match self {
            Appearance::SystemDefault if system_is_dark => crate::theme::ThemeMode::Dark,
            Appearance::SystemDefault => crate::theme::ThemeMode::Light,
            Appearance::Light => crate::theme::ThemeMode::Light,
            Appearance::Dark => crate::theme::ThemeMode::Dark,
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

/// Which single toggle a settings-panel row flips — see
/// `ui::settings::apply_toggle`, the one place that maps this to a field
/// mutation and a save. Hiding the currently-active sidebar filter doesn't
/// force-switch `App::filter` away from it — the content area keeps
/// filtering by it either way, it just loses its own sidebar row.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingToggle {
    LaunchOnStartup,
    RunInBackground,
    GlobalShortcutEnabled,
    SidebarAllFonts,
    SidebarSystemFonts,
    SidebarStarred,
    SidebarRecents,
}
