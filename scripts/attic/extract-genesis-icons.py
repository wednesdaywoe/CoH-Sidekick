#!/usr/bin/env python3
"""Extract the Genesis incarnate icon PNGs from the Rebirth texture pigg.

Genesis is a Rebirth-only incarnate slot (Rebirth Issue 6), so its icon art
lives in the Rebirth texture overlay (`rebirth/z_rebirth_texGui1.pigg`), not in
the Homecoming assets the rest of the icons came from. The decoded 32x32 PNGs
are committed to `public/img/powers/` so the running app never needs the pigg.

Produces, in public/img/powers/:
  incarnate_genesis_{data,fate,socket,verdict}_{common,uncommon,rare,veryrare}.png
  incarnate_genesis_blank.png   (the Genesis-slot accolade badge, used for the
                                  empty-slot button)

Run on a machine with the Rebirth client installed:
  py -3 scripts/extract-genesis-icons.py [--assets-dir "G:/Thunderspy Gaming/Sweet Tea"]
"""
import argparse
import glob
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / "tools" / "pigg-wrangler"))
from pigg_wrangler.pigg import PiggArchive  # noqa: E402
from pigg_wrangler.texture import texture_to_png  # noqa: E402

DEFAULT_ASSETS = os.environ.get("COH_THUNDERSPY_ASSETS", r"G:/Thunderspy Gaming/Sweet Tea")
OUT_DIR = ROOT / "public" / "img" / "powers"

TREES = ["Data", "Fate", "Socket", "Verdict"]
TIERS = ["Common", "Uncommon", "Rare", "VeryRare"]
ICON_DIR = "texture_library/GUI/Icons/Powers"
BADGE_PATH = "texture_library/GUI/Icons/Badges/Badge_Accolade_GenesisSlot.texture"


def build_index(assets_dir):
    """Map lowercase internal texture path -> (pigg_path). Prefers the live
    `rebirth/` overlay over `rebirth_test/` when a texture exists in both."""
    piggs = glob.glob(os.path.join(assets_dir, "**", "*.pigg"), recursive=True)
    # Sort so that any *_test piggs are consulted last.
    piggs.sort(key=lambda p: ("test" in p.lower(), p.lower()))
    index = {}
    for p in piggs:
        try:
            archive = PiggArchive(p)
        except Exception:
            continue
        for path in archive.list_paths():
            index.setdefault(path.lower(), p)
    return index


def extract_png(index, internal_path):
    pigg = index.get(internal_path.lower())
    if not pigg:
        return None
    data = PiggArchive(pigg).extract(internal_path)
    return texture_to_png(data)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--assets-dir", default=DEFAULT_ASSETS,
                    help="Root of the Rebirth client install (searched recursively for *.pigg)")
    args = ap.parse_args()

    if not os.path.isdir(args.assets_dir):
        sys.exit(f"ERROR: assets dir not found: {args.assets_dir}")

    print(f"Indexing piggs under {args.assets_dir} ...")
    index = build_index(args.assets_dir)
    print(f"  {len(index)} texture paths indexed")

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    written, missing = 0, []

    # 16 power icons
    for tree in TREES:
        for tier in TIERS:
            internal = f"{ICON_DIR}/Incarnate_Genesis_{tree}_{tier}.texture"
            png = extract_png(index, internal)
            out = OUT_DIR / f"incarnate_genesis_{tree.lower()}_{tier.lower()}.png"
            if png is None:
                missing.append(internal)
                continue
            out.write_bytes(png)
            written += 1
            print(f"  + {out.name}")

    # Empty-slot icon: the Genesis-slot accolade badge.
    badge = extract_png(index, BADGE_PATH)
    if badge is None:
        missing.append(BADGE_PATH)
    else:
        (OUT_DIR / "incarnate_genesis_blank.png").write_bytes(badge)
        written += 1
        print("  + incarnate_genesis_blank.png")

    print(f"\nWrote {written} icons to {OUT_DIR}")
    if missing:
        print(f"MISSING ({len(missing)}):")
        for m in missing:
            print(f"  - {m}")
        sys.exit(1)


if __name__ == "__main__":
    main()
