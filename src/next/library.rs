//! The catalog as the views see it: faces grouped into families, plus the
//! sidebar filters, sorting and search over them.

use std::collections::{HashMap, HashSet};

use glyphclub::catalog::Catalog;
use glyphclub::coverage::CoverageIndex;
use glyphclub::font::{format_label, is_system_path};
use glyphclub::store::{Folder, FontRow, Tag};
use gpui_kit::SharedString;

use crate::settings::SortField;

/// How far back Recents reaches, by when a font arrived in its library.
const RECENT_WINDOW_SECS: i64 = 14 * 24 * 60 * 60;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Nav {
    All,
    System,
    Starred,
    Recents,
    Folder(i64),
    Tag(i64),
}

/// One face (one entry in a font file).
pub struct Face {
    pub id: i64,
    pub path: String,
    pub style: SharedString,
    pub postscript_name: String,
    pub weight: u16,
    pub italic: bool,
    /// When the file arrived in its library folder.
    pub added: i64,
}

/// One family within one library folder. The same family in two folders is
/// two separate sets of files, so it stays two separate entries.
pub struct Family {
    /// Stable across reloads, unlike the family's index; used for selection.
    pub key: SharedString,
    pub name: SharedString,
    pub folder_id: i64,
    /// Every face in the family, Regular-closest first.
    pub faces: Vec<Face>,
    pub is_system: bool,
    pub newest_added: i64,
    pub format: &'static str,
}

impl Family {
    /// The face tiles preview: the one closest to Regular.
    pub fn rep(&self) -> &Face {
        &self.faces[0]
    }

    pub fn ids(&self) -> impl Iterator<Item = i64> + '_ {
        self.faces.iter().map(|f| f.id)
    }
}

pub struct LibraryFolder {
    pub id: i64,
    pub path: String,
    pub name: SharedString,
    pub count: usize,
}

#[derive(Default)]
pub struct Library {
    pub families: Vec<Family>,
    pub folders: Vec<LibraryFolder>,
    pub tags: Vec<Tag>,
    /// font id -> tag ids
    pub font_tags: HashMap<i64, HashSet<i64>>,
    pub active: HashSet<i64>,
    pub starred: HashSet<i64>,
    pub coverage: CoverageIndex,
    pub face_count: usize,
    pub system_count: usize,
}

impl Library {
    pub fn load(catalog: &Catalog) -> anyhow::Result<Self> {
        let fonts = catalog.list_fonts("")?;
        let folders = catalog.list_folders()?;
        let mut lib = Self::build(fonts, folders);
        lib.active = catalog.active_ids()?;
        lib.starred = catalog.favorite_ids()?;
        lib.tags = catalog.list_tags()?;
        lib.font_tags = catalog.font_tags()?;
        lib.coverage.load(catalog.coverage().unwrap_or_default());
        Ok(lib)
    }

    fn build(fonts: Vec<FontRow>, folders: Vec<Folder>) -> Self {
        let mut by_key: HashMap<(i64, String), Family> = HashMap::new();
        let mut folder_counts: HashMap<i64, usize> = HashMap::new();
        let mut face_count = 0;
        let mut system_count = 0;

        for row in fonts {
            let rec = row.record;
            // Dot-prefixed families are macOS's private UI fonts; apps never
            // list them.
            if rec.family.starts_with('.') {
                continue;
            }
            face_count += 1;
            *folder_counts.entry(row.folder_id).or_default() += 1;
            let is_system = is_system_path(&rec.path);
            if is_system {
                system_count += 1;
            }
            let fam = by_key.entry((row.folder_id, rec.family.clone())).or_insert_with(|| Family {
                key: format!("{}:{}", row.folder_id, rec.family).into(),
                name: rec.family.clone().into(),
                folder_id: row.folder_id,
                faces: Vec::new(),
                is_system,
                newest_added: rec.added,
                format: format_label(&rec.path),
            });
            fam.newest_added = fam.newest_added.max(rec.added);
            fam.faces.push(Face {
                id: row.id,
                path: rec.path,
                style: rec.subfamily.into(),
                postscript_name: rec.postscript_name,
                weight: rec.weight,
                italic: rec.italic,
                added: rec.added,
            });
        }

        let mut families: Vec<Family> = by_key.into_values().collect();
        for fam in &mut families {
            fam.faces.sort_by_key(|f| {
                (
                    f.italic,
                    (f.weight as i32 - 400).abs(),
                    f.style.to_lowercase(),
                )
            });
        }
        families.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        let mut folders: Vec<LibraryFolder> = folders
            .into_iter()
            .filter(|f| !is_system_path(&f.path))
            .map(|f| LibraryFolder {
                id: f.id,
                name: f.path.rsplit(['/', '\\']).next().unwrap_or(&f.path).to_string().into(),
                count: folder_counts.get(&f.id).copied().unwrap_or(0),
                path: f.path,
            })
            .collect();
        folders.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        Self { families, folders, face_count, system_count, ..Default::default() }
    }

    pub fn family_active(&self, f: &Family) -> bool {
        f.ids().any(|id| self.active.contains(&id))
    }

    pub fn family_starred(&self, f: &Family) -> bool {
        f.ids().any(|id| self.starred.contains(&id))
    }

    pub fn family_tags(&self, f: &Family) -> HashSet<i64> {
        f.ids()
            .filter_map(|id| self.font_tags.get(&id))
            .flatten()
            .copied()
            .collect()
    }

    pub fn tag_name(&self, id: i64) -> Option<&str> {
        self.tags.iter().find(|t| t.id == id).map(|t| t.name.as_str())
    }

    fn first_tag_name(&self, f: &Family) -> Option<String> {
        let mut names: Vec<String> = self
            .family_tags(f)
            .into_iter()
            .filter_map(|id| self.tag_name(id).map(|n| n.to_lowercase()))
            .collect();
        names.sort();
        names.into_iter().next()
    }

    /// True when the family's preview face lacks any of `chars`. Unknown
    /// coverage counts as covered: never hide on a guess.
    pub fn missing_chars(&self, f: &Family, chars: &[u32]) -> bool {
        !chars.is_empty() && self.coverage.missing_any(f.rep().id, chars).unwrap_or(false)
    }

    pub fn tag_count(&self, tag: i64) -> usize {
        self.families
            .iter()
            .filter(|f| f.ids().any(|id| self.font_tags.get(&id).is_some_and(|t| t.contains(&tag))))
            .count()
    }

    /// Indices into `families` that pass the sidebar filter, the search and
    /// (optionally) the missing-characters filter, in sort order.
    pub fn filter(
        &self,
        nav: Nav,
        query: &str,
        sort: SortField,
        ascending: bool,
        hide_missing_for: Option<&[u32]>,
    ) -> Vec<usize> {
        let q = query.trim().to_lowercase();
        let recent_cutoff = now_unix() - RECENT_WINDOW_SECS;
        let mut out: Vec<usize> = self
            .families
            .iter()
            .enumerate()
            .filter(|(_, f)| match nav {
                Nav::All => true,
                Nav::System => f.is_system,
                Nav::Starred => self.family_starred(f),
                Nav::Recents => !f.is_system && f.newest_added >= recent_cutoff,
                Nav::Folder(id) => f.folder_id == id,
                Nav::Tag(tag) => f
                    .ids()
                    .any(|id| self.font_tags.get(&id).is_some_and(|t| t.contains(&tag))),
            })
            .filter(|(_, f)| {
                q.is_empty()
                    || f.name.to_lowercase().contains(&q)
                    || f.faces.iter().any(|face| face.postscript_name.to_lowercase().contains(&q))
            })
            .filter(|(_, f)| hide_missing_for.is_none_or(|chars| !self.missing_chars(f, chars)))
            .map(|(i, _)| i)
            .collect();

        let name = |i: usize| self.families[i].name.to_lowercase();
        out.sort_by(|&a, &b| {
            let (fa, fb) = (&self.families[a], &self.families[b]);
            let primary = match sort {
                SortField::Name | SortField::Category => std::cmp::Ordering::Equal,
                SortField::DateAdded => fa
                    // Your libraries before system fonts either way: macOS
                    // updates make system fonts' dates recent.
                    .is_system
                    .cmp(&fb.is_system)
                    .reverse()
                    .then(fa.newest_added.cmp(&fb.newest_added)),
                SortField::Enabled => self.family_active(fa).cmp(&self.family_active(fb)),
                SortField::Starred => self.family_starred(fa).cmp(&self.family_starred(fb)),
                SortField::Styles => fa.faces.len().cmp(&fb.faces.len()),
                SortField::Format => fa.format.cmp(fb.format),
                SortField::Tags => match (self.first_tag_name(fa), self.first_tag_name(fb)) {
                    // Tagged first, by tag name, whichever direction.
                    (Some(x), Some(y)) => x.cmp(&y),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                },
            };
            let primary = if ascending || sort == SortField::Tags { primary } else { primary.reverse() };
            let primary = if sort == SortField::Name && !ascending { name(b).cmp(&name(a)) } else { primary };
            primary.then_with(|| name(a).cmp(&name(b)))
        });
        out
    }
}

/// A family's category, from its OpenType classification (OS/2
/// sFamilyClass) the way RightFont's Category column shows it. Empty when
/// the font doesn't say.
pub fn category_of(data: &[u8]) -> &'static str {
    let Ok(face) = ttf_parser::Face::parse(data, 0) else { return "" };
    if face.is_monospaced() {
        return "Monospaced";
    }
    let Some(os2) = face.raw_face().table(ttf_parser::Tag::from_bytes(b"OS/2")) else { return "" };
    match os2.get(30) {
        Some(1 | 2 | 3 | 7) => "Serif",
        Some(4 | 5) => "Slab Serif",
        Some(8) => "Sans Serif",
        Some(9) => "Display",
        Some(10) => "Script",
        Some(12) => "Symbols",
        _ => "",
    }
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
