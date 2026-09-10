#!/usr/bin/env python3
"""
Emit Mids Reborn's power NAME table for one dataset — the input to
`scripts/convert-mids-name-map.cjs` (DATA-GAP MBDIMPORT-2).

Why this exists at all, given CLAUDE.md's "source from the binary export, not
second-hand Mids tables": the question here is not a game fact. It is *what Mids
calls a power*, and Mids is the only authority on its own namespace. A `.mbd`
carries the internal name and nothing else, so reading one means knowing which
of Mids' names denotes which of the game's powers. Nothing in the export can
answer that.

Each row is `[internal name, display name, unlock level]`. Display carries the join —
HC has rotated internal names under stable display names, so display is the identity
that survived and internal is the one that moved — and the level is what keeps the
join honest. HC reuses a display name too: Ninjitsu's old Blinding Powder is called
"Smoke Flash" now, so display alone would pair it with Mids' unrelated Smoke Flash.
The two unlock at different levels, and that is the tell.

The `.mhd` itself is a local Mids install, not a repo asset (`/Thunderspy/` is
gitignored, and the Wine prefix is outside the tree). This JSON is the committed
half, which is why it carries the database version and the file's sha256 — a
regeneration against a different Mids build has to be visible in the diff.

Usage:
    python3 tools/mids-oracle/emit_mids_names.py --dataset homecoming
    python3 tools/mids-oracle/emit_mids_names.py --dataset rebirth --mhd /path/to/I12.mhd
"""

from __future__ import annotations

import argparse
import collections
import hashlib
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from read_i12 import Reader, read_powers  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Where each dataset's Mids database lives on a developer machine.
#
# dataset-absent: brainstorm — Mids ships no Brainstorm build, so a Brainstorm .mbd is
# authored in Mids' Homecoming database and carries Homecoming's namespace. There is no
# fourth .mhd to read and a key here could never be satisfied. The routing is the
# converter's call, not this script's — see scripts/convert-mids-name-map.cjs.
#
# And Mids ships no Thunderspy build either, which makes the third entry the odd one:
# `Thunderspy/I12.mhd` is not a Mids Reborn release database. It is also not what its
# sibling is — `Thunderspy/EnhDB.mhd` is Mids' GENERIC database byte for byte, while the
# dump this produced from the I12 holds Thunderspy content (Spectral Melee, Hard Life, Pale
# Blade, the Defender melee sets; 286 of our 305 Thunderspy powersets are in it, against 272
# of Homecoming's 364). The file itself is no longer on disk and where it came from is
# recorded nowhere, so `mids-power-names.thunderspy.json` is now the only evidence of it and
# this entry cannot currently be re-run. Stated rather than assumed either way, because a
# header reading "Mids Reborn Powers Database" names no fork and a version alone names none
# — see DATA-GAP MBDEXPORT-2.
DEFAULT_MHD = {
    "homecoming": os.path.expanduser(
        "~/Games/mids-reborn/drive_c/MidsReborn/Databases/Homecoming/I12.mhd"
    ),
    "rebirth": os.path.expanduser(
        "~/Games/mids-reborn/drive_c/MidsReborn/Databases/Rebirth/I12.mhd"
    ),
    "thunderspy": os.path.join(REPO_ROOT, "Thunderspy", "I12.mhd"),
}


def read_header(buf: bytes) -> tuple[str, str]:
    """The first two records are the database name and its version string."""
    r = Reader(buf, 0)
    return r.string(), r.string()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="Emit Mids' power name table for one dataset")
    ap.add_argument("--dataset", required=True, choices=sorted(DEFAULT_MHD))
    ap.add_argument("--mhd", default=None, help="override the database path")
    ap.add_argument("--out", default=None, help="override the output path")
    args = ap.parse_args(argv)

    out_path = args.out or os.path.join(
        REPO_ROOT, "tools", "mids-oracle", f"mids-power-names.{args.dataset}.json"
    )

    mhd_path = args.mhd or DEFAULT_MHD[args.dataset]
    if not os.path.isfile(mhd_path):
        print(f"error: no such file: {mhd_path}", file=sys.stderr)
        return 2
    with open(mhd_path, "rb") as fh:
        buf = fh.read()
    name, version = read_header(buf)
    powers, total = read_powers(buf)

    # Keyed by Mids' OWN spelling of `group.set`, case and all. Folding it here is what
    # MBDEXPORT-6 was: the writer composes a `PowerName` out of these two segments, Mids
    # resolves one with an ordinal `==`, and a lower-cased path binds to nothing. Two
    # spellings that differ only in case would collide — neither database has such a pair,
    # and the assert below says so rather than leaving it to hold by luck.
    sets: dict[str, list] = collections.defaultdict(list)
    folded: dict[str, str] = {}
    for p in powers:
        group, pset = p["group"], p["set"]
        if not group or not pset:
            continue
        key = f"{group}.{pset}"
        first = folded.setdefault(key.lower(), key)
        if first != key:
            print(
                f"error: {args.dataset} spells one powerset two ways: {first!r} and {key!r}",
                file=sys.stderr,
            )
            return 3
        sets[key].append([p["power"], p.get("display") or "", p.get("level")])

    payload = {
        "dataset": args.dataset,
        "database": name,
        "version": version,
        "sha256": hashlib.sha256(buf).hexdigest(),
        "powerCount": total,
        # The marker a reader checks before trusting a key's case. A dump written before
        # MBDEXPORT-6 carries folded keys and no marker, and its case is unrecoverable
        # without the `.mhd` — which is a different answer from "Mids spells it that way".
        "powersetKeys": "literal",
        "powersets": {k: sets[k] for k in sorted(sets, key=str.lower)},
    }

    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, indent=1, sort_keys=False)
        fh.write("\n")

    print(
        f"[emit_mids_names] {args.dataset}: {payload['powerCount']} powers in "
        f"{len(payload['powersets'])} powersets (db {payload['version']}) -> "
        f"{os.path.relpath(out_path, REPO_ROOT)}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
