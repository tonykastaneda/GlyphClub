//! The About overlay — an in-app modal (same dimmed-backdrop,
//! click-outside-to-dismiss pattern as `ui::settings`, rather than macOS's
//! native About panel) showing the real icon+wordmark lockup, the running
//! version, a thank-you note, and a Close button. Opened from the macOS app
//! menu's "About GlyphClub" (`native_menu.rs`) or the Windows hand-rolled
//! Help menu (`ui::draw_windows_menu_bar`).

use vello::kurbo::Rect;
use vello::Scene;

use super::{fill_rect, stroke_rect, HitAction, HitRegion};
use crate::app::App;
use crate::text::TextCx;
use crate::theme;

pub fn draw(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    let panel_w = 320.0_f64.min(width - 40.0);
    let panel_h = 300.0_f64.min(height - 40.0);
    let x0 = (width - panel_w) / 2.0;
    let y0 = (height - panel_h) / 2.0;
    let panel = Rect::new(x0, y0, x0 + panel_w, y0 + panel_h);

    // Full-window backdrop, dimmed — also the click-outside-to-dismiss
    // target, pushed before the panel's own hit regions (see
    // `ui::settings::draw`'s identical comment for why the ordering matters).
    let backdrop = Rect::new(0.0, 0.0, width, height);
    fill_rect(scene, backdrop, vello::peniko::Color::from_rgba8(0x00, 0x00, 0x00, 0x66), 0.0);
    app.hit_regions.push(HitRegion {
        rect: backdrop,
        action: HitAction::CloseAbout,
    });

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 16.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 16.0, 1.0);
    // The panel's own blank space absorbs clicks — otherwise they fell
    // through to the backdrop and closed the panel.
    app.hit_regions.push(HitRegion {
        rect: panel,
        action: HitAction::Inert,
    });

    let cx = panel.x0 + panel_w / 2.0;
    let mut y = panel.y0 + 34.0;

    let icon_size = 52.0;
    let icon_rect = Rect::new(cx - icon_size / 2.0, y, cx + icon_size / 2.0, y + icon_size);
    crate::branding::draw_icon(scene, icon_rect, crate::branding::BRAND_LIME);
    y += icon_size + 16.0;

    let wordmark_h = 24.0;
    let wordmark_w = wordmark_h * (190.63 / 22.87);
    let wordmark_rect = Rect::new(cx - wordmark_w / 2.0, y, cx + wordmark_w / 2.0, y + wordmark_h);
    crate::branding::draw_wordmark(scene, wordmark_rect, theme::TEXT());
    y += wordmark_h + 12.0;

    let version = format!("Version {}", env!("CARGO_PKG_VERSION"));
    let vw = text.measure(&version, 12.0, None);
    text.draw(scene, &version, 12.0, None, theme::TEXT_SECONDARY(), cx - vw / 2.0, y);
    y += 34.0;

    let thanks = "Thanks for entrusting us with your largest collection of things.";
    draw_wrapped_centered(text, scene, thanks, cx, y, panel_w - 64.0, theme::TEXT_SECONDARY());

    let close_w = 108.0;
    let close_h = 34.0;
    let close_rect = Rect::new(cx - close_w / 2.0, panel.y1 - 24.0 - close_h, cx + close_w / 2.0, panel.y1 - 24.0);
    let close_hovered = close_rect.contains(app.hover);
    fill_rect(
        scene,
        close_rect,
        if close_hovered { theme::CONTROL_HOVER_SUBTLE() } else { theme::CONTROL_BG() },
        close_h / 2.0,
    );
    stroke_rect(scene, close_rect, theme::CONTROL_BORDER(), close_h / 2.0, 1.0);
    let label = "Close";
    let tw = text.measure(label, 13.0, None);
    text.draw_centered_v(
        scene,
        label,
        13.0,
        None,
        theme::TEXT(),
        close_rect.x0 + (close_w - tw) / 2.0,
        close_rect.y0,
        close_rect.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: close_rect,
        action: HitAction::CloseAbout,
    });
}

/// Word-wraps `body` to `max_w`, centering each resulting line under `cx` —
/// distinct from `ui::draw_wrapped_line`'s left-aligned wrap, since a short
/// centered note reads better than a left-hung paragraph inside a modal
/// this narrow. Returns the y just past the last line drawn.
fn draw_wrapped_centered(
    text: &mut TextCx,
    scene: &mut Scene,
    body: &str,
    cx: f64,
    y0: f64,
    max_w: f64,
    color: vello::peniko::Color,
) -> f64 {
    let size = 13.0;
    let line_h = 19.0;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in body.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text.measure(&candidate, size, None) > max_w && !line.is_empty() {
            lines.push(line);
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }

    let mut y = y0;
    for line in lines {
        let w = text.measure(&line, size, None);
        text.draw(scene, &line, size, None, color, cx - w / 2.0, y);
        y += line_h;
    }
    y
}
