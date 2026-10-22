//! GlyphClub ▸ Settings… (⌘,): a macOS-style preferences window, modeled on
//! RightFont's: an icon toolbar (General, Sidebar, Import, Auto Activation)
//! over checkbox forms.

use std::path::PathBuf;

use glyphclub::integrations::{self, IllustratorInstall};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants, DropdownButton};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::{Disableable, Icon, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::palette::Palette;
use crate::settings::{AppSettings, Appearance, Settings};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Sidebar,
    Import,
    /// The Illustrator plug-in and its options.
    AutoActivation,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::General => "General",
            Page::Sidebar => "Sidebar",
            Page::Import => "Import",
            Page::AutoActivation => "Auto Activation",
        }
    }
}

/// The open Settings window, so ⌘, focuses it instead of opening another.
struct SettingsWindowHandle(WindowHandle<gpui_kit::base::Root>, WeakEntity<SettingsView>);
impl Global for SettingsWindowHandle {}

/// What a settings change does beyond saving (login item, tray, the main
/// window), set by `main`.
pub type ApplyHook = fn(&Settings, &Settings, &mut App);
struct Hook(ApplyHook);
impl Global for Hook {}

pub fn set_apply_hook(hook: ApplyHook, cx: &mut App) {
    cx.set_global(Hook(hook));
}

/// Changes, saves and applies settings from anywhere.
pub fn change(cx: &mut App, f: impl FnOnce(&mut Settings)) {
    let before = cx.global::<AppSettings>().0;
    let mut after = before;
    f(&mut after);
    cx.global_mut::<AppSettings>().0 = after;
    after.save();
    if let Some(hook) = cx.try_global::<Hook>().map(|h| h.0) {
        hook(&before, &after, cx);
    }
}

pub fn open(page: Option<Page>, cx: &mut App) {
    if let Some((handle, view)) = cx.try_global::<SettingsWindowHandle>().map(|h| (h.0, h.1.clone())) {
        let reused = handle.update(cx, |_, window, _| window.activate_window()).is_ok();
        if reused {
            if let Some(p) = page {
                let _ = view.update(cx, |v, cx| v.set_page(p, cx));
            }
            return;
        }
    }
    let start = page.unwrap_or(Page::General);
    let bounds = Bounds::centered(None, size(px(600.), px(500.)), cx);
    let mut created: Option<WeakEntity<SettingsView>> = None;
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(start.title().into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(12.), px(12.))),
            }),
            app_owns_titlebar_drag: true,
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|cx| SettingsView::new(start, cx));
            created = Some(view.downgrade());
            cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
        },
    );
    if let (Ok(handle), Some(view)) = (opened, created) {
        cx.set_global(SettingsWindowHandle(handle, view));
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
}

pub struct SettingsView {
    page: Page,
    installs: Vec<IllustratorInstall>,
    bundled: Option<PathBuf>,
    bundled_version: Option<String>,
    /// Checked when the page refreshes, not on every redraw (it runs pgrep).
    illustrator_running: bool,
    status: Option<(SharedString, bool)>,
    busy: bool,
    _observe: Subscription,
}

impl SettingsView {
    fn new(page: Page, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            page,
            installs: Vec::new(),
            bundled: None,
            bundled_version: None,
            illustrator_running: false,
            status: None,
            busy: false,
            _observe: cx.observe_global::<AppSettings>(|_, cx| cx.notify()),
        };
        this.refresh_installs();
        this
    }

    fn set_page(&mut self, page: Page, cx: &mut Context<Self>) {
        self.page = page;
        if page == Page::AutoActivation {
            self.refresh_installs();
        }
        cx.notify();
    }

    fn refresh_installs(&mut self) {
        self.illustrator_running = integrations::illustrator_running();
        self.installs = integrations::find_illustrators();
        self.bundled = integrations::bundled_plugin();
        self.bundled_version = self.bundled.as_deref().and_then(integrations::plugin_version);
    }

    fn check(id: &'static str, label: &'static str, on: bool, f: impl Fn(&mut Settings, bool) + 'static) -> Checkbox {
        Checkbox::new(id).checked(on).label(label).on_click(move |checked, _, cx| {
            let checked = *checked;
            change(cx, |s| f(s, checked));
        })
    }

    fn note(text: &'static str, pal: &Palette) -> Div {
        div().pl(px(24.)).text_size(px(11.)).text_color(pal.text_dim).child(text)
    }

    /// A RightFont-style form row: a right-aligned label, then controls.
    fn form_row(label: &'static str, body: impl IntoElement, pal: &Palette) -> Div {
        div()
            .flex()
            .gap(px(14.))
            .py(px(14.))
            .px(px(16.))
            .child(
                div()
                    .w(px(130.))
                    .flex_shrink_0()
                    .flex()
                    .justify_end()
                    .pt(px(1.))
                    .text_size(px(13.))
                    .text_color(pal.text)
                    .child(label),
            )
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(6.)).child(body))
    }

    fn divider(pal: &Palette) -> Div {
        div().h(px(1.)).mx(px(16.)).bg(pal.border)
    }

    fn general(&self, s: &Settings, pal: &Palette) -> AnyElement {
        let appearance = s.appearance;
        let label = match appearance {
            Appearance::SystemDefault => "System Default",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        };
        let picker = DropdownButton::new("appearance")
            .button(Button::new("appearance-button").label(label).small())
            .small()
            .dropdown_menu(move |menu, _, _| {
                let item = |name: &'static str, a: Appearance| {
                    PopupMenuItem::new(name)
                        .checked(appearance == a)
                        .on_click(move |_, _, cx| change(cx, |s| s.appearance = a))
                };
                menu.item(item("System Default", Appearance::SystemDefault))
                    .item(item("Light", Appearance::Light))
                    .item(item("Dark", Appearance::Dark))
            });
        div()
            .flex()
            .flex_col()
            .child(Self::form_row(
                "Application:",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(Self::check("launch", "Launch GlyphClub on startup", s.launch_on_startup, |s, v| {
                        s.launch_on_startup = v
                    }))
                    .child(Self::check("background", "Run GlyphClub in background", s.menu_bar, |s, v| s.menu_bar = v))
                    .child(Self::note("Keep the app icon in the menu bar when the GlyphClub window is closed.", pal)),
                pal,
            ))
            .child(Self::divider(pal))
            .child(Self::form_row("Appearance:", div().child(picker), pal))
            .into_any_element()
    }

    fn sidebar(&self, s: &Settings, pal: &Palette) -> AnyElement {
        let row = |id: &'static str, icon: IconName, label: &'static str, on: bool, f: fn(&mut Settings, bool)| {
            div()
                .h(px(30.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(Checkbox::new(id).checked(on).on_click(move |checked, _, cx| {
                    let checked = *checked;
                    change(cx, |s| f(s, checked));
                }))
                .child(Icon::new(icon).size(px(16.)).text_color(pal.text))
                .child(div().text_size(px(13.)).child(label))
        };
        div()
            .px(px(36.))
            .py(px(18.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(13.)).child("Show these items in the sidebar:"))
            .child(
                div()
                    .py(px(6.))
                    .rounded(px(10.))
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.control)
                    .flex()
                    .flex_col()
                    .child(row("sb-all", IconName::Library, "All Fonts", s.sidebar_show_all, |s, v| s.sidebar_show_all = v))
                    .child(row("sb-sys", IconName::Monitor, "System Fonts", s.sidebar_show_system, |s, v| {
                        s.sidebar_show_system = v
                    }))
                    .child(row("sb-star", IconName::Star, "Starred", s.sidebar_show_starred, |s, v| s.sidebar_show_starred = v))
                    .child(row("sb-rec", IconName::Clock, "Recents", s.sidebar_show_recent, |s, v| s.sidebar_show_recent = v)),
            )
            .into_any_element()
    }

    fn import(&self, s: &Settings, pal: &Palette) -> AnyElement {
        div()
            .pt(px(6.))
            .child(Self::form_row(
                "When Importing:",
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(Self::check(
                        "skip-dupes",
                        "Skip duplicate fonts (same PostScript name)",
                        s.import_skip_duplicates,
                        |s, v| s.import_skip_duplicates = v,
                    ))
                    .child(Self::check("import-activate", "Activate fonts automatically", s.import_activate, |s, v| {
                        s.import_activate = v
                    }))
                    .child(Self::note("Importing is dropping font files onto the GlyphClub window.", pal)),
                pal,
            ))
            .into_any_element()
    }

    fn auto_activation(&self, s: &Settings, pal: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut apps = div().flex().flex_col();
        if self.installs.is_empty() {
            let where_ = if cfg!(windows) { "Program Files\\Adobe" } else { "/Applications" };
            apps = apps.child(
                div()
                    .p(px(10.))
                    .text_size(px(12.))
                    .text_color(pal.text_dim)
                    .child(format!("No Adobe Illustrator found in {where_}.")),
            );
        }
        for (i, install) in self.installs.iter().enumerate() {
            let (status, action): (String, Option<(&'static str, bool)>) = if !install.supported() {
                (format!("Needs Illustrator {} or later", integrations::MIN_ILLUSTRATOR_YEAR), None)
            } else if self.bundled.is_none() {
                (
                    match &install.installed_version {
                        Some(v) => format!("Plug-in {v}; this build doesn't include one"),
                        None => "This build doesn't include the plug-in".into(),
                    },
                    install.installed_version.as_ref().map(|_| ("Remove", false)),
                )
            } else {
                match (&install.installed_version, &self.bundled_version) {
                    (None, _) => ("Plug-in not installed".into(), Some(("Install", true))),
                    (Some(v), Some(latest)) if v != latest => (format!("Plug-in {v}, {latest} available"), Some(("Update", true))),
                    (Some(v), _) => (format!("Plug-in {v} installed"), Some(("Remove", false))),
                }
            };
            let control = match action {
                Some((label, primary)) => {
                    let b = Button::new(("ai-action", i)).label(label).xsmall().disabled(self.busy);
                    let b = if primary { b.primary() } else { b };
                    b.on_click(cx.listener(move |this, _, _, cx| this.run_integration(i, label == "Remove", cx)))
                        .into_any_element()
                }
                None => div().into_any_element(),
            };
            apps = apps.child(
                div()
                    .px(px(10.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when(i % 2 == 1, |d| d.bg(pal.stripe))
                    .child(
                        div()
                            .size(px(18.))
                            .flex_shrink_0()
                            .rounded(px(4.))
                            .bg(rgb(0x330000))
                            .border_1()
                            .border_color(rgb(0xff9a00))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(9.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(0xff9a00))
                            .child("Ai"),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().text_size(px(12.)).child(install.label.clone()))
                            .child(div().text_size(px(10.)).text_color(pal.text_dim).child(status)),
                    )
                    .child(div().flex_shrink_0().child(control)),
            );
        }

        let left = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                Checkbox::new("auto")
                    .checked(s.auto_activate_illustrator)
                    .label("Auto Activation")
                    .on_click(|checked, _, cx| {
                        let checked = *checked;
                        change(cx, |s| s.auto_activate_illustrator = checked);
                    }),
            )
            .child(
                div()
                    .min_h(px(200.))
                    .rounded(px(6.))
                    .border_1()
                    .border_color(pal.border)
                    .bg(pal.control)
                    .overflow_hidden()
                    .child(apps),
            )
            .child(
                div()
                    .text_size(px(11.))
                    .text_color(pal.text_dim)
                    .child("Automatically activates missing fonts when documents open in Illustrator, through the GlyphClub plug-in."),
            )
            .when(self.illustrator_running, |d| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(pal.text_dim)
                        .child("Illustrator is open: restart it after installing the plug-in."),
                )
            })
            .when_some(self.status.clone(), |d, (msg, err)| {
                d.child(
                    div()
                        .text_size(px(11.))
                        .text_color(if err { rgb(0xe5484d).into() } else { pal.text })
                        .child(msg),
                )
            });

        let heading = |t: &'static str| div().pt(px(4.)).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(t);
        let right = div()
            .w(px(230.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(heading("Activation Options"))
            .child(Self::check("whole-family", "Activate entire font family", s.activate_entire_family, |s, v| {
                s.activate_entire_family = v
            }))
            .child(Self::check("silent", "Silent Mode (no alerts)", s.silent_mode, |s, v| s.silent_mode = v))
            .child(div().h(px(10.)))
            .child(heading("Auto Deactivation"))
            .child(div().text_size(px(11.)).text_color(pal.text_dim).child("Deactivate when GlyphClub quits:"))
            .child(Self::check("deact-auto", "Auto-activated fonts", s.deactivate_auto_on_quit, |s, v| {
                s.deactivate_auto_on_quit = v
            }))
            .child(Self::check("deact-manual", "Manually activated fonts", s.deactivate_manual_on_quit, |s, v| {
                s.deactivate_manual_on_quit = v
            }));

        div()
            .p(px(18.))
            .flex()
            .gap(px(22.))
            .child(left)
            .child(right)
            .into_any_element()
    }

    fn run_integration(&mut self, i: usize, uninstall: bool, cx: &mut Context<Self>) {
        let Some(install) = self.installs.get(i).cloned() else { return };
        let bundled = self.bundled.clone();
        self.busy = true;
        self.status = Some((
            if uninstall {
                "Removing…"
            } else if cfg!(windows) {
                "Installing… Windows will ask for permission."
            } else {
                "Installing… macOS will ask for your password."
            }
            .into(),
            false,
        ));
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if uninstall {
                        integrations::uninstall(&install.plugins_dir).map(|_| format!("Removed from {}.", install.label))
                    } else {
                        let source = bundled.ok_or_else(|| "This build doesn't include the plug-in.".to_string())?;
                        integrations::install(&source, &install.plugins_dir)
                            .map(|_| format!("Installed in {}. Restart Illustrator to load it.", install.label))
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.status = Some(match result {
                    Ok(m) => (m.into(), false),
                    Err(e) => (e.into(), true),
                });
                this.refresh_installs();
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = Palette::get(cx);
        let s = cx.global::<AppSettings>().0;
        let page = self.page;
        window.set_window_title(page.title());

        let tab = |id: &'static str, icon: IconName, p: Page, cx: &mut Context<Self>| {
            let on = page == p;
            div()
                .id(id)
                .px(px(8.))
                .py(px(4.))
                .rounded(px(7.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(2.))
                .when(on, |d| d.bg(pal.nav_active))
                .when(!on, |d| d.hover(|s| s.bg(pal.hover)))
                .child(Icon::new(icon).size(px(20.)).text_color(if on { pal.select } else { pal.text_dim }))
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(if on { pal.select } else { pal.text_dim })
                        .child(p.title()),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| this.set_page(p, cx)))
        };

        let body = match page {
            Page::General => self.general(&s, &pal),
            Page::Sidebar => self.sidebar(&s, &pal),
            Page::Import => self.import(&s, &pal),
            Page::AutoActivation => self.auto_activation(&s, &pal, cx),
        };

        div()
            .size_full()
            .bg(pal.window)
            .text_color(pal.text)
            .flex()
            .flex_col()
            .child(
                // Title and the icon toolbar, in the title bar like a native
                // preferences window. Empty space drags the window.
                div()
                    .id("settings-titlebar")
                    .relative()
                    .flex_shrink_0()
                    .pt(px(8.))
                    .pb(px(8.))
                    .bg(pal.sidebar)
                    .border_b_1()
                    .border_color(pal.border)
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(6.))
                    .on_mouse_down(MouseButton::Left, |ev, window, _| crate::chrome::drag_or_zoom(ev, window))
                    .child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(page.title()))
                    .child(
                        div()
                            .flex()
                            .gap(px(4.))
                            .child(tab("tab-general", IconName::Settings, Page::General, cx))
                            .child(tab("tab-sidebar", IconName::PanelLeft, Page::Sidebar, cx))
                            .child(tab("tab-import", IconName::Download, Page::Import, cx))
                            .child(tab("tab-auto", IconName::Loader, Page::AutoActivation, cx)),
                    )
                    // Windows: the close button the hidden system title bar
                    // would have had (this window can't minimize or resize).
                    .when(cfg!(target_os = "windows"), |d| {
                        d.child(div().absolute().top_0().right_0().child(crate::chrome::window_controls(window, &pal, false)))
                    }),
            )
            .child(div().id("settings-body").flex_1().overflow_y_scroll().child(body))
    }
}
