"""Builds kimo's bundled asset library: crates/kimo-core/assets/library/library.json.

Reproducible from two npm packages (fetch them with `npm pack lucide-static world-atlas@2` and untar):

    python -I tools/build_asset_library.py <lucide-static/package> <world-atlas/package>

Contents
  icon/*       a curated subset of Lucide icons (ISC; some icons MIT via Feather), stroke style, 24x24
  map/world    world map, one <g> per country (Natural Earth 1:110m via world-atlas, public domain data / ISC package)
  character/*  two flat figures in seven poses, authored for kimo (MIT, same as kimo itself)
  prop/*       flat filled props, authored for kimo (MIT)

Every asset is a plain SVG whose top-level <g> elements are the animatable layers that import_svg creates.
"""
import json, os, re, sys

ICONS = """user users user-check briefcase building building-2 house factory store school hospital landmark globe map map-pin flag
compass rocket plane car bus train-front truck ship bike fuel battery battery-charging zap sun moon cloud cloud-rain umbrella
thermometer droplet flame leaf tree-pine trees mountain waves sprout recycle wind heart heart-pulse activity stethoscope pill
syringe brain dna microscope flask-conical atom lightbulb cpu laptop monitor smartphone tablet tv camera video mic headphones
speaker wifi signal server database hard-drive cloud-upload download upload link lock unlock key shield shield-check eye search
bell mail message-circle message-square phone send share calendar clock timer hourglass alarm-clock wallet credit-card banknote
coins piggy-bank receipt shopping-cart shopping-bag gift tag package trending-up trending-down chart-line chart-bar chart-pie
chart-column percent calculator target award trophy medal star thumbs-up thumbs-down check circle-check x circle-x circle-alert
triangle-alert info circle-help plus minus arrow-right arrow-left arrow-up arrow-down arrow-up-right refresh-cw repeat settings
wrench hammer scale gavel book book-open graduation-cap pencil pen-tool file file-text folder clipboard newspaper coffee utensils
pizza apple shirt baby dog cat bird fish music gamepad-2 puzzle palette brush scissors handshake hand smile frown sparkles bot""".split()

LUCIDE_LICENSE = "ISC (Lucide); icons derived from Feather are MIT (Cole Bemis)"
MAP_LICENSE = "Natural Earth 1:110m (public domain) via world-atlas 2.0.2 (ISC, Michael Bostock)"
KIMO_LICENSE = "MIT (authored for kimo)"


def lucide(pkg):
    tags = json.load(open(os.path.join(pkg, "tags.json"), encoding="utf-8"))
    out = []
    for n in ICONS:
        svg = open(os.path.join(pkg, "icons", n + ".svg"), encoding="utf-8").read()
        svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S).strip()
        svg = re.sub(r'\s*class="[^"]*"', "", svg)
        # each element becomes its own layer so a multi-part icon can be drawn on piece by piece
        body = re.findall(r"<(?:path|circle|rect|line|polyline|polygon|ellipse)\b[^>]*/>", svg)
        head = re.search(r"<svg\b[^>]*>", svg, flags=re.S).group(0)
        svg = head + "".join(body) + "</svg>"
        out.append({"id": "icon/" + n, "category": "icon", "name": n.replace("-", " "), "tags": sorted(set(tags.get(n, []))),
                    "license": LUCIDE_LICENSE, "svg": re.sub(r"\s+", " ", svg)})
    return out


def clip_lon(pts, lo, hi):
    """Sutherland-Hodgman clip of a closed ring to lo <= lon <= hi."""
    def clip(pts, inside, cross):
        out = []
        for i, cur in enumerate(pts):
            prev = pts[i - 1]
            if inside(cur):
                if not inside(prev): out.append(cross(prev, cur))
                out.append(cur)
            elif inside(prev): out.append(cross(prev, cur))
        return out
    def at(a, b, x): t = (x - a[0]) / (b[0] - a[0]); return (x, a[1] + (b[1] - a[1]) * t)
    pts = clip(pts, lambda p: p[0] >= lo, lambda a, b: at(a, b, lo))
    return clip(pts, lambda p: p[0] <= hi, lambda a, b: at(a, b, hi)) if pts else []


def split_antimeridian(pts):
    """A ring that crosses the 180th meridian (Russia, Fiji) is unwrapped to continuous longitudes, then drawn twice
    (as is and shifted a full turn) and clipped to the map, instead of a streak right across the world."""
    if all(abs(b[0] - a[0]) <= 180 for a, b in zip(pts, pts[1:])): return [pts]
    un = [pts[0]]; off = 0.0
    for a, b in zip(pts, pts[1:]):
        if b[0] - a[0] > 180: off -= 360
        elif b[0] - a[0] < -180: off += 360
        un.append((b[0] + off, b[1]))
    out = []
    for shift in (-360.0, 0.0, 360.0):
        c = clip_lon([(x + shift, y) for x, y in un], -180.0, 180.0)
        if len(c) >= 3: out.append(c)
    return out


def world(pkg):
    topo = json.load(open(os.path.join(pkg, "countries-110m.json"), encoding="utf-8"))
    sx, sy = topo["transform"]["scale"]; tx, ty = topo["transform"]["translate"]
    arcs = []
    for a in topo["arcs"]:
        x = y = 0; pts = []
        for dx, dy in a: x += dx; y += dy; pts.append((x * sx + tx, y * sy + ty))
        arcs.append(pts)
    W, top, bot = 1000.0, 84.0, -58.0  # equirectangular, Antarctica dropped, cropped to 84N..58S
    k = W / 360.0
    def proj(lon, lat): return ((lon + 180.0) * k, (top - lat) * k)
    def ring(idx):
        pts = []
        for i in idx:
            a = arcs[i] if i >= 0 else list(reversed(arcs[~i]))
            pts.extend(a if not pts else a[1:])
        return pts
    out = []
    for g in topo["objects"]["countries"]["geometries"]:
        name = g.get("properties", {}).get("name", "")
        if name == "Antarctica" or g.get("type") not in ("Polygon", "MultiPolygon"): continue
        polys = g["arcs"] if g["type"] == "MultiPolygon" else [g["arcs"]]
        d = []
        for poly in polys:
            for r in poly:
                for part in split_antimeridian(ring(r)):
                    p = [proj(*q) for q in part]
                    d.append("M" + " L".join(f"{x:.1f} {y:.1f}" for x, y in p) + "Z")
        gid = re.sub(r"[^A-Za-z0-9]+", "_", name).strip("_")
        out.append(f'<g id="{gid}" data-name="{name}" data-iso="{g.get("id", "")}"><path d="{"".join(d)}"/></g>')
    H = (top - bot) * k
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {W:.0f} {H:.0f}" fill="#c9d1d9" stroke="#0d1117" '
           f'stroke-width="0.6">' + "".join(out) + "</svg>")
    names = [re.search(r'data-name="([^"]*)"', o).group(1) for o in out]
    return [{"id": "map/world", "category": "map", "name": "world map (countries)", "tags": ["world", "globe", "countries", "earth", "atlas"] + names,
             "license": MAP_LICENSE, "svg": svg}]


# ---- characters (authored) -------------------------------------------------------------------------
POSES = {  # (arm_left, arm_right, leg_left, leg_right) as polyline/quad path data in a 200x400 box
    "standing":    ("M74 116 Q60 170 66 214", "M126 116 Q140 170 134 214", "M88 226 L86 356", "M112 226 L114 356"),
    "waving":      ("M74 116 Q60 170 66 214", "M126 116 Q162 100 166 46", "M88 226 L86 356", "M112 226 L114 356"),
    "pointing":    ("M74 116 Q60 170 66 214", "M126 116 L192 108", "M88 226 L86 356", "M112 226 L114 356"),
    "thinking":    ("M74 116 Q60 170 66 214", "M126 116 L148 166 L114 98", "M88 226 L86 356", "M112 226 L114 356"),
    "presenting":  ("M74 116 Q40 140 20 178", "M126 116 Q160 140 180 178", "M88 226 L82 356", "M112 226 L118 356"),
    "walking":     ("M74 116 Q66 168 90 208", "M126 116 Q142 164 156 200", "M88 226 L66 354", "M112 226 L136 354"),
    "celebrating": ("M74 116 Q50 80 42 30", "M126 116 Q150 80 158 30", "M88 226 L74 356", "M112 226 L126 356"),
}
PEOPLE = {
    "ava":  {"skin": "#e0ac8a", "hair": "#2b1d16", "shirt": "#1f9e8f", "pants": "#26324a", "shoe": "#1a1a1a", "long": True},
    "ravi": {"skin": "#a8714f", "hair": "#141414", "shirt": "#e2a63b", "pants": "#2f3b63", "shoe": "#3a2a1e", "long": False},
}


def end_point(d):
    nums = re.findall(r"-?\d+(?:\.\d+)?", d); return float(nums[-2]), float(nums[-1])


def figure(who, pose):
    c = PEOPLE[who]; al, ar, ll, lr = POSES[pose]
    def arm(gid, d):
        x, y = end_point(d)
        return (f'<g id="{gid}"><path d="{d}" fill="none" stroke="{c["shirt"]}" stroke-width="20" stroke-linecap="round"/>'
                f'<circle cx="{x:g}" cy="{y:g}" r="11" fill="{c["skin"]}"/></g>')
    def leg(d):
        x, y = end_point(d)
        return (f'<path d="{d}" fill="none" stroke="{c["pants"]}" stroke-width="24" stroke-linecap="round"/>'
                f'<ellipse cx="{x + 6:g}" cy="{y + 8:g}" rx="17" ry="9" fill="{c["shoe"]}"/>')
    hair_back = (f'<path d="M62 66 Q60 22 100 22 Q140 22 138 66 L140 112 Q120 116 116 96 L84 96 Q80 116 60 112 Z" fill="{c["hair"]}"/>'
                 if c["long"] else "")
    fringe = ("M66 60 Q70 22 100 24 Q132 22 134 60 Q116 40 92 44 Q76 46 66 60 Z" if c["long"] else
              "M67 56 Q68 20 100 22 Q132 20 133 56 Q122 36 100 40 Q80 36 67 56 Z")
    head = (f'<g id="head">{hair_back}<rect x="91" y="84" width="18" height="24" fill="{c["skin"]}"/>'
            f'<circle cx="100" cy="62" r="33" fill="{c["skin"]}"/><path d="{fringe}" fill="{c["hair"]}"/>'
            f'<circle cx="88" cy="64" r="3.6" fill="#1b1b1b"/><circle cx="112" cy="64" r="3.6" fill="#1b1b1b"/>'
            f'<path d="M89 78 Q100 87 111 78" fill="none" stroke="#5b2b22" stroke-width="3" stroke-linecap="round"/></g>')
    torso = f'<g id="torso"><rect x="68" y="100" width="64" height="132" rx="24" fill="{c["shirt"]}"/></g>'
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 400"><g id="legs">{leg(ll)}{leg(lr)}</g>{torso}'
           f'{arm("arm_left", al)}{arm("arm_right", ar)}{head}</svg>')
    return {"id": f"character/{who}_{pose}", "category": "character", "name": f"{who} {pose}",
            "tags": ["person", "people", "character", "figure", who, pose] + (["woman"] if c["long"] else ["man"]),
            "license": KIMO_LICENSE, "svg": svg}


PROPS = {
    "desk": ("furniture office table work", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200"><g id="top"><rect x="10" y="40" width="280" height="22" rx="6" fill="#8a5a3b"/></g><g id="legs"><rect x="30" y="60" width="16" height="130" fill="#6b4329"/><rect x="254" y="60" width="16" height="130" fill="#6b4329"/></g><g id="drawer"><rect x="170" y="62" width="84" height="60" fill="#7a4e33"/><rect x="198" y="86" width="28" height="8" rx="4" fill="#d9b48f"/></g></svg>'),
    "chair": ("furniture seat office", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 240"><g id="back"><rect x="30" y="10" width="100" height="110" rx="20" fill="#3b6ea5"/></g><g id="seat"><rect x="20" y="120" width="120" height="26" rx="10" fill="#2f5a88"/></g><g id="base"><rect x="72" y="146" width="16" height="56" fill="#444"/><path d="M30 222 L80 200 L130 222" fill="none" stroke="#444" stroke-width="12" stroke-linecap="round"/></g></svg>'),
    "plant": ("pot leaves green nature office decor", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 160 240"><g id="leaves"><path d="M80 150 Q30 120 26 50 Q70 70 80 150 Z" fill="#3f9d4f"/><path d="M80 150 Q130 120 136 46 Q90 70 80 150 Z" fill="#2f8a41"/><path d="M80 150 Q74 70 84 12 Q110 80 80 150 Z" fill="#4fb25f"/></g><g id="pot"><path d="M40 150 L120 150 L108 230 L52 230 Z" fill="#c8693f"/><rect x="34" y="140" width="92" height="20" rx="6" fill="#d97c50"/></g></svg>'),
    "laptop": ("computer device work screen tech", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200"><g id="screen"><rect x="50" y="10" width="200" height="140" rx="10" fill="#2b2f36"/><rect x="62" y="22" width="176" height="116" rx="4" fill="#5fb3f0"/></g><g id="base"><path d="M20 160 L280 160 L262 186 L38 186 Z" fill="#9aa3ad"/><rect x="128" y="160" width="44" height="8" rx="4" fill="#7c858f"/></g></svg>'),
    "coffee_mug": ("cup drink tea chai coffee break", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 200"><g id="steam"><path d="M70 50 Q60 30 72 12" fill="none" stroke="#b9c2cc" stroke-width="7" stroke-linecap="round"/><path d="M100 50 Q90 30 102 12" fill="none" stroke="#b9c2cc" stroke-width="7" stroke-linecap="round"/></g><g id="mug"><path d="M140 90 Q182 90 180 122 Q178 152 140 150" fill="none" stroke="#d6453d" stroke-width="14"/><rect x="40" y="64" width="110" height="120" rx="16" fill="#e8574e"/></g></svg>'),
    "book_stack": ("books study read school library", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 200"><g id="book1"><rect x="20" y="150" width="200" height="34" rx="5" fill="#3b6ea5"/></g><g id="book2"><rect x="36" y="114" width="172" height="36" rx="5" fill="#e2a63b"/></g><g id="book3"><rect x="28" y="78" width="184" height="36" rx="5" fill="#1f9e8f"/></g></svg>'),
    "speech_bubble": ("talk chat dialogue say callout", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 200"><g id="bubble"><path d="M30 20 L230 20 Q250 20 250 40 L250 120 Q250 140 230 140 L110 140 L60 186 L70 140 L30 140 Q10 140 10 120 L10 40 Q10 20 30 20 Z" fill="#ffffff" stroke="#1b1b1b" stroke-width="6"/></g></svg>'),
    "thought_bubble": ("think idea cloud callout", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 260 220"><g id="cloud"><path d="M70 130 Q20 130 24 92 Q22 54 66 56 Q74 18 120 24 Q154 6 180 36 Q236 34 234 82 Q252 124 204 134 Q190 160 150 150 Q120 168 96 148 Q72 154 70 130 Z" fill="#ffffff" stroke="#1b1b1b" stroke-width="6"/></g><g id="dots"><circle cx="70" cy="176" r="12" fill="#ffffff" stroke="#1b1b1b" stroke-width="5"/><circle cx="44" cy="204" r="7" fill="#ffffff" stroke="#1b1b1b" stroke-width="4"/></g></svg>'),
    "money_bag": ("money cash rupee wealth savings finance", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 220"><g id="bag"><path d="M70 60 Q20 110 26 160 Q32 206 100 206 Q168 206 174 160 Q180 110 130 60 Z" fill="#d8a23a"/><path d="M76 60 L124 60 L140 22 Q100 36 60 22 Z" fill="#c38d2a"/></g><g id="tie"><rect x="66" y="54" width="68" height="12" rx="6" fill="#8a5a1a"/></g><g id="symbol"><path d="M78 110 L124 110 M78 128 L124 128 M90 110 Q122 112 118 128 Q114 144 86 144 L120 178" fill="none" stroke="#fff4d6" stroke-width="9" stroke-linecap="round" stroke-linejoin="round"/></g></svg>'),
    "house_flat": ("home building residence property", '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 220"><g id="roof"><path d="M10 104 L120 14 L230 104 Z" fill="#c8493f"/></g><g id="walls"><rect x="36" y="100" width="168" height="110" fill="#f1e3c8"/></g><g id="door"><rect x="102" y="140" width="38" height="70" rx="4" fill="#7a4e33"/></g><g id="window"><rect x="54" y="122" width="34" height="34" fill="#5fb3f0" stroke="#ffffff" stroke-width="4"/><rect x="154" y="122" width="34" height="34" fill="#5fb3f0" stroke="#ffffff" stroke-width="4"/></g></svg>'),
}


def props():
    return [{"id": "prop/" + k, "category": "prop", "name": k.replace("_", " "), "tags": t.split(), "license": KIMO_LICENSE, "svg": s}
            for k, (t, s) in PROPS.items()]


def main():
    luc, atlas = sys.argv[1], sys.argv[2]
    lib = lucide(luc) + world(atlas) + [figure(w, p) for w in PEOPLE for p in POSES] + props()
    root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates", "kimo-core", "assets", "library")
    os.makedirs(os.path.join(root, "licenses"), exist_ok=True)
    with open(os.path.join(root, "library.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump({"version": 1, "assets": lib}, f, ensure_ascii=False, separators=(",", ":"))
    for src, dst in [(os.path.join(luc, "LICENSE"), "LUCIDE-ISC.txt"), (os.path.join(atlas, "LICENSE"), "WORLD-ATLAS-ISC.txt")]:
        with open(src, encoding="utf-8") as i, open(os.path.join(root, "licenses", dst), "w", encoding="utf-8", newline="\n") as o: o.write(i.read())
    print(len(lib), "assets")


if __name__ == "__main__":
    main()
