#!/usr/bin/env python3
"""F74 -- every panic site that survives into the SHIPPED wasm engine.

**Why this exists.** F74 is about `panic=abort` on `wasm32-unknown-unknown`: a panic inside
`coh_math::recalculate` is a dead tab that no `Result` and no `try` reaches. The row's own
measure of the problem was "~281 `unwrap`/`expect` sites in the crate, before counting raw
indexing" -- a source count, and the wrong denominator. What can kill a tab is a panic site the
OPTIMIZER could not delete, in code the doors can actually reach; a source count includes every
site behind a `#[cfg(test)]`, every one LLVM proved unreachable, and every one in a crate half
the build drops.

**What it measures.** `core::panic::Location` is a 16-byte record in linear memory -- a pointer
to the source path, its length, the line and the column -- emitted for each surviving panic
site, including the `#[track_caller]` ones every `unwrap`/`expect`/slice index goes through. So
the shipped artifact carries its own list, and this reads it: find every source-path string in
the data segments, then find every 16-byte record that points at one with a plausible line and
column. The result is the exact (file, line, column) set, which is checkable against the source
-- the printed listing names what is on that line.

**The reader was graded against a binary whose answer is known.** A scan for 16-byte records
that "look like" a Location can invent one, so it was run against a purpose-built wasm-bindgen
module holding exactly ONE panic (an out-of-bounds index, built release the way the engine is,
2026-09-23): it reported that one site and three in std, and nothing else. The corroboration on
the engine itself is the listing below -- almost every line it names is visibly an `unwrap`, an
index or a `panic!`, which a false positive would not be.

This is a CEILING on what a hostile build can reach, not a count of what it does reach: a site
in the list may still be unreachable from the doors. The floor is the other artifact --
the mutated-build harness, which drives the doors and grades
on whether anything unwinds. Neither alone is the argument.

**The adjudication is committed, not re-derived.** Reading this list and deciding each site is
total was the original argument, and it was load-bearing on nothing: no check failed when a later
edit turned one of the readings false, and the census had to be re-run by whoever remembered.
`f74-panic-sites-baseline.json` beside this file holds that reading -- one entry per site, with
its class and the argument for it -- and `--check` grades a shipped artifact against it, which is
what `beta-engine-staleness` runs. A site the baseline does not hold is a finding, not a number.

Run:  python3 scripts/keys/f74-wasm-panic-sites.py [path/to/coh_wasm_bg.wasm]
          the listing, adjudicated by eye (default: the beta's shipped artifact beside this
          checkout)
      python3 scripts/keys/f74-wasm-panic-sites.py --check [path]
          grade it against the committed baseline; exits 1 on any site outside it
      python3 scripts/keys/f74-wasm-panic-sites.py --self-test
          grade the checker against the four faults it must red on
"""

import json
import re
import struct
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
DEFAULT_WASM = REPO.parent / "CoH-Sidekick" / "src" / "engine" / "wasm" / "coh_wasm_bg.wasm"

# A path string in the data section is a source file iff it looks like one. Workspace members
# are emitted relative (the build remaps the registry and toolchain prefixes -- see the beta's
# scripts/build-engine.mjs), so a `crates/...` path is one of ours and `/cargo/...` is a dep.
SOURCE_PATH = re.compile(rb"(?:crates|vendor)/[A-Za-z0-9_./-]+\.rs|/cargo/[A-Za-z0-9_./+-]+\.rs|/rustc/[A-Za-z0-9_./-]+\.rs")


def read_uleb(buf, pos):
    result = 0
    shift = 0
    while True:
        byte = buf[pos]
        pos += 1
        result |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return result, pos
        shift += 7


def data_segments(wasm):
    """Every active data segment as (memory offset, bytes)."""
    assert wasm[:8] == b"\x00asm\x01\x00\x00\x00", "not a wasm module"
    pos = 8
    out = []
    while pos < len(wasm):
        section_id = wasm[pos]
        pos += 1
        size, pos = read_uleb(wasm, pos)
        end = pos + size
        if section_id == 11:  # data
            count, p = read_uleb(wasm, pos)
            for _ in range(count):
                flags, p = read_uleb(wasm, p)
                offset = 0
                if flags in (0, 2):
                    if flags == 2:
                        _memidx, p = read_uleb(wasm, p)
                    # init expr: i32.const <sleb> end
                    assert wasm[p] == 0x41, "unexpected data offset expression"
                    p += 1
                    value = 0
                    shift = 0
                    while True:
                        byte = wasm[p]
                        p += 1
                        value |= (byte & 0x7F) << shift
                        shift += 7
                        if not byte & 0x80:
                            if shift < 32 and byte & 0x40:
                                value |= -(1 << shift)
                            break
                    offset = value
                    assert wasm[p] == 0x0B, "unterminated data offset expression"
                    p += 1
                length, p = read_uleb(wasm, p)
                out.append((offset, wasm[p : p + length]))
                p += length
        pos = end
    return out


BASELINE = Path(__file__).resolve().parent / "f74-panic-sites-baseline.json"


def collect_sites(wasm):
    """Every surviving `core::panic::Location` in the artifact, split ours/theirs."""
    segments = data_segments(wasm)

    # address -> (path, length) for every source-path string in the data.
    strings = {}
    for offset, blob in segments:
        for match in SOURCE_PATH.finditer(blob):
            strings[offset + match.start()] = (match.group().decode(), len(match.group()))

    # Scan every 4-byte-aligned word for a Location record: ptr, len, line, col.
    sites = set()
    for offset, blob in segments:
        limit = len(blob) - 16
        start = (-offset) % 4
        for i in range(start, max(limit, 0), 4):
            ptr, length, line, col = struct.unpack_from("<IIII", blob, i)
            known = strings.get(ptr)
            if not known or known[1] != length:
                continue
            if not (1 <= line <= 100_000) or not (1 <= col <= 500):
                continue
            sites.add((known[0], line, col))

    ours = sorted(s for s in sites if s[0].startswith(("crates/", "vendor/")))
    theirs = sorted(s for s in sites if not s[0].startswith(("crates/", "vendor/")))
    return ours, theirs


_SOURCE_CACHE = {}


def source_text(file, line):
    """The stripped source line a site points at, or None if the tree cannot answer."""
    if file not in _SOURCE_CACHE:
        path = REPO / file
        _SOURCE_CACHE[file] = path.read_text().splitlines() if path.exists() else []
    lines = _SOURCE_CACHE[file]
    return lines[line - 1].strip() if 0 < line <= len(lines) else None


def print_listing(path, ours, theirs):
    by_file = {}
    for file, line, col in ours:
        by_file.setdefault(file, []).append((line, col))

    print(f"{path}")
    print(
        f"{len(ours) + len(theirs)} surviving panic sites: "
        f"{len(ours)} in this workspace, {len(theirs)} in std/deps\n"
    )
    for file in sorted(by_file):
        print(f"{file}  ({len(by_file[file])})")
        for line, col in sorted(by_file[file]):
            text = source_text(file, line) or "<no source>"
            print(f"  :{line}:{col}  {text[:110]}")
        print()


# ------------------------------------------------------------------- the check
#
# **Why the baseline is keyed on the SOURCE TEXT and not the line.** F74's own row records two
# stale-anchor corrections in five days and concludes that a finding should anchor on a symbol
# rather than a line. The same argument applies harder here: every unrelated edit above a site
# shifts its line, and a gate that reds on that is a gate somebody turns off. The text of the
# expression is what was adjudicated -- `.last_mut().expect("just pushed")` is total because of
# what it says -- so the text is the key, and an edit to the expression is exactly the event
# that should force a re-reading.
#
# **It reds in BOTH directions, and the second one is not pedantry.** A site LEAVING is how the
# `enhancement.rs` clamp fix was confirmed (44 became 43, and the one that left was the one the
# fix retired), so a departure is evidence and the baseline has to be re-stated to record it.
# A baseline allowed to describe more than the artifact holds decays into a wish.
#
# **What it cannot see**, stated because a green gate is a claim about the observer: the
# adjudication for a given site often rests on a guard somewhere ELSE in the function --
# `appliers/defense.rs`'s `base[base.len() - 1]` is total because of a two-step argument over
# guards thirty lines above it. Break that guard without touching the indexing line and this
# check stays green. The floor -- the mutated-build harness -- is what covers
# that direction, probabilistically, by driving the doors. Neither alone is the argument.

CLASSES = {
    "invariant": "total by an invariant in its own function, a guard above it, or the type",
    "loader": "total by a guarantee the dataset loader enforces and states",
    "rule1-const": "deliberate Rule-1 loudness over a const compiled into the binary",
}


def load_baseline(path=BASELINE):
    raw = json.loads(path.read_text())
    counts = {}
    for entry in raw["adjudicated"]:
        if entry["class"] not in CLASSES:
            sys.exit(f"{path.name}: unknown class {entry['class']!r}")
        counts[(entry["file"], entry["text"])] = entry
    if len(counts) != len(raw["adjudicated"]):
        sys.exit(f"{path.name}: two entries share one (file, text) key")
    return counts


def check(ours, baseline):
    """Faults, as printable lines. Empty means the artifact holds nothing unadjudicated."""
    faults = []
    observed = {}
    for file, line, col in ours:
        text = source_text(file, line)
        if text is None:
            faults.append(
                f"  SKEW        {file}:{line}:{col} -- the artifact names a line this tree does "
                f"not have, so the engine is stale against the source and no reading below is valid"
            )
            continue
        observed.setdefault((file, text), []).append((line, col))

    for key in sorted(observed):
        if key not in baseline:
            file, text = key
            where = ", ".join(f":{line}:{col}" for line, col in sorted(observed[key]))
            faults.append(f"  UNADJUDICATED  {file} {where}  {text[:100]}")
    for key in sorted(baseline):
        if key not in observed:
            file, text = key
            faults.append(
                f"  DEPARTED       {file}  {text[:100]}"
                f"  (adjudicated {baseline[key]['class']}; it is no longer in the artifact)"
            )
    for key in sorted(set(observed) & set(baseline)):
        want, got = baseline[key]["count"], len(observed[key])
        if want != got:
            faults.append(
                f"  COUNT          {key[0]}  {key[1][:80]}  baseline {want}, artifact {got}"
            )
    return faults


def self_test():
    """Grade the checker against artifacts it must refuse. A check nobody has seen fail is a wish."""
    baseline = load_baseline()
    any_key = sorted(baseline)[0]
    ours = []
    for (file, _text), entry in baseline.items():
        ours.extend([(file, entry["line"], entry["column"])] * entry["count"])

    failures = []

    faults = check(ours, baseline)
    if faults:
        failures.append(f"the committed baseline must grade its own artifact clean, got: {faults}")

    # A site the baseline has never seen.
    intruder = ours + [("crates/coh_math/src/totals.rs", 39, 1)]
    if not any("UNADJUDICATED" in f for f in check(intruder, baseline)):
        failures.append("a panic site absent from the baseline was not reported")

    # A site the baseline claims and the artifact no longer holds.
    fewer = [s for s in ours if s[0] != any_key[0]]
    if not any("DEPARTED" in f for f in check(fewer, baseline)):
        failures.append("a baseline site missing from the artifact was not reported")

    # The same site twice where the baseline adjudicated it once.
    doubled = ours + [s for s in ours if s[0] == any_key[0]][:1]
    if not any("COUNT" in f for f in check(doubled, baseline)):
        failures.append("a site occurring more often than the baseline says was not reported")

    # A line the tree cannot answer for -- the engine stale against the source.
    if not any("SKEW" in f for f in check(ours + [(any_key[0], 99_999, 1)], baseline)):
        failures.append("a site pointing past the end of its file was not reported")

    for line in failures:
        print(f"  FAILED  {line}")
    if failures:
        return 1
    print(f"self-test: the checker reds on all four faults, over {len(ours)} baselined sites")
    return 0


def main():
    argv = [a for a in sys.argv[1:] if not a.startswith("--")]
    flags = {a for a in sys.argv[1:] if a.startswith("--")}
    unknown = flags - {"--check", "--self-test"}
    if unknown:
        sys.exit(f"unknown flag(s): {' '.join(sorted(unknown))}")

    if "--self-test" in flags:
        return self_test()

    path = Path(argv[0]) if argv else DEFAULT_WASM
    if not path.exists():
        sys.exit(f"no wasm artifact at {path}")
    ours, theirs = collect_sites(path.read_bytes())

    if "--check" not in flags:
        print_listing(path, ours, theirs)
        return 0

    baseline = load_baseline()
    faults = check(ours, baseline)
    print(f"{path}")
    print(
        f"{len(ours)} panic sites in this workspace ({len(theirs)} in std/deps, not graded here -- "
        f"a toolchain bump moves that number and it says nothing about this tree)"
    )
    if faults:
        print(f"\nF74: {len(faults)} fault(s) against the adjudicated set:\n")
        for fault in faults:
            print(fault)
        print(
            "\nEvery site in the shipped engine must be adjudicated in "
            f"{BASELINE.name} before it ships. Read the site, decide which class it is in "
            "(or that it is reachable from a door, which is a finding), and re-state the baseline."
        )
        return 1
    tally = {}
    for entry in baseline.values():
        tally[entry["class"]] = tally.get(entry["class"], 0) + entry["count"]
    shape = ", ".join(f"{tally[c]} {c}" for c in sorted(tally))
    print(f"\nF74: every site is in the adjudicated set -- {shape}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
