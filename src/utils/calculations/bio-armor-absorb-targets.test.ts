import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import { createEmptyBuild } from '@/types/build';
import { calculateCharacterTotals } from './character-totals';
import { getBaselineHealth } from './stats';
import { absorbValue, absorbMaxHPFractionValue, atomsOf } from '@/data/core/atom-query';
import type { Power } from '@/types';

/**
 * Bio Armor absorb + targets-hit regression suite (2026-07-17 bug reports).
 *
 *   #1a Ablative Carapace absorb was 100% of MaxHP — a stale hand-written
 *       override pinned `absorb:{scale:1}` (Melee_Ones ⇒ scale is a MaxHP
 *       fraction) over the correct 30%. Override retired; generated recovers
 *       `maxHPFraction:0.3` from the live bin's Expression (incl. Sentinel's
 *       `@StdResult` form). Correct across every AT.
 *   #1b Parasitic Aura absorb was 110% of MaxHP/foe — the converter summed the
 *       Current/Magnitude grant (0.1) and its Maximum/Expression cap-twin's 1.0
 *       PLACEHOLDER scale. Deduped to the real 0.1 MaxHP/foe fraction.
 *   #4  A per-target power's untouched slider shows "Off" (0) but the calc used
 *       its N=1 value. `undefined` now reads as 0, matching the display.
 *   #5  Parasitic absorb ignored the targets-hit slider entirely (never ran
 *       through adjustForStacking); it now scales 0 → 10%/foe up to 100% @ 10.
 *   #2  Offensive Adaptation's -7.5% self -Res applied flat; CoH reduces it by
 *       the caster's own same-type resistance (effective = 7.5 × (1 − R)).
 */

const PARASITIC_ATS: ReadonlyArray<readonly [string, string]> = [
  ['scrapper/bio-armor', 'Scrapper'],
  ['brute/bio-armor', 'Brute'],
  ['tanker/bio-armor', 'Tanker'],
  ['stalker/bio-armor', 'Stalker'],
];

const ABLATIVE_ATS = [...PARASITIC_ATS, ['sentinel/bio-armor', 'Sentinel'] as const];

/** The ATs whose Parasitic Aura carries ONLY the Max-face Expression, so the per-foe increment
 *  has no atom to read — the ABSORB-4 residual's population on this power, pinned by name. */
const EXPRESSION_ONLY_PARASITIC: ReadonlySet<string> = new Set(['brute/bio-armor']);

/** The per-foe stamp on a power's Current-face absorb GRANT, or `undefined` when it authors
 *  only the Max-face ceiling. Read off the atom rather than through `absorbValue`, which folds
 *  the group to a scale/table and does not carry the stamp for this shape. */
function perFoeAbsorbStamp(pw: Power): number | undefined {
  for (const a of atomsOf(pw)) {
    if (a.effectType === 'Absorb' && a.aspect === 'Cur' && a.perTarget != null) return a.perTarget;
  }
  return undefined;
}

/** The Max-face Expression program behind a power's absorb ceiling, or `undefined`. */
function absorbCeilingProgram(pw: Power): string[] | undefined {
  for (const a of atomsOf(pw)) {
    if (a.effectType === 'Absorb' && a.aspect === 'Max' && a.attribType === 'Expression') {
      return a.magnitudeExpression as unknown as string[] | undefined;
    }
  }
  return undefined;
}

function power(setId: string, internalName: string): Power | undefined {
  return getPowerset(setId)?.powers.find((p) => p.internalName === internalName);
}

describe('Bio Armor absorb + targets-hit fixes (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  // --- #1a Ablative Carapace ---------------------------------------------
  describe('Ablative Carapace absorb = 30% of MaxHP (not 100%)', () => {
    // Read off the ATOMS: `effects.absorb` went with the writer-side strip (BPORT7), and the
    // fraction's real home was always the Max-face Expression the bag was projecting.
    // `absorbMaxHPFractionValue` evaluates that program — the same reader `absorb-stat` spends
    // and the mirror of Rust's `absorb_max_hp_fraction_value`.
    it.each(ABLATIVE_ATS)('%s recovers maxHPFraction 0.3, no stale bare scale', (setId) => {
      const pw = power(setId, 'Ablative_Carapace');
      expect(pw).toBeDefined();
      expect(absorbMaxHPFractionValue(pw!)).toBeCloseTo(0.3, 5);
      // `appliesStrength` was the bag's word for the strength term in that program; assert the
      // term itself. The export spells it two ways and both mean the caster's strength applies:
      // `@Strength` as an explicit multiplier with the fraction as a program literal, and
      // Sentinel's `@StdResult`, which IS the strength-applied standard result with the fraction
      // on the atom's own `scale`. Accepting either is the read; requiring one would have made
      // the Sentinel row a false red, which is how this assertion first failed.
      const ceiling = absorbCeilingProgram(pw!);
      expect(ceiling, `${setId} Ablative_Carapace ceiling program`).toBeDefined();
      expect(
        ceiling!.some((t) => t === '@Strength' || t === '@StdResult'),
        `${setId} ceiling applies strength (${ceiling!.join(' ')})`,
      ).toBe(true);
      // The stale override pinned a bare scale:1 (= 100% MaxHP). There is no bare-scale absorb
      // row at all on this power, which is the atom-side spelling of the same claim.
      expect(absorbValue(pw!)).toBeUndefined();
    });
  });

  // --- #1b Parasitic Aura absorb value -----------------------------------
  describe('Parasitic Aura absorb = 10% of MaxHP per foe (not 110%)', () => {
    // The 10%-per-foe fraction is authored in TWO equivalent spellings across the ATs: a
    // `_ones` `scale`/`perTarget` (Scrapper/Tanker/Stalker) and an explicit
    // `maxHPFraction`/`maxHPFractionPerTarget` (Brute, whose export carries the RPN form the
    // converter recovers). The calc resolves both identically — measured: all four ATs credit
    // exactly 0.10 × base HP per foe — so this asserts the FRACTION, not the encoding, and
    // still pins the defect it was written for (the summed cap-twin placeholder, 1.1).
    it.each(PARASITIC_ATS)('%s base absorb is 10%% of Max HP per foe', (setId) => {
      const pw = power(setId, 'Parasitic_Aura');
      expect(pw).toBeDefined();
      // The FRACTION is uniform across the ATs on the atoms — a stronger statement than the bag
      // could make, since the two spellings the old comment describes were a bag artefact.
      const fraction = absorbMaxHPFractionValue(pw!);
      expect(fraction).toBeCloseTo(0.1, 5);
      // Never the summed cap-twin placeholder (1.1).
      expect(fraction!).toBeLessThan(1);

      // The PER-FOE increment is a different matter, and the difference is measured rather
      // than smoothed over. Three of the four ATs author the grant as a Current/Magnitude atom
      // that carries `perTarget`; Brute authors ONLY the Max-face Expression, whose per-foe half
      // has no atom source — the ABSORB-4 carried residual ("the MaxHP-FRACTION half has no atom
      // source at all"), which the bag's `maxHPFractionPerTarget` used to paper over. Stated as
      // a partition so neither side can drift silently: an AT leaving the Expression-only set
      // reds here, and so does one joining it.
      const perTarget = perFoeAbsorbStamp(pw!);
      if (EXPRESSION_ONLY_PARASITIC.has(setId)) {
        expect(perTarget, `${setId}: gained a per-foe atom — ABSORB-4's residual may be closed`).toBeUndefined();
      } else {
        expect(perTarget, `${setId}: lost its per-foe atom`).toBeCloseTo(0.1, 5);
      }
    });
  });

  // --- #4 + #5 Parasitic absorb per-target scaling -----------------------
  describe('Parasitic Aura absorb scales with the targets-hit slider', () => {
    function absorbAt(targetsHit?: number): number {
      const b = createEmptyBuild();
      b.level = 50;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      b.archetype = { id: 'scrapper', name: 'Scrapper', stats: null, inherent: null } as any;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      b.secondary = {
        id: 'scrapper/bio-armor',
        name: 'Bio Armor',
        powers: [{ internalName: 'Parasitic_Aura', name: 'Parasitic Aura', powerSet: 'scrapper/bio-armor', level: 1, isActive: true, slots: [] }],
      } as any;
      const targetsHitValues: Record<string, number> = targetsHit === undefined ? {} : { Parasitic_Aura: targetsHit };
      return calculateCharacterTotals(b, false, undefined, { targetsHitValues }).globalBonuses.absorb;
    }

    it('#4 an untouched slider ("Off") contributes 0 absorb, not the 1-target value', () => {
      expect(absorbAt(undefined)).toBe(0);
      expect(absorbAt(0)).toBe(0);
    });

    it('#5 grows per foe hit: 1 foe = 10% MaxHP, 10 foes = 100% MaxHP', () => {
      const baseHP = getBaselineHealth('scrapper', 50).baseHealth;
      expect(absorbAt(1)).toBeCloseTo(0.1 * baseHP, 0);
      expect(absorbAt(10)).toBeCloseTo(1.0 * baseHP, 0);
      // Strictly monotonic in targets hit.
      expect(absorbAt(10)).toBeGreaterThan(absorbAt(5));
      expect(absorbAt(5)).toBeGreaterThan(absorbAt(1));
    });
  });

  // --- #2 Offensive Adaptation self -Res mitigation ----------------------
  describe('Offensive Adaptation -7.5% self -Res is resisted by same-type resistance', () => {
    function resWith(activeSubPower?: string) {
      const b = createEmptyBuild();
      b.level = 50;
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      b.archetype = { id: 'tanker', name: 'Tanker', stats: null, inherent: null } as any;
      const powers = [
        // Hardened Carapace grants +25% Smashing/Toxic resistance (0% Fire).
        { internalName: 'Hardened_Carapace', name: 'Hardened Carapace', powerSet: 'tanker/bio-armor', level: 1, isActive: true, slots: [] },
        { internalName: 'Adaptation', name: 'Evolving Armor', powerSet: 'tanker/bio-armor', level: 1, isActive: false, slots: [], activeSubPower },
      ];
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      b.primary = { id: 'tanker/bio-armor', name: 'Bio Armor', powers } as any;
      return calculateCharacterTotals(b, false, undefined, {}).globalBonuses;
    }

    it('a resisted type loses less than the flat 7.5 (25% → 19.375%, not 17.5%)', () => {
      const off = resWith(undefined);
      const on = resWith('Offensive_Adaptation');
      // Baseline 25% Smashing from Hardened Carapace.
      expect(off.resSmashing).toBeCloseTo(25, 3);
      // effective = 7.5 × (1 − 0.25) = 5.625 → 25 − 5.625 = 19.375 (NOT 17.5).
      expect(on.resSmashing).toBeCloseTo(19.375, 2);
      const drop = off.resSmashing - on.resSmashing;
      expect(drop).toBeCloseTo(5.625, 2);
      expect(drop).toBeLessThan(7.5); // mitigated, not flat
    });

    it('an unresisted type takes the full 7.5 (Fire 0% → -7.5%)', () => {
      const on = resWith('Offensive_Adaptation');
      // No Fire resistance ⇒ no mitigation ⇒ full nominal penalty.
      expect(on.resFire).toBeCloseTo(-7.5, 2);
    });
  });
});
