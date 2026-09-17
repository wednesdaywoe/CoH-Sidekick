import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { mapEnhancementUid } from './mappers';
import { enhancementLevelMultiplier } from '@/utils/calculations';
import type { Enhancement, OriginEnhancement } from '@/types';

/**
 * Mids `RelativeLevel` (the `eEnhRelative` enum) runs MinusThree..PlusFive, but
 * the importer's lookup table only listed `PlusOne`..`PlusFive`. Every `Minus*`
 * fell through the `?? 0` and imported as even.
 *
 * That silently overstated imported builds, and SOs are where it bites: a
 * levelling build is full of them, and on Homecoming one three levels under
 * your combat level is worth x0.70. Specials had the same hole, but a build
 * carrying under-level Hamidons is a far rarer thing than one carrying red SOs.
 *
 * **These cases used to pass `'SO'` and a bare `'Damage'`, and no .mbd contains
 * either.** Mids grades an SO `SingleO` and names the piece `Magic_Damage`, so
 * this suite verified the fix through a door no file can open — the origin
 * branch it exercised was unreachable, and every origin enhancement in a real
 * build was being lost while these stayed green (MBDIMPORT-6). The UIDs and
 * grades below are now the ones the corpus actually carries.
 */

beforeAll(async () => {
  await loadDataset('homecoming');
});

/**
 * `tier` and `origin` sit on `OriginEnhancement` alone, so reading either off the union is a
 * type error, and asserting the discriminator with `expect` narrows nothing — vitest's
 * matchers are not assertion functions. Narrow by throwing instead, which is the assertion
 * these two cases were reaching for anyway: the MBDIMPORT-6 bug WAS an origin UID falling
 * through to the IO-set parser, so a piece that comes back as anything but `'origin'` is the
 * regression, not a typing inconvenience.
 */
function asOrigin(enhancement: Enhancement | null, uid: string): OriginEnhancement {
  if (enhancement?.type !== 'origin') {
    throw new Error(
      `${uid} mapped to ${enhancement ? `type '${enhancement.type}'` : 'nothing'}, not an origin enhancement`
    );
  }
  return enhancement;
}

const RELATIVE_LEVELS: Array<[string, number]> = [
  ['MinusThree', -3],
  ['MinusTwo', -2],
  ['MinusOne', -1],
  ['Even', 0],
  ['PlusOne', 1],
  ['PlusTwo', 2],
  ['PlusThree', 3],
];

describe('Mids import — relative level', () => {
  it.each(RELATIVE_LEVELS)('reads an SO at %s as %i', (relativeLevel, expected) => {
    const { enhancement } = mapEnhancementUid('Magic_Damage', 49, relativeLevel, 'SingleO');
    expect(enhancement, `SO at ${relativeLevel} did not map`).toBeTruthy();
    // 0 is stored as undefined so an even slot stays slim on the wire.
    expect(enhancement!.boost ?? 0).toBe(expected);
  });

  it('the negative half actually reaches the calculation', () => {
    const under = mapEnhancementUid('Magic_Damage', 49, 'MinusThree', 'SingleO').enhancement!;
    const even = mapEnhancementUid('Magic_Damage', 49, 'Even', 'SingleO').enhancement!;
    expect(enhancementLevelMultiplier(under)).toBeCloseTo(0.7, 6);
    expect(enhancementLevelMultiplier(even)).toBeCloseTo(1, 6);
    // The bug: flooring MinusThree to Even made an out-levelled SO read as a
    // fresh one — a 43% overstatement of that slot's contribution.
    expect(enhancementLevelMultiplier(even) / enhancementLevelMultiplier(under))
      .toBeCloseTo(1 / 0.7, 5);
  });

  it('reads the other two tiers Mids grades', () => {
    // TrainingO and DualO reached no branch at all before: they matched neither
    // the special check above nor the `'TO'`/`'DO'` the origin branch tested for,
    // and fell through to the IO-set parser as "IO set not found: magic_damage".
    const to = asOrigin(mapEnhancementUid('Magic_Damage', 49, 'Even', 'TrainingO').enhancement, 'TrainingO');
    const dual = asOrigin(mapEnhancementUid('Magic_Damage', 49, 'Even', 'DualO').enhancement, 'DualO');
    expect([to.tier, dual.tier]).toEqual(['TO', 'DO']);
  });

  it('keeps the origin the piece names, on the tier that has one', () => {
    // `Magic_Damage` is a Magic SO; the origin half of the UID was thrown away
    // and `undefined` passed in its place.
    const so = asOrigin(mapEnhancementUid('Magic_Damage', 49, 'Even', 'SingleO').enhancement, 'SingleO');
    expect(so.origin).toBe('Magic');
  });

  it('carries the negative onto specials too', () => {
    const { enhancement } = mapEnhancementUid('Hamidon_Damage_Accuracy', 49, 'MinusTwo', 'SingleO');
    expect(enhancement, 'special did not map').toBeTruthy();
    expect(enhancement!.boost).toBe(-2);
  });

  it('an empty slot has no level offset to preserve', () => {
    // Mids writes "None" for an unfilled slot; it is not a -1.
    const { enhancement } = mapEnhancementUid('Magic_Damage', 49, 'None', 'SingleO');
    expect(enhancement!.boost ?? 0).toBe(0);
  });
});
