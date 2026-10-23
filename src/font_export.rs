//! File ▸ Export ▸ User Installed Fonts… — copies every font the user
//! installed into their own per-user font folder (see
//! `activate::user_font_dir`) into a folder or library they pick, and can
//! then hand them over to GlyphClub: activated from there, and removed from
//! the Fonts folder.
//!
//! The steps, which the export wizard (`gui::export_wizard`) and
//! `glyphclub-cli export-user-fonts` both follow:
//! 1. [`plan_user_export`]: what would be copied where, and which files the
//!    folder already has under the same name (a conflict), unless they're
//!    byte-for-byte the same file.
//! 2. [`run`]: copies, settling each conflict with a [`Resolution`].
//! 3. [`remove_originals`] (when GlyphClub takes the fonts over): uninstalls
//!    the originals whose copies made it.
//!
//! GlyphClub's own activations (`activate::MANAGED_DIR_NAME`) are left
//! out: those are copies of fonts that already live in a library, not
//! something the user installed.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{bail, Context, Result};
use walkdir::WalkDir;

use crate::activate;
use crate::font::has_font_extension;

/// A font file, with what the wizard shows to tell two copies apart.
#[derive(Debug, Clone)]
pub struct FontFile {
    pub path: PathBuf,
    pub file_name: String,
    /// The font's full name (`name` ID 4), e.g. "Helvetica Neue Bold";
    /// the file name when it has none.
    pub full_name: String,
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub size: u64,
}

impl FontFile {
    pub fn read(path: &Path) -> FontFile {
        let meta = std::fs::metadata(path).ok();
        let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let full_name = std::fs::read(path)
            .ok()
            .and_then(|data| crate::font::full_name(&data))
            .unwrap_or_else(|| file_name.clone());
        FontFile {
            path: path.to_path_buf(),
            file_name,
            full_name,
            created: meta.as_ref().and_then(|m| m.created().ok()),
            modified: meta.as_ref().and_then(|m| m.modified().ok()),
            size: meta.map(|m| m.len()).unwrap_or(0),
        }
    }
}

/// One font to export.
#[derive(Debug, Clone)]
pub struct Item {
    /// Where it goes, relative to the destination (subfolders kept).
    pub rel: PathBuf,
    pub mine: FontFile,
    /// What the destination already has at that path.
    pub theirs: Option<FontFile>,
    /// `theirs` is the same file byte for byte: nothing to copy or decide.
    pub identical: bool,
}

impl Item {
    pub fn is_conflict(&self) -> bool {
        self.theirs.is_some() && !self.identical
    }
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub source: PathBuf,
    pub dest: PathBuf,
    pub items: Vec<Item>,
}

impl Plan {
    /// Indices into `items` of the conflicts, in order.
    pub fn conflicts(&self) -> Vec<usize> {
        (0..self.items.len()).filter(|&i| self.items[i].is_conflict()).collect()
    }
}

/// How one conflict is settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// Your copy replaces the folder's.
    Overwrite,
    /// The folder's copy stays; yours isn't copied.
    KeepTheirs,
    /// Both stay: yours is copied as `name-1.ttf` (or `-2`, `-3`, … —
    /// the first free one).
    Append,
}

#[derive(Debug, Default, Clone)]
pub struct ExportSummary {
    pub dest: PathBuf,
    /// Copied where nothing was in the way.
    pub copied: usize,
    pub overwritten: usize,
    pub renamed: usize,
    /// Conflicts settled by keeping the folder's copy.
    pub kept: usize,
    /// Already there, byte for byte.
    pub identical: usize,
    pub failed: usize,
    /// Each exported original with the file in the destination that now
    /// stands for it (its copy, or the folder's own copy when that was kept
    /// or identical): what [`remove_originals`] may remove.
    pub exported: Vec<(PathBuf, PathBuf)>,
}

impl ExportSummary {
    /// Fonts that are now in the destination one way or another.
    pub fn total(&self) -> usize {
        self.exported.len()
    }
}

/// The user's Fonts folder: `GLYPHCLUB_USER_FONT_DIR` when set (for
/// testing the export on a stand-in folder), else the OS's.
pub fn user_font_dir() -> Option<PathBuf> {
    std::env::var_os("GLYPHCLUB_USER_FONT_DIR")
        .map(PathBuf::from)
        .or_else(activate::user_font_dir)
}

/// What exporting the user's installed fonts into `dest` would do.
pub fn plan_user_export(dest: &Path) -> Result<Plan> {
    let source = user_font_dir().context("could not determine the user font directory")?;
    plan(&source, dest)
}

pub fn plan(source: &Path, dest: &Path) -> Result<Plan> {
    // Exporting into the font folder itself would have the OS load every
    // exported font a second time.
    if dest.starts_with(source) || canonical(dest).starts_with(canonical(source)) {
        bail!("Pick a folder outside your Fonts folder.");
    }
    let mut items = Vec::new();
    for path in user_fonts_in(source) {
        let Ok(rel) = path.strip_prefix(source) else {
            continue;
        };
        let rel = rel.to_path_buf();
        let target = dest.join(&rel);
        let theirs = target.is_file().then(|| FontFile::read(&target));
        let identical = theirs.is_some() && same_contents(&path, &target);
        items.push(Item {
            rel,
            mine: FontFile::read(&path),
            theirs,
            identical,
        });
    }
    items.sort_by(|a, b| a.rel.cmp(&b.rel));
    Ok(Plan {
        source: source.to_path_buf(),
        dest: dest.to_path_buf(),
        items,
    })
}

/// The font files the user installed in `source`, skipping GlyphClub's
/// own activation folder.
fn user_fonts_in(source: &Path) -> Vec<PathBuf> {
    let managed = source.join(activate::MANAGED_DIR_NAME);
    // `follow_links(false)`: a symlink in the font folder points at a font
    // that lives somewhere else (typically another font manager's
    // activation), so it isn't an installed font in its own right.
    WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.path() != managed)
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && has_font_extension(e.path()))
        .map(|e| e.into_path())
        .collect()
}

/// Copies `plan`'s fonts, asking `resolve` how to settle each conflict
/// (by its index in `plan.items`).
pub fn run(plan: &Plan, resolve: impl Fn(usize) -> Resolution) -> ExportSummary {
    let mut s = ExportSummary {
        dest: plan.dest.clone(),
        ..Default::default()
    };
    if std::fs::create_dir_all(&plan.dest).is_err() {
        s.failed = plan.items.len();
        return s;
    }
    for (i, item) in plan.items.iter().enumerate() {
        let target = plan.dest.join(&item.rel);
        let original = item.mine.path.clone();
        if item.identical {
            s.identical += 1;
            s.exported.push((original, target));
            continue;
        }
        // Checked again rather than trusting the plan: the folder may have
        // changed since (a synced library especially).
        #[derive(Clone, Copy)]
        enum How {
            Copied,
            Overwritten,
            Renamed,
        }
        let (target, how) = if target.exists() {
            match if item.theirs.is_some() { resolve(i) } else { Resolution::Append } {
                Resolution::KeepTheirs => {
                    s.kept += 1;
                    s.exported.push((original, target));
                    continue;
                }
                Resolution::Overwrite => (target, How::Overwritten),
                Resolution::Append => (appended_path(&target), How::Renamed),
            }
        } else {
            (target, How::Copied)
        };
        if let Some(parent) = target.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                s.failed += 1;
                continue;
            }
        }
        match copy_file(&original, &target) {
            Ok(()) => {
                match how {
                    How::Copied => s.copied += 1,
                    How::Overwritten => s.overwritten += 1,
                    How::Renamed => s.renamed += 1,
                }
                s.exported.push((original, target));
            }
            Err(_) => s.failed += 1,
        }
    }
    s
}

/// Copies through a temporary file and renames it into place, so an
/// overwrite that fails halfway leaves the folder's copy as it was.
fn copy_file(from: &Path, to: &Path) -> Result<()> {
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = to.with_file_name(format!(".{name}.glyphclub-export"));
    std::fs::copy(from, &tmp).with_context(|| format!("couldn't copy {from:?}"))?;
    if let Err(e) = std::fs::rename(&tmp, to) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("couldn't write {to:?}"));
    }
    Ok(())
}

/// `Name.ttf` → the first of `Name-1.ttf`, `Name-2.ttf`, … that isn't taken.
pub fn appended_path(target: &Path) -> PathBuf {
    (1..)
        .map(|n| appended_name(target, n))
        .find(|p| !p.exists())
        .expect("some number is free")
}

/// `Name.ttf` with `-n` before the extension.
pub fn appended_name(target: &Path, n: usize) -> PathBuf {
    let stem = target.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let name = match target.extension() {
        Some(ext) => format!("{stem}-{n}.{}", ext.to_string_lossy()),
        None => format!("{stem}-{n}"),
    };
    target.with_file_name(name)
}

#[derive(Debug, Default)]
pub struct RemoveSummary {
    pub removed: usize,
    /// Still installed: in use, or its copy went missing.
    pub failed: usize,
}

/// Uninstalls each exported original from the user's Fonts folder, once
/// its stand-in in the destination is checked to be there. Anything
/// outside the Fonts folder, or in GlyphClub's own activation folder, is
/// never touched.
pub fn remove_originals(exported: &[(PathBuf, PathBuf)]) -> RemoveSummary {
    let mut s = RemoveSummary::default();
    let Some(fonts) = user_font_dir() else {
        s.failed = exported.len();
        return s;
    };
    let managed = fonts.join(activate::MANAGED_DIR_NAME);
    for (original, copy) in exported {
        let safe = original.starts_with(&fonts) && !original.starts_with(&managed) && original != copy;
        let copied = std::fs::metadata(copy).is_ok_and(|m| m.is_file() && m.len() > 0);
        if !safe || !copied || !original.is_file() {
            s.failed += 1;
            continue;
        }
        match platform::uninstall_user_font(original) {
            Ok(()) => s.removed += 1,
            Err(_) => s.failed += 1,
        }
    }
    s
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn same_contents(a: &Path, b: &Path) -> bool {
    let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) else {
        return false;
    };
    if ma.len() != mb.len() {
        return false;
    }
    matches!((std::fs::read(a), std::fs::read(b)), (Ok(x), Ok(y)) if x == y)
}

#[cfg(not(windows))]
mod platform {
    use std::path::Path;

    /// macOS's font daemon notices the file going and unregisters it.
    pub fn uninstall_user_font(path: &Path) -> anyhow::Result<()> {
        std::fs::remove_file(path)?;
        Ok(())
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    use anyhow::Result;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_SUCCESS, LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::RemoveFontResourceW;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE,
    };
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_FONTCHANGE};

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// A per-user font is the file plus a value under HKCU's Fonts key
    /// naming it (by full path, or by file name); both go, and the running
    /// session lets go of it. Fails, leaving it installed, when an app has
    /// the file open.
    pub fn uninstall_user_font(path: &Path) -> Result<()> {
        let w = wide(path.as_os_str());
        unsafe {
            let _ = RemoveFontResourceW(PCWSTR(w.as_ptr()));
        }
        std::fs::remove_file(path)?;
        let _ = remove_registry_value(path);
        unsafe {
            let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_FONTCHANGE, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 1000, None);
        }
        Ok(())
    }

    fn remove_registry_value(path: &Path) -> Result<()> {
        let subkey = wide(std::ffi::OsStr::new(r"Software\Microsoft\Windows NT\CurrentVersion\Fonts"));
        let full = path.to_string_lossy().to_lowercase();
        let file = path.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        let mut key = HKEY::default();
        unsafe {
            if RegOpenKeyExW(HKEY_CURRENT_USER, PCWSTR(subkey.as_ptr()), None, KEY_READ | KEY_SET_VALUE, &mut key)
                != ERROR_SUCCESS
            {
                return Ok(());
            }
            let mut matches: Vec<Vec<u16>> = Vec::new();
            let mut index = 0u32;
            loop {
                let mut name = vec![0u16; 1024];
                let mut name_len = name.len() as u32;
                let mut data = vec![0u8; 4096];
                let mut data_len = data.len() as u32;
                let mut kind = 0u32;
                let status = RegEnumValueW(
                    key,
                    index,
                    Some(PWSTR(name.as_mut_ptr())),
                    &mut name_len,
                    None,
                    Some(&mut kind as *mut u32),
                    Some(data.as_mut_ptr()),
                    Some(&mut data_len),
                );
                if status != ERROR_SUCCESS {
                    break;
                }
                index += 1;
                let units: Vec<u16> = data[..data_len as usize]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|&u| u != 0)
                    .collect();
                let value = String::from_utf16_lossy(&units).to_lowercase();
                if value == full || value == file {
                    name.truncate(name_len as usize);
                    name.push(0);
                    matches.push(name);
                }
            }
            for name in matches {
                let _ = RegDeleteValueW(key, PCWSTR(name.as_ptr()));
            }
            let _ = RegCloseKey(key);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, tempfile::TempDir) {
        let source = tempfile::tempdir().unwrap();
        let s = source.path();
        std::fs::write(s.join("A.otf"), b"a-mine").unwrap();
        std::fs::create_dir_all(s.join("Brand")).unwrap();
        std::fs::write(s.join("Brand").join("B.ttf"), b"b").unwrap();
        std::fs::write(s.join("C.ttf"), b"c-same").unwrap();
        std::fs::write(s.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(s.join(activate::MANAGED_DIR_NAME)).unwrap();
        std::fs::write(s.join(activate::MANAGED_DIR_NAME).join("D.ttf"), b"d").unwrap();
        let dest = tempfile::tempdir().unwrap();
        std::fs::write(dest.path().join("A.otf"), b"a-theirs").unwrap();
        std::fs::write(dest.path().join("C.ttf"), b"c-same").unwrap();
        (source, dest)
    }

    #[test]
    fn plans_fonts_and_finds_conflicts() {
        let (source, dest) = setup();
        let plan = plan(source.path(), dest.path()).unwrap();
        let rels: Vec<_> = plan.items.iter().map(|i| i.rel.clone()).collect();
        assert_eq!(rels, [PathBuf::from("A.otf"), Path::new("Brand").join("B.ttf"), PathBuf::from("C.ttf")]);
        // C is the same file, so only A is a conflict.
        assert_eq!(plan.conflicts(), [0]);
        assert!(plan.items[2].identical);
    }

    #[test]
    fn overwrite_keep_and_append() {
        for (resolution, expect_a, renamed) in [
            (Resolution::Overwrite, &b"a-mine"[..], false),
            (Resolution::KeepTheirs, &b"a-theirs"[..], false),
            (Resolution::Append, &b"a-theirs"[..], true),
        ] {
            let (source, dest) = setup();
            let d = dest.path();
            let plan = plan(source.path(), d).unwrap();
            let s = run(&plan, |_| resolution);
            assert_eq!(std::fs::read(d.join("A.otf")).unwrap(), expect_a);
            assert!(d.join("Brand").join("B.ttf").is_file());
            assert!(!d.join("notes.txt").exists());
            assert!(!d.join(activate::MANAGED_DIR_NAME).exists());
            assert_eq!(d.join("A-1.otf").is_file(), renamed);
            if renamed {
                assert_eq!(std::fs::read(d.join("A-1.otf")).unwrap(), b"a-mine");
            }
            assert_eq!((s.identical, s.failed, s.total()), (1, 0, 3));
        }
    }

    #[test]
    fn append_takes_the_next_free_number() {
        let dir = tempfile::tempdir().unwrap();
        let t = dir.path().join("Font.ttf");
        std::fs::write(&t, b"x").unwrap();
        std::fs::write(dir.path().join("Font-1.ttf"), b"x").unwrap();
        assert_eq!(appended_path(&t), dir.path().join("Font-2.ttf"));
    }

    #[test]
    fn refuses_to_export_into_the_font_folder_itself() {
        let source = tempfile::tempdir().unwrap();
        assert!(plan(source.path(), &source.path().join("Export")).is_err());
    }
}
