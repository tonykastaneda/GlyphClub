//! Color palette and layout metrics, carried over from the old `style.css`
//! (the "sparse gallery" look) so the rewrite reads as the same app.
//!
//! Colors are resolved at call time from whichever of [`DARK`]/[`LIGHT`]
//! [`current_mode`] says is active, rather than being plain `const`s — see
//! `set_mode`, called from `App`'s settings (the Appearance dropdown, see
//! `ui::settings`) and from `WindowEvent::ThemeChanged` (`main.rs`) when
//! the setting is `SystemDefault`. Every color below is a zero-arg
//! function rather than a constant for that reason; layout metrics
//! (sizes, not colors) are unaffected by theme and stay as plain `const`s.
#![allow(non_snake_case)]

use std::sync::atomic::{AtomicU8, Ordering};

use vello::peniko::Color;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

struct Palette {
    canvas: Color,
    sidebar_bg: Color,
    panel_bg: Color,
    toolbar_bg: Color,
    tile_bg: Color,
    tile_border: Color,
    brand_accent: Color,
    green: Color,
    amber: Color,
    gold: Color,
    text: Color,
    text_secondary: Color,
    text_tertiary: Color,
    separator: Color,
    control_bg: Color,
    control_bg_hover: Color,
    control_hover_subtle: Color,
    control_border: Color,
    control_focus: Color,
    nav_selected_bg: Color,
    nav_hover_bg: Color,
    dot_inactive: Color,
    danger: Color,
}

const DARK: Palette = Palette {
    canvas: Color::from_rgb8(0x1a, 0x1a, 0x1a),
    sidebar_bg: Color::from_rgb8(0x22, 0x22, 0x24),
    panel_bg: Color::from_rgb8(0x1c, 0x1c, 0x1e),
    toolbar_bg: Color::from_rgb8(0x1a, 0x1a, 0x1a),
    tile_bg: Color::from_rgb8(0x1c, 0x1c, 0x1e),
    tile_border: Color::from_rgba8(0xff, 0xff, 0xff, 0x24),
    brand_accent: Color::from_rgb8(0x9a, 0xff, 0x00),
    green: Color::from_rgb8(0x30, 0xd1, 0x58),
    amber: Color::from_rgb8(0xff, 0x9f, 0x0a),
    gold: Color::from_rgb8(0xff, 0xd6, 0x0a),
    text: Color::from_rgb8(0xf5, 0xf5, 0xf7),
    text_secondary: Color::from_rgb8(0xc3, 0xc3, 0xc8),
    text_tertiary: Color::from_rgb8(0x98, 0x98, 0x9f),
    separator: Color::from_rgba8(0xff, 0xff, 0xff, 0x1c),
    control_bg: Color::from_rgba8(0xff, 0xff, 0xff, 0x0e),
    control_bg_hover: Color::from_rgba8(0xff, 0xff, 0xff, 0x1c),
    control_hover_subtle: Color::from_rgba8(0xff, 0xff, 0xff, 0x14),
    control_border: Color::from_rgba8(0xff, 0xff, 0xff, 0x20),
    control_focus: Color::from_rgba8(0x9a, 0xff, 0x00, 0xb8),
    nav_selected_bg: Color::from_rgba8(0xff, 0xff, 0xff, 0x17),
    nav_hover_bg: Color::from_rgba8(0xff, 0xff, 0xff, 0x10),
    dot_inactive: Color::from_rgb8(0xb5, 0xb5, 0xb8),
    danger: Color::from_rgb8(0xff, 0x5f, 0x57),
};

// Same shapes as `DARK`, inverted: light, near-white "canvas" with white
// cards, near-black text. `brand_accent`/`control_focus` are a darker,
// more saturated olive-lime rather than `DARK`'s neon one — the neon
// lime reads fine against near-black but washes out and loses contrast
// against a light/white background, so it's adjusted here rather than
// kept identical across both themes. The semantic status colors
// (`green`/`gold`/`danger`) stay the same in both: they're already
// saturated enough to read on either a light or dark surface, and
// changing them per-theme would make e.g. "this font is active" look
// like a different state depending on appearance.
const LIGHT: Palette = Palette {
    canvas: Color::from_rgb8(0xec, 0xec, 0xef),
    sidebar_bg: Color::from_rgb8(0xff, 0xff, 0xff),
    panel_bg: Color::from_rgb8(0xff, 0xff, 0xff),
    toolbar_bg: Color::from_rgb8(0xec, 0xec, 0xef),
    tile_bg: Color::from_rgb8(0xff, 0xff, 0xff),
    tile_border: Color::from_rgba8(0x00, 0x00, 0x00, 0x18),
    brand_accent: Color::from_rgb8(0x5c, 0x8a, 0x00),
    green: Color::from_rgb8(0x30, 0xd1, 0x58),
    amber: Color::from_rgb8(0xff, 0x9f, 0x0a),
    gold: Color::from_rgb8(0xff, 0xd6, 0x0a),
    text: Color::from_rgb8(0x1c, 0x1c, 0x1e),
    text_secondary: Color::from_rgb8(0x48, 0x48, 0x4a),
    text_tertiary: Color::from_rgb8(0x8e, 0x8e, 0x93),
    separator: Color::from_rgba8(0x00, 0x00, 0x00, 0x14),
    control_bg: Color::from_rgba8(0x00, 0x00, 0x00, 0x06),
    control_bg_hover: Color::from_rgba8(0x00, 0x00, 0x00, 0x0c),
    control_hover_subtle: Color::from_rgba8(0x00, 0x00, 0x00, 0x08),
    control_border: Color::from_rgba8(0x00, 0x00, 0x00, 0x18),
    control_focus: Color::from_rgba8(0x5c, 0x8a, 0x00, 0xb8),
    nav_selected_bg: Color::from_rgba8(0x00, 0x00, 0x00, 0x0a),
    nav_hover_bg: Color::from_rgba8(0x00, 0x00, 0x00, 0x06),
    dot_inactive: Color::from_rgb8(0xb0, 0xb0, 0xb5),
    danger: Color::from_rgb8(0xff, 0x5f, 0x57),
};

static MODE: AtomicU8 = AtomicU8::new(0);

pub fn set_mode(mode: ThemeMode) {
    MODE.store(if mode == ThemeMode::Light { 1 } else { 0 }, Ordering::Relaxed);
}

pub fn current_mode() -> ThemeMode {
    if MODE.load(Ordering::Relaxed) == 1 {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    }
}

fn current() -> &'static Palette {
    match current_mode() {
        ThemeMode::Dark => &DARK,
        ThemeMode::Light => &LIGHT,
    }
}

pub fn CANVAS() -> Color {
    current().canvas
}
pub fn SIDEBAR_BG() -> Color {
    current().sidebar_bg
}
pub fn PANEL_BG() -> Color {
    current().panel_bg
}
pub fn TOOLBAR_BG() -> Color {
    current().toolbar_bg
}
pub fn TILE_BG() -> Color {
    current().tile_bg
}
pub fn TILE_BORDER() -> Color {
    current().tile_border
}
/// The real brand color (see `branding::BRAND_LIME`) — every accent
/// moment in the app uses this, including the grid/list selection
/// highlight, which used to be reserved blue before that was changed to
/// match the rest of the theme.
pub fn BRAND_ACCENT() -> Color {
    current().brand_accent
}
pub fn GREEN() -> Color {
    current().green
}
pub fn AMBER() -> Color {
    current().amber
}
pub fn GOLD() -> Color {
    current().gold
}
pub fn TEXT() -> Color {
    current().text
}
pub fn TEXT_SECONDARY() -> Color {
    current().text_secondary
}
pub fn TEXT_TERTIARY() -> Color {
    current().text_tertiary
}
pub fn SEPARATOR() -> Color {
    current().separator
}
pub fn CONTROL_BG() -> Color {
    current().control_bg
}
pub fn CONTROL_BG_HOVER() -> Color {
    current().control_bg_hover
}
/// Hover for a button that also has its own separate "active/selected"
/// look (e.g. a segmented control) — dimmer than `CONTROL_BG_HOVER` so the
/// two states stay visually distinct.
pub fn CONTROL_HOVER_SUBTLE() -> Color {
    current().control_hover_subtle
}
pub fn CONTROL_BORDER() -> Color {
    current().control_border
}
pub fn CONTROL_FOCUS() -> Color {
    current().control_focus
}
pub fn NAV_SELECTED_BG() -> Color {
    current().nav_selected_bg
}
pub fn NAV_HOVER_BG() -> Color {
    current().nav_hover_bg
}
pub fn DOT_INACTIVE() -> Color {
    current().dot_inactive
}
/// For the rare genuinely destructive action (Font ▸ Delete from Library's
/// confirm button) — same red as the macOS traffic-light close dot, reused
/// here for its established "danger" meaning rather than redefined.
pub fn DANGER() -> Color {
    current().danger
}

/// Custom-drawn traffic lights (macOS only — see `ui::sidebar` and
/// `traffic_lights.rs`), matching the native ones' standard colors. Same
/// in both themes — real macOS traffic lights don't change color between
/// light and dark mode either.
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
