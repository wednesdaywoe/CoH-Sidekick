#!/usr/bin/env python3
"""Extract Thunderspy custom power icons from the tspy .pigg archives into
`public/img/powers/`.

Many Thunderspy custom powers (Obedience Training, Spectral Aura, Knights, Pale
Blade, Tarantula, the custom assault sets, etc.) reference icons the app doesn't
bundle, so they render broken. Those icon textures live in Thunderspy's own
GUI/texture .pigg archives. This indexes the icon `.texture` files across all
tspy piggs, finds the ones our exported powers reference but the app lacks, and
converts each to a 32x32 RGBA PNG matching the referenced filename.

Usage:
  python3 scripts/extract-thunderspy-icons.py \
      [--assets-dir "<…/Sweet Tea/tspy>"] [--dry-run]

Env override: THUNDERSPY_ASSETS_DIR.
"""

import argparse
import glob
import io
import json
import os
import re
import sys

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO, "tools", "pigg-wrangler"))

from pigg_wrangler.pigg import PiggArchive  # noqa: E402
from pigg_wrangler import texture as tex  # noqa: E402
from PIL import Image  # noqa: E402

DEFAULT_ASSETS = os.environ.get(
    "THUNDERSPY_ASSETS_DIR",
    "/run/media/jiiwii/New Volume/Thunderspy Gaming/Sweet Tea/tspy",
)
EXPORT_DIR = os.path.join(REPO, "exported_powers", "thunderspy")
OUT_DIR = os.path.join(REPO, "public", "img", "powers")
ICON_SIZE = 32  # match the app's existing power-icon convention


def norm(name: str) -> str:
    """Normalize an icon filename for matching (drop ext + non-alnum)."""
    return re.sub(r"[^a-z0-9]", "", os.path.splitext(os.path.basename(name))[0].lower())


def index_pigg_icons(assets_dir: str) -> dict[str, tuple[str, str]]:
    """norm(name) -> (pigg_path, entry_path) for every icon .texture.

    Scans the given tspy folder AND its sibling base `piggs/` folder. Custom
    Thunderspy powersets reuse base icon packs whose textures live only in the
    shared texture piggs (e.g. Telekinetic/Psychokinetic Assault's `awakened_*`
    icons in Sweet Tea/piggs/stage1b.pigg's texture_library), NOT the tspy
    folder — so scanning the tspy folder alone silently missed 149 referenced
    power icons.
    """
    scan_dirs = [assets_dir]
    sibling = os.path.join(os.path.dirname(os.path.normpath(assets_dir)), "piggs")
    if os.path.isdir(sibling) and os.path.normpath(sibling) != os.path.normpath(assets_dir):
        scan_dirs.append(sibling)

    idx: dict[str, tuple[str, str]] = {}
    for d in scan_dirs:
        for pg in sorted(glob.glob(os.path.join(d, "*.pigg"))):
            try:
                arch = PiggArchive(pg)
            except Exception:
                continue
            for p in arch.list_paths():
                pl = p.lower()
                if pl.endswith(".texture") and "/icons/" in pl:
                    idx.setdefault(norm(p), (pg, p))
    return idx


def referenced_icons() -> set[str]:
    refs: set[str] = set()
    for f in glob.glob(os.path.join(EXPORT_DIR, "**", "*.json"), recursive=True):
        if f.endswith("index.json"):
            continue
        try:
            ic = json.load(open(f)).get("icon")
        except Exception:
            continue
        if ic:
            refs.add(ic.lower())
    return refs


def texture_to_png_bytes(raw: bytes) -> bytes:
    """Convert a .texture payload (DDS or JPEG) to a 32x32 RGBA PNG."""
    info = tex.parse_texture(raw)
    if info.image_format == "dds":
        rgba, w, h, _ = tex.decode_dds_to_rgba(tex.texture_to_dds(raw))
        img = Image.frombytes("RGBA", (w, h), rgba)
    else:  # jpeg
        img = Image.open(io.BytesIO(tex.texture_to_jpeg(raw))).convert("RGBA")
    if img.size != (ICON_SIZE, ICON_SIZE):
        img = img.resize((ICON_SIZE, ICON_SIZE), Image.LANCZOS)
    buf = io.BytesIO()
    img.save(buf, "PNG")
    return buf.getvalue()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--assets-dir", default=DEFAULT_ASSETS)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    if not os.path.isdir(args.assets_dir):
        print(f"ERROR: assets dir not found: {args.assets_dir}", file=sys.stderr)
        return 2

    idx = index_pigg_icons(args.assets_dir)
    print(f"Indexed {len(idx)} icon textures across tspy piggs")

    have = {os.path.basename(p).lower() for p in glob.glob(os.path.join(OUT_DIR, "*"))}
    refs = referenced_icons()
    missing = [r for r in refs if r not in have]
    covered = sorted(m for m in missing if norm(m) in idx)
    print(f"{len(refs)} referenced / {len(missing)} missing / {len(covered)} extractable from piggs")

    arch_cache: dict[str, PiggArchive] = {}
    written = failed = 0
    for ref in covered:
        pigg_path, entry = idx[norm(ref)]
        try:
            arch = arch_cache.get(pigg_path) or arch_cache.setdefault(pigg_path, PiggArchive(pigg_path))
            png = texture_to_png_bytes(arch.extract(entry))
        except Exception as e:
            failed += 1
            print(f"  FAIL {ref}: {e}")
            continue
        if not args.dry_run:
            with open(os.path.join(OUT_DIR, ref), "wb") as fh:
                fh.write(png)
        written += 1

    print(f"\n{'(dry-run) would write' if args.dry_run else 'Wrote'} {written} icons; {failed} failed")
    remaining = sorted(m for m in missing if norm(m) not in idx)
    print(f"{len(remaining)} referenced icons still missing (not in tspy piggs) — "
          f"mostly Lore-pet/NPC/enhancement/archetype icons")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
