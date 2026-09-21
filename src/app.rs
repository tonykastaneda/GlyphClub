use std::collections::{HashMap, HashSet};

use vello::kurbo::{Point, Rect};

use crate::catalog::Catalog;
use crate::store::FontRow;
use crate::text::TextCx;
use crate::ui::{ContextMenu, HitRegion};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Starred,
    Folder(i64),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    None,
    Search,
    PreviewText,
    /// The sample-text picker's editable field (`app.sample_text`) — the
    /// short string every grid/list tile renders its live preview in, not
    /// `preview_text` (the detail panel's own, separate specimen string).
    SampleText,
}

pub struct FolderEntry {
    pub id: i64,
    pub path: String,
}

pub struct FontEntry {
    pub id: i64,
    pub folder_id: i64,
    pub family: String,
    pub subfamily: String,
    pub weight: u16,
    pub italic: bool,
    pub monospace: bool,
    pub is_system: bool,
}

impl From<FontRow> for FontEntry {
    fn from(row: FontRow) -> Self {
        let r = row.record;
        FontEntry {
            id: row.id,
            folder_id: row.folder_id,
            is_system: crate::font::is_system_path(&r.path),
            family: r.family,
            subfamily: r.subfamily,
            weight: r.weight,
            italic: r.italic,
            monospace: r.monospace,
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

pub struct App {
    pub catalog: Catalog,
    pub folders: Vec<FolderEntry>,
    pub entries: Vec<FontEntry>,
    pub active_ids: HashSet<i64>,
    pub favorite_ids: HashSet<i64>,
    pub preview_cache: PreviewCache,

    pub filter: Filter,
    pub view_mode: ViewMode,
    pub search: String,
    pub tile_size: f64,
    pub selected: Option<i64>,
    pub sample_text: String,
    pub preview_text: String,
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

    pub hit_regions: Vec<HitRegion>,
}

impl App {
    pub fn new(mut catalog: Catalog) -> Self {
        let folders = load_folders(&catalog);
        let entries = load_entries(&catalog, "");
        let mut active_ids = catalog.active_ids().unwrap_or_default();
        active_ids.extend(
            entries
                .iter()
                .filter(|entry| entry.is_system)
                .map(|entry| entry.id),
        );
        let favorite_ids = catalog.favorite_ids().unwrap_or_default();
        let _ = &mut catalog;

        Self {
            catalog,
            folders,
            entries,
            active_ids,
            favorite_ids,
            preview_cache: HashMap::new(),
            filter: Filter::All,
            view_mode: ViewMode::Grid,
            search: String::new(),
            tile_size: crate::theme::DEFAULT_TILE,
            selected: None,
            sample_text: "Aa".to_string(),
            preview_text: "The quick brown fox jumps over the lazy dog 0123456789".to_string(),
            scroll_y: 0.0,
            status: String::new(),
            focus: Focus::None,
            tile_slider_track: None,
            dragging_tile_size: false,
            folder_rows: Vec::new(),
            context_menu: None,
            hover: Point::new(-1.0, -1.0),
            detail_open: false,
            scroll_track: None,
            dragging_scrollbar: false,
            traffic_light_rects: [Rect::ZERO; 3],
            sample_picker_open: false,
            hit_regions: Vec::new(),
        }
    }

    /// Entries under the active sidebar filter, layered on top of the
    /// search results already fetched from the catalog.
    pub fn visible_entries(&self) -> Vec<&FontEntry> {
        match self.filter {
            Filter::All => self.entries.iter().collect(),
            Filter::Starred => self
                .entries
                .iter()
                .filter(|e| self.favorite_ids.contains(&e.id))
                .collect(),
            Filter::Folder(id) => self.entries.iter().filter(|e| e.folder_id == id).collect(),
        }
    }

    pub fn reload_fonts(&mut self) {
        self.entries = load_entries(&self.catalog, &self.search);
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
    /// while a fresh id's load is still pending or it failed.
    pub fn preview_family(&mut self, text: &mut TextCx, id: i64) -> Option<String> {
        if let Some(cached) = self.preview_cache.get(&id) {
            return cached.clone();
        }
        let family = self
            .catalog
            .font_bytes(id)
            .ok()
            .and_then(|bytes| text.register_font_bytes(bytes));
        self.preview_cache.insert(id, family.clone());
        family
    }
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
