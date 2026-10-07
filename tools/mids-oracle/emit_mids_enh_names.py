#!/usr/bin/env python3
"""
Emit the Mids enhancement SHORT-NAME table for one dataset.

Why this exists: a legacy `.mxd` names a slotted enhancement twice and neither
name is one we can resolve on our own.

  - the compressed half carries an INDEX into the enhancement array of the Mids
    database that wrote the file. Mids reorders that array between releases —
    1,699 slots across the posted corpus land on a different piece of the same
    set when read against the database vendored here — so an index identifies a
    piece only inside its own file.
  - the post half carries Mids' SHORT CODE, `Ags-ResDam/EndRdx`, which is a
    name and survives reordering. It is also not derivable: it is the set's
    `ShortName` and the piece's `ShortName` joined with a hyphen, and both are
    fields in Mids' own database that no rule recovers from a display name.
    Gaussian's set short name ENDS in a hyphen, so `GssSynFr--Build%` is one
    code and not a typo.

So we read both out of Mids' own EnhDB.mhd, positionally, and ship them as
data. The array position IS the index the compressed half uses, which is why
this is emitted as a list rather than a map.

Writes `mids-tables/<dataset>/mids-enh-names.json`, which is COMMITTED;
`emit-pipeline-json.cjs` copies it into `pipeline/` and `emit-contract.cjs` reads
that. It wrote `src/data/datasets/<dataset>/generated/mids-enh-names.ts` until
2026-09-24 and that TypeScript is no longer regenerated. Like its twin it needs an
installed Mids Reborn, which is why the output is committed — see
`emit_mids_uids.py` and `mids-tables/README.md`.

The UID each row carries is the same one `emit_mids_uids.py` emits, reached a
different way: that script places a piece by the letter its UID ends in, this
one by where it sits in its set's member list. Three sets list their pieces in
a different order than their letters run, so the two disagreeing is a real
signal rather than a tautology; the two are compared.

Usage:
  python3 emit_mids_enh_names.py --dataset homecoming
  python3 emit_mids_enh_names.py --dataset all
"""

from __future__ import annotations

import argparse
import hashlib
import os
import sys

import read_enhdb
import read_i12
from emit_mids_uids import DATASET_PROVENANCE, DATASET_SOURCES, REPO_ROOT, write_table

# How Mids spells a record outside any set, which is what the post half's
# decoration is saying. The kind is carried as DATA rather than re-derived from
# the code, because the decoration Mids writes for one kind has changed: a
# D-Sync is `DSyncO:` in the 1.01 post and `DS:` in the 3.x one, so a reader
# keyed on the prefix would lose every D-Sync written by the other Mids.
#
# The mapping is Mids' own `eType`/`eSubtype` pair, not a judgement:
#   InventO           a crafted generic IO      post writes `Acc-I`
#   Normal            an origin enhancement     post writes `Acc`
#   SpecialO          Hamidon/Hydra/Titan/D-Sync, post writes `<family>:Nucle`
KIND_BY_TYPE = {
    "InventO": "generic",
    "Normal": "origin",
    "SpecialO": "special",
}

# A record inside a set. Named here rather than inferred from "not one of the
# above" so that a fifth Mids type breaks this table instead of quietly
# becoming a set piece.
KIND_SET = "set"


def build_table(mhd_path: str) -> dict:
    with open(mhd_path, "rb") as fh:
        raw = fh.read()
    enh, sets, _version, _ = read_enhdb.read_enhdb(raw)

    # A record's set is read from the SET side rather than from the record's own
    # `set_index`: 44 Homecoming records sit in a set whose member list names
    # them while their own field reads -1, and reading the field would file
    # three Bonesnap pieces as loose enhancements.
    set_of_index: dict[int, dict] = {}
    for enhancement_set in sets:
        for position in enhancement_set["enhancement_indices"]:
            if 0 <= position < len(enh):
                set_of_index.setdefault(position, enhancement_set)

    rows = []
    notes: list[str] = []
    for index, record in enumerate(enh):
        owner = set_of_index.get(index)
        if owner is not None:
            rows.append([owner["short_name"], record["short_name"], record["uid"], KIND_SET])
            continue
        kind = KIND_BY_TYPE.get(record["type"])
        if kind is None:
            notes.append(
                f"index {index} ({record['uid']}) is a {record['type']} in no set; "
                "this table has no kind for it and the reader will decline it"
            )
            kind = ""
        rows.append(["", record["short_name"], record["uid"], kind])

    codes: dict[str, int] = {}
    for index, (set_short, short, _uid, kind) in enumerate(rows):
        if kind != KIND_SET:
            continue
        code = f"{set_short}-{short}"
        if code in codes:
            notes.append(
                f"{code!r} names both index {codes[code]} and index {index}; "
                "the reader declines it unless the file's own index says which"
            )
        else:
            codes[code] = index

    return {
        "enhancements": rows,
        "sourceSha256": hashlib.sha256(raw).hexdigest(),
        "notes": notes,
    }


def render_json(table: dict) -> dict:
    """
    The table as the contract emitter's `MIDS_ENH_NAMES` export.

    `notes` stays out, the same as in `emit_mids_uids.render_json`: it is run commentary
    for the operator and was never part of the shipped table.
    """
    return {
        "MIDS_ENH_NAMES": {
            "enhancements": table["enhancements"],
            "sourceSha256": table["sourceSha256"],
        }
    }


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Emit the Mids enhancement short-name table")
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
        out_path = write_table(dataset, "mids-enh-names", render_json(table))
        in_sets = sum(1 for row in table["enhancements"] if row[3] == KIND_SET)
        print(
            f"[emit_mids_enh_names] {dataset}: {len(table['enhancements'])} records, "
            f"{in_sets} in sets → {os.path.relpath(out_path, REPO_ROOT)}",
            file=sys.stderr,
        )
        # Twin of the line in emit_mids_uids.main: JSON carries no header comment, so which
        # Mids database this is lives in the run and in DATASET_PROVENANCE, nowhere written.
        print(
            f"[emit_mids_enh_names] {dataset}: source {read_i12.cite_path(source)}, which is: "
            f"{DATASET_PROVENANCE[dataset]}",
            file=sys.stderr,
        )
        for note in table["notes"]:
            print(f"[emit_mids_enh_names] {dataset}: note: {note}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
