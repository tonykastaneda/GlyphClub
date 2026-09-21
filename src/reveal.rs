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
