//! Colors for the current appearance. Dark is sampled from RightFont; light
//! is the same structure inverted.

use gpui_kit::component::ActiveTheme;
use gpui_kit::{rgb, App, Hsla};

#[derive(Clone, Copy)]
pub struct Palette {
    pub window: Hsla,
    pub sidebar: Hsla,
    pub tile: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub text_dim: Hsla,
    pub preview: Hsla,
    pub hover: Hsla,
    pub nav_active: Hsla,
    pub control: Hsla,
    pub chip: Hsla,
    pub bar: Hsla,
    /// GlyphClub's lime: selection and every accent.
    pub select: Hsla,
    /// Text and icons on top of `select`.
    pub on_select: Hsla,
    pub stripe: Hsla,
    pub active_dot: Hsla,
    pub inactive_dot: Hsla,
    pub banner: Hsla,
}

fn hex(v: u32) -> Hsla {
    rgb(v).into()
}

/// Points gpui-component's own controls (checkboxes, switches, sliders,
/// primary buttons, focus rings) at the lime too.
pub fn apply_brand(cx: &mut App) {
    let pal = Palette::get(cx);
    let t = gpui_kit::component::Theme::global_mut(cx);
    t.colors.primary = pal.select;
    t.colors.primary_hover = pal.select.opacity(0.88);
    t.colors.primary_active = pal.select.opacity(0.78);
    t.colors.primary_foreground = pal.on_select;
    t.colors.ring = pal.select;
    t.colors.switch = pal.select;
    t.colors.slider_bar = pal.select;
}

impl Palette {
    pub fn get(cx: &App) -> Self {
        if cx.theme().mode.is_dark() {
            Self {
                window: hex(0x232325),
                sidebar: hex(0x29292b),
                tile: hex(0x262628),
                border: hex(0x38383b),
                text: hex(0xe6e6e6),
                text_dim: hex(0x9a9a9a),
                preview: hex(0xe8e8e8),
                hover: hex(0x323235),
                nav_active: hex(0x3f3f42),
                control: hex(0x2e2e31),
                chip: hex(0x505053),
                bar: hex(0x404043).opacity(0.96),
                select: hex(0x9aff00),
                on_select: hex(0x101010),
                stripe: hex(0x29292b),
                active_dot: hex(0x9aff00),
                inactive_dot: hex(0xdddddd),
                banner: hex(0x24344f),
            }
        } else {
            Self {
                window: hex(0xf6f6f6),
                sidebar: hex(0xececec),
                tile: hex(0xffffff),
                border: hex(0xdcdcdc),
                text: hex(0x1d1d1f),
                text_dim: hex(0x77777c),
                preview: hex(0x111111),
                hover: hex(0xe2e2e2),
                nav_active: hex(0xd8d8d8),
                control: hex(0xffffff),
                chip: hex(0xdedede),
                bar: hex(0xffffff).opacity(0.97),
                select: hex(0x5c8a00),
                on_select: hex(0xffffff),
                stripe: hex(0xefefef),
                active_dot: hex(0x5c8a00),
                inactive_dot: hex(0xc4c4c4),
                banner: hex(0xdde8fb),
            }
        }
    }
}
