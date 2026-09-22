<div align="center">
  <img src="branding/SVG/icon.svg" width="120" alt="GlyphClub icon" />

  # GlyphClub

  **A real cross-platform font manager.** Not a fancy web app wrapper.

  [![Build](https://github.com/tonykastaneda/GlyphClub/actions/workflows/build.yml/badge.svg)](https://github.com/tonykastaneda/GlyphClub/actions/workflows/build.yml)
</div>

GlyphClub activates and deactivates fonts from whatever folder your team
already syncs — Dropbox, Google Drive, iCloud, OneDrive, SMB, anything.
There's no OAuth, no cloud account, and no web app pretending to be a
desktop app: it's a native macOS/Windows application that just watches and
reads plain folders, built from scratch on `winit` + `wgpu` + `vello` +
`parley` with no widget toolkit underneath it.

## Features

- **Grid and list views** of your whole library, virtualized to stay smooth
  scrolling through very large libraries (100k+ fonts).
- **Live previews**, including System Fonts, with a dimmed fallback for any
  tile whose font genuinely lacks glyphs for the current sample text.
- **Starred and Recent** filters alongside your own library folders, each
  individually hideable from the sidebar via Settings.
- **One-click activation/deactivation**, plus a temporary-activation mode,
  reconciled against every library folder you have open.
- **A detail panel** for the selected font — a real live-type Preview
  editor (click-drag selection, word/line/paragraph keyboard navigation,
  alignment, point size), a Sizes specimen ladder, a searchable Glyphs
  grid with Unicode block filtering, and an Info tab with extracted
  metadata. Resizable by dragging its edge.
- **Light and Dark themes**, following System Default or set explicitly.
- **A real native menu bar on macOS** (via `muda`), and a matching
  hand-rolled one on Windows.

## Building and running

```sh
cargo run
```

Requires a recent stable Rust toolchain. Builds natively on macOS and
Windows; the `[[bin]]` is named `GlyphClub` specifically so the built
executable's own filename is what macOS's menu bar falls back to for the
bold application-menu title (there's no `.app` bundle/`Info.plist` yet to
pull a display name from otherwise).

## Project layout

- `src/` — the application itself: `app.rs` holds shared state, `ui/` is
  the immediate-mode rendering + hit-testing for every screen, `catalog.rs`/
  `scan.rs`/`store.rs` own the SQLite-backed font catalog and folder
  watching, and `activate.rs` does the actual OS-level font
  activation/deactivation.
- `branding/` — the source SVGs/icon assets this README and the app's own
  UI (format badges, dock icon, wordmark) are drawn from.
- `docs/` — the project's marketing site.
