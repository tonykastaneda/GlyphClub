//! The Settings panel — an in-app modal overlay (consistent with how
//! `ui::draw_delete_confirm`/`draw_sample_picker` already do modals here,
//! rather than a second native OS window) opened from the macOS app menu's
//! "Settings…" (`native_menu.rs`) or the Windows hand-rolled File menu
//! (`ui::draw_windows_menu_bar`). See `crate::settings` for the persisted
//! state this edits and, importantly, which of these toggles are real vs.
//! UI-present-but-not-yet-wired-to-the-OS.

use vello::kurbo::{Affine, BezPath, Point, Rect, Stroke};
use vello::peniko::Color;
use vello::Scene;

use super::sidebar::draw_nav_icon;
use super::{fill_rect, stroke_rect, HitAction, HitRegion};
use crate::app::{App, Filter, SettingsTab};
use crate::settings::{Appearance, SettingToggle};
use crate::text::TextCx;
use crate::theme;

pub fn draw(app: &mut App, text: &mut TextCx, scene: &mut Scene, width: f64, height: f64) {
    let panel_w = 460.0_f64.min(width - 40.0);
    let panel_h = 564.0_f64.min(height - 40.0);
    let x0 = (width - panel_w) / 2.0;
    let y0 = (height - panel_h) / 2.0;
    let panel = Rect::new(x0, y0, x0 + panel_w, y0 + panel_h);

    // Full-window backdrop, dimmed — also doubles as the click-outside-to-
    // dismiss target: pushed *before* the panel's own hit regions, so the
    // reverse-order lookup in `handle_click` still resolves a click on the
    // panel itself to whatever control is actually under it.
    let backdrop = Rect::new(0.0, 0.0, width, height);
    fill_rect(scene, backdrop, Color::from_rgba8(0x00, 0x00, 0x00, 0x66), 0.0);
    app.hit_regions.push(HitRegion {
        rect: backdrop,
        action: HitAction::CloseSettings,
    });

    fill_rect(scene, panel, theme::SIDEBAR_BG(), 16.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 16.0, 1.0);
    // The panel's own blank space absorbs clicks — otherwise they fell
    // through to the backdrop and closed the panel.
    app.hit_regions.push(HitRegion {
        rect: panel,
        action: HitAction::Inert,
    });

    let title = "Settings";
    let tw = text.measure(title, 16.0, None);
    text.draw(scene, title, 16.0, None, theme::TEXT(), panel.x0 + (panel_w - tw) / 2.0, panel.y0 + 28.0);

    let close_size = 22.0;
    let close_rect = Rect::new(
        panel.x1 - 16.0 - close_size,
        panel.y0 + 16.0,
        panel.x1 - 16.0,
        panel.y0 + 16.0 + close_size,
    );
    let close_hovered = close_rect.contains(app.hover);
    if close_hovered {
        fill_rect(scene, close_rect, theme::CONTROL_BG_HOVER(), 6.0);
    }
    draw_close_x(scene, close_rect, theme::TEXT_SECONDARY());
    app.hit_regions.push(HitRegion {
        rect: close_rect,
        action: HitAction::CloseSettings,
    });

    stroke_rect(scene, Rect::new(panel.x0, panel.y0 + 54.0, panel.x1, panel.y0 + 54.0), theme::SEPARATOR(), 0.0, 1.0);

    let content_x = panel.x0 + 24.0;
    let content_w = panel_w - 48.0;
    let tabs_bottom = draw_tabs(app, text, scene, panel.x0, panel.x1, panel.y0 + 70.0);
    let y = tabs_bottom + 22.0;

    match app.settings_tab {
        SettingsTab::General => draw_general(app, text, scene, content_x, y, content_w),
        SettingsTab::Integrations => draw_integrations(app, text, scene, content_x, y, content_w),
    }
}

/// General | Integrations, styled like the detail panel's tab bar
/// (`ui::detail::draw_tab_bar`). Returns the y just below it.
fn draw_tabs(app: &mut App, text: &mut TextCx, scene: &mut Scene, x0: f64, x1: f64, y0: f64) -> f64 {
    let tabs = [(SettingsTab::General, "General"), (SettingsTab::Integrations, "Integrations")];
    let h = 28.0;
    let pad_x = 14.0;
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
        let active = app.settings_tab == tab;
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
        app.hit_regions.push(HitRegion { rect, action: HitAction::SetSettingsTab(tab) });
        cx += w;
    }
    y0 + h
}

fn draw_general(app: &mut App, text: &mut TextCx, scene: &mut Scene, content_x: f64, y: f64, content_w: f64) {
    let mut y = y;

    let (next_y, appearance_trigger) = draw_appearance_row(app, text, scene, content_x, y, content_w);
    y = next_y;
    y += 8.0;
    stroke_rect(scene, Rect::new(content_x, y, content_x + content_w, y), theme::SEPARATOR(), 0.0, 1.0);
    y += 20.0;

    y = draw_section_label(text, scene, content_x, y, "APPLICATION");
    y = draw_toggle_row(
        app,
        text,
        scene,
        content_x,
        y,
        content_w,
        "Start on Login",
        None,
        app.settings.launch_on_startup,
        SettingToggle::LaunchOnStartup,
    );
    let menu_bar_description = if cfg!(target_os = "windows") {
        "Start silently in the system tray; closing the window keeps it there."
    } else {
        "Start silently in the menu bar; closing the window keeps it there."
    };
    y = draw_toggle_row(
        app,
        text,
        scene,
        content_x,
        y,
        content_w,
        crate::tray::SETTING_LABEL,
        Some(menu_bar_description),
        app.settings.menu_bar,
        SettingToggle::MenuBar,
    );
    y = draw_toggle_row(
        app,
        text,
        scene,
        content_x,
        y,
        content_w,
        "Enable shortcut to open GlyphClub",
        Some("A global shortcut brings GlyphClub to the front from anywhere."),
        app.settings.global_shortcut_enabled,
        SettingToggle::GlobalShortcutEnabled,
    );

    y += 8.0;
    stroke_rect(scene, Rect::new(content_x, y, content_x + content_w, y), theme::SEPARATOR(), 0.0, 1.0);
    y += 20.0;

    y = draw_section_label(text, scene, content_x, y, "SHOW IN SIDEBAR");
    y = draw_nav_toggle_row(app, text, scene, content_x, y, content_w, Filter::All, "All Fonts", app.settings.sidebar_show_all, SettingToggle::SidebarAllFonts);
    y = draw_nav_toggle_row(app, text, scene, content_x, y, content_w, Filter::System, "System Fonts", app.settings.sidebar_show_system, SettingToggle::SidebarSystemFonts);
    y = draw_nav_toggle_row(app, text, scene, content_x, y, content_w, Filter::Starred, "Starred", app.settings.sidebar_show_starred, SettingToggle::SidebarStarred);
    let _ = draw_nav_toggle_row(app, text, scene, content_x, y, content_w, Filter::Recent, "Recents", app.settings.sidebar_show_recent, SettingToggle::SidebarRecents);

    // Drawn last so it layers over every row below it, same as the Glyphs
    // tab's block-filter dropdown (`ui::detail::draw_glyph_block_dropdown`).
    if app.appearance_dropdown_open {
        draw_appearance_dropdown(app, text, scene, appearance_trigger);
    }
}

fn draw_integrations(app: &mut App, text: &mut TextCx, scene: &mut Scene, x: f64, y: f64, w: f64) {
    let mut y = draw_section_label(text, scene, x, y, "ADOBE ILLUSTRATOR");
    text.draw(scene, "Auto-activate missing fonts", 13.0, None, theme::TEXT(), x, y);
    y += 20.0;
    for line in [
        "When a document opens with fonts that aren't installed, the plug-in",
        "asks GlyphClub for them, starting it in the background if needed.",
        "They stay active until you quit GlyphClub.",
    ] {
        text.draw(scene, line, 11.0, None, theme::TEXT_TERTIARY(), x, y);
        y += 16.0;
    }
    y += 14.0;

    if !cfg!(target_os = "macos") {
        text.draw(scene, "The Illustrator plug-in is macOS-only for now.", 12.0, None, theme::TEXT_SECONDARY(), x, y);
        return;
    }
    let bundled = app.integrations.bundled_version.clone();
    if bundled.is_none() {
        text.draw(scene, "This build of GlyphClub doesn't include the plug-in.", 12.0, None, theme::TEXT_SECONDARY(), x, y);
        return;
    }
    if app.integrations.illustrators.is_empty() {
        text.draw(scene, "No Adobe Illustrator found in /Applications.", 12.0, None, theme::TEXT_SECONDARY(), x, y);
        return;
    }

    let busy = app.integrations.busy;
    for (i, install) in app.integrations.illustrators.clone().into_iter().enumerate() {
        let row = Rect::new(x, y, x + w, y + 52.0);
        fill_rect(scene, row, theme::CONTROL_BG(), 10.0);
        stroke_rect(scene, row, theme::CONTROL_BORDER(), 10.0, 1.0);
        text.draw(scene, &install.label, 13.0, None, theme::TEXT(), row.x0 + 14.0, row.y0 + 10.0);

        let bundled = bundled.as_deref().unwrap_or("");
        let outdated = install
            .installed_version
            .as_deref()
            .is_some_and(|v| v.is_empty() || crate::app::is_newer_version(bundled, v));
        let (status, action) = if !install.supported() {
            (format!("Needs Illustrator {} or later", crate::integrations::MIN_ILLUSTRATOR_YEAR), None)
        } else {
            match install.installed_version.as_deref() {
                None => ("Not installed".to_string(), Some(("Install", HitAction::InstallIllustratorPlugin(i)))),
                Some(v) if outdated => (
                    format!("Version {} installed, {bundled} available", if v.is_empty() { "?" } else { v }),
                    Some(("Update", HitAction::InstallIllustratorPlugin(i))),
                ),
                Some(v) => (format!("Installed ({v})"), Some(("Uninstall", HitAction::UninstallIllustratorPlugin(i)))),
            }
        };
        text.draw(scene, &status, 11.0, None, theme::TEXT_TERTIARY(), row.x0 + 14.0, row.y0 + 29.0);

        if let Some((label, action)) = action {
            let primary = !matches!(action, HitAction::UninstallIllustratorPlugin(_));
            let tw = text.measure(label, 12.0, None);
            let bw = tw + 28.0;
            let btn = Rect::new(row.x1 - 12.0 - bw, row.y0 + 12.0, row.x1 - 12.0, row.y1 - 12.0);
            let hovered = !busy && btn.contains(app.hover);
            let (fill, label_color) = if primary {
                let accent = theme::BRAND_ACCENT();
                (if busy { accent.with_alpha(0.4) } else if hovered { accent.with_alpha(0.85) } else { accent }, theme::CANVAS())
            } else {
                (if hovered { theme::CONTROL_BG_HOVER() } else { theme::SIDEBAR_BG() }, theme::TEXT_SECONDARY())
            };
            fill_rect(scene, btn, fill, btn.height() / 2.0);
            if !primary {
                stroke_rect(scene, btn, theme::CONTROL_BORDER(), btn.height() / 2.0, 1.0);
            }
            text.draw_centered_v(scene, label, 12.0, None, label_color, btn.x0 + (bw - tw) / 2.0, btn.y0, btn.y1);
            if !busy {
                app.hit_regions.push(HitRegion { rect: btn, action });
            }
        }
        y = row.y1 + 8.0;
    }

    y += 6.0;
    let note = if busy {
        Some(("Working\u{2026}".to_string(), false))
    } else if let Some(message) = app.integrations.message.clone() {
        Some(message)
    } else if app.integrations.illustrator_running {
        Some(("Illustrator is open: restart it after installing to load the plug-in.".to_string(), false))
    } else {
        None
    };
    if let Some((note, error)) = note {
        text.draw(
            scene,
            &note,
            11.0,
            None,
            if error { theme::DANGER() } else { theme::TEXT_SECONDARY() },
            x,
            y,
        );
    }
}

/// Settings ▸ Integrations ▸ Install/Update. Runs on a background thread
/// because it can sit on the macOS administrator prompt; the outcome comes
/// back as `AppEvent::IntegrationFinished`.
pub fn install_illustrator_plugin(app: &mut App, index: usize) {
    let Some(install) = app.integrations.illustrators.get(index).cloned() else {
        return;
    };
    let Some(source) = crate::integrations::bundled_plugin() else {
        return;
    };
    // Whatever Illustrator has loaded keeps reporting the old version until
    // it restarts; don't let that keep the update banner up.
    app.integrations.connected_plugin_version = None;
    start_integration_job(app, move || {
        crate::integrations::install(&source, &install.plugins_dir)?;
        Ok(if crate::integrations::illustrator_running() {
            format!("Installed in {}. Restart Illustrator to load it.", install.label)
        } else {
            format!("Installed in {}. Open Window > Glyph Club there to see it.", install.label)
        })
    });
}

pub fn uninstall_illustrator_plugin(app: &mut App, index: usize) {
    let Some(install) = app.integrations.illustrators.get(index).cloned() else {
        return;
    };
    start_integration_job(app, move || {
        crate::integrations::uninstall(&install.plugins_dir)?;
        Ok(format!("Removed from {}.", install.label))
    });
}

fn start_integration_job(app: &mut App, job: impl FnOnce() -> Result<String, String> + Send + 'static) {
    if app.integrations.busy {
        return;
    }
    app.integrations.busy = true;
    app.integrations.message = None;
    let proxy = app.update_proxy.clone();
    std::thread::spawn(move || {
        let _ = proxy.send_event(crate::AppEvent::IntegrationFinished(job()));
    });
}

/// Applies one settings-panel row's click: flips the one field it owns and
/// persists immediately, so a preference survives even if the app is
/// force-quit rather than closed normally.
pub fn apply_toggle(app: &mut App, which: SettingToggle) {
    let s = &mut app.settings;
    match which {
        SettingToggle::LaunchOnStartup => {
            let on = !s.launch_on_startup;
            match crate::login_item::set_enabled(on) {
                Ok(()) => s.launch_on_startup = on,
                Err(e) => app.status = format!("Couldn't change Start on Login: {e}"),
            }
        }
        // `main.rs` notices the change and adds or removes the icon.
        SettingToggle::MenuBar => s.menu_bar = !s.menu_bar,
        SettingToggle::GlobalShortcutEnabled => s.global_shortcut_enabled = !s.global_shortcut_enabled,
        SettingToggle::SidebarAllFonts => s.sidebar_show_all = !s.sidebar_show_all,
        SettingToggle::SidebarSystemFonts => s.sidebar_show_system = !s.sidebar_show_system,
        SettingToggle::SidebarStarred => s.sidebar_show_starred = !s.sidebar_show_starred,
        SettingToggle::SidebarRecents => s.sidebar_show_recent = !s.sidebar_show_recent,
    }
    app.settings.save();
}

fn draw_section_label(text: &mut TextCx, scene: &mut Scene, x: f64, y: f64, label: &str) -> f64 {
    text.draw(scene, label, 10.5, None, theme::TEXT_TERTIARY(), x, y);
    y + 22.0
}

/// The "Appearance:" label + dropdown trigger row. Returns the next `y`
/// and the trigger's own rect — `draw` uses the latter to anchor
/// `draw_appearance_dropdown`, drawn separately (and later) so it layers
/// over every row below this one rather than being clipped to this row's
/// height.
fn draw_appearance_row(app: &mut App, text: &mut TextCx, scene: &mut Scene, x: f64, y: f64, w: f64) -> (f64, Rect) {
    let row_h = 30.0;
    let label = "Appearance:";
    text.draw_centered_v(scene, label, 13.0, None, theme::TEXT(), x, y, y + row_h);

    let trigger_w = 180.0;
    let trigger_x = x + w - trigger_w;
    let trigger_rect = Rect::new(trigger_x, y, trigger_x + trigger_w, y + row_h);
    let hovered = trigger_rect.contains(app.hover);
    fill_rect(
        scene,
        trigger_rect,
        if hovered || app.appearance_dropdown_open { theme::CONTROL_BG_HOVER() } else { theme::CONTROL_BG() },
        8.0,
    );
    stroke_rect(scene, trigger_rect, theme::CONTROL_BORDER(), 8.0, 1.0);
    text.draw_centered_v(
        scene,
        app.settings.appearance.label(),
        13.0,
        None,
        theme::TEXT(),
        trigger_rect.x0 + 12.0,
        trigger_rect.y0,
        trigger_rect.y1,
    );
    draw_chevron_down(scene, trigger_rect.x1 - 16.0, trigger_rect.center().y, theme::TEXT_SECONDARY());

    app.hit_regions.push(HitRegion {
        rect: trigger_rect,
        action: HitAction::ToggleAppearanceDropdown,
    });
    (y + row_h + 2.0, trigger_rect)
}

fn draw_appearance_dropdown(app: &mut App, text: &mut TextCx, scene: &mut Scene, anchor: Rect) {
    let options = [Appearance::SystemDefault, Appearance::Light, Appearance::Dark];
    let row_h = 28.0;
    let h = row_h * options.len() as f64 + 8.0;
    let panel = Rect::new(anchor.x0, anchor.y1 + 4.0, anchor.x1, anchor.y1 + 4.0 + h);
    fill_rect(scene, panel, theme::SIDEBAR_BG(), 10.0);
    stroke_rect(scene, panel, theme::CONTROL_BORDER(), 10.0, 1.0);

    let mut y = panel.y0 + 4.0;
    for option in options {
        let row = Rect::new(panel.x0 + 4.0, y, panel.x1 - 4.0, y + row_h);
        if row.contains(app.hover) {
            fill_rect(scene, row, theme::NAV_HOVER_BG(), 6.0);
        }
        let selected = app.settings.appearance == option;
        text.draw_centered_v(
            scene,
            option.label(),
            12.5,
            None,
            if selected { theme::BRAND_ACCENT() } else { theme::TEXT() },
            row.x0 + 12.0,
            row.y0,
            row.y1,
        );
        app.hit_regions.push(HitRegion {
            rect: row,
            action: HitAction::SetAppearance(option),
        });
        y += row_h;
    }
}

fn draw_chevron_down(scene: &mut Scene, cx: f64, cy: f64, color: Color) {
    let mut path = BezPath::new();
    path.move_to(Point::new(cx - 4.0, cy - 2.0));
    path.line_to(Point::new(cx, cy + 2.5));
    path.line_to(Point::new(cx + 4.0, cy - 2.0));
    scene.stroke(&Stroke::new(1.4), Affine::IDENTITY, color, None, &path);
}

fn draw_toggle_row(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    x: f64,
    y: f64,
    w: f64,
    label: &str,
    description: Option<&str>,
    checked: bool,
    toggle: SettingToggle,
) -> f64 {
    let row_h = if description.is_some() { 44.0 } else { 30.0 };
    let row_rect = Rect::new(x, y, x + w, y + row_h);
    if row_rect.contains(app.hover) {
        fill_rect(scene, row_rect, theme::NAV_HOVER_BG(), 8.0);
    }

    let box_size = 17.0;
    let box_rect = Rect::new(x + 5.0, y + 6.0, x + 5.0 + box_size, y + 6.0 + box_size);
    draw_checkbox(scene, box_rect, checked);

    let label_x = box_rect.x1 + 11.0;
    text.draw(scene, label, 13.0, None, theme::TEXT(), label_x, y + 6.0);
    if let Some(desc) = description {
        text.draw(scene, desc, 11.0, None, theme::TEXT_TERTIARY(), label_x, y + 25.0);
    }

    app.hit_regions.push(HitRegion {
        rect: row_rect,
        action: HitAction::ToggleSetting(toggle),
    });
    y + row_h + 2.0
}

fn draw_nav_toggle_row(
    app: &mut App,
    text: &mut TextCx,
    scene: &mut Scene,
    x: f64,
    y: f64,
    w: f64,
    filter: Filter,
    label: &str,
    checked: bool,
    toggle: SettingToggle,
) -> f64 {
    let row_h = 30.0;
    let row_rect = Rect::new(x, y, x + w, y + row_h);
    if row_rect.contains(app.hover) {
        fill_rect(scene, row_rect, theme::NAV_HOVER_BG(), 8.0);
    }

    let box_size = 17.0;
    let box_rect = Rect::new(x + 5.0, y + 6.0, x + 5.0 + box_size, y + 6.0 + box_size);
    draw_checkbox(scene, box_rect, checked);

    let icon_x = box_rect.x1 + 12.0;
    draw_nav_icon(scene, filter, icon_x, y + row_h / 2.0 - 8.0, theme::TEXT_SECONDARY());
    text.draw(scene, label, 13.0, None, theme::TEXT(), icon_x + 27.0, y + 6.0);

    app.hit_regions.push(HitRegion {
        rect: row_rect,
        action: HitAction::ToggleSetting(toggle),
    });
    y + row_h + 2.0
}

fn draw_checkbox(scene: &mut Scene, rect: Rect, checked: bool) {
    if checked {
        fill_rect(scene, rect, theme::BRAND_ACCENT(), 5.0);
        let (x0, y0, w, h) = (rect.x0, rect.y0, rect.width(), rect.height());
        let mut check = BezPath::new();
        check.move_to(Point::new(x0 + w * 0.22, y0 + h * 0.52));
        check.line_to(Point::new(x0 + w * 0.42, y0 + h * 0.74));
        check.line_to(Point::new(x0 + w * 0.80, y0 + h * 0.26));
        // `Stroke::new` already defaults to round caps/joins.
        scene.stroke(
            &Stroke::new(1.8),
            Affine::IDENTITY,
            Color::from_rgb8(0x14, 0x14, 0x14),
            None,
            &check,
        );
    } else {
        fill_rect(scene, rect, theme::CONTROL_BG(), 5.0);
        stroke_rect(scene, rect, theme::CONTROL_BORDER(), 5.0, 1.0);
    }
}

fn draw_close_x(scene: &mut Scene, rect: Rect, color: Color) {
    // `Stroke::new` already defaults to round caps/joins.
    let inset = rect.inset(-5.0);
    let stroke = Stroke::new(1.4);
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &vello::kurbo::Line::new((inset.x0, inset.y0), (inset.x1, inset.y1)));
    scene.stroke(&stroke, Affine::IDENTITY, color, None, &vello::kurbo::Line::new((inset.x1, inset.y0), (inset.x0, inset.y1)));
}
