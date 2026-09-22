use vello::kurbo::{Affine, BezPath, Point, Rect};
use vello::peniko::{Color, Fill};
use vello::Scene;

use super::{fill_rect, stroke_rect, truncate_to_width, HitAction, HitRegion};
use crate::app::{App, DetailTab, Focus, TextAlign};
use crate::text::TextCx;
use crate::theme;

/// (menu label, actual specimen text) — the quick-preset dropdown in the
/// Preview tab's controls row. The first preset's label doubles as its own
/// content; the others describe what they insert rather than being it
/// verbatim ("A-Z 0-9" inserts the real character run, not that literal
/// string).
const PREVIEW_PRESETS: &[(&str, &str)] = &[
    (
        "The quick brown fox jumped over the lazy dog",
        "The quick brown fox jumped over the lazy dog",
    ),
    ("A-Z 0-9", "ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789"),
    ("a-z 0-9", "abcdefghijklmnopqrstuvwxyz 0123456789"),
    (
        "Lorem Ipsum",
        "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.",
    ),
];

const PREVIEW_SIZE_STEPS: &[f64] = &[
    6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 18.0, 21.0, 24.0, 36.0, 48.0, 60.0, 72.0,
];

const SIZES_LADDER: &[f64] = &[
    96.0, 72.0, 60.0, 48.0, 36.0, 28.0, 24.0, 20.0, 18.0, 16.0, 14.0, 12.0, 11.0, 10.0, 9.0, 8.0, 7.0,
];
// Repeated (plus digits) rather than a single pass — at the smaller sizes
// on the ladder, a single "ABCDEFGHIJKLMNOPQRSTUVWXYZ" leaves most of the
// row's width empty; `fit_run` truncates this down to however much
// actually fits, so a longer source here only ever fills more of a row
// that has the room, never overflows one that doesn't.
const ALPHABET: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890";

pub fn draw(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64, y0: f64, height: f64) {
    let Some(id) = app.selected else { return };

    fill_rect(scene, Rect::new(x0, y0, x1, height), theme::PANEL_BG(), 0.0);
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::SEPARATOR(),
        None,
        &Rect::new(x0, y0, x0 + 1.0, height),
    );

    // A generous invisible strip straddling the panel's own left edge —
    // drag it to resize `App::detail_w` (min `theme::DETAIL_W`, max
    // `App::max_detail_w`). Highlighted only on hover/while dragging so it
    // doesn't otherwise compete with the thin static separator line above.
    let resize_handle = Rect::new(x0 - 3.0, y0, x0 + 3.0, height);
    if resize_handle.contains(app.hover) || app.dragging_detail_resize {
        scene.fill(Fill::NonZero, Affine::IDENTITY, theme::BRAND_ACCENT(), None, &Rect::new(x0 - 1.0, y0, x0 + 1.0, height));
    }
    app.hit_regions.push(HitRegion {
        rect: resize_handle,
        action: HitAction::StartDetailResize,
    });

    let pad = 22.0;
    let entry = app.entry(id).map(|e| (e.family.clone(), e.subfamily.clone()));
    let Some((family, subfamily)) = entry else {
        app.selected = None;
        return;
    };
    let favorite = app.favorite_ids.contains(&id);

    let mut y = y0 + 22.0;

    // No close button here — the detail panel's only open/close control is
    // the toolbar's Info button (`HitAction::ToggleDetailPanel`); a second
    // way to close it from inside the panel itself was redundant and,
    // being unlabeled, easy to mistake for something else.
    let name_row_h = 26.0;
    let star_rect = Rect::new(x1 - pad - 20.0, y, x1 - pad, y + name_row_h);
    let star_color = if favorite { theme::GOLD() } else { theme::TEXT_SECONDARY() };
    text.draw_centered_v(scene, "\u{2605}", 14.0, None, star_color, star_rect.x0, star_rect.y0, star_rect.y1);
    app.hit_regions.push(HitRegion {
        rect: star_rect,
        action: HitAction::ToggleDetailFavorite,
    });

    text.draw(scene, &family, 19.0, None, theme::TEXT(), x0 + pad, y + 2.0);
    y += 26.0;
    text.draw(scene, &subfamily, 12.0, None, theme::TEXT_SECONDARY(), x0 + pad, y);
    y += 30.0;

    y = draw_tab_bar(app, text, scene, x0 + pad, x1 - pad, y);
    y += 14.0;

    let content = Rect::new(x0 + pad, y, x1 - pad, height - 20.0);
    match app.detail_tab {
        DetailTab::Preview => draw_preview_tab(app, text, scene, id, content),
        DetailTab::Sizes => draw_sizes_tab(app, text, scene, id, content),
        DetailTab::Glyphs => draw_glyphs_tab(app, text, scene, id, content),
        DetailTab::Info => draw_info_tab(app, text, scene, id, content),
    }
}

/// Returns the y just below the tab bar. The pill group is centered
/// within `[x0, x1]` rather than anchored to `x0` — its own width is
/// usually well short of the full panel width, and flush-left left it
/// looking stranded against the left edge instead of centered under the
/// font name the way it's meant to read.
fn draw_tab_bar(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64, y0: f64) -> f64 {
    let tabs = [
        (DetailTab::Preview, "Preview"),
        (DetailTab::Sizes, "Sizes"),
        (DetailTab::Glyphs, "Glyphs"),
        (DetailTab::Info, "Info"),
    ];
    let h = 28.0;
    let pad_x = 12.0;
    let widths: Vec<f64> = tabs.iter().map(|(_, label)| text.measure(label, 12.0, None) + pad_x * 2.0).collect();
    let total_w: f64 = widths.iter().sum();
    let start_x = x0 + ((x1 - x0) - total_w) / 2.0;

    let track = Rect::new(start_x, y0, start_x + total_w, y0 + h);
    fill_rect(scene, track, theme::CONTROL_BG(), h / 2.0);
    stroke_rect(scene, track, theme::CONTROL_BORDER(), h / 2.0, 1.0);

    let mut cx = start_x;
    for (i, (tab, label)) in tabs.into_iter().enumerate() {
        let w = widths[i];
        let rect = Rect::new(cx, y0, cx + w, y0 + h);
        let active = app.detail_tab == tab;
        if active {
            fill_rect(scene, rect.inset(-2.0), theme::BRAND_ACCENT(), (h - 4.0) / 2.0);
        }
        let tw = text.measure(label, 12.0, None);
        text.draw_centered_v(
            scene,
            label,
            12.0,
            None,
            if active { theme::CANVAS() } else { theme::TEXT_SECONDARY() },
            rect.x0 + (rect.width() - tw) / 2.0,
            rect.y0,
            rect.y1,
        );
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::SetDetailTab(tab),
        });
        cx += w;
    }
    y0 + h
}

/// Below the controls row (preset picker, alignment, size — see
/// `draw_preview_controls_row`), the rest of the tab is the input itself:
/// no separate small text field above a read-only specimen below it.
/// Click anywhere in it to focus `Focus::PreviewText`, then type: what you
/// type renders live, at specimen size/alignment, wrapped to the panel's
/// width, same as the rest of the app's text fields (append/backspace at
/// the end — there's no mid-string cursor positioning anywhere in this
/// app, so the caret is always drawn at the end of the typed text).
fn draw_preview_tab(app: &mut App, text: &mut TextCx, scene: &mut Scene, id: i64, area: Rect) {
    // `preview_size_input` is free text while focused; only a valid,
    // positive number ever actually moves `preview_size`, so a briefly
    // invalid intermediate state (e.g. the field emptied mid-edit) just
    // keeps showing the specimen at whatever size it last had.
    if let Ok(v) = app.preview_size_input.trim().parse::<f64>() {
        if v > 0.0 {
            app.preview_size = v.clamp(4.0, 400.0);
        }
    }

    let controls_row = Rect::new(area.x0, area.y0, area.x1, area.y0 + 30.0);
    let (preset_rect, size_rect) = draw_preview_controls_row(app, text, scene, controls_row);

    let is_focused = app.focus == Focus::PreviewText;
    let content_area = Rect::new(area.x0, controls_row.y1 + 22.0, area.x1, area.y1);
    app.hit_regions.push(HitRegion {
        rect: content_area,
        action: HitAction::FocusField(Focus::PreviewText),
    });
    // `main.rs`'s mouse handlers use this to convert a click/drag point into
    // the editor's own local coordinate space (see `App::preview_content_rect`).
    app.preview_content_rect = Some(content_area);

    // A preset picked from the dropdown gets applied here rather than at
    // the click itself — `ui::handle_click` doesn't have a `TextCx` to
    // drive `preview_editor`'s layout rebuild with, but this does.
    if let Some(preset) = app.pending_preview_preset.take() {
        app.preview_editor.set_text(preset);
    }

    let family_for_preview = app.preview_family(text, id);
    let size = app.preview_size as f32;
    let alignment = match app.preview_align {
        TextAlign::Left => parley::Alignment::Left,
        TextAlign::Center => parley::Alignment::Center,
        TextAlign::Right => parley::Alignment::Right,
        TextAlign::Justify => parley::Alignment::Justify,
    };

    let editor = &mut app.preview_editor;
    editor.set_width(Some(content_area.width() as f32));
    editor.set_alignment(alignment);
    {
        let styles = editor.edit_styles();
        styles.insert(parley::StyleProperty::FontSize(size));
        styles.insert(parley::StyleProperty::Brush(vello::peniko::Brush::Solid(theme::TEXT())));
        // `StyleSet::insert` requires a `'static` `StyleProperty` — `named`
        // itself returns one borrowing `name`, so `.into_owned()` (a small
        // allocation) is what actually makes it storable here.
        styles.insert(match family_for_preview.as_deref() {
            Some(name) => parley::StyleProperty::FontFamily(parley::FontFamily::named(name).into_owned()),
            None => parley::StyleProperty::FontFamily(parley::GenericFamily::SansSerif.into()),
        });
    }
    // Rebuilds the layout if anything above marked it dirty, then drops —
    // everything after this only needs shared (`&self`) access, which
    // `try_layout`/`selection_geometry`/`cursor_geometry` all give.
    text.preview_driver(editor).refresh_layout();

    let is_empty = app.preview_editor.raw_text().is_empty();
    if is_empty && !is_focused {
        text.draw(
            scene,
            "Type to preview\u{2026}",
            size,
            family_for_preview.as_deref(),
            theme::TEXT_TERTIARY(),
            content_area.x0,
            content_area.y0 + size as f64,
        );
        app.detail_tab_scroll_max = 0.0;
        app.detail_tab_scroll_y = 0.0;
    } else if let Some(layout) = app.preview_editor.try_layout() {
        let layout_h = layout.height() as f64;
        app.detail_tab_scroll_max = (layout_h - content_area.height()).max(0.0);
        app.detail_tab_scroll_y = app.detail_tab_scroll_y.clamp(0.0, app.detail_tab_scroll_max);

        let dx = content_area.x0;
        let dy = content_area.y0 - app.detail_tab_scroll_y;
        let transform = Affine::translate((dx, dy));

        scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &content_area);
        // Selection highlight behind the glyphs, one rect per visually
        // selected line — `selection_geometry` already handles a selection
        // spanning multiple wrapped lines correctly.
        for (bbox, _line) in app.preview_editor.selection_geometry() {
            let r = Rect::new(bbox.x0 + dx, bbox.y0 + dy, bbox.x1 + dx, bbox.y1 + dy);
            fill_rect(scene, r, theme::CONTROL_FOCUS().with_alpha(0.35), 2.0);
        }
        crate::text::emit(scene, layout, theme::TEXT(), transform);
        if is_focused {
            if let Some(bbox) = app.preview_editor.cursor_geometry(1.6) {
                let r = Rect::new(bbox.x0 + dx, bbox.y0 + dy, bbox.x1 + dx, bbox.y1 + dy);
                fill_rect(scene, r, theme::BRAND_ACCENT(), 0.0);
            }
        }
        scene.pop_layer();
    }

    // Both dropdowns drawn last so they layer over the specimen text
    // rather than being drawn under it (or clipped by the specimen's own
    // clip layer above).
    if app.preview_preset_open {
        draw_preset_dropdown(app, text, scene, preset_rect, area.x1);
    }
    if app.preview_size_dropdown_open {
        draw_size_dropdown(app, text, scene, size_rect);
    }
}

/// The preset dropdown, alignment segmented control, and size field, left
/// to right in one row. Returns the preset trigger's and size field's own
/// rects, so `draw_preview_tab` can anchor each one's dropdown panel under
/// it without recomputing this same layout.
fn draw_preview_controls_row(app: &mut App, text: &mut TextCx, scene: &mut Scene, row: Rect) -> (Rect, Rect) {
    let gap = 8.0;
    let align_w = 26.0 * 4.0;
    let size_w = 74.0;
    let preset_w = (row.width() - align_w - size_w - gap * 2.0).max(84.0);

    let preset_rect = Rect::new(row.x0, row.y0, row.x0 + preset_w, row.y1);
    let align_rect = Rect::new(preset_rect.x1 + gap, row.y0, preset_rect.x1 + gap + align_w, row.y1);
    let size_rect = Rect::new(align_rect.x1 + gap, row.y0, align_rect.x1 + gap + size_w, row.y1);

    draw_preset_trigger(app, text, scene, preset_rect);
    draw_align_control(app, scene, align_rect);
    draw_size_control(app, text, scene, size_rect);

    (preset_rect, size_rect)
}

fn draw_preset_trigger(app: &mut App, text: &mut TextCx, scene: &mut Scene, rect: Rect) {
    let hovered = rect.contains(app.hover);
    let r = 9.0;
    fill_rect(
        scene,
        rect,
        if hovered || app.preview_preset_open { theme::CONTROL_BG_HOVER() } else { theme::CONTROL_BG() },
        r,
    );
    stroke_rect(scene, rect, theme::CONTROL_BORDER(), r, 1.0);
    // The trigger names *which preset* is active, not the text itself —
    // showing the raw typed text there just wrapped/truncated into
    // something unreadable the moment it stopped being a one-liner.
    let current = app.preview_editor.raw_text();
    let label_source = PREVIEW_PRESETS
        .iter()
        .find(|&&(_, content)| content == current)
        .map_or("Custom", |&(label, _)| label);
    let label = truncate_to_width(text, label_source, 11.0, None, rect.width() - 30.0);
    text.draw_centered_v(scene, &label, 11.0, None, theme::TEXT(), rect.x0 + 10.0, rect.y0, rect.y1);
    draw_updown_chevron(scene, rect.x1 - 14.0, rect.center().y, theme::TEXT_SECONDARY());
    app.hit_regions.push(HitRegion {
        rect,
        action: HitAction::TogglePreviewPresetDropdown,
    });
}

fn draw_preset_dropdown(app: &mut App, text: &mut TextCx, scene: &mut Scene, anchor: Rect, max_x: f64) {
    let row_h = 30.0;
    let h = row_h * PREVIEW_PRESETS.len() as f64 + 8.0;
    let w = 260.0_f64.min((max_x - anchor.x0).max(anchor.width()));
    let panel = Rect::new(anchor.x0, anchor.y1 + 4.0, anchor.x0 + w, anchor.y1 + 4.0 + h);
    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut y = panel.y0 + 4.0;
    for &(label, content) in PREVIEW_PRESETS {
        let row = Rect::new(panel.x0 + 4.0, y, panel.x1 - 4.0, y + row_h);
        if row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 6.0);
        }
        let selected = app.preview_editor.raw_text() == content;
        let fitted = truncate_to_width(text, label, 12.0, None, row.width() - 24.0);
        text.draw_centered_v(
            scene,
            &fitted,
            12.0,
            None,
            if selected { theme::BRAND_ACCENT() } else { theme::TEXT() },
            row.x0 + 12.0,
            row.y0,
            row.y1,
        );
        app.hit_regions.push(HitRegion {
            rect: row,
            action: HitAction::SetPendingPreviewPreset(content),
        });
        y += row_h;
    }
}

fn draw_align_control(app: &mut App, scene: &mut Scene, rect: Rect) {
    let aligns = [TextAlign::Left, TextAlign::Center, TextAlign::Right, TextAlign::Justify];
    let r = 9.0;
    fill_rect(scene, rect, theme::CONTROL_BG(), r);
    stroke_rect(scene, rect, theme::CONTROL_BORDER(), r, 1.0);
    let btn_w = rect.width() / aligns.len() as f64;
    let active_index = aligns.iter().position(|&a| a == app.preview_align);
    for (i, align) in aligns.into_iter().enumerate() {
        let btn = Rect::new(rect.x0 + i as f64 * btn_w, rect.y0, rect.x0 + (i as f64 + 1.0) * btn_w, rect.y1);
        let active = active_index == Some(i);
        if active {
            fill_rect(scene, btn.inset(-2.0), theme::BRAND_ACCENT(), 7.0);
        } else if i > 0 && active_index != Some(i - 1) {
            // A thin divider between two neutral buttons — skipped next to
            // the active one, which already reads as its own block.
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                theme::CONTROL_BORDER(),
                None,
                &Rect::new(btn.x0, btn.y0 + 5.0, btn.x0 + 1.0, btn.y1 - 5.0),
            );
        }
        draw_align_icon(scene, align, btn, if active { theme::CANVAS() } else { theme::TEXT_SECONDARY() });
        app.hit_regions.push(HitRegion {
            rect: btn,
            action: HitAction::SetPreviewAlign(align),
        });
    }
}

/// Four short bars per icon, arranged to read as the alignment they
/// represent: ragged-right for left, ragged-left for right, ragged both
/// sides for center, and uniform full-width bars for justify.
fn draw_align_icon(scene: &mut Scene, align: TextAlign, rect: Rect, color: Color) {
    let icon_w = 12.0;
    let bar_h = 1.4;
    let gap = 2.4;
    let widths = [icon_w, icon_w * 0.65, icon_w, icon_w * 0.45];
    let total_h = widths.len() as f64 * bar_h + (widths.len() as f64 - 1.0) * gap;
    let x0 = rect.center().x - icon_w / 2.0;
    let mut y = rect.center().y - total_h / 2.0;
    for w in widths {
        let (bx0, bw) = match align {
            TextAlign::Left => (x0, w),
            TextAlign::Right => (x0 + (icon_w - w), w),
            TextAlign::Center => (x0 + (icon_w - w) / 2.0, w),
            TextAlign::Justify => (x0, icon_w),
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &Rect::new(bx0, y, bx0 + bw, y + bar_h));
        y += bar_h + gap;
    }
}

/// One shared outer container (not two separate boxes) — the number sits
/// as bare text with no box of its own, and the dropdown chevron is a
/// smaller filled button inset *within* that shared container.
fn draw_size_control(app: &mut App, text: &mut TextCx, scene: &mut Scene, rect: Rect) {
    let focused = app.focus == Focus::PreviewSize;
    let r = 9.0;
    fill_rect(scene, rect, theme::CONTROL_BG(), r);
    stroke_rect(
        scene,
        rect,
        if focused { theme::CONTROL_FOCUS() } else { theme::CONTROL_BORDER() },
        r,
        if focused { 1.5 } else { 1.0 },
    );

    let display = if focused { app.preview_size_input.clone() } else { (app.preview_size as i64).to_string() };
    text.draw_centered_v(scene, &display, 11.0, None, theme::TEXT(), rect.x0 + 10.0, rect.y0, rect.y1);
    if focused {
        let caret_x = rect.x0 + 10.0 + text.measure(&display, 11.0, None);
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::BRAND_ACCENT(),
            None,
            &Rect::new(caret_x + 1.0, rect.y0 + 6.0, caret_x + 2.0, rect.y1 - 6.0),
        );
    }

    let inset = 3.0;
    let chevron_rect = Rect::new(rect.x1 - 24.0 - inset, rect.y0 + inset, rect.x1 - inset, rect.y1 - inset);
    let chevron_hovered = chevron_rect.contains(app.hover);
    fill_rect(
        scene,
        chevron_rect,
        if chevron_hovered || app.preview_size_dropdown_open { theme::CONTROL_HOVER_SUBTLE() } else { theme::CONTROL_BG_HOVER() },
        6.0,
    );
    draw_chevron_down(scene, chevron_rect.center().x, chevron_rect.center().y, theme::TEXT_SECONDARY());

    let number_part = Rect::new(rect.x0, rect.y0, chevron_rect.x0, rect.y1);
    app.hit_regions.push(HitRegion {
        rect: number_part,
        action: HitAction::FocusField(Focus::PreviewSize),
    });
    app.hit_regions.push(HitRegion {
        rect: chevron_rect,
        action: HitAction::TogglePreviewSizeDropdown,
    });
}

fn draw_size_dropdown(app: &mut App, text: &mut TextCx, scene: &mut Scene, anchor: Rect) {
    let row_h = 26.0;
    let w = 84.0;
    let h = row_h * PREVIEW_SIZE_STEPS.len() as f64 + 8.0;
    let panel = Rect::new(anchor.x1 - w, anchor.y1 + 4.0, anchor.x1, anchor.y1 + 4.0 + h);
    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut y = panel.y0 + 4.0;
    for &size in PREVIEW_SIZE_STEPS {
        let row = Rect::new(panel.x0 + 4.0, y, panel.x1 - 4.0, y + row_h);
        if row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 6.0);
        }
        let selected = (app.preview_size - size).abs() < 0.01;
        let label = format!("{} pt", size as i64);
        text.draw_centered_v(
            scene,
            &label,
            12.0,
            None,
            if selected { theme::BRAND_ACCENT() } else { theme::TEXT() },
            row.x0 + 12.0,
            row.y0,
            row.y1,
        );
        app.hit_regions.push(HitRegion {
            rect: row,
            action: HitAction::SetPreviewSize(size),
        });
        y += row_h;
    }
}

fn draw_chevron_down(scene: &mut Scene, cx: f64, cy: f64, color: Color) {
    let mut path = BezPath::new();
    path.move_to(Point::new(cx - 4.0, cy - 2.0));
    path.line_to(Point::new(cx, cy + 2.5));
    path.line_to(Point::new(cx + 4.0, cy - 2.0));
    scene.stroke(&vello::kurbo::Stroke::new(1.4), Affine::IDENTITY, color, None, &path);
}

/// A stacked up/down caret pair — the preset dropdown's own trigger icon,
/// distinct from the plain single down-chevron the size dropdown uses.
fn draw_updown_chevron(scene: &mut Scene, cx: f64, cy: f64, color: Color) {
    let stroke = vello::kurbo::Stroke::new(1.4);
    let mut up = BezPath::new();
    up.move_to(Point::new(cx - 4.0, cy - 1.0));
    up.line_to(Point::new(cx, cy - 4.5));
    up.line_to(Point::new(cx + 4.0, cy - 1.0));
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &up);

    let mut down = BezPath::new();
    down.move_to(Point::new(cx - 4.0, cy + 1.0));
    down.line_to(Point::new(cx, cy + 4.5));
    down.line_to(Point::new(cx + 4.0, cy + 1.0));
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &down);
}

// Hand-rolled wrap/justify/caret logic (`draw_preview_specimen`,
// `draw_text_caret`) used to live here — replaced by `App::preview_editor`
// (a real `parley::PlainEditor`), which does its own line-breaking,
// alignment, and cursor/selection geometry. See `draw_preview_tab`.

fn draw_sizes_tab(app: &mut App, text: &mut TextCx, scene: &mut Scene, id: i64, area: Rect) {
    let family = app.preview_family(text, id);

    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &area);
    let mut y = area.y0 - app.detail_tab_scroll_y;
    let mut total_h = 0.0;
    for &size in SIZES_LADDER {
        let label = format!("{} pt", size as i64);
        // Just enough for the "N pt" label's own line (10.5pt) to clear
        // before the specimen starts — not `+ size`, which pushed the
        // specimen a full point-size further down than it needed to be,
        // the main reason rows read as sparse rather than the dense,
        // catalog-style stack this is meant to match.
        let label_h = 20.0;
        // The specimen's real rendered height, not a guessed multiple of
        // `size` — actual line-height (ascent+descent+leading) varies per
        // font, and a too-small guess here meant a row's specimen bled
        // down into the *next* row's label instead of stopping within its
        // own row, to the point that what looked like "font N's label,
        // paired with a blank/wrong specimen" was really the *previous*
        // row's oversized specimen overlapping it.
        let run = fit_run(text, ALPHABET, size as f32, family.as_deref(), area.width());
        let specimen_h = text.measure_height(&run, size as f32, family.as_deref());
        let row_h = label_h + specimen_h + 6.0;
        if y + row_h >= area.y0 && y <= area.y1 {
            text.draw(scene, &label, 10.5, None, theme::TEXT_TERTIARY(), area.x0, y + 10.0);
            text.draw(scene, &run, size as f32, family.as_deref(), theme::TEXT(), area.x0, y + label_h);
        }
        y += row_h;
        total_h += row_h;
    }
    scene.pop_layer();

    app.detail_tab_scroll_max = (total_h - area.height()).max(0.0);
    app.detail_tab_scroll_y = app.detail_tab_scroll_y.clamp(0.0, app.detail_tab_scroll_max);
}

/// How much of `text_content` (assumed all-caps, no spaces) fits within
/// `max_w` at `size` — used by the Sizes tab so each row shows as much of
/// the alphabet as its point size allows, rather than a fixed substring
/// that clips at large sizes or wastes space at small ones.
fn fit_run(text: &mut TextCx, content: &str, size: f32, family: Option<&str>, max_w: f64) -> String {
    let mut run = String::new();
    for ch in content.chars() {
        let candidate = format!("{run}{ch}");
        if text.measure(&candidate, size, family) > max_w && !run.is_empty() {
            break;
        }
        run = candidate;
    }
    run
}

fn draw_glyphs_tab(app: &mut App, text: &mut TextCx, scene: &mut Scene, id: i64, area: Rect) {
    let Some(info) = app.font_info(id) else {
        text.draw(scene, "Couldn't read this font's glyph table.", 12.5, None, theme::TEXT_TERTIARY(), area.x0, area.y0 + 16.0);
        return;
    };
    let codepoints = info.codepoints.clone();
    let family = app.preview_family(text, id);

    // Header row: block filter dropdown + search field.
    let header_h = 30.0;
    let dropdown_w = 150.0;
    let dropdown_rect = Rect::new(area.x0, area.y0, area.x0 + dropdown_w, area.y0 + header_h);
    let filtered_count = codepoints
        .iter()
        .filter(|&&cp| app.glyph_block_filter.is_none_or(|b| crate::font_info::block_for(cp) == b))
        .count();
    let dropdown_label = match app.glyph_block_filter {
        Some(block) => format!("{block} ({filtered_count})"),
        None => format!("All ({filtered_count})"),
    };
    fill_rect(scene, dropdown_rect, theme::CONTROL_BG(), 8.0);
    stroke_rect(scene, dropdown_rect, theme::CONTROL_BORDER(), 8.0, 1.0);
    text.draw_centered_v(scene, &dropdown_label, 11.5, None, theme::TEXT(), dropdown_rect.x0 + 10.0, dropdown_rect.y0, dropdown_rect.y1);
    app.hit_regions.push(HitRegion {
        rect: dropdown_rect,
        action: HitAction::ToggleGlyphBlockDropdown,
    });

    let search_rect = Rect::new(dropdown_rect.x1 + 10.0, area.y0, area.x1, area.y0 + header_h);
    fill_rect(scene, search_rect, theme::CONTROL_BG(), 8.0);
    let search_focused = app.focus == Focus::GlyphSearch;
    stroke_rect(
        scene,
        search_rect,
        if search_focused { theme::CONTROL_FOCUS() } else { theme::CONTROL_BORDER() },
        8.0,
        if search_focused { 1.4 } else { 1.0 },
    );
    if app.glyph_search.is_empty() && !search_focused {
        text.draw(scene, "Search", 11.5, None, theme::TEXT_TERTIARY(), search_rect.x0 + 10.0, search_rect.y0 + 19.0);
    } else {
        text.draw(scene, &app.glyph_search, 11.5, None, theme::TEXT(), search_rect.x0 + 10.0, search_rect.y0 + 19.0);
    }
    app.hit_regions.push(HitRegion {
        rect: search_rect,
        action: HitAction::FocusField(Focus::GlyphSearch),
    });

    // The dropdown's own options list, drawn last so it layers over the grid.
    if app.glyph_block_dropdown_open {
        draw_glyph_block_dropdown(app, text, scene, dropdown_rect, &codepoints);
    }

    // The grid itself.
    let grid_area = Rect::new(area.x0, area.y0 + header_h + 12.0, area.x1, area.y1);
    let search_filter = app.glyph_search.chars().next();
    let visible: Vec<u32> = codepoints
        .into_iter()
        .filter(|&cp| app.glyph_block_filter.is_none_or(|b| crate::font_info::block_for(cp) == b))
        .filter(|&cp| search_filter.is_none_or(|c| char::from_u32(cp) == Some(c)))
        .collect();

    let cell = 42.0;
    let gap = 6.0;
    let cols = ((grid_area.width() + gap) / (cell + gap)).floor().max(1.0) as usize;
    let rows = visible.len().div_ceil(cols);
    let content_h = rows as f64 * (cell + gap);

    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &grid_area);
    let start_row = (app.detail_tab_scroll_y / (cell + gap)).floor().max(0.0) as usize;
    let visible_rows = (grid_area.height() / (cell + gap)).ceil() as usize + 2;
    let end_row = (start_row + visible_rows).min(rows);

    for row in start_row..end_row {
        let y = grid_area.y0 + row as f64 * (cell + gap) - app.detail_tab_scroll_y;
        for col in 0..cols {
            let idx = row * cols + col;
            let Some(&cp) = visible.get(idx) else { break };
            let x = grid_area.x0 + col as f64 * (cell + gap);
            let rect = Rect::new(x, y, x + cell, y + cell);
            if rect.contains(app.hover) {
                fill_rect(scene, rect, theme::CONTROL_BG_HOVER(), 6.0);
            } else {
                fill_rect(scene, rect, theme::CONTROL_BG(), 6.0);
            }
            if let Some(ch) = char::from_u32(cp) {
                let glyph = ch.to_string();
                let gw = text.measure(&glyph, 17.0, family.as_deref());
                text.draw_centered_v(
                    scene,
                    &glyph,
                    17.0,
                    family.as_deref(),
                    theme::TEXT(),
                    rect.x0 + (rect.width() - gw) / 2.0,
                    rect.y0,
                    rect.y1,
                );
            }
        }
    }
    scene.pop_layer();

    app.detail_tab_scroll_max = (content_h - grid_area.height()).max(0.0);
    app.detail_tab_scroll_y = app.detail_tab_scroll_y.clamp(0.0, app.detail_tab_scroll_max);
}

fn draw_glyph_block_dropdown(app: &mut App, text: &mut TextCx, scene: &mut Scene, anchor: Rect, codepoints: &[u32]) {
    let mut counts: Vec<(Option<&'static str>, usize)> = vec![(None, codepoints.len())];
    for &(block, ..) in crate::font_info::UNICODE_BLOCKS {
        let n = codepoints.iter().filter(|&&cp| crate::font_info::block_for(cp) == block).count();
        if n > 0 {
            counts.push((Some(block), n));
        }
    }
    let other_n = codepoints.iter().filter(|&&cp| crate::font_info::block_for(cp) == crate::font_info::OTHER_BLOCK).count();
    if other_n > 0 {
        counts.push((Some(crate::font_info::OTHER_BLOCK), other_n));
    }

    let row_h = 26.0;
    let w = 220.0;
    let h = row_h * counts.len() as f64 + 8.0;
    let panel = Rect::new(anchor.x0, anchor.y1 + 4.0, anchor.x0 + w, anchor.y1 + 4.0 + h);
    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut y = panel.y0 + 4.0;
    for (block, count) in counts {
        let row = Rect::new(panel.x0 + 4.0, y, panel.x1 - 4.0, y + row_h);
        if row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 6.0);
        }
        let selected = app.glyph_block_filter == block;
        let label = match block {
            Some(name) => format!("{name} ({count})"),
            None => format!("All ({count})"),
        };
        text.draw_centered_v(
            scene,
            &label,
            12.0,
            None,
            if selected { theme::BRAND_ACCENT() } else { theme::TEXT() },
            row.x0 + 12.0,
            row.y0,
            row.y1,
        );
        app.hit_regions.push(HitRegion {
            rect: row,
            action: HitAction::SetGlyphBlockFilter(block),
        });
        y += row_h;
    }
}

fn draw_info_tab(app: &mut App, text: &mut TextCx, scene: &mut Scene, id: i64, area: Rect) {
    let path = app.entry(id).map(|e| e.path.clone()).unwrap_or_default();
    let Some(info) = app.font_info(id) else {
        text.draw(scene, "Couldn't read this font's metadata.", 12.5, None, theme::TEXT_TERTIARY(), area.x0, area.y0 + 16.0);
        return;
    };

    let rows: Vec<(&str, String)> = vec![
        ("Name", info.full_name.clone()),
        ("Family", info.family.clone()),
        ("Style", info.style.clone()),
        ("Format", info.format.to_string()),
        ("PostScript Name", info.postscript_name.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Unique Name", info.unique_name.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Location", path.clone()),
        ("Variable Font", if info.is_variable { "Yes".to_string() } else { "No".to_string() }),
        ("Version", info.version.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Glyph Count", info.glyph_count.to_string()),
        ("License", info.license_description.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Copyright", info.copyright.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Trademark", info.trademark.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Designer", info.designer.clone().unwrap_or_else(|| "\u{2013}".to_string())),
        ("Manufacturer", info.manufacturer.clone().unwrap_or_else(|| "\u{2013}".to_string())),
    ];

    scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &area);
    let mut y = area.y0 - app.detail_tab_scroll_y;
    let mut total_h = 0.0;
    for (label, value) in &rows {
        let label_h = 15.0;
        let value_h = wrapped_height(text, value, 12.5, area.width());
        let row_h = label_h + value_h + 16.0;
        if y + row_h >= area.y0 && y <= area.y1 {
            text.draw(scene, label, 11.0, None, theme::TEXT_TERTIARY(), area.x0, y + 10.0);
            let value_area = Rect::new(area.x0, y + label_h + 8.0, area.x1, y + row_h);
            if *label == "Location" {
                text.draw(scene, value, 12.5, None, theme::TEXT_SECONDARY(), value_area.x0, value_area.y0 + 12.0);
                app.hit_regions.push(HitRegion {
                    rect: Rect::new(value_area.x0, value_area.y0 - 4.0, area.x1, value_area.y0 + 16.0),
                    action: HitAction::RevealSelectedFont,
                });
            } else {
                draw_wrapped(text, scene, value, None, 12.5, 1.3, value_area, theme::TEXT_SECONDARY());
            }
        }
        y += row_h;
        total_h += row_h;
    }
    scene.pop_layer();

    app.detail_tab_scroll_max = (total_h - area.height()).max(0.0);
    app.detail_tab_scroll_y = app.detail_tab_scroll_y.clamp(0.0, app.detail_tab_scroll_max);
}

/// How tall `content` would need to draw at `size` wrapped to `max_w` —
/// `draw_wrapped`'s own line-breaking logic, run once just to measure.
fn wrapped_height(text: &mut TextCx, content: &str, size: f32, max_w: f64) -> f64 {
    if content.is_empty() {
        return size as f64 * 1.3;
    }
    let line_h = size as f64 * 1.3;
    let mut line = String::new();
    let mut lines = 1;
    for word in content.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text.measure(&candidate, size, None) > max_w && !line.is_empty() {
            lines += 1;
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    lines as f64 * line_h
}

/// Word-wrap at a fixed size — good enough for a preview pane / metadata
/// value where exact line-breaking doesn't need to match a real text
/// engine's rules.
/// Returns where the *next* character would start — the end of the last
/// line actually drawn — so a caller drawing a live-typed caret (see
/// `draw_preview_tab`) knows where to put it without redoing this same
/// line-breaking pass itself.
#[allow(clippy::too_many_arguments)]
fn draw_wrapped(
    text: &mut TextCx,
    scene: &mut Scene,
    content: &str,
    family: Option<&str>,
    size: f32,
    line_height_mult: f64,
    area: Rect,
    color: vello::peniko::Color,
) -> (f64, f64) {
    if content.is_empty() {
        return (area.x0, area.y0 + size as f64);
    }
    let line_h = size as f64 * line_height_mult;
    let max_w = area.width();

    let mut line = String::new();
    let mut y = area.y0 + size as f64;
    for word in content.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text.measure(&candidate, size, family) > max_w && !line.is_empty() {
            text.draw(scene, &line, size, family, color, area.x0, y);
            y += line_h;
            line = word.to_string();
        } else {
            line = candidate;
        }
        if y > area.y1 {
            return (area.x0 + text.measure(&line, size, family), y);
        }
    }
    if !line.is_empty() && y <= area.y1 {
        text.draw(scene, &line, size, family, color, area.x0, y);
    }
    (area.x0 + text.measure(&line, size, family), y)
}
