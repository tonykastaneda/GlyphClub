//! List view: a real column table — sortable (click a header), resizable
//! (drag a header's right edge) and reorderable (drag a header) columns,
//! with fonts grouped by family and an expand/collapse chevron for any
//! family with more than one style. This is meaningfully more machinery
//! than the grid view needs, which is why it's its own module.

use vello::kurbo::{Affine, BezPath, Circle, Point, Rect, Stroke};
use vello::peniko::Fill;
use vello::Scene;

use super::grid::{draw_empty_state, draw_scrollbar, draw_system_lock};
use super::{draw_format_badge, fill_rect, stroke_rect, truncate_to_width, HitAction, HitRegion};
use crate::app::{App, FamilyGroup, FontEntry, ListColumnKind, ListGroupsCacheKey, ListSort};
use crate::text::TextCx;
use crate::theme;

const HEADER_H: f64 = 34.0;
/// Width of the invisible strip at a header cell's right edge that starts
/// a resize drag instead of a reorder-or-sort press.
const RESIZE_HANDLE_W: f64 = 6.0;
const CHEVRON_COL_W: f64 = 18.0;
const DOT_COL_W: f64 = 28.0;
const INDENT_W: f64 = 26.0;

fn format_date(mtime: i64) -> String {
    chrono::DateTime::from_timestamp(mtime, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%b %-d, %Y at %-I:%M %p")
                .to_string()
        })
        .unwrap_or_default()
}

/// Groups `app.entries` by family — `O(n)` via a hash map keyed by family
/// name, not the linear "scan every group built so far" search this used
/// to do (`O(n × distinct families)`, which at a real library's size — a
/// hundred thousand-plus fonts — was the single biggest reason list view
/// used to choke). Only actually called when `App::list_groups_cache` is
/// stale (see `draw_list`), not on every frame.
fn rebuild_family_groups(app: &App) -> Vec<FamilyGroup> {
    let visible: std::collections::HashSet<i64> =
        app.visible_entries().into_iter().map(|e| e.id).collect();

    // `order` keeps first-seen family order stable regardless of hash
    // iteration order, so a family's position doesn't jitter between
    // otherwise-equal sort keys from one rebuild to the next.
    let mut order: Vec<&str> = Vec::new();
    let mut by_family: std::collections::HashMap<&str, Vec<&FontEntry>> = std::collections::HashMap::new();
    for entry in &app.entries {
        if !visible.contains(&entry.id) {
            continue;
        }
        let members = by_family.entry(entry.family.as_str()).or_insert_with(|| {
            order.push(entry.family.as_str());
            Vec::new()
        });
        members.push(entry);
    }

    order
        .into_iter()
        .map(|family| {
            let mut members = by_family.remove(family).unwrap_or_default();
            members.sort_by(|a, b| a.weight.cmp(&b.weight).then(a.subfamily.cmp(&b.subfamily)));
            let primary = members
                .iter()
                .find(|e| e.subfamily.eq_ignore_ascii_case("regular"))
                .copied()
                .unwrap_or(members[0]);
            let newest_mtime = members.iter().map(|e| e.mtime).max().unwrap_or(0);
            let any_favorite = members.iter().any(|e| app.favorite_ids.contains(&e.id));
            let family = family.to_string();
            FamilyGroup {
                family_sort_key: family.to_ascii_lowercase(),
                member_ids: members.iter().map(|e| e.id).collect(),
                primary_id: primary.id,
                format: crate::font::format_label(&primary.path),
                newest_mtime,
                any_favorite,
                family,
            }
        })
        .collect()
}

fn sort_family_groups(groups: &mut [FamilyGroup], sort: ListSort) {
    groups.sort_by(|a, b| {
        // Compares the precomputed `family_sort_key`, not a freshly
        // lowercased `family` — the naive version allocated two new
        // `String`s per comparison, every sort, for nothing.
        let ordering = match sort.column {
            ListColumnKind::Name | ListColumnKind::Preview => a.family_sort_key.cmp(&b.family_sort_key),
            ListColumnKind::Styles => a.member_ids.len().cmp(&b.member_ids.len()),
            ListColumnKind::Favorite => a.any_favorite.cmp(&b.any_favorite),
            ListColumnKind::Format => a.format.cmp(b.format),
            ListColumnKind::Category => std::cmp::Ordering::Equal,
            ListColumnKind::DateAdded => a.newest_mtime.cmp(&b.newest_mtime),
        };
        if sort.ascending {
            ordering
        } else {
            ordering.reverse()
        }
        .then_with(|| a.family_sort_key.cmp(&b.family_sort_key))
    });
}

enum Row {
    /// Index into the `groups` slice.
    Family(usize),
    /// `(group index, member id)` — only present under an expanded family.
    Style(usize, i64),
}

fn flatten_rows(groups: &[FamilyGroup], app: &App) -> Vec<Row> {
    let mut rows = Vec::with_capacity(groups.len());
    for (gi, group) in groups.iter().enumerate() {
        rows.push(Row::Family(gi));
        if group.member_ids.len() > 1 && app.expanded_families.contains(&group.family) {
            for &id in &group.member_ids {
                rows.push(Row::Style(gi, id));
            }
        }
    }
    rows
}

/// Left edge of each column, plus one trailing entry for the last column's
/// right edge — so column `i` spans `edges[i]..edges[i + 1]`.
fn column_edges(app: &App, x0: f64) -> Vec<f64> {
    let mut x = x0;
    let mut edges = vec![x];
    for col in &app.list_columns {
        x += col.width;
        edges.push(x);
    }
    edges
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
    // Regrouping+resorting all of `entries` is real work at 150k+ fonts —
    // reused across frames whenever nothing that would change the result
    // has, which is every frame except the one right after a search
    // keystroke, a filter/sort change, or a catalog reload. `take()` moves
    // the cached `Vec` out (no clone) so the row-drawing loop below is
    // free to take `&mut app` without fighting a borrow of it.
    let key = ListGroupsCacheKey {
        entries_version: app.entries_version,
        favorites_version: app.favorites_version,
        filter: app.filter,
        search: app.search.clone(),
        sort: app.list_sort,
    };
    let groups = match app.list_groups_cache.take() {
        Some((cached_key, groups)) if cached_key == key => groups,
        _ => {
            let mut groups = rebuild_family_groups(app);
            sort_family_groups(&mut groups, app.list_sort);
            groups
        }
    };

    if groups.is_empty() {
        draw_empty_state(app, text, scene, x0, x1, y0, height);
        app.list_header_rects.clear();
        app.list_groups_cache = Some((key, groups));
        return;
    }
    let rows = flatten_rows(&groups, app);

    draw_header(app, text, scene, x0, x1, y0);
    let rows_y0 = y0 + HEADER_H;
    let viewport_h = (height - rows_y0).max(0.0);

    let row_h = theme::ROW_H * (app.tile_size / theme::DEFAULT_TILE);
    let total = rows.len();
    let max_scroll = (total as f64 * row_h - viewport_h).max(0.0);
    app.scroll_y = app.scroll_y.clamp(0.0, max_scroll);

    let start = (app.scroll_y / row_h).floor().max(0.0) as usize;
    let visible = (viewport_h / row_h).ceil() as usize + 2;
    let end = (start + visible).min(total);

    scene.push_clip_layer(
        Fill::NonZero,
        Affine::IDENTITY,
        &Rect::new(x0, rows_y0, x1, height),
    );

    for i in start..end {
        let row_y = rows_y0 + i as f64 * row_h - app.scroll_y;
        match rows[i] {
            Row::Family(gi) => draw_family_row(app, text, scene, &groups[gi], x0, x1, row_y, row_h),
            Row::Style(gi, id) => {
                draw_style_row(app, text, scene, &groups[gi], id, x0, x1, row_y, row_h)
            }
        }
    }

    scene.pop_layer();
    draw_scrollbar(app, scene, x1, rows_y0, height, total as f64 * row_h, viewport_h);

    app.list_groups_cache = Some((key, groups));
}

fn draw_sort_arrow(scene: &mut Scene, center: Point, ascending: bool, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    let (dy1, dy2) = if ascending { (-2.6, 2.2) } else { (2.6, -2.2) };
    path.move_to((center.x - 3.4, center.y + dy2 * 0.15));
    path.line_to((center.x, center.y + dy1));
    path.line_to((center.x + 3.4, center.y + dy2 * 0.15));
    scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, color, None, &path);
}

fn draw_header(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64, y0: f64) {
    let header_rect = Rect::new(x0, y0, x1, y0 + HEADER_H);
    fill_rect(scene, header_rect, theme::PANEL_BG(), 0.0);
    stroke_rect(scene, header_rect, theme::SEPARATOR(), 0.0, 1.0);

    let edges = column_edges(app, x0 + DOT_COL_W);
    app.list_header_rects.clear();

    let dragging_i = app.dragging_column_reorder;

    for (i, col) in app.list_columns.iter().enumerate() {
        let cell = Rect::new(edges[i], y0, edges[i + 1], y0 + HEADER_H);
        app.list_header_rects.push(cell);

        if Some(i) == dragging_i {
            fill_rect(scene, cell, theme::CONTROL_BG_HOVER(), 0.0);
        }

        if i > 0 {
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                theme::SEPARATOR(),
                None,
                &vello::kurbo::Line::new((cell.x0, y0 + 8.0), (cell.x0, y0 + HEADER_H - 8.0)),
            );
        }

        let label = col.kind.label();
        if !label.is_empty() {
            let label_x = cell.x0 + 12.0;
            text.draw_centered_v(
                scene,
                label,
                12.5,
                None,
                theme::TEXT_SECONDARY(),
                label_x,
                cell.y0,
                cell.y1,
            );
            if app.list_sort.column == col.kind {
                let arrow_x = label_x + text.measure(label, 12.5, None) + 10.0;
                draw_sort_arrow(
                    scene,
                    Point::new(arrow_x, cell.y0 + HEADER_H / 2.0),
                    app.list_sort.ascending,
                    theme::TEXT_SECONDARY(),
                );
            }
        }

        let press_rect = Rect::new(cell.x0, cell.y0, cell.x1 - RESIZE_HANDLE_W, cell.y1);
        app.hit_regions.push(HitRegion {
            rect: press_rect,
            action: HitAction::PressColumnHeader(i),
        });
        let handle_rect = Rect::new(cell.x1 - RESIZE_HANDLE_W, cell.y0, cell.x1, cell.y1);
        app.hit_regions.push(HitRegion {
            rect: handle_rect,
            action: HitAction::StartColumnResize(i),
        });
    }

    // "Drop here" indicator: purely visual, so it's computed fresh from
    // the pointer's current position against these (still pre-reorder)
    // rendered boundaries rather than from `column_reorder_target` — that
    // field is in post-removal index space for the actual `Vec` move on
    // release, a different coordinate system than "which on-screen edge
    // is closest right now."
    if dragging_i.is_some() {
        if let Some(&nearest) = edges.iter().min_by(|a, b| {
            (**a - app.hover.x)
                .abs()
                .partial_cmp(&(**b - app.hover.x).abs())
                .unwrap()
        }) {
            scene.stroke(
                &Stroke::new(2.0),
                Affine::IDENTITY,
                theme::BRAND_ACCENT(),
                None,
                &vello::kurbo::Line::new((nearest, y0), (nearest, y0 + HEADER_H)),
            );
        }
    }
}

/// Common row chrome (selection/hover fill, activation dot, expand
/// chevron) shared by family and style rows — the two differ only in what
/// text/columns they draw past the indent.
struct RowContext {
    row_y: f64,
    row_h: f64,
    s: f64,
    selected: bool,
}

fn row_chrome(
    app: &mut App,
    scene: &mut Scene,
    id: i64,
    is_system: bool,
    active: bool,
    x0: f64,
    x1: f64,
    row_y: f64,
    row_h: f64,
) -> RowContext {
    let s = row_h / theme::ROW_H;
    let rect = Rect::new(x0, row_y, x1, row_y + row_h);
    let selected = app.selected == Some(id);

    // An outline, not a full lime fill — mirrors the grid tile's own
    // selected-border treatment instead of the much louder solid-color row
    // a fill produces at this width.
    if selected {
        fill_rect(scene, rect, theme::NAV_SELECTED_BG(), 0.0);
    } else if rect.contains(app.hover) {
        fill_rect(scene, rect, theme::CONTROL_BG(), 0.0);
    }
    stroke_rect(
        scene,
        Rect::new(x0, row_y, x1, row_y + row_h),
        if selected { theme::BRAND_ACCENT() } else { theme::SEPARATOR() },
        0.0,
        if selected { 1.5 } else { 1.0 },
    );

    let dot_center = Point::new(x0 + DOT_COL_W / 2.0, row_y + row_h / 2.0);
    if is_system {
        draw_system_lock(scene, dot_center, s);
    } else {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            if active { theme::GREEN() } else { theme::DOT_INACTIVE() },
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

    app.hit_regions.push(HitRegion {
        rect,
        action: HitAction::SelectFont(id),
    });

    RowContext {
        row_y,
        row_h,
        s,
        selected,
    }
}

fn draw_favorite_star(app: &mut App, text: &mut TextCx, scene: &mut Scene, id: i64, cell: Rect) {
    let favorite = app.favorite_ids.contains(&id);
    let color = if favorite { theme::GOLD() } else { theme::DOT_INACTIVE() };
    let glyph = "\u{2605}";
    let w = text.measure(glyph, 11.0, None);
    text.draw_centered_v(
        scene,
        glyph,
        11.0,
        None,
        color,
        cell.x0 + (cell.width() - w) / 2.0,
        cell.y0,
        cell.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: cell,
        action: HitAction::ToggleFavorite(id),
    });
}

#[allow(clippy::too_many_arguments)]
fn draw_family_row(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    group: &FamilyGroup,
    x0: f64,
    x1: f64,
    row_y: f64,
    row_h: f64,
) {
    let is_system = app.entry(group.primary_id).is_some_and(|e| e.is_system);
    let active = app.active_ids.contains(&group.primary_id) || is_system;
    let ctx = row_chrome(app, scene, group.primary_id, is_system, active, x0, x1, row_y, row_h);

    let expandable = group.member_ids.len() > 1;
    let expanded = expandable && app.expanded_families.contains(&group.family);

    // Same starting x as the header (`x0 + DOT_COL_W`) so column
    // boundaries always line up with it — the chevron below is drawn
    // *inside* the Name cell's own width, not by shifting every column
    // over to make room for it.
    let edges = column_edges(app, x0 + DOT_COL_W);
    let name_cell = Rect::new(edges[0], ctx.row_y, edges[1], ctx.row_y + ctx.row_h);

    if expandable {
        let chevron_center = Point::new(name_cell.x0 + CHEVRON_COL_W / 2.0, name_cell.y0 + ctx.row_h / 2.0);
        draw_chevron(scene, chevron_center, expanded, theme::TEXT_TERTIARY());
        app.hit_regions.push(HitRegion {
            rect: Rect::new(
                chevron_center.x - 10.0,
                chevron_center.y - 10.0,
                chevron_center.x + 10.0,
                chevron_center.y + 10.0,
            ),
            action: HitAction::ToggleFamilyExpanded(group.family.clone()),
        });
    }

    let is_unknown = group.family.eq_ignore_ascii_case("unknown");
    let family_font = if is_unknown { None } else { app.preview_family(text, group.primary_id) };
    // Only the Preview column's sample text needs glyph coverage — the
    // Name column keeps rendering the family name in its own font
    // regardless, same reasoning as `ui::grid`'s tiles.
    let covers_sample = !is_unknown && app.covers_sample(group.primary_id);

    // Cloned (cheap — `ListColumnSpec` is `Copy`) so the loop body is free
    // to take `&mut app` (e.g. `draw_favorite_star`) without fighting an
    // in-progress borrow of `app.list_columns` itself.
    let columns = app.list_columns.clone();
    for (i, col) in columns.iter().enumerate() {
        let cell = Rect::new(edges[i], ctx.row_y, edges[i + 1], ctx.row_y + ctx.row_h);
        let text_color = if ctx.selected { theme::TEXT() } else { theme::TEXT_SECONDARY() };
        match col.kind {
            ListColumnKind::Name => {
                let specimen_size = (14.5 * ctx.s).clamp(11.0, 28.0);
                let text_x0 = if expandable { cell.x0 + CHEVRON_COL_W } else { cell.x0 + 2.0 };
                let badge_w = draw_format_badge(scene, group.format, text_x0, cell.y0, cell.y1, theme::TEXT_TERTIARY());
                let name_x0 = text_x0 + if badge_w > 0.0 { badge_w + 6.0 } else { 0.0 };
                let name_max_w = (cell.x1 - 8.0 - name_x0).max(0.0);
                let family = truncate_to_width(text, &group.family, specimen_size as f32, family_font.as_deref(), name_max_w);
                text.draw_centered_v(
                    scene,
                    &family,
                    specimen_size as f32,
                    family_font.as_deref(),
                    if ctx.selected { theme::TEXT() } else { theme::TEXT() },
                    name_x0,
                    cell.y0,
                    cell.y1,
                );
            }
            ListColumnKind::Preview => {
                let sample = app.effective_sample_text();
                let size = (18.0 * ctx.s).clamp(13.0, 30.0);
                let preview_font = if covers_sample { family_font.as_deref() } else { None };
                let preview_color = if covers_sample { text_color } else { theme::TEXT_TERTIARY() };
                text.draw_centered_v(
                    scene,
                    sample,
                    size as f32,
                    preview_font,
                    preview_color,
                    cell.x0 + 2.0,
                    cell.y0,
                    cell.y1,
                );
            }
            ListColumnKind::Styles => {
                let label = if group.member_ids.len() == 1 {
                    "1 style".to_string()
                } else {
                    format!("{} styles", group.member_ids.len())
                };
                text.draw_centered_v(scene, &label, 12.5, None, text_color, cell.x0 + 2.0, cell.y0, cell.y1);
            }
            ListColumnKind::Favorite => draw_favorite_star(app, text, scene, group.primary_id, cell),
            ListColumnKind::Format => {
                text.draw_centered_v(scene, group.format, 12.5, None, text_color, cell.x0 + 2.0, cell.y0, cell.y1);
            }
            ListColumnKind::Category => {}
            ListColumnKind::DateAdded => {
                let label = format_date(group.newest_mtime);
                text.draw_centered_v(scene, &label, 12.5, None, text_color, cell.x0 + 2.0, cell.y0, cell.y1);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_style_row(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    group: &FamilyGroup,
    id: i64,
    x0: f64,
    x1: f64,
    row_y: f64,
    row_h: f64,
) {
    let Some(entry) = app.entry(id).map(|e| (e.subfamily.clone(), e.path.clone(), e.mtime, e.is_system)) else {
        return;
    };
    let (subfamily, path, mtime, is_system) = entry;
    let active = app.active_ids.contains(&id) || is_system;
    let ctx = row_chrome(app, scene, id, is_system, active, x0, x1, row_y, row_h);

    // Same starting x as the header, matching `draw_family_row` — only the
    // Name cell's own drawn text is inset (via `indented` below), not the
    // column boundaries themselves.
    let edges = column_edges(app, x0 + DOT_COL_W);
    let style_font = app.preview_family(text, id);
    let covers_sample = app.covers_sample(id);

    let columns = app.list_columns.clone();
    for (i, col) in columns.iter().enumerate() {
        let cell = Rect::new(edges[i], ctx.row_y, edges[i + 1], ctx.row_y + ctx.row_h);
        let text_color = if ctx.selected { theme::TEXT_SECONDARY() } else { theme::TEXT_TERTIARY() };
        match col.kind {
            ListColumnKind::Name => {
                let indented = Rect::new(cell.x0 + INDENT_W, cell.y0, cell.x1, cell.y1);
                let badge_w = draw_format_badge(
                    scene,
                    crate::font::format_label(&path),
                    indented.x0,
                    indented.y0,
                    indented.y1,
                    theme::TEXT_TERTIARY(),
                );
                let name_x0 = indented.x0 + if badge_w > 0.0 { badge_w + 6.0 } else { 0.0 };
                let style_size = (12.5 * ctx.s).clamp(10.0, 20.0) as f32;
                let name_max_w = (indented.x1 - 8.0 - name_x0).max(0.0);
                let subfamily = truncate_to_width(text, &subfamily, style_size, style_font.as_deref(), name_max_w);
                text.draw_centered_v(
                    scene,
                    &subfamily,
                    style_size,
                    style_font.as_deref(),
                    if ctx.selected { theme::TEXT() } else { theme::TEXT_SECONDARY() },
                    name_x0,
                    indented.y0,
                    indented.y1,
                );
            }
            ListColumnKind::Preview => {
                let sample = app.effective_sample_text();
                let size = (16.0 * ctx.s).clamp(11.0, 26.0);
                let preview_font = if covers_sample { style_font.as_deref() } else { None };
                let preview_color = if covers_sample { text_color } else { theme::TEXT_TERTIARY() };
                text.draw_centered_v(
                    scene,
                    sample,
                    size as f32,
                    preview_font,
                    preview_color,
                    cell.x0 + 2.0,
                    cell.y0,
                    cell.y1,
                );
            }
            ListColumnKind::Styles => {}
            ListColumnKind::Favorite => draw_favorite_star(app, text, scene, id, cell),
            ListColumnKind::Format => {
                let label = crate::font::format_label(&path);
                text.draw_centered_v(scene, label, 12.5, None, text_color, cell.x0 + 2.0, cell.y0, cell.y1);
            }
            ListColumnKind::Category => {}
            ListColumnKind::DateAdded => {
                let label = format_date(mtime);
                text.draw_centered_v(scene, &label, 12.5, None, text_color, cell.x0 + 2.0, cell.y0, cell.y1);
            }
        }
    }
    let _ = group;
}

fn draw_chevron(scene: &mut Scene, center: Point, expanded: bool, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    if expanded {
        path.move_to((center.x - 3.2, center.y - 1.6));
        path.line_to((center.x, center.y + 2.2));
        path.line_to((center.x + 3.2, center.y - 1.6));
    } else {
        path.move_to((center.x - 1.6, center.y - 3.2));
        path.line_to((center.x + 2.2, center.y));
        path.line_to((center.x - 1.6, center.y + 3.2));
    }
    scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, color, None, &path);
}
