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
# Stamps the crate's own version to match — otherwise `env!("CARGO_PKG_VERSION")`
# (what the in-app About overlay, Settings, and the update-checker's version
# comparison all read) stays whatever Cargo.toml last had it at, and every
# release would need a separate manual Cargo.toml edit+commit to keep it in
# sync with the tag actually being built. No-op when $VERSION already
# matches Cargo.toml (a local run with no $VERSION override).
sed -i.bak -E "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml && rm -f Cargo.toml.bak
rustup target add aarch64-apple-darwin >/dev/null 2>&1 || true
cargo build --release --locked --bin GlyphClub --bin glyphclub-cli --target aarch64-apple-darwin

echo "==> Assembling $APP_NAME.app"
rm -rf "$STAGE_DIR"
mkdir -p "$APP_BUNDLE/Contents/MacOS" "$APP_BUNDLE/Contents/Resources"
cp "$BUILD_DIR/$APP_NAME" "$APP_BUNDLE/Contents/MacOS/$APP_NAME"
cp branding/macos/GlyphClub.icns "$APP_BUNDLE/Contents/Resources/GlyphClub.icns"
# The bundled free icon font sets (sidebar > Icon Fonts), with their
# licenses; see scripts/fetch-icon-fonts.py and src/icons.rs.
ditto assets/icon-fonts "$APP_BUNDLE/Contents/Resources/icon-fonts"
sed "s/__VERSION__/$VERSION/g" packaging/macos/Info.plist > "$APP_BUNDLE/Contents/Info.plist"

# The Illustrator plug-in (plugin/, installed from Settings > Auto Activation)
# needs Adobe's Illustrator SDK, which can't live in this public repo — set
# SDK to wherever it is (default ~/Developer/AdobeIllustratorSDK; CI
# downloads it, see release.yml). Without it the app still builds, just
# without the plug-in, and Integrations says so. It's signed on its own
# first (hardened runtime, like the app) since codesign --deep doesn't
# reach into Contents/Resources.
SDK="${SDK:-$HOME/Developer/AdobeIllustratorSDK}"
if [ -d "$SDK/illustratorapi" ]; then
  echo "==> Building the Illustrator plug-in"
  make -C plugin clean
  make -C plugin SDK="$SDK" SIGN_ID="$SIGN_IDENTITY"
  ditto plugin/build/GlyphClub.aip "$APP_BUNDLE/Contents/Resources/GlyphClub.aip"
elif [ -d plugin/prebuilt/GlyphClub.aip ]; then
  # No SDK (CI): ship the prebuilt plug-in, but only if it was built from
  # the current sources (scripts/update-prebuilt-plugin.sh refreshes it).
  if [ "$(scripts/plugin-source-hash.sh)" != "$(cat plugin/prebuilt/SOURCE_HASH)" ]; then
    echo "ERROR: plugin/prebuilt is out of date with plugin/; run scripts/update-prebuilt-plugin.sh" >&2
    exit 1
  fi
  echo "==> Using the prebuilt Illustrator plug-in"
  ditto plugin/prebuilt/GlyphClub.aip "$APP_BUNDLE/Contents/Resources/GlyphClub.aip"
  codesign --force --options runtime --timestamp --sign "$SIGN_IDENTITY" "$APP_BUNDLE/Contents/Resources/GlyphClub.aip"
  codesign --verify --strict "$APP_BUNDLE/Contents/Resources/GlyphClub.aip"
else
  echo "==> WARNING: no Illustrator SDK at $SDK and no prebuilt plug-in, packaging without it"
fi

# Fail before signing if the application metadata or branding is incomplete.
# The Photoshop plug-in (plugin-photoshop/), built here from Adobe's
# Photoshop SDK (default ~/Developer/AdobePhotoshopSDK; CI decrypts this
# repo's own copy, see release.yml). Signed on its own first, like the
# Illustrator one.
PS_SDK="${PS_SDK:-$HOME/Developer/AdobePhotoshopSDK/pluginsdk/photoshopapi}"
if [ -d "$PS_SDK/photoshop" ]; then
  echo "==> Building the Photoshop plug-in"
  make -C plugin-photoshop clean
  make -C plugin-photoshop PS_SDK="$PS_SDK" SIGN_ID="$SIGN_IDENTITY"
  ditto plugin-photoshop/build/GlyphClub.plugin "$APP_BUNDLE/Contents/Resources/GlyphClub Photoshop.plugin"
elif [ -n "${REQUIRE_PS_PLUGIN:-}" ]; then
  echo "ERROR: no Photoshop SDK at $PS_SDK" >&2
  exit 1
else
  echo "==> WARNING: no Photoshop SDK at $PS_SDK, packaging without the Photoshop plug-in"
fi

plutil -lint "$APP_BUNDLE/Contents/Info.plist"
test -s "$APP_BUNDLE/Contents/MacOS/GlyphClub"
test -s "$APP_BUNDLE/Contents/Resources/GlyphClub.icns"
test -d "$APP_BUNDLE/Contents/Resources/icon-fonts/bootstrap-icons"

echo "==> Code signing with: $SIGN_IDENTITY"
codesign --force --deep --options runtime --timestamp \
  --entitlements packaging/macos/entitlements.plist \
  --sign "$SIGN_IDENTITY" \
  "$APP_BUNDLE"
codesign --verify --deep --strict --verbose=2 "$APP_BUNDLE"

echo "==> Building DMG"
# Deliberately not version-suffixed — the docs site's download button links
# straight to releases/latest/download/GlyphClub.dmg, which only resolves
# if every release publishes an asset under this exact same name.
DMG_PATH="$STAGE_DIR/$APP_NAME.dmg"
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

# Sign the distribution container as well as the app it contains, so
# Gatekeeper can validate the downloaded DMG itself.
echo "==> Signing disk image"
codesign --force --sign "$SIGN_IDENTITY" --timestamp "$DMG_PATH"
codesign --verify --strict --verbose=2 "$DMG_PATH"

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
xcrun stapler validate "$DMG_PATH"
spctl --assess --type open --context context:primary-signature --verbose=2 "$DMG_PATH"

echo "==> Done: $DMG_PATH"
