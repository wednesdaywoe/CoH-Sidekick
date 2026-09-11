/**
 * Incarnate Registry - centralized, data-driven incarnate definitions
 *
 * Provides:
 * - Slot configuration (colors, icon folders, toggleability, effect types)
 * - Tier configuration (colors, display names, level shifts)
 * - Tree descriptions for all slots
 * - Tree layout configuration (5-column grid positions)
 * - Display helpers (name abbreviation, rare power sorting)
 * - Query/utility functions
 * - Backward-compatible derived constants
 */

import type {
  IncarnateSlotId,
  IncarnateTier,
  IncarnateBranch,
  IncarnatePower,
} from '@/types';
import { INCARNATE_SLOT_ORDER } from '@/types';

// ============================================
// SLOT CONFIGURATION
// ============================================

export type IncarnateEffectType =
  | 'enhancement'   // Alpha: enhancement bonuses applied to all powers
  | 'click_aoe'     // Judgement: clickable AoE attack
  | 'proc_debuff'   // Interface: proc-based debuffs on attacks
  | 'click_buff'    // Destiny: clickable team buff
  | 'summon'        // Lore: summon faction pets
  | 'toggle'        // Hybrid: toggle power with stat bonuses
  | 'amplify';      // Genesis (Rebirth): amplifies a partner incarnate slot

export interface IncarnateSlotConfig {
  /** Slot identifier */
  id: IncarnateSlotId;
  /** Display name */
  displayName: string;
  /** UI theme color (hex) */
  color: string;
  /** Whether this slot provides toggleable stat bonuses for the dashboard */
  isToggleable: boolean;
  /** What kind of effect this slot provides */
  effectType: IncarnateEffectType;
  /** Brief description of the slot's purpose */
  description: string;
}

export const INCARNATE_SLOT_REGISTRY: Record<IncarnateSlotId, IncarnateSlotConfig> = {
  alpha: {
    id: 'alpha',
    displayName: 'Alpha',
    color: '#60A5FA',
isToggleable: true,
    effectType: 'enhancement',
    description: 'Enhancement bonuses that boost all powers',
  },
  judgement: {
    id: 'judgement',
    displayName: 'Judgement',
    color: '#F59E0B',
isToggleable: true,
    effectType: 'click_aoe',
    description: 'Powerful AoE attack',
  },
  interface: {
    id: 'interface',
    displayName: 'Interface',
    color: '#10B981',
isToggleable: true,
    effectType: 'proc_debuff',
    description: 'Proc-based debuffs on attacks',
  },
  destiny: {
    id: 'destiny',
    displayName: 'Destiny',
    color: '#8B5CF6',
isToggleable: true,
    effectType: 'click_buff',
    description: 'Powerful team buff',
  },
  lore: {
    id: 'lore',
    displayName: 'Lore',
    color: '#EC4899',
isToggleable: true,
    effectType: 'summon',
    description: 'Summon faction-themed pets',
  },
  hybrid: {
    id: 'hybrid',
    displayName: 'Hybrid',
    color: '#06B6D4',
isToggleable: true,
    effectType: 'toggle',
    description: 'Toggle power with stat bonuses',
  },
  genesis: {
    id: 'genesis',
    displayName: 'Genesis',
    color: '#A3E635',
    isToggleable: true,
    effectType: 'amplify',
    description: 'Amplifies a partner incarnate slot (Rebirth only)',
  },
};

// ============================================
// TIER CONFIGURATION
// ============================================

export interface IncarnateTierConfig {
  /** Tier identifier */
  id: IncarnateTier;
  /** Display name */
  displayName: string;
  /** Border/glow color (hex) */
  color: string;
  /** Level shift granted at this tier */
  levelShift: number;
}

export const INCARNATE_TIER_REGISTRY: Record<IncarnateTier, IncarnateTierConfig> = {
  common: {
    id: 'common',
    displayName: 'Common',
    color: '#FFFFFF',
    levelShift: 0,
  },
  uncommon: {
    id: 'uncommon',
    displayName: 'Uncommon',
    color: '#FBBF24',
    levelShift: 0,
  },
  rare: {
    id: 'rare',
    displayName: 'Rare',
    color: '#F97316',
    levelShift: 1,
  },
  veryrare: {
    id: 'veryrare',
    displayName: 'Very Rare',
    color: '#A855F7',
    levelShift: 1,
  },
};

// ============================================
// TREE DESCRIPTIONS
// ============================================

/**
 * Brief descriptions for each tree within each slot.
 * Used in the incarnate modal sidebar.
 */
export const TREE_DESCRIPTIONS: Record<string, Record<string, string>> = {
  alpha: {
    cardiac: 'Endurance Cost Reduction, Max Health, Endurance',
    nerve: 'Accuracy, Hold Duration, Slow',
    musculature: 'Damage, Immobilization Duration, Defense Debuff',
    spiritual: 'Recharge, Healing, Fear Duration',
    agility: 'Endurance Modification, Defense, Recharge Reduction',
    intuition: 'Range, Confusion Duration, Taunt',
    resilient: 'Mez Resistance, Defense, Healing',
    vigor: 'Recovery, Healing, Endurance Modification',
  },
  judgement: {
    cryonic: 'Cold damage AoE with Hold',
    ion: 'Energy damage AoE with Endurance drain',
    mighty: 'Smashing damage PBAoE with Knockback',
    pyronic: 'Fire damage Cone with DoT',
    vorpal: 'Lethal damage Cone with -Defense',
    void: 'Negative damage Sphere with -ToHit',
  },
  interface: {
    diamagnetic: '-ToHit and -Regen debuffs',
    gravitic: '-Speed debuffs',
    paralytic: 'Hold effects',
    reactive: 'DoT and -Resistance debuffs',
    cognitive: 'Confuse effects',
    degenerative: '-Max HP and Toxic DoT',
    spectral: '-Defense debuffs',
    preemptive: 'Slow debuffs and -Recharge',
  },
  destiny: {
    ageless: 'Recovery and +Recharge buff',
    barrier: 'Defense and Resistance buff',
    clarion: 'Mez protection',
    incandescence: 'Damage and ToHit buff',
    rebirth: 'Healing and Regeneration buff',
  },
  lore: {
    arachnos: 'Arachnos faction pets',
    banished: 'Banished Pantheon pets',
    carnival: 'Carnival of Shadows pets',
    cimeroran: 'Cimeroran pets',
    clockwork: 'Clockwork pets',
    demons: 'Demon pets',
    drones: 'Drone pets',
    elementals: 'Elemental pets',
    idf: 'Imperial Defense Force pets',
    knives: 'Knives of Artemis pets',
    lights: 'Nictus/Warshade pets',
    longbow: 'Longbow faction pets',
    nemesis: 'Nemesis faction pets',
    phantoms: 'Phantom pets',
    rikti: 'Rikti faction pets',
    rularuu: 'Rularuu pets',
    seers: 'Seer pets',
    talons: 'Talons of Vengeance pets',
    tsoo: 'Tsoo faction pets',
    vanguard: 'Vanguard faction pets',
    warworks: 'War Works faction pets',
  },
  hybrid: {
    assault: 'Damage and critical hit chance',
    control: 'Control magnitude and duration',
    melee: 'Melee damage and survival',
    ranged: 'Ranged damage',
    support: 'Buff/Debuff effectiveness',
  },
  genesis: {
    data: "Boosts your Lore pets' damage and Max HP",
    fate: 'Boosts your Destiny ability effects',
    socket: 'Boosts Interface procs and your Max HP/Endurance',
    verdict: 'Boosts your Judgement attack damage',
  },
};

// ============================================
// TREE LAYOUT CONFIGURATION
// ============================================

/**
 * Describes a single cell in the 5-column tree grid.
 * null = empty placeholder cell.
 */
export type TreeSlotDescriptor = {
  tier: IncarnateTier;
  branch: IncarnateBranch;
  /** For rare tier: 0 = outer position (Total), 1 = inner position (Partial) */
  index?: number;
} | null;

/** A row of exactly 5 cells in the tree grid */
export type TreeRowLayout = [
  TreeSlotDescriptor,
  TreeSlotDescriptor,
  TreeSlotDescriptor,
  TreeSlotDescriptor,
  TreeSlotDescriptor,
];

export interface TreeLayoutConfig {
  /** Rows ordered from top (very rare) to bottom (common) */
  rows: { tier: IncarnateTier; layout: TreeRowLayout }[];
}

/**
 * Standard incarnate power tree layout.
 *
 * Row 4 (Very Rare):  [Core]  [ ]  [ ]  [ ]  [Radial]
 * Row 3 (Rare):       [TC]   [PC]  [ ]  [PR]  [TR]
 * Row 2 (Uncommon):   [ ]   [Core] [ ] [Radial] [ ]
 * Row 1 (Common):     [ ]    [ ] [Base]  [ ]    [ ]
 */
export const STANDARD_TREE_LAYOUT: TreeLayoutConfig = {
  rows: [
    {
      tier: 'veryrare',
      layout: [
        { tier: 'veryrare', branch: 'core' },
        null,
        null,
        null,
        { tier: 'veryrare', branch: 'radial' },
      ],
    },
    {
      tier: 'rare',
      layout: [
        { tier: 'rare', branch: 'core', index: 0 },
        { tier: 'rare', branch: 'core', index: 1 },
        null,
        { tier: 'rare', branch: 'radial', index: 1 },
        { tier: 'rare', branch: 'radial', index: 0 },
      ],
    },
    {
      tier: 'uncommon',
      layout: [
        null,
        { tier: 'uncommon', branch: 'core' },
        null,
        { tier: 'uncommon', branch: 'radial' },
        null,
      ],
    },
    {
      tier: 'common',
      layout: [
        null,
        null,
        { tier: 'common', branch: 'base' },
        null,
        null,
      ],
    },
  ],
};

// ============================================
// DISPLAY HELPERS
// ============================================

/**
 * Sort order for rare-tier powers within a branch.
 * Index 0 = outer position (Total), Index 1 = inner position (Partial).
 */
export const RARE_SORT_KEYWORDS: { keyword: string; index: number }[] = [
  { keyword: 'total', index: 0 },
  { keyword: 'partial', index: 1 },
];

/**
 * Rules applied in order to shorten power display names in tree buttons.
 * The tree name prefix is stripped first, then these replacements are applied.
 */
export const NAME_ABBREVIATION_RULES: { match: string; replacement: string }[] = [
  { match: 'Core ', replacement: 'C.' },
  { match: 'Radial ', replacement: 'R.' },
  { match: 'Total ', replacement: 'T.' },
  { match: 'Partial ', replacement: 'P.' },
];

// ============================================
// QUERY FUNCTIONS
// ============================================

/** Get the full slot config */
export function getSlotConfig(slotId: IncarnateSlotId): IncarnateSlotConfig {
  return INCARNATE_SLOT_REGISTRY[slotId];
}

/** Get the full tier config */
export function getTierConfig(tier: IncarnateTier): IncarnateTierConfig {
  return INCARNATE_TIER_REGISTRY[tier];
}

/** Get the slot UI color */
export function getSlotColor(slotId: IncarnateSlotId): string {
  return INCARNATE_SLOT_REGISTRY[slotId].color;
}

/** Get the tier UI color */
export function getTierColor(tier: IncarnateTier): string {
  return INCARNATE_TIER_REGISTRY[tier].color;
}

/** Get the tier display name */
export function getTierDisplayName(tier: IncarnateTier): string {
  return INCARNATE_TIER_REGISTRY[tier].displayName;
}

/** Check if a slot provides toggleable stat bonuses */
export function isSlotToggleable(slotId: IncarnateSlotId): boolean {
  return INCARNATE_SLOT_REGISTRY[slotId].isToggleable;
}

/** Get all toggleable slot IDs */
export function getToggleableSlotIds(): IncarnateSlotId[] {
  return INCARNATE_SLOT_ORDER.filter((id) => INCARNATE_SLOT_REGISTRY[id].isToggleable);
}

/** Get the tree description for a specific tree within a slot */
export function getTreeDescription(slotId: string, treeId: string): string {
  return TREE_DESCRIPTIONS[slotId]?.[treeId] || '';
}

/**
 * Shorten a power display name using the abbreviation rules.
 * Removes the tree name prefix, then applies each rule in order.
 */
export function abbreviatePowerName(displayName: string, treeName: string): string {
  let result = displayName.replace(treeName + ' ', '');
  for (const rule of NAME_ABBREVIATION_RULES) {
    result = result.replace(rule.match, rule.replacement);
  }
  return result;
}

/**
 * Sort rare-tier powers by Total (outer) / Partial (inner) position.
 */
export function sortRarePowers(powers: IncarnatePower[]): IncarnatePower[] {
  return [...powers].sort((a, b) => {
    return getRareSortIndex(a.displayName) - getRareSortIndex(b.displayName);
  });
}

function getRareSortIndex(displayName: string): number {
  const lower = displayName.toLowerCase();
  for (const { keyword, index } of RARE_SORT_KEYWORDS) {
    if (lower.includes(keyword)) return index;
  }
  return 99;
}

/**
 * Resolve a tree row layout into actual powers.
 * Given the layout descriptor and grouped powers, returns a 5-element array
 * of powers (or null for empty cells).
 */
export function resolveTreeRow(
  rowLayout: TreeRowLayout,
  powersByTierAndBranch: Record<string, Record<string, IncarnatePower[]>>,
): (IncarnatePower | null)[] {
  return rowLayout.map((descriptor) => {
    if (!descriptor) return null;
    const { tier, branch, index } = descriptor;
    const powers = powersByTierAndBranch[tier]?.[branch] || [];
    if (index !== undefined) {
      const sorted = sortRarePowers(powers);
      return sorted[index] || null;
    }
    return powers[0] || null;
  });
}

// ============================================
// BACKWARD-COMPATIBLE DERIVED CONSTANTS
// ============================================

/** @deprecated Use getSlotColor() instead */
export const INCARNATE_SLOT_COLORS: Record<IncarnateSlotId, string> =
  Object.fromEntries(
    Object.entries(INCARNATE_SLOT_REGISTRY).map(([id, cfg]) => [id, cfg.color]),
  ) as Record<IncarnateSlotId, string>;

/** @deprecated Use getTierColor() instead */
export const INCARNATE_TIER_COLORS: Record<IncarnateTier, string> =
  Object.fromEntries(
    Object.entries(INCARNATE_TIER_REGISTRY).map(([id, cfg]) => [id, cfg.color]),
  ) as Record<IncarnateTier, string>;

/** @deprecated Use getTierDisplayName() instead */
export const INCARNATE_TIER_NAMES: Record<IncarnateTier, string> =
  Object.fromEntries(
    Object.entries(INCARNATE_TIER_REGISTRY).map(([id, cfg]) => [id, cfg.displayName]),
  ) as Record<IncarnateTier, string>;
