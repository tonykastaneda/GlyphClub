//! Thin app-facing wrapper around [`Store`] + [`scan_folder`] + [`activate`]
//! — the same operations the old Tauri `commands.rs` exposed over IPC,
//! called directly in-process now that the UI lives in the same binary.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Result};
use notify_debouncer_mini::notify::RecursiveMode;
use notify_debouncer_mini::{new_debouncer, Debouncer};

use crate::activate;
use crate::activation_memory;
use crate::scan::{force_touch_folder, scan_folder};
use crate::store::{Folder, FontRow, Store};

pub struct ScanResult {
    pub status: String,
}

/// The OS-managed font directories — auto-added as libraries (see
/// `Catalog::open`) rather than something the user has to point "Add
/// Library" at themselves, since they're what the sidebar's System Fonts
/// filter actually needs fonts to come from. `crate::font::is_system_path`
/// is the other half of this: it's what tells the sidebar's own
/// "LIBRARIES" list to hide these rows (they're not a folder the user
/// picked, so removing them the normal way wouldn't make sense) and what
/// marks a scanned font as `is_system` in the first place.
fn system_font_dirs() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        &["/System/Library/Fonts", "/Library/Fonts"]
    }
    #[cfg(target_os = "windows")]
    {
        &[r"C:\Windows\Fonts"]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        &[]
    }
}

fn catalog_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("glyphclub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("catalog.sqlite3")
}

pub struct Catalog {
    store: Store,
    /// Font file paths the user has activated at some point — see
    /// `activation_memory`. Kept in sync with every `toggle_activation`
    /// call and consulted by `reconcile_activation` after a scan, so
    /// removing then re-adding a library folder restores what was active
    /// in it instead of just losing that.
    active_memory: HashSet<String>,
    /// Paths a scan should skip entirely — see `store::ignored_paths`.
    /// Loaded once at `open()` and kept in sync by `remove_from_fontlist`,
    /// same pattern as `active_memory`.
    ignored: HashSet<String>,
    /// Watches every library folder for changes so a teammate dropping
    /// fonts straight into the (externally synced) shared folder shows up
    /// without anyone touching the app. `None` if the watcher failed to
    /// start (e.g. an unsupported platform backend) — everything still
    /// works via the manual rescan button, just without the automatic
    /// part. Kept in sync with the folder list by `add_folder`/
    /// `remove_folder`, which is why they need `&mut self`.
    watcher: Option<Debouncer<notify_debouncer_mini::notify::RecommendedWatcher>>,
}

impl Catalog {
    /// `on_change` fires (from the watcher's own background thread —
    /// callers that need to touch UI state should hop back to their main
    /// thread, e.g. via a `winit::event_loop::EventLoopProxy`) whenever a
    /// watched folder changes. Debounced to one call per ~800ms of
    /// activity, not once per individual file event.
    pub fn open(on_change: impl Fn() + Send + 'static) -> Result<Self> {
        let mut store = Store::open(&catalog_path())?;
        let active_memory = activation_memory::load();
        let ignored = store.ignored_paths().unwrap_or_default();

        // Ensures the OS's own font directories are always a library —
        // scanned every launch (not just the first) so System Fonts stays
        // current without anyone needing to hit Sync for it specifically;
        // cheap after the first run since `scan_folder` only re-parses
        // whatever actually changed.
        for &dir in system_font_dirs() {
            if !Path::new(dir).is_dir() {
                continue;
            }
            if let Ok(id) = store.add_folder(dir) {
                let folder = Folder { id, path: dir.to_string() };
                let _ = scan_folder(&mut store, &folder, &ignored);
            }
        }

        let mut watcher = new_debouncer(
            Duration::from_millis(800),
            move |res: notify_debouncer_mini::DebounceEventResult| {
                if res.is_ok() {
                    on_change();
                }
            },
        )
        .ok();

        if let Some(w) = watcher.as_mut() {
            if let Ok(folders) = store.list_folders() {
                for folder in &folders {
                    let _ = w
                        .watcher()
                        .watch(Path::new(&folder.path), RecursiveMode::Recursive);
                }
            }
        }

        Ok(Self {
            store,
            active_memory,
            ignored,
            watcher,
        })
    }

    pub fn list_folders(&self) -> Result<Vec<Folder>> {
        self.store.list_folders()
    }

    pub fn list_fonts(&self, query: &str) -> Result<Vec<FontRow>> {
        if query.trim().is_empty() {
            self.store.all_fonts()
        } else {
            self.store.search_fonts(query.trim())
        }
    }

    pub fn active_ids(&self) -> Result<HashSet<i64>> {
        self.store.active_ids()
    }

    pub fn favorite_ids(&self) -> Result<HashSet<i64>> {
        self.store.favorite_ids()
    }

    pub fn add_folder(&mut self, path: &str) -> Result<ScanResult> {
        let id = self.store.add_folder(path)?;
        if let Some(w) = self.watcher.as_mut() {
            let _ = w.watcher().watch(Path::new(path), RecursiveMode::Recursive);
        }
        let folders = self.store.list_folders()?;
        let mut status = match folders.iter().find(|f| f.id == id) {
            Some(folder) => match scan_folder(&mut self.store, folder, &self.ignored) {
                Ok(summary) => {
                    format!(
                        "{} indexed, {} failed",
                        summary.parsed_files, summary.failed_files
                    )
                }
                Err(e) => e.to_string(),
            },
            None => "Folder added".to_string(),
        };
        let restored = self.reconcile_activation();
        if restored > 0 {
            status.push_str(&format!(", {restored} reactivated from memory"));
        }
        Ok(ScanResult { status })
    }

    /// Uninstalls anything active in this folder before removing it — the
    /// `fonts`/`activations` rows are about to cascade-delete regardless
    /// (`ON DELETE CASCADE` on `folder_id`), and without this the actual
    /// installed symlink is orphaned rather than cleaned up. Doesn't touch
    /// `active_memory`: that's what lets re-adding the same folder later
    /// restore these instead of the intent being lost with the rows.
    pub fn remove_folder(&mut self, id: i64) -> Result<()> {
        let path = self
            .store
            .list_folders()
            .ok()
            .and_then(|fs| fs.into_iter().find(|f| f.id == id))
            .map(|f| f.path);

        if let Ok(fonts) = self.store.fonts_in_folder(id) {
            let active = self.store.active_ids().unwrap_or_default();
            for row in fonts {
                if active.contains(&row.id) {
                    if let Ok(Some(installed)) = self.store.clear_activation(row.id) {
                        let _ = activate::uninstall(Path::new(&installed));
                    }
                }
            }
        }
        self.store.remove_folder(id)?;

        if let (Some(w), Some(p)) = (self.watcher.as_mut(), path) {
            let _ = w.watcher().unwatch(Path::new(&p));
        }
        Ok(())
    }

    pub fn rescan(&mut self) -> Result<ScanResult> {
        let folders = self.store.list_folders()?;
        let mut parsed = 0usize;
        let mut removed = 0usize;
        let mut failed = 0usize;
        for folder in &folders {
            if let Ok(summary) = scan_folder(&mut self.store, folder, &self.ignored) {
                parsed += summary.parsed_files;
                removed += summary.removed_files;
                failed += summary.failed_files;
            }
        }
        let mut status = format!("{parsed} updated, {removed} removed, {failed} failed");
        let restored = self.reconcile_activation();
        if restored > 0 {
            status.push_str(&format!(", {restored} reactivated from memory"));
        }
        Ok(ScanResult { status })
    }

    /// What the toolbar's rescan button actually triggers: reads every
    /// font file in every library first (bypassing the normal
    /// unchanged-file skip), *then* does a normal rescan. See
    /// `scan::force_touch_folder` for why — a synced folder's cloud
    /// client sometimes needs a local touch before it'll actually sync.
    pub fn force_sync(&mut self) -> Result<ScanResult> {
        let folders = self.store.list_folders()?;
        let touched: usize = folders.iter().map(force_touch_folder).sum();
        let mut result = self.rescan()?;
        if touched > 0 {
            result.status = format!("touched {touched} files, {}", result.status);
        }
        Ok(result)
    }

    /// Re-activates any catalogued font whose path is in `active_memory`
    /// but isn't currently active — the actual recovery step after a
    /// folder (re-)scan. Best-effort: an install failure for one font
    /// (e.g. the file moved) just skips it rather than failing the scan.
    pub fn reconcile_activation(&mut self) -> usize {
        let Ok(all) = self.store.all_fonts() else {
            return 0;
        };
        let already_active = self.store.active_ids().unwrap_or_default();
        let mut restored = 0;
        for row in all {
            if already_active.contains(&row.id) || !self.active_memory.contains(&row.record.path)
            {
                continue;
            }
            let Ok(installed) =
                activate::install(Path::new(&row.record.path), &format!("glyphclub-{}", row.id))
            else {
                continue;
            };
            if self
                .store
                .record_activation(row.id, &installed.to_string_lossy())
                .is_ok()
            {
                restored += 1;
            }
        }
        restored
    }

    pub fn toggle_activation(&mut self, id: i64) -> Result<bool> {
        let currently_active = self.store.active_ids()?.contains(&id);
        let path = self.store.font_path(id)?.map(|(p, _)| p);
        if currently_active {
            if let Some(installed) = self.store.clear_activation(id)? {
                let _ = activate::uninstall(Path::new(&installed));
            }
            if let Some(p) = path {
                self.active_memory.remove(&p);
                activation_memory::save(&self.active_memory);
            }
            Ok(false)
        } else {
            let path = path.ok_or_else(|| anyhow!("font not found"))?;
            let installed = activate::install(Path::new(&path), &format!("glyphclub-{id}"))?;
            self.store
                .record_activation(id, &installed.to_string_lossy())?;
            self.active_memory.insert(path);
            activation_memory::save(&self.active_memory);
            Ok(true)
        }
    }

    /// Installs `id` without recording it in `active_memory` — it shows
    /// active for the rest of this session (and, like any other active
    /// font, survives a folder remove/re-add via `reconcile_activation`)
    /// but won't be restored on the next launch, and gets uninstalled
    /// outright when the app quits — see `main.rs`'s quit handling and
    /// `App::temp_active_ids`, which is what actually makes it temporary.
    /// A no-op if `id` is already active, temporarily or not.
    pub fn activate_temporarily(&mut self, id: i64) -> Result<bool> {
        if self.store.active_ids()?.contains(&id) {
            return Ok(false);
        }
        let (path, _) = self
            .store
            .font_path(id)?
            .ok_or_else(|| anyhow!("font not found"))?;
        let installed = activate::install(Path::new(&path), &format!("glyphclub-{id}"))?;
        self.store
            .record_activation(id, &installed.to_string_lossy())?;
        Ok(true)
    }

    /// Deactivates `id` without touching `active_memory` — the
    /// counterpart to `activate_temporarily`, and also what quit-time
    /// cleanup calls for every font that was only ever temporarily
    /// activated. A no-op if `id` isn't active.
    pub fn force_deactivate(&mut self, id: i64) -> Result<()> {
        if let Some(installed) = self.store.clear_activation(id)? {
            let _ = activate::uninstall(Path::new(&installed));
        }
        Ok(())
    }

    /// Font ▸ Remove from Fontlist: untracks every face of `id`'s file
    /// without touching it on disk — deactivates whichever of them are
    /// active, drops all of their `fonts` rows, and remembers the path so
    /// `scan_folder` won't just re-discover and re-add it on the next
    /// scan (see `store::ignored_paths`).
    pub fn remove_from_fontlist(&mut self, id: i64) -> Result<()> {
        let (path, _) = self
            .store
            .font_path(id)?
            .ok_or_else(|| anyhow!("font not found"))?;
        self.deactivate_path(&path)?;
        self.store.delete_path(&path)?;
        self.store.ignore_path(&path)?;
        self.ignored.insert(path);
        Ok(())
    }

    /// Font ▸ Delete from Library: deletes the actual file from disk — a
    /// real, unrecoverable, and (since a library is typically a team's
    /// externally-synced folder) *shared* action, which is why the UI
    /// that calls this always confirms first (see `App::confirm_delete`).
    pub fn delete_font_file(&mut self, id: i64) -> Result<()> {
        let (path, _) = self
            .store
            .font_path(id)?
            .ok_or_else(|| anyhow!("font not found"))?;
        self.deactivate_path(&path)?;
        self.store.delete_path(&path)?;
        std::fs::remove_file(&path)?;
        Ok(())
    }

    /// Deactivates every face sharing `path` and forgets them in
    /// `active_memory` — shared by `remove_from_fontlist` and
    /// `delete_font_file`, both of which need every co-located face
    /// (a `.ttc`/`.otc` collection can bundle several) cleaned up, not
    /// just whichever single face the UI had selected.
    fn deactivate_path(&mut self, path: &str) -> Result<()> {
        let active = self.store.active_ids().unwrap_or_default();
        for face_id in self.store.font_ids_for_path(path)? {
            if active.contains(&face_id) {
                self.force_deactivate(face_id)?;
            }
        }
        if self.active_memory.remove(path) {
            activation_memory::save(&self.active_memory);
        }
        Ok(())
    }

    pub fn toggle_favorite(&mut self, id: i64) -> Result<bool> {
        let now_favorite = !self.store.favorite_ids()?.contains(&id);
        self.store.set_favorite(id, now_favorite)?;
        Ok(now_favorite)
    }

    pub fn list_tags(&self) -> Result<Vec<crate::store::Tag>> {
        self.store.list_tags()
    }

    pub fn create_tag(&self, name: &str) -> Result<i64> {
        self.store.create_tag(name)
    }

    pub fn font_tags(&self) -> Result<HashMap<i64, HashSet<i64>>> {
        self.store.font_tags()
    }

    pub fn toggle_font_tag(&mut self, font_id: i64, tag_id: i64) -> Result<bool> {
        let now_tagged = !self
            .store
            .font_tags()?
            .get(&tag_id)
            .is_some_and(|ids| ids.contains(&font_id));
        self.store.set_font_tag(font_id, tag_id, now_tagged)?;
        Ok(now_tagged)
    }

    pub fn font_bytes(&self, id: i64) -> Result<Vec<u8>> {
        let (path, _face_index) = self
            .store
            .font_path(id)?
            .ok_or_else(|| anyhow!("font not found"))?;
        Ok(std::fs::read(path)?)
    }
}
