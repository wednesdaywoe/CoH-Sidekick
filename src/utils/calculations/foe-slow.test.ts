import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import { selfSlowValue, selfRechargeDebuffValue, atomsOfType } from '@/data/core/atom-query';

/**
 * Foe movement-slow ("-Speed") extraction.
 *
 * A CoH Slow reduces both Recharge AND movement (Run/Fly/Jump speed), encoded as
 * two AttribMods on a *_Slow table. The converter captured the -Recharge half
 * (`rechargeDebuff`) but DROPPED the movement half — the MOVEMENT block did
 * `continue` on any non-self movement effect. Debuffs are first-class power info,
 * so the movement half is now emitted as a foe `slow` (no `toWho:'Self'` market,
 * so the calc treats it as a foe debuff and the InfoPanel surfaces it).
 * Self-penalty slows (Granite) carry the marker per-entry and are unchanged.
 *
 * Retired bag → atom readers (STRIP-1). `atomsOfType(power, 'Movement')` with a
 * `*_Slow` table IS the movement half; `selfSlowValue` is the reader the totals
 * pass spends, and it publishes only what reaches the caster — a pure foe slow
 * (Ice Bolt's `AnyAffected` rows) is `[]`, while Granite's self rows populate it.
 * The old `hasSelfDirectedPenalty(effects)` bag predicate no longer exists.
 */
describe('Foe movement-slow extraction (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  it('surfaces the movement half of a foe Slow alongside the -Recharge half', () => {
    const ps = getPowerset('blaster/ice-blast');
    const bolt = ps?.powers.find((p) => p.internalName === 'Ice_Bolt');
    expect(bolt).toBeDefined();

    // The movement half: slow-table Movement atoms (were dropped by the old block).
    const movement = atomsOfType(bolt!, 'Movement');
    expect(movement.some((a) => (a.modifierTable ?? '').toLowerCase().includes('slow'))).toBe(true);
    const recharge = atomsOfType(bolt!, 'RechargeTime');
    expect(recharge.some((a) => (a.modifierTable ?? '').toLowerCase().includes('slow'))).toBe(true);

    // Foe debuff — must NOT reach the caster (it doesn't slow the player).
    expect(selfSlowValue(bolt!)).toEqual([]);
    expect(selfRechargeDebuffValue(bolt!)).toBeUndefined();
  });

  it('leaves self-penalty Slows (Granite Armor) self-directed (toWho:Self)', () => {
    const ps = getPowerset('tanker/stone-armor');
    const granite = ps?.powers.find((p) => p.internalName === 'Granite_Armor');
    expect(granite).toBeDefined();
    // Split from canonical's `selfSlowValue(granite!).length`: the reader returns
    // `MovementBuffEntry[] | undefined` in both repos, so the direct `.length` is a strict-null
    // error the beta's typecheck catches. Asserting defined first is also the stronger claim —
    // `undefined` and `[]` are different answers and only one of them is a self-directed slow.
    const slow = selfSlowValue(granite!);
    expect(slow).toBeDefined();
    expect(slow!.length).toBeGreaterThan(0);
    expect(selfRechargeDebuffValue(granite!)).toBeDefined();
  });
});
