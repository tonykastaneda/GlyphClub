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

/// What the grid and list are sorted by — View ▸ Sort By picks the first
/// five; the list's column headers can also pick the rest.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum SortField {
    Enabled,
    Name,
    Tags,
    DateAdded,
    Starred,
    Styles,
    Format,
    Category,
}

impl SortField {
    /// View ▸ Sort By, in menu order.
    pub const MENU: [(SortField, &'static str); 5] = [
        (SortField::Enabled, "Enabled"),
        (SortField::Name, "Name"),
        (SortField::Tags, "Tags"),
        (SortField::DateAdded, "Date Added"),
        (SortField::Starred, "Starred"),
    ];

    /// The direction a fresh pick of this field starts in: the one people
    /// mean by "sort by" it — active, starred, newest and most-styles
    /// first; A–Z otherwise (for Tags, tagged fonts first, by tag name).
    pub fn default_ascending(self) -> bool {
        !matches!(
            self,
            SortField::Enabled | SortField::DateAdded | SortField::Starred | SortField::Styles
        )
    }
}

impl ListColumnKind {
    /// The field clicking this column's header sorts by.
    pub fn sort_field(self) -> SortField {
        match self {
            ListColumnKind::Name | ListColumnKind::Preview => SortField::Name,
            ListColumnKind::Styles => SortField::Styles,
            ListColumnKind::Favorite => SortField::Starred,
            ListColumnKind::Format => SortField::Format,
            ListColumnKind::Category => SortField::Category,
            ListColumnKind::DateAdded => SortField::DateAdded,
        }
    }
}

/// The one sort both views share (persisted in `Settings`).
#[derive(Clone, Copy, PartialEq)]
pub struct ListSort {
    pub column: SortField,
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
    /// Any member active — the Enabled sort's key.
    pub any_active: bool,
    /// The alphabetically first tag name among members — the Tags sort's.
    pub first_tag: Option<String>,
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
    /// See `App::sort_signature`.
    pub sort_signature: u64,
    /// See `App::coverage_signature`.
    pub coverage_signature: u64,
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
    pub sort: ListSort,
    pub sort_signature: u64,
    pub coverage_signature: u64,
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

/// The Settings overlay's tabs — see `ui::settings::draw`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Integrations,
}

/// What Settings ▸ Integrations shows — see `crate::integrations`.
/// Re-read from disk whenever the tab is opened or an install finishes,
/// rather than every frame (it touches /Applications and runs `pgrep`).
#[derive(Default)]
pub struct IntegrationsState {
    pub illustrators: Vec<crate::integrations::IllustratorInstall>,
    pub bundled_version: Option<String>,
    pub illustrator_running: bool,
    /// An install/uninstall is running (the admin prompt may be up).
    pub busy: bool,
    /// The last install/uninstall's outcome — `(message, is_error)`.
    pub message: Option<(String, bool)>,
    /// The version a running plug-in reported over the bridge — catches an
    /// out-of-date copy even in an Illustrator outside /Applications.
    /// Cleared by a successful install, since Illustrator keeps the old
    /// one loaded (and reporting) until it restarts.
    pub connected_plugin_version: Option<String>,
    /// The "plug-in update" banner's close button, for this session.
    pub banner_dismissed: bool,
}

impl IntegrationsState {
    pub fn refresh(&mut self) {
        self.illustrators = crate::integrations::find_illustrators();
        self.bundled_version = crate::integrations::bundled_plugin()
            .map(|p| crate::integrations::plugin_version(&p).unwrap_or_default());
        self.illustrator_running = crate::integrations::illustrator_running();
    }

    /// The version to update to, when an installed or connected plug-in is
    /// older than the one this build ships.
    pub fn update_available(&self) -> Option<String> {
        let bundled = self.bundled_version.clone().filter(|v| !v.is_empty())?;
        let behind = |v: &str| v.is_empty() || crate::app::is_newer_version(&bundled, v);
        let installed_behind = self
            .illustrators
            .iter()
            .filter(|i| i.supported())
            .filter_map(|i| i.installed_version.as_deref())
            .any(behind);
        let connected_behind = self.connected_plugin_version.as_deref().is_some_and(behind);
        (installed_behind || connected_behind).then_some(bundled)
    }
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

/// Font-id -> its preview's registered family name, so a scrolled-past
/// tile's preview doesn't get re-read off disk and re-shaped every time it
/// scrolls back into view. A `None` family means a load was already tried
/// and failed (parse error, missing file) — don't retry every frame.
///
/// Bounded: each entry pins a whole font file inside fontique, so past
/// `PREVIEW_CACHE_CAP` registered fonts the least recently drawn are
/// unregistered (see `App::evict_previews`) and simply re-read if they
/// scroll back into view. Scrolling a whole large library used to keep
/// every file it passed in memory.
pub type PreviewCache = HashMap<i64, PreviewEntry>;

pub struct PreviewEntry {
    pub family: Option<String>,
    faces: Vec<crate::text::RegisteredFace>,
    /// `App::frame` when this preview was last drawn.
    last_used: u64,
}

/// Registered previews kept before evicting (down to `PREVIEW_CACHE_KEEP`).
/// A screenful is at most a few hundred tiles, so this never evicts
/// anything visible.
const PREVIEW_CACHE_CAP: usize = 256;
const PREVIEW_CACHE_KEEP: usize = 192;

/// A tile that's still on screen collects its bytes within a frame of them
/// arriving; anything older belongs to a tile that scrolled away.
const UNCLAIMED_BYTES_TTL: std::time::Duration = std::time::Duration::from_secs(3);

/// The result of asking `FontBytesPrefetcher` for a font file's bytes or
/// glyph coverage: a hot per-tile call site (`App::preview_family`/
/// `App::glyph_set`) needs to tell "still loading, ask again in a later
/// frame" apart from "loading genuinely failed" — the former must NOT get
/// written into `preview_cache`, or the real data arriving a
/// few frames later would never be picked up (those caches are otherwise
/// permanent).
enum Prefetch<T> {
    Ready(T),
    Pending,
    Failed,
}

/// Kicks off a one-shot, background version check against the GitHub
/// releases API. The silent startup check (`manual: false`) only ever
/// fires `AppEvent::UpdateAvailable` if a newer release actually exists —
/// nothing for the event loop to do on the common case but silently
/// receive nothing. A manual check (the "Check for Updates…" menu item)
/// passes `manual: true` instead, which additionally reports back on the
/// other two outcomes (already up to date, or the check itself failed) —
/// a user who explicitly asked should always get *some* answer, not
/// silence that reads the same as "still checking" or "forgot to click".
pub fn check_for_update(proxy: EventLoopProxy<AppEvent>, manual: bool) {
    std::thread::spawn(move || match fetch_latest_release_version() {
        Some(latest) if is_newer_version(&latest, env!("CARGO_PKG_VERSION")) => {
            let _ = proxy.send_event(AppEvent::UpdateAvailable(latest));
        }
        Some(_) if manual => {
            let _ = proxy.send_event(AppEvent::UpdateCheckUpToDate);
        }
        None if manual => {
            let _ = proxy.send_event(AppEvent::UpdateCheckFailed);
        }
        _ => {}
    });
}

fn fetch_latest_release_version() -> Option<String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .build();
    let agent: ureq::Agent = config.into();
    let body: serde_json::Value = agent
        .get("https://api.github.com/repos/tonykastaneda/GlyphClub/releases/latest")
        // GitHub's API rejects requests with no User-Agent at all.
        .header("User-Agent", "GlyphClub-UpdateCheck")
        .call()
        .ok()?
        .body_mut()
        .read_json()
        .ok()?;
    let tag = body.get("tag_name")?.as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

/// `false` on anything unparsable, not just "not newer" — an unrecognized
/// version format (a stray non-numeric tag, say) should never show an
/// update banner for something that might not even be a real newer build.
pub(crate) fn is_newer_version(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(c), Some(cur)) => c > cur,
        _ => false,
    }
}

#[cfg(test)]
mod plugin_update_tests {
    use super::IntegrationsState;
    use crate::integrations::IllustratorInstall;

    fn state(installed: Option<&str>, connected: Option<&str>) -> IntegrationsState {
        IntegrationsState {
            illustrators: vec![IllustratorInstall {
                label: "Illustrator 2026".into(),
                year: Some(2026),
                plugins_dir: "/tmp".into(),
                installed_version: installed.map(str::to_string),
            }],
            bundled_version: Some("0.2.0".into()),
            connected_plugin_version: connected.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn an_older_installed_or_connected_plugin_needs_updating() {
        assert_eq!(state(Some("0.1.1"), None).update_available().as_deref(), Some("0.2.0"));
        assert_eq!(state(None, Some("0.1.1")).update_available().as_deref(), Some("0.2.0"));
        // An install from before the plug-in had a readable version.
        assert_eq!(state(Some(""), None).update_available().as_deref(), Some("0.2.0"));
    }

    #[test]
    fn current_missing_or_newer_plugins_dont() {
        assert_eq!(state(Some("0.2.0"), Some("0.2.0")).update_available(), None);
        assert_eq!(state(None, None).update_available(), None);
        assert_eq!(state(Some("0.3.0"), None).update_available(), None);
    }
}

#[cfg(test)]
mod update_check_tests {
    use super::is_newer_version;

    #[test]
    fn newer_patch_and_minor_and_major_are_detected() {
        assert!(is_newer_version("0.1.1", "0.1.0"));
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("1.0.0", "0.9.9"));
    }

    #[test]
    fn equal_or_older_is_not_newer() {
        assert!(!is_newer_version("0.1.0", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "0.1.1"));
    }

    #[test]
    fn unparsable_versions_never_trigger_an_update() {
        assert!(!is_newer_version("not-a-version", "0.1.0"));
        assert!(!is_newer_version("0.1.0", "not-a-version"));
        assert!(!is_newer_version("v1.2", "0.1.0"));
    }
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
    /// Read but not yet picked up. `poll_bytes` *takes* them — once a
    /// preview registers a font with fontique, fontique holds the bytes, and
    /// keeping a second copy here was pure waste. Reads for tiles that
    /// scrolled away before their bytes arrived are never picked up, so
    /// `prune` drops them after `UNCLAIMED_BYTES_TTL`.
    bytes_ready: Arc<Mutex<HashMap<i64, (Vec<u8>, Instant)>>>,
    failed: Arc<Mutex<HashSet<i64>>>,
    /// Queued or being read — so a tile asking every frame queues once.
    pending: Arc<Mutex<HashSet<i64>>>,
    /// Waiting reads, newest last. A fixed pool of `READER_THREADS` works
    /// through it newest-first, and past `MAX_QUEUED_READS` the oldest are
    /// dropped: a fast scroll asks for every tile it flashes past, and
    /// reading all of them (a thread each, as this used to) spiked memory
    /// by gigabytes for fonts nobody would see.
    queue: Arc<(Mutex<Vec<(i64, String)>>, std::sync::Condvar)>,
}

const READER_THREADS: usize = 6;
const MAX_QUEUED_READS: usize = 96;

impl FontBytesPrefetcher {
    pub fn new(proxy: EventLoopProxy<AppEvent>) -> Self {
        let prefetcher = Self {
            bytes_ready: Arc::new(Mutex::new(HashMap::new())),
            failed: Arc::new(Mutex::new(HashSet::new())),
            pending: Arc::new(Mutex::new(HashSet::new())),
            queue: Arc::new((Mutex::new(Vec::new()), std::sync::Condvar::new())),
        };
        for _ in 0..READER_THREADS {
            let queue = Arc::clone(&prefetcher.queue);
            let bytes_ready = Arc::clone(&prefetcher.bytes_ready);
            let failed = Arc::clone(&prefetcher.failed);
            let pending = Arc::clone(&prefetcher.pending);
            let proxy = proxy.clone();
            std::thread::spawn(move || loop {
                let (id, path) = {
                    let (lock, ready) = &*queue;
                    let mut waiting = lock.lock().unwrap();
                    loop {
                        if let Some(job) = waiting.pop() {
                            break job;
                        }
                        waiting = ready.wait(waiting).unwrap();
                    }
                };
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        bytes_ready.lock().unwrap().insert(id, (bytes, Instant::now()));
                    }
                    Err(_) => {
                        failed.lock().unwrap().insert(id);
                    }
                }
                pending.lock().unwrap().remove(&id);
                let _ = proxy.send_event(AppEvent::Redraw);
            });
        }
        prefetcher
    }

    /// Queues a read for `id`/`path` unless one is already queued, in
    /// flight, or waiting to be picked up.
    fn ensure_spawned(&self, id: i64, path: &str) {
        if self.bytes_ready.lock().unwrap().contains_key(&id) {
            return;
        }
        if !self.pending.lock().unwrap().insert(id) {
            return;
        }
        let (lock, ready) = &*self.queue;
        let mut waiting = lock.lock().unwrap();
        waiting.push((id, path.to_string()));
        if waiting.len() > MAX_QUEUED_READS {
            let excess = waiting.len() - MAX_QUEUED_READS;
            let dropped: Vec<i64> = waiting.drain(..excess).map(|(id, _)| id).collect();
            let mut pending = self.pending.lock().unwrap();
            for id in dropped {
                pending.remove(&id);
            }
        }
        ready.notify_one();
    }

    /// Drops reads nobody picked up in time — see `bytes_ready`.
    fn prune(&self) {
        self.bytes_ready
            .lock()
            .unwrap()
            .retain(|_, (_, loaded)| loaded.elapsed() < UNCLAIMED_BYTES_TTL);
    }

    fn poll_bytes(&self, id: i64, path: &str) -> Prefetch<Vec<u8>> {
        if let Some((bytes, _)) = self.bytes_ready.lock().unwrap().remove(&id) {
            return Prefetch::Ready(bytes);
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
    /// Backs `preview_cache`'s first-load path — see `FontBytesPrefetcher`.
    font_prefetch: FontBytesPrefetcher,
    /// Counts drawn frames — the preview cache's recency clock.
    pub frame: u64,

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
    /// The sample-text picker card as last drawn (see
    /// `ui::draw_sample_picker`), so a click outside it can close it.
    pub sample_picker_rect: Rect,
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
    /// "Hide missing" (the sample-text card's chip, View ▸ Hide Fonts
    /// Missing Characters): hides fonts lacking any character of the sample
    /// text. Off at launch on purpose — see `toggle_hide_missing`.
    pub hide_missing: bool,
    pub coverage: crate::coverage::CoverageIndex,
    /// How many fonts the last grid/list rebuild hid — the chip's label.
    pub hidden_missing_count: usize,
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
    pub settings_tab: SettingsTab,
    pub integrations: IntegrationsState,
    /// The About overlay — see `ui::about::draw`.
    pub about_open: bool,
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

    /// `Some(version)` once the background check (`check_for_update`,
    /// kicked off once at startup) finds a newer release than this build
    /// — drives the dismissible corner banner. `None` both before that
    /// check finishes and when it finds nothing newer.
    pub update_available: Option<String>,
    /// Set when the banner's own close button is clicked — kept separate
    /// from `update_available` so a stray late-arriving check result
    /// can't un-dismiss a banner the user already closed this session.
    pub update_dismissed: bool,
    /// A clone of the same proxy `check_for_update` was first given at
    /// startup — kept on `App` itself so a manual "Check for Updates…"
    /// menu click (macOS's native app menu, Windows' hand-rolled File
    /// menu) can re-trigger the same background check from wherever it's
    /// wired up, without threading a whole extra proxy parameter through
    /// every call site in between.
    pub update_proxy: EventLoopProxy<AppEvent>,
}

impl App {
    pub fn new(mut catalog: Catalog, redraw_proxy: EventLoopProxy<AppEvent>) -> Self {
        // Belt-and-suspenders: restores anything `active_memory` (see
        // `catalog.rs`) says should be active but isn't right now, on
        // every launch — not just after an explicit add/rescan.
        catalog.drop_leftover_temporary_activations();
        catalog.repair_activations();
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
        // Read now (not just when Settings opens) so an out-of-date
        // Illustrator plug-in can show its update banner at launch.
        let mut integrations = IntegrationsState::default();
        integrations.refresh();
        let settings = Settings::load();
        let mut coverage = crate::coverage::CoverageIndex::default();
        if let Ok(rows) = catalog.coverage() {
            coverage.load(rows);
        }
        let tags = catalog.list_tags().unwrap_or_default();
        let tag_font_ids = catalog.font_tags().unwrap_or_default();
        let update_proxy = redraw_proxy.clone();

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
            update_available: None,
            update_dismissed: false,
            update_proxy,
            preview_cache: HashMap::new(),
            font_prefetch: FontBytesPrefetcher::new(redraw_proxy),
            frame: 0,
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
            sample_picker_rect: Rect::ZERO,
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
            hide_missing: false,
            coverage,
            hidden_missing_count: 0,
            confirm_delete: None,
            settings,
            settings_open: false,
            settings_tab: SettingsTab::General,
            integrations,
            about_open: false,
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
                column: settings.sort_field,
                ascending: settings.sort_ascending,
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
    /// A cheap, order-independent fingerprint of the state an Enabled or
    /// Tags sort depends on, so the grid/list caches re-sort when a font is
    /// activated or tagged. 0 for every other field (their inputs are
    /// already covered by the entries/favorites versions).
    pub fn sort_signature(&self) -> u64 {
        fn mix(mut x: u64) -> u64 {
            x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
            x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            x ^ (x >> 31)
        }
        match self.list_sort.column {
            SortField::Enabled => self
                .active_ids
                .iter()
                .fold(self.active_ids.len() as u64, |acc, &id| acc.wrapping_add(mix(id as u64))),
            SortField::Tags => {
                let mut acc = self.tags.len() as u64;
                for tag in &self.tags {
                    for byte in tag.name.bytes() {
                        acc = mix(acc ^ byte as u64);
                    }
                }
                for (&tag, ids) in &self.tag_font_ids {
                    for &id in ids {
                        acc = acc.wrapping_add(mix((tag as u64).rotate_left(32) ^ id as u64));
                    }
                }
                acc
            }
            _ => 0,
        }
    }

    /// Each tagged font's alphabetically first tag name (lowercased) — the
    /// Tags sort's key.
    pub fn first_tag_names(&self) -> HashMap<i64, String> {
        let mut first: HashMap<i64, String> = HashMap::new();
        for tag in &self.tags {
            let Some(ids) = self.tag_font_ids.get(&tag.id) else { continue };
            let name = tag.name.to_lowercase();
            for &id in ids {
                match first.get(&id) {
                    Some(existing) if *existing <= name => {}
                    _ => {
                        first.insert(id, name.clone());
                    }
                }
            }
        }
        first
    }

    /// The sample text's distinct characters — what "Hide missing" checks.
    pub fn sample_chars(&self) -> Vec<u32> {
        let mut chars: Vec<u32> = self.effective_sample_text().chars().map(|c| c as u32).collect();
        chars.sort_unstable();
        chars.dedup();
        chars
    }

    /// Changes whenever "Hide missing" would hide a different set of fonts:
    /// toggled, a new sample text, or more coverage results landing. 0
    /// while it's off, so the caches ignore sample-text edits then.
    pub fn coverage_signature(&self) -> u64 {
        if !self.hide_missing {
            return 0;
        }
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.effective_sample_text().hash(&mut hasher);
        self.coverage.version().hash(&mut hasher);
        hasher.finish() | 1
    }

    /// Flips "Hide missing" — instant, since every font's coverage is
    /// already in memory (see `coverage`).
    pub fn toggle_hide_missing(&mut self) {
        self.hide_missing = !self.hide_missing;
        self.scroll_y = 0.0;
    }

    /// Reloads the coverage index from the catalog — at launch and after
    /// every rescan, which is when new or changed fonts get theirs.
    pub fn reload_coverage(&mut self) {
        if let Ok(rows) = self.catalog.coverage() {
            self.coverage.load(rows);
        }
    }

    /// `visible_entries` minus what "Hide missing" hides, and how many
    /// that was.
    pub fn filtered_visible_entries(&self) -> (Vec<&FontEntry>, usize) {
        let entries = self.visible_entries();
        if !self.hide_missing {
            return (entries, 0);
        }
        let chars = self.sample_chars();
        let before = entries.len();
        let kept: Vec<&FontEntry> = entries
            .into_iter()
            .filter(|e| self.coverage.missing_any(e.id, &chars) != Some(true))
            .collect();
        let hidden = before - kept.len();
        (kept, hidden)
    }

    /// The grid's order: the filtered `visible_entries` (already by family,
    /// then weight) re-sorted by `list_sort`. Stable, so ties keep that
    /// order. Also returns how many "Hide missing" hid.
    pub fn sorted_visible_ids(&self) -> (Vec<i64>, usize) {
        let (mut entries, hidden) = self.filtered_visible_entries();
        let sort = self.list_sort;
        let first_tag = if sort.column == SortField::Tags { self.first_tag_names() } else { HashMap::new() };
        let family_key = |e: &FontEntry| e.family.to_lowercase();
        entries.sort_by(|a, b| {
            let ordering = match sort.column {
                SortField::Name | SortField::Styles | SortField::Category => {
                    if sort.column == SortField::Name {
                        family_key(a).cmp(&family_key(b))
                    } else {
                        std::cmp::Ordering::Equal
                    }
                }
                SortField::Enabled => self.active_ids.contains(&a.id).cmp(&self.active_ids.contains(&b.id)),
                SortField::Starred => self.favorite_ids.contains(&a.id).cmp(&self.favorite_ids.contains(&b.id)),
                SortField::DateAdded => a.mtime.cmp(&b.mtime),
                SortField::Format => crate::font::format_label(&a.path).cmp(crate::font::format_label(&b.path)),
                SortField::Tags => tag_order(first_tag.get(&a.id), first_tag.get(&b.id)),
            };
            if sort.ascending { ordering } else { ordering.reverse() }
        });
        (entries.into_iter().map(|e| e.id).collect(), hidden)
    }

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
        // New or changed fonts got their coverage during the rescan.
        self.reload_coverage();
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
        let frame = self.frame;
        if let Some(cached) = self.preview_cache.get_mut(&id) {
            cached.last_used = frame;
            return cached.family.clone();
        }
        let Some(path) = self.entry(id).map(|e| e.path.clone()) else {
            return None;
        };
        let (family, faces) = match self.font_prefetch.poll_bytes(id, &path) {
            Prefetch::Ready(bytes) => match text.register_font_bytes(bytes) {
                Some((family, faces)) => (Some(family), faces),
                None => (None, Vec::new()),
            },
            // Not cached either way: the real answer is still in flight,
            // ask again next frame instead of locking in "no family" now.
            Prefetch::Pending => return None,
            Prefetch::Failed => (None, Vec::new()),
        };
        self.preview_cache.insert(id, PreviewEntry { family: family.clone(), faces, last_used: frame });
        family
    }

    /// Past `PREVIEW_CACHE_CAP` registered previews, unregisters the least
    /// recently drawn down to `PREVIEW_CACHE_KEEP` — called once per frame
    /// after drawing, so nothing on screen is ever evicted.
    pub fn evict_previews(&mut self, text: &mut TextCx) {
        self.frame += 1;
        self.font_prefetch.prune();
        if self.preview_cache.len() <= PREVIEW_CACHE_CAP {
            return;
        }
        let mut by_age: Vec<(u64, i64)> = self.preview_cache.iter().map(|(&id, e)| (e.last_used, id)).collect();
        by_age.sort_unstable();
        let evict = self.preview_cache.len() - PREVIEW_CACHE_KEEP;
        let mut evicted_families = HashSet::new();
        for &(_, id) in &by_age[..evict] {
            if let Some(entry) = self.preview_cache.remove(&id) {
                text.unregister_faces(&entry.faces);
                evicted_families.extend(entry.family);
            }
        }
        // fontique knows a registered font only by family name + style, so
        // evicting one copy of a font (libraries often hold duplicates) can
        // take out the face a still-cached copy was drawing with. Drop those
        // too; they re-register from disk on their next draw.
        let orphaned: Vec<i64> = self
            .preview_cache
            .iter()
            .filter(|(_, e)| e.family.as_ref().is_some_and(|f| evicted_families.contains(f)))
            .map(|(&id, _)| id)
            .collect();
        for id in orphaned {
            if let Some(entry) = self.preview_cache.remove(&id) {
                text.unregister_faces(&entry.faces);
            }
        }
        // Cached layouts hold the evicted fonts' data too.
        text.clear_layout_cache();
    }

    /// Whether `id` actually has real glyphs for every character in the
    /// current sample text — a tile whose font fails this falls back to a
    /// dimmed system-font rendering rather than one that reads as
    /// supported when it isn't (see `ui::grid`/`ui::list`). Answered from
    /// the catalog's coverage (see `coverage`), so no file is read. Fails
    /// open (`true`) when coverage isn't known.
    pub fn covers_sample(&mut self, id: i64) -> bool {
        let chars = self.sample_chars();
        self.coverage.missing_any(id, &chars) != Some(true)
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

/// Tags sort order: tagged before untagged, then by first tag name.
pub fn tag_order(a: Option<&String>, b: Option<&String>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
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
