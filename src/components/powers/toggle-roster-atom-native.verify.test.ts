/**
 * BPORT6 — the click-power toggle roster, after `shouldShowToggle` left the bag.
 *
 * `hasPersistentBuffEffects` used to answer `key in power.effects` over 35 bag slots.
 * BPORT7 empties that object, at which point the predicate would say false for every click
 * power that has a toggle and each one would lose it with nothing red to say so: the tests
 * that touched this predicate handed it their own input, so they would have drained with it.
 *
 * That is the whole reason the migration lands BEFORE the strip. Here the bag is still
 * populated, so the retired predicate can be run beside the new one on the real corpus and
 * the two answers compared power by power — evidence the canonical repo could not get,
 * because it migrated after its strip and had to re-derive the roster from a pre-strip
 * checkout instead.
 *
 * **The measurement.** 14,249 powers, four datasets, all three partitions (archetype sets,
 * power pools, epic pools). The two predicates agree on 14,164 and differ on 85 — 82 powers
 * that lose a toggle and 3 that gain one. Every divergence is named below, and every one is
 * the atom path being right where the bag could not be:
 *
 *  - **49 ally- and pet-directed resource restores.** The rezzes (Resurrect, Rebirth, Power
 *    of the Phoenix), Defibrillate, and Robotics' Repair. Each carries an `Endurance` atom
 *    at `toWho: Target` with no `Self` in `targetsAffected` — `DeadPlayerFriend`,
 *    `DeadOrAliveLeaguemate`, `DeadOrAliveAny`, `MyPet`. The caster gains nothing. The bag
 *    filed all of it under `enduranceGain`, a slot with no recipient axis, so the ally's
 *    endurance read as the caster's. `affectsCaster` cannot catch these either:
 *    `ALLY_ONLY_TARGETS` knows `ally`, not `Dead Teammate` or `Own Pet (Alive)`.
 *  - **14 Envenom copies.** Its foe `HealResistance`/`Res` debuff is filed under
 *    `resistance`, a buff-named slot. `resistanceBuffValue` reads the atom's `Res` face and
 *    its `Target` recipient and declines.
 *  - **12 Defiance riders on damageless control powers** (Dark Pit, Scare, Touch of Fear,
 *    Lightning Field, Time Stop, Time Shift). `damageBuffValue` rejects Defiance atoms; the
 *    bag's `damageBuff` slot held them, and the `isDamagingAttack` skip meant to catch them
 *    cannot fire on a power that carries no damage entry at all.
 *  - **4 Disrupting Torrent copies.** A foe-directed `Regeneration` row filed under
 *    `regenBuffUnenhanced`.
 *  - **3 Fortify Pack copies.** `targetsAffected: ['MyPet']` — its Defense and Regen go to
 *    the henchmen, and the bag recorded them at scale 0 while still minting the keys.
 *  - **2 Thunderous Blast copies.** The nuke's AoE endurance DRAIN on the foes, filed under
 *    `enduranceGain` with the sign flipped positive.
 *
 * And the three gains are Team Teleport, on the forks that carry it. It teleports the caster
 * along with the team, and the caster takes the same post-teleport `Fly`/`Control`/`Friction`
 * penalty the passengers do — `selfSlowValue` sees those rows because `targetsAffected` names
 * `Self`. The bag had no self-directed slow entry for it at all.
 *
 * **What did NOT diverge, and why that is worth stating.** Wild Bastion and Force Barrier
 * state their absorb only as a `Max`-face `Expression` ceiling, which `absorbValue`
 * deliberately excludes; they were the one cluster of this migration that would have been a
 * regression rather than a correction. `absorbMaxHPFractionValue` already reads exactly that
 * shape, so wiring it into the query list closes the gap here rather than carrying it — the
 * canonical repo files the same 10 powers as an open residual against a reader it has
 * already landed.
 */
import { describe, it, expect } from 'vitest';
import { shouldShowToggle } from './power-row-utils';
import { hasSelfDirectedPenalty, type PowerEffects } from '@/types';
import { MODULAR_POWERSETS as HC } from '@/data/datasets/homecoming/powersets';
import { MODULAR_POWERSETS as RB } from '@/data/datasets/rebirth/powersets';
import { MODULAR_POWERSETS as TSPY } from '@/data/datasets/thunderspy/powersets';
import { MODULAR_POWERSETS as BS } from '@/data/datasets/brainstorm/powersets';
import { POWER_POOLS_RAW as HCP } from '@/data/datasets/homecoming/power-pools-raw';
import { EPIC_POOLS_RAW as HCE } from '@/data/datasets/homecoming/epic-pools-raw';
import { POWER_POOLS_RAW as RBP } from '@/data/datasets/rebirth/power-pools-raw';
import { EPIC_POOLS_RAW as RBE } from '@/data/datasets/rebirth/epic-pools-raw';
import { POWER_POOLS_RAW as TSP } from '@/data/datasets/thunderspy/power-pools-raw';
import { EPIC_POOLS_RAW as TSE } from '@/data/datasets/thunderspy/epic-pools-raw';
import { POWER_POOLS_RAW as BSP } from '@/data/datasets/brainstorm/power-pools-raw';
import { EPIC_POOLS_RAW as BSE } from '@/data/datasets/brainstorm/epic-pools-raw';

type AnyPower = Record<string, unknown> & { name?: string; internalName?: string };
type Tree = Record<string, { powers?: AnyPower[] }>;

const PARTITIONS: readonly (readonly [string, Tree])[] = [
  ['homecoming/set', HC as unknown as Tree], ['rebirth/set', RB as unknown as Tree],
  ['thunderspy/set', TSPY as unknown as Tree], ['brainstorm/set', BS as unknown as Tree],
  ['homecoming/pool', HCP as unknown as Tree], ['homecoming/epic', HCE as unknown as Tree],
  ['rebirth/pool', RBP as unknown as Tree], ['rebirth/epic', RBE as unknown as Tree],
  ['thunderspy/pool', TSP as unknown as Tree], ['thunderspy/epic', TSE as unknown as Tree],
  ['brainstorm/pool', BSP as unknown as Tree], ['brainstorm/epic', BSE as unknown as Tree],
];

function* corpus(): Generator<[string, AnyPower]> {
  for (const [label, tree] of PARTITIONS) {
    for (const [setId, set] of Object.entries(tree)) {
      for (const power of set?.powers ?? []) yield [`${label}/${setId}`, power];
    }
  }
}

const idOf = (p: AnyPower) => (p.internalName as string) ?? (p.name as string);

/**
 * The retired predicate, verbatim in behaviour, as the shadow oracle.
 *
 * Inlined rather than imported because it no longer exists — the point of this leg is to
 * compare against what the file used to do, and the only faithful copy of that is this one.
 * It reads `power.effects`, so it is scoped to a populated bag and says so below.
 */
const CASTER_BUFF_KEYS = [
  'tohitBuff', 'tohitBuffUnenhanced', 'damageBuff', 'defenseBuff', 'defenseBuffSuppressible',
  'rechargeBuff', 'recoveryBuff', 'recoveryBuffUnenhanced', 'regenBuff', 'regenBuffUnenhanced',
  'speedBuff', 'enduranceBuff', 'enduranceGain', 'maxHPBuff', 'maxEndBuff',
  'rangeBuff', 'enduranceDiscount', 'threatBuff', 'perceptionBuff', 'absorb',
  'defense', 'resistance', 'specialBuff',
  'runSpeed', 'flySpeed', 'jumpHeight', 'jumpSpeed', 'fly', 'movementControl', 'movementFriction',
  'stealthPvE', 'stealthPvP', 'translucency', 'mezResistance', 'debuffResistance',
];
const ROUTED_SUBTYPES: Record<string, Set<string>> = {
  mezResistance: new Set(['hold', 'stun', 'immobilize', 'sleep', 'confuse', 'fear', 'knockback']),
  debuffResistance: new Set([
    'movement', 'defense', 'recharge', 'endurance', 'recovery', 'tohit',
    'regeneration', 'perception', 'accuracy', 'range',
  ]),
};
const ALLY_ONLY_TARGETS = new Set(['ally', 'ally (alive)']);

function bagPredicate(power: AnyPower): boolean {
  const powerType = (power.powerType as string | undefined)?.toLowerCase();
  if (powerType === 'toggle') return true;
  if (powerType !== 'click') return false;
  const targetType = power.targetType as string | undefined;
  if (targetType && ALLY_ONLY_TARGETS.has(targetType.toLowerCase())) return false;
  const effects = power.effects as Record<string, unknown> | undefined;
  if (!effects) return false;
  if (hasSelfDirectedPenalty(effects as PowerEffects)) return true;
  const damage = power.damage;
  const entries = Array.isArray(damage) ? damage : damage ? [damage] : [];
  const isAttack = entries.some((d) => {
    const e = d as { type?: string; scale?: number };
    return e?.type !== 'Heal' && (e?.scale ?? 0) > 0;
  });
  const skip = new Set<string>(isAttack ? ['damageBuff', 'rangeBuff'] : []);
  if (targetType && targetType.toLowerCase() !== 'self') skip.add('specialBuff');
  return CASTER_BUFF_KEYS.some((key) => {
    if (!(key in effects) || skip.has(key)) return false;
    const routed = ROUTED_SUBTYPES[key];
    if (!routed) return true;
    const container = effects[key];
    if (!container || typeof container !== 'object') return false;
    return Object.keys(container as Record<string, unknown>)
      .some((subtype) => routed.has(subtype.toLowerCase()));
  });
}

/** Powers the atom path takes a toggle from, and how many copies of each. */
const LOSSES: Record<string, number> = {
  Rebirth: 16, Power_of_the_Phoenix: 16, Envenom: 14, Defibrillate: 8, Resurrect: 6,
  Disrupting_Torrent: 4, Fortify_Pack: 3, Dark_Pit: 2, Lightning_Field: 2, Scare: 2,
  Thunderous_Blast: 2, Time_Shift: 2, Time_Stop: 2, Touch_of_Fear: 2, Repair: 1,
};
/** Powers it hands one back. */
const GAINS: Record<string, number> = { 'Team Teleport': 3 };

describe('the toggle roster survives the bag', () => {
  it('agrees with the retired bag predicate everywhere but the 85 adjudicated powers', () => {
    // BPORT13. The scope test used to be "does any power still have a non-empty bag", and after
    // BPORT7 that is still TRUE — 36 homecoming powers keep an `effects` key, because the strip
    // emptied the converter's bag and not the hand-written overrides layer. So the skip never
    // fired and the leg ran against a shadow oracle with 36 rows in it, which is neither a
    // comparison nor a skip. The question the guard actually needs is narrower: does the bag
    // still supply any key THIS PREDICATE READS. It does not — the eight slots the overrides
    // still carry (`rechargeDebuff`, `buffDuration`, `stealth`, `taunt`, `stun`, `movement`,
    // `durations`, `effectDuration`) are disjoint from `CASTER_BUFF_KEYS`, so `bagPredicate`
    // answers false for all 14,249 powers and has nothing to say about any of them.
    const supplying = [...corpus()].filter(([, p]) => {
      const effects = p.effects as Record<string, unknown> | undefined;
      return !!effects && CASTER_BUFF_KEYS.some((k) => k in effects);
    });
    if (supplying.length === 0) {
      // Stated rather than silent, and stated about the right population. The two legs below
      // are the ones that outlive the strip; this one is scoped to a converter-supplied bag by
      // construction and says so instead of grading 14,164 vacuous agreements as a pass.
      expect(supplying, 'bag no longer supplies a key this predicate reads').toEqual([]);
      // The floor that keeps the skip from becoming permanent cover: the ATOM side must still
      // answer. A skip whose sibling has also gone quiet is not a scoped guard, it is a dead
      // file, and this is the one assertion that can tell the two apart.
      expect([...corpus()].filter(([, p]) => shouldShowToggle(p)).length).toBeGreaterThan(2_000);
      return;
    }
    const lost: Record<string, number> = {};
    const gained: Record<string, number> = {};
    for (const [, power] of corpus()) {
      const before = bagPredicate(power);
      const after = shouldShowToggle(power);
      if (before === after) continue;
      const bucket = before ? lost : gained;
      bucket[idOf(power)] = (bucket[idOf(power)] ?? 0) + 1;
    }
    expect(lost).toEqual(LOSSES);
    expect(gained).toEqual(GAINS);
  });

  it('keeps the roster whole — a re-drain would take it to zero, not to 85', () => {
    // The floor. `hasPersistentBuffEffects` going quiet the way the bag did is the failure
    // this file exists for, and it does not look like a handful of adjudicated names: it
    // looks like every click toggle in the corpus disappearing at once.
    let clicks = 0;
    let toggles = 0;
    for (const [, power] of corpus()) {
      if ((power.powerType as string | undefined)?.toLowerCase() !== 'click') continue;
      clicks++;
      if (shouldShowToggle(power)) toggles++;
    }
    expect(clicks).toBeGreaterThan(11_000);
    expect(toggles).toBeGreaterThan(2_000);
  });
});

describe('the named cases the predicate was last corrected for', () => {
  /** Every copy of a power in one dataset, across all three partitions. */
  const find = (dataset: string, name: string) => {
    const hits = [...corpus()]
      .filter(([l, p]) => l.startsWith(`${dataset}/`) && (p.name === name || p.internalName === name));
    expect(hits.length, `${dataset}/${name}: copies found`).toBeGreaterThan(0);
    return hits.map(([, p]) => p);
  };

  /**
   * Reported 2026-07-30: Fold Space had a toggle that did nothing. Its `mezResistance`
   * carries `teleport` — the 15s protection granted to the FOES you yank so they cannot be
   * chain-pulled — and the bag slot had nothing on it to say whose protection it was.
   *
   * The routability workaround this replaces answered by excluding the `teleport` subtype
   * outright. `mezResistanceValue` reads `teleport` now, and gets the same answer for the
   * right reason: `reachesCaster` sees a `Target` row on a power whose `targetsAffected`
   * names only foes. Team Teleport, which names `Self`, is the other side of that same
   * question and is in the gains above.
   */
  it.each([
    ['homecoming', 'Fold Space'], ['homecoming', 'Shadow Slip'],
    ['homecoming', 'Wormhole'], ['rebirth', 'Mass Translocate'],
    ['rebirth', 'Starless Gateway'], ['thunderspy', 'Teleport Foe'],
  ])('%s / %s gets no toggle off a foe-directed teleport protection', (dataset, name) => {
    for (const power of find(dataset, name)) expect(shouldShowToggle(power)).toBe(false);
  });

  it.each(['homecoming', 'rebirth', 'thunderspy', 'brainstorm'])(
    '%s / Aid Self keeps its toggle',
    (dataset) => {
      // The other side: a self-cast click whose `stun` mez resistance does reach the caster.
      for (const power of find(dataset, 'Aid Self')) {
        expect(shouldShowToggle(power)).toBe(true);
      }
    },
  );

  it.each(['homecoming', 'brainstorm'])(
    '%s / Wild Bastion keeps the toggle its Max-face absorb earns',
    (dataset) => {
      // The cluster that would have been a regression rather than a correction: an absorb
      // stated only as a `Max`-face Expression ceiling, which `absorbValue` excludes and
      // `absorbMaxHPFractionValue` reads. If this reader is ever dropped from the query
      // list, these go red instead of quietly losing 10 toggles.
      for (const power of find(dataset, 'Wild_Bastion')) {
        expect(shouldShowToggle(power)).toBe(true);
      }
      for (const power of find(dataset, 'Force Barrier')) {
        expect(shouldShowToggle(power)).toBe(true);
      }
    },
  );
});
