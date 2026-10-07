#!/usr/bin/env python3
"""Census for DATA-GAP-REGISTER TSPY-11: pick the recharge-buff FOLD RULE by divergence.

Run from the repo root:  python3 scripts/keys/tspy11-fold-census.py

The bug: `recharge_buff_value` folds with ``sum|scale|`` over every matching RechargeTime/Str
atom and applies no recipient test, so a power that carries the ally-facing copy AND the
caster's own copy of one buff double-counts (Conduit of Pain -> 1.0 for a +50%).

We grade three candidate folds over the whole corpus on all four forks and report exactly where
they disagree, with the atoms and the power's shortHelp so the right fold can be chosen by the
structure of the disagreement, not by the one power that exposed it:

  fold_sum        = sum(|scale|) over every matching atom      -- current Rust/naive behaviour
  fold_distinct   = sum of DISTINCT |scale| values             -- sum_distinct_abs (dedup copies)
  fold_recip      = sum over RECIPIENT groups of that group's   -- each recipient's own copy,
                    sum(|scale|)                                then the caster's own group only

`fold_recip` is the "what the caster actually receives" rule. The correct rule must read
Conduit of Pain at 0.5 and leave the 314 agreeing powers untouched.

Output: a summary of how many powers each pair of folds disagree on, then the full list of
disagreeing powers (per fork) with their atoms and shortHelp. Exit 0 always; this is a report,
not a pass/fail key.
"""
import json, glob, collections

# Positional atom layout in contract/<fork>/powersets/**/*.json (atoms are arrays).
A_TYPE, A_SCALE, A_TABLE, A_ASPECT, A_TOWHO, A_GATED, A_NOCC, A_STACK, A_DUR = 0, 2, 5, 6, 8, 10, 26, 11, 4

def g(a, i):
    return a[i] if i < len(a) else None

def buff_atoms(p):
    out = []
    for a in p.get('atoms', []):
        if not isinstance(a, list):
            continue
        if g(a, A_TYPE) != 'RechargeTime':
            continue
        if g(a, A_ASPECT) != 'Str':                     # buff face only
            continue
        if g(a, A_GATED) is True:                        # gated out
            continue
        if g(a, A_NOCC) is True:                         # converter's Thunderspy Ones guard
            continue
        scale = g(a, A_SCALE)
        if scale is None:
            continue
        table = g(a, A_TABLE) or ''
        is_debuff = (scale < 0) or ('slow' in table.lower())
        if is_debuff:                                   # crash face, not the buff
            continue
        out.append({
            'toWho': g(a, A_TOWHO), 'scale': float(scale), 'table': table,
            'stack': g(a, A_STACK), 'dur': g(a, A_DUR),
        })
    return out

def fold_sum(atoms):
    return sum(x['scale'] for x in atoms)

def fold_distinct(atoms):
    seen = set()
    total = 0.0
    for x in atoms:
        key = abs(x['scale'])
        if key in seen:
            continue
        seen.add(key)
        total += x['scale']
    return total

def fold_recip(atoms):
    # each recipient group sums its own copies; the caster receives only the Self group
    by_recip = collections.defaultdict(float)
    for x in atoms:
        by_recip[x['toWho']] += x['scale']
    # the value the totals panel shows for the caster = the caster's own received copy
    return by_recip.get('Self', 0.0)

rows = []
for f in glob.glob('contract/*/powersets/**/*.json', recursive=True):
    fork = f.split('/')[1]
    try:
        d = json.load(open(f))
    except Exception:
        continue
    for p in d.get('powers', []):
        if not isinstance(p, dict):
            continue
        atoms = buff_atoms(p)
        if len(atoms) < 2:            # single-atom powers: all folds agree by construction
            continue
        s, di, ri = fold_sum(atoms), fold_distinct(atoms), fold_recip(atoms)
        if abs(s - di) < 1e-9 and abs(s - ri) < 1e-9:
            continue                  # agrees with everything; not a divergence witness
        rows.append((fork, p.get('name'), p.get('shortHelp', ''), s, di, ri, atoms))

print(f"POWERS WITH >=2 RECHARGE-BUFF ATOMS THAT DISAGREE BETWEEN FOLDS: {len(rows)}")
print("(fold_sum = current sum|scale| ; fold_distinct = sum distinct|scale| ; "
      "fold_recip = caster's own recipient group)\n")

by_fork = collections.defaultdict(list)
for r in rows:
    by_fork[r[0]].append(r)

for fork in sorted(by_fork):
    print(f"== {fork} == {len(by_fork[fork])} powers ==")
    for _fork, name, sh, s, di, ri, atoms in sorted(by_fork[fork], key=lambda r: r[1]):
        print(f"\n  {name}")
        print(f"    shortHelp: {sh!r}")
        print(f"    fold_sum={s:.4f}  fold_distinct={di:.4f}  fold_recip={ri:.4f}")
        for a in atoms:
            print(f"      toWho={a['toWho']!s:8} scale={a['scale']:+.4f} "
                  f"table={a['table']!r:16} stack={a['stack']!s:8} dur={a['dur']}")
    print()

print("\n(Report only — no pass/fail. See the disagreement table above.)")
