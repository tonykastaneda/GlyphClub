//! Settings ▸ Auto Activation: installing GlyphClub's plug-ins (see
//! `plugin/`, `plugin-photoshop/`, `plugin-indesign/` and
//! `src/gui/bridge.rs`) into each Adobe Illustrator, Photoshop and InDesign
//! on this computer.
//!
//! macOS: the plug-ins ship inside GlyphClub.app (`Contents/Resources`, put
//! there by `scripts/package-macos.sh`); a `cargo run` build uses each
//! plug-in's own build folder instead. The apps' plug-in folders are owned
//! by root, so installing usually goes through the standard macOS
//! administrator prompt.
//!
//! Windows: each plug-in is a single DLL, shipped next to GlyphClub.exe as
//! `plugins\x64\…` and, for apps with native ARM64 builds,
//! `plugins\arm64\…` (`scripts/package-windows.ps1`); the one matching the
//! app's architecture is installed. The plug-in folders are under Program
//! Files, so installing goes through a UAC prompt.

use std::path::{Path, PathBuf};

/// The plug-ins are built against the 2026 SDKs.
pub const MIN_ILLUSTRATOR_YEAR: u32 = 2026;
pub const MIN_YEAR: u32 = 2026;

#[cfg(not(windows))]
pub use mac::*;
#[cfg(windows)]
pub use win::*;

/// An Adobe app GlyphClub has a plug-in for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HostApp {
    Illustrator,
    Photoshop,
    InDesign,
}

impl HostApp {
    pub const ALL: [HostApp; 3] = [HostApp::Illustrator, HostApp::Photoshop, HostApp::InDesign];

    pub fn name(self) -> &'static str {
        match self {
            HostApp::Illustrator => "Illustrator",
            HostApp::Photoshop => "Photoshop",
            HostApp::InDesign => "InDesign",
        }
    }

    /// The two-letter mark the Settings list shows.
    pub fn badge(self) -> &'static str {
        match self {
            HostApp::Illustrator => "Ai",
            HostApp::Photoshop => "Ps",
            HostApp::InDesign => "Id",
        }
    }

    /// How the app's folder starts: "Adobe Photoshop 2026".
    fn folder_prefix(self) -> &'static str {
        match self {
            HostApp::Illustrator => "Adobe Illustrator",
            HostApp::Photoshop => "Adobe Photoshop",
            HostApp::InDesign => "Adobe InDesign",
        }
    }

    /// The plug-in's name in the app's plug-ins folder.
    pub fn plugin_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (HostApp::Illustrator, _) => "GlyphClub.aip",
            (HostApp::Photoshop, false) => "GlyphClub.plugin",
            (HostApp::Photoshop, true) => "GlyphClub.8li",
            (HostApp::InDesign, false) => "GlyphClub.InDesignPlugin",
            (HostApp::InDesign, true) => "GlyphClub.pln",
        }
    }

    /// The plug-in's name inside GlyphClub (Contents/Resources on macOS,
    /// plugins\<arch> on Windows): distinct, since the macOS bundles share
    /// a folder.
    fn bundled_name(self) -> &'static str {
        match (self, cfg!(windows)) {
            (HostApp::Photoshop, false) => "GlyphClub Photoshop.plugin",
            (HostApp::InDesign, false) => "GlyphClub InDesign.InDesignPlugin",
            _ => self.plugin_name(),
        }
    }

    /// Where a local build of the plug-in lands (`make` / CMake), for
    /// `cargo run` builds.
    fn dev_build(self) -> PathBuf {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        match (self, cfg!(windows)) {
            (HostApp::Illustrator, false) => root.join("plugin").join("build"),
            (HostApp::Photoshop, false) => root.join("plugin-photoshop").join("build"),
            (HostApp::InDesign, false) => root.join("plugin-indesign").join("build"),
            (HostApp::Illustrator, true) => root.join("plugin").join("build-win-x64").join("Release"),
            (HostApp::Photoshop, true) => root.join("plugin-photoshop").join("build-win-x64").join("Release"),
            (HostApp::InDesign, true) => root.join("plugin-indesign").join("build-win-x64").join("Release"),
        }
        .join(self.plugin_name())
    }

    /// The plug-ins folder's name inside the app's folder.
    fn plugins_folder_names(self) -> &'static [&'static str] {
        // Localized macOS installs keep the `.localized` suffix on disk;
        // InDesign spells it Plug-Ins.
        &["Plug-ins.localized", "Plug-ins", "Plug-Ins"]
    }
}

#[derive(Clone)]
pub struct AppInstall {
    pub app: HostApp,
    /// "Illustrator 2026"
    pub label: String,
    pub year: Option<u32>,
    pub plugins_dir: PathBuf,
    /// The installed plug-in's version, if it's installed.
    pub installed_version: Option<String>,
}

/// The name this had when it was only Illustrator.
pub type IllustratorInstall = AppInstall;

impl AppInstall {
    pub fn supported(&self) -> bool {
        self.year.is_some_and(|year| year >= MIN_YEAR)
    }
}

/// Where this build's copy of the Illustrator plug-in is, if it has one.
pub fn bundled_plugin() -> Option<PathBuf> {
    bundled_plugin_for(HostApp::Illustrator)
}

/// Every supported app found, newest first within each app.
pub fn find_installs() -> Vec<AppInstall> {
    let mut found = Vec::new();
    for root in app_roots() {
        let Ok(entries) = std::fs::read_dir(&root) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            for app in HostApp::ALL {
                let Some(rest) = name.strip_prefix(app.folder_prefix()) else { continue };
                let rest = rest.trim_end_matches(".app");
                let Some(plugins_dir) = app.plugins_folder_names().iter().map(|sub| entry.path().join(sub)).find(|p| p.is_dir())
                else {
                    continue;
                };
                if found.iter().any(|f: &AppInstall| f.plugins_dir == plugins_dir) {
                    continue;
                }
                let year = rest.trim().split_whitespace().next().and_then(|y| y.parse().ok());
                let installed = plugins_dir.join(app.plugin_name());
                found.push(AppInstall {
                    app,
                    label: format!("{}{rest}", app.name()),
                    year,
                    installed_version: installed.exists().then(|| plugin_version(&installed).unwrap_or_default()),
                    plugins_dir,
                });
            }
        }
    }
    found.sort_by(|a, b| {
        let order = |x: &AppInstall| HostApp::ALL.iter().position(|h| *h == x.app);
        order(a).cmp(&order(b)).then(b.year.cmp(&a.year))
    });
    found
}

/// Every Illustrator found, newest first.
pub fn find_illustrators() -> Vec<AppInstall> {
    find_installs().into_iter().filter(|i| i.app == HostApp::Illustrator).collect()
}

pub fn illustrator_running() -> bool {
    app_running(HostApp::Illustrator)
}

#[cfg(not(windows))]
fn app_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/Applications")]
}

#[cfg(windows)]
fn app_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|p| PathBuf::from(p).join("Adobe"))
        .collect();
    roots.dedup();
    roots
}

#[cfg(not(windows))]
mod mac {
    use super::*;

    /// Where this build's copy of `app`'s plug-in is, if it has one.
    pub fn bundled_plugin_for(app: HostApp) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        // GlyphClub.app/Contents/MacOS/GlyphClub -> Contents/Resources
        let in_app = exe.parent()?.parent()?.join("Resources").join(app.bundled_name());
        if in_app.is_dir() {
            return Some(in_app);
        }
        let dev = app.dev_build();
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

    pub fn app_running(app: HostApp) -> bool {
        // "Adobe Illustrator", "Adobe Photoshop 2026", "Adobe InDesign 2026".
        std::process::Command::new("/usr/bin/pgrep")
            .args(["-f", &format!("/{}[^/]*\\.app/Contents/MacOS/", app.folder_prefix())])
            .output()
            .is_ok_and(|out| out.status.success())
    }

    /// Copies `source` into `app`'s plug-ins folder, replacing any older
    /// copy. Blocks (including on the administrator prompt), so call it off
    /// the main thread.
    ///
    /// The bundle is staged in the temp folder first, as this user: the
    /// administrator copy runs as root through a helper that doesn't inherit
    /// this app's privacy permissions, so it can't read a source in a
    /// protected folder like ~/Documents (a `cargo run` build's
    /// plugin/build/) — "Operation not permitted".
    pub fn install(app: HostApp, source: &Path, plugins_dir: &Path) -> Result<(), String> {
        let stage = std::env::temp_dir().join(format!("glyphclub-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        let staged = stage.join(app.plugin_name());
        let copied = std::process::Command::new("/usr/bin/ditto")
            .arg(source)
            .arg(&staged)
            .status()
            .map_err(|e| e.to_string())?;
        if !copied.success() {
            let _ = std::fs::remove_dir_all(&stage);
            return Err("Couldn't copy the plug-in out of GlyphClub".to_string());
        }

        let dest = plugins_dir.join(app.plugin_name());
        let result = run_privileged_if_needed(
            app,
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

    pub fn uninstall(app: HostApp, plugins_dir: &Path) -> Result<(), String> {
        let dest = plugins_dir.join(app.plugin_name());
        run_privileged_if_needed(app, plugins_dir, &format!("/bin/rm -rf {}", shell_quote(&dest)))
    }

    /// Runs `script` directly when `dir` is writable by this user (e.g. after
    /// someone chowned it), otherwise through macOS's own administrator prompt
    /// via `do shell script … with administrator privileges`.
    fn run_privileged_if_needed(app: HostApp, dir: &Path, script: &str) -> Result<(), String> {
        let writable = tempfile_probe(dir);
        let output = if writable {
            std::process::Command::new("/bin/sh").args(["-c", script]).output()
        } else {
            let applescript = format!(
                "do shell script \"{}\" with prompt \"GlyphClub wants to install its plug-in into Adobe {}.\" with administrator privileges",
                applescript_escape(script),
                app.name(),
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
    pub(super) fn readable_error(stderr: &str) -> String {
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

    pub(super) fn shell_quote(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', r"'\''"))
    }

    pub(super) fn applescript_escape(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }
}

#[cfg(windows)]
mod win {
    use super::*;
    use std::os::windows::process::CommandExt;

    /// Keeps helper processes (tasklist, PowerShell) from flashing a console.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    /// The x64 copy of `app`'s plug-in this build ships; `plugin_for` finds
    /// its ARM64 sibling.
    pub fn bundled_plugin_for(app: HostApp) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let packaged = exe.parent()?.join("plugins").join("x64").join(app.bundled_name());
        if packaged.is_file() {
            return Some(packaged);
        }
        let dev = app.dev_build();
        dev.is_file().then_some(dev)
    }

    /// The plug-in DLL's ProductVersion.
    pub fn plugin_version(dll: &Path) -> Option<String> {
        pe_product_version(&std::fs::read(dll).ok()?)
    }

    fn exe_name(app: HostApp) -> &'static str {
        match app {
            HostApp::Illustrator => "Illustrator.exe",
            HostApp::Photoshop => "Photoshop.exe",
            HostApp::InDesign => "InDesign.exe",
        }
    }

    pub fn app_running(app: HostApp) -> bool {
        let exe = exe_name(app);
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).to_ascii_lowercase().contains(&exe.to_ascii_lowercase()))
    }

    /// Whether the app owning `plugins_dir` is an ARM64 build.
    fn app_is_arm64(app: HostApp, plugins_dir: &Path) -> bool {
        let exe = plugins_dir.parent().map(|dir| match app {
            HostApp::Illustrator => dir.join("Support Files").join("Contents").join("Windows").join(exe_name(app)),
            _ => dir.join(exe_name(app)),
        });
        exe.and_then(|exe| std::fs::read(exe).ok())
            .and_then(|bytes| pe_machine(&bytes))
            .is_some_and(|machine| machine == 0xAA64)
    }

    /// The copy of the plug-in for this app: `source` is the x64 one, the
    /// ARM64 one sits in the matching folder ("x64" -> "arm64").
    fn plugin_for(app: HostApp, source: &Path, plugins_dir: &Path) -> Result<PathBuf, String> {
        if !app_is_arm64(app, plugins_dir) {
            return Ok(source.to_path_buf());
        }
        let text = source.to_string_lossy();
        let arm = text
            .rfind("x64")
            .map(|at| PathBuf::from(format!("{}arm64{}", &text[..at], &text[at + 3..])))
            .filter(|p| p.is_file());
        arm.ok_or_else(|| format!("This GlyphClub doesn't include the plug-in for ARM64 {}.", app.name()))
    }

    /// Copies the plug-in into `plugins_dir`, replacing any older copy.
    /// Blocks (including on the UAC prompt), so call it off the main thread.
    pub fn install(app: HostApp, source: &Path, plugins_dir: &Path) -> Result<(), String> {
        let source = plugin_for(app, source, plugins_dir)?;
        let dest = plugins_dir.join(app.plugin_name());
        if writable(plugins_dir) {
            return std::fs::copy(&source, &dest).map(|_| ()).map_err(|e| in_use_hint(app, &e.to_string()));
        }
        // The elevated copy runs as an administrator, who may not see this
        // user's folders the same way: stage the file in the temp folder.
        let stage = std::env::temp_dir().join(format!("glyphclub-plugin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&stage);
        std::fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
        let staged = stage.join(app.plugin_name());
        std::fs::copy(&source, &staged).map_err(|e| e.to_string())?;
        let result = run_elevated(&stage, &format!("copy /Y \"{}\" \"{}\"", staged.display(), dest.display()));
        let same = std::fs::read(&dest).ok() == std::fs::read(&staged).ok();
        let _ = std::fs::remove_dir_all(&stage);
        result?;
        if same {
            Ok(())
        } else {
            Err(in_use_hint(app, &format!("Couldn't copy the plug-in into {}'s Plug-ins folder", app.name())))
        }
    }

    pub fn uninstall(app: HostApp, plugins_dir: &Path) -> Result<(), String> {
        let dest = plugins_dir.join(app.plugin_name());
        if writable(plugins_dir) {
            return std::fs::remove_file(&dest).map_err(|e| in_use_hint(app, &e.to_string()));
        }
        let stage = std::env::temp_dir().join(format!("glyphclub-plugin-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&stage);
        let result = run_elevated(&stage, &format!("del /F /Q \"{}\"", dest.display()));
        let _ = std::fs::remove_dir_all(&stage);
        result?;
        if dest.exists() {
            Err(in_use_hint(app, "Couldn't remove the plug-in"))
        } else {
            Ok(())
        }
    }

    /// The app keeps a loaded plug-in open, so it can't be replaced or
    /// removed while the app runs.
    fn in_use_hint(app: HostApp, message: &str) -> String {
        if app_running(app) {
            format!("{message}. Quit {} and try again.", app.name())
        } else {
            message.to_string()
        }
    }

    /// Runs one cmd.exe line as an administrator, through Windows' UAC
    /// prompt, and waits for it.
    fn run_elevated(stage: &Path, command: &str) -> Result<(), String> {
        let script = stage.join("glyphclub-plugin.cmd");
        std::fs::write(&script, format!("@echo off\r\n{command}\r\nexit /b %errorlevel%\r\n")).map_err(|e| e.to_string())?;
        let quoted = script.display().to_string().replace('\'', "''");
        let output = std::process::Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                &format!(
                    "$p = Start-Process -FilePath 'cmd.exe' -ArgumentList '/c \"{quoted}\"' -Verb RunAs -WindowStyle Hidden -Wait -PassThru; exit $p.ExitCode"
                ),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("canceled by the user") || stderr.contains("cancelled by the user") {
            return Err("Cancelled".to_string());
        }
        Err("Couldn't install the plug-in (Windows refused the copy)".to_string())
    }

    fn writable(dir: &Path) -> bool {
        let probe = dir.join(".glyphclub-write-test");
        let ok = std::fs::write(&probe, b"").is_ok();
        let _ = std::fs::remove_file(&probe);
        ok
    }
}

/// A PE file's machine type (0x8664 x64, 0xAA64 ARM64).
#[cfg(any(windows, test))]
fn pe_machine(bytes: &[u8]) -> Option<u16> {
    let at = u32::from_le_bytes(bytes.get(0x3C..0x40)?.try_into().ok()?) as usize;
    if bytes.get(at..at + 4)? != b"PE\0\0" {
        return None;
    }
    Some(u16::from_le_bytes(bytes.get(at + 4..at + 6)?.try_into().ok()?))
}

/// The ProductVersion string from a PE file's version resource: the UTF-16
/// key "ProductVersion", its terminator and padding, then the value.
#[cfg(any(windows, test))]
fn pe_product_version(bytes: &[u8]) -> Option<String> {
    let key: Vec<u8> = "ProductVersion\0".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    let start = bytes.windows(key.len()).position(|w| w == key.as_slice())? + key.len();
    let mut units = bytes[start..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]]));
    let mut value = Vec::new();
    let mut started = false;
    for unit in units.by_ref().take(64) {
        match (unit, started) {
            (0, false) => continue,
            (0, true) => break,
            (u, _) => {
                started = true;
                value.push(u);
            }
        }
    }
    let value = String::from_utf16(&value).ok()?;
    (!value.is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_dll_s_machine_and_product_version() {
        let mut dll = vec![0u8; 0x200];
        dll[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        dll[0x80..0x84].copy_from_slice(b"PE\0\0");
        dll[0x84..0x86].copy_from_slice(&0xAA64u16.to_le_bytes());
        let entry: Vec<u8> = "ProductVersion\0\00.2.0\0".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        dll[0x100..0x100 + entry.len()].copy_from_slice(&entry);
        assert_eq!(pe_machine(&dll), Some(0xAA64));
        assert_eq!(pe_product_version(&dll).as_deref(), Some("0.2.0"));
        assert_eq!(pe_product_version(&[0; 64]), None);
    }

    #[cfg(not(windows))]
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

    #[cfg(not(windows))]
    #[test]
    fn quoting_survives_spaces_and_apostrophes() {
        let quoted = shell_quote(Path::new("/Applications/Adobe Illustrator 2026/Tony's"));
        assert_eq!(quoted, r"'/Applications/Adobe Illustrator 2026/Tony'\''s'");
        assert_eq!(applescript_escape(r#"a "b" \c"#), r#"a \"b\" \\c"#);
    }

    #[cfg(not(windows))]
    #[test]
    fn osascript_errors_are_trimmed_to_the_message() {
        assert_eq!(
            readable_error("0:375: execution error: ditto: /x/GlyphClub.aip: Operation not permitted (1)\n"),
            "Couldn't install: ditto: /x/GlyphClub.aip: Operation not permitted"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn installs_and_uninstalls_into_a_writable_folder() {
        let source = tempfile::tempdir().unwrap();
        let bundle = source.path().join(HostApp::Illustrator.plugin_name());
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(bundle.join("Contents").join("Info.plist"), "x").unwrap();
        let plugins = tempfile::tempdir().unwrap();

        install(HostApp::Illustrator, &bundle, plugins.path()).unwrap();
        assert!(plugins.path().join("GlyphClub.aip").join("Contents/Info.plist").is_file());
        uninstall(HostApp::Illustrator, plugins.path()).unwrap();
        assert!(!plugins.path().join("GlyphClub.aip").exists());
    }
}
