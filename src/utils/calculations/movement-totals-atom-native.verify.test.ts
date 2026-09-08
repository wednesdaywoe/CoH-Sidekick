/**
 * BPORT11 cluster 4 — the movement cluster, read off the atoms and graded against the bag.
 *
 * Three separate reads that had to move together, because they share one axis vocabulary and
 * two of them are halves of the same authored pair.
 *
 * **The five scalar slots go, and they were never carriers.** `runSpeed`, `runSpeedUnenhanced`,
 * `flySpeed`, `jumpHeight` and `jumpSpeed` have 0 powers on all four forks. The comment those
 * blocks carried named Sprint, Ninja Run and Beast Run as the powers reaching the calc that way;
 * those hand-authored inherents carry no bag at all and their movement is atom-native through
 * the axis map. BPORT1 had already filed `flySpeed` as zero-supply and left the other four as
 * `leave`, because the supply census could not see that their only supplier was a display mint
 * the totals path never reaches.
 *
 * **The axis map's data branch had exactly one carrier, and fork resolution takes it.**
 * `movementBuffValue` returns an ARRAY — usually empty — for any power with a movement atom,
 * and `??` keeps an empty array, so the bag branch only ever fired where the reader answered
 * `undefined`. Across 14,249 powers that is one power: rebirth Acrobatics, whose atoms fork by
 * class (AT-FORK-1), so a build-agnostic read saw none of them. Read through `mezSourceFor` it
 * answers with the same jumpHeight/jumpSpeed the bag holds, and the data branch is left with no
 * carrier at all.
 *
 * **The combat-debuff gate is a swap of instrument, not of verdict.** It asked whether two
 * sibling SLOTS were absent; `carries_combat_debuff` asks the atoms on the discriminators that
 * decide it. They agree on all 18,239 power×class views.
 *
 * **The self-penalty pair is where numbers actually move**, and every move is a self penalty the
 * converter's `toWho` tagging lost rather than a value the reader invented. Rebirth and
 * Thunderspy's Granite Armor and Rooted state their jump root as `JumpHeight -500 toWho:Target`
 * on a Self-target toggle — the "target" of a self-cast toggle IS the caster, which is why
 * `reachesCaster` consults the power's recipients — and the bag's untagged entry was dropped by
 * the `isSelfDirectedEffect` gate. So the root those two powers are named for reached no total
 * on two of the four forks. The block also grew its second half: `movementCapDebuff` is the
 * Maximum face of the same penalty, split out of `slow` by ENT-5, and this calc never grew the
 * read — 312 powers carry the slot and none was ever spent.
 *
 * Mutation-tested four ways, all red: routing the recipient question off the ROW rather than the
 * power (which is what loses Granite's root), routing the cap debuff off the Current face,
 * dropping `capEntries`' self-only gate, and stopping `carries_combat_debuff` discriminating on
 * aspect so ToHit-debuff RESISTANCE reads as a ToHit debuff.
 */
import { describe, it, expect } from 'vitest';
import { movementBuffValue, selfSlowValue, selfMovementCapDebuffValue } from '@/data/core/atom-query';
import { ATOM_TUPLE_FIELDS } from '@/data/core/atomic-effect';
import { mezSourceFor, carries_combat_debuff } from './character-totals';
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

function* powers(): Generator<[string, AnyPower]> {
  for (const { fork, trees } of FORKS)
    for (const [label, tree] of trees)
      for (const [setId, set] of Object.entries(tree))
        for (const p of set?.powers ?? []) yield [`${fork}/${label}/${setId}/${p.name}`, p];
}
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

/** The four axes `movementKeyMap` routes to a global. `fly` is the flight-MODE grant and is
 *  deliberately unmapped; `movementControl` / `movementFriction` have no global at all. */
const ROUTED = new Set(['runSpeed', 'flySpeed', 'jumpHeight', 'jumpSpeed']);

/** A per-fork tally of view ids shaped `fork/partition/set/name[ [token]]`. */
const byFork = (ids: string[]): Record<string, number> => {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const id of ids) out[id.split('/')[0]] += 1;
  return out;
};

/**
 * BPORT13. Every arm below was a two-armed comparison and BPORT7 took one arm away. Three of
 * the four arms here were scoped BY the bag — "powers whose `effects.movement` routes an axis",
 * "powers carrying `movementCapDebuff`" — so post-strip they iterate an empty set and the loop
 * body never runs. That is a silent pass, not a skip, and it is the reason this row exists.
 *
 * Restated on the atom side, where each claim's actual subject always was. The counts are the
 * ones the bag comparison established while it could still be asked; the populations are now
 * defined by the reader rather than by the slot.
 */
describe('BPORT11 cluster 4 — the movement cluster, censused off the atoms', () => {
  it('retires five scalar slots that no power on any fork carries', () => {
    const carriers: Record<string, string[]> = {};
    for (const [id, p] of powers())
      for (const slot of ['runSpeed', 'runSpeedUnenhanced', 'flySpeed', 'jumpHeight', 'jumpSpeed'])
        if (p.effects?.[slot] !== undefined) (carriers[slot] ??= []).push(id);
    expect(carriers).toEqual({});
  });

  it('answers the movement axis for every carrier once the fork is resolved', () => {
    // The old arm scoped itself by `effects.movement` routing an axis, which is now an empty
    // set on every power — the loop body stopped executing and the test stayed green. The
    // claim it was making is still askable of the atoms: `movementBuffValue` must answer for a
    // power that carries a routed axis, and the one power it could not answer for RAW was the
    // forked one, which resolves only when a class token is supplied.
    //
    // Acrobatics is the case the whole arm turns on, so it is asserted by name rather than
    // left to a count. Rebirth forks its atoms by archetype; read without a token the reader
    // has no fork to resolve and declines, and read per class it answers — which is exactly
    // why a raw sweep would have called it a hole.
    const carriers: string[] = [];
    const strandedResolved: string[] = [];
    let acrobaticsRawDeclines = 0;
    let acrobaticsResolved = 0;
    for (const { fork, trees, tokens } of FORKS)
      for (const [label, tree] of trees)
        for (const [setId, set] of Object.entries(tree))
          for (const p of set?.powers ?? []) {
            const id = `${fork}/${label}/${setId}/${p.name}`;
            const views_ = isForked(p) ? tokens.map((t) => mezSourceFor(p as never, t)) : [p as never];
            const answers = views_.filter((v) => movementBuffValue(v) !== undefined);
            if (answers.length) carriers.push(id);
            if (id === 'rebirth/pool/leaping/Acrobatics') {
              if (movementBuffValue(p as never) === undefined) acrobaticsRawDeclines++;
              acrobaticsResolved = answers.length;
              if (answers.length !== views_.length) strandedResolved.push(id);
            }
          }
    // NOTE on provenance: the 2100 below is a fresh census, not a number the bag ever
    // confirmed — the old arm counted `undefinedReaders`, not carriers, so there is nothing to
    // check it against. It is pinned as a movement tripwire and no more; the claims this test
    // actually rests on are the three Acrobatics assertions, which are the fork behaviour.
    expect(acrobaticsRawDeclines, 'Acrobatics answered without a fork to resolve').toBe(1);
    expect(acrobaticsResolved, 'Acrobatics answered for no class view').toBeGreaterThan(0);
    expect(strandedResolved, 'a class view of the forked power went unanswered').toEqual([]);
    expect(carriers, 'movementBuffValue carriers').toHaveLength(2100);
    expect(byFork(carriers)).toEqual({ homecoming: 585, rebirth: 462, thunderspy: 447, brainstorm: 606 });
  });

  it('swaps the combat-debuff gate without swapping its verdict', () => {
    // The retired gate was `effects.tohitDebuff === undefined && effects.damageDebuff ===
    // undefined`, and with the bag empty it answers "no debuff" for all 14,249 powers — so the
    // comparison agreed with `carries_combat_debuff` everywhere it disagreed with reality and
    // reported a clean pass. The atom gate is the one the call site spends, so it is censused
    // directly: the population is real, non-empty, and fork-shaped.
    // 1624 is likewise a fresh census — the old arm asserted only that the two gates agreed,
    // and never said on how many. Pinned so the gate cannot go quiet the way its bag twin did.
    const carrying = [...views()].filter(([, p]) => carries_combat_debuff(p as never)).map(([id]) => id);
    expect(carrying, 'carries_combat_debuff answered for nothing').not.toHaveLength(0);
    expect(carrying).toHaveLength(1624);
    expect(byFork(carrying)).toEqual({ homecoming: 433, rebirth: 387, thunderspy: 361, brainstorm: 443 });
  });

  it('restores the jump root two forks lost to an untagged bag entry', () => {
    // The atom arms stamp `toWho: 'Self'` because they have already answered the recipient
    // question; the bag entry carried whatever the converter tagged, and on these it tagged
    // nothing. Asserted on the powers by name, because a silent change of population here is
    // the failure this whole comparison exists to catch.
    const selfAxes = (rows: { axis: string; scale: number }[] | undefined) =>
      (rows ?? []).filter((e) => ROUTED.has(e.axis)).map((e) => `${e.axis}=${e.scale}`).sort().join(',');
    // The recovery, restated on the reader. The bag entry these were lost to carried no
    // recipient tag, so a self-directed read skipped them; the atom arms stamp `toWho: 'Self'`
    // because they have already answered the recipient question. Asserted by name and by axis
    // value, because a silent change of population here is the failure the comparison existed
    // to catch and a bare count would not see a swap.
    const rooted = [...powers()]
      .filter(([id]) => /Granite Armor|Rooted/.test(id))
      .map(([id, p]) => [id, selfAxes(selfSlowValue(p as never) as { axis: string; scale: number }[] | undefined)] as const)
      .filter(([, axes]) => axes.includes('jumpHeight=500'));
    expect(rooted, 'the jump root the untagged bag entry lost').toHaveLength(8);
    // Rebirth and Thunderspy only, which is the recovery stated as the shape it actually has.
    expect(byFork(rooted.map(([id]) => id))).toEqual({ homecoming: 0, rebirth: 4, thunderspy: 4, brainstorm: 0 });
    // And the whole self-directed slow population, so a gain elsewhere cannot hide behind it.
    const selfSlow = [...powers()]
      .filter(([, p]) => selfAxes(selfSlowValue(p as never) as { axis: string; scale: number }[] | undefined))
      .map(([id]) => id);
    expect(selfSlow).toHaveLength(36);
    expect(byFork(selfSlow)).toEqual({ homecoming: 4, rebirth: 16, thunderspy: 12, brainstorm: 4 });
  });

  it('gives the Maximum face of the penalty a reader for the first time', () => {
    // 312 powers carry `movementCapDebuff` and this calc read none of them: the slot was split
    // out of `slow` by ENT-5 and only the Current-face read was ever written here. Nothing
    // moves today — only 4 views are self-tagged and both arms agree on all 4 — but the axis
    // now has a reader on both faces, which is what stops the next cap debuff being silent.
    // The 312 `movementCapDebuff` slot carriers were the bag's and are gone; what the arm was
    // for is that the Maximum face now HAS a reader, and only the self-tagged rows reach the
    // caster. Four of them, which both arms agreed on while there were two arms.
    const carriers = [...powers()]
      .filter(([, p]) => (selfMovementCapDebuffValue(p as never) ?? []).some((e) => ROUTED.has(e.axis)))
      .map(([id]) => id);
    expect(carriers).toHaveLength(4);
    expect(byFork(carriers)).toEqual({ homecoming: 2, rebirth: 0, thunderspy: 0, brainstorm: 2 });
  });
});
