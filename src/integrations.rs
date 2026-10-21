//! Settings ▸ Integrations: installing GlyphClub's Illustrator plug-in (see
//! `plugin/` and `illustrator_bridge`) into each Illustrator on this Mac.
//!
//! The plug-in ships inside GlyphClub.app (`Contents/Resources/GlyphClub.aip`,
//! put there by `scripts/package-macos.sh`); a `cargo run` build uses
//! `plugin/build/GlyphClub.aip` from `make -C plugin` instead. Illustrator's
//! Plug-ins folder is owned by root, so installing usually goes through the
//! standard macOS administrator prompt.

use std::path::{Path, PathBuf};

/// The plug-in is built against the Illustrator 2026 SDK.
pub const MIN_ILLUSTRATOR_YEAR: u32 = 2026;

const PLUGIN_BUNDLE: &str = "GlyphClub.aip";

#[derive(Clone)]
pub struct IllustratorInstall {
    /// "Illustrator 2026"
    pub label: String,
    pub year: Option<u32>,
    pub plugins_dir: PathBuf,
    /// The installed plug-in's version, if it's installed.
    pub installed_version: Option<String>,
}

impl IllustratorInstall {
    pub fn supported(&self) -> bool {
        self.year.is_some_and(|year| year >= MIN_ILLUSTRATOR_YEAR)
    }
}

/// Where this build's copy of the plug-in is, if it has one.
pub fn bundled_plugin() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // GlyphClub.app/Contents/MacOS/GlyphClub -> Contents/Resources
    let in_app = exe.parent()?.parent()?.join("Resources").join(PLUGIN_BUNDLE);
    if in_app.is_dir() {
        return Some(in_app);
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("plugin")
        .join("build")
        .join(PLUGIN_BUNDLE);
    dev.is_dir().then_some(dev)
}

/// `CFBundleShortVersionString` from a plug-in bundle's Info.plist.
pub fn plugin_version(bundle: &Path) -> Option<String> {
    let plist = std::fs::read_to_string(bundle.join("Contents").join("Info.plist")).ok()?;
    let key = plist.find("<key>CFBundleShortVersionString</key>")?;
    let rest = &plist[key..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")? + start;
    Some(rest[start..end].trim().to_string())
}

/// Every Illustrator in /Applications, newest first.
pub fn find_illustrators() -> Vec<IllustratorInstall> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/Applications") else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name.strip_prefix("Adobe Illustrator") else {
            continue;
        };
        let dir = entry.path();
        // Localized installs keep the `.localized` suffix on disk.
        let plugins_dir = ["Plug-ins.localized", "Plug-ins"]
            .iter()
            .map(|sub| dir.join(sub))
            .find(|p| p.is_dir());
        let Some(plugins_dir) = plugins_dir else {
            continue;
        };
        let year = rest.trim().split_whitespace().next().and_then(|y| y.parse().ok());
        let installed = plugins_dir.join(PLUGIN_BUNDLE);
        found.push(IllustratorInstall {
            label: format!("Illustrator{rest}"),
            year,
            installed_version: installed.is_dir().then(|| plugin_version(&installed).unwrap_or_default()),
            plugins_dir,
        });
    }
    found.sort_by(|a, b| b.year.cmp(&a.year));
    found
}

pub fn illustrator_running() -> bool {
    std::process::Command::new("/usr/bin/pgrep")
        .args(["-x", "Adobe Illustrator"])
        .output()
        .is_ok_and(|out| out.status.success())
}

/// Copies `source` into `plugins_dir`, replacing any older copy. Blocks
/// (including on the administrator prompt), so call it off the main thread.
///
/// The bundle is staged in the temp folder first, as this user: the
/// administrator copy runs as root through a helper that doesn't inherit
/// this app's privacy permissions, so it can't read a source in a
/// protected folder like ~/Documents (a `cargo run` build's
/// plugin/build/) — "Operation not permitted".
pub fn install(source: &Path, plugins_dir: &Path) -> Result<(), String> {
    let stage = std::env::temp_dir().join(format!("glyphclub-plugin-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&stage);
    std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    let staged = stage.join(PLUGIN_BUNDLE);
    let copied = std::process::Command::new("/usr/bin/ditto")
        .arg(source)
        .arg(&staged)
        .status()
        .map_err(|e| e.to_string())?;
    if !copied.success() {
        let _ = std::fs::remove_dir_all(&stage);
        return Err("Couldn't copy the plug-in out of GlyphClub".to_string());
    }

    let dest = plugins_dir.join(PLUGIN_BUNDLE);
    let result = run_privileged_if_needed(
        plugins_dir,
        &format!(
            "/bin/rm -rf {dest} && /usr/bin/ditto {src} {dest}",
            dest = shell_quote(&dest),
            src = shell_quote(&staged),
        ),
    );
    let _ = std::fs::remove_dir_all(&stage);
    result
}

pub fn uninstall(plugins_dir: &Path) -> Result<(), String> {
    let dest = plugins_dir.join(PLUGIN_BUNDLE);
    run_privileged_if_needed(plugins_dir, &format!("/bin/rm -rf {}", shell_quote(&dest)))
}

/// Runs `script` directly when `dir` is writable by this user (e.g. after
/// someone chowned it), otherwise through macOS's own administrator prompt
/// via `do shell script … with administrator privileges`.
fn run_privileged_if_needed(dir: &Path, script: &str) -> Result<(), String> {
    let writable = tempfile_probe(dir);
    let output = if writable {
        std::process::Command::new("/bin/sh").args(["-c", script]).output()
    } else {
        let applescript = format!(
            "do shell script \"{}\" with prompt \"GlyphClub wants to install its plug-in into Adobe Illustrator.\" with administrator privileges",
            applescript_escape(script)
        );
        std::process::Command::new("/usr/bin/osascript")
            .args(["-e", &applescript])
            .output()
    }
    .map_err(|e| e.to_string())?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    // osascript's "User canceled." (-128)
    if stderr.contains("-128") {
        return Err("Cancelled".to_string());
    }
    Err(readable_error(&stderr))
}

/// osascript reports "0:375: execution error: <message> (1)"; keep just
/// the message.
fn readable_error(stderr: &str) -> String {
    let message = stderr.trim();
    let message = message
        .split_once("execution error: ")
        .map_or(message, |(_, rest)| rest);
    let message = message
        .rsplit_once(" (")
        .filter(|(_, code)| code.trim_end_matches(')').parse::<i32>().is_ok())
        .map_or(message, |(text, _)| text);
    format!("Couldn't install: {message}")
}

fn tempfile_probe(dir: &Path) -> bool {
    let probe = dir.join(".glyphclub-write-test");
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
}

fn applescript_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_version_out_of_a_plugin_info_plist() {
        let dir = tempfile::tempdir().unwrap();
        let contents = dir.path().join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            "<dict>\n\t<key>CFBundleShortVersionString</key>\n\t<string>0.1.2</string>\n</dict>",
        )
        .unwrap();
        assert_eq!(plugin_version(dir.path()).as_deref(), Some("0.1.2"));
    }

    #[test]
    fn quoting_survives_spaces_and_apostrophes() {
        let quoted = shell_quote(Path::new("/Applications/Adobe Illustrator 2026/Tony's"));
        assert_eq!(quoted, r"'/Applications/Adobe Illustrator 2026/Tony'\''s'");
        assert_eq!(applescript_escape(r#"a "b" \c"#), r#"a \"b\" \\c"#);
    }

    #[test]
    fn osascript_errors_are_trimmed_to_the_message() {
        assert_eq!(
            readable_error("0:375: execution error: ditto: /x/GlyphClub.aip: Operation not permitted (1)\n"),
            "Couldn't install: ditto: /x/GlyphClub.aip: Operation not permitted"
        );
    }

    #[test]
    fn installs_and_uninstalls_into_a_writable_folder() {
        let source = tempfile::tempdir().unwrap();
        let bundle = source.path().join(PLUGIN_BUNDLE);
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(bundle.join("Contents").join("Info.plist"), "x").unwrap();
        let plugins = tempfile::tempdir().unwrap();

        install(&bundle, plugins.path()).unwrap();
        assert!(plugins.path().join(PLUGIN_BUNDLE).join("Contents/Info.plist").is_file());
        uninstall(plugins.path()).unwrap();
        assert!(!plugins.path().join(PLUGIN_BUNDLE).exists());
    }
}
