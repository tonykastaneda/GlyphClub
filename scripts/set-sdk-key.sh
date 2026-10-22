#!/bin/sh
# Saves the Illustrator SDK key (the same one Slippy's CI uses, kept in the
# login Keychain as "Slippy CI SDK key") as this repo's SDK_KEY GitHub
# secret, which the Windows builds use to decrypt Adobe's SDK.
set -eu
cd "$(dirname "$0")/.."
key=$(security find-generic-password -s "Slippy CI SDK key" -w)
printf '%s' "$key" | gh secret set SDK_KEY
