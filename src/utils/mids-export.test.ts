/**
 * Regression: exporting a Defender Thermal Radiation / Water Blast build to
 * Mids produced unresolvable powerset paths ("Defender_Buff.Thermal_Radiation_Set.ico")
 * because `getMidsSetName` only stripped `.png` while the datasets ship `.ico`
 * icons. Mids dropped every power in the unresolvable sets → empty build.
 */
import { readFileSync } from 'node:fs';
import { beforeAll, describe, expect, it } from 'vitest';
import { loadDataset, type DatasetId } from '@/data/dataset';
import { getPowerset, getAllIOSets } from '@/data';
import { createGenericIOEnhancement, createIOSetEnhancement, createOriginEnhancement, createSpecialEnhancement } from '@/data/enhancement-registry';
import { getSpecialRegistry } from '@/data/special-enhancements';
import { COMMON_IO_TYPES } from '@/data/enhancements';
import { getMidsUids, resolveMidsUid } from '@/data/mids-uids';
import { parseIOSetUid } from '@/utils/enhancement-uid';
import { exportToMids, exportToMidsWithReport } from './mids-export';
import { hydrateBuild } from '@/utils/build-serialization';
import type { MbdFile } from '@/utils/mids-import/types';
import type { Build, Enhancement } from '@/types';

// Minimal but realistic build skeleton; only primary/secondary paths matter here.
function buildFor(primaryId: string, secondaryId: string): Build {
  const prim = getPowerset(primaryId)!;
  const sec = getPowerset(secondaryId)!;
  return {
    id: 'test',
    name: 'Test',
    serverId: 'homecoming',
    archetype: { id: 'defender', name: 'Defender' },
    level: 50,
    progressionMode: 'auto',
    primary: { id: primaryId, name: prim.name, powers: prim.powers.map((p) => ({ ...p, level: 1, slots: [] })) },
    secondary: { id: secondaryId, name: sec.name, powers: sec.powers.map((p) => ({ ...p, level: 1, slots: [] })) },
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

/** A one-power build whose single slot holds `enh`. */
function buildWithSlot(enh: Enhancement): Build {
  const build = buildFor('defender/thermal-radiation', 'defender/water-blast');
  build.primary.powers = [{ ...build.primary.powers[0], slots: [enh] }];
  build.secondary.powers = [];
  return build;
}

/** Every UID the active dataset's Mids database carries. */
function knownUids(): Set<string> {
  const table = getMidsUids();
  const known = new Set<string>([
    ...Object.values(table.ioSetPieces).flatMap((pieces) => [...pieces]),
    ...table.genericIO,
    ...table.special,
  ]);
  known.delete('');
  return known;
}

/** The UID of the first slotted enhancement in an exported .mbd. */
function firstSlottedUid(mbd: { PowerEntries: { SlotEntries: { Enhancement: { Uid: string } | null }[] }[] }): string | null {
  for (const pe of mbd.PowerEntries) {
    for (const se of pe.SlotEntries) {
      if (se.Enhancement?.Uid) return se.Enhancement.Uid;
    }
  }
  return null;
}

describe('mids-export powerset paths', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  it('strips the .ico extension from powerset icons (thermal/water defender)', () => {
    const mbd = JSON.parse(exportToMids(buildFor('defender/thermal-radiation', 'defender/water-blast')));
    // Second segment must be the bare Mids internal name, no extension, no "_set".
    expect(mbd.PowerSets[0]).toBe('Defender_Buff.Thermal_Radiation');
    expect(mbd.PowerSets[1]).toBe('Defender_Ranged.Water_Blast');
    // No power name may carry a file extension.
    for (const pe of mbd.PowerEntries) {
      expect(pe.PowerName).not.toContain('.ico');
      expect(pe.PowerName).not.toContain('.png');
    }
  });

  it('still works for .png icons', () => {
    // Force a .png icon through the same path (future-proofing against
    // datasets that keep .png icons).
    const mbd = JSON.parse(exportToMids(buildFor('defender/thermal-radiation', 'defender/water-blast')));
    expect(mbd.PowerSets[0]).not.toMatch(/\.png/);
    expect(mbd.PowerSets[0]).not.toMatch(/\.ico/);
  });
});
/**
 * The UID corpus.
 *
 * Mids resolves a slot by UID string and, on a miss, leaves the slot empty
 * without an error — `DatabaseAPI.GetEnhancementByUIDName` returns -1 and
 * `LoadEnhancementData` returns having set nothing. So an exported UID Mids
 * doesn't recognise is invisible on both sides: our tests pass, Mids opens the
 * file, and the user's build is just missing enhancements.
 *
 * That is what shipped. The exporter derived UIDs from set display names, which
 * cannot work — the prefix is a per-set fact and Mids carries its own spellings
 * — so Numina's Convalescence, every ATO, and seven of the generic IOs came out
 * as names Mids has never heard of. These sweeps are the standing check that
 * the whole slottable population resolves, not the handful a fixture covers.
 */
describe('mids-export enhancement UIDs', () => {
  /**
   * Sets our data carries that the vendored Mids database does not, so the
   * export has nothing to name them with. Declared per set rather than tolerated
   * as a count: a new gap has to be added here on purpose, which is the only way
   * a *regression* stays distinguishable from a fork whose sets Mids never
   * shipped. Each of these leaves the list by a newer EnhDB, not by editing the
   * expectation.
   */
  const UNNAMEABLE: Record<string, string[]> = {
    homecoming: [],
    brainstorm: [],
    // Mids' Rebirth EnhDB has no record for Return From the Grave's Recharge
    // piece: its sixth member is a byte-copy of its fifth, `_E` twice. The
    // superior twin has the piece and spells it `_F`, so this is one database
    // row, not a set Mids never shipped.
    //
    // It read `piece 1` until MBDEXPORT-10, which is the same defect wearing
    // the hole one slot along: the unlettered rez proc was filed at the last
    // free slot, so piece 1 went unnamed and piece 6 went out as the rez proc.
    // A list of what we cannot name had nowhere to record something we named
    // WRONG, and the mis-bind sat next to this line for as long as it existed.
    //
    // Superior Endless Nightmare used to be listed here too, on the claim that
    // Mids had no such set. It has one — a user slotted it in Mids and exported
    // the build. What Mids does not have is our spelling: its set record reads
    // `Superior _Endless_Nightmare`, with a stray space its own piece UIDs lack,
    // and the emitter's key kept it. The six pieces became nameable the moment
    // `set_key` dropped whitespace (MBDEXPORT-2), which is what reds this list.
    rebirth: ['return_from_the_grave piece 6'],
    // Thunderspy ships KB and the Primalist ATOs, and no Mids database holds any of the
    // three. This comment was for a while the ONLY place in either repo recording that our
    // Thunderspy UID table is Mids' Generic database — while the emitter next to it claimed
    // Thunderspy shipped its own — so the repo held a fact and its contradiction in two
    // files neither of which cited the other. The provenance is now stated at every site
    // that reads the file (MBDEXPORT-2), and this list is just the residue: three sets that
    // exist on one fork and in no Mids database ever.
    thunderspy: [
      ...[1, 2, 3, 4, 5, 6].flatMap((n) => [
        `kb piece ${n}`,
        `primalists_nature piece ${n}`,
        `superior_primalists_nature piece ${n}`,
      ]),
    ],
  };

  const DATASETS: DatasetId[] = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'];

  for (const datasetId of DATASETS) {
    describe(datasetId, () => {
      beforeAll(async () => { await loadDataset(datasetId); }, 120000);

      it('names every IO set piece the planner can slot with a UID Mids has', () => {
        // Mids matches by substring, so a UID that is merely a *prefix* of a real
        // one silently resolves to a different enhancement. Membership in the
        // roster is what rules that out; "we emitted something" does not.
        const known = knownUids();
        const missing: string[] = [];
        for (const set of Object.values(getAllIOSets())) {
          for (const [index, piece] of set.pieces.entries()) {
            const enh = createIOSetEnhancement(set, piece, index, { attuned: false, level: 50 });
            const mbd = JSON.parse(exportToMids(buildWithSlot(enh)));
            const uid = firstSlottedUid(mbd);
            if (!uid || !known.has(uid)) missing.push(`${set.id} piece ${piece.num}`);
          }
        }
        expect(missing.sort()).toEqual([...UNNAMEABLE[datasetId]].sort());
      });

      it('names every common IO with a UID Mids has', () => {
        const known = knownUids();
        const missing = COMMON_IO_TYPES.filter((stat) => {
          const uid = firstSlottedUid(JSON.parse(exportToMids(buildWithSlot(createGenericIOEnhancement(stat, 50)))));
          return !uid || !known.has(uid);
        });
        expect(missing).toEqual([]);
      });

      it('names every exotic (Hamidon/Titan/Hydra/D-Sync) enhancement with a UID Mids has', () => {
        const known = knownUids();
        const missing: string[] = [];
        for (const category of ['hamidon', 'titan', 'hydra', 'd-sync'] as const) {
          for (const [id, def] of Object.entries(getSpecialRegistry(category))) {
            const enh = createSpecialEnhancement(id, def, category);
            const uid = firstSlottedUid(JSON.parse(exportToMids(buildWithSlot(enh))));
            if (!uid || !known.has(uid)) missing.push(`${category}/${id}`);
          }
        }
        expect(missing).toEqual([]);
      });

    });
  }
});

/**
 * Round trip.
 *
 * Both directions read the same generated table, so this is checking that the
 * table is an injection: no two set pieces may share a UID, or the build that
 * comes back is not the build that went out. Nothing here would catch the two
 * halves agreeing on a UID Mids doesn't have — the sweeps above own that.
 */
describe('mids-export → mids-import round trip', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  it('returns every set piece to the same set and piece number', () => {
    const drift: string[] = [];
    for (const set of Object.values(getAllIOSets())) {
      for (const [index, piece] of set.pieces.entries()) {
        const enh = createIOSetEnhancement(set, piece, index, { attuned: false, level: 50 });
        const uid = firstSlottedUid(JSON.parse(exportToMids(buildWithSlot(enh))));
        if (!uid) continue;
        const back = resolveMidsUid(uid);
        if (back?.setId !== (set.id ?? '').replace(/-/g, '') || back?.pieceNum !== piece.num) {
          drift.push(`${set.id}#${piece.num} → ${uid} → ${back?.setId}#${back?.pieceNum}`);
        }
      }
    }
    expect(drift).toEqual([]);
  });

  /**
   * The text-parsing fallback is what handles a UID the table has never seen —
   * another fork's file, a set newer than the vendored EnhDB. It agrees with
   * the table almost everywhere; where it doesn't, the table has to win, and
   * these are the cases proving it does.
   */
  it('resolves the UIDs whose text lies about which set they belong to', () => {
    // Mids renamed Shrapnel to Artillery and kept the old piece UIDs.
    expect(resolveMidsUid('Crafted_Shrapnel_A')).toEqual({ setId: 'artillery', pieceNum: 1 });
    expect(parseIOSetUid('Crafted_Shrapnel_A')?.setId).toBe('shrapnel');
    // Exploit Weakness's third piece ends in a lowercase letter.
    expect(resolveMidsUid('Crafted_Exploit_Weakness_c')).toEqual({ setId: 'exploit_weakness', pieceNum: 3 });
    expect(parseIOSetUid('Crafted_Exploit_Weakness_c')?.pieceNum).toBe(6);
  });
});

/**
 * The build a user actually exported, from the bug report that started this.
 *
 * The first fix made Mids open the file at all (the powerset paths carried a
 * `.ico` extension, so every power landed in an unresolvable set). What came
 * back after it was still missing thirteen slotted enhancements, all four
 * Fitness inherents with the eight uniques in them, the alpha slot, the
 * character's origin, and every slot's placement level. Each of those is a
 * separate omission with its own way of looking like nothing is wrong, so each
 * gets its own assertion here.
 */
describe('mids-export regression: therm/water defender', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  function exported() {
    const raw = readFileSync(new URL('./mids-fixtures/therm-water-defender.skif', import.meta.url), 'utf8');
    const build = hydrateBuild(JSON.parse(raw).build);
    const { json, warnings } = exportToMidsWithReport(build);
    return { mbd: JSON.parse(json) as MbdFile, warnings };
  }

  it('names every enhancement in the build', () => {
    const { mbd, warnings } = exported();
    expect(warnings).toEqual([]);
    const slotted = mbd.PowerEntries.flatMap((pe) => pe.SlotEntries)
      .filter((se) => se.Enhancement !== null);
    // Every filled slot in the source build; Boxing's lone slot is empty there.
    expect(slotted).toHaveLength(94);
    expect(slotted.every((se) => resolveMidsUid(se.Enhancement!.Uid)
      || getMidsUids().genericIO.includes(se.Enhancement!.Uid)
      || getMidsUids().special.includes(se.Enhancement!.Uid))).toBe(true);
  });

  it('carries the Fitness inherents and what is slotted in them', () => {
    const { mbd } = exported();
    const byName = new Map(mbd.PowerEntries.map((pe) => [pe.PowerName, pe]));
    expect(byName.get('Inherent.Fitness.Health')?.SlotEntries.map((s) => s.Enhancement?.Uid))
      .toEqual(['Crafted_Miracle_F', 'Crafted_Panacea_F']);
    expect(byName.get('Inherent.Fitness.Stamina')?.SlotEntries.map((s) => s.Enhancement?.Uid))
      .toEqual(['Crafted_Performance_Shifter_A', 'Crafted_Performance_Shifter_B',
                'Crafted_Performance_Shifter_F', 'Crafted_Power_Transfer_F']);
  });

  /**
   * Verified against Mids Reborn 3.8.6 / DB 2026.5.1337 running under Wine.
   *
   * `CharacterBuildData.SortGridPowers` allocates `new PowerEntry[tList.Count]`
   * — one slot per `eGridType.Inherent` power the FILE carries — then writes each
   * power to a hardcoded index: Brawl 0, Sprint 1, Rest 2, Swift 3, Hurdle 4,
   * Health 5, Stamina 6. Sending only the four Fitness powers gives a
   * four-element array, so Swift lands at [3] and Hurdle throws IndexOutOfRange.
   *
   * The throw is caught in `LoadBuild`, which abandons the rest of the load: the
   * inherent grid is never assembled and `Validate()` never runs. On screen that
   * reads as inherents that are present but have lost every slot — not as an
   * error, which is why it survived a round of fixes.
   *
   * So all seven ship, always, empty ones included.
   */
  it("sends all seven of Mids' inherent-grid powers, so its fixed indices are in range", () => {
    const { mbd } = exported();
    expect(mbd.PowerEntries.slice(mbd.LastPower, mbd.LastPower + 7).map((pe) => pe.PowerName))
      .toEqual([
        'Inherent.Inherent.Brawl', 'Inherent.Inherent.Sprint', 'Inherent.Inherent.Rest',
        'Inherent.Fitness.Swift', 'Inherent.Fitness.Hurdle',
        'Inherent.Fitness.Health', 'Inherent.Fitness.Stamina',
      ]);
  });

  /**
   * MBDEXPORT-20. Not one of the seven above — `SortGridPowers` indexes those by
   * position and this one Mids addresses by name — and it had no arm at all, so every
   * exported build arrived at Mids without the power its own archetype is built around.
   *
   * Asserted by the name the EXPORT files it under. Our roster synthesises
   * `Inherent.<Archetype>.<Name>` for this power and Mids resolves none of that, so a
   * case that read the name off our own side would have passed throughout the defect.
   */
  it("carries the archetype inherent, under the export's own Inherent.Inherent name", () => {
    const { mbd } = exported();
    // Directly after the seven-power grid, which is where the writer puts it. Position is
    // free here — everything past `LastPower` is addressed by name — so this grades the
    // name and the presence, and pins the placement only so a move is deliberate.
    expect(mbd.PowerEntries[mbd.LastPower + 7].PowerName).toBe('Inherent.Inherent.Vigilance');
    expect(mbd.PowerEntries[mbd.LastPower + 7].Level).toBe(1);
  });

  it('carries the alpha slot', () => {
    const { mbd } = exported();
    expect(mbd.PowerEntries.map((pe) => pe.PowerName))
      .toContain('Incarnate.Alpha.Cardiac_Radial_Paragon');
  });

  it('counts the level-up run in LastPower, so entry LastPower is the first granted one', () => {
    const { mbd } = exported();
    // `LastPower` is a COUNT: entry `LastPower` is already auto-granted, and the last
    // pick sits one before it. It counts PICK SLOTS, not named powers — a build that
    // skipped its level-49 pick still has to keep the inherents out of that slot.
    expect(mbd.PowerEntries[mbd.LastPower - 1].PowerName).toBe('Pool.Fighting.Tough');
    // `Temporary_Powers.` is the accolades (MBDEXPORT-12); Mids files them past `LastPower`
    // in its own corpus, which is where the reader takes a name rather than an index.
    const GRANTED = ['Inherent.', 'Incarnate.', 'Temporary_Powers.'];
    expect(mbd.PowerEntries.slice(mbd.LastPower).every(
      (pe: { PowerName: string }) => GRANTED.some((prefix) => pe.PowerName.startsWith(prefix)),
    )).toBe(true);
  });

  /**
   * Mids assigns `CurrentBuild.Powers[powerIndex] = powerEntry` — the array is
   * positional, so entry i is the i-th level-up pick. Grouping by powerset gave
   * every power the right Level label in a grid position nobody picked: Hasten
   * at 8 rendered after Geyser at 30, Shark Skin at 35 after Tough at 49.
   */
  it('lays the chosen powers out along the pick schedule', () => {
    const { mbd } = exported();
    const picks = mbd.PowerEntries.slice(0, mbd.LastPower);
    const levels = picks.map((pe) => pe.Level);
    expect(levels).toEqual([...levels].sort((a, b) => a - b));
    expect(levels).toEqual([1, 1, 2, 4, 6, 8, 10, 12, 14, 16, 18, 20, 22, 24,
                            26, 28, 30, 32, 35, 38, 41, 44, 47, 49]);
  });

  it("uses the character's origin, not a placeholder", () => {
    expect(exported().mbd.Origin).toBe('Natural');
  });

  it('dates each slot from when it was placed, not from its power', () => {
    const { mbd } = exported();
    const warmth = mbd.PowerEntries.find((pe) => pe.PowerName.endsWith('.Warmth'))!;
    // Warmth is picked at 2 and holds four slots. Stamping all four with the
    // power's level claims four slot grants at level 2, which is not a build
    // anyone can level into.
    expect(warmth.SlotEntries[0].Level).toBe(2);
    expect(new Set(warmth.SlotEntries.map((s) => s.Level)).size).toBeGreaterThan(1);
    for (const pe of mbd.PowerEntries) {
      for (const se of pe.SlotEntries) expect(se.Level).toBeGreaterThanOrEqual(pe.Level);
    }
  });
});

/**
 * Origin enhancements. Mids keeps one record per stat, spelled `Magic_*`
 * whatever the character's origin, and reads the tier off `Grade`. The old
 * export wrote the bare stat name and relied on Mids' substring matcher landing
 * on something — which it did, on whichever record happened to contain the
 * string.
 */
describe('mids-export origin enhancements', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  it('names every origin enhancement tier and stat with a UID Mids has', () => {
    const known = new Set(getMidsUids().origin);
    const missing: string[] = [];
    for (const tier of ['TO', 'DO', 'SO'] as const) {
      for (const stat of COMMON_IO_TYPES) {
        const enh = createOriginEnhancement(stat, tier);
        const mbd = JSON.parse(exportToMids(buildWithSlot(enh)));
        const uid = firstSlottedUid(mbd);
        if (!uid || !known.has(uid)) missing.push(`${tier} ${stat}`);
      }
    }
    expect(missing).toEqual([]);
  });

  /**
   * `Grade` is Mids' `eEnhGrade`, whose members are `TrainingO`, `DualO` and
   * `SingleO` — never our `TO`/`DO`/`SO`. This assertion used to read `'SO'`,
   * under a title claiming that was "where Mids reads it", and it was green for
   * as long as it was wrong: Mids `Enum.Parse`s the field, throws inside
   * `LoadBuild`, and refuses the entire build. See DATA-GAP MBDEXPORT-4, and the
   * corpus-graded version of this claim in canonical's
   * `mids-import/mbd-roundtrip.test.ts` — this repo has no `fixtures/` tree.
   *
   * A hand-fed test states its author's model of the format. That is exactly how
   * MBDIMPORT-6 survived on the reader's side too, passing `'SO'` by hand
   * through a branch no real file could reach.
   */
  it('carries the tier in Grade, spelled as Mids spells it', () => {
    const mbd = JSON.parse(exportToMids(buildWithSlot(createOriginEnhancement('Accuracy', 'SO'))));
    const slot = mbd.PowerEntries.flatMap((pe: { SlotEntries: unknown[] }) => pe.SlotEntries)
      .find((se: { Enhancement: { Grade: string } | null }) => se.Enhancement)!;
    expect(slot.Enhancement.Grade).toBe('SingleO');
  });
});

/**
 * MBDEXPORT-16 — which sets a VEAT's file is FILED under.
 *
 * The corpus Night Widow grades the case where both roles hold a branch pick, and it is the
 * only VEAT the corpus has. These are the arms it cannot reach: a build that specialised in
 * one role only, a build that never specialised, and the boundary the round trip reds on if
 * the header ever drags the picks along with it.
 *
 * Hand-fed, and therefore stating this author's model of the format rather than Mids' —
 * which is how MBDIMPORT-6 survived. What keeps that honest here is that the claim being
 * modelled is OURS (which of the build's own sets the header names), and Mids' half of it is
 * graded against Mids' own bytes next door in `mbd_writer_roundtrip.rs`.
 */
describe('mids-export — a VEAT branch is one choice with two sets', () => {
  const BASE_PRIMARY = 'arachnos-widow/widow-training';
  const BASE_SECONDARY = 'arachnos-widow/teamwork';
  const BRANCH_PRIMARY = 'arachnos-widow/night-widow-training';
  const BRANCH_SECONDARY = 'arachnos-widow/widow-teamwork';

  /** A Widow holding one pick per named set, each filed under the set it came from. */
  function widow(...setIds: string[]): Build {
    const pick = (setId: string) => {
      const set = getPowerset(setId)!;
      return { ...set.powers[0], powerSet: setId, level: 1, slots: [null] };
    };
    const inRole = (role: 'primary' | 'secondary', baseId: string) => ({
      id: baseId,
      name: getPowerset(baseId)!.name,
      powers: setIds
        .filter((id) => (role === 'primary') === (id === BASE_PRIMARY || id === BRANCH_PRIMARY))
        .map(pick),
    });
    return {
      id: 'test', name: 'Widow', serverId: 'homecoming',
      archetype: { id: 'arachnos-widow', name: 'Arachnos Widow' },
      level: 50, progressionMode: 'auto',
      primary: inRole('primary', BASE_PRIMARY),
      secondary: inRole('secondary', BASE_SECONDARY),
      pools: [], epicPool: null, inherents: [], accolades: [],
      settings: { globalIOLevel: 50, origin: 'Natural' },
      sets: {}, incarnates: [], craftingChecklist: [], shoppingListAcquired: [], slotOrder: [],
    } as unknown as Build;
  }

  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  it('names the base sets for a Widow that never specialised', () => {
    const mbd = JSON.parse(exportToMids(widow(BASE_PRIMARY, BASE_SECONDARY))) as MbdFile;
    expect(mbd.PowerSets.slice(0, 2))
      .toEqual(['Widow_Training.Widow_Training', 'Teamwork.Teamwork']);
  });

  it('names BOTH branch sets when only one role holds a branch pick', () => {
    // Specialising is what put that pick there, and the archetype pairs the branch's two
    // sets — so a build whose branch evidence is all in the secondary is still filed under
    // the branch primary. Mids has no half-branched header to write.
    const mbd = JSON.parse(exportToMids(widow(BASE_PRIMARY, BRANCH_SECONDARY))) as MbdFile;
    expect(mbd.PowerSets.slice(0, 2))
      .toEqual(['Widow_Training.Night_Widow_Training', 'Teamwork.Widow_Teamwork']);
  });

  it('leaves every pick under the set it was filed in', () => {
    // The header moves and the picks do not. Dragging them along renames a base pick into
    // the branch set, which is a power Mids will not bind — the loudest way to "fix" this row
    // wrongly, and what the round trip reds on when the two are wired together.
    const mbd = JSON.parse(exportToMids(widow(BASE_PRIMARY, BRANCH_PRIMARY, BASE_SECONDARY))) as MbdFile;
    const picks = mbd.PowerEntries.map((e) => e.PowerName).filter((n) => !n.startsWith('Inherent.'));
    expect(picks.filter((n) => n.startsWith('Widow_Training.Widow_Training.'))).toHaveLength(1);
    expect(picks.filter((n) => n.startsWith('Widow_Training.Night_Widow_Training.'))).toHaveLength(1);
    expect(picks.filter((n) => n.startsWith('Teamwork.Teamwork.'))).toHaveLength(1);
  });

  it('reports a build holding two branches rather than picking one', () => {
    const { json, warnings } = exportToMidsWithReport(
      widow(BRANCH_PRIMARY, 'arachnos-widow/fortunata-teamwork'),
    );
    expect(warnings.map((w) => w.detail).join('\n')).toMatch(/holds powers from 2 branches/);
    expect((JSON.parse(json) as MbdFile).PowerSets.slice(0, 2))
      .toEqual(['Widow_Training.Widow_Training', 'Teamwork.Teamwork']);
  });
});
