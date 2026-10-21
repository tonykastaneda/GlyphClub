//! The main window's view: its state, every operation on the library, and
//! the action handlers the menus and shortcuts dispatch to. Rendering is
//! split across `chrome.rs` (sidebar, toolbar, sample bar), `grid.rs`,
//! `list.rs` and `detail.rs`.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use glyphclub::catalog::Catalog;
use glyphclub::font_info::FontInfo;
use glyphclub::{font_export, reveal};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::slider::{SliderEvent, SliderState, SliderValue};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme, Theme, ThemeMode, WindowExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::actions::{self, *};
use crate::bridge::{ActivateOutcome, BridgeEvent};
use crate::library::{Library, Nav};
use crate::palette::Palette;
use crate::settings::{AppSettings, Appearance, Settings, SortField, ViewMode};
use crate::update::{self, UpdateCheck};

pub const MIN_SIZE: f32 = 24.;
pub const MAX_SIZE: f32 = 160.;
/// Narrower than this, the sidebar tucks away and the toolbar compacts.
pub const COMPACT_W: f32 = 900.;
pub const DEFAULT_SAMPLE: &str = "Aa";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DetailTab {
    Preview,
    Sizes,
    Glyphs,
    Features,
    Info,
}

pub struct GlyphClub {
    pub catalog: Arc<Mutex<Catalog>>,
    pub lib: Library,
    pub nav: Nav,
    /// Indices into `lib.families` passing the current filter + search, in
    /// display order.
    pub visible: Vec<usize>,
    /// Selected families, by `Family::key`.
    pub selected: HashSet<SharedString>,
    /// Where a shift-click range starts, as an index into `visible`.
    pub anchor: Option<usize>,
    /// Fonts temporarily activated this session; deactivated on quit.
    pub temporary: HashSet<i64>,
    /// Fonts the Illustrator plug-in had activated this session.
    pub auto_activated: HashSet<i64>,
    /// Font files already handed to gpui's text system, and ones on the way.
    pub loaded: HashSet<i64>,
    pub pending: HashSet<i64>,
    /// The sidebar as you left it (collapsed by hand or not).
    pub sidebar_open: bool,
    /// Whether it's on screen this frame: a narrow window hides it unless
    /// it was opened on purpose while narrow (`sidebar_forced`).
    pub sidebar_shown: bool,
    pub sidebar_forced: bool,
    /// The window is narrow: toolbar buttons fold into the » menu.
    pub compact: bool,
    pub info_open: bool,
    pub busy: Option<SharedString>,
    pub notices: Vec<(SharedString, bool)>,
    pub update_available: Option<String>,
    pub dismissed_update: bool,
    /// Families expanded into their faces in the list view, by key.
    pub expanded: HashSet<SharedString>,
    pub sample: Entity<InputState>,
    pub search: Entity<InputState>,
    pub size_slider: Entity<SliderState>,
    pub grid_scroll: UniformListScrollHandle,
    pub list_scroll: UniformListScrollHandle,
    pub glyph_scroll: UniformListScrollHandle,
    pub resizing_sidebar: bool,
    pub resizing_detail: bool,
    pub focus: FocusHandle,
    pub applied_appearance: Option<(Appearance, bool)>,
    // Info panel
    pub detail_tab: DetailTab,
    pub detail_face: Option<i64>,
    pub detail_preview: Entity<TextareaState>,
    pub detail_size: Entity<SliderState>,
    pub detail_size_value: f32,
    pub detail_align: TextAlign,
    pub glyph_block: Option<&'static str>,
    pub info_cache: HashMap<i64, Option<Arc<FontInfo>>>,
    pub info_pending: HashSet<i64>,
    /// OpenType feature tags per face, read with its info.
    pub features_cache: HashMap<i64, Vec<String>>,
    /// Category per face, read when its file loads for a preview.
    pub categories: HashMap<i64, &'static str>,
    /// Mirrors Settings' expand_families for the views (set each render).
    pub expand_all: bool,
    /// The grid's tiles: (family, style when families are expanded).
    pub grid_items: Vec<(usize, Option<usize>)>,
    /// Columns in the grid's last layout (for scrolling to a tile).
    pub last_cols: usize,
    pub plugin_update_noted: Option<String>,
    /// Windows and Linux have no global menu bar, so the menus are drawn
    /// in the toolbar instead.
    pub app_menu_bar: Option<Entity<gpui_kit::component::menu::AppMenuBar>>,
    /// Features switched on or off in the info panel (tag -> on).
    pub detail_features: std::collections::BTreeMap<String, bool>,
    pub _subs: Vec<Subscription>,
}

impl GlyphClub {
    pub fn new(catalog: Catalog, changed: Arc<AtomicBool>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let lib = Library::load(&catalog).unwrap_or_else(|e| {
            eprintln!("loading catalog: {e:#}");
            Library::default()
        });
        let settings = cx.global::<AppSettings>().0;
        let sample = cx.new(|cx| InputState::new(window, cx).placeholder("Font name").default_value(DEFAULT_SAMPLE));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let size_slider = cx.new(|_| {
            SliderState::new().min(MIN_SIZE).max(MAX_SIZE).default_value(settings.preview_size)
        });
        let detail_preview = cx.new(|cx| {
            TextareaState::new(window, cx)
                .default_value("The quick brown fox jumps over the lazy dog. 0123456789")
        });
        let detail_size = cx.new(|_| SliderState::new().min(10.).max(200.).default_value(48.));

        let subs = vec![
            cx.subscribe(&sample, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) && cx.global::<AppSettings>().0.hide_missing {
                    this.refilter(cx);
                }
                cx.notify();
            }),
            cx.subscribe(&search, |this, _, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    this.refilter(cx);
                }
            }),
            cx.subscribe(&size_slider, |_, _, ev: &SliderEvent, cx| match ev {
                SliderEvent::Change(SliderValue::Single(v)) => {
                    let v = *v;
                    cx.global_mut::<AppSettings>().0.preview_size = v;
                    cx.notify();
                }
                SliderEvent::Release(_) => cx.global::<AppSettings>().0.save(),
                _ => {}
            }),
            cx.subscribe(&detail_size, |this, _, ev: &SliderEvent, cx| {
                if let SliderEvent::Change(SliderValue::Single(v)) = ev {
                    this.detail_size_value = *v;
                    cx.notify();
                }
            }),
            cx.observe_global::<AppSettings>(|this, cx| {
                this.refilter(cx);
            }),
            cx.observe_window_appearance(window, |this, _, cx| {
                this.applied_appearance = None;
                cx.notify();
            }),
        ];

        // The folder watcher fires on its own thread; this picks that up and
        // rescans in the background.
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(500)).await;
            if changed.swap(false, Ordering::AcqRel) && this.update(cx, |this, cx| this.rescan(cx)).is_err() {
                break;
            }
        })
        .detach();

        // A quiet update check on launch.
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async { update::check() }).await;
            if let UpdateCheck::Available(v) = result {
                let _ = this.update(cx, |this, cx| {
                    this.update_available = Some(v);
                    cx.notify();
                });
            }
        })
        .detach();

        let mut this = Self {
            catalog: Arc::new(Mutex::new(catalog)),
            lib,
            nav: Nav::All,
            visible: Vec::new(),
            selected: HashSet::new(),
            anchor: None,
            temporary: HashSet::new(),
            auto_activated: HashSet::new(),
            loaded: HashSet::new(),
            pending: HashSet::new(),
            sidebar_open: true,
            sidebar_shown: true,
            sidebar_forced: false,
            compact: false,
            info_open: false,
            busy: None,
            notices: Vec::new(),
            update_available: None,
            dismissed_update: false,
            expanded: HashSet::new(),
            sample,
            search,
            size_slider,
            grid_scroll: UniformListScrollHandle::new(),
            list_scroll: UniformListScrollHandle::new(),
            glyph_scroll: UniformListScrollHandle::new(),
            resizing_sidebar: false,
            resizing_detail: false,
            focus: cx.focus_handle(),
            applied_appearance: None,
            detail_tab: DetailTab::Preview,
            detail_face: None,
            detail_preview,
            detail_size,
            detail_size_value: 48.,
            detail_align: TextAlign::Left,
            glyph_block: None,
            info_cache: HashMap::new(),
            info_pending: HashSet::new(),
            features_cache: HashMap::new(),
            categories: HashMap::new(),
            expand_all: false,
            grid_items: Vec::new(),
            last_cols: 1,
            plugin_update_noted: None,
            app_menu_bar: (!cfg!(target_os = "macos")).then(|| gpui_kit::component::menu::AppMenuBar::new(cx)),
            detail_features: Default::default(),
            _subs: subs,
        };
        this.refilter(cx);
        this
    }

    pub fn update_settings(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Settings)) {
        f(&mut cx.global_mut::<AppSettings>().0);
        let s = cx.global::<AppSettings>().0;
        s.save();
        self.refresh_menus(cx);
        self.refilter(cx);
    }

    pub fn refresh_menus(&self, cx: &mut App) {
        let s = cx.global::<AppSettings>().0;
        actions::set_menus(&s, self.info_open, self.sidebar_open, cx);
        if let Some(bar) = &self.app_menu_bar {
            bar.update(cx, |bar, cx| bar.reload(cx));
        }
    }

    /// The sample text the previews show, never empty.
    /// The preview text, or `None` when it's empty: then every preview
    /// shows its own font's name.
    pub fn sample_text(&self, cx: &App) -> Option<SharedString> {
        let s = self.sample.read(cx).value();
        (!s.trim().is_empty()).then_some(s)
    }

    pub fn sample_chars(&self, cx: &App) -> Vec<u32> {
        let mut chars: Vec<u32> = self
            .sample_text(cx)
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| c as u32)
            .collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    pub fn refilter(&mut self, cx: &mut Context<Self>) {
        let s = cx.global::<AppSettings>().0;
        let query = self.search.read(cx).value();
        let chars = self.sample_chars(cx);
        self.visible = self.lib.filter(
            self.nav,
            &query,
            s.sort_field,
            s.sort_ascending,
            s.hide_missing.then_some(chars.as_slice()),
        );
        cx.notify();
    }

    pub fn reload(&mut self, cx: &mut Context<Self>) {
        let loaded = Library::load(&self.catalog.lock().unwrap());
        match loaded {
            Ok(lib) => self.lib = lib,
            Err(e) => self.notify_error(format!("Couldn't read the catalog: {e:#}")),
        }
        // Drop selections of families that no longer exist.
        let keys: HashSet<SharedString> = self.lib.families.iter().map(|f| f.key.clone()).collect();
        self.selected.retain(|k| keys.contains(k));
        self.refilter(cx);
    }

    pub fn notify(&mut self, message: impl Into<SharedString>) {
        self.notices.push((message.into(), false));
    }

    pub fn notify_error(&mut self, message: impl Into<SharedString>) {
        self.notices.push((message.into(), true));
    }

    /// Runs a slow catalog job (scanning, deleting) off the main thread,
    /// then reloads and reports its message.
    pub fn run_job(
        &mut self,
        label: &str,
        job: impl FnOnce(&mut Catalog) -> anyhow::Result<Option<String>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        self.busy = Some(label.to_string().into());
        cx.notify();
        let catalog = self.catalog.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { job(&mut catalog.lock().unwrap()) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Ok(Some(message)) => this.notify(message),
                    Ok(None) => {}
                    Err(e) => this.notify_error(format!("{e:#}")),
                }
                this.reload(cx);
            });
        })
        .detach();
    }

    pub fn rescan(&mut self, cx: &mut Context<Self>) {
        self.run_job("Syncing…", |c| c.rescan().map(|_| None), cx);
    }

    pub fn add_library_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let p = path.to_string_lossy().into_owned();
        self.run_job("Adding library…", move |c| c.add_folder(&p).map(|r| Some(r.status)), cx);
    }

    pub fn prompt_add_library(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: true,
            prompt: Some("Add Library".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let _ = this.update(cx, |this, cx| {
                for path in paths {
                    this.add_library_path(path, cx);
                }
            });
        })
        .detach();
    }

    pub fn remove_library(&mut self, id: i64, cx: &mut Context<Self>) {
        if self.nav == Nav::Folder(id) {
            self.nav = Nav::All;
        }
        self.run_job("Removing library…", move |c| c.remove_folder(id).map(|_| None), cx);
    }

    pub fn family_index(&self, key: &str) -> Option<usize> {
        self.lib.families.iter().position(|f| f.key.as_ref() == key)
    }

    /// The selected families, in display order.
    pub fn selection(&self) -> Vec<usize> {
        let mut out: Vec<usize> = self
            .selected
            .iter()
            .filter_map(|k| self.family_index(k))
            .collect();
        let order: HashMap<usize, usize> = self.visible.iter().enumerate().map(|(pos, &i)| (i, pos)).collect();
        out.sort_by_key(|i| order.get(i).copied().unwrap_or(usize::MAX));
        out
    }

    /// What an action from `key`'s own menu applies to: the selection if
    /// `key` is part of it, otherwise just `key`.
    pub fn targets(&self, key: Option<&SharedString>) -> Vec<usize> {
        match key {
            Some(k) if !self.selected.contains(k) => self.family_index(k).into_iter().collect(),
            _ => self.selection(),
        }
    }

    fn target_ids(&self, targets: &[usize]) -> Vec<i64> {
        targets.iter().flat_map(|&i| self.lib.families[i].ids().collect::<Vec<_>>()).collect()
    }

    pub fn select_click(&mut self, key: SharedString, modifiers: Modifiers, cx: &mut Context<Self>) {
        let pos = self.visible.iter().position(|&i| self.lib.families[i].key == key);
        if modifiers.shift {
            if let (Some(anchor), Some(pos)) = (self.anchor, pos) {
                let (a, b) = (anchor.min(pos), anchor.max(pos));
                if !modifiers.platform {
                    self.selected.clear();
                }
                for &i in &self.visible[a..=b] {
                    self.selected.insert(self.lib.families[i].key.clone());
                }
                cx.notify();
                return;
            }
        }
        if modifiers.platform {
            if !self.selected.remove(&key) {
                self.selected.insert(key);
            }
        } else {
            self.selected.clear();
            self.selected.insert(key);
            self.detail_face = None;
        }
        self.anchor = pos;
        cx.notify();
    }

    /// Activates every face of the targets, or deactivates them all if
    /// they're already fully active.
    pub fn toggle_active(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let ids = self.target_ids(&self.targets(key.as_ref()));
        let all_active = !ids.is_empty() && ids.iter().all(|id| self.lib.active.contains(id));
        self.set_active(ids, !all_active, cx);
    }

    pub fn set_active(&mut self, ids: Vec<i64>, on: bool, cx: &mut Context<Self>) {
        let mut failed = 0;
        {
            let mut catalog = self.catalog.lock().unwrap();
            for id in ids {
                let is_active = self.lib.active.contains(&id);
                if is_active != on {
                    if catalog.toggle_activation(id).is_err() {
                        failed += 1;
                    }
                    if !on {
                        self.temporary.remove(&id);
                    }
                }
            }
            self.lib.active = catalog.active_ids().unwrap_or_default();
        }
        if failed > 0 {
            self.notify_error(format!(
                "{failed} font{} couldn't be {}.",
                if failed == 1 { "" } else { "s" },
                if on { "activated" } else { "deactivated" }
            ));
        }
        self.after_state_change(cx);
    }

    pub fn activate_temporarily(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let ids = self.target_ids(&self.targets(key.as_ref()));
        {
            let mut catalog = self.catalog.lock().unwrap();
            for id in ids {
                match catalog.activate_temporarily(id) {
                    Ok(true) => {
                        self.temporary.insert(id);
                    }
                    Ok(false) => {}
                    Err(e) => self.notices.push((format!("Couldn't activate: {e:#}").into(), true)),
                }
            }
            self.lib.active = catalog.active_ids().unwrap_or_default();
        }
        self.after_state_change(cx);
    }

    pub fn toggle_star(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let ids = self.target_ids(&self.targets(key.as_ref()));
        let all_starred = !ids.is_empty() && ids.iter().all(|id| self.lib.starred.contains(id));
        {
            let mut catalog = self.catalog.lock().unwrap();
            for id in ids {
                if all_starred || !self.lib.starred.contains(&id) {
                    let _ = catalog.toggle_favorite(id);
                }
            }
            self.lib.starred = catalog.favorite_ids().unwrap_or_default();
        }
        self.after_state_change(cx);
    }

    pub fn toggle_tag(&mut self, key: Option<SharedString>, tag: i64, cx: &mut Context<Self>) {
        let ids = self.target_ids(&self.targets(key.as_ref()));
        let all_tagged = !ids.is_empty()
            && ids.iter().all(|id| self.lib.font_tags.get(id).is_some_and(|t| t.contains(&tag)));
        {
            let mut catalog = self.catalog.lock().unwrap();
            for id in ids {
                let tagged = self.lib.font_tags.get(&id).is_some_and(|t| t.contains(&tag));
                if tagged == all_tagged {
                    let _ = catalog.toggle_font_tag(id, tag);
                }
            }
            self.lib.font_tags = catalog.font_tags().unwrap_or_default();
        }
        self.after_state_change(cx);
    }

    pub fn create_tag(&mut self, name: &str, apply_to: Option<SharedString>, cx: &mut Context<Self>) {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let created = self.catalog.lock().unwrap().create_tag(name);
        match created {
            Ok(id) => {
                self.lib.tags = self.catalog.lock().unwrap().list_tags().unwrap_or_default();
                if apply_to.is_some() || !self.selected.is_empty() {
                    self.toggle_tag(apply_to, id, cx);
                }
            }
            Err(e) => self.notify_error(format!("Couldn't create the tag: {e:#}")),
        }
        cx.notify();
    }

    /// Re-filters only when the current view depends on what changed.
    fn after_state_change(&mut self, cx: &mut Context<Self>) {
        let sort = cx.global::<AppSettings>().0.sort_field;
        if matches!(self.nav, Nav::Starred | Nav::Tag(_))
            || matches!(sort, SortField::Enabled | SortField::Starred | SortField::Tags)
        {
            self.refilter(cx);
        }
        cx.notify();
    }

    pub fn copy_names(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let names: Vec<String> = self
            .targets(key.as_ref())
            .iter()
            .map(|&i| self.lib.families[i].name.to_string())
            .collect();
        if !names.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(names.join("\n")));
        }
    }

    pub fn reveal(&mut self, key: Option<SharedString>) {
        if let Some(&i) = self.targets(key.as_ref()).first() {
            let _ = reveal::reveal_file(Path::new(&self.lib.families[i].rep().path));
        }
    }

    /// Font ▸ Export…: copies the targets' font files into a chosen folder.
    pub fn export(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let mut paths: Vec<String> = self
            .targets(key.as_ref())
            .iter()
            .flat_map(|&i| self.lib.families[i].faces.iter().map(|f| f.path.clone()))
            .collect();
        paths.sort();
        paths.dedup();
        if paths.is_empty() {
            return;
        }
        let dest = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Export Here".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = dest.await else { return };
            let Some(dir) = dirs.into_iter().next() else { return };
            let (copied, failed) = cx
                .background_executor()
                .spawn(async move {
                    let (mut copied, mut failed) = (0, 0);
                    for p in paths {
                        let src = Path::new(&p);
                        let Some(name) = src.file_name() else { continue };
                        match std::fs::copy(src, dir.join(name)) {
                            Ok(_) => copied += 1,
                            Err(_) => failed += 1,
                        }
                    }
                    (copied, failed)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if failed > 0 {
                    this.notify_error(format!("Exported {copied} file(s); {failed} couldn't be copied."));
                } else {
                    this.notify(format!("Exported {copied} file{}.", if copied == 1 { "" } else { "s" }));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn export_user_fonts(&mut self, cx: &mut Context<Self>) {
        let dest = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Export Here".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dirs))) = dest.await else { return };
            let Some(dir) = dirs.into_iter().next() else { return };
            let result = cx
                .background_executor()
                .spawn(async move { font_export::export_user_fonts(&dir) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    Ok(s) => this.notify(format!(
                        "Exported {} font{} to {}{}{}",
                        s.copied,
                        if s.copied == 1 { "" } else { "s" },
                        s.dest.display(),
                        if s.skipped > 0 { format!(", {} already there", s.skipped) } else { String::new() },
                        if s.failed > 0 { format!(", {} failed", s.failed) } else { String::new() },
                    )),
                    Err(e) => this.notify_error(format!("Export failed: {e:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Font ▸ Remove from Fontlist: forgets the files (they stay on disk,
    /// and future scans skip them).
    pub fn remove_from_fontlist(&mut self, key: Option<SharedString>, cx: &mut Context<Self>) {
        let ids = self.target_ids(&self.targets(key.as_ref()));
        if ids.is_empty() {
            return;
        }
        self.run_job("Removing…", move |c| {
            for id in ids {
                c.remove_from_fontlist(id)?;
            }
            Ok(None)
        }, cx);
    }

    /// Font ▸ Delete from Library…: deletes the files themselves, after a
    /// confirmation, since a library is usually a shared synced folder.
    pub fn confirm_delete(&mut self, key: Option<SharedString>, window: &mut Window, cx: &mut Context<Self>) {
        let targets = self.targets(key.as_ref());
        if targets.is_empty() {
            return;
        }
        let ids = self.target_ids(&targets);
        let what = if targets.len() == 1 {
            format!("“{}”", self.lib.families[targets[0]].name)
        } else {
            format!("{} families", targets.len())
        };
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Delete {what} from the library?"),
            Some(&format!(
                "This deletes {} font file{} from disk, including from any shared folder it syncs with. It can't be undone.",
                ids.len(),
                if ids.len() == 1 { "" } else { "s" }
            )),
            &["Delete", "Cancel"],
            cx,
        );
        cx.spawn(async move |this, cx| {
            if answer.await != Ok(0) {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.run_job("Deleting…", move |c| {
                    for id in ids {
                        c.delete_font_file(id)?;
                    }
                    Ok(None)
                }, cx);
            });
        })
        .detach();
    }

    /// The card's Ai button: activates the family's preview face if needed,
    /// then sets the selected text in Illustrator to it.
    pub fn apply_in_illustrator(&mut self, key: SharedString, cx: &mut Context<Self>) {
        let Some(i) = self.family_index(&key) else { return };
        let fam = &self.lib.families[i];
        let face = self
            .detail_face
            .filter(|_| self.selected.contains(&key))
            .and_then(|id| fam.faces.iter().find(|f| f.id == id))
            .unwrap_or(fam.rep());
        let (id, ps) = (face.id, face.postscript_name.clone());
        if ps.is_empty() {
            self.notify_error("This font has no PostScript name, so Illustrator can't select it.");
            cx.notify();
            return;
        }
        if !crate::illustrator::is_running() {
            self.notify_error("Illustrator isn't running.");
            cx.notify();
            return;
        }
        let newly_active = !self.lib.active.contains(&id);
        if newly_active {
            let activated = self.catalog.lock().unwrap().activate_temporarily(id);
            match activated {
                Ok(_) => {
                    self.lib.active.insert(id);
                    self.temporary.insert(id);
                }
                Err(e) => {
                    self.notify_error(format!("Couldn't activate the font: {e:#}"));
                    cx.notify();
                    return;
                }
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            use crate::illustrator::{apply_font, ApplyResult};
            // Illustrator can take a moment to see a font that was just
            // activated; retry a few times before giving up.
            let mut result = ApplyResult::FontNotLoaded;
            for attempt in 0..6 {
                if attempt > 0 || newly_active {
                    cx.background_executor().timer(Duration::from_millis(800)).await;
                }
                let ps = ps.clone();
                result = cx.background_executor().spawn(async move { apply_font(&ps) }).await;
                if !matches!(result, ApplyResult::FontNotLoaded) {
                    break;
                }
            }
            let _ = this.update(cx, |this, cx| {
                match result {
                    ApplyResult::Applied => {}
                    ApplyResult::NoDocument => this.notify_error("Open a document in Illustrator first."),
                    ApplyResult::NoTextSelected => this.notify_error("Select some text in Illustrator first."),
                    ApplyResult::FontNotLoaded => this.notify_error("Illustrator hasn't picked up the font yet. Try again in a moment."),
                    ApplyResult::NotRunning => this.notify_error("Illustrator isn't running."),
                    ApplyResult::Failed(e) => this.notify_error(format!("Illustrator: {e}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// "Which library?" for font files dropped on the window: copies them
    /// into the chosen library folder (never overwriting) and rescans.
    pub fn open_drop_picker(&mut self, files: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        if self.lib.folders.is_empty() {
            self.notify_error("Add a library first, then drop fonts into it.");
            cx.notify();
            return;
        }
        let folders: Vec<(String, SharedString)> =
            self.lib.folders.iter().map(|f| (f.path.clone(), f.name.clone())).collect();
        let weak = cx.entity().downgrade();
        let n = files.len();
        let title = if n == 1 {
            let name = files[0].file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            format!("Add “{name}” to…")
        } else {
            format!("Add {n} fonts to…")
        };
        window.open_dialog(cx, move |dialog, _, cx| {
            let pal = Palette::get(cx);
            let mut list = div().flex().flex_col().gap(px(2.));
            for (k, (path, name)) in folders.iter().enumerate() {
                let (path, files, weak) = (path.clone(), files.clone(), weak.clone());
                list = list.child(
                    div()
                        .id(("drop-lib", k))
                        .h(px(32.))
                        .px(px(10.))
                        .rounded(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .hover(|s| s.bg(pal.hover))
                        .cursor_pointer()
                        .child(gpui_kit::component::Icon::new(gpui_kit::assets::IconName::FolderOpen).size(px(16.)))
                        .child(div().text_size(px(13.)).child(name.clone()))
                        .on_click(move |_, window, cx| {
                            let (path, files) = (path.clone(), files.clone());
                            let _ = weak.update(cx, |this, cx| this.import_into(path, files, cx));
                            window.close_dialog(cx);
                        }),
                );
            }
            dialog.title(title.clone()).w(px(360.)).child(list)
        });
    }

    fn import_into(&mut self, folder: String, files: Vec<PathBuf>, cx: &mut Context<Self>) {
        let s = cx.global::<AppSettings>().0;
        let (skip_dupes, activate) = (s.import_skip_duplicates, s.import_activate);
        self.run_job("Adding fonts…", move |c| {
            let (mut copied, mut skipped, mut failed) = (0, 0, 0);
            let mut imported_names: Vec<String> = Vec::new();
            for src in &files {
                let Some(name) = src.file_name() else { continue };
                let dest = Path::new(&folder).join(name);
                if dest.exists() {
                    skipped += 1;
                    continue;
                }
                // PostScript names inside the file, for duplicate checks and
                // activating it afterwards.
                let names: Vec<String> = std::fs::read(src)
                    .map(|data| {
                        glyphclub::font::parse_faces(&data, src, 0, 0)
                            .into_iter()
                            .map(|f| f.postscript_name)
                            .filter(|n| !n.is_empty())
                            .collect()
                    })
                    .unwrap_or_default();
                if skip_dupes
                    && !names.is_empty()
                    && names.iter().all(|n| !c.font_ids_for_postscript_name(n).unwrap_or_default().is_empty())
                {
                    skipped += 1;
                    continue;
                }
                match std::fs::copy(src, &dest) {
                    Ok(_) => {
                        copied += 1;
                        imported_names.extend(names);
                    }
                    Err(_) => failed += 1,
                }
            }
            c.rescan()?;
            if activate {
                let active = c.active_ids().unwrap_or_default();
                for name in &imported_names {
                    for id in c.font_ids_for_postscript_name(name).unwrap_or_default() {
                        if !active.contains(&id) {
                            let _ = c.toggle_activation(id);
                        }
                    }
                }
            }
            let mut msg = format!("Added {copied} font{}", if copied == 1 { "" } else { "s" });
            if skipped > 0 {
                msg.push_str(&format!(", {skipped} skipped (already in the library)"));
            }
            if failed > 0 {
                msg.push_str(&format!(", {failed} couldn't be copied"));
            }
            Ok(Some(msg + "."))
        }, cx);
    }

    /// On quit: temporary activations always go; plug-in activations and
    /// manual ones per Settings ▸ Auto Activation ▸ Auto Deactivation.
    pub fn deactivate_on_quit(&mut self, settings: &Settings) {
        let mut catalog = self.catalog.lock().unwrap();
        for id in self.temporary.drain() {
            let _ = catalog.force_deactivate(id);
        }
        if settings.deactivate_auto_on_quit {
            for id in self.auto_activated.drain() {
                let _ = catalog.force_deactivate(id);
            }
        }
        if settings.deactivate_manual_on_quit {
            let manual: Vec<i64> = catalog
                .active_ids()
                .unwrap_or_default()
                .into_iter()
                .filter(|id| !self.auto_activated.contains(id))
                .collect();
            for id in manual {
                let _ = catalog.toggle_activation(id);
            }
        }
    }

    pub fn set_nav(&mut self, nav: Nav, cx: &mut Context<Self>) {
        self.nav = nav;
        self.selected.clear();
        self.anchor = None;
        self.refilter(cx);
    }

    pub fn set_sort(&mut self, field: SortField, cx: &mut Context<Self>) {
        self.update_settings(cx, |s| {
            if s.sort_field == field {
                s.sort_ascending = !s.sort_ascending;
            } else {
                s.sort_field = field;
                s.sort_ascending = field.default_ascending();
            }
        });
    }

    pub fn zoom(&mut self, delta: f32, window: &mut Window, cx: &mut Context<Self>) {
        let v = (cx.global::<AppSettings>().0.preview_size + delta).clamp(MIN_SIZE, MAX_SIZE);
        self.update_settings(cx, |s| s.preview_size = v);
        self.size_slider.update(cx, |s, cx| s.set_value(v, window, cx));
    }

    /// The Illustrator plug-in's request: temporarily activate whatever the
    /// libraries have of the document's missing fonts.
    /// The Illustrator plug-in's request: activate whatever the libraries
    /// have of the document's missing fonts, per Settings ▸ Auto Activation.
    pub fn activate_for_illustrator(&mut self, fonts: Vec<String>, cx: &mut Context<Self>) -> ActivateOutcome {
        let s = cx.global::<AppSettings>().0;
        let mut outcome = ActivateOutcome::default();
        let mut seen = HashSet::new();
        {
            let mut catalog = self.catalog.lock().unwrap();
            for name in fonts {
                let name = name.trim().to_string();
                if name.is_empty() || !seen.insert(name.clone()) {
                    continue;
                }
                let ids = catalog.font_ids_for_postscript_name(&name).unwrap_or_default();
                let Some(&first) = ids.first() else {
                    outcome.not_found.push(name);
                    continue;
                };
                if ids.iter().any(|id| self.lib.active.contains(id)) {
                    outcome.already_active.push(name);
                    continue;
                }
                // The face itself, or its whole family.
                let mut targets = vec![first];
                if s.activate_entire_family {
                    if let Some(fam) = self.lib.families.iter().find(|f| f.ids().any(|id| id == first)) {
                        targets = fam.ids().filter(|id| !self.lib.active.contains(id)).collect();
                    }
                }
                let mut ok = false;
                for id in targets {
                    // Kept only for the session unless auto-activated fonts
                    // are set to stay after quitting.
                    let result = if s.deactivate_auto_on_quit {
                        catalog.activate_temporarily(id).map(|_| ())
                    } else {
                        catalog.toggle_activation(id).map(|_| ())
                    };
                    if result.is_ok() {
                        ok |= id == first;
                        self.lib.active.insert(id);
                        self.auto_activated.insert(id);
                    }
                }
                if ok {
                    outcome.activated.push(name);
                } else {
                    outcome.failed.push(name);
                }
            }
        }
        if !outcome.activated.is_empty() && !s.silent_mode {
            let n = outcome.activated.len();
            self.notify(format!("Activated {n} font{} for Illustrator", if n == 1 { "" } else { "s" }));
        }
        cx.notify();
        outcome
    }

    pub fn handle_bridge(&mut self, event: BridgeEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            BridgeEvent::Activate { fonts, reply } => {
                let outcome = if cx.global::<AppSettings>().0.auto_activate_illustrator {
                    self.activate_for_illustrator(fonts, cx)
                } else {
                    ActivateOutcome { not_found: Vec::new(), failed: fonts, ..Default::default() }
                };
                let _ = reply.send(outcome);
            }
            BridgeEvent::Show => {
                cx.activate(true);
                window.activate_window();
            }
            BridgeEvent::OpenIntegrations => {
                cx.activate(true);
                crate::settings_window::open(Some(crate::settings_window::Page::AutoActivation), cx);
            }
            BridgeEvent::Reload => self.reload(cx),
            BridgeEvent::Ui { command, args, reply } => {
                let result = self.handle_ui(&command, &args, window, cx);
                let _ = reply.send(result);
            }
            BridgeEvent::PluginVersionSeen(seen) => {
                // Once per session: an older plug-in than this build ships.
                if self.plugin_update_noted.is_none() {
                    if let Some(latest) = crate::bridge::latest_plugin_version() {
                        if seen != latest {
                            self.notify(format!(
                                "The Illustrator plug-in ({seen}) is out of date. Update it to {latest} in Settings ▸ Integrations."
                            ));
                            cx.notify();
                        }
                    }
                    self.plugin_update_noted = Some(seen);
                }
            }
        }
    }

    /// Hands the font files for `wanted` to gpui's text system, reading them
    /// off the main thread.
    pub fn request_fonts(&mut self, wanted: Vec<(i64, String)>, cx: &mut Context<Self>) {
        let todo: Vec<_> = wanted
            .into_iter()
            .filter(|(id, _)| !self.loaded.contains(id) && self.pending.insert(*id))
            .collect();
        if todo.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move {
                    todo.into_iter()
                        .map(|(id, path)| {
                            let bytes = std::fs::read(path).ok();
                            let category = bytes.as_deref().map(crate::library::category_of).unwrap_or("");
                            (id, bytes, category)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let mut blobs = Vec::new();
                for (id, bytes, category) in read {
                    this.pending.remove(&id);
                    this.loaded.insert(id);
                    this.categories.insert(id, category);
                    if let Some(b) = bytes {
                        blobs.push(Cow::Owned(b));
                    }
                }
                if !blobs.is_empty() {
                    let _ = cx.text_system().add_fonts(blobs);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Reads one face's metadata and characters for the info panel.
    pub fn request_info(&mut self, id: i64, path: String, cx: &mut Context<Self>) {
        if self.info_cache.contains_key(&id) || !self.info_pending.insert(id) {
            return;
        }
        cx.spawn(async move |this, cx| {
            let (info, features) = cx
                .background_executor()
                .spawn(async move {
                    let Ok(data) = std::fs::read(&path) else { return (None, Vec::new()) };
                    let info = glyphclub::font_info::extract(&data, &path).map(Arc::new);
                    (info, crate::detail::feature_tags(&data))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.info_pending.remove(&id);
                this.info_cache.insert(id, info);
                this.features_cache.insert(id, features);
                cx.notify();
            });
        })
        .detach();
    }

    fn apply_appearance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = cx.global::<AppSettings>().0.appearance;
        let system_dark = matches!(window.appearance(), WindowAppearance::Dark | WindowAppearance::VibrantDark);
        if self.applied_appearance == Some((appearance, system_dark)) {
            return;
        }
        self.applied_appearance = Some((appearance, system_dark));
        let dark = match appearance {
            Appearance::SystemDefault => system_dark,
            Appearance::Light => false,
            Appearance::Dark => true,
        };
        Theme::change(if dark { ThemeMode::Dark } else { ThemeMode::Light }, Some(window), cx);
        crate::palette::apply_brand(cx);
    }

    fn flush_notices(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for (message, is_error) in self.notices.drain(..) {
            let note = if is_error { Notification::error(message) } else { Notification::info(message) };
            window.push_notification(note, cx);
        }
    }

    pub fn open_about(window: &mut Window, cx: &mut App) {
        window.open_dialog(cx, |dialog, _, cx| {
            let pal = Palette::get(cx);
            dialog
                .w(px(360.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.))
                        .py(px(10.))
                        .child(img("branding/icon.png").size(px(84.)))
                        .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD).child("GlyphClub"))
                        .child(
                            div()
                                .text_size(px(12.))
                                .text_color(pal.text_dim)
                                .child(format!("Version {}", env!("CARGO_PKG_VERSION"))),
                        )
                        .child(
                            div()
                                .pt(px(6.))
                                .text_size(px(12.))
                                .text_color(pal.text_dim)
                                .child("A font manager for the folders your team already syncs."),
                        )
                        .child(
                            div()
                                .id("about-github")
                                .pt(px(8.))
                                .text_size(px(12.))
                                .text_color(pal.select)
                                .cursor_pointer()
                                .child("GlyphClub on GitHub")
                                .on_click(|_, _, cx| cx.open_url(update::REPO_URL)),
                        ),
                )
        });
    }

    pub fn check_for_updates_manually(&mut self, cx: &mut Context<Self>) {
        self.notify("Checking for updates…");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = cx.background_executor().spawn(async { update::check() }).await;
            let _ = this.update(cx, |this, cx| {
                match result {
                    UpdateCheck::Available(v) => {
                        this.update_available = Some(v);
                        this.dismissed_update = false;
                    }
                    UpdateCheck::UpToDate => this.notify(format!(
                        "GlyphClub {} is the latest version.",
                        env!("CARGO_PKG_VERSION")
                    )),
                    UpdateCheck::Failed => this.notify_error("Couldn't reach GitHub to check for updates."),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn register_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|_, _: &About, window, cx| Self::open_about(window, cx)))
            .on_action(cx.listener(|this, _: &CheckForUpdates, _, cx| this.check_for_updates_manually(cx)))
            .on_action(cx.listener(|_, _: &OpenSettings, _, cx| crate::settings_window::open(None, cx)))
            .on_action(cx.listener(|this, _: &AddLibrary, _, cx| this.prompt_add_library(cx)))
            .on_action(cx.listener(|this, _: &SyncNow, _, cx| this.rescan(cx)))
            .on_action(cx.listener(|this, _: &ExportUserFonts, _, cx| this.export_user_fonts(cx)))
            .on_action(cx.listener(|this, _: &Find, window, cx| {
                this.search.update(cx, |s, cx| s.focus(window, cx));
            }))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.selected = this.visible.iter().map(|&i| this.lib.families[i].key.clone()).collect();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ClearSelection, window, cx| {
                this.selected.clear();
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ViewGrid, _, cx| this.update_settings(cx, |s| s.view_mode = ViewMode::Grid)))
            .on_action(cx.listener(|this, _: &ViewList, _, cx| this.update_settings(cx, |s| s.view_mode = ViewMode::List)))
            .on_action(cx.listener(|this, _: &ToggleInfo, _, cx| {
                this.info_open = !this.info_open;
                this.refresh_menus(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSidebar, _, cx| {
                if this.sidebar_shown {
                    this.sidebar_open = false;
                    this.sidebar_forced = false;
                } else {
                    this.sidebar_open = true;
                    this.sidebar_forced = this.compact;
                }
                this.refresh_menus(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &ToggleSampleBar, _, cx| {
                this.update_settings(cx, |s| s.show_sample_bar = !s.show_sample_bar)
            }))
            .on_action(cx.listener(|this, _: &ZoomIn, window, cx| this.zoom(12., window, cx)))
            .on_action(cx.listener(|this, _: &ZoomOut, window, cx| this.zoom(-12., window, cx)))
            .on_action(cx.listener(|this, _: &ToggleHideMissing, _, cx| {
                this.update_settings(cx, |s| s.hide_missing = !s.hide_missing)
            }))
            .on_action(cx.listener(|this, _: &SortByName, _, cx| this.set_sort(SortField::Name, cx)))
            .on_action(cx.listener(|this, _: &SortByDateAdded, _, cx| this.set_sort(SortField::DateAdded, cx)))
            .on_action(cx.listener(|this, _: &SortByEnabled, _, cx| this.set_sort(SortField::Enabled, cx)))
            .on_action(cx.listener(|this, _: &SortByStarred, _, cx| this.set_sort(SortField::Starred, cx)))
            .on_action(cx.listener(|this, _: &SortByStyles, _, cx| this.set_sort(SortField::Styles, cx)))
            .on_action(cx.listener(|this, _: &SortByFormat, _, cx| this.set_sort(SortField::Format, cx)))
            .on_action(cx.listener(|this, _: &SortByTags, _, cx| this.set_sort(SortField::Tags, cx)))
            .on_action(cx.listener(|this, _: &Activate, _, cx| {
                let ids = this.target_ids(&this.selection());
                this.set_active(ids, true, cx);
            }))
            .on_action(cx.listener(|this, _: &ActivateTemporarily, _, cx| this.activate_temporarily(None, cx)))
            .on_action(cx.listener(|this, _: &Deactivate, _, cx| {
                let ids = this.target_ids(&this.selection());
                this.set_active(ids, false, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleStar, _, cx| this.toggle_star(None, cx)))
            .on_action(cx.listener(|this, _: &CopyFontName, _, cx| this.copy_names(None, cx)))
            .on_action(cx.listener(|this, _: &ExportSelected, _, cx| this.export(None, cx)))
            .on_action(cx.listener(|this, _: &RevealSelected, _, _| this.reveal(None)))
            .on_action(cx.listener(|this, _: &RemoveFromFontlist, _, cx| this.remove_from_fontlist(None, cx)))
            .on_action(cx.listener(|this, _: &DeleteSelected, window, cx| this.confirm_delete(None, window, cx)))
            .on_action(cx.listener(|this, _: &ExpandFamilies, _, cx| {
                this.update_settings(cx, |s| s.expand_families = !s.expand_families)
            }))
            .on_action(cx.listener(|this, _: &EditPreviewText, window, cx| {
                if !cx.global::<AppSettings>().0.show_sample_bar {
                    this.update_settings(cx, |s| s.show_sample_bar = true);
                }
                this.sample.update(cx, |s, cx| {
                    s.focus(window, cx);
                    s.select_all(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &ToggleDarkMode, window, cx| {
                let dark = cx.theme().mode.is_dark();
                let _ = window;
                this.update_settings(cx, |s| s.appearance = if dark { Appearance::Light } else { Appearance::Dark });
                this.applied_appearance = None;
            }))
            .on_action(cx.listener(|_, _: &ToggleFullScreen, window, _| window.toggle_fullscreen()))
            .on_action(cx.listener(|_, _: &Minimize, window, _| window.minimize_window()))
            .on_action(cx.listener(|_, _: &ZoomWindow, window, _| window.zoom_window()))
    }
}

impl Focusable for GlyphClub {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for GlyphClub {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.apply_appearance(window, cx);
        self.flush_notices(window, cx);
        let pal = Palette::get(cx);
        let settings = cx.global::<AppSettings>().0;
        self.expand_all = settings.expand_families;
        self.compact = f32::from(window.viewport_size().width) < COMPACT_W;
        if !self.compact {
            self.sidebar_forced = false;
        }
        self.sidebar_shown = self.sidebar_open && (!self.compact || self.sidebar_forced);

        let content = if self.visible.is_empty() {
            self.empty_state(&pal, cx).into_any_element()
        } else if settings.view_mode == ViewMode::List {
            self.render_list(&pal, window, cx).into_any_element()
        } else {
            self.render_grid(&pal, window, cx).into_any_element()
        };

        let root = div()
            .id("glyphclub")
            .key_context(actions::CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .bg(pal.window)
            .text_color(pal.text)
            .flex()
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                if this.resizing_detail {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.resizing_detail = false;
                        cx.global::<AppSettings>().0.save();
                        return;
                    }
                    let w = (f32::from(window.viewport_size().width) - f32::from(ev.position.x))
                        .clamp(crate::detail::DETAIL_MIN_W, crate::detail::DETAIL_MAX_W);
                    cx.global_mut::<AppSettings>().0.detail_width = w;
                    cx.notify();
                }
                if this.resizing_sidebar {
                    if ev.pressed_button != Some(MouseButton::Left) {
                        this.resizing_sidebar = false;
                        cx.global::<AppSettings>().0.save();
                        return;
                    }
                    let w = f32::from(ev.position.x).clamp(crate::chrome::SIDEBAR_MIN_W, crate::chrome::SIDEBAR_MAX_W);
                    cx.global_mut::<AppSettings>().0.sidebar_width = w;
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.resizing_sidebar || this.resizing_detail {
                    this.resizing_sidebar = false;
                    this.resizing_detail = false;
                    cx.global::<AppSettings>().0.save();
                }
            }))
            .when(self.resizing_sidebar || self.resizing_detail, |d| d.cursor_col_resize())
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                // Folders become libraries; font files go into a library you
                // pick.
                let mut fonts = Vec::new();
                for p in paths.paths() {
                    if p.is_dir() {
                        this.add_library_path(p.clone(), cx);
                    } else if glyphclub::font::has_font_extension(p) {
                        fonts.push(p.clone());
                    }
                }
                if !fonts.is_empty() {
                    this.open_drop_picker(fonts, window, cx);
                }
            }))
            .when(self.sidebar_shown, |d| d.child(self.sidebar(&pal, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .relative()
                    .flex()
                    .flex_col()
                    .child(self.toolbar(&pal, cx))
                    .when_some(self.update_banner(&pal, cx), |d, b| d.child(b))
                    .child(div().flex_1().min_h_0().child(content))
                    .when(settings.show_sample_bar, |d| d.child(self.sample_bar(&pal, cx))),
            )
            .when(self.info_open, |d| d.child(self.render_detail(&pal, window, cx)));

        let root: Div = div().size_full().child(root);
        self.register_actions(root, cx)
    }
}
