//! The info panel (View ▸ Show Font Info, ⌘I): the selected family's faces
//! and four tabs: an editable Preview, a Sizes ladder, the Glyphs it has and
//! its metadata.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::Textarea;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::slider::Slider;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Icon, Sizable};
use std::sync::Arc;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use glyphclub::font_info::{block_for, UNICODE_BLOCKS, OTHER_BLOCK};

use crate::app::{DetailTab, GlyphClub};
use crate::palette::Palette;

/// The info panel's default and minimum width; drag its edge to widen it.
pub const DETAIL_MIN_W: f32 = 360.;
pub const DETAIL_MAX_W: f32 = 640.;

pub fn detail_width(cx: &App) -> f32 {
    cx.global::<crate::settings::AppSettings>().0.detail_width.clamp(DETAIL_MIN_W, DETAIL_MAX_W)
}

/// Largest first, like a type specimen.
const SIZES: &[f32] = &[96., 72., 60., 48., 36., 28., 24., 20., 18., 16., 14., 12., 11., 10., 9., 8., 7.];
const SIZES_TEXT: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz1234567890";

/// Every OpenType feature tag in the font's GSUB and GPOS tables, sorted.
pub fn feature_tags(data: &[u8]) -> Vec<String> {
    let Ok(face) = ttf_parser::Face::parse(data, 0) else { return Vec::new() };
    let mut tags = std::collections::BTreeSet::new();
    for table in [face.tables().gsub, face.tables().gpos].into_iter().flatten() {
        for feature in table.features {
            tags.insert(feature.tag.to_string());
        }
    }
    tags.into_iter().collect()
}

/// A readable name for the common feature tags.
fn feature_name(tag: &str) -> &'static str {
    match tag {
        "aalt" => "Access All Alternates",
        "calt" => "Contextual Alternates",
        "case" => "Case-Sensitive Forms",
        "ccmp" => "Glyph Composition",
        "clig" => "Contextual Ligatures",
        "cpsp" => "Capital Spacing",
        "c2sc" => "Small Capitals From Capitals",
        "dlig" => "Discretionary Ligatures",
        "dnom" => "Denominators",
        "frac" => "Fractions",
        "hist" => "Historical Forms",
        "kern" => "Kerning",
        "liga" => "Standard Ligatures",
        "lnum" => "Lining Figures",
        "locl" => "Localized Forms",
        "mark" => "Mark Positioning",
        "mkmk" => "Mark to Mark Positioning",
        "numr" => "Numerators",
        "onum" => "Oldstyle Figures",
        "ordn" => "Ordinals",
        "ornm" => "Ornaments",
        "pnum" => "Proportional Figures",
        "salt" => "Stylistic Alternates",
        "sinf" => "Scientific Inferiors",
        "smcp" => "Small Capitals",
        "subs" => "Subscript",
        "sups" => "Superscript",
        "swsh" => "Swash",
        "titl" => "Titling",
        "tnum" => "Tabular Figures",
        "zero" => "Slashed Zero",
        t if t.starts_with("ss") => "Stylistic Set",
        t if t.starts_with("cv") => "Character Variant",
        _ => "",
    }
}

/// On by default when the font has them; everything else starts off.
fn feature_default_on(tag: &str) -> bool {
    matches!(tag, "calt" | "ccmp" | "clig" | "kern" | "liga" | "locl" | "mark" | "mkmk" | "rlig")
}
const GLYPH_CELL: f32 = 52.;

impl GlyphClub {
    pub fn render_detail(&mut self, pal: &Palette, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selection = self.selection();
        let panel = div()
            .w(px(detail_width(cx)))
            .h_full()
            .flex_shrink_0()
            .relative()
            .child(
                // The resize handle along the left edge.
                div()
                    .id("detail-resize")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(-3.))
                    .w(px(6.))
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.resizing_detail = true;
                    })),
            )
            .bg(pal.sidebar)
            .border_l_1()
            .border_color(pal.border)
            .flex()
            .flex_col()
            .child(
                div()
                    .id("detail-titlebar")
                    .h(px(52.))
                    .flex_shrink_0()
                    .on_mouse_down(MouseButton::Left, |ev, window, _| crate::chrome::drag_or_zoom(ev, window)),
            );

        let Some(&idx) = selection.first() else {
            return panel.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .px(px(30.))
                    .text_size(px(12.))
                    .text_color(pal.text_dim)
                    .child("Select a font to see its details."),
            );
        };

        let fam = &self.lib.families[idx];
        let face = self
            .detail_face
            .and_then(|id| fam.faces.iter().find(|f| f.id == id))
            .unwrap_or(fam.rep());
        let (face_id, face_path) = (face.id, face.path.clone());
        let (family, weight, italic) = (fam.name.clone(), face.weight, face.italic);
        let face_list: Vec<(i64, SharedString)> = fam.faces.iter().map(|f| (f.id, f.style.clone())).collect();
        let fam_name = fam.name.clone();
        let style_count = fam.faces.len();
        let face_style = face.style.clone();
        self.request_fonts(vec![(face_id, face_path.clone())], cx);
        self.request_info(face_id, face_path, cx);
        let ready = self.loaded.contains(&face_id);
        let features = self.active_features(face_id);

        // Faces: one chip per style.
        let mut faces = div().flex().flex_wrap().gap(px(4.));
        for (k, (id, style)) in face_list.into_iter().enumerate() {
            let on = id == face_id;
            let active = self.lib.active.contains(&id);
            faces = faces.child(
                div()
                    .id(("face", k))
                    .h(px(22.))
                    .px(px(8.))
                    .rounded(px(11.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .text_size(px(11.))
                    .when(on, |d| d.bg(pal.select).text_color(white()))
                    .when(!on, |d| d.bg(pal.chip).text_color(pal.text).hover(|s| s.opacity(0.85)))
                    .when(active, |d| d.child(div().size(px(5.)).rounded_full().bg(pal.active_dot)))
                    .child(style)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.detail_face = Some(id);
                        cx.notify();
                    })),
            );
        }

        let tab = |label: &'static str, t: DetailTab, this: &Self, cx: &mut Context<Self>| {
            let on = this.detail_tab == t;
            div()
                .id(label)
                .px(px(10.))
                .h(px(22.))
                .rounded(px(11.))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .text_color(pal.text)
                .when(on, |d| d.bg(pal.nav_active))
                .when(!on, |d| d.hover(|s| s.bg(pal.hover)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.detail_tab = t;
                    cx.notify();
                }))
        };

        let body: AnyElement = match self.detail_tab {
            DetailTab::Preview => {
                let size = self.detail_size_value;
                let align = self.detail_align;
                let align_btn = |icon: IconName, a: TextAlign, this: &Self, cx: &mut Context<Self>| {
                    let on = this.detail_align == a;
                    div()
                        .id(SharedString::from(format!("align-{a:?}")))
                        .size(px(24.))
                        .rounded(px(5.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(on, |d| d.bg(pal.nav_active))
                        .hover(|s| s.bg(pal.hover))
                        .child(Icon::new(icon).size(px(14.)).text_color(pal.text))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.detail_align = a;
                            cx.notify();
                        }))
                };
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .child(align_btn(IconName::TextAlignStart, TextAlign::Left, self, cx))
                            .child(align_btn(IconName::TextAlignCenter, TextAlign::Center, self, cx))
                            .child(align_btn(IconName::TextAlignEnd, TextAlign::Right, self, cx))
                            .child(div().w(px(10.)))
                            .child(div().flex_1().child(Slider::new(&self.detail_size)))
                            .child(
                                div()
                                    .w(px(40.))
                                    .text_size(px(11.))
                                    .text_color(pal.text_dim)
                                    .child(format!("{}pt", size.round())),
                            ),
                    )
                    .child(
                        div()
                            .id("detail-preview")
                            .flex_1()
                            .min_h_0()
                            .rounded(px(8.))
                            .bg(pal.tile)
                            .border_1()
                            .border_color(pal.border)
                            .p(px(10.))
                            .text_color(pal.preview)
                            .text_align(align)
                            .child(
                                Textarea::new(&self.detail_preview)
                                    .appearance(false)
                                    .h_full()
                                    .when(ready, |t| {
                                        t.font_family(family.clone())
                                            .font_weight(FontWeight(weight as f32))
                                            .when(italic, |t| t.italic())
                                            .font_features(features.clone())
                                    })
                                    .text_size(px(size))
                                    .line_height(px(size * 1.25))
                                    .text_align(align)
                                    .text_color(pal.preview),
                            ),
                    )
                    .into_any_element()
            }
            DetailTab::Sizes => {
                let mut ladder = div().flex().flex_col().gap(px(8.));
                let line_w = detail_width(cx) - 28.;
                let full: SharedString = SIZES_TEXT.into();
                for &sz in SIZES {
                    let text = if ready {
                        let font = crate::fit::preview_font(&family, weight, italic, features.clone());
                        crate::fit::fit(window, &full, font, sz, line_w)
                    } else {
                        SharedString::default()
                    };
                    ladder = ladder.child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(pal.text_dim)
                                    .child(format!("{sz} pt")),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .when(ready, |d| {
                                        d.font_family(family.clone())
                                            .font_weight(FontWeight(weight as f32))
                                            .when(italic, |d| d.italic())
                                            .font_features(features.clone())
                                            .text_size(px(sz))
                                            .line_height(px(sz * 1.15))
                                            .text_color(pal.preview)
                                            .child(text)
                                    }),
                            ),
                    );
                }
                div()
                    .id("sizes")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(ladder)
                    .into_any_element()
            }
            DetailTab::Features => self.features_tab(face_id, pal, cx),
            DetailTab::Glyphs => self.glyphs_tab(face_id, family.clone(), weight, italic, ready, pal, cx),
            DetailTab::Info => self.info_tab(face_id, pal),
        };

        panel.child(
            div()
                .flex_1()
                .min_h_0()
                .px(px(14.))
                .pb(px(14.))
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(px(16.)).font_weight(FontWeight::SEMIBOLD).truncate().child(fam_name))
                        .child(div().text_size(px(12.)).text_color(pal.text_dim).child(face_style.clone())),
                )
                .when(style_count > 1, |d| d.child(faces))
                .child(
                    div().flex().justify_center().child(
                    div()
                        .p(px(2.))
                        .rounded(px(13.))
                        .bg(pal.control)
                        .flex()
                        .gap(px(2.))
                        .child(tab("Preview", DetailTab::Preview, self, cx))
                        .child(tab("Sizes", DetailTab::Sizes, self, cx))
                        .child(tab("Glyphs", DetailTab::Glyphs, self, cx))
                        .child(tab("Features", DetailTab::Features, self, cx))
                        .child(tab("Info", DetailTab::Info, self, cx)),
                    ),
                )
                .child(body),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn glyphs_tab(
        &mut self,
        face_id: i64,
        family: SharedString,
        weight: u16,
        italic: bool,
        ready: bool,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(Some(info)) = self.info_cache.get(&face_id).cloned() else {
            return loading(pal, self.info_cache.contains_key(&face_id));
        };
        // Blocks this font actually has, in the table's order.
        let mut blocks: Vec<&'static str> = Vec::new();
        for &cp in &info.codepoints {
            let b = block_for(cp);
            if !blocks.contains(&b) {
                blocks.push(b);
            }
        }
        let order = |b: &str| {
            UNICODE_BLOCKS
                .iter()
                .position(|(name, _, _)| *name == b)
                .unwrap_or(if b == OTHER_BLOCK { usize::MAX } else { usize::MAX - 1 })
        };
        blocks.sort_by_key(|b| order(b));
        let current = self.glyph_block.filter(|b| blocks.contains(b));
        // Control characters have nothing to show.
        let glyphs: Vec<u32> = info
            .codepoints
            .iter()
            .copied()
            .filter(|&cp| !(cp < 0x20 || (0x7f..0xa0).contains(&cp)))
            .filter(|&cp| current.is_none_or(|b| block_for(cp) == b))
            .collect();

        let mut chips = div().flex().flex_wrap().gap(px(4.));
        let chip = |id: SharedString, label: SharedString, on: bool, block: Option<&'static str>, cx: &mut Context<Self>| {
            div()
                .id(id)
                .h(px(20.))
                .px(px(7.))
                .rounded(px(10.))
                .flex()
                .items_center()
                .text_size(px(10.))
                .when(on, |d| d.bg(pal.select).text_color(white()))
                .when(!on, |d| d.bg(pal.chip).text_color(pal.text))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.glyph_block = block;
                    cx.notify();
                }))
        };
        chips = chips.child(chip("block-all".into(), format!("All ({})", info.codepoints.len()).into(), current.is_none(), None, cx));
        for b in blocks {
            chips = chips.child(chip(format!("block-{b}").into(), b.into(), current == Some(b), Some(b), cx));
        }

        let per_row = ((detail_width(cx) - 28.) / GLYPH_CELL).floor().max(1.) as usize;
        let rows = glyphs.len().div_ceil(per_row);
        let pal = *pal;
        let list = uniform_list(
            "glyph-grid",
            rows,
            cx.processor(move |_, range: std::ops::Range<usize>, _, _| {
                range
                    .map(|r| {
                        let mut row = div().flex().gap(px(0.));
                        for &cp in glyphs.iter().skip(r * per_row).take(per_row) {
                            let ch = char::from_u32(cp).map(String::from).unwrap_or_default();
                            row = row.child(
                                div()
                                    .id(("glyph", cp as usize))
                                    .size(px(GLYPH_CELL))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(6.))
                                    .hover(|s| s.bg(pal.hover))
                                    .cursor_pointer()
                                    .when(ready, |d| {
                                        d.child(
                                            div()
                                                .font_family(family.clone())
                                                .font_weight(FontWeight(weight as f32))
                                                .when(italic, |d| d.italic())
                                                .text_size(px(24.))
                                                .line_height(px(30.))
                                                .text_color(pal.preview)
                                                .child(ch.clone()),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_size(px(8.))
                                            .text_color(pal.text_dim)
                                            .child(format!("{cp:04X}")),
                                    )
                                    .tooltip(move |window, cx| {
                                        Tooltip::new(format!("U+{cp:04X} — click to copy")).build(window, cx)
                                    })
                                    .on_click({
                                        let ch = ch.clone();
                                        move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(ch.clone()))
                                    }),
                            );
                        }
                        row
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.glyph_scroll)
        .size_full();

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(chips)
            .child(
                div()
                    .id("glyph-area")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(list)
                    .vertical_scrollbar(&self.glyph_scroll),
            )
            .into_any_element()
    }

    /// The features to shape previews with: each font feature's switch.
    fn active_features(&self, face_id: i64) -> FontFeatures {
        let tags = self.features_cache.get(&face_id).cloned().unwrap_or_default();
        let list = tags
            .iter()
            .filter_map(|tag| {
                let on = self.detail_features.get(tag).copied();
                let default = feature_default_on(tag);
                match on {
                    Some(v) if v != default => Some((tag.clone(), v as u32)),
                    _ => None,
                }
            })
            .collect();
        FontFeatures(Arc::new(list))
    }

    fn features_tab(&mut self, face_id: i64, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(tags) = self.features_cache.get(&face_id).cloned() else {
            return loading(pal, false);
        };
        if tags.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .text_color(pal.text_dim)
                .child("This font has no OpenType features.")
                .into_any_element();
        }
        let mut list = div().flex().flex_col();
        for tag in tags {
            let on = self.detail_features.get(&tag).copied().unwrap_or(feature_default_on(&tag));
            let name = feature_name(&tag);
            let t = tag.clone();
            list = list.child(
                div()
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .border_b_1()
                    .border_color(pal.border)
                    .child(
                        div()
                            .w(px(44.))
                            .text_size(px(11.))
                            .font_family("Menlo")
                            .text_color(pal.text_dim)
                            .child(tag.clone()),
                    )
                    .child(div().flex_1().text_size(px(12.)).child(if name.is_empty() { tag.clone() } else { name.to_string() }))
                    .child(
                        Switch::new(SharedString::from(format!("feat-{tag}")))
                            .checked(on)
                            .small()
                            .on_click(cx.listener(move |this, checked: &bool, _, cx| {
                                this.detail_features.insert(t.clone(), *checked);
                                cx.notify();
                            })),
                    ),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(pal.text_dim)
                    .child("Switches apply to the Preview and Sizes tabs."),
            )
            .child(div().id("features").flex_1().min_h_0().overflow_y_scrollbar().child(list))
            .into_any_element()
    }

    fn info_tab(&self, face_id: i64, pal: &Palette) -> AnyElement {
        let Some(Some(info)) = self.info_cache.get(&face_id).cloned() else {
            return loading(pal, self.info_cache.contains_key(&face_id));
        };
        let mut rows: Vec<(&str, String)> = vec![
            ("Full name", info.full_name.clone()),
            ("Family", info.family.clone()),
            ("Style", info.style.clone()),
            ("Format", info.format.to_string()),
        ];
        let opt = |rows: &mut Vec<(&str, String)>, label, v: &Option<String>| {
            if let Some(v) = v.as_ref().filter(|v| !v.trim().is_empty()) {
                rows.push((label, v.clone()));
            }
        };
        opt(&mut rows, "PostScript name", &info.postscript_name);
        opt(&mut rows, "Unique name", &info.unique_name);
        opt(&mut rows, "Version", &info.version);
        rows.push(("Variable", if info.is_variable { "Yes".into() } else { "No".into() }));
        rows.push(("Glyphs", info.glyph_count.to_string()));
        rows.push(("Characters", info.codepoints.len().to_string()));
        opt(&mut rows, "Designer", &info.designer);
        opt(&mut rows, "Manufacturer", &info.manufacturer);
        opt(&mut rows, "Copyright", &info.copyright);
        opt(&mut rows, "Trademark", &info.trademark);
        opt(&mut rows, "License", &info.license_description);
        if let Some(face) = self.lib.families.iter().flat_map(|f| f.faces.iter()).find(|f| f.id == face_id) {
            rows.push(("Location", face.path.clone()));
        }

        let mut list = div().flex().flex_col().gap(px(10.));
        for (label, value) in rows {
            list = list.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(10.)).text_color(pal.text_dim).child(label))
                    .child(div().text_size(px(12.)).text_color(pal.text).child(value)),
            );
        }
        div()
            .id("info")
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .child(list)
            .into_any_element()
    }
}

fn loading(pal: &Palette, failed: bool) -> AnyElement {
    div()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .text_color(pal.text_dim)
        .child(if failed { "Couldn't read this font file." } else { "Reading font…" })
        .into_any_element()
}
