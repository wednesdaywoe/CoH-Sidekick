/**
 * Build serialization — slim export (v2) and hydration
 *
 * The v2 format strips all derivable data (power definitions, enhancement details)
 * and keeps only build-specific selections. On import, full objects are reconstructed
 * from the app's data layer.
 */

import type {
  Build,
  SelectedPower,
  Enhancement,
  IOSetEnhancement,
  GenericIOEnhancement,
  SpecialEnhancement,
  OriginEnhancement,
  PoolSelection,
  Power,
  EnhancementStatType,
  EnhancementTier,
  SlimBuildData,
} from '@/types';
import { createEmptyIncarnateBuildState } from '@/types';
import { isDatasetId } from '@/data/dataset';
import {
  getArchetype,
  getPowerset,
  getPowerPool,
  getEpicPool,
  getIOSet,
  getInherentPowers,
  getArchetypeInherentPowers,
  createArchetypeInherentPower,
  createIOSetEnhancement,
  createGenericIOEnhancement,
  createOriginEnhancement,
  createSpecialEnhancement,
  getSpecialRegistry,
  getInherentAutoGrantedSlotCount,
  getInherentGrantLevel,
} from '@/data';
import type { InherentPowerDef } from '@/data';
import type { SpecialCategory } from '@/data';
import { currentInherentName } from '@/data/inherent-aliases';
import { encodeImportFragment } from '@/utils/import-url';

// ============================================
// SLIM TYPES (exported for BuildExport typing)
// ============================================

/**
 * `boost` here is the stored level offset, and it is SIGNED for `origin` and
 * `special` (relative level) while unsigned for the two IO kinds (booster
 * combines) — see `Enhancement.boost`. The writers below gate on truthiness, so
 * a negative persists and an even/unboosted slot stays off the wire; older
 * builds simply have no negatives to read.
 */
export type SlimEnhancement =
  | { type: 'io-set'; setId: string; pieceNum: number; attuned?: boolean; level?: number; boost?: number }
  | { type: 'io-generic'; stat: string; level: number; boost?: number }
  | { type: 'special'; id: string; category: string; boost?: number }
  | { type: 'origin'; stat: string; tier: string; boost?: number };

export interface SlimPower {
  name: string;
  internalName?: string;
  level: number;
  slots: (SlimEnhancement | null)[];
  isActive?: boolean;
  activeSubPower?: string;
  isAutoGranted?: boolean;
  grantedByPower?: string;
  inherentSlotCount?: number;
}

export interface SlimPowersetSelection {
  id: string | null;
  name: string;
  powers: SlimPower[];
}

export interface SlimPoolSelection {
  id: string;
  name: string;
  powers: SlimPower[];
}

// ============================================
// EXPORT: Build → Slim
// ============================================

/**
 * Strip a full Build to slim format for JSON export.
 * Returns a plain object ready for JSON.stringify.
 */
export function slimBuild(build: Build): SlimBuildData {
  return {
    name: build.name,
    serverId: build.serverId,
    archetype: { id: build.archetype.id, name: build.archetype.name },
    level: build.level,
    progressionMode: build.progressionMode,
    primary: slimPowersetSelection(build.primary),
    secondary: slimPowersetSelection(build.secondary),
    pools: build.pools.map(slimPoolSelection),
    epicPool: build.epicPool ? slimPoolSelection(build.epicPool) : null,
    inherents: slimInherents(build.inherents),
    accolades: build.accolades,
    settings: build.settings,
    sets: Object.fromEntries(
      Object.entries(build.sets).map(([setId, tracking]) => [
        setId,
        { count: tracking.count, pieces: Array.from(tracking.pieces) },
      ])
    ),
    incarnates: build.incarnates,
    craftingChecklist: build.craftingChecklist,
    incarnateObtained: build.incarnateObtained,
    shoppingListAcquired: build.shoppingListAcquired,
    slotOrder: build.slotOrder,
    activeModes: build.activeModes,
    attackChains: build.attackChains ?? [],
    procOverrides: build.procOverrides ?? {},
    mutedOverCapStats: build.mutedOverCapStats ?? [],
  };
}

/**
 * slimBuild → JSON → deflate-raw → base64, the encoding used by share links
 * and the `/import` route. Lives here (next to `slimBuild`, store-free) so
 * both `url-build-sync` and `buildStore.importBuild` can reach it without a
 * store import cycle.
 *
 * Strips device-local progress (`craftingChecklist`, `incarnateObtained`,
 * `shoppingListAcquired`) — these reference inventory items, not the build
 * identity, and shouldn't ride along in a shared/reload URL.
 */
export function encodeBuildToHash(build: Build): string {
  const slim = slimBuild(build);
  const { craftingChecklist: _cc, incarnateObtained: _io, shoppingListAcquired: _sl, ...shareable } = slim;
  void _cc; void _io; void _sl;
  return encodeImportFragment(JSON.stringify({ version: 4, build: shareable }));
}

function slimPowersetSelection(ps: { id: string | null; name: string; powers: SelectedPower[] }): SlimPowersetSelection {
  return {
    id: ps.id,
    name: ps.name,
    powers: ps.powers.map(slimPower),
  };
}

function slimPoolSelection(pool: PoolSelection): SlimPoolSelection {
  return {
    id: pool.id,
    name: pool.name,
    powers: pool.powers.map(slimPower),
  };
}

function slimPower(power: SelectedPower): SlimPower {
  const slim: SlimPower = {
    name: power.name,
    internalName: power.internalName,
    level: power.level,
    slots: power.slots.map((slot) => (slot ? slimEnhancement(slot) : null)),
  };
  if (power.isActive !== undefined) slim.isActive = power.isActive;
  if (power.activeSubPower) slim.activeSubPower = power.activeSubPower;
  if (power.isAutoGranted) slim.isAutoGranted = power.isAutoGranted;
  if (power.grantedByPower) slim.grantedByPower = power.grantedByPower;
  if (power.inherentSlotCount) slim.inherentSlotCount = power.inherentSlotCount;
  return slim;
}

function slimEnhancement(enh: Enhancement): SlimEnhancement {
  switch (enh.type) {
    case 'io-set': {
      const e = enh as IOSetEnhancement;
      const slim: SlimEnhancement = { type: 'io-set', setId: e.setId, pieceNum: e.pieceNum };
      if (e.attuned) slim.attuned = true;
      if (e.level !== undefined) slim.level = e.level;
      if (e.boost) slim.boost = e.boost;
      return slim;
    }
    case 'io-generic': {
      const e = enh as GenericIOEnhancement;
      const slim: SlimEnhancement = { type: 'io-generic', stat: e.stat, level: e.level! };
      if (e.boost) slim.boost = e.boost;
      return slim;
    }
    case 'special': {
      const e = enh as SpecialEnhancement;
      // id is "category-registryId", extract registryId
      const registryId = e.id.startsWith(`${e.category}-`)
        ? e.id.slice(e.category.length + 1)
        : e.id;
      const slim: SlimEnhancement = { type: 'special', id: registryId, category: e.category };
      if (e.boost) slim.boost = e.boost;
      return slim;
    }
    case 'origin': {
      const e = enh as OriginEnhancement;
      const slim: SlimEnhancement = { type: 'origin', stat: e.stat, tier: e.tier };
      if (e.boost) slim.boost = e.boost;
      return slim;
    }
  }
}

/**
 * Include inherents that have been modified from their default state:
 * either they have a slotted enhancement, or they have extra slots placed
 * (even if empty — the user has allocated slot picks to them).
 */
function slimInherents(inherents: SelectedPower[]): SlimPower[] {
  return inherents
    .filter((p) => p.slots.some((s) => s !== null) || p.slots.length > 1)
    .map(slimPower);
}

// ============================================
// IMPORT: Slim → Build (hydration)
// ============================================

const SPECIAL_CATEGORIES: ReadonlySet<string> = new Set([
  'hamidon', 'titan', 'hydra', 'd-sync', 'prestige',
]);

/**
 * For VEAT archetypes, collect power definitions from all branch powersets
 * so hydration can find branch powers (e.g., Night Widow's Slash in widow-training builds).
 */
function getBranchPowerDefs(
  archetype: ReturnType<typeof getArchetype> | null,
  role: 'primary' | 'secondary',
): Power[] {
  if (!archetype?.branches) return [];
  const branchPowers: Power[] = [];
  for (const branch of Object.values(archetype.branches)) {
    if (!branch) continue;
    const branchSetId = role === 'primary' ? branch.primarySet : branch.secondarySet;
    if (!branchSetId) continue;
    const branchDef = getPowerset(branchSetId);
    if (branchDef) {
      branchPowers.push(...branchDef.powers);
    }
  }
  return branchPowers;
}

/**
 * One thing the file named that the dataset being hydrated against does not carry.
 *
 * Hydration is TOLERANT by design — an unresolved power is kept as a minimal `SelectedPower`
 * so the pick survives, and an unresolved enhancement leaves its slot empty. Tolerant and
 * SILENT are different things, though: opening a build against a dataset that was never its
 * own is a legitimate act (porting a Homecoming build to Brainstorm to see what the next
 * patch does to it), and the user is owed a list of what did not come across.
 */
export interface HydrationNote {
  /** Where it was found, in the user's terms — "Primary", "Fire Blast", "Slot 3". */
  context: string;
  /** What could not be resolved, named as the file spells it. */
  detail: string;
}

/**
 * An accolade list written by an older Sidekick, read as selected ids.
 *
 * Two migrations, and they are one question — "what did a previous version of this app store
 * here?" — so they live in one place rather than one each:
 *
 *  - **the fold.** Accolades were stored as whole `{ id, bonuses, … }` objects; only the id
 *    survives, because the export owns those values (Rule 0).
 *  - **the rename.** Two ids predate the game-internal-name convention. The table is CLOSED,
 *    not a best effort: the id vocabulary has had exactly two shapes, the four of 2026-01-20
 *    (`atlas_medallion`, `freedom_phalanx`, `task_force_commander`, `portal_jockey`) and the
 *    eight of 2026-03-21 that renamed two of them, so these two entries are the whole delta
 *    and no third legacy id can exist.
 *
 * Not a Rule 0 violation, though a table of CoH proper nouns looks like one. These are not
 * names the export ever used — no fork spells an accolade either way — they are ids this
 * app's own serializer wrote, and the table is a record of Sidekick's storage history rather
 * than a game fact hardcoded out of the data.
 *
 * Here rather than in `buildStore`'s persisted-state migration, which is where the rename used
 * to live alone: localStorage is one door of five, and the other four (share link, JSON paste,
 * `.skif` open, a cloud build) come through `hydrateBuild` — DATA-GAP ACCOLADE-3. A migration
 * on one door is not a migration. `importBuild`'s v1 arm is the fifth and reaches neither, so
 * it calls this directly.
 *
 * Renaming is not resolving. An id this table does not know is returned unchanged and left for
 * the engine to REPORT (`coh_math::gather::resolve_accolades`); guessing here would put the
 * silence back one layer down.
 */
export function normalizeAccoladeIds(raw: unknown): string[] {
  const legacyIds: Record<string, string> = {
    atlas_medallion: 'the_atlas_medallion',
    freedom_phalanx: 'freedom_phalanx_reserve',
  };
  if (!Array.isArray(raw)) return [];
  return raw
    .map((entry: unknown) =>
      typeof entry === 'string'
        ? entry
        : typeof (entry as { id?: unknown })?.id === 'string'
          ? ((entry as { id: string }).id)
          : null
    )
    .filter((id): id is string => id !== null)
    .map((id) => legacyIds[id] ?? id);
}

/**
 * Reconstruct a full Build from a v2 slim export.
 *
 * Pass `notes` to collect what this dataset could not carry. Optional because most callers
 * hydrate against the build's own dataset, where the list is always empty.
 */
export function hydrateBuild(slim: Record<string, any>, notes?: HydrationNote[]): Build {
  // Archetype
  const archetypeId = slim.archetype?.id ?? null;
  const archetype = archetypeId ? getArchetype(archetypeId) : null;

  const archetypeSelection = {
    id: archetypeId,
    name: archetype?.name ?? '',
    stats: archetype?.stats ?? null,
    inherent: archetype?.inherent ?? null,
  };

  // Primary powerset (include branch powers for VEATs)
  const primaryId = slim.primary?.id ?? null;
  const primaryDef = primaryId ? getPowerset(primaryId) : null;
  const allPrimaryDefs = [...(primaryDef?.powers ?? []), ...getBranchPowerDefs(archetype, 'primary')];
  if (primaryId && !primaryDef) notes?.push({ context: 'Primary', detail: primaryId });
  const primaryPowers = hydratePowers(slim.primary?.powers ?? [], allPrimaryDefs, primaryId ?? '', notes);

  // Secondary powerset (include branch powers for VEATs)
  const secondaryId = slim.secondary?.id ?? null;
  const secondaryDef = secondaryId ? getPowerset(secondaryId) : null;
  const allSecondaryDefs = [...(secondaryDef?.powers ?? []), ...getBranchPowerDefs(archetype, 'secondary')];
  if (secondaryId && !secondaryDef) notes?.push({ context: 'Secondary', detail: secondaryId });
  const secondaryPowers = hydratePowers(slim.secondary?.powers ?? [], allSecondaryDefs, secondaryId ?? '', notes);

  // Pools
  const pools: PoolSelection[] = (slim.pools ?? []).map((slimPool: SlimPoolSelection) => {
    const poolDef = getPowerPool(slimPool.id);
    if (!poolDef) notes?.push({ context: 'Pool', detail: slimPool.id });
    return {
      id: slimPool.id,
      name: poolDef?.name ?? slimPool.id,
      powers: hydratePowers(slimPool.powers, poolDef?.powers ?? [], slimPool.id, notes),
    };
  });

  // Epic pool
  let epicPool: PoolSelection | null = null;
  if (slim.epicPool) {
    const epicDef = getEpicPool(slim.epicPool.id);
    if (!epicDef) notes?.push({ context: 'Epic pool', detail: slim.epicPool.id });
    epicPool = {
      id: slim.epicPool.id,
      name: epicDef?.name ?? slim.epicPool.id,
      powers: hydratePowers(slim.epicPool.powers, epicDef?.powers ?? [], slim.epicPool.id, notes),
    };
  }

  // Inherent powers — auto-populate, then merge slot data from slim.
  // Auto-granted inherent slots (e.g. Rebirth Health/Stamina) are
  // resolved against the active dataset's inherent rules; the active
  // dataset is loaded at app boot to match the build being hydrated.
  const inherents = getInherentSelectedPowers(
    archetypeId,
    archetypeSelection.name,
    archetypeSelection.inherent,
    slim.level ?? 50,
  );
  const slimInherents: SlimPower[] = slim.inherents ?? [];
  for (const slimInh of slimInherents) {
    // Builds saved before the universal inherents were sourced from the export
    // store names the game has never used; `currentInherentName` translates the
    // eight that were retired.
    const storedName = slimInh.internalName
      ? currentInherentName(slimInh.internalName)
      : undefined;
    let match = storedName
      ? inherents.find((inh) => inh.internalName === storedName)
      : undefined;
    if (!match) {
      match = inherents.find(
        (inh) => inh.name.toLowerCase() === slimInh.name.toLowerCase()
      );
    }
    // Don't restore stored slots onto a now-unslottable inherent (maxSlots 0):
    // a build shared before Ninja Run / Beast Run were corrected would otherwise
    // re-apply their phantom base slot. getInherentSelectedPowers already gave
    // such powers an empty slots array.
    if (match && slimInh.slots.length > 0 && match.maxSlots !== 0) {
      match.slots = slimInh.slots.map((s: SlimEnhancement | null) =>
        s ? hydrateEnhancement(s) : null
      );
    }
    if (match && slimInh.inherentSlotCount) {
      match.inherentSlotCount = slimInh.inherentSlotCount;
    }
  }

  // Sets — convert pieces arrays back to Sets
  const setsEntries = Object.entries(slim.sets || {}) as [
    string,
    { count: number; pieces: number[] }
  ][];
  const sets = Object.fromEntries(
    setsEntries.map(([setId, tracking]) => [
      setId,
      { count: tracking.count, pieces: new Set(tracking.pieces) },
    ])
  );

  return {
    name: slim.name ?? 'Imported Build',
    // Dataset identifier — older exports predate the multi-dataset migration and don't carry
    // this field; default to Homecoming so legacy builds keep loading the same data they were
    // authored against. Anything the planner ships is kept as written: this fallback used to
    // name the forks inline and never grew a Brainstorm arm, so a Brainstorm save came back
    // stamped Homecoming and the engine — which keys on THIS field — computed the build
    // against live while the header badge, reading the loaded dataset, still said Brainstorm.
    serverId: isDatasetId(slim.serverId) ? slim.serverId : 'homecoming',
    archetype: archetypeSelection,
    level: slim.level ?? 50,
    progressionMode: slim.progressionMode ?? 'auto',
    primary: {
      id: primaryId,
      name: primaryDef?.name ?? '',
      powers: primaryPowers,
    },
    secondary: {
      id: secondaryId,
      name: secondaryDef?.name ?? '',
      powers: secondaryPowers,
    },
    pools,
    epicPool,
    inherents,
    accolades: normalizeAccoladeIds(slim.accolades),
    settings: slim.settings ?? { origin: 'Natural' },
    sets,
    incarnates: slim.incarnates ?? createEmptyIncarnateBuildState(),
    craftingChecklist: slim.craftingChecklist ?? {},
    incarnateObtained: slim.incarnateObtained ?? {},
    shoppingListAcquired: slim.shoppingListAcquired ?? {},
    slotOrder: slim.slotOrder ?? [],
    ...(Array.isArray(slim.activeModes) ? { activeModes: slim.activeModes } : {}),
    ...(Array.isArray(slim.attackChains) ? { attackChains: slim.attackChains } : {}),
    ...(slim.procOverrides && typeof slim.procOverrides === 'object'
      ? { procOverrides: slim.procOverrides as Build['procOverrides'] }
      : {}),
    mutedOverCapStats: Array.isArray(slim.mutedOverCapStats) ? slim.mutedOverCapStats : [],
  };
}

/**
 * Hydrate an array of slim powers by matching against powerset definitions.
 */
function hydratePowers(
  slimPowers: SlimPower[],
  powerDefs: readonly Power[],
  powerSetId: string,
  notes?: HydrationNote[],
): SelectedPower[] {
  const hydrated = slimPowers.map((slim) => {
    // Find the matching power definition. Lookup order:
    //   1. exact internalName (fast path)
    //   2. case-insensitive internalName (covers HC-patch casing changes
    //      like Tough_Hide → Tough_hide, Telekinetic_Blast → Telekinetic_blast,
    //      Lingering_radiation → Lingering_Radiation, etc.)
    //   3. case-insensitive display name (covers renames where the display
    //      name stayed the same, e.g. "Range" → "Boost_Range" both display "Boost Range")
    let def = slim.internalName
      ? powerDefs.find((p) => p.internalName === slim.internalName)
      : undefined;
    if (!def && slim.internalName) {
      const slimNameLower = slim.internalName.toLowerCase();
      def = powerDefs.find((p) => p.internalName.toLowerCase() === slimNameLower);
    }
    if (!def) {
      def = powerDefs.find(
        (p) => p.name.toLowerCase() === slim.name.toLowerCase()
      );
    }

    // Hydrate enhancement slots. A power with `maxSlots: 0` takes none at
    // all — the mode setters and stance switchers (Bio Armor's Adaptation /
    // Evolution, Dual Pistols' Swap Ammo, Staff Fighting's Staff Mastery,
    // Martial Combat's Reach for the Limit). `addPower` strips their base
    // slot on pick and the serializer stores `slots: []`, so "ensure at least
    // one slot" would hand every imported build a slot the picker never
    // gives; and restoring stored ones from a build saved against an older,
    // slottable definition would spend real budget on a power that can't use
    // it. Same guard the inherent merge above applies.
    const unslottable = def?.maxSlots === 0;
    const slots: (Enhancement | null)[] = unslottable
      ? []
      : slim.slots.map((s: SlimEnhancement | null) => {
          if (!s) return null;
          const enh = hydrateEnhancement(s);
          if (!enh) notes?.push({ context: slim.name, detail: enhancementLabel(s) });
          return enh;
        });

    // Ensure at least one slot
    if (slots.length === 0 && !unslottable) slots.push(null);

    if (def) {
      // Full reconstruction: spread power definition, overlay build-specific fields
      return {
        ...def,
        powerSet: powerSetId,
        level: slim.level,
        slots,
        ...(slim.isActive !== undefined ? { isActive: slim.isActive } : {}),
        ...(slim.activeSubPower ? { activeSubPower: slim.activeSubPower } : {}),
        ...(slim.isAutoGranted ? { isAutoGranted: slim.isAutoGranted } : {}),
        ...(slim.grantedByPower ? { grantedByPower: slim.grantedByPower } : {}),
        ...(slim.inherentSlotCount ? { inherentSlotCount: slim.inherentSlotCount } : {}),
      } as SelectedPower;
    }

    // Fallback: minimal SelectedPower when definition not found
    notes?.push({ context: powerSetId || 'Build', detail: slim.name });
    return {
      name: slim.name,
      internalName: slim.name.replace(/\s+/g, '_'),
      powerSet: powerSetId,
      level: slim.level,
      available: 0,
      maxSlots: 6,
      slots,
      allowedEnhancements: [],
      description: '',
      powerType: 'Click' as const,
      effects: {},
      ...(slim.isActive !== undefined ? { isActive: slim.isActive } : {}),
      ...(slim.activeSubPower ? { activeSubPower: slim.activeSubPower } : {}),
      ...(slim.isAutoGranted ? { isAutoGranted: slim.isAutoGranted } : {}),
      ...(slim.grantedByPower ? { grantedByPower: slim.grantedByPower } : {}),
      ...(slim.inherentSlotCount ? { inherentSlotCount: slim.inherentSlotCount } : {}),
    } as SelectedPower;
  });

  // Drop duplicate internal names within a powerset. A category can never
  // legitimately hold the same power twice (addPower guards that), so a dup
  // here is always corruption — most often a retired hidden power that now
  // resolves, by display name, onto the real power it shadowed. The classic
  // case: Martial Combat's "Build_Up_Proc" (a GlobalBoost proc that shared the
  // display name "Reach for the Limit") collapsing onto Reach_for_the_Limit and
  // showing twice. Keeping the first occurrence preserves the user's slots.
  const seen = new Set<string>();
  return hydrated.filter((p) => {
    if (seen.has(p.internalName)) return false;
    seen.add(p.internalName);
    return true;
  });
}

/**
 * What to call an enhancement in a note, given only the wire record that could not resolve.
 * The set's real display name lives in the dataset that does not have it, so the id it was
 * saved under is the most specific thing that can honestly be said.
 */
function enhancementLabel(slim: SlimEnhancement): string {
  switch (slim.type) {
    case 'io-set':
      return `${slim.setId} #${slim.pieceNum}`;
    case 'special':
      return `${slim.category} ${slim.id}`;
    default:
      return slim.type;
  }
}

/**
 * Reconstruct a full Enhancement from slim data.
 */
function hydrateEnhancement(slim: SlimEnhancement): Enhancement | null {
  switch (slim.type) {
    case 'io-set': {
      const ioSet = getIOSet(slim.setId);
      if (!ioSet) return null;
      const piece = ioSet.pieces.find((p) => p.num === slim.pieceNum);
      if (!piece) return null;
      return createIOSetEnhancement(ioSet, piece, slim.pieceNum - 1, {
        attuned: slim.attuned ?? false,
        level: slim.level ?? 50,
        boost: slim.boost,
      });
    }
    case 'io-generic': {
      return createGenericIOEnhancement(
        slim.stat as EnhancementStatType,
        slim.level,
        slim.boost,
      );
    }
    case 'special': {
      if (!SPECIAL_CATEGORIES.has(slim.category)) return null;
      const category = slim.category as SpecialCategory;
      // Active-dataset registry: a build slot whose piece the dataset doesn't
      // carry (e.g. a D-Sync on a fork) drops here rather than resurrecting
      // another server's enhancement.
      const def = getSpecialRegistry(category)[slim.id];
      if (!def) return null;
      return createSpecialEnhancement(slim.id, def, category, slim.boost);
    }
    case 'origin': {
      return createOriginEnhancement(
        slim.stat as EnhancementStatType,
        slim.tier as EnhancementTier,
        undefined,
        slim.boost,
      );
    }
    default:
      return null;
  }
}

// ============================================
// INHERENT POWER HELPERS (mirrors importer.ts)
// ============================================

function createInherentSelectedPower(
  def: InherentPowerDef,
  characterLevel: number,
): SelectedPower {
  const slots: (Enhancement | null)[] = def.maxSlots === 0 ? [] : [null];
  // Pre-fill any auto-granted inherent slots from the active dataset's
  // rules (e.g. Rebirth Health/Stamina). Mirrors the shared logic in
  // buildStore.ts.
  const inherentSlotCount = getInherentAutoGrantedSlotCount(def.internalName, characterLevel);
  for (let i = 0; i < inherentSlotCount; i++) slots.push(null);
  return {
    ...def,
    powerSet: 'Inherent',
    level: getInherentGrantLevel(def),
    slots,
    isLocked: def.isLocked ?? true,
    inherentCategory: def.category,
    ...(inherentSlotCount > 0 ? { inherentSlotCount } : {}),
  };
}

function getInherentSelectedPowers(
  archetypeId: string | null,
  archetypeName: string,
  archetypeInherent: { name: string; description: string } | null,
  characterLevel: number,
): SelectedPower[] {
  const powers = getInherentPowers().map((def) => createInherentSelectedPower(def, characterLevel));
  if (archetypeName && archetypeInherent) {
    const atInherentDef = createArchetypeInherentPower(archetypeName, archetypeInherent);
    powers.unshift(createInherentSelectedPower(atInherentDef, characterLevel));
  }
  // Archetype-specific inherents (Kheldian travel powers: Energy/Combat Flight,
  // Shadow Step/Recall). These are SLOTTABLE, so they have to be in the list the
  // stored inherents merge onto — otherwise the slim entry finds no match and its
  // slots and enhancements are dropped. `syncBuildDefinitions` appends a pristine
  // copy afterwards, which is why the power still shows up but comes back empty.
  for (const def of getArchetypeInherentPowers(archetypeId || undefined)) {
    powers.push(createInherentSelectedPower(def, characterLevel));
  }
  return powers;
}
