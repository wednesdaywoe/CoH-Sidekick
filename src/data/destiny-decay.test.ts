import { describe, it, expect, beforeAll } from 'vitest';
import { getActiveDataset, loadDataset } from '@/data/dataset';
import {
  getDestinyEffects,
  getDestinyEffectsAtTime,
  getDestinyTimeline,
  getDestinyTotalDuration,
  getDestinySustainedFloorTime,
} from '@/data';

/**
 * Destiny buffs (Barrier, Ageless, …) apply several overlapping timed buffs at
 * once; the effective value at time t is the SUM of tiers whose duration > t.
 * These tests pin the additive-decay resolver against the in-game reference
 * for T4 Barrier Core Epiphany — 90% at cast, decaying to a 5% floor — so a
 * future re-gen or a regression to the old "single strongest tier" behaviour
 * can't slip through.
 *
 *   raw tiers: 0.575@10s, 0.25@30s, 0.025@60s, 0.05@120s
 *   0–10s   : 0.575+0.25+0.025+0.05 = 0.90  (90%)
 *   10–30s  :       0.25+0.025+0.05 = 0.325 (32.5%)
 *   30–60s  :            0.025+0.05 = 0.075 (7.5%)
 *   60–120s :                  0.05 = 0.05  (5%)
 *   120s+   :                          0    (expired)
 */
describe('Destiny decay resolver (Homecoming — Barrier Core Epiphany)', () => {
  const ID = 'barrier_core_epiphany';

  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  it('exposes a decay timeline with the raw additive tiers', () => {
    const timeline = getDestinyTimeline(ID);
    expect(timeline).not.toBeNull();
    expect(timeline!.defenseAll).toEqual([
      { value: 0.575, duration: 10 },
      { value: 0.25, duration: 30 },
      { value: 0.025, duration: 60 },
      { value: 0.05, duration: 120 },
    ]);
    expect(getDestinyTotalDuration(ID)).toBe(120);
  });

  it('sustained floor is the start of the final plateau (60s → 5%)', () => {
    // Barrier T4 floor phase is [60s, 120s); the auto default resolves here.
    expect(getDestinySustainedFloorTime(ID)).toBe(60);
    const floor = getDestinyEffectsAtTime(ID, getDestinySustainedFloorTime(ID))!;
    expect(floor.defenseAll).toBeCloseTo(0.05, 6);
    expect(floor.resistanceAll).toBeCloseTo(0.05, 6);
  });

  it.each([
    [0, 0.9],
    [5, 0.9],
    [10, 0.325], // 0.575 tier just expired
    [20, 0.325],
    [30, 0.075],
    [45, 0.075],
    [60, 0.05],
    [90, 0.05],
    [120, 0], // fully expired
    [200, 0],
  ])('resolves defense/resistance at t=%is to %f (additive)', (t, expected) => {
    const fx = getDestinyEffectsAtTime(ID, t)!;
    expect(fx.defenseAll).toBeCloseTo(expected, 6);
    expect(fx.resistanceAll).toBeCloseTo(expected, 6);
  });

  it('preserves the time-independent level shift across the timeline', () => {
    expect(getDestinyEffectsAtTime(ID, 0)!.levelShift).toBe(1);
    expect(getDestinyEffectsAtTime(ID, 200)!.levelShift).toBe(1);
  });

  it('undefined time returns the flat peak values unchanged', () => {
    const flat = getDestinyEffects(ID)!;
    const resolved = getDestinyEffectsAtTime(ID, undefined)!;
    expect(resolved.defenseAll).toBe(flat.defenseAll);
    expect(resolved.resistanceAll).toBe(flat.resistanceAll);
  });

  it('the additive peak (90%) exceeds the legacy single-tier peak (57.5%)', () => {
    // Regression guard: the flat table still stores the strongest single tier;
    // the resolver must sum, not pick the max.
    expect(getDestinyEffects(ID)!.defenseAll).toBeCloseTo(0.575, 6);
    expect(getDestinyEffectsAtTime(ID, 0)!.defenseAll).toBeCloseTo(0.9, 6);
  });
});

/**
 * Second independent reference (Ageless Core Epiphany) — verified against the
 * Rebirth wiki / community guides: recharge 70→30→20→10%, recovery
 * 800→300→200→100%, plus an instantaneous endurance refill at cast.
 */
describe('Destiny decay resolver (Homecoming — Ageless Core Epiphany)', () => {
  const ID = 'ageless_core_epiphany';

  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  it.each([
    [0, 0.7, 8],
    [10, 0.3, 3],
    [30, 0.2, 2],
    [60, 0.1, 1],
    [120, 0, 0],
  ])('at t=%is resolves recharge=%f, recovery=%f (additive)', (t, rech, rec) => {
    const fx = getDestinyEffectsAtTime(ID, t)!;
    expect(fx.recharge).toBeCloseTo(rech, 6);
    expect(fx.recovery).toBeCloseTo(rec, 6);
  });

  it('instantaneous endurance refill (duration 0) counts only at t=0', () => {
    expect(getDestinyEffectsAtTime(ID, 0)!.endurance).toBe(1);
    expect(getDestinyEffectsAtTime(ID, 5)!.endurance).toBe(0);
  });
});

describe('Destiny timeline guard — same-duration twins stay additive', () => {
  const ID = 'zz_dsh8_destiny_twin_guard';

  beforeAll(async () => {
    await loadDataset('homecoming');
    // Synthetic fixture: two same-duration tiers on one stat + one longer tier.
    // This models a resistible/unresistable twin represented as duplicate timeline
    // rows at equal duration. The resolver must sum both rows while active.
    //
    // Injected through the ACTIVE dataset, which is the object the resolver reads
    // (`getActiveDataset().incarnateEffectsRaw.destiny`). Writing to the generated
    // module's export instead would only be the same object by coincidence of how the
    // dataset was loaded — and under vitest it is not: the dataset comes from the
    // flattened bundle, so the write landed on a copy nothing reads.
    const raw = getActiveDataset().incarnateEffectsRaw;
    raw.destiny[ID] = { recharge: 0.2 };
    raw.destinyTimeline[ID] = {
      recharge: [
        { value: 0.1, duration: 30 },
        { value: 0.2, duration: 30 },
        { value: 0.05, duration: 60 },
      ],
    };
  });

  it('sums duplicate-duration rows instead of collapsing to one', () => {
    expect(getDestinyEffectsAtTime(ID, 0)!.recharge).toBeCloseTo(0.35, 6);
    // At t=30, the 30s rows have expired (duration > t rule), leaving only 60s.
    expect(getDestinyEffectsAtTime(ID, 30)!.recharge).toBeCloseTo(0.05, 6);
  });
});
