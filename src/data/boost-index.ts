/**
 * Boost-index facade.
 *
 * The index — every enhancement the game can name, keyed by the spelling the
 * game client prints for it, each entry pointing at the section that describes
 * it — lives in each dataset's generated module
 * (`src/data/datasets/<id>/generated/boost-index.ts`, emitted from the binary
 * export by `scripts/convert-boost-index.cjs`). This forwards reads to
 * whichever dataset is active, mirroring `enhancement-curves.ts`.
 */

import { getActiveDataset } from './dataset';
import type { BoostIndexData } from './dataset';

export type { BoostIndexData, BoostIndexEntry } from './dataset';

export function getBoostIndex(): BoostIndexData {
  return getActiveDataset().boostIndex;
}

/** Derived rosters, keyed on the index they were read from. */
const commonIOLevelCache = new WeakMap<BoostIndexData, number[]>();

/**
 * The levels a common ("generic") IO exists at, as the export names them:
 * `Crafted_Accuracy_10` through `Crafted_Accuracy_50`, nine per stat in steps of
 * five on all four datasets, and nothing above 50. (A tenth record per stat,
 * bare `Crafted_Accuracy`, is the level-scaling template and pins no level, so
 * it states nothing about the band.)
 *
 * This is the picker's level band. It used to be `min={10} max={53}` typed into
 * the spinner, which offered a level-37 Accuracy IO the game has no recipe for
 * and three levels past the top of the roster — and those three meant two
 * different things, because Homecoming's strength curve runs 105 entries deep
 * and computes a real larger number there while the forks' stop at 50 and
 * collapse. 51-53 is the pre-booster spelling of a level-50 IO with three
 * combines, which the planner already carries on its own axis as
 * `Enhancement.boost` (BOOST-6).
 *
 * Every stat family must name the same levels. One that disagrees is export
 * news rather than something to reconcile, so it throws.
 */
export function getCommonIOLevels(index = getBoostIndex()): number[] {
  const cached = commonIOLevelCache.get(index);
  if (cached) return cached;

  const byStat = new Map<string, number[]>();
  for (const entry of Object.values(index.entries)) {
    if (entry.kind !== 'common-io' || entry.level == null) continue;
    const stat = (entry.stats ?? []).join('/');
    const levels = byStat.get(stat);
    if (levels) levels.push(entry.level);
    else byStat.set(stat, [entry.level]);
  }
  if (byStat.size === 0) {
    throw new Error(`Boost index for "${index.dataset}" names no crafted common IO`);
  }

  const spellings = new Map<string, string[]>();
  for (const [stat, levels] of byStat) {
    const key = [...new Set(levels)].sort((a, b) => a - b).join(',');
    const stats = spellings.get(key);
    if (stats) stats.push(stat);
    else spellings.set(key, [stat]);
  }
  if (spellings.size > 1) {
    const shown = [...spellings].map(([key, stats]) => `${stats.join('/')}: ${key}`).join(' | ');
    throw new Error(
      `Boost index for "${index.dataset}" crafts its common IOs at different levels per stat — ${shown}`,
    );
  }

  const levels = [...spellings.keys()][0].split(',').map(Number);
  commonIOLevelCache.set(index, levels);
  return levels;
}

/**
 * The level a generic IO picked at `level` is actually crafted at — the roster
 * level at or below it, and the lowest one for anything under the floor. The
 * picker's band is shared with the set tab, where every integer in a set's own
 * range is a real recipe; this is what keeps a level carried over from there
 * from minting a common IO the game does not ship.
 */
export function craftedCommonIOLevel(level: number, levels = getCommonIOLevels()): number {
  let crafted = levels[0];
  for (const candidate of levels) {
    if (candidate > level) break;
    crafted = candidate;
  }
  return crafted;
}
