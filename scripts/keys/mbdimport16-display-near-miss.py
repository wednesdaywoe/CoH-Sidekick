#!/usr/bin/env python3
"""Key for DATA-GAP MBDIMPORT-16: a Mids display-name misspelling defeats the name-map join.

Run from the repo root:  python3 scripts/keys/mbdimport16-display-near-miss.py

The row was cut on a census: the pairs where Mids' display name and ours differ by an edit or
two at the same unlock level. That census argued the population was SMALL and BOUNDED, so a
widened join could be adjudicated pair by pair rather than gated behind a similarity policy. If
it were large, the argument would invert — a fuzzy join over hundreds of pairs is a guessing
machine, and MBDIMPORT-2's whole point is that a wrong bind is silent.

So this still breaks UPWARD, and now also breaks on the OUTCOME, in two directions:

  a misspelling MUST be bound     Mids' spelling and ours survive `stripSeparators` as different
                                  strings, so the import matcher's own ladder cannot reach it
                                  either. Nothing but a name-map row binds this pick, and the
                                  residual pass in `convert-mids-name-map.cjs` is what mints it.

  a separator drift must NOT be   the two names are equal once every non-alphanumeric is gone,
                                  so the ladder already resolves it. MBDIMPORT-13 adjudicated
                                  this class deliberately: a forward row for a pair the matcher
                                  answers on its own is a row that is not a rotation, and a row
                                  that is not a rotation is a chance to bind the wrong power.

Which side a pair falls on is DERIVED from the two spellings, not listed here, so a fork that
renames its way from one class into the other is re-adjudicated rather than silently exempt.

Note what this does NOT say: reachability from a corpus is the sweep's question
(`cargo run -p coh_data --release --example mbd_corpus_sweep`), not this one.
"""
import json, glob, os, re, sys

FORKS = ("homecoming", "rebirth", "thunderspy", "brainstorm")
MAX_DIST = 2
# The census was cut on 4 pairs. MBDIMPORT-17 retired one of them by giving the pool partition
# the `internalName` it had been missing, so Mids' `Quick_Sand` now binds on the internal name
# and never reaches this census at all. Left at the number the row was cut on: this guard is
# for a population that GREW, and tightening it to today's 3 would red on a fork's next rename
# for no reason.
MAX_PAIRS = 4


def lev(a, b):
    if a == b:
        return 0
    if abs(len(a) - len(b)) > MAX_DIST + 1:
        return 99
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        cur = [i]
        for j, cb in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (ca != cb)))
        prev = cur
    return prev[-1]


def strip_separators(s):
    """The import matcher's own ladder: every non-alphanumeric gone."""
    return re.sub(r"[^a-z0-9]", "", str(s or "").lower())


def name_map(fork):
    """`MIDS_NAME_MAP` out of the generated module, as Mids-internal -> ours, per powerset."""
    path = f"src/data/datasets/{fork}/generated/mids-name-map.ts"
    if not os.path.exists(path):
        return {}
    src = open(path, encoding="utf-8").read()
    m = re.search(r"export const MIDS_NAME_MAP[^=]*=\s*(\{.*?\n\});", src, re.S)
    return json.loads(m.group(1)) if m else {}


def ours(fork):
    m = {}
    for f in glob.glob(f"contract/{fork}/powersets/*/*/*.json"):
        d = json.load(open(f))
        sp = d.get("setPath")
        if sp:
            m.setdefault(sp.lower(), []).extend(
                (p.get("internalName") or "", p.get("name") or "", (p.get("available") or 0) + 1)
                for p in d.get("powers", []))
    for fn in ("power-pools", "epic-pools"):
        path = f"contract/{fork}/{fn}.json"
        if not os.path.exists(path):
            continue
        for v in json.load(open(path)).values():
            if isinstance(v, dict) and v.get("setPath"):
                m.setdefault(v["setPath"].lower(), []).extend(
                    (q.get("internalName") or "", q.get("name") or "", (q.get("available") or 0) + 1)
                    for q in v.get("powers", []))
    return m


def main():
    if not os.path.isdir("contract"):
        print("run from the repo root (no ./contract)", file=sys.stderr)
        return 2
    found = []
    for fork in FORKS:
        oracle = f"tools/mids-oracle/mids-power-names.{fork}.json"
        if not os.path.exists(oracle):
            continue
        mids = json.load(open(oracle))["powersets"]
        mine = ours(fork)
        rows = name_map(fork)
        for pskey, entries in mids.items():
            side = mine.get(pskey.lower())
            if not side:
                continue
            mids_internals = {e[0].lower() for e in entries if isinstance(e, list) and e}
            my_int = {i.lower() for i, _, _ in side}
            my_disp = {n.lower() for _, n, _ in side}
            for e in entries:
                if not isinstance(e, list) or len(e) < 3:
                    continue
                mi, md, ml = e[0], e[1], e[2]
                if mi.lower() in my_int or md.lower() in my_disp:
                    continue
                cands = [(i, n, lvl) for i, n, lvl in side
                         if i.lower() not in mids_internals
                         and lev(md.lower(), n.lower()) <= MAX_DIST and abs(lvl - ml) <= 1]
                if cands:
                    bound = rows.get(pskey.lower(), {}).get(str(mi).strip().lower())
                    found.append((fork, pskey, mi, md, ml, cands, bound))

    print(f"near-miss pairs (edit distance <= {MAX_DIST}, level within 1): {len(found)}\n")
    unbound_typos, stray_rows = [], []
    for fork, pskey, mi, md, ml, cands, bound in found:
        ladder = len(cands) == 1 and strip_separators(md) == strip_separators(cands[0][1])
        flag = "" if len(cands) == 1 else "  <-- AMBIGUOUS, more than one candidate"
        print(f"  [{fork}] {pskey}{flag}")
        print(f"      Mids {mi!r} displays {md!r} @L{ml}")
        for i, n, lvl in cands:
            print(f"      ours {i!r} displays {n!r} @L{lvl}  (distance {lev(md.lower(), n.lower())}, dL {lvl - ml})")
        if len(cands) == 1:
            where = f"[{fork}] {pskey}: {mi} / {cands[0][0]}"
            if ladder:
                print(f"      -> separator drift; the matcher's ladder reaches it, so NO row is right"
                      f" — row: {bound!r}")
                if bound:
                    stray_rows.append(f"{where} — bound to {bound!r}, but the ladder already answers it")
            else:
                print(f"      -> misspelling; nothing but a name-map row binds it — row: {bound!r}")
                if bound != cands[0][0]:
                    unbound_typos.append(f"{where} — expected a row to {cands[0][0]!r}, found {bound!r}")
    print()
    if len(found) > MAX_PAIRS:
        print(f"CLAIM BROKEN: the row was cut on {MAX_PAIRS} pairs, this run finds {len(found)}. "
              "A bounded hand-adjudication is no longer the right shape.")
        return 1
    if any(len(c) > 1 for *_, c, _ in found):
        print("CLAIM BROKEN: a pair has more than one candidate — not decodable by distance.")
        return 1
    for line in unbound_typos:
        print(f"CLAIM BROKEN: {line}")
    for line in stray_rows:
        print(f"CLAIM BROKEN: {line}")
    if unbound_typos or stray_rows:
        return 1
    n_typo = sum(1 for _f, _p, _mi, md, _ml, c, _b in found
                 if len(c) == 1 and strip_separators(md) != strip_separators(c[0][1]))
    n_ladder = len(found) - n_typo
    print(f"ok: {len(found)} near-miss pairs, each with one candidate. {n_typo} "
          f"{'is a misspelling' if n_typo == 1 else 'are misspellings'} carrying a name-map row; "
          f"{n_ladder} {'is' if n_ladder == 1 else 'are'} separator drift the matcher's ladder "
          "answers, and none of those was given a row.")
    return 0



if __name__ == "__main__":
    sys.exit(main())
