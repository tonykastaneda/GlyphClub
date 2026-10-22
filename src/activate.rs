use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Installs (activates) a font so every other app on the system can see it,
/// and returns the path of the file/link we created — the caller persists
/// that so `uninstall` can clean up the right thing later.
///
/// Caveat: a `.ttc`/`.otc` collection file holds several faces together.
/// Activating one face's dot makes the *whole* collection available system-
/// wide, since splitting a single face out of a collection into its own
/// font file isn't something we do (it would need a font-subsetting step).
pub fn install(source: &Path, unique_name: &str) -> Result<PathBuf> {
    let installed = platform::install(source, unique_name)?;
    // Copies keep the source's dates; stamp this one with now so the
    // orphan sweep (`installed_files`) can tell a fresh copy from an old one.
    let _ = std::fs::File::options()
        .write(true)
        .open(&installed)
        .and_then(|file| file.set_modified(std::time::SystemTime::now()));
    Ok(installed)
}

/// Removes a previously installed font. `installed` is the path returned by
/// `install`.
pub fn uninstall(installed: &Path) -> Result<()> {
    platform::uninstall(installed)
}

/// The subfolder of the user font directory that `install` activates
/// into — everything under it is GlyphClub's own, not a font the user
/// installed themselves.
pub const MANAGED_DIR_NAME: &str = "glyphclub";

/// The per-user font folder the OS loads fonts from (`~/Library/Fonts` on
/// macOS, `%LOCALAPPDATA%\Microsoft\Windows\Fonts` on Windows) — where
/// Font Book / Windows' own "Install" puts a font by default.
pub fn user_font_dir() -> Option<PathBuf> {
    platform::user_font_dir()
}

/// The font copies in GlyphClub's own activation folder.
pub fn installed_files() -> Vec<PathBuf> {
    let Some(dir) = platform::user_font_dir().map(|d| d.join(MANAGED_DIR_NAME)) else {
        return Vec::new();
    };
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("glyphclub-"))
        })
        .collect()
}

fn managed_dir() -> Result<PathBuf> {
    let base = platform::user_font_dir().context("could not determine the user font directory")?;
    let dir = base.join(MANAGED_DIR_NAME);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[cfg(target_os = "macos")]
mod platform {
    use super::managed_dir;
    use anyhow::{Context, Result};
    use std::path::{Path, PathBuf};

    pub fn user_font_dir() -> Option<PathBuf> {
        dirs::font_dir()
    }

    /// Copies the file into `~/Library/Fonts`, which `fontd` watches.
    /// A symlink would avoid the copy, but `fontd` never registers one
    /// (checked on macOS 26: top level or subfolder, pointing into iCloud
    /// or not), so the font would look active here and be missing
    /// everywhere else. Copying also makes iCloud/Dropbox materialize a
    /// cloud-only placeholder, which a symlink wouldn't.
    pub fn install(source: &Path, unique_name: &str) -> Result<PathBuf> {
        let dir = managed_dir()?;
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("ttf");
        let dest = dir.join(format!("{unique_name}.{ext}"));

        if dest.symlink_metadata().is_ok() {
            std::fs::remove_file(&dest).ok();
        }
        std::fs::copy(source, &dest)
            .with_context(|| format!("failed to copy {source:?} to {dest:?}"))?;

        Ok(dest)
    }

    pub fn uninstall(installed: &Path) -> Result<()> {
        if installed.symlink_metadata().is_ok() {
            std::fs::remove_file(installed)
                .with_context(|| format!("failed to remove {installed:?}"))?;
        }
        Ok(())
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    #[test]
    fn installs_a_real_copy_and_uninstalls_it() {
        let source = Path::new("/System/Library/Fonts/Helvetica.ttc");
        let installed = install(source, "glyphclub-activation-test").expect("install should succeed");

        let meta = installed.symlink_metadata().expect("installed file should exist");
        assert!(meta.file_type().is_file(), "expected a real file, not a symlink, at {installed:?}");
        assert_eq!(meta.len(), source.metadata().unwrap().len());

        uninstall(&installed).expect("uninstall should succeed");
        assert!(
            installed.symlink_metadata().is_err(),
            "installed copy should be gone after uninstall"
        );
    }
}

// Confirmed to actually *compile* against the real `windows` 0.62 crate by
// CI (see .github/workflows/build.yml) on a real Windows runner — it
// caught two API-signature mismatches (SendMessageTimeoutW's wparam/lparam
// aren't `Option`, they're `WPARAM`/`LPARAM`; AddFontResourceExW's flags
// argument is the `FONT_RESOURCE_CHARACTERISTICS` value itself, not its
// inner `.0`), now fixed. Compiling is not the same as verified correct at
// runtime, though — nobody has confirmed this actually activates a font on
// a real Windows machine yet.
#[cfg(target_os = "windows")]
mod platform {
    use super::managed_dir;
    use anyhow::{bail, Context, Result};
    use std::path::{Path, PathBuf};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{AddFontResourceExW, RemoveFontResourceExW, FR_PRIVATE};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_FONTCHANGE,
    };

    pub fn user_font_dir() -> Option<PathBuf> {
        dirs::data_local_dir().map(|dir| dir.join("Microsoft").join("Windows").join("Fonts"))
    }

    fn wide_null(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn broadcast_font_change() {
        unsafe {
            let _ = SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_FONTCHANGE,
                WPARAM(0),
                LPARAM(0),
                SMTO_ABORTIFHUNG,
                1000,
                None,
            );
        }
    }

    /// Windows needs the font file physically present under the per-user
    /// Fonts folder (no admin rights required there, unlike `C:\Windows\Fonts`),
    /// then registered for the running session via `AddFontResourceExW`.
    pub fn install(source: &Path, unique_name: &str) -> Result<PathBuf> {
        let dir = managed_dir()?;
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("ttf");
        let dest = dir.join(format!("{unique_name}.{ext}"));

        std::fs::copy(source, &dest)
            .with_context(|| format!("failed to copy {source:?} to {dest:?}"))?;

        let wide = wide_null(&dest);
        let added = unsafe { AddFontResourceExW(PCWSTR(wide.as_ptr()), FR_PRIVATE, None) };
        if added == 0 {
            bail!("AddFontResourceExW failed for {dest:?}");
        }
        broadcast_font_change();

        Ok(dest)
    }

    pub fn uninstall(installed: &Path) -> Result<()> {
        let wide = wide_null(installed);
        unsafe {
            let _ = RemoveFontResourceExW(PCWSTR(wide.as_ptr()), FR_PRIVATE.0, None);
        }
        broadcast_font_change();

        if installed.exists() {
            std::fs::remove_file(installed)
                .with_context(|| format!("failed to remove {installed:?}"))?;
        }
        Ok(())
    }
}
