//! Thin app-facing wrapper around [`Store`] + [`scan_folder`] + [`activate`]
//! — the same operations the old Tauri `commands.rs` exposed over IPC,
//! called directly in-process now that the UI lives in the same binary.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

use crate::activate;
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
}

impl Catalog {
    pub fn open() -> Result<Self> {
        Ok(Self {
            store: Store::open(&catalog_path())?,
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
        let status = match folders.iter().find(|f| f.id == id) {
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
        Ok(ScanResult { status })
    }

    pub fn remove_folder(&mut self, id: i64) -> Result<()> {
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
        Ok(ScanResult {
            status: format!("{parsed} updated, {removed} removed, {failed} failed"),
        })
    }

    pub fn toggle_activation(&mut self, id: i64) -> Result<bool> {
        let currently_active = self.store.active_ids()?.contains(&id);
        if currently_active {
            if let Some(installed) = self.store.clear_activation(id)? {
                let _ = activate::uninstall(Path::new(&installed));
            }
            Ok(false)
        } else {
            let (path, _face_index) = self
                .store
                .font_path(id)?
                .ok_or_else(|| anyhow!("font not found"))?;
            let installed = activate::install(Path::new(&path), &format!("glyphclub-{id}"))?;
            self.store
                .record_activation(id, &installed.to_string_lossy())?;
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
