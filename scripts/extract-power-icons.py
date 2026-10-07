#!/usr/bin/env python3
"""Fill gaps in the vendored power-icon tree from the installed game clients.

A power's `icon` is a filename the export owns; the app resolves it against a
flat, lower-cased `powers/` folder and falls back to `Unknown.png` when the file
is absent (Rule 1 — visible, never a broken image). The fallback is honest, but
it is still a power the planner cannot draw, and the gap is systematic rather
than random: every fork that ships content Homecoming *live* does not have —
the HC beta's Sonic Aura and Light Affinity, Rebirth's Guardian inherent,
Thunderspy's custom sets — references icons that were never in the tree,
because the tree was built from a live-Homecoming client.

The art is not ours to invent: it lives in each client's GUI texture `.pigg`.
So this reads the SHIPPED contract bundles (what the app actually renders, not
the intermediate export), diffs their icon references against the vendored tree,
finds each absent one in the installed clients, and writes a 32x32 RGBA PNG —
the tree's existing convention (2,987 of 2,991 files) — under the exact
lower-cased name the reference asks for.

It is a gap-filler, not a regenerator: a name already present is never rewritten,
so a hand-corrected or overridden icon survives a re-run. Nothing here runs at
build time — the PNGs are committed, and the running app never sees a .pigg.

Usage:
  python3 scripts/extract-power-icons.py [--dry-run] [--verbose]
      [--hc-assets DIR] [--sweettea-assets DIR]

Env overrides: COH_HC_ASSETS, COH_SWEETTEA_ASSETS.
"""

import argparse
import glob
import gzip
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

CONTRACT_DIR = os.path.join(REPO, "crates", "app", "assets", "contract")

# Both icon trees are the same tree: the Dioxus app bundles `assets/img` as a
# folder asset, the React beta serves `public/img`, and the resolvers were
# written against each other. Writing one and not the other is how a fork's art
# comes back for one surface and stays missing on the other.
OUT_DIRS = [
    os.path.join(REPO, "crates", "app", "assets", "img", "powers"),
    os.path.join(os.path.dirname(REPO), "CoH-Sidekick", "public", "img", "powers"),
]

DEFAULT_HC = os.environ.get(
    "COH_HC_ASSETS", os.path.expanduser("~/.wine/drive_c/Games/Homecoming/assets")
)
DEFAULT_SWEETTEA = os.environ.get(
    "COH_SWEETTEA_ASSETS",
    os.path.expanduser(
        "~/Games/coh-sweettea/drive_c/users/jiiwii/AppData/Local/"
        "Thunderspy Gaming/Sweet Tea"
    ),
)

ICON_SIZE = 32


def norm(name: str) -> str:
    """Match key for an icon reference against a texture entry.

    The export writes `sonicaura_sonicboom.png`, the client stores
    `SonicAura_SonicBoom.texture`, and Thunderspy's converter leaves a
    `.texture` in the middle of one name. Stripping every extension and every
    non-alphanumeric leaves the one part both sides agree on.
    """
    stem = os.path.basename(name).lower()
    while True:
        stem, ext = os.path.splitext(stem)
        if not ext:
            break
    return re.sub(r"[^a-z0-9]", "", stem)


# Mirrors `vendored_power_icon` in crates/app/src/view/icons.rs — the app asks for a lower-cased
# `.png` whatever the export calls the file, because `.texture`/`.dds`/`.tga` are the client's
# names for art we ship as PNG. Writing the raw reference instead would put `martialmastery_
# warcry.dds` in the tree and leave the power blank anyway.
CLIENT_TEXTURE_EXTENSIONS = (".texture", ".dds", ".tga", ".png")


def vendored_name(icon: str) -> str:
    stem = icon.strip().lower()
    while True:
        for ext in CLIENT_TEXTURE_EXTENSIONS:
            if stem.endswith(ext):
                stem = stem[: -len(ext)]
                break
        else:
            return stem + ".png"


def referenced_icons() -> dict[str, set[str]]:
    """icon filename -> {fork}, over every power in every shipped bundle.

    Powerset-level `icon` fields are deliberately excluded: they are `.ico`
    names the app never resolves (no `.ico` has ever been vendored), so
    including them would report ~90 permanent "gaps" the UI does not have.
    """
    refs: dict[str, set[str]] = {}
    for fork in sorted(os.listdir(CONTRACT_DIR)):
        bundle = os.path.join(CONTRACT_DIR, fork, "bundle.json.gz")
        if not os.path.exists(bundle):
            continue
        with gzip.open(bundle) as fh:
            data = json.load(fh)
        # The three sections `PowerDatabase::all_powers` reads. Pools and epics were the ones
        # worth remembering: Rebirth's War Cry lives in an epic pool, so a powersets-only sweep
        # reports a clean tree while the power renders blank.
        for section in ("powersets", "power-pools", "epic-pools"):
            for owner in (data.get(section) or {}).values():
                for power in owner.get("powers") or []:
                    icon = (power.get("icon") or "").strip()
                    if icon:
                        refs.setdefault(vendored_name(icon), set()).add(fork)
    return refs


def index_client_textures(roots: list[str], verbose: bool) -> dict[str, tuple[str, str]]:
    """norm(name) -> (pigg path, entry path) for every power-icon texture.

    Channels are ordered so the ones a fork actually serves win: Homecoming's
    `beta` is the Brainstorm shard the `brainstorm` bundle comes from, and
    `rebirth/` is live where `rebirth_test/` is not. `setdefault` then makes the
    first channel to claim a name the one that keeps it.
    """
    def channel_rank(path: str) -> tuple[int, str]:
        low = path.lower()
        for rank, marker in enumerate(["/beta/", "/rebirth/", "/tspy/", "/live/"]):
            if marker in low:
                return (rank, low)
        # closedbeta, experimental, rebirth_test and the shared base piggs last:
        # they are stale or speculative copies of the same names.
        return (99, low)

    piggs: list[str] = []
    for root in roots:
        if not os.path.isdir(root):
            print(f"  (skipping absent client tree {root})")
            continue
        piggs += glob.glob(os.path.join(root, "**", "*.pigg"), recursive=True)
    piggs.sort(key=channel_rank)

    index: dict[str, tuple[str, str]] = {}
    for pigg in piggs:
        try:
            archive = PiggArchive(pigg)
            paths = archive.list_paths()
        except Exception as exc:  # a non-texture or unreadable pigg is not fatal
            if verbose:
                print(f"  (unreadable {pigg}: {exc})")
            continue
        for entry in paths:
            low = entry.lower()
            if low.endswith(".texture") and "/icons/" in low:
                index.setdefault(norm(entry), (pigg, entry))
    return index


def texture_to_png_bytes(raw: bytes) -> bytes:
    """A `.texture` payload (DDS or JPEG) as a 32x32 RGBA PNG."""
    info = tex.parse_texture(raw)
    if info.image_format == "dds":
        rgba, width, height, _ = tex.decode_dds_to_rgba(tex.texture_to_dds(raw))
        image = Image.frombytes("RGBA", (width, height), rgba)
    else:
        image = Image.open(io.BytesIO(tex.texture_to_jpeg(raw))).convert("RGBA")
    if image.size != (ICON_SIZE, ICON_SIZE):
        image = image.resize((ICON_SIZE, ICON_SIZE), Image.LANCZOS)
    buf = io.BytesIO()
    image.save(buf, "PNG")
    return buf.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--hc-assets", default=DEFAULT_HC)
    parser.add_argument("--sweettea-assets", default=DEFAULT_SWEETTEA)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--verbose", action="store_true")
    args = parser.parse_args()

    refs = referenced_icons()
    have = {f.lower() for f in os.listdir(OUT_DIRS[0])}
    missing = sorted(name for name in refs if name not in have)
    print(f"{len(refs)} power icons referenced across shipped bundles, {len(missing)} absent")
    if not missing:
        return 0

    index = index_client_textures([args.hc_assets, args.sweettea_assets], args.verbose)
    print(f"indexed {len(index)} icon textures across the installed clients")

    written = unmatched = failed = 0
    for name in missing:
        forks = ",".join(sorted(refs[name]))
        found = index.get(norm(name))
        if not found:
            unmatched += 1
            print(f"  MISS {name}  [{forks}] — no matching texture in any client")
            continue
        pigg, entry = found
        try:
            png = texture_to_png_bytes(PiggArchive(pigg).extract(entry))
        except Exception as exc:
            failed += 1
            print(f"  FAIL {name}  [{forks}] — {entry}: {exc}")
            continue
        print(f"  ok   {name}  [{forks}] <- {os.path.basename(pigg)}:{entry}")
        if not args.dry_run:
            for out_dir in OUT_DIRS:
                if not os.path.isdir(out_dir):
                    print(f"       (skipping absent tree {out_dir})")
                    continue
                with open(os.path.join(out_dir, name), "wb") as fh:
                    fh.write(png)
        written += 1

    verb = "would write" if args.dry_run else "wrote"
    print(f"{verb} {written}, {unmatched} unmatched, {failed} failed")
    # Unmatched is not an error: a fork can reference art no installed client
    # carries, and that is a finding to record, not a crash. A decode that blew
    # up on a texture we DID find is a real break.
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
