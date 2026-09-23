#!/usr/bin/env bash
# Builds, signs, notarizes, and DMG-packages GlyphClub for macOS
# (Apple Silicon only — see the packaging discussion this shipped with for
# why: it's where the Mac market actually is, and it keeps this script and
# CI job to one target instead of a lipo-merged universal binary).
#
# Run locally (uses your own Developer ID cert + a notarytool keychain
# profile already set up on this machine):
#   ./scripts/package-macos.sh
#
# Run in CI (no keychain profile — auth is an App Store Connect API key
# instead, via NOTARY_KEY_PATH/NOTARY_KEY_ID/NOTARY_ISSUER_ID): see
# .github/workflows/release.yml, which sets those and SIGN_IDENTITY.
set -euo pipefail
cd "$(dirname "$0")/.."

VERSION="${VERSION:-$(grep -m1 '^version' Cargo.toml | sed -E 's/version = "(.*)"/\1/')}"
SIGN_IDENTITY="${SIGN_IDENTITY:-Developer ID Application: Anthony Castaneda (GM98GV6S97)}"
NOTARY_PROFILE="${NOTARY_PROFILE:-glyphclub-notary}"
APP_NAME="GlyphClub"
BUILD_DIR="target/aarch64-apple-darwin/release"
STAGE_DIR="target/package/macos"
APP_BUNDLE="$STAGE_DIR/$APP_NAME.app"

echo "==> Building release ($VERSION)"
rustup target add aarch64-apple-darwin >/dev/null 2>&1 || true
cargo build --release --target aarch64-apple-darwin

echo "==> Assembling $APP_NAME.app"
rm -rf "$STAGE_DIR"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"
cp "$BUILD_DIR/$APP_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp branding/macos/GlyphClub.icns "$APP_BUNDLE/Contents/Resources/GlyphClub.icns"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP_BUNDLE/Contents/Info.plist"

echo "==> Code signing with: $SIGN_IDENTITY"
codesign --force --deep --options runtime --timestamp \
  --entitlements packaging/macos/entitlements.plist \
  --sign "$SIGN_IDENTITY" \
  "$APP_BUNDLE"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

echo "==> Building DMG"
DMG_PATH="$STAGE_DIR/$APP_NAME-$VERSION.dmg"
rm -f "$DMG_PATH"
if command -v create-dmg >/dev/null 2>&1; then
  create-dmg \
    --volname "$APP_NAME" \
    --window-size 540 380 \
    --icon-size 128 \
    --icon "$APP_NAME.app" 130 170 \
    --app-drop-link 410 170 \
    "$DMG_PATH" \
    "$APP_BUNDLE" || true
fi
if [ ! -f "$DMG_PATH" ]; then
  # Plain fallback if create-dmg isn't installed (`brew install create-dmg`
  # for the nicer drag-to-Applications layout) — still a real, working DMG.
  STAGING="$STAGE_DIR/dmg-root"
  rm -rf "$STAGING"
  mkdir -p "$STAGING"
  cp -R "$APP_BUNDLE" "$STAGING/"
  ln -s /Applications "$STAGING/Applications"
  hdiutil create -volname "$APP_NAME" -srcfolder "$STAGING" -ov -format UDZO "$DMG_PATH"
fi

echo "==> Notarizing"
if [ -n "${NOTARY_KEY_PATH:-}" ]; then
  xcrun notarytool submit "$DMG_PATH" \
    --key "$NOTARY_KEY_PATH" --key-id "$NOTARY_KEY_ID" --issuer "$NOTARY_ISSUER_ID" \
    --wait
else
  xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
fi

echo "==> Stapling"
xcrun stapler staple "$DMG_PATH"

echo "==> Done: $DMG_PATH"
