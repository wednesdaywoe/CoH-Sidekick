/**
 * Effect Registry - Data-driven configuration for power effect display
 *
 * Instead of hardcoding checks for each effect type in components,
 * this registry defines how each effect should be displayed.
 * Components iterate over effects and use this registry to render them.
 */

import type { ArchetypeId, NumberOrScaled, NumberOrMez, MezEffect } from '@/types';
import { getScaleValue } from '@/types';
import { calculateBuffDebuffPercent } from '@/utils/calculations';
import { getTableValue } from '@/data/at-tables';
import { STAT_COLORS } from './stat-colors';
import { formatPrecision } from '@/utils/format-precision';

// ============================================
// TYPES
// ============================================

export type EffectCategory =
  | 'execution'   // Power execution stats (endurance, recharge, accuracy, range)
  | 'damage'      // Direct damage, DoT
  | 'control'     // Hold, stun, immobilize, etc.
  | 'buff'        // Positive effects on self/allies
  | 'debuff'      // Negative effects on enemies
  | 'protection'  // Defense, resistance, mez protection
  | 'movement'    // Speed, fly, teleport
  | 'special';    // Summons, unique effects

export type EffectFormat =
  | 'percent'     // Display as percentage (e.g., +25%)
  | 'value'       // Display as raw value
  | 'mag'         // Display as magnitude (Mag 3)
  | 'duration'    // Display with seconds (12s)
  | 'scale'       // Display scale value
  | 'damage'      // Use damage calculation system
  | 'degrees'     // Display as degrees (e.g., 30°)
  | 'distance'    // Display as whole feet (20ft) — `range`, `radius`
  | 'custom';     // Needs special handling

export interface EffectDisplayConfig {
  /** Display label */
  label: string;
  /** Effect category for grouping */
  category: EffectCategory;
  /** Tailwind color class */
  colorClass: string;
  /** How to format the value */
  format: EffectFormat;
  /** Enhancement aspect that modifies this (for three-tier display) */
  enhancementAspect?: string;
  /**
   * Build-wide global / +Strength key that scales this effect, when it differs from
   * `enhancementAspect`. The server reads Strength at the attrib mod's OWN offset, while one
   * boost template may enhance several attribs at once — a Healing IO lists `Heal_Dmg` AND
   * `Absorb`, so absorb IS heal-enhanceable, but +Heal Strength and the +Heal set bonus target
   * `Heal_Dmg` alone and never reach it. Absent = same key as the enhancement aspect.
   */
  strengthAspect?: string;
  /** Whether this is a buff or debuff for percentage calculation */
  calculation?: 'buff' | 'debuff';
  /** Priority for display order within category (lower = first) */
  priority?: number;
  /** Whether this effect can have "by type" variants (defense, resistance) */
  canBeByType?: boolean;
  /** Whether by-type variants should expand into individual rows (vs abbreviated summary) */
  expandByType?: boolean;
  /**
   * Max decimal places for the displayed value, declared per effect so the
   * data — not the rendering component — owns precision. Falls back to
   * DEFAULT_EFFECT_PRECISION[format] when unset (2 for the percent/value
   * stat-like tier). Set explicitly only for exceptions, e.g. an effect whose
   * authored value carries a 3rd decimal. See [[formatPrecision]] for the
   * shared round-then-strip mechanic and [[formatBonusValue]] for the
   * set-bonus 3-decimal tier. */
  precision?: number;
  /** Custom render key if different from effect key */
  renderAs?: string;
  /**
   * Words that name this effect in a power's authored one-line summary — the export's
   * `shortHelp`, which is the game designers' own answer to "what is this power for"
   * (`Toggle: Ranged (Targeted AoE), Foe -DEF, -To Hit`). Unsigned: the clause's `+`/`-` picks
   * between the buff and debuff key through `category`, so `def` sits on `defense` and
   * `defenseDebuff` alike. Absent on the `execution` rows, which are never gated.
   *
   * Carried here to keep this copy level with `contract/effect-registry.json` — the drift gate
   * in `effectRegistryDrift.test.ts` compares the two, and the contract is where the tokens are
   * authored. Nothing beta-side reads them yet: the gate that consumes them
   * (`coh_math::effect_registry::summary_gate`) is Rust, and has no TS counterpart.
   */
  summaryTokens?: string[];
  /** Base value to multiply by (e.g., accuracy is multiplier × 75% base to-hit) */
  baseMultiplier?: number;
  /**
   * If set, treat the effect as a flat percentage per scale point (ignoring
   * any AT-table reference in the data). Used by effects like maxHPBuff,
   * which the game stores with a heal-table reference for engine bookkeeping
   * but actually applies as a fixed 5% per scale point. Without this flag,
   * the display would multiply scale × heal-table-value × 100 and produce
   * absurd percentages.
   */
  flatPercentPerScale?: number;
  /**
   * A NON-by-type value on an `expandByType` effect resolves through the table-base
   * resistance-percent path rather than the generic percent path. Declared as data so
   * neither the component nor the engine branches on the effect NAME.
   */
  scalarFromTablePercent?: boolean;
  /**
   * A `value`-format effect whose scale resolves to an AMOUNT through its AT table at the
   * build level (heal / absorb HP) rather than displaying the bare scale.
   */
  valueFromTable?: boolean;
  /**
   * An authored `maxHPFraction`, or a scale on a `*_Ones` table, means "this fraction of
   * Max HP" and displays as a percent instead of an amount.
   */
  maxHpFractionPercentForm?: boolean;
}

// ============================================
// VALUE FORMATTING
// ============================================

/**
 * Default max-decimals per effect format. Power-effect values are the
 * planner's 2-decimal "stat-like" tier — 2 covers every current effect
 * (defense 3.75%, resistance 7.5%, +recovery, …). Authored 3-decimal values
 * live in set bonuses ([[formatBonusValue]]), not here. Override a specific
 * effect via EffectDisplayConfig.precision only when it genuinely needs finer
 * or coarser precision.
 */
const DEFAULT_EFFECT_PRECISION: Record<EffectFormat, number> = {
  percent: 2,
  value: 2,
  duration: 2,
  scale: 2,
  damage: 1,
  mag: 1,
  degrees: 0,
  distance: 0,
  custom: 2,
};

/** Resolve an effect's display precision: its declared `precision`, else the
 *  format default. */
export function effectValuePrecision(config: EffectDisplayConfig): number {
  return config.precision ?? DEFAULT_EFFECT_PRECISION[config.format];
}

/**
 * Format a resolved effect value (already in display units — percent points,
 * seconds, etc.) per its full registry config: read the config's declared
 * precision (or the format default) and round-then-strip via the bare
 * [[formatEffectValue]]. Adds the one label-dependent case (Range/Radius
 * render as feet). This is the entry point power-effect rows render through,
 * so precision is owned by the effect definition rather than each component's
 * `toFixed`.
 */
export function formatEffectValueForConfig(value: number, config: EffectDisplayConfig): string {
  if (config.format === 'value' && (config.label === 'Range' || config.label === 'Radius')) {
    return `${formatPrecision(value, 0)}ft`;
  }
  return formatEffectValue(value, config.format, effectValuePrecision(config));
}

// ============================================
// EFFECT REGISTRY
// ============================================

export const EFFECT_REGISTRY: Record<string, EffectDisplayConfig> = {
  // === EXECUTION (Power Stats) ===
  enduranceCost: {
    label: 'End Cost',
    category: 'execution',
    colorClass: STAT_COLORS.endurance,
    format: 'value',
    enhancementAspect: 'endurance',
    priority: 1,
  },
  buffDuration: {
    label: 'Duration',
    category: 'execution',
    colorClass: STAT_COLORS.buffDuration,
    format: 'duration',
    priority: 1.5,
  },
  recharge: {
    label: 'Rech Time',
    category: 'execution',
    colorClass: STAT_COLORS.recharge,
    format: 'duration',
    enhancementAspect: 'recharge',
    priority: 2,
  },
  accuracy: {
    label: 'Accuracy',
    category: 'execution',
    colorClass: STAT_COLORS.accuracy,
    format: 'percent',
    enhancementAspect: 'accuracy',
    priority: 3,
    baseMultiplier: 75,  // Accuracy is multiplier × 75% base to-hit
  },
  range: {
    label: 'Pwr Range',
    category: 'execution',
    colorClass: STAT_COLORS.range,
    format: 'distance',
    enhancementAspect: 'range',
    priority: 4,
  },
  castTime: {
    label: 'Activation',
    category: 'execution',
    colorClass: STAT_COLORS.castTime,
    format: 'duration',
    priority: 5,
  },
  effectDuration: {
    label: 'Effect Dur',
    category: 'execution',
    colorClass: STAT_COLORS.effectDuration,
    format: 'duration',
    priority: 6.5,
  },
  radius: {
    label: 'Radius',
    category: 'execution',
    colorClass: STAT_COLORS.radius,
    format: 'distance',
    priority: 7,
  },
  arc: {
    label: 'Arc',
    category: 'execution',
    colorClass: STAT_COLORS.radius,
    format: 'degrees',
    priority: 8,
  },
  maxTargets: {
    label: 'Max Targets',
    category: 'execution',
    colorClass: STAT_COLORS.radius,
    format: 'value',
    priority: 9,
  },

  // === CONTROL (Mez Effects — all pink) ===
  hold: {
    summaryTokens: ['hold'],
    label: 'Hold',
    category: 'control',
    colorClass: STAT_COLORS.hold,
    format: 'mag',
    priority: 1,
  },
  stun: {
    summaryTokens: ['stun', 'disorient'],
    label: 'Stun',
    category: 'control',
    colorClass: STAT_COLORS.stun,
    format: 'mag',
    priority: 2,
  },
  immobilize: {
    summaryTokens: ['immobilize', 'immob'],
    label: 'Immobilize',
    category: 'control',
    colorClass: STAT_COLORS.immobilize,
    format: 'mag',
    priority: 3,
  },
  sleep: {
    summaryTokens: ['sleep'],
    label: 'Sleep',
    category: 'control',
    colorClass: STAT_COLORS.sleep,
    format: 'mag',
    priority: 4,
  },
  fear: {
    summaryTokens: ['fear', 'terrorize'],
    label: 'Fear',
    category: 'control',
    colorClass: STAT_COLORS.fear,
    format: 'mag',
    priority: 5,
  },
  confuse: {
    summaryTokens: ['confuse'],
    label: 'Confuse',
    category: 'control',
    colorClass: STAT_COLORS.confuse,
    format: 'mag',
    priority: 6,
  },
  taunt: {
    summaryTokens: ['taunt', 'threat'],
    label: 'Taunt',
    category: 'control',
    colorClass: STAT_COLORS.taunt,
    format: 'mag',
    priority: 7,
  },
  placate: {
    summaryTokens: ['placate'],
    label: 'Placate',
    category: 'control',
    colorClass: STAT_COLORS.placate,
    format: 'mag',
    priority: 8,
  },
  knockback: {
    summaryTokens: ['knockback', 'knockdown', 'kb'],
    label: 'Knockback',
    category: 'control',
    colorClass: STAT_COLORS.knockback,
    format: 'mag',
    priority: 10,
  },
  knockup: {
    summaryTokens: ['knockup', 'knockback', 'knockdown'],
    label: 'Knockup',
    category: 'control',
    colorClass: STAT_COLORS.knockup,
    format: 'mag',
    priority: 11,
  },
  repel: {
    summaryTokens: ['repel'],
    label: 'Repel',
    category: 'control',
    colorClass: STAT_COLORS.repel,
    format: 'mag',
    priority: 12,
  },

  // === DEBUFFS (dimmed versions of buff colors) ===
  tohitDebuff: {
    summaryTokens: ['to hit', 'tohit', 'to-hit'],
    label: '-ToHit',
    category: 'debuff',
    colorClass: STAT_COLORS.tohitDebuff,
    format: 'percent',
    calculation: 'debuff',
    enhancementAspect: 'tohitDebuff',
    priority: 1,
  },
  accuracyDebuff: {
    summaryTokens: ['acc', 'accuracy'],
    label: '-Accuracy',
    category: 'debuff',
    colorClass: STAT_COLORS.accuracy,
    format: 'percent',
    calculation: 'debuff',
    priority: 1,
  },
  defenseDebuff: {
    summaryTokens: ['def', 'defense'],
    label: '-Defense',
    category: 'debuff',
    colorClass: STAT_COLORS.defenseDebuff,
    format: 'percent',
    calculation: 'debuff',
    enhancementAspect: 'defenseDebuff',
    canBeByType: true,
    priority: 2,
  },
  resistanceDebuff: {
    summaryTokens: ['res', 'resist', 'resistance'],
    label: '-Resist',
    category: 'debuff',
    colorClass: STAT_COLORS.resistanceDebuff,
    format: 'percent',
    calculation: 'debuff',
    enhancementAspect: 'resistanceDebuff',
    canBeByType: true,
    priority: 3,
  },
  damageDebuff: {
    summaryTokens: ['dmg', 'damage', 'dam'],
    label: '-Damage',
    category: 'debuff',
    colorClass: STAT_COLORS.damageDebuff,
    format: 'percent',
    calculation: 'debuff',
    enhancementAspect: 'damageDebuff',
    priority: 4,
  },
  regenDebuff: {
    summaryTokens: ['regen', 'regeneration'],
    label: '-Regen',
    category: 'debuff',
    colorClass: STAT_COLORS.regenDebuff,
    format: 'percent',
    baseMultiplier: 100,
    priority: 5,
  },
  recoveryDebuff: {
    summaryTokens: ['recovery'],
    label: '-Recovery',
    category: 'debuff',
    colorClass: STAT_COLORS.recoveryDebuff,
    format: 'percent',
    baseMultiplier: 100,
    priority: 6,
  },
  rechargeDebuff: {
    summaryTokens: ['recharge', 'rech'],
    label: '-Recharge',
    category: 'debuff',
    colorClass: STAT_COLORS.rechargeDebuff,
    format: 'percent',
    calculation: 'debuff',
    priority: 7,
  },
  slow: {
    summaryTokens: ['spd', 'speed', 'slow'],
    label: '-Speed',
    category: 'debuff',
    colorClass: STAT_COLORS.slow,
    format: 'percent',
    canBeByType: true,
    // Movement slow IS enhanced by Slow enhancements (the calc's `slow` aspect,
    // Schedule A). Note: -Recharge (rechargeDebuff) deliberately has NO
    // enhancementAspect — its templates carry IgnoreStrength in the binary, so
    // Slow enhancements do not boost it (verified vs CoD2: Glue Arrow's
    // RechargeTime debuff has IgnoreStrength while its RunningSpeed does not).
    enhancementAspect: 'slow',
    priority: 8,
  },
  movementCapDebuff: {
    summaryTokens: ['spd', 'speed'],
    // The other half of a slow: `slow` lowers how fast you move, this lowers how fast you are
    // ALLOWED to move. Same attrib, different face of it (aspect Max vs Cur), so a power can
    // carry both and they must not collapse into one row.
    label: '-Speed Cap',
    category: 'debuff',
    colorClass: STAT_COLORS.slow,
    format: 'percent',
    canBeByType: true,
    enhancementAspect: 'slow',
    priority: 8,
  },
  enduranceDrain: {
    summaryTokens: ['end', 'endurance'],
    label: '-End Drain',
    category: 'debuff',
    colorClass: STAT_COLORS.enduranceDrain,
    format: 'percent',
    priority: 9,
  },
  enduranceCrash: {
    summaryTokens: ['end', 'endurance'],
    label: '-End (Crash)',
    category: 'debuff',
    colorClass: STAT_COLORS.enduranceDrain,
    format: 'value',
    priority: 9,
  },
  threatDebuff: {
    summaryTokens: ['threat'],
    label: '-Threat',
    category: 'debuff',
    colorClass: STAT_COLORS.threatDebuff,
    format: 'percent',
    priority: 10,
  },
  perceptionDebuff: {
    summaryTokens: ['perception'],
    label: '-Perception',
    category: 'debuff',
    colorClass: STAT_COLORS.perceptionDebuff,
    format: 'percent',
    priority: 11,
  },
  specialDebuff: {
    summaryTokens: ['special'],
    label: '-Special',
    category: 'debuff',
    colorClass: STAT_COLORS.tohitDebuff,
    format: 'percent',
    expandByType: true,
    priority: 12,
  },

  // === BUFFS ===
  tohitBuff: {
    summaryTokens: ['to hit', 'tohit', 'to-hit'],
    label: '+ToHit',
    category: 'buff',
    colorClass: STAT_COLORS.tohit,
    format: 'percent',
    calculation: 'buff',
    enhancementAspect: 'tohit',
    priority: 1,
  },
  accuracyBuff: {
    summaryTokens: ['acc', 'accuracy'],
    label: '+Accuracy',
    category: 'buff',
    colorClass: STAT_COLORS.accuracy,
    format: 'percent',
    calculation: 'buff',
    // No enhancementAspect: accuracy enhancements boost attack-roll accuracy,
    // not a buff power's own +Accuracy (Focused Accuracy can't slot it).
    priority: 1,
  },
  damageBuff: {
    summaryTokens: ['dmg', 'damage', 'dam'],
    label: '+Damage',
    category: 'buff',
    colorClass: STAT_COLORS.damage,
    format: 'percent',
    calculation: 'buff',
    // No enhancementAspect: a +Damage BUFF (Assault, Build Up, Fulcrum Shift…)
    // is a fixed buff to the target's Damage strength. It is NOT scaled by the
    // caster's Damage enhancements or global +Damage bonuses — those raise the
    // damage OUTPUT of attack powers, not the magnitude of a buff. There is no
    // "Damage Buff" enhancement in CoH, so the value is flat across all tiers.
    // (Same reasoning as accuracyBuff above.)
    priority: 2,
  },
  defenseBuff: {
    summaryTokens: ['def', 'defense'],
    label: '+Defense',
    category: 'buff',
    colorClass: STAT_COLORS.defense,
    format: 'percent',
    enhancementAspect: 'defense',
    canBeByType: true,
    expandByType: true,
    priority: 3,
  },
  defenseBuffSuppressible: {
    summaryTokens: ['def', 'defense'],
    label: '+Defense (Suppressible)',
    category: 'buff',
    colorClass: STAT_COLORS.defense,
    format: 'percent',
    enhancementAspect: 'defense',
    canBeByType: true,
    expandByType: true,
    priority: 3,
  },
  rechargeBuff: {
    summaryTokens: ['recharge', 'rech'],
    label: '+Recharge',
    category: 'buff',
    colorClass: STAT_COLORS.rechargeBuff,
    format: 'percent',
    calculation: 'buff',
    priority: 4,
  },
  recoveryBuff: {
    summaryTokens: ['recovery'],
    label: '+Recovery',
    category: 'buff',
    colorClass: STAT_COLORS.recoveryBuff,
    format: 'percent',
    baseMultiplier: 100,
    enhancementAspect: 'enduranceMod',
    priority: 5,
  },
  regenBuff: {
    summaryTokens: ['regen', 'regeneration'],
    label: '+Regen',
    category: 'buff',
    colorClass: STAT_COLORS.regen,
    format: 'percent',
    baseMultiplier: 100,
    enhancementAspect: 'heal',
    priority: 6,
  },
  speedBuff: {
    summaryTokens: ['spd', 'speed', 'run speed', 'movement'],
    label: '+Speed',
    category: 'buff',
    colorClass: STAT_COLORS.speed,
    format: 'percent',
    calculation: 'buff',
    priority: 7,
  },
  maxHPBuff: {
    summaryTokens: ['max hp', 'hp', 'health'],
    label: '+Max HP',
    category: 'buff',
    colorClass: STAT_COLORS.maxHP,
    format: 'percent',
    calculation: 'buff',
    enhancementAspect: 'heal',
    // 10% per scale point. Verified against in-game canonical +HP powers
    // (Tanker HPT scale=2 → +20%, Brute Dull Pain scale=2 → +20%,
    // Earth's Embrace scale=4 → +40%). The bin reference table
    // `Melee_HealSelf` is engine-internal bookkeeping (literally
    // baseMaxHP/10) and isn't used for the percentage display.
    flatPercentPerScale: 10,
    priority: 8,
  },
  maxEndBuff: {
    summaryTokens: ['max end', 'end'],
    label: '+Max End',
    category: 'buff',
    colorClass: STAT_COLORS.maxEnd,
    format: 'percent',
    calculation: 'buff',
    priority: 9,
  },
  rangeBuff: {
    summaryTokens: ['range'],
    label: '+Range',
    category: 'buff',
    colorClass: STAT_COLORS.rangeBuff,
    format: 'percent',
    calculation: 'buff',
    priority: 10,
  },
  enduranceDiscount: {
    summaryTokens: ['end', 'endurance', 'end cost'],
    label: '-End Cost',
    category: 'buff',
    colorClass: STAT_COLORS.enduranceDiscount,
    format: 'percent',
    calculation: 'buff',
    priority: 11,
  },
  enduranceGain: {
    summaryTokens: ['end', 'endurance'],
    label: '+End Gain',
    category: 'buff',
    colorClass: STAT_COLORS.enduranceGain,
    format: 'percent',
    priority: 12,
  },
  threatBuff: {
    summaryTokens: ['threat', 'taunt'],
    label: '+Threat',
    category: 'buff',
    colorClass: STAT_COLORS.threat,
    format: 'percent',
    priority: 13,
  },
  perceptionBuff: {
    summaryTokens: ['perception'],
    label: '+Perception',
    category: 'buff',
    colorClass: STAT_COLORS.perception,
    format: 'percent',
    priority: 14,
  },
  absorb: {
    summaryTokens: ['absorb'],
    label: 'Absorb',
    category: 'buff',
    colorClass: STAT_COLORS.absorb,
    format: 'value',
    enhancementAspect: 'heal',
    strengthAspect: 'absorb',
    priority: 15,
    valueFromTable: true,
    maxHpFractionPercentForm: true,
  },
  specialBuff: {
    summaryTokens: ['special'],
    label: '+Special',
    category: 'buff',
    colorClass: STAT_COLORS.tohit,
    format: 'percent',
    expandByType: true,
    priority: 20,
  },

  // === MOVEMENT (all teal) ===
  // Movement speed/height buffs are percentages (scale × the AT movement table,
  // e.g. Super Speed's 1.0 × Melee_SpeedRunning ≈ +350% @50), so they render as
  // `percent` and route through getEffectBaseValue's AT-table branch — the same
  // resolution the dashboard does via resolveMovementPercent. `format: 'value'`
  // here used to surface the raw scale (a bare "1.0") which read as a modifier,
  // not a speed. Teleport/untouchable below stay non-percent (distance/duration).
  fly: {
    summaryTokens: ['fly', 'flight'],
    label: 'Fly',
    category: 'movement',
    colorClass: STAT_COLORS.fly,
    format: 'percent',
    enhancementAspect: 'fly',
    priority: 1,
  },
  flySpeed: {
    summaryTokens: ['fly', 'flight', 'fly speed'],
    label: 'Fly Speed',
    category: 'movement',
    colorClass: STAT_COLORS.flySpeed,
    format: 'percent',
    enhancementAspect: 'fly',
    priority: 1,
  },
  runSpeed: {
    summaryTokens: ['spd', 'speed', 'run speed'],
    label: 'Run Speed',
    category: 'movement',
    colorClass: STAT_COLORS.runSpeed,
    format: 'percent',
    // The Run Speed enhancement category (SO and IO) normalizes to 'run'
    // in ASPECT_NAME_MAP. Use the same key here so slotted enhancements
    // are reflected in the Power Effects three-tier display.
    enhancementAspect: 'run',
    priority: 2,
  },
  jumpSpeed: {
    summaryTokens: ['jump', 'jump speed'],
    label: 'Jump Speed',
    category: 'movement',
    colorClass: STAT_COLORS.jumpSpeed,
    format: 'percent',
    // The Jumping enhancement boosts both jump speed and jump height in-game.
    enhancementAspect: 'jump',
    priority: 3,
  },
  jumpHeight: {
    summaryTokens: ['jump', 'jump height'],
    label: 'Jump Height',
    category: 'movement',
    colorClass: STAT_COLORS.jumpHeight,
    format: 'percent',
    enhancementAspect: 'jump',
    priority: 4,
  },
  teleport: {
    summaryTokens: ['teleport'],
    label: 'Teleport',
    category: 'movement',
    colorClass: STAT_COLORS.teleport,
    format: 'value',
    priority: 5,
  },
  untouchable: {
    summaryTokens: ['intangible', 'untouchable', 'phase'],
    label: 'Intangible',
    category: 'movement',
    colorClass: STAT_COLORS.untouchable,
    format: 'duration',
    priority: 6,
  },

  // === SPECIAL ===
  summon: {
    summaryTokens: ['summon', 'pet', 'pets'],
    label: 'Summon',
    category: 'special',
    colorClass: STAT_COLORS.summon,
    format: 'custom',
    priority: 1,
  },
  healing: {
    summaryTokens: ['heal', 'healing', 'heal over time', 'healing over time', 'hp'],
    label: 'Heal',
    category: 'buff',
    colorClass: STAT_COLORS.healing,
    format: 'value',
    enhancementAspect: 'heal',
    priority: 0.5,
    valueFromTable: true,
  },

  // === ARMOR & PROTECTION ===
  defense: {
    summaryTokens: ['def', 'defense'],
    label: 'Def',
    category: 'protection',
    colorClass: STAT_COLORS.defense,
    format: 'percent',
    enhancementAspect: 'defense',
    expandByType: true,
    priority: 1,
  },
  resistance: {
    summaryTokens: ['res', 'resist', 'resistance'],
    label: 'Res',
    category: 'protection',
    colorClass: STAT_COLORS.resistance,
    format: 'percent',
    enhancementAspect: 'resistance',
    expandByType: true,
    priority: 2,
  },
  elusivity: {
    summaryTokens: ['ddr', 'elusivity'],
    label: 'DDR',
    category: 'protection',
    colorClass: STAT_COLORS.elusivity,
    format: 'percent',
    expandByType: true,
    priority: 3,
    scalarFromTablePercent: true,
  },
  protection: {
    summaryTokens: ['prot', 'protection', 'status'],
    label: 'Prot',
    category: 'protection',
    colorClass: STAT_COLORS.protection,
    format: 'mag',
    expandByType: true,
    priority: 4,
  },
  debuffResistance: {
    summaryTokens: ['debuff res'],
    label: 'Debuff Res',
    category: 'protection',
    colorClass: STAT_COLORS.debuffResistance,
    format: 'percent',
    canBeByType: true,
    expandByType: true,
    priority: 5,
  },
  mezResistance: {
    summaryTokens: ['status', 'status res', 'mez'],
    label: 'Status Res',
    category: 'protection',
    colorClass: STAT_COLORS.debuffResistance,
    format: 'percent',
    canBeByType: true,
    expandByType: true,
    priority: 6,
  },
};

// ============================================
// CATEGORY DISPLAY CONFIG
// ============================================

export interface CategoryDisplayConfig {
  label: string;
  colorClass: string;
  priority: number;
}

export const CATEGORY_CONFIG: Record<EffectCategory, CategoryDisplayConfig> = {
  execution: { label: 'Power Stats', colorClass: 'text-slate-400', priority: 0 },
  damage: { label: 'Damage', colorClass: 'text-red-500', priority: 1 },
  control: { label: 'Control', colorClass: 'text-purple-500', priority: 2 },
  debuff: { label: 'Debuffs', colorClass: 'text-yellow-500', priority: 3 },
  buff: { label: 'Buffs', colorClass: 'text-green-500', priority: 4 },
  protection: { label: 'Protection', colorClass: 'text-orange-500', priority: 5 },
  movement: { label: 'Movement', colorClass: 'text-cyan-500', priority: 6 },
  special: { label: 'Special', colorClass: 'text-amber-500', priority: 7 },
};

// ============================================
// HELPER FUNCTIONS
// ============================================

/**
 * Check if a value is a "by type" object (e.g., defense by damage type)
 */
export function isByTypeObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null) return false;
  const keys = Object.keys(value);
  const typeKeys = [
    'smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'psionic', 'toxic',
    'melee', 'ranged', 'aoe', 'all', 'run', 'fly', 'jump',
    // Movement speed keys (used by slow effects)
    'runspeed', 'flyspeed', 'jumpspeed', 'jumpheight',
    // Debuff resistance stat types
    'defense', 'endurance', 'tohit', 'movement', 'regeneration', 'recovery', 'recharge', 'range', 'perception',
    // Resistance subtypes
    'heal',
    // Mez types (for specialDebuff/specialBuff)
    'hold', 'stun', 'immobilize', 'sleep', 'confuse', 'fear', 'knockback', 'knockup', 'repel',
  ];
  return keys.some(k => typeKeys.includes(k.toLowerCase()));
}

/**
 * Check if a value is a MezEffect object
 */
export function isMezEffect(value: unknown): value is MezEffect {
  if (typeof value !== 'object' || value === null) return false;
  const obj = value as Record<string, unknown>;
  return 'mag' in obj && typeof obj.mag === 'number';
}

/**
 * Format a mez effect for display
 */
export function formatMezValue(value: NumberOrMez): string {
  if (typeof value === 'number') {
    return `Mag ${value}`;
  }
  if (isMezEffect(value)) {
    const mag = value.mag;
    // Duration is never negative — the sign on `scale` is the protection spelling (MEZFACE-1).
    const duration = value.scale ? `${Math.abs(value.scale).toFixed(1)}s` : '';
    return duration ? `Mag ${mag} (${duration})` : `Mag ${mag}`;
  }
  return String(value);
}

/**
 * Calculate effect value based on config
 */
export function calculateEffectValue(
  value: NumberOrScaled,
  config: EffectDisplayConfig,
  archetypeId?: ArchetypeId
): number {
  if (config.calculation === 'buff' || config.calculation === 'debuff') {
    // Use AT table directly when available (accurate per-AT values)
    if (archetypeId && typeof value === 'object' && value !== null && 'table' in value && 'scale' in value) {
      const tableVal = getTableValue(archetypeId, (value as { scale: number; table: string }).table, 50);
      if (tableVal !== undefined) {
        return Math.abs((value as { scale: number; table: string }).scale * tableVal);
      }
    }
    // Fallback to legacy formula for plain number scales
    return calculateBuffDebuffPercent(value, archetypeId, config.calculation);
  }
  return getScaleValue(value) ?? 0;
}

/**
 * Format effect value for display
 */
export function formatEffectValue(
  value: number,
  format: EffectFormat,
  precision?: number
): string {
  const dp = precision ?? DEFAULT_EFFECT_PRECISION[format];
  switch (format) {
    case 'percent':
      return `${formatPrecision(value, dp)}%`;
    case 'duration':
      return `${formatPrecision(value, dp)}s`;
    case 'mag':
      return `Mag ${value}`;
    case 'scale':
      return `${formatPrecision(value, dp)} scale`;
    case 'degrees':
      return `${Math.round(value)}°`;
    // Carried from the contract at PR8: a distance is a UNIT the registry declares, not a
    // label the renderer recognises. The canonical side used to append feet on
    // `label === 'Range' || 'Radius'` and no key is labelled `Range` — `range` is `Pwr Range`
    // — so a power's range read as a bare number beside a radius reading `20ft`.
    case 'distance':
      return `${Math.round(value)}ft`;
    case 'value':
    default:
      return formatPrecision(value, dp);
  }
}

/**
 * Get the first value from a by-type object
 */
export function getByTypeFirstValue(obj: Record<string, unknown>): NumberOrScaled | undefined {
  const values = Object.values(obj);
  if (values.length === 0) return undefined;
  const first = values[0];
  if (typeof first === 'number') return first;
  // `scaleTerms` counts as well as `scale`: a pet's `slow` holds one value per movement
  // axis and an axis can state two rows off two tables, so the entry inside a by-type
  // object is exactly the shape a top-level value can be (ENT-8). Rejecting it collapsed
  // the whole key to nothing and the row vanished.
  if (typeof first === 'object' && first !== null && ('scale' in first || 'scaleTerms' in first)) {
    return first as NumberOrScaled;
  }
  return undefined;
}

/**
 * Get type abbreviations from a by-type object
 */
export function getByTypeAbbreviations(obj: Record<string, unknown>): string {
  const typeAbbrev: Record<string, string> = {
    smashing: 'S', lethal: 'L', fire: 'F', cold: 'C',
    energy: 'E', negative: 'N', psionic: 'P', toxic: 'T',
    melee: 'Mel', ranged: 'Rng', aoe: 'AoE',
    run: 'Run', fly: 'Fly', jump: 'Jmp',
    runspeed: 'Run', flyspeed: 'Fly', jumpspeed: 'Jmp', jumpheight: 'JmpH',
  };
  const allDamageTypes = ['smashing', 'lethal', 'fire', 'cold', 'energy', 'negative', 'psionic', 'toxic'];
  const keys = Object.keys(obj).map(k => k.toLowerCase());
  if (allDamageTypes.every(t => keys.includes(t))) return 'All';
  return keys
    .map(k => typeAbbrev[k] || k.charAt(0).toUpperCase())
    .join('');
}

// ============================================
// EFFECT GROUPING
// ============================================

export interface GroupedEffect {
  key: string;
  /** The registry key this entry resolved through — `key` itself, or the base key
   *  of a `<base>Unenhanced` split slot. */
  effectKey: string;
  value: unknown;
  config: EffectDisplayConfig;
  /** True when `key` is a `<base>Unenhanced` split slot. Such a row is flat: the
   *  slot NAME is the IgnoreStrength mark. */
  fromSplitSlot?: boolean;
}

export interface GroupedEffects {
  category: EffectCategory;
  categoryConfig: CategoryDisplayConfig;
  effects: GroupedEffect[];
}

/**
 * Group effects by category for organized display
 */
export function groupEffectsByCategory(
  effects: Record<string, unknown>
): GroupedEffects[] {
  const groups: Map<EffectCategory, GroupedEffect[]> = new Map();

  for (const [key, value] of Object.entries(effects)) {
    // Skip null/undefined values
    if (value == null) continue;

    // Skip non-effect properties (stats, flags, etc.) — but a `<base>Unenhanced`
    // slot IS an effect: it is the converter's IgnoreStrength verdict expressed as
    // a key, so it resolves through its base key's registration and renders flat.
    // A power carrying both halves shows both rows, the way the game's own monitor
    // does (ENT-6). Mirrors `granted.rs::resolve_granted_magnitudes`.
    let effectKey = key;
    let fromSplitSlot = false;
    let config = EFFECT_REGISTRY[key];
    if (!config && key.endsWith('Unenhanced')) {
      effectKey = key.slice(0, -'Unenhanced'.length);
      config = EFFECT_REGISTRY[effectKey];
      fromSplitSlot = config !== undefined;
    }
    if (!config) continue;

    const category = config.category;
    if (!groups.has(category)) {
      groups.set(category, []);
    }
    groups.get(category)!.push({ key, effectKey, value, config, fromSplitSlot });
  }

  // Sort effects within each group by priority
  for (const effectList of groups.values()) {
    effectList.sort((a, b) => (a.config.priority ?? 99) - (b.config.priority ?? 99));
  }

  // Convert to array and sort by category priority
  const result: GroupedEffects[] = [];
  for (const [category, effects] of groups) {
    result.push({
      category,
      categoryConfig: CATEGORY_CONFIG[category],
      effects,
    });
  }
  result.sort((a, b) => a.categoryConfig.priority - b.categoryConfig.priority);

  return result;
}

/**
 * Get all effects that have registry entries (for validation/debugging)
 */
export function getRegisteredEffectKeys(): string[] {
  return Object.keys(EFFECT_REGISTRY);
}

/**
 * Check if an effect key is registered
 */
export function isRegisteredEffect(key: string): boolean {
  return key in EFFECT_REGISTRY;
}
