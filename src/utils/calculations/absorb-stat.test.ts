import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { createEmptyBuild } from '@/types/build';
import { calculateCharacterTotals } from './character-totals';
import { getBaselineHealth } from './stats';
import { absorbMaxHPFractionValue } from '@/data/core/atom-query';
import { WildBastion } from '@/data/datasets/homecoming/powersets/corruptor/secondary/nature-affinity/wild-bastion';

/**
 * Absorb as a character-total stat (Survival & Mobility).
 *
 * Absorb was modeled per-power but never aggregated into a build total. Two
 * magnitude forms now flow into `globalBonuses.absorb` (absolute HP):
 *   • MaxHP-fraction — Wild Bastion's `Max.kHitPoints source> 0.25 * @Strength *`
 *     Expression, recovered by the converter as `absorb.maxHPFraction = 0.25`
 *     and resolved to HP against the build's final Max HP (so +HP accolades and
 *     +Absorb strength grow it, matching the game).
 *   • Flat HP — Psychokinetic Barrier's `scale × Melee_HealSelf`, a base-HP
 *     shield that does NOT scale with current Max HP.
 * The genuinely-conditional Expression absorbs (Master Brawler's missing-HP
 * formula, @StdResult chains) are intentionally deferred (duration-only).
 */
describe('Absorb character total (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function build(atId: string, atName: string, powersetId: string, powersetName: string, powers: any[]): any {
    const b = createEmptyBuild();
    b.level = 50;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    b.archetype = { id: atId, name: atName, stats: null, inherent: null } as any;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    b.secondary = { id: powersetId, name: powersetName, powers } as any;
    return b;
  }

  it('recovers Wild Bastion as a 25%-of-MaxHP absorb (atom-native)', () => {
    // The bag's `effects.absorb.maxHPFraction` is gone with the strip (STRIP-1); the
    // fraction now lives on the Expression atom, evaluated by the atom-native reader
    // that mirrors Rust's `absorb_max_hp_fraction_value` (ATOM10).
    expect(absorbMaxHPFractionValue(WildBastion as never)).toBeCloseTo(0.25, 5);
  });

  it('Wild Bastion active contributes 25% of the build Max HP as absorb', () => {
    const b = build('corruptor', 'Corruptor', 'corruptor/nature-affinity', 'Nature Affinity', [
      { internalName: 'Wild_Bastion', name: 'Wild Bastion', powerSet: 'corruptor/nature-affinity', level: 1, isActive: true, slots: [] },
    ]);
    const { globalBonuses } = calculateCharacterTotals(b, false, undefined, {});
    // No slotting / no +MaxHP buffs → actualHP == base HP, strength mult == 1.
    const baseHP = getBaselineHealth('corruptor', 50).baseHealth;
    expect(globalBonuses.absorb).toBeCloseTo(0.25 * baseHP, 1);
  });

  it('is zero when the absorb power is inactive', () => {
    const b = build('corruptor', 'Corruptor', 'corruptor/nature-affinity', 'Nature Affinity', [
      { internalName: 'Wild_Bastion', name: 'Wild Bastion', powerSet: 'corruptor/nature-affinity', level: 1, isActive: false, slots: [] },
    ]);
    expect(calculateCharacterTotals(b, false, undefined, {}).globalBonuses.absorb).toBe(0);
  });

  it('Psychokinetic Barrier contributes a flat HP absorb (does not scale with Max HP)', () => {
    // Brute secondary Psionic Armor. Fortify_Mind = Psychokinetic Barrier.
    const b = build('brute', 'Brute', 'brute/psionic-armor', 'Psionic Armor', [
      { internalName: 'Fortify_Mind', name: 'Psychokinetic Barrier', powerSet: 'brute/psionic-armor', level: 1, isActive: true, slots: [] },
    ]);
    const absorb = calculateCharacterTotals(b, false, undefined, {}).globalBonuses.absorb;
    // scale 3 × Melee_HealSelf (~baseHP/10) ≈ 30% of base HP, as absolute HP.
    expect(absorb).toBeGreaterThan(0);
    const baseHP = getBaselineHealth('brute', 50).baseHealth;
    expect(absorb).toBeCloseTo(0.3 * baseHP, 0);
  });
});
