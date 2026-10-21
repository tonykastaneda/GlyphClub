//! The icon browser (sidebar ▸ Icon Fonts): a named grid of a set's icons.
//! Drag an icon out of the window and it arrives as an SVG file of real
//! vector paths (Illustrator places it as editable artwork); right-click to
//! copy it as SVG, the character or its name.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use glyphclub::icons::{icon_svg, IconSet};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

use crate::app::GlyphClub;
use crate::palette::Palette;

/// What an icon drag carries: enough to write its SVG when it leaves the
/// window.
#[derive(Clone)]
pub struct IconDrag {
    pub font: Arc<Vec<u8>>,
    pub family: SharedString,
    pub codepoint: u32,
    pub name: SharedString,
}

/// The little preview that follows the pointer inside the window.
pub struct IconDragPreview(IconDrag);

impl Render for IconDragPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::get(cx);
        div()
            .size(px(56.))
            .rounded(px(10.))
            .bg(pal.tile)
            .border_1()
            .border_color(pal.select)
            .flex()
            .items_center()
            .justify_center()
            .font_family(self.0.family.clone())
            .text_size(px(30.))
            .text_color(pal.preview)
            .child(char::from_u32(self.0.codepoint).map(String::from).unwrap_or_default())
    }
}

/// Writes the icon as an SVG file in a temp folder, for dragging out.
fn write_svg(d: &IconDrag) -> Option<PathBuf> {
    let svg = icon_svg(&d.font, d.codepoint, 64.)?;
    let dir = std::env::temp_dir().join("GlyphClub Icons");
    std::fs::create_dir_all(&dir).ok()?;
    let safe: String = d
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' })
        .collect();
    let path = dir.join(format!("{}.svg", safe.trim()));
    std::fs::write(&path, svg).ok()?;
    Some(path)
}

pub fn copy_svg(d: &IconDrag, cx: &mut App) -> bool {
    match icon_svg(&d.font, d.codepoint, 64.) {
        Some(svg) => {
            cx.write_to_clipboard(ClipboardItem::new_string(svg));
            true
        }
        None => false,
    }
}

impl GlyphClub {
    /// The set's font bytes, loaded into gpui's text system on first use.
    pub fn icon_font(&mut self, set: usize, cx: &mut Context<Self>) -> Option<Arc<Vec<u8>>> {
        if let Some(data) = self.icon_font_data.get(&set) {
            return Some(data.clone());
        }
        let path = self.icon_sets.get(set)?.font_path.clone();
        let data = Arc::new(std::fs::read(path).ok()?);
        let _ = cx.text_system().add_fonts(vec![std::borrow::Cow::Owned((*data).clone())]);
        self.icon_font_data.insert(set, data.clone());
        Some(data)
    }

    /// Indices into the set's icons matching the search.
    pub fn icon_matches(&self, set: &IconSet, cx: &App) -> Vec<usize> {
        let q = self.search.read(cx).value().trim().to_lowercase();
        set.icons
            .iter()
            .enumerate()
            .filter(|(_, i)| q.is_empty() || i.name.to_lowercase().contains(&q))
            .map(|(k, _)| k)
            .collect()
    }

    pub fn render_icons(&mut self, set_ix: usize, pal: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(set) = self.icon_sets.get(set_ix).cloned() else {
            return div().into_any_element();
        };
        let Some(font) = self.icon_font(set_ix, cx) else {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(pal.text_dim)
                .child("Couldn't read this icon font.")
                .into_any_element();
        };
        let matches = self.icon_matches(&set, cx);
        let s = cx.global::<crate::settings::AppSettings>().0;

        // Cells grow with the size slider.
        let icon_px = (s.preview_size * 0.5).clamp(20., 72.);
        let cell = (icon_px * 2.6).max(84.);
        let mut avail = f32::from(window.viewport_size().width) - 28.;
        if self.sidebar_shown {
            avail -= s.sidebar_width.clamp(crate::chrome::SIDEBAR_MIN_W, crate::chrome::SIDEBAR_MAX_W);
        }
        if self.info_open {
            avail -= crate::detail::detail_width(cx);
        }
        let gap = 10.;
        let cols = ((avail + gap) / (cell + gap)).floor().max(1.) as usize;
        let rows = matches.len().div_ceil(cols);
        let pal = *pal;
        let family: SharedString = set.family.clone().into();
        let selected = self.selected_icon.filter(|(s, _)| *s == set_ix).map(|(_, i)| i);

        let header = div()
            .flex_shrink_0()
            .px(px(16.))
            .pb(px(10.))
            .flex()
            .items_baseline()
            .gap(px(10.))
            .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(set.name.clone()))
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(pal.text_dim).child(format!(
                "{} icon{}{}",
                matches.len(),
                if matches.len() == 1 { "" } else { "s" },
                match (set.version.is_empty(), set.license.is_empty()) {
                    (false, false) => format!(" · v{} · {}", set.version, set.license),
                    _ => String::new(),
                }
            )));

        let set_arc = set.clone();
        let list = uniform_list(
            "icons",
            rows,
            cx.processor(move |this, range: Range<usize>, _window, cx| {
                range
                    .map(|r| {
                        let mut row = div().w_full().px(px(14.)).pb(px(gap)).flex().gap(px(gap));
                        for &k in matches.iter().skip(r * cols).take(cols) {
                            let icon = &set_arc.icons[k];
                            let drag = IconDrag {
                                font: font.clone(),
                                family: family.clone(),
                                codepoint: icon.codepoint,
                                name: icon.name.clone().into(),
                            };
                            row = row.child(this.icon_cell(set_ix, k, drag, selected == Some(k), cell, icon_px, &pal, cx));
                        }
                        row
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.icon_scroll)
        .size_full()
        .pb(px(24.));

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("icon-area")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .vertical_scrollbar(&self.icon_scroll),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(16.))
                    .py(px(8.))
                    .text_size(px(11.))
                    .text_color(pal.text_dim)
                    .child("Drag an icon into Illustrator for editable vectors, or right-click to copy it as SVG."),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn icon_cell(
        &self,
        set_ix: usize,
        k: usize,
        drag: IconDrag,
        selected: bool,
        cell: f32,
        icon_px: f32,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let glyph = char::from_u32(drag.codepoint).map(String::from).unwrap_or_default();
        let name = drag.name.clone();
        let (d_menu, d_click) = (drag.clone(), drag.clone());
        div()
            .id(("icon", k))
            .w(px(cell))
            .h(px(cell))
            .flex_shrink_0()
            .rounded(px(10.))
            .bg(pal.tile)
            .border(px(if selected { 2. } else { 1. }))
            .border_color(if selected { pal.select } else { pal.border })
            .hover(|s| s.bg(pal.hover))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(
                div()
                    .font_family(drag.family.clone())
                    .text_size(px(icon_px))
                    .line_height(px(icon_px * 1.1))
                    .text_color(pal.preview)
                    .child(glyph),
            )
            .child(
                div()
                    .max_w(px(cell - 10.))
                    .truncate()
                    .text_size(px(11.))
                    .text_color(pal.text_dim)
                    .child(name.clone()),
            )
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                this.selected_icon = Some((set_ix, k));
                if ev.click_count() == 2 {
                    if copy_svg(&d_click, cx) {
                        this.notify(format!("Copied “{}” as SVG", d_click.name));
                    }
                }
                cx.notify();
            }))
            .on_drag(drag, |d: &IconDrag, _, _, cx| cx.new(|_| IconDragPreview(d.clone())))
            .external_drag_payload(|d: &IconDrag, _, _| {
                let path = write_svg(d)?;
                Some(ExternalDragPayload::Files(FileDragPaths::new([(path, false)])))
            })
            .context_menu(move |menu, _, _| {
                let (a, b, c, e) = (d_menu.clone(), d_menu.clone(), d_menu.clone(), d_menu.clone());
                menu.item(PopupMenuItem::new("Copy as SVG").on_click(move |_, _, cx| {
                    copy_svg(&a, cx);
                }))
                .item(PopupMenuItem::new("Copy Character").on_click(move |_, _, cx| {
                    let ch = char::from_u32(b.codepoint).map(String::from).unwrap_or_default();
                    cx.write_to_clipboard(ClipboardItem::new_string(ch));
                }))
                .item(PopupMenuItem::new("Copy Name").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(c.name.to_string()));
                }))
                .separator()
                .item(PopupMenuItem::new("Show SVG in Finder").on_click(move |_, _, _| {
                    if let Some(path) = write_svg(&e) {
                        let _ = glyphclub::reveal::reveal_file(&path);
                    }
                }))
            })
            .into_any_element()
    }
}
