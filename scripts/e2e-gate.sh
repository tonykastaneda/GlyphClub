#!/bin/zsh
# Claude Code Stop hook: before Claude finishes a turn, run the end-to-end
# tests (scripts/e2e.sh) if the code changed since they last passed. A
# failure blocks the stop (exit 2) and hands Claude the failures to fix.
#
# "The code" is the app, CLI, scripts, plug-in, icon sets and build files;
# a fingerprint of them is kept in .claude/e2e-passed after each pass, so
# turns that don't touch code cost nothing.
cd "${0:A:h:h}" || exit 0
cat >/dev/null   # the hook's JSON input isn't needed

paths=(src scripts plugin/Source plugin/Resources plugin/Makefile plugin/VERSION plugin/prebuilt assets Cargo.toml Cargo.lock build.rs vendor .github)
fingerprint=$(
  {
    git ls-files -s -- $paths
    git diff -- $paths
    git ls-files --others --exclude-standard -z -- $paths | xargs -0 shasum -a 256 2>/dev/null
  } | shasum -a 256 | cut -d' ' -f1
)
stamp=.claude/e2e-passed
[[ -f $stamp && $(cat $stamp) == $fingerprint ]] && exit 0

log=$(mktemp /tmp/glyphclub-e2e-gate.XXXXXX)
if scripts/e2e.sh >"$log" 2>&1; then
  mkdir -p .claude && print -r -- "$fingerprint" > $stamp
  print -r -- "{\"systemMessage\": \"GlyphClub end-to-end tests passed ($(grep -c '^PASS' "$log") checks)\"}"
  rm -f "$log"
  exit 0
fi
{
  print "GlyphClub's end-to-end tests failed after the latest code changes. Fix these before finishing:"
  grep -E '^FAIL|^  - ' "$log" | head -40
  print "Full log: $log"
} >&2
exit 2
