/**
 * BPORT11 cluster 1 — the seven scalar families, read off the atoms.
 *
 * **BPORT13 restated it.** This was a two-armed comparison and BPORT7 removed one arm: with
 * `power.effects` gone every `bagOnly` is empty, every `agree` is zero, and the whole corpus
 * falls into `atomOnly`. Three of the four buckets therefore passed vacuously while the other
 * two went red — the worse half being the ones that stayed green.
 *
 * What survives is not nothing. The bag was the oracle that MINTED these carrier counts, and a
 * number outlives the oracle that established it: `accuracyBuff` agreeing on 56 carriers and
 * nothing else is the same fact as `accuracyBuff` having exactly 56 atom carriers, once the
 * bag holds none. So each arm below is now a one-armed census pinned to the count its own
 * comparison produced, plus a floor asserting the bag arm really is empty — which keeps the
 * retired half honest rather than assumed, and reds if a supplier ever refills it.
 *
 * The counts are pinned PER FORK rather than as a total. A total is not a roster: it survives a
 * power moving between forks, and cross-fork movement is where this corpus actually drifts.
 *
 * That restatement immediately earned itself. `rechargeBuff` was documented as 309 agreements
 * plus 25 Thunderspy-only gains, so the census should read 334; it reads 309. The 25 left when
 * `atom-query.ts` was ported wholesale from canonical (STACK-7, 2026-08-27) and arrived
 * carrying `slowIsDebuff`, ten days before the strip. They were never a recovery: Time Wall,
 * the named example, states its −recharge as a `Ranged_Slow` row at `toWho: Target`, and the
 * pre-port reader credited the CASTER with a foe's debuff — the identical defect this file's
 * own header describes for `maxEndBuffValue` two paragraphs down, in a sibling family, found
 * the same way. The port fixed it and nothing said so, because the comparison that would have
 * said so was already red for a different reason. The 25 are recorded here as falsified.
 *
 * This was the comparison BPORT7 destroys. The regen that empties `power.effects` also empties
 * the shadow oracle every one of these arms was checked against, so a carry landing after the
 * strip can only be checked against itself. Canonical hit exactly that on `shouldShowToggle`
 * and had to re-derive the roster from a pre-strip checkout. The order here is the lesson:
 * migrate first, grade against the incumbent, then strip.
 *
 * Six of the seven agree with the bag on every carrier of every fork. The two that do not are
 * the reason the comparison is worth running:
 *
 *  - **`maxEndBuffValue` credited the caster with the foe's drain.** Soul Consumption states
 *    `-1 Target` beside its `+1 Self` and the reader's `Math.abs` fold summed both, answering
 *    2 where the bag, the game and the Rust twin all say 1 — four powersets on two forks. Its
 *    Rust counterpart (`coh_math::appliers::resources::max_endurance_buff_value`) has carried
 *    the recipient test since ATOM8; the TypeScript half never grew it, and nothing compared
 *    the two until this carry. Fixed in `atom-query.ts`, pinned below.
 *  - **`rechargeBuffValue` answered for 25 powers the Thunderspy bag never held.** Recorded
 *    at the time as the migration working. It was not — see the BPORT13 note above; those 25
 *    were foe `*_Slow` rows credited to the caster, and `slowIsDebuff` has since declined them.
 *    1,039 powers carry such a row and 1,042 of the atoms are `toWho: Target`. The nine that
 *    are not all belong to Reaction Time, a PBAoE whose every foe row has an exact negative
 *    twin aimed at `Self` — the caster's carve-out from its own field, not a caster buff — so
 *    declining the family whole is right on both populations.
 *
 * The seventh is `elusivity`, which BPORT1 filed as zero-supply. Both arms confirm it from
 * their own side: no power on any fork carries the bag entry, and no power carries an atom the
 * reader would return. The arm is kept rather than deleted precisely because it is empty — a
 * reader that answers the day a fork ships one beats a deletion somebody has to notice.
 */
import { describe, it, expect } from 'vitest';
import {
  accuracyBuffValue, rechargeBuffValue, rangeBuffValue, perceptionBuffValue,
  enduranceDiscountValue, maxEndBuffValue, elusivityValue, baseAtoms,
} from '@/data/core/atom-query';
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

type AnyPower = Record<string, unknown> & { name?: string; targetType?: string; effects?: Record<string, unknown> };
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
  for (const [label, tree] of PARTITIONS)
    for (const [setId, set] of Object.entries(tree))
      for (const power of set?.powers ?? []) yield [`${label}/${setId}`, power];
}

/**
 * A slot value as the number it resolves to, independent of level and archetype: equal
 * `{scale, table}` pairs feed `resolveScaledEffect` the same two arguments, so comparing the
 * pair compares every number either arm could produce. `undefined` is "this arm declines".
 */
type Val = string | undefined;
const pair = (v: unknown): Val => {
  if (v === undefined || v === null) return undefined;
  if (typeof v === 'number') return `${v}@`;
  const o = v as { scale?: number; table?: string };
  if (typeof o.scale !== 'number') return `?${JSON.stringify(v)}`;
  return `${o.scale}@${(o.table ?? '').toLowerCase()}`;
};

type Split = { agree: string[]; differ: string[]; bagOnly: string[]; atomOnly: string[] };

/** One family's two arms over the whole corpus, split four ways. */
function grade(
  slot: string,
  atomArm: (p: AnyPower) => unknown,
  gate?: (p: AnyPower) => boolean,
): Split {
  const out: Split = { agree: [], differ: [], bagOnly: [], atomOnly: [] };
  for (const [where, power] of corpus()) {
    if (gate && !gate(power)) continue;
    const b = pair(power.effects?.[slot]);
    const a = pair(atomArm(power));
    if (b === undefined && a === undefined) continue;
    const id = `${where}/${power.name}`;
    if (b !== undefined && a !== undefined) (b === a ? out.agree : out.differ).push(`${id} bag=${b} atom=${a}`);
    else if (b !== undefined) out.bagOnly.push(`${id} bag=${b}`);
    else out.atomOnly.push(`${id} atom=${a}`);
  }
  return out;
}

/** Post-strip the bag supplies nothing, so a carrier is an `atomOnly` row. Keyed by fork. */
function census(g: Split): Record<string, number> {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const row of g.atomOnly) out[row.split('/')[0]] += 1;
  return out;
}

/**
 * The retired arm, asserted rather than assumed. Every bucket that needs the bag to be
 * populated must be empty; if one is not, a supplier has come back and the census below is
 * silently comparing against a half-filled oracle again.
 */
function bagIsGone(g: Split, slot: string): void {
  expect([...g.agree, ...g.differ, ...g.bagOnly], `${slot}: the bag arm answered`).toEqual([]);
}

describe('BPORT11 cluster 1 — the scalar families, censused off the atoms', () => {
  it.each([
    ['accuracyBuff', (p: AnyPower) => accuracyBuffValue(p as never), undefined,
      { homecoming: 15, rebirth: 17, thunderspy: 9, brainstorm: 15 }],
    ['enduranceDiscount', (p: AnyPower) => enduranceDiscountValue(p as never), undefined,
      { homecoming: 28, rebirth: 25, thunderspy: 18, brainstorm: 32 }],
    ['perceptionBuff', (p: AnyPower) => perceptionBuffValue(p as never), undefined,
      { homecoming: 72, rebirth: 56, thunderspy: 47, brainstorm: 81 }],
    // The oracle only credits a `rangeBuff` on a Self-target power (the Fast Snipe range bump
    // is not a persistent caster buff), so the comparison runs under the same gate — grading
    // an arm on a population its call site never reaches proves nothing about the call site.
    ['rangeBuff', (p: AnyPower) => rangeBuffValue(p as never),
      (p: AnyPower) => p.targetType?.toLowerCase() === 'self',
      { homecoming: 21, rebirth: 1, thunderspy: 2, brainstorm: 21 }],
    ['maxEndBuff', (p: AnyPower) => maxEndBuffValue(p as never), undefined,
      { homecoming: 13, rebirth: 6, thunderspy: 16, brainstorm: 13 }],
  ])('%s: the carrier census the bag comparison minted, per fork', (slot, arm, gate, expected) => {
    const g = grade(slot as string, arm as (p: AnyPower) => unknown, gate as ((p: AnyPower) => boolean) | undefined);
    bagIsGone(g, slot as string);
    expect(census(g), `${slot} carriers`).toEqual(expected);
  });

  it('reads recharge off 309 carriers, and declines the foe slow it used to credit', () => {
    const g = grade('rechargeBuff', (p) => rechargeBuffValue(p as never));
    bagIsGone(g, 'rechargeBuff');
    expect(census(g)).toEqual({ homecoming: 96, rebirth: 58, thunderspy: 46, brainstorm: 109 });
    // The falsified half, kept as a live claim rather than a struck-out comment. Time Wall's
    // −recharge is a `Ranged_Slow` row aimed at the target on every fork that carries it; the
    // caster's own recharge reader must not answer for it, and `slowIsDebuff` is what makes
    // that true. This is the assertion the pre-port reader failed.
    const timeWalls = [...corpus()].filter(([, p]) => p.name === 'Time Wall');
    expect(timeWalls.length).toBeGreaterThan(0);
    for (const [where, p] of timeWalls) {
      const slow = baseAtoms(p as never)
        .filter((a) => a.effectType === 'RechargeTime' && (a.modifierTable ?? '').toLowerCase().includes('slow'));
      expect(slow.length, where).toBeGreaterThan(0);
      expect(slow.every((a) => a.toWho === 'Target'), where).toBe(true);
      expect(rechargeBuffValue(p as never), where).toBeUndefined();
    }
  });

  it('leaves elusivity empty on the arm that still has a side, which is why the reader stays', () => {
    // The one arm the strip did not weaken. It was empty from both sides before and the atom
    // side is still empty now, so the claim is unchanged rather than restated: no power on any
    // fork carries an atom this reader would answer for.
    const g = grade('elusivity', (p) => elusivityValue(p as never));
    expect(g).toEqual({ agree: [], differ: [], bagOnly: [], atomOnly: [] });
  });
});

describe('BPORT11 — the recipient test maxEndBuffValue was missing', () => {
  /** Soul Consumption: an AoE that drains foes and hands the caster what it took. */
  const soulConsumption = () => {
    const p = (HCE as unknown as Tree)['blaster_dark_mastery']?.powers
      ?.find((x) => x.name === 'Soul Consumption');
    expect(p, 'Soul Consumption is the fixture; a rename must red here').toBeDefined();
    return p!;
  };

  it('reads the caster half of a drain, not the sum of both halves', () => {
    const p = soulConsumption();
    const rows = baseAtoms(p as never)
      .filter((a) => a.effectType === 'MaxEndurance' && a.aspect === 'Max');
    // Stated on the atoms rather than on the answer: the two rows differ ONLY in recipient and
    // sign, which is the collapse the whole model exists to prevent. If a future export stops
    // shipping the pair, this test should say so rather than quietly grading one row.
    expect(rows.map((a) => `${a.toWho}:${a.scale}`).sort()).toEqual(['Self:1', 'Target:-1']);
    expect(maxEndBuffValue(p as never)).toEqual({ scale: 1, table: 'Ranged_EndDrain' });
  });

  it('keeps a SELF debuff, which is the crash a power really does inflict', () => {
    // The predicate is `to_who === Target && isDebuff`, not `isDebuff` — narrower at both ends,
    // and both ends carry powers. Burnout's −25 MaxEnd lands on the caster and the bag counts
    // it, so dropping every debuff would have been a second wrong answer in the other
    // direction. (Whether |−25| should be credited as +25 at all is older than this carry and
    // is what the bag, the atoms and the Rust engine all currently say.)
    const burnout = (HCP as unknown as Tree)['speed']?.powers?.find((x) => x.name === 'Burnout');
    expect(burnout).toBeDefined();
    const rows = baseAtoms(burnout as never)
      .filter((a) => a.effectType === 'MaxEndurance' && a.aspect === 'Max');
    expect(rows.map((a) => `${a.toWho}:${a.scale}`)).toEqual(['Self:-25']);
    expect(maxEndBuffValue(burnout as never)).toEqual({ scale: 25, table: 'Melee_Ones' });
  });
});
