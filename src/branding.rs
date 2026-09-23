//! The real GlyphClub mark and wordmark (source: `branding/SVG/icon.svg`,
//! `branding/SVG/wordmark.svg`), embedded as raw SVG path data and parsed
//! into `BezPath`s once. Drawing a vector mark this way — rather than
//! rasterizing the PNG/`.icon` export into a texture — keeps it crisp at
//! any tile size and matches how the traffic-light hover glyphs already
//! work in `ui/sidebar.rs`.

use std::sync::OnceLock;

use vello::kurbo::{Affine, BezPath, Rect, Shape};
use vello::peniko::{Color, Fill};
use vello::Scene;

const ICON_VIEWBOX: (f64, f64) = (1059.91, 848.11);
const ICON_PATH: &str = "M733.81,417.78h0c-109.71,0-173.15-88.94-141.69-198.66h-30.65c-29.13,101.58-129.39,185.2-231.17,197.06C393.87,208.82,542.25,16.93,671.11,16.93c101.57,0,153.41,75.12,153.41,211.6,0,65.47-15.08,101.63-21.96,121.11h24.58L965.71,18.73h-20.32c-3.58,6.76-8.51,14.9-17.18,23.58-10.57,10.59-29.62,19.05-51.84,19.05-67.71,0-110.05-61.36-206.33-61.36C437.91,0,206.66,209.31,150.69,417.78H0v30.64h143.78c-3.82,20.69-5.94,41.29-5.94,61.55,0,164,128.02,270.86,257.1,270.86s172.47-65.6,232.78-65.6c40.19,0,47.86,28.64,47.86,57.21,0,37.62-9.01,60.05-13.97,75.66h23.8c10.33-24.27,31.32-77.84,107.35-266.19,34.8-86.57,56.6-121.7,81.02-133.49h186.12v-30.64h-326.1ZM674.01,514.36c-26.8,81.09-122.51,247.43-263.19,247.43-40.21,0-112.15-18-112.15-151.31,0-51.83,8.03-106.33,22.05-160.45,94.9,11.92,147.15,95.51,118.03,197.04h30.65c21.69-75.64,82.83-141.38,154.66-174.93,2.11-.78,71.63-25.89,49.94,42.22Z";

const WORDMARK_VIEWBOX: (f64, f64) = (190.63, 22.87);
const WORDMARK_PATH: &str = "M185.24,4.81c0-2.43-2.23-4.02-5.88-4.02h-17.52v.5c1.28.09,1.66.58,1.66,1.84,0,1.61-1.07,5.03-2.21,8.14h-9.61l2.63-6.23c1.27-3,1.96-3.75,4.42-3.76v-.49h-10.76v.52c1.61.14,2.14.87,2.14,2,0,.91-.4,2.05-.94,3.34l-1.98,4.63h-13.71l2.34-6.11c1.14-2.94,2.2-3.91,4.31-3.91.42,0,.71,0,1.05.02v-.47h-11.18v.53c1.04.13,1.77.63,1.77,1.98,0,.94-.34,2.23-.97,3.85l-1.59,4.12h-17.98c1.57-5.03,5.56-10.39,9.53-10.39,2.05,0,3.4,1.06,3.4,2.8,0,2.97-4.05,2.65-4.05,5.14,0,.8.57,1.48,1.63,1.48,1.83,0,3.6-2.17,3.6-4.99s-1.26-4.85-4.88-4.85c-6.57,0-12.43,5.44-13.96,10.82h-4.62l1.38-3.6c1.72-4.46,2.78-6.22,5.01-6.38v-.5h-9.79v.52c1.38.14,1.76.91,1.76,1.88,0,.74-.82,3.61-2.19,6.84-.22.53-.61,1.01-1.33,1.24h-7.2c-.13-.09-.17-.21-.17-.3,0-.26.79-2.27,1.19-3.3,1.67-4.35,2.78-6.14,4.83-6.36v-.51h-9.78v.51c1.11.11,1.9.6,1.9,2,0,.94-.34,2.23-.97,3.85l-1.59,4.12h-6.7c2.82-.89,5.38-2.8,5.38-6,0-2.77-2.43-4.48-7.08-4.48h-15.94v.49h.64c.69,0,1.17.29,1.17.74,0,.48-.17.97-1.46,2.34l-5.28,5.65c-.74.8-1,.94-1.17.94-.26,0-.46-.23-.74-1.26l-1.43-5.34c-.49-1.94-.06-3.11,1.26-3.11.09,0,.18,0,.28,0v-.46h-9.54v.5c1.7.06,2.47.81,3.07,2.73l1.74,5.82c.18.57.29,1.04.37,1.43h-16.49l2.34-6.11c1.14-2.94,2.2-3.91,4.31-3.91.49,0,.8.01,1.23.02v-.48h-12.02v.49c1.39,0,2.43.41,2.43,2.02,0,.94-.34,2.23-.97,3.85l-1.59,4.12h-9.18,0c-2.96,0-4.67-2.4-3.82-5.36h-.83c-.76,2.65-3.32,4.84-5.97,5.27C10.65,5.6,14.63.46,18.1.46c2.74,0,4.14,2.03,4.14,5.71,0,1.77-.41,2.74-.59,3.27h.66L26.04.51h-.55c-.1.18-.23.4-.46.64-.29.29-.8.51-1.4.51-1.83,0-2.97-1.65-5.56-1.65C11.81,0,5.58,5.64,4.07,11.27H0v.83h3.89c-.1.56-.17,1.11-.17,1.66,0,4.42,3.45,7.3,6.93,7.3s4.65-1.77,6.28-1.77c1.08,0,1.29.77,1.29,1.54,0,1.01-.24,1.62-.38,2.04h.64c.28-.65.84-2.1,2.9-7.18.94-2.34,1.53-3.29,2.19-3.6h4.82l-.57,1.48c-2.03,5.19-3.08,7.08-6.05,7.08h0v.48h0s18,0,18,0c.6,0,.77-.26.88-.6s1.28-3.68,2.08-5.68c.47-1.18.89-2.13,1.16-2.76h5.67c0,.09.01.19.01.28,0,1.57-1.34,4.71-2.28,6.28-.76,1.32-1.69,1.98-3.36,1.99v.49h9.49v-.48h-.65c-.88,0-1.17-.37-1.17-.91,0-.8,1.31-4.25,2.43-6.59.17-.4.39-.73.64-1.05h10.1l-.57,1.48c-1.98,5.08-3.04,6.99-5.86,7.07v.49h12.06v-.48h-1.09c-1.54,0-2.34-.8-2.34-2.05,0-.83.23-1.65.86-3.31l.94-2.45c.13-.33.29-.58.5-.74h15.31l-.57,1.48c-1.99,5.09-3.04,7-5.88,7.07v.49h10.16v-.55c-1.21-.21-1.55-.93-1.55-1.85,0-.74.34-2.11.86-3.45l.52-1.44c.51-1.34,1.4-1.71,2.43-1.71h5.31c.86,0,.97.34.97.66,0,.26-.06.57-.23,1.03l-.15.44c-1.64,4.21-2.95,6.02-5.04,6.33v.54h10.46v-.47h-.06c-1.88-.01-2.34-.84-2.34-1.92,0-.74.34-2.11.86-3.45l1.22-3.19h4.74c-.12.6-.2,1.2-.2,1.79,0,4.74,4.02,7.62,8.28,7.62,2.68,0,5.28-1.14,7.13-3,.63-.63,1.06-1.28,1.06-1.71,0-.29-.14-.51-.49-.51-.77,0-1.46,1.17-2.8,2.51-1.28,1.28-2.65,2.14-4.17,2.14-3.37,0-4.59-2.6-4.59-5.59,0-1,.17-2.11.47-3.25h17.91l-.57,1.48c-1.92,4.92-2.97,6.86-5.59,7.06v.5h17.55c.6,0,.77-.26.88-.6s1.28-3.68,2.08-5.68c.46-1.18.89-2.13,1.16-2.76h2.45c-.83,1.98-1.33,3.52-1.33,4.62,0,3.11,2.4,4.82,6.59,4.82s6.68-1.83,8.53-6.62l1.08-2.82h5.74l-.57,1.48c-1.91,4.89-2.95,6.84-5.54,7.06v.51l13.36.03c5.45,0,8.13-3.34,8.13-6.02,0-1.55-.78-2.46-2.04-3.05h5.19v-.83h-6.69c3.15-1.01,5.95-3.62,5.95-6.46ZM11.08,20.54c-1.08,0-3.02-.49-3.02-4.08,0-1.39.22-2.84.59-4.29,2.4.43,3.7,2.62,2.94,5.28h.83c.68-2.36,2.78-4.34,5.1-5.05.91-.19.89.55.83.88-.36,1.81-3.07,7.27-7.26,7.27ZM33.11,20.62c-1.88,0-2.51-.88-2.51-1.88,0-.74.37-2.17.86-3.45l1.22-3.19h10.71c-1.62,4.3-6.5,8.53-10.28,8.53ZM55.4,11.27l7.87-8.6c.59-.65,1.18-1.01,1.79-1.2.62-.17,1.11-.1,1.32-.05.78.22,1.29.75,1.29,1.88,0,.94-.34,2.23-.97,3.85l-1.59,4.12h-9.71ZM70.11,11.27c-.14-.04-.24-.09-.31-.15-.2-.2-.17-.49.14-1.26l1.37-3.68c1.08-2.94,1.88-4.94,4.59-4.94,2.14,0,3.42,1.37,3.42,3.82,0,2.97-2.05,5.59-5.23,6.2h-3.99ZM133.6,20.62c-1.88,0-2.51-.88-2.51-1.88,0-.74.37-2.17.86-3.45l1.22-3.19h10.71c-1.62,4.3-6.5,8.53-10.28,8.53ZM159.56,15.63c-1.63,3.79-3.94,5.25-6.68,5.25-1.97,0-3.17-1.26-3.17-3.14,0-1.48.43-2.82,1.4-5.14l.21-.51h9.65c-.5,1.33-1,2.57-1.42,3.53ZM162.05,11.27l2.08-5.43c1.14-2.96,2.33-4.1,3.6-4.44,0,0,.01,0,.02,0,.12-.03.24-.05.36-.06.22-.02.42,0,.58.02.99.15,1.67.66,1.67,1.96,0,.94-.31,2.17-.97,3.85l-1.59,4.12h-5.74ZM178.31,14.14c0,3.74-2.94,6.51-5.76,6.51-2.34,0-2.88-.86-2.88-1.91,0-.74.37-2.17.86-3.45l.93-2.26c.17-.43.36-.73.6-.93h5.44c.49.35.82.98.82,2.05ZM172.96,11.27c-.39-.09-.57-.24-.57-.52,0-.11.03-.26.08-.43l1.44-3.82c1.2-3.08,2-5.25,4.51-5.25,1.37,0,2.71,1,2.71,3.02s-1.94,6.31-5.28,7h-2.89Z";
const WORDMARK_TM_POLY_1: &[(f64, f64)] = &[
    (186.88, 1.08),
    (187.39, 1.08),
    (187.39, 2.54),
    (187.85, 2.54),
    (187.85, 1.08),
    (188.36, 1.08),
    (188.36, 0.79),
    (186.88, 0.79),
];
const WORDMARK_TM_POLY_2: &[(f64, f64)] = &[
    (189.94, 0.79),
    (189.6, 2.02),
    (189.59, 2.02),
    (189.26, 0.79),
    (188.57, 0.79),
    (188.57, 2.54),
    (189.0, 2.54),
    (189.0, 1.14),
    (189.42, 2.54),
    (189.78, 2.54),
    (190.19, 1.14),
    (190.2, 1.14),
    (190.2, 2.54),
    (190.63, 2.54),
    (190.63, 0.79),
];

/// The lime green from the source SVGs — distinct from (brighter/more
/// saturated than) `theme::TRAFFIC_ZOOM`'s green, so this stays the
/// authoritative brand color rather than reusing a UI-chrome one.
pub const BRAND_LIME: Color = Color::from_rgb8(0x9a, 0xff, 0x00);

/// The card motif's light-grey layer color, matching `branding/SVG/otf.svg`
/// and `ttf.svg`'s `.cls-1` exactly.
pub const CARD_LIGHT: Color = Color::from_rgb8(0xc6, 0xc6, 0xc6);

const CARD_VIEWBOX: (f64, f64) = (84.86, 108.54);

// Each SVG's `.cls-1` (light) and `.cls-2` (dark, meant to show whatever
// it's composited over — see `draw_card`) paths, kept as the separate `d`
// strings they were authored as rather than hand-merged, then unioned into
// one `BezPath` per color at parse time (safe: nonzero fill of several
// non-overlapping — or, for the outline's own inner/outer counter,
// deliberately opposite-wound — closed subpaths renders identically to
// filling them one at a time).
const OTF_LIGHT_PATHS: &[&str] = &[
    "M75.4,0H9.46C4.23,0,0,4.23,0,9.46v89.62c0,5.22,4.23,9.46,9.46,9.46h65.94c5.23,0,9.46-4.24,9.46-9.46V9.46C84.86,4.23,80.63,0,75.4,0ZM80.86,99.08c0,1.51-.61,2.86-1.6,3.86-.99.99-2.35,1.6-3.86,1.6H9.46c-1.51,0-2.87-.61-3.86-1.6-.99-1-1.6-2.35-1.6-3.86V9.46c0-1.52.61-2.87,1.6-3.86.99-1,2.35-1.6,3.86-1.6h65.94c1.51,0,2.87.6,3.86,1.6.99.99,1.6,2.34,1.6,3.86v89.62Z",
    "M10.87,18.3h1.78v1.75h-1.78v-1.75Z",
    "M18.18,20.28c-1.23,0-2.18-.34-2.83-1.01-.88-.82-1.31-2.01-1.31-3.56s.44-2.77,1.31-3.56c.65-.67,1.6-1.01,2.83-1.01s2.18.34,2.83,1.01c.87.79,1.31,1.98,1.31,3.56s-.44,2.74-1.31,3.56c-.65.67-1.6,1.01-2.83,1.01ZM19.89,17.96c.42-.53.63-1.28.63-2.25s-.21-1.72-.63-2.25-.99-.79-1.7-.79-1.29.26-1.71.79-.64,1.28-.64,2.25.21,1.72.64,2.25,1,.79,1.71.79,1.28-.26,1.71-.79Z",
    "M29.97,11.41v1.53h-2.58v7.11h-1.82v-7.11h-2.6v-1.53h7Z",
    "M37.15,12.94h-4.33v1.99h3.79v1.5h-3.79v3.62h-1.79v-8.62h6.12v1.52Z",
];
// The big center glyph (below) lives here, not in `*_LIGHT_PATHS` — the
// source SVG (`branding/SVG/otf.svg`/`ttf.svg`) files it under `.cls-1`
// (the same flat `#c6c6c6` as the card body itself), which makes it
// invisible once actually rendered: same fill color on top of the same
// fill color. `.cls-2`'s `#1a1a1a` isn't a coincidence either — it's an
// exact match for `theme::DARK`'s canvas color, confirming the intent was
// always a genuine two-tone punch-through (see `draw_card`'s `dark_layer`
// param), not a solid watermark. Moving the glyph here is what actually
// makes it show up as a contrasting mark rather than disappearing into
// the card's own light-gray fill.
const OTF_DARK_PATHS: &[&str] = &[
    "M80.86,9.46v89.62c0,1.51-.61,2.86-1.6,3.86-.99.99-2.35,1.6-3.86,1.6H9.46c-1.51,0-2.87-.61-3.86-1.6-.99-1-1.6-2.35-1.6-3.86V9.46c0-1.52.61-2.87,1.6-3.86.99-1,2.35-1.6,3.86-1.6h65.94c1.51,0,2.87.6,3.86,1.6.99.99,1.6,2.34,1.6,3.86Z",
    "M29.29,49.79c5.47-6.41,11.59-9.62,18.36-9.62,4.51,0,8.21,1.36,11.11,4.07s4.35,6.39,4.35,11.02c0,6.76-2.56,13.18-7.68,19.26-5.47,6.51-11.75,9.77-18.85,9.77-4.47,0-8.1-1.36-10.91-4.09s-4.21-6.36-4.21-10.92c0-6.86,2.61-13.36,7.83-19.51ZM31.22,78.36c1,2.51,2.81,3.77,5.45,3.77,2.47,0,4.71-.91,6.73-2.74s4.11-5.31,6.27-10.46c1.37-3.25,2.44-6.67,3.21-10.26.78-3.59,1.16-6.5,1.16-8.73,0-2.1-.52-3.9-1.56-5.41-1.04-1.5-2.66-2.25-4.87-2.25-5.2,0-9.58,4.62-13.13,13.87-2.69,7.02-4.04,12.87-4.04,17.55,0,1.82.25,3.37.77,4.65Z",
];

const TTF_LIGHT_PATHS: &[&str] = &[
    "M10.87,18.3h1.78v1.75h-1.78v-1.75Z",
    "M20.64,11.41v1.53h-2.58v7.11h-1.82v-7.11h-2.6v-1.53h7Z",
    "M27.97,11.41v1.53h-2.58v7.11h-1.82v-7.11h-2.6v-1.53h7Z",
    "M35.15,12.94h-4.33v1.99h3.79v1.5h-3.79v3.62h-1.79v-8.62h6.12v1.52Z",
    "M75.4,0H9.46C4.24,0,0,4.24,0,9.46v89.62c0,5.23,4.24,9.46,9.46,9.46h65.94c5.23,0,9.46-4.24,9.46-9.46V9.46c0-5.23-4.24-9.46-9.46-9.46ZM80.86,99.08c0,1.52-.61,2.87-1.6,3.86-1,.99-2.35,1.6-3.86,1.6H9.46c-1.52,0-2.87-.61-3.86-1.6-.99-1-1.6-2.35-1.6-3.86V9.46c0-1.52.61-2.87,1.6-3.86,1-.99,2.35-1.6,3.86-1.6h65.94c1.52,0,2.87.61,3.86,1.6.99,1,1.6,2.35,1.6,3.86v89.62Z",
];
// Same reasoning as `OTF_DARK_PATHS` above — the big glyph belongs here,
// not in `TTF_LIGHT_PATHS`, or it's invisible against the card body.
const TTF_DARK_PATHS: &[&str] = &[
    "M80.86,9.46v89.62c0,1.51-.61,2.86-1.6,3.86-.99.99-2.35,1.6-3.86,1.6H9.46c-1.51,0-2.87-.61-3.86-1.6-.99-1-1.6-2.35-1.6-3.86V9.46c0-1.52.61-2.87,1.6-3.86.99-1,2.35-1.6,3.86-1.6h65.94c1.51,0,2.87.6,3.86,1.6.99.99,1.6,2.34,1.6,3.86Z",
    "M64.02,53.04h-23.57v-12.36c2.3.06,4.04.42,5.2,1.06,2.06,1.18,3.31,3.44,3.75,6.77h1.33l-.04-9.33h-27.85l-.04,9.33h1.33c.29-3.31,1.54-5.56,3.75-6.77,1.2-.65,2.94-1,5.19-1.06v24.37c0,1.69-.3,2.81-.89,3.38-.6.57-1.8.85-3.6.85v1.17h16.49v-1.17c-1.88,0-3.12-.28-3.71-.84-.6-.56-.89-1.69-.89-3.39v-8.97c.24-.18.49-.34.75-.48,1.2-.65,2.94-1,5.19-1.06v24.37c0,1.69-.3,2.81-.89,3.38-.6.57-1.8.85-3.6.85v1.17h16.49v-1.17c-1.88,0-3.12-.28-3.71-.84-.6-.56-.89-1.69-.89-3.39v-24.37c2.3.06,4.04.42,5.2,1.06,2.06,1.18,3.31,3.44,3.75,6.77h1.33l-.04-9.33Z",
];

// Just the big center glyph from each card (the second entry in
// `OTF_DARK_PATHS`/`TTF_DARK_PATHS` above), on its own — no card outline,
// no corner label, for the small format badge next to a font's name in
// list/grid rows (`ui::draw_format_badge`).
const OTF_MARK_PATH: &str = "M29.29,49.79c5.47-6.41,11.59-9.62,18.36-9.62,4.51,0,8.21,1.36,11.11,4.07s4.35,6.39,4.35,11.02c0,6.76-2.56,13.18-7.68,19.26-5.47,6.51-11.75,9.77-18.85,9.77-4.47,0-8.1-1.36-10.91-4.09s-4.21-6.36-4.21-10.92c0-6.86,2.61-13.36,7.83-19.51ZM31.22,78.36c1,2.51,2.81,3.77,5.45,3.77,2.47,0,4.71-.91,6.73-2.74s4.11-5.31,6.27-10.46c1.37-3.25,2.44-6.67,3.21-10.26.78-3.59,1.16-6.5,1.16-8.73,0-2.1-.52-3.9-1.56-5.41-1.04-1.5-2.66-2.25-4.87-2.25-5.2,0-9.58,4.62-13.13,13.87-2.69,7.02-4.04,12.87-4.04,17.55,0,1.82.25,3.37.77,4.65Z";
const TTF_MARK_PATH: &str = "M64.02,53.04h-23.57v-12.36c2.3.06,4.04.42,5.2,1.06,2.06,1.18,3.31,3.44,3.75,6.77h1.33l-.04-9.33h-27.85l-.04,9.33h1.33c.29-3.31,1.54-5.56,3.75-6.77,1.2-.65,2.94-1,5.19-1.06v24.37c0,1.69-.3,2.81-.89,3.38-.6.57-1.8.85-3.6.85v1.17h16.49v-1.17c-1.88,0-3.12-.28-3.71-.84-.6-.56-.89-1.69-.89-3.39v-8.97c.24-.18.49-.34.75-.48,1.2-.65,2.94-1,5.19-1.06v24.37c0,1.69-.3,2.81-.89,3.38-.6.57-1.8.85-3.6.85v1.17h16.49v-1.17c-1.88,0-3.12-.28-3.71-.84-.6-.56-.89-1.69-.89-3.39v-24.37c2.3.06,4.04.42,5.2,1.06,2.06,1.18,3.31,3.44,3.75,6.77h1.33l-.04-9.33Z";

fn otf_mark() -> &'static BezPath {
    static PATH: OnceLock<BezPath> = OnceLock::new();
    PATH.get_or_init(|| BezPath::from_svg(OTF_MARK_PATH).expect("branding otf mark path is valid"))
}

fn ttf_mark() -> &'static BezPath {
    static PATH: OnceLock<BezPath> = OnceLock::new();
    PATH.get_or_init(|| BezPath::from_svg(TTF_MARK_PATH).expect("branding ttf mark path is valid"))
}

/// Fills `path` into `target`, fit to *its own* tight bounding box and
/// centered — unlike `draw_fitted` (used for the mark/wordmark/card,
/// authored against a known logical viewBox), these two marks are cropped
/// out of a larger card illustration, so the box to fit is whatever the
/// path itself actually spans, not a fixed canvas size. Returns the
/// rendered width, so a caller placing text right after it (`ui::
/// draw_format_badge`) knows how much space it actually used rather than
/// assuming it filled all of `target`.
fn draw_fitted_to_bounds(scene: &mut Scene, path: &BezPath, color: Color, target: Rect) -> f64 {
    let bounds = path.bounding_box();
    let scale = (target.width() / bounds.width()).min(target.height() / bounds.height());
    let x = target.x0 + (target.width() - bounds.width() * scale) / 2.0 - bounds.x0 * scale;
    let y = target.y0 + (target.height() - bounds.height() * scale) / 2.0 - bounds.y0 * scale;
    let transform = Affine::translate((x, y)) * Affine::scale(scale);
    scene.fill(Fill::NonZero, transform, color, None, path);
    bounds.width() * scale
}

/// The OpenType format badge — just the center "O" mark, no card.
/// Returns the rendered width.
pub fn draw_otf_mark(scene: &mut Scene, target: Rect, color: Color) -> f64 {
    draw_fitted_to_bounds(scene, otf_mark(), color, target)
}

/// The TrueType format badge — just the center "Tr" mark, no card.
/// Returns the rendered width.
pub fn draw_ttf_mark(scene: &mut Scene, target: Rect, color: Color) -> f64 {
    draw_fitted_to_bounds(scene, ttf_mark(), color, target)
}

fn merged_path(parts: &[&str]) -> BezPath {
    let mut merged = BezPath::new();
    for d in parts {
        merged.extend(BezPath::from_svg(d).expect("branding card path data is valid"));
    }
    merged
}

struct Card {
    light: BezPath,
    dark: BezPath,
}

fn otf_card() -> &'static Card {
    static CARD: OnceLock<Card> = OnceLock::new();
    CARD.get_or_init(|| Card {
        light: merged_path(OTF_LIGHT_PATHS),
        dark: merged_path(OTF_DARK_PATHS),
    })
}

fn ttf_card() -> &'static Card {
    static CARD: OnceLock<Card> = OnceLock::new();
    CARD.get_or_init(|| Card {
        light: merged_path(TTF_LIGHT_PATHS),
        dark: merged_path(TTF_DARK_PATHS),
    })
}

/// Draws one file-type card (`.otf` or `.ttf`, see `draw_otf_card`/
/// `draw_ttf_card`) at `height` logical px tall, centered on `center` and
/// rotated `degrees` around that same center point. `dark_layer` should be
/// whatever color the card is sitting on top of — the SVG's own dark fill
/// is a punch-through, not a real color, so it only looks right matched to
/// the actual backdrop rather than hardcoded.
fn draw_card(
    scene: &mut Scene,
    card: &Card,
    center: vello::kurbo::Point,
    height: f64,
    degrees: f64,
    light_layer: Color,
    dark_layer: Color,
) {
    let scale = height / CARD_VIEWBOX.1;
    let w = CARD_VIEWBOX.0 * scale;
    let h = CARD_VIEWBOX.1 * scale;
    let transform = Affine::translate((center.x, center.y))
        * Affine::rotate(degrees.to_radians())
        * Affine::translate((-w / 2.0, -h / 2.0))
        * Affine::scale(scale);
    scene.fill(Fill::NonZero, transform, light_layer, None, &card.light);
    scene.fill(Fill::NonZero, transform, dark_layer, None, &card.dark);
}

/// The `.otf` file-type card — see `draw_card`.
pub fn draw_otf_card(scene: &mut Scene, center: vello::kurbo::Point, height: f64, degrees: f64, dark_layer: Color) {
    draw_card(scene, otf_card(), center, height, degrees, CARD_LIGHT, dark_layer);
}

/// The `.ttf` file-type card — see `draw_card`.
pub fn draw_ttf_card(scene: &mut Scene, center: vello::kurbo::Point, height: f64, degrees: f64, dark_layer: Color) {
    draw_card(scene, ttf_card(), center, height, degrees, CARD_LIGHT, dark_layer);
}

fn polygon_path(points: &[(f64, f64)]) -> BezPath {
    let mut path = BezPath::new();
    if let Some(&first) = points.first() {
        path.move_to(first);
        for &pt in &points[1..] {
            path.line_to(pt);
        }
        path.close_path();
    }
    path
}

fn icon_path() -> &'static BezPath {
    static PATH: OnceLock<BezPath> = OnceLock::new();
    PATH.get_or_init(|| BezPath::from_svg(ICON_PATH).expect("branding icon.svg path data is valid"))
}

fn wordmark_path() -> &'static BezPath {
    static PATH: OnceLock<BezPath> = OnceLock::new();
    PATH.get_or_init(|| {
        let mut path =
            BezPath::from_svg(WORDMARK_PATH).expect("branding wordmark.svg path data is valid");
        path.extend(polygon_path(WORDMARK_TM_POLY_1));
        path.extend(polygon_path(WORDMARK_TM_POLY_2));
        path
    })
}

/// Fills `path` (authored in its own SVG-unit coordinate space of size
/// `viewbox_size`), scaled uniformly to fit inside `target` and centered
/// there — never stretched off its native aspect ratio.
fn draw_fitted(scene: &mut Scene, path: &BezPath, viewbox_size: (f64, f64), color: Color, target: Rect) {
    let scale = (target.width() / viewbox_size.0).min(target.height() / viewbox_size.1);
    let drawn_w = viewbox_size.0 * scale;
    let drawn_h = viewbox_size.1 * scale;
    let x = target.x0 + (target.width() - drawn_w) / 2.0;
    let y = target.y0 + (target.height() - drawn_h) / 2.0;
    let transform = Affine::translate((x, y)) * Affine::scale(scale);
    scene.fill(Fill::NonZero, transform, color, None, path);
}

/// Draws the "G" mark, fit and centered inside `target`.
pub fn draw_icon(scene: &mut Scene, target: Rect, color: Color) {
    draw_fitted(scene, icon_path(), ICON_VIEWBOX, color, target);
}

/// Draws the "GlyphClub" wordmark, fit and centered inside `target`.
pub fn draw_wordmark(scene: &mut Scene, target: Rect, color: Color) {
    draw_fitted(scene, wordmark_path(), WORDMARK_VIEWBOX, color, target);
}
