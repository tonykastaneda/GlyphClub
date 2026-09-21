use vello::kurbo::{Affine, Rect};
use vello::peniko::Fill;
use vello::Scene;

use super::{fill_rect, stroke_rect, HitAction, HitRegion};
use crate::app::{App, Focus};
use crate::text::TextCx;
use crate::theme;

pub fn draw(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64, height: f64) {
    let Some(id) = app.selected else { return };

    fill_rect(scene, Rect::new(x0, 0.0, x1, height), theme::PANEL_BG, 0.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::SEPARATOR,
        None,
        &Rect::new(x0, 0.0, x0 + 1.0, height),
    );

    let pad = 22.0;
    let entry = app
        .entries
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.family.clone(), e.subfamily.clone()));
    let Some((family, subfamily)) = entry else {
        app.selected = None;
        return;
    };
    let favorite = app.favorite_ids.contains(&id);

    let mut y = 30.0;

    let close_rect = Rect::new(x1 - pad - 18.0, y - 4.0, x1 - pad, y + 14.0);
    text.draw(
        scene,
        "\u{2715}",
        12.0,
        None,
        theme::TEXT_SECONDARY,
        close_rect.x0 + 3.0,
        y + 5.0,
    );
    app.hit_regions.push(HitRegion {
        rect: close_rect,
        action: HitAction::CloseDetail,
    });

    let star_rect = Rect::new(close_rect.x0 - 28.0, y - 4.0, close_rect.x0 - 8.0, y + 14.0);
    let star_color = if favorite {
        theme::GOLD
    } else {
        theme::TEXT_SECONDARY
    };
    text.draw(
        scene,
        "\u{2605}",
        14.0,
        None,
        star_color,
        star_rect.x0 + 1.0,
        y + 7.0,
    );
    app.hit_regions.push(HitRegion {
        rect: star_rect,
        action: HitAction::ToggleDetailFavorite,
    });

    text.draw(scene, &family, 19.0, None, theme::TEXT, x0 + pad, y + 2.0);
    y += 26.0;
    text.draw(
        scene,
        &subfamily,
        12.0,
        None,
        theme::TEXT_SECONDARY,
        x0 + pad,
        y,
    );
    y += 30.0;

    // Editable preview-text field.
    let field_h = 36.0;
    let field_rect = Rect::new(x0 + pad, y, x1 - pad, y + field_h);
    fill_rect(scene, field_rect, theme::CONTROL_BG, 9.0);
    let is_focused = app.focus == Focus::PreviewText;
    stroke_rect(
        scene,
        field_rect,
        if is_focused {
            theme::CONTROL_FOCUS
        } else {
            theme::CONTROL_BORDER
        },
        9.0,
        if is_focused { 1.5 } else { 1.0 },
    );
    let field_text_y = y + field_h / 2.0 - 5.0;
    let field_clip = Rect::new(
        field_rect.x0 + 9.0,
        field_rect.y0 + 3.0,
        field_rect.x1 - 9.0,
        field_rect.y1 - 3.0,
    );
    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &field_clip);
    text.draw(
        scene,
        &app.preview_text,
        12.5,
        None,
        theme::TEXT,
        field_rect.x0 + 10.0,
        field_text_y,
    );
    if app.focus == Focus::PreviewText {
        let caret_x = field_rect.x0 + 10.0 + text.measure(&app.preview_text, 12.5, None);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::ACCENT,
            None,
            &Rect::new(caret_x + 1.0, y + 6.0, caret_x + 2.0, y + field_h - 6.0),
        );
    }
    scene.pop_layer();
    app.hit_regions.push(HitRegion {
        rect: field_rect,
        action: HitAction::FocusField(Focus::PreviewText),
    });
    y += field_h + 24.0;

    let family_for_preview = app.preview_family(text, id);
    let sample_area = Rect::new(x0 + pad, y, x1 - pad, height - 24.0);
    draw_wrapped_sample(
        text,
        scene,
        &app.preview_text,
        family_for_preview.as_deref(),
        sample_area,
    );
}

/// Crude word-wrap at a fixed size — good enough for a preview pane where
/// exact line-breaking doesn't need to match a real text engine's rules.
fn draw_wrapped_sample(
    text: &mut TextCx,
    scene: &mut Scene,
    content: &str,
    family: Option<&str>,
    area: Rect,
) {
    if content.is_empty() {
        return;
    }
    let size = 30.0f32;
    let line_h = size as f64 * 1.35;
    let max_w = area.width();

    let mut line = String::new();
    let mut y = area.y0 + size as f64;
    for word in content.split_whitespace() {
        let candidate = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if text.measure(&candidate, size, family) > max_w && !line.is_empty() {
            text.draw(scene, &line, size, family, theme::TEXT, area.x0, y);
            y += line_h;
            line = word.to_string();
        } else {
            line = candidate;
        }
        if y > area.y1 {
            return;
        }
    }
    if !line.is_empty() && y <= area.y1 {
        text.draw(scene, &line, size, family, theme::TEXT, area.x0, y);
    }
}
