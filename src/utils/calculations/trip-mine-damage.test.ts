import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPetEntity } from '@/data/pet-entities';
import { getPowerset } from '@/data';
import { calculatePetDamage, calculateResolvedPseudoPetDamage } from './pet-damage';
import { dotTickCount } from './damage';
import type { Power } from '@/types';

/* eslint-disable @typescript-eslint/no-explicit-any */

/**
 * Trip Mine damage (report 2026-07-26: "on Defender/Blaster the damage is
 * incorrectly very high"; "Dominator Trip Mine has no damage currently").
 *
 * A Trip Mine is a summon: the damage lives on the pet, and the InfoPanel turns
 * it into a per-cast number as `damagePerHit × firesPerSpawn`. Three separate
 * defects met there:
 *
 *  1. `firesPerSpawn` divided the 260s summon window by the attack's cycle time,
 *     so a mine that detonates ONCE read as 13 detonations. Only the
 *     Controller/Corruptor/Mastermind mine escaped — its shared entity carries a
 *     1000s attack recharge, which rounds the same formula down to 1 by accident.
 *     Fixed by `PetEntity.oneShot`, stamped by the converter from the pet's
 *     bundled immediate Self_Destruct.
 *  2. The converter dropped the effect group's `chance`, so the mines' 50%-chance
 *     third Fire template was summed at full value (~+14%).
 *  3. The Dominator's mine redirects only to `TripMine_Resistance` and
 *     `TripMine_Info` — the real `TripMine` is executed by the pet's
 *     Self_Destruct and never redirected — and the pseudo-pet resolver dropped
 *     `*_Info` as tooltip-only, leaving nothing to resolve.
 *
 * Expected values are level 50, unslotted, `minion_pets` melee_damage = 55.66
 * (the Dominator's resolved pseudo-pet reads the SUMMONER's table instead).
 */
describe('Trip Mine damage (homecoming)', () => {
  beforeAll(async () => { await loadDataset('homecoming'); });

  const SUMMON_WINDOW = 260;

  // Every AT's mine is a one-shot: it is destroyed by its own detonation.
  it.each([
    ['Pets_Mine', 'blaster'],
    ['Pets_Traps_Mine_Defender', 'defender'],
    ['Pets_Traps_Mine', 'controller/corruptor/mastermind'],
  ])('%s is marked oneShot (%s)', (entityName) => {
    expect(getPetEntity(entityName)?.oneShot).toBe(true);
  });

  it('a mine fires once, not once per attack-recharge over the summon window', () => {
    // Guards the bug's actual mechanism: the naive formula must still disagree,
    // else this test would pass for the wrong reason on any future data change.
    const r = calculatePetDamage('Pets_Mine', 50, 1, SUMMON_WINDOW, 0, false, 0, [])!;
    const naiveFires = dotTickCount(SUMMON_WINDOW, r.abilities[0].cycleTime);
    expect(naiveFires).toBeGreaterThan(1);
    expect(r.oneShot).toBe(true);
  });

  it('weights the 50%-chance third Fire template instead of summing it whole', () => {
    const defender = getPetEntity('Pets_Traps_Mine_Defender')!;
    const dmg = defender.abilities[0].damage;
    expect(dmg.map((d) => d.chance)).toEqual([undefined, undefined, 0.5]);
    // 1.3 + 0.65 + 0.5×0.65 = 2.275 scale, not 2.6.
    const r = calculatePetDamage('Pets_Traps_Mine_Defender', 50, 1, SUMMON_WINDOW, 0, false, 0, [])!;
    expect(r.abilities[0].damagePerHit).toBeCloseTo(2.275 * 55.66, 0);
  });

  it('per-detonation damage matches the scales, per AT', () => {
    const perHit = (e: string) =>
      calculatePetDamage(e, 50, 1, SUMMON_WINDOW, 0, false, 0, [])!.abilities[0].damagePerHit;
    // Blaster 2 + 1 + 0.5×1 = 3.5; was shown as 13 × 222.6 = 2894.
    expect(perHit('Pets_Mine')).toBeCloseTo(3.5 * 55.66, 0);
    // Defender 1.3 + 0.65 + 0.5×0.65 = 2.275; was shown as 13 × 144.7 = 1881.
    expect(perHit('Pets_Traps_Mine_Defender')).toBeCloseTo(2.275 * 55.66, 0);
    // The shared mine was already right — it must not move.
    expect(perHit('Pets_Traps_Mine')).toBeCloseTo(3 * 55.66, 0);
  });

  it("the Dominator's mine resolves damage from its Info redirect", () => {
    const power = getPowerset('dominator/arsenal-assault')?.powers
      .find((p: Power) => p.internalName === 'Trip_Mine') as Power | undefined;
    // The writer lifts `summon` out of the bag to the top level (STRIP-1/BPORT7); the bag
    // slot it used to sit in is gone.
    const resolved = (power?.summon as any)?.resolvedEntities?.[0];
    expect(resolved, 'Dominator Trip Mine resolvedEntities').toBeTruthy();

    const r = calculateResolvedPseudoPetDamage(resolved, 'dominator', 50, 0, false, 0, false)!;
    expect(r).toBeTruthy();
    const dmg = r.abilities[0].damagePerHit;
    expect(dmg).toBeGreaterThan(0);
    // 1.0954 × dominator melee_damage(50) = 58.39. The PvP twin (0.9077 ×
    // Melee_PvPDamage) must NOT also be counted.
    expect(dmg).toBeCloseTo(1.0954 * 58.39, 0);
  });
});
