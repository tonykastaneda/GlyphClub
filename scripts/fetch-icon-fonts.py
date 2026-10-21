#!/usr/bin/env python3
"""Fetches the free icon fonts GlyphClub bundles, from each project's own
published package (via the jsDelivr CDN), into assets/icon-fonts/.

Each set gets its own folder:
    font.ttf / font.otf   the icon font
    names.json            [[name, codepoint], ...] from the set's own CSS
                          (or Google's codepoints file for Material Icons)
    LICENSE               the set's license, as published
    set.json              display name, version, source and license name

Run it to add or update sets; its output is committed, so building the app
never needs the network. Only sets whose licenses allow bundling them in an
app are listed (no Remix Icon 4: its license forbids icon collections).

    python3 scripts/fetch-icon-fonts.py
"""

import json
import re
import struct
import sys
import urllib.request
import zlib
from pathlib import Path

NPM = "https://cdn.jsdelivr.net/npm/"
GH = "https://cdn.jsdelivr.net/gh/"
OFL = "https://cdn.jsdelivr.net/gh/google/fonts@main/ofl/notosans/OFL.txt"

MATERIAL = GH + "google/material-design-icons@master/font/"

SETS = [
    # Font Awesome Free 6: fonts SIL OFL 1.1, icons CC BY 4.0.
    dict(slug="font-awesome-6-solid", name="Font Awesome 6 Solid", version="6.7.2", license="SIL OFL 1.1 / CC BY 4.0",
         font=NPM + "@fortawesome/fontawesome-free@6.7.2/webfonts/fa-solid-900.ttf",
         css=NPM + "@fortawesome/fontawesome-free@6.7.2/css/all.css", prefix="fa-",
         license_url=NPM + "@fortawesome/fontawesome-free@6.7.2/LICENSE.txt"),
    dict(slug="font-awesome-6-regular", name="Font Awesome 6 Regular", version="6.7.2", license="SIL OFL 1.1 / CC BY 4.0",
         font=NPM + "@fortawesome/fontawesome-free@6.7.2/webfonts/fa-regular-400.ttf",
         css=NPM + "@fortawesome/fontawesome-free@6.7.2/css/all.css", prefix="fa-",
         license_url=NPM + "@fortawesome/fontawesome-free@6.7.2/LICENSE.txt"),
    dict(slug="font-awesome-6-brands", name="Font Awesome 6 Brands", version="6.7.2", license="SIL OFL 1.1 / CC BY 4.0",
         font=NPM + "@fortawesome/fontawesome-free@6.7.2/webfonts/fa-brands-400.ttf",
         css=NPM + "@fortawesome/fontawesome-free@6.7.2/css/all.css", prefix="fa-",
         license_url=NPM + "@fortawesome/fontawesome-free@6.7.2/LICENSE.txt"),
    dict(slug="font-awesome-4", name="Font Awesome 4", version="4.7.0", license="SIL OFL 1.1",
         font=NPM + "font-awesome@4.7.0/fonts/fontawesome-webfont.ttf",
         css=NPM + "font-awesome@4.7.0/css/font-awesome.css", prefix="fa-", license_url=OFL),
    # Material Icons (Google): Apache 2.0.
    *[
        dict(slug=f"material-icons-{key}", name=f"Material Icons {label}", version="4.0.0", license="Apache 2.0",
             font=MATERIAL + file, codepoints=MATERIAL + file.rsplit(".", 1)[0] + ".codepoints",
             license_url=GH + "google/material-design-icons@master/LICENSE")
        for key, label, file in [
            ("filled", "Filled", "MaterialIcons-Regular.ttf"),
            ("outlined", "Outlined", "MaterialIconsOutlined-Regular.otf"),
            ("round", "Round", "MaterialIconsRound-Regular.otf"),
            ("sharp", "Sharp", "MaterialIconsSharp-Regular.otf"),
            ("two-tone", "Two Tone", "MaterialIconsTwoTone-Regular.otf"),
        ]
    ],
    # Material Design Icons (Pictogrammers): Apache 2.0.
    dict(slug="material-design-icons", name="Material Design Icons", version="7.4.47", license="Apache 2.0",
         font=NPM + "@mdi/font@7.4.47/fonts/materialdesignicons-webfont.ttf",
         css=NPM + "@mdi/font@7.4.47/css/materialdesignicons.css", prefix="mdi-",
         license_url=NPM + "@mdi/font@7.4.47/LICENSE"),
    # Phosphor: MIT.
    *[
        dict(slug=f"phosphor{'-' + w.lower() if w != 'Regular' else ''}", name=f"Phosphor{'-' + w if w != 'Regular' else ''}",
             version="2.1.2", license="MIT",
             font=NPM + f"@phosphor-icons/web@2.1.2/src/{w.lower()}/{'Phosphor' if w == 'Regular' else 'Phosphor-' + w}.ttf",
             css=NPM + f"@phosphor-icons/web@2.1.2/src/{w.lower()}/style.css", prefix="ph-",
             license_url=NPM + "@phosphor-icons/web@2.1.2/LICENSE")
        for w in ["Regular", "Thin", "Light", "Bold", "Fill", "Duotone"]
    ],
    # Tabler Icons: MIT.
    dict(slug="tabler-icons", name="Tabler Icons", version="3.49.0", license="MIT",
         font=NPM + "@tabler/icons-webfont@3.49.0/dist/fonts/tabler-icons.ttf",
         css=NPM + "@tabler/icons-webfont@3.49.0/dist/tabler-icons.css", prefix="ti-",
         license_url=NPM + "@tabler/icons-webfont@3.49.0/LICENSE"),
    dict(slug="tabler-icons-filled", name="Tabler Icons Filled", version="3.49.0", license="MIT",
         font=NPM + "@tabler/icons-webfont@3.49.0/dist/fonts/tabler-icons-filled.ttf",
         css=NPM + "@tabler/icons-webfont@3.49.0/dist/tabler-icons-filled.css", prefix="ti-",
         license_url=NPM + "@tabler/icons-webfont@3.49.0/LICENSE"),
    # Lucide: ISC.
    dict(slug="lucide", name="Lucide", version="1.61.0", license="ISC",
         font=NPM + "lucide-static@1.61.0/font/lucide.ttf",
         css=NPM + "lucide-static@1.61.0/font/lucide.css", prefix="icon-",
         license_url=NPM + "lucide-static@1.61.0/LICENSE"),
    # Boxicons: MIT.
    dict(slug="boxicons", name="boxicons", version="2.1.4", license="MIT",
         font=NPM + "boxicons@2.1.4/fonts/boxicons.ttf",
         css=NPM + "boxicons@2.1.4/css/boxicons.css", prefix="bx-|bxs-|bxl-",
         license_url=NPM + "boxicons@2.1.4/LICENSE"),
    # Remix Icon 3 (Apache 2.0; version 4 changed to a license that forbids this).
    dict(slug="remixicon", name="remixicon", version="3.7.0", license="Apache 2.0",
         font=NPM + "remixicon@3.7.0/fonts/remixicon.ttf",
         css=NPM + "remixicon@3.7.0/fonts/remixicon.css", prefix="ri-",
         license_url=NPM + "remixicon@3.7.0/License"),
    # Ionicons 4: MIT.
    dict(slug="ionicons", name="Ionicons", version="4.6.3", license="MIT",
         font=NPM + "ionicons@4.6.3/dist/fonts/ionicons.ttf",
         css=NPM + "ionicons@4.6.3/dist/css/ionicons.css", prefix="ion-md-|ion-ios-|ion-logo-|ion-",
         license_url=NPM + "ionicons@4.6.3/LICENSE"),
    # Bootstrap Icons: MIT (shipped as WOFF only; unpacked to TTF below).
    dict(slug="bootstrap-icons", name="Bootstrap Icons", version="1.13.2", license="MIT",
         woff=NPM + "bootstrap-icons@1.13.2/font/fonts/bootstrap-icons.woff",
         css=NPM + "bootstrap-icons@1.13.2/font/bootstrap-icons.css", prefix="bi-",
         license_url=NPM + "bootstrap-icons@1.13.2/LICENSE"),
    # Typicons: SIL OFL 1.1.
    dict(slug="typicons", name="Typicons", version="2.1.2", license="SIL OFL 1.1",
         font=NPM + "typicons.font@2.1.2/src/font/typicons.ttf",
         css=NPM + "typicons.font@2.1.2/src/font/typicons.css", prefix="typcn-",
         license_url=NPM + "typicons.font@2.1.2/LICENCE.md"),
    # Weather Icons: SIL OFL 1.1.
    dict(slug="weather-icons", name="Weather Icons", version="1.3.2", license="SIL OFL 1.1",
         font=NPM + "weather-icons@1.3.2/font/weathericons-regular-webfont.ttf",
         css=NPM + "weather-icons@1.3.2/css/weather-icons.css", prefix="wi-", license_url=OFL),
    # Devicon: MIT.
    dict(slug="devicon", name="devicon", version="2.17.0", license="MIT",
         font=NPM + "devicon@2.17.0/fonts/devicon.ttf",
         css=NPM + "devicon@2.17.0/devicon-base.css", prefix="devicon-",
         license_url=NPM + "devicon@2.17.0/LICENSE"),
    # Foundation Icons: MIT.
    dict(slug="foundation-icons", name="Foundation", version="2.0.0", license="MIT",
         font=NPM + "foundation-icons@2.0.0/foundation-icons.ttf",
         css=NPM + "foundation-icons@2.0.0/foundation-icons.css", prefix="fi-",
         license_text="MIT License. Foundation Icons by ZURB (package.json declares MIT)."),
    # Open Iconic: font SIL OFL 1.1, icons MIT.
    dict(slug="open-iconic", name="Open Iconic", version="1.1.1", license="SIL OFL 1.1 / MIT",
         font=NPM + "open-iconic@1.1.1/font/fonts/open-iconic.ttf",
         css=NPM + "open-iconic@1.1.1/font/css/open-iconic.css", prefix="oi-",
         license_url=NPM + "open-iconic@1.1.1/FONT-LICENSE"),
]


def fetch(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "GlyphClub-IconFetch"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


RULE = re.compile(r"([^{}]+)\{([^}]*)\}")
# An escape like "\\e900", or (Ionicons) the character itself.
CONTENT = re.compile(r"""(?:content|--fa|--fa-primary)\s*:\s*["'](?:\\([0-9a-fA-F]{2,6})|([^"'\\]))["']""")
CLASS = re.compile(r"\.([A-Za-z0-9_-]+)(?:::?before)?\s*$")
GLYPH_ATTR = re.compile(r"\[data-glyph=['\"]?([A-Za-z0-9_-]+)['\"]?\]")


def names_from_css(css: str, prefix: str) -> list:
    prefixes = prefix.split("|")
    seen = {}
    for selectors, body in RULE.findall(css):
        m = CONTENT.search(body)
        if not m:
            continue
        cp = int(m.group(1), 16) if m.group(1) else ord(m.group(2))
        for sel in selectors.split(","):
            sel = sel.strip()
            a = GLYPH_ATTR.search(sel)
            if a:
                name = a.group(1)
            else:
                c = CLASS.search(sel)
                if not c:
                    continue
                name = c.group(1)
                for p in prefixes:
                    if name.startswith(p):
                        name = name[len(p):]
                        break
                else:
                    continue
            name = name.replace("-", " ").replace("_", " ").strip()
            if name and cp not in seen:
                seen[cp] = name
    return sorted(([n, cp] for cp, n in seen.items()), key=lambda x: x[0])


def names_from_codepoints(text: str) -> list:
    seen = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2:
            cp = int(parts[1], 16)
            seen.setdefault(cp, parts[0].replace("_", " "))
    return sorted(([n, cp] for cp, n in seen.items()), key=lambda x: x[0])


def woff_to_sfnt(data: bytes) -> bytes:
    """Unpacks a WOFF 1.0 font (zlib-compressed tables) back to TTF/OTF."""
    sig, flavor, _length, num_tables = struct.unpack(">4sIIH", data[:14])
    if sig != b"wOFF":
        raise ValueError("not a WOFF file")
    tables = []
    for i in range(num_tables):
        tag, off, comp, orig, checksum = struct.unpack(">4sIIII", data[44 + i * 20 : 64 + i * 20])
        raw = data[off : off + comp]
        tables.append((tag, zlib.decompress(raw) if comp < orig else raw, checksum))
    tables.sort(key=lambda t: t[0])
    entry_selector = max(i for i in range(16) if 2**i <= num_tables)
    search_range = 2**entry_selector * 16
    header = struct.pack(">IHHHH", flavor, num_tables, search_range, entry_selector, num_tables * 16 - search_range)
    offset = 12 + 16 * num_tables
    records, body = b"", b""
    for tag, table, checksum in tables:
        records += struct.pack(">4sIII", tag, checksum, offset + len(body), len(table))
        body += table + b"\0" * ((4 - len(table) % 4) % 4)
    return header + records + body


def main() -> int:
    root = Path(__file__).resolve().parent.parent / "assets" / "icon-fonts"
    root.mkdir(parents=True, exist_ok=True)
    only = set(sys.argv[1:])
    for s in SETS:
        if only and s["slug"] not in only:
            continue
        out = root / s["slug"]
        out.mkdir(exist_ok=True)
        if "woff" in s:
            font, ext = woff_to_sfnt(fetch(s["woff"])), ".ttf"
        else:
            font, ext = fetch(s["font"]), "." + s["font"].rsplit(".", 1)[1].lower()
        for old in out.glob("font.*"):
            old.unlink()
        (out / ("font" + ext)).write_bytes(font)
        if "codepoints" in s:
            names = names_from_codepoints(fetch(s["codepoints"]).decode("utf-8"))
        else:
            names = names_from_css(fetch(s["css"]).decode("utf-8", "replace"), s["prefix"])
        (out / "names.json").write_text(json.dumps(names, separators=(",", ":")))
        license_text = s.get("license_text") or fetch(s["license_url"]).decode("utf-8", "replace")
        (out / "LICENSE").write_text(license_text)
        source = s.get("font") or s.get("woff")
        (out / "set.json").write_text(json.dumps(
            dict(name=s["name"], version=s["version"], license=s["license"], source=source), indent=2))
        print(f"{s['name']:<28} {len(names):>5} names  {len(font) // 1024:>5} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
