//! Which characters every catalogued font has — for "Hide missing" (the
//! sample-text card's chip, View ▸ Hide Fonts Missing Characters) and the
//! tiles' dimmed-fallback check (`App::covers_sample`).
//!
//! Scanning records each face's coverage in the catalog (`fonts.coverage`)
//! as sorted codepoint ranges — a few hundred for a typical font, so the
//! whole index for a 10k-font library is a few MB — and it's loaded into
//! memory at launch and after each rescan. Every lookup is then in-memory:
//! no font file is read to answer "does this font have €".

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub type Ranges = Arc<[(u32, u32)]>;

#[derive(Default)]
pub struct CoverageIndex {
    ranges: HashMap<i64, Ranges>,
    /// Bumped on every reload, so cached filtered lists rebuild (see
    /// `App::coverage_signature`).
    version: u64,
}

impl CoverageIndex {
    /// Replaces the index with the catalog's `(id, blob)` rows.
    pub fn load(&mut self, rows: Vec<(i64, Vec<u8>)>) {
        self.ranges = rows
            .into_iter()
            .map(|(id, blob)| (id, decode(&blob).into()))
            .collect();
        self.version += 1;
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    /// `Some(true)` when `id` lacks any of `chars`; `None` when its coverage
    /// isn't known (callers treat that as covered — never hide on a guess).
    pub fn missing_any(&self, id: i64, chars: &[u32]) -> Option<bool> {
        let ranges = self.ranges.get(&id)?;
        Some(chars.iter().any(|&c| !contains(ranges, c)))
    }
}

/// Sorted, merged, inclusive ranges.
pub fn to_ranges(set: HashSet<u32>) -> Vec<(u32, u32)> {
    let mut points: Vec<u32> = set.into_iter().collect();
    points.sort_unstable();
    let mut ranges: Vec<(u32, u32)> = Vec::new();
    for p in points {
        match ranges.last_mut() {
            Some((_, end)) if *end + 1 == p => *end = p,
            _ => ranges.push((p, p)),
        }
    }
    ranges
}

/// The catalog's blob format: each range as two little-endian u32s.
pub fn encode(ranges: &[(u32, u32)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(ranges.len() * 8);
    for &(start, end) in ranges {
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&end.to_le_bytes());
    }
    out
}

pub fn decode(blob: &[u8]) -> Vec<(u32, u32)> {
    blob.chunks_exact(8)
        .map(|pair| {
            let start = u32::from_le_bytes(pair[0..4].try_into().unwrap());
            let end = u32::from_le_bytes(pair[4..8].try_into().unwrap());
            (start, end)
        })
        .collect()
}

fn contains(ranges: &[(u32, u32)], c: u32) -> bool {
    let i = ranges.partition_point(|&(_, end)| end < c);
    ranges.get(i).is_some_and(|&(start, _)| start <= c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranges_merge_and_answer_lookups() {
        let set: HashSet<u32> = [65, 66, 67, 97, 98, 0x2014].into_iter().collect();
        let ranges = to_ranges(set);
        assert_eq!(ranges, vec![(65, 67), (97, 98), (0x2014, 0x2014)]);
        assert!(contains(&ranges, 66));
        assert!(contains(&ranges, 0x2014));
        assert!(!contains(&ranges, 68));
        assert!(!contains(&ranges, 0));
        assert!(!contains(&ranges, 0x10ffff));
    }

    #[test]
    fn blobs_round_trip_into_the_index() {
        let ranges = vec![(32, 126), (0x20ac, 0x20ac)];
        let mut index = CoverageIndex::default();
        index.load(vec![(1, encode(&ranges)), (2, encode(&[]))]);
        assert_eq!(decode(&encode(&ranges)), ranges);
        assert_eq!(index.missing_any(1, &['A' as u32, 0x20ac]), Some(false));
        assert_eq!(index.missing_any(1, &['A' as u32, 0x1F600]), Some(true));
        assert_eq!(index.missing_any(2, &['A' as u32]), Some(true));
        assert_eq!(index.missing_any(3, &['A' as u32]), None);
    }
}
