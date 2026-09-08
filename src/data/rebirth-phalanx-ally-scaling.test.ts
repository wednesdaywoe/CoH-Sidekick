import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

/**
 * Rebirth Phalanx Fighting — per-ally defense scaling must survive the PvE filter.
 *
 * Rebirth (Parse6) has no explicit `is_pvp` flag, so the parser synthesizes it
 * from each AttribMod's RPN `requires`. Phalanx's per-ally +Def increment is a
 * Self-targeted buff whose requires counts nearby OTHER players via the
 * self-exclusion clause `entref target> entref source> eq ! enttype target>
 * player eq &&`. The old heuristic saw the `player eq` substring and tagged it
 * PVP_ONLY, so convert-powerset dropped it from the PvE planner — Rebirth
 * Phalanx generated a flat +Def with NO ally scaling, contradicting its own
 * description ("this bonus grows for each ally near you") and HC's EITHER flag.
 *
 * Fix: a Self-targeted AttribMod with the self-exclusion clause is a proximity/
 * ally-counting self-buff, not a PvE/PvP combat split → classify EITHER. This
 * guards that the ally scaling (`perTarget`) is present in generated.
 */
const RB = fileURLToPath(new URL('./datasets/rebirth/generated/powersets', import.meta.url));

const PHALANX = [
  ['Brute', 'brute/secondary/shield-defense/phalanx-fighting.ts'],
  ['Scrapper', 'scrapper/secondary/shield-defense/phalanx-fighting.ts'],
  ['Tanker', 'tanker/primary/shield-defense/phalanx-fighting.ts'],
] as const;

describe('Rebirth Phalanx Fighting per-ally defense scaling (is_pvp ally pattern)', () => {
  // The per-ally increment no longer reaches the generated file as a bag key
  // (`"perTarget"` was `effects` slot, stripped 2026-08 — STRIP-1). It now lives as
  // the atom stream's own `per_target` stamp, the wire field at tuple index 24
  // (atom-reference §2). So the guard scans the emitted ATOMS for the increment
  // rather than the retired bag shape, keeping the claim: the PvE classifier must
  // not drop the ally-counting Self-buff.
  it.each(PHALANX)('%s Phalanx defenseBuff carries perTarget ally scaling', (_at, file) => {
    const text = fs.readFileSync(`${RB}/${file}`, 'utf8');
    // The per-ally increments are Defense atoms with scale 0.3 (slot 2) whose per-foe
    // stamp (slot 24) is also 0.3, and they are gated (the `player eq` self-exclusion is
    // a live gate, slot 23 = true). Match the whole atom row: scale 0.3, then the
    // `true` gate, then per_target 0.3 at the tail. This is the increment row, distinct
    // from the always-on 0.5 base rows (which are ungated, per_target null).
    const perTargetRows = text.matchAll(/"Defense","(Melee|Ranged|AoE)",0\.3,.*?,true,0\.3,/g);
    const vectors = new Set([...perTargetRows].map((m) => m[1]));
    // Every vector (melee, ranged, area) of the ally increment must carry the per-foe
    // stamp — scale 0.3, gated true, per_target 0.3 in the same row. Seeing all three
    // keeps the claim from going green by the power losing its scaling.
    expect(vectors.size).toBe(3); // melee, ranged, area
  });
});
