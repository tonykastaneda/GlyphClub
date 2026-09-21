//! Persists which font *file paths* the user has activated, in a small
//! JSON file separate from the SQLite catalog. The catalog's own
//! `fonts`/`activations` rows are tied to a folder via a `ON DELETE
//! CASCADE` foreign key — exactly right for keeping the catalog itself
//! consistent, but it also means removing a library folder (including by
//! mistake) silently erases which of its fonts were active, with no way
//! back. This file is that way back: re-adding a folder whose font paths
//! show up in here re-activates them automatically (see
//! `Catalog::reconcile_activation`).

use std::collections::HashSet;
use std::path::PathBuf;

fn memory_path() -> PathBuf {
    let base = dirs::data_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("glyphclub");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("active_fonts.json")
}

/// Empty (not an error) if the file doesn't exist yet or fails to parse —
/// this is a convenience record, not the source of truth for what's
/// actually installed right now (the catalog's own `activations` table
/// still is), so it degrades gracefully rather than failing the caller.
pub fn load() -> HashSet<String> {
    std::fs::read(memory_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(paths: &HashSet<String>) {
    if let Ok(bytes) = serde_json::to_vec_pretty(paths) {
        let _ = std::fs::write(memory_path(), bytes);
    }
}
