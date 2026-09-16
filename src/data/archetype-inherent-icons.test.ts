import { describe, it, expect, beforeAll } from 'vitest';
import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { DATASET_IDS, loadDataset, type Dataset } from '@/data/dataset';
import { createArchetypeInherentPower } from '@/data';
import { getPowerIconPath } from '@/utils/power-icons';
import type { Archetype } from '@/types';

/**
 * An archetype inherent draws the art the export names for it, or it draws nothing.
 *
 * `createArchetypeInherentPower` used to fabricate a filename when the archetype data carried
 * no icon — `inherent_${archetype}_${power}.png` — and the reason that survived for so long is
 * that it usually RESOLVED. Nine of those invented names are byte-identical aliases of the one
 * file the export names for that whole family, because the game really does draw a single icon
 * for Defiance, Containment, Critical Hit, Gauntlet, Fury, Scourge, Assassination and both
 * Conditionings. A rule that is a no-op thirteen times looks like a rule that works.
 *
 * It was wrong the three times it mattered, and each failure was silent in its own way:
 *
 *  - **Rebirth's Guardian** asked for `inherent_guardian_resolve.png`, a name nothing has ever
 *    carried, where the export says `inherent_guardianresolve.png`. Resolve drew a placeholder
 *    from the day the fork was added.
 *  - **Thunderspy's Primalist** asked for one too. Primal Energy has no icon in any fork's data,
 *    so there is nothing to find and the placeholder is the honest answer.
 *  - **Sentinel** asked for `inherent_sentinel_opportunity.png` where the export says
 *    `inherent_targetlock.png`. Both exist and they are DIFFERENT images, so the two planners
 *    drew different art for one power and neither reported anything.
 *
 * So this grades the two halves of the fix rather than the fix itself: the data has to name a
 * file that is really there, and the factory must not invent one when the data names none.
 * Existence is the check because a wrong-but-present name is what the aliases already proved
 * we cannot catch by resolution alone — the export is the source, and `seed` in
 * `scripts/` is what put it there.
 */

const TREE = join(__dirname, '../../public/img/powers');

function archetypes(ds: Dataset): [string, Archetype][] {
  return Object.entries(ds.archetypes.registry) as [string, Archetype][];
}

describe.each(DATASET_IDS)('%s — archetype inherent art', (forkId) => {
  let ds: Dataset;
  beforeAll(async () => { ds = await loadDataset(forkId); }, 300000);

  it('names a file that is actually vendored, for every inherent that names one', () => {
    const absent: string[] = [];
    for (const [id, archetype] of archetypes(ds)) {
      const icon = archetype.inherent?.icon;
      if (!icon) continue;
      if (!existsSync(join(TREE, icon.toLowerCase()))) {
        absent.push(`${id} (${archetype.inherent?.name}): ${icon}`);
      }
    }
    expect(absent).toEqual([]);
  });

  it('invents nothing for an inherent the data gives no icon', () => {
    for (const [, archetype] of archetypes(ds)) {
      const inherent = archetype.inherent;
      if (!inherent || inherent.icon) continue;
      const power = createArchetypeInherentPower(archetype.name, inherent);
      // Undefined, not a guess — and the resolver turns that into the visible placeholder
      // rather than a 404, which is the whole point of letting it through.
      expect(power.icon).toBeUndefined();
      expect(getPowerIconPath(power.icon)).toMatch(/Unknown\.png$/);
    }
  });

  it('carries an icon for all but the inherents known to have none', () => {
    // A floor, so deleting the icons wholesale cannot pass the two cases above by leaving
    // nothing to check. The Primalist is the one archetype in any fork whose inherent the
    // export gives no art at all (INHERENT-10 is the same absence seen from the name side).
    const without = archetypes(ds)
      .filter(([, a]) => a.inherent && !a.inherent.icon)
      .map(([, a]) => a.inherent!.name);
    expect(without.length).toBeLessThanOrEqual(1);
    expect(archetypes(ds).filter(([, a]) => a.inherent?.icon).length).toBeGreaterThan(8);
  });
});
