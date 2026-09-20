/**
 * BPORT11 — the suppliers a data-seam retirement cannot see, and the arms that must survive it.
 *
 * `power.effects` has five suppliers and BPORT7's regen empties exactly one of them. Two of the
 * other four are built by the totals pass itself: `expandActiveConditionals` turns an active
 * stance or mode into a synthetic power whose `effects` object is the conditional's own, and
 * `expandBuffPetAuras` folds a toggled pet's aura into a second one. Neither synthetic carries
 * `atoms` — there is nothing to carry, the value is minted here — so an atom-native arm answers
 * `undefined` for every one of them.
 *
 * A family that retires its bag branch outright therefore reads 0 for its whole synthetic
 * population, and reads it silently: no exception, no unmapped key, just a Bio Armor stance
 * that stops contributing. {@link syntheticEffects} is the branch that keeps them, named so it
 * is distinguishable from the DATA read beside it.
 *
 * **This is not hypothetical, and it is why the census below is a guard rather than a note.**
 * The canonical fork retired its bag branches in one pass and dropped the synthetic arm on ten
 * slots its own conditionals still mint — the same four datasets ship in both repos, so the
 * populations are identical. Measured through each arm's own credit gate: recoveryBuffUnenhanced
 * 110, regenBuffUnenhanced 95, maxHPBuffUnenhanced 43, enduranceDiscount 29, tohitBuff 25,
 * rechargeBuff 20, slow 12, maxEndBuff 6, knockback / immobilize / knockup 4 each,
 * perceptionBuff / rangeBuff / mezResistance 2 each, maxHPBuff 1 — 359 credited contributions,
 * every one of them a named Bio Armor, Temporal Manipulation, Dual Blades or Kheldian stance.
 * Its pet-aura fold was re-keyed from `effects.defense` to `effects.defenseBuff` so the defence
 * arm kept working. This repo's fold did the same at BPORT11c's defence carry, and it did it
 * because the pin below went red — `effects.defense` has no converted carrier on any fork, so
 * the fold was its only supplier and the old key would have gone quiet.
 *
 * So the invariant, checked against the oracle's source: a slot with a credited synthetic
 * supplier is read either through the bag (not carried yet) or through `syntheticEffects`
 * (carried, arm kept) — never through neither. It grades the arms as they are written, so each
 * remaining cluster inherits the check without an edit here.
 */
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { AT_INHERENT_CONDITIONAL_IDS } from '@/utils/conditional-effects';
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

type AnyPower = Record<string, unknown> & {
  name?: string; targetType?: string;
  conditionalEffects?: { id: string; label?: string; effects?: Record<string, unknown> }[];
};
type Tree = Record<string, { powers?: AnyPower[] }>;

const PARTITIONS: readonly (readonly [string, Tree])[] = [
  ['homecoming/set', HC as unknown as Tree], ['rebirth/set', RB as unknown as Tree],
  ['thunderspy/set', TSPY as unknown as Tree], ['brainstorm/set', BS as unknown as Tree],
  ['homecoming/pool', HCP as unknown as Tree], ['homecoming/epic', HCE as unknown as Tree],
  ['rebirth/pool', RBP as unknown as Tree], ['rebirth/epic', RBE as unknown as Tree],
  ['thunderspy/pool', TSP as unknown as Tree], ['thunderspy/epic', TSE as unknown as Tree],
  ['brainstorm/pool', BSP as unknown as Tree], ['brainstorm/epic', BSE as unknown as Tree],
];

const isRes = (v: unknown) => {
  const t = (v && typeof v === 'object' && typeof (v as { table?: unknown }).table === 'string')
    ? (v as { table: string }).table.toLowerCase() : '';
  return t.includes('res_boolean');
};
const scaleOf = (v: unknown) =>
  typeof v === 'number' ? v
    : (v && typeof v === 'object' && typeof (v as { scale?: unknown }).scale === 'number'
      ? (v as { scale: number }).scale : 0);
const MEZ_RES_KEYS = new Set(['hold', 'stun', 'sleep', 'immobilize', 'confuse', 'fear', 'knockback']);
const MOVE_KEYS = new Set(['runSpeed', 'flySpeed', 'fly', 'jumpHeight', 'jumpSpeed']);
const DEBUFF_RES_KEYS = new Set(['defense', 'movement', 'endurance', 'recovery', 'regeneration',
  'tohit', 'accuracy', 'recharge', 'range', 'perception']);
const isMap = (v: unknown) => !!v && typeof v === 'object';
const someSelf = (v: unknown, keys?: Set<string>) => isMap(v)
  && Object.entries(v as Record<string, unknown>)
    .some(([k, x]) => (!keys || keys.has(k)) && isSelfDirectedEffect(x));

/**
 * Each arm's own credit gate, transcribed from its call site in the oracle. Minting a slot is
 * not being credited through it — Dual Pistols' Ice Ammo mints `slow` 175 times and every one
 * is foe-facing, which `isSelfDirectedEffect` has always dropped. Counting mints would have
 * reported four times the real exposure and made the guard easy to dismiss.
 */
const GATE: Record<string, (v: unknown, p: AnyPower, all: Record<string, unknown>) => boolean> = {
  tohitBuff: () => true,
  tohitBuffUnenhanced: () => true,
  accuracyBuff: () => true,
  damageBuff: () => true,
  damageDebuff: (v, _p, all) => isSelfDirectedEffect(v) && all.damageBuff === undefined,
  defenseBuff: isMap,
  defenseBuffSuppressible: isMap,
  defense: isMap,
  resistance: isMap,
  resistanceDebuff: (v) => someSelf(v),
  debuffResistance: (v) => isMap(v)
    && Object.keys(v as object).some((k) => DEBUFF_RES_KEYS.has(k.toLowerCase())),
  mezResistance: (v) => isMap(v)
    && Object.keys(v as object).some((k) => MEZ_RES_KEYS.has(k.toLowerCase())),
  elusivity: isMap,
  movement: (v, _p, all) => isMap(v) && all.tohitDebuff === undefined && all.damageDebuff === undefined,
  movementCapDebuff: (v) => someSelf(v, MOVE_KEYS),
  slow: (v) => someSelf(v, MOVE_KEYS),
  rechargeBuff: () => true,
  rechargeDebuff: (v) => isSelfDirectedEffect(v),
  regenBuff: (v) => !isRes(v),
  regenBuffUnenhanced: () => true,
  recoveryBuff: (v) => !isRes(v),
  recoveryBuffUnenhanced: () => true,
  maxHPBuff: () => true,
  maxHPBuffUnenhanced: () => true,
  maxEndBuff: () => true,
  absorb: (v) => v !== undefined && v !== null,
  enduranceDiscount: (v) => scaleOf(v) > 0,
  perceptionBuff: (v) => scaleOf(v) > 0,
  rangeBuff: (v, p) => p.targetType?.toLowerCase() === 'self' && scaleOf(v) > 0,
  repel: (v) => Math.abs(scaleOf(v)) > 0,
  teleport: (v) => scaleOf(v) > 0,
  taunt: (v) => typeof v !== 'number' && isRes(v),
  placate: (v) => typeof v !== 'number' && isRes(v),
  stealth: (v) => !!v,
  hold: (v) => typeof v !== 'number' && isRes(v),
  stun: (v) => typeof v !== 'number' && isRes(v),
  sleep: (v) => typeof v !== 'number' && isRes(v),
  immobilize: (v) => typeof v !== 'number' && isRes(v),
  confuse: (v) => typeof v !== 'number' && isRes(v),
  fear: (v) => typeof v !== 'number' && isRes(v),
  knockback: (v) => typeof v !== 'number' && isRes(v),
  knockup: (v) => typeof v !== 'number' && isRes(v),
};

/** Slots a REACHABLE conditional mints and the arm would credit, with their populations. */
function conditionalCredits(): Map<string, number> {
  const credit = new Map<string, number>();
  for (const [, tree] of PARTITIONS)
    for (const [, set] of Object.entries(tree))
      for (const p of set?.powers ?? [])
        for (const c of p.conditionalEffects ?? []) {
          // The two skips `expandActiveConditionals` applies before it synthesizes anything.
          if (AT_INHERENT_CONDITIONAL_IDS.has(c.id)) continue;
          if (!c.effects || Object.keys(c.effects).length === 0) continue;
          for (const [k, v] of Object.entries(c.effects)) {
            if (!(k in GATE) || !GATE[k](v, p, c.effects)) continue;
            credit.set(k, (credit.get(k) ?? 0) + 1);
          }
        }
  return credit;
}

const ORACLE = path.resolve(__dirname, 'legacy-totals.oracle.ts');
const source = () => readFileSync(ORACLE, 'utf8');

/** The pass that spends the bag, isolated so the pet-aura fold's WRITES are not read as reads. */
function applyBlock(src: string): string {
  const start = src.indexOf('function applyActivePowerBonuses(');
  expect(start, 'applyActivePowerBonuses is the block this guard grades').toBeGreaterThan(-1);
  const end = src.indexOf('\nfunction ', start + 1);
  return src.slice(start, end === -1 ? src.length : end);
}

describe('BPORT11 — every slot a synthetic still supplies keeps an arm that can read it', () => {
  const credits = conditionalCredits();

  it('counts what a reachable conditional would actually be credited for', () => {
    // Pinned so the population is a measurement rather than a claim. A dataset refresh that
    // moves these is a fact worth seeing; a refresh that empties one is worth seeing more.
    // Two moved at i28p4 RC3 and both are one cause, named rather than bumped: damageBuff
    // 48 -> 52 is Spotlight and regenBuff 20 -> 24 is Lightfield, each on the same four Light
    // Affinity ATs and each gated on `chain_jolt_mode`. That is upstream's own token — HC hung
    // the Radiance rework on the existing Chain Jolt mode rather than minting one — so the
    // gate reads oddly here and in the UI, and is faithful to the export either way.
    const top = [...credits].filter(([, n]) => n >= 20).sort((a, b) => b[1] - a[1]);
    expect(Object.fromEntries(top)).toEqual({
      recoveryBuffUnenhanced: 110,
      regenBuffUnenhanced: 95,
      resistance: 65,
      defenseBuff: 64,
      damageBuff: 52,
      absorb: 50,
      maxHPBuffUnenhanced: 43,
      tohitBuffUnenhanced: 37,
      enduranceDiscount: 29,
      tohitBuff: 25,
      regenBuff: 24,
      rechargeBuff: 20,
    });
    // The gate is doing real work, not waving everything through: `slow` mints 175 times and
    // is credited 12, `rechargeDebuff` mints 134 and is credited none — both are foe debuffs
    // wearing a slot the caster's totals also use.
    expect(credits.get('slow')).toBe(12);
    expect(credits.get('rechargeDebuff') ?? 0).toBe(0);
  });

  it('reads no synthetic contribution through an arm that has no branch for it', () => {
    const block = applyBlock(source());
    const orphaned: string[] = [];
    for (const [slot, n] of credits) {
      const dataRead = new RegExp(`\\beffects\\.${slot}\\b`).test(block);
      const synthRead = new RegExp(`syntheticEffects\\([^)]*\\)\\?\\.${slot}\\b`).test(block);
      // The mez fold indexes its slot from a roster (`[field]`), so it names none of its eight
      // slots at the read site and neither the data nor the synthetic pattern above can see
      // it. That is BPORT6's finder lesson one level down — a roster-bound read is minted from
      // a register, never found — so it is asked for by shape instead: either arm counts, and
      // BOTH must disappear together for these eight to become orphans.
      const ROSTER_SLOTS = ['hold', 'stun', 'sleep', 'immobilize', 'confuse', 'fear',
        'knockback', 'knockup'];
      const rosterRead = ROSTER_SLOTS.includes(slot)
        && (/\beffects\[field\]/.test(block) || /syntheticEffects\([^)]*\)\?\.\[field\]/.test(block));
      if (!dataRead && !synthRead && !rosterRead) orphaned.push(`${slot} (${n} credited)`);
    }
    expect(orphaned.sort()).toEqual([]);
  });

  it('marks both synthetic producers, or the channel reads nothing at all', () => {
    const src = source();
    // The accessor is a no-op without the marker, and the marker is set in exactly two places.
    // A third producer added without one would be invisible to every arm that keeps a branch.
    expect(src.match(/syntheticContribution: true/g) ?? []).toHaveLength(2);
    for (const fn of ['expandActiveConditionals', 'expandBuffPetAuras']) {
      const start = src.indexOf(`function ${fn}(`);
      expect(start, fn).toBeGreaterThan(-1);
      const end = src.indexOf('\nfunction ', start + 1);
      expect(src.slice(start, end === -1 ? src.length : end), fn)
        .toContain('syntheticContribution: true');
    }
  });

  it('names the pet-aura fold slots, which are keyed on this side and not the other', () => {
    const src = source();
    const start = src.indexOf('function buffPetAuraEffects(');
    const end = src.indexOf('\nfunction ', start + 1);
    const fold = src.slice(start, end === -1 ? src.length : end);
    const mints = [...fold.matchAll(/effects\.(\w+) =/g)].map((m) => m[1]);
    // `defenseBuff`, not `defense`. This pin is what made the decision visible: the defence
    // carry went red here the moment it landed, because `effects.defense` had this fold as its
    // ONLY supplier on any fork — 0 converted powers carry the slot — so retiring the data read
    // and leaving the fold on the old key would have zeroed every buff-pet defence aura in
    // silence. Canonical re-keyed its own copy for the same reason.
    expect([...new Set(mints)].sort()).toEqual([
      'absorb', 'defenseBuff', 'rechargeBuff', 'recoveryBuff', 'regenBuff', 'resistance',
      'tohitBuffUnenhanced',
    ]);
  });
});
