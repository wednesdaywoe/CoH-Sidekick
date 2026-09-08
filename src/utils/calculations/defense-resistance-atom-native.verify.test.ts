/**
 * BPORT11 cluster 3 — defence, resistance, debuff resistance and the self −Res penalty, read
 * off the atoms and graded against the bag they replace.
 *
 * Five per-type maps, four of which cross fork-resolved: rebirth's pool and armour powers state
 * identical defence and resistance arms per class (AT-FORK-1), and a build-agnostic read returns
 * `undefined` exactly where the build's own arm exists. Canonical hit that from the other side —
 * rebirth Weave reading 0 defence while every other fork read it — which is why the resolution
 * came across with the mez fold rather than after it.
 *
 * Measured over 18,239 power×class views, one per power plus the class fan-out for the powers
 * whose atoms actually fork:
 *
 *  - `debuffResistance` 1,319 carrier views agree, nothing on either side alone;
 *  - `resistance` 1,606 agree;
 *  - `resistanceDebuff`, restricted to the self-tagged entries the call site keeps, 17 agree;
 *  - `defence` (the `defense` / `defenseBuff` pair as one arm) 1,143 agree, with 5 views held by
 *    the bag alone and every one of them a map of ZEROS — Fortify Pack's eleven `0@ranged_buff_def`
 *    entries and Superior Invisibility's one. The reader declines them and the retired branch
 *    credited `0`, so no total moves; what changes is that a breakdown row stops appearing for a
 *    power that contributes nothing;
 *  - `defenseBuffSuppressible` 97 agree and 12 are GAINS, all of them Personal Force Field on
 *    rebirth and Thunderspy, whose +7.5 suppressible defence those forks' bags never carried.
 *
 * The re-key is the part worth stating twice. `effects.defense` had exactly one supplier on any
 * fork and it was not the converter: **0 converted powers carry the slot**, and the only writer
 * is `buffPetAuraEffects`. Retiring the data read while the fold still wrote the old key would
 * have zeroed every buff-pet defence aura in silence — the SYNTH-1 shape, in the one place this
 * repo could have walked into it. The fold mints `defenseBuff` now, so a pet aura and a mode
 * conditional reach the arm the same way.
 */
import { describe, it, expect } from 'vitest';
import {
  defenseBuffValue, defenseBuffSuppressibleValue, defenseBuffIsTeamOnly,
  resistanceBuffValue, resistanceSelfDebuffValue, debuffResistanceValue,
} from '@/data/core/atom-query';
import { ATOM_TUPLE_FIELDS } from '@/data/core/atomic-effect';
import { isSelfDirectedEffect } from '@/types';
import { mezSourceFor } from './character-totals';
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
import { ARCHETYPES as HCA } from '@/data/datasets/homecoming/archetypes';
import { ARCHETYPES as RBA } from '@/data/datasets/rebirth/archetypes';
import { ARCHETYPES as TSA } from '@/data/datasets/thunderspy/archetypes';
import { ARCHETYPES as BSA } from '@/data/datasets/brainstorm/archetypes';

type AnyPower = Record<string, unknown> & { name?: string; atoms?: unknown[]; effects?: Record<string, unknown> };
type Tree = Record<string, { powers?: AnyPower[] }>;

const classTokens = (reg: unknown): string[] => [...new Set(
  Object.values(reg as Record<string, { stats?: { className?: string } }>)
    .map((a) => a?.stats?.className).filter((t): t is string => !!t),
)];

const FORKS = [
  { fork: 'homecoming', tokens: classTokens(HCA), trees: [['set', HC], ['pool', HCP], ['epic', HCE]] },
  { fork: 'rebirth', tokens: classTokens(RBA), trees: [['set', RB], ['pool', RBP], ['epic', RBE]] },
  { fork: 'thunderspy', tokens: classTokens(TSA), trees: [['set', TSPY], ['pool', TSP], ['epic', TSE]] },
  { fork: 'brainstorm', tokens: classTokens(BSA), trees: [['set', BS], ['pool', BSP], ['epic', BSE]] },
] as unknown as { fork: string; tokens: string[]; trees: [string, Tree][] }[];

const FORK_IDX = ATOM_TUPLE_FIELDS.indexOf('casterArchetypes');
const isForked = (p: AnyPower) => (p.atoms ?? []).some((t) => !!(t as unknown[])[FORK_IDX]);

/** One view per power, plus the class fan-out for the powers whose atoms fork. */
function* views(): Generator<[string, AnyPower, AnyPower]> {
  for (const { fork, trees, tokens } of FORKS)
    for (const [label, tree] of trees)
      for (const [setId, set] of Object.entries(tree))
        for (const p of set?.powers ?? []) {
          const at = `${fork}/${label}/${setId}/${p.name}`;
          if (!isForked(p)) { yield [at, p, p]; continue; }
          for (const tok of tokens) yield [`${at} [${tok}]`, p, mezSourceFor(p as never, tok) as AnyPower];
        }
}

const pair = (v: unknown) => {
  if (v === undefined || v === null) return undefined;
  if (typeof v === 'number') return `${v}@`;
  const o = v as { scale?: number; table?: string; perTarget?: number };
  if (typeof o.scale !== 'number') return `?${JSON.stringify(v)}`;
  return `${o.scale}@${(o.table ?? '').toLowerCase()}${o.perTarget ? `+${o.perTarget}` : ''}`;
};
/** A per-type map as the string its call site's loop would spend, restricted to routed keys. */
const fmtMap = (m: unknown, keys: Set<string>) => {
  if (!m || typeof m !== 'object') return undefined;
  const rows = Object.entries(m as Record<string, unknown>)
    .filter(([k]) => keys.has(k.toLowerCase()))
    .map(([k, v]) => `${k.toLowerCase()}=${pair(v)}`).sort();
  return rows.length ? rows.join(',') : undefined;
};
const DEF_KEYS = new Set(['smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'psionic',
  'toxic', 'melee', 'ranged', 'aoe']);
const RES_KEYS = new Set(['smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'psionic', 'toxic']);
const DEBUFF_RES_KEYS = new Set(['movement', 'recharge', 'defense', 'tohit', 'endurance',
  'recovery', 'regeneration', 'perception', 'accuracy', 'range']);

type Split = { agree: number; differ: string[]; bagOnly: string[]; atomOnly: string[] };
const empty = (): Split => ({ agree: 0, differ: [], bagOnly: [], atomOnly: [] });
const record = (t: Split, id: string, bag?: string, atom?: string) => {
  if (bag === undefined && atom === undefined) return;
  if (bag !== undefined && atom !== undefined) {
    if (bag === atom) t.agree++; else t.differ.push(`${id} bag=${bag} atom=${atom}`);
  } else if (bag !== undefined) t.bagOnly.push(`${id} bag=${bag}`);
  else t.atomOnly.push(`${id} atom=${atom}`);
};

/** A per-fork tally of view ids shaped `fork/partition/set/name[ [token]]`. */
const byFork = (ids: string[]): Record<string, number> => {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const id of ids) out[id.split('/')[0]] += 1;
  return out;
};

/**
 * BPORT13. The bag half of every comparison in this file is gone, so `agree`, `differ` and
 * `bagOnly` are all structurally zero and the whole population falls into `atomOnly`. Two of
 * those three passed vacuously and one went red, which is the worst possible split: the arm
 * that reported was not the arm that had stopped grading.
 *
 * Each arm is now a one-armed census pinned to the carrier count its own comparison produced
 * while the bag still answered — the bag was the oracle that minted these numbers, and a number
 * outlives its oracle. Per fork, because a total is not a roster and cross-fork movement is
 * where this corpus drifts. `bagIsGone` keeps the retired half asserted rather than assumed.
 */
const bagIsGone = (t: Split, slot: string): void => {
  expect([t.agree, ...t.differ, ...t.bagOnly], `${slot}: the bag arm answered`).toEqual([0]);
};

describe('BPORT11 cluster 3 — defence and resistance, censused off the atoms', () => {
  it.each([
    ['resistance', RES_KEYS, (src: AnyPower) => resistanceBuffValue(src as never), 1606,
      { homecoming: 439, rebirth: 360, thunderspy: 321, brainstorm: 486 }],
    ['debuffResistance', DEBUFF_RES_KEYS, (src: AnyPower) => debuffResistanceValue(src as never), 1319,
      { homecoming: 383, rebirth: 277, thunderspy: 224, brainstorm: 435 }],
  ])('%s: the carrier-view census the bag comparison minted (%d)', (slot, keys, arm, total, expected) => {
    const t = empty();
    for (const [id, power, source] of views())
      record(t, id, fmtMap(power.effects?.[slot as string], keys as Set<string>),
        fmtMap((arm as (s: AnyPower) => unknown)(source), keys as Set<string>));
    bagIsGone(t, slot as string);
    expect(t.atomOnly, `${slot} total`).toHaveLength(total as number);
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0])), `${slot} carriers`).toEqual(expected);
  });

  it('keeps only the self-tagged half of the -Res penalty, as the call site does', () => {
    // Most `resistanceDebuff` entries are foe-facing. The call site filters per entry, not per
    // power, because Rebirth Granite carries both in one map — so the comparison filters the
    // same way or it grades values no total ever sees.
    const selfOnly = (m: unknown) => {
      if (!m || typeof m !== 'object') return undefined;
      const rows = Object.entries(m as Record<string, unknown>)
        .filter(([k, v]) => RES_KEYS.has(k.toLowerCase()) && isSelfDirectedEffect(v))
        .map(([k, v]) => `${k.toLowerCase()}=${pair(v)}`).sort();
      return rows.length ? rows.join(',') : undefined;
    };
    const t = empty();
    for (const [id, power, source] of views())
      record(t, id, selfOnly(power.effects?.resistanceDebuff),
        selfOnly(resistanceSelfDebuffValue(source as never)));
    bagIsGone(t, 'resistanceDebuff');
    expect(t.atomOnly).toHaveLength(17);
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0]))).toEqual({ homecoming: 5, rebirth: 4, thunderspy: 3, brainstorm: 5 });
  });

  it('agrees on defence, and declines five maps of zeros the bag credited', () => {
    // The team-only gate is part of the arm, not a filter over it: Grant Cover's defence goes to
    // the team, and the retired fallback would have handed back the number the atom read had
    // just declined to give. So the comparison applies it to BOTH sides.
    const t = empty();
    for (const [id, power, source] of views()) {
      const teamOnly = defenseBuffIsTeamOnly(power as never);
      const bagArm = power.effects?.defense ?? (teamOnly ? undefined : power.effects?.defenseBuff);
      const atomArm = teamOnly ? undefined : defenseBuffValue(source as never);
      record(t, id, fmtMap(bagArm, DEF_KEYS), fmtMap(atomArm, DEF_KEYS));
    }
    // The five bag-only views this arm used to report were all-zero breakdown maps, so the
    // arithmetic never changed and only an empty row disappeared. That population was defined
    // by the bag holding a value and BPORT13 retires it rather than re-deriving it: the atom
    // side cannot know which powers the converter once minted a zeros map for, and inventing a
    // stand-in would be the reader agreeing with itself.
    bagIsGone(t, 'defense');
    expect(t.atomOnly).toHaveLength(1143);
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0]))).toEqual({ homecoming: 289, rebirth: 352, thunderspy: 203, brainstorm: 299 });
  });

  it('recovers Personal Force Field, whose suppressible defence two forks never carried', () => {
    const t = empty();
    for (const [id, power] of views())
      record(t, id, fmtMap(power.effects?.defenseBuffSuppressible, DEF_KEYS),
        fmtMap(defenseBuffSuppressibleValue(power as never), DEF_KEYS));
    // 109 carrier views: the 97 both arms agreed on plus the 12 Personal Force Field views the
    // bag never held on two forks. Post-strip they are one population and the fork split is
    // what keeps the recovery visible — PFF is why this arm exists, and a per-fork count is the
    // only shape that reds if it silently goes back to being a homecoming-only reader.
    bagIsGone(t, 'defenseBuffSuppressible');
    expect(t.atomOnly).toHaveLength(109);
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0]))).toEqual({ homecoming: 37, rebirth: 22, thunderspy: 12, brainstorm: 38 });
    // 24 PFF views, not the 12 the old arm reported: `atomOnly` used to mean "the bag lacked
    // this" and now means "a carrier", so the 12 views the bag DID hold on Homecoming and
    // Brainstorm join the 12 it never held on Rebirth and Thunderspy. The recovery is therefore
    // stated as the fork split rather than as a total — the claim was always that two forks
    // carried none, and a bare 24 would survive all of them moving to one fork.
    const pff = byFork(t.atomOnly.filter((r) => r.includes('Personal Force Field'))
      .map((r) => r.split(' atom=')[0]));
    expect(pff).toEqual({ homecoming: 6, rebirth: 6, thunderspy: 6, brainstorm: 6 });
  });

  it('leaves effects.defense with no supplier but the pet-aura fold', () => {
    // The fact the re-key rests on. If a converter ever writes this slot again, reading only
    // `defenseBuff` starts dropping it and this is where that shows up.
    //
    // Vacuous on the converter's bag now, and kept anyway, because the converter is no longer
    // the only writer: 36 homecoming override files still carry an `effects` key (censused in
    // `buffs-atom-native.verify.test.ts`). `defense` is not among the eight slots they carry,
    // and this is the assertion that says so for this one.
    const carriers: string[] = [];
    for (const { fork, trees } of FORKS)
      for (const [label, tree] of trees)
        for (const [setId, set] of Object.entries(tree))
          for (const p of set?.powers ?? [])
            if (p.effects?.defense !== undefined) carriers.push(`${fork}/${label}/${setId}/${p.name}`);
    expect(carriers).toEqual([]);
  });
});
