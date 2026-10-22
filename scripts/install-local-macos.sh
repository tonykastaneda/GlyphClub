#!/usr/bin/env bash
# Builds GlyphClub.app from this checkout and installs it to /Applications,
# then opens it the normal way (as from Finder or Spotlight) — a real app
# bundle like the release's, for testing a change in a minute or two
# instead of waiting on the release workflow. Same bundle as
# scripts/package-macos.sh, but signed locally (ad hoc) and not notarized
# or put in a DMG, so it's for this Mac only.
#
#   ./scripts/install-local-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

APP_NAME="GlyphClub"
VERSION="$(sed -nE 's/^version = "(.*)"/\1/p' Cargo.toml | head -1)"
STAGE="target/local-app/$APP_NAME.app"

echo "==> Building $APP_NAME $VERSION"
cargo build --release --bin GlyphClub --bin glyphclub-cli

echo "==> Assembling $APP_NAME.app"
rm -rf "$STAGE"
mkdir -p "$STAGE/Contents/MacOS" "$STAGE/Contents/Resources"
cp "target/release/$APP_NAME" "$STAGE/Contents/MacOS/$APP_NAME"
cp branding/macos/GlyphClub.icns "$STAGE/Contents/Resources/GlyphClub.icns"
ditto assets/icon-fonts "$STAGE/Contents/Resources/icon-fonts"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$STAGE/Contents/Info.plist"
if [ -d plugin/build/GlyphClub.aip ]; then
  ditto plugin/build/GlyphClub.aip "$STAGE/Contents/Resources/GlyphClub.aip"
elif [ -d plugin/prebuilt/GlyphClub.aip ]; then
  ditto plugin/prebuilt/GlyphClub.aip "$STAGE/Contents/Resources/GlyphClub.aip"
fi
PS_SDK="${PS_SDK:-$HOME/Developer/AdobePhotoshopSDK/pluginsdk/photoshopapi}"
if [ -d "$PS_SDK/photoshop" ]; then
  make -s -C plugin-photoshop PS_SDK="$PS_SDK" >/dev/null
  ditto plugin-photoshop/build/GlyphClub.plugin "$STAGE/Contents/Resources/GlyphClub Photoshop.plugin"
fi
ID_SDK="${ID_SDK:-$HOME/Developer/AdobeInDesignSDK}"
if [ -d "$ID_SDK/source/public" ]; then
  make -s -C plugin-indesign ID_SDK="$ID_SDK" >/dev/null
  ditto plugin-indesign/build/GlyphClub.InDesignPlugin "$STAGE/Contents/Resources/GlyphClub InDesign.InDesignPlugin"
fi
codesign --force --deep --sign - "$STAGE"

echo "==> Installing to /Applications"
if pgrep -x "$APP_NAME" >/dev/null; then
  # The real quit (Cmd-Q only hides GlyphClub while it lives in the menu bar).
  osascript -e "quit app \"$APP_NAME\"" >/dev/null 2>&1 || true
  for _ in $(seq 1 20); do pgrep -x "$APP_NAME" >/dev/null || break; sleep 0.25; done
  pkill -x "$APP_NAME" 2>/dev/null || true
fi
rm -rf "/Applications/$APP_NAME.app"
ditto "$STAGE" "/Applications/$APP_NAME.app"

echo "==> Opening $APP_NAME $VERSION"
open -a "/Applications/$APP_NAME.app"
