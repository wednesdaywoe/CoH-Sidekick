import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getAllPowersets } from '@/data/powersets';
import {
  resolveProcRollSchedule,
  procRollsInPatch,
  calculateScheduledProcChance,
  calculateProcChance,
} from './proc-data';
import { getProcPotential, procPotentialTier } from './proc-potential';
import { resolveProcPatchDuration } from '@/utils/calculations/pet-damage';
import { calculateSlottedProcDamagePerCast } from '@/utils/calculations/power-proc-damage';
import { Sleet } from './datasets/homecoming/generated/powersets/defender/primary/cold-domination/sleet';
import { Bonfire } from './datasets/homecoming/generated/powersets/controller/primary/fire-control/bonfire';
import { Burn } from './datasets/homecoming/generated/powersets/tanker/primary/fiery-aura/burn';
import { LightningRod } from './datasets/homecoming/generated/powersets/scrapper/primary/electrical-melee/lightning-rod';
import { LifegivingSpores } from './datasets/homecoming/generated/powersets/controller/secondary/nature-affinity/lifegiving-spores';
import { GreaterFireSword } from './datasets/homecoming/generated/powersets/scrapper/primary/fiery-melee/greater-fire-sword';
import { ShieldCharge } from './datasets/homecoming/generated/powersets/scrapper/secondary/shield-defense/shield-charge';
import { RainofArrows } from './datasets/homecoming/generated/powersets/blaster/primary/archery/rain-of-arrows';
import type { Power } from '@/types/power';
import type { IOSetEnhancement } from '@/types';

/**
 * RAIN / PATCH PROC ROLLS — measured in game 2026-08-05.
 *
 * The app used to score a rain's procs against the PARENT's recharge, which put
 * every proc slotted in Sleet at the 90% ceiling and made every rain read as a
 * top-tier proc bomb. The rolls do not happen there. They happen on the summoned
 * patch, which is an `Auto` with recharge 0, so each is scored against the 10s
 * period every PPM IO carries — and the patch lives long enough for several.
 *
 * 26 clean Sleet casts × 5 procs × 2 windows = 260 trials, 37 firings (14.2%,
 * Wilson 95% CI 10.5–19.0%). Firings landed at exactly two patch ages, 0s and
 * 10s, seventeen times over. The 10s-period-with-area-factor model predicts
 * 17.9% (z = −1.56); no-area-factor predicts 58.3% (z = −14.4) and the parent's
 * recharge predicts 90% (z = −40.7).
 *
 * Two halves are pinned, and the second is the one that decays quietly:
 *  - the SCHEDULE — 2 rolls for a 15s rain, at 17.9% each, recharge-immune;
 *  - the CONSEQUENCE — no patch power carries a proc-bomb badge any more,
 *    because none of them can reach the cap the badge counts.
 */

const HC = (p: unknown) => p as Power;

/** areaDenominator for a 20ft sphere: 0.25 + 0.75 × (1 + 20 × 4500/30000) = 3.25. */
const SLEET_DENOM = 3.25;
/** A 3.5 PPM proc on Sleet's patch: 3.5 × 10 / (60 × 3.25). */
const SLEET_PER_ROLL = (3.5 * 10) / (60 * SLEET_DENOM);

const DAMAGE_PROC = {
  id: 'test-proc',
  type: 'io-set',
  name: 'Chance for Fire Damage',
  setName: 'Bombardment',
  isProc: true,
  level: 50,
  pieceNum: 6,
} as unknown as IOSetEnhancement;

/** Sleet's own numbers, as the damage calc receives them. */
const SLEET_DAMAGE_INPUT = {
  slots: [DAMAGE_PROC],
  baseRecharge: 60,
  castTime: 2.03,
  radius: 20,
  arcDegrees: 360,
  rechargeEnh: 0,
  buildLevel: 50,
  powerType: 'Click',
  patchDuration: 15,
};

describe('patch proc rolls', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  describe('the schedule', () => {
    it('gives a 15s rain two rolls on the proc period, not one on the recharge', () => {
      const schedule = resolveProcRollSchedule({
        powerType: 'Click',
        baseRecharge: 60,
        castTime: 2.03,
        patchDuration: 15,
      });
      expect(schedule).toEqual({ window: 10, castTime: 0, rolls: 2, fixedPeriod: true });
      expect(calculateScheduledProcChance(3.5, schedule, 20, 360)).toBeCloseTo(SLEET_PER_ROLL, 6);
      // 17.9%, inside the measured 10.5–19.0% interval and nowhere near 90%.
      expect(SLEET_PER_ROLL).toBeCloseTo(0.1795, 3);
    });

    it('leaves an ordinary click exactly as it was', () => {
      const schedule = resolveProcRollSchedule({ powerType: 'Click', baseRecharge: 12, castTime: 1.5 });
      expect(schedule).toEqual({ window: 12, castTime: 1.5, rolls: 1, fixedPeriod: false });
      // Byte-for-byte the old path, recharge terms and all.
      expect(calculateScheduledProcChance(3.5, schedule, 0, 360, 0.95, 0.7))
        .toBe(calculateProcChance(3.5, 12, 1.5, 0, 360, 0.95, 0.7));
    });

    it('counts rolls at patch ages 0, 10, 20 … and not one at expiry', () => {
      // A cycle long enough not to bind, so this exercises the duration alone.
      const rolls = (d: number) => procRollsInPatch(d, 600);
      expect([rolls(1), rolls(7), rolls(10), rolls(15), rolls(20), rolls(45)])
        .toEqual([1, 1, 1, 2, 2, 5]);
    });

    it('caps a patch that outlives its own cooldown at one cycle of rolls', () => {
      // Lifegiving Spores is authored with duration 99999 — "until you recast".
      // Recasting replaces the patch, so a cast is only ever credited with the
      // rolls that land before the next one. Uncapped this would be 10,000.
      expect(procRollsInPatch(99999, 6.33)).toBe(1);
      expect(procRollsInPatch(240, 120)).toBe(12);
    });
  });

  describe('recharge cannot move a patch roll', () => {
    // The most build-relevant consequence, and the one a future refactor is most
    // likely to undo by threading the recharge terms back through.
    it('slotted and global recharge leave the chance untouched', () => {
      const bare = calculateSlottedProcDamagePerCast(SLEET_DAMAGE_INPUT);
      const slotted = calculateSlottedProcDamagePerCast({
        ...SLEET_DAMAGE_INPUT, rechargeEnh: 0.95, globalRechargeEnh: 0.6625,
      });
      expect(bare).toBeGreaterThan(0);
      expect(slotted).toBe(bare);
    });

    it('MUTANT: the same slotting on a non-patch power DOES move with recharge', () => {
      const asClick = { ...SLEET_DAMAGE_INPUT, patchDuration: undefined };
      expect(calculateSlottedProcDamagePerCast({ ...asClick, rechargeEnh: 0.95 }))
        .not.toBe(calculateSlottedProcDamagePerCast(asClick));
    });

    it('pays out both rolls — the fix is not just a discount', () => {
      // 2 × 17.9% against the single 90% roll the app used to bill: a real cut,
      // but a rain must not collapse to one roll on the way there.
      const oneRoll = calculateSlottedProcDamagePerCast({ ...SLEET_DAMAGE_INPUT, patchDuration: 10 });
      expect(calculateSlottedProcDamagePerCast(SLEET_DAMAGE_INPUT) / oneRoll).toBeCloseTo(2, 6);
    });
  });

  describe('the powers, end to end', () => {
    it('reads Sleet as two 17.9% rolls', () => {
      expect(resolveProcPatchDuration(0, HC(Sleet).summon)).toBe(15);
      const p = getProcPotential(HC(Sleet))!;
      expect({ rolls: p.rolls, recharge: p.recharge, radius: p.radius, fromPseudoPet: p.fromPseudoPet })
        .toEqual({ rolls: 2, recharge: 10, radius: 20, fromPseudoPet: true });
      expect(p.total).toBeGreaterThan(0);
      expect(p.atCap).toBe(0);
      expect(Math.max(...p.entries.map((e) => e.chance))).toBeLessThan(0.3);
    });

    it('scales rolls with patch lifetime across the family', () => {
      const rolls = (power: unknown) => getProcPotential(HC(power))!.rolls;
      expect({
        bonfire: rolls(Bonfire),               // 45s
        sleet: rolls(Sleet),                   // 15s
        burn: rolls(Burn),                     // 10s — one roll, no second window
        lightningRod: rolls(LightningRod),     // 1s
        lifegivingSpores: rolls(LifegivingSpores), // 99999s, capped by a 6.3s cycle
      }).toEqual({ bonfire: 5, sleet: 2, burn: 1, lightningRod: 1, lifegivingSpores: 1 });
    });

    /**
     * The in-game measurements, as a gate.
     *
     * Everything else in this file checks the code against itself. This checks
     * it against the game: for each power, the chance the app computes for a
     * 3.5 PPM proc must land inside the 95% confidence interval that power's
     * logged firings actually produced. It is the only test here that can catch
     * a plausible-looking change to the geometry or the schedule that happens
     * to be wrong — a radius read from the parent instead of the patch, a
     * missing area factor, the parent's recharge creeping back in.
     *
     * Intervals are wide because in-game trials are expensive; that is the
     * point. A model inside all four is not proven, but a model outside one has
     * been contradicted by something more authoritative than the bins.
     */
    it('lands inside the measured 95% interval on every power tested in game', () => {
      const MEASURED: Array<[string, unknown, number, number, number]> = [
        // power                  firings/trials     CI low  CI high
        ['Lightning Rod',  LightningRod,  19 / 85,  0.148, 0.323],
        ['Shield Charge',  ShieldCharge,  12 / 68,  0.104, 0.284],
        ['Rain of Arrows', RainofArrows,  31 / 224, 0.099, 0.190],
        ['Sleet',          Sleet,         37 / 260, 0.105, 0.190],
      ];
      const verdicts = MEASURED.map(([name, power, , lo, hi]) => {
        // Every set below fields a 3.5 PPM damage proc, so one PPM makes all
        // four comparable; the mixed-PPM arithmetic lives in the write-up.
        const chance = calculateScheduledProcChance(
          3.5,
          resolveProcRollSchedule({
            powerType: HC(power).powerType,
            baseRecharge: HC(power).stats?.recharge ?? 0,
            castTime: HC(power).stats?.castTime ?? 0,
            patchDuration: resolveProcPatchDuration(
              HC(power).stats?.radius ?? 0, HC(power).summon),
          }),
          getProcPotential(HC(power))!.radius,
          360,
        );
        return { name, inside: chance >= lo && chance <= hi };
      });
      expect(verdicts).toEqual(MEASURED.map(([name]) => ({ name, inside: true })));
    });

    it('leaves a plain attack on one recharge-scored roll', () => {
      const p = getProcPotential(HC(GreaterFireSword))!;
      expect({ rolls: p.rolls, fromPseudoPet: p.fromPseudoPet })
        .toEqual({ rolls: 1, fromPseudoPet: false });
      expect(p.recharge).toBe(HC(GreaterFireSword).stats?.recharge);
    });
  });

  describe('the proc-bomb badge', () => {
    it('no patch power in Homecoming carries one', () => {
      let patches = 0;
      let badged: string[] = [];
      for (const set of Object.values(getAllPowersets())) {
        for (const power of set.powers ?? []) {
          const potential = getProcPotential(power);
          if (!potential || potential.rolls <= 1) continue;
          patches++;
          if (procPotentialTier(potential) > 0) badged.push(power.name);
        }
      }
      // The `badged` half passes trivially if the sweep matched nothing, so the
      // patch count is asserted too. 68 HC powers reach the patch schedule
      // today; only those living past 10s show up here as multi-roll.
      expect(badged).toEqual([]);
      expect(patches).toBeGreaterThanOrEqual(40);
    });

    it('MUTANT: Sleet is a proc bomb again if the patch clock is ignored', () => {
      // Strip the summon and hand the parent the patch's radius — i.e. exactly
      // the old model, a 60s Click at 20ft. 16 of the 20 procs in Sleet's pool
      // pin to the 90% ceiling there (the stragglers are the 1–2 PPM utility
      // procs, which even 62s cannot rescue through a ÷3.25 area tax), against
      // a 6-slot ceiling. That is the badge users were shown.
      // `summon` is top-level now (the writer lifts it out of the bag — BPORT7), so the
      // "what if this were a plain click" fixture drops it there.
      const asClick = {
        ...HC(Sleet),
        stats: { ...HC(Sleet).stats, radius: 20 },
        summon: undefined,
      } as Power;
      const p = getProcPotential(asClick)!;
      expect(p.rolls).toBe(1);
      expect(p.atCap).toBeGreaterThanOrEqual(p.maxSlots);
      expect(procPotentialTier(p)).toBeGreaterThanOrEqual(1);
    });
  });
});
