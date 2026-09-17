// MUST be first: the store caches its persist storage at module-eval time.
import '@/test/localstorage-polyfill';
import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { useUIStore } from '@/stores/uiStore';
import { enhancementLevelRange } from '@/utils/calculations';
import { formatSignedOffset } from '@/components/ui/LevelSpinner';

/**
 * The picker's level-offset spinner straddled two mechanics through ONE stored
 * number, clamped `Math.max(0, Math.min(5, level))` — a hardcoded domain for a
 * range the export owns, sitting across an axis boundary the codebase documents
 * at length everywhere else (`enhancementLevelAxis`: boosters are unsigned IO
 * combines, relative level is a signed SO/special offset from different bins).
 *
 * It broke in both directions. Reported 2026-09-17:
 *
 *  - Slot a special, and the tab's narrower range clamps the shared value down.
 *    Return to an IO and nothing widens it back, because the picker's fold only
 *    fires when a value is OUT of range and +3 is legal for a booster. So the
 *    IO silently takes +3 where the user last chose +5, and the only cue is a
 *    number in the header they have no reason to re-read.
 *  - `Math.max(0, …)` floored the entire NEGATIVE half. The spinner offered -3
 *    on Homecoming and -9 on Rebirth, every one of which was stored as 0 — an
 *    under-level SO could not be slotted from the picker at all, and the dial
 *    said it had been.
 *
 * So the value is per-axis now, and the domain comes off the curves.
 */

beforeAll(async () => {
  await loadDataset('homecoming');
});

describe('picker level offset', () => {
  it('keeps a below-even relative level instead of flooring it to even', () => {
    const { min } = enhancementLevelRange('origin');
    expect(min, 'homecoming below-curve should reach -3').toBeLessThan(0);

    useUIStore.getState().setGlobalRelativeLevel(min);
    expect(useUIStore.getState().globalRelativeLevel).toBe(min);
  });

  it('does not let the narrower axis eat the wider one', () => {
    // The reported sequence: +5 on an IO, slot a special, come back to the IO.
    const boosterMax = enhancementLevelRange('io-set').max;
    const relativeMax = enhancementLevelRange('special').max;
    expect(relativeMax, 'the axes must actually differ for this to be a test').toBeLessThan(boosterMax);

    useUIStore.getState().setGlobalBoostLevel(boosterMax);
    useUIStore.getState().setGlobalRelativeLevel(relativeMax);

    expect(useUIStore.getState().globalBoostLevel).toBe(boosterMax);
  });

  it('takes both domains from the dataset rather than a hardcoded 0..5', () => {
    const booster = enhancementLevelRange('io-set');
    const relative = enhancementLevelRange('origin');

    useUIStore.getState().setGlobalBoostLevel(booster.max + 10);
    expect(useUIStore.getState().globalBoostLevel).toBe(booster.max);
    useUIStore.getState().setGlobalBoostLevel(-4);
    expect(useUIStore.getState().globalBoostLevel, 'no such thing as a negative combine').toBe(0);

    useUIStore.getState().setGlobalRelativeLevel(relative.min - 10);
    expect(useUIStore.getState().globalRelativeLevel).toBe(relative.min);
    useUIStore.getState().setGlobalRelativeLevel(relative.max + 10);
    expect(useUIStore.getState().globalRelativeLevel).toBe(relative.max);
  });
});

describe('signed offset display', () => {
  /**
   * `+-2`, live in the picker the moment the clamp above stopped flooring the
   * negative half. Two bugs in the same control had been covering each other: the
   * sign was pasted on unconditionally, which nothing could reveal while no value
   * below 0 could ever be stored.
   */
  it('signs a penalty with its own minus, not a pasted plus', () => {
    expect(formatSignedOffset(-2)).toBe('-2');
    expect(formatSignedOffset(-1)).toBe('-1');
  });

  it('keeps +0, which is how Mids spells an even offset', () => {
    expect(formatSignedOffset(0)).toBe('+0');
  });

  it('still marks a boost as positive', () => {
    expect(formatSignedOffset(5)).toBe('+5');
  });
});
