import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { calculateResolvedPseudoPetDamage, calculatePetDamage } from './pet-damage';
import { getPetEntity } from '@/data/pet-entities';
import { StormCell } from '@/data/datasets/homecoming/generated/powersets/blaster/primary/storm-blast/storm-cell';
import { CategoryFive } from '@/data/datasets/homecoming/generated/powersets/blaster/primary/storm-blast/category-five';
import { TarPatch } from '@/data/datasets/homecoming/generated/powersets/defender/primary/dark-miasma/tar-patch';
import { Meteor } from '@/data/datasets/homecoming/generated/powersets/blaster/primary/seismic-blast/meteor';
import { Sleet } from '@/data/datasets/homecoming/generated/powersets/defender/primary/cold-domination/sleet';
import { OilSlickArrow } from '@/data/datasets/homecoming/generated/powersets/controller/secondary/trick-arrow/oil-slick-arrow';
import { Burn as BlasterBurn } from '@/data/datasets/homecoming/generated/powersets/blaster/secondary/fire-manipulation/burn';
import { Burn as TankerBurn } from '@/data/datasets/homecoming/generated/powersets/tanker/primary/fiery-aura/burn';
import { Fog as FreezingRain } from '@/data/datasets/homecoming/generated/powersets/defender/primary/storm-summoning/fog';
import { VoltaicSentinel } from '@/data/datasets/homecoming/generated/powersets/blaster/primary/electrical-blast/voltaic-sentinel';
import { RainofFire as SentinelRainOfFire } from '@/data/datasets/homecoming/generated/powersets/sentinel/primary/fire-blast/rain-of-fire';
import type { ResolvedPseudoPet } from '@/types/power';

/**
 * Pseudo-pet redirect resolution (Storm Cell / Category Five).
 *
 * These location powers deliver all damage + debuffs through a pseudo-pet that
 * runs a list of redirect powers (summon.powers). The converter resolves those
 * into summon.resolvedEntities; the runtime computes damage off the SUMMONER's
 * archetype table (not a fixed pet class). Numbers below are anchored to the
 * in-game tooltip for a level-1 Blaster (verified 2026-06-06):
 *   - Storm Cell lightning: 5.12 energy / tick   (0.5 × Ranged_Damage(blaster,1))
 *   - Category Five storm:  0.08 cold, 0.82 smashing / tick
 *   - Category Five Eye:    5.12 energy / tick (lightning)
 *
 * STRIP-1 restatement: every `X.effects!.summon!` read moved to `X.summon!`. The strip
 * retired the power-level `effects` bag, and ENT-22 (2026-08-29) hoisted the summon verdict
 * to top-level `power.summon` (`src/types/power.ts:1211`) so pets survive the bag's
 * removal — one writer, one address, on every partition. The nested `ability.effects`
 * trees inside `summon.resolvedEntities` are NOT bag reads: the strip left the resolved
 * payload intact, so those reads are unchanged.
 */
describe('pseudo-pet redirect resolution', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  // Guaranteed damage lands in result.abilities (counts toward the headline DoT).
  const dmgType = (
    entity: ResolvedPseudoPet, abilityName: string, type: string,
  ): number | undefined => {
    const r = calculateResolvedPseudoPetDamage(entity, 'blaster', 1);
    const ab = r?.abilities.find(a => a.ability.name === abilityName);
    return ab?.damageByType.find(d => d.type === type)?.base;
  };

  describe('Storm Cell', () => {
    const summon = StormCell.summon!;
    const pets = summon.resolvedEntities!;

    it('resolves a single pseudo-pet (60s) with Tempest + Lightning', () => {
      expect(pets).toHaveLength(1);
      expect(pets[0].duration).toBe(60);
      const names = pets[0].abilities.map(a => a.name);
      expect(names).toContain('StormCell_Tempest');
      expect(names).toContain('Lightning_Proc');
    });

    it('Tempest carries -recharge, -speed, -tohit (all IgnoreStrength); recharge split from speed', () => {
      const tempest = pets[0].abilities.find(a => a.name === 'StormCell_Tempest')!;
      const rech = tempest.effects!.find(e => e.type === 'RechargeDebuff')!;
      const slow = tempest.effects!.find(e => e.type === 'Slow')!;
      const tohit = tempest.effects!.find(e => e.type === 'ToHitDebuff')!;
      expect(rech).toMatchObject({ scale: 0.07, ignoreStrength: true });
      expect(slow).toMatchObject({ scale: 0.14, ignoreStrength: true }); // movement, distinct from -rech
      expect(tohit).toMatchObject({ scale: 0.7, table: 'Ranged_Debuff_ToHit', ignoreStrength: true });
    });

    it('Tempest carries the empowered WindSpeed values (~2x) for the High Winds toggle', () => {
      const tempest = pets[0].abilities.find(a => a.name === 'StormCell_Tempest')!;
      const pu = tempest.poweredUpEffects!;
      expect(pu.find(e => e.type === 'RechargeDebuff')!.scale).toBeCloseTo(0.14, 3);
      expect(pu.find(e => e.type === 'Slow')!.scale).toBeCloseTo(0.28, 3);
      expect(pu.find(e => e.type === 'ToHitDebuff')!.scale).toBeCloseTo(1.4, 3);
    });

    it('lightning is an ALWAYS-ON damage aura (base 0.5), not mode-gated', () => {
      // The base Storm Cell Lightning Aura fires continuously from the moment the
      // cell exists (verified in-game combat log), like a Death Shroud / Quills
      // aura — its IncreaseStormStrength chance:0 group is an accumulator, not a
      // suppressor. So it counts as guaranteed DoT, not a "while High Winds" effect.
      const lightning = pets[0].abilities.find(a => a.name === 'Lightning_Proc')!;
      expect(lightning.conditionalDamage).toBeFalsy();
      expect(lightning.damage[0]).toMatchObject({ damageType: 'Energy', scale: 0.5 });
      // Guaranteed in the headline DoT at the verified 5.12/tick (lvl 1).
      expect(dmgType(pets[0], 'Lightning_Proc', 'Energy')).toBeCloseTo(5.12, 1);
    });

    it('does not double-count the storm-powered-up Energy copy', () => {
      const lightning = pets[0].abilities.find(a => a.name === 'Lightning_Proc')!;
      expect(lightning.damage.filter(d => d.damageType === 'Energy')).toHaveLength(1);
    });

    it('surfaces the lightning Stun at its 33% proc chance (from the lightning pet)', () => {
      const lightning = pets[0].abilities.find(a => a.name === 'Lightning_Proc')!;
      const stun = lightning.effects!.find(e => e.type === 'Stun')!;
      expect(stun.chance).toBeCloseTo(0.33, 2);
      expect(stun).toMatchObject({ scale: 4, table: 'Ranged_Stun', magnitude: 3 });
    });

    it('no lightning effects are mode-gated (the base aura always fires)', () => {
      const tempest = pets[0].abilities.find(a => a.name === 'StormCell_Tempest')!;
      const lightning = pets[0].abilities.find(a => a.name === 'Lightning_Proc')!;
      expect(tempest.effects!.every(e => !e.conditional)).toBe(true);
      expect(lightning.effects!.every(e => !e.conditional)).toBe(true);
    });

    it('Storm Cell exposes guaranteed lightning damage (the always-on aura)', () => {
      const r = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 1)!;
      expect(r.abilities.some(a => a.ability.name === 'Lightning_Proc')).toBe(true);
      expect(r.totalDpsBase).toBeGreaterThan(0);
    });

    it('powered up (High Winds): lightning doubles to Strong, Tempest shows WindSpeed', () => {
      const off = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 50)!;
      const on = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 50, 0, false, 0, true)!;
      // Base aura always contributes; powered up escalates it to the Strong (2×) variant.
      expect(off.totalDpsBase).toBeGreaterThan(0);
      expect(on.totalDpsBase).toBeCloseTo(off.totalDpsBase * 2, 0);
      // On: Tempest debuffs show the empowered WindSpeed values (~2× the base).
      const rechOff = off.allEffects.find(e => e.type === 'RechargeDebuff')!.value!;
      const rechOn = on.allEffects.find(e => e.type === 'RechargeDebuff')!.value!;
      expect(rechOn).toBeCloseTo(rechOff * 2, 1);
      // Nothing is flagged conditional now — the base aura is always on.
      expect(off.allEffects.every(e => !e.conditional)).toBe(true);
      expect(on.allEffects.every(e => !e.conditional)).toBe(true);
    });

    it('Lightning_Proc carries the Strong-lightning powered-up variant (1.0 = 2x the 0.5 aura)', () => {
      const lightning = pets[0].abilities.find(a => a.name === 'Lightning_Proc')!;
      expect(lightning.damage[0]).toMatchObject({ damageType: 'Energy', scale: 0.5 });
      // StormCell_LightningAura (the "Strong Storm Cell Lightning") swaps in when active.
      expect(lightning.poweredUpDamage).toBeDefined();
      expect(lightning.poweredUpDamage![0]).toMatchObject({ damageType: 'Energy', scale: 1 });
    });

    it('computed effects carry magnitude (mez) + fractional value (debuffs) for display', () => {
      // Backs the Creates-block formatter: debuffs render as "%" (value×100), mez
      // as "mag N". Raw modifiers (0.07) would otherwise read as meaningless.
      const r = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 50)!;
      const stun = r.allEffects.find(e => e.type === 'Stun')!;
      expect(stun.magnitude).toBe(3); // → "mag 3"
      const rech = r.allEffects.find(e => e.type === 'RechargeDebuff')!;
      expect(rech.value).toBeCloseTo(0.07, 2); // fraction → "7%"
    });

    it('powered up: the lightning per-hit doubles (base aura → Strong)', () => {
      const off = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 50)!;
      const on = calculateResolvedPseudoPetDamage(pets[0], 'blaster', 50, 0, false, 0, true)!;
      const hit = (r: typeof off) => r.abilities.find(a => a.ability.name === 'Lightning_Proc')!
        .damageByType.find(d => d.type === 'Energy')!.base;
      expect(hit(on)).toBeCloseTo(hit(off) * 2, 0);
    });
  });

  describe('Category Five', () => {
    const summon = CategoryFive.summon!;
    const pets = summon.resolvedEntities!;

    it('resolves TWO pseudo-pets — the 20s storm and the 17s Eye (collapse fix)', () => {
      expect(pets).toHaveLength(2);
      expect(pets.map(p => p.duration).sort()).toEqual([17, 20]);
    });

    it('storm ticks 0.08 cold + 0.82 smashing at level 1 (guaranteed DoT)', () => {
      const storm = pets.find(p => p.duration === 20)!;
      expect(dmgType(storm, 'Category_Five', 'Cold')).toBeCloseTo(0.08, 2);
      expect(dmgType(storm, 'Category_Five', 'Smashing')).toBeCloseTo(0.82, 2);
    });

    it('Eye lightning is a 25% proc — counted at EXPECTED value (chance × per-hit)', () => {
      const eye = pets.find(p => p.duration === 17)!;
      const lightning = eye.abilities.find(a => a.name === 'Category_Five_Lightning')!;
      expect(lightning.conditionalDamage).toBeUndefined(); // not mode-gated — it's a proc
      expect(lightning.damageChance).toBeCloseTo(0.25, 2);
      // Counted in the headline DoT at expected value: 5.12/tick × 0.25 ≈ 1.28 (lvl 1).
      expect(dmgType(eye, 'Category_Five_Lightning', 'Energy')).toBeCloseTo(5.12 * 0.25, 1);
    });

    it('Eye carries the mag-50 Fear (IgnoreStrength)', () => {
      const eye = pets.find(p => p.duration === 17)!;
      const fear = eye.abilities.flatMap(a => a.effects ?? []).find(e => e.type === 'Fear');
      expect(fear).toMatchObject({ magnitude: 50, ignoreStrength: true });
    });

    it('Eye lightning Stun uses the PvE table (Ranged_Stun×4), not PvP', () => {
      const eye = pets.find(p => p.duration === 17)!;
      const stun = eye.abilities.flatMap(a => a.effects ?? []).find(e => e.type === 'Stun')!;
      expect(stun).toMatchObject({ scale: 4, table: 'Ranged_Stun', magnitude: 3 });
    });
  });

  describe('Tar Patch (typed resistance debuff capture)', () => {
    it('captures the -resistance debuff (all-types template at aspect=Resistance)', () => {
      const pets = TarPatch.summon!.resolvedEntities!;
      const effs = pets.flatMap(p => p.abilities).flatMap(a => a.effects ?? []);
      const res = effs.find(e => e.type === 'ResistanceDebuff');
      expect(res).toBeDefined();
      expect(res!.table).toMatch(/Res_Dmg/i);
      // and it still carries the Slow (it's a -res AND -speed patch)
      expect(effs.some(e => e.type === 'Slow')).toBe(true);
    });
  });

  describe('named shells (Meteor — nested Create_Entity hop)', () => {
    it('resolves damage one Create_Entity hop deep (the spawned MeteorHit)', () => {
      const pets = Meteor.summon!.resolvedEntities!;
      const dmg = pets.flatMap(p => p.abilities).flatMap(a => a.damage);
      const types = new Set(dmg.map(d => d.damageType));
      expect(types.has('Fire')).toBe(true);
      expect(types.has('Smashing')).toBe(true);
    });
  });

  describe('Oil Slick Arrow (ignited conditional entity)', () => {
    const summon = OilSlickArrow.summon!;

    it('base summon is the inert oil slick; the burn patch is a conditional entity', () => {
      expect(summon.entity).toBe('Pets_OilSlickOil'); // inert: KB/Slow/-Def, no damage
      const ce = summon.conditionalEntities!;
      expect(ce).toHaveLength(1);
      expect(ce[0]).toMatchObject({ entity: 'Pets_OilSlickBurn', toggleId: 'oilslick_ignited' });
    });

    it('surfaces an "Oil Slick Ignited" per-power toggle', () => {
      const toggle = OilSlickArrow.conditionalEffects!.find(c => c.id === 'oilslick_ignited')!;
      expect(toggle).toMatchObject({ scope: 'per-power', label: 'Oil Slick Ignited' });
    });

    it('the ignited burn entity carries enhanceable Fire damage', () => {
      const r = calculatePetDamage('Pets_OilSlickBurn', 50, 1, summon.duration);
      expect(r).not.toBeNull();
      expect(r!.abilities.some(a => a.damageByType.some(d => d.type === 'Fire'))).toBe(true);
    });
  });

  describe('Burn (PET_ENTITIES-overlap redirect override)', () => {
    // Burn resolves the Redirects.Fiery_Aura.Burn patch. Post I28P3 (2026-06-23)
    // there are TWO variants: base `burn` (Fire 0.14) and `fieryburn`
    // (0.14 + a persistent Fiery-Embrace bonus 0.063; "FE now lasts the full
    // duration of the patch", scale 0.08→0.14, FE per-tick 0.036→0.063). The
    // Blaster Fire-Manipulation Burn summons the BASE variant (0.14); all five
    // Fiery-Aura armor Burns (tanker/brute/scrapper/sentinel/stalker) summon the
    // FE-active `fieryburn` variant (0.14 + 0.063) — internally consistent across
    // those five ATs (verified in the I28P3 regen). The chassis is normalized to
    // PL_StaticObject so the stale Pets_Burn entity (Fire 0.06) isn't double-
    // counted. The 0.063 FE component is kept (not stripped) because Fiery Aura is
    // a genuine fire-themed set — see _filterFieryEmbraceBonus's fire-set bypass.
    it('normalizes the "Burn" chassis to the generic shell so the pet path finds nothing', () => {
      expect(BlasterBurn.summon!.entity).toBe('PL_StaticObject');
      expect(getPetEntity('PL_StaticObject')).toBeUndefined(); // no double-count
    });

    it('Blaster Fire-Manipulation Burn resolves the BASE patch variant (Fire 0.14 / 0.8s), not the stale 0.06 entity', () => {
      const ab = BlasterBurn.summon!.resolvedEntities![0].abilities[0];
      expect(ab.name).toBe('Burn');
      expect(ab.activatePeriod).toBe(0.8);
      expect(ab.damage).toEqual([{ damageType: 'Fire', scale: 0.14, table: 'Melee_Damage' }]);
    });

    it('Fiery-Aura armor Burns resolve the FE-active variant (Fire 0.14 + FE 0.063), consistent across the 5 armor ATs', () => {
      const tank = TankerBurn.summon!.resolvedEntities![0].abilities[0];
      expect(tank.damage).toEqual([
        { damageType: 'Fire', scale: 0.14, table: 'Melee_Damage' },
        { damageType: 'Fire', scale: 0.063, table: 'Melee_Damage' },
      ]);
    });

    it('does NOT collapse the Fiery-Embrace bonus patch into the base count', () => {
      // scrapper Burn carries a chance:0 Fiery-Embrace duplicate EntCreate sharing
      // the base signature — it must not bump count to 2 (a conditional variant).
      const re = BlasterBurn.summon!.resolvedEntities![0];
      expect(re.count).toBeUndefined(); // i.e. count === 1
    });
  });

  describe('Freezing Rain (absent-entity_def priority_list shell)', () => {
    // entity_def is absent; the shell name (PL_StaticObject) lives in
    // priority_list and the payload in Pets.* redirects. SHELL-1 (fixed):
    // the resolver falls back to priority_list for the effective entity, so
    // these summons carry resolvedEntities + the shell name as entity.
    const re = () => FreezingRain.summon!.resolvedEntities![0];
    const fr = () => re().abilities.find(a => a.name === 'FreezingRain')!;

    it('surfaces the signature -res / -def / -rech / slow debuff kit', () => {
      const byType = (t: string) => fr().effects!.find(e => e.type === t);
      expect(byType('ResistanceDebuff')).toMatchObject({ ignoreStrength: true });
      expect(byType('ResistanceDebuff')!.table).toMatch(/Res_Dmg/i);
      expect(byType('DefenseDebuff')).toBeDefined();
      expect(byType('RechargeDebuff')).toBeDefined();
      expect(byType('Slow')).toBeDefined();
    });

    it('carries minor Cold damage scaled off the summoner AT', () => {
      expect(fr().damage.some(d => d.damageType === 'Cold')).toBe(true);
      const r = calculateResolvedPseudoPetDamage(re(), 'defender', 50);
      expect(r!.abilities.some(a => a.damageByType.some(d => d.type === 'Cold' && d.base > 0))).toBe(true);
    });
  });

  describe('Voltaic Sentinel (PL_Untargetable_FightPreferRanged shell, permanent pet)', () => {
    const summon = VoltaicSentinel.summon!;
    const re = () => summon.resolvedEntities![0];

    it('resolves the bolt (Energy damage + EndDrain) off the generic mobile shell', () => {
      expect(getPetEntity(summon.entity!)).toBeUndefined(); // Pet_NoCollision → no double-count
      const bolt = re().abilities.find(a => a.name === 'Electrical_Bolt')!;
      expect(bolt.damage.some(d => d.damageType === 'Energy')).toBe(true);
      expect(bolt.effects!.some(e => e.type === 'EndDrain')).toBe(true);
    });

    it('is a "permanent" pet (99999s sentinel) — headline shows per-activation, not a 99999s total', () => {
      // The InfoPanel shows PER-ACTIVATION damage for these (PERMANENT_PSEUDOPET_DURATION)
      // rather than an astronomical lifetime total; the calc yields bounded per-hit/DPS.
      expect(re().duration).toBeGreaterThanOrEqual(1000);
      const r = calculateResolvedPseudoPetDamage(re(), 'blaster', 50)!;
      expect(r.totalDpsBase).toBeGreaterThan(0);
      expect(Number.isFinite(r.totalDpsBase)).toBe(true);
      // per-activation (one bolt) damage is bounded and non-zero
      const bolt = r.abilities.find(a => a.ability.name === 'Electrical_Bolt')!;
      expect(bolt.damagePerHit).toBeGreaterThan(0);
      expect(Number.isFinite(bolt.damagePerHit)).toBe(true);
    });
  });

  describe('Sentinel Rain of Fire (inherent-damage split, no double-count)', () => {
    const re = () => SentinelRainOfFire.summon!.resolvedEntities![0];

    it('keeps both Melee_Damage and Melee_InherentDamage entries in the data', () => {
      const fr = re().abilities.find(a => a.name === 'RainofFire')!;
      const tables = fr.damage.filter(d => d.damageType === 'Fire').map(d => d.table);
      expect(tables).toContain('Melee_Damage');
      expect(tables).toContain('Melee_InherentDamage');
    });

    it('counts the Fire hit ONCE at runtime (inherent table is unknown → skipped, not doubled)', () => {
      const r = calculateResolvedPseudoPetDamage(re(), 'sentinel', 50)!;
      const fire = r.abilities.flatMap(a => a.damageByType).filter(d => d.type === 'Fire');
      expect(fire).toHaveLength(1); // Melee_InherentDamage contributes nothing
      expect(fire[0].base).toBeGreaterThan(0);
    });
  });

  describe('un-prefixed priority_list (Sleet → Pets_Sleet fallback)', () => {
    it("getPetEntity tolerates the bare name and finds the Pets_-prefixed entity", () => {
      expect(getPetEntity('Sleet')?.name).toBe('Pets_Sleet');
    });
    // SHELL-1 (fixed): HC Sleet is the same absent-entity_def shell class as
    // Freezing Rain — the payload lives in Pets.Sleet.* redirects, so it
    // resolves as a shell (entity = PL_StaticObject + resolvedEntities), NOT
    // via the getPetEntity('Sleet') pet-entity fallback the beta used.
    it('Sleet summon resolves the shell so its damage/effects surface', () => {
      const summon = Sleet.summon!;
      expect(summon.entity).toBe('PL_StaticObject');
      expect(getPetEntity(summon.entity!)).toBeUndefined(); // no double-count
      const re = summon.resolvedEntities![0];
      const sleet = re.abilities.find(a => a.name === 'Sleet')!;
      // Pets.Sleet.Sleet carries Cold damage + the -res/-def/slow kit
      expect(sleet.damage.some(d => d.damageType === 'Cold')).toBe(true);
      expect(sleet.effects!.some(e => e.type === 'ResistanceDebuff')).toBe(true);
      expect(sleet.effects!.some(e => e.type === 'DefenseDebuff')).toBe(true);
      const r = calculateResolvedPseudoPetDamage(re, 'defender', 50);
      expect(r!.abilities.some(a => a.damageByType.some(d => d.type === 'Cold' && d.base > 0))).toBe(true);
    });
  });
});
