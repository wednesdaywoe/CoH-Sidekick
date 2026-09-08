import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import { calculateProcChance } from '@/data/proc-data';
import { getPseudoPetAoEGeometry, resolveProcAreaGeometry } from './pet-damage';

/**
 * Proc area-factor geometry for summon-shell AoEs.
 *
 * Powers whose area of effect lives on a summoned pseudo-pet / ground patch
 * carry radius 0 on the parent power, so the PPM proc area-factor used to score
 * them single-target — reporting a proc chance far too high:
 *   • Burn — Self/SingleTarget shell, AoE on an inline `resolvedEntities`
 *     pseudo-pet (PL_StaticObject, radius 15).
 *   • Rain of Fire — Location parent, AoE on a real PET_ENTITIES patch
 *     (Pets_*_RainofFire, radius 25).
 * resolveProcAreaGeometry borrows the pet footprint in both cases.
 */
describe('proc area-factor geometry (summon-shell AoEs)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  const findPower = (psId: string, name: string) =>
    getPowerset(psId)?.powers.find((p) => p.name === name);

  it('Burn borrows its inline pseudo-pet radius (resolvedEntities)', () => {
    const burn = findPower('blaster/fire-manipulation', 'Burn');
    expect(burn).toBeTruthy();
    const geom = getPseudoPetAoEGeometry(burn!.summon);
    expect(geom).toEqual({ radius: 15, arcDegrees: 360 });
  });

  it('Rain of Fire borrows its PET_ENTITIES patch radius', () => {
    const rof = findPower('blaster/fire-blast', 'Rain of Fire');
    expect(rof).toBeTruthy();
    // Parent is Location with no radius — geometry comes from the real pet.
    const direct = rof!.stats?.radius ?? rof!.effects?.radius ?? 0;
    expect(direct).toBe(0);
    const geom = resolveProcAreaGeometry(direct, undefined, rof!.summon);
    expect(geom.radius).toBe(25);
  });

  it('applies the AoE area factor instead of a single-target proc chance', () => {
    // 3.5 PPM, 16s recharge, 1s cast. Single-target would clamp to 90%; the
    // 25 ft sphere area factor pulls it well below that.
    const st = calculateProcChance(3.5, 16, 1, 0, 360);
    const aoe = calculateProcChance(3.5, 16, 1, 25, 360);
    expect(st).toBeGreaterThan(aoe);
    expect(aoe).toBeLessThan(0.55);
  });

  it('leaves single-target powers untouched (no summon)', () => {
    expect(resolveProcAreaGeometry(0, undefined, undefined)).toEqual({
      radius: 0,
      arcDegrees: 360,
    });
  });

  it('prefers the parent power radius when it has its own AoE (Fire Ball)', () => {
    const fb = findPower('blaster/fire-blast', 'Fire Ball');
    expect(fb).toBeTruthy();
    const direct = fb!.stats?.radius ?? fb!.effects?.radius ?? 0;
    expect(direct).toBeGreaterThan(0);
    const geom = resolveProcAreaGeometry(direct, 360, fb!.summon);
    expect(geom.radius).toBe(direct);
  });
});
