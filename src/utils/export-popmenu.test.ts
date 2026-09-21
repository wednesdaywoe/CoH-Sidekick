/**
 * The popmenu writes the game's own name for an enhancement, on every fork.
 *
 * A `.mnu` is executed by the game, not read back by us, and `boost` answers a
 * record it does not know with a console line among seventy others. So the
 * failure mode of a wrong name is an enhancement that is simply not in the tray,
 * with nothing anywhere saying why — which is how the exporter spelled Fly
 * `Crafted_Flight` and Slow `Crafted_Slow` for eleven months while every gate
 * stayed green (POPMENU-1). The game calls them `Crafted_Fly` and
 * `Crafted_Snare`.
 *
 * The oracle here is `exported_powers/<fork>/boosts/`, read off disk — the
 * export's own roster of record names, not the generated index the exporter
 * resolves through. That is the point: an index built wrong would agree with an
 * exporter reading it, and the two together would still name nothing the game
 * has. The directory is the third party.
 *
 * Names are compared case-insensitively because the game's are: a boost set is
 * looked up in a stash table with string keys (case-insensitive by default) and
 * a boost power by `Crc32cLowerStringHash`. `Crafted_ToHit_Debuff` and the
 * export's `Crafted_ToHit_DeBuff` are the same record; `Crafted_Flight` and
 * `Crafted_Fly` are not.
 */
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { beforeAll, describe, expect, it } from 'vitest';
import { loadDataset, type DatasetId } from '@/data/dataset';
import { getAllIOSets } from '@/data';
import { getBoostIndex } from '@/data/boost-index';
import { COMMON_IO_TYPES } from '@/data/enhancements';
import {
  createGenericIOEnhancement,
  createIOSetEnhancement,
  createOriginEnhancement,
  createSpecialEnhancement,
} from '@/data/enhancement-registry';
import { getSpecialRegistry, type SpecialCategory } from '@/data/special-enhancements';
import { generatePopmenuWithReport } from './export-popmenu';
import type { Build, Enhancement, EnhancementStatType } from '@/types';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const DATASET_IDS: DatasetId[] = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'];
const SPECIAL_CATEGORIES: SpecialCategory[] = ['hamidon', 'titan', 'hydra', 'd-sync', 'prestige'];

/**
 * Every record name the export ships, lower-cased.
 *
 * One directory per boost record, named for the record — the same listing
 * `convert-boost-index.cjs` walks, read here without it.
 */
function exportedRecordNames(dataset: DatasetId): Set<string> {
  const root =
    dataset === 'homecoming'
      ? path.join(HERE, '../../exported_powers')
      : path.join(HERE, '../../exported_powers', dataset);
  return new Set(fs.readdirSync(path.join(root, 'boosts')).map((d) => d.toLowerCase()));
}

/**
 * A build holding exactly these slots, in one power.
 *
 * Deliberately not built from a real powerset: the exporter reads names and
 * slots and nothing else, and a skeleton keeps this readable on four datasets
 * whose powerset rosters differ.
 */
function buildWithSlots(...slots: Enhancement[]): Build {
  return {
    id: 'test',
    name: 'Test',
    level: 50,
    primary: { id: 'p', name: 'P', powers: [{ name: 'Test Power', slots }] },
    secondary: { id: 's', name: 'S', powers: [] },
    pools: [],
    epicPool: null,
    inherents: [],
  } as unknown as Build;
}

/** The record names a generated popmenu actually grants, in order. */
function grantedUids(build: Build): string[] {
  const { content } = generatePopmenuWithReport(build, 'Test');
  return [...content.matchAll(/boost (\S+) (\S+) (\d+)/g)].map((m) => {
    // The command names the set and the power, and for a boost record they are
    // the same name. A pair that disagreed would be a malformed command.
    expect(m[2]).toBe(m[1]);
    return m[1];
  });
}

/** The one record name a single-slot build grants, or null if it granted none. */
function grantedUid(enh: Enhancement): string | null {
  return grantedUids(buildWithSlots(enh))[0] ?? null;
}

describe.each(DATASET_IDS)('Popmenu boost records (%s)', (datasetId) => {
  let exported: Set<string>;

  beforeAll(async () => {
    await loadDataset(datasetId);
    exported = exportedRecordNames(datasetId);
  }, 120000);

  it('names a record the export ships, for every generic IO the game has', () => {
    const unknown: string[] = [];
    for (const stat of getBoostIndex().commonIoTypes) {
      const uid = grantedUid(createGenericIOEnhancement(stat as EnhancementStatType, 50));
      if (!uid || !exported.has(uid.toLowerCase())) unknown.push(`${stat} -> ${uid ?? 'nothing'}`);
    }
    expect(unknown).toEqual([]);
  });

  it('offers exactly the generic IOs the game has a record for', () => {
    // The two rosters are independent by design: the picker's is hand-written
    // and the index's is derived from the crafted boost family, and BOOST-1 kept
    // them that way on purpose so neither is checking itself. Set equality is
    // what makes that pair an oracle rather than two lists.
    //
    // It ran one short for six weeks. BOOST-1 measured the hand list at 25 of
    // the game's 26 and fixed the DERIVED side, so the engine offered Intangible
    // and the picker did not; nothing compared them. Order is presentation and
    // is deliberately not compared — the index's copy is sorted, the hand list
    // is grouped for the tab strip.
    //
    // Set equality holds on all four datasets because the crafted Intangible
    // family is on all four. Which POWERS accept one is a different question
    // and a per-fork answer (6 each on Rebirth and Thunderspy, none on
    // Homecoming or Brainstorm); that is `allowedEnhancements`' job, not this
    // list's.
    expect([...COMMON_IO_TYPES].sort()).toEqual([...getBoostIndex().commonIoTypes].sort());
  });

  it('spells the two the hand-written table got wrong', () => {
    // POPMENU-1, pinned by name in both directions: the right record, and the
    // absence of the invented one. The planner stat is not the record name with
    // `Crafted_` on the front, and these are the two places that shows.
    expect(grantedUid(createGenericIOEnhancement('Fly', 50))).toBe('Crafted_Fly');
    expect(grantedUid(createGenericIOEnhancement('Slow', 50))).toBe('Crafted_Snare');
    expect(exported.has('crafted_flight')).toBe(false);
    expect(exported.has('crafted_slow')).toBe(false);
  });

  it('names a record the export ships, for every IO set piece', () => {
    const unknown: string[] = [];
    for (const set of Object.values(getAllIOSets())) {
      for (const [index, piece] of set.pieces.entries()) {
        for (const attuned of [false, true]) {
          const enh = createIOSetEnhancement(set, piece, index, { attuned, level: 50 });
          const uid = grantedUid(enh);
          // A piece with no record is allowed — a craftable set with no attuned
          // twin is real — but only when the exporter SAYS so. `grantedUid` is
          // null there, and the conservation test below is what proves the
          // warning was raised rather than the slot dropped.
          if (uid && !exported.has(uid.toLowerCase())) {
            unknown.push(`${set.id} piece ${piece.num}${attuned ? ' attuned' : ''} -> ${uid}`);
          }
        }
      }
    }
    expect(unknown).toEqual([]);
  });

  it('names a record the export ships, for every special enhancement', () => {
    const unknown: string[] = [];
    for (const category of SPECIAL_CATEGORIES) {
      for (const [id, def] of Object.entries(getSpecialRegistry(category))) {
        const uid = grantedUid(createSpecialEnhancement(id, def, category));
        if (!uid || !exported.has(uid.toLowerCase())) {
          unknown.push(`${category}-${id} -> ${uid ?? 'nothing'}`);
        }
      }
    }
    expect(unknown).toEqual([]);
  });

  it('grants every special the dataset carries', () => {
    // The hand-written map this replaced held 62 of Homecoming's 78 records and
    // named no prestige enhancement at all, so all five dropped out of the file
    // in silence. Counted rather than spot-checked: a family added to a dataset
    // and not to the exporter is the same defect arriving again.
    for (const category of SPECIAL_CATEGORIES) {
      const ids = Object.keys(getSpecialRegistry(category));
      const granted = ids.filter((id) =>
        grantedUid(createSpecialEnhancement(id, getSpecialRegistry(category)[id], category)),
      );
      expect(granted.length).toBe(ids.length);
    }
  });

  it('accounts for every slot, as a command or as a warning', () => {
    // The conservation law this file exists for. Anything the exporter cannot
    // name has to SAY so; a slot that is neither granted nor reported is the
    // silent drop, whatever the reason for it.
    const slots: Enhancement[] = [
      createGenericIOEnhancement('Fly', 50),
      createGenericIOEnhancement('Slow', 50),
      createOriginEnhancement('Damage', 'SO'),
    ];
    const firstSet = Object.values(getAllIOSets())[0];
    slots.push(createIOSetEnhancement(firstSet, firstSet.pieces[0], 0, { attuned: false, level: 50 }));

    const { content, warnings } = generatePopmenuWithReport(buildWithSlots(...slots), 'Test');
    const granted = [...content.matchAll(/boost \S+ \S+ \d+/g)].length;
    expect(granted + warnings.length).toBe(slots.length);
    // The origin is the reported one, and it is reported as a policy rather
    // than as a missing record.
    expect(warnings.map((w) => w.kind)).toEqual(['not-grantable']);
    expect(warnings[0].power).toBe('Test Power');
    expect(warnings[0].slot).toBe(3);
  });
});

describe('Popmenu — the build that was reported', () => {
  beforeAll(async () => {
    await loadDataset('brainstorm');
  }, 120000);

  it('grants the Fly IO slotted in Swift', () => {
    // The Wailing Dead, 2026-09-21: 96 slots, two Option lines, and the only
    // command the game rejected was the Swift slot in the second one. The
    // reporter saw "the Fly IO for Swift didn't load" and reloaded the tray.
    const swift = {
      id: 'test',
      name: 'Test',
      level: 50,
      primary: { id: 'p', name: 'P', powers: [] },
      secondary: { id: 's', name: 'S', powers: [] },
      pools: [],
      epicPool: null,
      inherents: [{ name: 'Swift', slots: [createGenericIOEnhancement('Fly', 50, 5)] }],
    } as unknown as Build;

    const { content, warnings } = generatePopmenuWithReport(swift, 'Test');
    expect(warnings).toEqual([]);
    expect(content).toContain('boost Crafted_Fly Crafted_Fly 50');
    expect(content).not.toContain('Crafted_Flight');
  });
});
