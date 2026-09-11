/**
 * `attunedOnly` — the export's own statement of which sets have no craftable
 * variant, and the only thing `isInherentlyAttuned` is allowed to read.
 *
 * The fact lives in the export's piece membership: a set's BoostList members are
 * `Crafted_*` and/or `Attuned_*` records, and a set naming no `Crafted_*` member
 * has nothing to slot at a level. `extract-rebirth-io-sets-v2.py` reads it per
 * dataset and stamps it; this file guards that nothing downstream re-derives it.
 *
 * It replaces two guesses, and the pins below are the shape of what each got
 * wrong. `maxLevel <= 1` misses the reward sets that keep a 10–50 range. The hand
 * list that patched those was keyed on DISPLAY NAME, which swept in Winter's Gift
 * — craftable on every fork. A user reported that on 2026-07-30 ("set as Attuned
 * … despite its Winter theme, behaves as a normal IO") and the beta took it off
 * the list; canonical's copy of the list kept it, and it cost every Winter's Gift
 * piece its level on a `.mbd` write until 2026-09-10 (MBDEXPORT-14). The same key
 * cannot tell Thunderspy's two "Subaluwa" records apart either, and they differ on
 * exactly this field.
 */
import { describe, it, expect } from 'vitest';
import { isInherentlyAttuned } from './enhancement-registry';
import { IO_SETS_RAW as HC } from './datasets/homecoming/io-sets-raw';
import { IO_SETS_RAW as REBIRTH } from './datasets/rebirth/io-sets-raw';
import { IO_SETS_RAW as TSPY } from './datasets/thunderspy/io-sets-raw';
import { IO_SETS_RAW as BRAINSTORM } from './datasets/brainstorm/io-sets-raw';

const DATASETS = {
  homecoming: HC,
  rebirth: REBIRTH,
  thunderspy: TSPY,
  brainstorm: BRAINSTORM,
} as const;

/** The measured roster size per dataset, 2026-09-10. */
const ATTUNED_ONLY_COUNT: Record<keyof typeof DATASETS, number> = {
  homecoming: 64,
  rebirth: 80,
  thunderspy: 61,
  brainstorm: 64,
};

describe('attunedOnly — the export states it, nothing re-derives it', () => {
  it('every set on every dataset carries the field', () => {
    for (const [ds, registry] of Object.entries(DATASETS)) {
      const absent = Object.entries(registry)
        .filter(([, set]) => typeof (set as { attunedOnly?: unknown }).attunedOnly !== 'boolean')
        .map(([id]) => id);
      expect(absent, `${ds}: sets with no attunedOnly`).toEqual([]);
    }
  });

  it('isInherentlyAttuned answers the field and only the field', () => {
    for (const [ds, registry] of Object.entries(DATASETS)) {
      for (const [id, set] of Object.entries(registry)) {
        expect(isInherentlyAttuned(set), `${ds}/${id}`).toBe(set.attunedOnly);
      }
    }
  });

  it('the roster is the measured size on each dataset', () => {
    for (const [ds, registry] of Object.entries(DATASETS)) {
      const attuned = Object.values(registry).filter((s) => s.attunedOnly);
      expect(attuned.length, `${ds} attuned-only sets`).toBe(
        ATTUNED_ONLY_COUNT[ds as keyof typeof DATASETS],
      );
    }
  });
});

describe('attunedOnly — what the two guesses got wrong', () => {
  /**
   * `maxLevel <= 1` under-reads: these sets keep a craft range in the data and
   * are still attuned-only. If this list empties, the heuristic has become
   * equivalent and someone will be tempted to go back to it.
   */
  it('attuned-only sets exist that maxLevel <= 1 would miss', () => {
    const missed = Object.entries(REBIRTH)
      .filter(([, s]) => s.attunedOnly && s.maxLevel > 1)
      .map(([id]) => id)
      .sort();
    expect(missed).toEqual([
      'forced_indoctrination',
      'imperial_might',
      'inexhaustibility',
      'libertys_belt',
      'overwhelming_force',
      'superior_winters_gift',
    ]);
  });

  /**
   * MBDEXPORT-14. Winter's Gift is a Universal Travel set with `Crafted_*`
   * pieces on every fork; the hand list called it attuned because of the name it
   * shares with the winter event sets, and the level went with it.
   */
  it("Winter's Gift is craftable on every dataset that ships it", () => {
    for (const [ds, registry] of Object.entries(DATASETS)) {
      const set = registry['winters_gift'];
      expect(set, `${ds} should ship winters_gift`).toBeDefined();
      expect(set.name).toBe("Winter's Gift");
      expect(isInherentlyAttuned(set), `${ds}: winters_gift`).toBe(false);
    }
    // Rebirth also ships the Superior set, which IS attuned-only — the two
    // differ on this field and agree on the word the hand list keyed on.
    expect(REBIRTH['superior_winters_gift'].attunedOnly).toBe(true);
  });

  /**
   * Thunderspy prints "Subaluwa" for two different records: its own craftable
   * `KB` set, and an `Overwhelming_Force` record that ships attuned-only. A rule
   * keyed on the display name answers one thing for both.
   */
  it('Thunderspy keeps its two "Subaluwa" records apart', () => {
    expect(TSPY['kb'].name).toBe('Subaluwa');
    expect(isInherentlyAttuned(TSPY['kb'])).toBe(false);
    expect(isInherentlyAttuned(TSPY['overwhelming_force'])).toBe(true);
  });

  it('ordinary invention sets stay craftable', () => {
    expect(isInherentlyAttuned(HC['kinetic_combat'])).toBe(false);
  });

  it('ATOs stay attuned', () => {
    expect(HC['blistering_cold'].maxLevel).toBeLessThanOrEqual(1);
    expect(isInherentlyAttuned(HC['blistering_cold'])).toBe(true);
  });
});
