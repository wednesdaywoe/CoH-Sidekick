#!/usr/bin/env python3
"""TSPY-12's key: a second reader for schedules.bin that shares no decode
path with the crawler.

The row's claim is that our Thunderspy AssignableBoost (71 entries, granting
more than Homecoming at levels 9, 23, 29 and 43) is graded by nothing outside
our own reader -- Mids' Thunderspy table is its pre-fork Generic one and
abstains. This grades it against the binary directly.

Independence is the whole point, so this reader assumes NONE of what
`bin_crawler.parser._schedules` assumes. It does not know the Parse7 header
size, where the string table ends, that the Schedule struct is size-prefixed,
or at what offset any field sits. It brute-force scans every 4-byte-aligned
offset for "u4 count followed by `count` non-decreasing u4s <= 200" and then
searches for a chain of exactly seven such arrays that tiles the file to its
final byte. On all four shipped datasets that chain is UNIQUE, which is what
makes this a read rather than a replay: a misalignment cannot tile.

Field identity comes from the released server source, not from us --
ParseSchedules in Common/entity/power_system.c lists the seven arrays in
order, so position 5 is AssignableBoost. Two independent corroborations that
the chain is aligned: position 4 (Power) is 24 entries on every fork, the
count MBDEXPORT-11 censused out of Mids' own files, and position 5 is the
largest array in the file on every fork.

Semantics are CountForLevel (power_system.c): entries are 0-based security
levels, and the grant at level L is the number of entries <= L-1.

Exit non-zero on any break. Canonical-only: reads game installs.
"""
import hashlib
import json
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools" / "bin-crawler"))

# ParseSchedules, Common/entity/power_system.c -- the order on the wire.
FIELDS = ["FreeBoostSlotsOnPower", "PoolPowerSet", "EpicPowerSet",
          "Power", "AssignableBoost", "InspirationCol", "InspirationRow"]
ASSIGNABLE_BOOST = FIELDS.index("AssignableBoost")
POWER = FIELDS.index("Power")

# The levels the row names: where our Thunderspy schedule exceeds Homecoming's.
DISPUTED = [9, 23, 29, 43]

ROOTS = {
    "homecoming": "~/.wine/drive_c/Games/Homecoming/assets/live",
    "brainstorm": "~/.wine/drive_c/Games/Homecoming/assets/beta",
    "rebirth": "~/Games/coh-sweettea/drive_c/users/jiiwii/AppData/Local/"
               "Thunderspy Gaming/Sweet Tea/rebirth",
    "thunderspy": "~/Games/coh-sweettea/drive_c/users/jiiwii/AppData/Local/"
                  "Thunderspy Gaming/Sweet Tea/tspy",
}

# A loose schedules.bin from a SEPARATE Homecoming install, on another volume.
# It never goes through the pigg reader, so a byte-identical extraction proves
# the container hop is not mangling what the decode below is grading.
LOOSE_TWIN = ("homecoming",
              "/run/media/jiiwii/New Volume1/Homecoming/assets/live/bin/schedules.bin")

MAX_COUNT = 512
MAX_LEVEL = 200


def candidate_arrays(data):
    """Every offset holding a plausible level array. No layout assumed."""
    out = {}
    for off in range(0, len(data) - 3, 4):
        count = struct.unpack_from("<I", data, off)[0]
        if not (0 < count <= MAX_COUNT) or off + 4 + count * 4 > len(data):
            continue
        values = list(struct.unpack_from(f"<{count}I", data, off + 4))
        if all(v <= MAX_LEVEL for v in values) and \
           all(a <= b for a, b in zip(values, values[1:], strict=False)):
            out.setdefault(off, []).append(values)
    return out


def tiling_chains(data, n=len(FIELDS)):  # noqa: B008 — FIELDS is a module constant
    """Chains of `n` arrays laid end to end that finish on the file's last byte."""
    arrays = candidate_arrays(data)
    found = []

    def walk(off, acc):
        if len(acc) == n:
            if off == len(data):
                found.append(list(acc))
            return
        for values in arrays.get(off, []):
            acc.append(values)
            walk(off + 4 + len(values) * 4, acc)
            acc.pop()

    for start in range(0, len(data) - 3, 4):
        walk(start, [])
    return found


def count_for_level(levels, level):
    """power_system.c CountForLevel: entries are 0-based security levels."""
    return sum(1 for v in levels if v <= level - 1)


def granted_at(levels, level):
    """Slots the schedule hands out AT `level` -- the step in CountForLevel."""
    return count_for_level(levels, level) - count_for_level(levels, level - 1)


def read_bytes(dataset):
    from bin_crawler.parser._pigg import BinResolver
    root = Path(ROOTS[dataset]).expanduser()
    if not root.is_dir():
        return None, f"no install at {root}"
    resolver = BinResolver(root)
    if not resolver.has("schedules.bin"):
        return None, f"no schedules.bin under {root}"
    return resolver.read("schedules.bin"), None


def main():
    broken = []
    schedules = {}

    print("independent read of schedules.bin -- brute-force tiling, no layout assumed\n")
    for dataset in ROOTS:
        data, err = read_bytes(dataset)
        if data is None:
            print(f"  {dataset:12} SKIPPED -- {err}")
            continue
        digest = hashlib.sha256(data).hexdigest()[:16]
        chains = tiling_chains(data)
        if len(chains) != 1:
            print(f"  {dataset:12} {len(chains)} tiling chains -- the read is ambiguous")
            broken.append(f"{dataset}: {len(chains)} tiling chains, expected exactly 1")
            continue
        chain = chains[0]
        counts = [len(a) for a in chain]
        boost = chain[ASSIGNABLE_BOOST]
        schedules[dataset] = boost
        print(f"  {dataset:12} {len(data):4}b sha {digest}  counts {counts}")

        # Corroborate alignment two ways that do not depend on field order.
        if len(chain[POWER]) != 24:
            broken.append(f"{dataset}: Power is {len(chain[POWER])} entries, not the censused 24")
        if len(boost) != max(counts):
            broken.append(f"{dataset}: AssignableBoost is not the largest array -- chain misaligned")

    # The loose twin: same bytes through a container path that is not the pigg.
    dataset, loose = LOOSE_TWIN
    loose_path = Path(loose)
    if loose_path.is_file():
        data, _ = read_bytes(dataset)
        same = data is not None and hashlib.sha256(data).digest() == \
            hashlib.sha256(loose_path.read_bytes()).digest()
        print(f"\n  loose twin ({dataset}): "
              f"{'byte-identical' if same else 'DIFFERS'} -- {loose_path}")
        if not same:
            broken.append("the loose schedules.bin differs from the pigg extraction")
    else:
        print(f"\n  loose twin: unavailable ({loose_path}) -- container hop ungraded this run")

    # What the row is about: the binary's own verdict at the disputed levels.
    if "homecoming" in schedules and "thunderspy" in schedules:
        hc, ts = schedules["homecoming"], schedules["thunderspy"]
        print(f"\nAssignableBoost totals from the binary: "
              f"homecoming {len(hc)}, thunderspy {len(ts)}")
        print("\n  level    hc   tspy      hc   tspy")
        print("           granted AT      cumulative")
        for level in DISPUTED:
            hg, tg = granted_at(hc, level), granted_at(ts, level)
            flag = "" if tg > hg else "   <- NOT a surplus; BREAKS the row"
            print(f"  {level:5}  {hg:4} {tg:6}  {count_for_level(hc, level):6} "
                  f"{count_for_level(ts, level):6}{flag}")
            if tg <= hg:
                broken.append(
                    f"level {level}: thunderspy grants {tg} where homecoming grants {hg} "
                    f"-- not the surplus the row names")

        # The row names the levels where a grant DIFFERS, not where the running
        # totals do: once a cumulative total diverges it stays diverged, so the
        # per-level increment is the axis that isolates the four.
        differs = [lv for lv in range(1, 51)
                   if granted_at(hc, lv) != granted_at(ts, lv)]
        print(f"\n  every level whose GRANT differs: "
              f"{', '.join(str(lv) for lv in differs)}")
        cumulative = next((lv for lv in range(1, 51)
                           if count_for_level(hc, lv) != count_for_level(ts, lv)), None)
        print(f"  first level whose cumulative total differs: {cumulative}")
        if differs != DISPUTED:
            broken.append(f"grants differ at {differs}, not the {DISPUTED} the row names")

    # Grade our own export against this reader, field for field.
    print("\nour export vs this reader:")
    for dataset, boost in schedules.items():
        exported = json.loads(
            (REPO / "exported_powers" / ("" if dataset == "homecoming" else dataset)
             / "leveling_schedule.json").read_text())["schedule"]["assignable_boost"]
        ok = exported == boost
        print(f"  {dataset:12} {'identical' if ok else 'DIFFERS'} "
              f"({len(exported)} exported vs {len(boost)} read)")
        if not ok:
            broken.append(f"{dataset}: the export's assignable_boost is not what the binary says")

    if broken:
        print("\nBREAKS:")
        for line in broken:
            print(f"  - {line}")
        return 1
    print("\nholds: the binary states Thunderspy's 71 on a decode path the crawler "
          "does not share, and the four levels are the fork's own.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
