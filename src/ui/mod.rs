mod detail;
mod grid;
mod sidebar;

use vello::kurbo::{Affine, Arc, BezPath, Circle, Line, Point, Rect, Stroke, Vec2};
use vello::peniko::Fill;
use vello::Scene;

use crate::app::{App, Filter, Focus, ViewMode};
use crate::text::TextCx;
use crate::theme;

pub enum HitAction {
    SelectNav(Filter),
    RemoveFolder(i64),
    RevealFolder(i64),
    AddFolder,
    Rescan,
    SetViewMode(ViewMode),
    FocusField(Focus),
    SelectFont(i64),
    ToggleActivation(i64),
    ToggleFavorite(i64),
    CloseDetail,
    ToggleDetailFavorite,
    ToggleDetailPanel,
    StartTileSizeDrag,
    StartScrollDrag,
    ToggleSamplePicker,
    SetSampleText(String),
    ClearSampleText,
}

pub struct HitRegion {
    pub rect: Rect,
    pub action: HitAction,
}

/// A library row's right-click menu — "Remove from list" and "Show in
/// Finder/Explorer" only reachable this way, never a stray click, since a
/// bare `x` on every row was too easy to fat-finger.
pub struct ContextMenu {
    pub folder_id: i64,
    pub anchor: Point,
    /// Filled in the next time this menu is drawn; `handle_click` uses it
    /// to tell a click on the menu apart from a click that should dismiss
    /// it — so it starts as an empty rect (nothing is "inside" it) until
    /// then.
    pub panel_rect: Rect,
}

pub fn fill_rect(scene: &mut Scene, rect: Rect, color: vello::peniko::Color, radius: f64) {
    if radius > 0.0 {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            color,
            None,
            &rect.to_rounded_rect(radius),
        );
    } else {
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rect);
    }
}

pub fn stroke_rect(
    scene: &mut Scene,
    rect: Rect,
    color: vello::peniko::Color,
    radius: f64,
    width: f64,
) {
    let shape = rect
        .inset(width / 2.0)
        .to_rounded_rect((radius - width / 2.0).max(0.0));
    scene.stroke(&Stroke::new(width), Affine::IDENTITY, color, None, &shape);
}

/// A circular-arrow refresh glyph — replaces what used to be a bare "R"
/// text glyph on the rescan button, which didn't read as a button at all.
fn draw_refresh_icon(scene: &mut Scene, center: Point, color: vello::peniko::Color) {
    let radius = 6.0;
    let start_angle = -2.0;
    let sweep = 5.0;
    let arc = Arc::new(center, Vec2::new(radius, radius), start_angle, sweep, 0.0);
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, color, None, &arc);

    // A small filled arrowhead at the arc's end, tangent to the circle
    // there, so it reads as "this arrow is chasing its own tail" rather
    // than a stray triangle.
    let end_angle = start_angle + sweep;
    let p = center + Vec2::new(end_angle.cos(), end_angle.sin()) * radius;
    let tangent = Vec2::new(-end_angle.sin(), end_angle.cos());
    let normal = Vec2::new(end_angle.cos(), end_angle.sin());
    let tip = p + tangent * 2.6;
    let left = p - tangent * 1.6 + normal * 2.0;
    let right = p - tangent * 1.6 - normal * 2.0;
    let mut arrowhead = BezPath::new();
    arrowhead.move_to(tip);
    arrowhead.line_to(left);
    arrowhead.line_to(right);
    arrowhead.close_path();
    scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &arrowhead);
}

pub fn draw_search_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    scene.stroke(
        &Stroke::new(1.6),
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(Point::new(x + 6.0, y + 6.0), 4.2),
    );
    scene.stroke(
        &Stroke::new(1.6),
        Affine::IDENTITY,
        color,
        None,
        &Line::new((x + 9.2, y + 9.2), (x + 13.0, y + 13.0)),
    );
}

fn draw_grid_icon(scene: &mut Scene, cx: f64, cy: f64, color: vello::peniko::Color) {
    let sq = 5.4;
    let gap = 2.2;
    for dx in [-1.0, 1.0] {
        for dy in [-1.0, 1.0] {
            let x0 = cx + dx * (sq + gap) / 2.0 - sq / 2.0;
            let y0 = cy + dy * (sq + gap) / 2.0 - sq / 2.0;
            fill_rect(scene, Rect::new(x0, y0, x0 + sq, y0 + sq), color, 1.3);
        }
    }
}

fn draw_list_icon(scene: &mut Scene, cx: f64, cy: f64, color: vello::peniko::Color) {
    let w = 13.0;
    let h = 1.8;
    let gap = 3.6;
    for i in -1..=1 {
        let y = cy + i as f64 * gap;
        fill_rect(
            scene,
            Rect::new(cx - w / 2.0, y - h / 2.0, cx + w / 2.0, y + h / 2.0),
            color,
            h / 2.0,
        );
    }
}

/// Just the "i" glyph — the circular outline comes from the button
/// container itself now, not this icon, so it isn't drawn twice.
fn draw_info_icon(
    scene: &mut Scene,
    text: &mut TextCx,
    cx: f64,
    cy: f64,
    color: vello::peniko::Color,
) {
    let w = text.measure("i", 11.0, None);
    text.draw_centered_v(
        scene,
        "i",
        11.0,
        None,
        color,
        cx - w / 2.0,
        cy - 7.0,
        cy + 7.0,
    );
}

/// Placeholder icon for the not-yet-wired user-preview-text toggle: a
/// small text field with a caret, since that's what the button will
/// eventually open.
fn draw_preview_icon(scene: &mut Scene, cx: f64, cy: f64, color: vello::peniko::Color) {
    let field = Rect::new(cx - 7.5, cy - 5.5, cx + 7.5, cy + 5.5);
    stroke_rect(scene, field, color, 2.5, 1.3);
    scene.stroke(
        &Stroke::new(1.3),
        Affine::IDENTITY,
        color,
        None,
        &Line::new((cx - 2.5, cy - 2.4), (cx - 2.5, cy + 2.4)),
    );
}

/// x-extent of the main content area (grid/list), after the sidebar and
/// the detail panel (when a font is selected) claim their strips.
fn content_x_range(width: f64, detail_open: bool) -> (f64, f64) {
    let x0 = theme::SIDEBAR_W + theme::FRAME_PAD;
    let x1 = if detail_open {
        width - theme::DETAIL_W
    } else {
        width
    };
    (x0, x1.max(x0))
}

pub fn draw(app: &mut App, text: &mut TextCx, width: f64, height: f64) -> Scene {
    app.hit_regions.clear();
    app.tile_slider_track = None;
    app.folder_rows.clear();
    let mut scene = Scene::new();

    fill_rect(
        &mut scene,
        Rect::new(0.0, 0.0, width, height),
        theme::CANVAS,
        0.0,
    );

    sidebar::draw(app, text, &mut scene, height);

    let detail_open = app.detail_open && app.selected.is_some();
    let (content_x0, content_x1) = content_x_range(width, detail_open);

    draw_toolbar(app, text, &mut scene, content_x0, content_x1);

    let content_y0 = theme::TOOLBAR_H;
    match app.view_mode {
        ViewMode::Grid => grid::draw_grid(
            app, text, &mut scene, content_x0, content_x1, content_y0, height,
        ),
        ViewMode::List => grid::draw_list(
            app, text, &mut scene, content_x0, content_x1, content_y0, height,
        ),
    }

    if detail_open {
        detail::draw(
            app,
            text,
            &mut scene,
            width - theme::DETAIL_W,
            width,
            height,
        );
    }

    if app.sample_picker_open {
        draw_sample_picker(app, text, &mut scene, width, height);
    }

    draw_context_menu(app, text, &mut scene);

    scene
}

/// A card anchored to the bottom-center of the window, opened from the
/// toolbar's preview button: an editable field holding `app.sample_text`
/// (what every grid/list tile renders its live preview in, distinct from
/// the detail panel's own `preview_text`), plus quick-fill preset chips.
fn draw_sample_picker(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    let card_w = 340.0;
    let card_h = 100.0;
    let x0 = (width - card_w) / 2.0;
    let y0 = height - 24.0 - card_h;
    let card = Rect::new(x0, y0, x0 + card_w, y0 + card_h);

    fill_rect(scene, card, theme::SIDEBAR_BG, 18.0);
    stroke_rect(scene, card, theme::CONTROL_BORDER, 18.0, 1.0);

    // The sample text itself — an editable field, not just a display, so
    // typing here directly changes what every tile previews live.
    let field_rect = Rect::new(card.x0 + 16.0, card.y0 + 10.0, card.x1 - 40.0, card.y0 + 46.0);
    if app.focus == Focus::SampleText {
        stroke_rect(scene, field_rect.inset(4.0), theme::CONTROL_FOCUS, 8.0, 1.5);
    }
    let is_placeholder = app.sample_text.trim().is_empty();
    let display_text = app.effective_sample_text().to_string();
    text.draw_centered_v(
        scene,
        &display_text,
        26.0,
        None,
        if is_placeholder { theme::TEXT_TERTIARY } else { theme::TEXT },
        field_rect.x0,
        field_rect.y0,
        field_rect.y1,
    );
    if app.focus == Focus::SampleText {
        let caret_x = field_rect.x0 + text.measure(&display_text, 26.0, None) + 3.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::ACCENT,
            None,
            &Rect::new(caret_x, field_rect.y0 + 3.0, caret_x + 2.0, field_rect.y1 - 3.0),
        );
    }
    app.hit_regions.push(HitRegion {
        rect: field_rect,
        action: HitAction::FocusField(Focus::SampleText),
    });

    // This "×" clears the field back to the "Aa" fallback — it does not
    // close the card. The only way to close it is the toolbar's preview
    // button (the one that opened it), so an accidental click here can't
    // lose the card entirely, just its current text.
    let close_rect = Rect::new(card.x1 - 34.0, card.y0 + 10.0, card.x1 - 10.0, card.y0 + 34.0);
    let close_hovered = close_rect.contains(app.hover);
    scene.stroke(
        &Stroke::new(1.4),
        Affine::IDENTITY,
        if close_hovered { theme::TEXT } else { theme::TEXT_TERTIARY },
        None,
        &Circle::new(close_rect.center(), 9.0),
    );
    let cx0 = close_rect.center();
    let mut cross = BezPath::new();
    cross.move_to((cx0.x - 3.0, cx0.y - 3.0));
    cross.line_to((cx0.x + 3.0, cx0.y + 3.0));
    cross.move_to((cx0.x - 3.0, cx0.y + 3.0));
    cross.line_to((cx0.x + 3.0, cx0.y - 3.0));
    scene.stroke(
        &Stroke::new(1.4),
        Affine::IDENTITY,
        if close_hovered { theme::TEXT } else { theme::TEXT_TERTIARY },
        None,
        &cross,
    );
    app.hit_regions.push(HitRegion {
        rect: close_rect,
        action: HitAction::ClearSampleText,
    });

    // Quick-fill preset chips.
    let presets = ["123", "ABC", "abc", "lorem"];
    let chip_h = 26.0;
    let chip_y = card.y0 + 58.0;
    let mut cx = card.x0 + 16.0;
    for label in presets {
        let w = text.measure(label, 12.5, None) + 22.0;
        let r = Rect::new(cx, chip_y, cx + w, chip_y + chip_h);
        let active = app.sample_text == label;
        let hovered = r.contains(app.hover);
        let bg = if active {
            theme::NAV_SELECTED_BG
        } else if hovered {
            theme::CONTROL_HOVER_SUBTLE
        } else {
            theme::CONTROL_BG
        };
        fill_rect(scene, r, bg, chip_h / 2.0);
        let tw = text.measure(label, 12.5, None);
        text.draw_centered_v(
            scene,
            label,
            12.5,
            None,
            if active { theme::TEXT } else { theme::TEXT_SECONDARY },
            r.x0 + (w - tw) / 2.0,
            r.y0,
            r.y1,
        );
        app.hit_regions.push(HitRegion {
            rect: r,
            action: HitAction::SetSampleText(label.to_string()),
        });
        cx += w + 8.0;
    }
}

fn draw_context_menu(app: &mut App, text: &mut TextCx, scene: &mut Scene) {
    let Some(menu) = &app.context_menu else {
        return;
    };
    let anchor = menu.anchor;
    let folder_id = menu.folder_id;

    let w = 176.0;
    let item_h = 30.0;
    let items = [
        ("Remove from list", HitAction::RemoveFolder(folder_id)),
        (crate::reveal::LABEL, HitAction::RevealFolder(folder_id)),
    ];
    let panel = Rect::new(
        anchor.x,
        anchor.y,
        anchor.x + w,
        anchor.y + item_h * items.len() as f64 + 8.0,
    );

    fill_rect(scene, panel, theme::SIDEBAR_BG, 8.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER, 8.0, 1.0);

    for (i, (label, action)) in items.into_iter().enumerate() {
        let item_rect = Rect::new(
            panel.x0 + 4.0,
            panel.y0 + 4.0 + item_h * i as f64,
            panel.x1 - 4.0,
            panel.y0 + 4.0 + item_h * (i as f64 + 1.0),
        );
        text.draw(
            scene,
            label,
            12.5,
            None,
            theme::TEXT,
            item_rect.x0 + 10.0,
            item_rect.y0 + 19.0,
        );
        app.hit_regions.push(HitRegion {
            rect: item_rect,
            action,
        });
    }

    if let Some(m) = app.context_menu.as_mut() {
        m.panel_rect = panel;
    }
}

fn draw_toolbar(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64) {
    fill_rect(
        scene,
        Rect::new(x0, 0.0, x1, theme::TOOLBAR_H),
        theme::TOOLBAR_BG,
        0.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::SEPARATOR,
        None,
        &Rect::new(x0, theme::TOOLBAR_H - 1.0, x1, theme::TOOLBAR_H),
    );

    let pad = 16.0;
    let mut cx = x0 + pad;
    let ctrl_h = 32.0;
    let ctrl_y = (theme::TOOLBAR_H - ctrl_h) / 2.0;

    // Rescan.
    let rescan_rect = Rect::new(cx, ctrl_y, cx + ctrl_h, ctrl_y + ctrl_h);
    let rescan_bg = if rescan_rect.contains(app.hover) {
        theme::CONTROL_HOVER_SUBTLE
    } else {
        theme::CONTROL_BG
    };
    fill_rect(scene, rescan_rect, rescan_bg, ctrl_h / 2.0);
    stroke_rect(scene, rescan_rect, theme::CONTROL_BORDER, ctrl_h / 2.0, 1.0);
    draw_refresh_icon(scene, rescan_rect.center(), theme::TEXT_SECONDARY);
    app.hit_regions.push(HitRegion {
        rect: rescan_rect,
        action: HitAction::Rescan,
    });
    cx += ctrl_h + 16.0;

    // Tile/row size: compact macOS-style type-size slider. Applies in both
    // Grid (tile dimensions) and List (row height + text size) modes.
    {
        let slider_w = 116.0;
        let slider_rect = Rect::new(cx, ctrl_y - 2.0, cx + slider_w, ctrl_y + ctrl_h + 2.0);
        fill_rect(scene, slider_rect, theme::CONTROL_BG, 18.0);
        stroke_rect(scene, slider_rect, theme::CONTROL_BORDER, 18.0, 1.0);

        text.draw_centered_v(
            scene,
            "A",
            10.0,
            None,
            theme::TEXT_TERTIARY,
            cx + 10.0,
            ctrl_y,
            ctrl_y + ctrl_h,
        );
        text.draw_centered_v(
            scene,
            "A",
            14.0,
            None,
            theme::TEXT_SECONDARY,
            cx + slider_w - 20.0,
            ctrl_y,
            ctrl_y + ctrl_h,
        );

        let track_x0 = cx + 31.0;
        let track_x1 = cx + slider_w - 31.0;
        let track_y = ctrl_y + ctrl_h / 2.0;
        let progress = ((app.tile_size - theme::MIN_TILE) / (theme::MAX_TILE - theme::MIN_TILE))
            .clamp(0.0, 1.0);
        scene.stroke(
            &Stroke::new(3.0),
            Affine::IDENTITY,
            theme::TEXT_TERTIARY,
            None,
            &Line::new((track_x0, track_y), (track_x1, track_y)),
        );
        let thumb_x = track_x0 + (track_x1 - track_x0) * progress;
        app.tile_slider_track = Some((track_x0, track_x1));
        scene.stroke(
            &Stroke::new(3.0),
            Affine::IDENTITY,
            theme::ACCENT,
            None,
            &Line::new((track_x0, track_y), (thumb_x, track_y)),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::TEXT,
            None,
            &Circle::new(Point::new(thumb_x, track_y), 6.0),
        );
        stroke_rect(
            scene,
            Rect::new(thumb_x - 6.0, track_y - 6.0, thumb_x + 6.0, track_y + 6.0),
            theme::CONTROL_BORDER,
            6.0,
            1.0,
        );

        app.hit_regions.push(HitRegion {
            rect: slider_rect,
            action: HitAction::StartTileSizeDrag,
        });
        cx += slider_w + 14.0;
    }

    let _ = cx;

    // Search field, right-aligned.
    let search_w = 236.0;
    let search_rect = Rect::new(x1 - pad - search_w, ctrl_y, x1 - pad, ctrl_y + ctrl_h);

    // Preview · Grid/List · Info — laid out as their own right-aligned
    // group immediately to the left of the search field, not tacked onto
    // the rescan/slider cluster on the far left.
    let seg_w = ctrl_h;
    let cluster_w = ctrl_h + 12.0 + seg_w * 2.0 + 12.0 + ctrl_h;
    let mut ccx = search_rect.x0 - pad - cluster_w;

    // Preview toggle — opens the sample-text picker card (`draw_sample_picker`).
    let preview_rect = Rect::new(ccx, ctrl_y, ccx + ctrl_h, ctrl_y + ctrl_h);
    let preview_active = app.sample_picker_open;
    if preview_active {
        fill_rect(scene, preview_rect, theme::CONTROL_BG_HOVER, ctrl_h / 2.0);
    } else if preview_rect.contains(app.hover) {
        fill_rect(
            scene,
            preview_rect,
            theme::CONTROL_HOVER_SUBTLE,
            ctrl_h / 2.0,
        );
    }
    stroke_rect(
        scene,
        preview_rect,
        theme::CONTROL_BORDER,
        ctrl_h / 2.0,
        1.0,
    );
    draw_preview_icon(
        scene,
        preview_rect.x0 + ctrl_h / 2.0,
        preview_rect.y0 + ctrl_h / 2.0,
        if preview_active { theme::TEXT } else { theme::TEXT_SECONDARY },
    );
    app.hit_regions.push(HitRegion {
        rect: preview_rect,
        action: HitAction::ToggleSamplePicker,
    });
    ccx += ctrl_h + 12.0;

    // Grid / List — paired together in one pill, since they're the same
    // choice (view mode), unlike the standalone buttons on either side.
    let seg_track = Rect::new(ccx, ctrl_y, ccx + seg_w * 2.0, ctrl_y + ctrl_h);
    fill_rect(scene, seg_track, theme::CONTROL_BG, ctrl_h / 2.0);
    stroke_rect(scene, seg_track, theme::CONTROL_BORDER, ctrl_h / 2.0, 1.0);
    for (i, mode) in [ViewMode::Grid, ViewMode::List].into_iter().enumerate() {
        let r = Rect::new(
            ccx + seg_w * i as f64,
            ctrl_y,
            ccx + seg_w * (i as f64 + 1.0),
            ctrl_y + ctrl_h,
        );
        let active = app.view_mode == mode;
        if active {
            fill_rect(scene, r.inset(-2.0), theme::CONTROL_BG_HOVER, seg_w / 2.0);
        } else if r.contains(app.hover) {
            fill_rect(
                scene,
                r.inset(-1.0),
                theme::CONTROL_HOVER_SUBTLE,
                seg_w / 2.0,
            );
        }
        let icon_color = if active {
            theme::TEXT
        } else {
            theme::TEXT_SECONDARY
        };
        let (icon_cx, icon_cy) = (r.x0 + seg_w / 2.0, ctrl_y + ctrl_h / 2.0);
        match mode {
            ViewMode::Grid => draw_grid_icon(scene, icon_cx, icon_cy, icon_color),
            ViewMode::List => draw_list_icon(scene, icon_cx, icon_cy, icon_color),
        }
        app.hit_regions.push(HitRegion {
            rect: r,
            action: HitAction::SetViewMode(mode),
        });
    }
    ccx += seg_w * 2.0 + 12.0;

    // Info — standalone circular button (full radius, not just rounded
    // corners), matching the reference's distinct "(i)" pill shape.
    let info_rect = Rect::new(ccx, ctrl_y, ccx + ctrl_h, ctrl_y + ctrl_h);
    let info_active = app.detail_open;
    if info_active {
        fill_rect(scene, info_rect, theme::CONTROL_BG_HOVER, ctrl_h / 2.0);
    } else if info_rect.contains(app.hover) {
        fill_rect(scene, info_rect, theme::CONTROL_HOVER_SUBTLE, ctrl_h / 2.0);
    }
    stroke_rect(scene, info_rect, theme::CONTROL_BORDER, ctrl_h / 2.0, 1.0);
    draw_info_icon(
        scene,
        text,
        info_rect.x0 + ctrl_h / 2.0,
        info_rect.y0 + ctrl_h / 2.0,
        if info_active {
            theme::TEXT
        } else {
            theme::TEXT_SECONDARY
        },
    );
    app.hit_regions.push(HitRegion {
        rect: info_rect,
        action: HitAction::ToggleDetailPanel,
    });
    let search_radius = ctrl_h / 2.0;
    fill_rect(scene, search_rect, theme::CONTROL_BG, search_radius);
    let is_focused = app.focus == Focus::Search;
    stroke_rect(
        scene,
        search_rect,
        if is_focused {
            theme::CONTROL_FOCUS
        } else {
            theme::CONTROL_BORDER
        },
        search_radius,
        if is_focused { 1.5 } else { 1.0 },
    );
    draw_search_icon(
        scene,
        search_rect.x0 + 11.0,
        ctrl_y + 9.0,
        theme::TEXT_TERTIARY,
    );
    let text_x = search_rect.x0 + 34.0;
    // `TextCx::draw` receives an origin, not a UIKit-style visual baseline.
    // Keep the origin deliberately above the optical center so ascenders and
    // descenders remain inside this compact control.
    let text_y = ctrl_y + 11.0;
    let text_clip = Rect::new(
        text_x,
        search_rect.y0 + 3.0,
        search_rect.x1 - 8.0,
        search_rect.y1 - 3.0,
    );
    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &text_clip);
    if app.search.is_empty() && app.focus != Focus::Search {
        text.draw(
            scene,
            "Search family\u{2026}",
            13.0,
            None,
            theme::TEXT_TERTIARY,
            text_x,
            text_y,
        );
    } else {
        text.draw(scene, &app.search, 13.0, None, theme::TEXT, text_x, text_y);
        if app.focus == Focus::Search {
            let caret_x = text_x + text.measure(&app.search, 13.0, None);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                theme::ACCENT,
                None,
                &Rect::new(
                    caret_x + 1.0,
                    ctrl_y + 6.0,
                    caret_x + 2.0,
                    ctrl_y + ctrl_h - 6.0,
                ),
            );
        }
    }
    scene.pop_layer();
    app.hit_regions.push(HitRegion {
        rect: search_rect,
        action: HitAction::FocusField(Focus::Search),
    });
}

/// Dispatches a click at `point` against the regions the last [`draw`] call
/// recorded (topmost-drawn first), then blurs any focused text field if
/// nothing under the click claims focus for itself.
pub fn handle_click(app: &mut App, point: Point) {
    if let Some(menu) = &app.context_menu {
        let inside = menu.panel_rect.contains(point);
        app.context_menu = None;
        if !inside {
            return;
        }
    }

    let hit = app
        .hit_regions
        .iter()
        .rev()
        .find(|r| r.rect.contains(point))
        .map(|r| &r.action);

    let Some(action) = hit else {
        app.focus = Focus::None;
        return;
    };

    match action {
        HitAction::SelectNav(f) => {
            app.filter = *f;
            app.focus = Focus::None;
        }
        HitAction::RemoveFolder(id) => {
            let id = *id;
            app.focus = Focus::None;
            if app.filter == Filter::Folder(id) {
                app.filter = Filter::All;
            }
            if app.catalog.remove_folder(id).is_ok() {
                app.reload_folders_and_fonts("Folder removed".to_string());
            }
        }
        HitAction::RevealFolder(id) => {
            let id = *id;
            app.focus = Focus::None;
            if let Some(folder) = app.folders.iter().find(|f| f.id == id) {
                let _ = crate::reveal::reveal(std::path::Path::new(&folder.path));
            }
        }
        HitAction::AddFolder => {
            app.focus = Focus::None;
            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                let path = dir.to_string_lossy().into_owned();
                match app.catalog.add_folder(&path) {
                    Ok(result) => app.reload_folders_and_fonts(result.status),
                    Err(e) => app.status = e.to_string(),
                }
            }
        }
        HitAction::Rescan => {
            app.focus = Focus::None;
            app.status = "Scanning\u{2026}".to_string();
            match app.catalog.rescan() {
                Ok(result) => app.reload_folders_and_fonts(result.status),
                Err(e) => app.status = e.to_string(),
            }
        }
        HitAction::SetViewMode(mode) => {
            app.view_mode = *mode;
            app.scroll_y = 0.0;
            app.focus = Focus::None;
        }
        HitAction::StartTileSizeDrag => {
            app.dragging_tile_size = true;
            update_tile_size_from_point(app, point);
            app.focus = Focus::None;
        }
        HitAction::StartScrollDrag => {
            app.dragging_scrollbar = true;
            update_scroll_from_point(app, point);
            app.focus = Focus::None;
        }
        HitAction::ToggleDetailPanel => {
            app.detail_open = !app.detail_open;
            app.focus = Focus::None;
        }
        HitAction::ToggleSamplePicker => {
            app.sample_picker_open = !app.sample_picker_open;
            app.focus = if app.sample_picker_open {
                Focus::SampleText
            } else {
                Focus::None
            };
        }
        HitAction::SetSampleText(text) => {
            app.sample_text = text.clone();
            app.focus = Focus::SampleText;
        }
        HitAction::ClearSampleText => {
            app.sample_text.clear();
            app.focus = Focus::SampleText;
        }
        HitAction::FocusField(f) => {
            app.focus = *f;
        }
        HitAction::SelectFont(id) => {
            app.selected = Some(*id);
            app.focus = Focus::None;
        }
        HitAction::ToggleActivation(id) => {
            let id = *id;
            app.focus = Focus::None;
            if let Ok(active) = app.catalog.toggle_activation(id) {
                if active {
                    app.active_ids.insert(id);
                } else {
                    app.active_ids.remove(&id);
                }
            }
        }
        HitAction::ToggleFavorite(id) => {
            let id = *id;
            app.focus = Focus::None;
            toggle_favorite(app, id);
        }
        HitAction::ToggleDetailFavorite => {
            app.focus = Focus::None;
            if let Some(id) = app.selected {
                toggle_favorite(app, id);
            }
        }
        HitAction::CloseDetail => {
            app.detail_open = false;
            app.focus = Focus::None;
        }
    }
}

/// Right-clicking a library row opens its menu; right-clicking anywhere
/// else closes whatever menu is open, same as a left click would.
pub fn handle_right_click(app: &mut App, point: Point) {
    let hit = app
        .folder_rows
        .iter()
        .find(|(_, rect)| rect.contains(point))
        .map(|(id, _)| *id);
    app.context_menu = hit.map(|folder_id| ContextMenu {
        folder_id,
        anchor: point,
        panel_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
    });
}

/// Maps a pointer position on the rendered type-size track to a tile size.
/// Called both at drag start and on subsequent pointer motion while captured.
pub fn update_tile_size_from_point(app: &mut App, point: Point) {
    let Some((x0, x1)) = app.tile_slider_track else {
        return;
    };
    let progress = ((point.x - x0) / (x1 - x0)).clamp(0.0, 1.0);
    app.tile_size = theme::MIN_TILE + progress * (theme::MAX_TILE - theme::MIN_TILE);
}

/// Maps a pointer position on the rendered scrollbar track to a `scroll_y`.
/// Called both at drag start and on subsequent pointer motion while captured.
pub fn update_scroll_from_point(app: &mut App, point: Point) {
    let Some((track_y0, track_y1, content_h, viewport_h)) = app.scroll_track else {
        return;
    };
    let max_scroll = (content_h - viewport_h).max(0.0);
    if max_scroll <= 0.0 || track_y1 <= track_y0 {
        return;
    }
    let progress = ((point.y - track_y0) / (track_y1 - track_y0)).clamp(0.0, 1.0);
    app.scroll_y = progress * max_scroll;
}

fn toggle_favorite(app: &mut App, id: i64) {
    if let Ok(favorite) = app.catalog.toggle_favorite(id) {
        if favorite {
            app.favorite_ids.insert(id);
        } else {
            app.favorite_ids.remove(&id);
        }
    }
}
