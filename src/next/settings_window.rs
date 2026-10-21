//! GlyphClub ▸ Settings… (⌘,): a separate window with General, Sidebar and
//! Integrations pages.

use std::path::PathBuf;

use glyphclub::integrations::{self, IllustratorInstall};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Disableable, Sizable, Theme};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::palette::Palette;
use crate::settings::{AppSettings, Appearance, Settings};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    General,
    Sidebar,
    Integrations,
}

/// The open Settings window, so ⌘, focuses it instead of opening another.
struct SettingsWindowHandle(WindowHandle<SettingsView>);
impl Global for SettingsWindowHandle {}

/// Applied when the window opens, by the main view (login item, tray).
pub type ApplyHook = fn(&Settings, &Settings, &mut App);
struct Hook(ApplyHook);
impl Global for Hook {}

pub fn set_apply_hook(hook: ApplyHook, cx: &mut App) {
    cx.set_global(Hook(hook));
}

pub fn open(page: Option<Page>, cx: &mut App) {
    if let Some(handle) = cx.try_global::<SettingsWindowHandle>().map(|h| h.0) {
        let ok = handle
            .update(cx, |view, window, cx| {
                if let Some(p) = page {
                    view.page = p;
                    view.refresh_installs();
                }
                window.activate_window();
                cx.notify();
            })
            .is_ok();
        if ok {
            return;
        }
    }
    let bounds = Bounds::centered(None, size(px(620.), px(460.)), cx);
    let handle = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Settings".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            is_resizable: false,
            is_minimizable: false,
            ..Default::default()
        },
        |window, cx| {
            let view = cx.new(|_| SettingsView::new(page.unwrap_or(Page::General)));
            cx.new(|cx| gpui_kit::base::Root::new(view, window, cx))
        },
    );
    if let Ok(handle) = handle {
        // `open_window` hands back the Root; keep the inner view's handle.
        let inner = handle.update(cx, |root, _, _| root.view().clone().downcast::<SettingsView>().ok()).ok().flatten();
        if let Some(view) = inner {
            if let Some(h) = view.read(cx).window_handle {
                cx.set_global(SettingsWindowHandle(h));
            }
        }
        let _ = handle.update(cx, |_, window, _| window.activate_window());
    }
}

pub struct SettingsView {
    page: Page,
    installs: Vec<IllustratorInstall>,
    bundled: Option<PathBuf>,
    bundled_version: Option<String>,
    status: Option<(SharedString, bool)>,
    busy: bool,
    window_handle: Option<WindowHandle<SettingsView>>,
}

impl SettingsView {
    fn new(page: Page) -> Self {
        let mut this = Self {
            page,
            installs: Vec::new(),
            bundled: None,
            bundled_version: None,
            status: None,
            busy: false,
            window_handle: None,
        };
        this.refresh_installs();
        this
    }

    fn refresh_installs(&mut self) {
        self.installs = integrations::find_illustrators();
        self.bundled = integrations::bundled_plugin();
        self.bundled_version = self.bundled.as_deref().and_then(integrations::plugin_version);
    }

    fn change(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        let before = cx.global::<AppSettings>().0;
        let mut after = before;
        f(&mut after);
        cx.global_mut::<AppSettings>().0 = after;
        after.save();
        if let Some(hook) = cx.try_global::<Hook>().map(|h| h.0) {
            hook(&before, &after, cx);
        }
        cx.notify();
    }

    fn row(label: &'static str, detail: Option<&'static str>, control: impl IntoElement, pal: &Palette) -> Div {
        div()
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(16.))
            .border_b_1()
            .border_color(pal.border)
            .child(
                div()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().text_size(px(13.)).child(label))
                    .when_some(detail, |d, t| d.child(div().text_size(px(11.)).text_color(pal.text_dim).child(t))),
            )
            .child(control)
    }

    fn switch(
        id: &'static str,
        on: bool,
        cx: &mut Context<Self>,
        f: impl Fn(&mut Settings, bool) + 'static,
    ) -> impl IntoElement {
        Switch::new(id).checked(on).on_click(cx.listener(move |this, checked: &bool, _, cx| {
            let checked = *checked;
            this.change(cx, |s| f(s, checked));
        }))
    }

    fn general(&self, s: &Settings, pal: &Palette, cx: &mut Context<Self>) -> Div {
        let appearance = s.appearance;
        let mut seg = div().flex().p(px(2.)).rounded(px(7.)).bg(pal.control).border_1().border_color(pal.border);
        for (label, a) in [("System", Appearance::SystemDefault), ("Light", Appearance::Light), ("Dark", Appearance::Dark)] {
            let on = appearance == a;
            seg = seg.child(
                div()
                    .id(label)
                    .px(px(12.))
                    .h(px(24.))
                    .rounded(px(5.))
                    .flex()
                    .items_center()
                    .text_size(px(12.))
                    .when(on, |d| d.bg(pal.nav_active))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| this.change(cx, |s| s.appearance = a))),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(Self::row("Appearance", None, seg, pal))
            .child(Self::row(
                "Start GlyphClub when you log in",
                None,
                Self::switch("login", s.launch_on_startup, cx, |s, v| s.launch_on_startup = v),
                pal,
            ))
            .child(Self::row(
                "Show in the menu bar",
                Some("GlyphClub starts as a menu bar icon, and closing the window keeps it running there."),
                Self::switch("menubar", s.menu_bar, cx, |s, v| s.menu_bar = v),
                pal,
            ))
    }

    fn sidebar(&self, s: &Settings, pal: &Palette, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .child(Self::row("All Fonts", None, Self::switch("sb-all", s.sidebar_show_all, cx, |s, v| s.sidebar_show_all = v), pal))
            .child(Self::row("System Fonts", None, Self::switch("sb-sys", s.sidebar_show_system, cx, |s, v| s.sidebar_show_system = v), pal))
            .child(Self::row("Starred", None, Self::switch("sb-star", s.sidebar_show_starred, cx, |s, v| s.sidebar_show_starred = v), pal))
            .child(Self::row("Recents", None, Self::switch("sb-rec", s.sidebar_show_recent, cx, |s, v| s.sidebar_show_recent = v), pal))
    }

    fn integrations(&self, s: &Settings, pal: &Palette, cx: &mut Context<Self>) -> Div {
        let mut page = div().flex().flex_col().child(Self::row(
            "Activate missing fonts for Illustrator",
            Some("When a document opens with missing fonts, the plug-in asks GlyphClub, which temporarily activates any your libraries have."),
            Self::switch("ai-auto", s.auto_activate_illustrator, cx, |s, v| s.auto_activate_illustrator = v),
            pal,
        ));

        page = page.child(
            div()
                .pt(px(16.))
                .pb(px(4.))
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(pal.text_dim)
                .child(match &self.bundled_version {
                    Some(v) => format!("ILLUSTRATOR PLUG-IN {v}"),
                    None => "ILLUSTRATOR PLUG-IN".to_string(),
                }),
        );

        if !cfg!(target_os = "macos") {
            return page.child(div().text_size(px(12.)).text_color(pal.text_dim).child("The Illustrator plug-in is macOS-only for now."));
        }
        if self.installs.is_empty() {
            return page.child(div().py(px(10.)).text_size(px(12.)).text_color(pal.text_dim).child("No Adobe Illustrator found in /Applications."));
        }
        for (i, install) in self.installs.iter().enumerate() {
            let (status, action): (String, Option<(&'static str, bool)>) = if !install.supported() {
                (format!("Needs Illustrator {} or later", integrations::MIN_ILLUSTRATOR_YEAR), None)
            } else if self.bundled.is_none() {
                (
                    match &install.installed_version {
                        Some(v) => format!("Installed ({v}). This build doesn't include the plug-in."),
                        None => "This build doesn't include the plug-in.".into(),
                    },
                    install.installed_version.as_ref().map(|_| ("Uninstall", false)),
                )
            } else {
                match (&install.installed_version, &self.bundled_version) {
                    (None, _) => ("Not installed".into(), Some(("Install", true))),
                    (Some(v), Some(latest)) if v != latest => (format!("Installed ({v}), {latest} available"), Some(("Update", true))),
                    (Some(v), _) => (format!("Installed ({v})"), Some(("Uninstall", false))),
                }
            };
            let control = match action {
                Some((label, primary)) => {
                    let b = Button::new(("ai-action", i)).label(label).small().disabled(self.busy);
                    let b = if primary { b.primary() } else { b };
                    b.on_click(cx.listener(move |this, _, _, cx| this.run_integration(i, label == "Uninstall", cx)))
                        .into_any_element()
                }
                None => div().into_any_element(),
            };
            page = page.child(
                div()
                    .py(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .border_b_1()
                    .border_color(pal.border)
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(px(13.)).child(install.label.clone()))
                            .child(div().text_size(px(11.)).text_color(pal.text_dim).child(status)),
                    )
                    .child(control),
            );
        }
        if integrations::illustrator_running() {
            page = page.child(
                div()
                    .pt(px(10.))
                    .text_size(px(11.))
                    .text_color(pal.text_dim)
                    .child("Illustrator is open: restart it after installing to load the plug-in."),
            );
        }
        page.when_some(self.status.clone(), |d, (msg, err)| {
            d.child(
                div()
                    .pt(px(10.))
                    .text_size(px(12.))
                    .text_color(if err { rgb(0xe5484d).into() } else { pal.text })
                    .child(msg),
            )
        })
    }

    fn run_integration(&mut self, i: usize, uninstall: bool, cx: &mut Context<Self>) {
        let Some(install) = self.installs.get(i).cloned() else { return };
        let bundled = self.bundled.clone();
        self.busy = true;
        self.status = Some((if uninstall { "Removing…" } else { "Installing… (macOS will ask for your password)" }.into(), false));
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
        if self.window_handle.is_none() {
            self.window_handle = window.window_handle().downcast::<SettingsView>();
        }
        let _ = Theme::global(cx);
        let pal = Palette::get(cx);
        let s = cx.global::<AppSettings>().0;
        let tab = |label: &'static str, p: Page, this: &Self, cx: &mut Context<Self>| {
            let on = this.page == p;
            div()
                .id(label)
                .px(px(14.))
                .h(px(28.))
                .rounded(px(7.))
                .flex()
                .items_center()
                .text_size(px(13.))
                .when(on, |d| d.bg(pal.nav_active))
                .when(!on, |d| d.hover(|s| s.bg(pal.hover)))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.page = p;
                    if p == Page::Integrations {
                        this.refresh_installs();
                    }
                    cx.notify();
                }))
        };
        let body = match self.page {
            Page::General => self.general(&s, &pal, cx),
            Page::Sidebar => self.sidebar(&s, &pal, cx),
            Page::Integrations => self.integrations(&s, &pal, cx),
        };
        div()
            .size_full()
            .bg(pal.window)
            .text_color(pal.text)
            .flex()
            .flex_col()
            .child(
                div()
                    .flex()
                    .justify_center()
                    .gap(px(4.))
                    .py(px(12.))
                    .border_b_1()
                    .border_color(pal.border)
                    .child(tab("General", Page::General, self, cx))
                    .child(tab("Sidebar", Page::Sidebar, self, cx))
                    .child(tab("Integrations", Page::Integrations, self, cx)),
            )
            .child(div().id("settings-body").flex_1().overflow_y_scroll().px(px(28.)).py(px(8.)).child(body))
    }
}
