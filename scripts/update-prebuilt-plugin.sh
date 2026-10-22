#!/bin/sh
# Rebuilds a macOS plug-in and stores it in its prebuilt/ folder, with a
# fingerprint of the sources it was built from. Release builds use these
# copies when Adobe's SDK isn't available (CI has no Illustrator SDK, and only
# the Windows part of the InDesign SDK), signing them with the release's
# Developer ID. Run it whenever the plug-in changes; the end-to-end tests
# (scripts/e2e.sh) fail while the Illustrator copy is out of date.
#   update-prebuilt-plugin.sh            Illustrator (needs ~/Developer/AdobeIllustratorSDK or SDK=...)
#   update-prebuilt-plugin.sh indesign   InDesign (needs ~/Developer/AdobeInDesignSDK or ID_SDK=...)
set -eu
cd "$(dirname "$0")/.."
case "${1:-illustrator}" in
  indesign)
    make -C plugin-indesign clean
    make -C plugin-indesign ${ID_SDK:+ID_SDK="$ID_SDK"}
    rm -rf plugin-indesign/prebuilt/GlyphClub.InDesignPlugin
    mkdir -p plugin-indesign/prebuilt
    ditto plugin-indesign/build/GlyphClub.InDesignPlugin plugin-indesign/prebuilt/GlyphClub.InDesignPlugin
    scripts/plugin-source-hash.sh indesign > plugin-indesign/prebuilt/SOURCE_HASH
    echo "Updated plugin-indesign/prebuilt/GlyphClub.InDesignPlugin ($(cat plugin-indesign/VERSION), $(cut -c1-12 plugin-indesign/prebuilt/SOURCE_HASH))"
    ;;
  *)
    make -C plugin clean
    make -C plugin ${SDK:+SDK="$SDK"}
    rm -rf plugin/prebuilt/GlyphClub.aip
    mkdir -p plugin/prebuilt
    ditto plugin/build/GlyphClub.aip plugin/prebuilt/GlyphClub.aip
    scripts/plugin-source-hash.sh > plugin/prebuilt/SOURCE_HASH
    echo "Updated plugin/prebuilt/GlyphClub.aip ($(cat plugin/VERSION), $(cut -c1-12 plugin/prebuilt/SOURCE_HASH))"
    ;;
esac
