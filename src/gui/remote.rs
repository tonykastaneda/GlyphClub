//! UI commands from `glyphclub-cli app …`, over the local bridge: lets the
//! app be driven and inspected without touching the mouse or keyboard.
//! Every command replies with the resulting UI state.

use gpui_kit::*;
use serde_json::{json, Value};

use crate::app::{DetailTab, GlyphClub};
use crate::library::Nav;
use crate::settings::{AppSettings, Appearance, SortField, ViewMode};

pub const HELP: &str = "state | view grid|list | nav all|system|starred|recents|library <name>|tag <name>|icons <set> | \
select <family>… | deselect | info open|close | tab preview|sizes|glyphs|features|info | face <style> | \
sample <text> | size <n> | search <text> | sort <field> [asc|desc] | expand on|off | sidebar show|hide | \
theme light|dark|system | window <w> <h> | settings general|sidebar|import|auto | action <name> | scroll top|<row> | illustrator-apply <family> | \
icon-copy <set> <icon> [vectors|svg]";

impl GlyphClub {
    pub fn handle_ui(
        &mut self,
        command: &str,
        args: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Value, String> {
        let arg = |i: usize| args.get(i).map(|s| s.as_str()).unwrap_or("");
        let rest = args.join(" ");
        match command {
            "state" | "" => {}
            "view" => {
                let mode = match arg(0) {
                    "grid" => ViewMode::Grid,
                    "list" => ViewMode::List,
                    other => return Err(format!("view: expected grid or list, got “{other}”")),
                };
                self.update_settings(cx, |s| s.view_mode = mode);
            }
            "nav" => {
                let nav = match arg(0) {
                    "all" => Nav::All,
                    "system" => Nav::System,
                    "starred" => Nav::Starred,
                    "recents" => Nav::Recents,
                    "library" => {
                        let name = args[1..].join(" ").to_lowercase();
                        let f = self
                            .lib
                            .folders
                            .iter()
                            .find(|f| f.name.to_lowercase() == name || f.id.to_string() == name)
                            .ok_or_else(|| format!("no library “{name}”"))?;
                        Nav::Folder(f.id)
                    }
                    "icons" if args.len() == 1 => Nav::AllIcons,
                    "icons" => {
                        let name = args[1..].join(" ").to_lowercase();
                        let i = self
                            .icon_sets
                            .iter()
                            .position(|s| s.name.to_lowercase() == name || s.slug == name)
                            .ok_or_else(|| format!("no icon set “{name}”"))?;
                        self.icons_open = true;
                        Nav::Icons(i)
                    }
                    "tag" => {
                        let name = args[1..].join(" ").to_lowercase();
                        let t = self
                            .lib
                            .tags
                            .iter()
                            .find(|t| t.name.to_lowercase() == name)
                            .ok_or_else(|| format!("no tag “{name}”"))?;
                        Nav::Tag(t.id)
                    }
                    other => return Err(format!("nav: unknown “{other}”")),
                };
                self.set_nav(nav, cx);
            }
            "select" => {
                if args.is_empty() {
                    return Err("select: name one or more families".into());
                }
                let mut keys = Vec::new();
                for name in args {
                    let lower = name.to_lowercase();
                    // Prefer a family in the current view.
                    let found = self
                        .visible
                        .iter()
                        .map(|&i| &self.lib.families[i])
                        .find(|f| f.name.to_lowercase() == lower)
                        .or_else(|| self.lib.families.iter().find(|f| f.name.to_lowercase() == lower))
                        .ok_or_else(|| format!("no family “{name}”"))?;
                    keys.push(found.key.clone());
                }
                // Not in the current view: show everything.
                if !keys.iter().all(|k| self.visible.iter().any(|&i| &self.lib.families[i].key == k)) {
                    self.search.update(cx, |s, cx| s.set_value("", window, cx));
                    self.set_nav(Nav::All, cx);
                }
                self.selected = keys.iter().cloned().collect();
                self.detail_face = None;
                if let Some(pos) = self.visible.iter().position(|&i| self.lib.families[i].key == keys[0]) {
                    self.anchor = Some(pos);
                    self.scroll_to_position(pos, cx);
                }
            }
            "deselect" => self.selected.clear(),
            "info" => {
                self.info_open = match arg(0) {
                    "open" | "" => true,
                    "close" => false,
                    "toggle" => !self.info_open,
                    other => return Err(format!("info: expected open or close, got “{other}”")),
                };
                self.refresh_menus(cx);
            }
            "tab" => {
                self.detail_tab = match arg(0) {
                    "preview" => DetailTab::Preview,
                    "sizes" => DetailTab::Sizes,
                    "glyphs" => DetailTab::Glyphs,
                    "features" => DetailTab::Features,
                    "info" => DetailTab::Info,
                    other => return Err(format!("tab: unknown “{other}”")),
                };
                self.info_open = true;
            }
            "face" => {
                let fam = self
                    .selection()
                    .first()
                    .map(|&i| &self.lib.families[i])
                    .ok_or("face: select a family first")?;
                let lower = rest.to_lowercase();
                let face = fam
                    .faces
                    .iter()
                    .find(|f| f.style.to_lowercase() == lower)
                    .ok_or_else(|| format!("no style “{rest}” in {}", fam.name))?;
                self.detail_face = Some(face.id);
            }
            "sample" => {
                let text = rest.clone();
                self.sample.update(cx, |s, cx| s.set_value(text, window, cx));
                self.refilter(cx);
            }
            "size" => {
                let v: f32 = arg(0).parse().map_err(|_| "size: expected a number".to_string())?;
                let v = v.clamp(crate::app::MIN_SIZE, crate::app::MAX_SIZE);
                self.update_settings(cx, |s| s.preview_size = v);
                self.size_slider.update(cx, |s, cx| s.set_value(v, window, cx));
            }
            "search" => {
                let text = rest.clone();
                self.search.update(cx, |s, cx| s.set_value(text, window, cx));
                self.refilter(cx);
            }
            "sort" => {
                let field = match arg(0) {
                    "name" => SortField::Name,
                    "added" | "date" => SortField::DateAdded,
                    "active" | "activated" => SortField::Enabled,
                    "starred" => SortField::Starred,
                    "styles" => SortField::Styles,
                    "format" => SortField::Format,
                    "tags" => SortField::Tags,
                    other => return Err(format!("sort: unknown field “{other}”")),
                };
                let ascending = match arg(1) {
                    "asc" => true,
                    "desc" => false,
                    _ => field.default_ascending(),
                };
                self.update_settings(cx, |s| {
                    s.sort_field = field;
                    s.sort_ascending = ascending;
                });
            }
            "expand" => {
                let on = matches!(arg(0), "on" | "true" | "");
                self.update_settings(cx, |s| s.expand_families = on);
            }
            "sidebar" => match arg(0) {
                "show" => {
                    self.sidebar_open = true;
                    self.sidebar_forced = self.compact;
                }
                "hide" => {
                    self.sidebar_open = false;
                    self.sidebar_forced = false;
                }
                other => return Err(format!("sidebar: expected show or hide, got “{other}”")),
            },
            "theme" => {
                let a = match arg(0) {
                    "light" => Appearance::Light,
                    "dark" => Appearance::Dark,
                    "system" => Appearance::SystemDefault,
                    other => return Err(format!("theme: unknown “{other}”")),
                };
                self.update_settings(cx, |s| s.appearance = a);
                self.applied_appearance = None;
            }
            "window" => {
                let w: f32 = arg(0).parse().map_err(|_| "window: expected <width> <height>".to_string())?;
                let h: f32 = arg(1).parse().map_err(|_| "window: expected <width> <height>".to_string())?;
                window.resize(size(px(w), px(h)));
            }
            "settings" => {
                use crate::settings_window::Page;
                let page = match arg(0) {
                    "general" | "" => Page::General,
                    "sidebar" => Page::Sidebar,
                    "import" => Page::Import,
                    "auto" | "auto-activation" | "illustrator" => Page::AutoActivation,
                    other => return Err(format!("settings: unknown page “{other}”")),
                };
                crate::settings_window::open(Some(page), cx);
            }
            "action" => {
                let name = if arg(0).contains("::") { arg(0).to_string() } else { format!("glyphclub::{}", arg(0)) };
                let action = cx.build_action(&name, None).map_err(|e| format!("action {name}: {e}"))?;
                window.dispatch_action(action, cx);
            }
            "scroll" => {
                let pos = match arg(0) {
                    "top" | "" => 0,
                    n => n.parse::<usize>().map_err(|_| "scroll: expected top or a row number".to_string())?,
                };
                self.scroll_to_row(pos, cx);
            }
            "illustrator-apply" => {
                // What a card's Ai button does: set Illustrator's selected
                // text to this family (activating it first if needed).
                let lower = rest.to_lowercase();
                let key = self
                    .lib
                    .families
                    .iter()
                    .find(|f| f.name.to_lowercase() == lower)
                    .map(|f| f.key.clone())
                    .ok_or_else(|| format!("no family “{rest}”"))?;
                self.apply_in_illustrator(key, cx);
            }
            "icon-copy" => {
                // What an icon's double-click (vectors) or Copy as SVG does.
                let set = arg(0).to_lowercase();
                let set_ix = self
                    .icon_sets
                    .iter()
                    .position(|s| s.slug == set || s.name.to_lowercase() == set)
                    .ok_or_else(|| format!("no icon set “{set}”"))?;
                let icon = self.icon_sets[set_ix]
                    .icons
                    .iter()
                    .find(|i| i.name == arg(1))
                    .map(|i| (i.codepoint, i.name.clone()))
                    .ok_or_else(|| format!("no icon “{}” in {set}", arg(1)))?;
                let font = self.icon_font(set_ix, cx).ok_or("couldn't load the icon font")?;
                let drag = crate::icon_view::IconDrag {
                    font,
                    outline: "".into(),
                    codepoint: icon.0,
                    name: icon.1.into(),
                };
                let copied = match arg(2) {
                    "" | "vectors" => crate::icon_view::copy_vectors(&drag, cx),
                    "svg" => crate::icon_view::copy_svg(&drag, cx),
                    other => return Err(format!("icon-copy: expected vectors or svg, got “{other}”")),
                };
                if !copied {
                    return Err("couldn't copy that icon".into());
                }
            }
            "help" => return Ok(json!({ "commands": HELP })),
            other => return Err(format!("unknown command “{other}”. Commands: {HELP}")),
        }
        cx.notify();
        self.sync_view_state(window);
        self.apply_appearance(window, cx);
        Ok(self.ui_state(window, cx))
    }

    fn scroll_to_position(&mut self, pos: usize, cx: &mut Context<Self>) {
        let s = cx.global::<AppSettings>().0;
        if s.view_mode == ViewMode::List {
            self.list_scroll.scroll_to_item(pos, ScrollStrategy::Top);
        } else {
            // Grid rows hold `last_cols` tiles each.
            let row = pos / self.last_cols.max(1);
            self.grid_scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
    }

    fn scroll_to_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let s = cx.global::<AppSettings>().0;
        if s.view_mode == ViewMode::List {
            self.list_scroll.scroll_to_item(row, ScrollStrategy::Top);
        } else {
            self.grid_scroll.scroll_to_item(row, ScrollStrategy::Top);
        }
    }

    pub fn ui_state(&self, window: &Window, cx: &App) -> Value {
        let s = cx.global::<AppSettings>().0;
        let nav = match self.nav {
            Nav::All => json!("all"),
            Nav::System => json!("system"),
            Nav::Starred => json!("starred"),
            Nav::Recents => json!("recents"),
            Nav::Folder(id) => json!({ "library": self.lib.folders.iter().find(|f| f.id == id).map(|f| f.name.to_string()) }),
            Nav::Tag(id) => json!({ "tag": self.lib.tag_name(id) }),
            Nav::AllIcons => json!({ "icons": "all" }),
            Nav::Icons(i) => json!({ "icons": self.icon_sets.get(i).map(|s| s.name.clone()) }),
        };
        let selected: Vec<String> = self.selection().iter().map(|&i| self.lib.families[i].name.to_string()).collect();
        let visible: Vec<Value> = self
            .visible
            .iter()
            .take(60)
            .map(|&i| {
                let f = &self.lib.families[i];
                json!({
                    "family": f.name.to_string(),
                    "styles": f.faces.len(),
                    "active": self.lib.family_active(f),
                    "starred": self.lib.family_starred(f),
                })
            })
            .collect();
        let detail_face = self
            .selection()
            .first()
            .map(|&i| &self.lib.families[i])
            .map(|f| {
                self.detail_face
                    .and_then(|id| f.faces.iter().find(|x| x.id == id))
                    .unwrap_or(f.rep())
                    .style
                    .to_string()
            });
        let size = window.viewport_size();
        json!({
            "pid": std::process::id(),
            "window": { "width": f32::from(size.width), "height": f32::from(size.height) },
            "compact": self.compact,
            "nav": nav,
            "view": if s.view_mode == ViewMode::Grid { "grid" } else { "list" },
            "expandFamilies": s.expand_families,
            "sort": { "field": format!("{:?}", s.sort_field), "ascending": s.sort_ascending },
            "previewSize": s.preview_size,
            "sample": self.sample.read(cx).value().to_string(),
            "search": self.search.read(cx).value().to_string(),
            "hideMissing": s.hide_missing,
            "sidebar": { "open": self.sidebar_open, "shown": self.sidebar_shown, "width": s.sidebar_width },
            "info": {
                "open": self.info_open,
                "tab": format!("{:?}", self.detail_tab).to_lowercase(),
                "style": detail_face,
                "width": s.detail_width,
            },
            "theme": format!("{:?}", s.appearance),
            "dark": gpui_kit::component::ActiveTheme::theme(cx).mode.is_dark(),
            "selected": selected,
            "visibleCount": self.visible.len(),
            "visible": visible,
            "busy": self.busy.as_ref().map(|b| b.to_string()),
            "updateAvailable": self.update_available,
            "notices": self.recent_notices.iter().map(|(m, e)| json!({ "message": m.to_string(), "error": e })).collect::<Vec<_>>(),
        })
    }
}
