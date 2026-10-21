//! Settings ▸ General ▸ Menu Bar (macOS) / System Tray (Windows): when it's
//! on, GlyphClub starts silently with just this icon — the Illustrator
//! plug-in can start it without a window popping up — and closing the
//! window hides it back here instead of quitting. `main.rs` owns the
//! window side of that (hiding/showing, and on macOS switching between a
//! Dock app and a menu-bar-only one); this is the icon and its menu.

use tray_icon::menu::{Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

/// The Settings checkbox's label on this platform.
#[cfg(target_os = "windows")]
pub const SETTING_LABEL: &str = "System Tray";
#[cfg(not(target_os = "windows"))]
pub const SETTING_LABEL: &str = "Menu Bar";

pub enum TrayAction {
    Open,
    Quit,
}

pub struct Tray {
    _icon: TrayIcon,
    open_id: MenuId,
    quit_id: MenuId,
}

impl Tray {
    /// `None` if the OS refused the icon — the setting then just behaves
    /// as if it were off, rather than hiding GlyphClub with no way back.
    pub fn new() -> Option<Self> {
        let open = MenuItem::new("Open GlyphClub", true, None);
        let quit = MenuItem::new("Quit GlyphClub", true, None);
        let menu = Menu::new();
        menu.append_items(&[&open, &PredefinedMenuItem::separator(), &quit]).ok()?;

        let builder = TrayIconBuilder::new()
            .with_tooltip("GlyphClub")
            .with_menu(Box::new(menu))
            .with_icon(icon()?);
        // macOS: a monochrome template the menu bar tints for light/dark.
        // Windows: the full-color app icon, and a left click opens the
        // window (the menu is on right click, as tray apps usually do).
        #[cfg(target_os = "macos")]
        let builder = builder.with_icon_as_template(true);
        #[cfg(not(target_os = "macos"))]
        let builder = builder.with_menu_on_left_click(false);

        Some(Self {
            _icon: builder.build().ok()?,
            open_id: open.id().clone(),
            quit_id: quit.id().clone(),
        })
    }

    /// Whether `event` is one of this menu's items. The macOS menu bar
    /// (`native_menu.rs`) shares muda's one event channel, so `main.rs`
    /// drains it once and offers each event to both.
    pub fn menu_action(&self, event: &MenuEvent) -> Option<TrayAction> {
        if event.id == self.open_id {
            Some(TrayAction::Open)
        } else if event.id == self.quit_id {
            Some(TrayAction::Quit)
        } else {
            None
        }
    }
}

/// Clicks on the icon itself (not its menu) — only Windows uses these: a
/// left click opens the window.
pub fn drain_icon_clicks() -> bool {
    let mut open = false;
    while let Ok(event) = TrayIconEvent::receiver().try_recv() {
        if let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        } = event
        {
            open = cfg!(target_os = "windows");
        }
    }
    open
}

fn icon() -> Option<Icon> {
    #[cfg(target_os = "macos")]
    let png: &[u8] = include_bytes!("../branding/tray/menubar-template@2x.png");
    #[cfg(not(target_os = "macos"))]
    let png: &[u8] = include_bytes!("../branding/tray/tray-64.png");
    let image = image::load_from_memory(png).ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}
