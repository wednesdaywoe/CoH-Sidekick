/**
 * Build type definitions - represents a complete character build
 */

import type { DatasetId } from '@/data/dataset';
import type { Origin, ProgressionMode } from './common';
import type { Archetype, ArchetypeId } from './archetype';
import type { SelectedPower } from './power';
import type { IncarnateBuildState, CraftingChecklistState, IncarnateObtainedState } from './incarnate';
import { createEmptyIncarnateBuildState, createEmptyCraftingChecklistState, createEmptyIncarnateObtainedState } from './incarnate';

// ============================================
// POWERSET SELECTION
// ============================================

export interface PowersetSelection {
  /** Powerset ID (e.g., "blaster/fire-blast") */
  id: string | null;
  /** Display name */
  name: string;
  /** Selected powers from this set */
  powers: SelectedPower[];
}

// ============================================
// POOL SELECTION
// ============================================

export interface PoolSelection {
  /** Pool ID (e.g., "speed") */
  id: string;
  /** Display name */
  name: string;
  /** Selected powers from this pool */
  powers: SelectedPower[];
}

// ============================================
// BUILD SETTINGS
// ============================================

export interface BuildSettings {
  /** Character origin */
  origin: Origin;
}

// ============================================
// SET TRACKING
// ============================================

export interface SetTracking {
  /** Number of pieces slotted from this set */
  count: number;
  /** Which piece numbers are slotted */
  pieces: Set<number>;
}

// ============================================
// ARCHETYPE SELECTION (in build)
// ============================================

export interface ArchetypeSelection {
  /** Archetype ID */
  id: ArchetypeId | null;
  /** Display name */
  name: string;
  /** Full archetype stats (if selected) */
  stats: Archetype['stats'] | null;
  /** Inherent power info */
  inherent: Archetype['inherent'] | null;
}

// ============================================
// MAIN BUILD TYPE
// ============================================

/**
 * A saved attack chain — a named, ordered rotation the user has settled on
 * (e.g. "Single Target", "AoE"). `powers` is the cast order stored as stable
 * ChainPower ids ("bucket:internalName" — see attack-chain-powers.ts), mapped
 * back to the current build's powers on load (entries whose power is no longer
 * in the build are dropped). Lives on the Build so chains travel with the
 * character through save / load / export / share.
 */
export interface AttackChain {
  id: string;
  name: string;
  /** Cast order as ChainPower ids. */
  powers: string[];
  /**
   * The caster form the chain OPENS in — a mode id from `buildFormModes`
   * (a Kheldian's Bright Nova / White Dwarf), or null/absent for human form.
   *
   * Load-bearing, not decorative. It was originally "the one form this chain
   * lives in", because a form's attacks are castable only inside it and the
   * candidate roster was rebuilt per form: reopening a Nova chain in human form
   * resolved none of its ids, `idsToSequence` dropped them all, and one click of
   * Save wrote that emptied list back over the saved rotation.
   *
   * A chain can now SPAN forms via `switch` steps, so the roster is a union and
   * the ids resolve whatever form is set. What this field still decides is the
   * form the walk starts from — which determines which variant each cast fires
   * and which casts are flagged illegal. Same rotation, different opening form,
   * different numbers.
   */
  startForm?: string | null;
  /**
   * Whether this chain charges each form switch its FULL uncancelled shapeshift
   * animation (~2.24s of ArcanaTime) instead of only the blocking segment
   * (0.26s) it always pays. Absent/false = the blocking segment, the default.
   *
   * Stored per chain, and it has to be: the two readings are different
   * rotations, not different views of one. A Kheldian form toggle declares no
   * activation time and its animation is cancelled by the next attack, so a
   * chain that casts straight after switching never pays the tail — but a chain
   * built by a player who does not cancel is a slower rotation with a lower DPS,
   * and reloading it under the other assumption and pressing Save would
   * overwrite the original with numbers its author never chose. See
   * SHAPESHIFT_BLOCKING_CAST / SHAPESHIFT_FULL_ANIM in attack-chain-powers.ts
   * for where the two figures come from.
   */
  fullShiftAnimations?: boolean;
}

export interface Build {
  /** Build name */
  name: string;

  /**
   * Identifier of the dataset (CoH server) this build targets.
   * Determines which powerset/AT/IO-set definitions are loaded for the
   * build. Older builds without this field migrate to `'homecoming'`.
   * See `src/data/dataset.ts` and `MULTI_DATASET_PLAN.md`.
   */
  serverId: DatasetId;

  /** Selected archetype */
  archetype: ArchetypeSelection;

  /** Current character level (1-50) */
  level: number;

  /** Progression mode */
  progressionMode: ProgressionMode;

  /** Primary powerset */
  primary: PowersetSelection;

  /** Secondary powerset */
  secondary: PowersetSelection;

  /** Power pools (up to 4) */
  pools: PoolSelection[];

  /** Epic/Patron pool */
  epicPool: PoolSelection | null;

  /** Inherent powers */
  inherents: SelectedPower[];

  /** Selected accolade ids (internal name, lower-cased) — resolved to powers via getAccolades() */
  accolades: string[];

  /** Build settings */
  settings: BuildSettings;

  /** IO set tracking for bonus calculations */
  sets: Record<string, SetTracking>;

  /** Incarnate powers (level 50+) */
  incarnates: IncarnateBuildState;

  /** Incarnate crafting checklist progress */
  craftingChecklist: CraftingChecklistState;

  /** Highest crafting tier already obtained per incarnate slot (0 = none, 1-4).
   *  Obtained tiers drop out of the crafting cost summary and shopping list. */
  incarnateObtained: IncarnateObtainedState;

  /** Shopping list: count of salvage items marked as acquired across all incarnate slots */
  shoppingListAcquired: Record<string, number>;

  /** Chronological order of slot additions for leveling mode.
   *  Each entry = one extra slot added (slot index 1+ on a power).
   *  Empty = respec mode (slot levels computed by power-pick order).
   *  `category` disambiguates powers with the same internalName across categories
   *  (e.g., "Conserve_Power" in both secondary and epic). Optional for backward compat.
   *  `level` is the grant-pool level this slot was assigned at the time it was placed.
   *  Storing it lets removeSlot + re-add behave like Mids: the freed level returns
   *  to a pool rather than cascading subsequent slots downward. Optional for legacy
   *  builds; missing values are back-filled greedily on load. */
  slotOrder: {
    powerName: string;
    slotIndex: number;
    category?: string;
    level?: number;
    /**
     * Where `level` came from — the discriminator MBDEXPORT-21 is about.
     *
     * Three code paths write a level into this array and they do not mean the same thing:
     * a placement in the UI and MBDIMPORT's `seedSlotOrderFromFile` are both a record of
     * when an author put the slot there, while `ensureSlotOrderPopulated` fills the whole
     * array at once from the respec solver, which is a PACKING and not a history. Until
     * this field existed nothing downstream could tell them apart, so the `.mbd` writer
     * had to guess provenance from a proxy — canonical from "is there a stored entry",
     * the beta from a UI toggle the file does not even carry — and each proxy was wrong
     * for at least one of the three.
     *
     * Absent means UNSTATED, not a default: it is an entry written before this field, and
     * its provenance is genuinely unknown rather than known to be either one. The writer
     * carries an unstated level for the same reason it carries an authored one — every
     * build already in the wild was exported that way, and re-solving them all would
     * rewrite history rather than stop claiming it.
     */
    levelSource?: 'authored' | 'packed';
  }[];

  /**
   * Caster modes the player has switched on — the ids a selected power's `modeVariants` is
   * keyed by (`Peacebringer_Blaster_Mode`, `HunterMode`, `FastMode`, `SeismicPower`). While a
   * mode is live the display shows the variant the game's PowerRedirector fires instead of the
   * base record. No effect on slot allocation: slots stay on the base power, which every mode
   * shares. Absent means no mode is live.
   */
  activeModes?: string[];

  /**
   * If this build was loaded from the user's Build Library, the source
   * build's id. Used by the Save → Library flow to update the existing
   * entry rather than creating a duplicate. Cleared when the user starts
   * a new build or imports an unrelated one. The presence of an owner
   * token (separate, stored per-id by sharedBuilds.ts) is what actually
   * authorizes the update; this field is just the link. Field name kept
   * as `vaultId` (and the matching action `setVaultId`) since the backend
   * + ownership tokens still use that internal name; user-facing copy
   * says "library" — see the rename rationale in the corresponding feat
   * commit.
   */
  vaultId?: string;

  /** Saved attack chains (named rotations like "Single Target" / "AoE").
   *  Optional for backward compat; missing → no saved chains. See
   *  {@link AttackChain} and the Attack Chain Builder. */
  attackChains?: AttackChain[];

  /** Per-slotted-proc control overrides, keyed `${powerName}:${slotIndex}`.
   *  Sparse: only procs the user has explicitly touched appear here; an absent
   *  key means the proc is enabled and on its Auto (expected-uptime) default.
   *  Build-identity data — travels through save / load / export / share. See
   *  {@link ProcOverride} and the InfoPanel "Slotted Procs" block. */
  procOverrides?: Record<string, ProcOverride>;

  /** Canonical stat keys whose Rule-of-5 over-cap *warnings* the user has muted.
   *  Display-only: the engine still rejects the 6th+ bonus and the muted bonus
   *  still doesn't count toward any total — this suppresses warnings only. Keys
   *  are `group|label` (see `toCanonicalStatKey` in set-bonus-groups.ts). Sparse
   *  (typically 0–2 entries); build identity — travels through save / load /
   *  export / share. */
  mutedOverCapStats: string[];
}

/**
 * Per-proc control override stored on the Build (see {@link Build.procOverrides}).
 * Absent key ⇒ `{ enabled: true, mode: 'auto' }` (the runtime default). The
 * resolver + default live in `src/data/proc-data.ts`.
 */
export interface ProcOverride {
  /** Master gate for this specific slotted proc. Default true. */
  enabled: boolean;
  /** `auto` = expected-uptime default; `stacks`/`hp` = a user-pinned value. */
  mode: 'auto' | 'stacks' | 'hp';
  /** Pinned stack count when mode === 'stacks' (0..maxStacks). */
  stacks?: number;
  /** Pinned %HP when mode === 'hp' (0..100; 100 = full HP = floor). */
  hpPct?: number;
}

// ============================================
// DEFAULT BUILD FACTORY
// ============================================

/**
 * Placeholder name a build carries until the user renames it. Callers that
 * distinguish "the user named this" from "nobody has named this yet" — the
 * document title, for one — must compare against this rather than against `''`,
 * because the factory seeds the placeholder rather than leaving the name blank.
 */
export const DEFAULT_BUILD_NAME = 'Untitled Build';

export function createEmptyBuild(serverId: DatasetId = 'homecoming'): Build {
  return {
    name: DEFAULT_BUILD_NAME,
    serverId,
    archetype: {
      id: null,
      name: '',
      stats: null,
      inherent: null,
    },
    level: 1,
    progressionMode: 'auto',
    primary: {
      id: null,
      name: '',
      powers: [],
    },
    secondary: {
      id: null,
      name: '',
      powers: [],
    },
    pools: [],
    epicPool: null,
    inherents: [],
    accolades: [],
    settings: {
      origin: 'Natural',
    },
    sets: {},
    incarnates: createEmptyIncarnateBuildState(),
    craftingChecklist: createEmptyCraftingChecklistState(),
    incarnateObtained: createEmptyIncarnateObtainedState(),
    shoppingListAcquired: {},
    slotOrder: [],
    mutedOverCapStats: [],
  };
}

// ============================================
// BUILD EXPORT FORMAT (for JSON serialization)
// ============================================

export interface BuildExportV1 {
  /** Schema version */
  version: 1;
  /** Full build data (legacy) */
  build: Omit<Build, 'sets'> & {
    /** Sets with pieces as array instead of Set */
    sets: Record<string, { count: number; pieces: number[] }>;
  };
  /** Optional metadata */
  meta?: BuildExportMeta;
}

export interface BuildExportV2 {
  /** Schema version */
  version: 2;
  /** Slim build data — identity + build-specific fields, power definitions stripped */
  build: SlimBuildData;
  /** Optional metadata */
  meta?: BuildExportMeta;
}

/** Shape of the slim build data in v2 exports */
export interface SlimBuildData {
  name: string;
  /** Dataset / server identifier. Optional for backward compat — older
   * exports predate multi-dataset support and default to `'homecoming'`. */
  serverId?: DatasetId;
  archetype: { id: string | null; name: string };
  level: number;
  primary: { id: string | null; name: string; powers: { name: string; level: number; slots: unknown[] }[] };
  secondary: { id: string | null; name: string; powers: { name: string; level: number; slots: unknown[] }[] };
  pools: { id: string; name: string; powers: { name: string; level: number; slots: unknown[] }[] }[];
  epicPool: { id: string; name: string; powers: { name: string; level: number; slots: unknown[] }[] } | null;
  /** Saved attack chains (named rotations). Optional for backward compat. */
  attackChains?: AttackChain[];
  /** Per-slotted-proc control overrides. Optional for backward compat. */
  procOverrides?: Record<string, ProcOverride>;
  /** Muted over-cap warning stat keys. Optional for backward compat. */
  mutedOverCapStats?: string[];
  [key: string]: unknown;
}

export interface BuildExportMeta {
  exportedAt: string;
  authorName?: string;
  authorServer?: string;
}

/** Union of all export versions */
export type BuildExport = BuildExportV1 | BuildExportV2;
