#!/usr/bin/env python3
"""Key for DATA-GAP-REGISTER TSPY-11: "a `reaches_caster` filter is not the fix".

Run from the repo root:  python3 scripts/keys/tspy11-filter-cost.py

The claim closes a door — it tells the next session not to try the obvious filter — so it is
checkable rather than believed. It mirrors `recharge_buff_value`'s population
(RechargeTime + aspect Str + not gated + not notOnCaster) and `reaches_caster`'s rule
(a `Target` atom reaches the caster when the power's targetsAffected names Self and no gate
excludes him; a `Self` atom always does).

Exit 0 = claim holds. Exit 1 = a named power would SURVIVE the filter, or carries no such row
at all, and the register's sentence needs re-measuring.
"""
import json, glob, sys

NAMED = ["Speed Boost", "Adrenalin Boost", "Amp Up", "Mutation", "Resurrect",
         "Temporal Selection", "Enforced Morale", "Shifting Tides", "Time Bomb"]
I_TYPE, I_ASPECT, I_TOWHO, I_GATED, I_NOC = 0, 6, 8, 23, 26

def get(a, i):
    return a[i] if i < len(a) else None

rows = {}
for f in glob.glob('contract/*/powersets/**/*.json', recursive=True):
    fork = f.split('/')[1]
    try:
        d = json.load(open(f))
    except Exception:
        continue
    for p in d.get('powers', []):
        if p.get('name') not in NAMED:
            continue
        pop = [a for a in p.get('atoms', [])
               if get(a, I_TYPE) == 'RechargeTime' and get(a, I_ASPECT) == 'Str'
               and get(a, I_GATED) is not True and get(a, I_NOC) is not True]
        if not pop:
            continue
        self_named = 'Self' in (p.get('targetsAffected') or [])
        kept = [a for a in pop if get(a, I_TOWHO) in ('Self', 'SelfOnly') or self_named]
        rows.setdefault(p['name'], set()).add((fork, self_named, len(pop), len(kept)))

broken = []
for n in NAMED:
    if n not in rows:
        broken.append(f"{n}: carries NO recharge-buff row on any fork — the filter cannot zero "
                      f"what is already absent, so naming it here proves nothing")
        continue
    for fork, self_named, npop, nkept in sorted(rows[n]):
        mark = "SURVIVES" if nkept else "zeroed"
        print(f"  {n:20} {fork:11} rows={npop} Self-in-targetsAffected={str(self_named):5} -> {mark}")
        if nkept:
            broken.append(f"{n}/{fork}: targetsAffected names Self and the row carries no gate, "
                          f"so reaches_caster KEEPS it — the filter does not zero this one")

print()
if broken:
    print("CLAIM BROKEN — the register's sentence is not true as written:")
    for b in broken:
        print("  -", b)
    sys.exit(1)
print("CLAIM HOLDS: every named power is zeroed by a reaches_caster filter.")
