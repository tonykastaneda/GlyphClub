//! GlyphClub’s shared core: catalog, scanning, activation, fonts and OS integrations.
//! The main GPUI application in `src/gui` and the CLI both build on this.

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
pub mod library_file;
pub mod login_item;
pub mod reveal;
pub mod scan;
pub mod store;
pub mod tray;
