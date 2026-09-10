/**
 * Mapping functions to convert Mids Reborn naming conventions to app internal IDs
 */

import type { ArchetypeId, Enhancement, Power, Origin } from '@/types';
import type { DatasetId } from '@/data/dataset';
import {
  getAllPowersets,
  getPowerset,
  getPowerPool,
  getPowerPoolIds,
  getEpicPoolsForArchetype,
  getEpicPool,
  getAllEpicPools,
  getIOSet,
  getAllIOSets,
  createIOSetEnhancement,
  createGenericIOEnhancement,
  createOriginEnhancement,
  createSpecialEnhancement,
  getSpecialRegistry,
} from '@/data';
import { LEGACY_PIECE_ALIASES } from './legacy-piece-aliases';
import type { MidsImportWarning } from './types';
import { warnFallback } from '@/utils/fallback-warnings';
import { midsNameRemap, midsNameOwners } from '@/data/mids-name-map';
import {
  parseIOSetUid,
  mapArchetypeClass,
  SPECIAL_SUFFIX_MAPS,
  type SpecialCategory,
} from '@/utils/enhancement-uid';
import { resolveMidsUid } from '@/data/mids-uids';

// ============================================
// FORK <-> MIDS DATABASE
// ============================================

/**
 * Which Mids database a `.mbd` for each of our forks names, and whether Mids has one for that
 * fork at all.
 *
 * Mids Reborn ships three databases — Generic, Homecoming, Rebirth — and has never had a
 * Thunderspy or a Brainstorm one, so two of our four forks have no honest answer here. Both
 * directions used to invent one from the same else-branch: the writer wrote `Homecoming` for
 * everything that was not Rebirth, and the reader read that string back as the fork. A
 * Thunderspy build exported and re-imported came back refused, with
 * "This build was made for Homecoming, but the planner is currently running Homecoming" —
 * one branch printing both labels.
 *
 * `own: false` marks a stand-in. Brainstorm is Homecoming's open beta and shares its
 * namespace, which is the database its UID and name tables are already cut from. Thunderspy
 * names `Generic`, which is the database its UID table IS — `Thunderspy/EnhDB.mhd` is
 * byte-identical to Mids' `Generic/EnhDB.mhd` — and the only string measured to survive: in
 * Mids 3.8.6 under Wine a build naming `Generic` opens, and one naming `Thunderspy` dies in a
 * .NET error box with no build at all. See DATA-GAP MBDEXPORT-2.
 */
export const MIDS_DATABASE_FOR_DATASET: Record<DatasetId, { database: string; own: boolean }> = {
  homecoming: { database: 'Homecoming', own: true },
  rebirth: { database: 'Rebirth', own: true },
  brainstorm: { database: 'Homecoming', own: false },
  thunderspy: { database: 'Generic', own: false },
};

/**
 * The forks a `.mbd` naming this database may have been built for — the inverse of the table
 * above rather than a second copy of it, so the two directions cannot drift apart.
 *
 * A database identifies a fork only where some fork carries it as its OWN. `Generic` is a real
 * Mids database in its own right, and a build authored in it is a plain CoH build rather than
 * evidence of a fork, so it answers with nothing and the reader takes the file under whatever
 * dataset is loaded. Empty likewise for a name we have never heard of, which is what Mids'
 * own third-party databases would arrive as.
 */
export function datasetsForMidsDatabase(database: string | undefined): DatasetId[] {
  if (!database) return [];
  const named = (Object.entries(MIDS_DATABASE_FOR_DATASET) as [DatasetId, { database: string; own: boolean }][])
    .filter(([, entry]) => entry.database === database);
  return named.some(([, entry]) => entry.own) ? named.map(([id]) => id) : [];
}

// ============================================
// ARCHETYPE MAPPING
// ============================================

export function mapArchetype(midsClass: string): ArchetypeId | null {
  return mapArchetypeClass(midsClass);
}

// ============================================
// ORIGIN MAPPING
// ============================================

const MIDS_ORIGIN_MAP: Record<string, Origin> = {
  'Magic': 'Magic',
  'Mutation': 'Mutation',
  'Natural': 'Natural',
  'Science': 'Science',
  'Technology': 'Technology',
};

export function mapOrigin(midsOrigin: string): Origin {
  return MIDS_ORIGIN_MAP[midsOrigin] ?? 'Natural';
}

/**
 * Mids' `eEnhGrade`, spelled as Mids writes it into a `.mbd`.
 *
 * This importer used to branch on `'SO' | 'DO' | 'TO'`, which no file has ever
 * carried — dead code that made origin enhancements look handled. `SingleO` was
 * meanwhile swallowed one branch earlier by the special-enhancement check, so
 * every single-origin piece in a real build came back "Unrecognized special
 * enhancement prefix" and every DO and TO came back "IO set not found".
 *
 * It read as covered because the only origin-graded pieces in the Homecoming
 * corpus are Hamidon Os, which the special path genuinely owns, and because the
 * test that verified the negative relative levels passed `'SO'` by hand. The
 * first real levelling build ever read lost 79 of its 89 enhancements. See
 * DATA-GAP MBDIMPORT-6.
 *
 * Exported because the writer needs it too, and needed it for exactly as long.
 * `mids-export.ts` wrote our own `SO` back into the file, and Mids `Enum.Parse`s
 * that field: it throws inside `LoadBuild` and the whole build refuses to open,
 * not one slot of it. Fixing the reader and leaving the writer to its own
 * spelling is how MBDEXPORT-4 survived MBDIMPORT-6 by a day. One table, read
 * forwards here and backwards there.
 */
export const MIDS_ORIGIN_TIER: Record<string, 'TO' | 'DO' | 'SO'> = {
  TrainingO: 'TO',
  DualO: 'DO',
  SingleO: 'SO',
};

/**
 * Split `Magic_Endurance_Discount` into its origin and its stat.
 *
 * Mids names an origin enhancement `<Origin>_<Stat>`, and the stat half is what
 * `MIDS_STAT_MAP` keys on. Only the five real origins split — a UID that starts
 * with anything else is left whole, so a shape this does not know reaches the
 * stat lookup intact and fails there by name rather than being quietly halved.
 */
function splitOriginUid(uid: string): [Origin | undefined, string] {
  const cut = uid.indexOf('_');
  if (cut > 0) {
    const head = uid.slice(0, cut);
    if (MIDS_ORIGIN_MAP[head]) return [MIDS_ORIGIN_MAP[head], uid.slice(cut + 1)];
  }
  return [undefined, uid];
}

// ============================================
// POWERSET MAPPING
// ============================================

/**
 * Manual overrides for powerset icon names that don't match Mids' internal names.
 * Key: lowercase Mids internal name (second segment of dotted path)
 * Value: the icon stem that should be used for lookup
 */
const POWERSET_ICON_OVERRIDES: Record<string, string> = {
  // Add overrides here as mismatches are discovered during testing
  // e.g., 'some_mids_name': 'some_icon_stem',
};

/**
 * Build a reverse lookup from Mids powerset naming to app powerset IDs.
 * Uses the powerset icon field as the bridge (icon stems match Mids internal names).
 *
 * Returns Map<string, string> where:
 *   key = "{archetype}:{mids_internal_name}" (lowercased)
 *   value = app powerset ID (e.g., "brute/kinetic-melee")
 */
export function buildPowersetLookup(): Map<string, string> {
  const lookup = new Map<string, string>();
  const allPowersets = getAllPowersets();

  for (const [id, powerset] of Object.entries(allPowersets)) {
    if (!powerset.archetype || !powerset.icon) continue;

    // Extract internal name from icon: "kinetic_attack_set.png" → "kinetic_attack"
    // Rebirth powerset icons use .ico, HC uses .png — strip either.
    const iconStem = powerset.icon
      .replace(/_set\.(png|ico)$/, '')
      .replace(/\.(png|ico)$/, '');

    // Build the lookup key: "brute:kinetic_attack"
    const key = `${powerset.archetype}:${iconStem}`.toLowerCase();

    // Handle icon collisions (e.g., psionic_armor reuses dark_armor_set.png):
    // Prefer the powerset whose ID slug matches the icon stem
    const existing = lookup.get(key);
    if (existing) {
      const newSlug = id.split('/')[1] ?? '';
      const iconSlug = iconStem.replace(/_/g, '-');
      // Keep whichever one's slug matches the icon stem
      if (newSlug === iconSlug) {
        lookup.set(key, id);
      }
      // Otherwise keep existing (it either matches or was set first)
    } else {
      lookup.set(key, id);
    }
  }

  return lookup;
}

/**
 * Resolve a Mids powerset path to an app powerset ID.
 * @param midsPath - e.g., "Brute_Melee.Kinetic_Attack"
 * @param archetypeId - the mapped archetype ID (e.g., "brute")
 * @param powersetLookup - the reverse lookup map from buildPowersetLookup()
 */
export function resolvePowerset(
  midsPath: string,
  archetypeId: string,
  powersetLookup: Map<string, string>,
): string | null {
  // Some Mids exports include trailing whitespace in segments
  // (Rebirth Guardian builds emit "Guardian_Composition.Energy_Composition "
  // with the trailing space). Trim every segment defensively.
  const segments = midsPath.split('.').map(s => s.trim());
  if (segments.length < 2) return null;

  const midsInternalName = segments[1].toLowerCase();

  // Check overrides first
  const overrideName = POWERSET_ICON_OVERRIDES[midsInternalName];
  if (overrideName) {
    const overrideKey = `${archetypeId}:${overrideName}`.toLowerCase();
    const overrideResult = powersetLookup.get(overrideKey);
    if (overrideResult) return overrideResult;
  }

  // Try direct ID construction first (most precise — avoids icon collisions)
  // "Dark_Armor" → "dark-armor" → "tanker/dark-armor"
  const idSlug = midsInternalName.replace(/_/g, '-');
  const directId = `${archetypeId}/${idSlug}`;
  if (getPowerset(directId)) return directId;

  // Fallback: icon-based lookup (handles cases where Mids name differs from app slug)
  const key = `${archetypeId}:${midsInternalName}`.toLowerCase();
  const iconResult = powersetLookup.get(key);
  if (iconResult) return iconResult;

  return null;
}

// ============================================
// POWER MAPPING (within a powerset)
// ============================================

/**
 * Hand-authored Mids-name → app internalName remaps, applied ONLY after every other
 * matcher has failed against the original name.
 *
 * This is now the RESIDUE of the derived map, not the mechanism. `convert-mids-name-map.cjs`
 * joins Mids' power list to the export on display name and reproduces six of the rows below
 * on its own, scoped per powerset — which is strictly better than the caveat this table used
 * to carry about `Conserve_Power` being valid in Brute Energy Aura and not Tanker's.
 *
 * What is left is what a display-name join cannot reach: a display name Mids itself spells
 * differently ("Brillant Barrage"), and the VEAT branch prefixes, which are a naming
 * convention rather than a rename. Rows the derived map now covers are kept because it is
 * built per dataset, and a fork whose Mids database is older still arrives here.
 */
const MIDS_NAME_TYPOS: Record<string, string> = {
  'spectral_terrror': 'Spectral_Terror',

  // Stalker Assassin-power renames.
  'assassins_smash': 'Assassins_Rockslide',     // Stone Melee
  'assassins_whisper': 'Assassins_Resonance',   // Sonic Melee

  // Pyrotechnic Control T9: renamed/reworked from Multipurpose_Missiles → Glitz
  // (display: "Brilliant Barrage").
  'multipurpose_missiles': 'Glitz',

  // Tanker Energy Aura: Conserve_Power was removed and its function folded
  // into Energize. Brute Energy Aura still has Conserve_Power natively, so
  // this rename is only used as a fallback.
  'conserve_power': 'Energize',

  // Mastermind Kinetics T9: Kinetic_Transfer is the internal redirect power for
  // Fulcrum_Shift on the MM variant. Controller/Defender Kinetics still has
  // Kinetic_Transfer natively; the rename fires only for MM builds.
  'kinetic_transfer': 'Fulcrum_Shift',

  // VEAT prefix stripping. Mids prefixes some powers with a branch code
  // (BS_, FRT_, NW_) that the HC client data doesn't use.
  'bs_bash': 'Bash',                   // Arachnos Soldier / Bane Spider Soldier
  'frt_fate_sealed': 'Fate_Sealed',    // Arachnos Widow / Fortunata Teamwork
  'nw_pain_tolerance': 'Pain_Tolerance', // Arachnos Widow / Widow Teamwork

  // Mids quirk: an extra `P` in EMP Pulse for Mastermind Radiation Emission.
  'emp_pulse': 'EM_Pulse',
};

/**
 * Mids full paths that reference powers/effects with no user-selectable
 * counterpart in HC. When encountered at the top level of PowerEntries we
 * silently skip them (no warning, no failure) — they're auto-granted passives
 * or Mids serialization artifacts.
 *
 * Keys are lowercase full Mids paths (e.g. `mastermind_summon.beast_mastery.pack_mentality`).
 */
export const MIDS_SILENT_SKIP_PATHS = new Set<string>([
  // Auto-granted passive from Beast Mastery summons; not a player pick in HC.
  'mastermind_summon.beast_mastery.pack_mentality',

  // NOT skipped: `Mastermind_Buff.Radiation_Emission.Radiation_Emission` used to sit here
  // as a presumed set-root artifact. It is a real power — Mids displays it as "Radiant
  // Aura", which is `Radiant_Aura` here — so the skip was throwing away a Mastermind's
  // heal on every import. The derived name map (MBDIMPORT-2) resolves it; the guess that
  // put it on this list was made from the name alone, without asking what Mids displays.
]);

function tryMatch(powers: Power[], name: string): Power | null {
  // Exact internalName match
  const byInternal = powers.find(
    (p) => p.internalName?.toLowerCase() === name.toLowerCase(),
  );
  if (byInternal) return byInternal;

  // Display name: "Quick_Strike" → "Quick Strike"
  const normalized = name.replace(/_/g, ' ');
  const byDisplay = powers.find(
    (p) => p.name.toLowerCase() === normalized.toLowerCase(),
  );
  if (byDisplay) {
    warnFallback('findPowerByMidsName', `'${name}' matched by display name → '${byDisplay.name}' (internalName '${byDisplay.internalName}') — internalName lookup failed`);
    return byDisplay;
  }

  // Collapse runs of separators: "Enervating__Field" → "enervating field".
  const normalizeAll = (s: string) => s.toLowerCase().replace(/[-_\s]+/g, ' ').trim();
  const normalizedAll = normalizeAll(name);
  const byDisplayNormalized = powers.find(
    (p) => normalizeAll(p.name) === normalizedAll,
  );
  if (byDisplayNormalized) {
    warnFallback('findPowerByMidsName', `'${name}' matched by hyphen-normalized display name → '${byDisplayNormalized.name}' (internalName '${byDisplayNormalized.internalName}')`);
    return byDisplayNormalized;
  }
  const byInternalNormalized = powers.find(
    (p) => p.internalName && normalizeAll(p.internalName) === normalizedAll,
  );
  if (byInternalNormalized) {
    warnFallback('findPowerByMidsName', `'${name}' matched by collapsed-separator internalName → '${byInternalNormalized.name}' (internalName '${byInternalNormalized.internalName}')`);
    return byInternalNormalized;
  }

  // fullName last segment (e.g. "Combat_Flight" → Pool.Flight.Combat_Flight / "Hover")
  const lowerName = name.toLowerCase();
  const byFullName = powers.find((p) => {
    if (!p.fullName) return false;
    const segment = p.fullName.split('.').pop() ?? '';
    return segment.toLowerCase() === lowerName;
  });
  if (byFullName) {
    warnFallback('findPowerByMidsName', `'${name}' matched by fullName last-segment → '${byFullName.name}' (internalName '${byFullName.internalName}')`);
    return byFullName;
  }

  // Last-resort: strip ALL separators and compare alphanumerics only.
  // Catches cases where Mids has a separator we don't (or vice-versa) —
  // e.g. Rebirth Mids emits "Moon_Beam" but our internalName is "Moonbeam"
  // (Dark Assault), or "Build_Up" vs "BuildUp".
  const stripSep = (s: string) => s.toLowerCase().replace(/[^a-z0-9]/g, '');
  const stripped = stripSep(name);
  const byStripped = powers.find((p) =>
    (p.internalName && stripSep(p.internalName) === stripped) ||
    stripSep(p.name) === stripped
  );
  if (byStripped) {
    warnFallback('findPowerByMidsName', `'${name}' matched by all-separators-stripped fallback → '${byStripped.name}' (internalName '${byStripped.internalName}')`);
    return byStripped;
  }

  return null;
}

/**
 * A powerset the candidate powers were drawn from — a powerset, a power pool, an epic pool.
 *
 * Both shapes are accepted because the two carry their identity differently: an archetype
 * powerset states it as `setPath`, while pool and epic powers carry a `fullName` and their
 * container carries no path at all. Asking for whichever exists beats asking every caller
 * to spell the key, which would put the answer in nine places instead of one.
 */
export interface MidsPowersetSource {
  setPath?: string;
  powers?: ReadonlyArray<{ fullName?: string }>;
}

/** Every `group.powerset` key these sources represent, lower-cased as the map is keyed. */
function powersetKeysOf(sources: ReadonlyArray<MidsPowersetSource | null | undefined>): string[] {
  const keys = new Set<string>();
  for (const source of sources) {
    if (!source) continue;
    const path = source.setPath ?? source.powers?.find((p) => p.fullName)?.fullName ?? '';
    const segments = path.split('.');
    if (segments.length >= 2) keys.add(`${segments[0]}.${segments[1]}`.toLowerCase());
  }
  return [...keys];
}

/**
 * True when `powersetPath`'s own table shows this Mids name belongs to a DIFFERENT power
 * here and offers the entry no counterpart of its own — the shape of a power HC retired.
 *
 * Stalker Willpower is the case: Mids' `Reconstruction` is a heal click the set no longer
 * has, while the name now labels the rez. Without this the entry falls past the powerset
 * that rejected it into the cross-archetype and brute-force fallbacks, and binds
 * Regeneration's `Reconstruction` — a power from a set the character never took. A wrong
 * answer that looks deliberate is worse than none, so the .mbd naming the powerset is
 * taken as final: this name, in this set, resolves to nothing.
 */
export function midsNameIsRetired(powersetPath: string, midsName: string): boolean {
  if (midsNameRemap(powersetPath, midsName)) return false;
  const owner = midsNameOwners(powersetPath).get(midsName.toLowerCase());
  return owner !== undefined && owner !== midsName.toLowerCase();
}

/**
 * Find a power within a list of Power definitions by Mids internal name.
 *
 * Strategy:
 *   0. Consult the DERIVED Mids→dataset name map FIRST (MBDIMPORT-2), scoped to the powersets
 *      `sources` names. It has to run ahead
 *      of the exact match, because the bug it closes is an exact match that SUCCEEDS on
 *      the wrong power: HC rotated internal names under stable display names, so Tactical
 *      Arrow's `Gymnastics` resolves to Oil Slick Arrow and takes its slots, and the entry
 *      that owned them is deduped away. A matcher can only be ordered by reliability, and
 *      an exact hit on a rotated namespace is not evidence. `sources` is REQUIRED rather
 *      than optional: an optional key is a call-site gate, and a site that omits it loses
 *      the remap silently, which is the failure shape this whole path exists to end.
 *   1. Try every matcher on the original Mids name (exact, display, normalized, fullName-tail).
 *   2. Only if all of those fail, apply MIDS_NAME_TYPOS and retry the exact-match
 *      lookup with the renamed target. This keeps renames safe in powersets
 *      where the old name is still valid.
 */
export function findPowerByMidsName(
  powers: Power[],
  midsName: string,
  sources: ReadonlyArray<MidsPowersetSource | null | undefined>,
): Power | null {
  const keys = powersetKeysOf(sources);
  const lowerMidsName = midsName.toLowerCase();

  for (const key of keys) {
    const remapped = midsNameRemap(key, midsName);
    if (!remapped) continue;
    const target = powers.find(
      (p) => p.internalName?.toLowerCase() === remapped.toLowerCase(),
    );
    if (target) return target;
  }

  // Powers another Mids name owns, withheld from every matcher below. Their rightful
  // claimant already had its turn in the loop above, so nothing legitimate is being
  // hidden — what is being refused is a same-spelled name arriving from a namespace
  // where it meant something else.
  const spokenFor = new Set<string>();
  for (const key of keys) {
    for (const [ourName, owner] of midsNameOwners(key)) {
      if (owner !== lowerMidsName) spokenFor.add(ourName);
    }
  }
  const available = spokenFor.size === 0
    ? powers
    : powers.filter((p) => !spokenFor.has((p.internalName ?? '').toLowerCase()));

  const direct = tryMatch(available, midsName);
  if (direct) return direct;

  const renamed = MIDS_NAME_TYPOS[midsName.toLowerCase()];
  if (renamed && renamed.toLowerCase() !== midsName.toLowerCase()) {
    const viaRename = available.find(
      (p) => p.internalName?.toLowerCase() === renamed.toLowerCase(),
    );
    if (viaRename) {
      warnFallback('findPowerByMidsName', `'${midsName}' mapped via rename → '${viaRename.name}' (internalName '${viaRename.internalName}')`);
      return viaRename;
    }
  }

  return null;
}

// ============================================
// POOL MAPPING
// ============================================

export interface PoolPowerMatch {
  poolId: string;
  poolName: string;
  power: Power;
}

/**
 * Build a lookup from pool power fullNames to pool info.
 * Key: fullName like "Pool.Speed.Hasten"
 * Value: { poolId, poolName, power }
 */
export function buildPoolLookup(): Map<string, PoolPowerMatch> {
  const lookup = new Map<string, PoolPowerMatch>();
  const poolIds = getPowerPoolIds();

  for (const poolId of poolIds) {
    const pool = getPowerPool(poolId);
    if (!pool) continue;

    for (const power of pool.powers) {
      if (power.fullName) {
        lookup.set(power.fullName, { poolId, poolName: pool.name, power });
      }
    }
  }

  return lookup;
}

/**
 * Mids pool name → app pool ID aliases for pools whose names diverge.
 * Keys are lowercase Mids pool names (after "Pool." prefix).
 */
const MIDS_POOL_ALIASES: Record<string, string> = {
  // HC renamed Presence → Manipulation in newer exports; our app still uses "presence".
  manipulation: 'presence',
};

/**
 * Resolve a Mids pool powerset path to an app pool ID.
 * @param midsPath - e.g., "Pool.Fighting" or "Pool.Force_of_Will"
 */
export function resolvePoolId(midsPath: string): string | null {
  const segments = midsPath.split('.').map(s => s.trim());
  if (segments.length < 2 || segments[0] !== 'Pool') return null;

  // "Force_of_Will" → "force_of_will"
  const rawId = segments[1].toLowerCase();
  return MIDS_POOL_ALIASES[rawId] ?? rawId;
}

// ============================================
// EPIC POOL MAPPING
// ============================================

export interface EpicPowerMatch {
  epicPoolId: string;
  epicPoolName: string;
  power: Power;
}

/**
 * Build a lookup from epic power fullNames to epic pool info.
 * Key: fullName like "Epic.Energy_Mastery_Brute.Focused_Accuracy"
 * Value: { epicPoolId, epicPoolName, power }
 */
export function buildEpicLookup(archetypeId: string, additionalPoolIds?: string[]): Map<string, EpicPowerMatch> {
  const lookup = new Map<string, EpicPowerMatch>();
  const epicPools = getEpicPoolsForArchetype(archetypeId);

  // Include any explicitly resolved pool IDs that weren't found via archetype filter
  if (additionalPoolIds) {
    for (const poolId of additionalPoolIds) {
      if (!epicPools.some((p) => p.id === poolId)) {
        const pool = getEpicPool(poolId);
        if (pool) epicPools.push(pool);
      }
    }
  }

  for (const pool of epicPools) {
    for (const power of pool.powers) {
      if (power.fullName) {
        lookup.set(power.fullName, {
          epicPoolId: pool.id,
          epicPoolName: pool.name,
          power,
        });
      }
    }
  }

  return lookup;
}

/**
 * Mids sometimes abbreviates words in epic pool names.
 * e.g., "Sentinel_Psi_Mastery" instead of "Sentinel_Psionic_Mastery".
 * Maps lowercase abbreviated word → full word.
 */
const MIDS_WORD_ABBREVIATIONS: Record<string, string> = {
  'psi': 'psionic',
  'elec': 'electricity',
};

function expandMidsAbbreviations(name: string): string {
  const words = name.split('_');
  const expanded = words.map(w => MIDS_WORD_ABBREVIATIONS[w] || w);
  return expanded.join('_');
}

/**
 * Mids uses AT abbreviation suffixes on epic pool internal names.
 * e.g., "Ice_Mastery_DefCorr" = Ice Mastery for Defenders/Corruptors.
 * Maps lowercase suffix → AT IDs to try when constructing pool ID.
 */
const MIDS_EPIC_AT_SUFFIXES: Record<string, string[]> = {
  '_defcorr': ['defender'],
  '_def': ['defender'],
  '_corr': ['corruptor'],
  '_brute': ['brute', 'tanker', 'tank'],
  '_tank': ['tanker', 'tank'],
  // Shared Tank/Brute pools (e.g. Psionic Mastery): our data uses the `tank_` prefix.
  '_tankbrute': ['tank', 'tanker', 'brute'],
  '_scrap': ['scrapper', 'melee'],
  '_stalk': ['stalker', 'scrapper', 'melee'],
  // Shared Scrapper/Stalker pools (e.g. Psionic Mastery): our data uses the `melee_` prefix.
  '_scrapstalk': ['melee', 'scrapper', 'stalker'],
  '_blast': ['blaster'],
  '_sent': ['sentinel', 'blaster'],
  '_cont': ['controller'],
  '_dom': ['dominator', 'controller'],
  '_mm': ['mastermind', 'defender'],
  // Pool name ends with the full AT name (e.g. "Dark_Mastery_Mastermind"
  // maps to `mastermind_dark_mastery`).
  '_mastermind': ['mastermind'],
  '_defender': ['defender'],
  '_corruptor': ['corruptor'],
  '_dominator': ['dominator'],
  '_controller': ['controller'],
  '_blaster': ['blaster'],
  '_tanker': ['tanker', 'tank'],
  '_scrapper': ['scrapper'],
  '_stalker': ['stalker'],
  '_sentinel': ['sentinel'],
};

/**
 * Mids also uses AT-abbreviation PREFIXES on some epic pool names.
 * e.g., "Corr_Flame_Mastery" (Corruptor Flame Mastery) → `flame_mastery`.
 * Maps lowercase prefix → AT IDs to try when constructing pool ID.
 */
const MIDS_EPIC_AT_PREFIXES: Record<string, string[]> = {
  'def_': ['defender'],
  'corr_': ['corruptor'],
  'brute_': ['brute', 'tanker', 'tank'],
  'tank_': ['tanker', 'tank'],
  'scrap_': ['scrapper', 'melee'],
  'stalk_': ['stalker', 'scrapper', 'melee'],
  'blast_': ['blaster'],
  'sent_': ['sentinel', 'blaster'],
  'sentinel_': ['sentinel', 'blaster'],
  'cont_': ['controller'],
  'dom_': ['dominator', 'controller'],
  'mm_': ['mastermind', 'defender'],
  'mastermind_': ['mastermind'],
};

/**
 * Direct Mids → app epic-pool ID overrides. These cover cases where the AT
 * prefix/suffix logic picks the wrong pool due to HC set renames.
 * Key is the lowercased Mids pool name (after stripping `Epic.`).
 */
const MIDS_EPIC_POOL_OVERRIDES: Record<string, string> = {
  // HC renamed the Corruptor/Defender epic pool from "Flame Mastery" to
  // "Fire Mastery". Mids still uses the old name.
  'corr_flame_mastery': 'corruptor_fire_mastery',
  'def_flame_mastery': 'defender_fire_mastery',
  // Rebirth Guardian — Mids puts the AT after the school
  // ("Psionic_Mastery_Guardian"); our generated id puts the AT first
  // ("guardian_psionic_mastery"). Map all Guardian epic pools.
  'fire_mastery_guardian': 'guardian_fire_mastery',
  'ice_mastery_guardian': 'guardian_ice_mastery',
  'leviathan_mastery_guardian': 'guardian_leviathan_mastery',
  'mace_mastery_guardian': 'guardian_mace_mastery',
  'mu_mastery_guardian': 'guardian_mu_mastery',
  'munitions_mastery_guardian': 'guardian_munitions_mastery',
  'primal_forces_mastery_guardian': 'guardian_primal_forces_mastery',
  'psionic_mastery_guardian': 'guardian_psionic_mastery',
  'soul_mastery_guardian': 'guardian_soul_mastery',
};

/**
 * Resolve a Mids epic powerset path to an app epic pool ID.
 * @param midsPath - e.g., "Epic.Energy_Mastery_Brute"
 * @param archetypeId - the mapped archetype ID
 */
export function resolveEpicPoolId(
  midsPath: string,
  archetypeId: string,
): string | null {
  const segments = midsPath.split('.').map(s => s.trim());
  if (segments.length < 2 || segments[0] !== 'Epic') return null;

  // "Energy_Mastery_Brute" → "energy_mastery_brute"
  const midsEpicName = segments[1].toLowerCase();

  // Direct override for known Mids/HC naming divergences (set renames).
  const override = MIDS_EPIC_POOL_OVERRIDES[midsEpicName];
  if (override && getEpicPool(override)) return override;

  // Try direct match with the epic pool IDs for this archetype
  const epicPools = getEpicPoolsForArchetype(archetypeId);
  for (const pool of epicPools) {
    if (pool.id.toLowerCase() === midsEpicName) {
      return pool.id;
    }
  }

  // Try matching without archetype suffix (e.g., "energy_mastery_brute" vs "energy_mastery")
  for (const pool of epicPools) {
    if (midsEpicName.startsWith(pool.id.toLowerCase()) ||
        pool.id.toLowerCase().startsWith(midsEpicName)) {
      return pool.id;
    }
  }

  // Fallback: direct ID lookup bypassing archetype filter
  const directPool = getEpicPool(midsEpicName);
  if (directPool) return directPool.id;

  // Fallback: strip Mids AT abbreviation suffix and try {at}_{baseName} pattern
  // This must run BEFORE the broad startsWith search to avoid e.g. "ice_mastery_defcorr"
  // matching "ice_mastery" (blaster pool) instead of "defender_ice_mastery".
  const allEpicPools = getAllEpicPools();
  for (const [suffix, ats] of Object.entries(MIDS_EPIC_AT_SUFFIXES)) {
    if (midsEpicName.endsWith(suffix)) {
      const baseName = midsEpicName.slice(0, -suffix.length);
      for (const at of ats) {
        const candidateId = `${at}_${baseName}`;
        for (const pool of Object.values(allEpicPools)) {
          if (pool.id.toLowerCase() === candidateId) {
            return pool.id;
          }
        }
      }
      // Also try baseName alone
      for (const pool of Object.values(allEpicPools)) {
        if (pool.id.toLowerCase() === baseName) {
          return pool.id;
        }
      }
    }
  }

  // Also try AT-abbreviation PREFIXES (e.g. "corr_flame_mastery" → try
  // `corruptor_flame_mastery` or just `flame_mastery`).
  for (const [prefix, ats] of Object.entries(MIDS_EPIC_AT_PREFIXES)) {
    if (midsEpicName.startsWith(prefix)) {
      const baseName = midsEpicName.slice(prefix.length);
      for (const at of ats) {
        const candidateId = `${at}_${baseName}`;
        for (const pool of Object.values(allEpicPools)) {
          if (pool.id.toLowerCase() === candidateId) {
            return pool.id;
          }
        }
      }
      for (const pool of Object.values(allEpicPools)) {
        if (pool.id.toLowerCase() === baseName) {
          return pool.id;
        }
      }
    }
  }

  // Fallback: search ALL epic pools for a match
  for (const pool of Object.values(allEpicPools)) {
    if (pool.id.toLowerCase() === midsEpicName) {
      return pool.id;
    }
  }
  for (const pool of Object.values(allEpicPools)) {
    if (midsEpicName.startsWith(pool.id.toLowerCase()) ||
        pool.id.toLowerCase().startsWith(midsEpicName)) {
      return pool.id;
    }
  }

  // Fallback: expand known Mids abbreviations (e.g., "psi" → "psionic") and retry
  const expandedName = expandMidsAbbreviations(midsEpicName);
  if (expandedName !== midsEpicName) {
    for (const pool of epicPools) {
      if (pool.id.toLowerCase() === expandedName) {
        return pool.id;
      }
    }
    for (const pool of Object.values(allEpicPools)) {
      if (pool.id.toLowerCase() === expandedName ||
          expandedName.startsWith(pool.id.toLowerCase()) ||
          pool.id.toLowerCase().startsWith(expandedName)) {
        return pool.id;
      }
    }
  }

  return null;
}

// ============================================
// ENHANCEMENT MAPPING
// ============================================

/**
 * Dev-name aliases: maps internal/development set names to their live set IDs.
 * HC sometimes ships sets with dev names that differ from the final display name.
 */
const DEV_NAME_ALIASES: Record<string, string> = {
  'shrapnel': 'artillery',
};

/**
 * Build a reverse lookup from IO set display names (normalized) to set IDs.
 * Used as fallback when UID-based matching fails.
 */
function buildIOSetNameLookup(): Map<string, string> {
  const lookup = new Map<string, string>();
  const allSets = getAllIOSets();
  for (const [id, set] of Object.entries(allSets)) {
    // Normalize: "Brute's Fury" → "brutes_fury" (strip apostrophes, lowercase, spaces to underscores)
    const normalized = set.name
      .toLowerCase()
      .replace(/['']/g, '')
      .replace(/\s+/g, '_');
    lookup.set(normalized, id);

    // Also store with hyphens removed (Mids strips hyphens: "Fire-Control" → "FireControl")
    const noHyphens = normalized.replace(/-/g, '');
    if (noHyphens !== normalized) {
      lookup.set(noHyphens, id);
    }

    // Always key by the set ID itself — catches cases where our ID spelling
    // diverges from the display-name spelling (e.g. `superior_ascendency_of_the_dominator`
    // has display "Superior Ascendancy of the Dominator" but Mids sends the ID spelling).
    lookup.set(id, id);
    const idNoHyphens = id.replace(/-/g, '');
    if (idNoHyphens !== id) {
      lookup.set(idNoHyphens, id);
    }
  }

  // Add dev-name aliases (internal names that differ from live names)
  for (const [devName, setId] of Object.entries(DEV_NAME_ALIASES)) {
    lookup.set(devName, setId);
  }

  return lookup;
}

// Cache the name lookup
let _ioSetNameLookup: Map<string, string> | null = null;
function getIOSetNameLookup(): Map<string, string> {
  if (!_ioSetNameLookup) {
    _ioSetNameLookup = buildIOSetNameLookup();
  }
  return _ioSetNameLookup;
}

/**
 * Mids UID suffix → planner stat. Many-to-one: Mids has spelled the same stat
 * several ways across its history, and every spelling has to import.
 *
 * The export path inverts this against the EnhDB roster rather than keeping a
 * second hand-written map — see `midsGenericIOSuffix` in mids-export.ts.
 */
export const MIDS_STAT_MAP: Record<string, string> = {
  'Damage': 'Damage',
  'Accuracy': 'Accuracy',
  'Recharge': 'Recharge',
  'EnduranceReduction': 'EnduranceReduction',
  'Endurance_Reduction': 'EnduranceReduction',
  'Endurance_Discount': 'EnduranceReduction',
  'EndRdx': 'EnduranceReduction',
  'Range': 'Range',
  'Defense': 'Defense',
  'Defense_Buff': 'Defense',
  'Resistance': 'Resistance',
  'Healing': 'Healing',
  'Heal': 'Healing',
  'ToHit': 'ToHit',
  'To_Hit': 'ToHit',
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
  'Recovery': 'EnduranceModification',
  'Recovery_Buff': 'EnduranceModification',
  // Boost-table spellings. Unmapped stats fall through to the raw UID suffix
  // below, which is a valid EnhancementStatType only by luck — these four are
  // the ones where it isn't, so they used to import as a 0-value enhancement
  // wearing the Unknown icon rather than warning.
  'Res_Damage': 'Resistance',
  'Defense_Debuff': 'Defense Debuff',
  'Defense_DeBuff': 'Defense Debuff',
  'ToHit_Debuff': 'ToHit Debuff',
  'ToHit_DeBuff': 'ToHit Debuff',
  'Snare': 'Slow',
  'Intangible': 'Intangible',
  'Interrupt': 'Interrupt',
  'Absorb': 'Absorb',
};

export interface EnhancementMapResult {
  enhancement: Enhancement | null;
  warning: MidsImportWarning | null;
}

/**
 * Parse a Mids `RelativeLevel` (the `eEnhRelative` enum) to a signed level
 * offset. "MinusThree" → -3 … "Even" → 0 … "PlusFive" → 5.
 *
 * The negative half was previously missing, so every `Minus*` fell through the
 * `?? 0` and imported as even. That silently overstated the build: on Homecoming
 * an enhancement three levels under your combat level is worth x0.70, so a
 * levelling build slotted with red SOs read as though every one of them were
 * fresh. Specials had the same hole, but SOs are where it bites — they are what
 * a sub-50 build is actually full of.
 *
 * "None" and anything unrecognised still mean even; Mids writes "None" for an
 * empty slot, which carries no level offset to preserve.
 */
const RELATIVE_LEVEL_OFFSET: Record<string, number> = {
  'MinusThree': -3,
  'MinusTwo': -2,
  'MinusOne': -1,
  'Even': 0,
  'PlusOne': 1,
  'PlusTwo': 2,
  'PlusThree': 3,
  'PlusFour': 4,
  'PlusFive': 5,
};

function parseBoostLevel(relativeLevel: string): number {
  return RELATIVE_LEVEL_OFFSET[relativeLevel] ?? 0;
}

/**
 * Map a Mids enhancement UID to an app Enhancement object.
 * @param uid - e.g., "Superior_Attuned_Superior_Brutes_Fury_A" or "Crafted_Damage"
 * @param ioLevel - the IoLevel from the .mbd file (0-based)
 * @param relativeLevel - "Even", "PlusOne", "PlusTwo", etc.
 * @param grade - "IO", "SO", "DO", "TO", etc.
 */
export function mapEnhancementUid(
  uid: string,
  ioLevel: number,
  relativeLevel: string,
  grade?: string,
): EnhancementMapResult {
  // Defensive: Mids slot entries occasionally have undefined/null Uid values.
  // Treat them as an empty slot rather than crashing.
  if (uid == null || typeof uid !== 'string' || uid.length === 0) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: '(empty)', message: 'Enhancement entry had no Uid' },
    };
  }

  // The ioLevel in .mbd is 0-based (49 = level 50)
  const level = Math.min(Math.max(ioLevel + 1, 1), 53);
  const boost = parseBoostLevel(relativeLevel);

  // Special enhancements (Hamidon, Synthetic HO, Titan, Hydra, D-Sync, Prestige)
  // are identified by their UID prefix and nothing else. Grade cannot do it:
  // Mids grades a Hamidon `SingleO`, and so is every ordinary SO in the game.
  if (uid.startsWith('Synthetic_Hamidon_') || uid.startsWith('Hamidon_') || uid.startsWith('Titan_') || uid.startsWith('Hydra_') || uid.startsWith('DSync_') || uid.startsWith('Dsync_') || uid.startsWith('Generic_')) {
    return mapSpecialEnhancementUid(uid, boost);
  }

  // Origin enhancements (TO/DO/SO), which Mids grades TrainingO/DualO/SingleO
  // and names `<Origin>_<Stat>`.
  const tier = grade ? MIDS_ORIGIN_TIER[grade] : undefined;
  if (tier) {
    const [origin, statUid] = splitOriginUid(uid);
    const stat = MIDS_STAT_MAP[statUid] ?? statUid;
    try {
      const enh = createOriginEnhancement(stat as any, tier, origin, boost || undefined);
      return { enhancement: enh, warning: null };
    } catch {
      return {
        enhancement: null,
        warning: { type: 'enhancement', midsName: uid, message: `Unknown origin enhancement stat: ${statUid}` },
      };
    }
  }

  // Check for generic IOs: "Crafted_Damage", "Crafted_Accuracy", etc.
  if (uid.startsWith('Crafted_')) {
    const statPart = uid.slice('Crafted_'.length);
    // Check if this is a generic IO (no piece letter suffix)
    if (!statPart.match(/_[A-F]$/) || isGenericStat(statPart)) {
      const stat = MIDS_STAT_MAP[statPart] ?? statPart;
      try {
        const enh = createGenericIOEnhancement(stat as any, level, boost || undefined);
        return { enhancement: enh, warning: null };
      } catch {
        return {
          enhancement: null,
          warning: { type: 'enhancement', midsName: uid, message: `Unknown generic IO stat: ${statPart}` },
        };
      }
    }
  }

  // IO Set enhancement. The dataset's own copy of Mids' UID table answers first
  // — it knows the fossil spellings (`Crafted_Shrapnel_*` is Mids' Artillery,
  // `Crafted_Exploit_Weakness_c` is that set's third piece) that reading the UID
  // as text gets wrong. `parseIOSetUid` stays as the fallback for UIDs the table
  // doesn't carry: other forks' files, and sets newer than the vendored EnhDB.
  const resolved = resolveMidsUid(uid);
  const parsed = resolved
    ? { ...parseIOSetUid(uid), ...resolved } as ReturnType<typeof parseIOSetUid>
    : parseIOSetUid(uid);
  if (!parsed) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: uid, message: `Could not parse enhancement UID` },
    };
  }

  let { setId, pieceNum, attuned } = parsed;

  // For Superior_Attuned_ UIDs where the set name doesn't already include "superior_",
  // prefer the superior variant (e.g., "blistering_cold" → "superior_blistering_cold").
  // Mids uses "Superior_Attuned_Blistering_Cold" for winter sets but our data stores
  // the superior version as "superior_blistering_cold". For ATOs like Brutes Fury,
  // Mids embeds the "Superior_" in the set name itself, so setId is already correct.
  let ioSet;
  if (parsed.superior && !setId.startsWith('superior_')) {
    const superiorId = `superior_${setId}`;
    const superiorSet = getIOSet(superiorId);
    if (superiorSet) {
      ioSet = superiorSet;
      setId = superiorId;
    }
  }
  if (!ioSet) {
    ioSet = getIOSet(setId);
  }

  // Fallback: try name-based lookup
  if (!ioSet) {
    const nameLookup = getIOSetNameLookup();
    const fallbackId = nameLookup.get(setId);
    if (fallbackId) {
      ioSet = getIOSet(fallbackId);
    }
  }

  // Special-suffix UIDs (e.g. Mids' "Superior_Return_From_the_Grave_Rez_Effects"
  // for special non-letter pieces) keep the trailing descriptor in setId.
  // Peel back one underscore-segment at a time until we find a real set,
  // capping at 4 strips so we don't accidentally collide with a different set.
  if (!ioSet && setId.includes('_')) {
    let trimmed = setId;
    for (let i = 0; i < 4; i++) {
      const idx = trimmed.lastIndexOf('_');
      if (idx <= 0) break;
      trimmed = trimmed.slice(0, idx);
      const candidate = getIOSet(trimmed);
      if (candidate) {
        ioSet = candidate;
        setId = trimmed;
        break;
      }
      // Also try the apostrophe-stripped fallback table — keeps us robust
      // if the trailing segments included punctuation.
      const fb = getIOSetNameLookup().get(trimmed);
      if (fb) {
        ioSet = getIOSet(fb);
        setId = fb;
        break;
      }
    }
  }

  if (!ioSet) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: uid, message: `IO set not found: ${setId}` },
    };
  }

  // Find the piece
  const piece = ioSet.pieces.find((p) => p.num === pieceNum);
  if (!piece) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: uid, message: `Piece ${pieceNum} not found in set ${ioSet.name}` },
    };
  }

  const enh = createIOSetEnhancement(ioSet, piece, pieceNum - 1, {
    attuned,
    level: attuned ? 50 : level,
    boost: boost || undefined,
  });

  return { enhancement: enh, warning: null };
}

// ============================================
// LEGACY ENHANCEMENT FORMAT (pre-2024 Mids)
// ============================================

/**
 * Normalize an IO set piece display name so minor divergences between data
 * sources (Mids, HC binary, our app data) collapse to the same key.
 *
 * Handles:
 *  - `Resistance` ↔ `Damage Resistance`
 *  - `+End` ↔ `+Endurance`, `+HP` ↔ `+Hit Points` ↔ `+Health`
 *  - `Increased Global Recharge Speed` ↔ `+Recharge`
 *  - `RechargeTime` ↔ `Recharge`, `Endurance Reduction` ↔ `Endurance`
 *  - `Chance for/of/to X` collapses
 *  - `Damage(Negative)` ↔ `Damage(Negative Energy)` — strip " Energy"
 *  - `Knockback Reduction (N points)` ↔ `Knockback Protection`
 *  - `Scaling Resist Damage` ↔ `+Res(All)`
 *  - `TP Protection +3% Def (All)` ↔ `+Def(All)`
 */
function normalizePieceName(name: string): string {
  let n = name.toLowerCase().trim();

  // Strip apostrophes and curly quotes.
  n = n.replace(/['']/g, '');

  // Canonical slash separator (no spaces around "/").
  n = n.replace(/\s*\/\s*/g, '/');

  // Recharge phrasings.
  n = n.replace(/increased global recharge speed/g, '+recharge');
  n = n.replace(/\bglobal recharge\b/g, '+recharge');
  n = n.replace(/\brechargetime\b/g, 'recharge');

  // Chance-for-X collapses.
  n = n.replace(/\bchance (for|to|of)\s+/g, 'chance ');

  // Endurance phrasings.
  n = n.replace(/\bendurance reduction\b/g, 'endurance');
  n = n.replace(/\bend mod\b/g, 'endmod');
  n = n.replace(/\bendurance modification\b/g, 'endmod');
  n = n.replace(/\+endurance\b/g, '+end');
  n = n.replace(/\+end\b/g, '+end');

  // Health / HP aliases.
  n = n.replace(/\+hit points\b/g, '+hp');
  n = n.replace(/\+health\b/g, '+hp');
  n = n.replace(/heal self\b/g, '+hp');

  // Damage type expansions → short forms.
  n = n.replace(/negative energy/g, 'negative');

  // Knockback Reduction (N points) / Knockback Protection → kb protection.
  n = n.replace(/knockback reduction\s*\(\d+\s*points?\)/g, 'knockback protection');
  n = n.replace(/knockback reduction/g, 'knockback protection');

  // Scaling Resist Damage → +res(all).
  n = n.replace(/scaling resist damage/g, '+res(all)');
  n = n.replace(/scaling resistance/g, '+res(all)');

  // Shield Wall: "+Res (Teleportation), +5% Res (All)" → +res(all)
  n = n.replace(/\+res\s*\(teleportation\),\s*\+\d+%\s*res\s*\(all\)/g, '+res(all)');
  n = n.replace(/\+res\s*\(all\)/g, '+res(all)');

  // Gladiator's Armor: "TP Protection +3% Def (All)" → +def(all)
  n = n.replace(/tp protection\s*\+\d+%\s*def\s*\(all\)/g, '+def(all)');
  n = n.replace(/\+def\s*\(all\)/g, '+def(all)');
  n = n.replace(/\+def\s*\d+%\s*/g, '+def(all) ');
  n = n.replace(/resistance\/\+def\s*\d+%?/g, 'damage resistance/+def(all)');

  // Max HP aliases.
  n = n.replace(/\+max hp\b/g, '+max hitpoints');
  n = n.replace(/\+max hitpoints\b/g, '+max hitpoints');

  // Strip "damage " prefix from each slash-segment (Aegis / Unbreakable Guard).
  n = n.split('/').map((part) => part.replace(/^damage\s+/, '').trim()).join('/');

  // Parenthetical spacing.
  n = n.replace(/\s*\(\s*/g, '(').replace(/\s*\)\s*/g, ')');

  // Collapse whitespace runs.
  n = n.replace(/\s+/g, ' ').trim();

  return n;
}

/** Single-word aspect aliases used by parsePieceAspects. */
const ASPECT_WORD_ALIASES: Record<string, string> = {
  'accuracy': 'accuracy',
  'acc': 'accuracy',
  'damage': 'damage',
  'dam': 'damage',
  'dmg': 'damage',
  'endurance': 'endurance',
  'end': 'endurance',
  'endmod': 'endmod',
  'endurancemod': 'endmod',
  'enduranceamod': 'endmod',
  'endurancemodification': 'endmod',
  'recharge': 'recharge',
  'rech': 'recharge',
  'rechargetime': 'recharge',
  'range': 'range',
  'defense': 'defense',
  'def': 'defense',
  'resistance': 'resistance',
  'res': 'resistance',
  'damageresistance': 'resistance',
  'heal': 'heal',
  'healing': 'heal',
  'absorb': 'absorb',
  'tohit': 'tohit',
  'tohitbuff': 'tohit',
  'tohitdebuff': '-tohit',
  '-tohit': '-tohit',
  // Mez aliases — Mids uses past tense (Confused, Stunned, Held, Slept); our data uses infinitive.
  'holdduration': 'hold',
  'hold': 'hold',
  'held': 'hold',
  'immobilize': 'immobilize',
  'immobilizeduration': 'immobilize',
  'immob': 'immobilize',
  'immobilized': 'immobilize',
  'stun': 'stun',
  'stunduration': 'stun',
  'stunned': 'stun',
  'sleep': 'sleep',
  'sleepduration': 'sleep',
  'slept': 'sleep',
  'fear': 'fear',
  'fearduration': 'fear',
  'feared': 'fear',
  'terrorized': 'fear',
  'confuse': 'confuse',
  'confused': 'confuse',
  'confuseduration': 'confuse',
  'taunt': 'taunt',
  'tauntduration': 'taunt',
  'taunted': 'taunt',
  'placate': 'placate',
  'placated': 'placate',
  'threat': 'taunt',
  'slow': 'slow',
  'slowmovement': 'slow',
  'knockback': 'knockback',
  'flight': 'flight',
  'flightspeed': 'flight',
  'jumping': 'jump',
  'jump': 'jump',
  'running': 'run',
  'runspeed': 'run',
  'interrupt': 'interrupt',
  'interrupttime': 'interrupt',
  // ATO-specific bonus aspect names. Mids uses verbose names; our data uses
  // terser "+X%" forms. All collapse to the same `atobonus` canonical key.
  'criticalhitbonus': 'atobonus',
  'criticalhit': 'atobonus',
  'furybonus': 'atobonus',
  'fury': 'atobonus',
  'buildupproc': 'atobonus',
  'buildup': 'atobonus',
  'rchbuildup': 'atobonus',
  'energyfont': 'atobonus',
  'fieryorb': 'atobonus',
  'dominationbonus': 'atobonus',
  'domination': 'atobonus',
  'containmentproc': 'atobonus',
  'containment': 'atobonus',
  'assassinbonus': 'atobonus',
  'assassination': 'atobonus',
  'gauntletbonus': 'atobonus',
  'gauntlet': 'atobonus',
  'minionbonus': 'atobonus',
  'petbonus': 'atobonus',
  'petresistregen': 'atobonus',
  'petaoedefenseaura': 'atobonus',
  'petdefenseaura': 'atobonus',
  'chanceofdamage': 'atobonus',
  // Mids "Control Duration" is the same concept as our data's generic "Mez"
  // aspect on Dominator ATO sets (Overpowering Presence, Dominating Grasp,
  // Will of the Controller, Ascendency of the Dominator). Map both to `mez`,
  // and in the aspect matcher treat `mez` as a wildcard for any specific mez
  // aspect (Confuse/Hold/etc.) in case a set uses a specific one instead.
  'controlduration': 'mez',
  'mez': 'mez',
};

/** Mez-type canonical aspect names that the `mez` wildcard should match. */
const MEZ_ASPECT_NAMES = new Set(['confuse', 'hold', 'immobilize', 'stun', 'sleep', 'fear', 'taunt', 'placate', 'slow', 'mez']);

/**
 * Check if a display-name-only enhancement is a special enhancement
 * (Hamidon Exposure, Titan, Hydra, D-Sync).
 */
function isSpecialEnhancementName(name: string): boolean {
  return /\b(exposure|origin|nucleus)\b/i.test(name)
    || name.startsWith('Titan ')
    || name.startsWith('Hydra ')
    || name.startsWith('D-Sync ');
}

/**
 * Look up a special enhancement (Hamidon/Titan/Hydra/D-Sync/Prestige) by display name.
 */
function findSpecialByDisplayName(displayName: string, boost: number): Enhancement | null {
  const needle = displayName.toLowerCase().trim();
  const categories: SpecialCategory[] = ['hamidon', 'titan', 'hydra', 'd-sync', 'prestige'];
  for (const category of categories) {
    for (const [id, def] of Object.entries(getSpecialRegistry(category))) {
      if (def.name.toLowerCase() === needle) {
        try {
          return createSpecialEnhancement(id, def, category, boost || undefined);
        } catch {
          return null;
        }
      }
    }
  }
  return null;
}

/**
 * Parse a piece display name into a canonical set of aspect words, stripping
 * apostrophes, normalizing slashes, and mapping common synonyms so that
 * "Damage/Endurance/Accuracy/RechargeTime" and "Accuracy/Damage/Endurance/Recharge"
 * produce the same aspect set.
 *
 * Returns an empty array for proc/special pieces whose names don't follow the
 * slash-separated aspect convention.
 */
function parsePieceAspects(name: string): string[] {
  // Skip if this looks like a true proc or KB/teleport special.
  // Note: parentheses alone don't signal a proc — "+Res(All)" is just a
  // formatting convention for ATO-bonus pieces.
  if (/\bchance\b|\bscaling\b|\bknockback (protection|reduction)\b|\breduction \(/i.test(name)) {
    return [];
  }

  const segments = name
    .toLowerCase()
    .replace(/['']/g, '')
    .split(/\s*\/\s*/)
    .map((s) => s.trim())
    .filter(Boolean);

  if (segments.length === 0) return [];

  const aspects: string[] = [];
  for (const seg of segments) {
    // Segments starting with `+` (e.g. "+Regeneration", "+Res(All)",
    // "+Critical Hit%") or containing "Pet " are ATO-bonus aspects — collapse
    // them into a single `atobonus` so our "Endurance/+Regen/+Res(All)" piece
    // matches Mids' "Endurance/Pet +Resist +Regen".
    if (seg.startsWith('+') || /\bpet\b/.test(seg) || /^-/.test(seg)) {
      aspects.push('atobonus');
      continue;
    }
    // Normalize: strip non-alphanumeric runs.
    const key = seg.replace(/[^a-z0-9-]+/g, '');
    if (!key) return [];
    aspects.push(ASPECT_WORD_ALIASES[key] ?? key);
  }

  // Dedupe atobonus (multi-segment bonus aspects collapse to one).
  const deduped: string[] = [];
  let atobonusSeen = false;
  for (const a of aspects) {
    if (a === 'atobonus') {
      if (atobonusSeen) continue;
      atobonusSeen = true;
    }
    deduped.push(a);
  }

  deduped.sort();
  return deduped;
}

/**
 * Older Mids exports (e.g. 3.5.x, DB 2023.x) store enhancements by display
 * name inside an inner `Enhancement` string field, not a `Uid`. Format is
 * `"Set Name: Piece Name"`, e.g. `"Blood Mandate: Accuracy/Damage"`. Generic
 * IOs use `"Invention: Stat"`. This parser resolves those to app enhancements
 * via the display-name lookups already used elsewhere in the mapper.
 */
export function mapEnhancementByDisplayName(
  displayName: string,
  ioLevel: number,
  relativeLevel: string,
  grade?: string,
): EnhancementMapResult {
  if (typeof displayName !== 'string' || displayName.length === 0) {
    return { enhancement: null, warning: null };
  }

  const level = Math.min(Math.max(ioLevel + 1, 1), 53);
  const boost = parseBoostLevel(relativeLevel);

  // Special enhancements (Hamidon, Titan, Hydra, D-Sync). Mids encodes these
  // with Grade='SingleO' and the full display name in the Enhancement string
  // (e.g. "Membrane Exposure").
  if (grade === 'SingleO' || isSpecialEnhancementName(displayName)) {
    const enh = findSpecialByDisplayName(displayName, boost);
    if (enh) return { enhancement: enh, warning: null };
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: displayName, message: `Unknown special enhancement: ${displayName}` },
    };
  }

  // Split on the first ": " (sets may contain colons in piece names like "Chance for +End").
  const splitIdx = displayName.indexOf(': ');
  if (splitIdx < 0) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: displayName, message: `Legacy enhancement lacks "Set: Piece" format` },
    };
  }

  const setNameRaw = displayName.slice(0, splitIdx).trim();
  const pieceNameRaw = displayName.slice(splitIdx + 2).trim();

  // Generic IOs: "Invention: Accuracy"
  if (setNameRaw.toLowerCase() === 'invention') {
    const stat = MIDS_STAT_MAP[pieceNameRaw.replace(/\s+/g, '_')]
      ?? MIDS_STAT_MAP[pieceNameRaw]
      ?? pieceNameRaw.replace(/\s+/g, '_');
    try {
      const enh = createGenericIOEnhancement(stat as any, level, boost || undefined);
      return { enhancement: enh, warning: null };
    } catch {
      return {
        enhancement: null,
        warning: { type: 'enhancement', midsName: displayName, message: `Unknown generic IO stat: ${pieceNameRaw}` },
      };
    }
  }

  // Set IO: look up set by display name, then piece by display name.
  // Apply known spelling fixes before normalizing (e.g. Mids misspells
  // "Convalescence" as "Convalesence").
  const setNameFixed = setNameRaw.replace(/Convalesence/gi, 'Convalescence');

  const normalizedSet = setNameFixed
    .toLowerCase()
    .replace(/['']/g, '')
    .replace(/\s+/g, '_');

  const nameLookup = getIOSetNameLookup();
  let setId = nameLookup.get(normalizedSet);
  if (!setId) {
    // Try without hyphens (same normalization the lookup builder uses).
    setId = nameLookup.get(normalizedSet.replace(/-/g, ''));
  }
  if (!setId) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: displayName, message: `Legacy set not found: ${setNameRaw}` },
    };
  }

  const ioSet = getIOSet(setId);
  if (!ioSet) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: displayName, message: `Set resolved to ${setId} but not retrievable` },
    };
  }

  // Piece-name resolution. Try in order:
  //   1. Auto-generated alias table keyed by HC display name.
  //   2. Normalized matcher (handles common word aliases).
  //   3. Aspect-set matching for compound pieces (e.g. "Damage/Endurance/Accuracy").
  //   4. Proc detection: if the Mids name looks like a proc ("Chance for/to/of",
  //      "+X"), fall back to the set's unique proc piece.
  let piece = null as (typeof ioSet.pieces)[number] | null;

  const aliasKey = `${setId}\u0000${pieceNameRaw.toLowerCase().trim()}`;
  const aliasPieceNum = LEGACY_PIECE_ALIASES[aliasKey];
  if (aliasPieceNum != null) {
    piece = ioSet.pieces.find((p) => p.num === aliasPieceNum) ?? null;
  }

  if (!piece) {
    const pieceNorm = normalizePieceName(pieceNameRaw);
    piece = ioSet.pieces.find(
      (p) => normalizePieceName(p.name) === pieceNorm,
    ) ?? null;
  }

  // Aspect-set match: compare multiset of aspect words. Handles two wildcards:
  //   - `mez` (from Mids "Control Duration" or our "Mez") matches any mez aspect.
  //   - `atobonus` (from Mids "X Bonus" / "+X%") matches our +X%/ATO-bonus piece.
  if (!piece) {
    const midsAspects = parsePieceAspects(pieceNameRaw);
    if (midsAspects.length > 0) {
      piece = ioSet.pieces.find((p) => {
        const ourAspects = parsePieceAspects(p.name);
        if (ourAspects.length !== midsAspects.length) return false;
        const ourRemaining = [...ourAspects];
        for (const m of midsAspects) {
          let idx = -1;
          if (m === 'mez') {
            idx = ourRemaining.findIndex((o) => MEZ_ASPECT_NAMES.has(o));
          } else if (m === 'atobonus') {
            idx = ourRemaining.findIndex((o) => o === 'atobonus');
          } else {
            idx = ourRemaining.indexOf(m);
          }
          if (idx < 0) return false;
          ourRemaining.splice(idx, 1);
        }
        return ourRemaining.length === 0;
      }) ?? null;
    }
  }

  // Proc fallback: if Mids name looks proc-y and the set has a single proc piece, use it.
  if (!piece) {
    const looksProcy = /\bchance\b|\+\w|\bscaling\b/i.test(pieceNameRaw);
    if (looksProcy) {
      const procPieces = ioSet.pieces.filter((p) => p.proc);
      if (procPieces.length === 1) {
        piece = procPieces[0];
      }
    }
  }

  // Last-resort fuzzy aspect match: same count, at most 1 mismatched aspect.
  // Catches cases like Perfect Zinger's `Threat/Placate/Recharge` (Mids)
  // vs `Range/Recharge/Threat` (ours) — same 3-aspect piece, one aspect
  // labeled differently.
  if (!piece) {
    const midsAspects = parsePieceAspects(pieceNameRaw);
    if (midsAspects.length >= 2) {
      const candidates = ioSet.pieces
        .map((p) => ({
          p,
          ourAspects: parsePieceAspects(p.name),
        }))
        .filter(({ ourAspects }) => ourAspects.length === midsAspects.length);
      const matches = candidates.map(({ p, ourAspects }) => {
        const overlap = ourAspects.filter((a) => midsAspects.includes(a)).length;
        return { p, overlap };
      });
      // Accept if the best candidate has exactly one mismatch and is unique.
      matches.sort((a, b) => b.overlap - a.overlap);
      if (
        matches.length > 0 &&
        matches[0].overlap === midsAspects.length - 1 &&
        (matches.length === 1 || matches[0].overlap > matches[1].overlap)
      ) {
        piece = matches[0].p;
      }
    }
  }

  if (!piece) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: displayName, message: `Piece not found in ${ioSet.name}: ${pieceNameRaw}` },
    };
  }

  // Older format uses `Grade: "None"` for IOs; treat all legacy entries as non-attuned
  // unless the caller explicitly maps them elsewhere.
  const enh = createIOSetEnhancement(ioSet, piece, piece.num - 1, {
    attuned: false,
    level,
    boost: boost || undefined,
  });

  return { enhancement: enh, warning: null };
}

// ============================================
// INTERNAL HELPERS
// ============================================

function isGenericStat(name: string): boolean {
  return name in MIDS_STAT_MAP && !name.match(/_[A-F]$/);
}


// ============================================
// SPECIAL ENHANCEMENT MAPPING (HamiO/Titan/Hydra/D-Sync)
// ============================================

interface SpecialRegistryDef {
  name: string;
  aspects: { stat: string; value: number }[];
}

/**
 * Maps Mids stat-based UID keywords to normalized stat categories.
 * Multiple HamiO aspects can map to the same keyword (e.g., Defense Debuff + ToHit Debuff → "debuff").
 */
const STAT_TO_UID_KEYWORD: Record<string, string> = {
  'accuracy': 'accuracy',
  'damage': 'damage',
  'recharge': 'recharge',
  'endurancereduction': 'endurance_discount',
  'range': 'range',
  'defense': 'defense_buff',
  'resistance': 'resist',
  'healing': 'heal',
  'tohit': 'tohit_buff',
  'defense debuff': 'debuff',
  'tohit debuff': 'debuff',
  'hold': 'mez',
  'stun': 'mez',
  'immobilize': 'mez',
  'sleep': 'mez',
  'confuse': 'mez',
  'fear': 'mez',
  'slow': 'slow',
  'fly': 'travel',
  'jump': 'travel',
  'run speed': 'travel',
  'knockback': 'knockback',
  'taunt': 'taunt',
  'endurancemodification': 'endmod',
  'absorb': 'absorb',
};

/** Multi-word UID keywords to check first (longest match) */
const MULTI_WORD_UID_KEYWORDS = [
  'endurance_discount', 'defense_buff', 'tohit_buff',
];

/** Single-word UID keywords */
const SINGLE_UID_KEYWORDS = [
  'accuracy', 'damage', 'recharge', 'range', 'debuff', 'mez',
  'resist', 'heal', 'slow', 'travel', 'knockback', 'taunt', 'endmod', 'absorb',
];

/** Extract stat keywords from a Mids UID suffix (after stripping prefix) */
function extractUidKeywords(suffix: string): Set<string> {
  const keywords = new Set<string>();
  let remaining = suffix.toLowerCase();

  for (const kw of MULTI_WORD_UID_KEYWORDS) {
    if (remaining.includes(kw)) {
      keywords.add(kw);
      remaining = remaining.replace(kw, '');
    }
  }
  for (const kw of SINGLE_UID_KEYWORDS) {
    if (remaining.includes(kw)) {
      keywords.add(kw);
    }
  }
  return keywords;
}

/** Build expected keyword set from a registry entry's aspects */
function buildExpectedKeywords(aspects: { stat: string }[]): Set<string> {
  const keywords = new Set<string>();
  for (const aspect of aspects) {
    const kw = STAT_TO_UID_KEYWORD[aspect.stat.toLowerCase()];
    if (kw) keywords.add(kw);
  }
  return keywords;
}

function setsEqual(a: Set<string>, b: Set<string>): boolean {
  if (a.size !== b.size) return false;
  for (const item of a) if (!b.has(item)) return false;
  return true;
}

/**
 * Find the best matching registry entry for a UID suffix using keyword-based matching.
 */
function matchByKeywords(
  uidSuffix: string,
  registry: Record<string, SpecialRegistryDef>,
): string | null {
  const inputKw = extractUidKeywords(uidSuffix);
  if (inputKw.size === 0) return null;

  // Try exact keyword set match
  for (const [id, def] of Object.entries(registry)) {
    const expectedKw = buildExpectedKeywords(def.aspects);
    if (setsEqual(inputKw, expectedKw)) return id;
  }

  // Fallback: best partial match (highest overlap ratio)
  let bestId = '';
  let bestScore = 0;
  for (const [id, def] of Object.entries(registry)) {
    const expectedKw = buildExpectedKeywords(def.aspects);
    let matches = 0;
    for (const kw of inputKw) {
      if (expectedKw.has(kw)) matches++;
    }
    const score = matches / Math.max(inputKw.size, expectedKw.size);
    if (score > bestScore) {
      bestScore = score;
      bestId = id;
    }
  }

  return bestId && bestScore >= 0.5 ? bestId : null;
}

/**
 * Direct mapping from Mids UID suffix (lowercased) to registry entry ID.
 * Mids UIDs use stat-based naming like "Damage_Range", "Buff_Endurance_Discount".
 * This table maps those suffixes to the named entries in each enhancement registry.
 *
 * Key Mids UID stat keywords:
 *   Buff = Defense + ToHit aspects
 *   DeBuff = Defense Debuff + ToHit Debuff
 *   Mez = all mez types (Hold/Stun/Immob/Sleep/Confuse/Fear)
 *   Travel = Fly + Jump + Run Speed
 *   Res_Damage = Resistance
 *   Endurance_Discount = EnduranceReduction
 *   Endurance_Modification = EnduranceModification
 *   Threat = Taunt
 *   Heal = Healing (may also include Absorb in some entries)
 */
const SPECIAL_PREFIXES: [string, SpecialCategory][] = [
  // Synthetic_Hamidon_ must come before Hamidon_ so startsWith matches the longer prefix first.
  // Synthetic HOs share the Hamidon registry (identical aspect values in-game).
  ['Synthetic_Hamidon_', 'hamidon'],
  ['Hamidon_', 'hamidon'],
  ['Titan_', 'titan'],
  ['Hydra_', 'hydra'],
  ['DSync_', 'd-sync'],
  ['Dsync_', 'd-sync'],  // Mids sometimes uses lowercase 's'
  ['Generic_', 'prestige'],
];

/**
 * Map a Mids special enhancement UID to an app Enhancement object.
 * Uses keyword-based aspect matching against the known registries.
 */
function mapSpecialEnhancementUid(uid: string, boost?: number): EnhancementMapResult {
  // Determine category from prefix
  let category: SpecialCategory | null = null;
  let suffix = uid;

  for (const [prefix, cat] of SPECIAL_PREFIXES) {
    if (uid.startsWith(prefix)) {
      category = cat;
      suffix = uid.slice(prefix.length);
      break;
    }
  }

  if (!category) {
    return {
      enhancement: null,
      warning: { type: 'enhancement', midsName: uid, message: `Unrecognized special enhancement prefix` },
    };
  }

  const registry = getSpecialRegistry(category);

  // Try direct suffix lookup first (most reliable)
  const suffixMap = SPECIAL_SUFFIX_MAPS[category];
  const directId = suffixMap?.[suffix.toLowerCase()];
  if (directId && registry[directId]) {
    const def = registry[directId];
    const enh = createSpecialEnhancement(directId, def, category, boost);
    return { enhancement: enh, warning: null };
  }

  // Fallback: keyword-based matching for unknown suffixes
  const matchedId = matchByKeywords(suffix, registry);

  if (matchedId) {
    const def = registry[matchedId];
    const enh = createSpecialEnhancement(matchedId, def, category, boost);
    return { enhancement: enh, warning: null };
  }

  return {
    enhancement: null,
    warning: { type: 'enhancement', midsName: uid, message: `Could not match special enhancement: ${uid}` },
  };
}
