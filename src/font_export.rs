//! File ▸ Export User Fonts… — copies every font the user installed into
//! their own per-user font folder (see `activate::user_font_dir`) out into
//! a `User-FontExport` folder wherever they pick. A one-shot "get my fonts
//! out of the OS" step, e.g. before moving them into a synced library.
//!
//! GlyphClub's own activations (`activate::MANAGED_DIR_NAME`) are left
//! out: those are symlinks/copies of fonts that already live in a library,
//! not something the user installed.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

use crate::activate;
use crate::font::has_font_extension;

pub const EXPORT_DIR_NAME: &str = "User-FontExport";

#[derive(Debug, Default)]
pub struct ExportSummary {
    pub dest: PathBuf,
    pub copied: usize,
    /// Already present in the export folder from an earlier run — never
    /// overwritten.
    pub skipped: usize,
    pub failed: usize,
}

/// Always exports into a folder named `User-FontExport` — inside whatever
/// folder was picked, or that folder itself if the user already picked
/// one with that exact name (so a second run doesn't nest a
/// `User-FontExport/User-FontExport`).
pub fn export_dir_for(chosen: &Path) -> PathBuf {
    if chosen.file_name().is_some_and(|n| n == EXPORT_DIR_NAME) {
        chosen.to_path_buf()
    } else {
        chosen.join(EXPORT_DIR_NAME)
    }
}

pub fn export_user_fonts(chosen: &Path) -> Result<ExportSummary> {
    let source = activate::user_font_dir().context("could not determine the user font directory")?;
    export_from(&source, &export_dir_for(chosen))
}

fn export_from(source: &Path, dest: &Path) -> Result<ExportSummary> {
    // Exporting into the font folder itself would have the OS load every
    // exported font a second time.
    if dest.starts_with(source) {
        bail!("pick a folder outside your Fonts folder");
    }
    std::fs::create_dir_all(dest).with_context(|| format!("couldn't create {dest:?}"))?;

    let managed = source.join(activate::MANAGED_DIR_NAME);
    let mut summary = ExportSummary {
        dest: dest.to_path_buf(),
        ..Default::default()
    };

    // `follow_links(false)`: a symlink in the font folder points at a font
    // that lives somewhere else (typically another font manager's
    // activation), so it isn't an installed font in its own right.
    for entry in WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.path() != managed)
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !entry.file_type().is_file() || !has_font_extension(path) {
            continue;
        }
        // Keeps any subfolders the user organized their fonts into, which
        // also means two same-named files in different subfolders can't
        // collide.
        let Ok(rel) = path.strip_prefix(source) else {
            continue;
        };
        let target = dest.join(rel);
        if target.exists() {
            summary.skipped += 1;
            continue;
        }
        if let Some(parent) = target.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                summary.failed += 1;
                continue;
            }
        }
        match std::fs::copy(path, &target) {
            Ok(_) => summary.copied += 1,
            Err(_) => summary.failed += 1,
        }
    }

    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_dir_is_always_named_user_font_export() {
        assert_eq!(
            export_dir_for(Path::new("/tmp/Desktop")),
            Path::new("/tmp/Desktop/User-FontExport")
        );
        assert_eq!(
            export_dir_for(Path::new("/tmp/Desktop/User-FontExport")),
            Path::new("/tmp/Desktop/User-FontExport")
        );
    }

    #[test]
    fn copies_user_fonts_and_skips_glyphclub_activations() {
        let source = tempfile::tempdir().unwrap();
        let out = tempfile::tempdir().unwrap();
        let s = source.path();
        std::fs::write(s.join("A.otf"), b"a").unwrap();
        std::fs::create_dir_all(s.join("Brand")).unwrap();
        std::fs::write(s.join("Brand").join("B.ttf"), b"b").unwrap();
        std::fs::write(s.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(s.join(activate::MANAGED_DIR_NAME)).unwrap();
        std::fs::write(s.join(activate::MANAGED_DIR_NAME).join("C.ttf"), b"c").unwrap();

        let dest = export_dir_for(out.path());
        let first = export_from(s, &dest).unwrap();
        assert_eq!((first.copied, first.skipped, first.failed), (2, 0, 0));
        assert!(dest.join("A.otf").is_file());
        assert!(dest.join("Brand").join("B.ttf").is_file());
        assert!(!dest.join("notes.txt").exists());
        assert!(!dest.join(activate::MANAGED_DIR_NAME).exists());

        // A second run leaves what's already there alone.
        let second = export_from(s, &dest).unwrap();
        assert_eq!((second.copied, second.skipped), (0, 2));
    }

    #[test]
    fn refuses_to_export_into_the_font_folder_itself() {
        let source = tempfile::tempdir().unwrap();
        let dest = export_dir_for(source.path());
        assert!(export_from(source.path(), &dest).is_err());
    }
}
