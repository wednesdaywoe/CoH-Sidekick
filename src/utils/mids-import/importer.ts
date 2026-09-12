/**
 * Mids Reborn .mbd import orchestrator
 *
 * Parses .mbd JSON, maps all powers/enhancements to app format,
 * and constructs a complete Build object.
 */

import type {
  Build,
  SelectedPower,
  SelectedIncarnatePower,
  IncarnateSlotId,
  Power,
  Enhancement,
  PoolSelection,
  SetTracking,
} from '@/types';
import { createEmptyIncarnateBuildState, INCARNATE_SLOT_ORDER } from '@/types';
import {
  getArchetype,
  getAllPowersets,
  getPowerset,
  getPowerPool,
  getEpicPool,
  getInherentPowers,
  getArchetypeInherentPowers,
  createArchetypeInherentPower,
  getIncarnatePower,
  getIncarnateTree,
  GRANTED_POWER_GROUPS,
  STANCE_GROUPS,
  findStanceParent,
  getInherentGrantLevel,
} from '@/data';
import { getActiveDataset, getAllDatasetMetadata, type DatasetId } from '@/data/dataset';
import type { InherentPowerDef } from '@/data';

// ============================================
// SERVER DETECTION
// ============================================
//
// Mids Reborn carries the source database in `BuiltWith.Database`. Two
// known values today:
//   - "Homecoming"
//   - "Rebirth"
// Anything else falls back to the active dataset (best effort) with a
// general warning.
//
// Imports are blocked when the .mbd's database doesn't match the active
// dataset because all powerset/power lookups read from the active
// dataset's registry — a Rebirth build dropped into an HC session would
// fail to find Guardian (or any other Rebirth-only powerset) and
// produce a corrupt build. Caller should switch servers via the picker
// (which reloads with the new dataset) before retrying the import.

// The forks a named database may be comes from `MIDS_DATABASE_FOR_DATASET`, which the WRITER
// stamps from — one table, both directions, so a file we wrote is a file we can read. It was
// two hand-written else-branches, and they disagreed by construction: the writer stamped
// `Homecoming` on everything that was not Rebirth, and this refused the result with "made for
// Homecoming, but the planner is currently running Homecoming" — the same branch printing both
// halves of the sentence. Brainstorm and Thunderspy were unopenable by us for that reason.
// See DATA-GAP MBDEXPORT-2.

/**
 * Per-server full-path remaps for Mids power names whose meaning has shifted
 * since Mids' last database snapshot for that server.
 *
 * Rebirth: the Flight pool was reworked after Mids 2023.x. Old Mids builds
 * emit `Pool.Flight.Afterburner` for what used to be the toggle (now called
 * "Aerobatics" with internal name `Pool.Flight.Group_Fly`). Current Rebirth
 * data has `Pool.Flight.Afterburner` pointing at a wholly different power
 * ("Dive Attack", a tier-5 attack). Without remap, old builds import the
 * wrong power.
 */
const MIDS_FULL_PATH_REMAP: Record<DatasetId, Record<string, string>> = {
  homecoming: {},
  rebirth: {
    'Pool.Flight.Afterburner': 'Pool.Flight.Group_Fly',
  },
  // No known Mids path remaps for Thunderspy yet.
  thunderspy: {},
  // Brainstorm is Homecoming one release ahead, and Mids ships no Brainstorm database,
  // so nothing here yet — a remap would arrive with a pool rework, same as Rebirth's.
  brainstorm: {},
};

function remapMidsPath(path: string, server: DatasetId | null): string {
  if (!server) return path;
  return MIDS_FULL_PATH_REMAP[server]?.[path] ?? path;
}

import type {
  MbdFile,
  MbdPowerEntry,
  MbdSlotEntry,
  MidsImportResult,
  MidsImportWarning,
  MidsImportSummary,
} from './types';
import {
  mapArchetype,
  mapOrigin,
  buildPowersetLookup,
  resolvePowerset,
  findPowerByMidsName,
  buildPoolLookup,
  buildEpicLookup,
  resolvePoolId,
  resolveEpicPoolId,
  mapEnhancementUid,
  mapEnhancementByDisplayName,
  MIDS_SILENT_SKIP_PATHS,
  midsNameIsRetired,
  datasetsForMidsDatabase,
} from './mappers';

/**
 * Resolve a single Mids slot-entry enhancement to an app Enhancement.
 * Handles both the current Uid-based format and the legacy
 * display-name format used by older Mids versions.
 */
function resolveSlotEnhancement(enh: {
  Uid?: string;
  Enhancement?: string;
  IoLevel: number;
  RelativeLevel: string;
  Grade?: string;
}): ReturnType<typeof mapEnhancementUid> {
  if (enh.Uid && enh.Uid.length > 0) {
    return mapEnhancementUid(enh.Uid, enh.IoLevel, enh.RelativeLevel, enh.Grade);
  }
  // Legacy 2023-era Mids: display-name string in `Enhancement` field.
  if (typeof enh.Enhancement === 'string' && enh.Enhancement.length > 0) {
    return mapEnhancementByDisplayName(enh.Enhancement, enh.IoLevel, enh.RelativeLevel, enh.Grade);
  }
  return { enhancement: null, warning: null };
}
import type { PoolPowerMatch, EpicPowerMatch } from './mappers';
import { getAccolades, getAllAccolades, accoladeId } from '@/data/accolades';
import { countBudgetPowerPicks } from '@/utils/build-budget';
import { warnFallback } from '@/utils/fallback-warnings';
import { ensureSlotOrderPopulated } from '@/utils/slot-levels';

// ============================================
// MAIN IMPORT FUNCTION
// ============================================

export function importMidsBuild(jsonString: string): MidsImportResult {
  const warnings: MidsImportWarning[] = [];
  const summary: MidsImportSummary = {
    powersImported: 0,
    accoladesImported: 0,
    incarnatesImported: 0,
    powersFailed: 0,
    enhancementsImported: 0,
    enhancementsFailed: 0,
    slotsImported: 0,
  };

  // 1. Parse JSON
  let mbd: MbdFile;
  try {
    mbd = JSON.parse(jsonString);
  } catch {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'general', midsName: '', message: 'Invalid JSON: could not parse .mbd file' }],
      summary,
    };
  }

  // 2. Validate required fields
  if (!mbd.Class || !mbd.PowerSets || !mbd.PowerEntries) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'general', midsName: '', message: 'Missing required fields (Class, PowerSets, or PowerEntries)' }],
      summary,
    };
  }

  // 2a. Detect source server and require it to match the active dataset.
  // Rebirth builds reference Guardian / Composition / etc. that don't
  // exist in HC's registry (and vice-versa for HC's Sentinel sets), so
  // the importer can't proceed cross-dataset. Caller should switch the
  // server picker first.
  const namedDatabase = mbd.BuiltWith?.Database;
  const forkCandidates = datasetsForMidsDatabase(namedDatabase);
  const activeServer = (() => {
    try { return getActiveDataset().id; } catch { return null; }
  })();
  if (forkCandidates.length && activeServer && !forkCandidates.includes(activeServer)) {
    const label = (id: DatasetId) =>
      getAllDatasetMetadata().find((meta) => meta.id === id)?.displayName ?? id;
    return {
      success: false,
      build: null,
      warnings: [{
        type: 'general',
        midsName: namedDatabase ?? '',
        message: `This build names Mids' ${namedDatabase} database, which this planner reads as `
          + `${forkCandidates.map(label).join(' or ')}, but you are running ${label(activeServer)}. `
          + 'Switch servers via the Build Identity picker and retry the import.',
      }],
      summary,
    };
  }

  // Which fork this build is being read INTO. The guard above has already refused any file
  // whose database names a fork other than the one loaded, so the active dataset is the answer
  // whenever there is one; a database naming exactly one fork answers for a session that has
  // none. `Generic` and every unknown name answer nothing and read under whatever is loaded —
  // a plain CoH build is not evidence of a fork.
  const importFork: DatasetId | null =
    activeServer ?? (forkCandidates.length === 1 ? forkCandidates[0] : null);

  // 3. Map archetype
  const archetypeId = mapArchetype(mbd.Class);
  if (!archetypeId) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'archetype', midsName: mbd.Class, message: `Unknown archetype: ${mbd.Class}` }],
      summary,
    };
  }

  const archetype = getArchetype(archetypeId);
  if (!archetype) {
    return {
      success: false,
      build: null,
      warnings: [{ type: 'archetype', midsName: archetypeId, message: `Archetype data not found: ${archetypeId}` }],
      summary,
    };
  }

  // 4. Map origin and level
  // Mids top-level Level is 0-based, power entry levels are 1-based
  const origin = mapOrigin(mbd.Origin);
  const parsedLevel = (parseInt(mbd.Level, 10) || 49) + 1;
  // Also determine level from power entries — Mids Level may reflect the highest
  // power pick (0-based 48 = game level 49) rather than the character's actual level.
  // If any power is at the last pick level (49), the character is level 50.
  let maxPowerEntryLevel = 0;
  for (const entry of mbd.PowerEntries) {
    if (entry.Level > maxPowerEntryLevel) maxPowerEntryLevel = entry.Level;
  }
  const levelFromPowers = maxPowerEntryLevel >= 49 ? 50 : maxPowerEntryLevel;
  const level = Math.min(Math.max(parsedLevel, levelFromPowers, 1), 50);

  // 5. Build lookup tables
  const powersetLookup = buildPowersetLookup();
  const poolLookup = buildPoolLookup();

  // 6. Resolve powersets
  const primaryPath = mbd.PowerSets[0] || '';
  const secondaryPath = mbd.PowerSets[1] || '';

  let primaryId = primaryPath ? resolvePowerset(primaryPath, archetypeId, powersetLookup) : null;
  let secondaryId = secondaryPath ? resolvePowerset(secondaryPath, archetypeId, powersetLookup) : null;

  if (!primaryId && primaryPath) {
    warnings.push({ type: 'powerset', midsName: primaryPath, message: `Could not resolve primary powerset` });
  }
  if (!secondaryId && secondaryPath) {
    warnings.push({ type: 'powerset', midsName: secondaryPath, message: `Could not resolve secondary powerset` });
  }

  let primaryPowerset = primaryId ? getPowerset(primaryId) : null;
  let secondaryPowerset = secondaryId ? getPowerset(secondaryId) : null;

  // 6b. VEAT branch detection: if the resolved primary/secondary is a branch powerset,
  //     normalize to the base powerset. The planner expects build.primary.id / secondary.id
  //     to always be the BASE powerset, with branch powers stored in the powers array.
  let detectedBranch: string | null = null;
  // Track branch powerset powers so we can include them in first-pass lookups
  let branchPrimaryPowers: Power[] = [];
  let branchSecondaryPowers: Power[] = [];
  // Build a set of all VEAT branch powerset IDs mapped to primary/secondary
  const branchPrimarySetIds = new Set<string>();
  const branchSecondarySetIds = new Set<string>();
  if (archetype.branches) {
    for (const branchDef of Object.values(archetype.branches)) {
      if (branchDef.primarySet) branchPrimarySetIds.add(branchDef.primarySet);
      if (branchDef.secondarySet) branchSecondarySetIds.add(branchDef.secondarySet);
    }
    for (const [branchId, branchDef] of Object.entries(archetype.branches)) {
      if (primaryId === branchDef.primarySet || secondaryId === branchDef.secondarySet) {
        detectedBranch = branchId;
        // Save branch powers before replacing with base
        branchPrimaryPowers = (branchDef.primarySet ? getPowerset(branchDef.primarySet)?.powers : undefined) ?? [];
        branchSecondaryPowers = (branchDef.secondarySet ? getPowerset(branchDef.secondarySet)?.powers : undefined) ?? [];
        // Replace with base powersets — keep the resolved IDs for power lookup
        primaryId = archetype.primarySets[0] ?? primaryId;
        secondaryId = archetype.secondarySets[0] ?? secondaryId;
        primaryPowerset = primaryId ? getPowerset(primaryId) : null;
        secondaryPowerset = secondaryId ? getPowerset(secondaryId) : null;
        break;
      }
    }
  }

  // 7. Resolve pool and epic powersets from PowerSets array
  const poolIds: string[] = [];
  let epicPoolId: string | null = null;

  for (let i = 2; i < mbd.PowerSets.length; i++) {
    const path = mbd.PowerSets[i];
    if (!path) continue;

    if (path.startsWith('Pool.')) {
      const poolId = resolvePoolId(path);
      if (poolId && getPowerPool(poolId)) {
        // Guard against a source .mbd that lists the same pool twice (or two
        // name variants that resolvePoolId collapses to one id). Without this,
        // poolIds gets duplicate entries and the pools .map below emits two
        // identical PoolSelection objects. Mirrors the guarded push at the
        // PowerEntries stage (see `if (!poolIds.includes(poolId))` below).
        if (!poolIds.includes(poolId)) poolIds.push(poolId);
      } else if (poolId) {
        warnings.push({ type: 'pool', midsName: path, message: `Pool not found: ${poolId}` });
      }
    } else if (path.startsWith('Epic.')) {
      epicPoolId = resolveEpicPoolId(path, archetypeId);
      if (!epicPoolId) {
        warnings.push({ type: 'epic', midsName: path, message: `Could not resolve epic pool` });
      }
    }
  }

  // 7b. Build epic lookup (after resolving epicPoolId so it can be included)
  const epicLookup = buildEpicLookup(archetypeId, epicPoolId ? [epicPoolId] : undefined);

  // 8. Process PowerEntries

  // Build reverse lookup: sub-power display name → parent power name
  // Only for non-slottable groups (e.g., Adaptation toggles, Swap Ammo)
  const grantedSubPowerParent = new Map<string, string>();
  // Reverse lookup for SLOTTABLE form sub-powers (Kheldian Nova/Dwarf attacks):
  // sub-power internalName (lowercased) → parent form name. Mids exports these
  // under Inherent.Inherent.* rather than in the form's powerset.
  const slottableSubPowerParent = new Map<string, string>();
  for (const [parentName, group] of Object.entries(GRANTED_POWER_GROUPS)) {
    if (group.slottable) {
      for (const subName of group.grantedPowers) {
        slottableSubPowerParent.set(subName.toLowerCase(), parentName);
      }
      continue;
    }
    for (const subName of group.grantedPowers) {
      grantedSubPowerParent.set(subName, parentName);
    }
  }
  // Track which sub-power is active (StatInclude: true) per parent
  const activeSubPowers = new Map<string, string>();

  const primaryPowers: SelectedPower[] = [];
  const secondaryPowers: SelectedPower[] = [];
  const poolPowersMap: Record<string, SelectedPower[]> = {};
  const epicPowers: SelectedPower[] = [];
  const inherentSlotData: SelectedPower[] = []; // Slot data from Inherent.* entries
  const incarnateResults: Partial<Record<IncarnateSlotId, SelectedIncarnatePower>> = {};
  const accoladeIds: string[] = [];
  /**
   * Which Mids entry took each resolved power, so a later entry landing on the same one
   * can tell a duplicate apart from a collision. See `claimSlot` below.
   */
  const claimedBy = new Map<string, string>();
  /**
   * The slot rows the file wrote for each power we resolved, kept so step 13b can seed
   * `slotOrder` with the level the AUTHOR placed each slot at (MBDEXPORT-18).
   *
   * `SlotEntries[].Level` was read nowhere but `midsMaxUsedLevel`, which takes a max over the
   * whole file and throws the individual levels away. So every imported build reached
   * `ensureSlotOrderPopulated` with an empty `slotOrder`, fell into respec mode, and had a
   * levelling history synthesised over the top of the one it arrived with — 126 powers over
   * all eight corpus files disagreed with their own source, and display, print, forum export
   * and the `.mbd` writer all read that synthesis because all four share the solver.
   *
   * Keyed by the resolved `SelectedPower` object rather than by name: the inherent merge
   * below re-homes slot data onto a different object, and a name is not unique across
   * categories. An entry whose claim then fails is simply never walked.
   */
  const fileSlotRows = new Map<SelectedPower, MbdSlotEntry[]>();
  // Capture Mids' per-power slider (VariableValue) for things like Siphon
  // Speed stacks or Domination duration. Keyed by power internalName so the
  // caller can write directly to uiStore.targetsHitValues after applying
  // the build.
  const targetsHitValues: Record<string, number> = {};

  for (const poolId of poolIds) {
    poolPowersMap[poolId] = [];
  }

  for (const entry of mbd.PowerEntries) {
    if (!entry.PowerName) continue;

    /**
     * What this entry brought, read off the file before anything below has had a
     * chance to decline it (MBDIMPORT-5).
     *
     * There are a dozen ways out of this body — a silent skip, an unrecognised
     * format, the reassign guard, a name no powerset holds, a claim another entry
     * already made — and each one used to leave whatever the entry was carrying in
     * no count at all. The Stalker corpus file lost six enhancements down one of
     * them beside `enhancementsFailed: 0`, which is worse than a break: the summary
     * positively asserted that nothing had failed.
     *
     * So the accounting is `finally` rather than a line at each exit. A counter you
     * have to remember to reach is a counter the next refusal path will miss, and
     * this row exists because that is exactly what happened.
     */
    const held = entryEnhancements(entry);
    const slotsHeld = entry.SlotEntries?.length ?? 0;
    const enhBefore = summary.enhancementsImported + summary.enhancementsFailed;
    const slotsBefore = summary.slotsImported;
    try {

    // Handle incarnate powers separately (Incarnate.Alpha.Musculature_Radial_Paragon)
    if (entry.PowerName.startsWith('Incarnate.')) {
      const incResult = processIncarnateEntry(entry, warnings, summary);
      if (incResult) {
        incarnateResults[incResult.slotId] = incResult;
      }
      continue;
    }

    // Accolades ride in as temporary powers, and the generic `Temporary_Powers.` skip
    // further down would drop them. They are toggles rather than picks, so they are
    // resolved here and never reach `processEntry`.
    if (entry.PowerName.startsWith('Temporary_Powers.Accolades.')) {
      const id = processAccoladeEntry(entry, warnings, summary);
      if (id && !accoladeIds.includes(id)) accoladeIds.push(id);
      continue;
    }

    // Skip non-slottable granted sub-powers (e.g., Defensive/Efficient/Offensive Adaptation)
    // These are auto-displayed under their parent power; importing them as separate entries
    // would cause them to appear as standalone picked powers.
    {
      const segments = entry.PowerName.split('.');
      const internalName = segments[segments.length - 1];
      const parentName = grantedSubPowerParent.get(internalName);
      if (parentName) {
        // Capture which sub-power is active (StatInclude: true)
        if (entry.StatInclude) {
          activeSubPowers.set(parentName, internalName);
        }
          continue;
      }
    }

    // Slottable form sub-powers (Kheldian Nova/Dwarf attacks). Mids exports
    // these under `Inherent.Inherent.<name>` instead of in the form's powerset,
    // so the generic Inherent.* path would drop them (no matching auto-populated
    // inherent to merge into). Resolve against the primary/secondary powerset
    // and attach as auto-granted, slot-preserving sub-powers so they nest under
    // the parent form instead of being lost.
    {
      const segments = entry.PowerName.split('.').map((s) => s.trim());
      const subInternal = segments[segments.length - 1];
      const parentName = slottableSubPowerParent.get(subInternal.toLowerCase());
      if (parentName) {
        const primaryMatch = primaryId
          ? findPowerByMidsName(
              [...(primaryPowerset?.powers ?? []), ...branchPrimaryPowers],
              subInternal,
              [primaryPowerset, ...[...branchPrimarySetIds].map((id) => getPowerset(id))],
            )
          : null;
        const secondaryMatch = !primaryMatch && secondaryId
          ? findPowerByMidsName(
              [...(secondaryPowerset?.powers ?? []), ...branchSecondaryPowers],
              subInternal,
              [secondaryPowerset, ...[...branchSecondarySetIds].map((id) => getPowerset(id))],
            )
          : null;
        const match = primaryMatch ?? secondaryMatch;
        if (match) {
          const setId = (primaryMatch ? primaryId : secondaryId)!;
          const subPower = buildSelectedPower(
            match,
            setId,
            entry.Level,
            entry.StatInclude,
            entry.SlotEntries ?? [],
            warnings,
            summary,
          );
          subPower.isAutoGranted = true;
          subPower.grantedByPower = parentName;
          // `isActive` is whatever `buildSelectedPower` read off `StatInclude`, and it is not
          // re-derived here. This used to force Toggle and Auto sub-powers on and everything
          // else off, which predates the StatInclude mirror and outlived its reason: it threw
          // away the author's flag on ten of the Warshade's form attacks (MBDEXPORT-13), and
          // for an Auto it was never needed — the calc gate reads `isAuto || isActive`.
          // This block `continue`s, so the generic capture below never sees a form
          // sub-power and its slider was the one power's the round trip kept losing
          // (MBDEXPORT-19). Keyed on the resolved power's own internalName, which is what
          // the UI store and the writer both address it by.
          if (entry.VariableValue && match.internalName) {
            targetsHitValues[match.internalName] = entry.VariableValue;
          }
          // Step 13b seeds `slotOrder` from this map, and this branch `continue`s before the
          // generic capture that fills it — so a form attack reached the solver with no stored
          // level and was packed greedily (MBDEXPORT-18's fourth cause; `collectAllPowers`
          // already lets these powers in here, which is the half SLOT-1 had done).
          if (entry.SlotEntries?.length) fileSlotRows.set(subPower, entry.SlotEntries);
          const targetList = primaryMatch ? primaryPowers : secondaryPowers;
          if (!targetList.some((p) => p.internalName === match.internalName)) {
            targetList.push(subPower);
                }
        } else {
          warnings.push({ type: 'power', midsName: entry.PowerName, message: 'Form sub-power not found in primary/secondary powerset' });
          summary.powersFailed++;
        }
        continue;
      }
    }

    const result = processEntry(
      entry,
      archetypeId,
      primaryId,
      [...(primaryPowerset?.powers ?? []), ...branchPrimaryPowers],
      secondaryId,
      [...(secondaryPowerset?.powers ?? []), ...branchSecondaryPowers],
      poolLookup,
      epicLookup,
      powersetLookup,
      branchPrimarySetIds,
      branchSecondarySetIds,
      warnings,
      summary,
      importFork,
    );

    if (!result) continue;

    // The one place both the file's rows and the power they resolved to are in hand. Form
    // sub-powers `continue` above and never reach here, which costs nothing: they are
    // auto-granted, and `collectAllPowers` keeps auto-granted powers out of `slotOrder`.
    if (entry.SlotEntries?.length) fileSlotRows.set(result.power, entry.SlotEntries);

    // Capture the slider value Mids exports for this power (stacks /
    // targets hit). Only record non-zero so we don't clutter the UI
    // store with default values.
    if (entry.VariableValue && result.power.internalName) {
      targetsHitValues[result.power.internalName] = entry.VariableValue;
    }

    // Deduplicate: skip if a power with the same internalName already exists
    // in the target list. Mids .mbd files can contain duplicate entries for the
    // same power, which would cause the power to appear twice (same slot, same level).
    const powerName = result.power.internalName;

    /**
     * Take the slot for this power, or report why we could not.
     *
     * A repeat of the SAME Mids name is an ordinary duplicate entry and stays silent —
     * Mids files carry those. Two DIFFERENT names landing on one power is not a duplicate,
     * it is a collision, and it was the quiet half of MBDIMPORT-2: the loser's slots and
     * enhancements were dropped by a bare `break`, with `warnings: []` and `powersFailed: 0`
     * still on the summary. The user's only symptom was a power missing from a build they
     * watched import cleanly. A soft failure is a lie; this is the line that makes it a
     * bug report instead.
     */
    const claimSlot = (held: SelectedPower[]): boolean => {
      const incumbent = held.find((p) => p.internalName === powerName);
      if (!incumbent) {
        claimedBy.set(powerName, entry.PowerName);
        return true;
      }
      /**
       * The slots were counted as they resolved, and it is only here that we learn
       * they have nowhere to land. Hand them back so the reconciliation at the foot
       * of the loop reports them, rather than the summary counting as imported six
       * pieces the build does not hold. A duplicate carrying nothing still returns
       * silently; one carrying pieces now says which pieces.
       */
      summary.enhancementsImported -= (result.power.slots ?? []).filter(Boolean).length;
      summary.slotsImported -= entry.SlotEntries?.length ?? 0;
      const by = claimedBy.get(powerName);
      if (by && by !== entry.PowerName) {
        warnings.push({
          type: 'power',
          midsName: entry.PowerName,
          message: `Resolved to '${result.power.name}', which '${by}' already claimed — `
            + 'this entry and its slots were dropped',
        });
      }
      return false;
    };

    switch (result.category) {
      case 'primary':
        if (!claimSlot(primaryPowers)) break;
        primaryPowers.push(result.power);
        break;
      case 'secondary':
        if (!claimSlot(secondaryPowers)) break;
        secondaryPowers.push(result.power);
        break;
      case 'pool': {
        const poolId = result.poolId!;
        if (!poolPowersMap[poolId]) {
          poolPowersMap[poolId] = [];
          if (!poolIds.includes(poolId)) poolIds.push(poolId);
        }
        if (!claimSlot(poolPowersMap[poolId])) break;
        poolPowersMap[poolId].push(result.power);
        break;
      }
      case 'epic':
        if (!claimSlot(epicPowers)) break;
        epicPowers.push(result.power);
        break;
      case 'inherent':
        inherentSlotData.push(result.power);
        break;
    }

    } finally {
      const enhReached = summary.enhancementsImported + summary.enhancementsFailed - enhBefore;
      const slotsReached = summary.slotsImported - slotsBefore;
      if (slotsHeld > slotsReached) {
        summary.slotsSkipped = (summary.slotsSkipped ?? 0) + (slotsHeld - slotsReached);
      }
      if (held > enhReached) {
        const orphaned = held - enhReached;
        summary.enhancementsFailed += orphaned;
        warnings.push({
          type: 'enhancement',
          midsName: entry.PowerName,
          message: `${orphaned} slotted enhancement${orphaned === 1 ? '' : 's'} left the `
            + 'build with this entry',
        });
      }
    }
  }

  // 8b. Apply activeSubPower to parent powers from granted sub-power tracking.
  //
  // `parentName` comes from the reverse map, which collapses last-write-wins for
  // stances whose sub-powers are granted by more than one internal name — Bio
  // Armor registers BOTH "Adaptation" and "Evolution" as granting the stances,
  // so `parentName` is always "Evolution". That is correct on Scrapper/Brute/
  // Tanker but WRONG on Stalker/Sentinel, whose switcher is internal
  // "Adaptation" (no "Evolution" power exists) — a bare `internalName === parentName`
  // match would drop the imported stance there. For stance-group sub-powers,
  // resolve the real switcher via `findStanceParent` (which prefers the
  // `parentMechanic` and never picks the same-named "Evolving Armor" toggle);
  // fall back to the name match for non-stance grants (Boomerang Slice, etc.).
  const stanceGroupBySubPower = new Map<string, (typeof STANCE_GROUPS)[number]>();
  for (const group of STANCE_GROUPS) {
    for (const o of group.options) {
      if (o.subPower) stanceGroupBySubPower.set(o.subPower, group);
    }
  }
  const allImportedPlayerPowers = [...primaryPowers, ...secondaryPowers];
  for (const [parentName, activeSubName] of activeSubPowers) {
    const stanceGroup = stanceGroupBySubPower.get(activeSubName);
    const parent = stanceGroup
      ? (findStanceParent(allImportedPlayerPowers, stanceGroup) as SelectedPower | undefined)
      : allImportedPlayerPowers.find(p => p.internalName === parentName);
    if (parent) parent.activeSubPower = activeSubName;
  }

  // 8c. Auto-detect primary/secondary powerset if initial resolution failed
  //     but powers were found via brute-force fallback
  if (!primaryId && primaryPowers.length > 0) {
    const detectedId = primaryPowers[0].powerSet;
    if (detectedId && detectedId !== 'Inherent') {
      const detected = getPowerset(detectedId);
      if (detected) {
        primaryId = detectedId;
        primaryPowerset = detected;
      }
    }
  }
  if (!secondaryId && secondaryPowers.length > 0) {
    const detectedId = secondaryPowers[0].powerSet;
    if (detectedId && detectedId !== 'Inherent') {
      const detected = getPowerset(detectedId);
      if (detected) {
        secondaryId = detectedId;
        secondaryPowerset = detected;
      }
    }
  }

  // 9. Build pool selections
  const pools: PoolSelection[] = poolIds.map((id) => {
    const pool = getPowerPool(id);
    return {
      id,
      name: pool?.name ?? id,
      // Fresh array per pool: never let two PoolSelection objects alias the
      // same poolPowersMap[id] array (defense-in-depth if a duplicate id ever
      // slips past the guard above).
      powers: [...(poolPowersMap[id] ?? [])],
    };
  });

  // 10. Build epic pool selection
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

  // 11. Build inherent powers
  const inherents = getInherentSelectedPowers(
    archetypeId,
    archetype.name,
    archetype.inherent,
  );

  // 11b. Merge slot data from .mbd Inherent.* entries into the auto-populated inherents
  for (const slotPower of inherentSlotData) {
    const match = inherents.find(
      (inh) => inh.name.toLowerCase() === slotPower.name.toLowerCase()
    );
    if (!match) continue;
    // The include flag comes off the file even when the entry brought no slots with it; the
    // synthesised inherent is a roster row, so the file is the only thing that ever states it.
    if (slotPower.isActive) {
      match.isActive = true;
    }
    if (slotPower.slots.length > 0) {
      match.slots = slotPower.slots;
      // The slots move to the roster row, so the file rows that describe them move with
      // them — `slotPower` itself is dropped here and never reaches step 13b.
      const rows = fileSlotRows.get(slotPower);
      if (rows) fileSlotRows.set(match, rows);
      // Carry over inherent slot count (Rebirth Health/Stamina auto-grants)
      if (slotPower.inherentSlotCount) {
        match.inherentSlotCount = slotPower.inherentSlotCount;
      }
    }
  }

  // 12. Construct the Build object
  const build: Build = {
    name: mbd.Name || `${archetype.name} Import`,
    // Server identifier — the fork this build was read INTO, which the mismatch guard above
    // (step 2a) has already made agree with the file's own claim wherever the file makes one.
    serverId: importFork ?? 'homecoming',
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
    accolades: accoladeIds,
    settings: {
      origin,
    },
    sets: {},
    incarnates: {
      ...createEmptyIncarnateBuildState(),
      ...incarnateResults,
    },
    craftingChecklist: {},
    incarnateObtained: {},
    shoppingListAcquired: {},
    slotOrder: [],
    mutedOverCapStats: [],
  };

  // 13. Recompute set tracking
  build.sets = computeSetTracking(build);

  // 13b. Seed slotOrder with the level the FILE places each slot at, before step 14 gets a
  // chance to invent one (MBDEXPORT-18).
  seedSlotOrderFromFile(build, fileSlotRows);

  // 14. Populate slotOrder with one entry per non-base slot, anchored at the
  // respec-computed level. Without this, the first add/remove slot
  // interaction flips slot-level computation into leveling mode with only
  // the touched entry recorded — every other slot collapses to its power's
  // pick level. See `ensureSlotOrderPopulated` for the full diagnosis.
  //
  // This pure parser has no store access and so no opinion on Level Up mode
  // (SLOT-3) — `false` here is a permanent no-op; the store's own
  // `importMidsBuild` action re-runs this same call against the live mode
  // right after, which is the one that can actually populate real levels.
  //
  // It now runs over a slotOrder step 13b has already seeded, and only fills what the file
  // did not state — a slot on a power no entry resolved to, or one the file left short.
  ensureSlotOrderPopulated(build, false);

  // Counted from the finished build rather than tallied on the way in, so the number the
  // import dialog reports and the number the dashboard's Pwr chip reports are the same
  // number computed the same way. See `countBudgetPowerPicks`.
  summary.powersImported = countBudgetPowerPicks(build);

  /**
   * The reconciliation the per-entry accounting above is supposed to make true, asserted
   * rather than assumed. It cannot catch a shortfall — the `finally` already sweeps those
   * into `enhancementsFailed` — but it does catch the other direction, an entry counted
   * twice, which is what a dropped duplicate looked like before `claimSlot` handed its
   * count back.
   */
  const fileEnhancements = mbd.PowerEntries.reduce((n, e) => n + entryEnhancements(e), 0);
  const accounted = summary.enhancementsImported + summary.enhancementsFailed;
  if (accounted !== fileEnhancements) {
    warnings.push({
      type: 'general',
      midsName: '',
      message: `Import accounted for ${accounted} enhancements, and the file holds `
        + `${fileEnhancements}`,
    });
  }

  return {
    success: true,
    build,
    warnings,
    summary,
    detectedBranch,
    targetsHit: Object.keys(targetsHitValues).length > 0 ? targetsHitValues : undefined,
  };
}

/**
 * Write the file's own slot levels into `slotOrder`, one entry per placed slot.
 *
 * Mids states a level on every `SlotEntry`, and it is the only record of when the author
 * actually put that slot there. Nothing downstream can re-derive it: the grant schedule says
 * which levels were AVAILABLE, never which one this slot took, so a solver handed an empty
 * `slotOrder` packs the slots as a respec would and the result is a levelling history the
 * author never made (MBDEXPORT-18). Display, print, forum export and the `.mbd` writer all
 * read `computeAllSlotLevels`, so seeding here corrects all four at once.
 *
 * Two kinds of slot are skipped, and both are skipped because they are not allocations:
 * index 0 comes free with the power pick, and a row the file marks `IsInherent` is an
 * auto-granted freebie sitting at a fixed level (Rebirth's Health and Stamina). Those are the
 * same two `userSlotFloor` excludes, read off the file rather than re-derived.
 *
 * The seeded level is a preference, not a guarantee, and under SLOT-1's matching solver that
 * is a stronger preference than canonical's copy can offer: `assignGrants` seeds every legal
 * stored level first and only displaces one when leaving it would cost another slot its grant
 * entirely. A level the schedule grants nothing at — Mids' respec rows at 47 and 49 — is
 * cleared by `scrubFabricatedSlotLevels` before the store's own pass ever sees it, which is
 * the honest outcome: the file states a placement this fork cannot house.
 *
 * Returns how many entries it wrote.
 */
function seedSlotOrderFromFile(
  build: Build,
  fileSlotRows: Map<SelectedPower, MbdSlotEntry[]>,
): number {
  const walk: { power: SelectedPower; category: string }[] = [
    ...build.inherents.map((power) => ({ power, category: 'inherent' })),
    ...build.primary.powers.map((power) => ({ power, category: 'primary' })),
    ...build.secondary.powers.map((power) => ({ power, category: 'secondary' })),
    ...build.pools.flatMap((pool) => pool.powers.map((power) => ({ power, category: 'pool' }))),
    ...(build.epicPool?.powers ?? []).map((power) => ({ power, category: 'epic' })),
  ];

  const seeded: Build['slotOrder'] = [];
  for (const { power, category } of walk) {
    const rows = fileSlotRows.get(power);
    if (!rows) continue;
    // `buildSelectedPower` pushes one slot per row, so the two are parallel — except for the
    // empty-entry case, which synthesises a base slot the file did not write. Bound by both.
    const n = Math.min(power.slots.length, rows.length);
    for (let s = 1; s < n; s++) {
      const row = rows[s];
      if (row.IsInherent) continue;
      if (typeof row.Level !== 'number') continue;
      // The file's own record of when its author placed the slot — a history, just not one
      // made here, so it rides back out unchanged rather than being re-solved (MBDEXPORT-21).
      seeded.push({
        powerName: power.internalName,
        slotIndex: s,
        category,
        level: row.Level,
        levelSource: 'authored',
      });
    }
  }

  build.slotOrder = [...build.slotOrder, ...seeded];
  return seeded.length;
}

// ============================================
// ENTRY PROCESSING
// ============================================

interface ProcessedEntry {
  category: 'primary' | 'secondary' | 'pool' | 'epic' | 'inherent';
  power: SelectedPower;
  poolId?: string;
}

/**
 * Determine whether a powerset should be categorized as primary or secondary.
 * VEAT branch powersets have category 'epic' in their definitions, so we check
 * if the powerset ID is a known branch primary/secondary set.
 */
function categorizePowerset(
  powersetId: string,
  rawCategory: string | undefined,
  branchPrimarySetIds: Set<string>,
  branchSecondarySetIds: Set<string>,
): 'primary' | 'secondary' {
  if (branchPrimarySetIds.has(powersetId)) return 'primary';
  if (branchSecondarySetIds.has(powersetId)) return 'secondary';
  return rawCategory === 'primary' ? 'primary' : 'secondary';
}

function processEntry(
  entry: MbdPowerEntry,
  archetypeId: string,
  primaryId: string | null,
  primaryPowers: Power[],
  secondaryId: string | null,
  secondaryPowers: Power[],
  poolLookup: Map<string, PoolPowerMatch>,
  epicLookup: Map<string, EpicPowerMatch>,
  powersetLookup: Map<string, string>,
  branchPrimarySetIds: Set<string>,
  branchSecondarySetIds: Set<string>,
  warnings: MidsImportWarning[],
  summary: MidsImportSummary,
  server: DatasetId | null,
): ProcessedEntry | null {
  // Some Mids exports (Rebirth Guardian builds we've seen) emit power
  // names with trailing whitespace inside segments
  // ("Guardian_Composition.Energy_Composition .Kinetic_Shield"). Normalize
  // the whole name once so every downstream check (skip-prefix tests,
  // segment splits, lookup-map keys) sees a clean form.
  const PowerName = remapMidsPath(
    entry.PowerName.split('.').map(s => s.trim()).join('.'),
    server,
  );
  const { Level: midsLevel, StatInclude, SlotEntries } = entry;
  const appLevel = midsLevel; // Mids Level is already 1-based

  // Extract segments: "Brute_Melee.Kinetic_Attack.Quick_Strike" → ["Brute_Melee", "Kinetic_Attack", "Quick_Strike"]
  const segments = PowerName.split('.');

  /**
   * This powerset has given the name to a different power, and offers no counterpart for
   * the one Mids means (MBDIMPORT-2). Every exact-name door below has to honour it — the
   * powerset matcher does so through `findPowerByMidsName`, and the pool and epic fullName
   * lookups need it stated here because they never call that.
   */
  const reassigned = segments.length >= 3
    && midsNameIsRetired(`${segments[0]}.${segments[1]}`, segments[2]);

  // Skip temporary powers. Accolades share this prefix but are pulled out by the caller
  // before we get here — see `processAccoladeEntry`. Everything else under it (day-job
  // powers, mission temps) has no planner representation at all.
  if (PowerName.startsWith('Temporary_Powers.')) {
    return null;
  }

  // Silent skip list: Mids-only artifacts or auto-granted passives with no
  // user-selectable counterpart in HC. Drop them without a warning.
  if (MIDS_SILENT_SKIP_PATHS.has(PowerName.toLowerCase())) {
    return null;
  }

  // Mastermind pet shadow entries: older Mids exports duplicate every pet with
  // a `_H` suffix and zero slots. They're auto-granted "henchman" upgrade
  // references, not user picks. Skip them silently.
  if (PowerName.startsWith('Mastermind_Summon.') && PowerName.endsWith('_H')) {
    return null;
  }

  // Process inherent powers for their slot data (powers are auto-populated,
  // but we need to preserve any slotted enhancements from the import)
  if (PowerName.startsWith('Inherent.')) {
    if (segments.length < 3 || !SlotEntries) {
      return null;
    }
    const powerInternalName = segments[2];
    // An inherent entry carries two things the roster cannot: the author's include flag, and
    // the slots they spent on it. Both used to be conditional on a piece sitting in one of
    // those slots — the test here was `!SlotEntries.some(s => s.Enhancement)` and an all-empty
    // entry was dropped whole. That lost the flag (MBDEXPORT-13): Swift and Hurdle hold one
    // empty slot each, so on the round trip the entry stopped carrying anything and both powers
    // disappeared. It also lost the placements (MBDEXPORT-15) — Rebirth grants Health and
    // Stamina two slots each and the file is the only record of them, so an entry of three
    // empty slots came back a bare roster row.
    //
    // So every entry goes through the loop below now. A placement survives whether or not it
    // holds anything, and `slotsImported` counts it where the early return left it to
    // `slotsSkipped` — the MBDIMPORT-5 reconciliation is the same sum either way, but the
    // slots are on the side that says they arrived.
    //
    // Ported from canonical, whose `.mbd` round trip against its own writer measured both.
    const slots: (Enhancement | null)[] = [];
    let inherentSlotCount = 0;
    for (const slotEntry of SlotEntries) {
      summary.slotsImported++;
      if (slotEntry.IsInherent) inherentSlotCount++;
      if (!slotEntry.Enhancement) {
        slots.push(null);
        continue;
      }
      const { enhancement, warning } = resolveSlotEnhancement(slotEntry.Enhancement);
      if (warning) { warnings.push(warning); summary.enhancementsFailed++; }
      if (enhancement) { summary.enhancementsImported++; }
      slots.push(enhancement);
    }
    return {
      category: 'inherent',
      power: {
        name: powerInternalName.replace(/_/g, ' '),
        // Carried so the slider capture in the main loop can key on it (MBDEXPORT-19). This
        // struct is a transport — step 11b merges it back by DISPLAY name and reads only
        // `isActive`, `slots` and `inherentSlotCount` — so nothing else consumes this field;
        // without it the capture guard fails its `internalName` test and an inherent's
        // `VariableValue` is dropped on the way in. A Warshade's Dark Sustenance is the
        // corpus case. `claimSlot` is never called for an inherent, so dedup is untouched.
        internalName: powerInternalName,
        powerSet: 'Inherent',
        level: 1,
        available: -1,
        maxSlots: 6,
        slots,
        effects: {},
        inherentSlotCount: inherentSlotCount > 0 ? inherentSlotCount : undefined,
        isActive: StatInclude ? true : undefined,
      } as SelectedPower,
    };
  }

  // Try pool powers first (Pool.X.Y)
  if (PowerName.startsWith('Pool.')) {
    // The fullName lookup is an exact internal-name match wearing a different hat, so it
    // needs the same guard: `Pool.Flight.Afterburner` is a real fullName here, and an
    // older Mids file naming the pre-rework Afterburner would walk straight past the remap
    // into the power now displayed "Evasive Maneuvers". Withholding the direct hit sends
    // it to `findPowerByMidsName`, which resolves it on display instead.
    const poolMatch = reassigned ? undefined : poolLookup.get(PowerName);
    if (poolMatch) {
      const power = buildSelectedPower(
        poolMatch.power,
        poolMatch.poolId,
        appLevel,
        StatInclude,
        SlotEntries,
        warnings,
        summary,
      );
      return { category: 'pool', power, poolId: poolMatch.poolId };
    }

    // Fallback: try to find by power name within the pool
    if (segments.length >= 3) {
      const poolId = segments[1].toLowerCase();
      const pool = getPowerPool(poolId);
      if (pool) {
        const powerDef = findPowerByMidsName(pool.powers, segments[2], [pool]);
        if (powerDef) {
          const power = buildSelectedPower(powerDef, poolId, appLevel, StatInclude, SlotEntries, warnings, summary);
              return { category: 'pool', power, poolId };
        }
      }
    }

    warnings.push({ type: 'pool', midsName: PowerName, message: 'Pool power not found' });
    summary.powersFailed++;
    return null;
  }

  // Try epic powers (Epic.X.Y)
  if (PowerName.startsWith('Epic.')) {
    // Same second door as the pool branch above — see `reassigned`.
    const epicMatch = reassigned ? undefined : epicLookup.get(PowerName);
    if (epicMatch) {
      const power = buildSelectedPower(
        epicMatch.power,
        epicMatch.epicPoolId,
        appLevel,
        StatInclude,
        SlotEntries,
        warnings,
        summary,
      );
      return { category: 'epic', power };
    }

    // Fallback: search epic pool by power name if the fullName lookup failed
    if (segments.length >= 3) {
      const epicPoolId = resolveEpicPoolId(`Epic.${segments[1]}`, archetypeId);
      if (epicPoolId) {
        const epicPool = getEpicPool(epicPoolId);
        if (epicPool) {
          const powerDef = findPowerByMidsName(epicPool.powers, segments[2], [epicPool]);
          if (powerDef) {
            const power = buildSelectedPower(powerDef, epicPoolId, appLevel, StatInclude, SlotEntries, warnings, summary);
                  return { category: 'epic', power };
          }
        }
      }
    }

    warnings.push({ type: 'epic', midsName: PowerName, message: 'Epic power not found' });
    summary.powersFailed++;
    return null;
  }

  // Regular power: determine if primary or secondary
  if (segments.length < 3) {
    warnings.push({ type: 'power', midsName: PowerName, message: 'Unrecognized power format' });
    summary.powersFailed++;
    return null;
  }

  const powerInternalName = segments[2];

  // Try primary
  if (primaryId) {
    const match = findPowerByMidsName(primaryPowers, powerInternalName, [
      getPowerset(primaryId),
      ...[...branchPrimarySetIds].map((id) => getPowerset(id)),
    ]);
    if (match) {
      // Determine correct powerset ID — may be a branch powerset, not the base
      let correctSetId = primaryId;
      for (const branchSetId of branchPrimarySetIds) {
        const branchSet = getPowerset(branchSetId);
        if (branchSet?.powers.some(p => p.internalName === match.internalName)) {
          correctSetId = branchSetId;
          break;
        }
      }
      const power = buildSelectedPower(match, correctSetId, appLevel, StatInclude, SlotEntries, warnings, summary);
      return { category: 'primary', power };
    }
  }

  // Try secondary
  if (secondaryId) {
    const match = findPowerByMidsName(secondaryPowers, powerInternalName, [
      getPowerset(secondaryId),
      ...[...branchSecondarySetIds].map((id) => getPowerset(id)),
    ]);
    if (match) {
      // Determine correct powerset ID — may be a branch powerset, not the base
      let correctSetId = secondaryId;
      for (const branchSetId of branchSecondarySetIds) {
        const branchSet = getPowerset(branchSetId);
        if (branchSet?.powers.some(p => p.internalName === match.internalName)) {
          correctSetId = branchSetId;
          break;
        }
      }
      const power = buildSelectedPower(match, correctSetId, appLevel, StatInclude, SlotEntries, warnings, summary);
      return { category: 'secondary', power };
    }
  }

  // The .mbd names the powerset, and its own matchers have now had their turn. A name that
  // set has reassigned, with no counterpart of its own, is a definite answer: the power is
  // gone. Ranging on into the fallbacks below would bind a same-named power from a set the
  // character never took — Willpower's `Reconstruction` finding Regeneration's — and a
  // wrong answer that looks deliberate is worse than none.
  if (reassigned) {
    warnings.push({
      type: 'power',
      midsName: PowerName,
      message: `'${powerInternalName}' names a different power in this dataset, `
        + 'and the one Mids means has no counterpart here',
    });
    summary.powersFailed++;
    return null;
  }

  // Fallback 1: resolve the powerset directly from the power's path
  // Handles cross-archetype prefixes (e.g., "Tanker_Defense.Dark_Armor" for a Brute)
  const fallbackPowersetId = resolvePowerset(PowerName, archetypeId, powersetLookup);
  if (fallbackPowersetId) {
    const fallbackPowerset = getPowerset(fallbackPowersetId);
    if (fallbackPowerset) {
      const match = findPowerByMidsName(fallbackPowerset.powers, powerInternalName, [fallbackPowerset]);
      if (match) {
        const category = categorizePowerset(fallbackPowersetId, fallbackPowerset.category, branchPrimarySetIds, branchSecondarySetIds);
        warnFallback('mids-import/processEntry', `power '${PowerName}' resolved via powerset path (fallback 1) to '${fallbackPowersetId}' as ${category}`);
        const power = buildSelectedPower(match, fallbackPowersetId, appLevel, StatInclude, SlotEntries, warnings, summary);
          return { category, power };
      }
    }
  }

  // Fallback 2: brute-force search all powersets for this archetype
  const allPowersets = getAllPowersets();
  for (const [psId, ps] of Object.entries(allPowersets)) {
    if (ps.archetype?.toLowerCase() !== archetypeId.toLowerCase()) continue;
    const match = findPowerByMidsName(ps.powers, powerInternalName, [ps]);
    if (match) {
      const category = categorizePowerset(psId, ps.category, branchPrimarySetIds, branchSecondarySetIds);
      warnFallback('mids-import/processEntry', `power '${PowerName}' resolved via brute-force search (fallback 2) — found in '${psId}' as ${category}`);
      const power = buildSelectedPower(match, psId, appLevel, StatInclude, SlotEntries, warnings, summary);
      return { category: category as 'primary' | 'secondary', power };
    }
  }

  warnings.push({ type: 'power', midsName: PowerName, message: `Power not found in any ${archetypeId} powerset` });
  summary.powersFailed++;
  return null;
}

// ============================================
// INCARNATE POWER PROCESSING
// ============================================

/**
 * Resolve one `Temporary_Powers.Accolades.*` entry to the planner's accolade toggle id.
 *
 * Accolades are not picked powers here — the planner carries them as independent on/off
 * toggles keyed on the accolade's own internal name (`src/data/accolades.ts`), so they never
 * travel the powerset path the rest of this file walks.
 *
 * `StatInclude` is the mapping, not mere presence. Mids keeps "owned" and "counted" apart;
 * the planner has one state, and it is the counted one, so an accolade the user excluded
 * from their Mids totals must not arrive switched on.
 *
 * Three outcomes, and keeping them distinct is the point of the row that opened this:
 *   - a stat toggle  → its id, folded into `build.accolades`
 *   - a real accolade with no permanent buff (Eye of the Magus, Long Range Teleport) →
 *     silent, because the planner offers no toggle for it by design
 *   - a name in neither roster → a warning, because that is a roster divergence and the
 *     only thing that would surface it
 */
function processAccoladeEntry(
  entry: MbdPowerEntry,
  warnings: MidsImportWarning[],
  summary: MidsImportSummary,
): string | null {
  const internalName = entry.PowerName.split('.').map((s) => s.trim())[2] ?? '';
  const matches = (candidate: { internalName: string }) =>
    candidate.internalName.toLowerCase() === internalName.toLowerCase();

  const toggle = getAccolades().find(matches);
  if (toggle) {
    // `StatInclude` false means the user owns it but excluded it from their Mids totals;
    // the planner has only the counted state, so it arrives off and is not counted here.
    if (!entry.StatInclude) return null;
    summary.accoladesImported = (summary.accoladesImported ?? 0) + 1;
    return accoladeId(toggle);
  }

  if (getAllAccolades().some(matches)) {
    return null;
  }

  warnings.push({
    type: 'power',
    midsName: entry.PowerName,
    message: 'Accolade not found in this dataset',
  });
  summary.powersFailed++;
  return null;
}

function processIncarnateEntry(
  entry: MbdPowerEntry,
  warnings: MidsImportWarning[],
  summary: MidsImportSummary,
): SelectedIncarnatePower | null {
  // Silent skip: Mids-only incarnate artifacts with no HC counterpart.
  if (MIDS_SILENT_SKIP_PATHS.has(entry.PowerName.toLowerCase())) {
    return null;
  }

  // Hybrid `*_Genome_<n>` entries are Mids-only numeric indexes for Hybrid
  // tree tiers (e.g. Support_Genome_8, Melee_Genome_8). HC exposes Hybrid
  // powers under named tiers (Support_Core_Genome, Support_Total_Core_Graft,
  // etc.), not numeric suffixes. Drop the enumerated entries silently — the
  // build's actual selected power lives in its named entry elsewhere.
  if (/^Incarnate\.Hybrid\.[A-Za-z]+_Genome_\d+$/.test(entry.PowerName)) {
    return null;
  }

  const segments = entry.PowerName.split('.');
  if (segments.length < 3) return null;

  const slotName = segments[1].toLowerCase();
  // Validate slot ID
  if (!INCARNATE_SLOT_ORDER.includes(slotName as IncarnateSlotId)) {
    warnings.push({ type: 'power', midsName: entry.PowerName, message: `Unknown incarnate slot: ${slotName}` });
    summary.powersFailed++;
    return null;
  }

  const slotId = slotName as IncarnateSlotId;

  // Look up by fullName (e.g., "Incarnate.Alpha.Musculature_Radial_Paragon")
  const power = getIncarnatePower(slotId, entry.PowerName);
  if (!power) {
    warnings.push({ type: 'power', midsName: entry.PowerName, message: `Incarnate power not found` });
    summary.powersFailed++;
    return null;
  }

  const tree = getIncarnateTree(slotId, power.treeId);
  summary.incarnatesImported = (summary.incarnatesImported ?? 0) + 1;

  return {
    slotId,
    powerId: power.id,
    powerName: power.id,
    displayName: power.displayName,
    icon: power.icon,
    tier: power.tier,
    treeId: power.treeId,
    treeName: tree?.name || power.treeId,
  };
}

// ============================================
// SELECTED POWER CONSTRUCTION
// ============================================

/** What the file says this entry holds, counted off the file rather than off what resolved. */
function entryEnhancements(entry: MbdPowerEntry): number {
  return (entry.SlotEntries ?? []).filter((s) => s.Enhancement).length;
}

function buildSelectedPower(
  powerDef: Power,
  powerSetId: string,
  level: number,
  isActive: boolean,
  slotEntries: MbdPowerEntry['SlotEntries'],
  warnings: MidsImportWarning[],
  summary: MidsImportSummary,
): SelectedPower {
  // Build enhancement slots
  const slots: (Enhancement | null)[] = [];
  let inherentSlotCount = 0;

  for (const slotEntry of slotEntries) {
    summary.slotsImported++;
    if (slotEntry.IsInherent) inherentSlotCount++;

    if (!slotEntry.Enhancement) {
      slots.push(null);
      continue;
    }

    const { enhancement, warning } = resolveSlotEnhancement(slotEntry.Enhancement);

    if (warning) {
      warnings.push(warning);
      summary.enhancementsFailed++;
    }

    if (enhancement) {
      summary.enhancementsImported++;
    }

    slots.push(enhancement);
  }

  // Ensure at least one slot (the free first slot)
  if (slots.length === 0) {
    slots.push(null);
  }

  // Apply Mids' `StatInclude` directly to `isActive` for every power. Mids
  // treats StatInclude as "this power is currently contributing to my
  // totals" — Toggle/Auto powers, long self-buff Clicks like Hasten, and
  // also short self-buff Clicks (Build Up, Soul Drain) and click +Rech
  // self-buffs (Siphon Speed) all carry it. Mirroring Mids 1:1 here means a
  // fresh .mbd import reproduces Mids' default totals without the user
  // hunting for which powers to manually toggle on.
  //
  // Stored as `undefined` (rather than `false`) for powers Mids didn't
  // include, so the JSON stays minimal and the calc layer's
  // `isAuto || power.isActive` gate behaves identically.
  const effectiveIsActive = isActive ? true : undefined;

  return {
    ...powerDef,
    powerSet: powerSetId,
    level,
    slots,
    isActive: effectiveIsActive,
    inherentSlotCount: inherentSlotCount > 0 ? inherentSlotCount : undefined,
  };
}

// ============================================
// INHERENT POWERS
// ============================================

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
  // Flight, Combat Flight). Mids exports these under Inherent.Inherent.* but
  // the inherent path only preserves slot data; without auto-populating them
  // here they'd never appear for Kheldians.
  for (const def of getArchetypeInherentPowers(archetypeId || undefined)) {
    powers.push(createInherentSelectedPower(def));
  }

  return powers;
}

// ============================================
// SET TRACKING
// ============================================

function computeSetTracking(build: Build): Record<string, SetTracking> {
  const sets: Record<string, SetTracking> = {};

  const processSlots = (slots: (Enhancement | null)[]) => {
    for (const enh of slots) {
      if (enh && enh.type === 'io-set') {
        const setId = (enh as any).setId as string;
        const pieceNum = (enh as any).pieceNum as number;
        if (!sets[setId]) {
          sets[setId] = { count: 0, pieces: new Set<number>() };
        }
        if (!sets[setId].pieces.has(pieceNum)) {
          sets[setId].count++;
          sets[setId].pieces.add(pieceNum);
        }
      }
    }
  };

  build.primary.powers.forEach((p) => processSlots(p.slots));
  build.secondary.powers.forEach((p) => processSlots(p.slots));
  build.pools.forEach((pool) => pool.powers.forEach((p) => processSlots(p.slots)));
  if (build.epicPool) {
    build.epicPool.powers.forEach((p) => processSlots(p.slots));
  }
  build.inherents.forEach((p) => processSlots(p.slots));

  return sets;
}
