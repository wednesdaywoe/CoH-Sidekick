/**
 * An untouched targets-hit slider must not delete a base the game gives unconditionally.
 *
 * Phalanx Fighting is `EntsAffected kLeaguemate, kCaster` over a 12-foot sphere with
 * `maxTargets 3`: an unconditional `Replace` base of 0.5 beside a `target ≠ source` increment of
 * 0.3. The caster fills the first of those three seats for as long as the power is on, so N never
 * reaches zero — and reading the untouched slider as "no foes" deleted the 5% melee/ranged/AoE
 * defence the game hands out with no ally in sight (PERFOE-3, filed from a beta bug report).
 *
 * Guarded Spin is the second shape, and it is a CLICK rather than an always-on power (PERFOE-4,
 * filed from a Thunderspy report: "does not show in the dashboard when toggled on. I checked every
 * AT it's available for"). Its `EntsAffected` is `kFoe` alone, so the seat above says nothing about
 * it, and its +Def(Melee, Lethal) is one `kStackType_Stack` mod aimed at the caster which the game
 * applies once per foe the cone lands on — the per-foe growth is real. The empty count is not:
 * `character_tick.c` refuses a queued power outright when the target entity is null or of the wrong
 * type, and a cone range-checks that same entity before firing. So a foe-aimed power that fired had
 * a foe in front of it, and the one count it could not be at is the one an untouched slider meant.
 *
 * The engine is where the shipped number comes from and `coh_math::stacking` is graded there
 * (`per_target_floor.rs`, plus the two `Per-target floor` totals fixtures). This is the TS half:
 * the oracle's arithmetic, and the slider control that has to agree with it about where its axis
 * starts.
 */

import { describe, it, expect } from 'vitest';
import {
  adjustForStackCap,
  aimGuaranteesATarget,
  casterOccupiesATargetSlot,
  perTargetCountCannotBeZero,
} from './character-totals';
import { getStackingInfo } from '@/components/info/buildDisplayEffects';
import { encodeAtom, type AtomicEffect } from '@/data/core/atomic-effect';
import type { Power } from '@/types/power';

/**
 * The per-foe increment as an ATOM, which is where the slider reads it (STACKINFO-1).
 *
 * These fixtures used to state their `perTarget` in the `effects` object alone, which the slider
 * read until STRIP-1 emptied that object corpus-wide and the reader went blind. `adjustForStackCap`
 * below still takes a bag VALUE — it is handed one row at a time by the accumulator — so each
 * power now states the increment in both places, exactly as a converted power does.
 */
function perFoeAtoms(increment: number): AtomicEffect[] {
  return [encodeAtom({
    effectType: 'Defense', subType: 'Melee', toWho: 'Self', aspect: 'Cur',
    attribType: 'Magnitude', modifierTable: 'Melee_Buff_Def', scale: 0.5, magnitude: 0.5,
    duration: 0, stacking: 'Replace', baseProbability: 1, pvMode: 'Any', resistible: true,
    perTarget: increment,
  } as AtomicEffect)] as unknown as AtomicEffect[];
}

/** Phalanx Fighting's shape, as the converter emits it. */
const phalanx = {
  name: 'Phalanx Fighting',
  internalName: 'Phalanx_Fighting',
  powerType: 'Auto',
  targetType: 'Self',
  targetsAffected: ['Leaguemate', 'Self'],
  effectArea: 'AoE',
  stats: { maxTargets: 3 },
  atoms: perFoeAtoms(0.3),
  effects: { defenseBuff: { melee: { scale: 0.5, table: 'Melee_Buff_Def', perTarget: 0.3 } } },
};

/** Invincibility's: a foe aura, whose caster is reached only through a target. */
const invincibility = {
  name: 'Invincibility',
  internalName: 'Invincibility',
  powerType: 'Toggle',
  targetType: 'Self',
  targetsAffected: ['Foe'],
  effectArea: 'AoE',
  stats: { maxTargets: 10 },
  atoms: perFoeAtoms(0.1),
  effects: { defenseBuff: { melee: { scale: 0.6, table: 'Melee_Buff_Def', perTarget: 0.1 } } },
};

/** Reactive Regeneration's: `perTarget` from the `Execute_Power` redirect branch, counting how
 *  recently you were hit rather than seats in a sphere. No geometry at all. */
const reactiveRegeneration = {
  name: 'Reactive Regeneration',
  internalName: 'Instant_Regeneration',
  powerType: 'Toggle',
  targetType: 'Self',
  targetsAffected: ['Self'],
  effectArea: 'SingleTarget',
  stats: {},
  atoms: perFoeAtoms(0.25),
  effects: { regenBuff: { scale: 0.25, table: 'Melee_Ones', perTarget: 0.25 } },
};

/** Guarded Spin's shape: a foe-AIMED cone whose defence lands on the caster. The recipient list
 *  names nobody but the foe, so the seat predicate declines it and only the aim floors it. */
const guardedSpin = {
  name: 'Guarded Spin',
  internalName: 'Guarded_Spin',
  powerType: 'Click',
  targetType: 'Foe',
  targetsAffected: ['Foe'],
  effectArea: 'Cone',
  stats: { maxTargets: 5 },
  atoms: perFoeAtoms(1.5),
  effects: { defenseBuff: { melee: { scale: 1.5, table: 'Melee_Buff_Def', perTarget: 1.5 } } },
};

/** The trap on the other side of the aim floor: foe-aimed, but its count is not an entity count.
 *  `perTarget` reaches a SingleTarget power from the `Execute_Power` redirect branch, and flooring
 *  there would assert a combat state rather than a foe in front of you. */
const foeAimedSingleTarget = {
  ...guardedSpin,
  internalName: 'Foe_Aimed_Single_Target',
  effectArea: 'SingleTarget',
  stats: {},
};

const defenseValue = { scale: 0.5, table: 'Melee_Buff_Def', perTarget: 0.3 };

describe('casterOccupiesATargetSlot', () => {
  it('needs both the recipient list and the AoE geometry', () => {
    expect(casterOccupiesATargetSlot(phalanx as never)).toBe(true);
    // The caster is not among the entities counted.
    expect(casterOccupiesATargetSlot(invincibility as never)).toBe(false);
    // The count is not an entity count, so there is no seat to hold.
    expect(casterOccupiesATargetSlot(reactiveRegeneration as never)).toBe(false);
    // An unbounded team-wide spread is not an axis, and neither is a missing list.
    expect(casterOccupiesATargetSlot({ ...phalanx, stats: { maxTargets: 255 } } as never)).toBe(false);
    expect(casterOccupiesATargetSlot({ ...phalanx, targetsAffected: undefined } as never)).toBe(false);
  });
});

describe('adjustForStackCap — the per-target floor', () => {
  it('reads an untouched slider as the solo value when the caster holds a seat', () => {
    for (const n of [undefined, 0, 1]) {
      expect(adjustForStackCap(defenseValue, n, undefined, phalanx as never)).toMatchObject({
        scale: 0.5,
      });
    }
  });

  it('still grows above the floor', () => {
    expect(adjustForStackCap(defenseValue, 2, undefined, phalanx as never)).toMatchObject({ scale: 0.8 });
    const three = adjustForStackCap(defenseValue, 3, undefined, phalanx as never) as { scale: number };
    expect(three.scale).toBeCloseTo(1.1, 12);
  });

  it('leaves a foe aura at zero, which is what nobody-in-radius means for it', () => {
    const foeValue = { scale: 0.6, table: 'Melee_Buff_Def', perTarget: 0.1 };
    expect(adjustForStackCap(foeValue, undefined, undefined, invincibility as never)).toMatchObject({ scale: 0 });
    expect(adjustForStackCap(foeValue, 0, undefined, invincibility as never)).toMatchObject({ scale: 0 });
    expect(adjustForStackCap(foeValue, 1, undefined, invincibility as never)).toMatchObject({ scale: 0.6 });
  });

  it('does not floor a stack depth — N there counts casts, not entities', () => {
    const stacking = { scale: 5, table: 'Melee_Ones' };
    expect(adjustForStackCap(stacking, 0, 2, phalanx as never)).toMatchObject({ scale: 0 });
  });
});

describe('aimGuaranteesATarget', () => {
  it('needs the aim to name an entity AND the count to be an entity count', () => {
    expect(aimGuaranteesATarget(guardedSpin as never)).toBe(true);
    // Aimed at a foe, but nothing in a sphere is being counted.
    expect(aimGuaranteesATarget(foeAimedSingleTarget as never)).toBe(false);
    // Aimed at the caster, who is always there — this floor has nothing to say, and Invincibility
    // must stay at zero because a foe aura reaches its caster only through a foe.
    expect(aimGuaranteesATarget(invincibility as never)).toBe(false);
    expect(aimGuaranteesATarget(phalanx as never)).toBe(false);
    // A point on the ground is not an entity the game insists on.
    expect(aimGuaranteesATarget({ ...guardedSpin, targetType: 'Location' } as never)).toBe(false);
    expect(aimGuaranteesATarget({ ...guardedSpin, targetType: undefined } as never)).toBe(false);
  });

  it('is a separate reason from the seat, and the union is what the calc asks', () => {
    // Neither power satisfies both terms, so each proves one arm of the union on its own.
    expect(casterOccupiesATargetSlot(guardedSpin as never)).toBe(false);
    expect(perTargetCountCannotBeZero(guardedSpin as never)).toBe(true);
    expect(aimGuaranteesATarget(phalanx as never)).toBe(false);
    expect(perTargetCountCannotBeZero(phalanx as never)).toBe(true);
    expect(perTargetCountCannotBeZero(invincibility as never)).toBe(false);
    expect(perTargetCountCannotBeZero(reactiveRegeneration as never)).toBe(false);
  });
});

describe('adjustForStackCap — the aim floor', () => {
  const spinValue = { scale: 1.5, table: 'Melee_Buff_Def', perTarget: 1.5 };

  it('reads an untouched slider as one foe on a power the game will not fire at nobody', () => {
    for (const n of [undefined, 0, 1]) {
      expect(adjustForStackCap(spinValue, n, undefined, guardedSpin as never)).toMatchObject({
        scale: 1.5,
      });
    }
  });

  it('still grows with the foes hit', () => {
    expect(adjustForStackCap(spinValue, 2, undefined, guardedSpin as never)).toMatchObject({ scale: 3 });
    expect(adjustForStackCap(spinValue, 5, undefined, guardedSpin as never)).toMatchObject({ scale: 7.5 });
  });

  it('leaves the foe-aimed single target at zero — its N is not counting foes', () => {
    expect(adjustForStackCap(spinValue, undefined, undefined, foeAimedSingleTarget as never))
      .toMatchObject({ scale: 0 });
  });
});

describe('getStackingInfo — where the slider starts', () => {
  it('starts the targets axis at one foe on a power the game aims at one', () => {
    expect(getStackingInfo(guardedSpin as unknown as Power)).toEqual({
      maxStacks: 5,
      minStacks: 1,
      label: 'Targets Hit',
    });
  });

  it('starts the targets axis at one seat when the caster holds one', () => {
    expect(getStackingInfo(phalanx as unknown as Power)).toEqual({
      maxStacks: 3,
      minStacks: 1,
      label: 'Targets Hit',
    });
  });

  it('starts a foe aura at zero', () => {
    expect(getStackingInfo(invincibility as unknown as Power)).toEqual({
      maxStacks: 10,
      minStacks: 0,
      label: 'Targets Hit',
    });
  });

  it('starts a stack count at zero', () => {
    // The depth is the ATOM's `stackCap` now, not a `maxStacks` slot: the converter minted that
    // slot for only some of the powers whose atoms state a depth, and STRIP-1 took the rest.
    const buildUp = {
      stats: {},
      atoms: [encodeAtom({
        effectType: 'DamageBuff', toWho: 'Self', aspect: 'Str', attribType: 'Magnitude',
        modifierTable: 'Melee_Ones', scale: 1, magnitude: 1, duration: 10,
        stacking: 'Stack', stackCap: 2, baseProbability: 1, pvMode: 'Any', resistible: true,
      } as AtomicEffect)],
    };
    expect(getStackingInfo(buildUp as unknown as Power)).toMatchObject({ minStacks: 0, label: 'Stacks' });
  });
});
