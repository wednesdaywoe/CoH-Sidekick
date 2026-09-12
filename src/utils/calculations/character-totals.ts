/**
 * Character Totals — the dashboard calculation the app runs.
 *
 * `calculateCharacterTotals` delegates to the Rust engine (`engineCalculate`) and reshapes
 * its output to this file's result type. What remains here beyond that reshape are the
 * helpers live components still call directly (Alpha bonuses, Strength buffs, breakdown
 * readers), plus the shared types.
 *
 * The pre-engine TypeScript implementation lives in `legacy-totals.oracle.ts` — test-only.
 */

import type { Build, ConditionalEffect, Enhancement, EnhancementStatType, IncarnateActiveState, IncarnateBuildState } from '@/types';
import type { ProcSettings } from '@/stores/uiStore';
import type { PowerProjection } from '@/engine/engineTotalsMap';
import { withoutIllegalSlots } from '@/utils/build-enhancement-validation';
import { getAlphaEffects, getAlphaEdBypass } from '@/data';
import type { AlphaEffects } from '@/data';
import { getTableValue } from '@/data/at-tables';
import { type AggregatedBonuses, type BonusTracking } from './set-bonuses';
import { createEmptyStats, type CharacterStats } from './stats';
import { type EnhancementBonuses } from './enhancement-values';
import type { EncodedAtom } from '@/data/core/atomic-effect';
import { SPECIAL_BUFF_STACK, specialBuffValue, stackCapOf, atomsOf, isDebuffAtom, type AtomSource } from '@/data/core/atom-query';
import { ATOM_TUPLE_FIELDS, decodeAtoms, encodeAtom } from '@/data/core/atomic-effect';
import { warnFallback } from '@/utils/fallback-warnings';
import { engineCalculate } from '@/engine/engineTotals';
import { useEngineStore } from '@/engine/engineStore';
import type { AdapterCalcContext } from '@/engine/characterStateAdapter';

// ============================================
// TYPES
// ============================================

/**
 * Global bonuses from set bonuses and active powers
 * These modify all powers (Final values)
 */
export interface GlobalBonuses {
  damage: number;
  accuracy: number;
  toHit: number;
  recharge: number;
  endurance: number;
  range: number;
  // Defense
  defMelee: number;
  defRanged: number;
  defAoE: number;
  defSmashing: number;
  defLethal: number;
  defFire: number;
  defCold: number;
  defEnergy: number;
  defNegative: number;
  defPsionic: number;
  defToxic: number;
  // Resistance
  resSmashing: number;
  resLethal: number;
  resFire: number;
  resCold: number;
  resEnergy: number;
  resNegative: number;
  resPsionic: number;
  resToxic: number;
  // Recovery & Health
  maxHP: number;
  maxEndurance: number;
  absorb: number;
  regeneration: number;
  recovery: number;
  // Movement
  runSpeed: number;
  jumpHeight: number;
  jumpSpeed: number;
  flySpeed: number;
  // Mez Resistance (generic, from IO set bonuses — applies to all types)
  mezResist: number;
  // Per-type mez resistance (from active power effects)
  mezResistHold: number;
  mezResistStun: number;
  mezResistImmobilize: number;
  mezResistSleep: number;
  mezResistConfuse: number;
  mezResistFear: number;
  mezResistKnockback: number;
  // Mez Protection (magnitude points)
  protHold: number;
  protStun: number;
  protImmobilize: number;
  protSleep: number;
  protConfuse: number;
  protFear: number;
  protKnockback: number;
  // Debuff Resistance
  debuffResistSlow: number;
  debuffResistDefense: number;
  debuffResistRecharge: number;
  debuffResistEndurance: number;
  debuffResistRecovery: number;
  debuffResistToHit: number;
  debuffResistRegeneration: number;
  debuffResistPerception: number;
  // Accuracy/range debuff resistance (DEBUFFRES-1). ENGINE-ONLY: the legacy calc's
  // `debuffResMapping` names neither, so it leaves both at 0 and `serverParity` lists them
  // UNMAPPED. Declared here because the field set is the shared shape both sides fill.
  debuffResistAccuracy: number;
  debuffResistRange: number;
  // Special
  healOther: number;
  // Healing Received — Res(Heal) buff (percent). Positive = more healing
  // received (e.g. Incandescence Destiny +50%). Distinct from healOther, which
  // is heal *strength* (boosts heals you cast). Populated by incarnate Destiny.
  healReceived: number;
  threatLevel: number;
  // Stealth
  stealthRadiusPvE: number;
  stealthRadiusPvP: number;
  perceptionRadius: number;
  // Additional mez protection
  protRepel: number;
  // Additional mez resistance
  mezResistTaunt: number;
  mezResistPlacate: number;
  // Incarnate
  levelShift: number;
  // Toggle endurance cost (end/sec from active toggles)
  toggleEndCost: number;
  // Endurance discount from powers like Conserve Power (reduces end costs by this %)
  enduranceDiscount: number;
  // Net endurance per second (recovery minus toggle costs)
  netEndPerSec: number;
  // Purple patch - hit chance against target level
  baseToHit: number;
  hitChance: number;
  combatModifier: number;
  // +Strength buffs (Power Boost family). Stored as fractions (e.g. 1.229 =
  // +122.9% strength), NOT percentages like the fields above. These are
  // non-ED multipliers applied to the caster's OWN power outputs for the
  // matching aspect (def/tohit/heal/absorb/endmod/movement/mez), additive
  // with enhancement strength. They never touch damage, resistance, or set
  // bonuses, and add nothing on their own ("twice nothing is nothing").
  strengthDefense: number;
  strengthToHit: number;
  strengthHeal: number;
  strengthAbsorb: number;
  strengthEndMod: number;
  strengthMovement: number;
  strengthMez: number;

  // Offensive mez/control duration buffs from IO set bonuses (percent). Applied
  // to the duration of the mez a power inflicts (Immobilize/Hold/Stun/etc.) —
  // mapped to the matching power effect key via GLOBAL_BONUS_ASPECT_MAP.
  immobilizeDuration: number;
  holdDuration: number;
  stunDuration: number;
  sleepDuration: number;
  confuseDuration: number;
  terrorDuration: number;
}

/**
 * Source of a stat contribution for breakdown
 */
export interface StatSource {
  name: string;
  value: number;
  type: 'set-bonus' | 'active-power' | 'inherent' | 'enhancement' | 'accolade' | 'incarnate' | 'proc';
  setId?: string;
  pieces?: number;
  capped?: boolean; // True if this instance hit the Rule of 5 cap
  /** True if this instance is excluded because a STRONGER same-group buff wins
   *  (mutual suppression: travel-speed `kTravelBuff` powers, stealth-radius
   *  `StealthToggle` powers — only the largest applies) or the In-Combat toggle
   *  suppresses it. Distinct from `capped`: suppression is normal game mechanics,
   *  NOT a Rule of 5 violation, so it dims the breakdown row (like capped) but
   *  must NOT feed the Rule-of-5 warning ring / banner / auto-track. */
  suppressed?: boolean;
  /** Display name of the power that supplied this source (set bonus / proc), used to highlight powers contributing capped bonuses. */
  powerName?: string;
}

/**
 * Complete stat breakdown for a dashboard stat
 */
export interface DashboardStatBreakdown {
  total: number;
  base: number;
  sources: StatSource[];
  cappedSources: number; // Count of sources that are capped
}

/**
 * Full calculation result with all data needed
 */
export interface CharacterCalculationResult {
  stats: CharacterStats;
  globalBonuses: GlobalBonuses;
  breakdown: Map<string, DashboardStatBreakdown>;
  setBonuses: AggregatedBonuses;
  /** Raw Rule of 5 tracking — use getBonusCount/isBonusCapped to query */
  bonusTracking: BonusTracking;
  /** Per-power non-DPS execution + perma + granted magnitudes for every SELECTED power,
   *  keyed `${powerSet}\0${internalName}` (PROD6B-1/2). Read it through
   *  `usePowerProjection`, which also covers powers the build does not hold. */
  powerProjection: Map<string, PowerProjection>;
  /** The exact `CharacterState` JSON the engine ran for this result, or null when the
   *  dataset wasn't loaded. `usePowerProjection` replays it to project an UNHELD power
   *  against the identical state these totals came from — reusing the string is what
   *  guarantees the hovered power and the picked one are computed the same way, rather
   *  than a second assembly of the context drifting from this one. */
  engineStateJson: string | null;
  /** The `GlobalBonuses` keys the what-if TEAM-BUFF layer moved, and by how much — the
   *  engine's own record of what it injected. A surface marks a number SIMULATED by asking
   *  whether its breakdown key is in here, never by re-reading the sliders: that second
   *  answer could describe a different calculation than the one on screen. Empty on every
   *  build with nothing simulated. */
  whatIfMoved: Record<string, number>;
  /** The scale the damage bars read against — the hardest hit the build's chosen powersets can
   *  produce, each power asked what it would deal with its OWN slots filled for damage
   *  (`coh_math::projection::damage_ceiling`).
   *
   *  A property of the BUILD rather than of any power, which is why it rides here instead of
   *  being derived per bar. It replaced `useBuildMaxAttackDamage`, which folded over the powers
   *  the build had PICKED — a maximum that is always attained, so every build had exactly one
   *  power pegged at 100% and learned nothing from seeing it there. Measured against the sets
   *  instead, a full bar is something a build can fail to reach.
   *
   *  `null` when nothing in reach resolves to damage; callers fall back to a per-power
   *  reference, as they did when the hook returned 0. */
  damageCeiling: number | null;
}

// ============================================
// GLOBAL BONUS INITIALIZATION
// ============================================

export function createEmptyGlobalBonuses(): GlobalBonuses {
  return {
    damage: 0,
    accuracy: 0,
    toHit: 0,
    recharge: 0,
    endurance: 0,
    range: 0,
    defMelee: 0,
    defRanged: 0,
    defAoE: 0,
    defSmashing: 0,
    defLethal: 0,
    defFire: 0,
    defCold: 0,
    defEnergy: 0,
    defNegative: 0,
    defPsionic: 0,
    defToxic: 0,
    resSmashing: 0,
    resLethal: 0,
    resFire: 0,
    resCold: 0,
    resEnergy: 0,
    resNegative: 0,
    resPsionic: 0,
    resToxic: 0,
    maxHP: 0,
    maxEndurance: 0,
    absorb: 0,
    regeneration: 0,
    recovery: 0,
    runSpeed: 0,
    jumpHeight: 0,
    jumpSpeed: 0,
    flySpeed: 0,
    mezResist: 0,
    mezResistHold: 0,
    mezResistStun: 0,
    mezResistImmobilize: 0,
    mezResistSleep: 0,
    mezResistConfuse: 0,
    mezResistFear: 0,
    mezResistKnockback: 0,
    protHold: 0,
    protStun: 0,
    protImmobilize: 0,
    protSleep: 0,
    protConfuse: 0,
    protFear: 0,
    protKnockback: 0,
    debuffResistSlow: 0,
    debuffResistDefense: 0,
    debuffResistRecharge: 0,
    debuffResistEndurance: 0,
    debuffResistRecovery: 0,
    debuffResistToHit: 0,
    debuffResistRegeneration: 0,
    debuffResistPerception: 0,
    debuffResistAccuracy: 0,
    debuffResistRange: 0,
    healOther: 0,
    healReceived: 0,
    threatLevel: 0,
    stealthRadiusPvE: 0,
    stealthRadiusPvP: 0,
    perceptionRadius: 0,
    protRepel: 0,
    mezResistTaunt: 0,
    mezResistPlacate: 0,
    levelShift: 0,
    toggleEndCost: 0,
    enduranceDiscount: 0,
    netEndPerSec: 0,
    baseToHit: 0.75,
    hitChance: 0.75,
    combatModifier: 1.0,
    strengthDefense: 0,
    strengthToHit: 0,
    strengthHeal: 0,
    strengthAbsorb: 0,
    strengthEndMod: 0,
    strengthMovement: 0,
    strengthMez: 0,
    immobilizeDuration: 0,
    holdDuration: 0,
    stunDuration: 0,
    sleepDuration: 0,
    confuseDuration: 0,
    terrorDuration: 0,
  };
}

// ============================================
// SHARED EFFECT TYPES AND SCALING
// ============================================

export type ScalarOrScaled = number | { scale: number; table?: string };
export type MezScaled = { mag?: number; scale: number; table: string };

/** `maxTargets` sentinel for an unbounded AoE — a team-wide spread with no axis to count. */
const UNBOUNDED_MAX_TARGETS = 255;

/**
 * Is the caster himself one of the entities his own AoE counts — so that N is never 0?
 *
 * Both terms are the ones `computeAoePerTargetPatches` reads to mint the `perTarget` stamp in the
 * first place, and reading them back is what keeps the floor and the value it floors on one story:
 *
 *  - The geometry — a sphere or cone with a bounded `maxTargets` — is that pass's own
 *    `isAoEWithTargets` gate. It matters because `perTarget` also reaches atoms from the
 *    `Execute_Power` redirect branch, where the increment counts something else entirely
 *    (Reactive Regeneration's stacks count how recently you were hit, on a `SingleTarget` toggle
 *    with no sphere at all), and a floor there would assert a combat state rather than a seat in
 *    a sphere.
 *  - The recipient list — the converter's `selfIsCountedTarget`, `Power::affects_caster` in the
 *    engine — is what makes the caster one of those seats. It is the same term
 *    `firstTargetExcluded` uses to decide the value AT one target, so the two halves of "N = 1 is
 *    solo" answer from one field instead of one of them being left unsaid.
 *
 * `maxTargets` is read under `stats`, which is where every per-target power in all four forks
 * carries it (109/102/114/109, none at the power level). The pseudo-pet ability records that
 * spell it a second way are not powers and never reach this path.
 *
 * Absent reads false on both terms: no list is not a licence to credit the caster with a slot.
 *
 * The parameter is the three fields read, not a `PowerWithToggle`: the display's own slider asks
 * the same question of a plain `Power` (`getStackingInfo`), whose `effects` is the WIRE shape and
 * not the resolved one. Naming the reads is what lets one predicate answer both callers instead
 * of the pair drifting the way the hand-spelled `targetsAffected` half already had.
 */
export function casterOccupiesATargetSlot(
  power: {
    targetsAffected?: readonly string[];
    effectArea?: string;
    stats?: { maxTargets?: number } | Record<string, unknown>;
  },
): boolean {
  if (!power.targetsAffected?.includes('Self')) return false;
  if (power.effectArea !== 'AoE' && power.effectArea !== 'Cone') return false;
  const maxTargets = power.stats?.maxTargets;
  return typeof maxTargets === 'number' && maxTargets > 1 && maxTargets !== UNBOUNDED_MAX_TARGETS;
}

/**
 * Adjust a scaled effect for per-target stacking. The `scale` field on a
 * perTarget-flagged effect stores the value at N=1 (base + 1 × per-target
 * contribution); each additional target adds `perTarget` to it.
 *
 * Semantics with respect to the targets-hit slider:
 *   - N undefined → no slider input; reads the same as an explicit 0, which is the axis' floor
 *                   (0 for a foe aura, 1 where the caster holds a seat — see below).
 *   - N = 0       → the sphere landed on nobody; the buff doesn't fire. Return scale 0.
 *   - N = 1       → original value.
 *   - N ≥ 2       → scale + perTarget × (N − 1).
 *
 * Effects without `perTarget` metadata (always-on buffs) are unaffected
 * by the slider regardless of N.
 *
 * `casterIsCounted` ({@link casterOccupiesATargetSlot}) decides whether N = 0 is a state the
 * power can be in at all. A foe aura reaches its caster only through a target, so with nobody in
 * radius no block runs and zero is honest. A power that lists the caster among the entities its
 * own sphere lands on is the other shape: he fills the first of its `maxTargets` slots for as
 * long as it is running, so the count floors at one and the slider only ever adds to it
 * (PERFOE-3, Phalanx Fighting — 5% melee/ranged/AoE defence with no ally in sight).
 */
function adjustForPerTarget(
  value: ScalarOrScaled,
  targetsHit: number | undefined,
  casterIsCounted: boolean,
): ScalarOrScaled {
  if (typeof value !== 'object' || value === null) return value;
  const obj = value as Record<string, unknown>;
  // A MaxHP-fraction absorb carries its per-foe growth as `maxHPFractionPerTarget`
  // beside `maxHPFraction`: the magnitude rides an Expression, so there is no
  // `scale` for the increment to grow. Same arithmetic, other field pair.
  const [magnitudeKey, incrementKey] = typeof obj.maxHPFractionPerTarget === 'number'
    ? ['maxHPFraction', 'maxHPFractionPerTarget']
    : ['scale', 'perTarget'];
  const increment = obj[incrementKey];
  const magnitude = obj[magnitudeKey];
  if (typeof increment !== 'number' || !increment || typeof magnitude !== 'number') return value;
  // An untouched slider must read as whatever the CONTROL shows, or the power computes one number
  // while the UI states another (the "defaults to 1-target values despite showing Off" bug). Both
  // sides now start the axis at the same place: `getStackingInfo` gives the slider its
  // `minStacks`, and `casterOccupiesATargetSlot` gives the calc the same floor.
  const n = Math.max(targetsHit ?? 0, casterIsCounted ? 1 : 0);
  if (n <= 0) return { ...value, [magnitudeKey]: 0 } as ScalarOrScaled;
  if (n === 1) return value;
  return { ...value, [magnitudeKey]: magnitude + increment * (n - 1) } as ScalarOrScaled;
}

/**
 * The bag-shaped `adjustForStacking` stood here until BPORT11 and is gone with its last caller.
 * It took `stacksLinear`, an effect KEY, `maxStacks` and `stackCaps` — four arguments spelling
 * one question the atoms answer in one — and the totals oracle was the only thing that called
 * it. {@link adjustForStackCap} is the same arithmetic keyed on the resolved cap.
 */

export interface ActivePowerEffect {
  tohitBuff?: number;
  tohitBuffUnenhanced?: ScalarOrScaled;
  accuracyBuff?: number;
  damageBuff?: number;
  rechargeBuff?: ScalarOrScaled;
  defense?: Record<string, ScalarOrScaled>;
  defenseBuff?: Record<string, ScalarOrScaled>;
  defenseBuffSuppressible?: Record<string, ScalarOrScaled>;
  resistance?: Record<string, ScalarOrScaled>;
  // -Resistance debuff. Enemy-facing by default; entries tagged toWho:'Self'
  // (Bio Armor Offensive Adaptation) subtract from the CASTER's own resistance.
  resistanceDebuff?: Record<string, ScalarOrScaled>;
  debuffResistance?: Record<string, ScalarOrScaled>;
  mezResistance?: Record<string, ScalarOrScaled>;
  elusivity?: Record<string, ScalarOrScaled>;
  runSpeed?: number;
  // IgnoreStrength run-speed template (e.g. Sprint's second RunningSpeed effect):
  // contributes to totals but is NOT boosted by slotted Run Speed enhancements.
  // Mirrors the tohitBuffUnenhanced / regenBuffUnenhanced convention.
  runSpeedUnenhanced?: ScalarOrScaled;
  flySpeed?: number;
  jumpHeight?: number;
  jumpSpeed?: number;
  regeneration?: ScalarOrScaled;
  recovery?: ScalarOrScaled;
  maxEndurance?: ScalarOrScaled;
  maxHealth?: ScalarOrScaled;
  regenBuff?: ScalarOrScaled;
  regenBuffUnenhanced?: ScalarOrScaled;
  recoveryBuff?: ScalarOrScaled;
  recoveryBuffUnenhanced?: ScalarOrScaled;
  maxHPBuff?: ScalarOrScaled;
  // Unenhanceable half of a +MaxHP twin (IgnoreStrength — "half of this max-HP
  // increase is unenhanceable"). Summed onto maxHP like maxHPBuff but WITHOUT
  // the +Healing enhancement multiplier. See the converter's hitPoints/maximum
  // split and the maxHPBuff handler below.
  maxHPBuffUnenhanced?: ScalarOrScaled;
  maxEndBuff?: ScalarOrScaled;
  // Absorb shield. Two magnitude forms (see the aggregation below):
  //  • flat HP     — {scale, table} on a Heal table (Psychokinetic Barrier).
  //  • % of MaxHP  — `maxHPFraction` (recovered from the Expression magnitude,
  //    e.g. Wild Bastion 0.25) or a `_ones`-table {scale} (bare fraction).
  // `appliesStrength` (default true) marks the MaxHP-fraction form as scaled by
  // +Absorb strength (Power Boost / Clarion) and slotted Heal.
  absorb?: { scale?: number; table?: string; maxHPFraction?: number; appliesStrength?: boolean };
  // Effect targeting (SingleTarget, AoE, etc.)
  effectArea?: string;
  // Mez protection (pool/epic style — direct magnitudes)
  protection?: Record<string, number>;
  // Mez effects that may be protection when using Res_Boolean tables
  hold?: number | MezScaled;
  stun?: number | MezScaled;
  immobilize?: number | MezScaled;
  sleep?: number | MezScaled;
  confuse?: number | MezScaled;
  fear?: number | MezScaled;
  knockback?: number | MezScaled;
  knockup?: number | MezScaled;
  // Additional status effects
  repel?: ScalarOrScaled;
  teleport?: ScalarOrScaled;
  taunt?: number | MezScaled;
  placate?: number | MezScaled;
  // Stealth
  stealth?: {
    stealthPvE?: ScalarOrScaled;
    stealthPvP?: ScalarOrScaled;
    /** Binary stealth-stacking group (`stack_key`); non-null = suppress group
     *  (max-wins), null/absent = additive. See resolveStealthRadius. */
    stackKey?: string | null;
  };
  // Perception
  perceptionBuff?: ScalarOrScaled;
  // Range buff (Boost Range, Aim's +Range, …). A caster +Range self-buff; the
  // same field on Foe-targeted attacks is the per-power Fast Snipe range bump,
  // which is NOT aggregated here (see applyActivePowerBonuses).
  rangeBuff?: ScalarOrScaled;
  // Endurance cost per second (for toggles)
  enduranceCost?: number;
  // Endurance discount (e.g., Conserve Power — reduces end costs by a percentage)
  enduranceDiscount?: ScalarOrScaled;
  // Self-debuffs (e.g., Granite Armor) — applied per-value when the debuff is
  // self-directed (toWho:'Self'). Most powers with these fields target enemies.
  tohitDebuff?: ScalarOrScaled;
  slow?: ScalarOrScaled | Record<string, ScalarOrScaled>;
  movement?: Record<string, ScalarOrScaled>;
  rechargeDebuff?: ScalarOrScaled;
  damageDebuff?: ScalarOrScaled;
  /** Linear self-stacking metadata — see PowerEffects.stacksLinear */
  stacksLinear?: readonly string[];
  maxStacks?: number;
  /** Per-effect stack caps — see PowerEffects.stackCaps */
  stackCaps?: Record<string, number>;
  /** +Strength self-buff container (Power Boost family) — keyed by aspect
   *  (defense/melee/.../tohit/heal/absorb/endurance/movement/mez). Each entry
   *  is a Strength-aspect multiplier on the caster's own matching output. */
  specialBuff?: Record<string, ScalarOrScaled>;
}

export interface PowerWithToggle {
  name: string;
  internalName: string;
  powerType?: string;
  targetType?: string;
  /** `EntsAffected` — who this power's effects land on, which `targetType` (where it is AIMED)
   *  does not answer. Read by the atom-native appliers through `reachesCaster`, and by
   *  {@link casterOccupiesATargetSlot} for the AoE target count. */
  targetsAffected?: string[];
  effectArea?: string;
  isActive?: boolean;
  effects?: ActivePowerEffect;
  /** Pre-projection atom list (Plan B) — the atom-native appliers read this in
   *  place of a bag slot. Carried from the powerset definition by `enrich`. */
  atoms?: EncodedAtom[];
  slots?: (Enhancement | null)[];
  stats?: { endurance?: number; activatePeriod?: number; [key: string]: unknown };
  /** Used by `combineWithAlphaED` to gate Alpha bonuses by what the power
   * actually accepts as enhancements. See enhancement-values.ts. */
  allowedEnhancements?: EnhancementStatType[];
  /** Mode-/state-gated contributions (Bio Armor adaptation modes, Hide, …)
   *  carried from the powerset definition. The active ones are expanded into
   *  synthetic active-power contributions by `expandActiveConditionals`. */
  conditionalEffects?: ConditionalEffect[];
  /** Selected stance sub-power for parents with mutually exclusive stances
   *  (Bio Armor Adaptation, Staff Mastery) — the single source of truth for
   *  which stance is active. Carried through from the stored power. */
  activeSubPower?: string;
  /** A synthetic power the totals pass built for itself (`expandActiveConditionals`,
   *  `expandBuffPetAuras`): its `effects` object is that function's own output — a
   *  stance/mode conditional, or a folded pet aura — rather than converted data. Neither
   *  synthetic carries `atoms`, so the atom-native arms answer `undefined` for them and
   *  the bag branch is the only path they have. {@link syntheticEffects} is that branch,
   *  named, so BPORT11 can retire the DATA seams beside it without also deleting the
   *  handoff. */
  syntheticContribution?: boolean;
}

/**
 * The `effects` a SYNTHETIC contribution carries, or `undefined` for any real power.
 *
 * A writer→reader handoff inside one totals pass, not a data-bag read: the object came from
 * `expandActiveConditionals` or `buffPetAuraEffects` a few lines earlier and the power it
 * rides has no atoms to read instead. A converted power never carries the marker, so this is
 * `undefined` across the whole generated corpus and an arm that keeps it behaves exactly as
 * one that dropped the bag outright.
 */
export function syntheticEffects(power: PowerWithToggle): ActivePowerEffect | undefined {
  return power.syntheticContribution ? power.effects : undefined;
}

/**
 * Accumulated +Strength self-buffs (Power Boost family), as FRACTIONS
 * (e.g. 0.5 = +50% strength). Applied as a non-ED additive term in the
 * per-power enhancement multiplier for the matching aspect.
 */
export interface StrengthBuffs {
  defense: number;
  toHit: number;
  heal: number;
  absorb: number;
  endMod: number;
  movement: number;
  mez: number;
}

export function emptyStrengthBuffs(): StrengthBuffs {
  return { defense: 0, toHit: 0, heal: 0, absorb: 0, endMod: 0, movement: 0, mez: 0 };
}

/** specialBuff keys representing defense strength (all positions + all types + Base_Defense). */
const STRENGTH_DEFENSE_KEYS = new Set([
  'defense', 'melee', 'ranged', 'aoe',
  'smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'psionic', 'toxic',
]);
/** specialBuff keys representing mez strength (boosts both magnitude and duration). */
const STRENGTH_MEZ_KEYS = new Set([
  'hold', 'stun', 'sleep', 'confuse', 'fear', 'immobilize',
]);

/**
 * The stacking adjustment, taking the cap as a resolved number rather than reading it out of
 * three bag slots.
 *
 * `stackCapOf` answers membership and depth together — a number is both "this family
 * self-stacks" and how far — which is what the atoms say directly and what `stacksLinear` +
 * (`stackCaps[key]` ?? `maxStacks`) said in three places. BPORT11 moved the oracle's call sites
 * onto this one family by family, and the bag-shaped helper went with the last of them.
 *
 * The one case with no atom-native spelling is the bag's `stacksLinear`-without-a-cap
 * (stack uncapped), because on the atom side a cap is what admits a row to the family at all.
 * Measured over 14,249 powers before the first carry: wherever both sides say a family
 * stacks they name the SAME depth — 0 disagreements across every site — so the uncapped case
 * is a shape the corpus does not hold, not a value being rounded away.
 *
 * `power` is here for the per-target arm alone: whether an absent count means zero targets or one
 * is a question about the power, not about the value ({@link casterOccupiesATargetSlot}).
 */
export function adjustForStackCap(
  value: ScalarOrScaled,
  targetsHit: number | undefined,
  stackCap: number | undefined,
  power: PowerWithToggle,
): ScalarOrScaled {
  const hasPerTarget = typeof value === 'object' && value !== null
    && (!!(value as { perTarget?: number }).perTarget
      || !!(value as { maxHPFractionPerTarget?: number }).maxHPFractionPerTarget);
  if (hasPerTarget) return adjustForPerTarget(value, targetsHit, casterOccupiesATargetSlot(power));
  // The floor reaches the AoE path alone. N on a stacking self-buff counts casts, and a click
  // the build has not fired is at zero stacks however the power addresses its caster.
  if (targetsHit === 0 && stackCap !== undefined) {
    if (typeof value === 'object' && value !== null) return { ...value, scale: 0 };
    return 0;
  }
  if (!targetsHit || targetsHit <= 1) return value;
  if (stackCap === undefined) return value;
  if (typeof value !== 'object' || value === null) return value;
  const obj = value as { scale: number };
  return { ...value, scale: obj.scale * Math.min(targetsHit, stackCap) };
}

/**
 * Does this power carry a live combat debuff — a `-ToHit` on a combat face, or a `-Damage`
 * strength debuff?
 *
 * The movement-buff arm skips a power that does, because a `-Speed` riding a debuff aura is
 * the enemy's penalty and not the caster's buff. The bag stated the same test as
 * `effects.tohitDebuff === undefined && effects.damageDebuff === undefined`, which is a
 * question about two slots rather than about the power; asking the atoms keeps the answer once
 * the slots are gone, and keeps it on the same discriminators — `aspect` separates a ToHit
 * debuff from ToHit-debuff RESISTANCE, and `Str` is what makes a DamageBuff row a debuff to
 * the caster's own output. Gated atoms are excluded: a mode's debuff is that mode's.
 */
export function carries_combat_debuff(power: AtomSource): boolean {
  return atomsOf(power).some((a) => {
    if (a.gated || !isDebuffAtom(a)) return false;
    if (a.effectType === 'ToHit') return a.aspect !== 'Res' && a.aspect !== 'Str';
    if (a.effectType === 'DamageBuff') return a.aspect === 'Str';
    return false;
  });
}

/**
 * The power as ONE player class sees it, for the archetype-fork-aware protection reads.
 *
 * A forked atom (Rebirth's Combat Jumping immobilize splits on `casterArchetypes`) is base for
 * one class and absent from every other's, and a build-agnostic reader drops it entirely
 * (AT-FORK-1). This returns the unforked atoms plus the build's own arms with the stamp
 * cleared — the same shape the shadow gates' `forkResolvedViews` hand the readers (see
 * `scripts/planb-shadow-sweep.cjs`) — so a forked protection atom is credited for the classes
 * it genuinely applies to rather than silently dropped when the bag stops answering.
 *
 * No token, or no forked atom on the power: the raw power, which is the conventional
 * single-class view unchanged. Measured over 213,735 power×class views at BPORT11: the fork
 * resolution recovers rebirth Weave's immobilize protection for the two Kheldian classes and
 * changes nothing else.
 */
export function mezSourceFor(power: AtomSource, playerClassToken?: string): AtomSource {
  if (
    playerClassToken
    && (power.atoms || []).some((t) => t[ATOM_TUPLE_FIELDS.indexOf('casterArchetypes')])
  ) {
    return {
      targetsAffected: power.targetsAffected,
      atoms: decodeAtoms(power.atoms)
        .filter((a) => !a.casterArchetypes || a.casterArchetypes.split(',').includes(playerClassToken))
        .map((a) => encodeAtom({ ...a, casterArchetypes: undefined })),
    };
  }
  return power;
}

/**
 * Collect active +Strength self-buffs (Power Boost, Power Build Up, Power Up,
 * Bass Boost, Gather Shadows, Adrenal Booster, …) from the build's active
 * powers. Each entry is a Strength-aspect buff that multiplies the caster's OWN
 * matching power output — non-ED, additive with enhancement strength, and it
 * adds nothing on its own ("twice nothing is nothing").
 *
 * Atom-native via {@link specialBuffValue}, the TS twin of
 * `coh_math::strength::special_buff_map` — the Rust side's reader since M3, and a half this
 * file never had. BPORT7 empties `effects.specialBuff`, at which point the bag read below it
 * would have gone to zero on real data while the engine kept reading the atom, with the
 * fixture emitter selecting on the same dead slot and reporting a roster gap rather than a
 * divergence.
 *
 * Within one power the defense (and mez) sub-keys are uniform — the binary
 * simply enumerates every defense/mez attribute — so we take the
 * representative (max) per power rather than summing the ~12 defense keys.
 * Across multiple active strength powers (and self-stacks) the fractions add.
 * Returns fractions per aspect.
 */
export function collectStrengthBuffs(
  powers: PowerWithToggle[],
  archetypeId: string,
  buildLevel: number,
  targetsHitValues: Record<string, number> = {},
): StrengthBuffs {
  const sb = emptyStrengthBuffs();
  for (const power of powers) {
    const isAuto = power.powerType?.toLowerCase() === 'auto';
    if (!(isAuto || power.isActive)) continue;
    // No power-level target gate: `specialBuffValue` tests each atom's own recipient, which
    // is strictly narrower than the bag path's whole-power `targetType !== 'self'` skip and
    // needs no such gate. The skip existed because a legacy foe -Special debuff (Benumb,
    // Weaken, Time Stop) stored its magnitude as a POSITIVE `specialBuff` on a Foe-targeted
    // power, with nothing on the slot to say whose strength it was. An atom says so itself.
    const special = specialBuffValue(power);
    if (!special) continue;

    const targetsHit = targetsHitValues[power.internalName];
    // One cap for the whole map: every entry of a power's `specialBuff` is an `aspect: Str`
    // atom of the same family, so the depth is the power's, not the key's.
    const specialCap = stackCapOf(power, SPECIAL_BUFF_STACK);
    let defMax = 0;
    let mezMax = 0;
    for (const [key, raw] of Object.entries(special)) {
      const adjusted = adjustForStackCap(raw, targetsHit, specialCap, power);
      const frac = resolveScaledEffect(adjusted, archetypeId, buildLevel);
      if (frac <= 0) continue;
      if (STRENGTH_DEFENSE_KEYS.has(key)) defMax = Math.max(defMax, frac);
      else if (STRENGTH_MEZ_KEYS.has(key)) mezMax = Math.max(mezMax, frac);
      else if (key === 'tohit') sb.toHit += frac;
      else if (key === 'heal') sb.heal += frac;
      else if (key === 'absorb') sb.absorb += frac;
      else if (key === 'endurance') sb.endMod += frac;
      else if (key === 'movement') sb.movement += frac;
    }
    sb.defense += defMax;
    sb.mez += mezMax;
  }
  return sb;
}

/**
 * Resolve a ScaledEffect to its actual decimal value using AT tables.
 * For { scale: 3, table: "Melee_Res_Dmg" } with archetype "tanker" at level 50:
 *   → 3 × 0.10 = 0.30 (30% resistance)
 * For plain numbers, returns as-is.
 */
export function resolveScaledEffect(
  effect: ScalarOrScaled | undefined,
  archetypeId: string,
  level: number,
  _debugContext?: string
): number {
  if (effect === undefined) return 0;
  if (typeof effect === 'number') return effect;
  if (effect.table) {
    // "Ones" tables (Melee_Ones, Ranged_Ones) are constant 1.0 for all ATs
    if (effect.table.toLowerCase().endsWith('_ones')) {
      return effect.scale * 1.0;
    }
    const tableValue = getTableValue(archetypeId, effect.table.toLowerCase(), level);
    if (tableValue !== undefined) {
      return effect.scale * tableValue;
    }
  }
  // Fallback: use a default multiplier of 0.10 for resistance/defense tables.
  // Deduped per (table, archetype) so a recurring miss (e.g. Melee_Buff_Dmg on
  // damage buffs, which has no AT table) logs once instead of flooding on every
  // recalc.
  if (effect.table) {
    warnFallback('resolveScaledEffect', `AT table "${effect.table}" not found for "${archetypeId}" — using fallback 0.10`);
  }
  return effect.scale * 0.10;
}

// ============================================
// INCARNATE PROCESSING
// ============================================

/**
 * Extract Alpha incarnate effects as enhancement bonuses
 * These bonuses apply to ALL powers that accept the corresponding enhancement type
 * Returns a map of enhancement aspect to bonus value (as decimal, e.g., 0.33 = 33%)
 */
export function getAlphaEnhancementBonuses(
  incarnates: IncarnateBuildState | undefined,
  incarnateActive: IncarnateActiveState | undefined,
  incarnatesSuppressed = false,
): EnhancementBonuses {
  const alphaEffects = activeAlphaEffects(incarnates, incarnateActive, incarnatesSuppressed, getAlphaEffects);
  return alphaEffects ? mapAlphaEffectsToEnhancementBonuses(alphaEffects) : {};
}

/**
 * Per-aspect ED-bypass portion of the Alpha bonuses, in the same
 * EnhancementBonuses shape as `getAlphaEnhancementBonuses` — read from the
 * silent grant power's BoostIgnoreDiminishing / `Ones` templates rather than
 * a per-tier ratio (Thunderspy's authored splits diverge from the HC rarity
 * pattern). `combineWithAlphaED` adds exactly this slice after ED.
 */
export function getAlphaEdBypassBonuses(
  incarnates: IncarnateBuildState | undefined,
  incarnateActive: IncarnateActiveState | undefined,
  incarnatesSuppressed = false,
): EnhancementBonuses {
  const bypass = activeAlphaEffects(incarnates, incarnateActive, incarnatesSuppressed, getAlphaEdBypass);
  return bypass ? mapAlphaEffectsToEnhancementBonuses(bypass) : {};
}

/** Shared active/suppression gate for the alpha total and ED-bypass lookups. */
function activeAlphaEffects(
  incarnates: IncarnateBuildState | undefined,
  incarnateActive: IncarnateActiveState | undefined,
  incarnatesSuppressed: boolean,
  lookup: (powerId: string) => AlphaEffects | null,
): AlphaEffects | null {
  // Exemplared below 45 — incarnate abilities are off entirely.
  if (incarnatesSuppressed) return null;
  if (!incarnates?.alpha) return null;

  // Check if alpha is active
  const active = incarnateActive || { alpha: true, destiny: true, hybrid: true, interface: true, judgement: true, lore: true };
  if (!active.alpha) return null;

  return lookup(incarnates.alpha.powerId);
}

/**
 * Alpha data key → enhancement-aspect key. The two vocabularies genuinely
 * differ (`enduranceReduction`/`endurance`, `toHitBuff`/`tohit`), so the
 * translation is a table, not identity — and an alpha key MISSING from it is
 * a boost that vanishes with nothing to notice, which is how both the
 * `enduranceModification` long form and the `intangible`→Absorb slot-reuse
 * silently dropped whole bonuses. `alpha-aspect-coverage.test.ts` gates every
 * generated dataset's keys against this map, so a new one has to be mapped.
 * `levelShift` is deliberately absent: it isn't an enhancement aspect.
 */
export const ALPHA_KEY_TO_ENH_ASPECT: Record<string, string> = {
  damage: 'damage',
  accuracy: 'accuracy',
  recharge: 'recharge',
  enduranceReduction: 'endurance',
  enduranceModification: 'enduranceMod',
  range: 'range',
  heal: 'heal',
  defense: 'defense',
  resistance: 'resistance',
  hold: 'hold',
  stun: 'stun',
  immobilize: 'immobilize',
  sleep: 'sleep',
  fear: 'fear',
  confuse: 'confuse',
  slow: 'slow',
  toHitDebuff: 'tohitDebuff',
  defenseDebuff: 'defenseDebuff',
  toHitBuff: 'tohit',
  taunt: 'taunt',
  runSpeed: 'runSpeed',
  jumpSpeed: 'jumpSpeed',
  flySpeed: 'flySpeed',
  absorb: 'absorb',
};

function mapAlphaEffectsToEnhancementBonuses(alphaEffects: AlphaEffects): EnhancementBonuses {
  const bonuses: EnhancementBonuses = {};
  for (const [alphaKey, value] of Object.entries(alphaEffects) as [string, number | undefined][]) {
    const aspect = ALPHA_KEY_TO_ENH_ASPECT[alphaKey];
    if (aspect !== undefined && value !== undefined) bonuses[aspect] = value;
  }
  return bonuses;
}

export interface CalculationOptions {
  /** Per-category proc settings (default: all enabled) */
  procSettings?: ProcSettings;
  /** Per-target slider values keyed by power name (0 = inactive, 1+ = targets hit) */
  targetsHitValues?: Record<string, number>;
  /** Exemplar level for enhancement scaling (undefined = no scaling) */
  exemplarLevel?: number;
  /** Target enemy level offset for hit chance calculation (e.g. +3 = enemy is 3 levels above) */
  targetLevelOffset?: number;
  /** Vigilance team size for Defenders (0 = solo, 1+ = teammates) */
  vigilanceTeamSize?: number;
  /** Fury level for Brutes (0-100) */
  furyLevel?: number;
  /** How many of the loadout's EARNED incarnate level shifts to read the build with;
   *  `null`/undefined = all of them. A ceiling the engine spends down the earned grants
   *  (Alpha first), never a magnitude it can exceed. Independent of the per-slot stat
   *  toggles — equipping a shifting slot is what earns the shift. */
  incarnateLevelShift?: number | null;
  /** Seconds after cast to evaluate the (diminishing) Destiny buff at. `null` =
   *  the equipped power's sustained floor (default); 0 = additive peak; `undefined`
   *  = legacy flat peak values. Only affects Destiny powers that diminish over
   *  time (Mids-style time slider). */
  destinyTime?: number | null;
  /** How many foes are inside an active Melee Hybrid's sphere — the per-foe layer's only
   *  input. `undefined`/0 = solo, which applies nothing. The engine clamps it to the
   *  equipped tier's own ceiling (4, 7 or 9), so this never has to know which tier is on. */
  hybridTargetsHit?: number;
  /** Combat mode: suppress defenseBuffSuppressible from stealth/travel powers */
  combatMode?: boolean;
  /** Active global Mechanic Adjuster state — caster-state toggles shared across
   *  powers (Bio Armor adaptation modes, Hide, In Combat, …), keyed by the bare
   *  conditional `id`. Drives mode-gated conditionalEffects into the dashboard
   *  totals. Default `{}` → no conditionals applied, so totals are unchanged
   *  from a build with no toggles selected. */
  globalAdjusters?: Record<string, boolean>;
  /** Active per-power Mechanic Adjuster state — target-state toggles keyed
   *  `<internalName>:<id>` (drowning, Disintegrating, …). Default `{}`. */
  mechanicAdjusters?: Record<string, boolean>;
  /** Header Domination toggle — drives the `domination` conditional id, which the beta reads
   *  from AT-inherent state rather than `globalAdjusters` (PROD6C-3k). Display-only. */
  dominationActive?: boolean;
  /** Header alpha-strike toggle — with the build's Hide power it becomes the engine's `hidden`,
   *  which gates a from-Hide opener's mid-combat cast (PROD6C-3k). Display-only. */
  stalkerHidden?: boolean;
  /** The what-if TEAM-BUFF layer, keyed by the `GlobalBonuses` field each buff lands in.
   *  Unlike the display-only inputs above this one MOVES THE TOTALS — the engine injects it
   *  into the accumulators before projection, exactly where a teammate's real buff would land,
   *  so the archetype ceilings bind against it. Default `{}` (nothing simulated). */
  whatIfBuffs?: Record<string, number>;
}

/**
 * SPIKE5 — engine-backed totals. Assembles the calc context from the same args the beta
 * hook already passes, runs the build through `coh_math::recalculate` (wasm), and reshapes
 * the result. Falls back to an empty (all-zero) result in two cases, both loud:
 *   - the dataset isn't loaded yet at boot (`engineCalculate` returns null); the memo
 *     re-fires once `useEngineStore` marks it loaded, and
 *   - an input the engine can't honor (adjusters / destinyTime) makes the adapter throw —
 *     logged here rather than white-screening the app (fail-loud, not fail-fatal).
 */
export function calculateCharacterTotals(
  build: Build,
  exemplarMode = false,
  incarnateActive?: IncarnateActiveState,
  options?: CalculationOptions
): CharacterCalculationResult {
  const ctx: AdapterCalcContext = {
    exemplarMode,
    exemplarLevel: options?.exemplarLevel ?? 50,
    incarnateActive: incarnateActive ?? { alpha: false, destiny: false, hybrid: false, interface: false, judgement: false, lore: false, genesis: false },
    incarnateLevelShift: options?.incarnateLevelShift ?? null,
    targetsHitValues: options?.targetsHitValues ?? {},
    targetLevelOffset: options?.targetLevelOffset ?? 0,
    vigilanceTeamSize: options?.vigilanceTeamSize ?? 0,
    furyLevel: options?.furyLevel ?? 75,
    combatMode: options?.combatMode ?? false,
    destinyTime: options?.destinyTime ?? null,
    hybridTargetsHit: options?.hybridTargetsHit ?? null,
    globalAdjusters: options?.globalAdjusters ?? {},
    mechanicAdjusters: options?.mechanicAdjusters ?? {},
    dominationActive: options?.dominationActive ?? false,
    stalkerHidden: options?.stalkerHidden ?? false,
    whatIfBuffs: options?.whatIfBuffs ?? {},
  };

  try {
    const result = engineCalculate(withoutIllegalSlots(build), ctx);
    if (result) return result;
  } catch (err) {
    console.error('[engine] calculateCharacterTotals fell back to empty totals:', err);
    // The user must SEE this: the fallback below renders a full planner reading 0% with no
    // other signal. Deferred out of the render pass this runs inside (it is called from a
    // useMemo) — a zustand write during render warns; a microtask lands after it.
    queueMicrotask(() => useEngineStore.getState().setError(String(err)));
  }
  return {
    stats: createEmptyStats(),
    globalBonuses: createEmptyGlobalBonuses(),
    breakdown: new Map<string, DashboardStatBreakdown>(),
    setBonuses: {},
    bonusTracking: {},
    powerProjection: new Map(),
    engineStateJson: null,
    whatIfMoved: {},
    damageCeiling: null,
  };
}

/**
 * Get breakdown for a specific stat
 */
export function getBreakdownForStat(
  result: CharacterCalculationResult,
  stat: string
): DashboardStatBreakdown | undefined {
  return result.breakdown.get(stat);
}

/**
 * Check if a stat has any bonuses
 */
export function hasStatBonuses(
  result: CharacterCalculationResult,
  stat: string
): boolean {
  const bd = result.breakdown.get(stat);
  return bd !== undefined && bd.sources.length > 0;
}
