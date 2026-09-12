/**
 * Game importer
 *
 * Converts a build exported from a running CoH client (Homecoming's
 * `/buildexport` command and the matching JSON shape used by other
 * servers) into a complete Build object that can be loaded into the
 * build store.
 */

import type {
  Build,
  SelectedPower,
  Power,
  Enhancement,
  SpecialEnhancement,
  PoolSelection,
  Origin,
} from '@/types';
import { createEmptyIncarnateBuildState } from '@/types';
import {
  getArchetype,
  getPowerset,
  getAllPowersets,
  getPowerPool,
  getEpicPool,
  getEpicPoolsForArchetype,
  getIOSet,
  getAllIOSets,
  getInherentPowers,
  getArchetypeInherentPowers,
  getPickShadowingInherentPowers,
  createArchetypeInherentPower,
  createIOSetEnhancement,
  createGenericIOEnhancement,
  createOriginEnhancement,
  createSpecialEnhancement,
  getSpecialRegistry,
  getInherentGrantLevel,
} from '@/data';
import { GRANTED_POWER_GROUPS } from '@/data/granted-powers';
import type { InherentPowerDef } from '@/data';
import { computeSetTracking } from '@/utils/calculations/set-tracking';

import { warnFallback } from '@/utils/fallback-warnings';
import {
  parseIOSetUid,
  mapArchetypeClass,
  SPECIAL_SUFFIX_MAPS,
} from '@/utils/enhancement-uid';
import { parseGameExport } from './parser';
import type {
  GameExportData,
  GameExportEnhancement,
  GameImportResult,
  GameImportWarning,
  GameImportSummary,
} from './types';

// ============================================
// ARCHETYPE MAPPING
// ============================================

const ORIGIN_MAP: Record<string, Origin> = {
  'Magic': 'Magic',
  'Mutation': 'Mutation',
  'Natural': 'Natural',
  'Science': 'Science',
  'Technology': 'Technology',
};

// Game-side pool names that don't match our pool IDs after lowercasing.
// Presence is the only known case — internally still called "Manipulation"
// in the game data but exposed in the planner as "presence".
const POOL_NAME_OVERRIDES: Record<string, string> = {
  manipulation: 'presence',
};

/**
 * Maps game export category prefixes to primary/secondary.
 * The game export uses "{Archetype}_{Role}" format for categories.
 */
const CATEGORY_ROLE_MAP: Record<string, 'primary' | 'secondary'> = {
  // Primary roles
  'Ranged': 'primary',      // Blaster, Corruptor, Defender (primary ranged), Sentinel
  'Melee': 'primary',       // Scrapper, Stalker, Brute (varies)
  'Control': 'primary',     // Controller, Dominator
  'Summon': 'primary',      // Mastermind
  'Defensive': 'primary',   // Peacebringer/Warshade (can be secondary too)
  // Secondary roles
  'Support': 'secondary',   // Blaster secondary
  'Buff': 'secondary',      // Defender, Controller, Corruptor, Mastermind secondary
  'Defense': 'secondary',   // Tanker primary → but also Scrapper/Brute/Stalker/Sentinel secondary
  'Assault': 'secondary',   // Dominator secondary
};

/**
 * Determine if a category prefix indicates primary or secondary for a given archetype.
 * Some roles swap meaning depending on archetype (e.g., Defense is primary for Tanker, secondary for Scrapper).
 */
function getCategoryType(category: string, archetypeId: string): 'primary' | 'secondary' | null {
  // Arachnos and Kheldian archetypes use non-standard category names
  // that don't follow the standard "{Archetype}_{Role}" pattern
  if (archetypeId === 'arachnos-widow') {
    if (category === 'Widow_Training') return 'primary';
    if (category === 'Teamwork') return 'secondary';
  }
  if (archetypeId === 'arachnos-soldier') {
    if (category === 'Arachnos_Soldiers') return 'primary';
    if (category === 'Training_Gadgets' || category === 'Training_and_Gadgets') return 'secondary';
  }
  if (archetypeId === 'peacebringer') {
    if (category === 'Peacebringer_Offensive') return 'primary';
    if (category === 'Peacebringer_Defensive') return 'secondary';
  }
  if (archetypeId === 'warshade') {
    if (category === 'Warshade_Offensive') return 'primary';
    if (category === 'Warshade_Defensive') return 'secondary';
  }

  // Standard format: "Archetype_Role" (e.g., "Corruptor_Ranged")
  const parts = category.split('_');
  if (parts.length < 2) return null;

  const role = parts.slice(1).join('_'); // Handle multi-word roles

  // Special cases where role meaning flips based on archetype
  if (role === 'Defense') {
    // Defense is PRIMARY for Tanker, SECONDARY for everyone else
    return archetypeId === 'tanker' ? 'primary' : 'secondary';
  }
  if (role === 'Melee') {
    // Melee is PRIMARY for Scrapper/Stalker/Brute, SECONDARY for Tanker
    return archetypeId === 'tanker' ? 'secondary' : 'primary';
  }
  if (role === 'Ranged') {
    // Ranged is PRIMARY for Blaster/Corruptor/Sentinel, SECONDARY for Defender
    return archetypeId === 'defender' ? 'secondary' : 'primary';
  }
  if (role === 'Buff') {
    // Buff is PRIMARY for Defender, SECONDARY for Controller/Corruptor/Mastermind
    return archetypeId === 'defender' ? 'primary' : 'secondary';
  }

  return CATEGORY_ROLE_MAP[role] ?? null;
}

// ============================================
// GENERIC IO STAT MAPPING
// ============================================

/**
 * Boost-UID stat suffix → EnhancementStatType.
 *
 * The authoritative vocabulary is the game's own `Boosts.Crafted_<stat>_<level>`
 * table — 26 stats, enumerated by the `crafted_*` directories under
 * `exported_powers/boosts/`. Every one of them must appear here or the importer
 * drops the slot: the four debuff/utility stats a player is least likely to slot
 * (Defense_Debuff, ToHit_Debuff, Intangible, Snare) were the ones missing, so
 * the gap only surfaced on builds that used them.
 */
const GENERIC_STAT_MAP: Record<string, string> = {
  'Damage': 'Damage',
  'Accuracy': 'Accuracy',
  'Recharge': 'Recharge',
  'EnduranceReduction': 'EnduranceReduction',
  'Endurance_Reduction': 'EnduranceReduction',
  'EndRdx': 'EnduranceReduction',
  'Range': 'Range',
  'Defense': 'Defense',
  'Defense_Buff': 'Defense',
  'Resistance': 'Resistance',
  'Healing': 'Healing',
  'Heal': 'Healing',
  'ToHit': 'ToHit',
  'ToHit_Buff': 'ToHit',
  'Hold': 'Hold',
  'Stun': 'Stun',
  'Immobilize': 'Immobilize',
  'Immob': 'Immobilize',
  'Sleep': 'Sleep',
  'Confuse': 'Confuse',
  'Fear': 'Fear',
  'Knockback': 'Knockback',
  'Run_Speed': 'Run Speed',
  'RunSpeed': 'Run Speed',
  'Run': 'Run Speed',
  'Jump': 'Jump',
  'Fly': 'Fly',
  'Flight': 'Fly',
  'Slow': 'Slow',
  'Taunt': 'Taunt',
  'EnduranceModification': 'EnduranceModification',
  'EndMod': 'EnduranceModification',
  'Interrupt': 'Interrupt',
  'Absorb': 'Absorb',
  'Intangible': 'Intangible',
  // Internal boost category names — the spelling the game's own boost table uses.
  'Res_Damage': 'Resistance',
  'ToHit_Debuff': 'ToHit Debuff',
  'Defense_Debuff': 'Defense Debuff',
  'Snare': 'Slow',
  'Recovery': 'EnduranceModification',
  'Endurance_Discount': 'EnduranceReduction',
};

// ============================================
// SO/DO ORIGIN PARSING
// ============================================

const SO_ORIGINS = ['Natural', 'Magic', 'Mutation', 'Science', 'Technology'] as const;

/**
 * Stat names used in SO/DO UIDs that differ from GENERIC_STAT_MAP.
 *
 * `Cone` and `Drain_Endurance` have no crafted-IO counterpart at all — they exist
 * only as origin enhancements ("Science Range", "Magic Endurance"), so they would
 * never appear in the generic map.
 */
const SO_STAT_MAP: Record<string, string> = {
  'Recovery': 'EnduranceModification',  // Endurance Recovery SOs
  'Endurance_Discount': 'EnduranceReduction',
  'Defense_Buff': 'Defense',
  'Run': 'Run Speed',
  'Cone': 'Range',                      // "Enhances the range of a power"
  'Drain_Endurance': 'EnduranceModification',
};

/**
 * Resolve a stat key against one or more maps: exact match across all maps
 * first, then a case-insensitive fallback. External tools titleCase stat
 * names inconsistently ("Tohit_Buff" vs "ToHit_Buff", "EndMod" vs "Endmod"),
 * so an exact-only lookup silently drops otherwise-valid enhancements.
 */
function lookupStat(key: string, ...maps: Record<string, string>[]): string | undefined {
  for (const map of maps) {
    if (map[key] !== undefined) return map[key];
  }
  const lower = key.toLowerCase();
  for (const map of maps) {
    const hit = Object.entries(map).find(([k]) => k.toLowerCase() === lower);
    if (hit) return hit[1];
  }
  return undefined;
}

interface ParsedSO {
  /** The stat suffix after origin prefixes are stripped (e.g. "Accuracy", "Run"). */
  stat: string;
  /** SO if one origin prefix, DO if two. */
  tier: 'SO' | 'DO';
  /** First (outermost) origin prefix — used as the SO's "origin" tag. */
  origin: typeof SO_ORIGINS[number];
}

/**
 * Parse a SO/DO UID by stripping up to 2 origin prefixes.
 * Returns the stat / tier / origin if a prefix was found, otherwise null.
 * Examples:
 *   "Magic_Recovery"     → { stat: "Recovery", tier: "SO", origin: "Magic" }
 *   "Natural_Magic_Run"  → { stat: "Run",      tier: "DO", origin: "Natural" }
 */
function parseSOEnhancement(uid: string): ParsedSO | null {
  // IO set pieces end with _[A-F] — skip those
  if (/^.+_[A-F]$/.test(uid)) return null;

  let remaining = uid;
  let firstOrigin: typeof SO_ORIGINS[number] | null = null;
  let stripped = 0;
  for (let i = 0; i < 2; i++) {
    const match = SO_ORIGINS.find((o) => remaining.startsWith(o + '_'));
    if (match) {
      if (firstOrigin === null) firstOrigin = match;
      remaining = remaining.slice(match.length + 1);
      stripped++;
    } else {
      break;
    }
  }
  if (stripped === 0 || firstOrigin === null) return null;
  return {
    stat: remaining,
    tier: stripped >= 2 ? 'DO' : 'SO',
    origin: firstOrigin,
  };
}

// ============================================
// IO SET NAME LOOKUP
// ============================================

let _ioSetNameLookup: Map<string, string> | null = null;

function getIOSetNameLookup(): Map<string, string> {
  if (_ioSetNameLookup) return _ioSetNameLookup;

  const lookup = new Map<string, string>();
  const allSets = getAllIOSets();
  for (const [id, set] of Object.entries(allSets)) {
    // Normalize: "Positron's Blast" → "positrons_blast"
    const normalized = set.name
      .toLowerCase()
      .replace(/['']/g, '')
      .replace(/\s+/g, '_');
    lookup.set(normalized, id);

    // Also store the raw ID
    lookup.set(id, id);

    // Hyphen-stripped variant for IDs like "fire-control" → "firecontrol"
    // (external tools may omit hyphens in internal names)
    const stripped = normalized.replace(/-/g, '');
    if (stripped !== normalized) {
      lookup.set(stripped, id);
    }
    const idStripped = id.replace(/-/g, '');
    if (idStripped !== id) {
      lookup.set(idStripped, id);
    }
  }

  // Dev-name aliases: HC sometimes ships sets with internal names that differ from live
  lookup.set('shrapnel', 'artillery');

  return lookup;
}

// ============================================
// MAIN IMPORT FUNCTION
// ============================================

/**
 * Import from raw game export text (from /buildexport command).
 * Parses the text, then delegates to importFromParsedData.
 */
export function importGameExport(text: string): GameImportResult {
  const parsed = parseGameExport(text);
  if (!parsed) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'general', name: '', message: 'Could not parse game export text. Make sure it includes the header line and power entries.' }],
      summary: { powersImported: 0, powersFailed: 0, enhancementsImported: 0, enhancementsFailed: 0, slotsImported: 0 },
    };
  }

  return importFromParsedData(parsed);
}

/**
 * Import from pre-parsed GameExportData.
 * Shared by the game text importer and external tool JSON importer.
 */
export function importFromParsedData(parsed: GameExportData): GameImportResult {
  const warnings: GameImportWarning[] = [];
  const summary: GameImportSummary = {
    powersImported: 0,
    powersFailed: 0,
    enhancementsImported: 0,
    enhancementsFailed: 0,
    slotsImported: 0,
  };

  // 1. Map archetype
  const archetypeId = mapArchetypeClass(parsed.header.archetype);
  if (!archetypeId) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'archetype', name: parsed.header.archetype, message: `Unknown archetype: ${parsed.header.archetype}` }],
      summary,
    };
  }

  const archetype = getArchetype(archetypeId);
  if (!archetype) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'archetype', name: archetypeId, message: `Archetype data not found: ${archetypeId}` }],
      summary,
    };
  }

  // 3. Map origin and level
  let origin = ORIGIN_MAP[parsed.header.origin];
  if (!origin) {
    warnFallback('game-importer/origin', `unknown origin '${parsed.header.origin}' — defaulting to 'Natural'`);
    origin = 'Natural';
  }
  const level = Math.min(Math.max(parsed.header.level, 1), 50);

  // Build set of non-slottable granted sub-power names to skip during import
  // (e.g., Double Jump from Super Jump, Speed Phase from Super Speed)
  const nonSlottableSubPowers = new Set<string>();
  // Reverse lookup for slottable form sub-powers (Kheldian Nova/Dwarf attacks):
  // sub-power internalName (lowercased) → parent form internalName. These are
  // imported as auto-granted sub-powers nested under the form, not as picks.
  const slottableSubPowerParent = new Map<string, string>();
  for (const [parentName, group] of Object.entries(GRANTED_POWER_GROUPS)) {
    if (group.slottable) {
      for (const subName of group.grantedPowers) {
        slottableSubPowerParent.set(subName.toLowerCase(), parentName);
      }
      continue;
    }
    for (const subName of group.grantedPowers) {
      nonSlottableSubPowers.add(subName.toLowerCase());
    }
  }

  // Kheldian travel inherents (Energy Flight, Combat Flight) appear in the
  // powerset data but are modeled as auto-granted archetype inherents. Their
  // entries are routed to the inherent merge instead of consuming a power pick.
  // The shared list only, not the merged one: Thunderspy reuses the internal
  // names of its server additions (Hide, Placate) for unrelated Stalker set
  // powers, and the merged list would route a picked Possess (internalName
  // Placate) into the inherent merge and drop the pick.
  const archetypeInherentNames = new Set(
    getPickShadowingInherentPowers(archetypeId).map((p) => p.internalName.toLowerCase()),
  );

  // 4. Process powers by category
  const primaryPowers: SelectedPower[] = [];
  const secondaryPowers: SelectedPower[] = [];
  const poolPowersMap: Record<string, SelectedPower[]> = {};
  const epicPowers: SelectedPower[] = [];
  const inherentSlotData: SelectedPower[] = [];

  let primaryId: string | null = null;
  let secondaryId: string | null = null;
  const poolIds: string[] = [];
  let epicPoolId: string | null = null;

  for (const entry of parsed.powers) {
    // Skip Redirects (inherent procs like Gauntlet_Proc)
    if (entry.category === 'Redirects') continue;

    // Inherent powers
    if (entry.category === 'Inherent') {
      const slots = resolveEnhancements(entry.enhancements, warnings, summary);
      if (slots.some((s) => s !== null)) {
        inherentSlotData.push({
          name: entry.powerName.replace(/_/g, ' '),
          powerSet: 'Inherent',
          level: 1,
          available: -1,
          maxSlots: 6,
          slots,
          effects: {},
        } as SelectedPower);
      }
      continue;
    }

    // Pool powers
    if (entry.category === 'Pool') {
      const rawPoolId = entry.powerset.toLowerCase();
      const poolId = POOL_NAME_OVERRIDES[rawPoolId] ?? rawPoolId;
      const pool = getPowerPool(poolId);
      if (!pool) {
        warnings.push({ type: 'pool', name: `${entry.powerset}/${entry.powerName}`, message: `Pool not found: ${poolId}` });
        summary.powersFailed++;
        continue;
      }

      const powerDef = findPowerByInternalName(pool.powers, entry.powerName);
      if (!powerDef) {
        // Some powers are auto-granted (like Fly_Boost) and may not have slots
        if (entry.level === 0 && entry.enhancements.length === 0) continue;
        warnings.push({ type: 'pool', name: `${entry.powerset}/${entry.powerName}`, message: `Pool power not found: ${entry.powerName}` });
        summary.powersFailed++;
        continue;
      }

      // Skip auto-granted pool sub-powers (Double Jump, Speed Phase, Afterburner, etc.)
      // These have available: -1 and are granted by parent travel powers
      if (powerDef.available === -1) continue;

      if (!poolPowersMap[poolId]) {
        poolPowersMap[poolId] = [];
        poolIds.push(poolId);
      }

      const slots = resolveEnhancements(entry.enhancements, warnings, summary);
      poolPowersMap[poolId].push(buildSelectedPower(powerDef, poolId, entry.level, slots));
      summary.powersImported++;
      continue;
    }

    // Epic powers
    if (entry.category === 'Epic') {
      const epicId = resolveEpicPoolId(entry.powerset, archetypeId);
      if (!epicId) {
        warnings.push({ type: 'epic', name: `${entry.powerset}/${entry.powerName}`, message: `Epic pool not found: ${entry.powerset}` });
        summary.powersFailed++;
        continue;
      }

      const epicPool = getEpicPool(epicId);
      if (!epicPool) {
        warnings.push({ type: 'epic', name: epicId, message: `Epic pool data not found: ${epicId}` });
        summary.powersFailed++;
        continue;
      }

      epicPoolId = epicId;
      const powerDef = findPowerByInternalName(epicPool.powers, entry.powerName);
      if (!powerDef) {
        warnings.push({ type: 'epic', name: entry.powerName, message: `Epic power not found: ${entry.powerName}` });
        summary.powersFailed++;
        continue;
      }

      const slots = resolveEnhancements(entry.enhancements, warnings, summary);
      epicPowers.push(buildSelectedPower(powerDef, epicId, entry.level, slots));
      summary.powersImported++;
      continue;
    }

    // Primary/Secondary powers
    const catType = getCategoryType(entry.category, archetypeId);
    if (!catType) {
      warnings.push({ type: 'power', name: `${entry.category}/${entry.powerName}`, message: `Unknown category: ${entry.category}` });
      summary.powersFailed++;
      continue;
    }

    // Resolve powerset ID
    const powersetId = resolvePowersetId(entry.powerset, archetypeId);
    if (!powersetId) {
      warnings.push({ type: 'powerset', name: entry.powerset, message: `Could not resolve powerset: ${entry.powerset} for ${archetypeId}` });
      summary.powersFailed++;
      continue;
    }

    const powerset = getPowerset(powersetId);
    if (!powerset) {
      warnings.push({ type: 'powerset', name: powersetId, message: `Powerset data not found: ${powersetId}` });
      summary.powersFailed++;
      continue;
    }

    const powerDef = findPowerByInternalName(powerset.powers, entry.powerName);
    if (!powerDef) {
      warnings.push({ type: 'power', name: entry.powerName, message: `Power not found in ${powerset.name}: ${entry.powerName}` });
      summary.powersFailed++;
      continue;
    }

    // Skip non-slottable granted sub-powers (Adaptation toggles, Swap Ammo, etc.)
    // Check both the entry name and the resolved internalName against the known set
    if (nonSlottableSubPowers.has(entry.powerName.toLowerCase()) ||
        nonSlottableSubPowers.has(powerDef.internalName.toLowerCase())) continue;

    const formParent = slottableSubPowerParent.get(powerDef.internalName.toLowerCase());

    // Skip auto-granted powerset powers that aren't slottable form sub-powers
    // (e.g. Quantum Boost, granted while Energy Flight is active). available:-1
    // marks them as granted rather than picked, mirroring the pool-power path.
    if (!formParent && powerDef.available === -1) continue;

    const slots = resolveEnhancements(entry.enhancements, warnings, summary);

    // Kheldian travel inherents (Energy Flight, Combat Flight) live in the
    // powerset data but are modeled as auto-granted archetype inherents. Route
    // their slot data into the inherent merge instead of consuming a power pick.
    if (!formParent && archetypeInherentNames.has(powerDef.internalName.toLowerCase())) {
      if (slots.some((s) => s !== null)) {
        inherentSlotData.push({
          name: powerDef.name,
          powerSet: 'Inherent',
          level: 1,
          available: -1,
          maxSlots: 6,
          slots,
          effects: {},
        } as SelectedPower);
      }
      continue;
    }

    const selectedPower = buildSelectedPower(powerDef, powersetId, entry.level, slots);

    // Slottable form sub-powers (Kheldian Nova/Dwarf attacks) attach to their
    // parent form as auto-granted sub-powers: they nest under the form's slot
    // in the chronological view and don't consume a power pick.
    if (formParent) {
      selectedPower.isAutoGranted = true;
      selectedPower.grantedByPower = formParent;
      selectedPower.isActive =
        selectedPower.powerType === 'Toggle' || selectedPower.powerType === 'Auto'
          ? true
          : undefined;
    }

    if (catType === 'primary') {
      primaryId = powersetId;
      primaryPowers.push(selectedPower);
    } else {
      secondaryId = powersetId;
      secondaryPowers.push(selectedPower);
    }
    summary.powersImported++;
  }

  // 5. Build pool selections
  const pools: PoolSelection[] = poolIds.map((id) => {
    const pool = getPowerPool(id);
    return {
      id,
      name: pool?.name ?? id,
      powers: poolPowersMap[id] ?? [],
    };
  });

  // 6. Build epic pool selection
  let epicPool: PoolSelection | null = null;
  if (epicPoolId) {
    const epic = getEpicPool(epicPoolId);
    if (epic) {
      epicPool = {
        id: epicPoolId,
        name: epic.name,
        powers: epicPowers,
      };
    }
  }

  // 7. Build inherent powers
  const inherents = getInherentSelectedPowers(archetypeId, archetype.name, archetype.inherent);

  // Merge slot data from inherent entries
  for (const slotPower of inherentSlotData) {
    const match = inherents.find(
      (inh) => inh.name.toLowerCase() === slotPower.name.toLowerCase()
    );
    if (match && slotPower.slots.length > 0) {
      match.slots = slotPower.slots;
    }
  }

  // 8. Resolve powerset names
  const primaryPowerset = primaryId ? getPowerset(primaryId) : null;
  const secondaryPowerset = secondaryId ? getPowerset(secondaryId) : null;

  // 9. Construct Build
  const build: Build = {
    name: parsed.header.characterName || `${archetype.name} Import`,
    // Game-import results target the currently-active dataset.
    // Imports from another server would need a dataset switch +
    // inference mapping, which is out of scope for now.
    serverId: 'homecoming',
    archetype: {
      id: archetypeId,
      name: archetype.name,
      stats: archetype.stats,
      inherent: archetype.inherent,
    },
    level,
    progressionMode: 'auto',
    primary: {
      id: primaryId,
      name: primaryPowerset?.name ?? '',
      powers: primaryPowers,
    },
    secondary: {
      id: secondaryId,
      name: secondaryPowerset?.name ?? '',
      powers: secondaryPowers,
    },
    pools,
    epicPool,
    inherents,
    accolades: [],
    settings: {
      origin,
    },
    sets: {},
    incarnates: createEmptyIncarnateBuildState(),
    craftingChecklist: {},
    incarnateObtained: {},
    shoppingListAcquired: {},
    slotOrder: [],
    mutedOverCapStats: [],
  };

  // 10. Recompute set tracking
  build.sets = computeSetTracking(build);

  return {
    success: true,
    build,
    warnings,
    summary,
  };
}

// ============================================
// POWER RESOLUTION
// ============================================

/**
 * Find a power within a powerset by internal name (underscore format).
 */
function findPowerByInternalName(powers: Power[], internalName: string): Power | null {
  // Exact internalName match
  const byInternal = powers.find(
    (p) => p.internalName?.toLowerCase() === internalName.toLowerCase()
  );
  if (byInternal) return byInternal;

  // Display name: "Chain_Lightning" → "Chain Lightning"
  const normalized = internalName.replace(/_/g, ' ');
  const byDisplay = powers.find(
    (p) => p.name.toLowerCase() === normalized.toLowerCase()
  );
  if (byDisplay) return byDisplay;

  // fullName last segment match (e.g., "Combat_Flight" matches Pool.Flight.Combat_Flight → "Hover")
  const lowerName = internalName.toLowerCase();
  const byFullName = powers.find((p) => {
    if (!p.fullName) return false;
    const segment = p.fullName.split('.').pop() ?? '';
    return segment.toLowerCase() === lowerName;
  });
  if (byFullName) return byFullName;

  return null;
}

/**
 * Game internal powerset names that differ from our slug IDs.
 * Maps game_name (hyphenated) → our slug.
 */
const POWERSET_ALIASES: Record<string, string> = {
  'bio-organic-armor': 'bio-armor',
  'kinetic-attack': 'kinetic-melee',
  'brawling': 'street-justice',
  'shock-therapy': 'electrical-affinity',
  'sonic-debuff': 'sonic-resonance',
  'radiation-manipulation': 'atomic-manipulation',
  'quills': 'spines',
  'ninja-sword': 'ninja-blade',
  'gadgets': 'devices',
  'martial-manipulation': 'martial-combat',
  'time-manipulation': 'temporal-manipulation',
  'electricity-manipulation': 'electricity-assault',
};

/**
 * Resolve a powerset name from the game export to our powerset ID.
 * Game uses: "Storm_Blast" → we need: "corruptor/storm-blast"
 */
function resolvePowersetId(gameSetName: string, archetypeId: string): string | null {
  // Convert: "Storm_Blast" → "storm-blast"
  const slug = gameSetName.toLowerCase().replace(/_/g, '-');
  const directId = `${archetypeId}/${slug}`;

  // Try direct ID
  if (getPowerset(directId)) return directId;

  // Try alias lookup (game internal names that differ from our slugs)
  const aliasSlug = POWERSET_ALIASES[slug];
  if (aliasSlug) {
    const aliasId = `${archetypeId}/${aliasSlug}`;
    if (getPowerset(aliasId)) return aliasId;
  }

  // Fallback: brute-force search all powersets for this archetype
  const allPowersets = getAllPowersets();
  for (const [id, ps] of Object.entries(allPowersets)) {
    if (ps.archetype?.toLowerCase() !== archetypeId) continue;

    // Match by internal name in the powerset data
    const psSlug = id.split('/')[1];
    if (psSlug === slug) {
      warnFallback('game-importer/resolvePowersetId', `'${gameSetName}' (${archetypeId}) matched by brute-force slug scan → '${id}'`);
      return id;
    }

    // Match by display name
    const psDisplaySlug = ps.name.toLowerCase().replace(/\s+/g, '-');
    if (psDisplaySlug === slug) {
      warnFallback('game-importer/resolvePowersetId', `'${gameSetName}' (${archetypeId}) matched by display-name scan → '${id}'`);
      return id;
    }
  }

  return null;
}

/**
 * Resolve an epic pool name from the game export to our epic pool ID.
 * Game uses: "Corruptor_Mace_Mastery" or "Energy_Mastery"
 */
function resolveEpicPoolId(gameName: string, archetypeId: string): string | null {
  const normalized = gameName.toLowerCase();
  const withArchetype = `${archetypeId.replace(/-/g, '_')}_${normalized}`;

  // Search archetype-specific pools FIRST to avoid cross-archetype collisions
  // (e.g., "Psi_Mastery" exists for controller, scrapper, stalker, etc.)
  const epicPools = getEpicPoolsForArchetype(archetypeId);
  for (const ep of epicPools) {
    const epId = ep.id.toLowerCase();
    // Direct ID match
    if (epId === normalized || epId === withArchetype) return ep.id;
    // Partial match: "Psi_Mastery" matches "melee_psionic_mastery"
    if (epId.endsWith(`_${normalized}`)) return ep.id;
    // Display name match: "Psi Mastery" → "psi_mastery" matches pool name
    if (ep.name.toLowerCase().replace(/\s+/g, '_') === normalized) return ep.id;
  }

  // Fallback: direct match across all pools (for fully-qualified IDs like "corruptor_mace_mastery")
  const pool = getEpicPool(normalized);
  if (pool) return pool.id;

  return null;
}

// ============================================
// ENHANCEMENT RESOLUTION
// ============================================

/**
 * Resolve an array of parsed enhancements into Enhancement objects.
 */
function resolveEnhancements(
  enhancements: (GameExportEnhancement | null)[],
  warnings: GameImportWarning[],
  summary: GameImportSummary,
): (Enhancement | null)[] {
  return enhancements.map((enh) => {
    summary.slotsImported++;
    if (!enh) return null;

    const result = resolveEnhancement(enh);
    if (result.warning) {
      warnings.push(result.warning);
      summary.enhancementsFailed++;
    }
    if (result.enhancement) {
      summary.enhancementsImported++;
    }
    return result.enhancement;
  });
}

interface EnhancementResolveResult {
  enhancement: Enhancement | null;
  warning: GameImportWarning | null;
}

// Special enhancement prefix → category mapping (registries come from the
// active dataset at resolve time). Synthetic HOs share the Hamidon registry
// (identical aspect values in-game; the "synthetic" distinction is cosmetic).
// Prefix order matters — the longer `Synthetic_Hamidon_` must come before
// `Hamidon_` so `startsWith` matches it first.
const SPECIAL_ENH_PREFIXES: [string, SpecialEnhancement['category']][] = [
  ['Synthetic_Hamidon_', 'hamidon'],
  ['Hamidon_', 'hamidon'],
  ['Titan_', 'titan'],
  ['Hydra_', 'hydra'],
  ['DSync_', 'd-sync'],
  ['Dsync_', 'd-sync'],
  ['Generic_', 'prestige'],
];

/**
 * Try to resolve a UID as a special enhancement (HamiO, Titan, Hydra, D-Sync, Prestige).
 * Returns null if the UID doesn't match any special prefix.
 */
function resolveSpecialEnhancement(uid: string, boost?: number, level?: number): EnhancementResolveResult | null {
  for (const [prefix, category] of SPECIAL_ENH_PREFIXES) {
    if (!uid.startsWith(prefix)) continue;

    const registry = getSpecialRegistry(category);
    const suffix = uid.slice(prefix.length).toLowerCase();
    const suffixMap = SPECIAL_SUFFIX_MAPS[category];
    const id = suffixMap?.[suffix];
    if (id && registry[id]) {
      // Special IOs have no crafted level in our model — only boost scales them.
      // The game text export encodes boost explicitly (e.g. "Hydra_… (50+3)" →
      // boost=3), but the external .json export folds it into the level
      // ("level": 53, "numCombines": null). Derive the boost from level-50 when
      // no explicit boost is given so both formats preserve a level-53 special.
      // createSpecialEnhancement caps the result at +3.
      const effectiveBoost = boost ?? (level !== undefined && level > 50 ? level - 50 : undefined);
      return {
        enhancement: createSpecialEnhancement(id, registry[id], category, effectiveBoost),
        warning: null,
      };
    }

    return {
      enhancement: null,
      warning: { type: 'enhancement', name: uid, message: `Unknown ${category} enhancement: ${suffix}` },
    };
  }
  return null;
}

/**
 * Resolve a single parsed enhancement into an Enhancement object.
 */
function resolveEnhancement(enh: GameExportEnhancement): EnhancementResolveResult {
  const { uid, level, boost, attuned } = enh;

  // Check for generic IOs: "Crafted_Accuracy", "Generic_Damage", etc.
  // Both "Crafted_" and "Generic_" prefixes can denote plain generic IOs.
  const genericPrefix = uid.startsWith('Crafted_') ? 'Crafted_'
    : uid.startsWith('Generic_') ? 'Generic_'
    : null;
  if (genericPrefix && !hasIOSetPieceSuffix(uid)) {
    const statPart = uid.slice(genericPrefix.length);
    // Case-insensitive lookup (external tool titleCase may differ: "Tohit_Buff" vs "ToHit_Buff")
    const stat = lookupStat(statPart, GENERIC_STAT_MAP);
    if (stat) {
      const enhancement = createGenericIOEnhancement(
        stat as any,
        level ?? 50,
        boost,
      );
      return { enhancement, warning: null };
    }
    // For Crafted_ prefix, unknown stat is an error; for Generic_, fall through to prestige check
    if (genericPrefix === 'Crafted_') {
      return {
        enhancement: null,
        warning: { type: 'enhancement', name: uid, message: `Unknown generic IO stat: ${statPart}` },
      };
    }
  }

  // Check for Single/Dual Origin enhancements: "Magic_Accuracy", "Natural_Magic_Run", etc.
  // SOs/DOs have an origin prefix but no _[A-F] piece suffix.
  const soResult = parseSOEnhancement(uid);
  if (soResult !== null) {
    const stat = lookupStat(soResult.stat, SO_STAT_MAP, GENERIC_STAT_MAP);
    if (stat) {
      const enhancement = createOriginEnhancement(
        stat as any,
        soResult.tier,
        soResult.origin,
        boost,
      );
      return { enhancement, warning: null };
    }
    // Origin prefix found but stat is unknown — warn but don't block
    return {
      enhancement: null,
      warning: { type: 'enhancement', name: uid, message: `Unknown SO/DO stat: ${soResult.stat}` },
    };
  }

  // Special enhancements: Hamidon, Titan, Hydra, D-Sync
  const specialResult = resolveSpecialEnhancement(uid, boost, level);
  if (specialResult) return specialResult;

  // IO Set enhancement
  const parsed = parseIOSetUid(uid);
  if (!parsed) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', name: uid, message: `Could not parse enhancement UID` },
    };
  }

  // For Superior_Attuned_ UIDs where the set name doesn't already include
  // "superior_", prefer the superior variant (e.g., "blistering_cold" →
  // "superior_blistering_cold"). Game UIDs use "Superior_Attuned_Blistering_Cold"
  // for winter sets but our data stores the superior version as
  // "superior_blistering_cold". For ATOs like Brutes Fury the UID itself
  // embeds "Superior_" in the set name, so parsed.setId is already correct.
  let ioSet;
  if (parsed.superior && !parsed.setId.startsWith('superior_')) {
    const superiorId = `superior_${parsed.setId}`;
    const superiorSet = getIOSet(superiorId);
    if (superiorSet) {
      ioSet = superiorSet;
      parsed.setId = superiorId;
    }
  }
  if (!ioSet) {
    ioSet = getIOSet(parsed.setId);
  }

  // Fallback: name-based lookup
  if (!ioSet) {
    const nameLookup = getIOSetNameLookup();
    const fallbackId = nameLookup.get(parsed.setId);
    if (fallbackId) {
      warnFallback('game-importer/resolveEnhancement', `IO set '${parsed.setId}' resolved via name-based lookup → '${fallbackId}' (UID '${uid}')`);
      ioSet = getIOSet(fallbackId);
    }
  }

  if (!ioSet) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', name: uid, message: `IO set not found: ${parsed.setId}` },
    };
  }

  // Find the piece
  const piece = ioSet.pieces.find((p) => p.num === parsed.pieceNum);
  if (!piece) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', name: uid, message: `Piece ${parsed.pieceNum} not found in set ${ioSet.name}` },
    };
  }

  const enhancement = createIOSetEnhancement(ioSet, piece, parsed.pieceNum - 1, {
    attuned: attuned || parsed.attuned,
    level: attuned ? 50 : (level ?? 50),
    boost,
  });

  return { enhancement, warning: null };
}

/**
 * Check if a UID has an IO set piece suffix (_A through _F).
 * Distinguishes "Crafted_Accuracy" (generic) from "Crafted_Hecatomb_A" (set piece).
 */
function hasIOSetPieceSuffix(uid: string): boolean {
  // Strip the Crafted_ prefix and check if what remains ends with _A through _F
  const afterPrefix = uid.slice('Crafted_'.length);
  // Must have at least something before the _X suffix
  return /^.+_[A-F]$/.test(afterPrefix);
}

// ============================================
// HELPERS
// ============================================

function buildSelectedPower(
  powerDef: Power,
  powersetId: string,
  level: number,
  slots: (Enhancement | null)[],
): SelectedPower {
  // Ensure at least one slot (the inherent slot)
  const finalSlots = slots.length > 0 ? slots : [null];

  return {
    ...powerDef,
    powerSet: powersetId,
    level: Math.max(level, 1),
    slots: finalSlots,
  };
}

function createInherentSelectedPower(def: InherentPowerDef): SelectedPower {
  const slots: (Enhancement | null)[] = def.maxSlots === 0 ? [] : [null];
  return {
    ...def,
    powerSet: 'Inherent',
    level: getInherentGrantLevel(def),
    slots,
    isLocked: def.isLocked ?? true,
    inherentCategory: def.category,
  };
}

function getInherentSelectedPowers(
  archetypeId: string | null,
  archetypeName: string,
  archetypeInherent: { name: string; description: string } | null,
): SelectedPower[] {
  const powers = getInherentPowers().map(createInherentSelectedPower);

  if (archetypeName && archetypeInherent) {
    const atInherentDef = createArchetypeInherentPower(archetypeName, archetypeInherent);
    powers.unshift(createInherentSelectedPower(atInherentDef));
  }

  // Archetype-specific inherent powers (e.g. Kheldian travel powers — Energy
  // Flight, Combat Flight). These aren't in getInherentPowers(); without them
  // a Kheldian build would be missing its inherent travel toggles.
  for (const def of getArchetypeInherentPowers(archetypeId || undefined)) {
    powers.push(createInherentSelectedPower(def));
  }

  return powers;
}
