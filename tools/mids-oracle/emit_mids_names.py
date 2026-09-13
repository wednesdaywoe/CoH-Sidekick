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
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import read_i12  # noqa: E402
from read_i12 import provenance, read_powers  # noqa: E402

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Where each dataset's Mids database lives on a developer machine.
#
# dataset-absent: brainstorm — Mids ships no Brainstorm build, so a Brainstorm .mbd is
# authored in Mids' Homecoming database and carries Homecoming's namespace. There is no
# fourth .mhd to read and a key here could never be satisfied. The routing is the
# converter's call, not this script's — see scripts/convert-mids-name-map.cjs.
#
# And Mids ships no Thunderspy build either, which makes the third entry the odd one.
# `/Thunderspy/` is a third-party DATABASE DROP: Mids' Generic database with four files
# replaced — `I12.mhd` (2026.3.346, the Thunderspy powers), `NLevels.mhd`, `RLevels.mhd`,
# `SData.mhd` — plus that fork's powerset icons. Everything else in it, `EnhDB.mhd` included,
# is Generic byte for byte, which is WHY our Thunderspy enhancement UIDs are Generic's: the
# drop's own author rebuilt the powers database and left the enhancement one alone. Who built
# it is unrecorded; Mids Reborn's releases carry Generic, Homecoming and Rebirth, so it is not
# from there. Stated rather than assumed, because a header reading "Mids Reborn Powers
# Database" names no fork and a version alone names none — see DATA-GAP MBDEXPORT-2.
#
# The drop lives in BOTH repos now. It was in the beta alone until 2026-09-10, and this
# script's default path (`REPO_ROOT/Thunderspy/I12.mhd`) therefore resolved in one twin and
# not the other — which is how a session looking only at canonical concluded the file was
# gone. A gitignored vendored directory is per-checkout; check both before calling one empty.
DEFAULT_MHD = {
    # Spelled in `read_i12` so the DSH5 harness and this bridge read the same file by
    # construction rather than by two copies of one string agreeing (PROV-3).
    "homecoming": read_i12.DEFAULT_MHD,
    "rebirth": os.path.expanduser(
        "~/Games/mids-reborn/drive_c/MidsReborn/Databases/Rebirth/I12.mhd"
    ),
    "thunderspy": os.path.join(REPO_ROOT, "Thunderspy", "I12.mhd"),
}


# The two `eGridType` values that place an archetype's own inherent. 2 is the class grid —
# exactly one power per class on Homecoming, and where fourteen of fifteen sit on every
# fork. 4 is the universal grid `SortGridPowers` indexes by position, which carries
# Brawl/Sprint/Rest and the four Fitness powers on every fork and is also where Rebirth
# files Assassination and the Thunderspy drop files Resolve. Both bind: the Rebirth sweep
# resolves `Inherent.Inherent.Assassination` with no refusal. 0 is the one that does not.
GRIDDED_INHERENT_TYPES = (2, 4)


def archetype_inherents(powers) -> tuple[dict[str, str], list]:
    """`Class_X` -> Mids' name for that archetype's own inherent, and the classes it refused.

    The writer needs this because the `.mbd` archetype-inherent row is addressed by name and
    Mids has more names than powers. Homecoming carries THREE rows displaying "Opportunity" —
    `Opportunity`, `Opportunity_Icon` and `Opportunity_Meter`, all level 1, all gated to
    `Class_Sentinel` — so neither display nor level separates them, and the name map's join
    withdraws rather than guessing. It is right to withdraw: read backwards, that merge has no
    answer in the names. The answer is in a different field.

    `eGridType` is that field. It says which grid Mids places a power on, and 0 means none:
    a `.mbd` naming a power Mids grids nowhere is refused with no error and the row is simply
    gone. `Opportunity` is 0 and so is `Opportunity_Icon`; `Opportunity_Meter` is 2, the grid
    the other fourteen archetype inherents sit on. Measured, not reasoned: patching one swept
    build's `PowerName` to each of the three in turn, Mids binds `Opportunity_Meter` at the
    same front index every other archetype's inherent takes and refuses the other two.

    So the join is the CLASS GATE, not the name: the power gated to exactly this one class
    that Mids grids. Over all three databases that predicate selects 15, 16 and 15 powers, all
    of them in `Inherent.Inherent`, and never two for one class — the gate is what excludes
    Thunderspy's `Restraint` and `Tenacity`, which are gridded at 2 and gated on fourteen
    classes each.

    A class with more than one is returned unanswered rather than resolved by a preference,
    the same withdrawal the name rows take. A class with NONE gets no key, which is a real
    state and not an oversight: the Thunderspy drop grids `Assassination` at 0, so a
    Thunderspy Stalker has no name here that Mids would bind, and the writer says so.
    """
    by_class: dict[str, list[str]] = collections.defaultdict(list)
    for p in powers:
        if p["inherent_type"] not in GRIDDED_INHERENT_TYPES:
            continue
        if len(p["class_name"]) != 1:
            continue
        by_class[p["class_name"][0]].append(p["power"])
    answered = {c: rows[0] for c, rows in sorted(by_class.items()) if len(rows) == 1}
    contested = [(c, rows) for c, rows in sorted(by_class.items()) if len(rows) > 1]
    return answered, contested


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
    powers, total = read_powers(buf)
    prov = provenance(mhd_path, buf, total)

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

    # The archetype inherent Mids grids for each class, which the display join cannot
    # reach. See `archetype_inherents` below.
    inherents, contested = archetype_inherents(powers)
    for cls, rows in contested:
        print(
            f"  {args.dataset}: {cls} has {len(rows)} gridded single-class inherents "
            f"({', '.join(rows)}) — no row emitted, the writer keeps our spelling",
            file=sys.stderr,
        )

    # The provenance fields stay at top level, in this order, because the emitted file is
    # a committed artefact with readers (`scripts/convert-mids-name-map.cjs`, the census
    # keys). `provenance()` supplies them; `path` is dropped, being per-machine.
    payload = {
        "dataset": args.dataset,
        "database": prov["database"],
        "version": prov["version"],
        "sha256": prov["sha256"],
        "powerCount": prov["powerCount"],
        # The marker a reader checks before trusting a key's case. A dump written before
        # MBDEXPORT-6 carries folded keys and no marker, and its case is unrecoverable
        # without the `.mhd` — which is a different answer from "Mids spells it that way".
        "powersetKeys": "literal",
        # `Class_X` -> Mids' name for that archetype's own inherent. A separate key rather
        # than rows in `powersets`, because it answers a different question with a different
        # join: these are not two spellings of one name but the one power of several that
        # Mids will actually place. DATA-GAP MBDEXPORT-24.
        "archetypeInherents": inherents,
        "powersets": {k: sets[k] for k in sorted(sets, key=str.lower)},
    }

    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(payload, fh, ensure_ascii=False, indent=1, sort_keys=False)
        fh.write("\n")

    print(
        f"[emit_mids_names] {args.dataset}: {payload['powerCount']} powers in "
        f"{len(payload['powersets'])} powersets, {len(inherents)} archetype inherents "
        f"(db {payload['version']}) -> "
        f"{os.path.relpath(out_path, REPO_ROOT)}",
        file=sys.stderr,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
