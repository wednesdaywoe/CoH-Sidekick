import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getAllPowersets, getPowerPoolIds, getPowerPool, getAllEpicPools } from '@/data';
import { midsNameMap, midsNameRemap } from '@/data/mids-name-map';
import {
  MIDS_NAME_MAP as HOMECOMING_MAP,
  MIDS_POWERSET_ALIAS as HOMECOMING_ALIAS,
} from '@/data/datasets/homecoming/generated/mids-name-map';
import {
  MIDS_NAME_MAP as REBIRTH_MAP,
  MIDS_POWERSET_ALIAS as REBIRTH_ALIAS,
} from '@/data/datasets/rebirth/generated/mids-name-map';
import {
  MIDS_NAME_MAP as THUNDERSPY_MAP,
  MIDS_POWERSET_ALIAS as THUNDERSPY_ALIAS,
} from '@/data/datasets/thunderspy/generated/mids-name-map';
import {
  MIDS_NAME_MAP as BRAINSTORM_MAP,
  MIDS_POWERSET_ALIAS as BRAINSTORM_ALIAS,
} from '@/data/datasets/brainstorm/generated/mids-name-map';
import { findPowerByMidsName } from './mappers';
import { importMidsBuild } from '@/utils/mids-import';
import type { Power } from '@/types';

/**
 * MBDIMPORT-2 — Mids' internal-name namespace has drifted from the game's. HC rotated
 * internal names underneath stable display names, so an exact internal-name match binds
 * the WRONG power and nothing fails: Tactical Arrow's `Gymnastics` is Oil Slick Arrow in
 * the export, took Gymnastics' slots, and the entry that owned them was deduped away with
 * `warnings: []`. Stalker Willpower and Shield Defense are the same shape.
 *
 * The population below is the DERIVED map itself, not the handful of names a bug report
 * named. Homecoming carries over a hundred rows and the report found four of them; a test
 * written against the four would be green on the day the next rework lands.
 */

beforeAll(async () => {
  await loadDataset('homecoming');
});

/** Every `group.powerset` the map can be graded against, as its powers. */
function candidatesByKey(): Map<string, Power[]> {
  const out = new Map<string, Power[]>();
  const add = (path: string | undefined, powers: Power[]) => {
    const segments = (path ?? '').split('.');
    if (segments.length >= 2) out.set(`${segments[0]}.${segments[1]}`.toLowerCase(), powers);
  };
  for (const ps of Object.values(getAllPowersets())) add(ps.setPath, ps.powers);
  for (const id of getPowerPoolIds()) {
    const pool = getPowerPool(id);
    if (pool) add(pool.powers.find((p) => p.fullName)?.fullName, pool.powers);
  }
  for (const epic of Object.values(getAllEpicPools())) {
    add(epic.powers.find((p) => p.fullName)?.fullName, epic.powers);
  }
  return out;
}

/** A minimal well-formed .mbd for one archetype, primary + secondary, given entries. */
function mbd(
  cls: string,
  powerSets: string[],
  entries: Array<{ PowerName: string; Level: number }>,
  // The importer refuses a file whose database is not the loaded dataset, so a Rebirth
  // probe has to say so; everything else here is Homecoming.
  database: 'Homecoming' | 'Rebirth' = 'Homecoming',
): string {
  return JSON.stringify({
    BuiltWith: { App: 'Mids Reborn', Version: '3.7.5.21', Database: database },
    Level: '50',
    Class: cls,
    Origin: 'Natural',
    Name: 'name-map probe',
    PowerSets: powerSets,
    PowerEntries: entries.map((e) => ({ ...e, StatInclude: true, SlotEntries: [] })),
  });
}

const bound = (result: ReturnType<typeof importMidsBuild>, slot: 'primary' | 'secondary') =>
  (result.build?.[slot]?.powers ?? []).map((p) => `${p.internalName}@${p.level}`);

describe('Mids .mbd import — the derived name map', () => {
  it('resolves every row it carries, against the powerset the row is scoped to', () => {
    const candidates = candidatesByKey();
    let graded = 0;

    for (const [key, rows] of Object.entries(midsNameMap())) {
      const powers = candidates.get(key);
      if (!powers) continue; // pet/redirect sets the importer never resolves powers against
      for (const [midsName, ourName] of Object.entries(rows)) {
        // The target has to still exist — a regenerated export that renames a power again
        // must red here rather than leaving a row pointing at nothing.
        const target = powers.find((p) => p.internalName?.toLowerCase() === ourName.toLowerCase());
        expect(target, `${key}: row ${midsName} → ${ourName} names no power here`).toBeDefined();
        expect(findPowerByMidsName(powers, midsName, [{ powers, setPath: key }])).toBe(target);
        graded++;
      }
    }

    // Guards the grading itself: a lookup helper that silently matched nothing would leave
    // every assertion above unexecuted and the test green.
    expect(graded).toBeGreaterThan(50);
  });

  it('leaves a name both sides already agree on alone', () => {
    // Stated on a power in a powerset that DOES carry rows, so this is the remap declining
    // rather than a set the map never looks at.
    expect(midsNameRemap('blaster_support.tactical_arrow', 'Glue_Arrow')).toBeUndefined();

    const result = importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Tactical_Arrow'],
      [{ PowerName: 'Blaster_Support.Tactical_Arrow.Glue_Arrow', Level: 1 }]));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual(['Glue_Arrow@1']);
  });

  it('lands Tactical Arrow on both sides of the rotation — the reported build', () => {
    const result = importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Tactical_Arrow'],
      [
        { PowerName: 'Blaster_Support.Tactical_Arrow.Gymnastics', Level: 24 },
        { PowerName: 'Blaster_Support.Tactical_Arrow.Oil_Slick_Arrow', Level: 30 },
      ]));

    expect(result.warnings).toEqual([]);
    // Gymnastics is `Quickness` here and Oil Slick Arrow is `Gymnastics`; binding the
    // names as spelled put the toggle's slots on the location AoE and dropped the AoE.
    expect(bound(result, 'secondary')).toEqual(['Quickness@24', 'Gymnastics@30']);
  });

  it('unwinds Shield Defense, where all three names moved', () => {
    const result = importMidsBuild(mbd('Class_Stalker',
      ['Stalker_Melee.Martial_Arts', 'Stalker_Defense.Shield_Defense'],
      [
        { PowerName: 'Stalker_Defense.Shield_Defense.Deflection', Level: 1 },
        { PowerName: 'Stalker_Defense.Shield_Defense.Battle_Agility', Level: 4 },
        { PowerName: 'Stalker_Defense.Shield_Defense.Active_Defense', Level: 16 },
      ]));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual([
      'Active_Defense@1', 'Deflection@4', 'Battle_Agility@16',
    ]);
  });

  it('gives Willpower its rez back, and warns for the power HC removed', () => {
    const result = importMidsBuild(mbd('Class_Stalker',
      ['Stalker_Melee.Martial_Arts', 'Stalker_Defense.Willpower'],
      [
        { PowerName: 'Stalker_Defense.Willpower.Reconstruction', Level: 8 },
        { PowerName: 'Stalker_Defense.Willpower.Resurgence', Level: 47 },
      ]));

    // Mids' `Resurgence` is the self-rez, which is `Reconstruction` here.
    expect(bound(result, 'secondary')).toEqual(['Reconstruction@47']);
    // Mids' `Reconstruction` is a heal click Stalker Willpower no longer has. It must not
    // squat on the rez (it is listed first, so ordering alone would hand it the slot), and
    // it must not fall through to Regeneration's same-named power either.
    expect(result.warnings).toEqual([
      expect.objectContaining({ type: 'power', midsName: 'Stalker_Defense.Willpower.Reconstruction' }),
    ]);
  });

  it('imports Radiant Aura, which the skip list had written off as an artifact', () => {
    const result = importMidsBuild(mbd('Class_Mastermind',
      ['Mastermind_Summon.Beast_Mastery', 'Mastermind_Buff.Radiation_Emission'],
      [{ PowerName: 'Mastermind_Buff.Radiation_Emission.Radiation_Emission', Level: 2 }]));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual(['Radiant_Aura@2']);
  });

  it('keeps Ninjitsu on its own name, where HC recycled the display name', () => {
    // HC renamed Blinding Powder to "Smoke Flash" — the display name Mids' own unrelated
    // Smoke Flash still carries. A join on display alone pairs those two and hands this
    // power's slots to the wrong entry. The unlock levels (28 against 24) refuse it, and
    // no row is minted, so the name resolves to itself.
    expect(midsNameRemap('stalker_defense.ninjitsu', 'Smoke_Flash')).toBeUndefined();

    const result = importMidsBuild(mbd('Class_Stalker',
      ['Stalker_Melee.Martial_Arts', 'Stalker_Defense.Ninjitsu'],
      [{ PowerName: 'Stalker_Defense.Ninjitsu.Blinding_Powder', Level: 28 }]));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual(['Blinding_Powder@28']);
  });

  it('routes pool powers through the map too, at both Flight spellings', () => {
    // Pools resolve by exact fullName against a prebuilt lookup — a second door that never
    // calls `findPowerByMidsName`. `Pool.Flight.Afterburner` is a real fullName here, so an
    // older Mids file naming the pre-rework Afterburner walked past the map into the power
    // now displayed "Evasive Maneuvers". Both spellings are stated, and both must land:
    // asserting only the current one would pass for an importer that drops the older file.
    const fly = (name: string) => importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Tactical_Arrow', 'Pool.Flight'],
      [{ PowerName: `Pool.Flight.${name}`, Level: 14 }]));
    const pooled = (r: ReturnType<typeof importMidsBuild>) =>
      (r.build?.pools ?? []).flatMap((p) => p.powers.map((q) => `${q.internalName}@${q.level}`));

    expect(pooled(fly('Evasive_Maneuvers'))).toEqual(['Afterburner@14']);
    expect(pooled(fly('Afterburner'))).toEqual(['Fly_Boost@14']);
    // And a name neither side moved is untouched by any of it.
    expect(pooled(fly('Group_Fly'))).toEqual(['Group_Fly@14']);
  });

  it('warns when two entries land on one power instead of dropping the loser', () => {
    // Mids has spelled this power both ways across versions, and the display fallback
    // resolves the second spelling onto the same power the first already took. Each is
    // stated landing on its own first: a bare "only one power" assertion would pass for an
    // importer that dropped both, which is the shape being guarded against.

    const one = (name: string) => importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
      [{ PowerName: `Blaster_Support.Electricity_Manipulation.${name}`, Level: 10 }]));
    expect(bound(one('Havok_Punch'), 'secondary')).toEqual(['Havok_Punch@10']);
    expect(bound(one('Havoc_Punch'), 'secondary')).toEqual(['Havok_Punch@10']);


    const both = importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
      [
        { PowerName: 'Blaster_Support.Electricity_Manipulation.Havok_Punch', Level: 10 },
        { PowerName: 'Blaster_Support.Electricity_Manipulation.Havoc_Punch', Level: 10 },
      ]));

    expect(bound(both, 'secondary')).toEqual(['Havok_Punch@10']);
    expect(both.warnings).toEqual([
      expect.objectContaining({
        type: 'power',
        midsName: 'Blaster_Support.Electricity_Manipulation.Havoc_Punch',
        message: expect.stringContaining('already claimed'),
      }),
    ]);
  });

  it('unwinds Electricity Manipulation, where two names swapped outright', () => {
    const result = importMidsBuild(mbd('Class_Blaster',
      ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
      [
        { PowerName: 'Blaster_Support.Electricity_Manipulation.Lightning_Clap', Level: 28 },
        { PowerName: 'Blaster_Support.Electricity_Manipulation.Lightning_Field', Level: 20 },
      ]));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual(['Lightning_Field@28', 'Lightning_Clap@20']);
  });

});

/**
 * MBDIMPORT-7 — the map is a join, and the join has to reach the powerset first.
 *
 * It paired Mids' powersets to ours by comparing `group.set` as a string, and the group
 * segment drifts as readily as a power name does: Rebirth's Guardian secondaries are
 * `Guardian_Comp` in the export and `Guardian_Composition` in Mids. All 13 missed, the
 * generator answered each miss with a bare `continue`, and the corpus Guardian arrived
 * six enhancements light because Mids' `Groundeding_Shield` had no row to rotate it.
 *
 * The powersets themselves join here, and the two ways to spell the key both land on the
 * rows — the map is keyed by ours, the importer's retired-name check holds Mids'.
 */
describe('Mids .mbd import — a powerset whose group segment drifted', () => {
  beforeAll(async () => { await loadDataset('rebirth'); });
  afterAll(async () => { await loadDataset('homecoming'); });

  it('reaches one set of rows from either namespace', () => {
    expect(midsNameRemap('guardian_comp.atmospheric_composition', 'Groundeding_Shield'))
      .toBe('Grounding_Shield');
    expect(midsNameRemap('Guardian_Composition.Atmospheric_Composition', 'Groundeding_Shield'))
      .toBe('Grounding_Shield');
    // Mids writes the segment with trailing whitespace in real files, and a space where we
    // write an underscore (`Guardian_Composition.Stone composition`). Both are the same key.
    expect(midsNameRemap('Guardian_Composition.Atmospheric_Composition ', 'Groundeding_Shield '))
      .toBe('Grounding_Shield');
    expect(midsNameRemap('Guardian_Composition.Stone composition', "Gaia's_Blessing"))
      .toBe('Gaias_Blessing');
  });

  it('binds the power the group spelling hid, at Mids own path', () => {
    const result = importMidsBuild(mbd('Class_Guardian',
      ['Guardian_Assault.Dark_Assault', 'Guardian_Composition.Atmospheric_Composition'],
      [{ PowerName: 'Guardian_Composition.Atmospheric_Composition.Groundeding_Shield', Level: 4 }],
      'Rebirth'));

    expect(result.warnings).toEqual([]);
    expect(bound(result, 'secondary')).toEqual(['Grounding_Shield@4']);
  });

  it('refuses a name this set has reassigned, reached at Mids own path', () => {
    // The importer's retired-name check is the reader that holds the .mbd's spelling, so
    // this is the alias earning its place: ours is `Grounding_Shield` and Mids' name for
    // it is `Groundeding_Shield`, which makes a .mbd saying `Grounding_Shield` a name this
    // set has given to a different power. Keyed on Mids' path with no alias, the check
    // answers "not reassigned" and the entry ranges on into the cross-set fallbacks.
    const result = importMidsBuild(mbd('Class_Guardian',
      ['Guardian_Assault.Dark_Assault', 'Guardian_Composition.Atmospheric_Composition'],
      [{ PowerName: 'Guardian_Composition.Atmospheric_Composition.Grounding_Shield', Level: 4 }],
      'Rebirth'));

    expect(bound(result, 'secondary')).toEqual([]);
    expect(result.warnings.map((w) => w.message)).toEqual([
      "'Grounding_Shield' names a different power in this dataset, "
      + 'and the one Mids means has no counterpart here',
    ]);
  });

  it('leaves a Guardian name both sides agree on alone', () => {
    // The same set, one power over: the pairing has to widen the join, not rewrite names
    // that were never rotated.
    expect(midsNameRemap('Guardian_Composition.Atmospheric_Composition', 'Charged_Armor'))
      .toBeUndefined();
  });
});

/**
 * The alias table's own invariant, over every fork rather than the loaded one.
 *
 * An alias is a second door onto one row. If it ever named a key the map also carries,
 * that door would open on another powerset's rows and nothing would say so — which is the
 * silent mis-bind the whole map exists to end. The generator throws on it; this stands in
 * front of a hand-edit to the generated file.
 */
describe('Mids .mbd import — the powerset alias table', () => {
  const forks = {
    homecoming: [HOMECOMING_MAP, HOMECOMING_ALIAS],
    rebirth: [REBIRTH_MAP, REBIRTH_ALIAS],
    thunderspy: [THUNDERSPY_MAP, THUNDERSPY_ALIAS],
    brainstorm: [BRAINSTORM_MAP, BRAINSTORM_ALIAS],
  } as const;

  it('points every alias at rows, and shadows no key of its own map', () => {
    let graded = 0;
    for (const [fork, [map, alias]] of Object.entries(forks)) {
      for (const [midsKey, ourKey] of Object.entries(alias)) {
        expect(map[ourKey], `${fork}: alias ${midsKey} → ${ourKey} names no rows`).toBeDefined();
        expect(map[midsKey], `${fork}: alias ${midsKey} shadows a key of the map`).toBeUndefined();
        graded++;
      }
    }
    // Rebirth's four Guardian secondaries are the reason this table exists. A regeneration
    // that empties it has lost the join, not found the names agreeing.
    expect(graded).toBeGreaterThanOrEqual(4);
  });
});
