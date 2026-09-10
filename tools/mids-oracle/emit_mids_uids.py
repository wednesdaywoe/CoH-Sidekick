#!/usr/bin/env python3
"""
Emit the Mids enhancement-UID table for one dataset.

Why this exists: Mids resolves a slot's enhancement by UID string
(`DatabaseAPI.GetEnhancementByUIDName`, substring match) and, on a miss,
leaves `I9Slot.Enh = -1` — an empty slot, no error, no log. So an exporter
that *derives* a UID instead of reading one loses enhancements silently.
Deriving is also impossible in general: the prefix is per-set (`Crafted_` /
`Attuned_` / `Superior_Attuned_`), and Mids carries its own spellings
("Numinas_Convalesence", "ToHit_DeBuff") that no rule recovers from a display
name.

So we read the UIDs out of Mids' own EnhDB.mhd — Mids is the only authority on
its own namespace — and ship them as data.

Usage:
  python3 emit_mids_uids.py --dataset homecoming
  python3 emit_mids_uids.py --dataset all
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
import textwrap

import read_enhdb

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))

MIDS_DB = os.path.join(REPO_ROOT, "MidsReborn-master", "MidsReborn", "Databases")

# Which EnhDB each dataset reads. Brainstorm is Homecoming's open beta and
# shares HC's enhancement namespace.
#
# THUNDERSPY IS NOT A MIDS DATABASE. Mids Reborn ships Generic, Homecoming and
# Rebirth, and has never supported Thunderspy (user, 2026-09-09). The vendored
# `Thunderspy/EnhDB.mhd` this reads is byte-identical to Mids' own GENERIC
# database — the closest stand-in there is, not the fork's own file, and this
# comment used to claim otherwise. It gets 210 of Thunderspy's 213 sets because
# they are shared CoH sets; the three it cannot get (`kb` and the two Primalist
# ATOs) exist in no Mids database at all. See DATA-GAP MBDEXPORT-2 — the
# staleness gate below pins this file's sha256, which grades freshness and is
# blind to which database it is.
DATASET_SOURCES = {
    "homecoming": os.path.join(MIDS_DB, "Homecoming", "EnhDB.mhd"),
    "brainstorm": os.path.join(MIDS_DB, "Homecoming", "EnhDB.mhd"),
    "rebirth": os.path.join(MIDS_DB, "Rebirth", "EnhDB.mhd"),
    "thunderspy": os.path.join(REPO_ROOT, "Thunderspy", "EnhDB.mhd"),
}

# Which Mids database each dataset's table really came from, stated in the FILE the reader
# opens rather than only here. The `.mhd` cannot say it — its header is "Mids Reborn
# Enhancement Database" and nothing else, no fork and no version — so a path is all the
# generated header used to carry, and `Thunderspy/EnhDB.mhd` reads like the fork's own file.
# A sha256 gate over it (src/data/mids-uids-staleness.test.ts) grades freshness and is
# structurally blind to which database it is. See DATA-GAP MBDEXPORT-2.
DATASET_PROVENANCE = {
    "homecoming": "Mids Reborn's own Homecoming database.",
    "rebirth": "Mids Reborn's own Rebirth database.",
    "brainstorm": (
        "Mids Reborn's HOMECOMING database. Mids ships no Brainstorm one, and Brainstorm is "
        "Homecoming's open beta sharing its enhancement namespace, so this is the fork's "
        "database in everything but name."
    ),
    "thunderspy": (
        "Mids Reborn's GENERIC database, byte-identical (md5 2e8d24a3…) to the "
        "`Generic/EnhDB.mhd` a Mids install ships — and NOT a stand-in this repo picked. "
        "`/Thunderspy/` is a third-party database drop built on Generic, with the powers DB "
        "and three small tables swapped out; its author left the enhancement database alone, "
        "so Generic's is what that fork ships. Mids Reborn itself carries Generic, Homecoming "
        "and Rebirth and has never had a Thunderspy database. It resolves 210 of Thunderspy's "
        "213 sets because those are shared CoH sets; the three it cannot — `kb` and the two "
        "Primalist ATOs — exist in no Mids database at all, and the export reports them per "
        "slot. DATA-GAP MBDEXPORT-2."
    ),
}

# The attunement prefix a record's UID carries, longest first so
# `Superior_Attuned_` is never read as `Attuned_`.
#
# Both halves of the file need this. `set_key` strips it to reach the set stem,
# and `prefix_class` KEEPS it, because it is the only thing that tells apart the
# records a set stem alone cannot: `Crafted_Shrapnel_A` and its attuned twin sit
# at the same set and piece, and the game carries both under names Mids has
# never heard of. Stripping the prefix and discarding which one it was left the
# reader with a set and a piece where the game has two records, and six of
# Homecoming's UIDs unresolvable for want of one word.
UID_PREFIX_CLASS = (
    ("Superior_Attuned_", "superior-attuned"),
    ("Attuned_", "attuned"),
    ("Crafted_", "crafted"),
)

UID_PREFIXES = tuple(prefix for prefix, _ in UID_PREFIX_CLASS)

# A record whose UID carries none of them. NOT a synonym for "not attuned" —
# Rebirth spells 23 of its sets bare and 117 of those pieces are the game's
# ATTUNED records. It means Mids states nothing, and the reader must not infer.
NO_PREFIX = "bare"


def prefix_class(uid: str) -> str:
    """Which attunement prefix a UID carries, as the table names it."""
    for prefix, name in UID_PREFIX_CLASS:
        if uid.startswith(prefix):
            return name
    return NO_PREFIX


def set_key(uid: str) -> str:
    """
    Normalize an EnhDB set UID to the planner's `setId`.

    The planner's ids come from the import path, which lowercases the UID stem
    and drops apostrophes. Two sets need more: `Attuned_Cupids_Crush` carries a
    prefix on the *set* record, and `Gaussians_Synchronized_Fire-Control` keeps
    a hyphen the piece UIDs spell as `FireControl`.

    And whitespace is dropped, because Mids carries the game's typos exactly.
    Rebirth's EnhDB names one set `Superior _Endless_Nightmare`, with a stray
    space that its own piece UIDs do not have — the same shape as its
    `Disrupting _Torrent` power. Keeping it produced the setId
    `superior _endless_nightmare`, which matches nothing on either side: the
    export reported "Mids has no enhancement by that name" for a set Mids
    plainly has, and the import failed all six pieces. One key in four forks.
    See DATA-GAP MBDEXPORT-2.
    """
    stem = uid
    for prefix in UID_PREFIXES:
        if stem.startswith(prefix):
            stem = stem[len(prefix):]
            break
    return (
        stem.lower()
        .replace("'", "’")
        .replace("’", "")
        .replace("-", "")
        .replace(" ", "")
    )


def piece_index(uid: str) -> int | None:
    """
    The 1-based piece number a UID's trailing letter names (`..._C` → 3).

    The import path already treats the letter as the piece number
    (`parseIOSetUid`), so the export path has to agree or the round trip
    renumbers a build's pieces. Three sets (Javelin Volley, Gladiator's Armor,
    Gladiator's Javelin) list their pieces in a different order than their
    letters run; the letter is what both halves key on, so that ordering
    difference stays out of this table.
    """
    if len(uid) > 2 and uid[-2] == "_" and uid[-1].upper() in "ABCDEF":
        return ord(uid[-1].upper()) - ord("A") + 1
    return None


def build_table(mhd_path: str) -> dict:
    with open(mhd_path, "rb") as fh:
        raw = fh.read()
    enh, sets, _version, _ = read_enhdb.read_enhdb(raw)
    source_sha256 = hashlib.sha256(raw).hexdigest()

    io_set_pieces: dict[str, list[str]] = {}
    io_set_prefix: dict[str, str] = {}
    notes: list[str] = []
    for s in sets:
        key = set_key(s["uid"])
        if key in io_set_pieces:
            raise SystemExit(f"duplicate set key {key!r} in {mhd_path}")

        # `EnhancementSet.Enhancements` holds array positions into the
        # enhancement list, NOT the `StaticIndex` field each record also
        # carries. The two diverge, and reading the wrong one shifts a set's
        # pieces onto its neighbour's UIDs.
        members = [enh[i]["uid"] for i in s["enhancement_indices"] if 0 <= i < len(enh)]

        by_piece: dict[int, str] = {}
        unlettered: list[tuple[int, str]] = []
        for position, uid in enumerate(members, start=1):
            num = piece_index(uid)
            if num is None:
                unlettered.append((position, uid))
            elif num in by_piece:
                notes.append(f"{s['uid']}: duplicate piece letter on {uid}, kept {by_piece[num]}")
            else:
                by_piece[num] = uid

        # Descriptive-suffix pieces ("..._Rez_Effects") carry no letter, so the
        # only thing left that places them is where they SIT in the member list.
        #
        # This used to file them at the set's LAST free slot, on the importer's
        # own rule, and that is right for every set with one hole. Rebirth's
        # Return From the Grave has two: its sixth record duplicates its fifth,
        # so the letter key drops it and slots 1 and 6 both come free. The rez
        # proc is member ONE, and "last free" bound it to our piece 6, Recharge
        # — silently in both directions, because the import path reads this same
        # table. Position says member one, and Mids' own names agree with our
        # piece order down the set. MBDEXPORT-10.
        for position, uid in unlettered:
            if position in by_piece:
                notes.append(
                    f"{s['uid']}: unlettered member {uid} sits at position {position}, "
                    f"held by {by_piece[position]}; left unplaced"
                )
            else:
                by_piece[position] = uid

        # Sized on Mids' member count, not on the letters that resolved, so a
        # member we could not place leaves a hole rather than shortening the set.
        # An empty string is the table's "Mids cannot name this piece" — the
        # export reports that slot instead of emitting a UID Mids would open
        # empty, and the reverse index skips it.
        size = max([*by_piece, len(members)]) if members else 0
        io_set_pieces[key] = [by_piece.get(n, "") for n in range(1, size + 1)]

        # The set's attunement prefix, read off the pieces this table actually
        # emits rather than off the SET record's own UID — those are two
        # different observations, and it is the piece UID a `.mbd` names.
        #
        # A fold, and one that fails loud rather than picking a winner: measured
        # across all four forks every set's pieces agree, so one value per set
        # says the same thing as one per piece at a fifth the size. If a
        # re-vendored database ever mixes them, the fold is wrong and the
        # generator stops instead of shipping a class that is right for five
        # pieces and wrong for the sixth.
        classes = {prefix_class(uid) for uid in by_piece.values()}
        if len(classes) > 1:
            raise SystemExit(
                f"{s['uid']}: pieces carry more than one attunement prefix "
                f"({sorted(classes)}); the per-set prefix cannot describe them"
            )
        if classes:
            io_set_prefix[key] = classes.pop()

        holes = [n for n in range(1, size + 1) if n not in by_piece]
        if holes:
            notes.append(f"{s['uid']}: no UID for piece {holes}, emitted empty")

    # Generic (crafted) IOs and the special/exotic rosters. Both are flat name
    # spaces the exporter validates against rather than a per-set list.
    generic = sorted(e["uid"] for e in enh if e["type"] == "InventO")
    special = sorted(e["uid"] for e in enh if e["type"] == "SpecialO")
    origin = sorted(e["uid"] for e in enh if e["type"] == "Normal")

    return {
        "ioSetPieces": io_set_pieces,
        "ioSetPrefix": io_set_prefix,
        "genericIO": generic,
        "special": special,
        "origin": origin,
        "sourceSha256": source_sha256,
        "notes": notes,
    }


def wrap_comment(text: str, width: int = 76) -> list[str]:
    """The provenance note as ` * ` lines, so the generated header stays readable."""
    return [f" * {line}" for line in textwrap.wrap(text, width=width)]


def render_ts(dataset: str, source: str, table: dict) -> str:
    rel_source = os.path.relpath(source, REPO_ROOT)
    lines = [
        "/**",
        f" * GENERATED — do not edit. Regenerate with:",
        f" *   python3 tools/mids-oracle/emit_mids_uids.py --dataset {dataset}",
        " *",
        f" * Source: {rel_source}, which is:",
        *wrap_comment(DATASET_PROVENANCE[dataset]),
        " *",
        " * Mids resolves a slotted enhancement by UID substring match and leaves the",
        " * slot empty on a miss, so the export path reads these rather than deriving",
        " * them. See tools/mids-oracle/emit_mids_uids.py for why deriving cannot work.",
        " */",
        "",
        "import type { MidsUidTable } from '../../../mids-uids';",
        "",
        "export const MIDS_UIDS: MidsUidTable = {",
        "  /** setId → piece UID, indexed by pieceNum - 1. */",
        "  ioSetPieces: {",
    ]
    for key in sorted(table["ioSetPieces"]):
        pieces = ", ".join(json.dumps(p) for p in table["ioSetPieces"][key])
        lines.append(f"    {json.dumps(key)}: [{pieces}],")
    lines.append("  },")
    lines.append("")
    lines.append("  /** setId → the attunement prefix its piece UIDs carry. */")
    lines.append("  ioSetPrefix: {")
    for key in sorted(table["ioSetPrefix"]):
        lines.append(f"    {json.dumps(key)}: {json.dumps(table['ioSetPrefix'][key])},")
    lines.append("  },")
    lines.append("")
    lines.append("  /** Every crafted generic IO UID Mids knows. */")
    lines.append("  genericIO: [")
    for uid in table["genericIO"]:
        lines.append(f"    {json.dumps(uid)},")
    lines.append("  ],")
    lines.append("")
    lines.append("  /** Hamidon / Hydra / Titan / D-Sync and friends. */")
    lines.append("  special: [")
    for uid in table["special"]:
        lines.append(f"    {json.dumps(uid)},")
    lines.append("  ],")
    lines.append("")
    lines.append("  /** Origin (TO/DO/SO) enhancement UIDs. */")
    lines.append("  origin: [")
    for uid in table["origin"]:
        lines.append(f"    {json.dumps(uid)},")
    lines.append("  ],")
    lines.append("")
    lines.append("  /** SHA-256 of the EnhDB this was read from; the staleness gate compares it. */")
    lines.append(f"  sourceSha256: {json.dumps(table['sourceSha256'])},")
    lines.append("};")
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Emit the Mids enhancement-UID table")
    ap.add_argument("--dataset", default="homecoming",
                    help="dataset id, or 'all' (default: homecoming)")
    args = ap.parse_args(argv)

    targets = sorted(DATASET_SOURCES) if args.dataset == "all" else [args.dataset]
    for dataset in targets:
        source = DATASET_SOURCES.get(dataset)
        if source is None:
            print(f"error: unknown dataset {dataset!r}", file=sys.stderr)
            return 2
        if not os.path.isfile(source):
            print(f"error: no EnhDB for {dataset}: {source}", file=sys.stderr)
            return 2

        table = build_table(source)
        out_path = os.path.join(
            REPO_ROOT, "src", "data", "datasets", dataset, "generated", "mids-uids.ts"
        )
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        with open(out_path, "w", encoding="utf-8") as fh:
            fh.write(render_ts(dataset, source, table))
        print(
            f"[emit_mids_uids] {dataset}: {len(table['ioSetPieces'])} sets, "
            f"{len(table['genericIO'])} generic, {len(table['special'])} special, "
            f"{len(table['origin'])} origin → {os.path.relpath(out_path, REPO_ROOT)}",
            file=sys.stderr,
        )
        for note in table["notes"]:
            print(f"[emit_mids_uids] {dataset}: note: {note}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
