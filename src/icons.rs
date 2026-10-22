//! Icon fonts: the free sets bundled with GlyphClub (see
//! scripts/fetch-icon-fonts.py) plus any icon font dropped into the user's
//! "Icon Fonts" folder, and each icon's outline as SVG, so it can be pasted
//! or dragged into Illustrator as editable vector paths.

use std::path::{Path, PathBuf};

use ttf_parser::{Face, OutlineBuilder};

pub struct IconSet {
    /// Folder name for bundled sets, file name for the user's own.
    pub slug: String,
    pub name: String,
    pub version: String,
    pub license: String,
    pub font_path: PathBuf,
    /// The font's family name, for rendering it once it's loaded.
    pub family: String,
    pub icons: Vec<Icon>,
    pub bundled: bool,
}

#[derive(Clone)]
pub struct Icon {
    pub name: String,
    pub codepoint: u32,
}

/// Where the bundled sets live: inside the app bundle, else the source tree
/// (a `cargo run` build).
pub fn bundled_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    // macOS: GlyphClub.app/Contents/MacOS/GlyphClub -> Contents/Resources.
    if let Some(in_app) = exe.parent().and_then(Path::parent).map(|c| c.join("Resources").join("icon-fonts")) {
        if in_app.is_dir() {
            return Some(in_app);
        }
    }
    // Windows: next to GlyphClub.exe.
    if let Some(beside) = exe.parent().map(|d| d.join("icon-fonts")) {
        if beside.is_dir() {
            return Some(beside);
        }
    }
    let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("icon-fonts");
    dev.is_dir().then_some(dev)
}

/// The user's own icon fonts: any font file dropped in here becomes a set.
pub fn user_dir() -> PathBuf {
    let base = dirs::data_dir().or_else(dirs::data_local_dir).unwrap_or_else(std::env::temp_dir);
    base.join("glyphclub").join("Icon Fonts")
}

/// Every set, bundled first, sorted by name within each group.
pub fn load_sets() -> Vec<IconSet> {
    let mut sets = Vec::new();
    if let Some(dir) = bundled_dir() {
        let mut bundled: Vec<IconSet> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| load_bundled(&e.path()))
            .collect();
        bundled.sort_by_key(|s| s.name.to_lowercase());
        sets.extend(bundled);
    }
    let mut own: Vec<IconSet> = std::fs::read_dir(user_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| crate::font::has_font_extension(p))
        .filter_map(|p| load_user(&p))
        .collect();
    own.sort_by_key(|s| s.name.to_lowercase());
    sets.extend(own);
    sets
}

fn load_bundled(dir: &Path) -> Option<IconSet> {
    let meta: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("set.json")).ok()?).ok()?;
    let font_path = ["font.ttf", "font.otf"].iter().map(|f| dir.join(f)).find(|p| p.is_file())?;
    let names: Vec<(String, u32)> = serde_json::from_slice(&std::fs::read(dir.join("names.json")).ok()?).ok()?;
    let data = std::fs::read(&font_path).ok()?;
    let face = Face::parse(&data, 0).ok()?;
    // Only icons this font really has: a set's CSS can name icons that live
    // in a sibling font (Font Awesome's solid/regular/brands share one).
    let icons = names
        .into_iter()
        .filter(|(_, cp)| char::from_u32(*cp).and_then(|c| face.glyph_index(c)).is_some_and(|g| g.0 != 0))
        .map(|(name, codepoint)| Icon { name, codepoint })
        .collect();
    let str_field = |k: &str| meta.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    Some(IconSet {
        slug: dir.file_name()?.to_string_lossy().into_owned(),
        name: str_field("name"),
        version: str_field("version"),
        license: str_field("license"),
        family: family_name(&face)?,
        font_path,
        icons,
        bundled: true,
    })
}

/// A user's icon font: names come from the font's own glyph names, or the
/// codepoint when it has none.
fn load_user(path: &Path) -> Option<IconSet> {
    let data = std::fs::read(path).ok()?;
    let face = Face::parse(&data, 0).ok()?;
    let mut icons = Vec::new();
    if let Some(cmap) = face.tables().cmap {
        let mut seen = std::collections::HashSet::new();
        for sub in cmap.subtables {
            if !sub.is_unicode() {
                continue;
            }
            sub.codepoints(|cp| {
                if cp < 0x21 || !seen.insert(cp) {
                    return;
                }
                if let Some(gid) = sub.glyph_index(cp).filter(|g| g.0 != 0) {
                    let name = face
                        .glyph_name(gid)
                        .map(|n| n.replace(['-', '_'], " "))
                        .unwrap_or_else(|| format!("U+{cp:04X}"));
                    icons.push(Icon { name, codepoint: cp });
                }
            });
        }
    }
    icons.sort_by(|a, b| a.name.cmp(&b.name));
    let family = family_name(&face)?;
    Some(IconSet {
        slug: path.file_name()?.to_string_lossy().into_owned(),
        name: family.clone(),
        version: String::new(),
        license: String::new(),
        family,
        font_path: path.to_path_buf(),
        icons,
        bundled: false,
    })
}

fn family_name(face: &Face) -> Option<String> {
    let mut fallback = None;
    for n in face.names() {
        if n.name_id == 16 || n.name_id == 1 {
            if let Some(s) = n.to_string() {
                if n.name_id == 16 {
                    return Some(s);
                }
                fallback.get_or_insert(s);
            }
        }
    }
    fallback
}

/// The icon at `codepoint` as an SVG document of real vector paths, cropped
/// to the icon and `height` pixels tall. `None` if the font has no outline
/// for it.
pub fn icon_svg(font_data: &[u8], codepoint: u32, height: f32) -> Option<String> {
    let face = Face::parse(font_data, 0).ok()?;
    let gid = face.glyph_index(char::from_u32(codepoint)?)?;
    let mut path = SvgPath::default();
    let bbox = face.outline_glyph(gid, &mut path)?;
    let (w, h) = (
        (bbox.x_max - bbox.x_min) as f32,
        (bbox.y_max - bbox.y_min) as f32,
    );
    if w <= 0. || h <= 0. {
        return None;
    }
    // Font units grow upward; SVG's grow downward. The viewBox crops to the
    // icon; the outline is flipped with a transform so its numbers stay
    // exact.
    let scale = height / face.units_per_em() as f32;
    Some(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{:.2}\" height=\"{:.2}\" viewBox=\"{} {} {} {}\">\
         <path transform=\"scale(1,-1)\" d=\"{}\"/></svg>",
        w * scale,
        h * scale,
        bbox.x_min,
        -bbox.y_max,
        w,
        h,
        path.0.trim_end(),
    ))
}

/// A single-page vector PDF, suitable for native artwork clipboard exchange.
pub fn icon_pdf(font_data: &[u8], codepoint: u32, height: f32) -> Option<Vec<u8>> {
    if !height.is_finite() || height <= 0. { return None; }
    let face = Face::parse(font_data, 0).ok()?;
    let gid = face.glyph_index(char::from_u32(codepoint)?)?;
    let mut path = PdfPath::default();
    let b = face.outline_glyph(gid, &mut path)?;
    let scale = height / face.units_per_em() as f32;
    let w = (b.x_max as f32 - b.x_min as f32) * scale;
    let h = (b.y_max as f32 - b.y_min as f32) * scale;
    if w <= 0. || h <= 0. { return None; }
    let stream = format!("q\n{scale} 0 0 {scale} {} {} cm\n{}f\nQ\n",
        -(b.x_min as f32) * scale, -(b.y_min as f32) * scale, path.data);
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << >> /Contents 4 0 R >>"),
        format!("<< /Length {} >>\nstream\n{}endstream", stream.len(), stream),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in &offsets[1..] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    Some(pdf)
}

#[derive(Default)]
struct PdfPath { data: String, current: (f32, f32), start: (f32, f32) }
impl OutlineBuilder for PdfPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.data.push_str(&format!("{x} {y} m\n"));
        self.current = (x, y); self.start = (x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.data.push_str(&format!("{x} {y} l\n")); self.current = (x, y);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (px, py) = self.current;
        self.curve_to(px + (x1-px)*2./3., py + (y1-py)*2./3.,
            x + (x1-x)*2./3., y + (y1-y)*2./3., x, y);
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.data.push_str(&format!("{x1} {y1} {x2} {y2} {x} {y} c\n")); self.current = (x, y);
    }
    fn close(&mut self) { self.data.push_str("h\n"); self.current = self.start; }
}

#[derive(Default)]
struct SvgPath(String);

impl OutlineBuilder for SvgPath {
    fn move_to(&mut self, x: f32, y: f32) {
        self.0.push_str(&format!("M{x} {y} "));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.0.push_str(&format!("L{x} {y} "));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.0.push_str(&format!("Q{x1} {y1} {x} {y} "));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.0.push_str(&format!("C{x1} {y1} {x2} {y2} {x} {y} "));
    }
    fn close(&mut self) {
        self.0.push_str("Z ");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_set_loads_with_icons() {
        let sets = load_sets();
        let bundled: Vec<_> = sets.iter().filter(|s| s.bundled).collect();
        assert!(bundled.len() >= 20, "found {} bundled sets", bundled.len());
        for s in bundled {
            assert!(!s.icons.is_empty(), "{} has no icons", s.name);
            assert!(!s.family.is_empty(), "{} has no family name", s.name);
        }
    }

    #[test]
    fn pdf_quadratics_use_equivalent_cubic_controls() {
        let mut path = PdfPath::default();
        path.move_to(0., 0.);
        path.quad_to(3., 6., 6., 0.);
        assert!(path.data.contains("2 4 4 4 6 0 c"));
    }

    #[test]
    fn every_set_exports_vector_pdf_with_valid_offsets() {
        let sets = load_sets();
        assert!(!sets.is_empty());
        for set in sets {
            let data = std::fs::read(&set.font_path).unwrap();
            let pdf = icon_pdf(&data, set.icons[0].codepoint, 64.).expect(&set.name);
            let text = std::str::from_utf8(&pdf).unwrap();
            let xref: usize = text.split("startxref\n").nth(1).unwrap().lines().next().unwrap().parse().unwrap();
            assert!(text[xref..].starts_with("xref\n"));
            for (i, row) in text[xref..].lines().skip(3).take(4).enumerate() {
                let offset: usize = row.split_whitespace().next().unwrap().parse().unwrap();
                assert!(text[offset..].starts_with(&format!("{} 0 obj", i+1)));
            }
            let length: usize = text.split("/Length ").nth(1).unwrap().split_whitespace().next().unwrap().parse().unwrap();
            let start = text.find("stream\n").unwrap() + 7;
            assert!(text[start+length..].starts_with("endstream"));
            assert!(text[start..start+length].contains("f\n"));
        }
    }

    #[test]
    fn icons_export_as_svg_paths() {
        let sets = load_sets();
        let set = sets.iter().find(|s| s.slug == "phosphor").expect("phosphor set");
        let data = std::fs::read(&set.font_path).unwrap();
        let icon = &set.icons[0];
        let svg = icon_svg(&data, icon.codepoint, 64.).expect("an outline");
        assert!(svg.starts_with("<svg") && svg.contains(" d=\"M"), "{svg}");
    }
}
