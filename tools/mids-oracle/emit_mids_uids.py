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

Writes `mids-tables/<dataset>/mids-uids.json`, which is COMMITTED;
`emit-pipeline-json.cjs` copies it to `pipeline/<dataset>/mids-uids.json` and
`emit-contract.cjs` reads that. It wrote
`src/data/datasets/<dataset>/generated/mids-uids.ts` until 2026-09-24 and that
TypeScript is no longer regenerated.

The committed copy is why `mids-tables/` exists rather than this writing straight
into `pipeline/`. Every other pipeline input is re-derived from the committed
`exported_powers/`, so a clone rebuilds it; this one needs an installed Mids
Reborn (see MIDS_DB below), which CI does not have and a clone will not have. The
dependency is real and cannot be designed away — see the note at the top about why
a UID cannot be derived — so it is vendored and named instead of hidden.

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

import read_enhdb
import read_i12

REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))

# The installed Mids' `Databases` directory, derived from the one file `read_i12`
# spells — so every leg here reads the install `read_enhdb` and the DSH harness read,
# by construction rather than by copies of a string agreeing (PROV-3).
#
# This pointed into `MidsReborn-master/` until 2026-09-24, finishing the move
# `read_enhdb.DEFAULT_MHD` made on 2026-09-13. That tree still held an `EnhDB.mhd`,
# so nothing broke loudly — it was simply an OLDER Homecoming database (bf5b978e…
# against the prefix's b5b4379d…), which meant these UID tables were generated from
# a database nothing else in the repo read. Rebirth was byte-identical across the
# two trees, which is what made the Homecoming difference signal rather than noise.
MIDS_DB = os.path.dirname(os.path.dirname(read_enhdb.DEFAULT_MHD))

# Which EnhDB each dataset reads. Brainstorm is Homecoming's open beta and
# shares HC's enhancement namespace.
#
# THUNDERSPY IS NOT A MIDS DATABASE. Mids Reborn ships Generic, Homecoming and
# Rebirth, and has never supported Thunderspy (user, 2026-09-09). This read the
# drop's `Thunderspy/EnhDB.mhd` until 2026-09-24 and now reads Mids' own GENERIC
# database, which is the same bytes (sha256 f7478632…, verified against the drop
# before the switch) reached without a vendored tree. It is the closest stand-in
# there is, not the fork's own file, and this comment used to claim otherwise. It
# gets 210 of Thunderspy's 213 sets because they are shared CoH sets; the three it
# cannot get (`kb` and the two Primalist ATOs) exist in no Mids database at all.
# See DATA-GAP MBDEXPORT-2 — the staleness gate pins this file's sha256, which
# grades freshness and is blind to which database it is.
DATASET_SOURCES = {
    "homecoming": os.path.join(MIDS_DB, "Homecoming", "EnhDB.mhd"),
    "brainstorm": os.path.join(MIDS_DB, "Homecoming", "EnhDB.mhd"),
    "rebirth": os.path.join(MIDS_DB, "Rebirth", "EnhDB.mhd"),
    "thunderspy": os.path.join(MIDS_DB, "Generic", "EnhDB.mhd"),
}

# Which Mids database each dataset's table really came from. The `.mhd` cannot say it — its
# header is "Mids Reborn Enhancement Database" and nothing else, no fork and no version — and
# `Thunderspy/EnhDB.mhd` reads like the fork's own file when it is not. `sourceSha256` inside
# the table grades freshness and is structurally blind to WHICH database the bytes are. See
# DATA-GAP MBDEXPORT-2.
#
# This used to be rendered into the generated TypeScript's header comment, so the file the
# reader opened stated it. The output is JSON now and carries no comments, so the run prints
# it to stderr and this dict is the only written home it has. Losing it from the artefact is
# a real cost of the move and is recorded here rather than papered over.
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
    io_set_levels: dict[str, list[int]] = {}
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

        # The level range Mids will hold this set's pieces at, read off the SET record.
        #
        # Both records carry `level_min`/`level_max` — the set's and each enhancement's —
        # and they disagree on 96 of Homecoming's 227 sets, so which one is read is a
        # decision and not a detail. It was measured rather than reasoned: Rebirth's
        # `Rolling_Barrage_A` states 34 on its own record and its set states 24, and a slot
        # written at 24 came back from Mids' own reader untouched. The piece record is not
        # the gate; the set record is. MBDEXPORT-22.
        #
        # Stored as GAME levels. The field is in `IoLevel` space, which is 0-based (a
        # level-50 piece is 49) and which Mids clamps against raw, so +1 here puts it in
        # the same space as the export's own `minLevel`/`maxLevel` and the two can be
        # compared without either side having to know the other's base.
        io_set_levels[key] = [s["level_min"] + 1, s["level_max"] + 1]

        holes = [n for n in range(1, size + 1) if n not in by_piece]
        if holes:
            notes.append(f"{s['uid']}: no UID for piece {holes}, emitted empty")

    # Generic (crafted) IOs and the special/exotic rosters. Both are flat name
    # spaces the exporter validates against rather than a per-set list.
    generic = sorted(e["uid"] for e in enh if e["type"] == "InventO")

    # The crafted generic IOs' own level range, folded across the roster the way the
    # per-set prefix is folded across a set's pieces: measured identical on all 26 records
    # in every fork (10-50), so one pair says what 26 would. A database that ever mixes
    # them stops the generator rather than shipping a bound that is right for 25 of them.
    #
    # It is not the sets' range restated. Our picker offers 10-53 and Homecoming's own
    # strength curve runs past 50, so a level-53 generic IO is a real piece on our side and
    # a level-50 one on Mids' — measured, clamped silently. MBDEXPORT-22.
    generic_ranges = sorted({(e["level_min"] + 1, e["level_max"] + 1)
                             for e in enh if e["type"] == "InventO"})
    if len(generic_ranges) > 1:
        raise SystemExit(
            f"{mhd_path}: crafted generic IOs carry more than one level range "
            f"({generic_ranges}); one pair cannot describe them"
        )

    special = sorted(e["uid"] for e in enh if e["type"] == "SpecialO")
    origin = sorted(e["uid"] for e in enh if e["type"] == "Normal")

    return {
        "ioSetPieces": io_set_pieces,
        "ioSetPrefix": io_set_prefix,
        "ioSetLevels": io_set_levels,
        # Absent, not defaulted, when the database names no crafted generic IO at all:
        # Mids states nothing there and the writer must not invent a bound.
        "genericIOLevels": list(generic_ranges[0]) if generic_ranges else None,
        "genericIO": generic,
        "special": special,
        "origin": origin,
        "sourceSha256": source_sha256,
        "notes": notes,
    }


# A scalar list this long or shorter is printed on one line. It is the layout the retired
# TypeScript had, and it is what makes the committed file reviewable: a set's six piece UIDs
# and an enhancement's four name fields read as one row each, while the flat rosters
# (`genericIO`, `special`, `origin`) stay one entry per line, so a Mids version bump that
# adds or drops one shows as a single changed line.
INLINE_LIST_MAX = 8


def _pretty(value, indent: int = 0) -> str:
    """JSON with two-space indent, except that short scalar lists stay on their own line."""
    pad = "  " * indent
    inner = "  " * (indent + 1)
    if isinstance(value, list):
        if not value:
            return "[]"
        scalar = all(not isinstance(v, (list, dict)) for v in value)
        if scalar and len(value) <= INLINE_LIST_MAX:
            return "[" + ", ".join(json.dumps(v, ensure_ascii=False) for v in value) + "]"
        rows = ",\n".join(inner + _pretty(v, indent + 1) for v in value)
        return "[\n" + rows + "\n" + pad + "]"
    if isinstance(value, dict):
        if not value:
            return "{}"
        rows = ",\n".join(
            f"{inner}{json.dumps(k, ensure_ascii=False)}: {_pretty(v, indent + 1)}"
            for k, v in value.items()
        )
        return "{\n" + rows + "\n" + pad + "}"
    return json.dumps(value, ensure_ascii=False)


def write_table(dataset: str, name: str, payload: dict) -> str:
    """
    Write one table to `mids-tables/<dataset>/<name>.json` and return the path.

    COMMITTED, and the only pipeline input not derived from `exported_powers/`.
    `emit-pipeline-json.cjs` copies it into `pipeline/<dataset>/`, which is what
    `emit-contract.cjs` reads, so a clone with no Mids install still builds the contract.
    That copy is a `JSON.stringify` of this file's parsed contents, so the byte format here
    is free and the KEY ORDER is not: it survives the copy and must not be disturbed.

    Pretty-printed for the same reason `hand-data/` is. This is a file a person reviews on a
    Mids version bump, and a one-line JSON diff says only that the line changed.
    """
    out_path = os.path.join(REPO_ROOT, "mids-tables", dataset, f"{name}.json")
    os.makedirs(os.path.dirname(out_path), exist_ok=True)
    with open(out_path, "w", encoding="utf-8") as fh:
        fh.write(_pretty(payload) + "\n")
    return out_path


def render_json(table: dict) -> dict:
    """
    The table as the contract emitter's `MIDS_UIDS` export.

    Key order is the order the retired TypeScript literal declared, and the three maps are
    sorted, because that is what the emitter read back out of the transpiled module. Both
    are load-bearing on a byte comparison and neither is recoverable from the data.

    `notes` stays out: it is run commentary for the operator, printed to stderr, and was
    never part of the shipped table.
    """
    out = {
        "ioSetPieces": {k: table["ioSetPieces"][k] for k in sorted(table["ioSetPieces"])},
        "ioSetPrefix": {k: table["ioSetPrefix"][k] for k in sorted(table["ioSetPrefix"])},
        "ioSetLevels": {k: table["ioSetLevels"][k] for k in sorted(table["ioSetLevels"])},
    }
    # Absent, not null, when the database names no crafted generic IO — `build_table`'s
    # own distinction, and the TypeScript field was optional for the same reason.
    if table["genericIOLevels"] is not None:
        out["genericIOLevels"] = table["genericIOLevels"]
    out["genericIO"] = table["genericIO"]
    out["special"] = table["special"]
    out["origin"] = table["origin"]
    out["sourceSha256"] = table["sourceSha256"]
    return {"MIDS_UIDS": out}


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
        out_path = write_table(dataset, "mids-uids", render_json(table))
        print(
            f"[emit_mids_uids] {dataset}: {len(table['ioSetPieces'])} sets, "
            f"{len(table['genericIO'])} generic, {len(table['special'])} special, "
            f"{len(table['origin'])} origin → {os.path.relpath(out_path, REPO_ROOT)}",
            file=sys.stderr,
        )
        # The two facts the retired TypeScript header carried. Nothing written states them
        # now, so the run has to — see DATASET_PROVENANCE.
        print(
            f"[emit_mids_uids] {dataset}: source {read_i12.cite_path(source)}, which is: "
            f"{DATASET_PROVENANCE[dataset]}",
            file=sys.stderr,
        )
        for note in table["notes"]:
            print(f"[emit_mids_uids] {dataset}: note: {note}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
