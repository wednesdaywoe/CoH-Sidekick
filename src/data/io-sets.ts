/**
 * IO Set data and accessor functions
 * Migrated from legacy/js/data/io-sets.js
 *
 * The raw IO set data is imported from a separate file due to its size (~28k lines).
 * This module provides typed accessors and utility functions.
 */

import type {
  IOSet,
  IOSetRegistry,
  IOSetRarity,
  IOSetCategory,
  IOSetPiece,
  SetBonus,
} from '@/types';
import { getActiveDataset } from './dataset';

// ============================================
// IO SET CATEGORIES MAPPING
// ============================================

/**
 * Maps legacy category strings to typed IOSetRarity
 */
const CATEGORY_MAP: Record<string, IOSetRarity> = {
  uncommon: 'uncommon',
  rare: 'rare',
  purple: 'purple',
  ato: 'ato',
  pvp: 'pvp',
  event: 'event',
};

/**
 * Maps archetype IDs to their ATO (Archetype Origin) set category.
 * In CoH, ATOs can be slotted into ANY power of the matching archetype.
 */
export const ARCHETYPE_ATO_CATEGORY: Record<string, IOSetCategory> = {
  blaster: 'Blaster Archetype Sets',
  brute: 'Brute Archetype Sets',
  controller: 'Controller Archetype Sets',
  corruptor: 'Corruptor Archetype Sets',
  defender: 'Defender Archetype Sets',
  dominator: 'Dominator Archetype Sets',
  mastermind: 'Mastermind Archetype Sets',
  scrapper: 'Scrapper Archetype Sets',
  stalker: 'Stalker Archetype Sets',
  tanker: 'Tanker Archetype Sets',
  sentinel: 'Sentinel Archetype Sets',
  peacebringer: 'Kheldian Archetype Sets',
  warshade: 'Kheldian Archetype Sets',
  'arachnos-soldier': 'Soldiers of Arachnos Archetype Sets',
  'arachnos-widow': 'Soldiers of Arachnos Archetype Sets',
  // Guardian is a Rebirth-only AT; its ATOs (Guardian's Gift, Absolute
  // Resolution) slot into any Guardian power.
  guardian: 'Guardian Archetype Sets',
  // Primalist is a Thunderspy-only AT; its ATOs (Primalist's Nature) slot into
  // any Primalist power.
  primalist: 'Primalist Archetype Sets',
};

// ============================================
// RAW IO SET DATA
// ============================================

/**
 * Raw IO set data imported from legacy.
 * This is a large object (~227 sets) that's loaded at runtime.
 *
 * For now, we'll define the type and import the data.
 * In production, this could be lazy-loaded or code-split.
 */

// Type for the raw legacy data format
interface LegacyIOSetPiece {
  num: number;
  name: string;
  aspects: string[];
  proc: boolean;
  unique: boolean;
  totalAspects?: number;
}

interface LegacySetBonusEffect {
  stat: string;
  value: number;
  desc: string;
  pvp?: boolean;
}

interface LegacySetBonus {
  pieces: number;
  effects: LegacySetBonusEffect[];
}

interface LegacyIOSet {
  name: string;
  category: string;
  /** Binary rarity tier from boostsets.bin (feeds getSetRarityMultiplier). */
  rarity: string;
  type: string;
  minLevel: number;
  maxLevel: number;
  attunedOnly: boolean;
  bonuses: LegacySetBonus[];
  pieces: LegacyIOSetPiece[];
  icon: string;
}

export type LegacyIOSetRegistry = Record<string, LegacyIOSet>;

// ============================================
// DATA TRANSFORMATION
// ============================================

/**
 * Transform legacy IO set data to typed format
 */
function transformIOSet(id: string, legacy: LegacyIOSet): IOSet {
  return {
    id,
    name: legacy.name,
    category: (CATEGORY_MAP[legacy.category] || 'uncommon') as IOSetRarity,
    rarity: legacy.rarity,
    type: legacy.type,
    minLevel: legacy.minLevel,
    maxLevel: legacy.maxLevel,
    attunedOnly: legacy.attunedOnly,
    bonuses: legacy.bonuses.map((b) => ({
      pieces: b.pieces,
      effects: b.effects.map((e) => ({
        stat: e.stat,
        value: e.value,
        desc: e.desc,
        ...(e.pvp && { pvp: true }),
      })),
    })),
    pieces: legacy.pieces.map((p) => ({
      num: p.num,
      name: p.name,
      aspects: p.aspects,
      proc: p.proc,
      unique: p.unique,
      ...(p.totalAspects && { totalAspects: p.totalAspects }),
    })),
    icon: legacy.icon,
  };
}

/**
 * Transform entire registry
 */
function transformRegistry(legacy: LegacyIOSetRegistry): IOSetRegistry {
  const registry: IOSetRegistry = {};
  for (const [id, set] of Object.entries(legacy)) {
    registry[id] = transformIOSet(id, set);
  }
  return registry;
}

// ============================================
// IO SET REGISTRY
// ============================================

// Lazy-load + cache the transformed registry per dataset. The raw registry
// rides in the active dataset's dynamic chunk via `getActiveDataset().ioSetsRaw`
// (not a static cross-dataset import), so only the active server's set data is
// downloaded; the transform runs once per dataset on first access.
const _registryCache = new Map<string, IOSetRegistry>();

function _activeRegistry(): IOSetRegistry {
  const ds = getActiveDataset();
  let r = _registryCache.get(ds.id);
  if (!r) {
    r = transformRegistry(ds.ioSetsRaw);
    _registryCache.set(ds.id, r);
  }
  return r;
}

/**
 * Get all IO sets
 */
export function getAllIOSets(): IOSetRegistry {
  return _activeRegistry();
}

const _commonSizeCache = new Map<string, number>();

/**
 * The piece count most of this dataset's sets have (Homecoming: 6, at 169 of 227).
 *
 * Derived rather than assumed, because the picker marks the sets that DIFFER
 * from it — writing `!== 6` would bake a game constant into UI logic and would
 * silently mark the wrong rows on a fork whose catalogue is shaped differently.
 * Computed over the whole catalogue, not a per-power slice, so a given set
 * carries the same mark in every power's picker.
 */
export function getMostCommonSetSize(): number {
  const ds = getActiveDataset();
  const cached = _commonSizeCache.get(ds.id);
  if (cached !== undefined) return cached;

  const tally = new Map<number, number>();
  for (const set of Object.values(_activeRegistry())) {
    const n = set.pieces.length;
    tally.set(n, (tally.get(n) ?? 0) + 1);
  }
  let commonest = 0;
  let best = -1;
  for (const [size, count] of tally) {
    if (count > best) {
      best = count;
      commonest = size;
    }
  }
  _commonSizeCache.set(ds.id, commonest);
  return commonest;
}

// ============================================
// ACCESSOR FUNCTIONS
// ============================================

/**
 * Get an IO set by ID
 * Falls back to hyphen-stripped lookup for backward compatibility
 * (e.g., "gaussians_synchronized_fire-control" → "gaussians_synchronized_firecontrol")
 */
export function getIOSet(id: string): IOSet | undefined {
  return _activeRegistry()[id] ?? _activeRegistry()[id.replace(/-/g, '')];
}

/**
 * Get all IO sets of a specific rarity
 */
export function getIOSetsByRarity(rarity: IOSetRarity): IOSet[] {
  return Object.values(_activeRegistry()).filter((set) => set.category === rarity);
}

/**
 * Get all IO sets that can be slotted in a power category
 */
export function getIOSetsForCategory(category: IOSetCategory): IOSet[] {
  // A set's `type` and a power's allowed categories are the same field read twice
  // (`BoostSet.GroupName`), so they compare as strings. The lookup table that used
  // to sit between them mapped all 56 headings to themselves and answered
  // `undefined` for anything it had not been told about, which dropped a fork's
  // sets from the picker silently rather than failing — see BOOST-2.
  return Object.values(_activeRegistry()).filter((set) => set.type === category);
}

/**
 * Get all IO sets that match any of the allowed categories for a power
 */
export function getIOSetsForPower(allowedCategories: IOSetCategory[] = []): IOSet[] {
  if (!allowedCategories || allowedCategories.length === 0) return [];
  return Object.values(_activeRegistry()).filter(
    (set) => allowedCategories.includes(set.type as IOSetCategory),
  );
}

/**
 * Get a specific piece from an IO set
 */
export function getIOSetPiece(setId: string, pieceNum: number): IOSetPiece | undefined {
  const set = getIOSet(setId);
  return set?.pieces.find((p) => p.num === pieceNum);
}

/**
 * Get bonuses for a given number of pieces from a set
 */
export function getSetBonusesAtCount(setId: string, pieceCount: number): SetBonus[] {
  const set = getIOSet(setId);
  if (!set) return [];

  return set.bonuses.filter((b) => b.pieces <= pieceCount);
}

/**
 * Get all unique IO set types (for filtering UI)
 */
export function getAllIOSetTypes(): string[] {
  const types = new Set<string>();
  for (const set of Object.values(_activeRegistry())) {
    types.add(set.type);
  }
  return Array.from(types).sort();
}

// ============================================
// IO SET RARITY DISPLAY INFO
// ============================================

export interface IOSetRarityInfo {
  id: IOSetRarity;
  name: string;
  description: string;
  color: string;
}

export const IO_SET_RARITIES: IOSetRarityInfo[] = [
  {
    id: 'uncommon',
    name: 'Uncommon',
    description: 'Uncommon invention sets available from invention salvage',
    color: 'text-yellow-400',
  },
  {
    id: 'rare',
    name: 'Rare',
    description: 'Rare invention sets with better bonuses',
    color: 'text-orange-300',
  },
  {
    id: 'purple',
    name: 'Purple',
    description: 'Rare level 50 sets with powerful bonuses',
    color: 'text-purple-400',
  },
  {
    id: 'ato',
    name: 'Archetype',
    description: 'Archetype-specific sets from Super Packs',
    color: 'text-orange-400',
  },
  {
    id: 'pvp',
    name: 'PvP',
    description: 'Sets earned from PvP activities',
    color: 'text-red-400',
  },
  {
    id: 'event',
    name: 'Event',
    description: 'Sets from seasonal events (Winter, Halloween)',
    color: 'text-cyan-400',
  },
];

/**
 * Get display info for a rarity
 */
export function getIOSetRarityInfo(rarity: IOSetRarity): IOSetRarityInfo | undefined {
  return IO_SET_RARITIES.find((r) => r.id === rarity);
}
