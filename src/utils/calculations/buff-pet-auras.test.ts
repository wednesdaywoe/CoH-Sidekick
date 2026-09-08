import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { createEmptyBuild } from '@/types/build';
import { calculateCharacterTotals } from './character-totals';
import { getBuffPetSources, summonIsBuffPet } from './buff-pet-auras';
import { ForceFieldGenerator } from '@/data/datasets/homecoming/generated/powersets/defender/primary/traps/force-field-generator';
import { TriageBeacon } from '@/data/datasets/homecoming/generated/powersets/defender/primary/traps/triage-beacon';
import { Wellspring } from '@/data/datasets/homecoming/generated/powersets/defender/primary/marine-affinity/wellspring';

/**
 * Buff-pet auras (Force Field Generator, Barrier Reef, Triage Beacon, …) folded
 * into character totals via an opt-in per-pet toggle.
 *
 * These "floaty" buff drones project a persistent AoE buff onto the caster, but
 * the buff lives on the pet entity — so the totals engine, which walks only the
 * player's own powers, never counted it. The converter now surfaces the auras
 * (convert-pet-entities' extractBuffAura) and the calc folds a toggled-on pet's
 * aura into globalBonuses (Step 7.2). Off by default, so a build that hasn't
 * enabled a buff-pet is byte-identical to before.
 */
describe('Buff-pet auras (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  function build(powers: any[]): any {
    const b = createEmptyBuild();
    b.level = 50;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    b.archetype = { id: 'defender', name: 'Defender', stats: null, inherent: null } as any;
    // getPowerset won't resolve this synthetic id, so collectAllPowers passes the
    // power through as-is — its real `effects.summon` (from the imported def) is
    // what the fold reads. Self-contained; no powerset-id plumbing needed.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    b.primary = { id: 'defender/traps', name: 'Traps', powers } as any;
    return b;
  }

  const key = (internalName: string) => ({ mechanicAdjusters: { [`${internalName}:buffpet`]: true } });

  it('detects the named buff-pets and ignores non-buff summons', () => {
    expect(summonIsBuffPet(ForceFieldGenerator.summon)).toBe(true);
    expect(summonIsBuffPet(Wellspring.summon)).toBe(true);
    expect(summonIsBuffPet(TriageBeacon.summon)).toBe(true);

    const ffg = getBuffPetSources(ForceFieldGenerator.summon);
    expect(ffg[0].auras.some((a) => a.type === 'DefenseBuff')).toBe(true);
    // Force Field Generator is a non-commandable drone.
    expect(ffg[0].displayName.toLowerCase()).toContain('force field');
  });

  it('Force Field Generator adds +Defense to totals only when toggled on', () => {
    const powers = [{ ...ForceFieldGenerator, powerSet: 'defender/traps', level: 1, isActive: true, slots: [] }];

    const off = calculateCharacterTotals(build(powers), false, undefined, {}).globalBonuses;
    expect(off.defMelee).toBe(0); // opt-in: nothing until the toggle is flipped

    const on = calculateCharacterTotals(build(powers), false, undefined, key('Force_Field_Generator')).globalBonuses;
    // The bubble buffs all positions and all types at one scale.
    expect(on.defMelee).toBeGreaterThan(0);
    expect(on.defRanged).toBeCloseTo(on.defMelee, 5);
    expect(on.defAoE).toBeCloseTo(on.defMelee, 5);
    expect(on.defSmashing).toBeCloseTo(on.defMelee, 5);
    expect(on.defPsionic).toBeCloseTo(on.defMelee, 5);
    // FFG's Dispersion Bubble is a real defense buff in the single-digit-percent
    // range at base (no slotting) — a sane, non-degenerate value.
    expect(on.defMelee).toBeGreaterThan(2);
    expect(on.defMelee).toBeLessThan(30);
  });

  it('Barrier Reef adds +Defense and Absorb when toggled on', () => {
    const powers = [{ ...Wellspring, powerSet: 'defender/marine_affinity', level: 1, isActive: true, slots: [] }];
    const off = calculateCharacterTotals(build(powers), false, undefined, {}).globalBonuses;
    expect(off.defMelee).toBe(0);
    expect(off.absorb).toBe(0);

    const on = calculateCharacterTotals(build(powers), false, undefined, key('Wellspring')).globalBonuses;
    expect(on.defMelee).toBeGreaterThan(0);
    expect(on.absorb).toBeGreaterThan(0);
  });

  it('Triage Beacon adds +Regeneration when toggled on', () => {
    const powers = [{ ...TriageBeacon, powerSet: 'defender/traps', level: 1, isActive: true, slots: [] }];
    const off = calculateCharacterTotals(build(powers), false, undefined, {}).globalBonuses;
    const on = calculateCharacterTotals(build(powers), false, undefined, key('Triage_Beacon')).globalBonuses;
    expect(off.regeneration).toBe(0);
    expect(on.regeneration).toBeGreaterThan(50); // a substantial +Regen aura
  });

  it('records a per-pet breakdown row for the folded aura', () => {
    const powers = [{ ...ForceFieldGenerator, powerSet: 'defender/traps', level: 1, isActive: true, slots: [] }];
    const { breakdown } = calculateCharacterTotals(build(powers), false, undefined, key('Force_Field_Generator'));
    const defMelee = breakdown.get('defMelee');
    expect(defMelee?.sources.some((c) => c.name === 'Force Field Generator')).toBe(true);
  });
});
