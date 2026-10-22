//! `<folder>.glyphclub` (e.g. `2017-2023.glyphclub`): a library's font
//! info, kept in the library folder itself so everyone who syncs that folder (a team on Dropbox, a
//! second Mac on iCloud) shares it. GlyphClub fills its own local catalog
//! from it instead of opening every font file — which, for a synced folder
//! with online-only files, would also mean downloading every font first.
//!
//! Only what's read out of the font files is shared: names, style, weight,
//! character coverage, the date added. Stars, tags, recents and what's
//! activated stay in each Mac's local catalog (`store.rs`).
//!
//! The local SQLite catalog stays the working copy; this file is plain
//! JSON inside (a database in a synced folder is a known way to get corruption
//! and conflicted copies). It's rewritten only when its content changes,
//! one font per line, sorted, so syncing stays quiet.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::font::FontRecord;

pub const EXTENSION: &str = "glyphclub";
const FORMAT: u32 = 1;

/// Where the library's file belongs: named after the folder.
fn file_path(root: &str) -> std::path::PathBuf {
    let root = Path::new(root);
    let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Library".into());
    root.join(format!("{name}.{EXTENSION}"))
}

/// The library's file as it is on disk: the folder-named one, or, after
/// the folder was renamed, whichever `.glyphclub` file is there.
fn existing_file(root: &str) -> Option<std::path::PathBuf> {
    let named = file_path(root);
    if named.is_file() {
        return Some(named);
    }
    let mut others: Vec<_> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == EXTENSION))
        .collect();
    others.sort();
    others.into_iter().next()
}

/// One font file in the library, by its path inside the library.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub file: String,
    pub size: i64,
    pub modified: i64,
    pub added: i64,
    pub faces: Vec<Face>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub index: u32,
    pub family: String,
    pub style: String,
    #[serde(default)]
    pub postscript: String,
    pub weight: u16,
    #[serde(default)]
    pub italic: bool,
    #[serde(default)]
    pub monospace: bool,
    /// Characters with real glyphs, as hex ranges: "20-7e,a0,a1-ff".
    #[serde(default)]
    pub coverage: String,
}

#[derive(Serialize, Deserialize)]
struct LibraryFile {
    format: u32,
    #[serde(default)]
    fonts: Vec<Entry>,
}

/// A library's shared entries, by path inside the library.
pub type Shared = HashMap<String, Entry>;

/// `path` relative to `root`, with `/` separators, so the same font has the
/// same key on every Mac and PC.
pub fn relative(root: &str, path: &str) -> Option<String> {
    let rel = Path::new(path).strip_prefix(root).ok()?;
    let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    (!parts.is_empty()).then(|| parts.join("/"))
}

fn encode_coverage(ranges: &[(u32, u32)]) -> String {
    ranges
        .iter()
        .map(|&(a, b)| if a == b { format!("{a:x}") } else { format!("{a:x}-{b:x}") })
        .collect::<Vec<_>>()
        .join(",")
}

fn decode_coverage(text: &str) -> Vec<(u32, u32)> {
    text.split(',')
        .filter_map(|part| {
            let (a, b) = part.split_once('-').unwrap_or((part, part));
            Some((u32::from_str_radix(a.trim(), 16).ok()?, u32::from_str_radix(b.trim(), 16).ok()?))
        })
        .collect()
}

/// Whether the library has a `.glyphclub` file.
pub fn has_file(root: &str) -> bool {
    existing_file(root).is_some()
}

/// The library's shared file, if it has a readable one.
pub fn load(root: &str) -> Shared {
    let Some(Ok(text)) = existing_file(root).map(std::fs::read_to_string) else {
        return Shared::new();
    };
    match serde_json::from_str::<LibraryFile>(&text) {
        Ok(file) if file.format <= FORMAT => file.fonts.into_iter().map(|e| (e.file.clone(), e)).collect(),
        _ => Shared::new(),
    }
}

/// `entry`'s faces as catalog records for the font at `path`, when its size
/// and date still match what's on disk — otherwise the file has to be read.
pub fn records_if_current(entry: &Entry, path: &str, size: i64, modified: i64) -> Option<Vec<FontRecord>> {
    if entry.size != size || entry.modified != modified || entry.faces.is_empty() {
        return None;
    }
    Some(
        entry
            .faces
            .iter()
            .map(|f| FontRecord {
                path: path.to_string(),
                face_index: f.index,
                family: f.family.clone(),
                subfamily: f.style.clone(),
                postscript_name: f.postscript.clone(),
                coverage: Some(decode_coverage(&f.coverage)),
                weight: f.weight,
                italic: f.italic,
                monospace: f.monospace,
                mtime: modified,
                size,
                added: entry.added,
            })
            .collect(),
    )
}

fn entry_for(file: String, records: &[FontRecord]) -> Entry {
    let first = &records[0];
    let mut faces: Vec<Face> = records
        .iter()
        .map(|r| Face {
            index: r.face_index,
            family: r.family.clone(),
            style: r.subfamily.clone(),
            postscript: r.postscript_name.clone(),
            weight: r.weight,
            italic: r.italic,
            monospace: r.monospace,
            coverage: encode_coverage(r.coverage.as_deref().unwrap_or(&[])),
        })
        .collect();
    faces.sort_by_key(|f| f.index);
    Entry { file, size: first.size, modified: first.mtime, added: first.added, faces }
}

/// Writes the library's shared file from `records` (every face this Mac
/// has catalogued in it, with coverage), when that changes its content.
/// An existing entry whose font info still matches is kept as it is, even
/// if this Mac sees a different file date, so two Macs don't take turns
/// rewriting it. Entries for `ignored` files (Remove from Fontlist, a
/// personal choice) are kept too. Best-effort: a library that can't be
/// written to (read-only share) just doesn't get one.
pub fn save(root: &str, records: &[FontRecord], ignored: &HashSet<String>) -> std::io::Result<bool> {
    let existing = load(root);
    let mut by_file: HashMap<String, Vec<FontRecord>> = HashMap::new();
    for r in records {
        if let Some(rel) = relative(root, &r.path) {
            by_file.entry(rel).or_default().push(r.clone());
        }
    }
    if by_file.is_empty() && existing.is_empty() {
        return Ok(false);
    }

    let mut entries: Vec<Entry> = by_file
        .into_iter()
        .map(|(file, faces)| {
            let fresh = entry_for(file.clone(), &faces);
            match existing.get(&file) {
                Some(old) if old.size == fresh.size && old.faces == fresh.faces => old.clone(),
                _ => fresh,
            }
        })
        .collect();
    let have: HashSet<String> = entries.iter().map(|e| e.file.clone()).collect();
    for ignored_path in ignored {
        if let Some(rel) = relative(root, ignored_path) {
            if let Some(old) = existing.get(&rel).filter(|_| !have.contains(&rel)) {
                if Path::new(ignored_path).exists() {
                    entries.push(old.clone());
                }
            }
        }
    }
    entries.sort_by(|a, b| a.file.cmp(&b.file));

    let mut text = format!(
        "{{\n\"format\": {FORMAT},\n\"about\": \"Font info for GlyphClub, shared with everyone who uses this library. GlyphClub keeps it up to date.\",\n\"fonts\": [\n"
    );
    for (i, entry) in entries.iter().enumerate() {
        text.push_str(&serde_json::to_string(entry).map_err(std::io::Error::other)?);
        text.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
    }
    text.push_str("]\n}\n");

    let path = file_path(root);
    let previous = existing_file(root);
    if previous.as_deref() == Some(path.as_path()) && std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
        return Ok(false);
    }
    // Written beside it, then swapped in, so a sync client never uploads a
    // half-written file.
    let tmp = Path::new(root).join(format!(".{EXTENSION}.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)?;
    // The folder was renamed: the old-named file goes.
    if let Some(old) = previous.filter(|old| *old != path) {
        let _ = std::fs::remove_file(old);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(path: &str, index: u32, size: i64, mtime: i64) -> FontRecord {
        FontRecord {
            path: path.to_string(),
            face_index: index,
            family: "Test Sans".into(),
            subfamily: "Bold".into(),
            postscript_name: "TestSans-Bold".into(),
            coverage: Some(vec![(0x20, 0x7e), (0xa0, 0xa0)]),
            weight: 700,
            italic: false,
            monospace: false,
            mtime,
            size,
            added: 1_700_000_000,
        }
    }

    #[test]
    fn round_trips_and_only_rewrites_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let font = format!("{root}/Sub/Test.otf");
        assert!(save(root, &[record(&font, 0, 1234, 99)], &HashSet::new()).unwrap());
        // Same content: left alone.
        assert!(!save(root, &[record(&font, 0, 1234, 99)], &HashSet::new()).unwrap());

        let shared = load(root);
        let entry = &shared["Sub/Test.otf"];
        let back = records_if_current(entry, &font, 1234, 99).unwrap();
        assert_eq!(back[0].postscript_name, "TestSans-Bold");
        assert_eq!(back[0].coverage.as_deref(), Some(&[(0x20, 0x7e), (0xa0, 0xa0)][..]));
        // A changed file has to be read again.
        assert!(records_if_current(entry, &font, 1235, 99).is_none());
    }

    #[test]
    fn named_after_the_folder_and_follows_a_rename() {
        let dir = tempfile::tempdir().unwrap();
        let lib = dir.path().join("2017-2023");
        std::fs::create_dir(&lib).unwrap();
        let root = lib.to_str().unwrap();
        let font = format!("{root}/Test.otf");
        save(root, &[record(&font, 0, 1234, 99)], &HashSet::new()).unwrap();
        assert!(lib.join("2017-2023.glyphclub").is_file());

        let renamed = dir.path().join("Archive");
        std::fs::rename(&lib, &renamed).unwrap();
        let root = renamed.to_str().unwrap();
        assert!(load(root).contains_key("Test.otf"), "still found under its old name");
        let font = format!("{root}/Test.otf");
        save(root, &[record(&font, 0, 1234, 99)], &HashSet::new()).unwrap();
        assert!(renamed.join("Archive.glyphclub").is_file());
        assert!(!renamed.join("2017-2023.glyphclub").exists());
    }

    #[test]
    fn another_mac_s_file_date_doesnt_cause_a_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_str().unwrap();
        let font = format!("{root}/Test.otf");
        save(root, &[record(&font, 0, 1234, 99)], &HashSet::new()).unwrap();
        // This Mac sees a different date for the same font: nothing to write.
        assert!(!save(root, &[record(&font, 0, 1234, 555)], &HashSet::new()).unwrap());
        assert_eq!(load(root)["Test.otf"].modified, 99);
    }
}
