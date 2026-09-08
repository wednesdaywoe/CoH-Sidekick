/**
 * BPORT11 cluster 2 — mez protection, read off the atoms, graded against the bag it replaces.
 *
 * The six MEZ types, Knockback/Knockup, `mezResistance`, Taunt and Placate all left the bag in
 * one pass, because they are one fold and one credit gate. As with cluster 1 the comparison is
 * only available before BPORT7's regen: the bag is the shadow oracle, and the regen that
 * empties the slot empties the oracle with it.
 *
 * Read PER PLAYER CLASS, which is the part a build-agnostic comparison cannot do. A protection
 * atom may fork on `casterArchetypes` — Rebirth's pool powers state identical arms per class
 * (AT-FORK-1) — and a raw read returns `undefined` exactly where the build's own arm exists.
 * The bag never had to care, because the converter wrote one slot per power with the fork
 * already collapsed into it; an atom reader has to resolve it or lose the value. So every
 * assertion here runs through {@link mezSourceFor}, the same resolution the call site uses.
 *
 * What the corpus says, measured over 213,735 power×class views:
 *
 *  - the six MEZ agree with the bag on all 36,780 carrier views, with nothing held by either
 *    side alone;
 *  - the Knockback/Knockup bag arm never fired once — every view it could have answered was
 *    one `kbProtectionValue` had already answered, which is what retires it, rather than a
 *    decision to drop it;
 *  - `mezResistance` agrees on all 8,760 carrier views of the keys this block routes to a
 *    global, and gains 2: Rebirth Weave's immobilize resistance for the two Kheldian classes,
 *    visible only through the fork resolution;
 *  - Taunt and Placate agree on all 4 and all 15;
 *  - `protRepel` is the one arm that moves numbers, and it moves them both ways: 202 agree, 71
 *    lose a credit and 15 gain one. `effects.repel` holds the repel a power INFLICTS, so Ki
 *    Push, Jet Stream, Hurricane and Repulsion Field were crediting the caster with protection
 *    equal to the push they deal out, while Increase Density — the example the retired block's
 *    own comment named — was one of the 15 the slot never carried;
 *  - `effects.protection` has no carrier anywhere, confirming BPORT1's zero-supply verdict from
 *    the data side as well as the supply side;
 *  - and `protTeleport` is gone, stat and read together (user decision, matching canonical). Its
 *    read was `effects.teleport` × 100 on a slot holding the teleport a power PERFORMS, so all
 *    127 credited carriers were teleport powers credited with protection equal to their own
 *    range. No power carries the protection-spelled row a corrected read would have needed.
 *
 * Mutation-tested five ways on the readers and the resolution: reversing `mezSlotValue`'s
 * larger-magnitude pick, dropping its sub-type key, and stopping `mezSourceFor` from resolving
 * the fork all go red. One does NOT, and it is reported rather than patched — removing
 * `mezSlotValue`'s `aspect === 'Res' || aspect === 'Str'` guard changes no answer, because 0 of
 * the corpus's 11,784 `Mez` atoms carry either face. Mez RESISTANCE is a `MezResist` row and
 * mez STRENGTH is a `specialBuff` row; both are separate effect types, not separate faces of
 * `Mez`, so that clause is a second filter over an empty population. It is correct to keep and
 * wrong to claim as tested.
 *
 * The cost of the per-class read is why the sweep below narrows: a power with no forked atom
 * answers identically for every class by construction, so it is measured once and the class
 * loop runs only for the powers that actually carry a `casterArchetypes` stamp. The narrowing
 * is asserted, not assumed — {@link forkedPowers} must be non-empty, or the whole fork axis
 * would be silently untested.
 */
import { describe, it, expect } from 'vitest';
import {
  mezSlotValue, mezResistanceValue, kbProtectionValue, tauntPlacateValue,
} from '@/data/core/atom-query';
import { repelProtectionValue } from './repel-protection';
import { baseAtoms } from '@/data/core/atom-query';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';

/** The census's own comment scanner, so a retirement NOTE naming a slot is not read as a read
 *  of it — the first thing BPORT4's file-level grep got wrong, one level down. */
const { stripComments } = createRequire(import.meta.url)(
  path.resolve(__dirname, '../../../scripts/beta-bag-reader-census.cjs'),
) as { stripComments: (src: string) => string };
import { ATOM_TUPLE_FIELDS } from '@/data/core/atomic-effect';
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
type Fork = { fork: string; trees: (readonly [string, Tree])[]; tokens: string[] };

const classTokens = (reg: unknown): string[] => [...new Set(
  Object.values(reg as Record<string, { stats?: { className?: string } }>)
    .map((a) => a?.stats?.className)
    .filter((t): t is string => !!t),
)];

const FORKS: Fork[] = [
  {
    fork: 'homecoming', tokens: classTokens(HCA),
    trees: [['set', HC as unknown as Tree], ['pool', HCP as unknown as Tree], ['epic', HCE as unknown as Tree]],
  },
  {
    fork: 'rebirth', tokens: classTokens(RBA),
    trees: [['set', RB as unknown as Tree], ['pool', RBP as unknown as Tree], ['epic', RBE as unknown as Tree]],
  },
  {
    fork: 'thunderspy', tokens: classTokens(TSA),
    trees: [['set', TSPY as unknown as Tree], ['pool', TSP as unknown as Tree], ['epic', TSE as unknown as Tree]],
  },
  {
    fork: 'brainstorm', tokens: classTokens(BSA),
    trees: [['set', BS as unknown as Tree], ['pool', BSP as unknown as Tree], ['epic', BSE as unknown as Tree]],
  },
];

const FORK_IDX = ATOM_TUPLE_FIELDS.indexOf('casterArchetypes');
const isForked = (p: AnyPower) =>
  (p.atoms ?? []).some((t) => !!(t as unknown[])[FORK_IDX]);

/**
 * Every (power, class-view) the readers can distinguish: one view per power, plus the full
 * class fan-out for the powers whose atoms actually fork. A power with no `casterArchetypes`
 * stamp is returned unchanged by `mezSourceFor` for every token, so the extra views are the
 * same comparison repeated — 213,735 of them where 14,249 + the forked fan-out will do.
 */
function* views(): Generator<[string, AnyPower, AnyPower]> {
  for (const { fork, trees, tokens } of FORKS)
    for (const [label, tree] of trees)
      for (const [setId, set] of Object.entries(tree))
        for (const p of set?.powers ?? []) {
          const at = `${fork}/${label}/${setId}/${p.name}`;
          if (!isForked(p)) { yield [at, p, mezSourceFor(p as never, undefined) as AnyPower]; continue; }
          for (const tok of tokens) yield [`${at} [${tok}]`, p, mezSourceFor(p as never, tok) as AnyPower];
        }
}

const forkedPowers = () => {
  const out: string[] = [];
  for (const { fork, trees } of FORKS)
    for (const [label, tree] of trees)
      for (const [setId, set] of Object.entries(tree))
        for (const p of set?.powers ?? []) if (isForked(p)) out.push(`${fork}/${label}/${setId}/${p.name}`);
  return out;
};

const isRes = (m: unknown) => {
  const t = (m && typeof m === 'object' && typeof (m as { table?: unknown }).table === 'string')
    ? (m as { table: string }).table.toLowerCase() : '';
  return t.includes('res_boolean');
};
/** The magnitude a Res_Boolean protection row resolves to, before the AT table is applied. */
const mag = (m: unknown) => (m && typeof m === 'object')
  ? `${Math.abs((m as { scale: number }).scale)}@${((m as { table?: string }).table ?? '').toLowerCase()}`
  : undefined;

const MEZ6 = ['hold', 'stun', 'immobilize', 'sleep', 'confuse', 'fear'] as const;

type Split = { agree: number; differ: string[]; bagOnly: string[]; atomOnly: string[] };
const empty = (): Split => ({ agree: 0, differ: [], bagOnly: [], atomOnly: [] });
const record = (t: Split, id: string, bag?: string, atom?: string) => {
  if (bag === undefined && atom === undefined) return;
  if (bag !== undefined && atom !== undefined) {
    if (bag === atom) t.agree++; else t.differ.push(`${id} bag=${bag} atom=${atom}`);
  } else if (bag !== undefined) t.bagOnly.push(`${id} bag=${bag}`);
  else t.atomOnly.push(`${id} atom=${atom}`);
};

/**
 * The carrier-view censuses, per fork. Every one is a population the bag comparison walked; the
 * numbers are what the atom arm answers for now that it is the only arm.
 */
const MEZ6_CENSUS: Record<string, Record<string, number>> = {
  hold: { homecoming: 140, rebirth: 111, thunderspy: 103, brainstorm: 158 },
  stun: { homecoming: 151, rebirth: 119, thunderspy: 110, brainstorm: 169 },
  immobilize: { homecoming: 137, rebirth: 120, thunderspy: 105, brainstorm: 155 },
  sleep: { homecoming: 138, rebirth: 103, thunderspy: 96, brainstorm: 152 },
  confuse: { homecoming: 55, rebirth: 56, thunderspy: 37, brainstorm: 59 },
  fear: { homecoming: 63, rebirth: 41, thunderspy: 35, brainstorm: 67 },
};
const MEZRES_CENSUS = { homecoming: 162, rebirth: 161, thunderspy: 123, brainstorm: 168 };
const TAUNT_PLACATE_CENSUS: Record<string, Record<string, number>> = {
  Taunt: { homecoming: 0, rebirth: 2, thunderspy: 2, brainstorm: 0 },
  Placate: { homecoming: 4, rebirth: 5, thunderspy: 2, brainstorm: 4 },
};
const REPEL_CENSUS = { homecoming: 61, rebirth: 44, thunderspy: 41, brainstorm: 71 };

/** A per-fork tally of view ids shaped `fork/partition/set/name[ [token]]`. */
const byFork = (ids: string[]): Record<string, number> => {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const id of ids) out[id.split('/')[0]] += 1;
  return out;
};

/**
 * BPORT13. BPORT7 took the bag arm out of every comparison in this file, so `agree` is zero,
 * `bagOnly` is empty, and the corpus falls into `atomOnly`. The `toBeGreaterThan(0)` floors
 * these arms carried are what went red — and they were the right assertions to have written,
 * because the `toEqual([])` verdicts beside them all passed vacuously.
 *
 * Restated as one-armed censuses. Where the bag comparison reported a number it is reused, and
 * where it did not the count is a fresh pin and says so. Per fork, because two of the findings
 * this file records — the Rebirth Weave fork and the repel direction — are fork-shaped, and a
 * total would survive either one collapsing onto one fork.
 */
const bagIsGone = (t: Split, slot: string): void => {
  expect([t.agree, ...t.differ, ...t.bagOnly], `${slot}: the bag arm answered`).toEqual([0]);
};

describe('BPORT11 cluster 2 — mez protection, censused off the atoms', () => {
  it('measures the fork axis it narrows on, rather than assuming it is empty', () => {
    const forked = forkedPowers();
    expect(forked.length).toBeGreaterThan(0);
    // Rebirth is where the unanimous per-class forks live; if this stops being true the
    // narrowing above stops being a narrowing and this file needs the full fan-out back.
    expect(forked.some((f) => f.startsWith('rebirth/'))).toBe(true);
  });

  it('reads the six MEZ types exactly as the bag stated them', () => {
    const per = new Map(MEZ6.map((f) => [f as string, empty()]));
    for (const [id, power, source] of views())
      for (const field of MEZ6) {
        const bv = power.effects?.[field];
        record(
          per.get(field)!, id,
          (bv !== undefined && typeof bv !== 'number' && isRes(bv)) ? mag(bv) : undefined,
          (() => { const a = mezSlotValue(source as never, field); return a && isRes(a) ? mag(a) : undefined; })(),
        );
      }
    const counts: Record<string, Record<string, number>> = {};
    for (const [field, t] of per) {
      bagIsGone(t, field);
      counts[field] = byFork(t.atomOnly.map((r) => r.split(' atom=')[0]));
    }
    expect(counts).toEqual(MEZ6_CENSUS);
  });

  it('never once needed the Knockback/Knockup bag arm it retires', () => {
    const stranded: string[] = [];
    let atomAnswers = 0;
    for (const [id, power, source] of views())
      for (const field of ['knockback', 'knockup'] as const) {
        if (kbProtectionValue(source as never, field)) { atomAnswers++; continue; }
        // The arm that just went: a non-number bag value on a Res_Boolean table, credited only
        // because the atom read declined. If this list is ever non-empty the retirement drops
        // a real number and the arm has to come back.
        const bv = power.effects?.[field];
        if (bv !== undefined && typeof bv !== 'number' && isRes(bv)) stranded.push(`${id}/${field}`);
      }
    expect(stranded).toEqual([]);
    expect(atomAnswers).toBeGreaterThan(0);
  });

  it('agrees on mez resistance, and recovers what only a fork-resolved read can see', () => {
    // `mezResMapping`'s keys, and only those: `taunt`/`placate`/`teleport` ride the same bag map
    // under keys it never routed, so comparing the raw map would compare values no arm spends.
    const MAPPED = new Set(['hold', 'stun', 'immobilize', 'sleep', 'confuse', 'fear', 'knockback']);
    const fmt = (o: unknown) => {
      if (!o || typeof o !== 'object') return undefined;
      const rows = Object.entries(o as Record<string, unknown>)
        .filter(([k]) => MAPPED.has(k.toLowerCase()))
        .map(([k, v]) => `${k.toLowerCase()}=${mag(typeof v === 'number' ? { scale: v, table: '' } : v)}`)
        .sort();
      return rows.length ? rows.join(',') : undefined;
    };
    const t = empty();
    for (const [id, power, source] of views())
      record(t, id, fmt(power.effects?.mezResistance), fmt(mezResistanceValue(source as never)));
    bagIsGone(t, 'mezResistance');
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0]))).toEqual(MEZRES_CENSUS);
    // The gain, named, and still askable of the atoms alone. Rebirth's Weave forks its
    // protection atoms by class, so the Kheldian arms answer only through a fork-resolved view
    // and a raw read sees nothing — the same shape as the Rebirth Weave defence bug canonical
    // hit from the other direction. Two views, and which two is the claim.
    const weave = t.atomOnly.filter((r) => r.includes('rebirth/pool/fighting/Weave'));
    expect(weave).toHaveLength(2);
    expect(weave.map((r) => r.match(/\[(\w+)\]/)?.[1]).sort())
      .toEqual(['Class_Peacebringer', 'Class_Warshade']);
  });

  it('agrees on taunt and placate resistance', () => {
    const per = new Map([['Taunt', empty()], ['Placate', empty()]]);
    for (const [id, power] of views())
      for (const which of ['Taunt', 'Placate'] as const) {
        const bv = power.effects?.[which.toLowerCase()];
        const av = tauntPlacateValue(power as never, which);
        record(
          per.get(which)!, id,
          (bv !== undefined && typeof bv !== 'number' && isRes(bv)) ? mag(bv) : undefined,
          (av && isRes(av)) ? mag(av) : undefined,
        );
      }
    // `taunt` is one of the eight slots the homecoming overrides layer still supplies (two
    // carriers — see the surviving-supply census in `buffs-atom-native.verify.test.ts`), so
    // `bagIsGone` is deliberately NOT asserted here: a bag answer would be a declared supplier
    // rather than a stale one, and reddening on it would be wrong. In fact neither override row
    // reaches this arm — both are magnitudes, and the arm reads only the `Res` face — which is
    // why Homecoming's Taunt column is 0 while Rebirth's and Thunderspy's are 2. That zero is a
    // measured fact about the override rows, not an absence of taunt resistance on Homecoming.
    const counts: Record<string, Record<string, number>> = {};
    for (const [which, t] of per) {
      expect(t.differ, `${which} differ`).toEqual([]);
      counts[which] = byFork([...t.atomOnly, ...t.bagOnly].map((r) => r.split(/ (?:atom|bag)=/)[0]));
    }
    expect(counts).toEqual(TAUNT_PLACATE_CENSUS);
  });

  it('reads repel protection off the atoms, and stops reading the push as protection', () => {
    // The one arm in this cluster that MOVES numbers, in both directions, and the direction is
    // the whole verdict. `effects.repel` holds the repel a power INFLICTS.
    const t = empty();
    for (const [id, power, source] of views()) {
      const bv = power.effects?.repel;
      record(
        t, id,
        bv === undefined ? undefined : mag(typeof bv === 'number' ? { scale: bv, table: '' } : bv),
        (() => { const a = repelProtectionValue(source as never); return a ? mag(a) : undefined; })(),
      );
    }
    bagIsGone(t, 'repel');
    // 217 carrier views: the 202 both arms agreed on plus the 15 the slot never carried. The
    // 71 dropped views were the other direction — `effects.repel` holds the repel a power
    // INFLICTS, so Ki Push, Jet Stream, Hurricane and Repulsion Field were being credited to
    // the caster as protection against their own knockback. That population was defined by the
    // bag holding a value and BPORT13 retires it: there is no atom that means "the bag once
    // said this", and minting a stand-in would be the reader confirming itself.
    //
    // The direction that survives is the one that matters. `repelProtectionValue` reads
    // protection, so an offensive-repel power must NOT be a carrier, and the four named
    // examples are asserted as absences rather than left to the retired count.
    expect(t.atomOnly).toHaveLength(217);
    expect(byFork(t.atomOnly.map((r) => r.split(' atom=')[0]))).toEqual(REPEL_CENSUS);
    for (const named of ['Ki Push', 'Jet Stream', 'Hurricane', 'Repulsion Field']) {
      expect(t.atomOnly.some((s) => s.includes(named)), `${named} is credited as protection`).toBe(false);
    }
    // Gained: real repel protection, which the slot never carried.
    expect(t.atomOnly.some((s) => s.includes('Increase Density'))).toBe(true);
    expect(t.atomOnly.some((s) => s.includes('Vengeance'))).toBe(true);
  });

  it('keeps protTeleport retired, stat and read together', () => {
    // A retirement with no tripwire is a comment. The read was `effects.teleport` × 100 and the
    // slot holds the teleport a power PERFORMS, so every one of its 127 credited carriers was a
    // teleport power being credited with protection equal to its own range — Wormhole at +410%.
    // Two things have to stay gone or it comes back by halves: the stat, and any read of the
    // slot that could be pointed at it again.
    const oracle = stripComments(readFileSync(path.resolve(__dirname, 'legacy-totals.oracle.ts'), 'utf8'));
    const totals = stripComments(readFileSync(path.resolve(__dirname, 'character-totals.ts'), 'utf8'));
    expect(/\bprotTeleport\b/.test(oracle)).toBe(false);
    expect(/\bprotTeleport\b/.test(totals)).toBe(false);
    expect(/\beffects\.teleport\b/.test(oracle)).toBe(false);

    // And the data that made it wrong, so the verdict is checkable rather than remembered: no
    // power carries a protection-spelled `Mez/Teleport` row, which is what a corrected read
    // would have needed. The honest family is `MezResist/Teleport` — resistance, not
    // protection — and it belongs with mez resistance, not here.
    const protectionSpelled: string[] = [];
    for (const [id, , source] of views())
      for (const a of baseAtoms(source as never))
        if (a.effectType === 'Mez' && a.subType === 'Teleport' && (a.scale ?? 0) < 0) {
          protectionSpelled.push(id);
        }
    expect(protectionSpelled).toEqual([]);
  });

  it('confirms effects.protection is empty from the data side too', () => {
    // BPORT1 called it zero-supply from the supply census. This is the other direction: not one
    // power in any dataset carries the object, so the block that read it was iterating nothing
    // and its removal moves no number.
    const carriers: string[] = [];
    for (const { fork, trees } of FORKS)
      for (const [label, tree] of trees)
        for (const [setId, set] of Object.entries(tree))
          for (const p of set?.powers ?? [])
            if (p.effects?.protection !== undefined) carriers.push(`${fork}/${label}/${setId}/${p.name}`);
    expect(carriers).toEqual([]);
  });
});
