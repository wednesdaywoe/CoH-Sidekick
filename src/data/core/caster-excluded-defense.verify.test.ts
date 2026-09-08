/**
 * A team-only defense buff must not land in the caster's totals.
 *
 * Shield Defense's Grant Cover carries the RPN clause `entref target> entref source>
 * eq !` — "target ≠ source" — on nine `Defense` rows aimed at `Target`. The game's own
 * help text says it outright: "The defense bonus from this power is only applied to
 * nearby team mates, but not yourself."
 *
 * This used to be handled by a hand-written `defenseBuffExcludesSelf` flag in the
 * overrides layer — four files on Homecoming, three on Rebirth, and none at all on
 * Thunderspy, which is exactly the hole a per-power hand-list leaves. The clause is on
 * the wire, so the applier reads it now and the flag is gone.
 *
 * Two halves, and the second is what keeps the first honest. Phalanx Fighting carries
 * the SAME clause on rows aimed at `Self` — it counts nearby allies to size a buff it
 * hands to the caster. A filter reading only the clause deletes Phalanx's per-ally
 * increment; a filter reading only the recipient keeps Grant Cover. Both powers sit in
 * the same powerset, so getting either wrong shows up here.
 *
 * **BPORT13.** The second half used to be stated as `expect(power.effects?.defenseBuff)
 * .toBeDefined()` — "the fallback has something to hand back, so the gate has to hold." BPORT7
 * removed the key and that assertion threw. The danger it named did not go away with it: the
 * oracle's seam is `defenseBuffIsTeamOnly(power) ? undefined : (defenseBuffValue(mezSource) ??
 * syntheticEffects(power)?.defenseBuff)`, and `syntheticEffects` returns the bag for any power
 * carrying `syntheticContribution` — the pet-aura fold and the conditional mints, which are
 * exactly the suppliers STRIP-1 left alive. So the claim is restated as the condition that makes
 * the gate load-bearing: hand the power the mint the fallback would read, and the gate must
 * still say team-only. That is the same question the old assertion was reaching for, asked of
 * the flag rather than of the converter.
 *
 * The reader half above grades the TypeScript oracle, whose seam spends
 * `defenseBuffIsTeamOnly` at `legacy-totals.oracle.ts:706`. The end-to-end half at the bottom
 * grades something else and is labelled as such: `calculateCharacterTotals` dispatches to
 * `engineCalculate`, so it reads the wasm engine the app actually runs, whose own mirror of
 * this rule is `crates/coh_math/tests/caster_excluded_defense.rs` in the canonical repo. The
 * TS oracle cannot be driven end-to-end at all right now — post-strip it skips every power at
 * `if (!power.effects) continue`, which is BPORT13's `serverParity` item — so the two halves
 * here are the reader and the engine, with the oracle's composed seam ungraded until that
 * lands. Stated rather than implied: a green file is not a statement about the oracle's
 * arithmetic, only about the verdict it consumes.
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { defenseBuffValue, defenseBuffSuppressibleValue, defenseBuffIsTeamOnly } from '@/data/core/atom-query';
import { calculateCharacterTotals } from '@/utils/calculations/character-totals';
import { createEmptyBuild } from '@/types/build';
import { loadDataset } from '@/data/dataset';
import type { DatasetId } from '@/data/dataset';
import { GrantCover as GrantCoverHC } from '@/data/datasets/homecoming/generated/powersets/tanker/primary/shield-defense/grant-cover';
import { GrantCover as GrantCoverRebirth } from '@/data/datasets/rebirth/generated/powersets/tanker/primary/shield-defense/grant-cover';
import { GrantCover as GrantCoverTspy } from '@/data/datasets/thunderspy/generated/powersets/tanker/primary/shield-defense/grant-cover';
import { PhalanxFighting } from '@/data/datasets/homecoming/generated/powersets/tanker/primary/shield-defense/phalanx-fighting';

const forks = [
  ['Homecoming', GrantCoverHC],
  ['Rebirth', GrantCoverRebirth],
  ['Thunderspy', GrantCoverTspy],
] as const;

/** A level-50 Shield Defense tanker with exactly one of the set's powers switched on. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function tankerWith(serverId: DatasetId, internalName: string): any {
  const b = createEmptyBuild(serverId);
  b.level = 50;
  b.archetype = { id: 'tanker', name: 'Tanker', stats: null, inherent: null } as never;
  // `powerSet` and `level` are load-bearing: `calculateCharacterTotals` is the engine here, and
  // it resolves a selected power out of its own bundle by (powerSet, internalName). Omit either
  // and the power resolves to nothing — the build contributes zero and every assertion below
  // would pass for the wrong reason.
  b.primary = {
    id: 'shield-defense',
    name: 'Shield Defense',
    powers: [{ internalName, name: internalName, powerSet: 'shield-defense', level: 1, isActive: true, slots: [] }],
  } as never;
  return b;
}

const DEF_KEYS = ['defMelee', 'defRanged', 'defAoE', 'defSmashing', 'defLethal'] as const;

describe('Grant Cover — the caster is not on the team it covers', () => {
  it.each(forks)('%s: the atom applier surfaces no caster defense', (_fork, power) => {
    expect(defenseBuffValue(power)).toBeUndefined();
    expect(defenseBuffSuppressibleValue(power)).toBeUndefined();
  });

  it.each(forks)('%s: and the verdict the seam reads is on the atoms, not a hand-list', (_fork, power) => {
    expect(defenseBuffIsTeamOnly(power)).toBe(true);
  });

  it.each(forks)('%s: the gate holds even when the fallback IS loaded', (_fork, power) => {
    // The condition that makes the gate load-bearing, constructed rather than waited for.
    // `syntheticEffects` hands the oracle `power.effects` for anything stamped
    // `syntheticContribution` — the pet-aura fold and the conditional mints — so a mint
    // carrying `defenseBuff` is a live route to the caster's totals for a power whose atoms
    // just declined. The flag reads the atoms and must be indifferent to what the mint holds;
    // a future version that consulted the bag would answer false here and the seam would open.
    const minted = {
      ...(power as unknown as Record<string, unknown>),
      syntheticContribution: true,
      effects: { defenseBuff: { melee: { scale: 0.9, table: 'Melee_Ones' } } },
    };
    expect(defenseBuffIsTeamOnly(minted as never)).toBe(true);
  });

  it.each(['homecoming', 'rebirth', 'thunderspy'] as const)(
    '%s: the wasm engine gives a build with Grant Cover running no defence at all',
    async (serverId) => {
      // Not the oracle — `calculateCharacterTotals` dispatches to `engineCalculate`. This is
      // the rule as the shipped engine applies it, on all three forks, which is the fork axis
      // the retired hand-list got wrong.
      await loadDataset(serverId);
      const t = calculateCharacterTotals(tankerWith(serverId, 'Grant_Cover'), false, undefined, { combatMode: true });
      for (const key of DEF_KEYS) expect(t.globalBonuses[key], `${serverId} ${key}`).toBe(0);
    },
  );
});

describe('Phalanx Fighting — the same clause, aimed at the caster', () => {
  beforeAll(async () => { await loadDataset('homecoming'); });

  it('keeps both the base and the per-ally increment', () => {
    // Pinned to both numbers, not to "defined": the clause rides only on the 0.3
    // increment, which folds into `perTarget`. A test asserting the power still
    // returns something passes with the increment deleted.
    const d = defenseBuffValue(PhalanxFighting)!;
    expect(Object.keys(d).sort()).toEqual(['aoe', 'melee', 'ranged']);
    for (const type of ['melee', 'ranged', 'aoe']) {
      expect(d[type].scale).toBeCloseTo(0.5);
      expect(d[type].perTarget).toBeCloseTo(0.3);
    }
  });

  it('is not team-only', () => {
    expect(defenseBuffIsTeamOnly(PhalanxFighting)).toBe(false);
  });

  it('and DOES reach the caster on the same build shape Grant Cover reads zero on', () => {
    // The anti-vacuity floor for the three totals assertions above: they would also read zero
    // if the fixture resolved no power at all. This is the same archetype, the same powerset
    // and the same slot, differing only in which power is on.
    const t = calculateCharacterTotals(tankerWith('homecoming', 'Phalanx_Fighting'), false, undefined, { combatMode: true });
    expect(t.globalBonuses.defMelee).toBeGreaterThan(0);
  });
});
