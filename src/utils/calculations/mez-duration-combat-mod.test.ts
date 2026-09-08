import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getTableValue } from '@/data/at-tables';
import { getCombatModifier } from '@/data/purple-patch';
import { Dominate } from '@/data/datasets/homecoming/generated/powersets/dominator/primary/mind-control/dominate';
import { mezSlotValue } from '@/data/core/atom-query';

/**
 * Enemy-level (purple patch) scaling of MEZ DURATION.
 *
 * The enemy-level slider already scaled damage but not control duration, so a
 * hold read the same length against an even-con enemy and a +4. In CoH, mez is
 * the effect class the game flags `UseDurationCombatMods` — verified against
 * Rebirth's decodable bool-block, it maps to exactly {Held, Immobilized,
 * Stunned, Sleep, Confused, Terrorized, Afraid}. Its `pfDuration` array is
 * server-side (not shipped in the client bins) and the wikis document mez
 * duration as scaling by the SAME combat-mod curve as damage, so the mez-row
 * render reuses the magnitude `getCombatModifier`.
 *
 * These lock: (a) the curve is a no-op at even con, (b) a +4 cuts a hold's
 * displayed duration to ~48%, mirroring `SharedPowerComponents`'
 * `finalDuration = baseDuration * durCombatMod`.
 *
 * The hold read is atom-native (`mezSlotValue(Dominate,'hold')`) — the retired
 * `effects.hold` bag slot no longer exists (STRIP-1).
 */
describe('mez duration scales with enemy level (purple patch)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  it('even-con enemy leaves duration unchanged (modifier = 1)', () => {
    expect(getCombatModifier(0)).toBe(1);
  });

  it('a +4 enemy cuts effect scaling to ~48%, +2 to 80%', () => {
    expect(getCombatModifier(4)).toBeCloseTo(0.48, 3);
    expect(getCombatModifier(2)).toBeCloseTo(0.8, 3);
  });

  it("Dominate's hold duration shrinks against higher-con enemies", () => {
    const hold = mezSlotValue(Dominate, 'hold');
    expect(hold).toBeDefined();
    expect(hold!.attribType).toBe('Duration');
    const mez = hold as { scale: number; table: string };

    // Mirror the SharedPowerComponents render: baseDuration = |scale × tableVal|,
    // finalDuration = baseDuration × combatMod(levelDiff).
    const tableVal = getTableValue('dominator', mez.table, 50);
    expect(tableVal).toBeDefined();
    const baseDuration = Math.abs(mez.scale * tableVal!);
    expect(baseDuration).toBeGreaterThan(0);

    const evenCon = baseDuration * getCombatModifier(0);
    const vsPlus4 = baseDuration * getCombatModifier(4);

    expect(evenCon).toBeCloseTo(baseDuration, 5);       // no change even-con
    expect(vsPlus4).toBeCloseTo(baseDuration * 0.48, 4); // ~48% vs +4
    expect(vsPlus4).toBeLessThan(baseDuration);          // strictly shorter
  });
});
