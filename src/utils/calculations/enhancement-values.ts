/**
 * City of Heroes - Enhancement Value Calculation System
 *
 * Parses IO set piece values and calculates enhancement bonuses
 * with Enhancement Diversification (ED) applied.
 *
 * Curve data (ED thresholds, schedule assignment, per-level strength,
 * multi-aspect scale, boost combine curve) comes from the active dataset's
 * generated enhancement-curves module via `@/data/enhancement-curves` —
 * emitted from the binary export and staleness-guarded against it.
 * The levels those curves may be read at come from the same export's crafted
 * boost roster via `@/data/boost-index`.
 */

import type { Enhancement, EnhancementStatType } from '@/types';
import { getEnhancementCurves } from '@/data/enhancement-curves';
import { getCommonIOLevels } from '@/data/boost-index';
import type { EnhancementSchedule, OriginTier } from '@/data/enhancement-curves';
import { isCalcDebugEnabled, debugGroup, debugGroupEnd, debugFormula } from '@/utils/calc-debug';

export type { EnhancementSchedule, OriginTier };

// ============================================
// SHARED CONSTANTS
// ============================================

/**
 * Multiplier an enhancement's values gain at +`boostLevel` combine boosts —
 * the `boosters` combine curve (`boost_effect_boosters.bin`). A boost level
 * outside the curve doesn't exist in-game, so it surfaces as an error.
 */
export function getBoostMultiplier(boostLevel: number): number {
  const { boosters } = getEnhancementCurves().boostEffectiveness;
  if (!Number.isInteger(boostLevel) || boostLevel < 0 || boostLevel >= boosters.length) {
    throw new Error(
      `Boost level must be an integer in 0..${boosters.length - 1}, got ${boostLevel}`,
    );
  }
  return boosters[boostLevel];
}

/**
 * The two axes an enhancement's stored level offset can sit on.
 *
 * These are DIFFERENT game mechanics read from DIFFERENT bins
 * (`boost_effect_boosters.bin` vs `boost_effect_above.bin` /
 * `boost_effect_below.bin`), and no enhancement has both:
 *
 *  - `booster` — Enhancement Booster combines, IOs only, unsigned.
 *  - `relative` — the enhancement's level minus the character's combat level,
 *    which is what an SO/DO/TO or a Hamidon-class special carries. Signed.
 *
 * Both sides of the twin previously read `boosters` for every type. That was
 * invisible because on Homecoming the two curves are numerically IDENTICAL over
 * 0..+3 — the whole range an unsigned field could express. They diverge only
 * below even (`below` falls at -10% per level, not -5%), and on Thunderspy,
 * whose above/below curves are flat 1.0 while its boosters curve is not.
 */
export type EnhancementLevelAxis = 'booster' | 'relative';

/** Which axis an enhancement's stored level offset sits on. */
export function enhancementLevelAxis(type: Enhancement['type']): EnhancementLevelAxis {
  return type === 'origin' || type === 'special' ? 'relative' : 'booster';
}

/**
 * The inclusive domain of the stored level offset for `type`, read from the
 * dataset's own curves rather than a hardcoded ±3.
 *
 * The datasets genuinely disagree and the difference is not cosmetic:
 *   Homecoming  above 0..+3, below 0..-3 (floor x0.70)
 *   Rebirth     above 0..+4, below 0..-9 (floor x0.10)
 *   Thunderspy  above/below flat 1.0 at every step — attenuation switched OFF
 *
 * A hardcoded rule would have invented a Thunderspy penalty the server does not
 * apply, and truncated Rebirth's much longer tail.
 */
export function enhancementLevelRange(
  type: Enhancement['type'],
  curves = getEnhancementCurves(),
): { min: number; max: number } {
  const { above, below, boosters } = curves.boostEffectiveness;
  if (enhancementLevelAxis(type) === 'booster') {
    return { min: 0, max: Math.max(0, boosters.length - 1) };
  }
  return { min: -Math.max(0, below.length - 1), max: Math.max(0, above.length - 1) };
}

/**
 * Effectiveness multiplier for a slot's stored level offset, off the dataset's
 * curves.
 *
 * Clamps at the ends of the curve rather than throwing, unlike
 * {@link getBoostMultiplier}: a Mids file can legitimately carry `PlusFive` on
 * an SO (past `above`'s four entries) and an older save can carry anything, so
 * an out-of-domain offset is ordinary input rather than a data defect.
 */
export function enhancementLevelMultiplier(
  slot: Pick<Enhancement, 'type' | 'boost'>,
  curves = getEnhancementCurves(),
): number {
  const offset = Math.trunc(slot.boost || 0);
  const { above, below, boosters } = curves.boostEffectiveness;
  // The booster axis is unsigned — there is no such thing as a negative
  // combine, so a negative there is a corrupt value, not a penalty to honour.
  const [curve, step] =
    enhancementLevelAxis(slot.type) === 'booster'
      ? [boosters, Math.max(0, offset)]
      : offset < 0
        ? [below, -offset]
        : [above, offset];
  if (curve.length === 0) return 1;
  return curve[Math.min(step, curve.length - 1)];
}

/** Base endurance recovery rate: 100 endurance in 60 seconds */
export const BASE_RECOVERY_RATE = 1.667;

/** Base regeneration rate: 100% max HP per 240 seconds */
export const BASE_REGEN_RATE = 100 / 240;

// ============================================
// EXEMPLAR MAGNITUDE HANDICAP
// ============================================

/**
 * curve[level - 1], top-clamped past the last entry — boost.c indexes the
 * handicap curves by combat level with a size-1 clamp. Levels are integers
 * in-game; a fractional level is a caller bug that must surface.
 */
function handicapCurveAt(curve: number[], level: number): number {
  if (!Number.isInteger(level)) {
    throw new Error(`Exemplar handicap curves are indexed by integer level, got ${level}`);
  }
  return curve[Math.min(Math.max(level, 1), curve.length) - 1];
}

/**
 * Apply the exemplar magnitude handicap to an enhancement value.
 *
 * Faithful boost.c `boost_HandicapExemplar` over the dataset's
 * exemplar_handicaps.bin curves, in the game's order:
 *   1. clamp to preClamp[exemplar] (0.4167 through level 45, then uncapped)
 *   2. if the magnitude reaches limits[exemplar] — the minor-bonus floor
 *      (5%/10%/20% by level band) — scale by weights[exemplar]/weights[ioLevel]
 *   3. clamp to postClamp[exemplar] (1.0 at every level)
 *
 * @param rawValue - The unscaled enhancement value (decimal, e.g. 0.424)
 * @param ioLevel - The true IO level (the level the enhancement was created at)
 * @param exemplarLevel - The level the character is exemplared to
 * @param isProc - Procs are never scaled
 */
export function applyExemplarScaling(
  rawValue: number,
  ioLevel: number,
  exemplarLevel: number,
  isProc = false
): number {
  // Procs are never scaled
  if (isProc) return rawValue;

  // No scaling needed if at or above IO level
  if (exemplarLevel >= ioLevel) return rawValue;

  const { limits, weights, preClamp, postClamp } = getEnhancementCurves().exemplarHandicaps;

  let magnitude = Math.min(rawValue, handicapCurveAt(preClamp, exemplarLevel));
  if (magnitude >= handicapCurveAt(limits, exemplarLevel)) {
    magnitude *= handicapCurveAt(weights, exemplarLevel) / handicapCurveAt(weights, ioLevel);
  }
  return Math.min(magnitude, handicapCurveAt(postClamp, exemplarLevel));
}

// ============================================
// ENHANCEMENT SCHEDULES
// ============================================

/**
 * Engine aspect key → `dim_returns` boost-type name.
 *
 * The schedule assignment is binary data: `dim_returns.bin` keys each ED
 * threshold set by boost type, with a default entry covering every type it
 * doesn't name. This map is the engine-side translation from normalized
 * aspect keys (the range of `normalizeAspectName`) to that boost-type
 * vocabulary; `null` marks aspects whose boost type has no named entry, so
 * they take the default bucket — the default entry IS the data, not a
 * fallback. An aspect key missing from this map entirely is a programming
 * error and fails loud in `getAspectSchedule`.
 */
export const ASPECT_BOOST_TYPE: Record<string, string | null> = {
  absorb: null,
  accuracy: null,
  confuse: null,
  damage: null,
  defenseDebuff: null,
  endurance: null,
  enduranceMod: null,
  fear: null,
  fly: null,
  heal: null,
  hold: null,
  immobilize: null,
  intangible: null,
  jump: null,
  mezDuration: null,
  recharge: null,
  run: null,
  sleep: null,
  slow: null,
  stun: null,
  taunt: null,

  defense: 'Buff_Defense',
  defenseBuff: 'Buff_Defense',
  tohit: 'Buff_ToHit',
  tohitBuff: 'Buff_ToHit',
  tohitDebuff: 'Debuff_ToHit',
  range: 'Range',
  resistance: 'Res_Damage',

  interrupt: 'Interrupt',

  knockback: 'Knockback',
};

/**
 * Get the enhancement schedule for a normalized aspect key.
 *
 * Mirrors the game's dim_returns lookup: a named boost type reads its
 * schedule from the dataset's `boostTypeSchedules`; a type the dataset
 * doesn't name (including every `null`-mapped aspect) takes the dataset's
 * default entry. An aspect outside the engine vocabulary throws — an
 * unknown aspect must surface, never silently enhance on the default
 * schedule.
 */
export function getAspectSchedule(normalizedAspect: string): EnhancementSchedule {
  const boostType = ASPECT_BOOST_TYPE[normalizedAspect];
  if (boostType === undefined) {
    throw new Error(
      `Unknown enhancement aspect "${normalizedAspect}" — not in the engine aspect vocabulary`,
    );
  }
  const { boostTypeSchedules, defaultSchedule } = getEnhancementCurves();
  if (boostType === null) return defaultSchedule;
  return boostTypeSchedules[boostType] ?? defaultSchedule;
}

/**
 * TO/DO/SO enhancement percentage for one aspect. The origin boost families
 * sit on flat Ones class tables, so a tier's value depends only on the
 * aspect's ED schedule — read per dataset from the curves' origin-tier grid
 * (Thunderspy rebalances the TO/DO ladders). A flat per-tier value would be
 * wrong in both directions: Schedule-B aspects enhance less (defense SO 20,
 * not 33.3), C/D more (interrupt SO 40, knockback SO 60).
 */
export function getOriginTierValue(tier: OriginTier, normalizedAspect: string): number {
  const { originTiers } = getEnhancementCurves();
  return originTiers[tier][getAspectSchedule(normalizedAspect)] * 100;
}

// ============================================
// IO EFFECTIVENESS BY LEVEL
// ============================================

/**
 * Enhancement strength of one boost at `level` for a schedule — a direct
 * read of the dataset's per-level strength curve (the `Melee_Boosts_*`
 * named class-modifier tables). Levels are integers in-game; a fractional
 * level is a caller bug and fails loud.
 *
 * The band is the crafted-record roster (10..50 on all four datasets), not the
 * curve: Homecoming's runs 105 entries deep because its class tables do, and
 * reading 51-53 off it paid out a level the game ships no recipe for — and paid
 * nothing on the forks, whose tables stop at 50. A level past the roster is a
 * stored value from before that band was read from the export (BOOST-6) or a
 * set level clamped elsewhere, so it clamps rather than throwing.
 */
export function getIOValueAtLevel(level: number, schedule: EnhancementSchedule = 'A'): number {
  if (!Number.isInteger(level)) {
    throw new Error(`Enhancement level must be an integer, got ${level}`);
  }
  const curve = getEnhancementCurves().schedules[schedule].strengthByBoostLevel;
  const craftLevels = getCommonIOLevels();
  const clampedLevel = Math.max(
    craftLevels[0],
    Math.min(craftLevels[craftLevels.length - 1], curve.length, level),
  );
  return curve[clampedLevel - 1];
}

// ============================================
// ASPECT NAME NORMALIZATION
// ============================================

/**
 * Map of aspect names to normalized internal keys
 */
export const ASPECT_NAME_MAP: Record<string, string> = {
  // Abbreviations
  Acc: 'accuracy',
  Dmg: 'damage',
  Dam: 'damage',
  Rech: 'recharge',
  EndRdx: 'endurance',
  EndMod: 'enduranceMod',
  Range: 'range',
  Heal: 'heal',
  Def: 'defense',
  Res: 'resistance',
  ToHit: 'tohit',
  ToHitDeb: 'tohitDebuff',
  DefDeb: 'defenseDebuff',
  DefBuff: 'defenseBuff',
  Hold: 'hold',
  Stun: 'stun',
  Immob: 'immobilize',
  Sleep: 'sleep',
  Confuse: 'confuse',
  Fear: 'fear',
  KB: 'knockback',
  Slow: 'slow',

  // EnhancementStatType values (used by generic IOs, origin enhancements, and specials)
  EnduranceReduction: 'endurance',
  EnduranceModification: 'enduranceMod',
  Interrupt: 'interrupt',
  Absorb: 'absorb',
  Intangible: 'intangible',
  'Mez Duration': 'mezDuration',

  // Full names
  Accuracy: 'accuracy',
  Damage: 'damage',
  Recharge: 'recharge',
  Endurance: 'endurance',
  'End Reduction': 'endurance',
  'Endurance Discount': 'endurance',
  'Endurance Reduction': 'endurance',
  'Endurance Modification': 'enduranceMod',
  Healing: 'heal',
  Defense: 'defense',
  'Defense Buff': 'defenseBuff',
  'Defense Debuff': 'defenseDebuff',
  'Resist Damage': 'resistance',
  'Damage Resistance': 'resistance',
  Resistance: 'resistance',
  'ToHit Buff': 'tohit',
  'To Hit Buff': 'tohit',
  'ToHit Debuff': 'tohitDebuff',
  'To Hit Debuff': 'tohitDebuff',
  'Hold Duration': 'hold',
  'Stun Duration': 'stun',
  Immobilize: 'immobilize',
  'Immobilization Duration': 'immobilize',
  'Sleep Duration': 'sleep',
  Confusion: 'confuse',
  'Confuse Duration': 'confuse',
  'Fear Duration': 'fear',
  Knockback: 'knockback',
  'Knockback Distance': 'knockback',
  Snare: 'slow',
  'Range Increase': 'range',
  Fly: 'fly',
  'Run Speed': 'run',
  Run: 'run',
  Jumping: 'jump',
  Jump: 'jump',
  Taunt: 'taunt',
  'Interrupt Time': 'interrupt',
  'Activation Acceleration': 'interrupt',
  // The set-piece spellings of aspects the powers name differently. An absent name is not an
  // inert one — every caller SKIPS the aspect — so each of these was a piece that enhanced
  // nothing at all while still being offered and still reading a value.
  Terrorize: 'fear',
  Threat: 'taunt',
  InterruptTime: 'interrupt',
};

/**
 * Normalize aspect names from IO pieces to internal keys
 */
export function normalizeAspectName(aspect: string): string | null {
  const trimmed = aspect.trim();
  return ASPECT_NAME_MAP[trimmed] || null;
}

/**
 * Aspect labels that fan out to several internal keys instead of normalizing to one,
 * mapped to the key a UI row should display for them. `Mez` fans out to six keys that
 * all carry the same value, so any is representative. `Move Speed` fans out to the
 * travel modes *and* Range, which sit on different schedules and so carry different
 * values — the row is labelled for movement, so it reads the movement value.
 */
const FANNED_ASPECT_DISPLAY_KEY: Record<string, string> = {
  Mez: 'hold',
  'Move Speed': 'run',
};

/**
 * Resolve the value a UI row should show for one piece aspect, handling the fan-out
 * labels that {@link normalizeAspectName} deliberately rejects. Shared so the picker
 * preview and the slotted-enhancement info panel cannot drift apart.
 */
export function readAspectDisplayValue(
  aspect: string,
  values: Record<string, number | undefined>,
): number | null {
  const normalized = normalizeAspectName(aspect);
  if (normalized) return values[normalized] ?? null;
  const fanned = FANNED_ASPECT_DISPLAY_KEY[aspect.trim()];
  return fanned ? values[fanned] ?? null : null;
}

/**
 * The enhancement a Generic (Common) IO of `stat` gives at `level`, as a PERCENT.
 *
 * A generic IO is single-aspect, so its value is that aspect's own schedule curve —
 * only the Schedule A stats reach the 42.4% that every generic IO used to be built
 * and labelled with. ToHit, ToHit Debuff, Defense, Resistance and Range sit on B
 * (25.5%), Interrupt on C (50.9%), Knockback on D (76.4%).
 *
 * Shared by the picker, the info panel and the slotting factory so the value a
 * player is shown is the one `accumulateRawSlotBonuses` actually applies — those
 * two read the same schedule off the same aspect name, and had no way to agree
 * while the display side went through a separate Schedule-A-only table.
 *
 * Null when the stat has no normalized aspect, which is a vocabulary gap rather
 * than a value of zero — `genericIOVocabulary` in enhancement-values.test.ts is
 * the guard that reds when a dataset introduces one.
 */
export function genericIOValueAtLevel(stat: string, level: number): number | null {
  const normalized = normalizeAspectName(stat);
  if (!normalized) return null;
  return getIOValueAtLevel(level, getAspectSchedule(normalized)) * 100;
}

// ============================================
// ENHANCEMENT DIVERSIFICATION
// ============================================

/**
 * Apply Enhancement Diversification to a bonus value.
 *
 * Diminishing returns in four tiers, thresholds and per-tier effectiveness
 * from the dataset's dim_returns data: full value to t1, then each slice
 * beyond a threshold counts at that tier's effectiveness (90% / 70% / 15%
 * on all three committed datasets).
 */
export function applyED(value: number, schedule: EnhancementSchedule = 'A'): number {
  const curves = getEnhancementCurves();
  const [t1, t2, t3] = curves.schedules[schedule].edThresholds;
  const [tier2Effectiveness, tier3Effectiveness, tier4Effectiveness] = curves.tierEffectiveness;

  if (value <= t1) {
    return value;
  } else if (value <= t2) {
    return t1 + (value - t1) * tier2Effectiveness;
  } else if (value <= t3) {
    const tier2 = t1 + (t2 - t1) * tier2Effectiveness;
    return tier2 + (value - t2) * tier3Effectiveness;
  } else {
    const tier2 = t1 + (t2 - t1) * tier2Effectiveness;
    const tier3 = tier2 + (t3 - t2) * tier3Effectiveness;
    return tier3 + (value - t3) * tier4Effectiveness;
  }
}

// ============================================
// IO SET PIECE VALUE PARSING
// ============================================

export interface ParsedBonuses {
  [aspect: string]: number;
}

/**
 * Per-aspect scale for an IO set piece by aspect count — the dataset's
 * multi-aspect ladder (modal standard crafted-piece fScale; 1 / 0.625 / 0.5
 * / 0.4375 on all three committed datasets). Counts past the ladder's last
 * rung keep its scale: an aspect list runs to four at most, but the
 * scale-derived `totalAspects` reaches 8 (Rebirth's Return From The Grave
 * "Rez Effects/Endurance/Recharge"), and 11 pieces across the three forks sit
 * past rung 4. A non-positive or fractional count is malformed input and
 * fails loud.
 */
export function getMultiAspectModifier(aspectCount: number): number {
  if (!Number.isInteger(aspectCount) || aspectCount < 1) {
    throw new Error(`IO piece aspect count must be a positive integer, got ${aspectCount}`);
  }
  const { multiAspectScale } = getEnhancementCurves();
  return multiAspectScale[Math.min(aspectCount, multiAspectScale.length) - 1];
}

/** Enhancement multiplier of the 25%-hot rarity tiers (purples + catalyzed Superior variants). */
const HIGH_RARITY_MULTIPLIER = 1.25;

/**
 * Enhancement multiplier per binary rarity tier (`boostsets.bin`). The 25%-hot
 * tiers are the very-rare (purple) tiers and the catalyzed Superior variants;
 * everything else — including the Rebirth one-off event tiers — enhances at
 * standard values. Closed vocabulary: an unknown tier must surface, never
 * silently enhance at 1.0. (Replaces the category=='purple' / "Superior"
 * name-prefix heuristic — SOURCE-1 item 3.)
 */
const RARITY_TIER_MULTIPLIER: Record<string, number> = {
  ECCommon: 1,
  ECUncommon: 1,
  ECRare: 1,
  ECPvP: 1,
  ECPVP: 1,
  ECATO: 1,
  ECATO2: 1,
  ECWinter: 1,
  ECHalloween: 1,
  ECSummer: 1,
  ECUniversalDamage: 1,
  // Rebirth one-off event tiers (Liberty's Belt, Imperial Might, Forced
  // Indoctrination, Synapse's Agility, Inexhaustibility's blank tier).
  LibertysBelt: 1,
  ImperialMight: 1,
  ForcedIndoctrination: 1,
  ECSpeedRun: 1,
  '': 1,
  ECVeryRare: HIGH_RARITY_MULTIPLIER,
  ECUltraRare: HIGH_RARITY_MULTIPLIER,
  ECSATO: HIGH_RARITY_MULTIPLIER,
  ECSATO2: HIGH_RARITY_MULTIPLIER,
  ECSWinter: HIGH_RARITY_MULTIPLIER,
  ECSHalloween: HIGH_RARITY_MULTIPLIER,
};

/** Get the rarity-based enhancement multiplier for an IO set's binary rarity tier. */
export function getSetRarityMultiplier(rarity: string): number {
  const multiplier = RARITY_TIER_MULTIPLIER[rarity];
  if (multiplier === undefined) {
    throw new Error(`Unknown set rarity tier "${rarity}" — not in the binary rarity vocabulary`);
  }
  return multiplier;
}

/**
 * Count the effective aspect "slots" on an IO set piece for the multi-aspect
 * scheduling penalty. The explicit `aspects` array only lists enhancement
 * attributes the piece directly enhances; it omits the global-proc and
 * power-grant segments that nonetheless count toward the penalty.
 *
 * Rules, in priority order:
 *   1. Explicit `totalAspects` wins. The extractor recovers it by inverting the
 *      piece's own enhancement scale, which is the game's authoritative dilution,
 *      and emits it exactly where the aspect list under-counts.
 *   2. Otherwise the aspect list, plus one for a proc's hidden segment.
 *
 * Heal and Absorb share a single enhancement category in-game: the Healing
 * enhancement boosts both attributes at the same value, so a piece listing
 * both occupies just ONE aspect slot for the scheduling penalty. The data
 * carries Absorb as its own aspect so it surfaces as a distinct enhanced stat,
 * but it must not dilute the per-aspect value — "Heal/Absorb/Recharge" is a
 * 2-aspect piece (26.5% @ L50), not 3 (21.2%), and a pure "Heal/Absorb" is a
 * 1-aspect piece (42.4%), not 2.
 *
 * The piece's display NAME is not an input. It once was, as a second count that
 * won when it was larger, back when names were assembled from the same attribs
 * as the aspect list. Names are the game's own now — read off the boost power —
 * and the game writes "Healing/Absorb", which the collapse above does not match:
 * counting its segments would dilute exactly the pieces the collapse exists for.
 * Measured across all three forks, taking the larger of the two counts moves
 * 145 of the 3,653 pieces, and every one of them moves the wrong way: 114 are
 * the Heal/Absorb pieces the collapse exists for, and the other 31 are pieces
 * whose name segments outrun their aspect list (Mocking Beratement's
 * "Taunt/Placate", Thunderspy's "Fear/Endurance/Recharge" over
 * `[Range, Terrorize]`). The extractor derives every piece's count from its
 * own enhancement scale and emits `totalAspects` whenever that exceeds the
 * aspect list, so on a piece with no override the list is not silence — it is
 * the measured answer, and a name count can only inflate it.
 */
export function getEffectiveAspectCount(
  aspects: string[],
  isProc: boolean,
  totalAspects: number | undefined,
): number {
  if (totalAspects != null) return totalAspects;
  const healAbsorb = aspects.includes('Heal') && aspects.includes('Absorb');
  return aspects.length + (isProc ? 1 : 0) - (healAbsorb ? 1 : 0);
}

// "Mez" is the universal-mez aspect carried by Controller and Dominator
// ATOs (Will of the Controller, Ascendency of the Dominator, Dominating
// Grasp, Overpowering Presence, and their Superior variants). In-game the
// game uses whichever of these six mez types the slotted power actually
// applies, so the planner expands the aspect to all six with the same
// value. Only the type that matches the power's effect contributes to its
// final stat; the rest are harmless extras that get filtered out at the
// effect-matching layer.
const UNIVERSAL_MEZ_KEYS = ['hold', 'stun', 'immobilize', 'sleep', 'confuse', 'fear'] as const;

/**
 * "Universal Travel" pieces (Winter's Gift, Blessing of the Zephyr) — ONE boost that
 * enhances every travel mode plus Range, which is why the game counts it as a single
 * aspect slot. Verified against the binary export
 * (`exported_powers/boosts/crafted_winters_gift_a`), whose in-game piece name is
 * literally "Run Speed, Jump, Flight Speed, Range":
 *
 *   ['RunningSpeed','FlyingSpeed','JumpingSpeed']  Melee_Boosts_33  scale 1.0
 *   ['JumpHeight']                                 Melee_Boosts_33  scale 1.0
 *   ['Range']                                      Melee_Boosts_20  scale 1.0
 *
 * The two tables are Schedule A and Schedule B respectively, and `scale` is just the
 * multi-aspect modifier (1.0 at one slot; the Endurance piece carries 0.625 across
 * all four templates, the two-slot rate) — so fanning out to these keys at their own
 * schedules reproduces the game numbers exactly. `jump` is the single jump category
 * covering both JumpingSpeed and JumpHeight, as elsewhere in the registries.
 *
 * The extractor labels this bundle "Move Speed" (`HC_PIECE_ASPECT_OVERRIDES` in
 * scripts/extract-rebirth-io-sets-v2.py), a name no consumer understood — so until
 * 2026-07-30 both Winter's Gift and Blessing of the Zephyr silently enhanced nothing
 * at all through their travel aspect.
 */
const UNIVERSAL_TRAVEL_KEYS = ['run', 'fly', 'jump', 'range'] as const;

/**
 * Every Heal-boosting enhancement boosts Absorb by the same amount — ONE boost
 * applied to two attribs, not two boosts. Verified in powers.bin: the generic
 * Healing IO `Boosts.Crafted_Heal_50` is a single `['Heal_Dmg','Absorb']`
 * @Strength template (its sibling template is `['HitPoints','Regeneration']`).
 *
 * IO-set pieces and most HOs list `Absorb` explicitly (see io-sets-heal-absorb
 * .test.ts); generic IOs, origin enhancements, and a few specials (Titan
 * Kyanite Shard, Positron Exposure) carry only "Healing". Mirror it in ONE
 * place per enhancement rather than patching every data source — and per
 * enhancement, so a piece that already lists Absorb is never counted twice.
 *
 * `absorb` is the aspect the absorb magnitude reads (engine `apply.rs`), which
 * is what lets an Absorb-ONLY boost (the Cardiac/Resilient Radial Alpha) land
 * without also inflating heals. Mutates and returns `bonuses`.
 */
function mirrorHealToAbsorb<T extends Record<string, number>>(bonuses: T): T {
  if (bonuses.heal !== undefined && bonuses.absorb === undefined) {
    (bonuses as Record<string, number>).absorb = bonuses.heal;
  }
  return bonuses;
}

/**
 * Merge ONE enhancement's per-aspect contribution into a power's running raw
 * totals, Heal→Absorb mirrored for that enhancement first. IO-set pieces come
 * pre-mirrored out of {@link parseIOSetPieceValues}; this is the seam for the
 * branches that build their aspects inline (generic IO, special, origin).
 */
function addEnhancementBonuses(raw: Record<string, number>, slotBonuses: Record<string, number>): void {
  for (const [aspect, value] of Object.entries(mirrorHealToAbsorb(slotBonuses))) {
    raw[aspect] = (raw[aspect] || 0) + value;
  }
}

/**
 * Parse IO set piece aspects into enhancement bonuses
 */
export function parseIOSetPieceValues(
  aspects: string[],
  level = 50,
  isProc = false,
  totalAspects?: number,
): ParsedBonuses {
  if (!aspects || !Array.isArray(aspects)) {
    return {};
  }

  const bonuses: ParsedBonuses = {};
  const effectiveAspectCount = getEffectiveAspectCount(aspects, isProc, totalAspects);
  const modifier = getMultiAspectModifier(effectiveAspectCount);

  // Each aspect gets the schedule's value modified by aspect count
  aspects.forEach((aspect) => {
    if (aspect.trim() === 'Mez') {
      // Universal-mez expansion (see UNIVERSAL_MEZ_KEYS comment above).
      for (const key of UNIVERSAL_MEZ_KEYS) {
        const value = getIOValueAtLevel(level, getAspectSchedule(key)) * modifier;
        bonuses[key] = (bonuses[key] ?? 0) + value;
      }
      return;
    }
    if (aspect.trim() === 'Move Speed') {
      // Universal-travel expansion (see UNIVERSAL_TRAVEL_KEYS above). Unlike Mez,
      // these do NOT share a schedule — Range is B where the travel modes are A —
      // so each key is looked up on its own.
      for (const key of UNIVERSAL_TRAVEL_KEYS) {
        const baseValue = getIOValueAtLevel(level, getAspectSchedule(key));
        bonuses[key] = (bonuses[key] ?? 0) + baseValue * modifier;
      }
      return;
    }
    const normalized = normalizeAspectName(aspect);
    if (normalized) {
      const schedule = getAspectSchedule(normalized);
      const baseValue = getIOValueAtLevel(level, schedule);
      bonuses[normalized] = baseValue * modifier;
    }
  });

  return mirrorHealToAbsorb(bonuses);
}

// ============================================
// POWER ENHANCEMENT BONUS CALCULATION
// ============================================

export interface SlotWithEnhancement {
  enhancement: Enhancement | null;
  level?: number;
}

export interface PowerWithSlots {
  name: string;
  slots: (Enhancement | null)[];
  /** What enhancement categories the power accepts. Used by
   * `combineWithAlphaED` to gate Alpha bonuses — without this, Alpha
   * Intuition Radial T4 (+33% Damage) would boost the damage of every
   * active toggle in the build, including ones like Tactical Training:
   * Assault whose `allowedEnhancements` is just EndRdx + Recharge. The
   * displayed +Damage on such powers would then inflate by the alpha
   * multiplier (15% × 1.33 = 19.95%) instead of the true 15%. */
  allowedEnhancements?: EnhancementStatType[];
}

/** Filter an EnhancementBonuses object to only include aspects the power
 * accepts via its `allowedEnhancements` list. Used by `combineWithAlphaED`
 * and by callers that bypass it (no-slot powers). When `allowed` is
 * undefined, returns the bonuses unchanged. */
export function filterAlphaByAllowedEnhancements(
  alphaBonuses: EnhancementBonuses,
  allowed: EnhancementStatType[] | undefined,
): EnhancementBonuses {
  if (!allowed) return { ...alphaBonuses };
  const allowedAspects = new Set(
    Object.entries(ASPECT_TO_ENH_TYPE)
      .filter(([, enhType]) => allowed.includes(enhType))
      .map(([aspectKey]) => aspectKey),
  );
  const filtered: EnhancementBonuses = {};
  for (const [aspect, value] of Object.entries(alphaBonuses)) {
    if (allowedAspects.has(aspect)) {
      filtered[aspect] = value;
    }
  }
  return filtered;
}

/** Aspect-key → EnhancementStatType reverse map. Mirrors `ASPECT_NAME_MAP`
 * which goes the other direction; used to gate Alpha bonuses by whether
 * a power accepts the corresponding enhancement category. */
const ASPECT_TO_ENH_TYPE: Record<string, EnhancementStatType> = {
  damage: 'Damage',
  accuracy: 'Accuracy',
  recharge: 'Recharge',
  endurance: 'EnduranceReduction',
  enduranceMod: 'EnduranceModification',
  range: 'Range',
  heal: 'Healing',
  defense: 'Defense',
  defenseBuff: 'Defense',
  resistance: 'Resistance',
  tohit: 'ToHit',
  tohitDebuff: 'ToHit Debuff',
  defenseDebuff: 'Defense Debuff',
  hold: 'Hold',
  stun: 'Stun',
  immobilize: 'Immobilize',
  sleep: 'Sleep',
  confuse: 'Confuse',
  fear: 'Fear',
  knockback: 'Knockback',
  slow: 'Slow',
  taunt: 'Taunt',
  interrupt: 'Interrupt',
  // Absorb has no enhancement category of its own — it is the second attrib of
  // the HEALING boost (see mirrorHealToAbsorb), so a power that accepts Healing
  // is exactly the power an Absorb buff reaches. Keying this to a nonexistent
  // "Absorb" category filtered the Cardiac/Resilient Radial Alpha's +33% Absorb
  // out of every power in the build.
  absorb: 'Healing',
  intangible: 'Intangible',
  mezDuration: 'Mez Duration',
  run: 'Run Speed',
  fly: 'Fly',
  jump: 'Jump',
};

export interface EnhancementBonuses {
  accuracy?: number;
  damage?: number;
  recharge?: number;
  endurance?: number;
  range?: number;
  heal?: number;
  defense?: number;
  resistance?: number;
  tohit?: number;
  tohitDebuff?: number;
  [key: string]: number | undefined;
}

/**
 * What ONE enhancement contributes, under the exact rules the dashboard applies to a real
 * slot — attunement, the set's level cap, exemplar scaling, boosters, the multi-aspect
 * penalty, and the `Mez` fan-out into the six mez aspects.
 *
 * The per-piece display surfaces (picker preview, enhancement info panel) render this
 * instead of re-deriving a level rule of their own. They each had one, and each contradicted
 * the calculation on three counts: capping an attuned IO at the set's `maxLevel` (which the
 * game does not do — see `ioLevel` below), ignoring exemplar, and dropping the `Mez` aspect
 * because `normalizeAspectName` has no entry for it (PROD6E-2).
 *
 * ED is applied — this routes through the full calculation deliberately, so a preview cannot
 * drift from the total it previews. At single-piece magnitudes it is a no-op: measured over
 * all 1138 aspect-bearing Homecoming set pieces at +5 boost, no piece reaches its schedule's
 * knee.
 */
export function calculateSingleEnhancementValues(
  slot: Enhancement,
  buildLevel: number,
  getIOSet: Parameters<typeof calculatePowerEnhancementBonuses>[2],
  exemplarLevel?: number
): EnhancementBonuses {
  return calculatePowerEnhancementBonuses(
    { name: '', slots: [slot] },
    buildLevel,
    getIOSet,
    exemplarLevel
  );
}

/**
 * Every slot's raw (pre-ED) per-aspect contribution, across the four slot kinds
 * (IO set / generic IO / special / origin).
 *
 * ONE accumulator, shared by `calculatePowerEnhancementBonuses` and `combineWithAlphaED`,
 * because they were separate copies of this loop and drifted: the Alpha copy never picked up
 * the `slot.level` fix, so with an Alpha active every non-attuned set IO was read at the
 * character's level instead of its craft level (a level-30 Artillery triple paid 21.2%
 * recharge, not 17.4%). An Alpha changes what is ADDED to a power's aspects — never how the
 * power's own slots are read — so there is nothing here for a caller to vary.
 */
function accumulateRawSlotBonuses(
  slots: PowerWithSlots['slots'],
  globalIOLevel: number,
  getIOSet: ((setId: string) => { pieces: Array<{ num: number; name?: string; aspects?: string[]; proc?: boolean; totalAspects?: number }>; maxLevel: number; rarity: string; category?: string; name?: string } | undefined) | undefined,
  exemplarLevel: number | undefined,
): Record<string, number> {
  const rawBonuses: Record<string, number> = {};

  slots.forEach((slot) => {
    if (!slot) return;

    const boostMultiplier = enhancementLevelMultiplier(slot);

    if (slot.type === 'io-set' && getIOSet) {
      // IO Set piece
      const set = getIOSet(slot.setId);
      if (!set) return;

      const piece = set.pieces.find((p) => p.num === slot.pieceNum);
      if (!piece?.aspects) return;

      // Attuned if: inherently attuned set (ATO/Event with maxLevel <= 1) OR
      // individually attuned IO (slot.attuned flag from catalyzed/purchased attuned)
      // When attuned, compute at character level (they auto-scale)
      // Non-attuned IOs compute at their fixed level, then get scaled
      const isAttuned = set.maxLevel <= 1 || slot.attuned === true;
      let ioLevel: number;
      if (isAttuned) {
        // Attuned IOs scale with character level, uncapped by set.maxLevel.
        // E.g., attuned Triage (set maxLevel 30) in an L50 character behaves
        // as an L50 IO (Heal dual = 26.5%, not the L30-capped 21.75%).
        // Verified 2026-05-18 against the in-game enhancement tooltip.
        ioLevel = exemplarLevel ?? globalIOLevel;
      } else {
        // Non-attuned: use the slot's stored level (the level the IO was
        // crafted at), then cap by the set's max level. Fall back to
        // globalIOLevel when the slot has no explicit level (legacy
        // builds, picker default). Previously this ignored `slot.level`
        // entirely and used globalIOLevel — which meant a Gaussian's
        // dual slotted at L27 was being computed as an L50 IO (33% recharge
        // pre-ED vs the 20.7% the user actually has).
        const desiredLevel = (slot as { level?: number }).level ?? globalIOLevel;
        ioLevel = Math.min(desiredLevel, set.maxLevel);
      }
      const rarityMultiplier = getSetRarityMultiplier(set.rarity);
      const bonuses = parseIOSetPieceValues(piece.aspects, ioLevel, piece.proc, piece.totalAspects);

      Object.entries(bonuses).forEach(([aspect, value]) => {
        let scaledValue = value * rarityMultiplier * boostMultiplier;
        // Apply exemplar scaling for non-attuned IOs (attuned already computed at exemplar level)
        // Pure procs (no aspects) skip exemplar scaling; hybrid procs scale their aspect portion
        const isPureProc = piece.proc && (!piece.aspects || piece.aspects.length === 0);
        if (exemplarLevel !== undefined && !isAttuned && !isPureProc) {
          scaledValue = applyExemplarScaling(scaledValue, ioLevel, exemplarLevel);
        }
        rawBonuses[aspect] = (rawBonuses[aspect] || 0) + scaledValue;
      });
    } else if (slot.type === 'io-generic') {
      // Common IO (non-attuned, fixed level)
      const aspect = slot.stat as string;
      const normalized = normalizeAspectName(aspect);
      if (normalized) {
        const schedule = getAspectSchedule(normalized);
        const genericIOLevel = slot.level || globalIOLevel;
        let value = getIOValueAtLevel(genericIOLevel, schedule) * boostMultiplier;
        // Apply exemplar scaling for generic IOs
        if (exemplarLevel !== undefined) {
          value = applyExemplarScaling(value, genericIOLevel, exemplarLevel);
        }
        addEnhancementBonuses(rawBonuses, { [normalized]: value });
      }
    } else if (slot.type === 'special') {
      // Special enhancements (Hamidon, Titan, etc.) - each aspect has its own value
      if (slot.aspects) {
        const slotBonuses: Record<string, number> = {};
        slot.aspects.forEach((aspect: { stat: string; value: number }) => {
          const normalized = normalizeAspectName(aspect.stat);
          if (normalized) {
            slotBonuses[normalized] = (slotBonuses[normalized] || 0) + (aspect.value / 100) * boostMultiplier;
          }
        });
        addEnhancementBonuses(rawBonuses, slotBonuses);
      }
    } else if (slot.type === 'origin') {
      // TO/DO/SO — value derived from tier + aspect schedule (per dataset),
      // not the stamped display value, so a persisted build can't carry
      // stale flat-tier numbers into the math.
      const normalized = normalizeAspectName(slot.stat as string);
      if (normalized) {
        const value = getOriginTierValue(slot.tier, normalized);
        addEnhancementBonuses(rawBonuses, { [normalized]: (value / 100) * boostMultiplier });
      }
    }
  });

  return rawBonuses;
}

/**
 * Calculate total enhancement bonuses from slotted enhancements
 * Applies Enhancement Diversification (ED) limits
 */
export function calculatePowerEnhancementBonuses(
  power: PowerWithSlots,
  globalIOLevel = 50,
  getIOSet?: (setId: string) => { pieces: Array<{ num: number; name?: string; aspects?: string[]; proc?: boolean; totalAspects?: number }>; maxLevel: number; rarity: string; category?: string; name?: string } | undefined,
  exemplarLevel?: number
): EnhancementBonuses {
  if (!power?.slots) {
    return {};
  }

  const rawBonuses = accumulateRawSlotBonuses(power.slots, globalIOLevel, getIOSet, exemplarLevel);

  // Apply Enhancement Diversification
  const edBonuses: EnhancementBonuses = {};
  Object.entries(rawBonuses).forEach(([aspect, rawValue]) => {
    const schedule = getAspectSchedule(aspect);
    edBonuses[aspect] = applyED(rawValue, schedule);
  });

  // Debug: log enhancement breakdown for this power
  if (isCalcDebugEnabled() && Object.keys(rawBonuses).length > 0) {
    debugGroup(`Enhancements: ${power.name}`);
    // Log each slot
    const slotCount = power.slots?.filter(Boolean).length ?? 0;
    debugFormula(`${slotCount} slot(s) filled`);
    // Log raw → ED per aspect
    for (const [aspect, rawValue] of Object.entries(rawBonuses)) {
      const schedule = getAspectSchedule(aspect);
      const edValue = edBonuses[aspect] ?? rawValue;
      const rawPct = (rawValue * 100).toFixed(1);
      const edPct = ((edValue as number) * 100).toFixed(1);
      const edHit = Math.abs(rawValue - (edValue as number)) > 0.001;
      debugFormula(`${aspect} (Schedule ${schedule}): raw ${rawPct}% → ED ${edPct}%${edHit ? ' (ED reduced)' : ''}`);
    }
    debugGroupEnd();
  }

  return edBonuses;
}

/**
 * Combine IO enhancement bonuses with Alpha incarnate bonuses, properly
 * handling the Alpha ED bypass mechanic.
 *
 * Alpha bonuses are split into two portions per aspect:
 * - ED-subject portion (total − bypass): added to raw IO totals BEFORE ED
 * - ED-bypass portion (`alphaEdBypass[aspect]`): added AFTER ED
 *
 * The bypass values come from the exported silent-grant data
 * (`getAlphaEdBypassBonuses` — the BoostIgnoreDiminishing / `Ones`
 * templates), not from a per-tier ratio: Thunderspy authors many aspects
 * with a different split than the HC/Rebirth rarity pattern.
 */
export function combineWithAlphaED(
  power: PowerWithSlots,
  globalIOLevel: number,
  getIOSet: Parameters<typeof calculatePowerEnhancementBonuses>[2],
  alphaBonuses: EnhancementBonuses,
  alphaEdBypass: EnhancementBonuses,
  exemplarLevel?: number
): EnhancementBonuses {
  if (!power?.slots) return { ...alphaBonuses };

  // Step 1: Compute raw IO bonuses (before ED) — the same accumulator the non-Alpha path
  // uses, so a slot is read identically whether or not an Alpha happens to be active.
  const rawBonuses = accumulateRawSlotBonuses(power.slots, globalIOLevel, getIOSet, exemplarLevel);

  // Alpha is a global boost that applies only to aspects the power's
  // `allowedEnhancements` list accepts (matches the game's behaviour —
  // Alpha Intuition Radial T4's +33% Damage doesn't boost Tactical
  // Training: Assault because TT:Assault only accepts EndRdx + Recharge).
  // When `allowedEnhancements` isn't supplied (legacy callers, generic
  // contexts), fall back to "apply everything" to preserve prior behaviour.
  const allowedAspectKeys: Set<string> | null = power.allowedEnhancements
    ? new Set(
        Object.entries(ASPECT_TO_ENH_TYPE)
          .filter(([, enhType]) => power.allowedEnhancements!.includes(enhType))
          .map(([aspectKey]) => aspectKey),
      )
    : null;
  const alphaAcceptsAspect = (aspect: string): boolean =>
    allowedAspectKeys === null || allowedAspectKeys.has(aspect);

  // Step 2: Add ED-subject portion of alpha (total − bypass) to raw totals
  for (const [aspect, value] of Object.entries(alphaBonuses)) {
    if (value !== undefined && value !== 0 && alphaAcceptsAspect(aspect)) {
      const edSubject = value - (alphaEdBypass[aspect] ?? 0);
      rawBonuses[aspect] = (rawBonuses[aspect] || 0) + edSubject;
    }
  }

  // Step 3: Apply ED to the combined totals
  const result: EnhancementBonuses = {};
  Object.entries(rawBonuses).forEach(([aspect, rawValue]) => {
    const schedule = getAspectSchedule(aspect);
    result[aspect] = applyED(rawValue, schedule);
  });

  // Step 4: Add ED-bypass portion of alpha on top (same gate as Step 2)
  for (const [aspect, value] of Object.entries(alphaBonuses)) {
    if (value !== undefined && value !== 0 && alphaAcceptsAspect(aspect)) {
      const bypass = alphaEdBypass[aspect] ?? 0;
      result[aspect] = (result[aspect] || 0) + bypass;
    }
  }

  return result;
}

// ============================================
// UTILITY FUNCTIONS
// ============================================

/**
 * Format enhancement value as percentage
 */
export function formatEnhancementValue(value: number): string {
  return `${(value * 100).toFixed(1)}%`;
}
