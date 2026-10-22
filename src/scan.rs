use std::collections::HashSet;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use rayon::prelude::*;
use walkdir::WalkDir;

use crate::font::{has_font_extension, parse_faces, FontRecord};
use crate::store::{Folder, Store};

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanSummary {
    pub parsed_files: usize,
    pub removed_files: usize,
    pub failed_files: usize,
    /// Taken from the library's shared file rather than read.
    pub from_shared_files: usize,
}

/// Walks `folder` on disk, parses whatever is new or changed since the last
/// scan (compared by mtime + size against the cache), and drops entries for
/// files that no longer exist. Unchanged files are never re-parsed, which is
/// what keeps repeat scans of a 20k-font library fast. `ignored` (Font ▸
/// Remove from Fontlist — see `store::ignored_paths`) is skipped entirely,
/// as if the file weren't there: not indexed, and not treated as "removed"
/// either, since it's really still sitting on disk.
///
/// `shared` is the library's `.glyphclub` file (see `library_file`): a new
/// or changed file it already describes, at the same size and date, is
/// taken from there instead of being opened.
pub fn scan_folder(store: &mut Store, folder: &Folder, ignored: &HashSet<String>) -> Result<ScanSummary> {
    scan_folder_with(store, folder, ignored, &crate::library_file::Shared::new())
}

pub fn scan_folder_with(
    store: &mut Store,
    folder: &Folder,
    ignored: &HashSet<String>,
    shared: &crate::library_file::Shared,
) -> Result<ScanSummary> {
    let known = store.known_files(folder.id)?;
    let mut seen = HashSet::with_capacity(known.len());
    let mut to_parse: Vec<(String, i64, i64)> = Vec::new();
    let mut from_file: Vec<(String, Vec<FontRecord>)> = Vec::new();

    for entry in WalkDir::new(&folder.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !has_font_extension(path) {
            continue;
        }
        let path_str = path.to_string_lossy().into_owned();
        if ignored.contains(&path_str) {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = metadata.len() as i64;

        seen.insert(path_str.clone());

        if known.get(&path_str) != Some(&(mtime, size)) {
            let from_shared = crate::library_file::relative(&folder.path, &path_str)
                .and_then(|rel| shared.get(&rel))
                .and_then(|entry| crate::library_file::records_if_current(entry, &path_str, size, mtime));
            match from_shared {
                Some(records) => from_file.push((path_str, records)),
                None => to_parse.push((path_str, mtime, size)),
            }
        }
    }

    let mut summary = ScanSummary::default();

    let parsed: Vec<(String, Vec<FontRecord>)> = to_parse
        .par_iter()
        .filter_map(|(path_str, mtime, size)| {
            let path = Path::new(path_str);
            let data = std::fs::read(path).ok()?;
            let mut records = parse_faces(&data, path, *mtime, *size);
            let added = crate::added::added_time(path).unwrap_or(*mtime);
            for r in &mut records {
                r.added = added;
            }
            if records.is_empty() {
                None
            } else {
                Some((path_str.clone(), records))
            }
        })
        .collect();

    summary.failed_files = to_parse.len() - parsed.len();

    for (path_str, records) in &parsed {
        store.replace_faces(folder.id, path_str, records)?;
        summary.parsed_files += 1;
    }
    for (path_str, records) in &from_file {
        store.replace_faces(folder.id, path_str, records)?;
        summary.from_shared_files += 1;
    }

    for stale in known.keys().filter(|p| !seen.contains(*p)) {
        store.delete_path(stale)?;
        summary.removed_files += 1;
    }

    Ok(summary)
}

/// Reads every font file in `folder` (regardless of whether its cached
/// mtime/size looks unchanged — that optimization is exactly what this
/// bypasses) and bumps its mtime. For a library folder that's really a
/// synced Dropbox/Drive/OneDrive/iCloud folder, those clients typically
/// only materialize a cloud-only placeholder — or notice it needs to sync
/// at all — once something local actually touches the file; a plain
/// directory listing often isn't enough. This is what the toolbar's
/// rescan button does before its normal rescan, as a manual "no really,
/// check right now" escape hatch for whichever sync client is being lazy.
/// Best-effort throughout: one unreadable file doesn't stop the rest.
pub fn force_touch_folder(folder: &Folder) -> usize {
    let mut touched = 0usize;
    for entry in WalkDir::new(&folder.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !has_font_extension(path) {
            continue;
        }
        if std::fs::read(path).is_ok() {
            let _ = filetime::set_file_mtime(path, filetime::FileTime::now());
            touched += 1;
        }
    }
    touched
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn scans_system_fonts_and_caches_on_rescan() {
        let db_file = tempfile::NamedTempFile::new().unwrap();
        let mut store = Store::open(db_file.path()).unwrap();
        let folder_id = store.add_folder("/System/Library/Fonts").unwrap();
        let folder = Folder {
            id: folder_id,
            path: "/System/Library/Fonts".to_string(),
        };

        let first = scan_folder(&mut store, &folder, &HashSet::new()).unwrap();
        assert!(first.parsed_files > 0, "expected system fonts to be found");

        let all = store.all_fonts().unwrap();
        assert!(!all.is_empty());
        assert!(
            all.iter().any(|row| !row.record.family.is_empty()),
            "every parsed face should have a family name"
        );

        // A second scan with nothing changed on disk should re-parse nothing.
        let second = scan_folder(&mut store, &folder, &HashSet::new()).unwrap();
        assert_eq!(second.parsed_files, 0);
        assert_eq!(second.removed_files, 0);
    }
}
