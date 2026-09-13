/**
 * `groupEffectsByCategory` resolves a `<base>Unenhanced` split slot — ENT-6.
 *
 * The converter expresses an IgnoreStrength verdict as a parallel bag slot rather than as a flag
 * on the value (`atom-query.ts` names the discriminator and the five slots the corpus ships), and
 * no such slot is registered in `EFFECT_REGISTRY`. So the grouping loop's `if (!config) continue`
 * used to drop every one of them, which is ENT-6's sentence verbatim: the `*Unenhanced` half of a
 * buff reached the totals with no display row.
 *
 * This graded nothing in either repo until the two copies converged (FORK-7) — the beta's
 * `beta-display.test.ts` asserts the CALL SITE, not the resolution. A rule living in two
 * hand-copied files with no gate over it is the shape FORK-7 exists to catch, so the rule is
 * graded here instead of trusted twice.
 */
import { describe, it, expect } from 'vitest';
import { EFFECT_REGISTRY, groupEffectsByCategory } from './effect-registry';

/** Flatten the grouped shape to `key -> entry`, which is what every assertion below wants. */
function byKey(effects: Record<string, unknown>) {
  const out = new Map<string, { effectKey: string; fromSplitSlot?: boolean; category: string }>();
  for (const group of groupEffectsByCategory(effects)) {
    for (const e of group.effects) {
      out.set(e.key, { effectKey: e.effectKey, fromSplitSlot: e.fromSplitSlot, category: group.category });
    }
  }
  return out;
}

/** The five parallel slots the bag actually ships (`atom-query.ts`), not a sample of them. */
const SHIPPED_SLOTS = [
  'maxHPBuffUnenhanced',
  'recoveryBuffUnenhanced',
  'regenBuffUnenhanced',
  'tohitBuffUnenhanced',
  'runSpeedUnenhanced',
] as const;

describe('groupEffectsByCategory: <base>Unenhanced split slots', () => {
  it('registers no *Unenhanced key — the resolution is the only way these rows exist', () => {
    const registered = Object.keys(EFFECT_REGISTRY).filter((k) => k.endsWith('Unenhanced'));
    expect(registered).toEqual([]);
  });

  it('resolves every shipped slot through its base key, and groups it there', () => {
    for (const slot of SHIPPED_SLOTS) {
      const base = slot.slice(0, -'Unenhanced'.length);
      expect(EFFECT_REGISTRY[base], `${base} must be registered for ${slot} to resolve`).toBeDefined();

      const got = byKey({ [slot]: 1 }).get(slot);
      expect(got, `${slot} was dropped`).toBeDefined();
      expect(got!.effectKey).toBe(base);
      expect(got!.fromSplitSlot).toBe(true);
      expect(got!.category).toBe(EFFECT_REGISTRY[base].category);
    }
  });

  it('shows both halves of a split pair as two rows, never one', () => {
    const rows = byKey({ regenBuff: 1, regenBuffUnenhanced: 1 });
    expect(rows.get('regenBuff')).toMatchObject({ effectKey: 'regenBuff', fromSplitSlot: false });
    expect(rows.get('regenBuffUnenhanced')).toMatchObject({ effectKey: 'regenBuff', fromSplitSlot: true });
  });

  it('leaves an ordinary key resolving through itself', () => {
    expect(byKey({ regenBuff: 1 }).get('regenBuff')).toMatchObject({
      effectKey: 'regenBuff',
      fromSplitSlot: false,
    });
  });

  it('still drops a slot whose base is not registered, rather than inventing a row', () => {
    expect(EFFECT_REGISTRY['notARealEffect']).toBeUndefined();
    expect(byKey({ notARealEffectUnenhanced: 1 }).size).toBe(0);
    expect(byKey({ Unenhanced: 1 }).size).toBe(0);
  });
});
