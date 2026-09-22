use vello::kurbo::{Affine, Circle, Point, Rect, Stroke};
use vello::peniko::Fill;
use vello::Scene;

use super::{draw_format_badge, fill_rect, stroke_rect, truncate_to_width, HitAction, HitRegion};
use crate::app::{App, VisibleIdsCacheKey};
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

    let ids_key = VisibleIdsCacheKey {
        entries_version: app.entries_version,
        favorites_version: app.favorites_version,
        filter: app.filter,
        search: app.search.clone(),
    };
    let ids = match app.visible_ids_cache.take() {
        Some((cached_key, ids)) if cached_key == ids_key => ids,
        _ => app.visible_entries().iter().map(|e| e.id).collect(),
    };
    let total = ids.len();

    if total == 0 {
        app.visible_ids_cache = Some((ids_key, ids));
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
    app.visible_ids_cache = Some((ids_key, ids));
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
    // One lookup, not three — `App::entry` is O(1), but even so, a tile
    // shouldn't hash the same id into `entries_by_id` repeatedly when one
    // lookup already has everything the rest of this function needs.
    let (has_entry, is_system, is_unknown, fam, sub, path) = match app.entry(id) {
        Some(e) => (
            true,
            e.is_system,
            e.family.eq_ignore_ascii_case("unknown"),
            e.family.clone(),
            e.subfamily.clone(),
            e.path.clone(),
        ),
        None => (false, false, true, String::new(), String::new(), String::new()),
    };

    let tile_rect = Rect::new(x, y, x + size, y + preview_h);
    let selected = app.selected == Some(id);
    let active = app.active_ids.contains(&id) || is_system;

    let border = if selected {
        theme::BRAND_ACCENT()
    } else {
        theme::TILE_BORDER()
    };
    fill_rect(scene, tile_rect, theme::TILE_BG(), 12.0);
    stroke_rect(
        scene,
        tile_rect,
        border,
        12.0,
        if selected { 2.0 } else { 1.0 },
    );

    let preview_rect = Rect::new(x, y, x + size, y + preview_h);
    let sample = app.effective_sample_text().to_string();
    // A font that doesn't actually have glyphs for the sample text (common
    // once System Fonts is in the mix — plenty of OS-bundled fonts only
    // cover one script) would otherwise silently render in whatever
    // fallback font the text engine substitutes, at full brightness, as if
    // it were genuinely supported. Falling back to the system font *and*
    // dimming the color is what actually signals "this one doesn't have
    // it" — matching the greyed-out look real font managers use for this.
    let covers_sample = !is_unknown && app.covers_sample(id);
    let family = if is_unknown || !covers_sample {
        None
    } else {
        app.preview_family(text, id)
    };
    let sample_color = if covers_sample { theme::TEXT() } else { theme::TEXT_TERTIARY() };
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
        sample_color,
        sample_x,
        baseline_y,
    );
    scene.pop_layer();

    if has_entry {
        // Centered against the same [y+10, y+28] band the activation pip
        // below is centered in, so the two don't drift apart the way a
        // fixed top-origin baseline would.
        let badge_w = draw_format_badge(
            scene,
            crate::font::format_label(&path),
            x + 32.0,
            y + 10.0,
            y + 28.0,
            theme::TEXT_TERTIARY(),
        );
        let name_x = x + 32.0 + if badge_w > 0.0 { badge_w + 6.0 } else { 0.0 };
        let name_max_w = (x + size - 12.0 - name_x).max(0.0);
        let fam = truncate_to_width(text, &fam, 12.0, None, name_max_w);
        text.draw_centered_v(scene, &fam, 12.0, None, theme::TEXT(), name_x, y + 10.0, y + 28.0);
        let _ = sub;
    }

    // Activation dot, top-left.
    let dot_rect = Rect::new(x + 12.0, y + 12.0, x + 26.0, y + 26.0);
    if is_system {
        draw_system_lock(scene, dot_rect.center(), 1.0);
    } else {
        let dot_color = if active {
            theme::GREEN()
        } else {
            theme::DOT_INACTIVE()
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
pub(super) fn draw_scrollbar(
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
        theme::TEXT_SECONDARY()
    } else {
        theme::TEXT_TERTIARY()
    };
    fill_rect(scene, thumb_rect, color, (track_x1 - track_x0) / 2.0);

    app.scroll_track = Some((track_y0, track_y1, content_h, viewport_h));
    app.hit_regions.push(HitRegion {
        rect: grab_rect,
        action: HitAction::StartScrollDrag,
    });
}

pub(super) fn draw_empty_state(
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

    let mark_size = 64.0;
    let mark_rect = Rect::new(
        cx - mark_size / 2.0,
        cy - mark_size - 14.0,
        cx + mark_size / 2.0,
        cy - 14.0,
    );
    crate::branding::draw_icon(scene, mark_rect, theme::TEXT_TERTIARY());

    let msg = "No fonts to show";
    let tw = text.measure(msg, 15.0, None);
    text.draw(
        scene,
        msg,
        15.0,
        None,
        theme::TEXT_SECONDARY(),
        cx - tw / 2.0,
        cy,
    );
}

/// A system font is already available to the OS, so it uses a fixed lock
/// instead of the user-toggleable activation pip — sized to the same ~9px
/// footprint as that pip's dot (`Circle::new(_, 4.5)`), not its own larger
/// natural proportions, so a row/tile of mixed system and user fonts reads
/// as one consistent-size indicator column rather than the lock visually
/// outsizing the dots next to it.
pub(super) fn draw_system_lock(scene: &mut Scene, center: Point, scale: f64) {
    let scale = scale * 0.87;
    let shackle = Rect::new(
        center.x - 3.2 * scale,
        center.y - 5.5 * scale,
        center.x + 3.2 * scale,
        center.y + 1.2 * scale,
    );
    scene.stroke(
        &Stroke::new(1.3 * scale),
        Affine::IDENTITY,
        theme::GREEN(),
        None,
        &shackle.to_rounded_rect(4.0 * scale),
    );
    let body = Rect::new(
        center.x - 4.3 * scale,
        center.y - 0.5 * scale,
        center.x + 4.3 * scale,
        center.y + 4.8 * scale,
    );
    fill_rect(scene, body, theme::GREEN(), 2.0 * scale);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::TILE_BG(),
        None,
        &Circle::new(Point::new(center.x, center.y + 2.0 * scale), 0.8 * scale),
    );
}
