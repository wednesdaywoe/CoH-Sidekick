#!/usr/bin/env python3
"""Key for DATA-GAP-REGISTER's Kheldian-shapeshift residual (host COND-11).

Run from the repo root:  python3 scripts/keys/shapeshift-suppression.py

The claim closes a door: "nothing is GRANTED through this channel, so no wrong number ships
today" is what makes the residual low priority rather than a live defect. It is checked here
against the export's own tagged EffectGroups, which is the artifact COND-11 censused.

Exit 0 = claim holds (every ShapeshiftActive group is chance 0, i.e. suppression only).
Exit 1 = a ShapeshiftActive group carries payload at a non-zero chance, which would mean the
channel GRANTS something and the residual is a live wrong number, not a dormant one.
"""
import json, glob, os, sys

ROOT = 'exported_powers'
act, deact, powers, live = 0, 0, set(), []
for fork in sorted(d for d in os.listdir(ROOT) if os.path.isdir(os.path.join(ROOT, d))):
    for f in glob.glob(f'{ROOT}/{fork}/**/*.json', recursive=True):
        try:
            txt = open(f, encoding='utf-8').read()
        except Exception:
            continue
        if 'Shapeshift' not in txt:
            continue
        try:
            d = json.loads(txt)
        except Exception:
            continue
        for g in d.get('effects', []) or []:
            tags = g.get('tags') or []
            if 'ShapeshiftActive' in tags:
                act += 1
                powers.add((fork, os.path.basename(f)))
                if g.get('chance'):
                    live.append((fork, os.path.basename(f), g.get('chance'),
                                 len(g.get('templates') or [])))
            elif 'ShapeshiftDeactive' in tags:
                deact += 1
                powers.add((fork, os.path.basename(f)))

print(f"ShapeshiftActive groups:   {act}")
print(f"ShapeshiftDeactive groups: {deact}")
print(f"powers carrying either:    {len(powers)}")
print(f"ShapeshiftActive groups at a NON-ZERO chance: {len(live)}")
for r in live[:10]:
    print("   ", r)
print()
if live:
    print("CLAIM BROKEN: the channel grants payload — this residual ships a wrong number today.")
    sys.exit(1)
print("CLAIM HOLDS: every ShapeshiftActive group is chance 0 — suppression only, nothing granted.")
