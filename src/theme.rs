//! Color palette and layout metrics, carried over from the old `style.css`
//! (the "sparse gallery" look) so the rewrite reads as the same app.

use vello::peniko::Color;

pub const CANVAS: Color = Color::from_rgb8(0x1a, 0x1a, 0x1a);
pub const SIDEBAR_BG: Color = Color::from_rgb8(0x22, 0x22, 0x24);
pub const PANEL_BG: Color = Color::from_rgb8(0x1c, 0x1c, 0x1e);
pub const TOOLBAR_BG: Color = Color::from_rgb8(0x1a, 0x1a, 0x1a);
pub const TILE_BG: Color = Color::from_rgb8(0x1c, 0x1c, 0x1e);
pub const TILE_BORDER: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x24);
pub const ACCENT: Color = Color::from_rgb8(0x0a, 0x84, 0xff);
pub const GREEN: Color = Color::from_rgb8(0x30, 0xd1, 0x58);
pub const GOLD: Color = Color::from_rgb8(0xff, 0xd6, 0x0a);
pub const TEXT: Color = Color::from_rgb8(0xf5, 0xf5, 0xf7);
pub const TEXT_SECONDARY: Color = Color::from_rgb8(0xc3, 0xc3, 0xc8);
pub const TEXT_TERTIARY: Color = Color::from_rgb8(0x98, 0x98, 0x9f);
pub const SEPARATOR: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x1c);
pub const CONTROL_BG: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x0e);
pub const CONTROL_BG_HOVER: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x1c);
/// Hover for a button that also has its own separate "active/selected"
/// look (e.g. a segmented control) — dimmer than `CONTROL_BG_HOVER` so the
/// two states stay visually distinct.
pub const CONTROL_HOVER_SUBTLE: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x14);
pub const CONTROL_BORDER: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x20);
pub const CONTROL_FOCUS: Color = Color::from_rgba8(0x0a, 0x84, 0xff, 0xb8);
pub const NAV_SELECTED_BG: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x17);
pub const NAV_HOVER_BG: Color = Color::from_rgba8(0xff, 0xff, 0xff, 0x10);
pub const DOT_INACTIVE: Color = Color::from_rgb8(0xb5, 0xb5, 0xb8);

/// Custom-drawn traffic lights (macOS only — see `ui::sidebar` and
/// `traffic_lights.rs`), matching the native ones' standard colors.
pub const TRAFFIC_CLOSE: Color = Color::from_rgb8(0xff, 0x5f, 0x57);
pub const TRAFFIC_MINIMIZE: Color = Color::from_rgb8(0xff, 0xbd, 0x2e);
pub const TRAFFIC_ZOOM: Color = Color::from_rgb8(0x28, 0xc8, 0x40);

pub const FRAME_PAD: f64 = 12.0;
pub const SIDEBAR_W: f64 = 236.0;
pub const DETAIL_W: f64 = 348.0;
pub const TOOLBAR_H: f64 = 56.0;
pub const NAV_ITEM_H: f64 = 32.0;
pub const FOLDER_ROW_H: f64 = 36.0;
pub const GRID_GAP: f64 = 16.0;
pub const GRID_PAD: f64 = 20.0;
pub const ROW_H: f64 = 52.0;

pub const MIN_TILE: f64 = 180.0;
pub const MAX_TILE: f64 = 360.0;
pub const DEFAULT_TILE: f64 = 240.0;
