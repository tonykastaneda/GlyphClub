//! Thin app-facing wrapper around [`Store`] + [`scan_folder`] + [`activate`]
//! — the same operations the old Tauri `commands.rs` exposed over IPC,
//! called directly in-process now that the UI lives in the same binary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::activate;
use crate::activation_memory;
use crate::scan::scan_folder;
use crate::store::{Folder, FontRow, Store};

pub struct ScanResult {
    pub status: String,
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
}

impl Catalog {
    pub fn open() -> Result<Self> {
        Ok(Self {
            store: Store::open(&catalog_path())?,
            active_memory: activation_memory::load(),
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
        let folders = self.store.list_folders()?;
        let mut status = match folders.iter().find(|f| f.id == id) {
            Some(folder) => match scan_folder(&mut self.store, folder) {
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
        self.store.remove_folder(id)
    }

    pub fn rescan(&mut self) -> Result<ScanResult> {
        let folders = self.store.list_folders()?;
        let mut parsed = 0usize;
        let mut removed = 0usize;
        let mut failed = 0usize;
        for folder in &folders {
            if let Ok(summary) = scan_folder(&mut self.store, folder) {
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

    pub fn toggle_favorite(&mut self, id: i64) -> Result<bool> {
        let now_favorite = !self.store.favorite_ids()?.contains(&id);
        self.store.set_favorite(id, now_favorite)?;
        Ok(now_favorite)
    }

    pub fn font_bytes(&self, id: i64) -> Result<Vec<u8>> {
        let (path, _face_index) = self
            .store
            .font_path(id)?
            .ok_or_else(|| anyhow!("font not found"))?;
        Ok(std::fs::read(path)?)
    }
}
