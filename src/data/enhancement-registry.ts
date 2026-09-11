/**
 * Enhancement Registry - centralized, data-driven enhancement definitions
 *
 * Provides:
 * - Stat-to-icon mapping (single source of truth)
 * - Hamidon aspect mapping
 * - IO set category-to-enhancement type mapping
 * - Category priority ordering for UI display
 * - Display configuration (rarity colors, tier colors/borders)
 * - Factory functions for creating Enhancement objects
 * - Query functions for available enhancements per power
 */

import type {
  EnhancementStatType,
  IOSetCategory,
  IOSet,
  IOSetPiece,
  IOSetEnhancement,
  GenericIOEnhancement,
  SpecialEnhancement,
  OriginEnhancement,
  SpecialEnhancementDef,
} from '@/types';
import { COMMON_IO_TYPES } from './enhancements';
import { getSpecialRegistry } from './special-enhancements';
import { genericIOValueAtLevel, getOriginTierValue, normalizeAspectName } from '@/utils/calculations/enhancement-values';
import { resolvePath } from '@/utils/paths';

// ============================================
// STAT ICON MAP
// ============================================

/**
 * Maps EnhancementStatType to icon filenames.
 * Single source of truth for both EnhancementIcon component
 * and generic IO icon resolution.
 */
export const STAT_ICON_MAP: Record<string, string> = {
  Accuracy: 'Acc.png',
  Damage: 'Damage.png',
  Recharge: 'Recharge.png',
  EnduranceReduction: 'EndRdx.png',
  Range: 'Range.png',
  Defense: 'Defbuff.png',
  Resistance: 'DamRes.png',
  Healing: 'Heal.png',
  ToHit: 'ToHitBuff.png',
  Hold: 'Hold.png',
  Stun: 'Disorient.png',
  Immobilize: 'Immob.png',
  Sleep: 'Sleep.png',
  Confuse: 'Confuse.png',
  Fear: 'Fear.png',
  Knockback: 'Knockback.png',
  'Run Speed': 'Run.png',
  Jump: 'Jump.png',
  Fly: 'Fly.png',
  'ToHit Debuff': 'ToHitDebuff.png',
  'Defense Debuff': 'DefDebuff.png',
  EnduranceModification: 'EndMod.png',
  Interrupt: 'Interrupt.png',
  Slow: 'Slow.png',
  Intangible: 'Intan.png',
  Taunt: 'Taunt.png',
  Absorb: 'Heal.png',
};

/** Get icon filename for a stat type */
export function getStatIconFilename(stat: string): string {
  return STAT_ICON_MAP[stat] || 'Damage.png';
}

/** Get the full resolved path for a generic IO icon */
export function getGenericIOIconPath(stat: EnhancementStatType): string {
  const filename = STAT_ICON_MAP[stat];
  if (!filename) return resolvePath('/img/Unknown.png');
  return resolvePath(`/img/Enhancements/Generic/${filename}`);
}

/** Get the full resolved path for an origin enhancement icon */
export function getOriginIconPath(stat: EnhancementStatType, _tier: string): string {
  // Origin enhancements use the same base icons as generic IOs (tier is handled by overlay frames)
  const filename = STAT_ICON_MAP[stat];
  if (!filename) return resolvePath('/img/Unknown.png');
  return resolvePath(`/img/Enhancements/Generic/${filename}`);
}

// ============================================
// CATEGORY PRIORITY
// ============================================

/**
 * Priority ordering for IO set categories in UI display.
 * The first matching category for a power's allowedSetCategories
 * is selected as the default sidebar filter.
 */
export const CATEGORY_PRIORITY: IOSetCategory[] = [
  // Primary damage categories (most common expectation)
  'Ranged Damage',
  'Melee Damage',
  'Ranged AoE Damage',
  'Melee AoE Damage',
  // Rebirth/Thunderspy's own GroupName for the same two concepts.
  'Targeted AoE Damage',
  'PBAoE Damage',
  'Sniper Attacks',
  'Pet Damage',
  'Recharge Intensive Pets',
  // Defense/Resistance (for defensive powers)
  'Defense Sets',
  'Resist Damage',
  // Control (for mez powers)
  'Holds',
  'Stuns',
  'Immobilize',
  'Sleep',
  'Confuse',
  'Fear',
  'Knockback',
  // Universal Control Duration sits with the other mez categories — a
  // single-set niche (Forced Indoctrination) that buffs every mez type.
  'Universal Control Duration Sets',
  // Support primary categories
  'Healing',
  'To Hit Buff',
  // Debuff categories (often secondary effects)
  'To Hit Debuff',
  'Defense Debuff',
  'Accurate To-Hit Debuff',
  'Accurate Defense Debuff',
  'Slow Movement',
  // Rebirth multi-aspect debuff event set (Witchcraft) — sits with the debuffs.
  'Universal Debuff',
  // Other support
  'Endurance Modification',
  'Threat Duration',
  // Rebirth/Thunderspy's own GroupName for the same Threat Duration concept.
  'Taunt',
  'Accurate Healing',
  // Rebirth resurrection event set (Return From The Grave, GroupName "Rez
  // Sets") — niche, sits with the support categories.
  'Rez Sets',
  // Travel (usually specific travel powers)
  'Running',
  'Running & Sprints',
  'Leaping',
  'Leaping & Sprints',
  'Flight',
  'Teleport',
  'Universal Travel',
  // Universal sets (lowest priority - always available)
  'Universal Damage Sets',
  // Archetype sets (usually shown separately)
  'Blaster Archetype Sets',
  'Brute Archetype Sets',
  'Controller Archetype Sets',
  'Corruptor Archetype Sets',
  'Defender Archetype Sets',
  'Dominator Archetype Sets',
  'Mastermind Archetype Sets',
  'Scrapper Archetype Sets',
  'Stalker Archetype Sets',
  'Tanker Archetype Sets',
  'Sentinel Archetype Sets',
  'Kheldian Archetype Sets',
  'Soldiers of Arachnos Archetype Sets',
  'Guardian Archetype Sets',
  'Primalist Archetype Sets',
  // Single-piece Rebirth Challenge enhancement (Inexhaustibility) — niche
  // but kept in the priority list so the sidebar surfaces it predictably
  // alongside other special categories once the parser ships piece data.
  'Rest Buff',
];

/**
 * Sort categories by priority for sidebar display.
 * Categories earlier in CATEGORY_PRIORITY appear first.
 */
export function sortCategoriesByPriority(categories: string[]): string[] {
  return categories.sort((a, b) => {
    const aIndex = CATEGORY_PRIORITY.indexOf(a as IOSetCategory);
    const bIndex = CATEGORY_PRIORITY.indexOf(b as IOSetCategory);
    const aPriority = aIndex === -1 ? 999 : aIndex;
    const bPriority = bIndex === -1 ? 999 : bIndex;
    return aPriority - bPriority;
  });
}

// ============================================
// DISPLAY CONFIGURATION
// ============================================

/** Display config for IO set rarity categories */
export const RARITY_DISPLAY: Record<string, { color: string }> = {
  uncommon: { color: 'text-yellow-400' },
  rare: { color: 'text-orange-300' },
  purple: { color: 'text-purple-400' },
  ato: { color: 'text-yellow-400' },
  pvp: { color: 'text-red-400' },
  event: { color: 'text-cyan-400' },
};

/** Get the Tailwind text color class for a rarity category */
export function getRarityColor(category: string): string {
  return RARITY_DISPLAY[category]?.color || 'text-gray-200';
}

/** Display config for origin enhancement tiers */
export const TIER_DISPLAY: Record<string, { textColor: string; borderColor: string }> = {
  TO: { textColor: 'text-gray-400', borderColor: 'border-gray-600 hover:border-gray-400' },
  DO: { textColor: 'text-yellow-400', borderColor: 'border-yellow-700 hover:border-yellow-400' },
  SO: { textColor: 'text-orange-400', borderColor: 'border-orange-700 hover:border-orange-400' },
};

/** Get the Tailwind text color class for an origin tier */
export function getTierTextColor(tier: string): string {
  return TIER_DISPLAY[tier]?.textColor || 'text-gray-300';
}

/** Get the Tailwind border color classes for an origin tier */
export function getTierBorderColor(tier: string): string {
  return TIER_DISPLAY[tier]?.borderColor || 'border-gray-600 hover:border-gray-400';
}

// ============================================
// FACTORY FUNCTIONS
// ============================================

/**
 * Whether the game ships this set only attuned. Attuned pieces have no fixed
 * level and can't be boosted, so slotting one at a level would misstate its
 * enhancement values and let it be erroneously boosted — the picker hides the
 * level slider for these, and every slotting path forces the flag on.
 *
 * The export states it and the converter carries it: a set whose pieces name no
 * `Crafted_*` boost record has nothing to slot at a level. Two guesses stood in
 * for that field, and the second outlived its own correction by six weeks.
 *
 * `maxLevel <= 1` is the marker most of the roster carries, and it misses the
 * reward sets that keep their 10–50 range. The hand list that patched those was
 * keyed on DISPLAY NAME, and Winter's Gift went onto it on exactly the
 * icon-prefix reasoning the list's own comment forbade (`SEO_` read as "Superior
 * Event Origin"). It is a craftable Universal Travel set — `Crafted_Winters_Gift_*`
 * sits in the export beside the attuned copies on all four datasets — and it came
 * off the list on 2026-07-30 after a user reported it behaving as a normal IO.
 * It was still on canonical's copy of the list six weeks later, where it cost
 * every Winter's Gift piece its level on a `.mbd` write (MBDEXPORT-14), because
 * this file sits outside the shared-surface guard and nothing measured the two
 * lists against each other.
 *
 * A name list gets the other direction wrong too, quietly: Thunderspy carries
 * TWO records displaying "Subaluwa" — its own craftable `KB` set and an
 * `Overwhelming_Force` record that is attuned-only — and no rule keyed on what
 * the game PRINTS can tell those apart.
 */
export function isInherentlyAttuned(set: Pick<IOSet, 'attunedOnly'>): boolean {
  return set.attunedOnly;
}

/**
 * Normalise a stored level offset (`Enhancement.boost`) for persistence.
 *
 * Drops the no-op so a slot at even level stays slim on the wire, and PRESERVES
 * a negative — an SO three levels under you is a real, common state worth x0.70
 * on Homecoming, and flooring it to 0 is how imported builds came to read
 * stronger than they are.
 *
 * Deliberately does NOT clamp to a range. The domain is a property of the active
 * dataset's curves (Homecoming stops at -3, Rebirth runs to -9, Thunderspy
 * applies no attenuation at all), and these factories run during deserialization
 * where the dataset may not be the one the build was authored on. The authority
 * is `enhancementLevelMultiplier`, which clamps to the curve at read time.
 */
function storedLevelOffset(offset?: number): number | undefined {
  if (!offset || !Number.isFinite(offset)) return undefined;
  const whole = Math.trunc(offset);
  return whole === 0 ? undefined : whole;
}

/** Create an IO Set Enhancement object */
export function createIOSetEnhancement(
  set: IOSet,
  piece: IOSetPiece,
  pieceIndex: number,
  options: { attuned: boolean; level: number; boost?: number },
): IOSetEnhancement {
  const setId = set.id || set.name;
  // ATO / event sets are inherently attuned (see isInherentlyAttuned) — force it
  // here so every slotting path (picker, imports, deserialization) is consistent.
  const attuned = options.attuned || isInherentlyAttuned(set);
  // Pure procs (no aspects) don't get boosted, and attuned enhancements can't be boosted
  // Hybrid procs (e.g. LotG Def/+Recharge) CAN be boosted — the aspect portion scales, the proc stays fixed
  const isPureProc = piece.proc && piece.aspects.length === 0;
  const boost = (options.boost && options.boost > 0 && !isPureProc && !attuned) ? options.boost : undefined;
  return {
    type: 'io-set',
    id: `${setId}-${pieceIndex}`,
    // NOTE: `name` is the canonical proc lookup key (findProcData keys on it in
    // the calc engine), so it MUST stay the raw piece label — resolving it to
    // the ioName here would collide with bare PROC_DATABASE keys (e.g. "Chance
    // for +Absorb" → Entomb). Placeholder "Chance" names are rescued to the
    // real ioName at the DISPLAY layer via resolveProcPieceName instead.
    name: piece.name,
    icon: set.icon || 'Unknown.png',
    setId,
    setName: set.name,
    pieceNum: piece.num,
    level: attuned ? undefined : options.level,
    attuned,
    boost,
    aspects: piece.aspects as EnhancementStatType[],
    isProc: piece.proc,
    isUnique: piece.unique,
  };
}

/** Create a Generic IO Enhancement object.
 *
 * `value` is a DERIVED display cache — it is not serialized (build-serialization
 * stores only `stat` + `level` and re-runs this factory on load), and the calc
 * never reads it: `accumulateRawSlotBonuses` re-derives from `stat` + `level`.
 * It carried the Schedule A value for every stat until 2026-07-31, so a ToHit IO
 * read +42.4% in the picker and the info panel while contributing the correct
 * 25.5%. Both now go through the one schedule-aware helper. */
export function createGenericIOEnhancement(
  stat: EnhancementStatType,
  level: number,
  boost?: number,
): GenericIOEnhancement {
  return {
    type: 'io-generic',
    id: `generic-io-${stat}-${level}`,
    name: `${stat} IO`,
    icon: getGenericIOIconPath(stat),
    level,
    boost: (boost && boost > 0) ? boost : undefined,
    stat,
    // Null = the stat has no normalized aspect, which is the one case the calc
    // also contributes nothing for. 0 keeps the two agreeing and reads as visibly
    // broken rather than plausibly wrong; `genericIOVocabulary` reds first.
    value: genericIOValueAtLevel(stat, level) ?? 0,
  };
}

/** Icon prefix for each special enhancement category */
const SPECIAL_ICON_PREFIX: Record<SpecialEnhancement['category'], string> = {
  hamidon: 'HO',
  titan: 'TN',
  hydra: 'HY',
  'd-sync': 'DS',
  prestige: 'Prestige_',
};

/** Overrides for compound-word IDs whose simple capitalize doesn't match the icon filename */
const SPECIAL_ICON_NAME_OVERRIDES: Record<string, string> = {
  antiproton: 'AntiProton',
  // Prestige enhancements: id → icon filename suffix
  clockwork_efficiency: 'ClockworkEfficiency',
  might_of_the_empire: 'MarkoftheEmpire',
  resistance_tactics: 'ResistanceTactics',
  syndicate_techniques: 'SyndicateTechniques',
  will_of_the_seers: 'WilloftheSeers',
};

/** Create a Special Enhancement object (Hamidon, Titan, Hydra, or D-Sync) */
export function createSpecialEnhancement(
  id: string,
  def: SpecialEnhancementDef,
  category: SpecialEnhancement['category'] = 'hamidon',
  boost?: number,
): SpecialEnhancement {
  const capitalizedId = SPECIAL_ICON_NAME_OVERRIDES[id] ?? (id.charAt(0).toUpperCase() + id.slice(1));
  const prefix = SPECIAL_ICON_PREFIX[category];
  // D-Sync enhancements all share a single icon
  const icon = category === 'd-sync' ? 'DSO_all.png' : `${prefix}${capitalizedId}.png`;
  return {
    type: 'special',
    id: `${category}-${id}`,
    name: def.name,
    icon,
    category,
    boost: storedLevelOffset(boost),
    aspects: def.aspects.map(a => ({ stat: a.stat as EnhancementStatType, value: a.value })),
  };
}

/** Create an Origin Enhancement object */
export function createOriginEnhancement(
  stat: EnhancementStatType,
  tier: 'TO' | 'DO' | 'SO',
  origin?: string,
  boost?: number,
): OriginEnhancement {
  const normalized = normalizeAspectName(stat);
  if (!normalized) {
    throw new Error(`Origin enhancement stat "${stat}" is outside the engine aspect vocabulary`);
  }
  return {
    type: 'origin',
    id: `origin-${tier}-${stat}`,
    name: `${stat} ${tier}`,
    icon: getOriginIconPath(stat, tier),
    tier,
    origin: tier === 'SO' ? (origin as OriginEnhancement['origin']) : undefined,
    boost: storedLevelOffset(boost),
    stat,
    // Per aspect, not per tier — a Defense SO is 20% where a Knockback SO is 60%.
    // Stamped from the same reader the math uses so the picker cannot show a number
    // the total disagrees with.
    value: getOriginTierValue(tier, normalized),
  };
}

// ============================================
// QUERY FUNCTIONS
// ============================================

/**
 * Get available generic IO types for a power.
 * Uses only the power's allowedEnhancements (from Homecoming's boosts_allowed)
 * which is the authoritative source for what generic IOs can be slotted.
 */
export function getAvailableGenericIOs(
  power: { allowedEnhancements: string[] } | null,
): EnhancementStatType[] {
  if (!power) return COMMON_IO_TYPES;

  const allowed = new Set(power.allowedEnhancements);
  return COMMON_IO_TYPES.filter((type) => allowed.has(type));
}

/**
 * Filter a special enhancement registry by power compatibility.
 * Returns entries where at least one aspect matches the power's allowed enhancement types.
 */
function filterSpecialEnhancements(
  registry: Record<string, SpecialEnhancementDef>,
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  const entries = Object.entries(registry);
  if (!power) return entries;

  const allowed = new Set(power.allowedEnhancements);
  return entries.filter(([, def]) => {
    return def.aspects.some((a) => allowed.has(a.stat));
  });
}

/** Get available Hamidon enhancements for a power */
export function getAvailableHamidons(
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  return filterSpecialEnhancements(getSpecialRegistry('hamidon'), power);
}

/** Get available Titan enhancements for a power */
export function getAvailableTitans(
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  return filterSpecialEnhancements(getSpecialRegistry('titan'), power);
}

/** Get available Hydra enhancements for a power */
export function getAvailableHydras(
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  return filterSpecialEnhancements(getSpecialRegistry('hydra'), power);
}

/** Get available D-Sync enhancements for a power (empty on datasets without the family) */
export function getAvailableDSyncs(
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  return filterSpecialEnhancements(getSpecialRegistry('d-sync'), power);
}

/** Get available Prestige enhancements for a power */
export function getAvailablePrestige(
  power: { allowedEnhancements: string[] } | null,
): [string, SpecialEnhancementDef][] {
  return filterSpecialEnhancements(getSpecialRegistry('prestige'), power);
}
