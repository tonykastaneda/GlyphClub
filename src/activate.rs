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
    platform::install(source, unique_name)
}

/// Removes a previously installed font. `installed` is the path returned by
/// `install`.
pub fn uninstall(installed: &Path) -> Result<()> {
    platform::uninstall(installed)
}

fn managed_dir() -> Result<PathBuf> {
    let base = platform::user_font_dir().context("could not determine the user font directory")?;
    let dir = base.join("glyphclub");
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

    /// macOS watches `~/Library/Fonts` (via `fontd`), so a symlink dropped
    /// there is enough to activate a font without copying the file.
    pub fn install(source: &Path, unique_name: &str) -> Result<PathBuf> {
        let dir = managed_dir()?;
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("ttf");
        let dest = dir.join(format!("{unique_name}.{ext}"));

        if dest.symlink_metadata().is_ok() {
            std::fs::remove_file(&dest).ok();
        }
        std::os::unix::fs::symlink(source, &dest)
            .with_context(|| format!("failed to symlink {source:?} into {dest:?}"))?;

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
    fn installs_and_uninstalls_a_symlink() {
        let source = Path::new("/System/Library/Fonts/Helvetica.ttc");
        let installed = install(source, "glyphclub-activation-test").expect("install should succeed");

        assert!(
            installed.symlink_metadata().is_ok(),
            "expected a symlink at {installed:?}"
        );
        let target = std::fs::read_link(&installed).expect("should be a symlink");
        assert_eq!(target, source);

        uninstall(&installed).expect("uninstall should succeed");
        assert!(
            installed.symlink_metadata().is_err(),
            "symlink should be gone after uninstall"
        );
    }
}

// Unverified: written against the stable Win32 font-resource API, but there
// is no Windows machine in this project's dev loop to actually compile or
// run it against. Treat this path as best-effort until someone confirms it
// on real Windows.
#[cfg(target_os = "windows")]
mod platform {
    use super::managed_dir;
    use anyhow::{bail, Context, Result};
    use std::path::{Path, PathBuf};
    use windows::core::PCWSTR;
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
                None,
                None,
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
        let added = unsafe { AddFontResourceExW(PCWSTR(wide.as_ptr()), FR_PRIVATE.0, None) };
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
