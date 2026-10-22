//! Settings ▸ General ▸ Start on Login.
//!
//! macOS: a LaunchAgent in `~/Library/LaunchAgents` (works for a
//! `cargo run` build too, unlike `SMAppService`, which needs the .app). It
//! launches GlyphClub.app through `open` when there is one, so it starts
//! like any other app. Windows: the per-user `Run` registry key.
//!
//! Whether GlyphClub then shows its window or starts silently is the Menu
//! Bar / System Tray setting's call, not this one's.

use std::path::PathBuf;

/// Registers or unregisters GlyphClub to start at login.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    platform::set_enabled(enabled)
}

/// Rewrites the registration with this copy's current location — run at
/// every launch while the setting is on, so moving or updating GlyphClub
/// doesn't leave login pointing at an old path.
pub fn refresh() {
    let _ = platform::set_enabled(true);
}

fn current_exe() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
mod platform {
    use std::path::PathBuf;

    const LABEL: &str = "com.tonykastaneda.glyphclub";

    fn agent_path() -> Result<PathBuf, String> {
        let home = dirs::home_dir().ok_or("no home folder")?;
        Ok(home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let path = agent_path()?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            };
        }
        let exe = super::current_exe()?;
        // GlyphClub.app/Contents/MacOS/GlyphClub -> GlyphClub.app
        let app = exe
            .ancestors()
            .nth(3)
            .filter(|p| p.extension().is_some_and(|e| e == "app"));
        let arguments: Vec<String> = match app {
            // --background: a login start stays in the menu bar.
            Some(app) => vec![
                "/usr/bin/open".into(),
                "-a".into(),
                app.to_string_lossy().into_owned(),
                "--args".into(),
                "--background".into(),
            ],
            None => vec![exe.to_string_lossy().into_owned(), "--background".into()],
        };
        let arguments: String = arguments
            .iter()
            .map(|a| format!("\t\t<string>{}</string>\n", xml_escape(a)))
            .collect();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
{arguments}	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>ProcessType</key>
	<string>Interactive</string>
</dict>
</plist>
"#
        );
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&path, plist).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use std::os::windows::process::CommandExt;

    const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
    /// Keeps `reg.exe` from flashing a console window.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        let mut command = std::process::Command::new("reg");
        if enabled {
            let exe = super::current_exe()?;
            command.args([
                "add",
                RUN_KEY,
                "/v",
                "GlyphClub",
                "/t",
                "REG_SZ",
                "/d",
                // --background: a login start stays in the tray.
                &format!("\"{}\" --background", exe.display()),
                "/f",
            ]);
        } else {
            command.args(["delete", RUN_KEY, "/v", "GlyphClub", "/f"]);
        }
        let output = command
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        // Deleting a value that isn't there is fine.
        if output.status.success() || !enabled {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn set_enabled(_enabled: bool) -> Result<(), String> {
        Err("Start on Login isn't supported on this platform".to_string())
    }
}
