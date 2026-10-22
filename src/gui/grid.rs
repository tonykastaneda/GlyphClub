//! The grid view: equal columns whose width follows the sample text, like
//! RightFont's.

use std::ops::Range;

use gpui_kit::assets::IconName;
use gpui_kit::component::menu::{ContextMenuExt, PopupMenu, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::Icon;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::app::GlyphClub;
use crate::chrome::{SIDEBAR_MAX_W, SIDEBAR_MIN_W};
use crate::detail::detail_width;
use crate::palette::Palette;
use crate::settings::AppSettings;

pub const GRID_GAP: f32 = 12.;

impl GlyphClub {
    pub fn render_grid(&mut self, pal: &Palette, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<AppSettings>().0;
        let sample = self.sample_text(cx);
        let chars = self.sample_chars(cx);

        // Every column is the same width, and that width follows the
        // content: the sample text at the current size, at an average glyph
        // width. "Aa" gives many narrow columns, a long word a few wide ones.
        let mut avail = f32::from(window.viewport_size().width) - 28.;
        if self.sidebar_shown {
            avail -= s.sidebar_width.clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W);
        }
        if self.info_open {
            avail -= detail_width(cx);
        }
        let avail = avail.max(200.);
        let font_px = s.preview_size * 1.25;
        // With no preview text each tile shows its font's name; size the
        // columns for a typical one.
        let n_chars = sample.as_ref().map(|s| s.chars().count()).unwrap_or(12).max(2) as f32;
        let min_tile = (n_chars * font_px * 0.62 + 40.).max(220.);
        let cols = ((avail + GRID_GAP) / (min_tile + GRID_GAP)).floor().max(1.) as usize;
        // Expand Font Families: every style gets its own tile.
        self.grid_items = if s.expand_families {
            self.visible
                .iter()
                .flat_map(|&i| (0..self.lib.families[i].faces.len()).map(move |k| (i, Some(k))))
                .collect()
        } else {
            self.visible.iter().map(|&i| (i, None)).collect()
        };
        self.last_cols = cols;
        let rows = self.grid_items.len().div_ceil(cols);
        let tile_h = 64. + s.preview_size * 1.75;
        // The room a preview has inside a tile: its share of the row, less
        // the tile's padding and border.
        let preview_w = (avail - GRID_GAP * (cols as f32 - 1.)) / cols as f32 - 26.;
        let pal = *pal;

        let list = uniform_list(
            "grid",
            rows,
            cx.processor(move |this, range: Range<usize>, window, cx| {
                let wanted: Vec<_> = range
                    .clone()
                    .flat_map(|r| this.grid_items.iter().skip(r * cols).take(cols))
                    .map(|&(i, k)| {
                        let fam = &this.lib.families[i];
                        let f = k.map(|k| &fam.faces[k]).unwrap_or(fam.rep());
                        (f.id, f.path.clone())
                    })
                    .collect();
                this.request_fonts(wanted, cx);
                range
                    .map(|r| {
                        let mut row = div()
                            .w_full()
                            .h(px(tile_h + GRID_GAP))
                            .px(px(14.))
                            .pb(px(GRID_GAP))
                            .flex()
                            .gap(px(GRID_GAP));
                        for k in 0..cols {
                            let pos = r * cols + k;
                            row = match this.grid_items.get(pos).copied() {
                                Some((i, face_k)) => row.child(this.tile(pos, i, face_k, sample.clone(), &chars, tile_h, font_px, preview_w, &pal, window, cx)),
                                None => row.child(div().flex_1().flex_basis(px(0.))),
                            };
                        }
                        row
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.grid_scroll)
        .size_full()
        // Room to scroll the last row clear of the floating sample bar.
        .pb(px(if s.show_sample_bar { 110. } else { 14. }));

        div()
            .id("grid-area")
            .relative()
            .size_full()
            .child(list)
            .vertical_scrollbar(&self.grid_scroll)
    }

    #[allow(clippy::too_many_arguments)]
    fn tile(
        &self,
        pos: usize,
        idx: usize,
        face_k: Option<usize>,
        sample: Option<SharedString>,
        chars: &[u32],
        tile_h: f32,
        font_px: f32,
        preview_w: f32,
        pal: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let f = &self.lib.families[idx];
        let key = f.key.clone();
        let rep = face_k.map(|k| &f.faces[k]).unwrap_or(f.rep());
        let selected = self.selected.contains(&key);
        let active = self.lib.family_active(f);
        let starred = self.lib.family_starred(f);
        let ready = self.loaded.contains(&rep.id);
        let missing = self.lib.missing_chars(f, chars);
        let any_temp = f.ids().any(|id| self.temporary.contains(&id));
        let label = match face_k {
            Some(_) => format!("{} {}", f.name, rep.style),
            None if f.faces.len() > 1 => format!("{} ({})", f.name, f.faces.len()),
            None => f.name.to_string(),
        };
        let group: SharedString = format!("tile-{pos}").into();
        let face_id = rep.id;
        let sample: SharedString = sample.unwrap_or_else(|| match face_k {
            Some(_) => format!("{} {}", f.name, rep.style).into(),
            None => f.name.clone(),
        });
        let sample = if ready {
            let font = crate::fit::preview_font(&f.name, rep.weight, rep.italic, FontFeatures::default());
            crate::fit::fit(window, &sample, font, font_px, preview_w)
        } else {
            sample
        };

        let preview = div()
            .whitespace_nowrap()
            .font_family(f.name.clone())
            .font_weight(FontWeight(rep.weight as f32))
            .when(rep.italic, |d| d.italic())
            .text_size(px(font_px))
            .line_height(px(font_px * 1.28))
            .text_color(pal.preview)
            // A face without every sample character falls back for those;
            // dim it so it doesn't pass for the real thing.
            .when(missing, |d| d.opacity(0.22))
            .child(sample);

        div()
            .id(("tile", pos))
            .group(group.clone())
            .flex_1()
            .flex_basis(px(0.))
            .min_w_0()
            .h(px(tile_h))
            .rounded(px(10.))
            .bg(pal.tile)
            .border(px(if selected { 2. } else { 1. }))
            .border_color(if selected { pal.select } else { pal.border })
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .h(px(28.))
                    .pl(px(10.))
                    .pr(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(self.activation_dot(("dot", pos), key.clone(), active, any_temp, pal, cx))
                    .child(format_badge(f.format, pal))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(11.))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(pal.text_dim)
                            .child(label),
                    )
                    .child(self.hover_actions(pos, &group, key.clone(), starred, pal, cx)),
            )
            .child(
                div()
                    .flex_1()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .when(ready, |d| d.child(preview)),
            )
            .on_click(cx.listener({
                let key = key.clone();
                move |this, ev: &ClickEvent, window, cx| {
                    window.focus(&this.focus, cx);
                    this.select_click(key.clone(), ev.modifiers(), cx);
                    if face_k.is_some() {
                        this.detail_face = Some(face_id);
                    }
                    // Double-click opens the info panel on it.
                    if ev.click_count() == 2 && !ev.modifiers().platform && !ev.modifiers().shift {
                        this.info_open = true;
                        this.refresh_menus(cx);
                    }
                }
            }))
            .context_menu(self.font_menu(key, active, starred, cx))
            .into_any_element()
    }

    /// The activation dot: green when active (dimmer when only temporarily),
    /// click to toggle.
    pub fn activation_dot(
        &self,
        id: impl Into<ElementId>,
        key: SharedString,
        active: bool,
        temporary: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .size(px(16.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .hover(|s| s.bg(pal.hover))
            .child(
                div()
                    .size(px(7.))
                    .rounded_full()
                    .bg(if active { pal.active_dot } else { pal.inactive_dot })
                    .when(temporary, |d| d.opacity(0.55)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                cx.stop_propagation();
                this.toggle_active(Some(key.clone()), cx);
            }))
    }

    /// The Illustrator and star buttons that show on hover (the star stays
    /// when the family is starred).
    pub fn hover_actions(
        &self,
        idx: usize,
        group: &SharedString,
        key: SharedString,
        starred: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let g1 = group.clone();
        let g2 = group.clone();
        div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                div()
                    .id(("ai", idx))
                    .invisible()
                    .group_hover(g1, |s| s.visible())
                    .h(px(16.))
                    .px(px(3.))
                    .rounded(px(3.))
                    .border_1()
                    .border_color(rgb(0xff9a00))
                    .bg(rgb(0x330000))
                    .flex()
                    .items_center()
                    .text_size(px(9.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xff9a00))
                    .child("Ai")
                    .tooltip(|window, cx| Tooltip::new("Apply to the selected text in Illustrator").build(window, cx))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener({
                        let key = key.clone();
                        move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.apply_in_illustrator(key.clone(), cx);
                        }
                    })),
            )
            .child(
                div()
                    .id(("star", idx))
                    .when(!starred, |d| d.invisible().group_hover(g2, |s| s.visible()))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|s| s.bg(pal.hover))
                    .child(
                        Icon::new(if starred { IconName::StarFill } else { IconName::Star })
                            .size(px(13.))
                            .text_color(if starred { rgb(0xf5c242).into() } else { pal.text_dim }),
                    )
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_star(Some(key.clone()), cx);
                    })),
            )
    }

    /// The right-click menu shared by grid tiles and list rows.
    pub fn font_menu(
        &self,
        key: SharedString,
        active: bool,
        starred: bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
        let weak = cx.entity().downgrade();
        let tags: Vec<(i64, String)> = self.lib.tags.iter().map(|t| (t.id, t.name.clone())).collect();
        let family_tags = self
            .family_index(&key)
            .map(|i| self.lib.family_tags(&self.lib.families[i]))
            .unwrap_or_default();
        move |menu, window, cx| {
            let Some(view) = weak.upgrade() else { return menu };
            // Right-clicking outside the selection acts on that one alone.
            view.update(cx, |this, cx| {
                if !this.selected.contains(&key) {
                    this.selected.clear();
                    this.selected.insert(key.clone());
                    cx.notify();
                }
            });
            let item = |label: &'static str, f: fn(&mut GlyphClub, Option<SharedString>, &mut Window, &mut Context<GlyphClub>)| {
                let (w, k) = (weak.clone(), key.clone());
                PopupMenuItem::new(label).on_click(move |_, window, cx| {
                    let _ = w.update(cx, |this, cx| f(this, Some(k.clone()), window, cx));
                })
            };
            let (wt, kt, tags, family_tags) = (weak.clone(), key.clone(), tags.clone(), family_tags.clone());
            menu.item(if active {
                item("Deactivate", |this, k, _, cx| this.toggle_active(k, cx))
            } else {
                item("Activate", |this, k, _, cx| this.toggle_active(k, cx))
            })
            .item(item("Activate Temporarily", |this, k, _, cx| this.activate_temporarily(k, cx)).disabled(active))
            .item(item("Apply in Illustrator", |this, k, _, cx| {
                if let Some(k) = k {
                    this.apply_in_illustrator(k, cx)
                }
            }))
            .separator()
            .item(if starred {
                item("Unstar", |this, k, _, cx| this.toggle_star(k, cx))
            } else {
                item("Star", |this, k, _, cx| this.toggle_star(k, cx))
            })
            .submenu("Tags", window, cx, move |mut menu, _, _| {
                for (id, name) in &tags {
                    let (w, k, id) = (wt.clone(), kt.clone(), *id);
                    menu = menu.item(
                        PopupMenuItem::new(name.clone())
                            .checked(family_tags.contains(&id))
                            .on_click(move |_, _, cx| {
                                let _ = w.update(cx, |this, cx| this.toggle_tag(Some(k.clone()), id, cx));
                            }),
                    );
                }
                if !tags.is_empty() {
                    menu = menu.separator();
                }
                let (w, k) = (wt.clone(), kt.clone());
                menu.item(PopupMenuItem::new("New Tag…").on_click(move |_, window, cx| {
                    crate::chrome::open_new_tag_dialog(w.clone(), Some(k.clone()), window, cx);
                }))
            })
            .item(item("Copy Font Name", |this, k, _, cx| this.copy_names(k, cx)))
            .separator()
            .item(item("Export…", |this, k, _, cx| this.export(k, cx)))
            .item(item("Show in Finder", |this, k, _, _| this.reveal(k)))
            .separator()
            .item(item("Remove from Fontlist", |this, k, _, cx| this.remove_from_fontlist(k, cx)))
            .item(item("Delete from Library…", |this, k, window, cx| this.confirm_delete(k, window, cx)))
        }
    }
}

/// RightFont-style format mark: an italic O for OpenType, Tт for TrueType.
pub fn format_badge(format: &str, pal: &Palette) -> impl IntoElement {
    let (label, italic) = if format.starts_with("OpenType") {
        ("O", true)
    } else if format.starts_with("TrueType") {
        ("Tт", false)
    } else {
        ("?", false)
    };
    div()
        .id(SharedString::from(format!("fmt-{format}")))
        .flex_shrink_0()
        .tooltip({
            let format = format.to_string();
            move |window, cx| Tooltip::new(format.clone()).build(window, cx)
        })
        .text_size(px(10.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(pal.text_dim)
        .when(italic, |d| d.italic().font_family("Georgia"))
        .child(label.to_string())
}
