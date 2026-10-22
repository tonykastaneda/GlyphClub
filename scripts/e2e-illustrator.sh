#!/bin/zsh
# End-to-end test of GlyphClub with Adobe Illustrator, macOS.
#
# Unlike scripts/e2e.sh this runs against your real GlyphClub and catalog,
# because the Illustrator plug-in always finds GlyphClub through your own
# ~/Library/Application Support/glyphclub/bridge.json. It needs:
#   - GlyphClub running (scripts/e2e-illustrator.sh doesn't start it),
#   - Illustrator running with the GlyphClub plug-in loaded, and no
#     documents open (nothing of yours is touched),
#   - two fonts from your libraries that aren't active (defaults below).
#
#   scripts/e2e-illustrator.sh ["Missing Font Family" "Apply Font Family"]
#
# It checks: a document whose font is missing gets it activated by the
# plug-in on open, and the cards' Ai button applies a font to the selected
# text. It leaves no fonts activated and no documents open.

set -u
REPO=${0:A:h:h}
cd "$REPO" || exit 1
CLI="$REPO/target/release/glyphclub-cli"
MISSING=${1:-Road Rage}
APPLY=${2:-Pixoryn}
T=$(mktemp -d /tmp/glyphclub-ai-e2e.XXXXXX)
DOC="$T/missing-font-test.ai"

pass=0; fail=0
ok()  { pass=$((pass+1)); print -r -- "PASS  $1"; }
bad() { fail=$((fail+1)); print -r -- "FAIL  $1${2:+ ($2)}"; }
ai()  { osascript -e "tell application id \"com.adobe.illustrator\" to do javascript \"$1\"" 2>&1; }
ps_of() { "$CLI" --json info "$1" | python3 -I -c "import json,sys; print(json.load(sys.stdin)['postscriptName'])"; }
active_families() { "$CLI" --json list --active | python3 -I -c "import json,sys; print('|'.join(f['family'] for f in json.load(sys.stdin)['families']))"; }
font_in_ai() { ai "try { app.textFonts.getByName('$1').name } catch (e) { 'missing' }"; }
wait_for() { local want=$1; shift; for i in {1..30}; do [[ $("$@") == $want ]] && return 0; sleep 1; done; return 1; }

cleanup() {
  ai "while (app.documents.length) app.documents[0].close(SaveOptions.DONOTSAVECHANGES); 'ok'" >/dev/null
  "$CLI" deactivate "$MISSING" "$APPLY" >/dev/null 2>&1
  rm -rf "$T"
}
trap cleanup EXIT

"$CLI" app state >/dev/null 2>&1 || { print "GlyphClub isn't running"; exit 1; }
[[ $(ai "app.documents.length") == 0 ]] || { print "Close your Illustrator documents first (nothing of yours is touched)"; exit 1; }
# A leftover dialog (e.g. Missing Fonts) blocks Illustrator from opening documents.
dialogs=$(osascript -l JavaScript -e 'ObjC.import("CoreGraphics"); ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(1,0))).filter(w => /^Adobe Illustrator/.test(w.kCGWindowOwnerName) && w.kCGWindowLayer == 0 && w.kCGWindowName && !/^Adobe Illustrator/.test(w.kCGWindowName)).map(w => w.kCGWindowName).join(", ")' 2>/dev/null)
[[ -z $dialogs ]] || { print "Close Illustrator's open dialog first: $dialogs"; exit 1; }
PS_MISSING=$(ps_of "$MISSING"); PS_APPLY=$(ps_of "$APPLY")
[[ -n $PS_MISSING && -n $PS_APPLY ]] || { print "Couldn't find $MISSING / $APPLY in your libraries"; exit 1; }
"$CLI" deactivate "$MISSING" "$APPLY" >/dev/null 2>&1

print "==> Plug-in: missing fonts activate when a document opens"
"$CLI" activate "$MISSING" >/dev/null
wait_for "$PS_MISSING" font_in_ai "$PS_MISSING" && ok "Illustrator sees an activated font" || bad "Illustrator sees an activated font"
ai "app.userInteractionLevel = UserInteractionLevel.DONTDISPLAYALERTS; var d = app.documents.add(DocumentColorSpace.RGB, 800, 400); var t = d.textFrames.add(); t.contents = 'GlyphClub plug-in test'; t.textRange.characterAttributes.textFont = app.textFonts.getByName('$PS_MISSING'); t.textRange.characterAttributes.size = 60; d.saveAs(new File('$DOC')); d.close(SaveOptions.DONOTSAVECHANGES); 'saved'" >/dev/null
[[ -f $DOC ]] && ok "test document saved" || bad "test document saved"
"$CLI" deactivate "$MISSING" >/dev/null
wait_for missing font_in_ai "$PS_MISSING" && ok "font is missing after deactivating" || bad "font is missing after deactivating"
ai "app.userInteractionLevel = UserInteractionLevel.DONTDISPLAYALERTS; app.open(new File('$DOC')); 'opened'" >/dev/null
wait_for "$MISSING" active_families && ok "plug-in asked GlyphClub, which activated it" || bad "plug-in asked GlyphClub, which activated it" "active: $(active_families)"
grep -q '[0-9]' ~/Library/Application\ Support/glyphclub/temporary_activations.json 2>/dev/null && ok "plug-in activation is temporary" || bad "plug-in activation is temporary"
wait_for "$PS_MISSING" font_in_ai "$PS_MISSING" && ok "Illustrator has the font back" || bad "Illustrator has the font back"
wait_for "$PS_MISSING" ai "app.activeDocument.textFrames[0].textRange.characterAttributes.textFont.name" && ok "document text uses the real font" || bad "document text uses the real font"

print "==> Ai button: apply a font to the selected text"
ai "var d = app.activeDocument; d.selection = null; d.textFrames[0].selected = true; 'ok'" >/dev/null
"$CLI" app illustrator-apply "$APPLY" >/dev/null
wait_for "$PS_APPLY" ai "app.activeDocument.textFrames[0].textRange.characterAttributes.textFont.name" && ok "Ai button applies the font" || bad "Ai button applies the font" "$("$CLI" app state | python3 -I -c "import json,sys; print(json.load(sys.stdin).get('notices', 'no notices: is this GlyphClub build too old?'))")"

print
print "$pass passed, $fail failed"
(( fail == 0 ))
