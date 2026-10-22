#!/bin/zsh
# End-to-end test of every GlyphClub feature, macOS.
#
# Builds the app and CLI, then exercises everything through glyphclub-cli
# and a separate GlyphClub instance. It runs entirely in a throwaway home
# folder (its own catalog, settings, Fonts folder and bridge), starts the
# app hidden as a menu bar app, and uses a throwaway library of test fonts,
# so your real GlyphClub, catalog and fonts are never touched.
#
#   scripts/e2e.sh            # exits 0 when every check passes
#   E2E_VISUAL=1 scripts/e2e.sh   # also shows the window and screenshots it
#
# Prints one PASS/FAIL line per check and a summary.

set -u
REPO=${0:A:h:h}
cd "$REPO" || exit 1

pass=0; fail=0; failures=()
ok()   { pass=$((pass+1)); print -r -- "PASS  $1"; }
bad()  { fail=$((fail+1)); failures+=("$1${2:+: $2}"); print -r -- "FAIL  $1${2:+ ($2)}"; }
check() { local name=$1; shift; if "$@" >/dev/null 2>&1; then ok "$name"; else bad "$name"; fi; }

print "==> Building"
if ! cargo build --release --locked --bin GlyphClub --bin glyphclub-cli >/tmp/glyphclub-e2e-build.log 2>&1; then
  tail -20 /tmp/glyphclub-e2e-build.log
  print "FAIL  build"; exit 1
fi
print "==> Illustrator plug-in build"
if [[ -f plugin/prebuilt/SOURCE_HASH && "$(scripts/plugin-source-hash.sh)" == "$(cat plugin/prebuilt/SOURCE_HASH)" ]]; then
  ok "prebuilt plug-in matches plugin/ sources"
else
  bad "prebuilt plug-in matches plugin/ sources" "run scripts/update-prebuilt-plugin.sh"
fi
[[ $(defaults read "$REPO/plugin/prebuilt/GlyphClub.aip/Contents/Info" CFBundleShortVersionString 2>/dev/null) == $(cat plugin/VERSION) ]] && ok "prebuilt plug-in version" || bad "prebuilt plug-in version"
print "==> Unit tests"
if cargo test --release --locked >/tmp/glyphclub-e2e-test.log 2>&1; then ok "cargo test"; else bad "cargo test" "$(grep -m3 -E 'panicked|FAILED' /tmp/glyphclub-e2e-test.log | tr '\n' ' ')"; fi

APP="$REPO/target/release/GlyphClub"
CLI_BIN="$REPO/target/release/glyphclub-cli"
T=$(mktemp -d /tmp/glyphclub-e2e.XXXXXX)
export HOME="$T/home"
DATA="$HOME/Library/Application Support/glyphclub"
UFONTS="$HOME/Library/Fonts/glyphclub"
LIB="$T/E2E Library"
mkdir -p "$DATA" "$LIB" "$HOME/Library/Fonts"
APP_PID=""
cleanup() {
  [[ -n $APP_PID ]] && kill $APP_PID 2>/dev/null
  sleep 0.5
  rm -rf "$T"
}
trap cleanup EXIT

cli() { "$CLI_BIN" "$@"; }
j() { python3 -I -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

# Hidden menu bar mode keeps the test window off your screen.
visual=${E2E_VISUAL:-0}
menu_bar=true; [[ $visual == 1 ]] && menu_bar=false
print -r -- "{\"menu_bar\": $menu_bar, \"preview_size\": 64, \"sidebar_width\": 220, \"detail_width\": 360}" > "$DATA/settings.json"

# Test fonts: a few bundled icon fonts, as an ordinary library.
cp assets/icon-fonts/typicons/font.ttf "$LIB/typicons.ttf"
cp assets/icon-fonts/open-iconic/font.ttf "$LIB/open-iconic.ttf"
cp assets/icon-fonts/weather-icons/font.ttf "$LIB/weather-icons.ttf"

# A copy left in the activation folder by an older catalog, dated a while ago.
mkdir -p "$UFONTS" && cp "$LIB/typicons.ttf" "$UFONTS/glyphclub-99999.ttf" && touch -t 202001010000 "$UFONTS/glyphclub-99999.ttf"

print "==> Catalog and CLI"
check "status runs" cli status
check "orphaned activation copy removed" test ! -e "$UFONTS/glyphclub-99999.ttf"
out=$(cli add "$LIB" 2>&1); [[ $out == *indexed* ]] && ok "add library" || bad "add library" "$out"
n=$(cli --json libraries | j "[l['fonts'] for l in d['libraries'] if l['name']=='E2E Library'][0]" 2>/dev/null)
[[ $n == 3 ]] && ok "library lists 3 fonts" || bad "library lists 3 fonts" "got $n"
fams=$(cli --json list --library "E2E Library" | j "' | '.join(f['family'] for f in d['families'])")
[[ -n $fams ]] && ok "list by library ($fams)" || bad "list by library"
FAM1=$(cli --json list --library "E2E Library" | j "d['families'][0]['family']")
FAM2=$(cli --json list --library "E2E Library" | j "d['families'][1]['family']")
FAM3=$(cli --json list --library "E2E Library" | j "d['families'][2]['family']")
PS1=$(cli --json info "$FAM1" | j "d['postscriptName']")
[[ -n $PS1 && $PS1 != None ]] && ok "info gives a PostScript name ($PS1)" || bad "info gives a PostScript name"
check "search finds a family" sh -c "\"$CLI_BIN\" --json search \"${FAM2:0:4}\" | grep -q \"$FAM2\""
added=$(python3 -I - "$DATA/catalog.sqlite3" <<'PY'
import pathlib, sqlite3, sys
db = sqlite3.connect(pathlib.Path(sys.argv[1]).as_uri() + "?mode=ro", uri=True)
print(db.execute("select count(*) from fonts where added is not null and folder_id in (select id from folders where path like '%E2E Library%')").fetchone()[0])
PY
)
[[ $added == 3 ]] && ok "date added recorded" || bad "date added recorded" "$added of 3"

print "==> Activation"
cli activate "$FAM1" >/dev/null 2>&1
[[ $(ls "$UFONTS" 2>/dev/null | wc -l | tr -d ' ') -ge 1 ]] && ok "activate installs the font file" || bad "activate installs the font file"
check "activated family is listed active" sh -c "\"$CLI_BIN\" --json list --active --library 'E2E Library' | grep -q \"$FAM1\""
cli deactivate "$FAM1" >/dev/null 2>&1
[[ $(ls "$UFONTS" 2>/dev/null | wc -l | tr -d ' ') == 0 ]] && ok "deactivate removes the font file" || bad "deactivate removes the font file"
cli activate "$FAM2" --temp >/dev/null 2>&1
check "temporary activation is recorded" grep -q '[0-9]' "$DATA/temporary_activations.json"
cli deactivate "$FAM2" >/dev/null 2>&1

print "==> Icons and export"
sets=$(cli --json icons | j "len(d['sets'])")
[[ $sets -ge 28 ]] && ok "icon sets load ($sets)" || bad "icon sets load" "$sets"
cli icon-svg phosphor airplane "$T/a.svg" >/dev/null 2>&1
check "icon exports as SVG paths" grep -q ' d="M' "$T/a.svg"
cp "$LIB/typicons.ttf" "$UFONTS/../user-e2e.ttf"
cli export-user-fonts "$T/export" >/dev/null 2>&1
check "export user fonts" test -d "$T/export/User-FontExport"

print "==> Running app"
cli activate "$FAM3" >/dev/null 2>&1   # a manual activation that must survive a restart
"$APP" >"$T/app.log" 2>&1 &
APP_PID=$!
for i in {1..60}; do [[ -f "$DATA/bridge.json" ]] && cli app state >/dev/null 2>&1 && break; sleep 0.5; done
st() { cli app state | j "$1"; }
check "app answers over the bridge" cli app state
[[ $(st "d['visibleCount'] > 0") == True ]] && ok "app shows fonts" || bad "app shows fonts"
check "manual activation survived a restart" sh -c "\"$CLI_BIN\" --json list --active --library 'E2E Library' | grep -q \"$FAM3\""
check "active font's copy kept by the orphan sweep" sh -c "ls \"$UFONTS\" | grep -q '^glyphclub-'"

cli app nav library "E2E Library" >/dev/null 2>&1
[[ $(st "d['visibleCount']") == 3 ]] && ok "nav to a library" || bad "nav to a library" "$(st "d['visibleCount']")"
cli activate "$FAM1" >/dev/null 2>&1; sleep 1
[[ $(st "[f['active'] for f in d['visible'] if f['family']=='$FAM1'][0]") == True ]] && ok "app reloads after a CLI change" || bad "app reloads after a CLI change"
cli deactivate "$FAM1" >/dev/null 2>&1

for v in list grid; do cli app view $v >/dev/null 2>&1; [[ $(st "d['view']") == $v ]] && ok "view $v" || bad "view $v"; done
cli app select "$FAM2" >/dev/null 2>&1
[[ $(st "d['selected'][0]") == "$FAM2" ]] && ok "select a family" || bad "select a family"
for t in preview sizes glyphs features info; do cli app tab $t >/dev/null 2>&1; [[ $(st "d['info']['tab']") == $t && $(st "d['info']['open']") == True ]] && ok "info tab $t" || bad "info tab $t"; done
cli app info close >/dev/null 2>&1; [[ $(st "d['info']['open']") == False ]] && ok "info closes" || bad "info closes"
cli app sample Hello >/dev/null 2>&1; [[ $(st "d['sample']") == Hello ]] && ok "sample text" || bad "sample text"
cli app sample >/dev/null 2>&1; [[ $(st "d['sample']") == "" ]] && ok "empty sample (font names)" || bad "empty sample (font names)"
cli app size 100 >/dev/null 2>&1; [[ $(st "d['previewSize']") == 100.0 ]] && ok "preview size" || bad "preview size"
cli app search "${FAM2:0:4}" >/dev/null 2>&1; n=$(st "d['visibleCount']"); [[ $n -ge 1 && $n -lt 3 ]] && ok "search filters" || bad "search filters" "$n"
cli app search >/dev/null 2>&1
for s in name added active starred styles format tags; do cli app sort $s >/dev/null 2>&1 && ok "sort $s" || bad "sort $s"; done
cli app expand on >/dev/null 2>&1; [[ $(st "d['expandFamilies']") == True ]] && ok "expand families" || bad "expand families"
cli app expand off >/dev/null 2>&1
cli app sidebar hide >/dev/null 2>&1; [[ $(st "d['sidebar']['shown']") == False ]] && ok "sidebar hides" || bad "sidebar hides"
cli app sidebar show >/dev/null 2>&1; [[ $(st "d['sidebar']['shown']") == True ]] && ok "sidebar shows" || bad "sidebar shows"
cli app theme light >/dev/null 2>&1; sleep 0.3; [[ $(st "d['dark']") == False ]] && ok "light theme" || bad "light theme"
cli app theme dark >/dev/null 2>&1; sleep 0.3; [[ $(st "d['dark']") == True ]] && ok "dark theme" || bad "dark theme"
cli app action ToggleDarkMode >/dev/null 2>&1; sleep 0.3; [[ $(st "d['dark']") == False ]] && ok "menu action (Toggle Dark Mode)" || bad "menu action (Toggle Dark Mode)"
cli app nav icons phosphor >/dev/null 2>&1; [[ $(st "d['nav']['icons']") == Phosphor ]] && ok "icon set view" || bad "icon set view"
cli app nav all >/dev/null 2>&1
for p in general sidebar import auto; do check "settings page $p" cli app settings $p; done
if [[ $visual == 1 ]]; then
  cli app window 700 600 >/dev/null 2>&1; sleep 0.5; [[ $(st "d['compact']") == True ]] && ok "narrow window compacts" || bad "narrow window compacts"
  cli app window 1400 900 >/dev/null 2>&1; sleep 0.5; [[ $(st "d['compact']") == False ]] && ok "wide window un-compacts" || bad "wide window un-compacts"
  cli app screenshot "$T/shot.png" >/dev/null 2>&1; [[ $(stat -f%z "$T/shot.png" 2>/dev/null || echo 0) -gt 20000 ]] && ok "screenshot renders" || bad "screenshot renders"
fi

print "==> Illustrator plug-in protocol"
reply=$(python3 -I - "$DATA/bridge.json" "$PS1" <<'PY'
import json, socket, sys
s = json.load(open(sys.argv[1]))
c = socket.create_connection(("127.0.0.1", s["port"]), timeout=10)
c.sendall((json.dumps({"token": s["token"], "method": "activate", "fonts": [sys.argv[2], "NoSuchFont-Regular"], "pluginVersion": "0.2.0"}) + "\n").encode())
print(c.makefile().readline().strip())
PY
)
[[ $reply == *"\"activated\":[\"$PS1\"]"* ]] && ok "plug-in request activates a missing font" || bad "plug-in request activates a missing font" "$reply"
[[ $reply == *NoSuchFont* ]] && ok "plug-in request reports fonts it can't find" || bad "plug-in request reports fonts it can't find"
bad_token=$(python3 -I - "$DATA/bridge.json" <<'PY'
import json, socket, sys
s = json.load(open(sys.argv[1]))
c = socket.create_connection(("127.0.0.1", s["port"]), timeout=10)
c.sendall(b'{"token":"wrong","method":"ping"}\n')
print(c.makefile().readline().strip())
PY
)
[[ $bad_token == *"bad token"* ]] && ok "bridge refuses a wrong token" || bad "bridge refuses a wrong token" "$bad_token"

print "==> Quit"
# The app runs in the menu bar here (menu_bar=true), where Cmd-Q only puts
# it away; QuitCompletely is the menu bar icon's Quit.
cli app action Quit >/dev/null 2>&1
sleep 1
if [[ $menu_bar == true ]]; then
  kill -0 $APP_PID 2>/dev/null && ok "Cmd-Q keeps it running in the menu bar" || bad "Cmd-Q keeps it running in the menu bar"
  # Put away in the menu bar means no Dock icon either ("UIElement").
  dock_type() { lsappinfo info -only ApplicationType -app "$(lsappinfo find pid=$APP_PID)" 2>/dev/null; }
  [[ $(dock_type) == *UIElement* ]] && ok "Cmd-Q takes it out of the Dock" || bad "Cmd-Q takes it out of the Dock" "$(dock_type)"
  cli app action CloseWindow >/dev/null 2>&1
  sleep 1
  kill -0 $APP_PID 2>/dev/null && ok "Cmd-W keeps it running in the menu bar" || bad "Cmd-W keeps it running in the menu bar"
fi
cli app action QuitCompletely >/dev/null 2>&1
for i in {1..20}; do kill -0 $APP_PID 2>/dev/null || break; sleep 0.5; done
kill -0 $APP_PID 2>/dev/null && bad "app quits" || { ok "app quits"; APP_PID=""; }
[[ -f "$DATA/bridge.json" ]] && bad "quit removes bridge.json" || ok "quit removes bridge.json"
ps_active=$("$CLI_BIN" --json list --active --library 'E2E Library' | j "' '.join(s['postscriptName'] for f in d['families'] for s in f['styles'] if s['active'])")
[[ $ps_active != *"$PS1"* ]] && ok "plug-in activation undone on quit" || bad "plug-in activation undone on quit"
[[ -n $ps_active ]] && ok "manual activation kept on quit" || bad "manual activation kept on quit"

print
print "$pass passed, $fail failed"
if (( fail > 0 )); then
  print "Failures:"; for f in "${failures[@]}"; do print -r -- "  - $f"; done
  exit 1
fi
exit 0
