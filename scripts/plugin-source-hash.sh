#!/bin/sh
# Prints a fingerprint of the Illustrator plug-in's sources (everything that
# goes into plugin/build/GlyphClub.aip), to tell whether the prebuilt copy in
# plugin/prebuilt/ matches them. See scripts/update-prebuilt-plugin.sh.
cd "$(dirname "$0")/.." || exit 1
find plugin/Source plugin/Resources plugin/Makefile plugin/VERSION -type f | LC_ALL=C sort | xargs shasum -a 256 | shasum -a 256 | cut -d' ' -f1
