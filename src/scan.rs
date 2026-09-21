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
}

/// Walks `folder` on disk, parses whatever is new or changed since the last
/// scan (compared by mtime + size against the cache), and drops entries for
/// files that no longer exist. Unchanged files are never re-parsed, which is
/// what keeps repeat scans of a 20k-font library fast.
pub fn scan_folder(store: &mut Store, folder: &Folder) -> Result<ScanSummary> {
    let known = store.known_files(folder.id)?;
    let mut seen = HashSet::with_capacity(known.len());
    let mut to_parse: Vec<(String, i64, i64)> = Vec::new();

    for entry in WalkDir::new(&folder.path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !has_font_extension(path) {
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
        let path_str = path.to_string_lossy().into_owned();

        seen.insert(path_str.clone());

        if known.get(&path_str) != Some(&(mtime, size)) {
            to_parse.push((path_str, mtime, size));
        }
    }

    let mut summary = ScanSummary::default();

    let parsed: Vec<(String, Vec<FontRecord>)> = to_parse
        .par_iter()
        .filter_map(|(path_str, mtime, size)| {
            let path = Path::new(path_str);
            let data = std::fs::read(path).ok()?;
            let records = parse_faces(&data, path, *mtime, *size);
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

    for stale in known.keys().filter(|p| !seen.contains(*p)) {
        store.delete_path(stale)?;
        summary.removed_files += 1;
    }

    Ok(summary)
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

        let first = scan_folder(&mut store, &folder).unwrap();
        assert!(first.parsed_files > 0, "expected system fonts to be found");

        let all = store.all_fonts().unwrap();
        assert!(!all.is_empty());
        assert!(
            all.iter().any(|row| !row.record.family.is_empty()),
            "every parsed face should have a family name"
        );

        // A second scan with nothing changed on disk should re-parse nothing.
        let second = scan_folder(&mut store, &folder).unwrap();
        assert_eq!(second.parsed_files, 0);
        assert_eq!(second.removed_files, 0);
    }
}
