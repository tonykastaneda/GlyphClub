use vello::kurbo::{Affine, BezPath, Rect, Stroke};
#[cfg(target_os = "macos")]
use vello::kurbo::{Circle, Point};
#[cfg(target_os = "macos")]
use vello::peniko::Fill;
use vello::Scene;

use super::{fill_rect, stroke_rect, HitAction, HitRegion};
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
    fill_rect(scene, panel, theme::SIDEBAR_BG, 16.0);
    stroke_rect(scene, panel, theme::SEPARATOR, 16.0, 1.0);

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

    // Placeholder for the real logo mark — same corner the macOS traffic
    // lights sit in, but Windows keeps its own native title bar (with its
    // own min/maximize/close) untouched, same as how Amalith handles it,
    // so this is purely a sidebar decoration, not window chrome.
    #[cfg(target_os = "windows")]
    {
        let logo_rect = Rect::new(
            theme::FRAME_PAD + 12.0,
            theme::FRAME_PAD + 8.0,
            theme::FRAME_PAD + 36.0,
            theme::FRAME_PAD + 32.0,
        );
        fill_rect(scene, logo_rect, theme::LOGO_PLACEHOLDER, 6.0);
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
    let starred_count = app
        .entries
        .iter()
        .filter(|e| app.favorite_ids.contains(&e.id))
        .count();

    let nav_items: [(&str, Option<usize>, Option<Filter>); 2] = [
        ("All Fonts", Some(all_count), Some(Filter::All)),
        ("Starred", Some(starred_count), Some(Filter::Starred)),
    ];

    for (label, count, filter) in nav_items {
        let rect = Rect::new(pad, y, right, y + theme::NAV_ITEM_H);
        let selected = filter.map(|f| f == app.filter).unwrap_or(false);
        if selected {
            fill_rect(scene, rect, theme::NAV_SELECTED_BG, 8.0);
        } else if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG, 8.0);
        }
        let color = if selected {
            theme::TEXT
        } else {
            theme::TEXT_SECONDARY
        };
        draw_nav_icon(scene, label, rect.x0 + 11.0, rect.center().y - 8.0, color);
        text.draw(scene, label, 14.0, None, color, rect.x0 + 38.0, y + 12.0);
        if let Some(n) = count {
            let s = n.to_string();
            let tw = text.measure(&s, 12.0, None);
            text.draw(
                scene,
                &s,
                12.0,
                None,
                theme::TEXT_TERTIARY,
                rect.x1 - 10.0 - tw,
                y + 11.0,
            );
        }
        if let Some(f) = filter {
            app.hit_regions.push(HitRegion {
                rect,
                action: HitAction::SelectNav(f),
            });
        }
        y += theme::NAV_ITEM_H + 2.0;
    }

    y += 18.0;

    text.draw(
        scene,
        "LIBRARIES",
        10.0,
        None,
        theme::TEXT_TERTIARY,
        pad + 2.0,
        y + 7.0,
    );
    y += 25.0;

    for folder in &app.folders {
        let rect = Rect::new(pad, y, right, y + theme::FOLDER_ROW_H);
        let folder_id = folder.id;
        let selected = app.filter == Filter::Folder(folder_id);
        if selected {
            fill_rect(scene, rect, theme::NAV_SELECTED_BG, 8.0);
        } else if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG, 8.0);
        }
        let name = folder
            .path
            .rsplit(['/', '\\'])
            .find(|s| !s.is_empty())
            .unwrap_or(&folder.path);
        text.draw(
            scene,
            name,
            12.5,
            None,
            if selected {
                theme::TEXT
            } else {
                theme::TEXT_SECONDARY
            },
            rect.x0 + 36.0,
            y + 14.0,
        );
        draw_folder_icon(
            scene,
            rect.x0 + 11.0,
            rect.center().y - 8.0,
            if selected {
                theme::TEXT
            } else {
                theme::TEXT_SECONDARY
            },
        );

        app.folder_rows.push((folder_id, rect));
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::SelectNav(Filter::Folder(folder_id)),
        });

        y += theme::FOLDER_ROW_H;
    }

    // Keep the primary library action fixed to the panel's bottom edge so
    // it is always available regardless of library count.
    let add_rect = Rect::new(pad, height - 48.0, right, height - 16.0);
    if add_rect.contains(app.hover) {
        fill_rect(scene, add_rect, theme::NAV_HOVER_BG, 8.0);
    }
    draw_add_icon(
        scene,
        add_rect.x0 + 12.0,
        add_rect.center().y,
        theme::TEXT_SECONDARY,
    );
    text.draw(
        scene,
        "Add Library",
        13.0,
        None,
        theme::TEXT_SECONDARY,
        add_rect.x0 + 30.0,
        add_rect.y0 + 10.0,
    );
    app.hit_regions.push(HitRegion {
        rect: add_rect,
        action: HitAction::AddFolder,
    });

    if !app.status.is_empty() {
        text.draw(
            scene,
            &app.status,
            11.0,
            None,
            theme::TEXT_TERTIARY,
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

fn draw_collection_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let back = Rect::new(x + 1.0, y + 1.0, x + 12.0, y + 12.0);
    let front = Rect::new(x + 4.0, y + 4.0, x + 15.0, y + 15.0);
    for rect in [back, front] {
        scene.stroke(
            &Stroke::new(1.45),
            Affine::IDENTITY,
            color,
            None,
            &rect.to_rounded_rect(2.0),
        );
    }
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
    scene.stroke(&Stroke::new(1.45), Affine::IDENTITY, color, None, &path);
}

fn draw_folder_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    path.move_to((x + 1.0, y + 5.0));
    path.line_to((x + 6.0, y + 5.0));
    path.line_to((x + 8.0, y + 2.0));
    path.line_to((x + 12.0, y + 2.0));
    path.line_to((x + 14.0, y + 5.0));
    path.line_to((x + 15.0, y + 5.0));
    path.line_to((x + 15.0, y + 14.0));
    path.line_to((x + 1.0, y + 14.0));
    path.close_path();
    scene.stroke(&Stroke::new(1.45), Affine::IDENTITY, color, None, &path);
}

fn draw_add_icon(scene: &mut Scene, x: f64, y: f64, color: vello::peniko::Color) {
    let mut path = BezPath::new();
    path.move_to((x - 5.0, y));
    path.line_to((x + 5.0, y));
    path.move_to((x, y - 5.0));
    path.line_to((x, y + 5.0));
    scene.stroke(&Stroke::new(1.6), Affine::IDENTITY, color, None, &path);
}

fn draw_nav_icon(scene: &mut Scene, label: &str, x: f64, y: f64, color: vello::peniko::Color) {
    match label {
        "All Fonts" => draw_collection_icon(scene, x, y, color),
        "Starred" => draw_star_icon(scene, x, y, color),
        _ => {}
    }
}
