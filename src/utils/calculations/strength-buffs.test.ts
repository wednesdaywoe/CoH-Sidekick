import { describe, it, expect, beforeAll } from 'vitest';
import { collectStrengthBuffs, calculateCharacterTotals } from './character-totals';
// The beta imported `legacyCalculateCharacterTotals` here, for cases that fed the calc
// SYNTHETIC power definitions the engine cannot resolve. That justification no longer matches
// a call site in this file: every `calculateCharacterTotals` call goes through `buildWith`,
// whose Hover and Power Boost are both REAL dataset powers. Post-strip the oracle skips them
// at its `if (!power.effects) continue` (BPORT13), so three of this block's four tests were
// passing on zeros. The synthetic powers in this file go to `collectStrengthBuffs`, which is
// unaffected. Carrying canonical's live reader is what makes the block grade again.
import { loadDataset } from '@/data/dataset';
import { getEpicPool } from '@/data/epic-pools';
import { createEmptyBuild } from '@/types/build';
import { calcThreeTier } from '@/components/info/powerDisplayUtils';
import { shouldShowToggle } from '@/components/powers/power-row-utils';
import { getPowerset } from '@/data/powersets';
import { getTableValue } from '@/data/at-tables';
import { encodeAtom, type AtomicEffect } from '@/data/core/atomic-effect';
import {
  atomsOf, baseAtoms, specialBuffValue, damageBuffValue, defenseBuffValue,
  toHitBuffValue, mezSlotValue,
} from '@/data/core/atom-query';

/**
 * Tests for the Power Boost / +Strength mechanic.
 *
 * collectStrengthBuffs is exercised with synthetic powers whose +Strength ATOMS
 * use the `Melee_Ones` table — resolveScaledEffect returns `scale × 1`
 * for *_Ones tables, so the resolved strength fraction equals the raw scale,
 * making assertions deterministic without depending on AT modifier tables.
 */

const ONES = 'Melee_Ones';

// Minimal PowerWithToggle-shaped object (the type is module-private).
// eslint-disable-next-line @typescript-eslint/no-explicit-any
const mk = (over: any) => ({ name: 'P', internalName: 'P', powerType: 'Click', isActive: false, ...over } as any);

// A +Strength atom — the `aspect: Str` row `specialBuffValue` folds into one map entry.
// The tests state ATOMS rather than a `specialBuff` map because the map is no longer an
// input: it is reconstructed from these, so handing the reader a hand-authored bag would
// only grade the assertion against itself.
const strAtom = (over: Partial<AtomicEffect>): AtomicEffect => ({
  effectType: 'Enhancement', pvMode: 'Any', resistible: true, toWho: 'Self',
  attribType: 'Magnitude', aspect: 'Str', modifierTable: ONES, scale: 1, magnitude: 1,
  duration: 10, stacking: 'Replace', baseProbability: 1, ...over,
});

// A Power Boost-like atom list: defense (aggregate + positional + typed), the six mez
// kinds, tohit, heal, absorb, endmod, movement — all at the same scale, mirroring the real
// binary, which enumerates every defense/mez sub-attribute uniformly.
const powerBoostAtoms = (scale: number) => [
  ...['All', 'Melee', 'Ranged', 'AoE', 'Smashing', 'Lethal', 'Fire',
    'Held', 'Stunned', 'Sleep', 'Confused', 'Terrorized', 'Immobilized',
  ].map((subType) => strAtom({ subType: subType as AtomicEffect['subType'], scale })),
  ...(['ToHit', 'Heal', 'Absorb', 'Endurance', 'Movement'] as const)
    .map((effectType) => strAtom({ effectType, scale })),
].map(encodeAtom);

describe('collectStrengthBuffs', () => {
  it('returns zero when no strength powers are active', () => {
    const sb = collectStrengthBuffs([
      mk({ internalName: 'PowerBoost', isActive: false, atoms: powerBoostAtoms(1.2) }),
    ], 'controller', 50);
    expect(sb).toEqual({ defense: 0, toHit: 0, heal: 0, absorb: 0, endMod: 0, movement: 0, mez: 0 });
  });

  it('collapses uniform defense/mez sub-keys to a single representative value (no 12x overcount)', () => {
    const sb = collectStrengthBuffs([
      mk({ internalName: 'PowerBoost', isActive: true, atoms: powerBoostAtoms(1.2) }),
    ], 'controller', 50);
    // defense and mez are the MAX of their sub-keys (uniform 1.2), not the sum
    expect(sb.defense).toBeCloseTo(1.2, 6);
    expect(sb.mez).toBeCloseTo(1.2, 6);
    // single-keyed aspects pass through
    expect(sb.toHit).toBeCloseTo(1.2, 6);
    expect(sb.heal).toBeCloseTo(1.2, 6);
    expect(sb.absorb).toBeCloseTo(1.2, 6);
    expect(sb.endMod).toBeCloseTo(1.2, 6);
    expect(sb.movement).toBeCloseTo(1.2, 6);
  });

  it('counts Auto powers as active even without an explicit isActive toggle', () => {
    const sb = collectStrengthBuffs([
      mk({ internalName: 'AutoBuff', powerType: 'Auto', isActive: false, atoms: [strAtom({ subType: 'All', scale: 0.5 })].map(encodeAtom) }),
    ], 'controller', 50);
    expect(sb.defense).toBeCloseTo(0.5, 6);
  });

  it('sums strength across multiple active strength powers', () => {
    const sb = collectStrengthBuffs([
      mk({ internalName: 'PowerBoost', isActive: true, atoms: [strAtom({ subType: 'All', scale: 1.2 }), strAtom({ effectType: 'ToHit', scale: 1.2 })].map(encodeAtom) }),
      mk({ internalName: 'PowerBuildUp', isActive: true, atoms: [strAtom({ subType: 'All', scale: 0.66 }), strAtom({ effectType: 'ToHit', scale: 0.66 })].map(encodeAtom) }),
    ], 'controller', 50);
    expect(sb.defense).toBeCloseTo(1.86, 6);
    expect(sb.toHit).toBeCloseTo(1.86, 6);
  });

  it('honors self-stacking read off the atoms, and the targets-hit slider', () => {
    // The depth comes from the ATOMS, not from `effects.maxStacks` / `stacksLinear` — those
    // two slots left the contract with the bag, and STACK-7 took their last reader. A
    // `Str`-aspect self row stating `Stack` at limit 2 IS the whole answer: membership,
    // value and depth in one place, now that the value map is read off the same rows.
    const stacksTwice = (over: Partial<AtomicEffect>): AtomicEffect =>
      strAtom({ stacking: 'Stack', stackCap: 2, ...over });
    const pb = () => mk({
      internalName: 'PB', isActive: true,
      atoms: [stacksTwice({ subType: 'All' }), stacksTwice({ subType: 'Held' })].map(encodeAtom),
    });
    // 2 stacks → doubled
    const two = collectStrengthBuffs([pb()], 'controller', 50, { PB: 2 });
    expect(two.defense).toBeCloseTo(2.0, 6);
    expect(two.mez).toBeCloseTo(2.0, 6);
    // slider beyond the atom's own cap is clamped to 2
    const capped = collectStrengthBuffs([pb()], 'controller', 50, { PB: 5 });
    expect(capped.defense).toBeCloseTo(2.0, 6);
    // and a power whose Str rows do NOT self-stack ignores the slider entirely
    const flat = mk({
      internalName: 'PB', isActive: true,
      atoms: [stacksTwice({ subType: 'All', stacking: 'Replace', stackCap: undefined })].map(encodeAtom),
    });
    expect(collectStrengthBuffs([flat], 'controller', 50, { PB: 5 }).defense).toBeCloseTo(1.0, 6);
  });

  it('ignores ally-only / non-specialBuff powers and never invents damage', () => {
    const sb = collectStrengthBuffs([
      // A flat +Defense buff: `aspect: Cur`, the CURRENT-value face. Same attrib as the
      // `All` row above and the same table — only the aspect separates a defense buff from
      // a defense-STRENGTH buff, which is the collapse the axis exists to prevent.
      mk({ internalName: 'Weave', isActive: true, atoms: [strAtom({ effectType: 'Defense', subType: 'Melee', aspect: 'Cur', scale: 0.5 })].map(encodeAtom) }),
    ], 'controller', 50);
    // Nothing on the `Str` face → no strength at all
    expect(sb).toEqual({ defense: 0, toHit: 0, heal: 0, absorb: 0, endMod: 0, movement: 0, mez: 0 });
  });

  it('excludes foe-directed +Strength rows (-Special debuffs like Benumb/Time Stop)', () => {
    // The recipient is read off each ATOM, not off the power. The bag path had to skip the
    // whole power on `targetType`, because a foe -Special stored its magnitude as a POSITIVE
    // `specialBuff` with nothing on the slot to say whose strength it was; `toWho: Target`
    // on a power that affects no `Self` says it directly.
    const sb = collectStrengthBuffs([
      mk({
        internalName: 'TimeStop', isActive: true, targetType: 'Foe', targetsAffected: ['Enemy'],
        atoms: [strAtom({ subType: 'Held', toWho: 'Target' }), strAtom({ subType: 'Immobilized', toWho: 'Target' })].map(encodeAtom),
      }),
    ], 'controller', 50);
    expect(sb.mez).toBe(0);
    expect(sb).toEqual({ defense: 0, toHit: 0, heal: 0, absorb: 0, endMod: 0, movement: 0, mez: 0 });
  });
});

describe('shouldShowToggle — Power Boost family is activatable', () => {
  // Stated on ATOMS for the same reason `collectStrengthBuffs` above is: the bag these four
  // used to hand in (`effects: { specialBuff: ... }`) is not an input to this predicate any
  // more, and after the bag strip it is not an input to anything. Handed a synthetic bag and
  // no atoms, all four went on passing or failing for a reason unrelated to the power —
  // which is how the two positive cases survived a predicate that had gone blind.
  const selfClick = (atoms: unknown[]) =>
    ({ powerType: 'Click', targetType: 'Self', targetsAffected: ['Self'], atoms }) as never;
  // ONLY `Enhancement`/`Str` rows — no ToHit, Heal, Absorb or Endurance riders. The full
  // `powerBoostAtoms` list carries those too, and each answers a query of its own, so an
  // input built from it stays green with `specialBuffValue` deleted outright and proves
  // nothing about the family this block is named for.
  const specialOnlyAtoms = ['All', 'Melee', 'Ranged', 'Smashing']
    .map((subType) => strAtom({ subType: subType as AtomicEffect['subType'], scale: 0.66 }))
    .map(encodeAtom);

  it('shows a toggle for a self Click whose only rows are +Strength (Power Boost)', () => {
    expect(shouldShowToggle(selfClick(specialOnlyAtoms))).toBe(true);
  });
  it('still shows a toggle for Build Up (self Click with a +Damage strength row)', () => {
    const buildUp = [strAtom({ effectType: 'DamageBuff', subType: 'Smashing', scale: 8, modifierTable: 'Melee_Buff_Dmg' })].map(encodeAtom);
    expect(shouldShowToggle(selfClick(buildUp))).toBe(true);
  });
  it('does NOT show a self toggle for a Foe Click whose +Strength rows are foe-directed', () => {
    // The -Special debuffs (Benumb, Weaken, Time Stop). Stated on the SAME rows the positive
    // case uses, turned around: the only difference is `toWho`, so this fails the moment the
    // recipient axis stops being read, and cannot pass for some unrelated declining filter.
    const foeSpecial = ['All', 'Melee', 'Ranged', 'Smashing']
      .map((subType) => strAtom({ subType: subType as AtomicEffect['subType'], scale: 0.66, toWho: 'Target' }))
      .map(encodeAtom);
    expect(shouldShowToggle({
      powerType: 'Click', targetType: 'Foe', targetsAffected: ['Enemy'], atoms: foeSpecial,
    } as never)).toBe(false);
  });
  it('does NOT show a toggle for a plain damage Click', () => {
    expect(shouldShowToggle({
      powerType: 'Click', targetType: 'Foe', targetsAffected: ['Enemy'],
      damage: { type: 'Smashing', scale: 1 }, atoms: [],
    } as never)).toBe(false);
  });
});

describe('Power Boost data integrity + endurance regression (rebirth)', () => {
  beforeAll(async () => {
    await loadDataset('rebirth');
  });

  it('Primal Forces Power Boost is a pure strength buff (no phantom damageBuff/flat defenseBuff)', () => {
    const pool = getEpicPool('primal_forces_mastery');
    expect(pool).toBeTruthy();
    const pb = pool!.powers.find(p => p.internalName === 'Power_Boost');
    expect(pb).toBeTruthy();

    // The positive half FIRST, because the three `toBeUndefined()`s below are silent about
    // why they are silent: a power whose `atoms` the epic transform failed to carry through
    // would starve every reader and pass all three. Assert the atoms are there, and that the
    // reason the other readers decline is the ASPECT — every row is `Str`, the strength face,
    // which is what "pure strength buff" means on the atom.
    const atoms = atomsOf(pb!);
    expect(atoms.length).toBeGreaterThan(0);
    expect([...new Set(atoms.map(a => a.aspect))]).toEqual(['Str']);

    expect(specialBuffValue(pb!)).toBeTruthy();
    expect(damageBuffValue(pb!)).toBeUndefined();
    expect(defenseBuffValue(pb!)).toBeUndefined();
    expect(toHitBuffValue(pb!)).toBeUndefined();
  });

  it('Power Boost (a Click) costs ~9.75 end, and no per-second division doubles it', () => {
    const pool = getEpicPool('primal_forces_mastery');
    const pb = pool!.powers.find(p => p.internalName === 'Power_Boost');
    // The bag's pre-divided `effects.enduranceCost` is gone with the strip; the raw cost now
    // rides `stats`, and the toggle-vs-click division moved to the end-drain pass.
    expect(pb!.powerType).toBe('Click');
    expect(pb!.stats?.endurance).toBeCloseTo(9.75, 5);
  });

  it('a Click contributes nothing to the toggle end-drain pass', () => {
    // The other half of the 19.5 regression, and the half that can still recur: the doubling
    // only ever came from dividing a flat click cost by the 0.5s tick period. The pass now
    // gates on powerType at its loop head, so an ACTIVE Power Boost must charge zero — a
    // guard on the gate, not on the arithmetic behind it.
    const on = calculateCharacterTotals(
      buildWith({ pool: false, powerBoostActive: true }), false, undefined, { combatMode: true },
    );
    expect(on.globalBonuses.toggleEndCost).toBe(0);
  });
});

describe('Power Info Final formula applies strength (calcThreeTier)', () => {
  it('mez duration Final = base × (1 + enh + strength), strength lands only in Final', () => {
    // base 10s hold, +0.4 slotted enh, +1.2 Power Boost mez strength
    const t = calcThreeTier('hold', 10, { hold: 0.4 }, { hold: 1.2 });
    expect(t.base).toBeCloseTo(10, 6);
    expect(t.enhanced).toBeCloseTo(14, 6);      // strength NOT in the Enhanced tier
    expect(t.final).toBeCloseTo(26, 6);          // 10 × (1 + 0.4 + 1.2)
  });
});

// eslint-disable-next-line @typescript-eslint/no-explicit-any
function buildWith(powers: { pool?: boolean; powerBoostActive?: boolean }): any {
  const b = createEmptyBuild();
  b.serverId = 'rebirth';
  b.level = 50;
  b.archetype = { id: 'controller', name: 'Controller', stats: null, inherent: null } as any;
  if (powers.pool) {
    // Hover, not Combat Jumping: on Rebirth every one of Combat Jumping's defence atoms
    // is archetype-forked, and these bag-view readers deliberately hold no build to
    // resolve a fork against, so it contributes nothing here (AT-FORK-1 — the Rust
    // gather is the reader that resolves it). Hover carries the same shape — a self
    // defence toggle, unforked on every fork — which keeps this test about Power Boost.
    b.pools = [{ id: 'flight', name: 'Flight', powers: [
      // `powerSet` and `level` are the beta's, not carried from canonical: here
      // `calculateCharacterTotals` IS the engine, and the engine resolves a selected power from
      // its own contract bundle by (powerSet, internalName). A fixture power with no `powerSet`
      // resolves to nothing and contributes zero — which is how three of this block's four tests
      // passed while the fourth asked for a positive number.
      { internalName: 'Combat_Flight', name: 'Hover', powerSet: 'flight', level: 1, isActive: true, slots: [] },
    ] }] as any;
  }
  b.epicPool = { id: 'primal_forces_mastery', name: 'Primal Forces Mastery', powers: [
    { internalName: 'Power_Boost', name: 'Power Boost', powerSet: 'primal_forces_mastery', level: 1, isActive: !!powers.powerBoostActive, slots: [] },
  ] } as any;
  return b;
}

describe('Power Boost integration — General totals (rebirth, controller)', () => {
  beforeAll(async () => {
    await loadDataset('rebirth');
  });

  it('multiplies an active defense power’s contribution when Power Boost is ON', () => {
    const off = calculateCharacterTotals(buildWith({ pool: true, powerBoostActive: false }), false, undefined, { combatMode: true });
    const on = calculateCharacterTotals(buildWith({ pool: true, powerBoostActive: true }), false, undefined, { combatMode: true });
    // Hover alone provides positive melee defense...
    expect(off.globalBonuses.defMelee).toBeGreaterThan(0);
    // ...and Power Boost (a strong +Strength buff, ~+120%) roughly doubles it.
    expect(on.globalBonuses.defMelee).toBeGreaterThan(off.globalBonuses.defMelee);
    expect(on.globalBonuses.defMelee / off.globalBonuses.defMelee).toBeGreaterThan(2);
    // strengthDefense is surfaced on globalBonuses for the Power Info display.
    expect(on.globalBonuses.strengthDefense).toBeGreaterThan(1);
    expect(off.globalBonuses.strengthDefense).toBe(0);
  });

  it('adds NOTHING on its own — "twice nothing is still nothing"', () => {
    const off = calculateCharacterTotals(buildWith({ pool: false, powerBoostActive: false }), false, undefined, { combatMode: true });
    const on = calculateCharacterTotals(buildWith({ pool: false, powerBoostActive: true }), false, undefined, { combatMode: true });
    // No defense powers → Power Boost yields no flat defense whether on or off.
    expect(off.globalBonuses.defMelee).toBe(0);
    expect(on.globalBonuses.defMelee).toBe(0);
    // And it never fabricates damage.
    expect(on.globalBonuses.damage).toBe(off.globalBonuses.damage);
  });
});

describe('Mez duration surfacing — prefer PvE template over PvP (homecoming)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  // Homecoming holds carry both a PvE template (e.g. Ranged_Immobilize) and a
  // PvP one (Ranged_PvPMez). The PvP table has no PvE AT-table entry, so if the
  // converter's "higher magnitude wins" rule picked it, the mez duration
  // (scale × table) silently vanished.
  //
  // The guard SITE moved with the bag strip, and the test states the new one. The converter
  // no longer picks a template at all — both rows survive as atoms, and the PvP row is
  // stamped `gated`, so `baseAtoms` drops it before `mezSlotValue` ever compares magnitudes.
  // That matters for what this test is allowed to assert: `expect(table).not.toMatch(/pvp/i)`
  // is vacuous on its own, since no PvP row can reach the reader to fail it. So each case
  // also pins the input that WOULD land — a gated PvP row at a strictly higher magnitude
  // than the PvE row that wins. Without `gated` doing the work, higher-magnitude-wins picks
  // it and the duration vanishes again.
  const cases: Array<[string, string, string]> = [
    ['controller/mind-control', 'Dominate', 'hold'],
    ['controller/mind-control', 'Total_Domination', 'hold'],
    ['controller/mind-control', 'Mesmerize', 'sleep'],
  ];

  for (const [psId, internalName, mezKey] of cases) {
    it(`${internalName} ${mezKey} uses a resolvable PvE table (duration surfaces)`, () => {
      const ps = getPowerset(psId);
      expect(ps, `powerset ${psId}`).toBeTruthy();
      const power = ps!.powers.find(p => p.internalName === internalName);
      expect(power, internalName).toBeTruthy();
      const mez = mezSlotValue(power!, mezKey as Parameters<typeof mezSlotValue>[1]);
      expect(mez, `${internalName}.${mezKey}`).toBeTruthy();
      // Not the PvP table…
      expect(mez!.table).not.toMatch(/pvp/i);
      // …and the AT table resolves, so duration = scale × table is computable & positive.
      const tableVal = getTableValue('controller', mez!.table, 50);
      expect(tableVal, `table ${mez!.table} resolves`).toBeGreaterThan(0);
      expect(mez!.scale * (tableVal as number)).toBeGreaterThan(0);

      // The axis: a PvP row for this same mez kind exists, outranks the surfaced row on
      // magnitude, and is kept out by `gated` alone.
      const pvp = atomsOf(power!).filter(a => /pvp/i.test(a.modifierTable ?? '') && a.effectType === 'Mez');
      expect(pvp.length, `${internalName} has a PvP mez row`).toBeGreaterThan(0);
      expect(pvp.every(a => a.gated), `${internalName} PvP rows are gated`).toBe(true);
      expect(Math.max(...pvp.map(a => a.magnitude))).toBeGreaterThan(mez!.mag);
      expect(baseAtoms(power!).some(a => /pvp/i.test(a.modifierTable ?? ''))).toBe(false);
    });
  }
});

// Guards the damage-buff AT tables (Melee_Buff_Dmg / Ranged_Buff_Dmg) that were
// missing from the extractor allowlist — without them, every damage buff (Build
// Up, Soul Drain, Against All Odds, …) fell back to a flat 0.10, over-valuing
// low-damage ATs and under-valuing high-damage ones. at-tables is a layered
// output outside the regen-diff guard, so this focused test is the backstop
// against a future re-extract silently dropping them.
describe('damage-buff AT tables (Melee/Ranged_Buff_Dmg)', () => {
  beforeAll(async () => {
    await loadDataset('homecoming');
  });

  it('resolve to AT-specific values, not the generic 0.10 fallback', () => {
    // Tanker is a low-damage AT (0.0875 → Build Up scale 8 = +70%);
    // Blaster/Scrapper are high (0.125 → +100%). Pre-fix all read 0.10 (+80%).
    expect(getTableValue('tanker', 'melee_buff_dmg', 50)).toBeCloseTo(0.0875, 4);
    expect(getTableValue('blaster', 'melee_buff_dmg', 50)).toBeCloseTo(0.125, 4);
    expect(getTableValue('scrapper', 'melee_buff_dmg', 50)).toBeCloseTo(0.125, 4);
    // Ranged variant present too (used by Aim and ranged damage buffs).
    expect(getTableValue('blaster', 'ranged_buff_dmg', 50)).toBeGreaterThan(0);
  });

  it('Build Up resolves to its AT-accurate value via the table (not 0.10)', () => {
    // Tanker Build Up (damageBuff scale 8) → 8 × 0.0875 = 0.70 (+70%), not 0.80.
    const ps = getPowerset('tanker/battle-axe');
    const buildUp = ps?.powers.find(p => p.internalName === 'Build_Up');
    expect(buildUp, 'Build_Up').toBeTruthy();
    const dmgBuff = damageBuffValue(buildUp!);
    expect(dmgBuff?.table).toBe('Melee_Buff_Dmg');
    const resolved = dmgBuff!.scale * (getTableValue('tanker', dmgBuff!.table, 50) as number);
    expect(resolved).toBeCloseTo(0.70, 2);
  });
});
