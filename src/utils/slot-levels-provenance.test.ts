import { beforeAll, describe, expect, it } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data';
import { createGenericIOEnhancement } from '@/data/enhancement-registry';
import { ensureSlotOrderPopulated, hasPackedSlotLevels } from '@/utils/slot-levels';
import type { Build } from '@/types';

/**
 * The stamp SITES for `slotOrder`'s `levelSource` — DATA-GAP MBDEXPORT-21.
 *
 * `mids-export-slot-claim.test.ts` is the shared half: it hand-builds a `slotOrder` and grades
 * what the WRITER does with each provenance, byte-identical in both repos because that identity
 * is the statement that the two writers agree. It therefore says nothing about whether the fills
 * stamp themselves correctly, which is where a wrong stamp would actually come from.
 *
 * This file is that half, and it is per-repo rather than shared for two honest reasons rather
 * than one convenient one: `ensureSlotOrderPopulated` still takes SLOT-3's `levelUpMode` argument
 * here and not in canonical, and the `.mbd` fixture corpus is canonical-only. A shared file could
 * only cover both by pretending that fork is closed where it is not.
 *
 * So canonical's twin carries one case this cannot: that MBDIMPORT's `seedSlotOrderFromFile`
 * stamps AUTHORED, graded against a real Mids file. The seed code itself is byte-identical
 * across the two repos and canonical mutation-tested it, which is the same standing this file's
 * sibling arguments have — the fixtures that measure a thing live where the corpus does.
 */

function oneSlotBuild(): Build {
  const prim = getPowerset('defender/thermal-radiation')!;
  const power = { ...prim.powers[0], level: 1, slots: [null, createGenericIOEnhancement('Accuracy', 50)] };
  return {
    id: 'test',
    name: 'Test',
    serverId: 'homecoming',
    archetype: { id: 'defender', name: 'Defender' },
    level: 50,
    progressionMode: 'auto',
    primary: { id: 'defender/thermal-radiation', name: prim.name, powers: [power] },
    secondary: { id: 'defender/water-blast', name: '', powers: [] },
    pools: [],
    epicPool: null,
    inherents: [],
    accolades: [],
    settings: { globalIOLevel: 50, origin: 'Natural' },
    sets: {},
    incarnates: [],
    craftingChecklist: [],
    shoppingListAcquired: [],
    slotOrder: [],
  } as unknown as Build;
}

describe('where a slot level says it came from (MBDEXPORT-21)', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  /**
   * The wholesale fill. It reads the respec solver and writes the whole array at once, which is
   * a legal packing and not a record of anything the author did — and until `levelSource` it
   * wrote into the same field a real placement does, which is the entire defect.
   */
  it('ensureSlotOrderPopulated stamps its wholesale fill as packed', () => {
    const build = oneSlotBuild();
    // The `true` is SLOT-3's `levelUpMode`: off, this returns an empty map and fills nothing,
    // so the stamp is only reachable with it on. That argument is the fork, not the decision.
    ensureSlotOrderPopulated(build, true);
    expect(build.slotOrder.length).toBeGreaterThan(0);
    expect(build.slotOrder.every((e) => e.levelSource === 'packed')).toBe(true);
    expect(hasPackedSlotLevels(build)).toBe(true);
  });
});
