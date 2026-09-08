import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data';
import { applyActiveConditionals, describeAdjusterContribution } from './powerDisplayUtils';
import { selectActiveConditionals } from '@/utils/conditional-effects';
import type { ConditionalEffect, Power } from '@/types';

/**
 * The "+ extra … instance" hint under a stance row must describe what the merger
 * actually records.
 *
 * `applyActiveConditionals` treats `durations` / `buffDuration` / `effectDuration`
 * as per-effect metadata and never records them as extra instances, but
 * `describeAdjusterContribution` — which writes the hint — skipped only
 * `durations`. So every Bio Armor stance row advertised an "extra Buff Duration
 * instance" that does not exist (screenshot 2026-07-26). Both now read one shared
 * key set; this pins the two against each other rather than against a literal.
 */
describe('adjuster contribution hint (homecoming)', () => {
  beforeAll(async () => { await loadDataset('homecoming'); });

  const bioPowers = () =>
    (getPowerset('scrapper/bio-armor')!.powers as Power[]).filter(
      (p) => (p.conditionalEffects ?? []).length > 0,
    );

  it('never names a duration-metadata key as a collision', () => {
    for (const power of bioPowers()) {
      for (const c of power.conditionalEffects as ConditionalEffect[]) {
        const { collisionKeys, newKeys } = describeAdjusterContribution(power, c);
        for (const k of [...collisionKeys, ...newKeys]) {
          expect(k, `${power.internalName}/${c.id}`).not.toMatch(/^(durations|buffDuration|effectDuration)$/);
        }
      }
    }
  });

  // The skip above states itself HERE, in a name. A `console.warn` cannot: this repo's reporter
  // prints console output only for a failing file, so every reach-counter in these suites is
  // invisible for exactly as long as the suite is green. A test name is in the record either way.
  it('STATED SKIP — every stance carrier has an empty authored bag, so no collision is findable', () => {
    const withAuthoredBase = bioPowers().filter(
      (p) => Object.keys((p.effects ?? {}) as Record<string, unknown>).length > 0,
    );
    expect(bioPowers().length).toBeGreaterThan(0);
    expect(withAuthoredBase.map((p) => p.internalName)).toEqual([]);
  });

  it('claims an extra instance only where the merger records one', () => {
    let checked = 0;
    for (const power of bioPowers()) {
      for (const c of power.conditionalEffects as ConditionalEffect[]) {
        const active = selectActiveConditionals(power, {}, { [c.id]: true });
        const { extraInstances } = applyActiveConditionals(power, active);
        const recorded = new Set(Object.keys(extraInstances));
        for (const k of describeAdjusterContribution(power, c).collisionKeys) {
          expect(recorded.has(k), `${power.internalName}/${c.id} claims "${k}"`).toBe(true);
          checked++;
        }
      }
    }

    // Both sides of the pin ask `power.effects` whether the base states a key, and STRIP-1
    // emptied it — so neither can find a collision and the two agree on nothing. That is a
    // STATED skip, not a pass: the precondition is asserted, so the day the base surface comes
    // back this flips itself to the real assertion instead of staying quietly green.
    //
    // It is the same defect as PROD6B-BETA-PARITY's last 980 rows, at a third site — there the
    // starved probe let a conditional's copy fill in, here it makes every colliding key read as
    // NEW, so the "+ extra … instance" hint has stopped rendering everywhere. The fix is the same
    // one and the beta cannot make it yet: it needs an atom -> bag-key projection to ask, which
    // is the TS half of the port. Recorded as a carried residual under PROD6B-BETA-PARITY.
    const withAuthoredBase = bioPowers().filter(
      (p) => Object.keys((p.effects ?? {}) as Record<string, unknown>).length > 0,
    );
    if (withAuthoredBase.length === 0) {
      expect(checked, 'the base bag is empty, so no collision can be found — but one was').toBe(0);
      return;
    }

    // The Bio Armor stances DO collide on real effect keys (Hardened Carapace's
    // resistance, Environmental Modification's defenseBuff) — if this hits zero
    // the test proved nothing.
    expect(checked).toBeGreaterThan(0);
  });
});
