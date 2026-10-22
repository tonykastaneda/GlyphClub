#!/bin/sh
# Rebuilds the Illustrator plug-in and stores it in plugin/prebuilt/, with a
# fingerprint of the sources it was built from. Release builds use this copy
# when Adobe's Illustrator SDK isn't available (CI has no SDK), signing it
# with the release's Developer ID. Run it whenever plugin/ changes; the
# end-to-end tests (scripts/e2e.sh) fail while the copy is out of date.
#
# Needs the SDK at ~/Developer/AdobeIllustratorSDK (or SDK=...).
set -eu
cd "$(dirname "$0")/.."
make -C plugin clean
make -C plugin ${SDK:+SDK="$SDK"}
rm -rf plugin/prebuilt/GlyphClub.aip
mkdir -p plugin/prebuilt
ditto plugin/build/GlyphClub.aip plugin/prebuilt/GlyphClub.aip
scripts/plugin-source-hash.sh > plugin/prebuilt/SOURCE_HASH
echo "Updated plugin/prebuilt/GlyphClub.aip ($(cat plugin/VERSION), $(cat plugin/prebuilt/SOURCE_HASH | cut -c1-12))"
