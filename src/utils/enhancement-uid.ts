/**
 * Shared enhancement-UID + archetype parsing for the build importers.
 *
 * Single source of truth for the .mxd-format knowledge that was previously
 * duplicated — and had already drifted — across mids-import, game-importer, and
 * mxd-import (MSOT-5). The Mids `parseIOSetUid` was the superset (it resolved
 * descriptive-suffix proc UIDs and stripped apostrophes); the game importer's
 * copy returned null on those, silently dropping pieces a .mbd resolved. This
 * module adopts the superset so every import path resolves the same UIDs.
 */
import type { ArchetypeId } from '@/types';

export interface ParsedIOSetUid {
  setId: string;
  pieceNum: number;
  attuned: boolean;
  /** True when the UID had a `Superior_Attuned_` prefix — caller should prefer the `superior_` variant. */
  superior: boolean;
}

/**
 * Parse an IO-set enhancement UID into its components.
 * Examples:
 *   "Superior_Attuned_Superior_Brutes_Fury_A" → { setId: "superior_brutes_fury", pieceNum: 1, attuned: true, superior: true }
 *   "Hecatomb_A"                              → { setId: "hecatomb", pieceNum: 1, attuned: false, superior: false }
 *   "Attuned_Basilisks_Gaze_A"                → { setId: "basilisks_gaze", pieceNum: 1, attuned: true, superior: false }
 *   "Panacea_Hea_End_Rech" (descriptive)      → { setId: "panacea_hea_end_rech", pieceNum: 6, ... }
 */
export function parseIOSetUid(uid: string): ParsedIOSetUid | null {
  let remaining = uid;
  let attuned = false;
  let superior = false;

  // Strip attuned/crafted prefixes
  if (remaining.startsWith('Superior_Attuned_')) {
    remaining = remaining.slice('Superior_Attuned_'.length);
    attuned = true;
    superior = true;
  } else if (remaining.startsWith('Attuned_')) {
    remaining = remaining.slice('Attuned_'.length);
    attuned = true;
  } else if (remaining.startsWith('Crafted_')) {
    remaining = remaining.slice('Crafted_'.length);
  }

  // Standard piece letter (last _X where X is A-F).
  const pieceMatch = remaining.match(/_([A-F])$/);
  let pieceNum: number;
  let setName: string;
  if (pieceMatch) {
    const pieceLetter = pieceMatch[1];
    pieceNum = pieceLetter.charCodeAt(0) - 'A'.charCodeAt(0) + 1;
    setName = remaining.slice(0, -2); // Remove "_X"
  } else {
    // A descriptive suffix where the letter should be ("..._Rez_Effects",
    // event-set procs). Nothing in the string names a slot, so 6 is a guess,
    // and no caller recovers from it: `mapEnhancementUid` takes
    // `pieces.find((p) => p.num === pieceNum)`. This used to claim a
    // display-name resolver did; there isn't one (MBDEXPORT-10).
    //
    // It stands because this function is the FALLBACK. Every such UID in a
    // vendored Mids database is in `MIDS_UIDS`, which `mapEnhancementUid`
    // reads first and which places these by their position in Mids' own member
    // list. What lands here is a UID no vendored database carries.
    pieceNum = 6;
    setName = remaining;
  }

  // Lowercase + strip apostrophes ("Vampire's_Bite" → "vampires_bite"). The
  // exporters preserve apostrophes in UIDs while our planner data is ASCII-only.
  const setId = setName.toLowerCase().replace(/['']/g, '');

  return { setId, pieceNum, attuned, superior };
}

// ============================================
// SPECIAL ENHANCEMENT SUFFIX → REGISTRY ID
// ============================================

export type SpecialCategory = 'hamidon' | 'titan' | 'hydra' | 'd-sync' | 'prestige';

/**
 * Direct mapping from a special-enhancement UID suffix (lowercased) to the
 * registry entry id, per category. Stat-keyword naming: Buff = Defense+ToHit,
 * DeBuff = their debuffs, Mez = all mez, Travel = Fly+Jump+Run, Res_Damage =
 * Resistance, Endurance_Discount = EnduranceReduction, Threat = Taunt.
 */
export const SPECIAL_SUFFIX_MAPS: Record<SpecialCategory, Record<string, string>> = {
  'hamidon': {
    'damage_accuracy': 'nucleolus', 'damage_range': 'centriole',
    'debuff_endurance_discount': 'enzyme', 'debuff_accuracy': 'lysosome',
    'buff_recharge': 'membrane', 'damage_mez': 'peroxisome',
    'res_damage_endurance_discount': 'ribosome', 'heal_endurance_discount': 'golgi',
    'accuracy_mez': 'endoplasm', 'buff_endurance_discount': 'cytoskeleton',
    'travel_endurance_discount': 'microfilament', 'endurance_modification_recharge': 'vesicle',
    'slow_recharge_endurance_discount': 'stereocilia', 'endurance_modification_accuracy': 'microtubule',
    'damage_endurance_discount': 'karyoplasm', 'accuracy_range': 'microvillus',
    'damage_recharge': 'chromatin', 'threat_accuracy_recharge': 'ectosome',
    'heal_recharge': 'amyloplast', 'heal_accuracy': 'chloroplast',
  },
  'titan': {
    'damage_mez': 'amethyst', 'accuracy_mez': 'calcite', 'buff_recharge': 'citrine',
    'damage_accuracy': 'diamond', 'debuff_accuracy': 'gypsum',
    'heal_endurance_discount': 'kyanite', 'res_damage_endurance_discount': 'peridont',
    'damage_range': 'quartz', 'travel_endurance_discount': 'selenite',
    'buff_endurance_discount': 'tanzanite', 'debuff_endurance_discount': 'zeolite',
  },
  'hydra': {
    'debuff_endurance_discount': 'antiproton', 'debuff_accuracy': 'delta',
    'res_damage_endurance_discount': 'electron', 'damage_mez': 'gluon',
    'accuracy_mez': 'graviton', 'damage_accuracy': 'neutrino',
    'damage_range': 'neutron', 'heal_endurance_discount': 'positron',
    'buff_endurance_discount': 'proton', 'buff_recharge': 'quark',
    'travel_endurance_discount': 'theta',
  },
  'd-sync': {
    'travel_endurance_discount': 'acceleration', 'accuracy_mez': 'binding',
    'endurance_modification_recharge': 'conduit', 'damage_mez': 'containment',
    'slow_recharge_endurance_discount': 'deceleration', 'endurance_modification_accuracy': 'drain',
    'damage_endurance_discount': 'efficiency', 'buff_endurance_discount': 'elusivity',
    'damage_accuracy': 'empowerment', 'damage_range': 'extension',
    'res_damage_endurance_discount': 'fortification', 'accuracy_range': 'guidance',
    'debuff_endurance_discount': 'marginalization', 'debuff_accuracy': 'obfuscation',
    'damage_recharge': 'optimization', 'threat_accuracy_recharge': 'provocation',
    'heal_endurance_discount': 'reconstitution', 'heal_recharge': 'reconstruction',
    'buff_recharge': 'shifting', 'heal_accuracy': 'siphon',
  },
  'prestige': {
    'might_of_the_empire': 'might_of_the_empire',
    'clockwork_efficiency': 'clockwork_efficiency',
    'will_of_the_seers': 'will_of_the_seers',
    'resistance_tactics': 'resistance_tactics',
    'syndicate_techniques': 'syndicate_techniques',
  },
};

// ============================================
// ARCHETYPE CLASS MAPS
// ============================================

/**
 * Canonical `Class_X` → ArchetypeId map (superset). Includes the Rebirth-only
 * `Class_Guardian` the game importer's copy was missing.
 */
export const ARCHETYPE_CLASS_MAP: Record<string, ArchetypeId> = {
  'Class_Blaster': 'blaster',
  'Class_Brute': 'brute',
  'Class_Controller': 'controller',
  'Class_Defender': 'defender',
  'Class_Scrapper': 'scrapper',
  'Class_Tanker': 'tanker',
  'Class_Sentinel': 'sentinel',
  'Class_Guardian': 'guardian', // Rebirth-only AT
  'Class_Corruptor': 'corruptor',
  'Class_Dominator': 'dominator',
  'Class_Mastermind': 'mastermind',
  'Class_Stalker': 'stalker',
  'Class_Peacebringer': 'peacebringer',
  'Class_Warshade': 'warshade',
  'Class_Arachnos_Soldier': 'arachnos-soldier',
  'Class_Arachnos_Widow': 'arachnos-widow',
};

/** Resolve a `Class_X` token to an ArchetypeId, or null if unknown. */
export function mapArchetypeClass(midsClass: string): ArchetypeId | null {
  return ARCHETYPE_CLASS_MAP[midsClass] ?? null;
}

/**
 * Display-name → ArchetypeId map (mxd format), derived from ARCHETYPE_CLASS_MAP
 * by stripping the `Class_` prefix and converting underscores to spaces
 * (`Class_Arachnos_Soldier` → `Arachnos Soldier`). Derivation keeps the mxd
 * importer in lockstep and gives it Guardian support for free.
 */
export const CLASS_NAME_TO_ARCHETYPE: Record<string, ArchetypeId> = Object.fromEntries(
  Object.entries(ARCHETYPE_CLASS_MAP).map(([cls, at]) => [cls.slice('Class_'.length).replace(/_/g, ' '), at])
);
