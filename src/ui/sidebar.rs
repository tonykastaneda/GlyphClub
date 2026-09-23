use vello::kurbo::{Affine, BezPath, Cap, Circle, Join, Point, Rect, Stroke};
#[cfg(target_os = "macos")]
use vello::peniko::Fill;
use vello::Scene;

use super::{fill_rect, stroke_rect, truncate_to_width, HitAction, HitRegion};
use crate::app::{App, Filter};
use crate::text::TextCx;
use crate::theme;

pub fn draw(app: &mut App, text: &mut TextCx, scene: &mut Scene, height: f64) {
    // Uniform floating-card inset on every side — on macOS the traffic
    // lights are repositioned in `traffic_lights.rs` to fit comfortably
    // inside this, rather than this panel working around wherever they'd
    // otherwise default to.
    let panel = Rect::new(
        theme::FRAME_PAD,
        theme::FRAME_PAD,
        theme::SIDEBAR_W,
        height - theme::FRAME_PAD,
    );
    fill_rect(scene, panel, theme::SIDEBAR_BG(), 16.0);
    stroke_rect(scene, panel, theme::SEPARATOR(), 16.0, 1.0);

    // Custom-drawn traffic lights: the native ones (hidden in
    // `traffic_lights.rs`) turned out to have a hard, unmovable floor
    // inside their own tiny native container, so this app draws — and
    // owns full click handling for, see `main.rs`'s
    // `handle_traffic_light_click` — its own instead.
    #[cfg(target_os = "macos")]
    {
        let colors = [theme::TRAFFIC_CLOSE, theme::TRAFFIC_MINIMIZE, theme::TRAFFIC_ZOOM];
        let cy = theme::FRAME_PAD + 16.0;
        let centers: [Point; 3] = std::array::from_fn(|i| {
            Point::new(theme::FRAME_PAD + 20.0 + i as f64 * 20.0, cy)
        });
        let hit_rects: [Rect; 3] =
            std::array::from_fn(|i| Rect::new(centers[i].x - 9.0, centers[i].y - 9.0, centers[i].x + 9.0, centers[i].y + 9.0));
        // Real macOS traffic lights show all three glyphs together the
        // moment the pointer is anywhere over the group, not just over
        // whichever one it's directly on.
        let group_hovered = hit_rects.iter().any(|r| r.contains(app.hover));

        for (i, color) in colors.into_iter().enumerate() {
            let hovered = hit_rects[i].contains(app.hover);
            let radius = if hovered { 7.0 } else { 6.5 };
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &Circle::new(centers[i], radius));
            if group_hovered {
                draw_traffic_glyph(scene, i, centers[i]);
            }
            app.traffic_light_rects[i] = hit_rects[i];
        }
    }

    // The real icon+wordmark lockup, same corner the macOS traffic lights
    // sit in — Windows keeps its own native title bar (with its own
    // min/maximize/close) untouched, same as how Amalith handles it, so
    // this is purely a sidebar decoration, not window chrome.
    #[cfg(target_os = "windows")]
    {
        let band_cy = theme::FRAME_PAD + 20.0;
        let icon_rect = Rect::new(
            theme::FRAME_PAD + 12.0,
            band_cy - 8.0,
            theme::FRAME_PAD + 32.0,
            band_cy + 8.0,
        );
        crate::branding::draw_icon(scene, icon_rect, crate::branding::BRAND_LIME);
        // Sized to the icon's own fitted height (not the raw 16px band)
        // so the wordmark sits flush against it with no centering gap.
        let wordmark_w = 16.0 * (190.63 / 22.87);
        let wordmark_rect = Rect::new(
            icon_rect.x1 + 8.0,
            band_cy - 8.0,
            icon_rect.x1 + 8.0 + wordmark_w,
            band_cy + 8.0,
        );
        crate::branding::draw_wordmark(scene, wordmark_rect, theme::TEXT());
    }

    let pad = theme::FRAME_PAD + 16.0;
    let right = theme::SIDEBAR_W - 16.0;
    // Clearance for whatever sits above the nav list: the (macOS-only)
    // repositioned traffic lights (see `traffic_lights.rs`), or the logo
    // placeholder on Windows.
    #[cfg(target_os = "macos")]
    let mut y = 58.0;
    #[cfg(target_os = "windows")]
    let mut y = 54.0;
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut y = 30.0;

    let all_count = app.entries.len();
    let starred_count = app.starred_count();

    // Which of these show up at all is user-configurable — see
    // `ui::settings`'s "Show in sidebar" section — so this filters down
    // from the full four rather than always rendering all of them.
    let nav_items: Vec<(&str, Option<usize>, Filter)> = [
        ("All Fonts", Some(all_count), Filter::All, app.settings.sidebar_show_all),
        ("System Fonts", None, Filter::System, app.settings.sidebar_show_system),
        ("Starred", Some(starred_count), Filter::Starred, app.settings.sidebar_show_starred),
        ("Recents", None, Filter::Recent, app.settings.sidebar_show_recent),
    ]
    .into_iter()
    .filter(|&(_, _, _, show)| show)
    .map(|(label, count, filter, _)| (label, count, filter))
    .collect();

    for (label, count, filter) in nav_items {
        let rect = Rect::new(pad, y, right, y + theme::NAV_ITEM_H);
        let selected = filter == app.filter;
        if selected {
            fill_rect(scene, rect, theme::NAV_SELECTED_BG(), 8.0);
        } else if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG(), 8.0);
        }
        let color = if selected {
            theme::TEXT()
        } else {
            theme::TEXT_SECONDARY()
        };
        let icon_x = rect.x0 + 11.0;
        draw_nav_icon(scene, filter, icon_x, rect.center().y - 8.0, color);
        text.draw_centered_v(scene, label, 14.0, None, color, icon_x + 27.0, rect.y0, rect.y1);
        if let Some(n) = count {
            let s = n.to_string();
            let tw = text.measure(&s, 12.0, None);
            text.draw_centered_v(
                scene,
                &s,
                12.0,
                None,
                theme::TEXT_TERTIARY(),
                rect.x1 - 10.0 - tw,
                rect.y0,
                rect.y1,
            );
        }
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::SelectNav(filter),
        });
        y += theme::NAV_ITEM_H + 2.0;
    }

    y += 18.0;

    text.draw(
        scene,
        "LIBRARIES",
        10.0,
        None,
        theme::TEXT_TERTIARY(),
        pad + 2.0,
        y + 7.0,
    );
    y += 25.0;

    // The OS's own font directories are auto-added libraries (see
    // `Catalog::open`) so System Fonts has something to filter, but
    // they're not a folder the user picked — showing (and letting someone
    // remove) them here the same way as a real library would be
    // confusing, so this list stays user-added-libraries only.
    // Primed once before the loop below borrows `app.folders` — `folder_count`
    // needs `&mut self` to rebuild its cache, which the loop's own borrow of
    // `app.folders` wouldn't allow inside it.
    app.folder_count(0);
    for folder in app.folders.iter().filter(|f| !crate::font::is_system_path(&f.path)) {
        let rect = Rect::new(pad, y, right, y + theme::FOLDER_ROW_H);
        let folder_id = folder.id;
        let selected = app.filter == Filter::Folder(folder_id);
        if selected {
            fill_rect(scene, rect, theme::NAV_SELECTED_BG(), 8.0);
        } else if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG(), 8.0);
        }
        let name = folder
            .path
            .rsplit(['/', '\\'])
            .find(|s| !s.is_empty())
            .unwrap_or(&folder.path)
            .to_string();
        let count = app
            .folder_count_cache
            .as_ref()
            .and_then(|(_, counts)| counts.get(&folder_id))
            .copied()
            .unwrap_or(0)
            .to_string();
        let count_w = text.measure(&count, 12.0, None);
        text.draw_centered_v(
            scene,
            &count,
            12.0,
            None,
            theme::TEXT_TERTIARY(),
            rect.x1 - 10.0 - count_w,
            rect.y0,
            rect.y1,
        );
        let name_max_w = (rect.x1 - 10.0 - count_w - 8.0 - (rect.x0 + 36.0)).max(0.0);
        let name = truncate_to_width(text, &name, 12.5, None, name_max_w);
        text.draw(
            scene,
            &name,
            12.5,
            None,
            if selected {
                theme::TEXT()
            } else {
                theme::TEXT_SECONDARY()
            },
            rect.x0 + 36.0,
            y + 14.0,
        );
        draw_folder_icon(
            scene,
            rect.x0 + 11.0,
            rect.center().y - 8.0,
            if selected {
                theme::TEXT()
            } else {
                theme::TEXT_SECONDARY()
            },
        );

        app.folder_rows.push((folder_id, rect));
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::SelectNav(Filter::Folder(folder_id)),
        });

        y += theme::FOLDER_ROW_H;
    }

    // Right under the last library row (not pinned to the panel's bottom
    // edge any more) — TAGS, below it, reads as its own peer section the
    // same way LIBRARIES does, rather than "Add Library" sitting between
    // them like an orphaned leftover row.
    let add_rect = Rect::new(pad, y, right, y + theme::FOLDER_ROW_H);
    if add_rect.contains(app.hover) {
        fill_rect(scene, add_rect, theme::NAV_HOVER_BG(), 8.0);
    }
    draw_add_icon(scene, add_rect.x0 + 11.0, add_rect.center().y, theme::TEXT_SECONDARY());
    text.draw_centered_v(
        scene,
        "Add Library",
        13.0,
        None,
        theme::TEXT_SECONDARY(),
        add_rect.x0 + 30.0,
        add_rect.y0,
        add_rect.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: add_rect,
        action: HitAction::AddFolder,
    });
    y += theme::FOLDER_ROW_H;

    y += 18.0;
    text.draw(scene, "TAGS", 10.0, None, theme::TEXT_TERTIARY(), pad + 2.0, y + 7.0);
    y += 25.0;

    for tag in app.tags.clone() {
        let rect = Rect::new(pad, y, right, y + theme::FOLDER_ROW_H);
        let selected = app.filter == Filter::Tag(tag.id);
        if selected {
            fill_rect(scene, rect, theme::NAV_SELECTED_BG(), 8.0);
        } else if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG(), 8.0);
        }
        let count = app.tag_font_ids.get(&tag.id).map_or(0, |ids| ids.len()).to_string();
        let count_w = text.measure(&count, 12.0, None);
        text.draw_centered_v(scene, &count, 12.0, None, theme::TEXT_TERTIARY(), rect.x1 - 10.0 - count_w, rect.y0, rect.y1);
        let name_max_w = (rect.x1 - 10.0 - count_w - 8.0 - (rect.x0 + 34.0)).max(0.0);
        let name = truncate_to_width(text, &tag.name, 12.5, None, name_max_w);
        text.draw_centered_v(
            scene,
            "#",
            13.0,
            None,
            if selected { theme::TEXT() } else { theme::TEXT_SECONDARY() },
            rect.x0 + 13.0,
            rect.y0,
            rect.y1,
        );
        text.draw_centered_v(
            scene,
            &name,
            12.5,
            None,
            if selected { theme::TEXT() } else { theme::TEXT_SECONDARY() },
            rect.x0 + 34.0,
            rect.y0,
            rect.y1,
        );
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::SelectNav(Filter::Tag(tag.id)),
        });
        y += theme::FOLDER_ROW_H;
    }

    if !app.status.is_empty() {
        text.draw(
            scene,
            &app.status,
            11.0,
            None,
            theme::TEXT_TERTIARY(),
            pad,
            height - 58.0,
        );
    }
}

/// Hover-only glyphs for the traffic lights (`kind`: 0=close,
/// 1=minimize, 2=zoom) — close/minimize traced from Feather Icons'
/// `x`/`minus` (MIT licensed), scaled to this ~13px circle. Zoom is a
/// plain filled dot rather than the corner-arrows "maximize-2" glyph —
/// simpler, and reads better at this size.
#[cfg(target_os = "macos")]
fn draw_traffic_glyph(scene: &mut Scene, kind: usize, center: Point) {
    let glyph_color = vello::peniko::Color::from_rgba8(0x00, 0x00, 0x00, 0x99);
    let s = 0.45;
    let pt = |dx: f64, dy: f64| Point::new(center.x + dx * s, center.y + dy * s);
    let stroke = Stroke::new(1.8);

    match kind {
        0 => {
            for (a, b) in [(pt(6.0, -6.0), pt(-6.0, 6.0)), (pt(-6.0, -6.0), pt(6.0, 6.0))] {
                let mut path = BezPath::new();
                path.move_to(a);
                path.line_to(b);
                scene.stroke(&stroke, Affine::IDENTITY, glyph_color, None, &path);
            }
        }
        1 => {
            let mut path = BezPath::new();
            path.move_to(pt(-6.0, 0.0));
            path.line_to(pt(6.0, 0.0));
            scene.stroke(&stroke, Affine::IDENTITY, glyph_color, None, &path);
        }
        _ => {
            scene.fill(Fill::NonZero, Affine::IDENTITY, glyph_color, None, &Circle::new(center, 2.4));
        }
    }
}

// All navigation icons use a 16px canvas and the same rounded stroke.
fn nav_stroke() -> Stroke {
    Stroke::new(1.5).with_caps(Cap::Round).with_join(Join::Round)
}

fn draw_collection_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    // Uppercase A with an open, legible counter.
    path.move_to((x + 0.5, y + 13.0));
    path.line_to((x + 4.5, y + 3.0));
    path.line_to((x + 8.5, y + 13.0));
    path.move_to((x + 2.0, y + 9.0));
    path.line_to((x + 7.0, y + 9.0));
    // Lowercase a, drawn as a single-storey letter at this small size.
    path.move_to((x + 15.0, y + 7.0));
    path.line_to((x + 15.0, y + 13.0));
    path.move_to((x + 15.0, y + 10.0));
    path.curve_to((x + 15.0, y + 6.0), (x + 10.0, y + 6.0), (x + 10.0, y + 10.0));
    path.curve_to((x + 10.0, y + 14.0), (x + 15.0, y + 14.0), (x + 15.0, y + 10.0));
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &path);
}

fn draw_star_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    let center = (x + 8.0, y + 8.0);
    for i in 0..=10 {
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
        let radius = if i % 2 == 0 { 7.0 } else { 3.1 };
        let point = (
            center.0 + radius * angle.cos(),
            center.1 + radius * angle.sin(),
        );
        if i == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &path);
}

fn draw_folder_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    path.move_to((x + 2.5, y + 2.5));
    path.line_to((x + 6.0, y + 2.5));
    path.line_to((x + 8.0, y + 4.5));
    path.line_to((x + 13.5, y + 4.5));
    path.quad_to((x + 15.0, y + 4.5), (x + 15.0, y + 6.0));
    path.line_to((x + 15.0, y + 12.5));
    path.quad_to((x + 15.0, y + 14.0), (x + 13.5, y + 14.0));
    path.line_to((x + 2.5, y + 14.0));
    path.quad_to((x + 1.0, y + 14.0), (x + 1.0, y + 12.5));
    path.line_to((x + 1.0, y + 4.0));
    path.quad_to((x + 1.0, y + 2.5), (x + 2.5, y + 2.5));
    path.close_path();
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &path);
}

fn draw_add_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    path.move_to((x - 5.0, y));
    path.line_to((x + 5.0, y));
    path.move_to((x, y - 5.0));
    path.line_to((x, y + 5.0));
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &path);
}

pub(super) fn draw_nav_icon(scene: &mut Scene, filter: Filter, x: f64, y: f64, color: vello::peniko::Color) {
    match filter {
        Filter::All => draw_collection_icon(scene, x, y, color),
        Filter::System => draw_system_icon(scene, x, y, color),
        Filter::Starred => draw_star_icon(scene, x, y, color),
        Filter::Recent => draw_clock_icon(scene, x, y, color),
        _ => {}
    }
}

/// A monitor represents fonts supplied by the computer's operating system.
fn draw_system_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let screen = Rect::new(x + 1.0, y + 2.0, x + 15.0, y + 11.0);
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &screen.to_rounded_rect(1.5));
    let mut stand = BezPath::new();
    stand.move_to((x + 8.0, y + 11.0));
    stand.line_to((x + 8.0, y + 14.0));
    stand.move_to((x + 5.0, y + 14.0));
    stand.line_to((x + 11.0, y + 14.0));
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &stand);
}

fn draw_clock_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let center = Point::new(x + 8.0, y + 8.0);
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &Circle::new(center, 7.0));
    let mut hands = BezPath::new();
    hands.move_to(center);
    hands.line_to((center.x, center.y - 4.2));
    hands.move_to(center);
    hands.line_to((center.x + 3.4, center.y + 1.0));
    scene.stroke(&nav_stroke(), Affine::IDENTITY, color, None, &hands);
}
