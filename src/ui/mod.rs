mod detail;
mod grid;
mod list;
mod settings;
mod sidebar;

use vello::kurbo::{Affine, Arc, BezPath, Circle, Line, Point, Rect, Stroke, Vec2};
use vello::peniko::Fill;
use vello::Scene;

use crate::app::{App, Filter, Focus, ViewMode};
use crate::settings::SettingToggle;
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
    ToggleDetailFavorite,
    ToggleDetailPanel,
    StartTileSizeDrag,
    StartScrollDrag,
    /// Mousedown on the detail panel's own left-edge resize handle — see
    /// `App::detail_w`/`update_detail_resize_from_point`.
    StartDetailResize,
    ToggleSamplePicker,
    SetSampleText(String),
    ClearSampleText,
    /// The Preview tab's quick-preset dropdown (see `ui::detail`).
    TogglePreviewPresetDropdown,
    /// Records the chosen preset for `ui::detail::draw_preview_tab` to
    /// actually apply next frame — `handle_click` has no `TextCx` to drive
    /// `App::preview_editor`'s layout rebuild with, but that function does.
    SetPendingPreviewPreset(&'static str),
    SetPreviewAlign(crate::app::TextAlign),
    TogglePreviewSizeDropdown,
    SetPreviewSize(f64),
    AssignDrop(i64),
    CancelDrop,
    /// Mousedown on a list-view column header's label area — not yet a
    /// sort (that only fires if the pointer never moves past a small
    /// threshold before release) or a reorder (that fires if it does).
    /// See `App::header_press`.
    PressColumnHeader(usize),
    /// Mousedown on a column header's right-edge resize handle — always a
    /// resize, no click-vs-drag ambiguity to resolve.
    StartColumnResize(usize),
    ToggleFamilyExpanded(String),
    ConfirmDelete(i64),
    CancelDelete,
    /// Hand-rolled Windows menu bar (see `draw_windows_menu_bar`) — opens
    /// the clicked top-level menu's dropdown, or closes it if it's the one
    /// already open.
    OpenWinMenu(crate::app::WinMenuKind),
    ActivateSelected,
    ActivateSelectedTemporarily,
    DeactivateSelected,
    ExportSelectedFont,
    RevealSelectedFont,
    RemoveSelectedFromFontlist,
    RequestDeleteSelected,
    ZoomIn,
    ZoomOut,
    OpenGitHub,
    /// Detail panel's tab bar — see `crate::app::DetailTab`.
    SetDetailTab(crate::app::DetailTab),
    ToggleGlyphBlockDropdown,
    /// `None` = "All" (no filter).
    SetGlyphBlockFilter(Option<&'static str>),
    /// Opens the Settings overlay — see `ui::settings`. Reachable from the
    /// hand-rolled Windows File menu; macOS's native app-menu "Settings…"
    /// item goes through `native_menu::MenuAction::OpenSettings` instead,
    /// set directly rather than via a `HitRegion` click.
    OpenSettings,
    /// The close button, or a click on the dimmed backdrop outside the
    /// panel — see `ui::settings::draw`.
    CloseSettings,
    ToggleSetting(SettingToggle),
    ToggleAppearanceDropdown,
    SetAppearance(crate::settings::Appearance),
    /// A font tile/row's right-click menu — opens/closes the Tags submenu.
    ToggleTagsSubmenu,
    /// Toggles `tag_id` on `font_id` (the font context menu's own
    /// `font_id`, not necessarily `App::selected`, though they're the same
    /// by the time this menu is open — see `handle_right_click`).
    ToggleFontTag(i64, i64),
    /// Switches the Tags submenu's "New Tag…" row into its inline text
    /// field, focused and ready to type.
    OpenNewTagField,
    /// The update banner's own close button.
    DismissUpdateBanner,
    /// The update banner's "Download" button.
    OpenUpdateDownload,
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

/// A font tile/row's right-click menu (Activate/Star/Tags/Export/Reveal/
/// Delete) — see `ui::grid`/`ui::list`'s right-click wiring and
/// `draw_font_context_menu`.
pub struct FontContextMenu {
    pub font_id: i64,
    pub anchor: Point,
    /// Same empty-until-first-draw pattern as `ContextMenu::panel_rect`.
    pub panel_rect: Rect,
    /// Whether the Tags submenu (opened from the main panel's "Tags" row)
    /// is showing.
    pub tags_open: bool,
    pub tags_panel_rect: Rect,
    /// `Some(buffer)` while the Tags submenu's "New Tag…" row has been
    /// switched into its inline text field — `None` otherwise, including
    /// before it's ever been opened.
    pub new_tag_text: Option<String>,
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

/// The OpenType/TrueType format badge — the real center marks cropped out
/// of `branding/SVG/otf.svg`/`ttf.svg` (see `branding::draw_otf_mark`/
/// `draw_ttf_mark`), not hand-drawn or text-rendered. Sits between the
/// activation pip and the font name in both grid tiles and list rows
/// (`ui::grid`/`ui::list`). Draws nothing for a format this doesn't
/// recognize (`crate::font::format_label`'s "Unknown"), and returns the
/// width actually used (`0.0` in that case) so the caller knows how much
/// to shift the name text over by.
pub fn draw_format_badge(scene: &mut Scene, format: &str, x: f64, y0: f64, y1: f64, color: vello::peniko::Color) -> f64 {
    let h = (y1 - y0).min(10.5);
    let target = Rect::new(x, (y0 + y1) / 2.0 - h / 2.0, x + h * 1.3, (y0 + y1) / 2.0 + h / 2.0);
    if format.starts_with("OpenType") {
        crate::branding::draw_otf_mark(scene, target, color)
    } else if format.starts_with("TrueType") {
        crate::branding::draw_ttf_mark(scene, target, color)
    } else {
        0.0
    }
}

/// `content` as-is if it already fits `max_w`, otherwise trimmed from the
/// end and suffixed with an ellipsis until it does — a long family name
/// (real ones run surprisingly long: "FONTSPRING DEMO - PODIUM Sharp 2.1")
/// would otherwise just draw straight past a tile's or row's own right
/// edge instead of stopping there.
pub fn truncate_to_width(text: &mut TextCx, content: &str, size: f32, family: Option<&str>, max_w: f64) -> String {
    if max_w <= 0.0 || text.measure(content, size, family) <= max_w {
        return content.to_string();
    }
    const ELLIPSIS: &str = "\u{2026}";
    let ellipsis_w = text.measure(ELLIPSIS, size, family);
    if ellipsis_w > max_w {
        return String::new();
    }
    let budget = max_w - ellipsis_w;
    let chars: Vec<char> = content.chars().collect();
    // Binary search for the longest fitting prefix — `measure`'s width is
    // monotonic in prefix length, so this needs `O(log n)` calls to it
    // instead of trying every length from the end down one at a time.
    // That distinction matters a lot in practice: `measure` isn't a cheap
    // string-length check, it's a full font-selection-and-shape pass, and
    // this runs for every over-long tile/row name, every frame — at a
    // real library's scale (100k+ fonts, many with long or CJK names) the
    // difference between `O(log n)` and `O(n)` here alone was measured to
    // be the single biggest contributor to this app struggling to scroll.
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = lo + (hi - lo + 1) / 2;
        let candidate: String = chars[..mid].iter().collect();
        if text.measure(&candidate, size, family) <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    if lo == 0 {
        return ELLIPSIS.to_string();
    }
    let prefix: String = chars[..lo].iter().collect();
    format!("{prefix}{ELLIPSIS}")
}

/// A five-pointed star, outlined when `filled` is false and solid when
/// true — drawn as a real vector path rather than the Unicode "★"/"☆"
/// glyphs, which sit at whatever vertical offset the current font's own
/// metrics happen to give them and don't actually line up with the rest
/// of a row (badge icon, activation dot, text) the way a hand-drawn icon
/// centered on `(cx, cy)` does.
pub fn draw_star_icon(scene: &mut Scene, cx: f64, cy: f64, r: f64, color: vello::peniko::Color, filled: bool) {
    // A stroked outline reads optically larger than a filled shape at the
    // same nominal radius — the stroke sits half outside the path, and an
    // open outline just has more visible edge than a solid one of the same
    // size. Shrinking the outline star's own radius is what actually makes
    // the two look the same size next to each other, rather than matching
    // on paper.
    let r = if filled { r } else { r * 0.82 };
    let inner = r * 0.42;
    let mut path = BezPath::new();
    for i in 0..10 {
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
        let radius = if i % 2 == 0 { r } else { inner };
        let point = Point::new(cx + radius * angle.cos(), cy + radius * angle.sin());
        if i == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    path.close_path();
    if filled {
        scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &path);
    } else {
        scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, color, None, &path);
    }
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

/// A self-contained circled-"i" glyph (its own ring, plus a dot and a
/// rounded stem inside it) — like the SF Symbol `info.circle`, not a bare
/// dot-and-stem relying on the button container's own ring for the circle
/// part. Sized smaller than the button itself so the two rings read as
/// "icon sitting inside a button," not one ring doing double duty.
fn draw_info_icon(scene: &mut Scene, cx: f64, cy: f64, color: vello::peniko::Color) {
    let ring_r = 9.0;
    scene.stroke(
        &Stroke::new(1.3),
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(Point::new(cx, cy), ring_r),
    );

    let dot_r = 1.5;
    let stem_w = 2.2;
    let stem_h = 7.4;
    let gap = 1.6;
    let total_h = dot_r * 2.0 + gap + stem_h;
    let top = cy - total_h / 2.0;

    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        color,
        None,
        &Circle::new(Point::new(cx, top + dot_r), dot_r),
    );
    let stem_top = top + dot_r * 2.0 + gap;
    fill_rect(
        scene,
        Rect::new(cx - stem_w / 2.0, stem_top, cx + stem_w / 2.0, stem_top + stem_h),
        color,
        stem_w / 2.0,
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
fn content_x_range(width: f64, detail_open: bool, detail_w: f64) -> (f64, f64) {
    let x0 = theme::SIDEBAR_W + theme::FRAME_PAD;
    let x1 = if detail_open { width - detail_w } else { width };
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
        theme::CANVAS(),
        0.0,
    );

    sidebar::draw(app, text, &mut scene, height);

    let detail_open = app.detail_open && app.selected.is_some();
    app.detail_w = app.detail_w.clamp(theme::DETAIL_W, app.max_detail_w(width));
    let (content_x0, content_x1) = content_x_range(width, detail_open, app.detail_w);

    // Full window width, not just the grid/list portion — otherwise a
    // wide detail panel (see `App::detail_w`) squeezes the toolbar's own
    // controls (the search field especially) into whatever's left, to the
    // point they'd start overlapping each other. The detail panel sits
    // *below* this shared toolbar (`content_y0`), not beside it.
    draw_toolbar(app, text, &mut scene, content_x0, width);

    let content_y0 = theme::TOOLBAR_H;
    match app.view_mode {
        ViewMode::Grid => grid::draw_grid(
            app, text, &mut scene, content_x0, content_x1, content_y0, height,
        ),
        ViewMode::List => list::draw_list(
            app, text, &mut scene, content_x0, content_x1, content_y0, height,
        ),
    }

    if detail_open {
        detail::draw(
            app,
            text,
            &mut scene,
            width - app.detail_w,
            width,
            content_y0,
            height,
        );
    }

    if app.sample_picker_open {
        draw_sample_picker(app, text, &mut scene, width, height);
    }

    // A file is being dragged over the window but not yet dropped —
    // distinct from `drop_flash_until` below, which only starts once
    // `WindowEvent::DroppedFile` actually fires.
    if app.drag_hovering {
        let progress = app
            .drag_hover_since
            .map(|since| since.elapsed().as_secs_f64() / DRAG_HOVER_FAN_SECS)
            .unwrap_or(1.0);
        draw_drag_hover(text, &mut scene, content_x0, content_x1, content_y0, height, progress);
    }

    // The flash is the "animation to signify they're adding a font";
    // once it expires the library picker takes over — one dropped file
    // drives both in sequence, no separate state machine needed.
    if let Some(flash_until) = app.drop_flash_until {
        if std::time::Instant::now() < flash_until {
            draw_drop_flash(&mut scene, text, width, height);
        } else {
            app.drop_flash_until = None;
        }
    }
    if app.drop_flash_until.is_none() && app.pending_drop.is_some() {
        draw_drop_picker(app, text, &mut scene, width, height);
    }

    draw_context_menu(app, text, &mut scene);
    draw_font_context_menu(app, text, &mut scene);
    #[cfg(target_os = "windows")]
    draw_windows_menu_dropdown(app, text, &mut scene, width);

    if app.settings_open {
        settings::draw(app, text, &mut scene, width, height);
    }

    draw_update_banner(app, text, &mut scene, width, height);

    // Drawn last: a real file delete is the one truly irreversible action
    // in this app, so its confirmation sits above every other overlay.
    if app.confirm_delete.is_some() {
        draw_delete_confirm(app, text, &mut scene, width, height);
    }

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

    fill_rect(scene, card, theme::SIDEBAR_BG(), 18.0);
    stroke_rect(scene, card, theme::CONTROL_BORDER(), 18.0, 1.0);

    // The sample text itself — an editable field, not just a display, so
    // typing here directly changes what every tile previews live.
    let field_rect = Rect::new(card.x0 + 16.0, card.y0 + 10.0, card.x1 - 40.0, card.y0 + 46.0);
    if app.focus == Focus::SampleText {
        stroke_rect(scene, field_rect.inset(4.0), theme::CONTROL_FOCUS(), 8.0, 1.5);
    }
    let is_placeholder = app.sample_text.trim().is_empty();
    let display_text = app.effective_sample_text().to_string();
    text.draw_centered_v(
        scene,
        &display_text,
        26.0,
        None,
        if is_placeholder { theme::TEXT_TERTIARY() } else { theme::TEXT() },
        field_rect.x0,
        field_rect.y0,
        field_rect.y1,
    );
    if app.focus == Focus::SampleText {
        let caret_x = field_rect.x0 + text.measure(&display_text, 26.0, None) + 3.0;
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::BRAND_ACCENT(),
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
        if close_hovered { theme::TEXT() } else { theme::TEXT_TERTIARY() },
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
        if close_hovered { theme::TEXT() } else { theme::TEXT_TERTIARY() },
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
            theme::NAV_SELECTED_BG()
        } else if hovered {
            theme::CONTROL_HOVER_SUBTLE()
        } else {
            theme::CONTROL_BG()
        };
        fill_rect(scene, r, bg, chip_h / 2.0);
        let tw = text.measure(label, 12.5, None);
        text.draw_centered_v(
            scene,
            label,
            12.5,
            None,
            if active { theme::TEXT() } else { theme::TEXT_SECONDARY() },
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

/// How long the cards take to fan out (and the glow to ease in) once a
/// drag-hover starts — see `draw_drag_hover`'s `progress` parameter.
/// `main.rs`'s `about_to_wait` also reads this to know how long to keep
/// forcing redraws for.
pub const DRAG_HOVER_FAN_SECS: f64 = 0.28;

/// Shown for as long as a file is being dragged over the content area but
/// hasn't been dropped yet (`WindowEvent::HoveredFile`) — a dashed brand-
/// lime border plus the `.otf`/`.ttf` file-card motif from
/// `branding/SVG/otf.svg`+`ttf.svg`, animating from stacked-and-square to
/// fanned like a dealt hand of cards. `progress` is `0.0` the instant the
/// hover starts, `1.0` once fully settled — see `App::drag_hover_since`.
fn draw_drag_hover(
    text: &mut TextCx,
    scene: &mut Scene,
    x0: f64,
    x1: f64,
    y0: f64,
    height: f64,
    progress: f64,
) {
    // Ease-out cubic: fast start, settles gently rather than snapping.
    let eased = 1.0 - (1.0 - progress.clamp(0.0, 1.0)).powi(3);

    let rect = Rect::new(x0 + 4.0, y0 + 4.0, x1 - 4.0, height - 4.0);
    let dashed = Stroke::new(2.5).with_dashes(0.0, [9.0, 7.0]);
    scene.stroke(&dashed, Affine::IDENTITY, theme::BRAND_ACCENT(), None, &rect.to_rounded_rect(14.0));

    let cx = (x0 + x1) / 2.0;
    let cy = y0 + (height - y0) / 2.0;

    // A soft glow behind the cards: a few concentric, increasingly
    // transparent lime circles rather than a true radial-gradient brush —
    // simple, and plenty convincing for a brief hover overlay. Eases in
    // alongside the cards rather than appearing instantly at full strength.
    for (radius, alpha) in [(120.0, 0x0cu8), (85.0, 0x16), (55.0, 0x22)] {
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            vello::peniko::Color::from_rgba8(0x9a, 0xff, 0x00, (alpha as f64 * eased) as u8),
            None,
            &Circle::new(Point::new(cx, cy), radius),
        );
    }

    let spread = 22.0 * eased;
    let angle = 8.0 * eased;
    crate::branding::draw_otf_card(scene, Point::new(cx - spread, cy), 92.0, -angle, theme::CANVAS());
    crate::branding::draw_ttf_card(scene, Point::new(cx + spread, cy), 92.0, angle, theme::CANVAS());

    let caption = "Drop to add a font";
    let tw = text.measure(caption, 14.0, None);
    text.draw(scene, caption, 14.0, None, theme::TEXT_SECONDARY(), cx - tw / 2.0, cy + 80.0);
}

/// A dismissible card, bottom-right corner — drawn only once
/// `App::update_available` (see `check_for_update`) has an answer and the
/// user hasn't already closed it this session. No auto-download/install:
/// the button just opens the releases page in the system browser.
fn draw_update_banner(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    if app.update_dismissed {
        return;
    }
    let Some(version) = app.update_available.clone() else {
        return;
    };

    let pad = 16.0;
    let card_w = 260.0;
    let card_h = 108.0;
    let card = Rect::new(width - 20.0 - card_w, height - 20.0 - card_h, width - 20.0, height - 20.0);

    fill_rect(scene, card, theme::SIDEBAR_BG(), 14.0);
    stroke_rect(scene, card, theme::CONTROL_BORDER(), 14.0, 1.0);

    text.draw(scene, "Update available", 13.0, None, theme::TEXT(), card.x0 + pad, card.y0 + 26.0);
    let subtitle = format!("Version {version} is ready");
    text.draw(scene, &subtitle, 11.5, None, theme::TEXT_SECONDARY(), card.x0 + pad, card.y0 + 46.0);

    let close_rect = Rect::new(card.x1 - 30.0, card.y0 + 4.0, card.x1 - 6.0, card.y0 + 28.0);
    if close_rect.contains(app.hover) {
        fill_rect(scene, close_rect, theme::NAV_HOVER_BG(), 6.0);
    }
    let (ccx, ccy) = (close_rect.center().x, close_rect.center().y);
    let s = 3.6;
    let close_color = theme::TEXT_SECONDARY();
    scene.stroke(&Stroke::new(1.3), Affine::IDENTITY, close_color, None, &Line::new((ccx - s, ccy - s), (ccx + s, ccy + s)));
    scene.stroke(&Stroke::new(1.3), Affine::IDENTITY, close_color, None, &Line::new((ccx - s, ccy + s), (ccx + s, ccy - s)));
    app.hit_regions.push(HitRegion { rect: close_rect, action: HitAction::DismissUpdateBanner });

    let btn = Rect::new(card.x0 + pad, card.y0 + 64.0, card.x1 - pad, card.y0 + 92.0);
    let btn_color = if btn.contains(app.hover) {
        theme::BRAND_ACCENT().with_alpha(0.85)
    } else {
        theme::BRAND_ACCENT()
    };
    fill_rect(scene, btn, btn_color, 13.0);
    let label = "Download";
    let tw = text.measure(label, 12.5, None);
    text.draw_centered_v(scene, label, 12.5, None, theme::CANVAS(), btn.x0 + (btn.width() - tw) / 2.0, btn.y0, btn.y1);
    app.hit_regions.push(HitRegion { rect: btn, action: HitAction::OpenUpdateDownload });
}

fn draw_drop_flash(scene: &mut Scene, text: &mut TextCx, width: f64, height: f64) {
    fill_rect(
        scene,
        Rect::new(0.0, 0.0, width, height),
        vello::peniko::Color::from_rgba8(0x9a, 0xff, 0x00, 0x20),
        0.0,
    );
    let msg = "Adding font\u{2026}";
    let size = 18.0;
    let tw = text.measure(msg, size, None);
    let pill_w = tw + 40.0;
    let pill_h = 44.0;
    let x0 = (width - pill_w) / 2.0;
    let y0 = (height - pill_h) / 2.0;
    let pill = Rect::new(x0, y0, x0 + pill_w, y0 + pill_h);
    fill_rect(scene, pill, theme::SIDEBAR_BG(), pill_h / 2.0);
    stroke_rect(scene, pill, theme::CONTROL_BORDER(), pill_h / 2.0, 1.0);
    text.draw_centered_v(scene, msg, size, None, theme::TEXT(), pill.x0 + 20.0, pill.y0, pill.y1);
}

/// A modal (dims the rest of the window, unlike the non-modal sample-text
/// dock) since a dropped file genuinely needs a decision — which library,
/// or cancel — before anything else makes sense to interact with.
fn draw_drop_picker(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    let Some(dropped) = app.pending_drop.clone() else {
        return;
    };
    let file_name = dropped
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "font file".to_string());

    let row_h = 40.0;
    let header_h = 56.0;
    let footer_h = 46.0;
    let folders: Vec<(i64, String)> = app.folders.iter().map(|f| (f.id, f.path.clone())).collect();
    let list_h = row_h * folders.len().max(1) as f64;
    let card_w = 380.0;
    let card_h = header_h + list_h + footer_h;
    let x0 = (width - card_w) / 2.0;
    let y0 = (height - card_h) / 2.0;
    let card = Rect::new(x0, y0, x0 + card_w, y0 + card_h);

    fill_rect(
        scene,
        Rect::new(0.0, 0.0, width, height),
        vello::peniko::Color::from_rgba8(0x00, 0x00, 0x00, 0x66),
        0.0,
    );
    fill_rect(scene, card, theme::SIDEBAR_BG(), 16.0);
    stroke_rect(scene, card, theme::CONTROL_BORDER(), 16.0, 1.0);

    text.draw(scene, "Add to which library?", 15.0, None, theme::TEXT(), card.x0 + 20.0, card.y0 + 26.0);
    text.draw(scene, &file_name, 11.5, None, theme::TEXT_SECONDARY(), card.x0 + 20.0, card.y0 + 44.0);

    if folders.is_empty() {
        text.draw(
            scene,
            "No libraries yet \u{2014} add one first.",
            12.5,
            None,
            theme::TEXT_TERTIARY(),
            card.x0 + 20.0,
            card.y0 + header_h + 24.0,
        );
    }

    for (i, (folder_id, path)) in folders.into_iter().enumerate() {
        let ry = card.y0 + header_h + row_h * i as f64;
        let row = Rect::new(card.x0 + 8.0, ry, card.x1 - 8.0, ry + row_h);
        if row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 8.0);
        }
        let name = path.rsplit(['/', '\\']).find(|s| !s.is_empty()).unwrap_or(&path);
        text.draw_centered_v(scene, name, 13.0, None, theme::TEXT(), row.x0 + 12.0, row.y0, row.y1);
        app.hit_regions.push(HitRegion {
            rect: row,
            action: HitAction::AssignDrop(folder_id),
        });
    }

    let footer_y = card.y0 + header_h + list_h;
    let cancel_rect = Rect::new(card.x0 + 20.0, footer_y + 8.0, card.x1 - 20.0, footer_y + footer_h - 8.0);
    let cancel_hovered = cancel_rect.contains(app.hover);
    fill_rect(
        scene,
        cancel_rect,
        if cancel_hovered { theme::CONTROL_HOVER_SUBTLE() } else { theme::CONTROL_BG() },
        8.0,
    );
    let cancel_label = "Cancel";
    let tw = text.measure(cancel_label, 12.5, None);
    text.draw_centered_v(
        scene,
        cancel_label,
        12.5,
        None,
        theme::TEXT_SECONDARY(),
        cancel_rect.x0 + (cancel_rect.width() - tw) / 2.0,
        cancel_rect.y0,
        cancel_rect.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: cancel_rect,
        action: HitAction::CancelDrop,
    });
}

/// Font ▸ Delete from Library's confirmation — a real, unrecoverable file
/// delete that (since a library folder is typically a team's externally
/// synced Dropbox/Drive/OneDrive/iCloud/SMB folder, not something GlyphClub
/// owns) can propagate to everyone else's synced copy too, so it always
/// stops for a confirmation here rather than deleting on the menu click
/// alone.
fn draw_delete_confirm(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    let Some(id) = app.confirm_delete else {
        return;
    };
    let family = app.entry(id).map(|e| e.family.clone()).unwrap_or_else(|| "this font".to_string());

    let card_w = 360.0;
    let card_h = 172.0;
    let x0 = (width - card_w) / 2.0;
    let y0 = (height - card_h) / 2.0;
    let card = Rect::new(x0, y0, x0 + card_w, y0 + card_h);

    fill_rect(
        scene,
        Rect::new(0.0, 0.0, width, height),
        vello::peniko::Color::from_rgba8(0x00, 0x00, 0x00, 0x66),
        0.0,
    );
    fill_rect(scene, card, theme::SIDEBAR_BG(), 16.0);
    stroke_rect(scene, card, theme::CONTROL_BORDER(), 16.0, 1.0);

    text.draw(scene, "Delete from Library?", 15.0, None, theme::TEXT(), card.x0 + 20.0, card.y0 + 30.0);
    let body = format!(
        "\u{201c}{family}\u{201d} will be permanently deleted from disk \u{2014} including everyone else\u{2019}s synced copy, if this library is shared. This can\u{2019}t be undone."
    );
    draw_wrapped_line(text, scene, &body, card.x0 + 20.0, card.y0 + 54.0, card_w - 40.0, theme::TEXT_SECONDARY());

    let btn_h = 38.0;
    let btn_y = card.y1 - 20.0 - btn_h;
    let cancel_rect = Rect::new(card.x0 + 20.0, btn_y, card.x0 + card_w / 2.0 - 6.0, btn_y + btn_h);
    let delete_rect = Rect::new(card.x0 + card_w / 2.0 + 6.0, btn_y, card.x1 - 20.0, btn_y + btn_h);

    let cancel_hovered = cancel_rect.contains(app.hover);
    fill_rect(
        scene,
        cancel_rect,
        if cancel_hovered { theme::CONTROL_HOVER_SUBTLE() } else { theme::CONTROL_BG() },
        8.0,
    );
    let cancel_label = "Cancel";
    let ctw = text.measure(cancel_label, 12.5, None);
    text.draw_centered_v(
        scene,
        cancel_label,
        12.5,
        None,
        theme::TEXT_SECONDARY(),
        cancel_rect.x0 + (cancel_rect.width() - ctw) / 2.0,
        cancel_rect.y0,
        cancel_rect.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: cancel_rect,
        action: HitAction::CancelDelete,
    });

    let delete_hovered = delete_rect.contains(app.hover);
    fill_rect(scene, delete_rect, theme::DANGER(), 8.0);
    if delete_hovered {
        stroke_rect(scene, delete_rect, theme::TEXT(), 8.0, 1.0);
    }
    let delete_label = "Delete";
    let dtw = text.measure(delete_label, 12.5, None);
    text.draw_centered_v(
        scene,
        delete_label,
        12.5,
        None,
        theme::TEXT(),
        delete_rect.x0 + (delete_rect.width() - dtw) / 2.0,
        delete_rect.y0,
        delete_rect.y1,
    );
    app.hit_regions.push(HitRegion {
        rect: delete_rect,
        action: HitAction::ConfirmDelete(id),
    });
}

/// Naive word-wrap for the one multi-line body string this file needs
/// (the delete confirmation's warning) — breaks on word boundaries at
/// `max_w`, no hyphenation, good enough for a couple of short lines.
fn draw_wrapped_line(
    text: &mut TextCx,
    scene: &mut Scene,
    body: &str,
    x: f64,
    y0: f64,
    max_w: f64,
    color: vello::peniko::Color,
) {
    let size = 12.5;
    let line_h = 18.0;
    let mut line = String::new();
    let mut y = y0;
    for word in body.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text.measure(&candidate, size, None) > max_w && !line.is_empty() {
            text.draw(scene, &line, size as f32, None, color, x, y);
            y += line_h;
            line = word.to_string();
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        text.draw(scene, &line, size as f32, None, color, x, y);
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

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 8.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 8.0, 1.0);

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
            theme::TEXT(),
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

/// A font tile/row's right-click menu — see `FontContextMenu`. Built from
/// `App::selected` (set to the right-clicked font by `handle_right_click`
/// before this ever draws), the same way the Font menu-bar menu's own
/// enabled/disabled logic already is, so every action here reuses the
/// exact same `HitAction::*Selected` variants and handlers that menu does.
fn draw_font_context_menu(app: &mut App, text: &mut TextCx, scene: &mut Scene) {
    let Some(font_id) = app.font_context_menu.as_ref().map(|m| m.font_id) else {
        return;
    };
    let anchor = app.font_context_menu.as_ref().unwrap().anchor;
    let is_system = selected_is_system(app, font_id);
    let is_active = app.active_ids.contains(&font_id);
    let favorite = app.favorite_ids.contains(&font_id);

    enum Row {
        Activate(&'static str, vello::peniko::Color, bool, HitAction),
        Star,
        Tags,
        Sep,
        Item(&'static str, HitAction, bool),
        Danger(&'static str, HitAction, bool),
    }

    let rows = [
        Row::Activate("Activate", theme::GREEN(), !is_system && !is_active, HitAction::ActivateSelected),
        Row::Activate(
            "Activate Temporarily",
            theme::AMBER(),
            !is_system && !is_active,
            HitAction::ActivateSelectedTemporarily,
        ),
        Row::Activate("Deactivate", theme::DOT_INACTIVE(), !is_system && is_active, HitAction::DeactivateSelected),
        Row::Sep,
        Row::Star,
        Row::Tags,
        Row::Sep,
        Row::Item("Export\u{2026}", HitAction::ExportSelectedFont, true),
        Row::Item(crate::reveal::LABEL, HitAction::RevealSelectedFont, true),
        Row::Sep,
        Row::Danger("Delete from Library\u{2026}", HitAction::RequestDeleteSelected, !is_system),
    ];

    let row_h = 30.0;
    let sep_h = 9.0;
    let w = 210.0;
    let panel_h = 8.0
        + rows
            .iter()
            .map(|r| if matches!(r, Row::Sep) { sep_h } else { row_h })
            .sum::<f64>();
    let panel = Rect::new(anchor.x, anchor.y, anchor.x + w, anchor.y + panel_h);

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut iy = panel.y0 + 4.0;
    let mut tags_row_rect = Rect::ZERO;
    for row in rows {
        if matches!(row, Row::Sep) {
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                theme::SEPARATOR(),
                None,
                &Line::new((panel.x0 + 10.0, iy + sep_h / 2.0), (panel.x1 - 10.0, iy + sep_h / 2.0)),
            );
            iy += sep_h;
            continue;
        }
        let rect = Rect::new(panel.x0 + 4.0, iy, panel.x1 - 4.0, iy + row_h);
        iy += row_h;

        let enabled = match &row {
            Row::Activate(.., enabled, _) => *enabled,
            Row::Item(_, _, enabled) | Row::Danger(_, _, enabled) => *enabled,
            Row::Star | Row::Tags => true,
            Row::Sep => unreachable!(),
        };
        if enabled && rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG(), 6.0);
        }
        let text_color = |danger: bool| {
            if !enabled {
                theme::TEXT_TERTIARY()
            } else if danger {
                theme::DANGER()
            } else {
                theme::TEXT()
            }
        };

        match row {
            Row::Activate(label, dot_color, enabled, action) => {
                scene.fill(Fill::NonZero, Affine::IDENTITY, dot_color, None, &Circle::new((rect.x0 + 16.0, rect.center().y), 4.5));
                text.draw_centered_v(scene, label, 12.5, None, text_color(false), rect.x0 + 32.0, rect.y0, rect.y1);
                if enabled {
                    app.hit_regions.push(HitRegion { rect, action });
                }
            }
            Row::Star => {
                let star_color = if favorite { theme::GOLD() } else { theme::TEXT_SECONDARY() };
                draw_star_icon(scene, rect.x0 + 16.0, rect.center().y, 6.5, star_color, favorite);
                text.draw_centered_v(scene, "Star", 12.5, None, theme::TEXT(), rect.x0 + 32.0, rect.y0, rect.y1);
                app.hit_regions.push(HitRegion { rect, action: HitAction::ToggleFavorite(font_id) });
            }
            Row::Tags => {
                text.draw_centered_v(scene, "#", 12.5, None, theme::TEXT_SECONDARY(), rect.x0 + 12.0, rect.y0, rect.y1);
                text.draw_centered_v(scene, "Tags", 12.5, None, theme::TEXT(), rect.x0 + 32.0, rect.y0, rect.y1);
                let mut chevron = BezPath::new();
                let (cx, cy) = (rect.x1 - 14.0, rect.center().y);
                chevron.move_to(Point::new(cx - 2.5, cy - 4.0));
                chevron.line_to(Point::new(cx + 2.0, cy));
                chevron.line_to(Point::new(cx - 2.5, cy + 4.0));
                scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, theme::TEXT_SECONDARY(), None, &chevron);
                app.hit_regions.push(HitRegion { rect, action: HitAction::ToggleTagsSubmenu });
                tags_row_rect = rect;
            }
            Row::Item(label, action, enabled) => {
                text.draw_centered_v(scene, label, 12.5, None, text_color(false), rect.x0 + 12.0, rect.y0, rect.y1);
                if enabled {
                    app.hit_regions.push(HitRegion { rect, action });
                }
            }
            Row::Danger(label, action, enabled) => {
                text.draw_centered_v(scene, label, 12.5, None, text_color(true), rect.x0 + 12.0, rect.y0, rect.y1);
                if enabled {
                    app.hit_regions.push(HitRegion { rect, action });
                }
            }
            Row::Sep => unreachable!(),
        }
    }

    let tags_open = app.font_context_menu.as_ref().is_some_and(|m| m.tags_open);
    if tags_open {
        draw_tags_submenu(app, text, scene, font_id, Point::new(panel.x1 + 4.0, tags_row_rect.y0));
    }

    if let Some(m) = app.font_context_menu.as_mut() {
        m.panel_rect = panel;
    }
}

/// The Tags submenu — every existing tag as a checkbox row (checked if
/// `font_id` currently carries it), plus a trailing "New Tag…" row that
/// switches into an inline text field (`FontContextMenu::new_tag_text`)
/// when clicked. Anchored to the right of the main panel's Tags row.
fn draw_tags_submenu(app: &mut App, text: &mut TextCx, scene: &mut Scene, font_id: i64, anchor: Point) {
    let row_h = 28.0;
    let w = 200.0;
    let tags = app.tags.clone();
    let new_tag_active = app.font_context_menu.as_ref().is_some_and(|m| m.new_tag_text.is_some());
    let rows = tags.len() + 1 /* New Tag row */ + if tags.is_empty() { 0 } else { 1 } /* separator */;
    let panel_h = 8.0 + row_h * rows as f64;
    let panel = Rect::new(anchor.x, anchor.y, anchor.x + w, anchor.y + panel_h);

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut iy = panel.y0 + 4.0;
    for tag in &tags {
        let rect = Rect::new(panel.x0 + 4.0, iy, panel.x1 - 4.0, iy + row_h);
        iy += row_h;
        if rect.contains(app.hover) {
            fill_rect(scene, rect, theme::NAV_HOVER_BG(), 6.0);
        }
        let checked = app.tag_font_ids.get(&tag.id).is_some_and(|ids| ids.contains(&font_id));
        if checked {
            text.draw_centered_v(scene, "\u{2713}", 12.0, None, theme::BRAND_ACCENT(), rect.x0 + 10.0, rect.y0, rect.y1);
        }
        let name = truncate_to_width(text, &tag.name, 12.5, None, rect.width() - 34.0);
        text.draw_centered_v(scene, &name, 12.5, None, theme::TEXT(), rect.x0 + 28.0, rect.y0, rect.y1);
        app.hit_regions.push(HitRegion { rect, action: HitAction::ToggleFontTag(font_id, tag.id) });
    }

    if !tags.is_empty() {
        scene.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            theme::SEPARATOR(),
            None,
            &Line::new((panel.x0 + 10.0, iy + 4.0), (panel.x1 - 10.0, iy + 4.0)),
        );
        iy += 9.0;
    }

    let new_row = Rect::new(panel.x0 + 4.0, iy, panel.x1 - 4.0, iy + row_h);
    if new_tag_active {
        fill_rect(scene, new_row.inset(-2.0), theme::CONTROL_BG(), 6.0);
        stroke_rect(scene, new_row.inset(-2.0), theme::CONTROL_FOCUS(), 6.0, 1.2);
        let buffer = app
            .font_context_menu
            .as_ref()
            .and_then(|m| m.new_tag_text.as_deref())
            .unwrap_or("");
        if buffer.is_empty() {
            text.draw_centered_v(scene, "Tag name\u{2026}", 12.5, None, theme::TEXT_TERTIARY(), new_row.x0 + 10.0, new_row.y0, new_row.y1);
        } else {
            text.draw_centered_v(scene, buffer, 12.5, None, theme::TEXT(), new_row.x0 + 10.0, new_row.y0, new_row.y1);
        }
    } else {
        if new_row.contains(app.hover) {
            fill_rect(scene, new_row, theme::NAV_HOVER_BG(), 6.0);
        }
        text.draw_centered_v(scene, "+ New Tag\u{2026}", 12.5, None, theme::TEXT_SECONDARY(), new_row.x0 + 10.0, new_row.y0, new_row.y1);
        app.hit_regions.push(HitRegion { rect: new_row, action: HitAction::OpenNewTagField });
    }

    if let Some(m) = app.font_context_menu.as_mut() {
        m.tags_panel_rect = panel;
    }
}

fn draw_toolbar(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64) {
    fill_rect(
        scene,
        Rect::new(x0, 0.0, x1, theme::TOOLBAR_H),
        theme::TOOLBAR_BG(),
        0.0,
    );
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        theme::SEPARATOR(),
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
        theme::CONTROL_HOVER_SUBTLE()
    } else {
        theme::CONTROL_BG()
    };
    fill_rect(scene, rescan_rect, rescan_bg, ctrl_h / 2.0);
    stroke_rect(scene, rescan_rect, theme::CONTROL_BORDER(), ctrl_h / 2.0, 1.0);
    draw_refresh_icon(scene, rescan_rect.center(), theme::TEXT_SECONDARY());
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
        fill_rect(scene, slider_rect, theme::CONTROL_BG(), 18.0);
        stroke_rect(scene, slider_rect, theme::CONTROL_BORDER(), 18.0, 1.0);

        text.draw_centered_v(
            scene,
            "A",
            10.0,
            None,
            theme::TEXT_TERTIARY(),
            cx + 10.0,
            ctrl_y,
            ctrl_y + ctrl_h,
        );
        text.draw_centered_v(
            scene,
            "A",
            14.0,
            None,
            theme::TEXT_SECONDARY(),
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
            theme::TEXT_TERTIARY(),
            None,
            &Line::new((track_x0, track_y), (track_x1, track_y)),
        );
        let thumb_x = track_x0 + (track_x1 - track_x0) * progress;
        app.tile_slider_track = Some((track_x0, track_x1));
        scene.stroke(
            &Stroke::new(3.0),
            Affine::IDENTITY,
            theme::BRAND_ACCENT(),
            None,
            &Line::new((track_x0, track_y), (thumb_x, track_y)),
        );
        scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            theme::TEXT(),
            None,
            &Circle::new(Point::new(thumb_x, track_y), 6.0),
        );
        stroke_rect(
            scene,
            Rect::new(thumb_x - 6.0, track_y - 6.0, thumb_x + 6.0, track_y + 6.0),
            theme::CONTROL_BORDER(),
            6.0,
            1.0,
        );

        app.hit_regions.push(HitRegion {
            rect: slider_rect,
            action: HitAction::StartTileSizeDrag,
        });
        cx += slider_w + 14.0;
    }

    // Search field, right-aligned.
    let search_w = 236.0;
    let search_rect = Rect::new(x1 - pad - search_w, ctrl_y, x1 - pad, ctrl_y + ctrl_h);

    // Preview · Grid/List · Info — laid out as their own right-aligned
    // group immediately to the left of the search field, not tacked onto
    // the rescan/slider cluster on the far left.
    let seg_w = ctrl_h;
    let cluster_w = ctrl_h + 12.0 + seg_w * 2.0 + 12.0 + ctrl_h;
    let mut ccx = search_rect.x0 - pad - cluster_w;

    // Windows only: macOS gets a real `NSMenu` (see `native_menu.rs`)
    // instead, so this fills what would otherwise be dead space between
    // the tile-size slider and the preview/view/info cluster.
    #[cfg(target_os = "windows")]
    draw_windows_menu_bar(app, text, scene, cx, ccx - pad, ctrl_y, ctrl_h);
    #[cfg(not(target_os = "windows"))]
    let _ = cx;

    // Preview toggle — opens the sample-text picker card (`draw_sample_picker`).
    let preview_rect = Rect::new(ccx, ctrl_y, ccx + ctrl_h, ctrl_y + ctrl_h);
    let preview_active = app.sample_picker_open;
    if preview_active {
        fill_rect(scene, preview_rect, theme::CONTROL_BG_HOVER(), ctrl_h / 2.0);
    } else if preview_rect.contains(app.hover) {
        fill_rect(
            scene,
            preview_rect,
            theme::CONTROL_HOVER_SUBTLE(),
            ctrl_h / 2.0,
        );
    }
    stroke_rect(
        scene,
        preview_rect,
        theme::CONTROL_BORDER(),
        ctrl_h / 2.0,
        1.0,
    );
    draw_preview_icon(
        scene,
        preview_rect.x0 + ctrl_h / 2.0,
        preview_rect.y0 + ctrl_h / 2.0,
        if preview_active { theme::TEXT() } else { theme::TEXT_SECONDARY() },
    );
    app.hit_regions.push(HitRegion {
        rect: preview_rect,
        action: HitAction::ToggleSamplePicker,
    });
    ccx += ctrl_h + 12.0;

    // Grid / List — paired together in one pill, since they're the same
    // choice (view mode), unlike the standalone buttons on either side.
    let seg_track = Rect::new(ccx, ctrl_y, ccx + seg_w * 2.0, ctrl_y + ctrl_h);
    fill_rect(scene, seg_track, theme::CONTROL_BG(), ctrl_h / 2.0);
    stroke_rect(scene, seg_track, theme::CONTROL_BORDER(), ctrl_h / 2.0, 1.0);
    for (i, mode) in [ViewMode::Grid, ViewMode::List].into_iter().enumerate() {
        let r = Rect::new(
            ccx + seg_w * i as f64,
            ctrl_y,
            ccx + seg_w * (i as f64 + 1.0),
            ctrl_y + ctrl_h,
        );
        let active = app.view_mode == mode;
        if active {
            fill_rect(scene, r.inset(-2.0), theme::CONTROL_BG_HOVER(), seg_w / 2.0);
        } else if r.contains(app.hover) {
            fill_rect(
                scene,
                r.inset(-1.0),
                theme::CONTROL_HOVER_SUBTLE(),
                seg_w / 2.0,
            );
        }
        let icon_color = if active {
            theme::TEXT()
        } else {
            theme::TEXT_SECONDARY()
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
        fill_rect(scene, info_rect, theme::CONTROL_BG_HOVER(), ctrl_h / 2.0);
    } else if info_rect.contains(app.hover) {
        fill_rect(scene, info_rect, theme::CONTROL_HOVER_SUBTLE(), ctrl_h / 2.0);
    }
    stroke_rect(scene, info_rect, theme::CONTROL_BORDER(), ctrl_h / 2.0, 1.0);
    draw_info_icon(
        scene,
        info_rect.x0 + ctrl_h / 2.0,
        info_rect.y0 + ctrl_h / 2.0,
        if info_active {
            theme::TEXT()
        } else {
            theme::TEXT_SECONDARY()
        },
    );
    app.hit_regions.push(HitRegion {
        rect: info_rect,
        action: HitAction::ToggleDetailPanel,
    });
    let search_radius = ctrl_h / 2.0;
    fill_rect(scene, search_rect, theme::CONTROL_BG(), search_radius);
    let is_focused = app.focus == Focus::Search;
    stroke_rect(
        scene,
        search_rect,
        if is_focused {
            theme::CONTROL_FOCUS()
        } else {
            theme::CONTROL_BORDER()
        },
        search_radius,
        if is_focused { 1.5 } else { 1.0 },
    );
    draw_search_icon(
        scene,
        search_rect.x0 + 11.0,
        ctrl_y + 9.0,
        theme::TEXT_TERTIARY(),
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
            theme::TEXT_TERTIARY(),
            text_x,
            text_y,
        );
    } else {
        text.draw(scene, &app.search, 13.0, None, theme::TEXT(), text_x, text_y);
        if app.focus == Focus::Search {
            let caret_x = text_x + text.measure(&app.search, 13.0, None);
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                theme::BRAND_ACCENT(),
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

/// The hand-rolled Windows menu bar — macOS gets a real `NSMenu` (see
/// `native_menu.rs`) driven by `muda`; Windows has its own plan (this),
/// drawn as an ordinary part of the toolbar rather than native window
/// chrome. Every real item routes through the same `ui::` functions the
/// macOS menu calls, so the two never drift into different behavior for
/// what's supposed to be the same command.
#[cfg(target_os = "windows")]
fn draw_windows_menu_bar(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    x0: f64,
    x1: f64,
    ctrl_y: f64,
    ctrl_h: f64,
) {
    use crate::app::WinMenuKind;

    let labels = [
        (WinMenuKind::File, "File"),
        (WinMenuKind::View, "View"),
        (WinMenuKind::Font, "Font"),
        (WinMenuKind::Help, "Help"),
    ];

    app.win_menu_bar_rects.clear();
    let mut cx = x0;
    for (kind, label) in labels {
        let w = text.measure(label, 13.0, None) + 24.0;
        if cx + w > x1 {
            break;
        }
        let rect = Rect::new(cx, ctrl_y, cx + w, ctrl_y + ctrl_h);
        let open = app.open_win_menu == Some(kind);
        if open || rect.contains(app.hover) {
            fill_rect(scene, rect, theme::CONTROL_BG(), 8.0);
        }
        let tw = text.measure(label, 13.0, None);
        text.draw_centered_v(
            scene,
            label,
            13.0,
            None,
            if open { theme::TEXT() } else { theme::TEXT_SECONDARY() },
            rect.x0 + (rect.width() - tw) / 2.0,
            rect.y0,
            rect.y1,
        );
        app.win_menu_bar_rects.push((kind, rect));
        app.hit_regions.push(HitRegion {
            rect,
            action: HitAction::OpenWinMenu(kind),
        });
        cx += w;
    }
}

/// The open Windows menu's own dropdown panel — deliberately *not* drawn
/// from inside `draw_windows_menu_bar` (called from `draw_toolbar`, which
/// runs before the grid/list) but from `draw` itself, last, alongside
/// `draw_context_menu`/`draw_font_context_menu`. Drawing it early meant
/// the grid/list painted over it every time a menu was open — this is
/// Windows-only code a macOS build never even type-checks, let alone
/// visually catches, which is exactly how it went unnoticed all session.
#[cfg(target_os = "windows")]
fn draw_windows_menu_dropdown(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64) {
    use crate::app::WinMenuKind;

    let Some(kind) = app.open_win_menu else {
        app.win_menu_panel_rect = Rect::ZERO;
        return;
    };
    let Some(&(_, anchor)) = app.win_menu_bar_rects.iter().find(|(k, _)| *k == kind) else {
        return;
    };
    let (x0, x1) = (0.0, width);

    let selected_font = app.selected.and_then(|id| app.entry(id));
    let is_system_selected = selected_font.map(|e| e.is_system).unwrap_or(false);
    let has_selection = selected_font.is_some();
    let is_active = app.selected.map(|id| app.active_ids.contains(&id)).unwrap_or(false);

    // A separator is `(None, true, None)`; a real item names its label,
    // whether it's currently enabled, and the action a click on it runs.
    // Checkmark state for the two View items comes from `app.view_mode`
    // directly rather than being threaded through here.
    //
    // Each separator below is its own fresh literal rather than a shared
    // `sep` variable — `HitAction` isn't `Copy`, so a single shared binding
    // can only be moved into one of these `vec![...]` slots before it's gone.
    let items: Vec<(Option<&str>, bool, Option<HitAction>)> = match kind {
        WinMenuKind::File => vec![
            (Some("Add Library\u{2026}"), true, Some(HitAction::AddFolder)),
            (Some("Sync Now"), true, Some(HitAction::Rescan)),
            (None, true, None),
            (Some("Settings\u{2026}"), true, Some(HitAction::OpenSettings)),
        ],
        WinMenuKind::View => vec![
            (Some("as Grid"), true, Some(HitAction::SetViewMode(ViewMode::Grid))),
            (Some("as List"), true, Some(HitAction::SetViewMode(ViewMode::List))),
            (None, true, None),
            (Some("Show Font Info"), true, Some(HitAction::ToggleDetailPanel)),
            (None, true, None),
            (Some("Zoom In"), true, Some(HitAction::ZoomIn)),
            (Some("Zoom Out"), true, Some(HitAction::ZoomOut)),
        ],
        WinMenuKind::Font => vec![
            (
                Some("Activate"),
                has_selection && !is_system_selected && !is_active,
                Some(HitAction::ActivateSelected),
            ),
            (
                Some("Activate Temporarily"),
                has_selection && !is_system_selected && !is_active,
                Some(HitAction::ActivateSelectedTemporarily),
            ),
            (
                Some("Deactivate"),
                has_selection && !is_system_selected && is_active,
                Some(HitAction::DeactivateSelected),
            ),
            (None, true, None),
            (Some("Mark Favorite"), has_selection, Some(HitAction::ToggleDetailFavorite)),
            (None, true, None),
            (Some("Export\u{2026}"), has_selection, Some(HitAction::ExportSelectedFont)),
            (Some("Show in Explorer"), has_selection, Some(HitAction::RevealSelectedFont)),
            (None, true, None),
            (
                Some("Remove from Fontlist"),
                has_selection && !is_system_selected,
                Some(HitAction::RemoveSelectedFromFontlist),
            ),
            (
                Some("Delete from Library\u{2026}"),
                has_selection && !is_system_selected,
                Some(HitAction::RequestDeleteSelected),
            ),
        ],
        WinMenuKind::Help => vec![(Some("GlyphClub on GitHub"), true, Some(HitAction::OpenGitHub))],
    };

    let row_h = 28.0;
    let sep_h = 9.0;
    let panel_w = 220.0;
    let panel_h = 12.0
        + items
            .iter()
            .map(|(label, ..)| if label.is_some() { row_h } else { sep_h })
            .sum::<f64>();
    let panel_x0 = anchor.x0.min(x1 - panel_w).max(x0);
    let panel = Rect::new(panel_x0, anchor.y1 + 4.0, panel_x0 + panel_w, anchor.y1 + 4.0 + panel_h);
    app.win_menu_panel_rect = panel;

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut iy = panel.y0 + 6.0;
    for (label, enabled, action) in items {
        let (Some(label), Some(action)) = (label, action) else {
            scene.stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                theme::SEPARATOR(),
                None,
                &Line::new((panel.x0 + 10.0, iy + sep_h / 2.0), (panel.x1 - 10.0, iy + sep_h / 2.0)),
            );
            iy += sep_h;
            continue;
        };
        let row = Rect::new(panel.x0 + 4.0, iy, panel.x1 - 4.0, iy + row_h);
        if enabled && row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 6.0);
        }
        let checked = (label == "as Grid" && app.view_mode == ViewMode::Grid)
            || (label == "as List" && app.view_mode == ViewMode::List);
        let label_x = row.x0 + if checked { 28.0 } else { 12.0 };
        if checked {
            text.draw_centered_v(scene, "\u{2713}", 12.0, None, theme::TEXT(), row.x0 + 12.0, row.y0, row.y1);
        }
        text.draw_centered_v(
            scene,
            label,
            12.5,
            None,
            if enabled { theme::TEXT() } else { theme::TEXT_TERTIARY() },
            label_x,
            row.y0,
            row.y1,
        );
        if enabled {
            app.hit_regions.push(HitRegion { rect: row, action });
        }
        iy += row_h;
    }
}

/// Dispatches a click at `point` against the regions the last [`draw`] call
/// recorded (topmost-drawn first), then blurs any focused text field if
/// nothing under the click claims focus for itself.
///
/// Returns whether the click landed on *something* this app cares about —
/// a context/win-menu (open, closing, or a click inside one) or a real
/// `HitRegion` — as opposed to genuinely empty background. `main.rs`'s
/// `MouseInput` handler uses that to decide whether a click that starts a
/// hold-and-drag should move the window instead (see
/// `traffic_lights::disable_titlebar_drag`'s doc comment for why that
/// can't be decided on the AppKit side alone).
pub fn handle_click(app: &mut App, point: Point) -> bool {
    if let Some(menu) = &app.context_menu {
        let inside = menu.panel_rect.contains(point);
        app.context_menu = None;
        if !inside {
            return true;
        }
    }

    // Same dismiss-on-outside-click pattern, but two panels to check —
    // the main menu and (while open) its Tags submenu, anchored off to
    // the side of it — a click inside either keeps the whole thing open.
    if let Some(menu) = &app.font_context_menu {
        let inside = menu.panel_rect.contains(point) || (menu.tags_open && menu.tags_panel_rect.contains(point));
        if !inside {
            app.font_context_menu = None;
            app.focus = Focus::None;
            return true;
        }
    }

    // Same dismiss-on-outside-click pattern as the folder context menu,
    // above — except a click on the menu bar itself (any label, not just
    // the open one) falls through to the normal hit-region match below
    // instead of being swallowed, so clicking a different top-level menu
    // switches straight to it rather than needing two clicks.
    if app.open_win_menu.is_some() {
        let on_bar = app.win_menu_bar_rects.iter().any(|(_, r)| r.contains(point));
        let inside_panel = app.win_menu_panel_rect.contains(point);
        if !on_bar && !inside_panel {
            app.open_win_menu = None;
            return true;
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
        return false;
    };

    // A click on any real action in the font context menu closes it — the
    // three Tags-submenu interactions below are the only ones meant to
    // leave it open, since more than one tag often gets toggled in a row.
    if app.font_context_menu.is_some()
        && !matches!(
            action,
            HitAction::ToggleTagsSubmenu | HitAction::ToggleFontTag(..) | HitAction::OpenNewTagField
        )
    {
        app.font_context_menu = None;
    }

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
            add_library(app);
        }
        HitAction::Rescan => {
            app.focus = Focus::None;
            sync_now(app);
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
        HitAction::StartDetailResize => {
            app.dragging_detail_resize = true;
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
        HitAction::TogglePreviewPresetDropdown => {
            app.preview_preset_open = !app.preview_preset_open;
        }
        HitAction::SetPendingPreviewPreset(preset) => {
            app.pending_preview_preset = Some(*preset);
            app.preview_preset_open = false;
        }
        HitAction::SetPreviewAlign(align) => {
            app.preview_align = *align;
        }
        HitAction::TogglePreviewSizeDropdown => {
            app.preview_size_dropdown_open = !app.preview_size_dropdown_open;
        }
        HitAction::SetPreviewSize(size) => {
            app.preview_size = *size;
            app.preview_size_input = (*size as i64).to_string();
            app.preview_size_dropdown_open = false;
        }
        HitAction::AssignDrop(folder_id) => {
            let folder_id = *folder_id;
            app.focus = Focus::None;
            if let Some(dropped) = app.pending_drop.take() {
                assign_dropped_file(app, folder_id, &dropped);
            }
        }
        HitAction::CancelDrop => {
            app.pending_drop = None;
            app.focus = Focus::None;
        }
        HitAction::FocusField(f) => {
            app.focus = *f;
        }
        HitAction::SelectFont(id) => {
            if app.selected != Some(*id) {
                app.glyph_search.clear();
                app.glyph_block_filter = None;
                app.detail_tab_scroll_y = 0.0;
            }
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
        HitAction::PressColumnHeader(i) => {
            app.header_press = Some((*i, point));
            app.focus = Focus::None;
        }
        HitAction::StartColumnResize(i) => {
            let width = app.list_columns[*i].width;
            app.column_resize_start = Some((*i, point.x, width));
            app.focus = Focus::None;
        }
        HitAction::ToggleFamilyExpanded(family) => {
            if !app.expanded_families.remove(family) {
                app.expanded_families.insert(family.clone());
            }
            app.focus = Focus::None;
        }
        HitAction::ConfirmDelete(id) => {
            let id = *id;
            app.confirm_delete = None;
            match app.catalog.delete_font_file(id) {
                Ok(()) => {
                    if app.selected == Some(id) {
                        app.selected = None;
                    }
                    app.reload_folders_and_fonts("Deleted".to_string());
                }
                Err(e) => app.status = e.to_string(),
            }
        }
        HitAction::CancelDelete => {
            app.confirm_delete = None;
        }
        HitAction::OpenWinMenu(kind) => {
            app.open_win_menu = if app.open_win_menu == Some(*kind) { None } else { Some(*kind) };
            app.focus = Focus::None;
        }
        HitAction::ActivateSelected => {
            app.open_win_menu = None;
            activate_selected(app);
        }
        HitAction::ActivateSelectedTemporarily => {
            app.open_win_menu = None;
            activate_selected_temporarily(app);
        }
        HitAction::DeactivateSelected => {
            app.open_win_menu = None;
            deactivate_selected(app);
        }
        HitAction::ExportSelectedFont => {
            app.open_win_menu = None;
            export_selected(app);
        }
        HitAction::RevealSelectedFont => {
            app.open_win_menu = None;
            reveal_selected(app);
        }
        HitAction::RemoveSelectedFromFontlist => {
            app.open_win_menu = None;
            remove_selected_from_fontlist(app);
        }
        HitAction::RequestDeleteSelected => {
            app.open_win_menu = None;
            request_delete_selected(app);
        }
        HitAction::ZoomIn => {
            app.open_win_menu = None;
            app.tile_size = (app.tile_size + 20.0).min(theme::MAX_TILE);
        }
        HitAction::ZoomOut => {
            app.open_win_menu = None;
            app.tile_size = (app.tile_size - 20.0).max(theme::MIN_TILE);
        }
        HitAction::OpenGitHub => {
            app.open_win_menu = None;
            open_github_repo();
        }
        HitAction::SetDetailTab(tab) => {
            app.detail_tab = *tab;
            app.glyph_block_dropdown_open = false;
            app.detail_tab_scroll_y = 0.0;
            app.focus = Focus::None;
        }
        HitAction::ToggleGlyphBlockDropdown => {
            app.glyph_block_dropdown_open = !app.glyph_block_dropdown_open;
        }
        HitAction::SetGlyphBlockFilter(block) => {
            app.glyph_block_filter = *block;
            app.glyph_block_dropdown_open = false;
            app.detail_tab_scroll_y = 0.0;
        }
        HitAction::OpenSettings => {
            app.open_win_menu = None;
            app.settings_open = true;
        }
        HitAction::CloseSettings => {
            app.settings_open = false;
        }
        HitAction::ToggleSetting(which) => {
            settings::apply_toggle(app, *which);
        }
        HitAction::ToggleAppearanceDropdown => {
            app.appearance_dropdown_open = !app.appearance_dropdown_open;
        }
        HitAction::SetAppearance(appearance) => {
            app.settings.appearance = *appearance;
            app.settings.save();
            app.apply_theme();
            app.appearance_dropdown_open = false;
        }
        HitAction::ToggleTagsSubmenu => {
            if let Some(m) = app.font_context_menu.as_mut() {
                m.tags_open = !m.tags_open;
                m.new_tag_text = None;
            }
            app.focus = Focus::None;
        }
        HitAction::ToggleFontTag(font_id, tag_id) => {
            let (font_id, tag_id) = (*font_id, *tag_id);
            if app.catalog.toggle_font_tag(font_id, tag_id).is_ok() {
                app.reload_tags();
            }
        }
        HitAction::OpenNewTagField => {
            if let Some(m) = app.font_context_menu.as_mut() {
                m.new_tag_text = Some(String::new());
            }
            app.focus = Focus::NewTag;
        }
        HitAction::DismissUpdateBanner => {
            app.update_dismissed = true;
        }
        HitAction::OpenUpdateDownload => {
            open_latest_release();
        }
    }
    true
}

/// Toggles `column`'s sort: clicking the already-active column flips
/// direction, clicking a different one switches to it at that column's
/// natural default direction (newest/most-first for Styles/Favorite/Date
/// Added, alphabetical for the rest).
fn apply_sort_click(app: &mut App, column: crate::app::ListColumnKind) {
    use crate::app::ListColumnKind;
    if app.list_sort.column == column {
        app.list_sort.ascending = !app.list_sort.ascending;
        return;
    }
    let ascending = !matches!(
        column,
        ListColumnKind::Styles | ListColumnKind::Favorite | ListColumnKind::DateAdded
    );
    app.list_sort = crate::app::ListSort { column, ascending };
}

/// Called on pointer motion while `app.header_press` or
/// `app.dragging_column_reorder` is set. Promotes a press to a drag once
/// the pointer has moved far enough, then keeps `column_reorder_target`
/// current so `ui::list` can draw an insertion indicator.
pub fn update_column_drag_from_point(app: &mut App, point: Point) {
    const THRESHOLD: f64 = 4.0;

    if app.dragging_column_reorder.is_none() {
        let Some((idx, start)) = app.header_press else {
            return;
        };
        if (point.x - start.x).abs() < THRESHOLD && (point.y - start.y).abs() < THRESHOLD {
            return;
        }
        app.dragging_column_reorder = Some(idx);
        app.header_press = None;
    }

    let Some(source) = app.dragging_column_reorder else {
        return;
    };
    // How many *other* columns' midpoints the pointer has moved past —
    // exactly the dragged column's final index once it's removed and
    // reinserted, so `finish_header_interaction` can use this directly
    // with no further off-by-one adjustment for the removal shift.
    let target = app
        .list_header_rects
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != source)
        .filter(|(_, r)| (r.x0 + r.x1) / 2.0 < point.x)
        .count();
    app.column_reorder_target = Some(target);
}

/// Called on pointer motion while `app.column_resize_start` is set.
pub fn update_column_resize_from_point(app: &mut App, point: Point) {
    let Some((idx, start_x, start_width)) = app.column_resize_start else {
        return;
    };
    let Some(col) = app.list_columns.get_mut(idx) else {
        return;
    };
    let min = col.kind.min_width();
    col.width = (start_width + (point.x - start_x)).max(min);
}

/// Called on mouse release: resolves whatever header interaction was in
/// flight — a plain click becomes a sort, a drag past the threshold
/// becomes an actual column move — and clears all of the transient drag
/// state regardless of which one it was.
pub fn finish_header_interaction(app: &mut App) {
    if let Some(source) = app.dragging_column_reorder.take() {
        if let Some(target) = app.column_reorder_target.take() {
            let col = app.list_columns.remove(source);
            app.list_columns.insert(target.min(app.list_columns.len()), col);
        }
        return;
    }
    if let Some((idx, _)) = app.header_press.take() {
        if let Some(col) = app.list_columns.get(idx) {
            apply_sort_click(app, col.kind);
        }
    }
    app.column_resize_start = None;
}

/// Right-clicking a library row opens its menu; right-clicking anywhere
/// else closes whatever menu is open, same as a left click would.
pub fn handle_right_click(app: &mut App, point: Point) {
    let folder_hit = app
        .folder_rows
        .iter()
        .find(|(_, rect)| rect.contains(point))
        .map(|(id, _)| *id);
    if let Some(folder_id) = folder_hit {
        app.context_menu = Some(ContextMenu {
            folder_id,
            anchor: point,
            panel_rect: Rect::ZERO,
        });
        app.font_context_menu = None;
        return;
    }

    // A font tile/row's whole-area `SelectFont` region always covers
    // wherever the right-click landed on it — same region left-click
    // selection already uses — so no separate per-tile rect list is
    // needed just to answer "which font was this".
    let font_hit = app.hit_regions.iter().rev().find_map(|r| {
        if !r.rect.contains(point) {
            return None;
        }
        match r.action {
            HitAction::SelectFont(id) => Some(id),
            _ => None,
        }
    });
    if let Some(font_id) = font_hit {
        app.selected = Some(font_id);
        app.font_context_menu = Some(FontContextMenu {
            font_id,
            anchor: point,
            panel_rect: Rect::ZERO,
            tags_open: false,
            tags_panel_rect: Rect::ZERO,
            new_tag_text: None,
        });
        app.context_menu = None;
        return;
    }

    app.context_menu = None;
    app.font_context_menu = None;
}

/// Maps a pointer position on the rendered type-size track to a tile size.
/// Called both at drag start and on subsequent pointer motion while captured.
/// Maps a pointer position (mid-drag on the detail panel's own left-edge
/// resize handle) to `App::detail_w` — wider as the pointer moves left,
/// clamped to `[theme::DETAIL_W, App::max_detail_w(window_width)]`.
pub fn update_detail_resize_from_point(app: &mut App, point: Point, window_width: f64) {
    let max_w = app.max_detail_w(window_width);
    app.detail_w = (window_width - point.x).clamp(theme::DETAIL_W, max_w);
}

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

/// Copies a dropped file into the chosen library folder and rescans. Once
/// it's landed there it's indistinguishable from a font a teammate added
/// straight to the (externally synced) shared folder — same folder, same
/// watcher, same scan path.
fn assign_dropped_file(app: &mut App, folder_id: i64, source: &std::path::Path) {
    let Some(folder) = app.folders.iter().find(|f| f.id == folder_id) else {
        return;
    };
    let Some(file_name) = source.file_name() else {
        return;
    };
    let dest = std::path::Path::new(&folder.path).join(file_name);
    match std::fs::copy(source, &dest) {
        Ok(_) => match app.catalog.rescan() {
            Ok(result) => app.reload_folders_and_fonts(result.status),
            Err(e) => app.status = e.to_string(),
        },
        Err(e) => app.status = format!("couldn't add font: {e}"),
    }
}

pub fn toggle_favorite(app: &mut App, id: i64) {
    if let Ok(favorite) = app.catalog.toggle_favorite(id) {
        if favorite {
            app.favorite_ids.insert(id);
        } else {
            app.favorite_ids.remove(&id);
        }
        app.favorites_version += 1;
    }
}

/// Commits the Tags submenu's "New Tag…" field — creates the tag (or
/// reuses one of the same name, since `Store::create_tag` is `INSERT OR
/// IGNORE`), applies it to the menu's own font, and drops back to the
/// submenu's normal tag list. A blank/whitespace-only name is a no-op,
/// same as every other empty-input-does-nothing field in this app.
pub fn confirm_new_tag(app: &mut App) {
    let Some(font_id) = app.font_context_menu.as_ref().map(|m| m.font_id) else {
        return;
    };
    let name = app
        .font_context_menu
        .as_ref()
        .and_then(|m| m.new_tag_text.as_deref())
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if !name.is_empty() {
        if let Ok(tag_id) = app.catalog.create_tag(&name) {
            let _ = app.catalog.toggle_font_tag(font_id, tag_id);
            app.reload_tags();
        }
    }
    if let Some(m) = app.font_context_menu.as_mut() {
        m.new_tag_text = None;
    }
    app.focus = Focus::None;
}

/// The "Add Library" flow — a folder picker, then indexing whatever it
/// finds. Shared by the toolbar button (`HitAction::AddFolder`) and the
/// macOS menu bar's File ▸ Add Library\u{2026}.
pub fn add_library(app: &mut App) {
    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
        let path = dir.to_string_lossy().into_owned();
        match app.catalog.add_folder(&path) {
            Ok(result) => app.reload_folders_and_fonts(result.status),
            Err(e) => app.status = e.to_string(),
        }
    }
}

/// The manual "no really, check right now" sync — see `Catalog::
/// force_sync`. Shared by the toolbar's refresh button
/// (`HitAction::Rescan`) and the macOS menu bar's File ▸ Sync Now.
pub fn sync_now(app: &mut App) {
    app.status = "Syncing\u{2026}".to_string();
    match app.catalog.force_sync() {
        Ok(result) => app.reload_folders_and_fonts(result.status),
        Err(e) => app.status = e.to_string(),
    }
}

/// The Font-menu actions below all act on `app.selected` and are shared
/// between the macOS native menu bar (`native_menu.rs`) and the
/// hand-rolled Windows one — every real item on either one routes through
/// exactly one of these, never a copy.
fn selected_is_system(app: &App, id: i64) -> bool {
    app.entry(id).is_some_and(|e| e.is_system)
}

/// Font ▸ Activate — explicit, unlike the toolbar dot's toggle: a no-op if
/// already active.
pub fn activate_selected(app: &mut App) {
    let Some(id) = app.selected else { return };
    if selected_is_system(app, id) || app.active_ids.contains(&id) {
        return;
    }
    if app.catalog.toggle_activation(id).unwrap_or(false) {
        app.active_ids.insert(id);
    }
}

/// Font ▸ Deactivate — explicit, unlike the toolbar dot's toggle: a no-op
/// if already inactive.
pub fn deactivate_selected(app: &mut App) {
    let Some(id) = app.selected else { return };
    if selected_is_system(app, id) || !app.active_ids.contains(&id) {
        return;
    }
    if !app.catalog.toggle_activation(id).unwrap_or(true) {
        app.active_ids.remove(&id);
    }
}

/// Font ▸ Activate Temporarily — see `Catalog::activate_temporarily` and
/// `App::temp_active_ids`, which is what makes the "temporarily" part real
/// (uninstalled on quit rather than persisted).
pub fn activate_selected_temporarily(app: &mut App) {
    let Some(id) = app.selected else { return };
    if selected_is_system(app, id) {
        return;
    }
    if app.catalog.activate_temporarily(id).unwrap_or(false) {
        app.active_ids.insert(id);
        app.temp_active_ids.insert(id);
    }
}

pub fn toggle_selected_favorite(app: &mut App) {
    if let Some(id) = app.selected {
        toggle_favorite(app, id);
    }
}

/// Font ▸ Export… — copies the selected font's file out to wherever the
/// user picks, e.g. to hand a single font to someone outside the library.
pub fn export_selected(app: &mut App) {
    let Some(entry) = app.selected.and_then(|id| app.entry(id)) else {
        return;
    };
    let source = std::path::PathBuf::from(&entry.path);
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("{}.ttf", entry.family));
    if let Some(dest) = rfd::FileDialog::new().set_file_name(&file_name).save_file() {
        if let Err(e) = std::fs::copy(&source, &dest) {
            app.status = format!("couldn't export: {e}");
        }
    }
}

/// Font ▸ Show in Finder — the selected font's specific *file*, not its
/// library folder (that's the sidebar row's own context-menu action).
pub fn reveal_selected(app: &App) {
    if let Some(entry) = app.selected.and_then(|id| app.entry(id)) {
        let _ = crate::reveal::reveal_file(std::path::Path::new(&entry.path));
    }
}

/// Font ▸ Remove from Fontlist — see `Catalog::remove_from_fontlist`.
pub fn remove_selected_from_fontlist(app: &mut App) {
    let Some(id) = app.selected else { return };
    if selected_is_system(app, id) {
        return;
    }
    match app.catalog.remove_from_fontlist(id) {
        Ok(()) => {
            app.selected = None;
            app.reload_folders_and_fonts("Removed from fontlist".to_string());
        }
        Err(e) => app.status = e.to_string(),
    }
}

/// Font ▸ Delete from Library — only ever opens the confirmation overlay
/// (`ui::draw_delete_confirm`); the real, unrecoverable delete happens on
/// `HitAction::ConfirmDelete`, never from a menu click alone.
pub fn request_delete_selected(app: &mut App) {
    let Some(id) = app.selected else { return };
    if !selected_is_system(app, id) {
        app.confirm_delete = Some(id);
    }
}

/// Help ▸ GlyphClub on GitHub — shared by the macOS menu and the
/// hand-rolled Windows one.
pub fn open_github_repo() {
    open_url("https://github.com/tonykastaneda/GlyphClub");
}

/// The update banner's "Download" button — same releases-page link the
/// docs site's own download buttons ultimately point at, rather than
/// pulling a file straight into the running app (no self-updater here,
/// see the packaging discussion this shipped with for why that's out of
/// scope for now).
pub fn open_latest_release() {
    open_url("https://github.com/tonykastaneda/GlyphClub/releases/latest");
}

fn open_url(url: &str) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd").args(["/C", "start", url]).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Uninstalls every temporarily-activated font — the one real bit of
/// cleanup GlyphClub needs before the process actually ends, which is why
/// `main.rs` routes every quit path (traffic-light close, `Cmd+Q`, the
/// menu's Quit item, `WindowEvent::CloseRequested`) through this before
/// calling `event_loop.exit()`, rather than any of them exiting directly.
pub fn cleanup_before_quit(app: &mut App) {
    for id in std::mem::take(&mut app.temp_active_ids) {
        let _ = app.catalog.force_deactivate(id);
    }
}
