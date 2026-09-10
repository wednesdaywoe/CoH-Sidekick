import { describe, it, expect, beforeAll } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { importMidsBuild } from '@/utils/mids-import';
import { countBudgetPowerPicks } from '@/utils/build-budget';

/**
 * The import dialog's "Powers: N" and the dashboard's `Pwr N/24` are the same claim, and
 * they used to be two different numbers: the summary tallied its own count as it resolved
 * entries and reported 31 for a build the dashboard then showed as 23 of 24 picks. It was
 * counting inherent slot-data entries, accolades and incarnate slots as powers, and
 * entries that resolved but were dropped as duplicates.
 *
 * The count now comes from the finished build. What is guarded here is that it keeps
 * coming from there — an assertion on a literal 3 would stay green if someone reinstated
 * the tally and it happened to agree on this fixture.
 */

beforeAll(async () => {
  await loadDataset('homecoming');
});

const ACCOLADES = ['The_Atlas_Medallion', 'Portal_Jockey'];

/** A build carrying every kind of entry that is NOT a power pick. */
function mbd(): string {
  return JSON.stringify({
    BuiltWith: { App: 'Mids Reborn', Version: '3.7.5.21', Database: 'Homecoming' },
    Level: '49',
    Class: 'Class_Blaster',
    Origin: 'Technology',
    Name: 'summary counts',
    PowerSets: ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
    PowerEntries: [
      { PowerName: 'Blaster_Ranged.Assault_Rifle.Burst', Level: 1 },
      { PowerName: 'Blaster_Support.Electricity_Manipulation.Charged_Brawl', Level: 1 },
      // Two spellings of one power: the second resolves, collides, and is dropped. The
      // old tally counted it, which is the "increments before it lands" half.
      { PowerName: 'Blaster_Support.Electricity_Manipulation.Havok_Punch', Level: 10 },
      { PowerName: 'Blaster_Support.Electricity_Manipulation.Havoc_Punch', Level: 10 },
      // Neither of these consumes a power pick.
      ...ACCOLADES.map((name) => ({ PowerName: `Temporary_Powers.Accolades.${name}`, Level: 50 })),
      { PowerName: 'Incarnate.Alpha.Musculature_Radial_Paragon', Level: 50 },
      // Inherents arrive as entries too, carrying slot data rather than a pick.
      { PowerName: 'Inherent.Inherent.Health', Level: 1 },
      { PowerName: 'Inherent.Inherent.Stamina', Level: 1 },
    ].map((e) => ({ ...e, StatInclude: true, SlotEntries: [] })),
  });
}

describe('Mids .mbd import — summary counts', () => {
  it('reports the picks the build actually holds, not the entries it resolved', () => {
    const result = importMidsBuild(mbd());
    expect(result.build).toBeTruthy();

    // The load-bearing assertion: the dialog's number IS the dashboard's number.
    expect(result.summary.powersImported).toBe(countBudgetPowerPicks(result.build!));

    // And that number is the three distinct powers — not the four power-ish entries, and
    // not the nine entries the file carries.
    expect(result.summary.powersImported).toBe(3);
  });

  it('counts accolades and incarnates apart from powers', () => {
    const result = importMidsBuild(mbd());

    expect(result.summary.accoladesImported).toBe(ACCOLADES.length);
    expect(result.summary.incarnatesImported).toBe(1);
    // Stated against the build, so a counter that drifted from what landed reds here.
    expect(result.build?.accolades).toHaveLength(ACCOLADES.length);
    expect(result.build?.incarnates.alpha).toBeTruthy();
  });

  /**
   * MBDIMPORT-5's other direction. The row was found on a refused power, whose slots
   * never reached a counter at all; this is the entry that reaches every counter and
   * is dropped afterwards, when a second spelling of the same power finds the first
   * already there. `enhancementsImported` then claimed pieces the build did not hold,
   * and the corpus cannot see it — none of the eight real files collides on a power
   * carrying anything, so the collision above is deliberately slotted here.
   */
  it('does not count the pieces of a colliding entry it drops', () => {
    const piece = (uid: string) => ({
      Level: 1, IsInherent: false, FlippedEnhancement: null,
      Enhancement: { Uid: uid, Grade: 'None', IoLevel: 50, RelativeLevel: 'Even', Obtained: false },
    });
    const collided = importMidsBuild(JSON.stringify({
      BuiltWith: { App: 'Mids Reborn', Version: '3.7.5.21', Database: 'Homecoming' },
      Level: '49', Class: 'Class_Blaster', Origin: 'Technology', Name: 'collision',
      PowerSets: ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
      PowerEntries: [
        {
          PowerName: 'Blaster_Support.Electricity_Manipulation.Havok_Punch',
          Level: 10, StatInclude: true, SlotEntries: [piece('Crafted_Kinetic_Combat_A')],
        },
        {
          PowerName: 'Blaster_Support.Electricity_Manipulation.Havoc_Punch',
          Level: 10, StatInclude: true,
          SlotEntries: [piece('Crafted_Kinetic_Combat_B'), piece('Crafted_Kinetic_Combat_C')],
        },
      ],
    }));

    const secondary = collided.build?.secondary?.powers ?? [];
    expect(secondary.map((p) => p.internalName)).toEqual(['Havok_Punch']);
    expect((secondary[0].slots ?? []).filter(Boolean)).toHaveLength(1);

    // One in the build, two dropped with the entry, three in the file — and the
    // dropped pair named at the enhancement level rather than only as a power warning.
    expect(collided.summary.enhancementsImported).toBe(1);
    expect(collided.summary.enhancementsFailed).toBe(2);
    expect(collided.summary.slotsImported + (collided.summary.slotsSkipped ?? 0)).toBe(3);
    expect(collided.warnings.map((w) => [w.type, w.midsName])).toEqual([
      ['power', 'Blaster_Support.Electricity_Manipulation.Havoc_Punch'],
      ['enhancement', 'Blaster_Support.Electricity_Manipulation.Havoc_Punch'],
    ]);
  });

  it('does not count an accolade the user excluded from their Mids totals', () => {
    // Stated against the included case above, which lands: a bare `toBe(0)` would pass
    // for an importer that counted no accolade at all.
    const excluded = importMidsBuild(JSON.stringify({
      BuiltWith: { App: 'Mids Reborn', Version: '3.7.5.21', Database: 'Homecoming' },
      Level: '49', Class: 'Class_Blaster', Origin: 'Technology', Name: 'excluded',
      PowerSets: ['Blaster_Ranged.Assault_Rifle', 'Blaster_Support.Electricity_Manipulation'],
      PowerEntries: [
        { PowerName: 'Blaster_Ranged.Assault_Rifle.Burst', Level: 1, StatInclude: true, SlotEntries: [] },
        { PowerName: 'Temporary_Powers.Accolades.Portal_Jockey', Level: 50, StatInclude: false, SlotEntries: [] },
      ],
    }));

    expect(excluded.summary.accoladesImported).toBe(0);
    expect(excluded.build?.accolades).toEqual([]);
  });
});
