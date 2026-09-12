/**
 * Build Store - manages character build state
 *
 * Uses Zustand for state management with persistence to localStorage.
 * This replaces the legacy global AppState.build object.
 */

import { create } from 'zustand';
import { persist, createJSONStorage, type StateStorage } from 'zustand/middleware';
import type {
  Build,
  AttackChain,
  ProcOverride,
  SelectedPower,
  Power,
  ArchetypeId,
  ArchetypeBranchId,
  Origin,
  ProgressionMode,
  Enhancement,
  SelectedIncarnatePower,
  IncarnateSlotId,
  CraftingChecklistKey,
} from '@/types';
import { createEmptyBuild, createEmptyIncarnateBuildState, createEmptyCraftingChecklistState } from '@/types';
import {
  getArchetype,
  getPowerset,
  getPowerPool,
  getEpicPool,
  getTotalSlotsAtLevel,
  getPowerPicksAtLevel,
  getMaxPowerPools,
  MAX_POWER_PICKS,
  EPIC_POOL_LEVEL,
  getPoolUnlockLevel,
  getInherentPowers,
  getInherentPowerDef,
  getArchetypeInherentPowers,
  createArchetypeInherentPower,
  POWER_PICK_LEVELS,
  getIOSet,
  GRANTED_POWER_GROUPS,
  getExcludedPools,
  getAllPowerPools,
  getInherentGrantLevel,
  getInherentAutoGrantedSlotCount,
  STANCE_GROUPS,
} from '@/data';
import type { InherentPowerDef } from '@/data';
import { currentInherentName } from '@/data/inherent-aliases';
import { computeSetTracking } from '@/utils/calculations/set-tracking';
import {
  DEFAULT_PROC_OVERRIDE,
  isDefaultProcOverride,
  procOverrideKey,
  pruneProcOverridesForRemovedPowers,
  reindexProcOverridesForRemovedSlot,
} from '@/data/proc-data';
import { slimBuild, hydrateBuild, normalizeAccoladeIds, type HydrationNote } from '@/utils/build-serialization';
import { encodeImportFragment } from '@/utils/import-url';
import { getActiveDataset, getAllDatasetMetadata, isDatasetId } from '@/data/dataset';
import { toCanonicalStatKey } from '@/data/set-bonus-groups';
import { showDatasetSwitchOverlay } from '@/utils/dataset-switch-overlay';
import {
  migratePerServerState,
  selectActiveBuild,
  composePersistedState,
  type StoredBuild,
} from '@/utils/per-server-builds';
import { findNextAvailableGrantLevel, backfillSlotOrderLevels, scrubFabricatedSlotLevels, reconcileStoredSlotLevels, ensureSlotOrderPopulated, canMoveSlotLevel, applySlotLevelMove, canRelocateSlot, type SlotLevelRef, type PowerRef } from '@/utils/slot-levels';
import { enhancementAllowedInPower } from '@/utils/enhancement-eligibility';
import { dedupePools } from '@/utils/build-powers';
import { branchPowersInBuild, branchSetIds } from '@/utils/branch-powers';
import { selectableModes, publishedModes } from '@/utils/mode-suppression';
// The one toggle classifier (`ba84984159` unified it); a second copy here is exactly
// the drift that unification closed, so the store reads the same function the rows do.
import { shouldShowToggle } from '@/components/powers/power-row-utils';
import { useHistoryStore } from './historyStore';
import { useUIStore } from './uiStore';

// ============================================
// CAPTURE-MODE PERSISTENCE
// ============================================
// A hidden capture-mode boot (see streams/BUILD_PREVIEW_BACKFILL_PLAN.md,
// PREVBF3/4) imports an arbitrary build — someone else's, for a preview
// render — into this store. That must never reach the visiting browser's
// real `localStorage`: this module is a singleton, and `persist` writes on
// every state change, so a real write here would overwrite whatever build
// that browser actually has saved. Capture-mode boots read the URL param
// once at store-creation time and swap in an in-memory stub instead — the
// import still works (the store still holds the data in memory for
// SharePreviewCapture to render), it just never leaves the tab.

function isPreviewCaptureMode(): boolean {
  if (typeof window === 'undefined') return false;
  return new URLSearchParams(window.location.search).has('previewCapture');
}

const memoryStorageData = new Map<string, string>();
const memoryStorage: StateStorage = {
  getItem: (name) => memoryStorageData.get(name) ?? null,
  setItem: (name, value) => void memoryStorageData.set(name, value),
  removeItem: (name) => void memoryStorageData.delete(name),
};

// ============================================
// POWER CATEGORY TYPE
// ============================================

export type PowerCategory = 'primary' | 'secondary' | 'pool' | 'epic' | 'inherent';

// ============================================
// BUILD STORE INTERFACE
// ============================================

interface BuildState {
  /** The current build */
  build: Build;

  /** Whether the store has been hydrated from storage */
  _hasHydrated: boolean;

  /**
   * Working builds for servers OTHER than the loaded one, kept as opaque
   * stored blobs so a dataset switch (dropdown or `?serverId=` deeplink)
   * preserves each server's build rather than clobbering it. Set at rehydrate,
   * carried through `partialize` untouched. See {@link file://./../utils/per-server-builds.ts}.
   */
  _inactiveServerBuilds: Partial<Record<Build['serverId'], StoredBuild>>;

  /**
   * What the last opened file named that the dataset it was read against does not carry.
   *
   * Only a cross-dataset open can fill this: read against its own server a build resolves
   * whole. Held in the store rather than returned because `importBuild`'s boolean is what
   * every caller and gate reads, and a receipt is for the one caller that offered the port.
   */
  lastImportNotes: HydrationNote[];
}

interface BuildActions {
  // Build metadata
  setBuildName: (name: string) => void;

  // Archetype
  setArchetype: (archetypeId: ArchetypeId) => void;
  clearArchetype: () => void;

  // Powersets
  setPrimary: (powersetId: string) => void;
  setSecondary: (powersetId: string) => void;
  /**
   * Move a VEAT to a different branch (or off one entirely), dropping the picks the
   * outgoing branch owned. Returns their display names so the caller can report the loss.
   *
   * The raw `uiStore.setSelectedBranch` still exists for the paths that are FOLLOWING the
   * build rather than changing it — import, rehydrate, undo — which must not strip anything.
   */
  switchBranch: (branchId: ArchetypeBranchId | null) => string[];

  // Powers
  addPower: (category: PowerCategory, power: SelectedPower) => void;
  removePower: (category: PowerCategory, powerName: string) => void;
  movePowerLevel: (category: PowerCategory, powerName: string, newLevel: number) => void;
  /** Categories are REQUIRED: internalName is not unique across categories, so
   *  resolving either power by bare name can swap the WRONG power's level. */
  swapPowerLevels: (powerNameA: string, categoryA: PowerCategory, powerNameB: string, categoryB: PowerCategory) => void;

  // Pools
  addPool: (poolId: string) => boolean;
  removePool: (poolId: string) => void;
  setEpicPool: (poolId: string | null) => void;

  // Slots (category is optional — disambiguates when multiple powers share the same internalName)
  addSlot: (powerName: string, category?: PowerCategory) => boolean;
  removeSlot: (powerName: string, slotIndex: number, category?: PowerCategory) => boolean;
  /**
   * Swap the grant LEVELS of two allocated slots, leaving their enhancers in
   * place. Mids-style "move a slot's level to another power". Returns false if
   * the swap is invalid (e.g. would put a slot below its power's pick level).
   */
  moveSlotLevel: (source: SlotLevelRef, target: SlotLevelRef) => boolean;
  /** Whether `moveSlotLevel(source, target)` would succeed (for UI hinting). */
  canMoveSlotLevel: (source: SlotLevelRef, target: SlotLevelRef) => boolean;
  /**
   * Relocate an allocated slot from one power to another (the "move a slot
   * between powers" gesture, distinct from moveSlotLevel). The slot's
   * enhancement travels with it when the target power allows that enhancement;
   * otherwise the slot moves empty and `enhancementDropped` is true. The slot
   * budget is net-neutral. Returns `{ ok: false }` when the move is invalid.
   */
  moveSlot: (source: SlotLevelRef, target: PowerRef) => { ok: boolean; enhancementDropped: boolean };
  /** Whether `moveSlot(source, target)` would succeed (for UI hinting). */
  canMoveSlot: (source: SlotLevelRef, target: PowerRef) => boolean;
  /**
   * Freeze real grant levels onto every slot placed while Level Up mode was
   * off (SLOT-3). Called once, right as the mode turns on — mirrors the
   * import-time migrations (`ensureSlotOrderPopulated` /
   * `backfillSlotOrderLevels` / `reconcileStoredSlotLevels`) so a build
   * planned free-form gets a legal, stable leveling order the moment there is
   * one to have, rather than a value that could reshuffle on the next slot
   * edit. A slot the schedule genuinely cannot serve is left level-less and
   * renders as the existing unhonorable-slot marker.
   */
  freezeSlotLevelsForLevelUpMode: () => void;

  // Enhancements
  setEnhancement: (powerName: string, slotIndex: number, enhancement: Enhancement, category?: PowerCategory) => void;
  clearEnhancement: (powerName: string, slotIndex: number, category?: PowerCategory) => void;
  clearAllEnhancements: (powerName: string, category?: PowerCategory) => void;

  // Settings
  setLevel: (level: number) => void;
  setProgressionMode: (mode: ProgressionMode) => void;
  setOrigin: (origin: Origin) => void;
  setActiveModes: (modes: string[]) => void;

  /**
   * Link the current in-memory build to a Build Library entry by id, so
   * the next "Save to Library" updates that entry rather than creating
   * a duplicate. Pass `null` to clear (e.g. when starting fresh or
   * forking). Called after loading a build from BuildDetailPage and
   * cleared automatically on import / reset. Action name kept as
   * `setVaultId` for backend continuity — see Build.vaultId.
   */
  setVaultId: (id: string | null) => void;

  // Saved attack chains (named rotations stored on the build, so they travel
  // with the character through save / load / export / share).
  /** Create a new saved chain from a cast-order id list; returns its id.
   *  `startForm` is the caster form the chain opens in and `fullShiftAnimations`
   *  whether it charges the full shapeshift animation — both are part of the
   *  rotation's identity, not view settings (see AttackChain). */
  saveAttackChain: (
    name: string,
    powers: string[],
    startForm?: string | null,
    fullShiftAnimations?: boolean,
  ) => string;
  /** Replace the cast order and both modelling assumptions of an existing saved
   *  chain — the "Save" action. Every field travels together: a chain reloaded
   *  under a different assumption is a different rotation, so writing the order
   *  back without the assumption it was measured under is the data loss this
   *  signature exists to prevent. */
  updateAttackChain: (
    id: string,
    powers: string[],
    startForm?: string | null,
    fullShiftAnimations?: boolean,
  ) => void;
  renameAttackChain: (id: string, name: string) => void;
  deleteAttackChain: (id: string) => void;

  // Per-slotted-proc control overrides (enable/disable + stack / HP-scaling
  // slider), keyed `${powerName}:${slotIndex}`. Sparse: absent = enabled + auto.
  /** Merge a partial override for one slotted proc; prunes back to absent when
   *  the result is the default (enabled + auto), keeping the map sparse. */
  setProcOverride: (powerName: string, slotIndex: number, patch: Partial<ProcOverride>) => void;
  /** Remove a proc's override entirely (back to enabled + auto). */
  clearProcOverride: (powerName: string, slotIndex: number) => void;

  // Per-build over-cap warning mutes (canonical `group|label` stat keys). Sparse.
  /** Add the stat's canonical key if absent, remove it if present. */
  toggleOverCapMute: (statKey: string) => void;
  /** Remove every over-cap mute (no-op when already empty). */
  clearOverCapMutes: () => void;

  /**
   * Walk every slotted enhancement and bump it to its "finalized" form.
   * All options are individually optional — pass only the ones you want
   * to change; omitted ones leave the corresponding state alone.
   *   - `relativeLevel`: set the signed relative level on every enhancement
   *     that carries one — origin (TO/DO/SO) and special (Hamidon/Titan/
   *     Hydra/D-Sync/prestige). Negative means under-level and is worth
   *     LESS (-10% per level on Homecoming); 0 clears the offset.
   *   - `ioLevel`: force every non-attuned IO (`type: 'io-set'`,
   *     `'io-generic'`) to this level. Set IOs are clamped to the set's
   *     [minLevel, maxLevel] range. Attuned IOs are skipped (they don't
   *     carry a meaningful level — they scale with character level).
   *   - `attuneAll`: flip `attuned: true` on every set IO. Doesn't apply
   *     to generic IOs (no attunement state). Mirrors the in-game catalyst
   *     conversion by also clearing `boost` on any IO that becomes attuned
   *     this way (attuned IOs cannot carry boosters).
   *   - `boostLevel`: set the +X catalyst boost on every NON-attuned IO
   *     at L50+ (generic or set). Attuned IOs cannot accept boosters in
   *     the game; sub-50 non-attuned IOs are similarly ineligible.
   * `relativeLevel` and `boostLevel` are disjoint by enhancement type, not
   * by accident: they are two different game mechanics off two different
   * curves, and no enhancement sits on both. Slots already at the target
   * value are left alone so the operation is idempotent and the breakdown
   * stays clean.
   *
   * Returns how many slots actually changed. Callers that offer this from
   * outside the tools modal need it: the eligibility rules above mean a
   * plausible-looking request can legitimately move nothing (every IO
   * attuned, every slot already at the target), and a bulk edit that
   * silently does nothing reads as a broken button.
   */
  maximizeEnhancementLevels: (options?: {
    relativeLevel?: number;
    ioLevel?: number;
    attuneAll?: boolean;
    boostLevel?: number;
  }) => number;

  // Accolades — selected ids (internal name, lower-cased)
  addAccolade: (accoladeId: string) => void;
  removeAccolade: (accoladeId: string) => void;

  // Incarnates
  setIncarnatePower: (slotId: IncarnateSlotId, power: SelectedIncarnatePower) => void;
  clearIncarnatePower: (slotId: IncarnateSlotId) => void;
  clearAllIncarnates: () => void;

  // Incarnate Crafting Checklist
  toggleCraftingCheckItem: (key: CraftingChecklistKey) => void;
  setCraftingCheckItem: (key: CraftingChecklistKey, checked: boolean) => void;
  clearCraftingChecklist: () => void;
  clearCraftingChecklistForSlot: (slotId: IncarnateSlotId) => void;
  /** Toggle whether a crafting-tree node is already obtained. Key is
   *  `{slotId}:{treeId}:{nodePath}`. An obtained node (and everything consumed
   *  to make it) drops from the crafting costs and shopping list. */
  toggleIncarnateObtainedNode: (key: string) => void;

  // Shopping List
  acquireShoppingItem: (salvageId: string) => void;
  unacquireShoppingItem: (salvageId: string) => void;
  clearShoppingListAcquired: () => void;

  // Power toggle (for stat calculations)
  togglePowerActive: (powerName: string, category?: PowerCategory) => void;
  /** Set the active sub-power for powers with mutually exclusive stances (e.g., Adaptation) */
  setActiveSubPower: (parentPowerName: string, subPowerName: string | null) => void;

  // Computed
  getTotalSlotsUsed: () => number;
  getSlotsRemaining: () => number;
  canAddSlot: (powerName: string, category?: PowerCategory) => boolean;
  canAddPool: () => boolean;
  isUniqueEnhancementSlotted: (setId: string, pieceNum: number) => boolean;

  // Import/Export
  exportBuild: () => string;
  /**
   * Read a build file into the planner.
   *
   * A file whose server is not the one loaded normally reloads onto ITS server (the active
   * dataset is a boot-time singleton). `intoLoadedDataset` says the user asked for the other
   * act — read this build against the dataset already loaded, e.g. a live Homecoming build
   * opened on Brainstorm to see what the next patch does to it. Whatever the target dataset
   * does not carry is kept in the build and returned as notes, never dropped silently.
   */
  importBuild: (json: string, options?: { intoLoadedDataset?: boolean }) => boolean;
  importMidsBuild: (build: Build) => void;
  resetBuild: () => void;
  clearPowers: () => void;
  /** Wipe every slotted enhancement across the whole build; keep slot structure intact. */
  clearAllEnhancementsGlobal: () => void;
  /** Reduce every power's slot list back to just its base slot (plus any auto-granted inherent slots), and wipe all enhancements. Slot allocations are reset. */
  clearAllExtraSlots: () => void;

  // Hydration
  setHasHydrated: (value: boolean) => void;

  // History (internal - used by undo/redo)
  _restoreBuild: (build: Build) => void;
}

type BuildStore = BuildState & BuildActions;

// ============================================
// HELPER FUNCTIONS
// ============================================

/**
 * Sync power definitions (effects, icons) and enhancement icons from current data.
 * Fixes stale data in builds saved before power/enhancement updates.
 * Called on both localStorage rehydration and build import.
 */
function syncBuildDefinitions(build: Build): void {
  // Track old→new internalName changes for slotOrder migration
  const internalNameMigrations = new Map<string, string>();

  // Helper: sync power metadata (effects, icon, classification flags) from
  // the current powerset definition. The slim serializer drops all of these
  // — stored powers only persist user-editable fields like slots/isActive —
  // so on rehydrate we have to re-attach the static metadata for the UI
  // (shouldShowToggle, info panels) and dashboard (ally-only filter,
  // toggle-vs-click branching) to function. Without powerType/targetType/
  // effectArea sync, click-power toggles like Healing Flames lose their
  // toggle UI and ally-buff guards see undefined and pass everything.
  //
  // `damage` and `shortHelp` are here for the same reason and were missing for the
  // same silent one: `shouldShowToggle` reads FOUR fields off the stored power
  // (powerType, targetType, damage, shortHelp) and only two of them were being
  // repaired. A build saved before a power gained its damage entry — or by any
  // writer that dropped it — asks the classifier whether an attack is an attack and
  // is told no, which flips the `damageBuff` skip and hands a per-cast Defiance proc
  // a toggle it should never have had.
  //
  // `atoms` and `targetsAffected` joined the list when `shouldShowToggle` stopped reading
  // the bag (BPORT6): the classifier now asks the atoms, and `reachesCaster` needs the
  // recipient list to answer. A stored power carries neither, so without repairing both
  // here every rehydrated build would answer the classifier from an empty atom array and
  // lose every click toggle it had — the same silent one the `damage`/`shortHelp` note
  // above records, on the field that replaced the one it was written about.
  type DefShape = Pick<
    SelectedPower,
    | 'name' | 'internalName' | 'effects' | 'icon' | 'powerType' | 'targetType' | 'effectArea'
    | 'damage' | 'shortHelp' | 'available' | 'atoms' | 'targetsAffected'
    | 'setsModes' | 'modesRequired' | 'modesDisallowed' | 'modesSuspended' | 'modeVariants'
    | 'allowedEnhancements' | 'allowedSetCategories'
  >;
  /**
   * The five mode-gating fields, lifted off a definition as one unit.
   *
   * They were absent from DefShape, and the omission was silent. The persisted
   * build stores whole `SelectedPower` objects, so a build saved while the
   * pool/epic facades were still dropping these fields (see `pickModeGates` in
   * power-pools.ts) keeps pool and epic clicks with NO gate at all — and this
   * sync, which exists precisely to repair stale saved metadata, walked past
   * them. The user-visible result is the Attack Chain Builder offering Boxing,
   * Kick, Cross Punch and Hasten inside Nova/Dwarf form on every pre-existing
   * build, and the cross-form legality check having nothing to check, until the
   * power is removed and re-added by hand.
   *
   * Written from the definition in BOTH directions — a gate the game data has
   * dropped must clear rather than linger, so an absent field is copied over as
   * absent rather than left alone. One literal serves the comparison and the
   * write, so the two cannot drift apart.
   */
  type ModeGates = Pick<DefShape, 'setsModes' | 'modesRequired' | 'modesDisallowed' | 'modesSuspended' | 'modeVariants'>;
  const modeGates = (p: DefShape): ModeGates => ({
    setsModes: p.setsModes,
    modesRequired: p.modesRequired,
    modesDisallowed: p.modesDisallowed,
    modesSuspended: p.modesSuspended,
    modeVariants: p.modeVariants,
  });
  /**
   * A persisted `isActive` on a power the UI renders no toggle for is UNREACHABLE
   * state: it makes the power contribute to every dashboard total with no control to
   * turn it off and no indicator that it is on. Cleared here, in the one funnel every
   * load and every import passes through.
   *
   * The reported case (2026-08-05) is a Blaster whose End of Time and Future Pain
   * added +16.4% global damage. Both are `Click` attacks carrying a Defiance rider,
   * so `shouldShowToggle` correctly gives them no toggle — but a Mids `.mbd` import
   * maps Mids' `StatInclude` onto `isActive` for EVERY power it brings in
   * (`mids-import/importer.ts`), and once planted the flag could not be seen or
   * cleared. Fixing the Defiance leak stops that pair moving the damage column;
   * clearing the flag stops the NEXT classifier change from re-opening the same trap
   * on a different family (the snipe `rangeBuff` skip and the unroutable-mez-resist
   * skip each retired a toggle the same way, leaving whatever was already stored).
   *
   * Deliberately narrow — three exemptions, each because the flag is legitimately
   * driven by something other than the toggle:
   *   - non-`click` powers: a Toggle always has its toggle, and an Auto contributes
   *     via `is_auto` whatever the flag says;
   *   - mode setters (`setsModes`): the flag mirrors the mode selector, written by
   *     the pick path and by `setActiveModes`, not by a toggle;
   *   - `activeSubPower` holders: the stance is stored on the parent and applies
   *     regardless of the parent's own flag, so the parent's `isActive` is still
   *     meaningful state a stance-carrying power owns.
   */
  const hasUnreachableActiveFlag = (power: SelectedPower): boolean => {
    if (power.isActive === undefined) return false;
    if (power.powerType?.toLowerCase() !== 'click') return false;
    if (power.setsModes?.length || power.activeSubPower) return false;
    return !shouldShowToggle(power);
  };

  const syncPowers = (powers: SelectedPower[], defPowers: readonly DefShape[]): SelectedPower[] => {
    let anyChanged = false;
    const synced = powers.map((power) => {
      // `currentInherentName` is a no-op for everything but the eight inherents
      // renamed when they stopped being hand-authored (see inherent-aliases.ts).
      const storedName = currentInherentName(power.internalName);
      let currentDef = defPowers.find((p) => p.internalName === storedName);
      if (!currentDef) {
        currentDef = defPowers.find((p) => p.name === power.name);
      }
      if (!currentDef) return power;
      const needsInternalName = currentDef.internalName !== power.internalName;
      const needsEffects = currentDef.effects && currentDef.effects !== power.effects;
      const needsIcon = currentDef.icon && currentDef.icon !== power.icon;
      const needsPowerType = currentDef.powerType && currentDef.powerType !== power.powerType;
      const needsTargetType = currentDef.targetType !== undefined && currentDef.targetType !== power.targetType;
      const needsEffectArea = currentDef.effectArea !== undefined && currentDef.effectArea !== power.effectArea;
      const needsShortHelp = currentDef.shortHelp !== undefined && currentDef.shortHelp !== power.shortHelp;
      // `available` is static def metadata like the rest, but its reader is the
      // pick-level relevel migration (and addPower's minimum-level floor): a
      // rehydrated power without it has no unlock level, and a relevel that can't
      // see the unlock can place the power below the level the game offers it at.
      const needsAvailable = currentDef.available !== undefined && currentDef.available !== power.available;
      // Structural, not reference: these arrive as fresh arrays out of JSON on
      // every rehydrate, so an identity check would report "changed" for every
      // power in every build forever.
      const needsDamage = JSON.stringify(currentDef.damage) !== JSON.stringify(power.damage);
      // Presence, not equality, for the two atom-classifier fields: the atom array is the
      // largest thing on a power definition, and stringifying both copies of it for every
      // power of every build on every rehydrate costs more than the repair is worth. A
      // stored power either carries them or does not — the slim serializer drops both — so
      // "absent here, present on the def" is the whole population this needs to catch.
      const needsAtoms = currentDef.atoms !== undefined && power.atoms === undefined;
      const needsTargetsAffected = currentDef.targetsAffected !== undefined
        && power.targetsAffected === undefined;
      const needsModeGates =
        JSON.stringify(modeGates(currentDef)) !== JSON.stringify(modeGates(power));
      // The slotting allow-lists, repaired for the same reason as the mode gates:
      // the persisted build stores whole SelectedPower objects, and a record that
      // hydrated while its powerset def was unreachable (the epic registry keyed by
      // a dataset not yet active) carries `allowedEnhancements: []` and no
      // allowedSetCategories forever. The legality check then rejects every
      // enhancement while the picker — a fresh def lookup — happily lists the sets
      // (the Rebirth Psionic Tornado / Ragnarok report, 2026-08-13). Both
      // directions, like the gates: a list the game data dropped must clear too.
      const needsAllowLists =
        JSON.stringify([currentDef.allowedEnhancements, currentDef.allowedSetCategories])
          !== JSON.stringify([power.allowedEnhancements, power.allowedSetCategories]);
      const metadataChanged = needsInternalName || needsEffects || needsIcon || needsPowerType
        || needsTargetType || needsEffectArea || needsShortHelp || needsAvailable || needsDamage
        || needsModeGates || needsAllowLists || needsAtoms || needsTargetsAffected;
      // Judge the toggle on the REPAIRED power, not the stored one — the whole point
      // of the metadata sync above is that the stored copy may be missing the fields
      // `shouldShowToggle` reads.
      const repaired: SelectedPower = metadataChanged
        ? {
            ...power,
            ...(needsInternalName ? { internalName: currentDef.internalName } : {}),
            ...(needsEffects ? { effects: currentDef.effects } : {}),
            ...(needsIcon ? { icon: currentDef.icon } : {}),
            ...(needsPowerType ? { powerType: currentDef.powerType } : {}),
            ...(needsTargetType ? { targetType: currentDef.targetType } : {}),
            ...(needsEffectArea ? { effectArea: currentDef.effectArea } : {}),
            ...(needsShortHelp ? { shortHelp: currentDef.shortHelp } : {}),
            ...(needsAvailable ? { available: currentDef.available } : {}),
            ...(needsDamage ? { damage: currentDef.damage } : {}),
            ...(needsAtoms ? { atoms: currentDef.atoms } : {}),
            ...(needsTargetsAffected ? { targetsAffected: currentDef.targetsAffected } : {}),
            ...(needsModeGates ? modeGates(currentDef) : {}),
            ...(needsAllowLists
              ? {
                  allowedEnhancements: currentDef.allowedEnhancements,
                  allowedSetCategories: currentDef.allowedSetCategories,
                }
              : {}),
          }
        : power;
      const dropsActive = hasUnreachableActiveFlag(repaired);
      if (!metadataChanged && !dropsActive) return power;
      anyChanged = true;
      if (needsInternalName) {
        internalNameMigrations.set(power.internalName, currentDef.internalName);
      }
      if (!dropsActive) return repaired;
      // Deleted, not set to `false`: the calc's gate is `isAuto || isActive`, so the
      // two behave alike, but an absent flag is what a freshly picked click power
      // carries and what the slim serializer omits — leaving `false` behind would
      // persist a distinction nothing reads.
      const { isActive: _unreachable, ...withoutActive } = repaired;
      return withoutActive as SelectedPower;
    });
    return anyChanged ? synced : powers;
  };

  // For VEATs, collect branch power definitions so sync covers branch powers too
  const archetype = build.archetype.id ? getArchetype(build.archetype.id) : null;

  // Helper: fix powerSet to use the correct powerset ID.
  // For VEATs with branches, a power's powerSet may legitimately be a branch set ID
  // (e.g., 'arachnos-widow/fortunata-training') rather than the base primary ID.
  // Also fixes branch powers that were incorrectly assigned the base set ID
  // (e.g., from older imports that didn't distinguish branch powersets).
  const fixPowerSetIds = (powers: SelectedPower[], correctId: string, role?: 'primary' | 'secondary'): SelectedPower[] => {
    // Collect all valid powerset IDs: base + branch sets
    const validIds = new Set([correctId]);
    // Build branch set map for correcting misassigned powers
    const branchSets: Array<{ id: string; powerNames: Set<string> }> = [];
    if (role && archetype?.branches) {
      for (const branch of Object.values(archetype.branches)) {
        if (!branch) continue;
        const branchSetId = role === 'primary' ? branch.primarySet : branch.secondarySet;
        if (branchSetId) {
          validIds.add(branchSetId);
          const branchPowerset = getPowerset(branchSetId);
          if (branchPowerset) {
            branchSets.push({
              id: branchSetId,
              powerNames: new Set(branchPowerset.powers.map(p => p.internalName)),
            });
          }
        }
      }
    }

    // Check if the base powerset actually contains each power
    const basePowerset = getPowerset(correctId);
    const basePowerNames = basePowerset
      ? new Set(basePowerset.powers.map(p => p.internalName))
      : new Set<string>();

    let anyChanged = false;
    const fixed = powers.map((power) => {
      if (!power.powerSet || !validIds.has(power.powerSet)) {
        // Invalid powerset — assign base
        anyChanged = true;
        return { ...power, powerSet: correctId };
      }
      // Fix branch powers incorrectly assigned to the base powerset:
      // if the power's current powerSet is the base but the power doesn't exist there,
      // find the correct branch powerset
      if (power.powerSet === correctId && !basePowerNames.has(power.internalName)) {
        for (const branch of branchSets) {
          if (branch.powerNames.has(power.internalName)) {
            anyChanged = true;
            return { ...power, powerSet: branch.id };
          }
        }
      }
      return power;
    });
    return anyChanged ? fixed : powers;
  };
  const getBranchPowers = (role: 'primary' | 'secondary'): readonly DefShape[] => {
    if (!archetype?.branches) return [];
    const powers: DefShape[] = [];
    for (const branch of Object.values(archetype.branches)) {
      if (!branch) continue;
      const branchSetId = role === 'primary' ? branch.primarySet : branch.secondarySet;
      if (!branchSetId) continue;
      const branchDef = getPowerset(branchSetId);
      if (branchDef) powers.push(...branchDef.powers);
    }
    return powers;
  };

  // Sync primary powers
  if (build.primary.id && build.primary.powers.length > 0) {
    const def = getPowerset(build.primary.id);
    if (def) {
      const allDefs = [...def.powers, ...getBranchPowers('primary')];
      let fixed = syncPowers(build.primary.powers, allDefs);
      fixed = fixPowerSetIds(fixed, build.primary.id, 'primary');
      if (fixed !== build.primary.powers) {
        build.primary = { ...build.primary, powers: fixed };
      }
    }
  }

  // Sync secondary powers
  if (build.secondary.id && build.secondary.powers.length > 0) {
    const def = getPowerset(build.secondary.id);
    if (def) {
      const allDefs = [...def.powers, ...getBranchPowers('secondary')];
      let fixed = syncPowers(build.secondary.powers, allDefs);
      fixed = fixPowerSetIds(fixed, build.secondary.id, 'secondary');
      if (fixed !== build.secondary.powers) {
        build.secondary = { ...build.secondary, powers: fixed };
      }
    }
  }

  // Enforce the pool-uniqueness invariant before any pool processing below.
  // A build may hold at most getMaxPowerPools(serverId) distinct pools, each id
  // once. Duplicate pool objects can enter from importers that trust the
  // source's powerset list (a Mids .mbd whose PowerSets array names the same
  // pool twice) and can even share one powers-array reference. Deduping here —
  // the single funnel every import AND localStorage rehydrate passes through —
  // both stops new corruption and self-heals already-saved builds on next load.
  // Must run before the misplaced-power redistribution below, whose id→bucket
  // Map collapses (and aliases) on duplicate ids.
  build.pools = dedupePools(build.pools, getMaxPowerPools(build.serverId));

  // Fix misplaced pool powers — move powers to their correct pool container
  if (build.pools.length > 1) {
    // Build lookup: powerName → correct poolId
    const allPools = getAllPowerPools();
    const powerToPool = new Map<string, string>();
    for (const [poolId, poolDef] of Object.entries(allPools)) {
      for (const p of poolDef.powers) {
        powerToPool.set(p.internalName, poolId);
      }
    }

    // Check for misplaced powers
    let hasMisplaced = false;
    for (const pool of build.pools) {
      for (const power of pool.powers) {
        const correctPoolId = powerToPool.get(power.internalName);
        if (correctPoolId && correctPoolId !== pool.id) {
          hasMisplaced = true;
          break;
        }
      }
      if (hasMisplaced) break;
    }

    if (hasMisplaced) {
      // Collect all pool powers, then redistribute to correct containers
      const poolIds = build.pools.map((p) => p.id);
      const poolPowers = new Map<string, SelectedPower[]>(poolIds.map((id) => [id, []]));

      for (const pool of build.pools) {
        for (const power of pool.powers) {
          const correctPoolId = powerToPool.get(power.internalName);
          const targetId = correctPoolId && poolIds.includes(correctPoolId) ? correctPoolId : pool.id;
          poolPowers.get(targetId)!.push({ ...power, powerSet: targetId });
        }
      }

      build.pools = build.pools.map((pool) => ({
        ...pool,
        powers: poolPowers.get(pool.id) ?? pool.powers,
      }));
    }
  }

  // Sync pool powers
  if (build.pools.length > 0) {
    build.pools = build.pools.map((pool) => {
      const def = getPowerPool(pool.id);
      if (!def) return pool;
      let fixed = syncPowers(pool.powers, def.powers);
      fixed = fixPowerSetIds(fixed, pool.id);
      return fixed !== pool.powers ? { ...pool, powers: fixed } : pool;
    });
  }

  // Sync epic pool powers
  if (build.epicPool && build.epicPool.powers.length > 0) {
    const def = getEpicPool(build.epicPool.id);
    if (def) {
      let fixed = syncPowers(build.epicPool.powers, def.powers);
      fixed = fixPowerSetIds(fixed, build.epicPool.id);
      if (fixed !== build.epicPool.powers) {
        build.epicPool = { ...build.epicPool, powers: fixed };
      }
    }
  }

  // Sync inherent power icons (handles PascalCase -> lowercase migration)
  if (build.inherents.length > 0) {
    const fixed = syncPowers(build.inherents, getInherentPowers());
    if (fixed !== build.inherents) {
      build.inherents = fixed;
    }
  }

  // Append any inherent powers added since this build was saved — e.g. the
  // Kheldian travel inherents (Energy/Combat Flight, Shadow Step/Recall) that
  // became discoverable in 2026-06, or Ninja/Beast Run. Without this, an older
  // saved build loads missing those always-on toggles and can't slot them. This
  // lives in the shared sync funnel so BOTH importBuild (share links / JSON) and
  // localStorage rehydrate self-heal — the import path previously skipped it,
  // which is why a pre-fix Peacebringer share showed no Energy/Combat Flight.
  if (build.inherents.length > 0 && build.archetype.id) {
    const desired = getInherentSelectedPowers(
      build.archetype.id,
      build.archetype.name || undefined,
      build.archetype.inherent,
      build.level,
    );
    const have = new Set(build.inherents.map((p) => p.internalName));
    const missing = desired.filter((p) => !have.has(p.internalName));
    if (missing.length > 0) {
      build.inherents = [...build.inherents, ...missing];
    }
  }

  // Fix IO set enhancement icons from current data
  const fixEnhancementIcons = (powers: SelectedPower[]) => {
    let anyChanged = false;
    const fixed = powers.map((power) => {
      let powerChanged = false;
      const slots = power.slots.map((slot) => {
        if (slot && slot.type === 'io-set') {
          const enh = slot as Enhancement & { setId?: string; icon?: string };
          const ioSet = enh.setId ? getIOSet(enh.setId) : null;
          if (ioSet?.icon && enh.icon !== ioSet.icon) {
            powerChanged = true;
            return { ...enh, icon: ioSet.icon };
          }
        }
        return slot;
      });
      if (powerChanged) {
        anyChanged = true;
        return { ...power, slots };
      }
      return power;
    });
    return anyChanged ? fixed : powers;
  };

  if (build.primary.powers.length > 0) {
    const fixed = fixEnhancementIcons(build.primary.powers);
    if (fixed !== build.primary.powers) {
      build.primary = { ...build.primary, powers: fixed };
    }
  }
  if (build.secondary.powers.length > 0) {
    const fixed = fixEnhancementIcons(build.secondary.powers);
    if (fixed !== build.secondary.powers) {
      build.secondary = { ...build.secondary, powers: fixed };
    }
  }
  if (build.pools.length > 0) {
    build.pools = build.pools.map((pool) => {
      const fixed = fixEnhancementIcons(pool.powers);
      return fixed !== pool.powers ? { ...pool, powers: fixed } : pool;
    });
  }
  if (build.epicPool && build.epicPool.powers.length > 0) {
    const fixed = fixEnhancementIcons(build.epicPool.powers);
    if (fixed !== build.epicPool.powers) {
      build.epicPool = { ...build.epicPool, powers: fixed };
    }
  }
  if (build.inherents && build.inherents.length > 0) {
    const fixed = fixEnhancementIcons(build.inherents);
    if (fixed !== build.inherents) {
      build.inherents = fixed;
    }
  }

  // Migrate slotOrder entries that reference old internalNames
  if (internalNameMigrations.size > 0 && build.slotOrder) {
    for (const entry of build.slotOrder) {
      const newName = internalNameMigrations.get(entry.powerName);
      if (newName) entry.powerName = newName;
    }
  }

  // Reconcile `activeModes` with what the build's own powers publish.
  //
  // The per-power toggle maintains this incrementally, so a build that arrives already holding
  // an active mode setter never passed through that writer: a saved build, an import, a hash
  // deeplink, or a power that auto-activated at pick time. Those carried `isActive: true` with
  // the mode missing, and a mode-gated redirect stayed unresolved until the user toggled the
  // power off and on again — the Power Boost / Stun case, still broken on every build saved
  // before the writer was fixed.
  //
  // A mode SOME power publishes is decided by that power's own state, which is exactly the
  // engine's `collect_source_modes` over active powers. A mode NO power publishes is left
  // alone: Thunderspy exports no `Set_Mode` template (TSPY-3's excluded tail), so its
  // Hunter/Prowler forms are pure display state with no setter to read, and recomputing over
  // publishers would silently switch them off.
  const allSelected: SelectedPower[] = [
    ...build.primary.powers,
    ...build.secondary.powers,
    ...build.pools.flatMap((pool) => pool.powers),
    ...(build.epicPool?.powers ?? []),
    ...(build.inherents ?? []),
  ];
  const publishable = new Set<string>();
  const live = new Set<string>();
  for (const power of allSelected) {
    const sets = power.setsModes ?? [];
    if (!sets.length) continue;
    const isOn = power.powerType?.toLowerCase() === 'auto' || !!power.isActive;
    for (const mode of sets) {
      publishable.add(mode);
      if (isOn) live.add(mode);
    }
  }
  if (publishable.size > 0) {
    const kept = (build.activeModes ?? []).filter((m) => !publishable.has(m));
    const next = [...new Set([...kept, ...live])];
    const prev = build.activeModes ?? [];
    const changed = next.length !== prev.length || next.some((m) => !prev.includes(m));
    if (changed) build.activeModes = next;
  }
}

/**
 * Find a power across all categories.
 *
 * `categoryHint` is AUTHORITATIVE: when given, the search is confined to that
 * category and a miss returns null. It does NOT fall through to a bare-name
 * search of the other categories.
 *
 * That distinction matters because `internalName` is not unique — the same name
 * lives in two categories in ~23-28 places per dataset ([dominator] `Fire_Blast`
 * = secondary Fire Blast and epic Rain of Fire). Falling through on a miss would
 * answer a question nobody asked ("is there ANY power called X?") and hand back a
 * DIFFERENT power, which callers then mutate via
 * `applyPowerUpdate(found.category, …)` — writing to the wrong power. "Not in the
 * category you named" must read as not-found, not as a cue to go guessing.
 *
 * With no hint the bare search remains, for callers that genuinely have no
 * category (legacy `slotOrder` entries predating the `category` field). It is
 * ambiguous for a collided name and cannot be otherwise — the stored data
 * carries no powerSet to disambiguate with.
 */
/**
 * Power defs for the build's two archetype powersets — where every mode setter and every
 * `modeVariants` table lives. Both the mode selector and the toggle sync read the def rather
 * than the selected power, which carries only the pick.
 */
function archetypePowerDefs(build: Build): Power[] {
  return [
    ...(build.primary.id ? getPowerset(build.primary.id)?.powers ?? [] : []),
    ...(build.secondary.id ? getPowerset(build.secondary.id)?.powers ?? [] : []),
  ];
}

function findPower(
  build: Build,
  powerName: string,
  categoryHint?: PowerCategory
): { power: SelectedPower; category: PowerCategory } | null {
  // A named category is the answer, hit or miss.
  if (categoryHint) return findPowerInCategory(build, powerName, categoryHint);

  // No hint: bare search in standard order, first match wins. (The per-category
  // `categoryHint !== 'x'` guards that used to wrap each step are gone — with the
  // hinted path returning above, categoryHint is always undefined here, so they
  // were dead conditions implying a selectivity this branch does not have.)
  const primaryPower = build.primary.powers.find((p) => p.internalName === powerName);
  if (primaryPower) return { power: primaryPower, category: 'primary' };

  const secondaryPower = build.secondary.powers.find((p) => p.internalName === powerName);
  if (secondaryPower) return { power: secondaryPower, category: 'secondary' };

  for (const pool of build.pools) {
    const poolPower = pool.powers.find((p) => p.internalName === powerName);
    if (poolPower) return { power: poolPower, category: 'pool' };
  }

  if (build.epicPool) {
    const epicPower = build.epicPool.powers.find((p) => p.internalName === powerName);
    if (epicPower) return { power: epicPower, category: 'epic' };
  }

  const inherentPower = build.inherents.find((p) => p.internalName === powerName);
  if (inherentPower) return { power: inherentPower, category: 'inherent' };

  return null;
}

/** Find a power in a specific category only. */
function findPowerInCategory(
  build: Build,
  powerName: string,
  category: PowerCategory
): { power: SelectedPower; category: PowerCategory } | null {
  switch (category) {
    case 'primary': {
      const p = build.primary.powers.find((p) => p.internalName === powerName);
      return p ? { power: p, category: 'primary' } : null;
    }
    case 'secondary': {
      const p = build.secondary.powers.find((p) => p.internalName === powerName);
      return p ? { power: p, category: 'secondary' } : null;
    }
    case 'pool': {
      for (const pool of build.pools) {
        const p = pool.powers.find((p) => p.internalName === powerName);
        if (p) return { power: p, category: 'pool' };
      }
      return null;
    }
    case 'epic': {
      const p = build.epicPool?.powers.find((p) => p.internalName === powerName);
      return p ? { power: p, category: 'epic' } : null;
    }
    case 'inherent': {
      const p = build.inherents.find((p) => p.internalName === powerName);
      return p ? { power: p, category: 'inherent' } : null;
    }
    default:
      return null;
  }
}

/**
 * Count placed (additional) slots that count against the slot budget.
 * Excludes the free first slot on each power.
 * Excludes inherent slot grants (Health/Stamina Fitness auto-slots),
 * which the game gives for free outside the 67-slot budget.
 * Inherents with maxSlots=0 (archetype inherents) have no slots so they contribute 0.
 */
function countPlacedSlots(build: Build): number {
  let total = 0;

  const countExtra = (powers: SelectedPower[]) => {
    for (const power of powers) {
      const inherent = power.inherentSlotCount ?? 0;
      total += Math.max(0, power.slots.length - 1 - inherent);
    }
  };

  countExtra(build.primary.powers);
  countExtra(build.secondary.powers);
  for (const pool of build.pools) countExtra(pool.powers);
  if (build.epicPool) countExtra(build.epicPool.powers);
  countExtra(build.inherents);

  return total;
}

/**
 * Get the number of placeable slots available at a given level.
 * This is the total slot grants (67 at level 50 on most servers; 71 on
 * Thunderspy). Free first slots from powers are separate and don't count
 * against this budget.
 */
function getPlacedSlotLimit(level: number, serverId?: string): number {
  return getTotalSlotsAtLevel(level, serverId);
}

/**
 * Count total slots used across all powers (including free and inherent).
 * Used for display purposes, not budget checks.
 */
function countTotalSlots(build: Build): number {
  let total = 0;

  // Primary powers
  for (const power of build.primary.powers) {
    total += power.slots.length;
  }

  // Secondary powers
  for (const power of build.secondary.powers) {
    total += power.slots.length;
  }

  // Pool powers
  for (const pool of build.pools) {
    for (const power of pool.powers) {
      total += power.slots.length;
    }
  }

  // Epic pool powers
  if (build.epicPool) {
    for (const power of build.epicPool.powers) {
      total += power.slots.length;
    }
  }

  // Inherent powers (some can be slotted)
  for (const power of build.inherents) {
    total += power.slots.length;
  }

  return total;
}

/**
 * Get the counterpart ATO set ID (regular ↔ superior).
 * Returns undefined if the set is not an ATO or has no counterpart.
 */
function getATOCounterpartSetId(setId: string): string | undefined {
  const set = getIOSet(setId);
  if (!set || set.category !== 'ato') return undefined;

  if (setId.startsWith('superior_')) {
    const regularId = setId.slice('superior_'.length);
    const regularSet = getIOSet(regularId);
    return regularSet ? regularId : undefined;
  } else {
    const superiorId = `superior_${setId}`;
    const superiorSet = getIOSet(superiorId);
    return superiorSet ? superiorId : undefined;
  }
}

/**
 * Check if a unique enhancement is already slotted anywhere in the build.
 * Also treats all pieces from purple, event, and archetype (ATO) sets as unique,
 * since the game enforces single-copy rules for these rarities.
 * For ATOs, also checks the counterpart set (regular ↔ superior) since
 * you cannot use both versions of the same ATO in a build.
 * @param build - The current build
 * @param setId - The IO set ID
 * @param pieceNum - The piece number within the set
 * @returns true if the enhancement is already slotted
 */
function isUniqueEnhancementSlotted(build: Build, setId: string, pieceNum: number): boolean {
  // Build the list of set IDs to check: the set itself + its ATO counterpart
  const counterpartId = getATOCounterpartSetId(setId);
  const setIdsToCheck = counterpartId ? [setId, counterpartId] : [setId];

  const checkSlots = (slots: (Enhancement | null)[]): boolean => {
    return slots.some((enh) => {
      if (!enh || enh.type !== 'io-set') return false;
      const ioEnh = enh as { setId: string; pieceNum: number };
      return setIdsToCheck.includes(ioEnh.setId) && ioEnh.pieceNum === pieceNum;
    });
  };

  // Check all power categories
  for (const power of build.primary.powers) {
    if (checkSlots(power.slots)) return true;
  }
  for (const power of build.secondary.powers) {
    if (checkSlots(power.slots)) return true;
  }
  for (const pool of build.pools) {
    for (const power of pool.powers) {
      if (checkSlots(power.slots)) return true;
    }
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) {
      if (checkSlots(power.slots)) return true;
    }
  }
  for (const power of build.inherents) {
    if (checkSlots(power.slots)) return true;
  }

  return false;
}


// Set tracking extracted to src/utils/calculations/set-tracking.ts
const updateSetTracking = computeSetTracking;

/**
 * For VEATs: if build.primary.id or secondary.id is a branch powerset,
 * normalize it back to the base powerset. The planner expects these to
 * always be base powersets, with branch powers stored alongside in the
 * powers array.
 *
 * Also moves powers that ended up in the wrong role array (primary vs
 * secondary). Older builds — and at least one stale Mids import path —
 * stored Night Widow Training primary powers (Build_Up, Slash,
 * Smoke_Grenade) inside the secondary Teamwork array, where hydration
 * couldn't find them and fell through to the iconless stub. The mover
 * compares each power against the archetype's full primary + secondary
 * def sets (base + every branch) and shuffles any that land in the
 * wrong role.
 */
function normalizeBranchPowersets(build: Build): void {
  const archetype = build.archetype.id ? getArchetype(build.archetype.id) : null;
  if (!archetype?.branches) return;

  for (const branchDef of Object.values(archetype.branches)) {
    if (build.primary.id === branchDef.primarySet) {
      const basePowerset = getPowerset(archetype.primarySets[0]);
      if (basePowerset) {
        build.primary.id = archetype.primarySets[0];
        build.primary.name = basePowerset.name;
      }
    }
    if (build.secondary.id === branchDef.secondarySet) {
      const basePowerset = getPowerset(archetype.secondarySets[0]);
      if (basePowerset) {
        build.secondary.id = archetype.secondarySets[0];
        build.secondary.name = basePowerset.name;
      }
    }
  }

  // Build a role lookup: every powerset reachable from this archetype
  // (base primary, base secondary, branch primaries, branch secondaries)
  // → its role. Two indexes per role: by lowercased internalName (strong
  // match) and by lowercased display name (covers builds that pre-date
  // the NW_/Frt_ internalName renames — e.g. their `Slash` still lines
  // up with the current `NW_Slash` power via shared display name).
  type RoleIndex = {
    byInternal: Map<string, { powerSetId: string }>;
    byName: Map<string, { powerSetId: string }>;
  };
  const makeIndex = (): RoleIndex => ({ byInternal: new Map(), byName: new Map() });
  const indexes: Record<'primary' | 'secondary', RoleIndex> = {
    primary: makeIndex(),
    secondary: makeIndex(),
  };

  const indexPowerset = (role: 'primary' | 'secondary', setId: string | undefined): void => {
    if (!setId) return;
    const ps = getPowerset(setId);
    if (!ps) return;
    for (const p of ps.powers) {
      const idx = indexes[role];
      const inKey = p.internalName.toLowerCase();
      const nameKey = p.name.toLowerCase();
      // First write wins — base set takes precedence over branch sets, so
      // a power present in both surfaces with the base powerset id.
      if (!idx.byInternal.has(inKey)) idx.byInternal.set(inKey, { powerSetId: setId });
      if (!idx.byName.has(nameKey)) idx.byName.set(nameKey, { powerSetId: setId });
    }
  };

  // Base sets first so they win on collisions, then branch sets.
  for (const setId of archetype.primarySets) indexPowerset('primary', setId);
  for (const setId of archetype.secondarySets) indexPowerset('secondary', setId);
  for (const branch of Object.values(archetype.branches)) {
    if (!branch) continue;
    indexPowerset('primary', branch.primarySet);
    indexPowerset('secondary', branch.secondarySet);
  }

  // Classify a power: which role does it belong to? Returns null when no
  // index matches — those stay put rather than being moved blindly.
  const classify = (power: SelectedPower): 'primary' | 'secondary' | null => {
    const inKey = power.internalName.toLowerCase();
    const nameKey = power.name.toLowerCase();
    if (indexes.primary.byInternal.has(inKey)) return 'primary';
    if (indexes.secondary.byInternal.has(inKey)) return 'secondary';
    if (indexes.primary.byName.has(nameKey)) return 'primary';
    if (indexes.secondary.byName.has(nameKey)) return 'secondary';
    return null;
  };

  const keepPrimary: SelectedPower[] = [];
  const keepSecondary: SelectedPower[] = [];
  const moves: string[] = [];

  // Refuse to move if the target role's powerset id is null — there's no
  // valid `powerSet` to assign and moving would just orphan it elsewhere.
  const canMoveToPrimary = build.primary.id !== null;
  const canMoveToSecondary = build.secondary.id !== null;

  for (const p of build.primary.powers) {
    const role = classify(p);
    if (role === 'secondary' && canMoveToSecondary) {
      keepSecondary.push({ ...p, powerSet: build.secondary.id as string });
      moves.push(`${p.internalName} primary→secondary`);
    } else {
      keepPrimary.push(p);
    }
  }
  for (const p of build.secondary.powers) {
    const role = classify(p);
    if (role === 'primary' && canMoveToPrimary) {
      keepPrimary.push({ ...p, powerSet: build.primary.id as string });
      moves.push(`${p.internalName} secondary→primary`);
    } else {
      keepSecondary.push(p);
    }
  }

  if (moves.length > 0) {
    build.primary = { ...build.primary, powers: keepPrimary };
    build.secondary = { ...build.secondary, powers: keepSecondary };
    console.warn(
      `[normalizeBranchPowersets] Moved ${moves.length} misplaced power(s) for ${archetype.name}: ${moves.join(', ')}`
    );
  }
}

/**
 * For VEATs: detect which branch the build's powers belong to.
 * Scans primary/secondary power names against each branch's powerset definitions.
 */
function detectBranch(build: Build): ArchetypeBranchId | null {
  const archetype = build.archetype.id ? getArchetype(build.archetype.id) : null;
  if (!archetype?.branches) return null;

  const allPowerNames = new Set([
    ...build.primary.powers.map((p) => p.internalName.toLowerCase()),
    ...build.secondary.powers.map((p) => p.internalName.toLowerCase()),
  ]);

  for (const [branchId, branch] of Object.entries(archetype.branches)) {
    if (!branch) continue;
    const branchPrimary = branch.primarySet ? getPowerset(branch.primarySet) : null;
    const branchSecondary = getPowerset(branch.secondarySet);
    const branchPowerNames = [
      ...(branchPrimary?.powers ?? []).map((p) => p.internalName.toLowerCase()),
      ...(branchSecondary?.powers ?? []).map((p) => p.internalName.toLowerCase()),
    ];
    if (branchPowerNames.some((name) => allPowerNames.has(name))) {
      return branchId as ArchetypeBranchId;
    }
  }
  return null;
}

/**
 * Apply a power array updater to the correct category in a build.
 * Eliminates the repeated switch-on-PowerCategory pattern.
 */
function applyPowerUpdate(
  build: Build,
  category: PowerCategory,
  updater: (powers: SelectedPower[]) => SelectedPower[]
): Build {
  const newBuild = { ...build };

  switch (category) {
    case 'primary':
      newBuild.primary = { ...newBuild.primary, powers: updater(newBuild.primary.powers) };
      break;
    case 'secondary':
      newBuild.secondary = { ...newBuild.secondary, powers: updater(newBuild.secondary.powers) };
      break;
    case 'pool':
      newBuild.pools = newBuild.pools.map(pool => ({ ...pool, powers: updater(pool.powers) }));
      break;
    case 'epic':
      if (newBuild.epicPool) {
        newBuild.epicPool = { ...newBuild.epicPool, powers: updater(newBuild.epicPool.powers) };
      }
      break;
    case 'inherent':
      newBuild.inherents = updater(newBuild.inherents);
      break;
  }

  return newBuild;
}

/**
 * Apply a power array updater to ALL non-inherent categories.
 * Used for operations like togglePowerActive that apply across the whole build.
 */
function applyToAllPowers(
  build: Build,
  updater: (powers: SelectedPower[]) => SelectedPower[]
): Build {
  return {
    ...build,
    primary: { ...build.primary, powers: updater(build.primary.powers) },
    secondary: { ...build.secondary, powers: updater(build.secondary.powers) },
    pools: build.pools.map(pool => ({ ...pool, powers: updater(pool.powers) })),
    epicPool: build.epicPool
      ? { ...build.epicPool, powers: updater(build.epicPool.powers) }
      : null,
    // Inherent powers (Sprint, Brawl, Rest, archetype inherent, etc.) need
    // the same treatment — otherwise togglePowerActive looks up Sprint in
    // `build.inherents`, computes the new isActive value, then writes it
    // nowhere, leaving the toggle visually stuck. Callers that explicitly
    // re-set `inherents` after this call (clearPowers, clearAllExtraSlots)
    // still win; they just override our pass-through with their own.
    inherents: updater(build.inherents),
  };
}

/**
 * Convert an InherentPowerDef to a SelectedPower.
 *
 * Per-server adjustments (e.g. Rebirth's L2 Fitness availability and
 * auto-granted Health/Stamina slots) are applied via the dataset's
 * inherent-rules hooks so each server can plug in its own variations
 * without touching this code.
 */
function createInherentSelectedPower(def: InherentPowerDef, characterLevel = 50): SelectedPower {
  // Archetype inherents have 0 maxSlots and should have no slots
  const slots: (Enhancement | null)[] = def.maxSlots === 0 ? [] : [null];
  // Resolve the effective `available` value: server override wins over the
  // shared default.
  const level = getInherentGrantLevel(def);

  // Pre-fill any auto-granted inherent slots (e.g. Rebirth Health/Stamina).
  const inherentSlotCount = getInherentAutoGrantedSlotCount(def.internalName, characterLevel);
  for (let i = 0; i < inherentSlotCount; i++) slots.push(null);

  return {
    ...def,
    powerSet: 'Inherent',
    level,
    slots,
    isLocked: def.isLocked ?? true, // All inherent powers are locked by default
    inherentCategory: def.category,
    ...(inherentSlotCount > 0 ? { inherentSlotCount } : {}),
  };
}

/**
 * Get all inherent powers as SelectedPower objects
 * @param archetypeId - The archetype ID for archetype-specific inherents (e.g. 'peacebringer')
 * @param archetypeName - The archetype name for the archetype-specific inherent
 * @param archetypeInherent - The archetype's inherent power definition
 */
function getInherentSelectedPowers(
  archetypeId?: string | null,
  archetypeName?: string,
  archetypeInherent?: { name: string; description: string } | null,
  characterLevel = 50,
): SelectedPower[] {
  const powers = getInherentPowers().map((def) => createInherentSelectedPower(def, characterLevel));

  // Add archetype-specific inherent if provided
  if (archetypeName && archetypeInherent) {
    const atInherentDef = createArchetypeInherentPower(archetypeName, archetypeInherent);
    // Insert archetype inherent at the beginning
    powers.unshift(createInherentSelectedPower(atInherentDef, characterLevel));
  }

  // Add archetype-specific inherent powers (e.g. Kheldian travel powers)
  const extraInherents = getArchetypeInherentPowers(archetypeId || undefined);
  for (const def of extraInherents) {
    powers.push(createInherentSelectedPower(def, characterLevel));
  }

  return powers;
}

/**
 * Reconcile auto-granted inherent slot counts (e.g. Rebirth Health/Stamina
 * grants at L8/L16/L12/L22) against the supplied level. Adds trailing empty
 * slots when the level newly qualifies; removes only trailing empty slots
 * when the level drops, to preserve user-placed enhancements at indices
 * outside the inherent range. Mirrors the inline logic in `setLevel` so
 * other level-mutating paths (e.g. auto-advance on power pick) stay in sync.
 */
function reconcileInherentSlots(inherents: SelectedPower[], level: number): SelectedPower[] {
  return inherents.map((p) => {
    const want = getInherentAutoGrantedSlotCount(p.internalName, level);
    const have = p.inherentSlotCount ?? 0;
    if (want === have) return p;
    const slots = [...p.slots];
    if (want > have) {
      for (let i = 0; i < want - have; i++) slots.push(null);
    } else {
      let toRemove = have - want;
      for (let i = slots.length - 1; i >= 0 && toRemove > 0; i--) {
        if (slots[i] === null) {
          slots.splice(i, 1);
          toRemove--;
        } else break;
      }
    }
    return {
      ...p,
      slots,
      ...(want > 0 ? { inherentSlotCount: want } : { inherentSlotCount: undefined }),
    };
  });
}

/**
 * Count total selected powers (excluding inherents and auto-granted form sub-powers)
 */
function countSelectedPowers(build: Build): number {
  const countNonGranted = (powers: SelectedPower[]) =>
    powers.filter(p => !p.isAutoGranted).length;

  return (
    countNonGranted(build.primary.powers) +
    countNonGranted(build.secondary.powers) +
    build.pools.reduce((sum, pool) => sum + countNonGranted(pool.powers), 0) +
    (build.epicPool ? countNonGranted(build.epicPool.powers) : 0)
  );
}

/**
 * Collect the set of pick levels already occupied by existing powers.
 */
function getOccupiedLevels(build: Build): Set<number> {
  const occupied = new Set<number>();
  const collectNonGranted = (powers: SelectedPower[]) =>
    powers.filter(p => !p.isAutoGranted).forEach(p => occupied.add(p.level));

  collectNonGranted(build.primary.powers);
  collectNonGranted(build.secondary.powers);
  build.pools.forEach((pool) => collectNonGranted(pool.powers));
  if (build.epicPool) collectNonGranted(build.epicPool.powers);
  return occupied;
}

/**
 * Calculate the correct build level based on the current number of selected powers.
 * Works bidirectionally — advances when powers are added, rewinds when removed.
 * Checks which pick levels are actually occupied to avoid assigning duplicates.
 */
export function calculateCorrectLevel(build: Build): number {
  // Level 1 special: need both a primary and secondary power before advancing
  const hasPrimary = build.primary.powers.length >= 1;
  const hasSecondary = build.secondary.powers.length >= 1;
  if (!hasPrimary || !hasSecondary) {
    return 1;
  }

  // Check which pick levels already have a power assigned
  const occupied = getOccupiedLevels(build);

  // Level 1 gets two picks — only consider it "full" if both primary and secondary exist
  const level1Count = [
    ...build.primary.powers.filter(p => !p.isAutoGranted),
    ...build.secondary.powers.filter(p => !p.isAutoGranted),
  ].filter(p => p.level === 1).length;

  // Find the first unoccupied pick level
  // Level 1 is special: it needs 2 powers, so only skip it if both slots are filled
  for (const level of POWER_PICK_LEVELS) {
    if (level === 1) {
      if (level1Count < 2) return 1;
    } else if (!occupied.has(level)) {
      return level;
    }
  }

  // All 24 picks used — advance to max level so final slots are unlocked
  return 50;
}

/**
 * Whether a category already owns its level-1 pick. Level 1 is character
 * creation: exactly one primary and one secondary pick. WHICH power fills each
 * is the player's choice among whatever the set offers at level 1 — Homecoming
 * opened the secondary's tier-2 power at creation in Issue 27 Page 5, retiring
 * the old forced first pick — but the count still holds, so once a category
 * holds a level-1 power the rest of that category floors at 2.
 */
export function categoryOwnsLevelOne(build: Build, category: 'primary' | 'secondary'): boolean {
  return build[category].powers.some((p) => !p.isAutoGranted && p.level === 1);
}

/**
 * Relevel every pick onto a legal, unoccupied pick level. Runs when any
 * power sits on a level that isn't a pick level, duplicates another pick,
 * or sits below the level the game first offers it. Powers keep their
 * relative order; each takes the earliest free pick level at or above its
 * floor, so the result is a build the picker could have produced click by
 * click. Shared by rehydration, importBuild, and addPower's no-free-level
 * case (B4: a silent level-50 stamp made the power invisible in the
 * by-level view while it stayed selected and slotted).
 */
export function relevelInvalidPicks(build: Build): void {
    // `poolIndex` is the only record of which pool a power came from
    // that survives the global sort below — the sort interleaves pools,
    // so position in this list stops implying pool membership.
    const allPowers: { power: SelectedPower; category: string; index: number; poolIndex?: number }[] = [];
    // Exclude auto-granted form sub-powers from level migration
    build.primary.powers.filter(p => !p.isAutoGranted).forEach((p, i) => allPowers.push({ power: p, category: 'primary', index: i }));
    build.secondary.powers.filter(p => !p.isAutoGranted).forEach((p, i) => allPowers.push({ power: p, category: 'secondary', index: i }));
    build.pools.forEach((pool, poolIndex) => pool.powers.filter(p => !p.isAutoGranted).forEach((p, i) => allPowers.push({ power: p, category: 'pool', index: i, poolIndex })));
    if (build.epicPool) {
      build.epicPool.powers.filter(p => !p.isAutoGranted).forEach((p, i) => allPowers.push({ power: p, category: 'epic', index: i }));
    }

    // The floor addPower applies at pick time, re-applied here: a power may
    // not sit below max(available + 1, its category's unlock floor). The
    // def sync above has already re-attached `available`, so the floor is
    // readable even though the slim serializer drops it.
    const minPickLevel = (entry: { power: SelectedPower; category: string }): number => {
      const categoryMin = entry.category === 'pool' ? getPoolUnlockLevel(build.serverId)
        : entry.category === 'epic' ? EPIC_POOL_LEVEL
        : 1;
      return Math.max((entry.power.available ?? 0) + 1, categoryMin);
    };

    // Check if any non-inherent power has a level that isn't a valid pick level,
    // OR if there are duplicate levels (e.g., 6 primaries all at level 1)
    const pickLevelSet = new Set(POWER_PICK_LEVELS);
    const hasInvalidLevels = allPowers.some((entry) => !pickLevelSet.has(entry.power.level));

    // A level that IS a valid pick level can still sit below the level the
    // game first offers the power — the earlier positional relevel produced
    // exactly that (a Defender's Distortion Field on the level-6 pick when
    // the game offers it at 8). Such builds re-enter the relevel here.
    const hasBelowUnlockLevels = allPowers.some((entry) => entry.power.level < minPickLevel(entry));

    // Detect duplicate levels: count how many powers occupy each pick.
    // Level 1 holds two picks, but one PRIMARY and one SECONDARY — two
    // level-1 powers from a single category are as impossible as two
    // powers anywhere else — so its occupancy counts per category.
    let hasDuplicateLevels = false;
    if (!hasInvalidLevels && allPowers.length > 0) {
      const slotCounts = new Map<string, number>();
      for (const entry of allPowers) {
        const slot = entry.power.level === 1 ? `1:${entry.category}` : `${entry.power.level}`;
        slotCounts.set(slot, (slotCounts.get(slot) || 0) + 1);
      }
      hasDuplicateLevels = [...slotCounts.values()].some((count) => count > 1);
    }

    if ((hasInvalidLevels || hasDuplicateLevels || hasBelowUnlockLevels) && allPowers.length > 0) {
      // Level 1 is special: one primary + one secondary. Pull those out first
      // to guarantee they get level 1, regardless of how many of each exist —
      // but only powers the game offers at level 1: a partial build whose
      // earliest surviving power is a high-tier pick keeps it at its unlock
      // level rather than dragging it to 1.
      const firstPrimaryIdx = allPowers.findIndex((e) => e.category === 'primary' && minPickLevel(e) <= 1);
      const firstSecondaryIdx = allPowers.findIndex((e) => e.category === 'secondary' && minPickLevel(e) <= 1);

      const level1Powers: typeof allPowers = [];
      const restPowers: typeof allPowers = [];

      allPowers.forEach((entry, idx) => {
        if (idx === firstPrimaryIdx || idx === firstSecondaryIdx) {
          level1Powers.push(entry);
        } else {
          restPowers.push(entry);
        }
      });

      // Sort level 1 powers: primary before secondary
      level1Powers.sort((a, b) => {
        const order = { primary: 0, secondary: 1, pool: 2, epic: 3 };
        return (order[a.category as keyof typeof order] ?? 9) -
               (order[b.category as keyof typeof order] ?? 9);
      });

      // Sort remaining powers by their original level, then by category
      const categoryOrder = { primary: 0, secondary: 1, pool: 2, epic: 3 };
      restPowers.sort((a, b) => {
        if (a.power.level !== b.power.level) return a.power.level - b.power.level;
        return (categoryOrder[a.category as keyof typeof categoryOrder] ?? 9) -
               (categoryOrder[b.category as keyof typeof categoryOrder] ?? 9);
      });

      // Recombine: level 1 powers first, then the rest
      const sorted = [...level1Powers, ...restPowers];

      // Assign pick levels sequentially: the level-1 pair first, then each
      // remaining power takes the EARLIEST free pick level at or above its
      // own floor — the same placement addPower's out-of-order path makes,
      // so a relevelled build is one the picker could have produced click
      // by click. Positional stamping (slot index → level) ignored the
      // floor and produced picks the game refuses to grant.
      let anyChanged = false;
      const assign = (entry: (typeof allPowers)[number], level: number) => {
        if (entry.power.level !== level) {
          entry.power = { ...entry.power, level };
          anyChanged = true;
        }
      };
      level1Powers.forEach((entry) => assign(entry, 1));
      const laterSlots = POWER_PICK_LEVELS.slice(1);
      const slotFilled = laterSlots.map(() => false);
      restPowers.forEach((entry) => {
        const floor = minPickLevel(entry);
        const slot = laterSlots.findIndex((level, i) => !slotFilled[i] && level >= floor);
        if (slot === -1) {
          // More powers than legal picks — overflow pins to the last pick
          // level, exactly as the positional stamping overflowed before.
          assign(entry, POWER_PICK_LEVELS[POWER_PICK_LEVELS.length - 1]);
        } else {
          slotFilled[slot] = true;
          assign(entry, laterSlots[slot]);
        }
      });

      // Use sorted array for write-back
      allPowers.length = 0;
      allPowers.push(...sorted);

      if (anyChanged) {
        // The write-back below rebuilds each category from `allPowers`,
        // which EXCLUDES auto-granted form sub-powers (Kheldian
        // Nova/Dwarf, Primalist Hunter/Prowler) — they don't occupy a
        // pick slot, so they're rightly left out of the relevelling.
        // But they're still in the build arrays, and rebuilding from
        // the filtered list alone deletes them together with every
        // slot and enhancement the user put in them. Carry them over,
        // restamped to their parent's corrected pick level (addPower
        // keeps a sub-power at its form's level; leaving it at the
        // stale one misreports slot placement — see
        // addAutoGrantedPowers in slot-levels.ts).
        const correctedLevels = new Map<string, number>();
        for (const entry of allPowers) {
          correctedLevels.set(`${entry.category}|${entry.power.internalName}`, entry.power.level);
        }
        const carryGranted = (powers: SelectedPower[], category: string): SelectedPower[] =>
          powers
            .filter((p) => p.isAutoGranted)
            .map((p) => {
              const parentLevel = p.grantedByPower
                ? correctedLevels.get(`${category}|${p.grantedByPower}`)
                : undefined;
              return parentLevel !== undefined && parentLevel !== p.level
                ? { ...p, level: parentLevel }
                : p;
            });

        // Write fixed levels back to the build
        const fixedPrimary = allPowers.filter((e) => e.category === 'primary').map((e) => e.power);
        const fixedSecondary = allPowers.filter((e) => e.category === 'secondary').map((e) => e.power);

        if (fixedPrimary.length > 0) {
          build.primary = {
            ...build.primary,
            powers: [...fixedPrimary, ...carryGranted(build.primary.powers, 'primary')],
          };
        }
        if (fixedSecondary.length > 0) {
          build.secondary = {
            ...build.secondary,
            powers: [...fixedSecondary, ...carryGranted(build.secondary.powers, 'secondary')],
          };
        }

        // Fix pool powers — regroup by the pool each power was
        // collected from. Pools are the one category that is a list OF
        // lists, so the write-back has to re-partition a list the sort
        // has reordered; partitioning it positionally would hand each
        // pool the right COUNT of powers and the wrong ones, which
        // reads as an intact build that computes low (the engine keys
        // pool powers `<poolId>/<internalName>` and drops the misses).
        const fixedPoolPowers = allPowers.filter((e) => e.category === 'pool');
        if (fixedPoolPowers.length > 0) {
          build.pools = build.pools.map((pool, poolIndex) => ({
            ...pool,
            powers: [
              ...fixedPoolPowers.filter((e) => e.poolIndex === poolIndex).map((e) => e.power),
              ...carryGranted(pool.powers, 'pool'),
            ],
          }));
        }

        // Fix epic powers
        const fixedEpic = allPowers.filter((e) => e.category === 'epic').map((e) => e.power);
        if (fixedEpic.length > 0 && build.epicPool) {
          build.epicPool = {
            ...build.epicPool,
            powers: [...fixedEpic, ...carryGranted(build.epicPool.powers, 'epic')],
          };
        }
      }
    }
}

// ============================================
// STORE CREATION
// ============================================

export const useBuildStore = create<BuildStore>()(
  persist(
    (set, get) => {
      // Undo/redo checkpoint helper — call before mutations
      const historyCheckpoint = () => {
        const { _isRestoring } = useHistoryStore.getState();
        if (!_isRestoring) {
          useHistoryStore.getState().checkpoint(get().build);
        }
      };

      return ({
      // Initial state
      build: createEmptyBuild(),
      _hasHydrated: false,
      _inactiveServerBuilds: {},
      lastImportNotes: [],

      // Hydration tracking
      setHasHydrated: (value) => set({ _hasHydrated: value }),

      // History restore (used by undo/redo)
      _restoreBuild: (build) => {
        set({ build });
        // History snapshots the Build only. Mirror the build-derived UI state
        // that other mutation paths (e.g. importBuild) set alongside the build,
        // so undo/redo doesn't leave it stale: the VEAT branch picker. (Kheldian
        // form lives on the build itself, so set({ build }) already restores it.)
        useUIStore.getState().setSelectedBranch(detectBranch(build));
      },

      // Build metadata
      setBuildName: (name) => {
        historyCheckpoint();
        set((state) => ({
          build: { ...state.build, name },
        }));
      },

      // Archetype
      setArchetype: (archetypeId) => {
        const archetype = getArchetype(archetypeId);
        if (!archetype) return;
        historyCheckpoint();

        set((state) => ({
          build: {
            ...state.build,
            archetype: {
              id: archetypeId,
              name: archetype.name,
              stats: archetype.stats,
              inherent: archetype.inherent,
            },
            // Reset powersets when archetype changes
            primary: { id: null, name: '', powers: [] },
            secondary: { id: null, name: '', powers: [] },
            pools: [],
            epicPool: null,
            inherents: [], // Clear inherents when archetype changes
          },
        }));
      },

      clearArchetype: () => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            archetype: { id: null, name: '', stats: null, inherent: null },
            primary: { id: null, name: '', powers: [] },
            secondary: { id: null, name: '', powers: [] },
            pools: [],
            epicPool: null,
            inherents: [], // Clear inherents
          },
        }));
      },

      // Powersets
      setPrimary: (powersetId) => {
        const powerset = getPowerset(powersetId);
        if (!powerset) return;
        historyCheckpoint();

        set((state) => {
          const removedNames = new Set(state.build.primary.powers.map((p) => p.internalName));
          const newBuild = {
            ...state.build,
            primary: {
              id: powersetId,
              name: powerset.name,
              powers: [], // Clear powers when powerset changes
            },
            slotOrder: state.build.slotOrder.filter((e) => !removedNames.has(e.powerName)),
            procOverrides: pruneProcOverridesForRemovedPowers(state.build.procOverrides, removedNames),
          };

          // Auto-grant inherent powers if both powersets are now selected
          if (newBuild.secondary.id && newBuild.inherents.length === 0) {
            newBuild.inherents = getInherentSelectedPowers(
              state.build.archetype.id,
              state.build.archetype.name || undefined,
              state.build.archetype.inherent,
              state.build.level,
            );
          }

          return { build: newBuild };
        });
      },

      setSecondary: (powersetId) => {
        const powerset = getPowerset(powersetId);
        if (!powerset) return;
        historyCheckpoint();

        set((state) => {
          const removedNames = new Set(state.build.secondary.powers.map((p) => p.internalName));
          const newBuild = {
            ...state.build,
            secondary: {
              id: powersetId,
              name: powerset.name,
              powers: [],
            },
            slotOrder: state.build.slotOrder.filter((e) => !removedNames.has(e.powerName)),
            procOverrides: pruneProcOverridesForRemovedPowers(state.build.procOverrides, removedNames),
          };

          // Auto-grant inherent powers if both powersets are now selected
          if (newBuild.primary.id && newBuild.inherents.length === 0) {
            newBuild.inherents = getInherentSelectedPowers(
              state.build.archetype.id,
              state.build.archetype.name || undefined,
              state.build.archetype.inherent,
              state.build.level,
            );
          }

          return { build: newBuild };
        });
      },

      switchBranch: (branchId) => {
        const archetype = get().build.archetype.id ? getArchetype(get().build.archetype.id!) : null;
        const outgoing = useUIStore.getState().selectedBranch;
        if (outgoing === branchId) return [];

        const orphaned = branchPowersInBuild(get().build, archetype, outgoing);
        if (orphaned.length > 0) {
          historyCheckpoint();
          set((state) => {
            // Match on powerSet, not name: the branch owns the SET, and a branch power can
            // share its name with the base set's.
            const dropped = new Set(branchSetIds(archetype, outgoing));
            const keep = (powers: SelectedPower[]) => powers.filter((p) => !dropped.has(p.powerSet));
            let newBuild = applyPowerUpdate(state.build, 'primary', keep);
            newBuild = applyPowerUpdate(newBuild, 'secondary', keep);

            const removedNames = new Set(orphaned.map((p) => p.internalName));
            newBuild.sets = updateSetTracking(newBuild);
            newBuild.slotOrder = newBuild.slotOrder.filter((e) => !removedNames.has(e.powerName));
            newBuild.procOverrides = pruneProcOverridesForRemovedPowers(newBuild.procOverrides, removedNames);
            return { build: newBuild };
          });
        }

        useUIStore.getState().setSelectedBranch(branchId);
        return orphaned.map((p) => p.name);
      },

      // Powers
      addPower: (category, power) => {
        historyCheckpoint();
        set((state) => {
          // Enforce 24-power limit (inherents don't count)
          if (category !== 'inherent' && countSelectedPowers(state.build) >= MAX_POWER_PICKS) {
            return state;
          }

          // Reject duplicates — a power may only be picked once per category.
          // The UI normally hides already-selected powers, but this is the
          // canonical guard against bugs that bypass that filter (focus
          // retention firing onSelect twice, slow re-renders, etc.).
          if (category !== 'inherent') {
            const existing = category === 'primary' ? state.build.primary.powers
              : category === 'secondary' ? state.build.secondary.powers
              : category === 'epic' ? (state.build.epicPool?.powers ?? [])
              : category === 'pool'
                ? state.build.pools.flatMap((p) => p.powers)
                : [];
            if (existing.some((p) => p.internalName === power.internalName)) {
              return state;
            }
          }

          // Level Up mode: enforce per-level pick quota so the user can't
          // pre-pay picks from future levels. If the cumulative cap at the
          // current character level is already reached, reject the add.
          if (category !== 'inherent' && useUIStore.getState().levelUpMode) {
            const cap = getPowerPicksAtLevel(state.build.level);
            if (countSelectedPowers(state.build) >= cap) {
              return state;
            }
          }

          let needsRelevel = false;

          // Assign a valid pick level for this power.
          // The level must be:
          //   1. A valid POWER_PICK_LEVEL (1, 2, 4, 6, 8, ...)
          //   2. Not already occupied by another power
          //   3. At or above the power's minimum required level (available + 1)
          // This allows picking powers in any order — each gets placed at the
          // earliest legal level, and the chronological view shows them correctly.
          if (category !== 'inherent') {
            // Level 1 is creation: one primary pick and one secondary pick,
            // exactly. Once this category owns its level-1 power, the rest of
            // the category floors at 2 (categoryOwnsLevelOne).
            const categoryMin = category === 'pool' ? getPoolUnlockLevel(state.build.serverId)
              : category === 'epic' ? EPIC_POOL_LEVEL
              : categoryOwnsLevelOne(state.build, category) ? 2
              : 1;
            const minLevel = Math.max((power.available ?? 0) + 1, categoryMin);
            const nextSequential = calculateCorrectLevel(state.build);
            if (nextSequential >= minLevel) {
              power = { ...power, level: nextSequential };
            } else {
              // Power requires a higher level — find first unoccupied pick level >= minLevel
              const occupied = getOccupiedLevels(state.build);
              const assignedLevel = POWER_PICK_LEVELS.find(
                l => l >= minLevel && (l === 1 ? false : !occupied.has(l))
              );
              if (assignedLevel === undefined) {
                // Every pick level at or above the floor is taken (the free
                // picks all sit below it). Stamp a provisional 50 and relevel
                // the whole build below: earlier picks cascade down into the
                // free low levels, freeing a legal level for this power. The
                // old `?? 50` stamp shipped as-is — a level with no pick row,
                // so the power was selected and slotted but invisible in the
                // by-level view (B4, 2026-08-17).
                needsRelevel = true;
                power = { ...power, level: 50 };
              } else {
                power = { ...power, level: assignedLevel };
              }
            }
          }

          // Powers with maxSlots=0 cannot accept enhancement slots (e.g., Reach for the Limit)
          if (power.maxSlots === 0 && power.slots.length > 0) {
            power = { ...power, slots: [] };
          }

          // Default toggle/auto powers to active; also activate Click self-buff powers
          // with long durations (60s+) like Hasten and Practiced Brawler, since they're
          // effectively permanent when maintained. Short-duration clicks like Build Up (10s)
          // are left off by default — users can toggle them on manually.
          if (power.isActive === undefined) {
            const pt = power.powerType?.toLowerCase();
            const buffDuration = (power.effects as Record<string, unknown>)?.buffDuration;
            const isLongSelfBuff = pt === 'click'
              && typeof buffDuration === 'number' && buffDuration >= 60
              && (power.targetType?.toLowerCase() === 'self'
                || (power.shortHelp?.toLowerCase().startsWith('self ') ?? false));
            if (pt === 'toggle' || pt === 'auto' || isLongSelfBuff) {
              power = { ...power, isActive: true };
            }
          }

          // A mode setter's initial isActive must match the mode selector. A freshly picked
          // Bright_Nova with no form selected should NOT auto-activate — that would silently
          // apply its damage buff before the user picks the form. `setsModes` identifies it.
          const settableModes = power.setsModes ?? [];
          if (settableModes.length) {
            const activeModes = new Set(state.build.activeModes ?? []);
            const selectable = new Set(selectableModes(archetypePowerDefs(state.build)));
            const owned = settableModes.filter((m) => selectable.has(m));
            if (owned.length) power = { ...power, isActive: owned.some((m) => activeModes.has(m)) };
          }

          // Enforce mutually exclusive powers (e.g., Slice vs Boomerang Slice)
          // If this power excludes another, check if the excluded power is already picked
          if (power.excludes?.length) {
            const categoryPowers = category === 'primary' ? state.build.primary.powers
              : category === 'secondary' ? state.build.secondary.powers
              : category === 'epic' ? (state.build.epicPool?.powers ?? [])
              : [];
            const hasExcluded = categoryPowers.some(p => power.excludes!.includes(p.internalName));
            if (hasExcluded) return state;
          }

          // Pool case: must target the specific pool by ID
          let newBuild: Build;
          if (category === 'pool') {
            newBuild = { ...state.build };
            const poolIndex = newBuild.pools.findIndex((p) =>
              p.id === power.powerSet || power.powerSet.includes(p.id)
            );
            if (poolIndex >= 0) {
              newBuild.pools = [...newBuild.pools];
              newBuild.pools[poolIndex] = {
                ...newBuild.pools[poolIndex],
                powers: [...newBuild.pools[poolIndex].powers, power],
              };
            }
          } else {
            newBuild = applyPowerUpdate(state.build, category, (powers) => [...powers, power]);
          }

          // Auto-grant slottable sub-powers when a form power is added (e.g., Kheldian forms)
          const formGroup = GRANTED_POWER_GROUPS[power.internalName];
          if (formGroup?.slottable && formGroup.grantedPowers.length > 0) {
            // Find the powerset definition to get full sub-power data
            const powersetId = power.powerSet;
            const powersetDef = powersetId ? getPowerset(powersetId) : null;
            if (powersetDef) {
              const subPowerDefs = powersetDef.powers.filter(p =>
                formGroup.grantedPowers.includes(p.internalName)
              );
              for (const subPowerDef of subPowerDefs) {
                const subPower: SelectedPower = {
                  ...subPowerDef,
                  powerSet: powersetId,
                  level: power.level,
                  slots: [null],
                  isAutoGranted: true,
                  grantedByPower: power.internalName,
                  isActive: (subPowerDef.powerType === 'Toggle' || subPowerDef.powerType === 'Auto') ? true : undefined,
                };
                // Add sub-power to the same category
                newBuild = applyPowerUpdate(newBuild, category, (powers) => [...powers, subPower]);
              }
            }
          }

          // No-free-level add (see above): relevel now that the power is in
          // the build. Runs after sub-power grants so carryGranted restamps
          // them to their parent's corrected level.
          if (needsRelevel) {
            relevelInvalidPicks(newBuild);
          }

          // Auto-advance level if all power picks for current level have been used.
          // Only auto-advance for primary/secondary/pool powers (not inherents).
          // Never decrease — respect user's manually-set level.
          // In Level Up mode, suppress auto-advance entirely: the user must
          // walk through each grant level (including slot-only levels like
          // L3, L5) manually via the Level Up button, so slot-placement phases
          // aren't skipped.
          if (category !== 'inherent' && !useUIStore.getState().levelUpMode) {
            const nextLevel = Math.max(state.build.level, calculateCorrectLevel(newBuild));
            if (nextLevel !== newBuild.level) {
              newBuild.level = nextLevel;
              // Auto-advance crosses Rebirth's L8/L16/L12/L22 thresholds;
              // mirror setLevel's reconciliation so Health/Stamina pick up
              // their auto-granted slots without requiring a manual level
              // change.
              newBuild.inherents = reconcileInherentSlots(newBuild.inherents, nextLevel);
            }
          }

          return { build: newBuild };
        });
      },

      removePower: (category, powerName) => {
        historyCheckpoint();
        set((state) => {
          // Inherent: only allow removal of unlocked powers
          const updater = category === 'inherent'
            ? (powers: SelectedPower[]) => powers.filter((p) => p.internalName !== powerName || p.isLocked)
            : (powers: SelectedPower[]) => powers.filter((p) => p.internalName !== powerName);

          let newBuild = applyPowerUpdate(state.build, category, updater);

          // Also remove auto-granted sub-powers when removing a form power
          const formGroup = GRANTED_POWER_GROUPS[powerName];
          if (formGroup?.slottable && formGroup.grantedPowers.length > 0) {
            const subPowerNames = new Set(formGroup.grantedPowers);
            newBuild = applyPowerUpdate(newBuild, category, (powers) =>
              powers.filter((p) => !subPowerNames.has(p.internalName))
            );
          }

          // Keep the user's current level — don't rewind on removal
          // (addPower auto-advances but removePower should never lower it)

          newBuild.sets = updateSetTracking(newBuild);
          // Remove slotOrder entries for removed power(s)
          const removedNames = new Set([powerName]);
          if (formGroup?.slottable) {
            for (const name of formGroup.grantedPowers) removedNames.add(name);
          }
          newBuild.slotOrder = newBuild.slotOrder.filter((e) => !removedNames.has(e.powerName));
          newBuild.procOverrides = pruneProcOverridesForRemovedPowers(newBuild.procOverrides, removedNames);
          return { build: newBuild };
        });
      },

      movePowerLevel: (category, powerName, newLevel) => {
        historyCheckpoint();
        set((state) => ({
          build: applyPowerUpdate(state.build, category, (powers) =>
            powers.map((p) => (p.internalName === powerName ? { ...p, level: newLevel } : p))
          ),
        }));
      },

      swapPowerLevels: (powerNameA, categoryA, powerNameB, categoryB) => {
        // Level 1 holds one primary and one secondary pick. A cross-category
        // swap moving a power onto level 1 would hand its category a SECOND
        // level-1 pick whenever that category's own is not part of the swap —
        // a state the game cannot produce — so that swap is refused. Same-
        // category swaps only exchange levels the category already holds.
        if (categoryA !== categoryB) {
          const build = get().build;
          const foundA = findPowerInCategory(build, powerNameA, categoryA);
          const foundB = findPowerInCategory(build, powerNameB, categoryB);
          const wouldCrowdLevelOne = (
            incomingCategory: string,
            landingLevel: number | undefined,
          ) =>
            landingLevel === 1
            && (incomingCategory === 'primary' || incomingCategory === 'secondary')
            && categoryOwnsLevelOne(build, incomingCategory);
          if (
            wouldCrowdLevelOne(categoryA, foundB?.power.level)
            || wouldCrowdLevelOne(categoryB, foundA?.power.level)
          ) return;
        }
        historyCheckpoint();
        set((state) => {
          // Resolve STRICTLY within the given category — `findPower`'s bare-name
          // fall-through would be a data-corrupting bug here, not a display one:
          // this reads `.level` off the resolved power and then writes back via
          // `applyPowerUpdate(foundX.category, …)`. For a Dominator swapping epic
          // Rain of Fire (internally `Fire_Blast`), a bare search hits the
          // SECONDARY Fire Blast first, so the swap read the wrong level and
          // rewrote the wrong power. The caller always knows both categories.
          const foundA = findPowerInCategory(state.build, powerNameA, categoryA);
          const foundB = findPowerInCategory(state.build, powerNameB, categoryB);
          if (!foundA || !foundB) return state;

          const levelA = foundA.power.level;
          const levelB = foundB.power.level;

          let newBuild = applyPowerUpdate(state.build, foundA.category, (powers) =>
            powers.map((p) => (p.internalName === powerNameA ? { ...p, level: levelB } : p))
          );
          newBuild = applyPowerUpdate(newBuild, foundB.category, (powers) =>
            powers.map((p) => (p.internalName === powerNameB ? { ...p, level: levelA } : p))
          );

          return { build: newBuild };
        });
      },

      // Pools
      addPool: (poolId) => {
        const state = get();
        if (state.build.pools.length >= getMaxPowerPools(state.build.serverId)) return false;

        const pool = getPowerPool(poolId);
        if (!pool) return false;

        // Check if pool is already selected
        if (state.build.pools.some((p) => p.id === poolId)) return false;

        // Check mutual exclusion (e.g., Sorcery / Experimentation / Force of Will)
        const excluded = getExcludedPools(poolId);
        if (excluded && state.build.pools.some((p) => excluded.includes(p.id))) return false;

        historyCheckpoint();
        set((s) => ({
          build: {
            ...s.build,
            pools: [
              ...s.build.pools,
              {
                id: poolId,
                name: pool.name,
                powers: [],
              },
            ],
          },
        }));

        return true;
      },

      removePool: (poolId) => {
        historyCheckpoint();
        set((state) => {
          const removedPool = state.build.pools.find((p) => p.id === poolId);
          const removedNames = new Set(removedPool?.powers.map((p) => p.internalName) ?? []);
          const newBuild = {
            ...state.build,
            pools: state.build.pools.filter((p) => p.id !== poolId),
            slotOrder: state.build.slotOrder.filter((e) => !removedNames.has(e.powerName)),
            procOverrides: pruneProcOverridesForRemovedPowers(state.build.procOverrides, removedNames),
          };
          newBuild.sets = updateSetTracking(newBuild);
          return { build: newBuild };
        });
      },

      setEpicPool: (poolId) => {
        historyCheckpoint();
        if (!poolId) {
          set((state) => {
            const removedNames = new Set(state.build.epicPool?.powers.map((p) => p.internalName) ?? []);
            const newBuild = {
              ...state.build,
              epicPool: null,
              slotOrder: state.build.slotOrder.filter((e) => !removedNames.has(e.powerName)),
              procOverrides: pruneProcOverridesForRemovedPowers(state.build.procOverrides, removedNames),
            };
            newBuild.sets = updateSetTracking(newBuild);
            return { build: newBuild };
          });
          return;
        }

        // Use the epic pool registry instead of regular power pools
        const pool = getEpicPool(poolId);
        if (!pool) return;

        set((state) => ({
          build: {
            ...state.build,
            epicPool: {
              id: poolId,
              name: pool.displayName || pool.name,
              powers: [],
            },
          },
        }));
      },

      // Slots
      addSlot: (powerName, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return false;

        const { power, category } = found;

        // Check if power can have more slots
        if (power.slots.length >= power.maxSlots) return false;

        // Check total placed slot limit (level-aware)
        // Only count additional slots beyond each power's free first slot.
        // Inherent power slots are excluded entirely from the budget.
        if (countPlacedSlots(state.build) >= getPlacedSlotLimit(state.build.level, state.build.serverId)) return false;

        // Outside Level Up mode a slot carries no dated level at all — a real
        // respec hands the player their full earned budget as one freely
        // assignable pool, with no per-power floor tied to pick order (SLOT-3).
        // The count budget above is the only limit that still applies.
        //
        // In Level Up mode, resolve the assigned grant level *before* the slot
        // exists on the power, so the solver sees the same build it will see in
        // compute. It returns the lowest grant >= the power's pick level that
        // the whole build can spare — re-housing another slot onto a freed
        // lower grant where that is what it takes.
        const levelUpMode = useUIStore.getState().levelUpMode;
        let assignedLevel: number | null = null;
        if (levelUpMode) {
          const pickLevel = category === 'inherent' ? 1 : power.level;
          assignedLevel = findNextAvailableGrantLevel(state.build, pickLevel);
          // No grant the slot could legally occupy: refuse, the way the count budget
          // above refuses. Placing it anyway is what wrote an entry with no level,
          // which the display then filled in with the power's pick level — a level
          // the game may grant no slots at (SLOT-1).
          if (assignedLevel === null) return false;
        }

        historyCheckpoint();
        const newSlotIndex = power.slots.length; // index of the slot being added
        set((s) => {
          const newBuild = applyPowerUpdate(s.build, category, (powers) =>
            powers.map((p) =>
              p.internalName === powerName ? { ...p, slots: [...p.slots, null] } : p
            )
          );
          const newEntry: Build['slotOrder'][number] = {
            powerName,
            slotIndex: newSlotIndex,
            category,
            // Click order is still worth keeping even without a level — it's
            // what a later switch into Level Up mode backfills from (SLOT-3).
            ...(assignedLevel !== null ? { level: assignedLevel } : {}),
          };
          newBuild.slotOrder = [...newBuild.slotOrder, newEntry];
          return { build: newBuild };
        });

        return true;
      },

      removeSlot: (powerName, slotIndex, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return false;

        const { power, category } = found;

        // Can't remove the first slot (it's free with the power)
        if (slotIndex === 0) return false;

        // Check if slot exists
        if (slotIndex >= power.slots.length) return false;

        historyCheckpoint();
        set((s) => {
          const newBuild = applyPowerUpdate(s.build, category, (powers) =>
            powers.map((p) =>
              p.internalName === powerName
                ? { ...p, slots: p.slots.filter((_, i) => i !== slotIndex) }
                : p
            )
          );
          newBuild.sets = updateSetTracking(newBuild);
          // Remove this slot from slotOrder and adjust higher indices for same power
          // Match by both powerName and category to avoid collisions with same-named powers
          const matchesEntry = (e: { powerName: string; category?: string }) =>
            e.powerName === powerName && (!e.category || e.category === category);
          newBuild.slotOrder = newBuild.slotOrder
            .filter((e) => !(matchesEntry(e) && e.slotIndex === slotIndex))
            .map((e) =>
              matchesEntry(e) && e.slotIndex > slotIndex
                ? { ...e, slotIndex: e.slotIndex - 1 }
                : e
            );
          // Drop the removed slot's proc override and shift higher same-power
          // indices down, mirroring the slotOrder reindex above.
          newBuild.procOverrides = reindexProcOverridesForRemovedSlot(
            newBuild.procOverrides,
            powerName,
            slotIndex,
          );
          return { build: newBuild };
        });

        return true;
      },

      moveSlotLevel: (source, target) => {
        const state = get();
        const moved = applySlotLevelMove(state.build, source, target, useUIStore.getState().levelUpMode);
        if (!moved) return false;
        historyCheckpoint();
        set(() => ({ build: moved }));
        return true;
      },

      canMoveSlotLevel: (source, target) => {
        return canMoveSlotLevel(get().build, source, target, useUIStore.getState().levelUpMode);
      },

      canMoveSlot: (source, target) => {
        return canRelocateSlot(get().build, source, target, useUIStore.getState().levelUpMode);
      },

      moveSlot: (source, target) => {
        const state = get();
        const levelUpMode = useUIStore.getState().levelUpMode;
        if (!canRelocateSlot(state.build, source, target, levelUpMode)) {
          return { ok: false, enhancementDropped: false };
        }

        const src = findPower(state.build, source.powerName, source.category);
        const tgt = findPower(state.build, target.powerName, target.category);
        if (!src || !tgt) return { ok: false, enhancementDropped: false };

        const sourceCategory = src.category;
        const targetCategory = tgt.category;
        const sourceIndex = source.slotIndex;

        // The enhancement in the source slot travels with the slot when the
        // destination power accepts it; otherwise the slot relocates empty.
        const movingEnh = src.power.slots[sourceIndex] ?? null;
        const carried = movingEnh && enhancementAllowedInPower(movingEnh, tgt.power) ? movingEnh : null;
        const enhancementDropped = movingEnh != null && carried == null;

        // The new slot lands at the end of the target's row. Removing the
        // source slot never touches the target's own slots array (different
        // power — enforced by canRelocateSlot), so this index stays valid.
        const targetNewIndex = tgt.power.slots.length;
        const targetPickLevel = targetCategory === 'inherent' ? 1 : tgt.power.level;

        historyCheckpoint();
        set((s) => {
          const matchesSource = (e: { powerName: string; category?: string }) =>
            e.powerName === source.powerName && (!e.category || e.category === sourceCategory);

          // 1) Remove the slot from the source power and fix up slotOrder
          //    (drop its entry, shift higher same-power indices down by one) —
          //    mirrors removeSlot.
          let newBuild = applyPowerUpdate(s.build, sourceCategory, (powers) =>
            powers.map((p) =>
              p.internalName === source.powerName
                ? { ...p, slots: p.slots.filter((_, i) => i !== sourceIndex) }
                : p
            )
          );
          newBuild.slotOrder = newBuild.slotOrder
            .filter((e) => !(matchesSource(e) && e.slotIndex === sourceIndex))
            .map((e) =>
              matchesSource(e) && e.slotIndex > sourceIndex
                ? { ...e, slotIndex: e.slotIndex - 1 }
                : e
            );
          // Carry the moved slot's proc override to its destination key when the
          // enhancement travels; then reindex the source power's remaining keys.
          const movedOverride = carried
            ? s.build.procOverrides?.[procOverrideKey(source.powerName, sourceIndex)]
            : undefined;
          newBuild.procOverrides = reindexProcOverridesForRemovedSlot(
            newBuild.procOverrides,
            source.powerName,
            sourceIndex,
          );
          if (movedOverride) {
            newBuild.procOverrides = {
              ...(newBuild.procOverrides ?? {}),
              [procOverrideKey(target.powerName, targetNewIndex)]: movedOverride,
            };
          }

          // 2) Resolve the destination grant level against the POST-removal
          //    slotOrder (so the freed source grant is back in the pool), then
          //    append the slot to the target power — mirrors addSlot. Outside
          //    Level Up mode there is no dated level to resolve (SLOT-3).
          const assignedLevel = levelUpMode
            ? findNextAvailableGrantLevel(newBuild, targetPickLevel)
            : null;
          newBuild = applyPowerUpdate(newBuild, targetCategory, (powers) =>
            powers.map((p) =>
              p.internalName === target.powerName
                ? { ...p, slots: [...p.slots, carried] }
                : p
            )
          );
          // `canRelocateSlot` above already refused a target the schedule cannot
          // serve, so this resolves; the guard keeps a level-less entry from
          // being written if the two ever drift apart.
          const newEntry: Build['slotOrder'][number] = {
            powerName: target.powerName,
            slotIndex: targetNewIndex,
            category: targetCategory,
            ...(assignedLevel !== null ? { level: assignedLevel } : {}),
          };
          newBuild.slotOrder = [...newBuild.slotOrder, newEntry];

          newBuild.sets = updateSetTracking(newBuild);
          return { build: newBuild };
        });

        return { ok: true, enhancementDropped };
      },

      freezeSlotLevelsForLevelUpMode: () => {
        const state = get();
        const build = { ...state.build, slotOrder: [...state.build.slotOrder] };
        // Same order as the rehydrate/import migrations: scrub before backfill
        // (a stale garbage level must not survive to seed it), ensure after
        // backfill (only entries still missing a level need populating),
        // reconcile last (it reads the solved assignment every prior step
        // could have changed).
        scrubFabricatedSlotLevels(build);
        const changed =
          [
            backfillSlotOrderLevels(build, true),
            ensureSlotOrderPopulated(build, true),
            reconcileStoredSlotLevels(build, true),
          ].filter(Boolean).length > 0;
        if (!changed) return;
        historyCheckpoint();
        set({ build });
      },

      // Enhancements
      setEnhancement: (powerName, slotIndex, enhancement, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return;

        historyCheckpoint();
        const { category } = found;

        set((s) => {
          const newBuild = applyPowerUpdate(s.build, category, (powers) =>
            powers.map((p) =>
              p.internalName === powerName
                ? { ...p, slots: p.slots.map((slot, i) => (i === slotIndex ? enhancement : slot)) }
                : p
            )
          );
          newBuild.sets = updateSetTracking(newBuild);
          return { build: newBuild };
        });
      },

      clearEnhancement: (powerName, slotIndex, categoryHint) => {
        get().setEnhancement(powerName, slotIndex, null as unknown as Enhancement, categoryHint);
      },

      clearAllEnhancements: (powerName, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return;

        historyCheckpoint();
        const { category } = found;

        set((s) => {
          const newBuild = applyPowerUpdate(s.build, category, (powers) =>
            powers.map((p) =>
              p.internalName === powerName ? { ...p, slots: p.slots.map(() => null) } : p
            )
          );
          newBuild.sets = updateSetTracking(newBuild);
          return { build: newBuild };
        });
      },

      // Settings
      setLevel: (level) => {
        historyCheckpoint();
        set((state) => {
          const newLevel = Math.max(1, Math.min(50, level));
          return {
            build: {
              ...state.build,
              level: newLevel,
              inherents: reconcileInherentSlots(state.build.inherents, newLevel),
            },
          };
        });
      },

      setProgressionMode: (mode) => {
        historyCheckpoint();
        set((state) => ({
          build: { ...state.build, progressionMode: mode },
        }));
      },

      setActiveModes: (modes) => {
        historyCheckpoint();
        // Switching a mode on also activates the power that SETS it, so its persistent effects
        // (Bright Nova's damageBuff, White Dwarf's resistance/mez-protection) flow into global
        // bonuses — and deactivates the setter of any mode that just went off. The setter is
        // whichever selected power carries the mode in `setsModes`, read from the power def, so
        // no list of form-power names is needed here.
        //
        // Thunderspy exports no `Set_Mode` template at all (TSPY-3's excluded tail), so its
        // Hunter/Prowler forms have no discoverable setter and stay pure display state.
        const active = new Set(modes);
        const build = get().build;
        const defsFor = (setId: string | null | undefined) => (setId ? getPowerset(setId)?.powers ?? [] : []);
        const selectable = new Set(selectableModes(archetypePowerDefs(build)));

        const syncPowerList = (powers: SelectedPower[], defs: Power[]): SelectedPower[] => {
          let changed = false;
          const out = powers.map((p) => {
            const owned = (defs.find((d) => d.internalName === p.internalName)?.setsModes ?? [])
              .filter((m) => selectable.has(m));
            if (!owned.length) return p;
            const desired = owned.some((m) => active.has(m));
            if (p.isActive === desired) return p;
            changed = true;
            return { ...p, isActive: desired };
          });
          return changed ? out : powers;
        };

        // The selector owns the SELECTABLE slice of `activeModes` and nothing else. Modes a
        // running power publishes (Power Boost's `BoostPower`, a stealth toggle's
        // `Hidden_Attack`) are owned by that power's own toggle, so switching form replaces the
        // form and leaves them standing — a wholesale replace here would switch them off
        // without switching the power off, and only the toggle can put them back.
        const kept = (get().build.activeModes ?? []).filter((m) => !selectable.has(m));
        const nextActiveModes = [...new Set([...modes, ...kept])];

        set((s) => ({
          build: {
            ...s.build,
            activeModes: nextActiveModes,
            primary: { ...s.build.primary, powers: syncPowerList(s.build.primary.powers, defsFor(s.build.primary.id)) },
            secondary: { ...s.build.secondary, powers: syncPowerList(s.build.secondary.powers, defsFor(s.build.secondary.id)) },
          },
        }));
      },

      setVaultId: (id) => {
        // Note: not history-checkpointed. This is metadata about *where* the
        // build came from, not part of the user's editable state — undoing a
        // "loaded from Library" event would surprise the user.
        set((state) => {
          const next = id ?? undefined;
          if (state.build.vaultId === next) return state;
          return { build: { ...state.build, vaultId: next } };
        });
      },

      // --- Saved attack chains ------------------------------------------------
      saveAttackChain: (name, powers, startForm = null, fullShiftAnimations = false) => {
        historyCheckpoint();
        const chain: AttackChain = {
          id: `chain-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`,
          name,
          powers,
          startForm,
          fullShiftAnimations,
        };
        set((state) => ({
          build: {
            ...state.build,
            attackChains: [...(state.build.attackChains ?? []), chain],
          },
        }));
        return chain.id;
      },

      updateAttackChain: (id, powers, startForm = null, fullShiftAnimations = false) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            attackChains: (state.build.attackChains ?? []).map((c) =>
              c.id === id ? { ...c, powers, startForm, fullShiftAnimations } : c,
            ),
          },
        }));
      },

      renameAttackChain: (id, name) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            attackChains: (state.build.attackChains ?? []).map((c) =>
              c.id === id ? { ...c, name } : c,
            ),
          },
        }));
      },

      deleteAttackChain: (id) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            attackChains: (state.build.attackChains ?? []).filter((c) => c.id !== id),
          },
        }));
      },

      setProcOverride: (powerName, slotIndex, patch) => {
        historyCheckpoint();
        const key = procOverrideKey(powerName, slotIndex);
        set((state) => {
          const prev = state.build.procOverrides?.[key] ?? DEFAULT_PROC_OVERRIDE;
          const next: ProcOverride = { ...prev, ...patch };
          const map = { ...(state.build.procOverrides ?? {}) };
          // Keep the map sparse (smaller share-URLs): drop the key when the
          // override is back to the default (enabled + auto).
          if (isDefaultProcOverride(next)) delete map[key];
          else map[key] = next;
          return { build: { ...state.build, procOverrides: map } };
        });
      },

      clearProcOverride: (powerName, slotIndex) => {
        const key = procOverrideKey(powerName, slotIndex);
        if (!get().build.procOverrides?.[key]) return; // nothing to clear
        historyCheckpoint();
        set((state) => {
          const map = { ...(state.build.procOverrides ?? {}) };
          delete map[key];
          return { build: { ...state.build, procOverrides: map } };
        });
      },

      toggleOverCapMute: (statKey) => {
        historyCheckpoint();
        const canonical = toCanonicalStatKey(statKey);
        set((state) => {
          const current = state.build.mutedOverCapStats ?? [];
          const next = current.includes(canonical)
            ? current.filter((k) => k !== canonical)
            : [...current, canonical];
          return { build: { ...state.build, mutedOverCapStats: next } };
        });
      },

      clearOverCapMutes: () => {
        if ((get().build.mutedOverCapStats ?? []).length === 0) return; // nothing to clear
        historyCheckpoint();
        set((state) => ({ build: { ...state.build, mutedOverCapStats: [] } }));
      },

      maximizeEnhancementLevels: (options) => {
        const { relativeLevel, ioLevel, attuneAll, boostLevel } = options ?? {};
        // Map a single slot to its maxed form. Returns the same object
        // when nothing changes so the React equality check downstream can
        // bail without a rerender. Each option is applied independently —
        // omitted options leave the corresponding state alone.
        const maxSlot = (slot: Enhancement | null): Enhancement | null => {
          if (!slot) return slot;
          // Origin and special enhancements carry a RELATIVE level, stored in
          // `boost`. This used to write `slot.level` on specials only, which
          // nothing reads and serialization drops — the control looked like it
          // worked because the slot badge renders `level`, and the value died
          // on the next save.
          if (slot.type === 'special' || slot.type === 'origin') {
            if (relativeLevel === undefined) return slot;
            const next = relativeLevel === 0 ? undefined : relativeLevel;
            if ((slot.boost ?? 0) === (next ?? 0)) return slot;
            return { ...slot, boost: next };
          }
          if (slot.type === 'io-set' || slot.type === 'io-generic') {
            let next: Enhancement = slot;

            // attuneAll: flip attuned=true on set IOs. Generic IOs have
            // no attunement state, so leave them alone. Applying a
            // catalyst in-game also strips any existing boosters from the
            // IO, so we clear `boost` at the same time to keep state
            // consistent.
            if (attuneAll && slot.type === 'io-set' && slot.attuned !== true) {
              next = { ...next, attuned: true, boost: undefined };
            }

            // ioLevel: force non-attuned IO level. Set IOs are clamped to
            // the set's [minLevel, maxLevel] range; generic IOs are clamped
            // to [10, 50]. Attuned IOs (including those just-flipped by
            // attuneAll above) don't carry a meaningful level, so we skip.
            const isAttuned = next.type === 'io-set' && next.attuned === true;
            if (ioLevel !== undefined && !isAttuned) {
              let target = ioLevel;
              if (next.type === 'io-set' && next.setId) {
                const set = getIOSet(next.setId);
                if (set) {
                  // set.minLevel may be < 10 in data but in-game floor is 10.
                  target = Math.min(set.maxLevel, Math.max(set.minLevel ?? 10, ioLevel));
                }
              } else if (next.type === 'io-generic') {
                target = Math.min(50, Math.max(10, ioLevel));
              }
              if ((next.level ?? 50) !== target) {
                next = { ...next, level: target };
              }
            }

            // boostLevel: apply +X catalyst boost. Eligible only for
            // NON-attuned L50+ IOs (generic or set). Attuned IOs cannot
            // accept boosters in-game — they scale with character level
            // and that's the only knob they have.
            // A piece with no aspects is a pure proc: there is no magnitude
            // for a booster to scale, and `createIOSetEnhancement` refuses one
            // at placement. Skipping it here keeps the two paths agreeing —
            // otherwise the same piece carries a boost or not depending on
            // which one last touched it, and the shopping list bills a booster
            // for a slot the picker would never have boosted.
            const isPureProc =
              next.type === 'io-set' && !!next.isProc && (next.aspects?.length ?? 0) === 0;
            if (boostLevel !== undefined && !isAttuned && !isPureProc && (next.level ?? 50) >= 50) {
              if ((next.boost ?? 0) !== boostLevel) {
                next = { ...next, boost: boostLevel };
              }
            }

            return next === slot ? slot : next;
          }
          return slot;
        };
        // Apply across every power list (primary/secondary/pools/epic/
        // inherents). Walk in a single set() so undo treats it as one
        // operation.
        let changedSlots = 0;
        const remap = (powers: SelectedPower[]): SelectedPower[] => {
          let powerChanged = false;
          const out = powers.map((p) => {
            if (!p.slots || p.slots.length === 0) return p;
            let slotChanged = false;
            const nextSlots = p.slots.map((s) => {
              const next = maxSlot(s);
              if (next !== s) {
                slotChanged = true;
                changedSlots += 1;
              }
              return next;
            });
            if (!slotChanged) return p;
            powerChanged = true;
            return { ...p, slots: nextSlots };
          });
          return powerChanged ? out : powers;
        };
        // Run the dry remap once to see whether anything would change —
        // skip the historyCheckpoint/set when the build is already maxed
        // so re-running the action is a true no-op.
        const before = get().build;
        const dryPrimary = remap(before.primary.powers);
        const drySecondary = remap(before.secondary.powers);
        const dryPools = before.pools.map((pool) => ({ ...pool, powers: remap(pool.powers) }));
        const dryEpic = before.epicPool ? { ...before.epicPool, powers: remap(before.epicPool.powers) } : null;
        const dryInherents = remap(before.inherents);
        if (changedSlots === 0) return 0;

        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            primary: { ...state.build.primary, powers: dryPrimary },
            secondary: { ...state.build.secondary, powers: drySecondary },
            pools: dryPools,
            epicPool: dryEpic,
            inherents: dryInherents,
          },
        }));
        return changedSlots;
      },

      setOrigin: (origin) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            settings: { ...state.build.settings, origin },
          },
        }));
      },

      // Accolades
      addAccolade: (accoladeId) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            accolades: state.build.accolades.includes(accoladeId)
              ? state.build.accolades
              : [...state.build.accolades, accoladeId],
          },
        }));
      },

      removeAccolade: (accoladeId) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            accolades: state.build.accolades.filter((id) => id !== accoladeId),
          },
        }));
      },

      // Incarnates
      setIncarnatePower: (slotId, power) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            incarnates: {
              ...state.build.incarnates,
              [slotId]: power,
            },
          },
        }));
      },

      clearIncarnatePower: (slotId) => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            incarnates: {
              ...state.build.incarnates,
              [slotId]: null,
            },
          },
        }));
      },

      clearAllIncarnates: () => {
        historyCheckpoint();
        set((state) => ({
          build: {
            ...state.build,
            incarnates: createEmptyIncarnateBuildState(),
          },
        }));
      },

      // Incarnate Crafting Checklist
      toggleCraftingCheckItem: (key) =>
        set((state) => ({
          build: {
            ...state.build,
            craftingChecklist: {
              ...state.build.craftingChecklist,
              [key]: !state.build.craftingChecklist[key],
            },
          },
        })),

      setCraftingCheckItem: (key, checked) =>
        set((state) => ({
          build: {
            ...state.build,
            craftingChecklist: {
              ...state.build.craftingChecklist,
              [key]: checked,
            },
          },
        })),

      clearCraftingChecklist: () =>
        set((state) => ({
          build: {
            ...state.build,
            craftingChecklist: createEmptyCraftingChecklistState(),
          },
        })),

      clearCraftingChecklistForSlot: (slotId) =>
        set((state) => {
          const slotPrefix = `${slotId}:`;
          const filtered = Object.fromEntries(
            Object.entries(state.build.craftingChecklist).filter(
              ([key]) => !key.startsWith(slotPrefix)
            )
          );
          // Clearing a slot's checklist also resets its obtained-node progress.
          const obtained = Object.fromEntries(
            Object.entries(state.build.incarnateObtained).filter(
              ([key]) => !key.startsWith(slotPrefix)
            )
          );
          return {
            build: { ...state.build, craftingChecklist: filtered, incarnateObtained: obtained },
          };
        }),

      toggleIncarnateObtainedNode: (key) =>
        set((state) => {
          const next = { ...state.build.incarnateObtained };
          if (next[key]) {
            delete next[key];
          } else {
            next[key] = true;
          }
          return { build: { ...state.build, incarnateObtained: next } };
        }),

      // Shopping List
      acquireShoppingItem: (salvageId) =>
        set((state) => ({
          build: {
            ...state.build,
            shoppingListAcquired: {
              ...state.build.shoppingListAcquired,
              [salvageId]: (state.build.shoppingListAcquired[salvageId] || 0) + 1,
            },
          },
        })),

      unacquireShoppingItem: (salvageId) =>
        set((state) => {
          const current = state.build.shoppingListAcquired[salvageId] || 0;
          if (current <= 0) return state;
          return {
            build: {
              ...state.build,
              shoppingListAcquired: {
                ...state.build.shoppingListAcquired,
                [salvageId]: current - 1,
              },
            },
          };
        }),

      clearShoppingListAcquired: () =>
        set((state) => ({
          build: { ...state.build, shoppingListAcquired: {} },
        })),

      // Power toggle (for stat calculations)
      togglePowerActive: (powerName, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return;
        historyCheckpoint();

        // Kheldian form toggles are mutually exclusive in-game: enabling
        // Bright Nova auto-disables White Dwarf (and vice versa). The mode
        // selector is kept consistent with the toggle further down, off the
        // power's own `setsModes`.
        const novaForms = new Set(['Bright_Nova', 'Dark_Nova']);
        const dwarfForms = new Set(['White_Dwarf', 'Black_Dwarf']);
        const isNovaToggle = novaForms.has(powerName);
        const isDwarfToggle = dwarfForms.has(powerName);
        // Ninja Run, Beast Run, and Athletic Run are travel toggles that the
        // game treats as a single "alt-run" slot — toggling one off the others.
        // The group stacks with Sprint (the user can have both Sprint and an
        // alt-run active), so only the alt-run toggles are mutually exclusive
        // with each other.
        const altRunPair = new Set(['Prestige_Ninja_Run', 'Prestige_Beast_Run', 'Prestige_Athletic_Run']);
        const isAltRunToggle = altRunPair.has(powerName);
        // NOTE: Staff Fighting's forms (Body/Mind/Soul) are NOT toggle powers —
        // they're non-slottable stance sub-powers selected via the parent's
        // `activeSubPower` (single-valued, so mutual exclusivity is inherent).
        // Same for Bio Armor adaptation stances. They never flow through here.
        const wasActive = found.power.isActive ?? false;
        const willBeActive = !wasActive;

        const transformPowers = (powers: SelectedPower[]) =>
          powers.map((p) => {
            if (p.internalName === powerName) {
              return { ...p, isActive: willBeActive };
            }
            // If turning on a Kheldian form toggle, deactivate the other
            // form (Bright Nova ↔ White Dwarf, Dark Nova ↔ Black Dwarf).
            if (willBeActive) {
              if (isNovaToggle && dwarfForms.has(p.internalName ?? '')) {
                return { ...p, isActive: false };
              }
              if (isDwarfToggle && novaForms.has(p.internalName ?? '')) {
                return { ...p, isActive: false };
              }
              // Ninja Run ↔ Beast Run ↔ Athletic Run mutual exclusivity.
              if (isAltRunToggle && p.internalName && p.internalName !== powerName && altRunPair.has(p.internalName)) {
                return { ...p, isActive: false };
              }
            }
            return p;
          });

        set((s) => {
          const updatedBuild = applyToAllPowers(s.build, transformPowers);
          // A power that SETS a mode makes that mode live when switched on and drops it when
          // switched off — every mode it sets, not just the ones the form selector offers. The
          // engine's `collect_source_modes` reads `setsModes` unfiltered and this is the same
          // rule, so `Source.Mode?` resolves the same way on both sides. Filtering by
          // `selectableModes` here is what kept Power Boost from ever publishing `BoostPower`.
          const defs = archetypePowerDefs(updatedBuild);
          const toggledModes = publishedModes(defs.find((d) => d.internalName === powerName));
          if (!toggledModes.length) return { build: updatedBuild };
          const nextModes = new Set(updatedBuild.activeModes ?? []);
          for (const mode of toggledModes) {
            if (willBeActive) nextModes.add(mode);
            else nextModes.delete(mode);
          }
          return { build: { ...updatedBuild, activeModes: [...nextModes] } };
        });
      },

      // Set active sub-power for powers with mutually exclusive stances.
      //
      // The write is AUTHORITATIVE for its stance group: every other power that
      // could be mistaken for the group's parent gets its `activeSubPower`
      // cleared. `internalName` is not unique — Bio Armor's switcher is internal
      // "Evolution" while the unrelated "Evolving Armor" toggle is internal
      // "Adaptation", and both match the adaptation group's `parents`. A build
      // that picked up a stray selection on the impostor (the pre-fix picker
      // could bind to it, and the engine reads `activeSubPower` per power) would
      // otherwise keep the stance stuck on even after clearing it here.
      setActiveSubPower: (parentPowerName, subPowerName) => {
        historyCheckpoint();
        // Sibling internalNames in the same stance group as the target, if any.
        const siblings = new Set(
          STANCE_GROUPS
            .filter((g) => g.parents.includes(parentPowerName))
            .flatMap((g) => g.parents)
            .filter((n) => n !== parentPowerName),
        );
        set((state) => ({
          build: applyToAllPowers(state.build, (powers) =>
            powers.map((p) => {
              if (p.internalName === parentPowerName) {
                return { ...p, activeSubPower: subPowerName ?? undefined };
              }
              if (siblings.has(p.internalName) && p.activeSubPower !== undefined) {
                return { ...p, activeSubPower: undefined };
              }
              return p;
            })
          ),
        }));
      },

      // Computed
      getTotalSlotsUsed: () => countTotalSlots(get().build),

      getSlotsRemaining: () => {
        const build = get().build;
        return getPlacedSlotLimit(build.level, build.serverId) - countPlacedSlots(build);
      },

      canAddSlot: (powerName, categoryHint) => {
        const state = get();
        const found = findPower(state.build, powerName, categoryHint);
        if (!found) return false;

        if (found.power.slots.length >= found.power.maxSlots) return false;
        if (countPlacedSlots(state.build) >= getPlacedSlotLimit(state.build.level, state.build.serverId)) {
          return false;
        }
        // In Level Up mode, the count budget is not the only limit: a slot must
        // also land on a grant at or above its power's pick level, and those run
        // out separately — Homecoming issues 24 grants from level 39 on, so a
        // build can be well inside its 67 and still have nowhere to put a slot
        // on a power taken at 38 (SLOT-1). Mirror addSlot so the + button and
        // the action agree.
        //
        // Outside Level Up mode there is no dated floor at all (SLOT-3) — the
        // count budget above is the whole check.
        if (!useUIStore.getState().levelUpMode) return true;
        const pickLevel = found.category === 'inherent' ? 1 : found.power.level;
        return findNextAvailableGrantLevel(state.build, pickLevel) !== null;
      },

      canAddPool: () => get().build.pools.length < getMaxPowerPools(get().build.serverId),

      isUniqueEnhancementSlotted: (setId: string, pieceNum: number) =>
        isUniqueEnhancementSlotted(get().build, setId, pieceNum),

      // Import/Export
      exportBuild: () => {
        const { build } = get();
        const exportData = {
          version: 4,
          build: slimBuild(build),
          meta: {
            exportedAt: new Date().toISOString(),
          },
        };
        return JSON.stringify(exportData, null, 2);
      },

      importBuild: (json, options) => {
        historyCheckpoint();
        try {
          const data = JSON.parse(json);

          // Cross-dataset import: the imported build belongs to a different
          // server than the one currently loaded. The active dataset is a
          // boot-time singleton (data/dataset.ts) and can't be hot-swapped, so
          // we reload with `?serverId=<id>` and carry the build across in the
          // URL hash — exactly the share-link path (bootServerId reads the
          // query param → loads the right dataset; useUrlBuildSync re-imports
          // the hash against it, where serverIds now match and this branch is
          // skipped, so there's no reload loop).
          //
          // This check MUST run on the RAW payload, before hydrateBuild():
          // hydration resolves the build against the *active* (still-wrong)
          // dataset and drops any powers/enhancements foreign to it — so a
          // Rebirth build hydrated under Homecoming loses its Rebirth-only
          // enhancements before we ever reload. We read serverId off the raw
          // data and carry the ORIGINAL, un-hydrated `json` across the hash, so
          // the post-reload re-import hydrates it against the correct dataset.
          //
          // Compare against the ACTIVE DATASET id, not get().build.serverId:
          // after such a reload the persisted build is still the old server and
          // onRehydrateStorage skips its URL-param sync while a hash is present,
          // so the store's serverId would be stale and re-trigger the reload —
          // an infinite loop. getActiveDataset().id reflects what's actually
          // loaded and, post-reload, already matches the imported build.
          //
          // Read through `isDatasetId` for the same reason `bootServerId` does: an id this
          // build does not ship is about to become `?serverId=<id>`, boot answers Homecoming
          // for it, and the re-import compares the raw id against Homecoming again — a reload
          // that never converges. Resolving it here to what boot will actually load makes the
          // comparison terminate.
          const targetServerId: Build['serverId'] = isDatasetId(data.build?.serverId)
            ? data.build.serverId
            : 'homecoming';
          // `intoLoadedDataset` is the user answering the other way at the prompt: read this
          // build HERE. Skipping the reload is the whole of it, plus the re-stamp below —
          // a ported build that kept its old serverId would compute against the server it
          // came from while every label named the one on screen.
          const port = options?.intoLoadedDataset === true;
          if (!port && typeof window !== 'undefined' && targetServerId !== getActiveDataset().id) {
            const label =
              getAllDatasetMetadata().find((d) => d.id === targetServerId)?.displayName
              ?? targetServerId;
            showDatasetSwitchOverlay(label);
            const url = new URL(window.location.href);
            url.searchParams.set('serverId', targetServerId);
            window.location.assign(`${url.pathname}${url.search}#${encodeImportFragment(json)}`);
            return true;
          }

          let build: Build;

          const notes: HydrationNote[] = [];

          if (data.version === 2 || data.version === 3 || data.version === 4) {
            // v2/v3/v4 slim format — reconstruct full Build from identity + build-specific fields
            //   v3 adds internalName to SlimPower; v2 uses display name fallback
            //   v4 adds `serverId` for multi-dataset support; v2/v3 default to 'homecoming'
            build = hydrateBuild(data.build, notes);
          } else {
            // v1 (legacy) — full Build object, just convert Set serialization
            const setsEntries = Object.entries(data.build.sets || {}) as [
              string,
              { count: number; pieces: number[] }
            ][];
            build = {
              ...data.build,
              // v1 is the one arm that does not reach `hydrateBuild`, so it is the one arm
              // that never folded its accolade objects to ids — and v1 is exactly the era
              // that wrote them as objects (the exporter stamped `version: 1` while the store
              // held `{ id, bonuses, … }`). Left raw, EVERY accolade in such a file misses the
              // roster and drops out of the totals, not just the two ACCOLADE-3 renames.
              accolades: normalizeAccoladeIds(data.build.accolades),
              sets: Object.fromEntries(
                setsEntries.map(([setId, tracking]) => [
                  setId,
                  { count: tracking.count, pieces: new Set(tracking.pieces) },
                ])
              ),
            };
          }

          // Default slotOrder for builds that don't have it (older saves)
          if (!build.slotOrder) {
            build.slotOrder = [];
          }

          // Sync power definitions (effects, icons) and enhancement icons
          // from current data — fixes stale data from older exports/shares
          syncBuildDefinitions(build);

          // Normalize VEAT branch powersets to base powersets
          normalizeBranchPowersets(build);

          // Clear stored levels the schedule never grants — an export written
          // before SLOT-1 can carry the old pick-level fallback frozen in.
          // Unconditional: harmless hygiene regardless of mode, and a wrong
          // stored level would wrongly seed a later switch into Level Up mode.
          scrubFabricatedSlotLevels(build);
          // The remaining three only mean anything in Level Up mode (SLOT-3) —
          // outside it a slot carries no level to populate, backfill, or
          // reconcile, and stored history stays exactly as imported.
          const importLevelUpMode = useUIStore.getState().levelUpMode;
          // Back-fill grant levels on slotOrder entries from pre-fix exports
          // so removing slots behaves like Mids from the first interaction.
          backfillSlotOrderLevels(build, importLevelUpMode);
          // Populate missing slotOrder entries so untouched slots stay at
          // their assigned levels when add/remove slot kicks computation
          // into leveling mode.
          ensureSlotOrderPopulated(build, importLevelUpMode);
          // Make storage agree with the assignment. Runs LAST: it reads the
          // solved levels, so every migration that can change them is above it.
          reconcileStoredSlotLevels(build, importLevelUpMode);

          // Repair invalid/duplicate/below-unlock pick levels in the import
          // itself — an export carrying addPower's old silent level-50 stamp
          // (B4) otherwise reproduces the invisible power until the next
          // full-page rehydrate happens to run this same migration.
          relevelInvalidPicks(build);

          // Auto-detect branch for VEAT builds (so branch powers appear in the picker)
          const branch = detectBranch(build);
          if (branch) {
            useUIStore.getState().setSelectedBranch(branch);
          }

          // Compare Slotting copies are keyed by powerset+power and hold whole
          // enhancements. An imported build shares neither the slot counts nor
          // (across a dataset switch) the enhancement vocabulary, so anything
          // held for the outgoing build would resurface as someone else's.
          useUIStore.getState().clearCompareSlottingCopies();

          // A ported build belongs to the dataset it was read against, whatever the file
          // said. `serverId` is what the engine calculates against, so leaving the file's
          // own id here is the split-brain this port exists to avoid.
          if (port) build.serverId = getActiveDataset().id;

          set({ build, lastImportNotes: port ? notes : [] });
          return true;
        } catch (e) {
          console.error('Failed to import build:', e);
          return false;
        }
      },

      importMidsBuild: (build) => {
        historyCheckpoint();
        if (!build.slotOrder) {
          build.slotOrder = [];
        }
        scrubFabricatedSlotLevels(build);
        // The rest only means anything in Level Up mode (SLOT-3) — outside it
        // a Mids import's slots carry no level, same as anything free-placed.
        const midsLevelUpMode = useUIStore.getState().levelUpMode;
        backfillSlotOrderLevels(build, midsLevelUpMode);
        // Mids imports come in with slotOrder empty (or partial). Lock in
        // respec-mode levels as stored entries so the first add/remove
        // slot interaction doesn't collapse untouched slots' levels.
        ensureSlotOrderPopulated(build, midsLevelUpMode);
        reconcileStoredSlotLevels(build, midsLevelUpMode);
        useUIStore.getState().clearCompareSlottingCopies();
        set({ build });
      },

      resetBuild: () => {
        historyCheckpoint();
        // Preserve the current server — New Build should keep you on the same
        // dataset (Rebirth stays Rebirth), not snap back to Homecoming.
        set((state) => ({ build: createEmptyBuild(state.build.serverId) }));
      },

      // Wipe every slotted enhancement across the whole build, keeping the
      // slot structure (number and placement of slots) intact. Slot order
      // and set tracking are reset because no enhancements remain to count.
      clearAllEnhancementsGlobal: () => {
        historyCheckpoint();
        set((state) => {
          const wipe = (powers: SelectedPower[]) =>
            powers.map((p) => ({ ...p, slots: p.slots.map(() => null) }));
          const newBuild: Build = {
            ...applyToAllPowers(state.build, wipe),
            inherents: wipe(state.build.inherents),
            sets: {},
          };
          return { build: newBuild };
        });
      },

      // Reduce every power's slots back to just its base slot (plus any
      // auto-granted inherent slots from the active dataset). Slot order
      // is cleared, all enhancements removed, set tracking reset.
      clearAllExtraSlots: () => {
        historyCheckpoint();
        set((state) => {
          const stripExtras = (powers: SelectedPower[]) =>
            powers.map((p) => {
              // Archetype inherents have maxSlots === 0 → no slots at all
              if (p.maxSlots === 0) return { ...p, slots: [] };
              // Inherent powers may have auto-granted slots (e.g. Rebirth
              // Health/Stamina) on top of the base slot — preserve those.
              const autoSlots = p.inherentSlotCount ?? 0;
              const total = 1 + autoSlots;
              return { ...p, slots: Array(total).fill(null) };
            });
          const newBuild: Build = {
            ...applyToAllPowers(state.build, stripExtras),
            inherents: stripExtras(state.build.inherents),
            slotOrder: [],
            sets: {},
          };
          return { build: newBuild };
        });
      },

      clearPowers: () => {
        historyCheckpoint();
        set((state) => {
          // Normalize branch powersets back to base for VEATs
          // (e.g., after importing a Crab Spider, branch powersets may be set as primary/secondary)
          let primary = { ...state.build.primary, powers: [] as SelectedPower[] };
          let secondary = { ...state.build.secondary, powers: [] as SelectedPower[] };
          const archetype = state.build.archetype.id ? getArchetype(state.build.archetype.id) : null;
          if (archetype?.branches) {
            for (const branchDef of Object.values(archetype.branches)) {
              if (primary.id === branchDef.primarySet) {
                const basePowerset = getPowerset(archetype.primarySets[0]);
                if (basePowerset) {
                  primary = { id: archetype.primarySets[0], name: basePowerset.name, powers: [] };
                }
              }
              if (secondary.id === branchDef.secondarySet) {
                const basePowerset = getPowerset(archetype.secondarySets[0]);
                if (basePowerset) {
                  secondary = { id: archetype.secondarySets[0], name: basePowerset.name, powers: [] };
                }
              }
            }
          }

          return {
            build: {
              ...state.build,
              primary,
              secondary,
              pools: [],
              epicPool: null,
              accolades: [],
              incarnates: createEmptyIncarnateBuildState(),
              craftingChecklist: createEmptyCraftingChecklistState(),
              incarnateObtained: {},
              sets: {},
              slotOrder: [],
              // Re-grant inherents with fresh empty slots
              inherents: getInherentSelectedPowers(
                state.build.archetype.id,
                state.build.archetype.name || undefined,
                state.build.archetype.inherent,
                state.build.level,
              ),
            },
          };
        });
      },
    });},
    {
      name: 'coh-planner-build',
      storage: createJSONStorage(() => (isPreviewCaptureMode() ? memoryStorage : localStorage)),
      // The rehydrate migrations reach into the active dataset (inherent rules,
      // power defs via syncBuildDefinitions). Auto-hydration runs at store-import
      // time — BEFORE main.tsx's loadDataset() — so those migrations threw
      // "No dataset loaded" and aborted (partial migration: new inherents weren't
      // appended, slots weren't reconciled). Skip auto-hydration and rehydrate
      // explicitly in main.tsx after the dataset is loaded.
      skipHydration: true,
      // Per-server build storage: `{ activeServerId, buildsByServer }`. Each
      // server keeps its own working build so switching datasets is
      // non-destructive. v0 was the single-slot `{ build }` shape.
      version: 1,
      migrate: (persisted, version) => migratePerServerState(persisted, version),
      // Reconstruct the active build from the per-server map, keyed off the
      // dataset actually LOADED this boot (getActiveDataset) — not the persisted
      // activeServerId, which a `?serverId=` deeplink may have overridden. The
      // other servers' builds are stashed on `_inactiveServerBuilds`, preserved
      // and re-persisted verbatim. migratePerServerState is idempotent, so
      // calling it here is safe whether or not `migrate` already ran.
      merge: (persisted, current) => {
        const normalized = migratePerServerState(persisted);
        const { build, inactiveServerBuilds } = selectActiveBuild(
          normalized,
          getActiveDataset().id,
        );
        return {
          ...(current as BuildStore),
          build: build as unknown as Build,
          _inactiveServerBuilds: inactiveServerBuilds,
        } as BuildStore;
      },
      partialize: (state) => composePersistedState(state.build, state._inactiveServerBuilds),
      onRehydrateStorage: () => (state) => {
        if (state) {
          // Align the build's server with the dataset actually loaded this boot.
          // `merge` handles this whenever there's persisted data, but for a
          // first-time visitor (empty storage) merge may not run — leaving the
          // hardcoded initial `homecoming` stamp on a build that boot loaded a
          // different dataset for (e.g. a `?serverId=rebirth` launcher deeplink).
          // Start a clean build on the loaded server so the stamp matches.
          const activeId = getActiveDataset().id;
          if (state.build.serverId !== activeId) {
            state.build = createEmptyBuild(activeId);
          }

          // Early structural init: the fields below are dereferenced
          // *unguarded* by later migrations (e.g. `inherents.length`,
          // `primary.powers`). Initialize them FIRST so that if any single
          // migration throws, the build is still structurally valid and the
          // first render won't crash. The migration body is also wrapped in
          // try/catch for the same reason — a failed migration must degrade to
          // "loads un-migrated", never "white screen on load". Paired with the
          // defensive guard in computeAllSlotLevels.
          if (!state.build.slotOrder) state.build.slotOrder = [];
          if (!Array.isArray(state.build.inherents)) state.build.inherents = [];
          if (!state.build.incarnates) state.build.incarnates = createEmptyIncarnateBuildState();
          if (!state.build.craftingChecklist) state.build.craftingChecklist = createEmptyCraftingChecklistState();
          if (!state.build.incarnateObtained) state.build.incarnateObtained = {};
          if (!state.build.shoppingListAcquired) state.build.shoppingListAcquired = {};
          if (!Array.isArray(state.build.accolades)) state.build.accolades = [];
          if (!state.build.sets) state.build.sets = {};
          if (!Array.isArray(state.build.mutedOverCapStats)) state.build.mutedOverCapStats = [];
          try {
          // Convert pieces arrays back to Sets after rehydration
          // The persisted state has arrays, but we need Sets
          const setsEntries = Object.entries(state.build.sets || {}) as [
            string,
            { count: number; pieces: number[] | Set<number> }
          ][];
          const sets = Object.fromEntries(
            setsEntries.map(([setId, tracking]) => [
              setId,
              {
                count: tracking.count,
                pieces:
                  tracking.pieces instanceof Set
                    ? tracking.pieces
                    : new Set(tracking.pieces as number[]),
              },
            ])
          );
          state.build.sets = sets;

          // Migration: Grant inherent powers if missing but both powersets are selected
          // This handles builds created before inherent powers were implemented
          if (
            state.build.inherents.length === 0 &&
            state.build.primary.id &&
            state.build.secondary.id
          ) {
            state.build.inherents = getInherentSelectedPowers(
              state.build.archetype.id,
              state.build.archetype.name || undefined,
              state.build.archetype.inherent,
              state.build.level,
            );
          }

          // Migration: Update existing inherent powers' maxSlots from current definitions
          // This fixes powers like Rest that may have been saved with wrong maxSlots
          if (state.build.inherents.length > 0) {
            state.build.inherents = state.build.inherents.map((power) => {
              const def = getInherentPowerDef(power.internalName)
                ?? getInherentPowerDef(power.name.replace(/\s+/g, '_'));
              if (def && power.maxSlots !== def.maxSlots) {
                const updated = { ...power, maxSlots: def.maxSlots };
                // If the definition is now unslottable (Ninja Run / Beast Run
                // corrected to maxSlots 0), drop any stale slots carried from
                // the old definition so they don't keep their phantom base slot.
                if (def.maxSlots === 0) updated.slots = [];
                return updated;
              }
              return power;
            });
          }

          // Migration: Append inherent powers added since this build was saved
          // (e.g. Kheldian travel inherents, Ninja/Beast Run) now lives in the
          // shared syncBuildDefinitions funnel below, so importBuild (share
          // links / JSON) self-heals too — not just localStorage rehydrate.

          // Migration: Reconcile per-server auto-granted inherent slots
          // (e.g. Rebirth's Health/Stamina grants at L8/L16/L12/L22). Builds
          // created before this rule landed — or hydrated while a different
          // dataset was active — would miss the grant. Sync against the
          // current character level here so the slots appear as soon as the
          // page loads.
          if (state.build.inherents.length > 0) {
            state.build.inherents = reconcileInherentSlots(state.build.inherents, state.build.level);
          }

          // serverId alignment is handled up front (the `activeId` guard above)
          // and by per-server `merge`: the loaded server's own build is chosen
          // from `buildsByServer`, so `?serverId=X` deeplinks are honored
          // WITHOUT clearing picks — each server keeps its own build. The old
          // "URL-param sync clears archetype/primary/secondary on mismatch"
          // block is gone; under per-server storage there is no cross-server
          // build to clear.

          // Migration: Initialize incarnates if missing (for builds created before incarnate system)
          if (!state.build.incarnates) {
            state.build.incarnates = createEmptyIncarnateBuildState();
          }

          // Migration: Initialize crafting checklist if missing
          if (!state.build.craftingChecklist) {
            state.build.craftingChecklist = createEmptyCraftingChecklistState();
          }

          // Migration: Convert old crafting keys with trailing :idx to new format without idx
          // Old: "alpha:vigor:1:core:salvage:ArcaneCantrip:0" → New: "alpha:vigor:1:core:salvage:ArcaneCantrip"
          if (state.build.craftingChecklist && Object.keys(state.build.craftingChecklist).length > 0) {
            const migrated: Record<string, boolean> = {};
            let needsMigration = false;
            for (const [key, value] of Object.entries(state.build.craftingChecklist)) {
              const match = key.match(/^(.+:salvage:\w+):\d+$/);
              if (match) {
                needsMigration = true;
                if (value) migrated[match[1]] = true;
              } else {
                migrated[key] = value;
              }
            }
            if (needsMigration) {
              state.build.craftingChecklist = migrated;
            }
          }

          // Migration: Initialize incarnate obtained-node progress if missing.
          // An earlier iteration stored a per-slot numeric tier level
          // (Record<slotId, number>); the model is now per-node boolean keys.
          // Drop the old numeric shape rather than guess node paths from it.
          if (!state.build.incarnateObtained) {
            state.build.incarnateObtained = {};
          } else if (
            Object.values(state.build.incarnateObtained).some((v) => typeof v !== 'boolean')
          ) {
            state.build.incarnateObtained = {};
          }

          // Migration: Initialize shopping list acquired if missing
          if (!state.build.shoppingListAcquired) {
            state.build.shoppingListAcquired = {};
          }

          // Migration: Initialize slotOrder if missing (builds before leveling mode)
          if (!state.build.slotOrder) {
            state.build.slotOrder = [];
          }

          // Migration: accolades written by an older Sidekick — the object fold and the two
          // pre-convention ids, both through the one normaliser `hydrateBuild` uses. The table
          // used to live here AND there, in two spellings; one of them is how ACCOLADE-3's
          // door census found the third copy that was missing (`importBuild`'s v1 arm).
          if (Array.isArray(state.build.accolades) && state.build.accolades.length > 0) {
            state.build.accolades = normalizeAccoladeIds(state.build.accolades);
          }

          // Migration: Normalize VEAT branch powersets to base powersets
          // Fixes builds where branch powersets (e.g., crab-spider-soldier) were
          // stored as the primary/secondary instead of the base powersets
          normalizeBranchPowersets(state.build);

          // Migration: Sync power definitions (effects, icons) and enhancement icons
          // from current data — fixes stale data from older builds
          syncBuildDefinitions(state.build);

          // Migration: Convert display-name identifiers to internalName
          // Old builds stored display names (e.g., "Super Speed") in slotOrder,
          // activeSubPower, and grantedByPower. Convert to internalName format.
          {
            // Build a display-name → internalName lookup from all hydrated powers
            const nameToInternal = new Map<string, string>();
            const allBuildPowers = [
              ...state.build.primary.powers,
              ...state.build.secondary.powers,
              ...state.build.pools.flatMap((pool) => pool.powers),
              ...(state.build.epicPool?.powers ?? []),
              ...state.build.inherents,
            ];
            for (const p of allBuildPowers) {
              if (p.name !== p.internalName) {
                nameToInternal.set(p.name, p.internalName);
              }
            }

            // Migrate slotOrder entries
            if (state.build.slotOrder) {
              for (const entry of state.build.slotOrder) {
                const mapped = nameToInternal.get(entry.powerName);
                if (mapped) entry.powerName = mapped;
              }
            }

            // Migrate activeSubPower and grantedByPower on all powers
            for (const p of allBuildPowers) {
              if (p.activeSubPower && p.activeSubPower.includes(' ')) {
                p.activeSubPower = p.activeSubPower.replace(/\s+/g, '_');
              }
              if (p.grantedByPower && p.grantedByPower.includes(' ')) {
                p.grantedByPower = p.grantedByPower.replace(/\s+/g, '_');
              }
            }
          }

          // Migration: clear stored slot levels the schedule never grants. A
          // build saved before SLOT-1 can carry a level frozen in from the old
          // pick-level fallback (e.g. `level: 38`, a level Homecoming grants no
          // slots at); it can never be honored, so drop it and let the solver
          // re-house the slot. Must run BEFORE the backfill, which would
          // otherwise leave it in place.
          scrubFabricatedSlotLevels(state.build);

          // Migration: Back-fill `level` on slotOrder entries placed before the
          // Mids-style remove/replace fix. Without this, the first removeSlot
          // on a legacy build still cascades subsequent slots — backfill makes
          // every entry stable from the next interaction onward.
          //
          // `true` here, not the live toggle: `onRehydrateStorage` can fire
          // before `useUIStore`'s own persisted state has rehydrated (see the
          // `setTimeout` a few lines below for the same reason), so reading it
          // here risks silently skipping this migration for a build that is
          // actually in Level Up mode. Running it unconditionally reproduces
          // this migration's exact pre-SLOT-3 behavior; the cost if the build
          // turns out to be in free mode is unused stored levels sitting in
          // `slotOrder` that nothing reads (SLOT-3).
          backfillSlotOrderLevels(state.build, true);

          // Migration: Populate any missing slotOrder entries so add/remove
          // slot interactions don't collapse untouched powers' slot levels
          // to their pick level (the symptom of Mids-imported / legacy
          // builds that left slotOrder empty or partial).
          ensureSlotOrderPopulated(state.build, true);

          // Migration: a build leveled through SLOT-2 carries a run of entries
          // all claiming one level. They display correctly but cascade on a
          // removal, because a level no grant can honor reads the same as no
          // level at all. Freeze the solved assignment in. Must run LAST.
          reconcileStoredSlotLevels(state.build, true);

          // Auto-detect branch for VEAT builds on rehydration
          const branch = detectBranch(state.build);
          if (branch) {
            // Defer to avoid store initialization ordering issues
            setTimeout(() => useUIStore.getState().setSelectedBranch(branch), 0);
          }

          // Migration: fix pick levels that are invalid, duplicated, or below
          // the power's unlock level (see relevelInvalidPicks).
          relevelInvalidPicks(state.build);

          } catch (err) {
            // A migration threw. The early structural init above guarantees
            // the build is still renderable, so load it with whatever
            // migrations completed rather than aborting hydration.
            console.error(
              'Build rehydrate migration failed; loading with partial migration applied',
              err
            );
          }
          state.setHasHydrated(true);
        }
      },
    }
  )
);

// ============================================
// SELECTOR HOOKS
// ============================================

/** Select just the build data */
export const useBuild = () => useBuildStore((state) => state.build);

/** Select just the archetype */
export const useArchetype = () => useBuildStore((state) => state.build.archetype);

/** Select the primary powerset */
export const usePrimary = () => useBuildStore((state) => state.build.primary);

/** Select the secondary powerset */
export const useSecondary = () => useBuildStore((state) => state.build.secondary);

/** Select all pools */
export const usePools = () => useBuildStore((state) => state.build.pools);

/** Select the epic pool */
export const useEpicPool = () => useBuildStore((state) => state.build.epicPool);

/** Select crafting checklist state */
export const useCraftingChecklist = () => useBuildStore((state) => state.build.craftingChecklist);
