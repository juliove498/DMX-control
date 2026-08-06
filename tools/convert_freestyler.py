#!/usr/bin/env python3
"""Convert a Freestyler .pff fixture archive into a DMX Control library JSON.

Usage:
    python3 tools/convert_freestyler.py "path/to/Fixture.pff" [output.json]

The .pff container is a simple sequence of entries:
    <name>\\0<decimal size>\\0<payload bytes>
The first entry is the .fxt text definition; the rest are the GIF/BMP
icons it references (gobos, colors, prisms...). This script:

1. Extracts everything in-memory.
2. Parses the .fxt: header, per-channel range tables (value + icon),
   the 24 channel names, special fields and the macro table (each macro
   = 24 channel values, -1 = untouched, plus an icon).
3. Maps channels to DMX Control roles by name heuristics (Pan/Tilt +
   fine, dimmer, shutter, color wheel, gobo, prism, focus, ring RGB...).
4. Emits a FixtureDefinition JSON with every range labelled and its
   icon embedded as a data URL (same convention as the fixtures already
   converted by hand). Single-channel macros become labelled ranges on
   their channel; multi-channel macros (e.g. ring RGB combos) are
   skipped — real RGB roles cover them.

After running, drop the JSON into the app's library directory
(~/Library/Application Support/dmx-control/fixtures on macOS) and use
"Recargar librería" in Config → Librería.
"""

import base64
import json
import os
import re
import sys


def extract_pff(path):
    data = open(path, "rb").read()
    entries = {}
    order = []
    i = 0
    while i < len(data):
        try:
            nul1 = data.index(b"\x00", i)
            nul2 = data.index(b"\x00", nul1 + 1)
            size = int(data[nul1 + 1 : nul2].decode("ascii"))
        except (ValueError, UnicodeDecodeError):
            break
        name = data[i:nul1].decode("latin-1")
        entries[name] = data[nul2 + 1 : nul2 + 1 + size]
        order.append(name)
        i = nul2 + 1 + size
    return entries, order


def data_url(entries, fs_path):
    payload = entries.get(fs_path)
    if payload is None:
        return None
    ext = fs_path.rsplit(".", 1)[-1].lower()
    mime = {"gif": "image/gif", "bmp": "image/bmp", "png": "image/png"}.get(ext, "image/gif")
    return f"data:{mime};base64," + base64.b64encode(payload).decode()


# ---- labels -----------------------------------------------------------------

COLOR_ES = {
    "white": "Blanco", "c white": "Blanco", "red": "Rojo", "green": "Verde",
    "blue": "Azul", "cyan": "Cian", "magenta": "Magenta", "pink": "Rosa",
    "orange": "Naranja", "uv": "UV", "yellow": "Amarillo",
    "yellow light": "Amarillo claro", "yellow mas claro": "Amarillo más claro",
    "lavanda": "Lavanda", "light yellow plus": "Amarillo claro +",
}
ROT_LABEL = {
    "Misc_Gobo_Rot_M": "Rotación CW media", "Misc_Gobo_Rot_S": "Rotación CW lenta",
    "Misc_Gobo_Rot_SS": "Rotación CW muy lenta", "Misc_Gobo_Rot_CSS": "Rotación CCW muy lenta",
    "Misc_Gobo_Rot_CS": "Rotación CCW lenta", "Misc_Gobo_Rot_CM": "Rotación CCW media",
}
RAINBOW_LABEL = {
    "Misc_Rainbow_CW_Fast": "Arco iris CW rápido", "Misc_Rainbow_CW_Med": "Arco iris CW medio",
    "Misc_Rainbow_CW_Slow": "Arco iris CW lento", "Misc_Rainbow_CCW_Slow": "Arco iris CCW lento",
    "Misc_Rainbow_CCW_Med": "Arco iris CCW medio", "Misc_Rainbow_CCW_Fast": "Arco iris CCW rápido",
}
HALF_COLOR_BASE = "Misc_Text_Half_Color"


def base_of(path):
    return path.replace("\\", "/").rsplit("/", 1)[-1].rsplit(".", 1)[0]


def prettify(s):
    s = re.sub(r"^\d+\s+", "", s)  # "10 Red" -> "Red" (digits WITH space only)
    s = s.replace("_", " ")
    s = re.sub(r"(?<=[a-zá-úñ])(?=[A-Z])", " ", s)  # CruzCurvas -> Cruz Curvas
    s = re.sub(r"(?<=[a-wyzA-WYZá-úñ])(?=\d)", " ", s)  # Anillos3 -> Anillos 3 (keeps x11)
    s = re.sub(r"\s+", " ", s).strip()
    return s[0].upper() + s[1:] if s else s


def color_label(path):
    c = re.sub(r"^\d+\s+", "", base_of(path)).replace("_", " ").replace(".", " ")
    c = re.sub(r"\s+", " ", c).strip().lower()
    if c in COLOR_ES:
        return COLOR_ES[c]
    if c.startswith("cto"):
        return "CTO " + c[3:].strip().replace(" ", "/")
    return prettify(base_of(path))


def gobo_label(path):
    b = base_of(path)
    if b in ROT_LABEL:
        return ROT_LABEL[b]
    shake = bool(re.search(r"[_\-]?sh$", b, re.I))
    core = re.sub(r"[_\-]?sh$", "", b, flags=re.I)
    if re.match(r"^0*\s*open", core, re.I) or "open" in core.lower():
        lbl = "Abierto"
    else:
        lbl = prettify(core)
    return f"{lbl} (shake)" if shake else lbl


# ---- role inference ---------------------------------------------------------


def role_of(name):
    n = name.lower()
    ring = n.startswith(("aro", "ring", "led"))

    def other(tag):
        return {"other": tag}

    if "pan" in n and "16" in n or ("pan" in n and "fine" in n):
        return "pan_fine"
    if "tilt" in n and ("16" in n or "fine" in n):
        return "tilt_fine"
    if n == "pan" or n.startswith("pan"):
        return "pan"
    if n == "tilt" or n.startswith("tilt"):
        return "tilt"
    if "speed" in n and ("xy" in n or "pan" in n):
        return other("speed_xy")
    if ring and "red" in n or n == "aro red":
        return "red"
    if ring and ("green" in n or "verde" in n):
        return "green"
    if ring and ("blue" in n or "azul" in n):
        return "blue"
    if "red" in n and not ring:
        return "red"
    if "green" in n and not ring:
        return "green"
    if "blue" in n and not ring:
        return "blue"
    if "frost" in n:
        return other("frost")
    if "shutter" in n:
        return "shutter"
    if "dimmer" in n and ring:
        return other("ring_dimmer")
    if "strobe" in n and ring:
        return other("ring_strobe")
    if "dimmer" in n or "master" in n:
        return "intensity"
    if "strobe" in n:
        return "strobe"
    if "arco" in n or "rainbow" in n:
        return other("rainbow")
    if "color" in n or "colour" in n:
        return "color_wheel"
    if "gobo" in n and "rot" in n:
        return other("gobo_rot")
    if "gobo" in n:
        return "gobo"
    if "prism" in n and "rot" in n:
        return other("prism_rot")
    if "prism" in n and "2" in n:
        return other("prism2")
    if "prism" in n:
        return other("prism")
    if "focus" in n or "foco" in n:
        return "focus"
    if "zoom" in n:
        return "zoom"
    if "iris" in n:
        return "iris"
    if "reset" in n or "lamp" in n:
        return other("reset")
    if "macro" in n:
        return other("ring_macro" if ring else "macro")
    if "auto" in n and "speed" in n:
        return other("speed")
    if "auto" in n:
        return other("auto_program")
    if "speed" in n:
        return other("speed")
    return other(re.sub(r"\W+", "_", n).strip("_") or "channel")


# ---- fxt parsing ------------------------------------------------------------


def convert(pff_path, out_path=None):
    entries, order = extract_pff(pff_path)
    fxt_name = order[0]
    lines = entries[fxt_name].decode("latin-1").replace("\r\n", "\n").split("\n")

    manufacturer = lines[0].strip()
    name = lines[3].strip()
    channel_count = int(lines[4].strip())
    fixture_icon = lines[7].strip()

    # Range tables: <channel> <count> then count × (<value>, <icon path>).
    i = 8
    tables = {}
    while True:
        try:
            ch = int(lines[i].strip())
            cnt = int(lines[i + 1].strip())
        except (ValueError, IndexError):
            break
        if not (1 <= ch <= channel_count) or not (1 <= cnt <= 64):
            break
        parsed = []
        j, ok = i + 2, True
        for _ in range(cnt):
            try:
                v = int(lines[j].strip())
            except (ValueError, IndexError):
                ok = False
                break
            parsed.append((v, lines[j + 1].strip()))
            j += 2
        if not ok:
            break
        tables[ch] = parsed
        i = j

    i += 6  # sentinel block
    names = [lines[i + k].strip() for k in range(channel_count)]

    # Macros.
    macros = []
    if "Macros" in lines:
        mi = lines.index("Macros")
        mc = int(lines[mi + 1].strip())
        j = mi + 2
        for _ in range(mc):
            vals = [lines[j + k].strip() for k in range(channel_count)]
            img = lines[j + channel_count].strip()
            j += channel_count + 1
            sets = [
                (k + 1, int(v))
                for k, v in enumerate(vals)
                if v not in ("-1", "") and v.lstrip("-").isdigit() and int(v) >= 0
            ]
            if img and "." in img:
                macros.append((sets, img))

    # Pan/tilt degrees live near the end: two consecutive plausible
    # values with pan STRICTLY wider than tilt (rules out the 127/127
    # channel-default rows). Last plausible pair wins.
    pan_deg, tilt_deg = 540.0, 270.0
    for k in range(len(lines) - 1):
        try:
            a, b = int(lines[k].strip()), int(lines[k + 1].strip())
        except ValueError:
            continue
        if 360 <= a <= 720 and 180 <= b <= 360 and a > b:
            pan_deg, tilt_deg = float(a), float(b)

    # ---- build ranges ----
    def spans(points):
        pts = sorted(points, key=lambda t: t[0])
        out = []
        for idx, (v, lbl, img) in enumerate(pts):
            to = pts[idx + 1][0] - 1 if idx + 1 < len(pts) else 255
            out.append({"from": v, "to": max(v, to), "label": lbl, "img": img})
        return out

    def mk(items):
        rs = []
        for it in items:
            r = {"from": it["from"], "to": it["to"], "label": it["label"]}
            img = it.get("img")
            if img:
                url = data_url(entries, img)
                if url:
                    r["image"] = url
                    r["image_path"] = img.replace("\\", "/")
            rs.append(r)
        return rs

    channels = []
    for idx, ch_name in enumerate(names, start=1):
        role = role_of(ch_name)
        c = {"role": role, "name": ch_name, "default": 0}
        if role in ("pan", "tilt", "pan_fine", "tilt_fine", "focus"):
            c["default"] = 127
        table = tables.get(idx)
        if table:
            pts = []
            for t_idx, (v, path) in enumerate(table):
                b = base_of(path)
                if b in RAINBOW_LABEL:
                    lbl = RAINBOW_LABEL[b]
                elif b == HALF_COLOR_BASE:
                    prev_l = color_label(table[t_idx - 1][1]) if t_idx > 0 else "?"
                    nxt = table[t_idx + 1][1] if t_idx + 1 < len(table) else None
                    if nxt and base_of(nxt) not in (HALF_COLOR_BASE,) and base_of(nxt) not in RAINBOW_LABEL:
                        lbl = f"Mitad {prev_l}/{color_label(nxt)}"
                    else:
                        lbl = f"Mitad {prev_l}"
                elif role == "gobo":
                    lbl = gobo_label(path)
                elif role == "color_wheel":
                    lbl = color_label(path)
                else:
                    lbl = prettify(base_of(path))
                pts.append((v, lbl, path))
            c["ranges"] = mk(spans(pts))
        channels.append(c)

    # Single-channel macros -> ranges on that channel (if it has none yet).
    by_ch = {}
    for sets, img in macros:
        if len(sets) == 1:
            ch_n, v = sets[0]
            by_ch.setdefault(ch_n, []).append((v, prettify(base_of(img)), img))
    for ch_n, pts in by_ch.items():
        c = channels[ch_n - 1]
        if not c.get("ranges"):
            if all(p[0] > 0 for p in pts):
                pts.append((0, "Off", None))
            c["ranges"] = mk(spans(pts))

    fixture_id = re.sub(r"\W+", "-", f"{manufacturer} {name}".lower()).strip("-")
    definition = {
        "id": fixture_id,
        "manufacturer": manufacturer,
        "name": name,
        "modes": [
            {
                "name": f"{channel_count}ch",
                "channels": channels,
                "pan_range": {"min": 0, "max": 65535, "physical_degrees": pan_deg},
                "tilt_range": {"min": 0, "max": 65535, "physical_degrees": tilt_deg},
            }
        ],
        "image": data_url(entries, fixture_icon),
    }

    out_path = out_path or f"{fixture_id}.json"
    json.dump(definition, open(out_path, "w"), ensure_ascii=False, indent=1)
    n_ranges = sum(len(c.get("ranges", [])) for c in channels)
    n_imgs = sum(1 for c in channels for r in c.get("ranges", []) if "image" in r)
    print(f"{out_path}: {os.path.getsize(out_path)} bytes, {channel_count} canales, "
          f"{n_ranges} rangos ({n_imgs} con icono)")
    return out_path


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    convert(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else None)
