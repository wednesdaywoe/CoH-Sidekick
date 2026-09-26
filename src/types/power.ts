/**
 * Power and Powerset type definitions
 */

import type {
  DamageType,
  PowerType,
  TargetType,
  EffectArea,
  EnhancementStatType,
  IOSetCategory,
} from './common';
// Plan B, Phase 0: the pre-projection atom list carried on `Power.atoms`.
// `atomic-effect.ts` has no imports, so this type-only reference is cycle-free.
import type { AttribType, EncodedAtom } from '@/data/core/atomic-effect';

// ============================================
// SCALED EFFECT (new format with AT tables)
// ============================================

/** Effect with scale and table reference for AT-based calculations */
export interface ScaledEffect {
  /** Scale multiplier */
  scale: number;
  /** AT table name (e.g., "Ranged_Debuff_ToHit") */
  table: string;
  /** The pet character class this value's table resolves against, written by
   *  `synthesizePseudoPetEffects` on every row a summoned entity supplies.
   *
   *  A value without it resolves against the build's archetype, which is what the power's
   *  own rows do. A pseudo-pet's rows are a SECOND character's: the client resolves them
   *  against `pDef->characterClassName` and passes the summoner alongside only for the
   *  `Requires` evaluation (`uiPowerInfo.c` `power_AddPetEffects`). The mark travels ON the
   *  value rather than being decided by the reader, because one bag holds both kinds — and
   *  after the ENT-8 split one KEY can hold rows from two pets of different classes
   *  (ENT-10). */
  petClass?: string;
  /** The template's IgnoreStrength flag, carried on the VALUE rather than only in a
   *  `<key>Unenhanced` slot name (ENT-4). A marked value takes no enhancement and no
   *  global: it renders flat and is tagged in the Info panel. */
  ignoreStrength?: boolean;
  /** Per-stack scale increment for stacking buffs (per-target AoE or damage-triggered).
   *  At N stacks: effective_scale = scale + perTarget × (N - 1)
   *  For AoE powers, N = targets hit. For non-AoE, N = stack count (see maxStacks). */
  perTarget?: number;
  /** HC splits some debuffs into two equal halves — one the target's debuff
   *  resistance can reduce, and one that bypasses it (IgnoreResistance). When
   *  true, this slot holds one half and an equal unresistable half also applies;
   *  the InfoPanel renders a second "(unresistable)" row (Flash Arrow -ToHit,
   *  Poison Gas -DMG). Set by the converter's twin pre-scan. */
  unresistable?: boolean;
  /** Additional instances of THIS debuff that the power applies with a different
   *  duration. CoH sometimes stacks the same debuff twice at distinct durations
   *  (EMP Arrow -500% regen at 15s AND 45s; Thunderous Blast -100% recovery at
   *  10s AND 20s) — genuinely separate applications that expire at different
   *  times, so they must not collapse into one summed value. The primary slot
   *  holds the longest-lived instance; each variant is rendered as its own row
   *  with its own duration. Set by the converter's duration-aware accumulate. */
  durationVariants?: { scale: number; duration: number }[];
  /** Per-effect projection of the DSH4 `eToWho` field: `'Self'` marks a debuff
   *  value that actually lands on the CASTER (Granite Armor's -damage/-recharge/
   *  -speed, Rage's crash, Reaction Time's self-slow) and so counts against the
   *  caster's own totals. Absence means the value is a foe/display-only debuff.
   *  This replaces the retired bag-level `selfPenalty` boolean — the converter
   *  classifies each template's target individually, so a foe slow co-located
   *  with a self slow (Rebirth Granite's foe -JumpHeight) no longer rides a
   *  bag-wide flag onto the caster. Read via `isSelfDirectedEffect`. */
  toWho?: 'Self';
  /** Binary suppress group (StackType kSuppress + StackByAttribAndKey): active
   *  powers sharing a key mutually suppress per stat — only the strongest
   *  applies. 'TravelBuff' covers the travel-power movement buffs (Combat
   *  Jumping, Super Jump, Super Speed's momentum, Fly, Ninja Run, …), which
   *  the calc resolves via strongest-wins grouping (resolveMovementTotals).
   *  Absent = stacks additively. */
  stackKey?: string;
  /** True when the game suppresses this buff in combat (the AttribMod carries
   *  `Suppress ActivateAttackClick/Attacked/Damaged…`) — Super Speed's run
   *  buff, Super Jump's jump buffs, Fly's speed. The In-Combat toggle drops
   *  these from totals. Combat Jumping / Hover have no suppress events. */
  suppressible?: boolean;
}

/** Helper type for effects that can be number OR scaled */
export type NumberOrScaled = number | ScaledEffect;

/** Absorb magnitude that scales off the caster's CURRENT Max HP rather than a
 *  flat AT-table value (Wild Bastion, Ablative Carapace, Force Barrier). The
 *  converter recovers `maxHPFraction` from the Expression magnitude
 *  (`Max.kHitPoints source> C *`), e.g. Wild Bastion's 0.25 = 25% of Max HP —
 *  which is why the shield grows with +HP accolades. `appliesStrength` (default
 *  true) marks it as scaled by +Absorb strength (Power Boost / Clarion) and
 *  slotted Heal enhancement. A flat absorb still uses {@link ScaledEffect}. */
export interface MaxHPFractionAbsorb {
  maxHPFraction: number;
  appliesStrength?: boolean;
  table?: string;
  /** Per-foe growth for an absorb inside a foe-targeted AoE (Parasitic Aura:
   *  +10% Max HP per foe, up to 10). The companion to {@link ScaledEffect}'s
   *  `perTarget`, which this form has no scale to carry: the magnitude is an
   *  Expression the converter recovers as a fraction, and each foe hit
   *  re-applies that same Expression. */
  maxHPFractionPerTarget?: number;
}

/**
 * Extract scale value from NumberOrScaled
 * Returns the number directly, or the scale from ScaledEffect
 */
export function getScaleValue(value: NumberOrScaled | undefined): number | undefined {
  if (value === undefined) return undefined;
  if (typeof value === 'number') return value;
  return value.scale;
}

/**
 * Check if a value is a ScaledEffect (has scale and table)
 */
export function isScaledEffect(value: unknown): value is ScaledEffect {
  return (
    typeof value === 'object' &&
    value !== null &&
    'scale' in value &&
    'table' in value
  );
}

/**
 * A debuff value that lands on the caster (DSH4 `eToWho === 'Self'`). Only the
 * object-shaped ScaledEffect form carries the marker — bare-number debuff slots
 * are foe/display-only by construction (the converter's self-penalty branches
 * always emit an object). Replaces the old bag-level `selfPenalty` gate.
 */
export function isSelfDirectedEffect(value: unknown): boolean {
  return isScaledEffect(value) && value.toWho === 'Self';
}

/**
 * True when a power carries ANY self-directed penalty — a debuff value the
 * caster actually suffers. Scans exactly the slots the converter's self-penalty
 * branches tag (damage/recharge/tohit/accuracy/range debuffs + the per-type
 * `slow` map). This is the power-level projection of the retired `selfPenalty`
 * boolean, now derived from per-effect `toWho` rather than a bag-wide flag.
 */
export function hasSelfDirectedPenalty(effects: PowerEffects | undefined): boolean {
  if (!effects) return false;
  if (
    isSelfDirectedEffect(effects.damageDebuff) ||
    isSelfDirectedEffect(effects.rechargeDebuff) ||
    isSelfDirectedEffect(effects.tohitDebuff) ||
    isSelfDirectedEffect(effects.accuracyDebuff)
  ) {
    return true;
  }
  const slow = effects.slow;
  if (slow && typeof slow === 'object') {
    for (const val of Object.values(slow)) {
      if (isSelfDirectedEffect(val)) return true;
    }
  }
  return false;
}

// ============================================
// DAMAGE EFFECT
// ============================================

export interface DamageEffect {
  /** Damage type (Fire, Smashing, etc.) */
  type: DamageType;
  /** Damage scale value */
  scale: number;
  /** Optional: damage table reference */
  table?: string;
  /** DoT total duration in seconds (present on periodic damage entries). */
  duration?: number;
  /** DoT tick period in seconds (present on periodic damage entries). */
  tickRate?: number;
  /** Per-tick apply chance (< 1) when the DoT is chance-gated. */
  chance?: number;
  /** Whether a missed tick cancels the remaining DoT chain (geometric decay). */
  cancelOnMiss?: boolean;
}

export interface MultiDamageEffect {
  /** Multiple damage types */
  types: DamageEffect[];
  /** Combined scale */
  scale: number;
}

// ============================================
// DOT (DAMAGE OVER TIME) EFFECT
// ============================================

export interface DotEffect {
  /** Damage type */
  type: DamageType;
  /** Damage scale per tick */
  scale: number;
  /** Number of ticks */
  ticks: number;
}

// ============================================
// MEZ EFFECT (stun, hold, sleep, etc.)
// ============================================

/** Mez effect with magnitude, duration scale, and table */
export interface MezEffect {
  /** Mez magnitude (determines what rank of enemies are affected) */
  mag: number;
  /** Duration scale, SIGNED (MEZFACE-1): the sign is the protection spelling, so a reader
   *  needs no table-name sniff to tell an armor's status protection from applied control. */
  scale: number;
  /** AT table for duration calculation */
  table: string;
  /**
   * Which of the mod's two numbers `scale × table` computes — the atom's own
   * {@link AttribType}, carried here because a named bag slot has nowhere else to put it.
   *
   * `'Duration'`: the product is SECONDS and {@link mag} is the mez rank the effect grabs.
   * `'Magnitude'`: the product is the MAGNITUDE, the duration is the template's own
   * `durations` entry, and {@link mag} is the def compiler's unscaled placeholder rather
   * than a value. `'Expression'`: the magnitude comes from a stack-machine program, so
   * neither product is the answer.
   *
   * Absent on a bundle minted before MEZDUR-1; a reader must say so rather than pick one.
   */
  attribType?: AttribType;
  /** See {@link ScaledEffect.ignoreStrength} — a mez whose duration takes no enhancement. */
  ignoreStrength?: boolean;
  /** `'Self'` when the atom is directed at the caster — the self-root (Hibernate, Icy
   *  Bastion) that holds the caster still, not applied control (MEZFACE-1). Absent means
   *  the atom is not caster-directed. */
  toWho?: string;
}

/** Helper type for mez that can be number (magnitude only) OR full MezEffect */
export type NumberOrMez = number | MezEffect;

/**
 * Check if a mez value is a full MezEffect (has mag, scale, table)
 */
export function isMezEffect(value: NumberOrMez | undefined): value is MezEffect {
  return (
    typeof value === 'object' &&
    value !== null &&
    'mag' in value &&
    'scale' in value &&
    'table' in value
  );
}

// ============================================
// PROTECTION EFFECTS (mez protection magnitude)
// ============================================

export interface ProtectionEffects {
  stun?: number;
  hold?: number;
  immobilize?: number;
  sleep?: number;
  confuse?: number;
  fear?: number;
  knockback?: number;
  knockup?: number;
  repel?: number;
}

// ============================================
// DEFENSE/RESISTANCE BY DAMAGE TYPE
// ============================================

/** Defense values can be number (legacy) or ScaledEffect (new format) */
export interface DefenseByType {
  smashing?: NumberOrScaled;
  lethal?: NumberOrScaled;
  fire?: NumberOrScaled;
  cold?: NumberOrScaled;
  energy?: NumberOrScaled;
  negative?: NumberOrScaled;
  psionic?: NumberOrScaled;
  toxic?: NumberOrScaled;
  melee?: NumberOrScaled;
  ranged?: NumberOrScaled;
  aoe?: NumberOrScaled;
}

/** Resistance values can be number (legacy) or ScaledEffect (new format) */
export interface ResistanceByType {
  smashing?: NumberOrScaled;
  lethal?: NumberOrScaled;
  fire?: NumberOrScaled;
  cold?: NumberOrScaled;
  energy?: NumberOrScaled;
  negative?: NumberOrScaled;
  psionic?: NumberOrScaled;
  toxic?: NumberOrScaled;
  /** The export's `Special` damage type (Thunderspy's generic damage atoms). */
  special?: NumberOrScaled;
  /** Heal resistance (affects incoming healing) */
  heal?: NumberOrScaled;
}

/** Elusivity (defense debuff resistance) by type */
export interface ElusivityByType {
  all?: NumberOrScaled;
  smashing?: NumberOrScaled;
  lethal?: NumberOrScaled;
  fire?: NumberOrScaled;
  cold?: NumberOrScaled;
  energy?: NumberOrScaled;
  negative?: NumberOrScaled;
  psionic?: NumberOrScaled;
  melee?: NumberOrScaled;
  ranged?: NumberOrScaled;
  aoe?: NumberOrScaled;
}

/** Movement effects (buffs or debuffs) */
export interface MovementByType {
  runSpeed?: NumberOrScaled;
  flySpeed?: NumberOrScaled;
  jumpHeight?: NumberOrScaled;
  jumpSpeed?: NumberOrScaled;
  fly?: NumberOrScaled;
  movementControl?: NumberOrScaled;
  movementFriction?: NumberOrScaled;
  // The paired IgnoreStrength halves (FLYPOOL-1): when an axis carries an
  // enhanceable row AND an IgnoreStrength twin, the twin lives under the
  // `<axis>Unenhanced` split-slot spelling instead of clobbering the axis.
  runSpeedUnenhanced?: NumberOrScaled;
  flySpeedUnenhanced?: NumberOrScaled;
  jumpHeightUnenhanced?: NumberOrScaled;
  jumpSpeedUnenhanced?: NumberOrScaled;
}

/** Stealth effects */
export interface StealthEffects {
  stealthPvE?: NumberOrScaled;
  stealthPvP?: NumberOrScaled;
  translucency?: NumberOrScaled;
  /** Binary stealth-stacking group (`stack_key`). Powers sharing a non-null
   *  key mutually suppress — only the largest StealthRadius in the group
   *  applies (e.g. "NictusFX": Stealth, Super Speed, Shinobi-Iri, the cloak
   *  toggles). Null/absent means the radius stacks additively. Consumed by
   *  resolveStealthRadius in character-totals. */
  stackKey?: string | null;
}

// ============================================
// SUMMON EFFECT (pets/pseudopets)
// ============================================

/** Summoned entity (pet or pseudopet) */
export interface SummonEffect {
  /** True if this is a pseudopet (invisible location-based effect) */
  isPseudoPet: boolean;
  /** Entity definition name (for real pets) - key into PET_ENTITIES */
  entity?: string;
  /** Display name of the summoned entity */
  displayName?: string;
  /** Powers the entity uses (where actual effects come from) */
  powers?: string[];
  /** Duration of the summon in seconds */
  duration?: number;
  /** Number of entities summoned (e.g., Gremlins = 2) */
  entityCount?: number;
  /** True if the summon template has CopyBoosts flag (pet inherits caster's enhancements) */
  copyBoosts?: boolean;
  /** Multi-entity summons (e.g., Mastermind henchmen with different entity types) */
  entities?: { entity: string; count: number }[];
  /**
   * The `entities` are mutually-exclusive variants — exactly ONE materializes,
   * not all of them. Soul Extraction summons a single Ghost whose tier
   * (Boss/Lt/Minion) matches the Undead henchman you sacrifice. Display as
   * "Summons 1 of: …" and never sum the variants' counts/damage.
   */
  mutuallyExclusive?: boolean;
  /**
   * Triggered pet entities gated behind a toggle — a SEPARATE PET_ENTITIES entity
   * that only applies when activated (Oil Slick Arrow's `Pets_OilSlickBurn` damage
   * patch, created when the oil is ignited by fire/energy). Off by default; the
   * runtime folds its (enhanceable) damage into the totals when the toggle is on.
   */
  conditionalEntities?: { entity: string; toggleId: string; label: string }[];
  /**
   * Pseudo-pets resolved from `powers` (redirect lists) at convert time, for
   * location pseudo-pets whose entity_def is a generic shell not backed by a
   * PET_ENTITIES record (Storm Cell, Category Five, Freezing Rain, …). Each
   * carries its own synthesized ability list (damage + debuffs). Damage scales
   * off the SUMMONER's archetype, not a fixed pet class.
   * See PSEUDO-PET-POWER-RESOLUTION.md.
   */
  resolvedEntities?: ResolvedPseudoPet[];
}

/** A debuff/mez on a synthesized pseudo-pet ability. */
export interface ResolvedPseudoPetEffect {
  type: string;
  scale?: number;
  table?: string;
  magnitude?: number;
  /** Mez rows only: which of the mod's two numbers `scale × table` computes, carried so the
   *  pseudo-pet merge hands the display reader the same discriminator a parent power's mez row
   *  carries. Read by the control merge; a knock row's quantity is a distance either way. See
   *  {@link MezEffect.attribType}. */
  attribType?: AttribType;
  /** Proc chance the binary gates this effect with (< 1), e.g. the 33% lightning stun. */
  chance?: number;
  /** IgnoreStrength: the player's enhancements/buffs do NOT scale this — show informational/unenhanced. */
  ignoreStrength?: boolean;
  /** Mez rows only: the row is directed at the CASTER — a self-root, not applied control.
   *  Rides the pseudo-pet merge the same way `attribType` does (MEZFACE-1). */
  toWho?: string;
  /** Mode-gated: only applies while the power is in its empowered/triggered state
   *  (Storm Cell's lightning effects — "while High Winds is active"). */
  conditional?: boolean;
  /** Which movement axis a `Slow` / `MovementCapDebuff` row names (`runSpeed`,
   *  `flySpeed`, `jumpSpeed`, `jumpHeight`). A pet row belongs in the same key as its
   *  parent-power twin, and the parent's rows are per axis (ENT-5), so the axis has to
   *  travel with the row rather than collapse into one ambiguous "Slow". */
  axis?: string;
  /** The damage types a `ResistanceBuff` row names, lower-cased. One modifier moves every
   *  type it names at one scale; a row naming none folds into no total at all (ENT-9). */
  resistanceTypes?: string[];
  /** The defense positions and damage types a `DefenseBuff` row names, lower-cased. Same
   *  shape and same reason as `resistanceTypes`: one modifier, every position it names. */
  defenseTypes?: string[];
  /** Which face of Absorb an `Absorb` row moves (`Maximum`, `Current`). Absorb is a pool
   *  with a cap, and a row raising the cap is not a row filling it (ENT-17). */
  absorbAspect?: string;
}

/** One redirect power resolved into a pseudo-pet ability (PetAbility-shaped). */
export interface ResolvedPseudoPetAbility {
  name: string;
  displayName: string;
  type: string;
  damage: { damageType: string; scale: number; table: string }[];
  /** Empowered ("High Winds") replacement for `effects` — the WindSpeed values
   *  (~2× the base Tempest debuffs). The runtime swaps to these when the
   *  "Storm Cell Active" toggle is on. */
  poweredUpEffects?: ResolvedPseudoPetEffect[];
  /** Empowered replacement for `damage` — the high-storm-strength "Strong Storm
   *  Cell Lightning" (StormCell_LightningAura, 1.0 ≈ 2× the base 0.5 aura). The
   *  runtime swaps to these when the "Storm Cell Active" toggle is on, so the
   *  lightning escalates from the base aura to the strong variant players see
   *  in-game once storm strength builds. */
  poweredUpDamage?: { damageType: string; scale: number; table: string }[];
  /** Damage lands at < 100% (storm-strength gated / proc) — kept OUT of the
   *  guaranteed headline DoT and surfaced as a conditional effect instead. */
  conditionalDamage?: boolean;
  /** Cumulative chance the conditional damage lands (0 = storm-strength gated). */
  damageChance?: number;
  effects?: ResolvedPseudoPetEffect[];
  recharge?: number;
  castTime?: number;
  activatePeriod?: number;
  effectArea?: string;
  radius?: number;
  maxTargets?: number;
  /** EntsAffected — who the ability's effects can land on, which is what an atom
   *  targeting `AnyAffected` means by "the target". */
  targetsAffected?: string[];
}

/** A pseudo-pet synthesized from one EntCreate's redirect list. */
export interface ResolvedPseudoPet {
  displayName: string;
  /** Pet lifespan in seconds (the DoT window). */
  duration?: number;
  /** Number of identical copies summoned. */
  count?: number;
  /** Inherits the summoner's enhancements/modifiers (damage off summoner AT). */
  copyCreatorMods: boolean;
  abilities: ResolvedPseudoPetAbility[];
}

// ============================================
// HEALING EFFECT
// ============================================

export interface HealingEffect {
  scale: number;
  table?: string;
  perTarget?: boolean;
}

// ============================================
// DEBUFF RESISTANCE
// ============================================

export interface DebuffResistance {
  defense?: NumberOrScaled;
  recharge?: NumberOrScaled;
  movement?: NumberOrScaled; // Also known as "Slow" resistance
  tohit?: NumberOrScaled;
  endurance?: NumberOrScaled; // Endurance drain resistance
  regeneration?: NumberOrScaled; // Regeneration debuff resistance
  recovery?: NumberOrScaled; // Recovery debuff resistance
  perception?: NumberOrScaled; // Perception debuff resistance
  range?: NumberOrScaled; // Range debuff resistance
  // Accuracy debuff resistance. Carried, not totalled — nothing accumulates it yet, the
  // same standing as `range` above. Brainstorm's Light Affinity (Lightfield, Spotlight) is
  // the first export to state one, so dropping the key would lose a real row rather than
  // decline an unused one.
  accuracy?: NumberOrScaled;
}

// ============================================
// MOVEMENT EFFECTS
// ============================================

export interface MovementEffect {
  scale: number;
  table?: string;
  /** Per-stack scale increment — see {@link ScaledEffect.perTarget}. */
  perTarget?: number;
  /** Binary suppress group — see {@link ScaledEffect.stackKey}. */
  stackKey?: string;
  /** Suppressed in combat — see {@link ScaledEffect.suppressible}. */
  suppressible?: boolean;
}

// ============================================
// POWER EFFECTS
// ============================================

export interface PowerEffects {
  /** Base accuracy modifier */
  accuracy?: number;
  /** Range in feet (0 for melee/self) */
  range?: number;
  /** Recharge time in seconds */
  recharge?: number;
  /** Endurance cost per tick (divide by activatePeriod for per-second) */
  enduranceCost?: number;
  /** Toggle tick interval in seconds (default 0.5). End/s = enduranceCost / activatePeriod */
  activatePeriod?: number;
  /** Cast/activation time in seconds */
  castTime?: number;
  /** Effect area type */
  effectArea?: EffectArea;
  /** Radius for AoE powers */
  radius?: number;
  /** Arc for cone powers */
  arc?: number;
  /** Max targets for AoE */
  maxTargets?: number;
  /** Direct damage (single or multi-type) */
  damage?: DamageEffect | MultiDamageEffect;
  /** Damage over time */
  dot?: DotEffect;
  /** Duration of buffs/debuffs */
  buffDuration?: number;
  /** Per-effect durations in seconds, keyed by effect name (e.g. { tohitDebuff: 6, rechargeBuff: 120 }) */
  durations?: Record<string, number>;

  // === BUFF EFFECTS ===
  /** ToHit buff value (scale or {scale, table}) */
  tohitBuff?: NumberOrScaled;
  /** ToHit buff value (unenhanceable — IgnoreStrength; not boosted by ToHit enh / global +ToHit) */
  tohitBuffUnenhanced?: NumberOrScaled;
  /** Accuracy self-buff value (scale or {scale, table}) — e.g. Focused Accuracy */
  accuracyBuff?: NumberOrScaled;
  /** Damage buff value (scale or {scale, table}) */
  damageBuff?: NumberOrScaled;
  /** Defense buff value - can be single value or by type */
  defenseBuff?: NumberOrScaled | DefenseByType;
  /** Defense buff suppressed in combat (stealth/travel powers) */
  defenseBuffSuppressible?: NumberOrScaled | DefenseByType;
  /** Recharge buff value (percentage as decimal, e.g., 0.30 = 30%) */
  rechargeBuff?: NumberOrScaled;
  /** Recovery buff value (percentage as decimal) */
  recoveryBuff?: NumberOrScaled;
  /** Recovery buff value (unenhanceable — IgnoreStrength; not boosted by End Mod / global +recovery) */
  recoveryBuffUnenhanced?: NumberOrScaled;
  /** Regeneration buff value */
  regenBuff?: NumberOrScaled;
  /** Regeneration buff value (unenhanceable — IgnoreStrength) */
  regenBuffUnenhanced?: NumberOrScaled;
  /** Run/Fly speed buff value (percentage as decimal) */
  speedBuff?: NumberOrScaled;
  /** Endurance buff value (flat value or scale) */
  enduranceBuff?: NumberOrScaled;
  /** Endurance gain (instant recovery) */
  enduranceGain?: NumberOrScaled;
  /** Max HP buff */
  maxHPBuff?: NumberOrScaled;
  /** Unenhanceable half of a +MaxHP twin (IgnoreStrength — "half of this max-HP
   *  increase is unenhanceable"). Co-applies with maxHPBuff but ignores +Healing
   *  strength; mirrors regenBuffUnenhanced / recoveryBuffUnenhanced. */
  maxHPBuffUnenhanced?: NumberOrScaled;
  /** Max Endurance buff */
  maxEndBuff?: NumberOrScaled;
  /** Range buff */
  rangeBuff?: NumberOrScaled;
  /** Endurance discount (reduced end cost) */
  enduranceDiscount?: NumberOrScaled;
  /** Threat level buff */
  threatBuff?: NumberOrScaled;
  /** Perception buff */
  perceptionBuff?: NumberOrScaled;
  /** Absorb shield — a flat {@link ScaledEffect} (Psychokinetic Barrier) or a
   *  {@link MaxHPFractionAbsorb} that scales off current Max HP (Wild Bastion). */
  absorb?: NumberOrScaled | MaxHPFractionAbsorb;

  // === STACKING ===
  /** Max stacks for non-AoE stacking powers (e.g., Reactive Regeneration = 20).
   *  For AoE per-target powers, use stats.maxTargets instead. */
  maxStacks?: number;
  /** Names of effect keys whose scale multiplies linearly with stack count
   *  (e.g. ['absorb', 'debuffResistance'] for Psychokinetic Barrier).
   *  Effects not listed here are treated as refresh-only — they re-apply
   *  with full duration but their magnitude does not stack. Only meaningful
   *  when `maxStacks` is set. */
  stacksLinear?: string[];
  /** Per-effect stack cap for powers whose `stacksLinear` effects have
   *  DIVERGENT limits, keyed by the same effectKey used in `stacksLinear`.
   *  Psychokinetic Barrier stacks its absorb to 2 but its debuff-resistance to
   *  3, so the slider ranges to `maxStacks` (3) while `stackCaps.absorb = 2`
   *  clamps absorb. A key absent here falls back to `maxStacks`. Only emitted
   *  for keys whose cap is strictly below `maxStacks`. */
  stackCaps?: Record<string, number>;
  /** Seconds between successive stack applications for "ramp" powers that
   *  apply 1 stack per tick within a single cast (e.g. Rebirth's Spirit
   *  Ward: 5×0.10 absorb stacks, one every 3s). Distinguishes this from
   *  recast-stacking (Crab Spider Serum etc.) where stacks come from
   *  recasting the power before previous stacks expire. The InfoPanel uses
   *  this to render "Stacks (every Xs)" on the slider label. */
  stackInterval?: number;

  // === SELF-PENALTY (per-effect `toWho`, see ScaledEffect.toWho) ===
  // The former bag-level `selfPenalty` boolean is retired: whether a debuff
  // lands on the caster is now carried per-value (`toWho:'Self'`), read via
  // `isSelfDirectedEffect` / `hasSelfDirectedPenalty`. This lets a foe debuff
  // co-located with a self debuff stay off the caster's totals.

  // === DEBUFF EFFECTS ===
  /** ToHit debuff value (scale or {scale, table}) */
  tohitDebuff?: NumberOrScaled;
  /** Accuracy debuff value — e.g. Geode's self -Accuracy while petrified */
  accuracyDebuff?: NumberOrScaled;
  /** Defense debuff value - can be single value or by type */
  defenseDebuff?: NumberOrScaled | DefenseByType;
  /** Resistance debuff value - can be single value or by type */
  resistanceDebuff?: NumberOrScaled | ResistanceByType;
  /** Damage debuff value (scale) - reduces enemy damage output */
  damageDebuff?: NumberOrScaled;
  /** Regeneration debuff value (scale) */
  regenDebuff?: NumberOrScaled;
  /** Recovery debuff value (scale) */
  recoveryDebuff?: NumberOrScaled;
  /** Endurance drain */
  enduranceDrain?: NumberOrScaled;
  /** Endurance crash (flat endurance point loss after delay) */
  enduranceCrash?: number;
  /** Threat level debuff */
  threatDebuff?: NumberOrScaled;
  /** Perception debuff */
  perceptionDebuff?: NumberOrScaled;
  /** Recharge debuff (slow recharge) */
  rechargeDebuff?: NumberOrScaled;
  /** Movement/speed debuff (slow) - can be single value or by type */
  slow?: NumberOrScaled | MovementByType;
  /** -Special: reduces target's secondary effect strength (mez, buffs, etc.) */
  specialDebuff?: Record<string, NumberOrScaled>;
  /** +Special: boosts own/ally secondary effect strength */
  specialBuff?: Record<string, NumberOrScaled>;

  /** Duration of effects in seconds (for debuffs, DoTs, etc.) */
  effectDuration?: number;

  // === DEFENSE & RESISTANCE (armor sets) ===
  /** Defense values by damage type */
  defense?: DefenseByType;
  /** Resistance values by damage type */
  resistance?: ResistanceByType;
  /** Elusivity (defense debuff resistance) */
  elusivity?: ElusivityByType;
  /** Movement buffs */
  movement?: MovementByType;
  /** Travel-power movement-speed CAP raises (aspect=Maximum templates):
   *  Super Speed +1.938 run-cap units (→120.25 mph), Super Jump +1.65 jump
   *  (→101.80), Fly +2.0475 fly (→87.95), Afterburner +1.0 on top (→102.27).
   *  Scale is in movement-scale units (Melee_Ones table): 1 unit = 21 fps =
   *  14.318 mph. Keyed like `movement` (runSpeed/jumpSpeed/flySpeed); each
   *  entry may carry `stackKey` (max per group, groups add) and
   *  `suppressible` (cap raise drops in combat — Super Jump's does, Super
   *  Speed's and Fly's persist). Consumed by getEffectiveMovementCaps. */
  movementCapBump?: MovementByType;
  /** The debuff direction of the same aspect=Maximum split: a movement CAP
   *  reduction, keyed like `movement`. Chilling Embrace states both faces of
   *  `runSpeed` from one template (0.7×Melee_Slow at Current, −1.0×
   *  Melee_SpeedRunning at Maximum), and while both wrote `slow` the later one
   *  replaced the earlier (ENT-5). Self-directed entries carry `toWho: 'Self'`
   *  and reach the movement globals the way `slow`'s do. */
  movementCapDebuff?: MovementByType;
  /** Stealth effects */
  stealth?: StealthEffects;
  /** Debuff resistance */
  debuffResistance?: DebuffResistance;
  /** Mez resistance (reduces mez duration) — per-type, e.g., { hold: { scale, table } } */
  mezResistance?: Record<string, NumberOrScaled>;

  // === HEALING ===
  /** Healing effect */
  healing?: HealingEffect;

  // === MEZ EFFECTS (control/stuns) ===
  // Can be number (magnitude only, legacy) or MezEffect (mag, scale, table)
  /** Stun effect */
  stun?: NumberOrMez;
  /** Hold effect */
  hold?: NumberOrMez;
  /** Immobilize effect */
  immobilize?: NumberOrMez;
  /** Sleep effect */
  sleep?: NumberOrMez;
  /** Fear effect */
  fear?: NumberOrMez;
  /** Confuse effect */
  confuse?: NumberOrMez;
  /** Knockback effect (scale/table, no mag) */
  knockback?: NumberOrScaled;
  /** Knockup effect (scale/table) */
  knockup?: NumberOrScaled;
  /** Repel effect (scale/table) */
  repel?: NumberOrScaled;
  /** Taunt effect */
  taunt?: NumberOrScaled;
  /** Placate effect */
  placate?: NumberOrScaled;
  /** Teleport effect */
  teleport?: NumberOrScaled;
  /** Fly (grants flight) */
  fly?: NumberOrScaled;
  /** Untouchable (intangible) */
  untouchable?: NumberOrScaled;
  /** Only affects self */
  onlyAffectsSelf?: NumberOrScaled;

  // === PROTECTION (mez protection for armors) ===
  /** Protection values granted */
  protection?: ProtectionEffects;

  // === SUMMON (pets/pseudopets) ===
  /** Summoned entity info */
  summon?: SummonEffect;

  // === LEGACY MOVEMENT (keep for backwards compatibility) ===
  /** @deprecated Use movement.runSpeed instead */
  runSpeed?: MovementEffect;
  /**
   * IgnoreStrength run-speed template — contributes to run-speed totals but is
   * NOT boosted by slotted Run Speed enhancements. Used where a power grants two
   * RunningSpeed templates, one enhanceable + one IgnoreStrength (e.g. Sprint and
   * the prestige sprints: +50% enhanceable + +50% unenhanceable). Totals-only,
   * mirroring tohitBuffUnenhanced / regenBuffUnenhanced.
   */
  runSpeedUnenhanced?: MovementEffect;
  /** @deprecated Use movement.jumpHeight instead */
  jumpHeight?: MovementEffect;
  /** @deprecated Use movement.jumpSpeed instead */
  jumpSpeed?: MovementEffect;
  /** @deprecated Use movement.flySpeed instead */
  flySpeed?: MovementEffect;
}

// ============================================
// POWER STATS (base power statistics)
// ============================================

export interface PowerStats {
  /** Base accuracy modifier */
  accuracy?: number;
  /** Range in feet (0 for melee/self) */
  range?: number;
  /** Radius for AoE powers */
  radius?: number;
  /** Recharge time in seconds */
  recharge?: number;
  /** Endurance cost (per tick — divide by activatePeriod for per-second) */
  endurance?: number;
  /** Toggle tick interval in seconds (default 0.5). End/s = endurance / activatePeriod */
  activatePeriod?: number;
  /** Cast/activation time in seconds. For snipes this is the Normal (not-in-
   *  combat) variant's full interruptible cast — the In-Combat fast form lives
   *  on `quickSnipe`. */
  castTime?: number;
  /** Interruptible channel time in seconds (snipes), already folded into
   *  `castTime`. Enhanceable by Interrupt Reduction; 0/absent otherwise. */
  interruptTime?: number;
  /** Animation root/lock duration in seconds (HC Parse7 field 48b) — the
   *  window during which the character is physically locked in place,
   *  distinct from `interruptTime`. For ordinary snipes this is roughly
   *  `castTime - interruptTime`; for Assassin's Strike from-Hide openers
   *  it's markedly shorter than that gap. Captured but not yet surfaced in
   *  the UI or consumed by attack-chain/DPS timing — Mids has no equivalent
   *  concept, and the queuing mechanic it would model needs in-game
   *  verification before it's exposed anywhere. Absent when equal to
   *  castTime (the common case) or unset (Rebirth/Thunderspy, whose Parse6
   *  layout omits the field). */
  timeToRoot?: number;
  /** Max targets for AoE */
  maxTargets?: number;
  /** Arc for cone powers */
  arc?: number;
}

// ============================================
// DAMAGE ARRAY (new format for AT tables)
// ============================================

export interface ScaledDamageEntry {
  type: string;
  scale: number;
  table: string;
  /** Duration for buff/effect-type damage entries */
  duration?: number;
  /** Tick rate for DoT entries */
  tickRate?: number;
  /** Per-tick apply chance (< 1) when the DoT is chance-gated. */
  chance?: number;
  /** Whether a missed tick cancels the remaining DoT chain (geometric decay). */
  cancelOnMiss?: boolean;
  /** For `type: 'Heal'` entries flagged IgnoreStrength — the heal is not boosted
   *  by Healing enhancement or global +Heal. */
  ignoreStrength?: boolean;
  /**
   * Bonus damage the archetype's hit-time mechanic (Containment, crit, Scourge,
   * Assassination, Opportunity) does NOT multiply. Set by the converter for
   * conditional groups the binary never duplicates onto an `*_InherentDamage`
   * table — currently Gravity Control's Impact. Still fully enhanceable.
   *
   * The entry stays in the damage total; only the AT multiplier steps around it,
   * so a Controller's Propel is `base × 2 + impact`, not `(base + impact) × 2`.
   */
  excludeFromAtMechanic?: boolean;
  /**
   * The `conditionalEffects` toggles this row is the mutex counterpart of — the ones
   * whose predicate this row's own gate negates. When such a toggle is active the row
   * is not part of the power any more, and the conditional's own damage is what stands
   * in its place, so `applyActiveConditionals` drops it instead of concatenating both.
   *
   * Stamped by the converter, because the negation lives on the effect group's requires
   * expression and the generated `damage[]` entry has no other trace of it. A row with
   * no `displacedBy` is always additive: Psi Blade's Insight conditional is `replace`
   * off a negated gate on its GrantPower atom, and its damage really is an extra DoT.
   */
  displacedBy?: string[];
}

/**
 * One executed child that rolls a parent's procs in the parent's place — the
 * PPM inputs `CalculateModChance` would read off that child, plus the set
 * categories that decide which slotted procs reach it. See
 * `Power.procRollSites`.
 */
export interface ProcRollSite {
  /** Full name of the child power, for provenance in the Info panel. */
  power: string;
  /**
   * The child's own `BoostsAllowed` — the routing key `CopyBoosts` filters by.
   * A proc piece rolls here when its own `boostsAllowed` intersects this list.
   */
  boostsAllowed: string[];
  /** The child's own AoE radius (ft). */
  radius: number;
  /** The child's own arc in RAW radians, as `stats.arc` carries it. */
  arc: number;
  /** The child's own `ProcMainTargetOnly`, when it sets one — see
   *  {@link Power.procsOnlyOnMainTarget}, same `true`-or-absent shape. */
  procsOnlyOnMainTarget?: true;
}

/**
 * A caster-state write the power performs: it grants or revokes copies of another power.
 *
 * The engine's own type is `coh_data::GrantEdge`; this is the wire shape the converter stamps,
 * and nothing in the TypeScript twin reads it yet. It's declared because the generated files
 * carry it on 987 powers, and an untyped key on a `Power` literal is a tsc error rather than a
 * quiet extra field.
 */
export interface GrantEdge {
  op: 'grant' | 'revoke';
  /** The granted or revoked power's dotted export path, verbatim — the same spelling the
   *  ownership gates read. */
  path: string;
  /** Copies added or removed. The game defaults an absent params count to 1 and the converter
   *  resolves that default, so it is always present. */
  count: number;
  /** The effect group chain's composed gate, as the verbatim token array every other expression
   *  on the wire uses (COND-8). Absent = unconditional. */
  condition?: string[];
  /** The group chain's composed roll, emitted only when it is below 1. */
  chance?: number;
  /** Seconds after activation the edge applies, group chain plus template delay. Absent = at
   *  cast. A delayed edge is a crash-shaped state change (DELAY-1). */
  delaySeconds?: number;
  /** The granted record's own `lifetime`: wall-clock seconds from grant to removal. Absent =
   *  the record authors no wall-clock limit. */
  expires?: number;
  /** The granted record's `lifetime_in_game`, the in-play seconds clock. */
  expiresInGame?: number;
  /** The granted record's `num_allowed` — the stack ceiling ownership counts test against. */
  maxCount?: number;
}

// ============================================
// POWER DEFINITION
// ============================================

export interface Power {
  /** Power name */
  name: string;
  /** Internal name from raw data (e.g., "Radiation_Infection") — canonical stable identifier */
  internalName: string;
  /** Full internal name (e.g., "Pool.Speed.Hasten") */
  fullName?: string;
  /**
   * The `Auto` record that hands this power over, where the game grants rather
   * than offers it. Homecoming files its free travel toggles this way: the
   * `Inherent.Inherent.Prestige_Ninja_Run` record carries no mechanic at all,
   * only a `Grant_Power` aimed at the toggle in the `Prestige` category, and
   * this power IS that toggle. Provenance — nothing reads it — but it is what
   * tells a walked grant apart from a power a fork authored inline (Rebirth
   * does the latter with the same two powers). See INHERENT-6.
   */
  grantedBy?: string;
  /** The caster-state writes this power performs (grant/revoke edges). Absent on the powers
   *  that change no ownership. */
  grantEdges?: GrantEdge[];
  /** Level available (0 = level 1, -1 = unlocked by prerequisite) */
  available: number;
  /** Tier within the powerset */
  tier?: number;
  /** Rank within a pool */
  rank?: number;
  /** Maximum enhancement slots */
  maxSlots: number;
  /** Allowed single enhancement types */
  allowedEnhancements: EnhancementStatType[];
  /** Allowed IO set categories */
  allowedSetCategories?: IOSetCategory[];
  /** Full description */
  description: string;
  /** Short help text shown in UI */
  shortHelp?: string;
  /**
   * The binary's AutoIssue flag. `character_GrantAutoIssuePowers`
   * (`character_base.c:1952`) grants a power instead of offering it as a pick when
   * AutoIssue is set AND its BuyRequires passes AND `available <= level` — so this is
   * only half the gate and never means "granted" on its own.
   */
  autoIssue?: boolean;
  /** The separate "costs no power pick" flag. `powers_load.c:950` forces AutoIssue ⟹ Free. */
  free?: boolean;
  /**
   * `EntsAffected` — the entity categories this power's effects can land on
   * (`['Self']`, `['Foe']`, `['Friend', 'Self']`, …), straight from the export.
   *
   * This is what resolves the pronoun in an atom targeting `AnyAffected`, which
   * says "whoever this power affects" rather than naming anybody: Static Shield
   * and Wormhole both author a `Teleport` resistance that way, and only this
   * field distinguishes the caster's own protection from the immunity the yanked
   * foe gets (DATA-GAP-REGISTER MEZRES-3). The power's `targetType` cannot — a
   * PBAoE that teleports foes is `targetType: 'Self'` too (Shadow Slip).
   *
   * Omitted when the export states nothing, so absent stays distinguishable from
   * an authored empty list.
   */
  targetsAffected?: string[];
  /**
   * The power's damage-type SET, emitted only where the per-template element is
   * genuinely absent from the export (Thunderspy, whose damage atoms arrive
   * `Unmapped`). Recovered from `attack_types` ∪ the shortHelp `DMG()`/`DoT()`
   * clauses, so it is power-level and undercounts rather than inventing a type —
   * never stamped onto individual atoms, which would fabricate the multi-component
   * split. Absent on HC/Rebirth, where the atoms carry the element themselves.
   */
  damageTypes?: DamageType[];
  /** Icon filename */
  icon?: string;
  /** Click, Toggle, Auto, or Passive */
  powerType: PowerType;
  /** Target type */
  targetType?: TargetType;
  /** Effect area */
  effectArea?: EffectArea;
  /** Max targets for AoE */
  maxTargets?: number;
  /**
   * Mez states this power can still be activated through (e.g. Blaster Defiance
   * lets low-tier attacks fire while Held/Slept/Stunned/Terrorized). Values:
   * 'hold' | 'sleep' | 'stun' | 'terror'. Absent when the power can't be cast
   * through any mez (the common case).
   */
  castThroughMez?: Array<'hold' | 'sleep' | 'stun' | 'terror'>;
  /**
   * Mez states that do NOT detoggle this power — it keeps running while you're
   * Held/Slept/Stunned (e.g. mez-protection toggles that must survive the mez
   * they guard against). Values: 'hold' | 'sleep' | 'stun'. Absent when the
   * power detoggles normally on any mez (the common case).
   */
  toggleIgnoreMez?: Array<'hold' | 'sleep' | 'stun'>;
  /**
   * ChainTarget — the raw RPN weighting that decides which target a chain power
   * jumps to next (bin field 43b). Electrical Affinity circuits use it to pick
   * the neediest ally (`kHitPoints%` / `kEndurance%` priority) or the nearest
   * one (`prevdistance`). Carried verbatim from the binary as the token list the
   * wire holds (COND-8); the Info panel humanizes the known patterns. Absent on
   * non-chain powers.
   */
  chainTargetExpression?: string[];
  /**
   * MaxTargetsExpr — a computed target-cap RPN (bin field 38) that overrides the
   * static `stats.maxTargets` when its condition holds (e.g. the circuits' cap
   * grows while the Static buff is stacked; a Tanker Gauntlet attack's cap).
   * A token list, like every other expression on the wire (COND-8). Absent
   * unless the cap is conditional.
   */
  maxTargetsExpression?: string[];
  /**
   * Attributes of this power that NO strength applies to — neither slotted
   * enhancement nor global buffs (e.g. 'RechargeTime' on Rune of Protection /
   * the armor T9s means Hasten and recharge set bonuses do NOT speed them up;
   * 'Range' on most melee attacks). Server-side data not present in the client
   * bin — sourced from the `raw defs/` oracle by the converter, HC only.
   * Absent on the vast majority of powers.
   */
  strengthsDisallowed?: string[];
  /**
   * Attributes for which only GLOBAL strength is ignored — slotted enhancement
   * still applies (e.g. 'RechargeTime' on Kuji-In Rin: recharge IOs work,
   * Hasten/set bonuses don't). Same sourcing/caveats as `strengthsDisallowed`.
   */
  globalStrengthsDisallowed?: string[];
  /**
   * `ProcMainTargetOnly` (power-level bin field). Procs slotted here roll against
   * the main target only, so they use the single-target PPM area factor even
   * though the power carries an AoE radius — that radius belongs to a splash the
   * procs don't follow (Propel's 15ft is its knockback). Applies to EVERY proc in
   * the power, damage or not, because it is a property of the power. Read by
   * `resolveProcRollGeometry`, the one place the rule lives.
   *
   * Authored per AT copy, which is why a name-keyed override could never express
   * it: Lightning Clap carries the flag on Brute/Scrapper/Tanker but not on
   * Stalker.
   *
   * Present on all three forks — 143/153/173 powers. It sits in the stock i24
   * parse table (`powers_load.c:2192`) right after `StrengthsDisallowed`, ahead of
   * HC's inserted `GlobalStrengthsDisallowed`; the Parse6 reader used to stop
   * short of it, so Rebirth and Thunderspy exported an all-zero column that read
   * as absence (PPM-3, recovered 2026-08-03).
   *
   * Only ever `true` — absence is the other state, so the type carries no `false`.
   */
  procsOnlyOnMainTarget?: true;
  /**
   * `ProcAllowed kNone` (HC power-level bin field). **This power never rolls a
   * PPM proc** — whatever is slotted, no PPM chance is computed against its
   * recharge. Sparse and only ever `false` — the flag's one authored value is
   * `kNone`, so absence means procs roll normally. Homecoming only: stock Parse6
   * has no such word (Rebirth), and Thunderspy carries the word but authors no
   * `kNone` (TSPY-5). Read through `powerFiresProcs`, the one place the rule lives.
   *
   * HC authors it on 165 powers, in two groups that mean different things to a
   * player:
   *
   *  - **Pet summons** (every Mastermind henchman, Fire Imps, Phantasm,
   *    Singularity, Gang War, Voltaic Sentinel, Auto Turret, Fold Space…). The
   *    summon cast itself rolls nothing, but the EntCreate template carries
   *    `CopyBoosts`, so a slotted proc still reaches the pet and rolls off the
   *    PET's attacks (this is how Soulbound Allegiance's Build Up proc works in
   *    henchmen). The proc is not dead — its chance simply has nothing to do
   *    with the summon's recharge, which is the number we were reporting.
   *  - **Ordinary attacks and controls** — Paralyzing Blast, Shocking Grasp,
   *    Shockwaves. Here nothing fires at all. These are exactly the
   *    long-recharge powers a PPM formula scores as perfect proc vehicles
   *    (Paralyzing Blast is 240s), which reads as HC closing that door
   *    deliberately.
   *
   * A third group looks like the second and is not: eleven powers (Fault,
   * Whitecap, Hypnotizing Lights, Spring Attack) hand their slotting to a
   * `CopyBoosts` executed child that rolls in the parent's place. They carry
   * `procRollSites`, and `powerFiresProcs` reports them as firing — read that
   * field's doc before treating this one as the last word.
   *
   * Note the contrast with the rains: HC sets this on Burn's patch power
   * (`Pets.Burn.Burn`) and NOT on `Pets.Corruptor_IceStorm.IceStorm` and its
   * kin, which is the clearest evidence in the data that a patch's pulsing
   * power is normally a live proc-firing site. What the bins do NOT say is how
   * often that pulse rolls — see `resolveProcContext`.
   */
  procsAllowed?: false;
  /**
   * Where this power's PPM procs roll when the power itself cannot — present
   * on the powers that pair `ProcAllowed kNone` with a `CopyBoosts` executed
   * child (Fault ×4 archetypes, Whitecap ×4, Hypnotizing Lights ×2, Spring
   * Attack). Stamped by `collectProcRollSites`.
   *
   * A site MOVES the one roll; it never adds one. Fault executes both of its
   * children every cast and no cast has ever paid two procs (37 firings over
   * 48 measured casts, never a double).
   *
   * A proc rolls in the ONE site whose `boostsAllowed` intersects the piece's
   * own — `CopyBoosts` filters by the destination's boost types. Fault logs a
   * to-hit per child and the proc tracked the cone's exactly: the cone missed
   * 3× and paid nothing, the sphere missed 2× and it still fired. A piece no
   * child can hold is handed to no one and fires nothing (Hypnotizing Lights'
   * Sleep procs, by contrast, DO fire — its wide child's `BoostsAllowed`
   * takes `Sleep` even though it accepts no IO sets); a piece two children
   * could hold has no single roll, and the routing throws.
   *
   * The WINDOW stays this power's own recharge and cast — only the geometry
   * comes from the site, which is why a site carries no schedule of its own.
   * See `collectProcRollSites` for the measurements.
   */
  procRollSites?: ProcRollSite[];
  /** Prerequisite power(s) - logical expression */
  requires?: string[];
  /**
   * Game "modes" this power ACTIVATES — combat-state flags set by a `Set_Mode`
   * effect (e.g. Granite Armor sets `Granite_Mode`; Momentum sets `FastMode`;
   * Bright Nova sets `Peacebringer_Blaster_Mode`; Swap Ammo sets `LethalAmmo`).
   * When this power is active in a build, these modes are "live" and drive
   * `modesSuspended` / `modesRequired` on other powers. Raw mode ids; noise
   * (`Disable_All`) stripped. Absent on the vast majority of powers.
   */
  setsModes?: string[];
  /**
   * What this power BECOMES while a mode is live, keyed by the mode id that selects it. The
   * game's PowerRedirector fires a different record entirely at activation time — a Kheldian
   * attack in Nova form, a Titan Weapons attack under Momentum, Seismic Blast under Seismic
   * Power — and the binary carries the whole table on the base power's `Redirect` block.
   *
   * Only the display half is carried: the base keeps its identity, slots and enhancements
   * because every mode shares them. Absent on all but ~30 powers per fork.
   */
  modeVariants?: Record<string, Partial<Power>>;
  /**
   * Modes that SUSPEND this power's own effect contribution while live. The
   * other Stone Armor toggles carry `['Granite_Mode']` (Granite suspends them);
   * pool/travel toggles carry the `Suppress_*` markers Granite/forms set. When
   * an active power in the build `setsModes` an intersecting mode, the totals
   * calc drops this power's direct effects (set bonuses still apply — the toggle
   * is still running) and the UI marks it "Suspended by <setter>". Raw mode ids.
   */
  modesSuspended?: string[];
  /**
   * Modes this power needs to be USABLE — Titan `FastMode` (Momentum) attacks,
   * Kheldian form attacks, Domination-only powers, travel-toggle-gated powers.
   * Never a picker gate: a build planner always slots these, so they are NOT
   * greyed out, and the InfoPanel just shows a "Requires: <mode>" note.
   *
   * The attack chain builder does read it as a cast gate, but only for the modes
   * its own form selector offers. A Momentum- or ammo-gated attack has no selectable
   * form to sit in and is scheduled as it always was. Raw mode ids.
   */
  modesRequired?: string[];
  /**
   * Modes that make this power UNCASTABLE while live — the other half of the gate
   * `modesRequired` opens. Nearly every human-form Kheldian power and every pool
   * click carries the four `*_Blaster_Mode` / `*_Tanker_Mode` form ids.
   *
   * NOT the complement of `modesRequired`: a power with a `modeVariants` redirect
   * into a form is absent from this list for that form specifically, because the
   * game plays the form's version rather than refusing the cast. Gleaming Bolt
   * disallows Nova and redirects under Dwarf; Glinting Eye does the reverse. So
   * "castable in mode M" is `!modesDisallowed?.includes(M)`, and the redirect is
   * what it resolves to — neither question is answerable from `modesRequired`.
   *
   * Raw mode ids, with the ubiquitous `Disable_All` and the meaningless
   * `ServerTrayOverride` stripped. What remains still includes the content gates
   * (`Disable_Epic`, `Disable_Incarnate`, `Disable_Pool`, …), which no build power
   * sets — a consumer only ever asks about modes something in the build is setting.
   */
  modesDisallowed?: string[];
  /**
   * If set, this power is a mechanic (non-standard) power:
   * - 'childToggle': Auto-granted child toggle (ammo types, stance forms, adaptations)
   * - 'parentMechanic': Pickable parent that grants child toggles (Swap Ammo, Staff Mastery)
   * - 'hiddenPassive': Hidden intrinsic passive (Seismic Shockwaves)
   * - 'hiddenAuto': Completely hidden auto-power (Phoenix Rising)
   */
  mechanicType?: 'childToggle' | 'parentMechanic' | 'hiddenPassive' | 'hiddenAuto';
  /** Base stats for this power (new format) */
  stats?: PowerStats;
  /** Damage entries with scale and table (new format) - can be array or single entry */
  damage?: ScaledDamageEntry[] | ScaledDamageEntry;
  /** All effects of this power */
  effects?: PowerEffects;
  /**
   * The pet this power summons — entity name(s) and count, lifespan, display name, the redirect
   * powers a location pseudo-pet stands for, and the inline `resolvedEntities` payload where the
   * entity table has no record. Absent when the power summons nothing.
   *
   * A converter VERDICT built with the whole power in view, not a projection of the atoms: which
   * `EntCreate` row is the power's window (ENT-14), how repeated rows aggregate into `entities`
   * with counts, which P-hash resolves to which sibling, which redirect list names a real entity
   * (ENT-16), and whether a tier-gated summon rebuilds at all.
   *
   * It rode inside {@link effects} and had no other address until 2026-08-29, so retiring that
   * bag took the pets with it and 400+ summoners per fork went dark — no pet on the display, and
   * the buff-pet aura fold at zero on all four forks (DATA-GAP-REGISTER ENT-22). Every partition
   * writes it here now: powersets, pools, epics, inherents and accolades alike, so which powers
   * show their pets stops depending on which bags a strip left behind.
   */
  summon?: SummonEffect;
  /**
   * The fast (uninterruptible) redirect form of an interruptible power, and the gate that
   * selects it. `condition` is the redirect's own expression VERBATIM, as the token list the
   * wire holds — Homecoming's `kEngaged … Experienced_Marksman …`, the forks'
   * `['cur.kToHit', 'source>', '.97', '>=']` — evaluated by the engine against the build. No
   * threshold is re-derived anywhere in the pipeline; matching one fork's gate text is what
   * left two forks with no fast form (SNIPE-2). Tokens rather than one joined string because
   * a joined gate cannot be split back apart (COND-8).
   */
  quickSnipe?: {
    condition: string[];
    stats: Partial<PowerStats>;
    damage: ScaledDamageEntry | ScaledDamageEntry[];
    /**
     * The fast form's own `atoms`, in the same compact wire form as `Power.atoms`. The engine
     * resolves projected damage from the atom list and reads `damage` only for display, so a
     * form that carried only `damage` swapped the cast and left the slow form's charged hit
     * standing — about twice the fast form's, on Homecoming (SNIPE-3).
     */
    atoms?: EncodedAtom[];
  };
  /**
   * Alternate records the power's redirect table selects by conditions the ENGINE
   * evaluates, in first-match order. The general case beside `quickSnipe` (keyed on an
   * interrupt) and `modeVariants` (keyed on a caster mode): these tables have neither
   * shape, so each branch is carried with its condition verbatim and `coh_math::expr`
   * decides. A condition a build cannot answer evaluates Indeterminate and the base
   * record stands — filtering those out in the converter would leave the same powers
   * unserved while looking served.
   */
  formVariants?: ({ condition: string[]; internalName: string; atoms?: EncodedAtom[] } & Partial<Power>)[];
  /**
   * Assassin's Strike from-Hide damage multiplier, expressed as a bonus over the
   * displayed (not-hidden) base — e.g. 2.174 = +217%. Replaces the generic
   * assassination crit (+100%) when AS is fired from Hide. Derived from the
   * Hidden redirect branch (visible Melee + Assassination InherentDamage) by
   * `extractAssassinStrikeDamage`; the ratio is enhancement-invariant so it
   * applies directly to the enhanced base damage. PvE.
   */
  fromHideBonus?: number;
  /**
   * Assassin's Strike's fast mid-combat (Quick) cast time in seconds. The base
   * `stats.castTime` is the slow interruptible from-Hide animation (~3s); fired
   * mid-combat AS is much faster (~0.67–1.77s by set). The attack-chain builder
   * defaults AS to this fast form and reserves the slow base cast for the
   * from-Hide form (opener / post-Placate). Absent on single-form AS (Rebirth).
   */
  midCombatCast?: number;
  /**
   * State-gated bonus effects. Each entry corresponds to a Mechanic Adjuster
   * toggle in the InfoPanel — when active, its `damage` and `effects` add on
   * top of the power's base. Surfaces what the converter's
   * `_isConditionalGate` filter strips from base damage / effects so the
   * underlying mechanic (drowning bonus, Disintegration bonus, Domination
   * boost, etc.) is still reachable.
   *
   * Source: per-template `requires_expression` gates classified by
   * `_classifyConditionalGate` in convert-powerset.cjs.
   */
  conditionalEffects?: ConditionalEffect[];
  /**
   * Procs and unique mechanic surfaces — chance-based effects that don't
   * fire on every cast. Renders in the InfoPanel's SPECIAL section as
   * "+X% chance to <description>" rows. Two flavors:
   *
   * - **grant**: a power-grant proc (Suffocate's "32.57% chance to grant
   *   Drowning on target") — usually a `Null`-attrib chance template
   *   linked to a sibling `conditionalEffects` entry by power-name.
   * - **effect-proc**: a chance-based effect like Water Burst's "33%
   *   chance Knockback" — a non-`Null` attrib template with `chance < 1`.
   */
  specialEffects?: SpecialEffect[];
  /** Damage-over-time procs the power GRANTS via a hidden `Temporary_Powers`
   *  power rather than carrying inline (Molten Embrace's Fire DoT, Stalker Hidden
   *  Flame, Envenomed Blades, Bio Offensive Adaptation, Plant Toxins). The grant
   *  hop is resolved at convert time. */
  grantedDamageProcs?: GrantedDamageProc[];
  /** Mutually exclusive power(s) — picking this power prevents picking the listed internalNames */
  excludes?: string[];
  /**
   * Plan B, Phase 0 — the power's effects as the pre-projection **atom list**
   * (DSH4 `AtomicEffect[]`), the same list `templatesToAtoms` feeds to the
   * `effects` bag projection. Carried in the compact positional `EncodedAtom`
   * wire form (see `src/data/core/atomic-effect.ts`); decode with `decodeAtoms`.
   *
   * Emitted alongside `effects` so no discriminator can be lost by projection.
   * Currently UNUSED by calc/UI — exposed so the atom-native calc primitives
   * (Phase 1) can read it behind a shadow-compare before the bag is retired.
   */
  atoms?: EncodedAtom[];
}

/**
 * A damage-over-time proc a power GRANTS through a hidden `Temporary_Powers`
 * power. The damage lives in the granted power (e.g.
 * `Temporary_Powers.Temporary_Powers.Molten_Embrace_Proc`), not inline on the
 * granting power, so it was invisible to the planner until resolved at convert
 * time. Surfaced informationally on the granting power — it procs off the
 * player's OWN attacks (at `tickChance`), so it isn't folded into the granting
 * power's own attack DPS.
 */
export interface GrantedDamageProc {
  /** Internal name of the granted proc power. */
  name: string;
  displayName: string;
  /** Damage components — scale/table resolve against the summoner's AT, like any attack. */
  damage: { damageType: string; scale: number; table: string }[];
  /** The player's damage enhancements/buffs scale this DoT (false ⇒ `IgnoreStrength`).
   *  The I28P3 Molten Embrace change is exactly flipping this true. */
  enhanceable: boolean;
  /** Per-application proc chance (< 1) — the "chance to inflict … over time". */
  tickChance?: number;
  /** DoT tick interval in seconds (`application_period`). */
  period?: number;
  /** Total DoT duration in seconds. */
  duration?: number;
}

/** A proc/conditional-grant entry for the InfoPanel SPECIAL section. */
export interface SpecialEffect {
  /**
   * - `'grant'`: a power-presence grant proc (e.g. "X% chance to grant
   *   Drowning on target"). Usually backed by a `Null`-attrib chance
   *   template paired with a `conditionalEffects` entry.
   * - `'effect-proc'`: a chance-based simple effect (e.g. Knockback).
   */
  kind: 'grant' | 'effect-proc';
  /** Chance per cast, 0..1. */
  chance: number;
  /** User-facing label fragment. For 'grant', usually a power name like
   *  "Drowning"; for 'effect-proc', the effect name like "Knockback". */
  label: string;
}

/** A single state-gated bonus that the InfoPanel renders as a toggle. */
export interface ConditionalEffect {
  /** Stable identifier — derived from the gate (e.g. 'drowning',
   *  'stealthed', 'disintegration'). Used for state persistence and
   *  curated label overrides. */
  id: string;
  /** Human-readable label shown next to the toggle. */
  label: string;
  /**
   * Where the toggle state lives:
   * - `'global'`: caster-state mechanics (Bio Armor adaptations, Hide,
   *   Domination, In Combat) — flipping the toggle on one power flips it
   *   on every power that shares the same `id`.
   * - `'per-power'`: target-state mechanics (drowning, Disintegrating) —
   *   independent state per power.
   *
   * Derived from the underlying gate's `side`: `source` → global,
   * `target` → per-power. Defaults to per-power when unspecified.
   */
  scope?: 'global' | 'per-power';
  /**
   * Mutually-exclusive group key. When present, only one member of the
   * group can be active at a time (Bio Armor's Defensive / Offensive /
   * Rested adaptations, Tidal Power's stack tiers, Dual Blades combo
   * levels). Members without a `group` render as independent checkboxes.
   */
  group?: string;
  /**
   * How this conditional combines with the base power's effects when active:
   * - `'replace'`: the conditional and a base entry are mutually-exclusive
   *   variants of the same mechanic gated on opposite predicates
   *   (Suffocate's -Def: base "if NOT drowning -11%", conditional "if
   *   drowning -14%"). Active conditional's `effects` shallow-merge over
   *   base, and `damage` entries replace the base entries' equivalents.
   * - `'additive'` (default when omitted): the conditional adds its
   *   contribution alongside the base — same effect-key in both means
   *   "two simultaneous instances" (Suffocate's hold: a base mag-3 cast
   *   plus a conditional mag-3 cast when Domination/Stealthed is on).
   *   Currently surfaces only as appended damage entries; effect-key
   *   collisions are left as base-only since multi-instance display
   *   isn't modeled in the InfoPanel yet.
   *
   * Detection happens at convert time: the converter looks for a base
   * template whose `requires_expression` carries the negated form of the
   * conditional's predicate (`! ownPower?`). When found, the conditional
   * is tagged `mode: 'replace'`.
   */
  mode?: 'additive' | 'replace';
  /** Whether the toggle starts on. Defaults to false; mechanics that fire
   *  automatically (e.g. snipe Quick variant when in combat) may default true. */
  defaultActive?: boolean;
  /**
   * The power this caster-side toggle asserts the build HOLDS, and how many copies
   * satisfy its gate. A build without that power is never offered the toggle — the gate
   * reads `<path> source.ownPower(Num)? [N <op>]`, so the claim is the smallest count
   * that satisfies every constraint in the group.
   */
  ownedPower?: { path: string; count: number };
  /**
   * The archetypes this conditional exists for, in the export's `Class_*` spelling —
   * absent when it exists for everyone.
   *
   * A gate can fork on who cast the power, and the fork is not a second toggle: the
   * Domination bonus a Dominator gets from Cross Punch is simply not there for anyone
   * else holding the same pool power. The Rust engine enforces it in
   * `coh_data::conditional_for_class`; `expandActiveConditionals` is the twin's
   * enforcement point (DATA-GAP-REGISTER COND-4, COND-13).
   */
  casterArchetypes?: string[];
  /** Damage entries that apply on top of base damage when active. */
  damage?: ScaledDamageEntry[] | ScaledDamageEntry;
  /** Effect deltas that apply on top of base effects when active. */
  effects?: PowerEffects;
}

// ============================================
// POWERSET DEFINITION
// ============================================

export interface Powerset {
  /** Internal ID (e.g., "blaster/fire-blast") */
  id?: string;
  /**
   * The set's own name in the binary, fully qualified (`"Brute_Defense.Bio_Organic_Armor"`,
   * `"Pool.Manipulation"`) — the export index's `key`.
   *
   * `id` slugs the DISPLAY name, and the two diverge on six sets across the three forks
   * (Bio Armor is `Bio_Organic_Armor`, Presence is `Manipulation`, Spines is `Quills`).
   * A `requires` gate names the set the binary way, so this is the only field a set path
   * can be matched against; matching `id` reads six sets as unheld by any build.
   */
  setPath?: string;
  /**
   * `SetBuyRequires` — the SET-level gate, in the same postfix language a power's
   * `requires` uses, with `buyRequiresFailed` the message the game shows when it
   * refuses. A power's own gate says whether the game would sell that power; this
   * says whether the set may be taken at all.
   *
   * The one rule it carries is "you can only have one Specialized power pool"
   * (Sorcery / Experimentation / Force of Will / Gadgetry / Utility Belt each list
   * the others' powers). Empty on every other set.
   */
  buyRequires?: string[];
  buyRequiresFailed?: string;
  /**
   * `SpecializeAt` / `SpecializeRequires` — the level a set branches at, and the gate
   * on taking THIS branch. The VEAT branch choices are written here at 23 (Bane vs
   * Crab, and the widow branches), Pool.Fitness at 6. The gate uses a `powerset?`
   * reader, which asks whether the build holds a set rather than a power.
   */
  specializeAt?: number;
  specializeRequires?: string[];
  /** Display name (e.g., "Fire Blast") */
  name: string;
  /** Display name (alternative) */
  displayName?: string;
  /** Archetype this powerset belongs to */
  archetype?: string;
  /** Category (Primary/Secondary, or 'primary'/'secondary') */
  category?: string;
  /** Description of the powerset */
  description: string;
  /** Icon filename */
  icon: string;
  /** Powers in this set */
  powers: Power[];
  /**
   * True when this set exists in the server's bins but is NOT released to
   * players — its powers are locked behind a dev-only access gate
   * (`accesslevel > 0`). Derived at convert time (see `deriveDormant` in
   * scripts/convert-powerset.cjs), not hand-curated. Dormant sets are filtered
   * out of the pickable registry at runtime (see src/data/powersets.ts).
   */
  dormant?: boolean;
}

// ============================================
// POWER POOL DEFINITION
// ============================================

export interface PowerPool extends Powerset {
  /** Pool ID (e.g., "speed") */
  id: string;
  /** Prerequisite expression */
  requires?: string[];
}

// ============================================
// SELECTED POWER (in a build)
// ============================================

import type { Enhancement } from './enhancement';

export interface SelectedPower extends Power {
  /** The powerset this power belongs to */
  powerSet: string;
  /** Level the power was taken at */
  level: number;
  /** Enhancement slots (null = empty slot) */
  slots: (Enhancement | null)[];
  /** If true, this power cannot be removed by the user (inherent powers) */
  isLocked?: boolean;
  /**
   * DISPLAY group for inherent powers (fitness, basic, prestige, archetype). `archetype` is the
   * expanded "<AT> Inherent" section; it says nothing about how the power is calculated — see
   * `derivedMechanic` for that, and `InherentPowerDef.derivedMechanic` for why they are separate.
   */
  inherentCategory?: 'fitness' | 'basic' | 'prestige' | 'archetype';
  /**
   * True for THE archetype mechanic, whose contribution the engine derives in its own pass and
   * therefore skips in the ordinary gather. Set only by `createArchetypeInherentPower`, read only
   * by the engine adapter. An ordinary granted power leaves it absent and is gathered normally,
   * even when it shares the `archetype` display group.
   */
  derivedMechanic?: boolean;
  /** If true, the power is toggled on and its effects apply to stats (for toggle/buff powers) */
  isActive?: boolean;
  /** For powers that grant mutually exclusive sub-powers (e.g., Adaptation), tracks which one is active */
  activeSubPower?: string;
  /** If true, this power was auto-granted by a parent form power (e.g., Kheldian form sub-powers) and does not count against the 24-power limit */
  isAutoGranted?: boolean;
  /** Name of the parent power that granted this power (e.g., "Bright Nova" for Bright Nova Bolt) */
  grantedByPower?: string;
  /**
   * Number of trailing slots in `slots` that were auto-granted by the game
   * (e.g. Fitness Health/Stamina inherent slot grants). These don't count
   * against the user's 67-slot budget.
   */
  inherentSlotCount?: number;
}
