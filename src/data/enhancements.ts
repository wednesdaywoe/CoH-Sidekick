/**
 * Non-IO enhancement vocabulary and presentation metadata.
 *
 * The special registries (Hamidon/Titan/Hydra/D-Sync/prestige) are per-dataset
 * generated data — `src/data/special-enhancements.ts` (SOURCE-1 item 9).
 * Origin-tier values are per-aspect data from the dataset's enhancement
 * curves — `getOriginTierValue` (SOURCE-1 SW8). Common-IO values are exact
 * per-level curve reads — `getIOValueAtLevel` (SOURCE-1 SW5).
 */

import type {
  EnhancementStatType,
  Origin,
  OriginTierInfo,
} from '@/types';

// ============================================
// COMMON IO TYPES
// ============================================

/**
 * The generic ("common") IO types the picker offers, in display order.
 *
 * Hand-written ON PURPOSE, and not to be replaced by a read of the boost index's
 * `commonIoTypes`. BOOST-1 derived that one from the crafted boost family and
 * deliberately left this list alone as the beta's oracle: two independent
 * statements of the same roster, which only mean something while neither is
 * computed from the other. `export-popmenu.test.ts` asserts they agree as sets
 * on all four datasets, so a fork that adds a type reds here rather than
 * quietly offering one list to the picker and another to the exporter.
 *
 * Order is presentation and is this list's alone — the index's copy is sorted.
 *
 * Intangible joined 2026-09-20. The hand list had named 25 of the game's 26
 * since BOOST-1 measured it and fixed only the derived side, while the icon
 * (`Intan.png`), the Schedule A curve and all ten `Crafted_Intangible_*` levels
 * were already in place — the one missing piece was this line. It is visible on
 * two forks: 6 powers each on Rebirth and Thunderspy accept Intangible in their
 * `allowedEnhancements` (Black Hole on four sets, Fold Space, Singularity), and
 * offered no common IO for it. Homecoming and Brainstorm ship the records but
 * no power that takes one, so there the fix is the roster agreeing, not a slot.
 */
export const COMMON_IO_TYPES: EnhancementStatType[] = [
  'Damage',
  'Accuracy',
  'Recharge',
  'EnduranceReduction',
  'EnduranceModification',
  'Range',
  'Interrupt',
  'Defense',
  'Defense Debuff',
  'Resistance',
  'Healing',
  'ToHit',
  'ToHit Debuff',
  'Slow',
  'Hold',
  'Stun',
  'Immobilize',
  'Sleep',
  'Confuse',
  'Fear',
  'Intangible',
  'Knockback',
  'Run Speed',
  'Jump',
  'Fly',
  'Taunt',
];

// ============================================
// TO/DO/SO TIERS
// ============================================

/**
 * Presentation metadata only. Tier VALUES are per-aspect data from the
 * dataset's enhancement curves — `getOriginTierValue` in
 * `@/utils/calculations/enhancement-values` (SOURCE-1 SW8).
 *
 * The flat 8.3 / 16.7 / 33.3 that used to sit here was Schedule A's ladder
 * wearing every schedule's name: the export pays a Defense SO 20% and a
 * Knockback SO 60%, and Thunderspy rebalances the whole ladder so its TO and
 * DO disagreed too. A same-named `getOriginTierValue` next to it shadowed the
 * curve read through the `@/data` barrel.
 */
export const ORIGIN_TIER_INFO: OriginTierInfo[] = [
  {
    name: 'Training Origin',
    short: 'TO',
    description: 'These are the least potent of all Enhancements.',
  },
  {
    name: 'Dual Origin',
    short: 'DO',
    description: 'These are twice as potent as TO Enhancements. Limited to 2 specific Origins.',
  },
  {
    name: 'Single Origin',
    short: 'SO',
    description: 'These are twice as potent as DO Enhancements. Limited to a single Origin.',
  },
];

// ============================================
// ORIGINS
// ============================================

export const ORIGINS: Origin[] = [
  'Magic',
  'Mutation',
  'Natural',
  'Science',
  'Technology',
];

// ============================================
// DUAL ORIGIN COMBINATIONS
// ============================================

export interface DualOriginCombo {
  name: string;
  origins: [Origin, Origin];
}

export const DUAL_ORIGIN_COMBOS: DualOriginCombo[] = [
  { name: 'Genetic Alteration', origins: ['Mutation', 'Science'] },
  { name: 'Mystical Artifact', origins: ['Magic', 'Technology'] },
  { name: 'Mutant Gene', origins: ['Mutation', 'Natural'] },
  { name: 'Technical Upgrade', origins: ['Natural', 'Technology'] },
  { name: 'Enchanted Weapon', origins: ['Magic', 'Natural'] },
  { name: 'Experimental Tech', origins: ['Science', 'Technology'] },
  { name: 'Arcane Mutation', origins: ['Magic', 'Mutation'] },
  { name: 'Scientific Discipline', origins: ['Natural', 'Science'] },
  { name: 'Technological Sorcery', origins: ['Magic', 'Science'] },
  { name: 'Evolved Mutation', origins: ['Mutation', 'Technology'] },
];

/**
 * Check if a dual origin enhancement is valid for a given origin
 */
export function isDualOriginValidForOrigin(combo: DualOriginCombo, origin: Origin): boolean {
  return combo.origins.includes(origin);
}

// ============================================
// ENHANCEMENT TYPE CATEGORIES
// ============================================

export interface EnhancementCategory {
  id: string;
  name: string;
  types: EnhancementStatType[];
}

export const ENHANCEMENT_CATEGORIES: EnhancementCategory[] = [
  {
    id: 'damage',
    name: 'Damage',
    types: ['Damage'],
  },
  {
    id: 'accuracy',
    name: 'Accuracy',
    types: ['Accuracy', 'ToHit'],
  },
  {
    id: 'recharge',
    name: 'Recharge',
    types: ['Recharge'],
  },
  {
    id: 'endurance',
    name: 'Endurance',
    types: ['EnduranceReduction'],
  },
  {
    id: 'defense',
    name: 'Defense',
    types: ['Defense'],
  },
  {
    id: 'resistance',
    name: 'Resistance',
    types: ['Resistance'],
  },
  {
    id: 'healing',
    name: 'Healing',
    types: ['Healing'],
  },
  {
    id: 'mez',
    name: 'Mez',
    types: ['Hold', 'Stun', 'Immobilize', 'Sleep', 'Confuse', 'Fear'],
  },
  {
    id: 'debuff',
    name: 'Debuff',
    types: ['ToHit Debuff', 'Defense Debuff', 'Slow'],
  },
  {
    id: 'travel',
    name: 'Travel',
    types: ['Run Speed', 'Jump', 'Fly'],
  },
  {
    id: 'utility',
    name: 'Utility',
    types: ['Range', 'Knockback', 'Taunt', 'Intangible'],
  },
];
