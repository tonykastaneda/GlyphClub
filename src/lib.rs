//! GlyphClub's core — catalog, scanning, activation, font parsing and the
//! OS integrations — with no UI in it. The gpui front end (`src/next`)
//! builds on this; the original hand-drawn front end (`src/main.rs`) still
//! declares these modules itself until it's retired.

pub mod activate;
pub mod added;
pub mod activation_memory;
pub mod catalog;
pub mod coverage;
pub mod font;
pub mod font_export;
pub mod font_info;
pub mod icons;
pub mod integrations;
pub mod login_item;
pub mod reveal;
pub mod scan;
pub mod store;
pub mod tray;
