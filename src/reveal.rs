//! Cross-platform "reveal in the OS file browser" for a library folder's
//! context menu — Finder on macOS, Explorer on Windows.

use std::path::Path;

#[cfg(target_os = "macos")]
pub const LABEL: &str = "Show in Finder";
#[cfg(target_os = "windows")]
pub const LABEL: &str = "Show in Explorer";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const LABEL: &str = "Show in File Manager";

pub fn reveal(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer").arg(path).spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        std::process::Command::new("xdg-open").arg(path).spawn()?;
    }
    Ok(())
}

/// Selects `path` (a single font *file*, not a library folder) in the OS
/// file browser — `reveal` above opens a folder itself, which isn't what
/// "Show in Finder"/"Show in Explorer" on a specific file means.
pub fn reveal_file(path: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open").arg("-R").arg(path).spawn()?;
    }
    #[cfg(target_os = "windows")]
    {
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path);
        std::process::Command::new("explorer").arg(arg).spawn()?;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // No universal "select in file manager" verb on Linux — falls
        // back to just opening the containing folder.
        if let Some(parent) = path.parent() {
            std::process::Command::new("xdg-open").arg(parent).spawn()?;
        }
    }
    Ok(())
}
