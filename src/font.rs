use std::path::Path;

/// Parsed metadata for a single font face, cheap enough to store for 20k+ entries.
#[derive(Debug, Clone)]
pub struct FontRecord {
    pub path: String,
    pub face_index: u32,
    pub family: String,
    pub subfamily: String,
    /// The `name` table's PostScript name (ID 6) — how Illustrator and
    /// other Adobe apps identify a font in a document. Empty if the face
    /// has none.
    pub postscript_name: String,
    /// Every character the face has a real glyph for, as sorted inclusive
    /// ranges (see `coverage`). Filled in by scanning; `None` when read
    /// back from the catalog, which keeps it in its own column instead.
    pub coverage: Option<Vec<(u32, u32)>>,
    pub weight: u16,
    pub italic: bool,
    pub monospace: bool,
    pub mtime: i64,
    pub size: i64,
    /// When the file arrived in its folder (see `added`); what "Date
    /// Added" sorts by. Starts as `mtime` until a scan reads the real date.
    pub added: i64,
}

/// Extracts one [`FontRecord`] per face found in `data` (a file may be a single
/// font or a TrueType/OpenType collection with several faces bundled together).
pub fn parse_faces(data: &[u8], path: &Path, mtime: i64, size: i64) -> Vec<FontRecord> {
    let face_count = ttf_parser::fonts_in_collection(data).unwrap_or(1);
    let path_str = path.to_string_lossy().into_owned();

    (0..face_count)
        .filter_map(|index| {
            let face = ttf_parser::Face::parse(data, index).ok()?;
            let family = best_name(&face, 16, 1).unwrap_or_else(|| "Unknown".to_string());
            let subfamily = best_name(&face, 17, 2).unwrap_or_else(|| "Regular".to_string());
            let postscript_name = postscript_name(&face).unwrap_or_default();
            let coverage = crate::coverage::to_ranges(crate::font_info::real_codepoints(&face));

            Some(FontRecord {
                path: path_str.clone(),
                face_index: index,
                family,
                subfamily,
                postscript_name,
                coverage: Some(coverage),
                weight: face.weight().to_number(),
                italic: matches!(
                    face.style(),
                    ttf_parser::Style::Italic | ttf_parser::Style::Oblique
                ),
                monospace: face.is_monospaced(),
                mtime,
                size,
                added: mtime,
            })
        })
        .collect()
}

/// The first face's full name (`name` ID 4), e.g. "Helvetica Neue Bold".
pub fn full_name(data: &[u8]) -> Option<String> {
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    best_name(&face, 4, 4).map(|n| n.trim().to_string()).filter(|n| !n.is_empty())
}

/// Prefers the typographic/preferred name entry, falling back to the legacy one,
/// and prefers a Unicode/Windows platform entry (which decodes cleanly as UTF-16).
fn best_name(face: &ttf_parser::Face, preferred_id: u16, fallback_id: u16) -> Option<String> {
    let mut fallback = None;

    for name in face.names() {
        if name.name_id != preferred_id && name.name_id != fallback_id {
            continue;
        }
        let Some(text) = name.to_string() else {
            continue;
        };
        if name.name_id == preferred_id && name.is_unicode() {
            return Some(text);
        }
        if fallback.is_none() {
            fallback = Some(text);
        }
    }

    fallback
}

/// Name ID 6. PostScript names are printable ASCII by spec, so a
/// Macintosh-platform entry (which `ttf_parser` won't decode) is read as
/// raw bytes rather than skipped — some older fonts only have that one.
fn postscript_name(face: &ttf_parser::Face) -> Option<String> {
    let mut mac = None;
    for name in face.names() {
        if name.name_id != ttf_parser::name_id::POST_SCRIPT_NAME {
            continue;
        }
        if let Some(text) = name.to_string() {
            let text = text.trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        } else if mac.is_none() && name.name.is_ascii() {
            mac = Some(String::from_utf8_lossy(name.name).trim().to_string());
        }
    }
    mac.filter(|n| !n.is_empty())
}

/// A human label for the list view's Format column, derived from the file
/// extension alone — true outline-format detection (TrueType `glyf` vs.
/// PostScript `CFF `/`CFF2` tables) would need re-parsing the font, but the
/// extension already tracks that distinction closely enough in practice
/// (almost every real-world `.otf` is CFF-outline, almost every `.ttf` is
/// `glyf`) to show here without the extra parse cost on every list render.
pub fn format_label(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("ttf") => "TrueType",
        Some("otf") => "OpenType PostScript",
        Some("ttc") => "TrueType Collection",
        Some("otc") => "OpenType Collection",
        _ => "Unknown",
    }
}

pub const FONT_EXTENSIONS: &[&str] = &["ttf", "otf", "ttc", "otc"];

pub fn has_font_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| FONT_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext)))
        .unwrap_or(false)
}

/// Whether `path` lives under an OS-managed font directory rather than one
/// the user pointed the catalog at themselves — the "System" sidebar filter
/// is this, not which folder a font happened to be scanned from, so it
/// still applies if a user manually adds `/System/Library/Fonts` as a
/// library.
pub fn is_system_path(path: &str) -> bool {
    #[cfg(target_os = "macos")]
    {
        path.starts_with("/System/Library/Fonts") || path.starts_with("/Library/Fonts")
    }
    #[cfg(target_os = "windows")]
    {
        let lower = path.to_ascii_lowercase();
        lower.starts_with(r"c:\windows\fonts")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = path;
        false
    }
}
