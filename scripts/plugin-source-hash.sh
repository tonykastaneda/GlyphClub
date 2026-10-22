#!/bin/sh
# Prints a fingerprint of a macOS plug-in's sources (everything that goes
# into its build), to tell whether its prebuilt copy matches them. See
# scripts/update-prebuilt-plugin.sh.
#   plugin-source-hash.sh            the Illustrator plug-in (plugin/)
#   plugin-source-hash.sh indesign   the InDesign plug-in (plugin-indesign/,
#                                    which also builds the shared bridge in plugin/Source)
# Windows-only files (*Win.cpp, Resources/Win, CMakeLists.txt) don't go into
# the macOS builds, so they're left out.
cd "$(dirname "$0")/.." || exit 1
case "${1:-illustrator}" in
  indesign)
    { find plugin-indesign/Source plugin-indesign/Resources plugin-indesign/Makefile plugin-indesign/VERSION -type f ! -path '*/Win/*'
      ls plugin/Source/Bridge.h plugin/Source/Bridge.mm plugin/Source/Platform.h plugin/Source/PlatformMac.mm plugin/Source/GlyphClubID.h; } ;;
  *)
    find plugin/Source plugin/Resources plugin/Makefile plugin/VERSION -type f \
      ! -name '*Win.cpp' ! -path 'plugin/Resources/Win/*' ;;
esac | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1
