#!/usr/bin/env python3
"""
Walk a fixture JSON and inline every `image_path` reference as a base64
data URL into the matching `image` field. Used to bake Freestyler artwork
(gobo/color/effect previews) into a fixture definition so the app does
not need filesystem access at runtime.

Usage:
    embed-fixture-images.py <fixture.json> <assets-root> [--in-place]

The script looks up each `image_path` (POSIX-style, e.g.
"CabezaMios/GobosMios/estrella.gif") under <assets-root>. If found, the
sibling `image` field is set to a `data:image/<ext>;base64,<...>` URL.
Missing files leave `image` as null and are reported on stderr.

Without --in-place the rewritten JSON goes to stdout.
"""
from __future__ import annotations

import argparse
import base64
import json
import mimetypes
import sys
from pathlib import Path
from typing import Any


def encode_file(path: Path) -> str:
    mime, _ = mimetypes.guess_type(path.name)
    if mime is None:
        ext = path.suffix.lower().lstrip(".")
        mime = f"image/{ext or 'octet-stream'}"
    payload = base64.b64encode(path.read_bytes()).decode("ascii")
    return f"data:{mime};base64,{payload}"


def walk(node: Any, assets_root: Path, stats: dict[str, int]) -> None:
    if isinstance(node, dict):
        rel = node.get("image_path")
        if isinstance(rel, str) and rel:
            target = assets_root / rel
            if target.is_file():
                node["image"] = encode_file(target)
                stats["embedded"] += 1
            else:
                stats["missing"] += 1
                print(f"  missing: {rel}", file=sys.stderr)
        for v in node.values():
            walk(v, assets_root, stats)
    elif isinstance(node, list):
        for item in node:
            walk(item, assets_root, stats)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("fixture", type=Path)
    ap.add_argument("assets_root", type=Path)
    ap.add_argument("--in-place", action="store_true")
    args = ap.parse_args()

    data = json.loads(args.fixture.read_text())
    stats = {"embedded": 0, "missing": 0}
    walk(data, args.assets_root, stats)

    out = json.dumps(data, indent=2, ensure_ascii=False) + "\n"
    if args.in_place:
        args.fixture.write_text(out)
    else:
        sys.stdout.write(out)

    print(
        f"embedded {stats['embedded']} image(s); {stats['missing']} missing",
        file=sys.stderr,
    )
    return 0 if stats["missing"] == 0 else 1


if __name__ == "__main__":
    raise SystemExit(main())
