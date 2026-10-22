//! What the window shows while font files are dragged over it: a dashed
//! lime border, a soft lime glow, and the .OTF / .TTF file cards fanning
//! out from a stack like a dealt hand, with "Drop to add a font" under
//! them. A short lime flash follows the drop. The same effect the old
//! hand-drawn UI had (see git history before 88b0067), rebuilt for gpui.
//!
//! The cards are `assets/drop/{otf,ttf}-{dark,light}.svg`: each card's
//! inside (dark) and its outline, label and glyph (light), drawn as two
//! single-color layers so they can be tinted for the theme and rotated.

use std::time::Duration;

use gpui_kit::*;

use crate::palette::Palette;

/// How long the cards take to fan out (and the glow to ease in).
const FAN: Duration = Duration::from_millis(280);
/// How long the lime flash after a drop takes to fade.
const FLASH: Duration = Duration::from_millis(350);

const CARD_H: f32 = 92.;
const CARD_W: f32 = CARD_H * 84.86 / 108.54;

/// Ease-out cubic: a fast start that settles gently rather than snapping.
fn ease_out_cubic(t: f32) -> f32 {
    1. - (1. - t).powi(3)
}

/// One layer of a card, `side` -1 (left, tilted left) or 1 (right).
fn card_layer(path: &'static str, color: Hsla, side: f32, hover: u64) -> impl IntoElement {
    svg()
        .path(path)
        .absolute()
        .size_full()
        .text_color(color)
        .with_animation(
            ElementId::NamedInteger(format!("drop-card-{path}").into(), hover),
            Animation::new(FAN).with_easing(ease_out_cubic),
            move |svg, t| {
                svg.with_transformation(
                    Transformation::rotate(radians((10. * t * side).to_radians()))
                        .with_translation(point(px(40. * t * side), px(0.))),
                )
            },
        )
}

fn card(kind: &'static str, side: f32, dark: bool, pal: &Palette, hover: u64) -> impl IntoElement {
    let (dark_path, light_path) = match kind {
        "otf" => ("drop/otf-dark.svg", "drop/otf-light.svg"),
        _ => ("drop/ttf-dark.svg", "drop/ttf-light.svg"),
    };
    // The inside matches the backdrop; the outline, label and glyph are the
    // brand card grey on dark, a darker grey on light.
    let line: Hsla = if dark { rgb(0xc6c6c6).into() } else { rgb(0x6e6e6e).into() };
    div()
        .absolute()
        .w(px(CARD_W))
        .h(px(CARD_H))
        .child(card_layer(dark_path, Hsla { a: 1., ..pal.window }, side, hover))
        .child(card_layer(light_path, line, side, hover))
}

/// The overlay for the content area while files hover over it. `hover`
/// changes with every new drag, so the fan-out plays again each time.
pub fn drop_hover(hover: u64, dark: bool, pal: &Palette) -> impl IntoElement {
    let lime = pal.select;
    let glow = |size: f32, alpha: f32| {
        div()
            .absolute()
            .size(px(size))
            .rounded_full()
            .bg(lime.opacity(alpha))
    };
    let backdrop = Hsla { a: 1., ..pal.window };
    div()
        .id("drop-hover")
        .absolute()
        .inset_0()
        .p(px(6.))
        .bg(backdrop)
        .child(
            div()
                .size_full()
                .rounded(px(14.))
                .border_2()
                .border_dashed()
                .border_color(lime)
                // Solid, so the grid underneath doesn't compete with the cards.
                .bg(backdrop)
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .child(
                    // The cards and the glow behind them, centered.
                    div()
                        .relative()
                        .size(px(240.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(
                            div()
                                .absolute()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(glow(240., 0.05))
                                .child(glow(170., 0.09))
                                .child(glow(110., 0.13))
                                .with_animation(
                                    ElementId::NamedInteger("drop-glow".into(), hover),
                                    Animation::new(FAN).with_easing(ease_out_cubic),
                                    |el, t| el.opacity(t),
                                ),
                        )
                        .child(card("otf", -1., dark, pal, hover))
                        .child(card("ttf", 1., dark, pal, hover)),
                )
                .child(
                    div()
                        .mt(px(-36.))
                        .text_size(px(14.))
                        .text_color(pal.text_dim)
                        .child("Drop to add a font"),
                ),
        )
}

/// The lime flash right after a drop, fading out.
pub fn drop_flash(flash: u64, pal: &Palette) -> impl IntoElement {
    let lime = pal.select;
    div()
        .absolute()
        .inset_0()
        .p(px(6.))
        .child(div().size_full().rounded(px(14.)).border_2().border_color(lime).bg(lime.opacity(0.12)))
        .with_animation(
            ElementId::NamedInteger("drop-flash".into(), flash),
            Animation::new(FLASH).with_easing(|t| t),
            |el, t| el.opacity(1. - t),
        )
}
