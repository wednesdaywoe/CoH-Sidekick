/**
 * Regression guard for the RefreshToCount stacking-flaw fix + Wild Bastion perma fix.
 *
 * **BPORT13.** Every claim here was written against `power.effects` — the stacking metadata
 * (`maxStacks`, `stacksLinear`, `stackCaps`), the per-foe increments, and the duration map all
 * lived in the bag. BPORT7 removed the key outright, so the six assertions did not go quiet,
 * they threw. Nothing about the SUBJECT changed: the converter still emits every one of these
 * facts, on the atom that carries it. So this is a restatement onto the atoms, not a skip —
 * each claim below is the same claim, read from where the fact actually lives:
 *
 *   - `effects.maxStacks` / `effects.stackCaps` → `atom.stackCap`, per atom rather than one
 *     number per power with a side-table of exceptions. That the two Absorb/debuff-res halves
 *     cap differently was the whole finding, and on the atoms it is not an exception at all:
 *     the Absorb atom says 2 and the four `Res`-face atoms say 3.
 *   - `effects.stacksLinear` → `stackCapOf`, which admits an atom only if `selfStacks` does.
 *   - `effects.debuffResistance.recharge` → `debuffResistanceValue`.
 *   - `effects.durations.absorb` → the Absorb atom's own `duration`.
 *
 * One claim is NOT restated here, deliberately. Consume Psyche's `perTarget` pair and its
 * ×10 arithmetic are asserted live, on the same power, off the same readers, in
 * `resources-atom-native.verify.test.ts` — restating them would be a second copy of one
 * question, and the copy would be the one nobody updates. What that file does NOT say is the
 * N² claim, so that is the half kept below.
 *
 * If HC rebalances Psionic Armor / Nature Affinity these values may shift — update them, but
 * the STRUCTURE (RefreshToCount→stacks, perTarget base+increment, divergent stackCaps,
 * absorb-duration→perma) must hold.
 */
import { describe, it, expect } from 'vitest';
import { isPermaEligible } from './perma';
import {
  atomsOf, absorbValue, absorbMaxHPFractionValue, debuffResistanceValue, rechargeBuffValue,
  regenBuffValue, recoveryBuffValue, stackCapOf, buffStack, DEBUFF_RESISTANCE_STACK,
} from '@/data/core/atom-query';
import { FortifyMind as BruteFortifyMind } from '@/data/datasets/homecoming/generated/powersets/brute/secondary/psionic-armor/fortify-mind';
import { ConsumePsyche } from '@/data/datasets/homecoming/generated/powersets/brute/secondary/psionic-armor/consume-psyche';
import { WildBastion } from '@/data/datasets/homecoming/generated/powersets/controller/secondary/nature-affinity/wild-bastion';

// Mirror the calc's cap math (adjustForStacking) so we assert the numbers a build would show.
const linearAt = (scale: number, n: number, cap: number) => scale * Math.min(n, cap);

describe('Psychokinetic Barrier (Fortify_Mind) — RefreshToCount debuff-res ×3', () => {
  it('surfaces a 3-stack axis whose two halves cap differently', () => {
    // The bag said `maxStacks: 3` with `stackCaps: {absorb: 2}` — one number and an
    // exception list. The atoms carry the cap per row, so "divergent" is just what the
    // rows say, and the exception list has no reason to exist.
    expect(stackCapOf(BruteFortifyMind, buffStack('Absorb'))).toBe(2);
    expect(stackCapOf(BruteFortifyMind, DEBUFF_RESISTANCE_STACK)).toBe(3);
    // And the axis is RefreshToCount on the half that reaches 3 — the flavour the fix was
    // about. Asserted on the rows rather than on a count: `Stack` here would be the bug.
    const res = atomsOf(BruteFortifyMind).filter((a) => a.aspect === 'Res');
    expect(res.length).toBe(4);
    expect(res.every((a) => a.stacking === 'RefreshToCount')).toBe(true);
    expect(res.every((a) => a.stackCap === 3)).toBe(true);
  });

  it('debuff-resistance scales to +60% at 3 stacks; absorb clamps at ×2', () => {
    const dr = debuffResistanceValue(BruteFortifyMind)!.recharge!.scale; // 0.2
    expect(linearAt(dr, 3, stackCapOf(BruteFortifyMind, DEBUFF_RESISTANCE_STACK)!)).toBeCloseTo(0.6);
    const abs = absorbValue(BruteFortifyMind)!.scale; // 3.0
    expect(linearAt(abs, 3, stackCapOf(BruteFortifyMind, buffStack('Absorb'))!)).toBeCloseTo(6.0); // NOT 9.0
  });

  it('did NOT leak the RechargeTime@Resistance template into a fake rechargeBuff', () => {
    // The claim that gained force on the atoms. The bag version asked whether the converter
    // had written a slot; this asks the reader the totals actually spend, and the thing
    // keeping it honest is `combatModifierSlot`'s `aspect === 'Res'` skip. Delete that skip
    // and the caster gets +20% recharge it does not have.
    expect(rechargeBuffValue(BruteFortifyMind)).toBeUndefined();
    expect(debuffResistanceValue(BruteFortifyMind)?.recharge).toBeDefined();
  });
});

describe('Consume Psyche — the per-foe increment must not also stack', () => {
  it('keeps perTarget off the stacking multiplier, so the buff is linear in N and not N²', () => {
    // The bag stated this as an absence — `stacksLinear` not containing `regenBuff` — which
    // is a claim about a list the converter writes. The atoms state it as a property of the
    // rows: the increment-bearing atoms are `RefreshToCount`, `selfStacks` declines that
    // flavour, and so `stackCapOf` has nothing to hand the multiplier. A converter that
    // re-stamped these as `Stack` would multiply a per-foe value by the stack count.
    for (const [type, arm] of [
      ['Regeneration', regenBuffValue], ['Recovery', recoveryBuffValue],
    ] as const) {
      expect(arm(ConsumePsyche)!.perTarget, type).toBeGreaterThan(0);
      const carriers = atomsOf(ConsumePsyche)
        .filter((a) => a.effectType === type && a.toWho === 'Self' && (a.perTarget ?? 0) > 0);
      expect(carriers.length, type).toBeGreaterThan(0);
      expect(carriers.every((a) => a.stacking === 'RefreshToCount'), type).toBe(true);
      expect(stackCapOf(ConsumePsyche, buffStack(type)), `${type} stacks AND scales per foe`)
        .toBeUndefined();
    }
  });
});

describe('Wild Bastion — absorb shield is now perma-trackable', () => {
  it('is perma-eligible (absorb duration counts as a self-state)', () => {
    // `effects.durations.absorb` was the bag's parallel duration map; the duration is a field
    // on the Absorb atom itself. Pinned to the value, not to `> 0`: the 60s is what makes the
    // shield perma-trackable at all, and a shorter one silently changes the verdict.
    const absorb = atomsOf(WildBastion).filter((a) => a.effectType === 'Absorb');
    expect(absorb).toHaveLength(1);
    expect(absorb[0].duration).toBeCloseTo(60);
    expect(isPermaEligible(WildBastion)).toBe(true);
  });

  it('states the absorb through the Max-face reader, which is the only one that sees it', () => {
    // The cluster BPORT6 named as the migration's one would-be regression: this absorb is a
    // `Max`-face Expression ceiling, so `absorbValue` excludes it by design and a guard
    // asserting "the power has an absorb" through that reader would pass on the wrong half.
    expect(absorbValue(WildBastion)).toBeUndefined();
    expect(absorbMaxHPFractionValue(WildBastion)).toBeCloseTo(0.25);
  });
});
