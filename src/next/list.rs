//! The list view, laid out like RightFont's: compact striped rows, one per
//! family (expandable into its faces), sortable by clicking a column header.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::app::GlyphClub;
use crate::palette::Palette;
use crate::settings::{AppSettings, SortField};

const ROW_H: f32 = 26.;
const PREVIEW_PX: f32 = 15.;

// Column widths; Preview takes the rest.
const W_DOT: f32 = 22.;
const W_NAME: f32 = 230.;
const W_STYLES: f32 = 76.;
const W_STAR: f32 = 40.;
const W_FORMAT: f32 = 160.;
const W_CATEGORY: f32 = 110.;
const W_ADDED: f32 = 190.;
const FIXED_W: f32 = W_DOT + W_NAME + W_STYLES + W_STAR + W_FORMAT + W_CATEGORY + W_ADDED;

#[derive(Clone, Copy)]
enum Row {
    Family(usize),
    Face(usize, usize),
}

impl GlyphClub {
    fn list_rows(&self) -> Vec<Row> {
        let expand_all = self.expand_all;
        let mut rows = Vec::with_capacity(self.visible.len());
        for &i in &self.visible {
            rows.push(Row::Family(i));
            let f = &self.lib.families[i];
            if f.faces.len() > 1 && (expand_all || self.expanded.contains(&f.key)) {
                rows.extend((0..f.faces.len()).map(|k| Row::Face(i, k)));
            }
        }
        rows
    }

    fn header_cell(
        &self,
        id: &'static str,
        label: impl IntoElement,
        field: Option<SortField>,
        width: Option<f32>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let s = cx.global::<AppSettings>().0;
        let current = field == Some(s.sort_field);
        div()
            .id(id)
            .h_full()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .border_l_1()
            .border_color(pal.border)
            .map(|d| match width {
                Some(w) => d.w(px(w)).flex_shrink_0(),
                None => d.flex_1().min_w_0(),
            })
            .text_color(pal.text)
            .child(label)
            .child(div().flex_1())
            .when(current, |d| {
                d.child(
                    Icon::new(if s.sort_ascending { IconName::ChevronUp } else { IconName::ChevronDown })
                        .size(px(11.))
                        .text_color(pal.text_dim),
                )
            })
            .when_some(field, |d, field| {
                d.cursor_pointer()
                    .hover(|s| s.bg(pal.hover))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_sort(field, cx)))
            })
    }

    pub fn render_list(&mut self, pal: &Palette, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<AppSettings>().0;
        // The Preview column gets what's left after the fixed columns.
        let mut avail = f32::from(window.viewport_size().width) - 16.;
        if self.sidebar_open {
            avail -= s.sidebar_width.clamp(crate::chrome::SIDEBAR_MIN_W, crate::chrome::SIDEBAR_MAX_W);
        }
        if self.info_open {
            avail -= crate::detail::detail_width(cx);
        }
        let preview_w = (avail - FIXED_W - 16.).max(30.);

        let rows = self.list_rows();
        let count = rows.len();
        let sample = self.sample_text(cx);
        let chars = self.sample_chars(cx);
        let pal = *pal;

        let header = div()
            .h(px(24.))
            .mx(px(8.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(pal.border)
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .child(self.header_cell("h-dot", div(), Some(SortField::Enabled), Some(W_DOT), &pal, cx).border_l_0())
            .child(self.header_cell("h-name", "Name", Some(SortField::Name), Some(W_NAME), &pal, cx))
            .child(self.header_cell("h-preview", "Preview", None, None, &pal, cx))
            .child(self.header_cell("h-styles", "Styles", Some(SortField::Styles), Some(W_STYLES), &pal, cx))
            .child(self.header_cell(
                "h-star",
                Icon::new(IconName::Star).size(px(12.)),
                Some(SortField::Starred),
                Some(W_STAR),
                &pal,
                cx,
            ))
            .child(self.header_cell("h-format", "Format", Some(SortField::Format), Some(W_FORMAT), &pal, cx))
            .child(self.header_cell("h-category", "Category", Some(SortField::Category), Some(W_CATEGORY), &pal, cx))
            .child(self.header_cell("h-added", "Date Added", Some(SortField::DateAdded), Some(W_ADDED), &pal, cx));

        let list = uniform_list(
            "list",
            count,
            cx.processor(move |this, range: Range<usize>, window, cx| {
                let rows = this.list_rows();
                let wanted: Vec<_> = range
                    .clone()
                    .filter_map(|r| rows.get(r).copied())
                    .map(|row| match row {
                        Row::Family(i) => this.lib.families[i].rep(),
                        Row::Face(i, k) => &this.lib.families[i].faces[k],
                    })
                    .map(|f| (f.id, f.path.clone()))
                    .collect();
                this.request_fonts(wanted, cx);
                range
                    .filter_map(|r| rows.get(r).copied().map(|row| (r, row)))
                    .map(|(r, row)| this.list_row(r, row, sample.clone(), &chars, preview_w, &pal, window, cx))
                    .collect()
            }),
        )
        .track_scroll(&self.list_scroll)
        .size_full()
        .pb(px(if s.show_sample_bar { 110. } else { 8. }));

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("list-area")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .pt(px(2.))
                    .child(list)
                    .vertical_scrollbar(&self.list_scroll),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn list_row(
        &self,
        r: usize,
        row: Row,
        sample: SharedString,
        chars: &[u32],
        preview_w: f32,
        pal: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (i, face_k) = match row {
            Row::Family(i) => (i, None),
            Row::Face(i, k) => (i, Some(k)),
        };
        let f = &self.lib.families[i];
        let key = f.key.clone();
        let face = face_k.map(|k| &f.faces[k]).unwrap_or(f.rep());
        let selected = self.selected.contains(&key) && face_k.is_none();
        let active = match face_k {
            Some(_) => self.lib.active.contains(&face.id),
            None => self.lib.family_active(f),
        };
        let starred = self.lib.family_starred(f);
        let temp = self.temporary.contains(&face.id);
        let ready = self.loaded.contains(&face.id);
        let missing = face_k.is_none() && self.lib.missing_chars(f, chars);
        let expandable = f.faces.len() > 1 && face_k.is_none();
        let expanded = self.expand_all || self.expanded.contains(&key);
        let group: SharedString = format!("row-{r}").into();
        let when = if face_k.is_some() { face.mtime } else { f.newest_mtime };
        let added = chrono::DateTime::from_timestamp(when, 0)
            .map(|d| d.with_timezone(&chrono::Local).format("%b %-d, %Y at %-I:%M %p").to_string())
            .unwrap_or_default();
        let category = self.categories.get(&face.id).copied().unwrap_or("");
        let fg = if selected { white() } else { pal.text };
        let fg_dim = if selected { white().opacity(0.85) } else { pal.text_dim };

        let sample = if ready {
            let font = crate::fit::preview_font(&f.name, face.weight, face.italic, FontFeatures::default());
            crate::fit::fit(window, &sample, font, PREVIEW_PX, preview_w)
        } else {
            sample
        };

        let cell = |w: f32| div().w(px(w)).flex_shrink_0().px(px(8.)).overflow_hidden().whitespace_nowrap();

        let name_cell = cell(W_NAME)
            .flex()
            .items_center()
            .gap(px(2.))
            .when(face_k.is_some(), |d| d.pl(px(26.)))
            .when(face_k.is_none(), |d| {
                if expandable {
                    let key = key.clone();
                    d.child(
                        div()
                            .id(("expand", r))
                            .size(px(14.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                Icon::new(if expanded { IconName::ChevronDown } else { IconName::ChevronRight })
                                    .size(px(11.))
                                    .text_color(fg_dim),
                            )
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if !this.expanded.remove(&key) {
                                    this.expanded.insert(key.clone());
                                }
                                cx.notify();
                            })),
                    )
                } else {
                    d.child(div().w(px(14.)))
                }
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .text_color(fg)
                    .child(match face_k {
                        Some(_) => face.style.to_string(),
                        None => f.name.to_string(),
                    }),
            );

        let preview = div()
            .flex_1()
            .min_w_0()
            .px(px(8.))
            .overflow_hidden()
            .whitespace_nowrap()
            .when(ready, |d| {
                d.child(
                    div()
                        .font_family(f.name.clone())
                        .font_weight(FontWeight(face.weight as f32))
                        .when(face.italic, |d| d.italic())
                        .text_size(px(PREVIEW_PX))
                        .line_height(px(ROW_H - 4.))
                        .text_color(fg)
                        .when(missing, |d| d.opacity(0.22))
                        .child(sample),
                )
            });

        let face_id = face.id;
        let dot = if face_k.is_some() {
            div()
                .id(("face-dot", r))
                .size(px(14.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .child(
                    div()
                        .size(px(9.))
                        .rounded_full()
                        .bg(if active { pal.active_dot } else { pal.inactive_dot })
                        .when(temp, |d| d.opacity(0.55)),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    let on = !this.lib.active.contains(&face_id);
                    this.set_active(vec![face_id], on, cx);
                }))
                .into_any_element()
        } else {
            let any_temp = f.ids().any(|id| self.temporary.contains(&id));
            self.activation_dot(("dot", r), key.clone(), active, any_temp, pal, cx).into_any_element()
        };

        let star_key = key.clone();
        let star = div()
            .id(("lstar", r))
            .when(face_k.is_none() && !starred, |d| d.invisible().group_hover(group.clone(), |s| s.visible()))
            .when(face_k.is_some(), |d| d.invisible())
            .child(
                Icon::new(if starred { IconName::StarFill } else { IconName::Star })
                    .size(px(12.))
                    .text_color(if starred { rgb(0xf5c242).into() } else { fg_dim }),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_star(Some(star_key.clone()), cx);
            }));

        let stripe = r % 2 == 1;
        let row = div()
            .id(("row", r))
            .group(group)
            .w_full()
            .h(px(ROW_H))
            .flex()
            .items_center()
            .rounded(px(5.))
            .text_size(px(12.))
            .when(selected, |d| d.bg(pal.select))
            .when(!selected && stripe, |d| d.bg(pal.stripe))
            .when(!selected, |d| d.hover(|s| s.bg(pal.hover)))
            .child(div().w(px(W_DOT)).flex_shrink_0().flex().justify_center().child(dot))
            .child(name_cell)
            .child(preview)
            .child(
                cell(W_STYLES).text_color(fg_dim).child(if face_k.is_none() {
                    let n = f.faces.len();
                    format!("{n} style{}", if n == 1 { "" } else { "s" })
                } else {
                    String::new()
                }),
            )
            .child(cell(W_STAR).flex().justify_center().child(star))
            .child(cell(W_FORMAT).text_color(fg_dim).child(f.format))
            .child(cell(W_CATEGORY).text_color(fg_dim).child(category))
            .child(cell(W_ADDED).text_color(fg_dim).child(added))
            .on_click(cx.listener({
                let key = key.clone();
                move |this, ev: &ClickEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.select_click(key.clone(), ev.modifiers(), cx);
                    if face_k.is_some() {
                        this.detail_face = Some(face_id);
                    }
                    // Double-click opens the info panel on it.
                    if ev.click_count() == 2 {
                        this.info_open = true;
                        this.refresh_menus(cx);
                    }
                }
            }))
            .context_menu(self.font_menu(key, active, starred, cx));
        div().w_full().px(px(8.)).child(row).into_any_element()
    }
}
