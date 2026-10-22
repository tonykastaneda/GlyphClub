//! The window chrome around the font views: sidebar, toolbar, the floating
//! sample-text bar, the update banner and the empty state.

use gpui_kit::assets::IconName;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{ContextMenuExt, PopupMenuItem};
use gpui_kit::component::slider::Slider;
use gpui_kit::component::{Icon, Sizable, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::actions::{ToggleInfo, ToggleSampleBar, ViewGrid, ViewList};
use crate::app::GlyphClub;
use crate::library::Nav;
use crate::palette::Palette;
use crate::settings::{AppSettings, ViewMode};
use crate::update;

/// The sidebar's default and minimum width; drag its edge to widen it.
pub const SIDEBAR_MIN_W: f32 = 220.;
pub const SIDEBAR_MAX_W: f32 = 420.;

const SAMPLE_PRESETS: &[(&str, &str)] = &[
    ("123", "0123456789"),
    ("ABC", "ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
    ("abc", "abcdefghijklmnopqrstuvwxyz"),
    ("lorem", "Lorem ipsum dolor sit amet"),
];

impl GlyphClub {
    fn nav_row(
        &self,
        id: impl Into<ElementId>,
        nav: Nav,
        icon: IconName,
        label: SharedString,
        count: Option<usize>,
        pal: &Palette,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let active = self.nav == nav;
        div()
            .id(id)
            .h(px(30.))
            // Rows keep their height when the sidebar overflows and scrolls.
            .flex_shrink_0()
            .mx(px(10.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .when(active, |d| d.bg(pal.nav_active))
            .when(!active, |d| d.hover(|s| s.bg(pal.hover)))
            .child(Icon::new(icon).size(px(16.)).text_color(pal.text))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.))
                    .text_color(pal.text)
                    .child(label),
            )
            .when_some(count, |d, n| {
                d.child(div().text_size(px(11.)).text_color(pal.text_dim).child(n.to_string()))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.set_nav(nav, cx)))
    }

    fn section_header(label: &'static str, pal: &Palette) -> Div {
        div()
            .flex_shrink_0()
            .mt(px(18.))
            .mb(px(4.))
            .px(px(20.))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(pal.text_dim)
            .child(label)
    }

    pub fn sidebar(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<AppSettings>().0;
        let starred = self.lib.families.iter().filter(|f| self.lib.family_starred(f)).count();
        let weak = cx.entity().downgrade();

        let mut libraries = div().flex().flex_col().gap(px(1.));
        for folder in &self.lib.folders {
            let (id, path) = (folder.id, folder.path.clone());
            let weak = weak.clone();
            libraries = libraries.child(
                self.nav_row(("lib", folder.id as u64), Nav::Folder(folder.id), IconName::FolderOpen, folder.name.clone(), Some(folder.count), pal, cx)
                    .context_menu(move |menu, _, _| {
                        let (w1, w2) = (weak.clone(), weak.clone());
                        let p = path.clone();
                        menu.item(PopupMenuItem::new("Show in Finder").on_click(move |_, _, _| {
                            let _ = glyphclub::reveal::reveal(std::path::Path::new(&p));
                        }))
                        .item(PopupMenuItem::new("Sync").on_click(move |_, _, cx| {
                            let _ = w1.update(cx, |this, cx| this.rescan(cx));
                        }))
                        .separator()
                        .item(PopupMenuItem::new("Remove Library").on_click(move |_, _, cx| {
                            let _ = w2.update(cx, |this, cx| this.remove_library(id, cx));
                        }))
                    }),
            );
        }
        libraries = libraries.child(
            div()
                .id("add-library-row")
                .h(px(30.))
                .mx(px(10.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .hover(|s| s.bg(pal.hover))
                .text_color(pal.text_dim)
                .child(Icon::new(IconName::Plus).size(px(16.)))
                .child(div().text_size(px(13.)).child("Add Library"))
                .on_click(cx.listener(|this, _, _, cx| this.prompt_add_library(cx))),
        );

        let mut tags = div().flex().flex_col().gap(px(1.));
        for tag in &self.lib.tags {
            tags = tags.child(self.nav_row(
                ("tag", tag.id as u64),
                Nav::Tag(tag.id),
                IconName::Tag,
                tag.name.clone().into(),
                Some(self.lib.tag_count(tag.id)),
                pal,
                cx,
            ));
        }
        tags = tags.child(
            div()
                .id("new-tag-row")
                .h(px(30.))
                .mx(px(10.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(6.))
                .hover(|s| s.bg(pal.hover))
                .text_color(pal.text_dim)
                .child(Icon::new(IconName::Plus).size(px(16.)))
                .child(div().text_size(px(13.)).child("New Tag…"))
                .on_click({
                    let weak = weak.clone();
                    move |_, window, cx| open_new_tag_dialog(weak.clone(), None, window, cx)
                }),
        );

        div()
            .w(px(s.sidebar_width.clamp(SIDEBAR_MIN_W, SIDEBAR_MAX_W)))
            .h_full()
            .flex_shrink_0()
            .relative()
            .bg(pal.sidebar)
            .border_r_1()
            .border_color(pal.border)
            .flex()
            .flex_col()
            .child(
                // Title-bar strip: the traffic lights sit at its left.
                div()
                    .id("sidebar-titlebar")
                    .h(px(52.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(2.))
                    .px(px(10.))
                    .on_mouse_down(MouseButton::Left, |ev, window, _| drag_or_zoom(ev, window))
                    .child(icon_button("toggle-sidebar", IconName::PanelLeft, pal, |_, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::ToggleSidebar), cx)
                    })),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .pb(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .when(s.sidebar_show_all, |d| {
                        d.child(self.nav_row("all", Nav::All, IconName::Library, "All Fonts".into(), Some(self.lib.face_count), pal, cx))
                    })
                    .when(s.sidebar_show_system, |d| {
                        d.child(self.nav_row("system", Nav::System, IconName::Monitor, "System Fonts".into(), Some(self.lib.system_count), pal, cx))
                    })
                    .when(s.sidebar_show_starred, |d| {
                        d.child(self.nav_row("starred", Nav::Starred, IconName::Star, "Starred".into(), (starred > 0).then_some(starred), pal, cx))
                    })
                    .when(s.sidebar_show_recent, |d| {
                        d.child(self.nav_row("recents", Nav::Recents, IconName::Clock, "Recents".into(), None, pal, cx))
                    })
                    .child(self.icon_fonts_group(pal, cx))
                    .child(Self::section_header("Libraries", pal))
                    .child(libraries)
                    .child(Self::section_header("Tags", pal))
                    .child(tags),
            )
            .child(
                // The resize handle along the right edge.
                div()
                    .id("sidebar-resize")
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .right(px(-3.))
                    .w(px(6.))
                    .cursor_col_resize()
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.resizing_sidebar = true;
                    })),
            )
            .child(
                // Footer: Settings, and what's running in the background.
                div()
                    .h(px(40.))
                    .flex_shrink_0()
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(icon_button("open-settings", IconName::Settings, pal, |_, window, cx| {
                        window.dispatch_action(Box::new(crate::actions::OpenSettings), cx)
                    }))
                    .when_some(self.busy.clone(), |d, label| {
                        d.child(div().flex_1().min_w_0().truncate().text_size(px(11.)).text_color(pal.text_dim).child(label))
                    }),
            )
    }

    /// Sidebar ▸ Icon Fonts: a collapsible group of every icon set.
    fn icon_fonts_group(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let total: usize = self.icon_sets.iter().map(|s| s.icons.len()).sum();
        let open = self.icons_open;
        let mut group = div().flex().flex_col().gap(px(1.)).child(
            div()
                .id("icon-fonts")
                .h(px(30.))
                .flex_shrink_0()
                .mx(px(10.))
                .pl(px(2.))
                .pr(px(10.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .when(self.nav == Nav::AllIcons, |d| d.bg(pal.nav_active))
                .hover(|s| s.bg(pal.hover))
                .child(
                    div().id("icon-fonts-toggle").w(px(18.)).h_full().flex().items_center().justify_center()
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.icons_open = !this.icons_open;
                        cx.notify();
                    }))
                    .child(Icon::new(if open { IconName::ChevronDown } else { IconName::ChevronRight })
                        .size(px(12.))
                        .text_color(pal.text_dim)),
                )
                .child(Icon::new(IconName::Shapes).size(px(16.)).text_color(pal.text))
                .child(div().flex_1().pl(px(4.)).text_size(px(13.)).text_color(pal.text).child("Icon Fonts"))
                .when(total > 0, |d| d.child(div().text_size(px(11.)).text_color(pal.text_dim).child(total.to_string())))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.set_nav(Nav::AllIcons, cx);
                })),
        );
        if open {
            for (i, set) in self.icon_sets.iter().enumerate() {
                group = group.child(
                    self.nav_row(("iconset", i as u64), Nav::Icons(i), IconName::Copy, set.name.clone().into(), None, pal, cx)
                        .pl(px(30.)),
                );
            }
        }
        group
    }

    pub fn toolbar(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<AppSettings>().0;
        let pill = |pal: &Palette| {
            div()
                .h(px(30.))
                .rounded(px(15.))
                .bg(pal.control)
                .border_1()
                .border_color(pal.border)
                .flex()
                .items_center()
                // Controls in the toolbar mustn't start a window drag.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        };
        let seg = |id: &'static str, icon: IconName, on: bool, pal: &Palette| {
            div()
                .id(id)
                .size(px(24.))
                .rounded(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .when(on, |d| d.bg(pal.nav_active))
                .when(!on, |d| d.hover(|s| s.bg(pal.hover)))
                .child(Icon::new(icon).size(px(15.)).text_color(pal.text))
        };
        let round = |id: &'static str, icon: IconName, on: bool, pal: &Palette| {
            pill(pal)
                .w(px(30.))
                .justify_center()
                .when(on, |d| d.bg(pal.nav_active))
                .child(div().id(id).size_full().rounded_full().flex().items_center().justify_center().child(
                    Icon::new(icon).size(px(16.)).text_color(pal.text),
                ))
        };

        div()
            .id("toolbar")
            .h(px(52.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(14.))
            // Leaves room for the traffic lights when the sidebar is hidden.
            .when(!self.sidebar_shown && cfg!(target_os = "macos"), |d| d.pl(px(84.)))
            .on_mouse_down(MouseButton::Left, |ev, window, _| drag_or_zoom(ev, window))
            .when(!self.sidebar_shown, |d| {
                d.child(icon_button("show-sidebar", IconName::PanelLeft, pal, |_, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::ToggleSidebar), cx)
                }))
            })
            .child(
                pill(pal)
                    .w(px(120.))
                    .px(px(10.))
                    .gap(px(6.))
                    .child(div().text_size(px(10.)).text_color(pal.text_dim).child("A"))
                    .child(div().flex_1().child(Slider::new(&self.size_slider).bg(pal.select).text_color(gpui_kit::white())))
                    .child(div().text_size(px(14.)).text_color(pal.text_dim).child("A")),
            )
            .child(div().flex_1().min_w_0())
            .when(self.compact, |d| d.child(self.overflow_menu(pal, cx)))
            .when(!self.compact, |d| d.child(
                round("tb-sample", IconName::TextCursorInput, s.show_sample_bar, pal)
                    .on_mouse_up(MouseButton::Left, |_, window, cx| window.dispatch_action(Box::new(ToggleSampleBar), cx)),
            )
            .child(
                pill(pal)
                    .px(px(3.))
                    .gap(px(2.))
                    .child(
                        seg("tb-grid", IconName::LayoutGrid, s.view_mode == ViewMode::Grid, pal)
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(ViewGrid), cx)),
                    )
                    .child(
                        seg("tb-list", IconName::List, s.view_mode == ViewMode::List, pal)
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(ViewList), cx)),
                    ),
            )
            .child(
                round("tb-info", IconName::Info, self.info_open, pal)
                    .on_mouse_up(MouseButton::Left, |_, window, cx| window.dispatch_action(Box::new(ToggleInfo), cx)),
            ))
            .child(
                pill(pal).w(px(if self.compact { 170. } else { 230. })).px(px(10.)).child(
                    Input::new(&self.search)
                        .appearance(false)
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).size(px(14.)).text_color(pal.text_dim))
                        .small(),
                ),
            )
    }

    /// The » button a narrow window shows in place of the toolbar buttons.
    fn overflow_menu(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::actions::*;
        use gpui_kit::component::button::{Button, ButtonVariants};
        use gpui_kit::component::menu::DropdownMenu;
        let s = cx.global::<AppSettings>().0;
        let info_open = self.info_open;
        fn fire(label: &'static str, action: fn() -> Box<dyn Action>) -> PopupMenuItem {
            PopupMenuItem::new(label).on_click(move |_, window, cx| window.dispatch_action(action(), cx))
        }
        div()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new("overflow")
                    .icon(Icon::new(IconName::ChevronsRight))
                    .ghost()
                    .small()
                    .dropdown_menu(move |menu, window, cx| {
                        let view_mode = s.view_mode;
                        menu
                            .item(fire("Edit Preview", || Box::new(EditPreviewText)))
                            .submenu("View", window, cx, move |m, _, _| {
                                m.item(fire("as Grid", || Box::new(ViewGrid)).checked(view_mode == ViewMode::Grid))
                                    .item(fire("as List", || Box::new(ViewList)).checked(view_mode == ViewMode::List))
                            })
                            .item(fire("Info Pane", || Box::new(ToggleInfo)).checked(info_open))
                    }),
            )
            .text_color(pal.text)
    }

    /// Windows: the strip across the top of the window, standing in for the
    /// system title bar the window hides: the menus on the left, the window
    /// buttons on the right, and the empty space between drags the window.
    pub fn windows_titlebar(&self, pal: &Palette, window: &Window) -> impl IntoElement {
        div()
            .id("win-titlebar")
            .h(px(WIN_TITLEBAR_H))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(pal.sidebar)
            .border_b_1()
            .border_color(pal.border)
            .when_some(self.app_menu_bar.clone(), |d, bar| d.child(div().pl(px(4.)).child(bar)))
            .child(div().flex_1().h_full().window_control_area(WindowControlArea::Drag))
            .child(window_controls(window, pal, true))
    }
}

/// Height of the Windows title bar strip and its window buttons.
pub const WIN_TITLEBAR_H: f32 = 32.;

/// Windows: minimize, maximize/restore and close, drawn by us because the
/// windows hide the system title bar. Windows itself handles the clicks on
/// these areas, so they behave like the native buttons (snap layouts too).
pub fn window_controls(window: &Window, pal: &Palette, min_max: bool) -> impl IntoElement {
    let button = |id: &'static str, icon: IconName, area: WindowControlArea| {
        let close = area == WindowControlArea::Close;
        let hover_bg = if close { rgb(0xc42b1c).into() } else { pal.hover };
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .text_color(pal.text)
            .hover(move |s| {
                let s = s.bg(hover_bg);
                if close { s.text_color(gpui_kit::white()) } else { s }
            })
            .window_control_area(area)
            .child(Icon::new(icon).size(px(14.)))
    };
    let maximize_icon = if window.is_maximized() { IconName::WindowRestore } else { IconName::WindowMaximize };
    div()
        .h(px(WIN_TITLEBAR_H))
        .flex()
        .flex_shrink_0()
        .when(min_max, |d| {
            d.child(button("win-min", IconName::WindowMinimize, WindowControlArea::Min))
                .child(button("win-max", maximize_icon, WindowControlArea::Max))
        })
        .child(button("win-close", IconName::WindowClose, WindowControlArea::Close))
}

impl GlyphClub {
    pub fn update_banner(&self, pal: &Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let version = self.update_available.clone().filter(|_| !self.dismissed_update)?;
        let version_for_skip = version.clone();
        Some(
            div()
                .mx(px(14.))
                .mb(px(8.))
                .h(px(34.))
                .px(px(12.))
                .rounded(px(8.))
                .bg(pal.banner)
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(12.))
                .child(div().flex_1().child(format!("GlyphClub {version} is available.")))
                .child(
                    div()
                        .id("update-download")
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(pal.select)
                        .cursor_pointer()
                        .child("Download")
                        .on_click(|_, _, cx| cx.open_url(update::WEBSITE_URL)),
                )
                .child(
                    div()
                        .id("update-skip")
                        .text_color(pal.text_dim)
                        .cursor_pointer()
                        .child("Skip This Version")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let skipped = update::parse_version(&version_for_skip);
                            this.update_settings(cx, |s| s.skipped_version = skipped);
                            this.dismissed_update = true;
                            cx.notify();
                        })),
                )
                .child(
                    div()
                        .id("update-dismiss")
                        .cursor_pointer()
                        .child(Icon::new(IconName::Close).size(px(14.)).text_color(pal.text_dim))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.dismissed_update = true;
                            cx.notify();
                        })),
                ),
        )
    }

    pub fn sample_bar(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let s = cx.global::<AppSettings>().0;
        let mut chips = div().flex().items_center().gap(px(6.));
        for (i, &(label, text)) in SAMPLE_PRESETS.iter().enumerate() {
            chips = chips.child(
                div()
                    .id(("preset", i))
                    .h(px(22.))
                    .px(px(8.))
                    .rounded(px(6.))
                    .bg(pal.chip)
                    .hover(|s| s.opacity(0.85))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(pal.text)
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.sample.update(cx, |s, cx| s.set_value(text, window, cx));
                        this.refilter(cx);
                    })),
            );
        }
        chips = chips.child(div().flex_1()).child(
            div()
                .id("hide-missing")
                .h(px(22.))
                .px(px(8.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .text_size(px(11.))
                .when(s.hide_missing, |d| d.bg(pal.select).text_color(pal.on_select))
                .when(!s.hide_missing, |d| d.text_color(pal.text_dim).hover(|s| s.bg(pal.hover)))
                .child("Hide missing")
                .on_click(cx.listener(|this, _, _, cx| this.update_settings(cx, |s| s.hide_missing = !s.hide_missing))),
        );
        div()
            .absolute()
            .bottom(px(24.))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(
                div()
                    .id("sample-bar")
                    // Clicks on the bar mustn't reach the tiles underneath.
                    .occlude()
                    .w(px(500.))
                    .rounded(px(14.))
                    .bg(pal.bar)
                    .border_1()
                    .border_color(pal.border)
                    .shadow_lg()
                    .px(px(10.))
                    .py(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(Input::new(&self.sample).appearance(false).cleanable(true).small())
                    .child(chips),
            )
    }

    pub fn empty_state(&self, pal: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = !self.search.read(cx).value().trim().is_empty();
        let message = match self.nav {
            _ if searching => "No fonts match your search.",
            Nav::Starred => "Star a font from its right-click menu and it shows up here.",
            Nav::Recents => "Fonts added in the last two weeks show up here.",
            Nav::Tag(_) => "Tag fonts from their right-click menu and they show up here.",
            _ if cx.global::<AppSettings>().0.hide_missing => "No fonts here have every character of the sample text.",
            _ if self.lib.folders.is_empty() => "Add a library with + (or drop a folder here) to get started.",
            _ => "No fonts here yet.",
        };
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(13.))
            .text_color(pal.text_dim)
            .child(message)
    }
}

pub fn open_new_tag_dialog(view: WeakEntity<GlyphClub>, apply_to: Option<SharedString>, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder("Tag name"));
    let focused = std::cell::Cell::new(false);
    window.open_dialog(cx, move |dialog, window, cx| {
        if !focused.replace(true) {
            input.update(cx, |s, cx| s.focus(window, cx));
        }
        let (input_ok, view_ok, apply_ok) = (input.clone(), view.clone(), apply_to.clone());
        dialog
            .title("New Tag")
            .w(px(340.))
            .child(Input::new(&input))
            .footer(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        gpui_kit::component::button::Button::new("tag-cancel")
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    )
                    .child({
                        use gpui_kit::component::button::ButtonVariants;
                        gpui_kit::component::button::Button::new("tag-create")
                            .primary()
                            .label("Create")
                            .on_click(move |_, window, cx| {
                                let name = input_ok.read(cx).value().to_string();
                                let apply = apply_ok.clone();
                                let _ = view_ok.update(cx, |this, cx| this.create_tag(&name, apply, cx));
                                window.close_dialog(cx);
                            })
                    }),
            )
    });
}

pub fn icon_button(
    id: &'static str,
    icon: IconName,
    pal: &Palette,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .size(px(28.))
        .rounded(px(6.))
        .flex()
        .items_center()
        .justify_center()
        .hover(|s| s.bg(pal.hover))
        .child(Icon::new(icon).size(px(17.)).text_color(pal.text_dim))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(on_click)
}

/// Lets the empty parts of the title-bar strips move the window, and
/// double-click zoom it, like a native title bar.
pub fn drag_or_zoom(ev: &MouseDownEvent, window: &mut Window) {
    if ev.click_count == 2 {
        window.titlebar_double_click();
    } else {
        window.start_window_move();
    }
}
