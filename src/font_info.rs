//! Everything the detail panel's Sizes/Glyphs/Info tabs need beyond what
//! [`crate::font::FontRecord`] already stores — computed on demand from the
//! font's own bytes (re-read the same way `App::preview_family` already
//! does) rather than added to the `fonts` table, since none of it is
//! needed outside the one selected font's own detail panel.

use std::collections::{BTreeSet, HashSet};

use ttf_parser::Face;

/// Every codepoint `data` actually has a real glyph for (not `.notdef`) —
/// the lightweight half of `extract`, used wherever only glyph coverage
/// matters (e.g. `App::covers_sample`'s "does this font actually have the
/// tile preview's characters" check) and re-parsing the whole `name`
/// table + collecting into a sorted `Vec` would be wasted work.
pub fn codepoint_set(data: &[u8]) -> Option<HashSet<u32>> {
    let face = Face::parse(data, 0).ok()?;
    Some(real_codepoints(&face))
}

fn real_codepoints(face: &Face) -> HashSet<u32> {
    let mut codepoints = HashSet::new();
    if let Some(cmap) = face.tables().cmap {
        for subtable in cmap.subtables {
            subtable.codepoints(|cp| {
                if char::from_u32(cp).is_some_and(|ch| face.glyph_index(ch).is_some_and(|g| g.0 != 0)) {
                    codepoints.insert(cp);
                }
            });
        }
    }
    codepoints
}

pub struct FontInfo {
    pub full_name: String,
    pub family: String,
    pub style: String,
    pub format: &'static str,
    pub postscript_name: Option<String>,
    pub unique_name: Option<String>,
    pub version: Option<String>,
    pub is_variable: bool,
    pub glyph_count: u16,
    pub copyright: Option<String>,
    pub trademark: Option<String>,
    pub designer: Option<String>,
    pub manufacturer: Option<String>,
    pub license_description: Option<String>,
    /// Every codepoint the font actually maps a glyph to (unioned across
    /// every `cmap` subtable), sorted — the Glyphs tab's whole data set.
    pub codepoints: Vec<u32>,
}

/// Prefers the typographic/preferred name entry (16/17), falling back to
/// the legacy one (1/2) — same preference order `font::best_name` already
/// uses for family/subfamily, extended here to every other name ID the
/// Info tab shows.
fn name_for(face: &Face, preferred_id: u16, fallback_id: u16) -> Option<String> {
    let mut fallback = None;
    for name in face.names() {
        if name.name_id != preferred_id && name.name_id != fallback_id {
            continue;
        }
        let Some(text) = name.to_string() else { continue };
        if name.name_id == preferred_id && name.is_unicode() {
            return Some(text);
        }
        if fallback.is_none() {
            fallback = Some(text);
        }
    }
    fallback
}

/// Same as `name_for` but for a name ID with no fallback (copyright,
/// trademark, designer, manufacturer, license, PostScript name, unique
/// name, version) — those only ever appear under one ID.
fn name_only(face: &Face, id: u16) -> Option<String> {
    name_for(face, id, id)
}

pub fn extract(data: &[u8], path: &str) -> Option<FontInfo> {
    let face = Face::parse(data, 0).ok()?;

    let family = name_for(&face, 16, 1).unwrap_or_else(|| "Unknown".to_string());
    let style = name_for(&face, 17, 2).unwrap_or_else(|| "Regular".to_string());
    let full_name = name_for(&face, 4, 4).unwrap_or_else(|| format!("{family} {style}"));

    // A font's cmap can list codepoints it doesn't actually have a real
    // glyph for (mapped to `.notdef`, glyph 0 — control characters and
    // other compatibility entries are common culprits) — `real_codepoints`
    // checks `glyph_index` and drops anything that resolves to `.notdef`,
    // which is what keeps the Glyphs tab from showing empty/tofu boxes.
    let codepoints: BTreeSet<u32> = real_codepoints(&face).into_iter().collect();

    Some(FontInfo {
        full_name,
        family,
        style,
        format: crate::font::format_label(path),
        postscript_name: name_only(&face, 6),
        unique_name: name_only(&face, 3),
        version: name_only(&face, 5),
        is_variable: face.is_variable(),
        glyph_count: face.number_of_glyphs(),
        copyright: name_only(&face, 0),
        trademark: name_only(&face, 7),
        designer: name_only(&face, 9),
        manufacturer: name_only(&face, 8),
        license_description: name_only(&face, 13),
        codepoints: codepoints.into_iter().collect(),
    })
}

/// A Unicode block's display name and inclusive `[start, end]` codepoint
/// range — just the blocks that actually turn up in real-world Latin-
/// script type foundry fonts, not the full ~300-block Unicode registry.
/// Anything outside all of these falls into the Glyphs tab's "Other"
/// bucket rather than being silently dropped.
pub const UNICODE_BLOCKS: &[(&str, u32, u32)] = &[
    ("Basic Latin", 0x0000, 0x007F),
    ("Latin-1 Supplement", 0x0080, 0x00FF),
    ("Latin Extended-A", 0x0100, 0x017F),
    ("Latin Extended-B", 0x0180, 0x024F),
    ("IPA Extensions", 0x0250, 0x02AF),
    ("Spacing Modifier Letters", 0x02B0, 0x02FF),
    ("Combining Diacritical Marks", 0x0300, 0x036F),
    ("Greek and Coptic", 0x0370, 0x03FF),
    ("Cyrillic", 0x0400, 0x04FF),
    ("Latin Extended Additional", 0x1E00, 0x1EFF),
    ("Greek Extended", 0x1F00, 0x1FFF),
    ("General Punctuation", 0x2000, 0x206F),
    ("Superscripts and Subscripts", 0x2070, 0x209F),
    ("Currency Symbols", 0x20A0, 0x20CF),
    ("Letterlike Symbols", 0x2100, 0x214F),
    ("Number Forms", 0x2150, 0x218F),
    ("Arrows", 0x2190, 0x21FF),
    ("Mathematical Operators", 0x2200, 0x22FF),
    ("Miscellaneous Technical", 0x2300, 0x23FF),
    ("Box Drawing", 0x2500, 0x257F),
    ("Geometric Shapes", 0x25A0, 0x25FF),
    ("Miscellaneous Symbols", 0x2600, 0x26FF),
    ("Dingbats", 0x2700, 0x27BF),
    ("Latin Extended-C", 0x2C60, 0x2C7F),
    ("Latin Extended-D", 0xA720, 0xA7FF),
    ("Alphabetic Presentation Forms", 0xFB00, 0xFB4F),
    ("Private Use Area", 0xE000, 0xF8FF),
];

pub const OTHER_BLOCK: &str = "Other";

/// Which of `UNICODE_BLOCKS` (or `OTHER_BLOCK`) `cp` falls into.
pub fn block_for(cp: u32) -> &'static str {
    UNICODE_BLOCKS
        .iter()
        .find(|(_, start, end)| (*start..=*end).contains(&cp))
        .map(|(name, ..)| *name)
        .unwrap_or(OTHER_BLOCK)
}
