import { beforeAll, describe, expect, it } from 'vitest';
import { loadDataset } from '@/data/dataset';
import { getPowerset } from '@/data';
import { createGenericIOEnhancement } from '@/data/enhancement-registry';
import { exportToMidsWithReport } from './mids-export';
import { carriedSlotLevels, hasPackedSlotLevels } from '@/utils/slot-levels';
import type { Build } from '@/types';

/**
 * What a `.mbd`'s `SlotEntry.Level` claims about a build authored HERE — DATA-GAP MBDEXPORT-21.
 *
 * Three code paths write a level into `slotOrder` and they do not mean the same thing. A
 * placement in the UI and MBDIMPORT's `seedSlotOrderFromFile` are records of when an author put
 * a slot somewhere; `ensureSlotOrderPopulated` fills the array wholesale from the respec solver,
 * which is a packing. Until `levelSource` existed the field could not tell them apart, so each
 * repo's writer guessed provenance from a proxy — canonical from "is there a stored entry", the
 * beta from `useUIStore.levelUpMode` — and each proxy was wrong for at least one of the three.
 *
 * The beta's proxy was wrong in the way that matters most for a file format: `levelUpMode` is
 * per-device UI state the `.skif` does not carry, so the same build exported by two people made
 * two different claims. That is why the writer takes no mode argument any more, and why these
 * cases vary the BUILD and never a flag.
 *
 * **Byte-identical in both repos, and that is the point rather than a convenience.** These seven
 * cases passing on both sides IS the statement that the two writers share one answer, which is
 * what MBDEXPORT-21's Done-when asks for. The stamp SITES live in each repo's own
 * `slot-levels-provenance.test.ts` instead: `ensureSlotOrderPopulated` still carries SLOT-3's
 * `levelUpMode` argument on the beta and not here, and the `.mbd` fixture corpus is
 * canonical-only — so a shared file could only grade those by pretending the fork is closed
 * where it is not.
 */

const SLOT_LEVEL_UNGRANTED = 47; // Mids' respec row; our schedule issues no grant here.

function levelledBuild(levelSource?: 'authored' | 'packed'): Build {
  const prim = getPowerset('defender/thermal-radiation')!;
  const power = { ...prim.powers[0], level: 1, internalName: prim.powers[0].internalName, slots: [null, createGenericIOEnhancement('Accuracy', 50)] };
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
    slotOrder: [
      {
        powerName: power.internalName,
        slotIndex: 1,
        category: 'primary',
        level: SLOT_LEVEL_UNGRANTED,
        ...(levelSource ? { levelSource } : {}),
      },
    ],
  } as unknown as Build;
}

/** The Level the file states for the one extra slot. */
function exportedSlotLevel(build: Build): number | undefined {
  const { json } = exportToMidsWithReport(build);
  const file = JSON.parse(json) as { PowerEntries: { SlotEntries: { Level: number }[] }[] };
  for (const pe of file.PowerEntries) {
    if (pe.SlotEntries.length > 1) return pe.SlotEntries[1].Level;
  }
  return undefined;
}

describe('what a slot Level claims (MBDEXPORT-21)', () => {
  beforeAll(async () => { await loadDataset('homecoming'); }, 120000);

  /**
   * The author placed it, so the file says what they placed — even at a level this server's
   * schedule does not grant, which is MBDIMPORT-14's open population. Re-solving it here is
   * what MBDEXPORT-18 closed, so this is the case that must not regress.
   */
  it('an AUTHORED level is the author\'s record, and rides out unchanged', () => {
    expect(exportedSlotLevel(levelledBuild('authored'))).toBe(SLOT_LEVEL_UNGRANTED);
  });

  /**
   * The decision. A packed level came from the same solver the writer starts from, so carrying
   * it would be handing the solver's answer back to itself while calling it the author's. The
   * file states the schedule's answer instead, and says once that it did.
   */
  it('a PACKED level is not a claim, so the file states the schedule\'s answer', () => {
    const level = exportedSlotLevel(levelledBuild('packed'));
    expect(level).not.toBe(SLOT_LEVEL_UNGRANTED);
    expect(level).toBe(exportedSlotLevel(levelledBuild('packed')));
  });

  /**
   * Absent is UNSTATED, not a default (the skill's rule: an absent axis is not a defaulted one).
   * The entry predates `levelSource`, so its provenance is unknown rather than known-packed —
   * and every build already in the wild was exported by carrying, so carrying is the reading
   * that stops making a NEW claim rather than the one that rewrites an old one.
   */
  it('an UNSTATED level is carried, because rewriting it would rewrite every build in the wild', () => {
    expect(exportedSlotLevel(levelledBuild(undefined))).toBe(SLOT_LEVEL_UNGRANTED);
  });

  it('hasPackedSlotLevels answers off the build, and only for packed entries', () => {
    expect(hasPackedSlotLevels(levelledBuild('packed'))).toBe(true);
    expect(hasPackedSlotLevels(levelledBuild('authored'))).toBe(false);
    expect(hasPackedSlotLevels(levelledBuild(undefined))).toBe(false);
  });

  /**
   * ONCE, not per slot. SLOT-3 asked for exactly this and gated it on a UI mode; it is derived
   * from the build now, so it travels with the file rather than with the device.
   */
  it('the synthetic-levels disclosure is emitted once, and only when something is packed', () => {
    const packed = exportToMidsWithReport(levelledBuild('packed')).warnings
      .filter((w) => w.detail.includes('legal packing'));
    expect(packed).toHaveLength(1);
    expect(exportToMidsWithReport(levelledBuild('authored')).warnings
      .filter((w) => w.detail.includes('legal packing'))).toEqual([]);
  });

  /**
   * The per-slot warning MBDEXPORT-18 added, kept honest against the disclosure above: a
   * carried level the schedule cannot grant still says so per slot, because that is a claim
   * about THIS slot rather than about how the build was planned.
   */
  it('a carried level the schedule does not grant still warns per slot', () => {
    const warnings = exportToMidsWithReport(levelledBuild('authored')).warnings;
    expect(warnings.some((w) => w.detail.includes('slot schedule does not grant'))).toBe(true);
  });

  /** The overlay is the only difference between the two maps, so a packed build has none. */
  it('carriedSlotLevels overlays authored entries and skips packed ones', () => {
    const key = [...carriedSlotLevels(levelledBuild('authored')).keys()][0];
    expect(carriedSlotLevels(levelledBuild('authored')).get(key)?.[1]).toBe(SLOT_LEVEL_UNGRANTED);
    expect(carriedSlotLevels(levelledBuild('packed')).get(key)?.[1]).not.toBe(SLOT_LEVEL_UNGRANTED);
  });
});
