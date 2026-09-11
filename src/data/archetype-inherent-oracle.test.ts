import { describe, it, expect, beforeAll } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { DATASET_IDS, loadDataset, type Dataset } from '@/data/dataset';
import { headlineArchetypeInherentName } from '@/data/inherent-rules';
import type { Archetype } from '@/types';

/**
 * The headline archetype inherent's EXPORT name, graded against canonical — MBDEXPORT-20's
 * beta half.
 *
 * Named for the oracle rather than for the subject, because canonical carries a file about
 * the same power under `archetype-inherent-naming.test.ts` and the two ask DIFFERENT
 * questions. Canonical's asserts that its gate tie-break picks one power per archetype.
 * This one asserts that a wholly separate derivation lands on the same answers.
 *
 * This repo does not carry canonical's `Inherent.Inherent` powerset, so it cannot
 * reach this name the way canonical does. It derives it instead, in
 * `convert-archetype-inherents.cjs`, from the same `exported_powers/` tree by the
 * same two facts: the archetype's declared inherent name, and the export's own
 * `@Class_` gate.
 *
 * Two derivations of one fact is the drift shape this project keeps getting bitten
 * by, so they are GRADED against each other rather than trusted to agree. The
 * fixture is canonical's answer for all 60 archetype-fork pairs, taken from its
 * `getArchetypeInherentPowerFor`. If either side moves and the other does not, this
 * reds — which is the whole reason the beta emit was allowed to exist.
 *
 * A snapshot can go stale in one direction: canonical changes, nobody re-cuts this.
 * That is a real limit and it is why the fixture records NULLs too — the Primalist's
 * absence (INHERENT-10) is as much a pinned answer as the 59 names.
 */

const ORACLE: Record<string, Record<string, string | null>> = JSON.parse(
  readFileSync(join(__dirname, 'fixtures/archetype-inherent-oracle.json'), 'utf8'),
);

function archetypes(ds: Dataset): [string, Archetype][] {
  return Object.entries(ds.archetypes.registry) as [string, Archetype][];
}

describe.each(DATASET_IDS)('%s — the headline inherent this fork names', (forkId) => {
  let ds: Dataset;
  beforeAll(async () => { ds = await loadDataset(forkId); }, 300000);

  it("names the same power canonical does, for every archetype this fork ships", () => {
    const mine = Object.fromEntries(
      archetypes(ds).map(([id]) => [id, headlineArchetypeInherentName(id) ?? null]),
    );
    expect(mine).toEqual(ORACLE[forkId]);
  });

  it('is measuring a non-empty roster, so an absent map cannot score a pass', () => {
    // Two all-null objects satisfy the case above if the oracle is all-null too.
    // This is what stops that, on the side that can actually go empty.
    expect(Object.values(ORACLE[forkId]).filter(Boolean).length).toBeGreaterThan(8);
  });

  it('names it under Inherent.Inherent, which is the namespace Mids reads', () => {
    for (const [id] of archetypes(ds)) {
      const name = headlineArchetypeInherentName(id);
      if (!name) continue;
      // Never `Inherent.<Archetype>.<Name>` — that is our own synthesised spelling,
      // and shipping it is indistinguishable from shipping nothing.
      expect(name.startsWith('Inherent.Inherent.')).toBe(true);
    }
  });

  it('gives no two archetypes the same power', () => {
    const byName = new Map<string, string[]>();
    for (const [id] of archetypes(ds)) {
      const name = headlineArchetypeInherentName(id);
      if (!name) continue;
      byName.set(name, [...(byName.get(name) ?? []), id]);
    }
    expect([...byName].filter(([, ids]) => ids.length > 1)).toEqual([]);
  });
});
