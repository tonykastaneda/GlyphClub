use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use parley::editing::PlainEditor;
use vello::kurbo::{Point, Rect};
use vello::peniko::Brush;
use winit::event_loop::EventLoopProxy;

use crate::catalog::Catalog;
use crate::settings::Settings;
use crate::store::FontRow;
use crate::text::TextCx;
use crate::ui::{ContextMenu, HitRegion};
use crate::AppEvent;

/// Which data a list-view column shows. `Name`'s width doubles as a lower
/// bound rather than a hard size — see `ListColumnSpec` — everything else
/// is a plain fixed width.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ListColumnKind {
    Name,
    Preview,
    Styles,
    Favorite,
    Format,
    Category,
    DateAdded,
}

impl ListColumnKind {
    pub fn label(self) -> &'static str {
        match self {
            ListColumnKind::Name => "Name",
            ListColumnKind::Preview => "Preview",
            ListColumnKind::Styles => "Styles",
            ListColumnKind::Favorite => "",
            ListColumnKind::Format => "Format",
            ListColumnKind::Category => "Category",
            ListColumnKind::DateAdded => "Date Added",
        }
    }

    pub fn default_width(self) -> f64 {
        match self {
            ListColumnKind::Name => 260.0,
            ListColumnKind::Preview => 120.0,
            ListColumnKind::Styles => 90.0,
            ListColumnKind::Favorite => 44.0,
            ListColumnKind::Format => 190.0,
            ListColumnKind::Category => 150.0,
            ListColumnKind::DateAdded => 190.0,
        }
    }

    pub fn min_width(self) -> f64 {
        match self {
            ListColumnKind::Name => 120.0,
            ListColumnKind::Preview => 60.0,
            ListColumnKind::Favorite => 36.0,
            _ => 70.0,
        }
    }
}

/// One column's current place in the list view — order in `App::list_columns`
/// is display order (drag-to-reorder just moves an entry within that `Vec`),
/// `width` is independently drag-resizable per column (see `ui::mod`'s
/// `update_column_resize_from_point`/`update_column_reorder_from_point`).
#[derive(Clone, Copy)]
pub struct ListColumnSpec {
    pub kind: ListColumnKind,
    pub width: f64,
}

pub fn default_list_columns() -> Vec<ListColumnSpec> {
    [
        ListColumnKind::Name,
        ListColumnKind::Preview,
        ListColumnKind::Styles,
        ListColumnKind::Favorite,
        ListColumnKind::Format,
        ListColumnKind::Category,
        ListColumnKind::DateAdded,
    ]
    .into_iter()
    .map(|kind| ListColumnSpec {
        kind,
        width: kind.default_width(),
    })
    .collect()
}

#[derive(Clone, Copy, PartialEq)]
pub struct ListSort {
    pub column: ListColumnKind,
    pub ascending: bool,
}

/// One family's worth of list-view rows, pre-grouped and holding only
/// owned/copied data — see `ui::list::rebuild_family_groups`, and
/// `App::list_groups_cache`, which is what actually keeps this from being
/// rebuilt on every frame.
pub struct FamilyGroup {
    pub family: String,
    /// Member ids in a stable display order (by weight, then subfamily).
    pub member_ids: Vec<i64>,
    pub primary_id: i64,
    pub format: &'static str,
    pub newest_mtime: i64,
    pub any_favorite: bool,
    /// Precomputed once per group at build time — every sort compares
    /// this rather than lowercasing `family` again on every comparison
    /// the sort makes (`O(n log n)` allocations otherwise, for nothing).
    pub family_sort_key: String,
}

/// What `list_groups_cache` was built from — recomputed (cheaply) every
/// frame `ui::list::draw_list` runs and compared against the cache's own
/// copy; a mismatch on any field means the grouped/sorted row list itself
/// is stale and needs rebuilding, not just re-drawing.
#[derive(Clone, PartialEq)]
pub struct ListGroupsCacheKey {
    pub entries_version: u64,
    pub favorites_version: u64,
    pub filter: Filter,
    pub search: String,
    pub sort: ListSort,
}

/// What `App::visible_ids_cache` was built from — same idea as
/// `ListGroupsCacheKey`, for `ui::grid::draw_grid`'s filtered id list.
/// `visible_entries()` itself is a full `O(n)` scan+allocation of
/// `entries`; grid view was calling it fresh every frame regardless of
/// scroll position, which at a 150k-font library meant re-touching all
/// 150k entries (twice — once to filter, once to map to ids) on every
/// single redraw even though only a couple hundred rows are ever drawn.
#[derive(Clone, PartialEq)]
pub struct VisibleIdsCacheKey {
    pub entries_version: u64,
    pub favorites_version: u64,
    pub filter: Filter,
    pub search: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    System,
    Starred,
    /// Anything added to a library within the last 14 days — approximated
    /// from each entry's file `mtime`, same as the list view's "Date
    /// Added" column, since GlyphClub has no separate "date added to
    /// catalog" timestamp of its own.
    Recent,
    Folder(i64),
    Tag(i64),
}

/// How far back "Recent" reaches.
pub const RECENT_WINDOW_SECS: i64 = 14 * 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

/// The hand-rolled Windows menu bar's top-level menus (`ui::sidebar`... no
/// — see `ui::draw_windows_menu_bar`, drawn in the toolbar's otherwise-
/// empty gap on Windows). macOS gets a real `NSMenu` instead (see
/// `native_menu.rs`), so this only ever renders there, though the state
/// itself isn't `cfg`'d out — it's cheap, and keeping `App` platform-
/// uniform avoids `cfg`-gating this struct definition.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WinMenuKind {
    File,
    View,
    Font,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    None,
    Search,
    PreviewText,
    /// The sample-text picker's editable field (`app.sample_text`) — the
    /// short string every grid/list tile renders its live preview in, not
    /// `preview_text` (the detail panel's own, separate specimen string).
    SampleText,
    /// The Glyphs tab's search field (`app.glyph_search`).
    GlyphSearch,
    /// The Preview tab's font-size field (`app.preview_size_input`) — free
    /// text while focused, parsed into `app.preview_size` on every valid
    /// keystroke (see `ui::detail::draw_preview_tab`).
    PreviewSize,
    /// The font context menu's "New Tag…" inline field (`FontContextMenu::
    /// new_tag_text`) — see `ui::mod::FontContextMenu`.
    NewTag,
}

/// The Preview tab's text-alignment control — see `ui::detail`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justify,
}

/// The detail panel's tab bar — see `ui::detail`. No "Features" tab (OT
/// feature toggling/preview), by explicit request; the other three plus
/// Info are the whole set.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Preview,
    Sizes,
    Glyphs,
    Info,
}

pub struct FolderEntry {
    pub id: i64,
    pub path: String,
}

pub struct FontEntry {
    pub id: i64,
    pub folder_id: i64,
    pub path: String,
    pub family: String,
    pub subfamily: String,
    pub weight: u16,
    pub italic: bool,
    pub monospace: bool,
    pub is_system: bool,
    pub mtime: i64,
}

impl From<FontRow> for FontEntry {
    fn from(row: FontRow) -> Self {
        let r = row.record;
        FontEntry {
            id: row.id,
            folder_id: row.folder_id,
            is_system: crate::font::is_system_path(&r.path),
            path: r.path,
            family: r.family,
            subfamily: r.subfamily,
            weight: r.weight,
            italic: r.italic,
            monospace: r.monospace,
            mtime: r.mtime,
        }
    }
}

/// Font-id -> registered family name, so a scrolled-past tile's preview
/// doesn't get re-read off disk and re-shaped every time it scrolls back
/// into view. `None` means a load was already tried and failed (parse
/// error, missing file) — don't retry every frame.
///
/// Unlike the old web UI's `fontCache`, this never evicts: unregistering a
/// font from fontique mid-session is more complexity than a font manager's
/// catalog (thousands of faces, but only ever a screenful loaded at once)
/// currently needs. Revisit if memory turns out to matter in practice.
pub type PreviewCache = HashMap<i64, Option<String>>;

/// The result of asking `FontBytesPrefetcher` for a font file's bytes or
/// glyph coverage: a hot per-tile call site (`App::preview_family`/
/// `App::glyph_set`) needs to tell "still loading, ask again in a later
/// frame" apart from "loading genuinely failed" — the former must NOT get
/// written into `preview_cache`/`glyph_sets`, or the real data arriving a
/// few frames later would never be picked up (those caches are otherwise
/// permanent).
enum Prefetch<T> {
    Ready(T),
    Pending,
    Failed,
}

/// Reading and parsing a font file the first time any given tile becomes
/// visible used to happen synchronously, inline in the render/hot path
/// (`App::preview_family`/`App::glyph_set`, both via `Catalog::font_bytes`)
/// — fine at a few hundred fonts, but fast-scrolling through a 150k-font
/// library means hundreds of *newly*-visible tiles per second, each one a
/// blocking disk read plus a full cmap-table walk (`font_info::codepoint_
/// set`, for the missing-glyph dimmed-fallback check) on the same thread
/// that's supposed to be producing frames. That's exactly what shows up as
/// the OS's spinning "app not responding" cursor during a fast scroll, not
/// just a dropped frame or two.
///
/// This moves both the disk read and the codepoint-set parse — the two
/// actually-slow parts, and both safe to do off the main thread since they
/// only ever produce plain owned data — onto a background thread per cold
/// id, deduplicated so `preview_family` and `glyph_set` asking for the same
/// id in the same frame only reads the file once. What's left on the main
/// thread per tile is just a couple of mutex-guarded hashmap lookups, plus
/// (`preview_family` only, once bytes are ready) registering already-in-
/// memory bytes with fontique — real font-table parsing, but no disk I/O
/// and a single font, not a whole visible screenful.
///
/// The hot path becomes: check an in-memory map, and if it's not there
/// yet, kick off a background load (if one isn't already in flight) and
/// render this frame's tile with a fallback/dimmed look — the real content
/// pops in on whichever frame comes after the load finishes, woken via
/// `AppEvent::Redraw` in case nothing else was already scheduling one.
pub struct FontBytesPrefetcher {
    bytes_ready: Arc<Mutex<HashMap<i64, Arc<[u8]>>>>,
    glyphs_ready: Arc<Mutex<HashMap<i64, Arc<Option<HashSet<u32>>>>>>,
    failed: Arc<Mutex<HashSet<i64>>>,
    pending: Arc<Mutex<HashSet<i64>>>,
    proxy: EventLoopProxy<AppEvent>,
}

impl FontBytesPrefetcher {
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        Self {
            bytes_ready: Arc::new(Mutex::new(HashMap::new())),
            glyphs_ready: Arc::new(Mutex::new(HashMap::new())),
            failed: Arc::new(Mutex::new(HashSet::new())),
            pending: Arc::new(Mutex::new(HashSet::new())),
            proxy,
        }
    }

    /// Kicks off a background read+parse for `id`/`path` if one isn't
    /// already in flight or done — idempotent, so both `poll_bytes` and
    /// `poll_glyphs` can call it freely without double-reading the file.
    fn ensure_spawned(&self, id: i64, path: &str) {
        let mut pending = self.pending.lock().unwrap();
        if !pending.insert(id) {
            return;
        }
        drop(pending);
        let bytes_ready = Arc::clone(&self.bytes_ready);
        let glyphs_ready = Arc::clone(&self.glyphs_ready);
        let failed = Arc::clone(&self.failed);
        let pending = Arc::clone(&self.pending);
        let proxy = self.proxy.clone();
        let path = path.to_string();
        std::thread::spawn(move || {
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let glyphs = crate::font_info::codepoint_set(&bytes);
                    glyphs_ready.lock().unwrap().insert(id, Arc::new(glyphs));
                    bytes_ready.lock().unwrap().insert(id, Arc::from(bytes));
                }
                Err(_) => {
                    failed.lock().unwrap().insert(id);
                }
            }
            pending.lock().unwrap().remove(&id);
            let _ = proxy.send_event(AppEvent::Redraw);
        });
    }

    fn poll_bytes(&self, id: i64, path: &str) -> Prefetch<Arc<[u8]>> {
        if let Some(bytes) = self.bytes_ready.lock().unwrap().get(&id) {
            return Prefetch::Ready(Arc::clone(bytes));
        }
        if self.failed.lock().unwrap().contains(&id) {
            return Prefetch::Failed;
        }
        self.ensure_spawned(id, path);
        Prefetch::Pending
    }

    fn poll_glyphs(&self, id: i64, path: &str) -> Prefetch<Arc<Option<HashSet<u32>>>> {
        if let Some(glyphs) = self.glyphs_ready.lock().unwrap().get(&id) {
            return Prefetch::Ready(Arc::clone(glyphs));
        }
        if self.failed.lock().unwrap().contains(&id) {
            return Prefetch::Failed;
        }
        self.ensure_spawned(id, path);
        Prefetch::Pending
    }
}

pub struct App {
    pub catalog: Catalog,
    pub folders: Vec<FolderEntry>,
    pub entries: Vec<FontEntry>,
    /// Font id -> its index in `entries` — every lookup-by-id in the UI
    /// (there are a lot: which tile/row is this, is it a system font, its
    /// family/subfamily/path…) goes through this instead of scanning
    /// `entries` linearly. At a few hundred fonts a linear scan per lookup
    /// is free; at 150k+ (a real library size — see the perf pass this was
    /// added for) doing that multiple times per visible tile, every frame,
    /// is the difference between smooth scrolling and a frozen UI. Rebuilt
    /// alongside `entries` itself, never touched separately.
    pub entries_by_id: HashMap<i64, usize>,
    /// Bumped every time `entries` is reloaded — the dirty-check for
    /// `list_groups_cache`, below.
    pub entries_version: u64,
    /// Bumped every time `favorite_ids` changes — a family's sort-by-
    /// favorite key and its any-favorite flag both depend on it, so a
    /// stale cache would show the old favorite state.
    pub favorites_version: u64,
    pub active_ids: HashSet<i64>,
    pub favorite_ids: HashSet<i64>,
    pub preview_cache: PreviewCache,
    /// Backs both `preview_cache` and `glyph_sets`' first-load path — see
    /// `FontBytesPrefetcher`.
    font_prefetch: FontBytesPrefetcher,
    /// Font-id -> the real (non-`.notdef`) codepoints that font has, cached
    /// the same way `preview_cache` is — see `App::covers_sample`, which is
    /// what actually uses this: a tile whose font can't render the current
    /// sample text falls back to a dimmed system-font rendering instead of
    /// silently substituting a font that reads as if it were supported.
    /// `Arc`-wrapped since `FontBytesPrefetcher` computes these on a
    /// background thread and hands them over already built — storing the
    /// `Arc` directly avoids cloning a potentially large `HashSet<u32>`
    /// (a CJK-heavy font can have thousands of codepoints) just to move it
    /// from the prefetcher's cache into this one.
    pub glyph_sets: HashMap<i64, Arc<Option<HashSet<u32>>>>,

    pub filter: Filter,
    pub view_mode: ViewMode,
    pub search: String,
    pub tile_size: f64,
    pub selected: Option<i64>,
    pub sample_text: String,
    /// The Preview tab's live-type specimen — a real `parley` text editor
    /// (cursor, selection, word/line/paragraph navigation) rather than a
    /// plain `String`, so it can back real keyboard/mouse text editing
    /// instead of just append/backspace-at-the-end. See `ui::detail`'s
    /// `draw_preview_tab` for how it's driven and rendered, and
    /// `main.rs`'s `handle_key`/mouse handlers for input.
    pub preview_editor: PlainEditor<Brush>,
    /// The live-type area's own rect, cached at the end of every
    /// `draw_preview_tab` call — same pattern as `tile_slider_track`/
    /// `scroll_track` — so `main.rs`'s mouse handlers can convert a click
    /// or drag point into the editor's local coordinate space without
    /// recomputing the panel's layout themselves.
    pub preview_content_rect: Option<Rect>,
    /// Set while dragging out a selection in the Preview tab — same
    /// pattern as `dragging_tile_size`.
    pub dragging_preview_selection: bool,
    /// A preset picked from the Preview tab's dropdown, applied at the top
    /// of the next `draw_preview_tab` call rather than immediately —
    /// `ui::handle_click` (where the dropdown click is handled) doesn't
    /// have a `TextCx` to drive `preview_editor` with, but `draw_preview_tab`
    /// does.
    pub pending_preview_preset: Option<&'static str>,
    /// The Preview tab's text-alignment setting — see `TextAlign`.
    pub preview_align: TextAlign,
    /// The Preview tab's specimen font size, in points.
    pub preview_size: f64,
    /// `preview_size`'s free-text editing buffer while `Focus::PreviewSize`
    /// is active — parsed back into `preview_size` on every keystroke that
    /// yields a valid positive number, so a briefly-invalid intermediate
    /// state (e.g. an empty field mid-edit) doesn't reset the specimen.
    pub preview_size_input: String,
    pub preview_preset_open: bool,
    pub preview_size_dropdown_open: bool,
    pub scroll_y: f64,
    pub status: String,
    pub focus: Focus,
    /// Horizontal bounds of the rendered type-size track, refreshed every frame.
    pub tile_slider_track: Option<(f64, f64)>,
    pub dragging_tile_size: bool,

    /// Sidebar library rows' `(folder_id, rect)`, refreshed every frame —
    /// right-click hit-tests against this directly rather than through
    /// `hit_regions`, since that list only ever answers "what's under a
    /// left click".
    pub folder_rows: Vec<(i64, Rect)>,
    pub context_menu: Option<ContextMenu>,

    /// Current pointer position, logical px — refreshed on every
    /// `CursorMoved` so hover highlights can be computed at draw time
    /// alongside the hit regions, rather than tracked as separate state.
    pub hover: Point,

    /// Whether the right-side font-info panel should show for `selected`
    /// — a separate flag from `selected` itself, so picking a font (e.g.
    /// to Cmd+R activate it) never yanks the layout open on its own; only
    /// the info button does.
    pub detail_open: bool,
    /// The detail panel's current width — user-resizable by dragging its
    /// left edge (see `HitAction::StartDetailResize` /
    /// `ui::update_detail_resize_from_point`), starting at
    /// `theme::DETAIL_W` (also its minimum) and capped so grid view never
    /// gets squeezed below room for one full-size column — see
    /// `App::max_detail_w`.
    pub detail_w: f64,
    /// Set while dragging the detail panel's own resize handle — same
    /// pattern as `dragging_tile_size`/`dragging_scrollbar`.
    pub dragging_detail_resize: bool,
    /// Which of the detail panel's tabs is showing — see `DetailTab`.
    pub detail_tab: DetailTab,
    /// `(font id, extracted info)` for whichever font `detail_tab` last
    /// needed it for — recomputed only when `selected` changes to a
    /// different id than this, not every frame, since it re-parses the
    /// font file and (for `Glyphs`) walks its whole `cmap`.
    pub font_info: Option<(i64, crate::font_info::FontInfo)>,
    /// Scroll offset for whichever detail-panel tab is showing (Sizes,
    /// Glyphs, Info — Preview doesn't scroll) — one shared value, reset to
    /// 0 on every tab switch (see `HitAction::SetDetailTab`) rather than
    /// tracked per tab, since they show unrelated content anyway.
    pub detail_tab_scroll_y: f64,
    /// The valid upper bound for `detail_tab_scroll_y`, written by
    /// whichever tab's draw function last ran (`ui::detail`'s
    /// `draw_sizes_tab`/`draw_glyphs_tab`/`draw_info_tab`, each with their
    /// own idea of total content height). `main.rs`'s `MouseWheel` handler
    /// clamps against this directly rather than just adding the wheel
    /// delta unclamped — a fast scroll gesture fires many wheel events
    /// before the next redraw, and without this, `detail_tab_scroll_y`
    /// could shoot hundreds of pixels past the real max in between
    /// redraws; the *next* redraw's own clamp would then yank it back,
    /// which is what showed up as "scrolls, then snaps back".
    pub detail_tab_scroll_max: f64,
    /// The Glyphs tab's Unicode-block filter dropdown — `None` shows every
    /// block ("All"). `bool` is whether the dropdown itself is open.
    pub glyph_block_filter: Option<&'static str>,
    pub glyph_block_dropdown_open: bool,
    pub glyph_search: String,

    /// Scrollbar track bounds for the content area, `(track_y0, track_y1,
    /// content_h, viewport_h)` — refreshed every frame so a drag can map
    /// a pointer position back to a `scroll_y`.
    pub scroll_track: Option<(f64, f64, f64, f64)>,
    pub dragging_scrollbar: bool,

    /// `[close, minimize, zoom]` hit rects for the custom-drawn traffic
    /// lights (macOS only — see `ui::sidebar` and `main.rs`'s
    /// `handle_traffic_light_click`), refreshed every frame. Zero rects
    /// (matching nothing) until the sidebar actually draws them.
    pub traffic_light_rects: [Rect; 3],

    /// Whether the sample-text picker card (opened from the toolbar's
    /// preview button) is showing.
    pub sample_picker_open: bool,

    /// A file just dropped onto the window, waiting on "which library?" —
    /// set by `WindowEvent::DroppedFile` in `main.rs`, resolved by
    /// `HitAction::AssignDrop`/`CancelDrop` once the user picks (or
    /// dismisses) in `ui::draw_drop_picker`.
    /// True while a file is being dragged over the window but not yet
    /// dropped — `WindowEvent::HoveredFile`/`HoveredFileCancelled` in
    /// `main.rs`. Drives the dashed-border/file-card overlay in
    /// `ui::draw_drag_hover`, separate from `drop_flash_until`'s
    /// post-drop flash.
    pub drag_hovering: bool,
    /// When the current drag-hover started — drives the cards' fan-out
    /// animation in `ui::draw_drag_hover` (`None` once it's cancelled).
    pub drag_hover_since: Option<Instant>,

    /// Font ids activated via "Activate Temporarily" (macOS Font menu /
    /// hand-rolled Windows menu) rather than the normal toggle — tracked
    /// here, in memory only, so quitting can uninstall every one of them
    /// before the process actually ends. See `catalog::activate_temporarily`.
    pub temp_active_ids: HashSet<i64>,
    /// A pending "Delete from Library" click, waiting on the confirmation
    /// overlay (`ui::draw_delete_confirm`) before `catalog::delete_font_file`
    /// actually runs — deleting the real file is unrecoverable and, for a
    /// team's externally-synced library, affects everyone else's copy too.
    pub confirm_delete: Option<i64>,
    /// Persisted app preferences — see `ui::settings::draw` for the panel
    /// that edits these, `ui::settings::apply_toggle` for the one place a
    /// checkbox row's click actually flips one, and `settings::Settings`
    /// itself for which of these are real vs.
    /// UI-present-but-not-yet-wired-to-the-OS.
    pub settings: Settings,
    pub settings_open: bool,
    /// The Appearance dropdown's open/closed state — same pattern as
    /// `glyph_block_dropdown_open`.
    pub appearance_dropdown_open: bool,
    /// The OS's current light/dark preference, last reported via
    /// `Window::theme()` (at startup) or `WindowEvent::ThemeChanged`
    /// (`main.rs`) — what `Appearance::SystemDefault` actually resolves
    /// to. Irrelevant once the user picks `Light`/`Dark` explicitly, but
    /// kept up to date regardless so switching back to System Default
    /// doesn't need a fresh theme-change event to catch up.
    pub system_is_dark: bool,
    pub pending_drop: Option<PathBuf>,
    /// A brief self-expiring flash shown the instant a file is dropped,
    /// before the library picker appears — the "animation to signify
    /// they're adding a font" the drop itself deserved, distinct from the
    /// picker that follows it.
    pub drop_flash_until: Option<Instant>,

    /// List view's columns, in display order — see `ListColumnSpec`.
    pub list_columns: Vec<ListColumnSpec>,
    pub list_sort: ListSort,
    /// Families currently expanded to show their individual styles as
    /// sub-rows in list view — collapsed (absent here) by default.
    pub expanded_families: HashSet<String>,
    /// `ui::list::draw_list`'s grouped-by-family, sorted row list, built
    /// once and reused across frames instead of regrouping+resorting all
    /// of `entries` on every single redraw (including every tick of an
    /// active scroll) — see `ListGroupsCacheKey`. `None` until the first
    /// time list view actually draws.
    pub list_groups_cache: Option<(ListGroupsCacheKey, Vec<FamilyGroup>)>,
    /// `ui::grid::draw_grid`'s filtered id list, built once and reused
    /// across frames the same way `list_groups_cache` is — see
    /// `VisibleIdsCacheKey`. `None` until grid view first draws.
    pub visible_ids_cache: Option<(VisibleIdsCacheKey, Vec<i64>)>,
    /// `ui::sidebar`'s "Starred" nav badge count — `(entries_version,
    /// favorites_version, count)`, recomputed only when either version
    /// changes rather than rescanning all of `entries` every frame the
    /// sidebar draws (i.e. always, since it's on-screen continuously).
    pub starred_count_cache: Option<(u64, u64, usize)>,
    /// `ui::sidebar`'s per-library font counts — `(entries_version, counts)`,
    /// a single pass over `entries` rather than one filter-and-count pass
    /// per folder row every frame (same reasoning as `starred_count_cache`).
    pub folder_count_cache: Option<(u64, std::collections::HashMap<i64, usize>)>,

    /// A mouse-down on a column header, not yet resolved as a click (sort)
    /// or a drag (reorder) — `(column index, press point)`. Promoted to
    /// `dragging_column_reorder` once the pointer moves past a small
    /// threshold; if released before that, it's a sort click instead. This
    /// is the same click-vs-drag ambiguity a native table header widget
    /// resolves for free, done by hand here since nothing in this
    /// immediate-mode UI does it already.
    pub header_press: Option<(usize, Point)>,
    pub dragging_column_reorder: Option<usize>,
    /// Where the dragged column would land if dropped right now, updated
    /// continuously during the drag so `ui::list` can draw an insertion
    /// indicator — the actual move only happens on release.
    pub column_reorder_target: Option<usize>,
    /// `(column index, pointer x at drag start, that column's width at
    /// drag start)` for a right-edge resize handle drag.
    pub column_resize_start: Option<(usize, f64, f64)>,
    /// Each column header's rendered rect, in `list_columns` order —
    /// refreshed every frame list view draws its header, and consulted by
    /// `ui::update_column_reorder_from_point` to figure out which column
    /// slot the pointer is currently over during a reorder drag.
    pub list_header_rects: Vec<Rect>,

    /// The hand-rolled Windows menu bar's currently-open dropdown, if any
    /// — see `WinMenuKind`.
    pub open_win_menu: Option<WinMenuKind>,
    /// Each top-level label's rendered rect, refreshed every frame the
    /// menu bar draws — `ui::dismiss_win_menu_if_outside` uses these (and
    /// `win_menu_panel_rect`, below) to tell a click that should close the
    /// open dropdown apart from one that's opening/using it.
    pub win_menu_bar_rects: Vec<(WinMenuKind, Rect)>,
    /// The currently-open dropdown panel's own rect, refreshed each frame
    /// it's open; a zeroed rect (matching nothing) otherwise.
    pub win_menu_panel_rect: Rect,

    pub hit_regions: Vec<HitRegion>,

    /// Every tag that exists, for the sidebar's TAGS section and the font
    /// context menu's Tags submenu — loaded at startup and refreshed by
    /// `reload_tags` after any tag create/delete/assign.
    pub tags: Vec<crate::store::Tag>,
    /// `tag_id -> font ids currently carrying it` — one pass over
    /// `font_tags` (see `Store::font_tags`) rather than a query per tag
    /// per sidebar/menu draw, same reasoning as `favorite_ids`.
    pub tag_font_ids: HashMap<i64, HashSet<i64>>,
    /// A font tile/row's right-click menu — `None` when closed. Separate
    /// from `context_menu` (which is folder-row-only) since this one also
    /// owns the Tags submenu's own open/closed state and its "New Tag…"
    /// inline text field.
    pub font_context_menu: Option<crate::ui::FontContextMenu>,
}

impl App {
    pub fn new(mut catalog: Catalog, redraw_proxy: EventLoopProxy<AppEvent>) -> Self {
        // Belt-and-suspenders: restores anything `active_memory` (see
        // `catalog.rs`) says should be active but isn't right now, on
        // every launch — not just after an explicit add/rescan.
        catalog.reconcile_activation();

        let folders = load_folders(&catalog);
        let entries = load_entries(&catalog, "");
        let entries_by_id = index_entries(&entries);
        let mut active_ids = catalog.active_ids().unwrap_or_default();
        active_ids.extend(
            entries
                .iter()
                .filter(|entry| entry.is_system)
                .map(|entry| entry.id),
        );
        let favorite_ids = catalog.favorite_ids().unwrap_or_default();
        let tags = catalog.list_tags().unwrap_or_default();
        let tag_font_ids = catalog.font_tags().unwrap_or_default();

        Self {
            catalog,
            folders,
            entries,
            entries_by_id,
            entries_version: 0,
            favorites_version: 0,
            active_ids,
            favorite_ids,
            tags,
            tag_font_ids,
            font_context_menu: None,
            preview_cache: HashMap::new(),
            font_prefetch: FontBytesPrefetcher::new(redraw_proxy),
            glyph_sets: HashMap::new(),
            filter: Filter::All,
            view_mode: ViewMode::Grid,
            search: String::new(),
            tile_size: crate::theme::DEFAULT_TILE,
            selected: None,
            sample_text: "Aa".to_string(),
            preview_editor: {
                let mut editor = PlainEditor::new(72.0);
                editor.set_text("ABCDEFGHIJKLMNOPQRSTUVWXYZ 0123456789");
                editor
            },
            preview_content_rect: None,
            dragging_preview_selection: false,
            pending_preview_preset: None,
            preview_align: TextAlign::Center,
            preview_size: 72.0,
            preview_size_input: "72".to_string(),
            preview_preset_open: false,
            preview_size_dropdown_open: false,
            scroll_y: 0.0,
            status: String::new(),
            focus: Focus::None,
            tile_slider_track: None,
            dragging_tile_size: false,
            folder_rows: Vec::new(),
            context_menu: None,
            hover: Point::new(-1.0, -1.0),
            detail_open: false,
            detail_w: crate::theme::DETAIL_W,
            dragging_detail_resize: false,
            detail_tab: DetailTab::Preview,
            font_info: None,
            detail_tab_scroll_y: 0.0,
            detail_tab_scroll_max: 0.0,
            glyph_block_filter: None,
            glyph_block_dropdown_open: false,
            glyph_search: String::new(),
            scroll_track: None,
            dragging_scrollbar: false,
            traffic_light_rects: [Rect::ZERO; 3],
            sample_picker_open: false,
            drag_hovering: false,
            drag_hover_since: None,
            temp_active_ids: HashSet::new(),
            confirm_delete: None,
            settings: Settings::load(),
            settings_open: false,
            appearance_dropdown_open: false,
            // Overwritten as soon as `resumed()` has a real `Window` to
            // ask via `Window::theme()` — this initial guess only matters
            // for the handful of frames before that first query lands,
            // and matches `theme::MODE`'s own default (dark).
            system_is_dark: true,
            pending_drop: None,
            drop_flash_until: None,
            list_columns: default_list_columns(),
            list_sort: ListSort {
                column: ListColumnKind::DateAdded,
                ascending: false,
            },
            expanded_families: HashSet::new(),
            list_groups_cache: None,
            visible_ids_cache: None,
            starred_count_cache: None,
            folder_count_cache: None,
            header_press: None,
            dragging_column_reorder: None,
            column_reorder_target: None,
            column_resize_start: None,
            list_header_rects: Vec::new(),
            open_win_menu: None,
            win_menu_bar_rects: Vec::new(),
            win_menu_panel_rect: Rect::ZERO,
            hit_regions: Vec::new(),
        }
    }

    /// Entries under the active sidebar filter, layered on top of the
    /// search results already fetched from the catalog.
    pub fn visible_entries(&self) -> Vec<&FontEntry> {
        match self.filter {
            Filter::All => self.entries.iter().collect(),
            Filter::System => self.entries.iter().filter(|e| e.is_system).collect(),
            Filter::Starred => self
                .entries
                .iter()
                .filter(|e| self.favorite_ids.contains(&e.id))
                .collect(),
            Filter::Recent => {
                let cutoff = now_unix() - RECENT_WINDOW_SECS;
                self.entries.iter().filter(|e| e.mtime >= cutoff).collect()
            }
            Filter::Folder(id) => self.entries.iter().filter(|e| e.folder_id == id).collect(),
            Filter::Tag(id) => {
                let empty = HashSet::new();
                let font_ids = self.tag_font_ids.get(&id).unwrap_or(&empty);
                self.entries.iter().filter(|e| font_ids.contains(&e.id)).collect()
            }
        }
    }

    /// Count of `entries` in `favorite_ids`, cached across frames — see
    /// `starred_count_cache`.
    pub fn starred_count(&mut self) -> usize {
        if let Some((ev, fv, count)) = self.starred_count_cache {
            if ev == self.entries_version && fv == self.favorites_version {
                return count;
            }
        }
        let count = self
            .entries
            .iter()
            .filter(|e| self.favorite_ids.contains(&e.id))
            .count();
        self.starred_count_cache = Some((self.entries_version, self.favorites_version, count));
        count
    }

    /// Font count for one library folder, from the shared per-folder
    /// count map (see `folder_count_cache`), rebuilding it first if
    /// `entries` has changed since the last call.
    pub fn folder_count(&mut self, folder_id: i64) -> usize {
        if self.folder_count_cache.as_ref().is_none_or(|(ev, _)| *ev != self.entries_version) {
            let mut counts = std::collections::HashMap::new();
            for e in &self.entries {
                *counts.entry(e.folder_id).or_insert(0usize) += 1;
            }
            self.folder_count_cache = Some((self.entries_version, counts));
        }
        self.folder_count_cache.as_ref().and_then(|(_, counts)| counts.get(&folder_id)).copied().unwrap_or(0)
    }

    /// O(1) id -> entry, via `entries_by_id` — the one lookup every other
    /// by-id accessor in the UI should go through rather than scanning
    /// `entries` itself.
    pub fn entry(&self, id: i64) -> Option<&FontEntry> {
        self.entries_by_id.get(&id).map(|&i| &self.entries[i])
    }

    pub fn reload_fonts(&mut self) {
        self.entries = load_entries(&self.catalog, &self.search);
        self.entries_by_id = index_entries(&self.entries);
        self.entries_version += 1;
        self.scroll_y = 0.0;
    }

    pub fn reload_folders_and_fonts(&mut self, status: String) {
        self.status = status;
        self.folders = load_folders(&self.catalog);
        self.reload_fonts();
        self.active_ids = self.catalog.active_ids().unwrap_or_default();
        self.active_ids.extend(
            self.entries
                .iter()
                .filter(|entry| entry.is_system)
                .map(|entry| entry.id),
        );
        self.favorite_ids = self.catalog.favorite_ids().unwrap_or_default();
        self.favorites_version += 1;
    }

    /// Re-reads `tags`/`tag_font_ids` from the catalog — called after any
    /// tag create or a font's tag assignment changes, same pattern as
    /// `favorite_ids` above.
    pub fn reload_tags(&mut self) {
        self.tags = self.catalog.list_tags().unwrap_or_default();
        self.tag_font_ids = self.catalog.font_tags().unwrap_or_default();
    }

    /// Pushes `settings.appearance` (resolved against `system_is_dark` for
    /// `SystemDefault`) into `theme::set_mode` — the one place that
    /// actually happens, called after startup settings load, every
    /// Appearance dropdown selection, and every `WindowEvent::ThemeChanged`
    /// while `SystemDefault` is active.
    pub fn apply_theme(&self) {
        crate::theme::set_mode(self.settings.appearance.resolve(self.system_is_dark));
    }

    /// The widest the detail panel is allowed to be dragged, for a window
    /// `window_width` wide: whatever's left over after the sidebar and one
    /// full-size grid column (at the current zoom level) still fit —
    /// dragging past this would either drop grid view to zero columns or
    /// force its one remaining column narrower than the zoom control says
    /// it should be.
    pub fn max_detail_w(&self, window_width: f64) -> f64 {
        let min_content_w = self.tile_size + crate::theme::GRID_PAD * 2.0;
        (window_width - crate::theme::SIDEBAR_W - crate::theme::FRAME_PAD - min_content_w)
            .max(crate::theme::DETAIL_W)
    }

    /// What every grid/list tile actually renders as its live preview —
    /// `sample_text` itself, unless it's empty (or whitespace-only), in
    /// which case this falls back to "Aa" rather than rendering nothing.
    /// The single source of truth for that fallback, so the picker's own
    /// placeholder and the tiles it controls can never disagree about it.
    pub fn effective_sample_text(&self) -> &str {
        if self.sample_text.trim().is_empty() {
            "Aa"
        } else {
            &self.sample_text
        }
    }

    /// Family name to draw a font's preview glyphs in, loading and
    /// registering the file the first time this id is seen. Returns `None`
    /// while a fresh id's load is still pending (the background read
    /// hasn't landed yet — see `FontBytesPrefetcher`) or it failed.
    pub fn preview_family(&mut self, text: &mut TextCx, id: i64) -> Option<String> {
        if let Some(cached) = self.preview_cache.get(&id) {
            return cached.clone();
        }
        let Some(path) = self.entry(id).map(|e| e.path.clone()) else {
            return None;
        };
        match self.font_prefetch.poll_bytes(id, &path) {
            Prefetch::Ready(bytes) => {
                let family = text.register_font_bytes(bytes.to_vec());
                self.preview_cache.insert(id, family.clone());
                family
            }
            // Not cached either way: the real answer is still in flight,
            // ask again next frame instead of locking in "no family" now.
            Prefetch::Pending => None,
            Prefetch::Failed => {
                self.preview_cache.insert(id, None);
                None
            }
        }
    }

    /// `id`'s real glyph coverage, loaded and cached the first time it's
    /// asked for. The codepoint-set parse itself already happened on
    /// `FontBytesPrefetcher`'s background thread — this just picks up the
    /// already-built result, no main-thread cmap parsing.
    fn glyph_set(&mut self, id: i64) -> Option<&HashSet<u32>> {
        if !self.glyph_sets.contains_key(&id) {
            let Some(path) = self.entry(id).map(|e| e.path.clone()) else {
                self.glyph_sets.insert(id, Arc::new(None));
                // `Arc<Option<HashSet<u32>>>` -> `Option<&HashSet<u32>>`:
                // the first `as_ref()` unwraps the `Arc` (`&Option<..>`),
                // the second unwraps the `Option` (`Option<&..>`).
                return self.glyph_sets.get(&id).and_then(|s| s.as_ref().as_ref());
            };
            match self.font_prefetch.poll_glyphs(id, &path) {
                Prefetch::Ready(set) => {
                    self.glyph_sets.insert(id, set);
                }
                // Leave uncached — retry on a later frame once the
                // background load lands, rather than freezing in "no
                // known coverage" forever.
                Prefetch::Pending => return None,
                Prefetch::Failed => {
                    self.glyph_sets.insert(id, Arc::new(None));
                }
            }
        }
        self.glyph_sets.get(&id).and_then(|s| s.as_ref().as_ref())
    }

    /// Whether `id` actually has real glyphs for every character in the
    /// current sample text — a tile whose font fails this falls back to a
    /// dimmed system-font rendering rather than one that reads as
    /// supported when it isn't (see `ui::grid`/`ui::list`). Fails open
    /// (`true`) when coverage can't be determined at all, e.g. the file
    /// couldn't be parsed — that's a different, rarer failure than "this
    /// font doesn't have these glyphs" and shouldn't look the same.
    pub fn covers_sample(&mut self, id: i64) -> bool {
        let sample = self.effective_sample_text().to_string();
        match self.glyph_set(id) {
            Some(set) => sample.chars().all(|c| set.contains(&(c as u32))),
            None => true,
        }
    }

    /// The Sizes/Glyphs/Info tabs' shared data — extracted from `id`'s own
    /// font file, cached until `id` changes. Returns `None` if the file
    /// can't be read or parsed (rare: the file moved/was deleted out from
    /// under an already-catalogued row).
    pub fn font_info(&mut self, id: i64) -> Option<&crate::font_info::FontInfo> {
        if self.font_info.as_ref().map(|(cached_id, _)| *cached_id) != Some(id) {
            let path = self.entry(id).map(|e| e.path.clone());
            let info = path.and_then(|path| {
                let bytes = self.catalog.font_bytes(id).ok()?;
                crate::font_info::extract(&bytes, &path)
            });
            self.font_info = info.map(|info| (id, info));
        }
        self.font_info.as_ref().filter(|(cached_id, _)| *cached_id == id).map(|(_, info)| info)
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn load_folders(catalog: &Catalog) -> Vec<FolderEntry> {
    catalog
        .list_folders()
        .unwrap_or_default()
        .into_iter()
        .map(|f| FolderEntry {
            id: f.id,
            path: f.path,
        })
        .collect()
}

fn load_entries(catalog: &Catalog, query: &str) -> Vec<FontEntry> {
    catalog
        .list_fonts(query)
        .unwrap_or_default()
        .into_iter()
        .map(FontEntry::from)
        .collect()
}

fn index_entries(entries: &[FontEntry]) -> HashMap<i64, usize> {
    entries.iter().enumerate().map(|(i, e)| (e.id, i)).collect()
}
