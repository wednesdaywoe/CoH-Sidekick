#!/usr/bin/env python3
"""Key for DATA-GAP MBDIMPORT-17: every contract power names itself.

Run from the repo root:  python3 scripts/keys/mbdimport17-pool-internal-names.py

MBDIMPORT-17 was a clean converter split — archetype-set powers carried `internalName` at 100%
and pool/epic powers carried it at 0%, 1,846 of them across four forks. Nothing was ever
mismatched, because the Rust loader derived the identity from `fullName`'s last segment and that
derivation agreed on all 1,846. The fix was to read the field the export owns instead of
reconstructing it, in both pool emitters.

This key guards the closure from two directions, because the defect can come back in two shapes:

  1. a power with no `internalName` at all — the emitter stopped writing it, and the loader's
     derivation silently covers for it again;
  2. an `internalName` that DISAGREES with `fullName`'s last segment — which the old derivation
     would have got wrong. This half can only fail once the field is actually read, so it is a
     check the register could not have run before the fix. A disagreement is not automatically a
     defect (the export may genuinely name a power unlike its path), but it means the loader's
     fallback is no longer equivalent, and that has to be adjudicated rather than discovered.

Exits non-zero on either. Both halves must stay at zero.
"""
import json, glob, os, re, sys

FORKS = ("homecoming", "rebirth", "thunderspy", "brainstorm")


def underscored(value):
    """The loader's `underscored`: every run of whitespace becomes one underscore."""
    return re.sub(r"\s+", "_", value)


def at_powers(fork):
    for f in glob.glob(f"contract/{fork}/powersets/*/*/*.json"):
        for p in json.load(open(f)).get("powers", []):
            yield p


def pool_powers(fork, fn):
    p = f"contract/{fork}/{fn}.json"
    if not os.path.exists(p):
        return
    for v in json.load(open(p)).values():
        if isinstance(v, dict):
            yield from v.get("powers", [])


def main():
    if not os.path.isdir("contract"):
        print("run from the repo root (no ./contract)", file=sys.stderr)
        return 2

    rows, missing_total, drift = [], 0, []
    for fork in FORKS:
        if not os.path.isdir(f"contract/{fork}"):
            continue
        counts = {}
        for side, powers in (
            ("AT", at_powers(fork)),
            ("pool", (p for fn in ("power-pools", "epic-pools") for p in pool_powers(fork, fn))),
        ):
            has = miss = 0
            for p in powers:
                ident = p.get("internalName")
                if not ident:
                    miss += 1
                    continue
                has += 1
                full = p.get("fullName")
                if full and underscored(full.rsplit(".", 1)[-1]) != ident:
                    drift.append((fork, side, full, ident))
            counts[side] = (has, miss)
            missing_total += miss
        rows.append((fork, *counts["AT"], *counts["pool"]))

    print(f"{'fork':<12} {'AT has':>8} {'AT MISSING':>11} {'pool has':>9} {'pool MISSING':>13}")
    for fork, ah, am, ph, pm in rows:
        print(f"{fork:<12} {ah:>8} {am:>11} {ph:>9} {pm:>13}")
    print()

    if missing_total:
        print(f"BROKEN: {missing_total} contract powers carry no internalName. An emitter stopped "
              f"writing the field, and the loader's fullName derivation is covering for it.")
        return 1
    if drift:
        print(f"BROKEN: {len(drift)} powers name themselves unlike their fullName tail — the "
              f"loader's derivation is no longer equivalent to the field. Adjudicate each:")
        for fork, side, full, ident in drift[:20]:
            print(f"    {fork:<12} {side:<5} {full}  ->  {ident!r}")
        if len(drift) > 20:
            print(f"    ... and {len(drift) - 20} more")
        return 1

    total = sum(ah + ph for _, ah, _, ph, _ in rows)
    print(f"ok: all {total} contract powers carry internalName, and every one agrees with its "
          f"fullName tail.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
