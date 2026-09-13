import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import type { DatasetId } from '@/data/dataset';
import { getBoostIndex, getCommonIOLevels, craftedCommonIOLevel } from '@/data/boost-index';
import { getIOValueAtLevel } from '@/utils/calculations/enhancement-values';
import type { EnhancementSchedule } from '@/data/enhancement-curves';

/**
 * The levels a generic IO can be crafted at are the ones the export names, on
 * every dataset (BOOST-6).
 *
 * The picker's spinner was `min={10} max={53}`, every integer between, typed in
 * — so it offered a level-37 Accuracy IO the game ships no recipe for, and
 * three levels past the top of the roster. Those three were not even one thing:
 * Homecoming's strength curve runs 105 entries deep and paid a real larger
 * number at 51-53, while Rebirth's and Thunderspy's stop at 50 and collapsed
 * silently. One spinner, two meanings, no export behind either.
 *
 * What this cannot see: the spinner itself. There is no DOM in this suite, so
 * the band reaching `LevelSpinner` is asserted at its source — the roster these
 * tests pin, and `craftedCommonIOLevel`, which is what the generic tab mints
 * with.
 */

const DATASET_IDS: DatasetId[] = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'];
const SCHEDULES: EnhancementSchedule[] = ['A', 'B', 'C', 'D'];

describe.each(DATASET_IDS)('Common-IO crafting levels (%s)', (datasetId) => {
  beforeAll(async () => {
    await loadDataset(datasetId);
  }, 120000);

  it('offers exactly the levels the export names a record for', () => {
    // By RECORD NAME rather than by the classified `level` field, so the two
    // reads have to agree: `Crafted_Accuracy_10` … `Crafted_Accuracy_50`.
    const named = Object.keys(getBoostIndex().entries)
      .map((name) => /^Crafted_Accuracy_(\d+)$/.exec(name))
      .filter((m): m is RegExpExecArray => m !== null)
      .map((m) => Number(m[1]))
      .sort((a, b) => a - b);

    expect(named.length).toBeGreaterThan(0);
    expect(getCommonIOLevels()).toEqual(named);
  });

  it('names no crafted common IO outside that band', () => {
    const levels = getCommonIOLevels();
    const band = new Set(levels);
    const outside = Object.entries(getBoostIndex().entries)
      .filter(([, entry]) => entry.kind === 'common-io' && entry.level != null)
      .filter(([, entry]) => !band.has(entry.level as number))
      .map(([name]) => name);
    expect(outside).toEqual([]);
  });

  it('crafts at the roster level at or below the one picked', () => {
    const levels = getCommonIOLevels();
    const top = levels[levels.length - 1];
    expect(craftedCommonIOLevel(top)).toBe(top);
    expect(craftedCommonIOLevel(levels[0] - 1)).toBe(levels[0]);
    // The three combine levels the spinner used to offer, and a level between
    // the roster's steps.
    expect(craftedCommonIOLevel(top + 3)).toBe(top);
    for (const level of levels) {
      expect(craftedCommonIOLevel(level + 1)).toBe(level === top ? top : level);
    }
  });

  it('pays nothing past the top of the roster', () => {
    const top = getCommonIOLevels()[getCommonIOLevels().length - 1];
    for (const schedule of SCHEDULES) {
      const atTop = getIOValueAtLevel(top, schedule);
      // Homecoming's class tables keep going (105 entries); the levels past the
      // roster are not recipes, so the curve must not be read there.
      for (const level of [top + 1, top + 2, top + 3]) {
        expect(getIOValueAtLevel(level, schedule), `${schedule} at ${level}`).toBe(atTop);
      }
    }
  });
});
