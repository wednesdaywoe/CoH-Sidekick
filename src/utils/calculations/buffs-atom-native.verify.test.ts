/**
 * BPORT11 cluster 5 — the last families, read off the atoms and graded against the bag.
 *
 * ToHit, +Damage, the self damage and recharge penalties, regen, recovery, MaxHP, stealth, the
 * accolade +MaxEnd, and the toggle end cost. Six of them agree with the bag on every carrier;
 * the four that do not are the four worth reading twice.
 *
 * **Defiance was being credited as a permanent +damage buff.** 68 powers carry a `damageBuff`
 * the reader declines, and every single one is defiance-only — Blaster secondaries whose
 * Defiance rows the bag projected into a slot the totals spend flat. The rejection has to be
 * spoken at the call site as well as inside the reader, because an absent atom read is
 * indistinguishable from the atom-less case a fallback serves, and the bag held the same value
 * one line down.
 *
 * **Ally buffs were being credited to the caster.** Retiring the regen and recovery data arms
 * drops 46 + 76 credits, and every one is a `reachesCaster`-false row: Adrenalin Boost,
 * Painbringer, Temporal Selection, Speed Boost. The bag slot is toWho-blind, so an ally's
 * +regen landed on whoever owned the power.
 *
 * **The Expression punt was safe only while the bag was there.** `resourceBuffValue` abstained
 * on any Expression-typed resource atom because the converter drops the `tick_chance`-0 ones
 * and `Expression ⟺ dropped` is false. Its stated reason was "safe either way: if the bag kept
 * it we fall back to the bag's value" — and the strip is exactly the change that makes the
 * other way unsafe, because abstention stops meaning "ask the bag" and starts meaning zero.
 * Measured before closing it: every Expression resource atom reaching the reader belongs to a
 * power whose bag KEPT the slot, and the reconstructed value equals the bag's on all 36. The
 * casualty avoided is Gamma Boost's +regen and +recovery on all four forks.
 *
 * **One stealth carrier leaves and it is not a stealth row.** 106 powers carry a bag `stealth`
 * the reader declines; 105 are the teleport family's `{translucency: …}` under a key this block
 * never reads, credited 0. The 106th is Assassin's Strike, whose four atoms are a Meta, two
 * Damage and a GrantPower — no stealth row anywhere — so its bag `stealthPvE/PvP` came through
 * a grant edge the atom reader does not follow. That is the grant-crossing question RB5-d owns,
 * not a gap in this reader, and it is a Click either way.
 *
 * ABSORB is absent from this file on purpose: it is the one family BPORT11 declined to carry.
 * See the block comment in the oracle and the ABSORB-4 residual.
 *
 * **BPORT13 restated the arms.** BPORT7 removed `power.effects`, so the bag half of every
 * comparison above is gone. Three different things follow, and lumping them together is how a
 * file like this quietly stops grading anything:
 *
 *  1. **Carrier counts survive their oracle.** `tohitBuff` agreeing with the bag on 874
 *     carriers and diverging nowhere is the same fact as the reader having exactly 874
 *     carriers, once the bag has none. Those arms are now one-armed censuses pinned per fork —
 *     per fork because a total is not a roster and cross-fork movement is the drift that
 *     actually happens here.
 *  2. **Populations defined BY the bag can still be restated, if the property is atom-side.**
 *     "68 declined damage buffs and every one is Defiance" was scoped by the bag holding a
 *     value; but `damageBuffIsDefianceOnly` is an atom reader, so the same claim is askable of
 *     the atoms directly. Same for the 46 + 76 ally-directed resource buffs, whose property
 *     was always `reachesCaster` being false.
 *  3. **Two arms genuinely lose their subject, and say so.** The Expression punt's 36
 *     agreements and 15 twin-routed carriers were counts of bag values; so was the stealth
 *     comparison's 341. Nothing on the atom side reconstructs which powers the bag once held,
 *     so those numbers retire in place, with the halves of each claim that ARE atom-side kept
 *     live below rather than retired alongside them.
 */
import { describe, it, expect } from 'vitest';
import {
  toHitBuffValue, damageBuffValue, damageBuffIsDefianceOnly, selfDamageDebuffValue,
  selfRechargeDebuffValue, regenBuffValue, recoveryBuffValue, maxHPBuffValue, stealthValue,
  maxEndBuffValue, baseAtoms, reachesCaster, isDebuffAtom,
} from '@/data/core/atom-query';
import { isSelfDirectedEffect } from '@/types';
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
import { ACCOLADES_POWERSET as HCACC } from '@/data/datasets/homecoming/generated/accolades';
import { ACCOLADES_POWERSET as RBACC } from '@/data/datasets/rebirth/generated/accolades';
import { ACCOLADES_POWERSET as TSACC } from '@/data/datasets/thunderspy/generated/accolades';
import { ACCOLADES_POWERSET as BSACC } from '@/data/datasets/brainstorm/generated/accolades';

type AnyPower = Record<string, unknown> & { name?: string; powerType?: string; effects?: Record<string, unknown> };
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
      for (const p of set?.powers ?? []) yield [`${label}/${setId}/${p.name}`, p];
}

const pair = (v: unknown) => {
  if (v === undefined || v === null) return undefined;
  if (typeof v === 'number') return `${v}@`;
  const o = v as { scale?: number; table?: string; perTarget?: number };
  if (typeof o.scale !== 'number') return `?${JSON.stringify(v)}`;
  return `${o.scale}@${(o.table ?? '').toLowerCase()}${o.perTarget ? `+${o.perTarget}` : ''}`;
};

type Split = { agree: number; differ: string[]; bagOnly: string[]; atomOnly: string[] };
const grade = (
  slot: string,
  arm: (p: AnyPower) => unknown,
  bagArm?: (p: AnyPower) => unknown,
): Split => {
  const t: Split = { agree: 0, differ: [], bagOnly: [], atomOnly: [] };
  for (const [id, p] of corpus()) {
    const b = pair(bagArm ? bagArm(p) : p.effects?.[slot]);
    const a = pair(arm(p));
    if (b === undefined && a === undefined) continue;
    if (b !== undefined && a !== undefined) {
      if (b === a) t.agree++; else t.differ.push(`${id} bag=${b} atom=${a}`);
    } else if (b !== undefined) t.bagOnly.push(`${id} bag=${b}`);
    else t.atomOnly.push(`${id} atom=${a}`);
  }
  return t;
};

/** Placeholder while the censuses below are being pinned; every use is replaced by a real map. */
const Z: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };

/** Post-strip the bag supplies nothing, so a carrier is an `atomOnly` row. Keyed by fork. */
const census = (t: Split): Record<string, number> => {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const row of t.atomOnly) out[row.split('/')[0]] += 1;
  return out;
};

/** A per-fork tally of ids shaped `fork/partition/set/name`. */
const byFork = (ids: string[]): Record<string, number> => {
  const out: Record<string, number> = { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 };
  for (const id of ids) out[id.split('/')[0]] += 1;
  return out;
};

/** The retired arm, asserted rather than assumed — a refilled bag reds here, not silently. */
const bagIsGone = (t: Split, slot: string): void => {
  expect([t.agree, ...t.differ, ...t.bagOnly], `${slot}: the bag arm answered`).toEqual([0]);
};

describe('BPORT11 cluster 5 — the last families, censused off the atoms', () => {
  it.each([
    ['tohitBuff', (p: AnyPower) => toHitBuffValue(p as never), 874,
      { homecoming: 230, rebirth: 205, thunderspy: 205, brainstorm: 234 }],
    ['tohitBuffUnenhanced', (p: AnyPower) => toHitBuffValue(p as never, { ignoreStrength: true }), 48,
      { homecoming: 13, rebirth: 4, thunderspy: 18, brainstorm: 13 }],
    ['maxHPBuff', (p: AnyPower) => maxHPBuffValue(p as never), 293,
      { homecoming: 99, rebirth: 45, thunderspy: 46, brainstorm: 103 }],
    ['maxHPBuffUnenhanced', (p: AnyPower) => maxHPBuffValue(p as never, { ignoreStrength: true }), 162,
      { homecoming: 48, rebirth: 33, thunderspy: 33, brainstorm: 48 }],
  ])('%s: the carrier census the bag comparison minted (%d), per fork', (slot, arm, total, expected) => {
    const t = grade(slot as string, arm as (p: AnyPower) => unknown);
    bagIsGone(t, slot as string);
    const c = census(t);
    expect(Object.values(c).reduce((a, b) => a + b, 0), `${slot} total`).toBe(total as number);
    expect(c, `${slot} carriers`).toEqual(expected);
  });

  it.each([
    ['damageDebuff', (p: AnyPower) => selfDamageDebuffValue(p as never), 43,
      { homecoming: 16, rebirth: 11, thunderspy: 2, brainstorm: 14 }],
    ['rechargeDebuff', (p: AnyPower) => selfRechargeDebuffValue(p as never), 8,
      { homecoming: 2, rebirth: 2, thunderspy: 2, brainstorm: 2 }],
  ])('%s: the self-tagged half, which is the only half spent (%d)', (slot, arm, total, expected) => {
    // The bag-side filter is KEPT rather than stubbed out, and that is load-bearing: the
    // override layer still supplies 23 homecoming `rechargeDebuff` entries (see the surviving-
    // supply test below). None is self-directed, so the call site drops all 23 and this arm's
    // bag half is legitimately empty — but it is empty because of the filter, not because the
    // slot is unsupplied, and an override turning self-directed must red here rather than
    // quietly rejoin the totals on one fork only.
    const t = grade(slot as string, arm as (p: AnyPower) => unknown,
      (p) => (isSelfDirectedEffect(p.effects?.[slot as string]) ? p.effects?.[slot as string] : undefined));
    bagIsGone(t, slot as string);
    const c = census(t);
    expect(Object.values(c).reduce((a, b) => a + b, 0), `${slot} total`).toBe(total as number);
    expect(c, slot).toEqual(expected);
  });

  it('declines 68 damage buffs and every one of them is Defiance', () => {
    // The population WAS bag-scoped — powers holding a `damageBuff` the reader refused. The
    // property behind it is not: a declined power is one with damage-buff atoms that
    // `damageBuffIsDefianceOnly` claims, so the same 68 are findable from the atom side, and
    // the claim gets stronger for it. Before, "declined" meant the bag disagreed; now it means
    // the reader would have answered and the Defiance gate stopped it.
    const declined = [...corpus()].filter(([, p]) => damageBuffIsDefianceOnly(p as never)).map(([id]) => id);
    expect(declined).toHaveLength(68);
    expect(byFork(declined)).toEqual({ homecoming: 31, rebirth: 0, thunderspy: 6, brainstorm: 31 });
    // Rebirth carries none, which is the shape of the finding rather than a gap in it: Defiance
    // is a Blaster inherent and Rebirth's Blaster secondaries state their rows differently.
    // Pinned per fork so that stays a measured fact and not an assumption.
    //
    // And the gate is exclusive, which is the half that keeps the count honest: no power is
    // BOTH claimed by the Defiance gate and answered for by the reader. If one ever were, the
    // call site would be skipping a real +damage buff on the strength of a Defiance rider.
    for (const [id, p] of corpus()) {
      if (!damageBuffIsDefianceOnly(p as never)) continue;
      expect(damageBuffValue(p as never), `${id} is Defiance-gated AND readable`).toBeUndefined();
    }
  });

  it.each([
    ['regenBuff', 'Regeneration', (p: AnyPower) => regenBuffValue(p as never), 50,
      { homecoming: 14, rebirth: 12, thunderspy: 10, brainstorm: 14 }],
    ['recoveryBuff', 'Recovery', (p: AnyPower) => recoveryBuffValue(p as never), 84,
      { homecoming: 28, rebirth: 14, thunderspy: 10, brainstorm: 32 }],
  ])('%s: drops only values the caster never receives (%s, %d)', (slot, type, arm, dropped, expected) => {
    // The dropped population was the bag's — a slot the reader declined. Its defining property
    // was never the bag's though, it was `reachesCaster`, so the set is reconstructible: a
    // power with buff-side resource atoms, none of which reach the caster, and a reader that
    // consequently says nothing. Adrenalin Boost, Painbringer, Temporal Selection, Speed Boost.
    //
    // The counts are 50 and 84, not the 46 and 76 the bag comparison reported, and the
    // difference is the bag's and not the reader's: `bagOnly` could only contain a power whose
    // bag ALSO held the slot, so it was the intersection of this set with the converter's
    // coverage. 4 and 8 ally-only carriers were outside it and invisible. The property is
    // identical on all of them; only the window widened.
    const allyOnly = [...corpus()].filter(([, p]) => {
      if ((arm as (q: AnyPower) => unknown)(p) !== undefined) return false;
      const atoms = baseAtoms(p as never).filter((a) => a.effectType === type
        && a.aspect !== 'Res' && !isDebuffAtom(a) && !a.notOnCaster);
      return atoms.length > 0 && !atoms.some((a) => reachesCaster(a, p as never));
    }).map(([id]) => id);
    expect(allyOnly, slot).toHaveLength(dropped as number);
    expect(byFork(allyOnly), slot).toEqual(expected);
    // And the other direction, which is the one that matters: every power the reader DOES
    // answer for has at least one atom that reaches the caster.
    const t = grade(slot as string, arm as (p: AnyPower) => unknown);
    bagIsGone(t, slot as string);
    for (const row of t.atomOnly) {
      const [id] = row.split(' atom=');
      const p = [...corpus()].find(([k]) => k === id)![1];
      const atoms = baseAtoms(p as never).filter((a) => a.effectType === type
        && a.aspect !== 'Res' && !isDebuffAtom(a) && !a.notOnCaster);
      expect(atoms.some((a) => reachesCaster(a, p as never)), id).toBe(true);
    }
  });

  it('closes the Expression punt on the half of it the atoms can still answer', () => {
    // The punt was: `resourceBuffValue` abstained on any Expression-typed resource atom,
    // reasoning that abstention meant "ask the bag". The strip is exactly the change that made
    // the other way unsafe — abstention now means zero — so the punt was closed and this
    // measured the closure against the bag: 36 agreements, 15 carriers routed to the
    // `ignoreStrength` twin, and 0 powers credited from a template the converter dropped.
    //
    // Two of those three numbers were counts of bag values and BPORT13 retires them; there is
    // nothing left to have agreed with. The third is the one the closure actually rested on
    // and it is atom-side, so it stays live below. Retiring the two rather than restating them
    // is deliberate: a count re-derived from the reader it grades would be the reader agreeing
    // with itself, which is what the bag was there to prevent.
    const droppedTemplateCarriers: string[] = [];
    for (const [id, p] of corpus()) {
      for (const [type, opts] of [
        ['Regeneration', {}], ['Regeneration', { ignoreStrength: true }],
        ['Recovery', {}], ['Recovery', { ignoreStrength: true }],
      ] as const) {
        const hasExpr = baseAtoms(p as never).some((a) => a.effectType === type
          && a.attribType === 'Expression' && a.aspect !== 'Res' && !isDebuffAtom(a) && !a.notOnCaster);
        if (!hasExpr) continue;
        const av = type === 'Regeneration'
          ? regenBuffValue(p as never, opts) : recoveryBuffValue(p as never, opts);
        if (av !== undefined) droppedTemplateCarriers.push(`${id} ${type}`);
      }
    }
    // The population the punt existed for: an Expression row the converter DROPPED that still
    // reaches this reader and would be credited as a real buff. Every Expression resource atom
    // that survives to the reader belongs to a power whose template was kept, so the reader
    // answering for one is not a risk — the risk was answering for a dropped one, and there
    // are none. Thunderspy's Fortify Pack is the single corpus power carrying a dropped-shape
    // Expression row and it is `toWho: Target` and `notOnCaster`, declined for a stated reason.
    expect(droppedTemplateCarriers.every((r) => !/Fortify Pack/.test(r))).toBe(true);
    // And the casualty the closure avoids, named so the closure has a subject: abstaining
    // would have zeroed Gamma Boost's +regen and +recovery on all four forks.
    const gamma = [...corpus()].filter(([id]) => id.endsWith('/Gamma Boost'));
    // 18 copies, not 4: the power appears in a powerset and in the epic tier on each fork, and
    // a per-fork count is what says so. The original said only `> 0`, which would have passed
    // on a single surviving copy after the other seventeen went silent.
    expect(gamma).toHaveLength(18);
    expect(byFork(gamma.map(([id]) => id))).toEqual({ homecoming: 5, rebirth: 4, thunderspy: 4, brainstorm: 5 });
    for (const [id, p] of gamma) {
      expect(regenBuffValue(p as never), id).toBeDefined();
      expect(recoveryBuffValue(p as never), id).toBeDefined();
    }
  });

  it('answers for 341 stealth carriers, and the one it declines is not a stealth row', () => {
    // The bag comparison here was 106 declined carriers, 105 of them the teleport family's
    // `{translucency: …}` under a key this block never reads. Those 105 were bag-only rows and
    // went with the strip. What did NOT go is the 106th, because the override layer still
    // supplies it: Assassin's Strike is one of the 36 surviving homecoming overrides, so the
    // named case the original claim turned on is still here to be asked, and it is now the
    // WHOLE declined population rather than one entry on a list of 106.
    const declined = [...corpus()].filter(([, p]) => p.effects?.stealth && !stealthValue(p as never));
    expect(declined).toHaveLength(1);
    expect(declined[0][0]).toContain("Assassin's Strike");
    // And the reason it declines, which was always the actual claim: the power carries no
    // stealth atom at all. Its bag `stealthPvE/PvP` came through a grant edge the atom reader
    // does not follow — the grant-crossing question RB5-d owns, not a gap in this reader.
    expect(baseAtoms(declined[0][1] as never).some((a) => a.effectType === 'Stealth')).toBe(false);
    expect((declined[0][1] as AnyPower).powerType).toBe('Click');
    // The carrier census, per fork — the 341 the bag comparison agreed on.
    const answered = [...corpus()].filter(([, p]) => stealthValue(p as never)).map(([id]) => id);
    expect(answered).toHaveLength(341);
    expect(byFork(answered)).toEqual({ homecoming: 114, rebirth: 67, thunderspy: 41, brainstorm: 119 });
  });

  it('reads the accolade +MaxEnd off the atoms, on the 28 carriers the bag agreed', () => {
    const accolades = [
      ['homecoming', HCACC], ['rebirth', RBACC], ['thunderspy', TSACC], ['brainstorm', BSACC],
    ] as unknown as [string, { powers?: AnyPower[] }][];
    const carriers: string[] = [];
    for (const [fork, set] of accolades)
      for (const p of set?.powers ?? []) {
        // Accolades were never stripped-adjacent: the bag held no `maxEndBuff` here after
        // BPORT7 either, so this is the census, not a comparison.
        expect(pair(p.effects?.maxEndBuff), `${fork}/${p.name}`).toBeUndefined();
        if (maxEndBuffValue(p as never) !== undefined) carriers.push(`${fork}/${p.name}`);
      }
    expect(carriers).toHaveLength(28);
    expect(byFork(carriers)).toEqual({ homecoming: 8, rebirth: 6, thunderspy: 6, brainstorm: 8 });
  });

  it('retires effects.enduranceCost against a population of nothing', () => {
    // Vacuous on its own terms now — no power carries any bag slot the converter wrote. It is
    // kept because the surviving-supply census below says exactly which slots DO still have a
    // carrier, and `enduranceCost` is not one of them: this is the tripwire for that slot in
    // particular, and it can still red if an override re-adds it.
    const carriers = [...corpus()].filter(([, p]) => p.effects?.enduranceCost !== undefined);
    expect(carriers).toEqual([]);
  });

  it('names the bag supply STRIP-1 left standing: 36 homecoming override files', () => {
    // BPORT7 emptied the CONVERTER's bag. It did not empty the hand-written overrides layer,
    // which is a separate supplier and was not on STRIP-1's list of five. 36 files under
    // `src/data/datasets/homecoming/overrides/` still carry an `effects` key and no other fork
    // has one — so any reader with a `?? effects.slot` seam answers differently on Homecoming
    // than on the other three, which is precisely the fork-shaped hole a per-power hand-list
    // leaves and the reason TEAMBUFF-1 was a bug.
    //
    // Pinned as a census rather than fixed here: retiring these is an overrides-layer audit
    // with its own oracle question (is the parser emitting the fact yet?), not a test edit.
    // What this guard owes is that the population cannot grow or move fork unnoticed.
    const bySlot: Record<string, Record<string, number>> = {};
    for (const [id, p] of corpus()) {
      for (const slot of Object.keys(p.effects ?? {})) {
        (bySlot[slot] ??= { homecoming: 0, rebirth: 0, thunderspy: 0, brainstorm: 0 })[id.split('/')[0]] += 1;
      }
    }
    expect(bySlot).toEqual({
      rechargeDebuff: { homecoming: 23, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      buffDuration: { homecoming: 21, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      stealth: { homecoming: 6, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      taunt: { homecoming: 2, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      stun: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      movement: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      durations: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
      effectDuration: { homecoming: 1, rebirth: 0, thunderspy: 0, brainstorm: 0 },
    });
    // And the one that would actually move a total: none of the 23 `rechargeDebuff` entries is
    // self-directed, so the call site's filter drops every one and the caster's totals never
    // see them. That is why the arm above reads an empty bag half. If an override ever became
    // self-directed it would rejoin the totals on one fork alone, so it is asserted, not noted.
    for (const [id, p] of corpus()) {
      const v = p.effects?.rechargeDebuff;
      if (v === undefined) continue;
      expect(isSelfDirectedEffect(v), `${id}: an override rejoined the totals`).toBe(false);
    }
  });
});
