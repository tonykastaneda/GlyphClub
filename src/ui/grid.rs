use vello::kurbo::{Affine, Circle, Point, Rect, Stroke};
use vello::peniko::Fill;
use vello::Scene;

use super::{fill_rect, stroke_rect, HitAction, HitRegion};
use crate::app::App;
use crate::text::TextCx;
use crate::theme;

/// How many columns fit at `min_tile_w`, and the *actual* per-tile width to
/// render at — stretched evenly to fill the row exactly (CSS grid's `1fr`
/// columns), so there's never a dead strip of empty canvas on the right
/// just because the row width didn't divide evenly by the nominal size.
/// Width only: tile *height* is governed separately, purely by the zoom
/// control — see `draw_grid`. Widening a tile to fit a longer sample
/// string used to inflate its height and font size right along with it
/// (both were derived from this same width), which just wasted vertical
/// space on tiles that only ever needed more room sideways.
fn columns_and_tile_w(min_tile_w: f64, content_w: f64) -> (usize, f64) {
    let avail = (content_w - theme::GRID_PAD * 2.0).max(1.0);
    let cell = min_tile_w + theme::GRID_GAP;
    let columns = (((avail + theme::GRID_GAP) / cell).floor() as usize).max(1);
    let tile_w = ((avail - theme::GRID_GAP * (columns as f64 - 1.0)) / columns as f64).max(min_tile_w);
    (columns, tile_w)
}

pub fn draw_grid(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    x0: f64,
    x1: f64,
    y0: f64,
    height: f64,
) {
    let content_w = x1 - x0;
    let viewport_h = (height - y0).max(0.0);

    // Height and font size come from the zoom control alone — never from
    // however wide a tile ends up, so a long sample string only ever
    // grows tiles sideways.
    let preview_h = app.tile_size * 0.70;
    let sample_size = ((preview_h * 0.68) as f32).clamp(36.0, 150.0);

    // A custom/longer sample string (the picker now lets someone type
    // anything, e.g. "lorem") needs wider tiles or it just clips — so the
    // minimum width columns are computed from is whichever is bigger: the
    // zoom control's own width, or what this string actually needs at the
    // font size it's actually drawn at, measured directly rather than
    // guessed.
    let sample_w = text.measure(app.effective_sample_text(), sample_size, None);
    let min_tile_w = app.tile_size.max(sample_w as f64 + 28.0);
    let (columns, tile_w) = columns_and_tile_w(min_tile_w, content_w);
    let row_h = preview_h + theme::GRID_GAP;

    let ids: Vec<i64> = app.visible_entries().iter().map(|e| e.id).collect();
    let total = ids.len();

    if total == 0 {
        draw_empty_state(app, text, scene, x0, x1, y0, height);
        return;
    }

    let total_rows = total.div_ceil(columns);
    let max_scroll = (total_rows as f64 * row_h - viewport_h).max(0.0);
    app.scroll_y = app.scroll_y.clamp(0.0, max_scroll);

    let start_row = (app.scroll_y / row_h).floor().max(0.0) as usize;
    let visible_rows = (viewport_h / row_h).ceil() as usize + 2;
    let end_row = (start_row + visible_rows).min(total_rows);

    scene.push_clip_layer(
        Fill::NonZero,
        Affine::IDENTITY,
        &Rect::new(x0, y0, x1, height),
    );

    for row in start_row..end_row {
        let row_top = y0 + theme::GRID_PAD + row as f64 * row_h - app.scroll_y;
        for col in 0..columns {
            let index = row * columns + col;
            if index >= total {
                break;
            }
            let id = ids[index];
            let tx = x0 + theme::GRID_PAD + col as f64 * (tile_w + theme::GRID_GAP);
            draw_tile(app, text, scene, id, tx, row_top, tile_w, preview_h);
        }
    }

    scene.pop_layer();
    draw_scrollbar(
        app,
        scene,
        x1,
        y0,
        height,
        total_rows as f64 * row_h,
        viewport_h,
    );
}

fn draw_tile(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    id: i64,
    x: f64,
    y: f64,
    size: f64,
    preview_h: f64,
) {
    let tile_rect = Rect::new(x, y, x + size, y + preview_h);
    let selected = app.selected == Some(id);
    let is_system = app
        .entries
        .iter()
        .any(|entry| entry.id == id && entry.is_system);
    let active = app.active_ids.contains(&id) || is_system;

    let border = if selected {
        theme::ACCENT
    } else {
        theme::TILE_BORDER
    };
    fill_rect(scene, tile_rect, theme::TILE_BG, 12.0);
    stroke_rect(
        scene,
        tile_rect,
        border,
        12.0,
        if selected { 2.0 } else { 1.0 },
    );

    let preview_rect = Rect::new(x, y, x + size, y + preview_h);
    let sample = app.effective_sample_text().to_string();
    let is_unknown = app
        .entries
        .iter()
        .find(|entry| entry.id == id)
        .is_some_and(|entry| entry.family.eq_ignore_ascii_case("unknown"));
    let family = if is_unknown {
        None
    } else {
        app.preview_family(text, id)
    };
    // Every tile in the grid shares this one ground line regardless of
    // which font it's previewing — different fonts have different
    // ascent-to-size ratios, so lining them up by baseline (not by the
    // top of each one's own layout box) is what keeps a row of tiles
    // reading as one aligned specimen sheet instead of a ragged jumble.
    let sample_size = (preview_h * 0.68).clamp(36.0, 150.0);
    let sample_x = preview_rect.x0 + 14.0;
    let baseline_y = preview_rect.y0 + preview_h * 0.80;
    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &preview_rect);
    text.draw_at_baseline(
        scene,
        &sample,
        sample_size as f32,
        family.as_deref(),
        theme::TEXT,
        sample_x,
        baseline_y,
    );
    scene.pop_layer();

    let entry_label = app
        .entries
        .iter()
        .find(|e| e.id == id)
        .map(|e| (e.family.clone(), e.subfamily.clone()));
    if let Some((fam, sub)) = entry_label {
        // Centered against the same [y+10, y+28] band the activation pip
        // below is centered in, so the two don't drift apart the way a
        // fixed top-origin baseline would.
        text.draw_centered_v(scene, &fam, 12.0, None, theme::TEXT, x + 32.0, y + 10.0, y + 28.0);
        let _ = sub;
    }

    // Activation dot, top-left.
    let dot_rect = Rect::new(x + 12.0, y + 12.0, x + 26.0, y + 26.0);
    if is_system {
        draw_system_lock(scene, dot_rect.center(), 1.0);
    } else {
        let dot_color = if active {
            theme::GREEN
        } else {
            theme::DOT_INACTIVE
        };
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            dot_color,
            None,
            &Circle::new(dot_rect.center(), 4.5),
        );
        app.hit_regions.push(HitRegion {
            rect: dot_rect,
            action: HitAction::ToggleActivation(id),
        });
    }

    // Whole-tile click selects it (added last among this tile's regions but
    // before later tiles, so the dot/star rects above still win — they're
    // checked in reverse draw order and are smaller and drawn after).
    app.hit_regions.push(HitRegion {
        rect: tile_rect,
        action: HitAction::SelectFont(id),
    });
}

/// A thin draggable thumb on the content area's right edge — hidden
/// entirely when everything already fits without scrolling.
fn draw_scrollbar(
    app: &mut App,
    scene: &mut Scene,
    x1: f64,
    y0: f64,
    y1: f64,
    content_h: f64,
    viewport_h: f64,
) {
    if content_h <= viewport_h {
        app.scroll_track = None;
        return;
    }

    let track_x0 = x1 - 9.0;
    let track_x1 = x1 - 3.0;
    let track_y0 = y0 + 4.0;
    let track_y1 = (y1 - 4.0).max(track_y0);
    let track_h = (track_y1 - track_y0).max(1.0);

    let thumb_h = (viewport_h / content_h * track_h).clamp(28.0, track_h);
    let max_scroll = (content_h - viewport_h).max(0.0);
    let progress = if max_scroll > 0.0 {
        (app.scroll_y / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_y0 = track_y0 + (track_h - thumb_h) * progress;
    let thumb_rect = Rect::new(track_x0, thumb_y0, track_x1, thumb_y0 + thumb_h);

    let grab_rect = Rect::new(track_x0 - 3.0, y0, track_x1 + 3.0, y1);
    let color = if app.dragging_scrollbar || grab_rect.contains(app.hover) {
        theme::TEXT_SECONDARY
    } else {
        theme::TEXT_TERTIARY
    };
    fill_rect(scene, thumb_rect, color, (track_x1 - track_x0) / 2.0);

    app.scroll_track = Some((track_y0, track_y1, content_h, viewport_h));
    app.hit_regions.push(HitRegion {
        rect: grab_rect,
        action: HitAction::StartScrollDrag,
    });
}

fn draw_empty_state(
    _app: &App,
    text: &mut TextCx,
    scene: &mut Scene,
    x0: f64,
    x1: f64,
    y0: f64,
    height: f64,
) {
    let cx = (x0 + x1) / 2.0;
    let cy = y0 + (height - y0) / 2.0;
    let msg = "No fonts to show";
    let tw = text.measure(msg, 15.0, None);
    text.draw(
        scene,
        msg,
        15.0,
        None,
        theme::TEXT_SECONDARY,
        cx - tw / 2.0,
        cy,
    );
}

/// A system font is already available to the OS, so it uses a fixed lock
/// instead of the user-toggleable activation pip.
fn draw_system_lock(scene: &mut Scene, center: Point, scale: f64) {
    let shackle = Rect::new(
        center.x - 3.2 * scale,
        center.y - 5.5 * scale,
        center.x + 3.2 * scale,
        center.y + 1.2 * scale,
    );
    scene.stroke(
        &Stroke::new(1.3 * scale),
        Affine::IDENTITY,
        theme::GREEN,
        None,
        &shackle.to_rounded_rect(4.0 * scale),
    );
    let body = Rect::new(
        center.x - 4.3 * scale,
        center.y - 0.5 * scale,
        center.x + 4.3 * scale,
        center.y + 4.8 * scale,
    );
    fill_rect(scene, body, theme::GREEN, 2.0 * scale);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::TILE_BG,
        None,
        &Circle::new(Point::new(center.x, center.y + 2.0 * scale), 0.8 * scale),
    );
}

pub fn draw_list(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    x0: f64,
    x1: f64,
    y0: f64,
    height: f64,
) {
    let viewport_h = (height - y0).max(0.0);
    let ids: Vec<i64> = app.visible_entries().iter().map(|e| e.id).collect();
    let total = ids.len();

    if total == 0 {
        draw_empty_state(app, text, scene, x0, x1, y0, height);
        return;
    }

    // Same zoom control as the grid: row height (and everything in it)
    // scales with `tile_size` relative to its default, rather than list
    // view ignoring it and staying a fixed height.
    let row_h = theme::ROW_H * (app.tile_size / theme::DEFAULT_TILE);

    let max_scroll = (total as f64 * row_h - viewport_h).max(0.0);
    app.scroll_y = app.scroll_y.clamp(0.0, max_scroll);

    let start = (app.scroll_y / row_h).floor().max(0.0) as usize;
    let visible = (viewport_h / row_h).ceil() as usize + 2;
    let end = (start + visible).min(total);

    scene.push_clip_layer(
        Fill::NonZero,
        Affine::IDENTITY,
        &Rect::new(x0, y0, x1, height),
    );

    for i in start..end {
        let id = ids[i];
        let row_y = y0 + i as f64 * row_h - app.scroll_y;
        draw_list_row(app, text, scene, id, x0, x1, row_y, row_h);
    }

    scene.pop_layer();
    draw_scrollbar(app, scene, x1, y0, height, total as f64 * row_h, viewport_h);
}

fn draw_list_row(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    id: i64,
    x0: f64,
    x1: f64,
    y: f64,
    row_h: f64,
) {
    let s = row_h / theme::ROW_H;
    let rect = Rect::new(x0, y, x1, y + row_h);
    let selected = app.selected == Some(id);
    let is_system = app
        .entries
        .iter()
        .any(|entry| entry.id == id && entry.is_system);
    let active = app.active_ids.contains(&id) || is_system;
    let favorite = app.favorite_ids.contains(&id);

    if selected {
        fill_rect(scene, rect, theme::ACCENT, 0.0);
    } else if rect.contains(app.hover) {
        fill_rect(scene, rect, theme::CONTROL_BG, 0.0);
    }

    let dot_center = vello::kurbo::Point::new(x0 + 26.0 * s, y + row_h / 2.0);
    if is_system {
        draw_system_lock(scene, dot_center, s);
    } else {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if active {
                theme::GREEN
            } else {
                theme::DOT_INACTIVE
            },
            None,
            &Circle::new(dot_center, 3.5 * s),
        );
        app.hit_regions.push(HitRegion {
            rect: Rect::new(
                dot_center.x - 9.0,
                dot_center.y - 9.0,
                dot_center.x + 9.0,
                dot_center.y + 9.0,
            ),
            action: HitAction::ToggleActivation(id),
        });
    }

    let entry_label = app.entries.iter().find(|e| e.id == id).map(|e| {
        (
            e.family.clone(),
            e.subfamily.clone(),
            e.italic,
            e.monospace,
            e.weight,
        )
    });
    if let Some((family_name, subfamily, italic, monospace, weight)) = entry_label {
        let is_unknown = family_name.eq_ignore_ascii_case("unknown");
        let family_font = if is_unknown {
            None
        } else {
            app.preview_family(text, id)
        };
        let color = theme::TEXT;
        // The family name itself renders in the font it names — the same
        // live preview the grid uses — not the UI's own font; only the
        // meta line below it (subfamily/weight) is chrome text.
        let specimen_size = (16.0 * s).clamp(11.0, 34.0);
        text.draw_centered_v(
            scene,
            &family_name,
            specimen_size as f32,
            family_font.as_deref(),
            color,
            x0 + 44.0 * s,
            y + row_h * 0.18,
            y + row_h * 0.68,
        );

        let mut bits = vec![subfamily];
        if italic {
            bits.push("Italic".to_string());
        }
        if monospace {
            bits.push("Mono".to_string());
        }
        let meta = format!("{} \u{00b7} weight {}", bits.join(" \u{00b7} "), weight);
        let meta_color = if selected {
            theme::TEXT_SECONDARY
        } else {
            theme::TEXT_TERTIARY
        };
        text.draw(
            scene,
            &meta,
            (10.5 * s).clamp(9.0, 15.0) as f32,
            None,
            meta_color,
            x0 + 44.0 * s,
            y + row_h * 0.72,
        );
    }

    let star_rect = Rect::new(
        x1 - 34.0,
        y + row_h / 2.0 - 9.0,
        x1 - 16.0,
        y + row_h / 2.0 + 9.0,
    );
    let star_color = if favorite {
        theme::GOLD
    } else {
        theme::DOT_INACTIVE
    };
    text.draw(
        scene,
        "\u{2605}",
        11.0,
        None,
        star_color,
        star_rect.x0 + 2.0,
        star_rect.y0 + 13.0,
    );
    app.hit_regions.push(HitRegion {
        rect: star_rect,
        action: HitAction::ToggleFavorite(id),
    });

    app.hit_regions.push(HitRegion {
        rect,
        action: HitAction::SelectFont(id),
    });
}
