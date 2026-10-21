//! When a font file arrived in its folder: what "Date Added" sorts by.
//!
//! A font's own modified date is usually when the foundry made it (years
//! ago), so a font you dropped in today would sort among old ones. macOS
//! records the moment a file was added to its folder (Finder's "Date Added"
//! column); elsewhere the file's creation time is the closest thing.

use std::path::Path;

/// Unix seconds, or `None` when the system doesn't know.
pub fn added_time(path: &Path) -> Option<i64> {
    #[cfg(target_os = "macos")]
    {
        mac::added_to_directory(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let created = std::fs::metadata(path).ok()?.created().ok()?;
        Some(created.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64)
    }
}

#[cfg(target_os = "macos")]
mod mac {
    use std::path::Path;

    use objc2::rc::Retained;
    use objc2::runtime::AnyObject;
    use objc2_foundation::{NSDate, NSString, NSURLAddedToDirectoryDateKey, NSURL};

    pub fn added_to_directory(path: &Path) -> Option<i64> {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        let mut value: Option<Retained<AnyObject>> = None;
        // SAFETY: the key is a valid NSURL resource key constant, and `value`
        // is an out-parameter Foundation fills with an NSDate or nothing.
        unsafe {
            url.getResourceValue_forKey_error(&mut value, NSURLAddedToDirectoryDateKey).ok()?;
        }
        let date = value?.downcast::<NSDate>().ok()?;
        Some(date.timeIntervalSince1970() as i64)
    }
}
