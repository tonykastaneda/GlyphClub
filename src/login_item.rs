//! Settings ▸ General ▸ Start on Login.
//!
//! Packaged macOS 13+ apps use ServiceManagement; development builds and
//! macOS 12 retain the LaunchAgent fallback. Windows uses the Run key.

use std::path::PathBuf;

/// Registers or unregisters GlyphClub to start at login.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    platform::set_enabled(enabled)
}

/// Refresh only registrations that depend on the executable's current path.
pub fn refresh() {
    if !uses_native_service() {
        let _ = platform::set_enabled(true);
    }
}

pub fn uses_native_service() -> bool {
    #[cfg(target_os = "macos")]
    return platform::use_sm_app_service();
    #[cfg(not(target_os = "macos"))]
    false
}

/// Read macOS's actual selection, including changes made in System Settings.
#[cfg(target_os = "macos")]
pub fn is_enabled() -> bool {
    platform::is_enabled()
}

pub fn launched_at_login() -> bool {
    #[cfg(target_os = "macos")]
    return platform::launched_at_login();
    #[cfg(not(target_os = "macos"))]
    false
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
        Ok(home
            .join("Library/LaunchAgents")
            .join(format!("{LABEL}.plist")))
    }

    fn bundled_app() -> Option<PathBuf> {
        super::current_exe()
            .ok()?
            .ancestors()
            .nth(3)
            .filter(|p| p.extension().is_some_and(|e| e == "app"))
            .map(PathBuf::from)
    }

    pub fn use_sm_app_service() -> bool {
        use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
        bundled_app().is_some()
            && NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(
                NSOperatingSystemVersion {
                    majorVersion: 13,
                    minorVersion: 0,
                    patchVersion: 0,
                },
            )
    }

    fn remove_legacy_agent() -> Result<(), String> {
        let path = agent_path()?;
        // Deleting the file does not unload an already bootstrapped job.
        let uid = std::process::Command::new("/usr/bin/id")
            .arg("-u")
            .output()
            .map_err(|e| e.to_string())?;
        if !uid.status.success() {
            return Err("could not determine login user".into());
        }
        let target = format!(
            "gui/{}/{}",
            String::from_utf8_lossy(&uid.stdout).trim(),
            LABEL
        );
        // Only boot out an existing job. Preserve unexpected unload failures
        // so migration can retry rather than silently leave two login starts.
        let loaded = std::process::Command::new("/bin/launchctl")
            .args(["print", &target])
            .output()
            .map_err(|e| e.to_string())?;
        if loaded.status.success() {
            let output = std::process::Command::new("/bin/launchctl")
                .args(["bootout", &target])
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
            }
        }
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn set_enabled(enabled: bool) -> Result<(), String> {
        use objc2_service_management::{SMAppService, SMAppServiceStatus};
        if !use_sm_app_service() {
            return legacy_set_enabled(enabled);
        }
        // All SMAppService calls are guarded by the macOS 13 availability check.
        unsafe {
            let service = SMAppService::mainAppService();
            let status = service.status();
            let result = if enabled {
                if status == SMAppServiceStatus::Enabled {
                    Ok(())
                } else {
                    service.registerAndReturnError()
                }
            } else if status == SMAppServiceStatus::NotRegistered {
                Ok(())
            } else {
                service.unregisterAndReturnError()
            };
            if enabled && service.status() == SMAppServiceStatus::RequiresApproval {
                SMAppService::openSystemSettingsLoginItems();
                // Registration exists but needs consent. Finish migration without
                // retrying registration (or reopening Settings) on every launch.
                return remove_legacy_agent();
            }
            // Preserve the old registration if registering the replacement failed.
            result.map_err(|e| e.localizedDescription().to_string())?;
        }
        remove_legacy_agent()
    }

    pub fn is_enabled() -> bool {
        use objc2_service_management::{SMAppService, SMAppServiceStatus};
        if use_sm_app_service() {
            unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
        } else {
            agent_path().is_ok_and(|p| p.exists())
        }
    }

    pub fn launched_at_login() -> bool {
        use objc2_foundation::NSAppleEventManager;
        // AERegistry.h: kCoreEventClass, kAEOpenApplication, keyAEPropData,
        // keyAELaunchedAsLogInItem. Read while AppKit handles the launch event.
        NSAppleEventManager::sharedAppleEventManager()
            .currentAppleEvent()
            .is_some_and(|e| is_login_event(&e))
    }

    fn is_login_event(event: &objc2_foundation::NSAppleEventDescriptor) -> bool {
        Some(event)
            .filter(|e| {
                e.eventClass() == u32::from_be_bytes(*b"aevt")
                    && e.eventID() == u32::from_be_bytes(*b"oapp")
            })
            .and_then(|e| e.paramDescriptorForKeyword(u32::from_be_bytes(*b"prdt")))
            .is_some_and(|p| p.enumCodeValue() == u32::from_be_bytes(*b"lgit"))
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use objc2_foundation::NSAppleEventDescriptor as Descriptor;

        fn open_event(id: [u8; 4]) -> objc2::rc::Retained<Descriptor> {
            Descriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
                u32::from_be_bytes(*b"aevt"),
                u32::from_be_bytes(id),
                None,
                -1,
                0,
            )
        }

        #[test]
        fn recognizes_login_launch_but_not_manual_launch_or_reopen() {
            objc2::rc::autoreleasepool(|_| {
                let event = open_event(*b"oapp");
                assert!(!is_login_event(&event));
                let login = Descriptor::descriptorWithEnumCode(u32::from_be_bytes(*b"lgit"));
                event.setParamDescriptor_forKeyword(&login, u32::from_be_bytes(*b"prdt"));
                assert!(is_login_event(&event));
                let reopen = open_event(*b"rapp");
                reopen.setParamDescriptor_forKeyword(&login, u32::from_be_bytes(*b"prdt"));
                assert!(!is_login_event(&reopen));
            });
        }
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    fn legacy_set_enabled(enabled: bool) -> Result<(), String> {
        let path = agent_path()?;
        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(e.to_string()),
            };
        }
        let exe = super::current_exe()?;
        let app = bundled_app();
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
