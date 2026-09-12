/**
 * Export a Sidekick build to Mids Reborn .mbd (JSON) format.
 * This is the reverse of src/utils/mids-import/.
 */

import type { Build, SelectedPower, Powerset, Enhancement, IOSetEnhancement, GenericIOEnhancement, SpecialEnhancement, OriginEnhancement } from '@/types';
import { INCARNATE_SLOT_ORDER } from '@/types';
import type { IncarnateSlotId, SelectedIncarnatePower } from '@/types';
import type { MbdFile, MbdPowerEntry, MbdSlotEntry, MbdEnhancement } from '@/utils/mids-import/types';
import { getPowerset, getPowersetsForArchetype } from '@/data/powersets';
import { getPowerPool } from '@/data/power-pools';
import { getEpicPool } from '@/data/epic-pools';
import { getIncarnatePower } from '@/data/incarnates';
import { getAccolade } from '@/data/accolades';
import { getIOSet } from '@/data/io-sets';
import { getMidsGenericIOUid, getMidsIOSetPieceUid, getMidsOriginUid, getMidsSpecialUid } from '@/data/mids-uids';
import {
  midsNameForExport,
  midsPowersetPathForExport,
  midsPowersetPathsKnown,
} from '@/data/mids-name-map';
import { MIDS_STAT_MAP, MIDS_ORIGIN_TIER, MIDS_DATABASE_FOR_DATASET } from '@/utils/mids-import/mappers';
import { ARCHETYPE_CLASS_MAP } from '@/utils/enhancement-uid';
import { getArchetype } from '@/data/archetypes';
import type { ArchetypeBranch, ArchetypeBranchId } from '@/types/archetype';
import { headlineArchetypeInherentName } from '@/data/inherent-rules';
import { isDatasetId, getAllDatasetMetadata } from '@/data/dataset';
import { getInherentPowers, getArchetypeInherentPowers, POWER_PICK_LEVELS, getPicksGrantedAtLevel, GRANTED_POWER_GROUPS } from '@/data';
import { computeExportSlotLevels, type SlotLevel } from '@/utils/slot-levels';
import { powerKey, type PowerCategory } from '@/utils/power-key';

// ============================================
// ARCHETYPE AND FORK, AS MIDS NAMES THEM
// ============================================

/**
 * Mids' `Class_*` token for this build's archetype — the game's own token, read off the
 * dataset rather than restated here.
 *
 * This was a hand table of fifteen `Class_` strings with `|| 'Class_Blaster'` behind it, and
 * the fallback was not theoretical: Rebirth's Guardian was never in it, so a Guardian `.mbd`
 * that MIDS ITSELF wrote came back out of here as `Class_Blaster` — and Mids has a
 * `Class_Guardian`. The export owns the token (`archetype.stats.className`, the string the
 * game's own effect gates compare against) and it agrees with all fifteen hand rows on every
 * fork, so it is read: CLAUDE.md Rule 0.
 *
 * An archetype no Mids database holds is still written under its real name, and reported.
 * `ARCHETYPE_CLASS_MAP` is the roster of classes MIDS knows — the reader's vocabulary, read
 * here as the vocabulary question it is — so "can Mids name this class" has one home rather
 * than a second list to drift from. The table, not `mapArchetypeClass`: the day that function
 * learns to answer from our own datasets (which is the reading side's business, not this
 * one's), it would answer for Primalist and quietly delete this warning. Measured in Mids
 * 3.8.6 under Wine: an unknown `Class_Primalist` opens with no error at all, unlike an unknown
 * `Database` string, so the honest token costs nothing the Blaster lie was buying.
 */
function midsClassForBuild(build: Build, warnings: MidsExportWarning[]): string {
  const archetypeId = build.archetype.id;
  const className = (archetypeId ? getArchetype(archetypeId)?.stats?.className : undefined)
    ?? build.archetype.stats?.className
    ?? '';
  if (!className) {
    warnings.push({
      power: build.archetype.name || String(archetypeId ?? 'archetype'),
      slot: 0,
      detail: 'this archetype states no class token, so Mids gets an empty Class and opens its default build',
    });
    return '';
  }
  if (!ARCHETYPE_CLASS_MAP[className]) {
    warnings.push({
      power: build.archetype.name || className,
      slot: 0,
      detail: `no Mids database has an archetype ${className} — Mids will not resolve the class or anything under it`,
    });
  }
  return className;
}

/**
 * `BuiltWith.Database`, and the warning owed when Mids has no database for this fork.
 *
 * The string used to be `serverId === 'rebirth' ? 'Rebirth' : 'Homecoming'`, so a Thunderspy
 * build asserted the one fork it definitely is not. `MIDS_DATABASE_FOR_DATASET` answers both
 * this and the reader's "which fork is this file", which is the pairing that broke: the writer
 * stamped Homecoming and the reader believed it, and our own Thunderspy export came back
 * refused. See DATA-GAP MBDEXPORT-2.
 */
function midsDatabaseForBuild(build: Build, warnings: MidsExportWarning[]): string {
  const stated = isDatasetId(build.serverId) ? build.serverId : null;
  if (!stated) {
    // A build that does not say which fork it is gets the export's oldest guess, and says so
    // rather than letting the guess pass as a fact — the whole shape of this row.
    warnings.push({
      power: String(build.serverId ?? 'no server'),
      slot: 0,
      detail: 'this build names no fork the planner ships, so the file is written as Homecoming',
    });
  }
  const fork = stated ?? 'homecoming';
  const { database, own } = MIDS_DATABASE_FOR_DATASET[fork];
  if (!own) {
    const label = getAllDatasetMetadata().find((d) => d.id === fork)?.displayName ?? fork;
    warnings.push({
      power: label,
      slot: 0,
      detail: `Mids has no ${label} database and never has, so this file names its ${database} one — `
        + 'every name below is read against that database, and what only this fork has cannot be resolved',
    });
  }
  return database;
}

// ============================================
// GENERIC IO STAT → MIDS UID SUFFIX
// ============================================

/**
 * Planner stat → the Mids UID suffix that names it, e.g. `EnduranceReduction`
 * → `Endurance_Discount` (Mids' spelling, not ours).
 *
 * Derived by inverting the import path's `MIDS_STAT_MAP` and keeping only the
 * spelling this dataset's Mids database actually ships. The hand-written
 * forward map that used to live here had drifted off Mids in seven places —
 * `EndRdx`, `Resistance`, `Immob`, `Run_Speed`, `Slow`, `EndMod` and the two
 * `Debuff`/`DeBuff` casings — and each one exported as an empty slot.
 *
 * Not memoized across datasets on purpose: the roster is per-server, and this
 * runs once per exported enhancement.
 */
function midsGenericIOSuffix(stat: string): string | null {
  for (const [suffix, mapped] of Object.entries(MIDS_STAT_MAP)) {
    if (mapped !== stat) continue;
    const uid = getMidsGenericIOUid(suffix);
    if (uid) return uid;
  }
  return null;
}

// ============================================
// HELPERS
// ============================================

type SetCategory = 'primary' | 'secondary' | 'pool' | 'epic';

/**
 * One powerset in the two namespaces a `PowerName` needs (MBDEXPORT-3, MBDEXPORT-6).
 *
 * A Mids `PowerName` is `group.set.power`. `ourKey` is the first two segments as THIS
 * dataset spells them, and it is the key both name tables are keyed by; `path` is the same
 * two segments as MIDS spells them, which is what actually goes in the file.
 */
interface MidsSet {
  ourKey: string;
  path: string;
}

/**
 * The `group.powerset` this powerset has in OUR namespace — the key the name map is keyed
 * by (MBDIMPORT-2), and the one thing the reverse lookups below cannot guess.
 */
function ourPowersetKey(powersetId: string, category: SetCategory): string {
  // Pools and epics carry no `setPath` on their runtime type, so their key comes off
  // their own first power — which is how `powersetKeysOf` reads them on the import side.
  const setPath = category === 'pool'
    ? getPowerPool(powersetId)?.powers.find((p) => p.fullName)?.fullName
    : category === 'epic'
      ? getEpicPool(powersetId)?.powers.find((p) => p.fullName)?.fullName
      : getPowerset(powersetId)?.setPath;
  const segments = (setPath ?? '').split('.');
  return segments.length >= 2 ? `${segments[0]}.${segments[1]}` : '';
}

/** What this planner calls the set, for a warning a user has to act on. */
function powersetLabel(powersetId: string, category: SetCategory): string {
  const set = category === 'pool'
    ? getPowerPool(powersetId)
    : category === 'epic'
      ? getEpicPool(powersetId)
      : getPowerset(powersetId);
  return set?.name || powersetId;
}

/**
 * A powerset resolved into Mids' namespace, or reported (DATA-GAP MBDEXPORT-6).
 *
 * The path used to be COMPOSED — an archetype table for the group and the powerset's icon
 * filename for the set — and neither is a read of what Mids calls the set. A Rebirth
 * Guardian went out as `Guardian_Comp.Electric_Armor` where Mids holds
 * `Guardian_Composition.Atmospheric_Composition`, with all nine power names already right
 * inside it; every power in the set arrived as a blank row still holding its slots.
 *
 * So it is looked up now, in the pairing the name map already computes. Where the lookup
 * misses, ours goes out — it is the game's own spelling, which is what Mids built its
 * database from, so it is the best available guess and often right — and the guess is
 * REPORTED, because the failure mode being fixed here is a silent one.
 */
function resolveMidsSet(ourKey: string, label: string, warnings: MidsExportWarning[]): MidsSet {
  if (!ourKey) return { ourKey: '', path: '' };

  const known = midsPowersetPathForExport(ourKey);
  if (known) return { ourKey, path: known };

  warnings.push({
    power: label,
    slot: 0,
    detail: midsPowersetPathsKnown()
      // The pairing ran and reached nothing: Mids has no set that answers to this one.
      ? `Mids has no powerset paired with ${ourKey} — its powers will open as empty rows`
      // No Mids database has been read for this fork at all, which is a different fact
      // and sends the reader somewhere else (MBDEXPORT-2).
      : `no Mids database has been read for this fork, so ${ourKey} is our own spelling`,
  });
  return { ourKey, path: ourKey };
}

/**
 * Resolve each `group.set` once per build, reporting each unpaired one once.
 *
 * The memo is not an optimisation: without it an unpaired set warns once per power in it,
 * and the report a user reads turns a single missing pairing into eighteen lines.
 */
function setResolver(warnings: MidsExportWarning[]): (ourKey: string, label: string) => MidsSet {
  const seen = new Map<string, MidsSet>();
  return (ourKey, label) => {
    const hit = seen.get(ourKey);
    if (hit) return hit;
    const set = resolveMidsSet(ourKey, label, warnings);
    seen.set(ourKey, set);
    return set;
  };
}

/**
 * The powerset that actually holds this power, which is not always the one the build
 * picked it from.
 *
 * An Arachnos Widow's branch powers live in their own sets — `Widow_Training.Night_Widow_Training`
 * and `Teamwork.Widow_Teamwork` — while the build holds `Widow_Training.Widow_Training`
 * and `Teamwork.Teamwork`, and Mids writes every power under its OWN set: the corpus Night
 * Widow, written by Mids itself, spreads 23 powers across four of them. Taking the build's
 * set for all of them is what cost that file nine entries and 34 enhancements.
 *
 * The build's own set answers first, so nothing changes for the archetypes that have no
 * branches. A power no single set in the archetype claims falls back to it too, because a
 * guess between two sets is not a decode — and the path lookup reports what it cannot pair.
 */
function owningPowerset(
  power: { name: string; internalName?: string },
  powersetId: string,
  archetypeId: string,
): { id: string; set: Powerset | undefined } {
  const internalName = power.internalName || power.name;
  const own = getPowerset(powersetId);
  if (own?.powers.some((p) => p.internalName === internalName)) return { id: powersetId, set: own };

  const holders = getPowersetsForArchetype(archetypeId)
    .filter((set) => set.powers.some((p) => p.internalName === internalName));
  return holders.length === 1
    ? { id: holders[0].id ?? powersetId, set: holders[0] }
    : { id: powersetId, set: own };
}

/**
 * The branch a VEAT specialised into, read off the picks that name its sets.
 *
 * A branch is one choice with two sets, and Mids records it in `PowerSets[0..1]` — the corpus
 * Night Widow is filed under `Widow_Training.Night_Widow_Training` and `Teamwork.Widow_Teamwork`
 * where its base picks still spell `Widow_Training.Widow_Training.*`. Our reader normalises the
 * two role ids to the BASE sets (the planner wants them there) and keeps the branch on the picks
 * that came from it, so the picks are where the file's claim survives on the build.
 *
 * Reading it back from them is not a re-derivation: `powerSet` is what the file said, pick by
 * pick, and `archetype.branches` is the export's own pairing of a branch to its two sets. What
 * would be a re-derivation is asking which set HOLDS a power — that answer is the dataset's and
 * has no memory of what the author chose (MBDEXPORT-16).
 *
 * One branch pick anywhere settles both roles, because specialising is what put it there. Two
 * different branches is a build no archetype offers, so it is reported and neither is used —
 * a half-branched header is a file Mids never writes and we would be inventing its meaning.
 */
function branchTaken(build: Build, warnings: MidsExportWarning[]): ArchetypeBranch | null {
  const archetypeId = build.archetype.id;
  const branches = (archetypeId ? getArchetype(archetypeId)?.branches : undefined) ?? null;
  if (!branches) return null;

  const branchOfSet = new Map<string, string>();
  for (const [branchId, branch] of Object.entries(branches)) {
    if (branch?.primarySet) branchOfSet.set(branch.primarySet, branchId);
    if (branch?.secondarySet) branchOfSet.set(branch.secondarySet, branchId);
  }

  const taken = new Set<string>();
  for (const power of [...build.primary.powers, ...build.secondary.powers]) {
    const branchId = branchOfSet.get(power.powerSet);
    if (branchId) taken.add(branchId);
  }

  if (taken.size === 0) return null;
  if (taken.size > 1) {
    warnings.push({
      power: build.archetype.name || String(archetypeId ?? 'archetype'),
      slot: 0,
      detail: `this build holds powers from ${taken.size} branches (${[...taken].join(', ')}), `
        + 'so the file names its base sets and Mids will not show it as either',
    });
    return null;
  }
  return branches[[...taken][0] as ArchetypeBranchId] ?? null;
}

/**
 * Mids' spelling of `ourInternalName`, or ours where the two agree (DATA-GAP MBDEXPORT-3).
 *
 * The rotation the importer undoes, redone. HC and Rebirth have moved internal names
 * underneath stable display names, so the name this planner holds is one Mids may have no
 * record of — 82 such names on Homecoming, 54 on Thunderspy, 32 on Rebirth. Mids answers a
 * `PowerName` it cannot resolve with a blank row that still holds the power's slots, so an
 * unrotated name costs the power AND every enhancement in it, silently, in a file that
 * looks complete from this side.
 *
 * Whatever comes back is written through byte for byte. `PiDFromUidPower` compares with
 * `==`, so Mids' case and its stray whitespace (`Epic.Martial_Mastery.Shukuchi `) are part
 * of the name; normalising them here would re-open the defect in a tidier spelling.
 */
function midsPowerSegment(powersetKey: string, ourInternalName: string): string {
  if (!powersetKey) return ourInternalName;
  return midsNameForExport(powersetKey, ourInternalName) ?? ourInternalName;
}

/** `Pool.Flight.Fly` with its power segment put into Mids' namespace. */
function rotateFullName(fullName: string, powersetKey: string): string {
  const segments = fullName.split('.');
  if (segments.length < 3) return fullName;
  const tail = segments.slice(2).join('.');
  return `${segments[0]}.${segments[1]}.${midsPowerSegment(powersetKey, tail)}`;
}

/**
 * This power's own last segment, in OUR namespace.
 *
 * `fullName` first — a stored power carries it, and it is the segment the export itself
 * uses — then the set definition's copy, because the .skif writer prunes `fullName` off a
 * stored power, and `internalName` last.
 */
function ourPowerSegment(
  power: { name: string; internalName?: string; fullName?: string },
  powersetId: string,
  category: SetCategory,
): string {
  const def = category === 'pool'
    ? getPowerPool(powersetId)?.powers.find((p) => p.internalName === power.internalName)
    : category === 'epic'
      ? getEpicPool(powersetId)?.powers.find((p) => p.internalName === power.internalName)
      : undefined;
  const fullName = power.fullName ?? def?.fullName;
  const segments = fullName?.split('.') ?? [];
  if (segments.length >= 3) return segments.slice(2).join('.');
  return power.internalName || power.name.replace(/\s+/g, '_');
}

/**
 * The full Mids `PowerName` for a power: `set.path` + this power's name in Mids' spelling.
 *
 * Both halves are lookups now and neither is a transform. A pool or epic `fullName` is
 * OURS, not Mids', and the two agree on most names — which is why it read as
 * already-Mids-shaped, and why `Pool.Force_of_Will` against a title-cased
 * `Pool.Force_Of_Will` stayed invisible.
 */
function buildPowerName(
  power: { name: string; internalName?: string; fullName?: string },
  powersetId: string,
  set: MidsSet,
  category: SetCategory,
): string {
  const midsName = midsPowerSegment(set.ourKey, ourPowerSegment(power, powersetId, category));
  return set.path ? `${set.path}.${midsName}` : midsName;
}

/**
 * Whether this power is a form sub-power Mids files under `Inherent.Inherent`
 * (DATA-GAP MBDEXPORT-5).
 *
 * A Kheldian's Nova and Dwarf attacks live in the form's powerset here — they are attached
 * to the parent form on the way in, and the finished build carries them there — but Mids
 * both writes and expects `Inherent.Inherent.Dark_Nova_Blast`. The importer already knows
 * this: `slottableSubPowerParent` reads exactly that prefix, and the two halves disagreed
 * about where a form power lives.
 *
 * The condition is read off `GRANTED_POWER_GROUPS`, not off a list of Kheldian names —
 * `slottable` is the flag that means "granted, and carries slots of its own", and it is
 * the same flag the reader keys on. A power that merely shares an internal name with one
 * is not caught, because being auto-granted is half the test.
 */
function isFormSubPower(power: SelectedPower): boolean {
  if (!power.isAutoGranted) return false;
  const internalName = (power.internalName || power.name).toLowerCase();
  return Object.values(GRANTED_POWER_GROUPS).some((group) => group.slottable
    && group.grantedPowers.some((granted) => granted.toLowerCase() === internalName));
}

/** The set a chosen power is written under, branch sets included. See `owningPowerset`. */
function setForPower(
  power: SelectedPower,
  powersetId: string,
  archetypeId: string,
  category: SetCategory,
  fallback: MidsSet,
  resolve: (ourKey: string, label: string) => MidsSet,
): { powersetId: string; set: MidsSet } {
  if (isFormSubPower(power)) {
    // One label for however many parents a fork's forms have — the memo keys on the set,
    // so a per-parent label would name whichever form happened to be collected first.
    return { powersetId, set: resolve('Inherent.Inherent', 'auto-granted form powers') };
  }
  if (category === 'pool' || category === 'epic') return { powersetId, set: fallback };
  const owner = owningPowerset(power, powersetId, archetypeId);
  if (owner.id === powersetId) return { powersetId, set: fallback };
  return {
    powersetId: owner.id,
    set: resolve(ourPowersetKey(owner.id, category), owner.set?.name || owner.id),
  };
}

// ============================================
// ENHANCEMENT UID CONSTRUCTION
// ============================================

/**
 * Build a Mids `RelativeLevel` (`eEnhRelative`) from a signed level offset.
 *
 * The negative half matters for the round-trip: Mids can express a -3 SO, so
 * exporting one as "Even" would launder an under-level build into a fresh one
 * on the way out the same way the importer used to on the way in. Mids' enum
 * bottoms out at MinusThree and tops out at PlusFive, so anything beyond that
 * clamps to the nearest end rather than silently becoming even.
 */
const RELATIVE_LEVEL_NAMES: Record<number, string> = {
  [-3]: 'MinusThree',
  [-2]: 'MinusTwo',
  [-1]: 'MinusOne',
  0: 'Even',
  1: 'PlusOne',
  2: 'PlusTwo',
  3: 'PlusThree',
  4: 'PlusFour',
  5: 'PlusFive',
};

function buildRelativeLevel(boost?: number): string {
  if (!boost || !Number.isFinite(boost)) return 'Even';
  const clamped = Math.min(5, Math.max(-3, Math.trunc(boost)));
  return RELATIVE_LEVEL_NAMES[clamped] ?? 'Even';
}

/**
 * An enhancement Mids has no UID for.
 *
 * Mids fails silently on an unknown UID — `GetEnhancementByUIDName` returns -1
 * and `LoadEnhancementData` leaves the slot empty — so an export that guesses
 * hands the user a build with holes in it and no way to know why. We refuse to
 * guess and report the hole instead.
 */
/**
 * One thing in the build that could not be written under a name Mids will resolve.
 *
 * Mids answers an unknown name with a blank row that keeps the slots, so an unreported
 * one is a build the user gets back with holes and no explanation.
 */
export interface MidsExportWarning {
  /** The power, or the powerset, this is about — whatever the user sees it called. */
  power: string;
  /** 1-based slot, or 0 when the subject is the power or powerset itself. */
  slot: number;
  detail: string;
}

/** Build Mids enhancement UID and metadata from an app Enhancement */
function buildEnhancement(enh: Enhancement): MbdEnhancement | null {
  switch (enh.type) {
    case 'io-set':
      return buildIOSetEnhancement(enh);
    case 'io-generic':
      return buildGenericIOEnhancement(enh);
    case 'origin':
      return buildOriginEnhancement(enh);
    case 'special':
      return buildSpecialEnhancement(enh);
    default:
      return null;
  }
}

/** Describe an enhancement for a warning message. */
function describeEnhancement(enh: Enhancement): string {
  switch (enh.type) {
    case 'io-set':
      return `${getIOSet(enh.setId)?.name ?? enh.setId} piece ${enh.pieceNum}`;
    case 'io-generic':
      return `${enh.stat} IO`;
    case 'origin':
      return `${enh.tier} ${enh.stat}`;
    case 'special':
      return enh.id;
    default:
      return 'enhancement';
  }
}

function buildIOSetEnhancement(enh: IOSetEnhancement): MbdEnhancement | null {
  const uid = getMidsIOSetPieceUid(enh.setId, enh.pieceNum);
  if (!uid) return null;

  // Attuned IOs scale with level and don't have a fixed IoLevel — use 0.
  // Non-attuned IOs use their fixed level (0-based).
  const ioLevel = enh.attuned ? 0 : Math.max(0, (enh.level ?? 50) - 1);

  return {
    Uid: uid,
    Grade: 'None',
    IoLevel: ioLevel,
    RelativeLevel: buildRelativeLevel(enh.boost),
    Obtained: false,
  };
}

function buildGenericIOEnhancement(enh: GenericIOEnhancement): MbdEnhancement | null {
  const uid = midsGenericIOSuffix(enh.stat);
  if (!uid) return null;
  return {
    Uid: uid,
    Grade: 'None',
    IoLevel: Math.max(0, (enh.level ?? 50) - 1),
    RelativeLevel: buildRelativeLevel(enh.boost),
    Obtained: false,
  };
}

/**
 * `Grade` as Mids spells it, keyed by our tier — `MIDS_ORIGIN_TIER` read the
 * other way round, so the two directions cannot drift apart again.
 *
 * They had, and it was the worst failure this exporter has had. `Grade` used to
 * be written as `enh.tier`, i.e. our own `SO`/`DO`/`TO`. Mids parses that field
 * with `Enum.Parse` against `eEnhGrade`, whose members are `TrainingO`, `DualO`
 * and `SingleO`; `SO` throws inside `LoadEnhancementData`, inside `LoadBuild`,
 * and Mids answers with "Requested value 'SO' was not found" and an empty
 * default character. Three origin pieces in a build of 86 enhancements cost the
 * user all 86 and every power holding them. See DATA-GAP MBDEXPORT-4.
 */
const MIDS_GRADE_BY_TIER: Record<string, string> = Object.fromEntries(
  Object.entries(MIDS_ORIGIN_TIER).map(([grade, tier]) => [tier, grade]),
);

/**
 * Origin (TO/DO/SO) enhancements. The stat half is the suffix the crafted IOs
 * use; `getMidsOriginUid` turns it into the record Mids actually carries, and
 * `Grade` supplies the tier — in Mids' spelling, never ours.
 *
 * A tier with no Mids grade returns null, which the caller turns into a warning
 * and an empty slot. That loses one enhancement; writing the token anyway loses
 * the entire build, so the empty slot is the conservative half of the trade.
 */
function buildOriginEnhancement(enh: OriginEnhancement): MbdEnhancement | null {
  const crafted = midsGenericIOSuffix(enh.stat);
  const uid = crafted && getMidsOriginUid(crafted);
  if (!uid) return null;
  const grade = MIDS_GRADE_BY_TIER[enh.tier];
  if (!grade) return null;
  return {
    Uid: uid,
    Grade: grade,
    IoLevel: 0,
    RelativeLevel: buildRelativeLevel(enh.boost),
    Obtained: false,
  };
}

/**
 * Reverse mapping from special enhancement registry ID → Mids UID suffix.
 * Built from the import code's SPECIAL_SUFFIX_MAPS (inverted).
 */
const REVERSE_SPECIAL_SUFFIX: Record<string, Record<string, string>> = {
  hamidon: {
    nucleolus: 'Damage_Accuracy', centriole: 'Damage_Range',
    enzyme: 'DeBuff_Endurance_Discount', lysosome: 'DeBuff_Accuracy',
    membrane: 'Buff_Recharge', peroxisome: 'Damage_Mez',
    ribosome: 'Res_Damage_Endurance_Discount', golgi: 'Heal_Endurance_Discount',
    endoplasm: 'Accuracy_Mez', cytoskeleton: 'Buff_Endurance_Discount',
    microfilament: 'Travel_Endurance_Discount', vesicle: 'Endurance_Modification_Recharge',
    stereocilia: 'Slow_Recharge_Endurance_Discount', microtubule: 'Endurance_Modification_Accuracy',
    karyoplasm: 'Damage_Endurance_Discount', microvillus: 'Accuracy_Range',
    chromatin: 'Damage_Recharge', ectosome: 'Threat_Accuracy_Recharge',
    amyloplast: 'Heal_Recharge', chloroplast: 'Heal_Accuracy',
  },
  titan: {
    amethyst: 'Damage_Mez', calcite: 'Accuracy_Mez',
    citrine: 'Buff_Recharge', diamond: 'Damage_Accuracy',
    gypsum: 'DeBuff_Accuracy', kyanite: 'Heal_Endurance_Discount',
    peridont: 'Res_Damage_Endurance_Discount', quartz: 'Damage_Range',
    selenite: 'Travel_Endurance_Discount', tanzanite: 'Buff_Endurance_Discount',
    zeolite: 'DeBuff_Endurance_Discount',
  },
  hydra: {
    antiproton: 'DeBuff_Endurance_Discount', delta: 'DeBuff_Accuracy',
    electron: 'Res_Damage_Endurance_Discount', gluon: 'Damage_Mez',
    graviton: 'Accuracy_Mez', neutrino: 'Damage_Accuracy',
    neutron: 'Damage_Range', positron: 'Heal_Endurance_Discount',
    proton: 'Buff_Endurance_Discount', quark: 'Buff_Recharge',
    theta: 'Travel_Endurance_Discount',
  },
  'd-sync': {
    acceleration: 'Travel_Endurance_Discount', binding: 'Accuracy_Mez',
    conduit: 'Endurance_Modification_Recharge', containment: 'Damage_Mez',
    deceleration: 'Slow_Recharge_Endurance_Discount', drain: 'Endurance_Modification_Accuracy',
    efficiency: 'Damage_Endurance_Discount', elusivity: 'Buff_Endurance_Discount',
    empowerment: 'Damage_Accuracy', extension: 'Damage_Range',
    fortification: 'Res_Damage_Endurance_Discount', guidance: 'Accuracy_Range',
    marginalization: 'DeBuff_Endurance_Discount', obfuscation: 'DeBuff_Accuracy',
    optimization: 'Damage_Recharge', provocation: 'Threat_Accuracy_Recharge',
    reconstitution: 'Heal_Endurance_Discount', reconstruction: 'Heal_Recharge',
    shifting: 'Buff_Recharge', siphon: 'Heal_Accuracy',
  },
};

function buildSpecialEnhancement(enh: SpecialEnhancement): MbdEnhancement | null {
  const prefixMap: Record<string, string> = {
    hamidon: 'Hamidon',
    titan: 'Titan',
    hydra: 'Hydra',
    'd-sync': 'DSync',
  };
  const prefix = prefixMap[enh.category] || 'Hamidon';

  // Extract registry ID from enhancement ID (e.g., "hamidon-enzyme" → "enzyme")
  const registryId = enh.id.replace(`${enh.category}-`, '');
  const suffixMap = REVERSE_SPECIAL_SUFFIX[enh.category];
  const suffix = suffixMap?.[registryId];

  if (!suffix) return null;

  // The suffix map is hand-inverted from the import side, so check the result
  // against the dataset's roster before shipping it — an unrecognised exotic
  // would otherwise leave an empty slot with no trace.
  const uid = getMidsSpecialUid(`${prefix}_${suffix}`);
  if (!uid) return null;

  return {
    Uid: uid,
    Grade: 'None',
    IoLevel: 0,
    RelativeLevel: buildRelativeLevel(enh.boost),
    Obtained: false,
  };
}

// ============================================
// SLOT ENTRIES
// ============================================

/**
 * `SlotEntry.Level` is the level the slot was PLACED, not the level of the
 * power holding it. Stamping every slot with the power's level made a
 * six-slotted level-2 power claim six slots at level 2, which Mids draws (with
 * "Slot Levels: On") as an illegal build.
 *
 * `computeAllSlotLevels` is the same solver the print and forum exports use, so
 * all three agree on what the build says.
 */
function buildSlotEntries(
  power: SelectedPower,
  levels: SlotLevel[] | undefined,
  inherentSlots: number,
  warnings: MidsExportWarning[],
): MbdSlotEntry[] {
  return power.slots.map((slot, index) => {
    const enhancement = slot ? buildEnhancement(slot) : null;
    if (slot && !enhancement) {
      warnings.push({
        power: power.name,
        slot: index + 1,
        detail: `${describeEnhancement(slot)} — Mids has no enhancement by that name`,
      });
    }
    return {
      Level: levels?.[index] ?? power.level,
      IsInherent: index > 0 && index <= inherentSlots,
      Enhancement: enhancement,
      FlippedEnhancement: null,
    };
  });
}

/**
 * One .mbd power entry, with its slots resolved.
 *
 * `StatInclude` is the author's include flag and it maps to `isActive` exactly, so only `true`
 * may write `true`. The importer stores the excluded case as `undefined` rather than `false`
 * (the calc gate reads `isAuto || isActive`, so a third state buys nothing and the JSON stays
 * minimal) — and `!== false`, which this used to test, let every one of those back in.
 *
 * The other candidate was to write what the calc gate answers, `isAuto || isActive === true`.
 * The corpus refuses it: Mids writes the four Fitness autos `true` in all five Homecoming files
 * and `false` in all three Rebirth ones, and Rebirth's Swift plainly still grants run speed. The
 * flag is not Mids' record of what contributes — auto powers contribute either way — so folding
 * `isAuto` in would state something the field does not carry, and would overwrite the author's
 * flag on every power Mids wrote `false`.
 *
 * Ported from canonical (MBDEXPORT-13). The fixtures that measured it are canonical-only, so the
 * argument above is the record here.
 */
function buildPowerEntry(
  power: SelectedPower,
  powerName: string,
  category: PowerCategory,
  slotLevels: Map<string, SlotLevel[]>,
  targetsHitValues: Record<string, number>,
  warnings: MidsExportWarning[],
): MbdPowerEntry {
  const inherentSlots = power.inherentSlotCount ?? 0;
  const levels = slotLevels.get(powerKey(category, power.internalName || power.name));
  return {
    PowerName: powerName,
    Level: power.level,
    StatInclude: power.isActive === true,
    ProcInclude: false,
    VariableValue: targetsHitValues[power.internalName] ?? 0,
    InherentSlotsUsed: inherentSlots,
    SubPowerEntries: [],
    SlotEntries: buildSlotEntries(power, levels, inherentSlots, warnings),
  };
}

// ============================================
// MAIN EXPORT FUNCTION
// ============================================

/**
 * The seven powers Mids files under `eGridType.Inherent`, in the order its
 * scratch array indexes them.
 *
 * Order is not cosmetic here — `SortGridPowers` writes each of these to a fixed
 * array position while sizing the array by how many of them the file carries, so
 * a file that omits any of them can crash the load. See the comment at the export
 * site.
 */
const MIDS_INHERENT_GRID = ['Brawl', 'Sprint', 'Rest', 'Swift', 'Hurdle', 'Health', 'Stamina'] as const;

/**
 * The level-up pick schedule expanded to one entry per pick — HC's level 1
 * grants two, so it appears twice.
 *
 * This is the shape `PowerEntries` has to take. Mids does not look a power up
 * by name and file it: `LoadBuild` walks the array by index and assigns
 * `CurrentBuild.Powers[powerIndex] = powerEntry`, so position i *is* the i-th
 * level-up pick. Grouping the array by powerset — primary, then secondary,
 * then pools — put Hasten in the level-28 slot and left the grid reading in an
 * order nobody picked.
 */
function pickSchedule(): number[] {
  const slots: number[] = [];
  for (const level of POWER_PICK_LEVELS) {
    for (let i = 0; i < getPicksGrantedAtLevel(level); i++) slots.push(level);
  }
  return slots;
}

/** An unfilled pick. Mids writes one of these for a skipped slot and reads it back as `NIDPower < 0`.
 *  `VariableValue` is 0 because there is no power here to carry a slider — not the MBDEXPORT-19
 *  literal, which was the same zero standing in for one. */
function blankPowerEntry(): MbdPowerEntry {
  return {
    PowerName: '',
    Level: 0,
    StatInclude: false,
    ProcInclude: false,
    VariableValue: 0,
    InherentSlotsUsed: 0,
    SubPowerEntries: [],
    SlotEntries: [],
  };
}

/**
 * Lay the chosen powers out along the pick schedule, blanking the slots the
 * build skipped.
 *
 * Padding matters as much as ordering: the inherents and incarnates that follow
 * are addressed by index too, so a build that skipped a pick would slide them
 * one slot forward into a level-up position.
 */
function orderByPickSchedule(chosen: { level: number; entry: MbdPowerEntry }[]): MbdPowerEntry[] {
  const sorted = [...chosen].sort((a, b) => a.level - b.level);
  const schedule = pickSchedule();
  const entries: MbdPowerEntry[] = [];

  let slot = 0;
  for (const power of sorted) {
    while (slot < schedule.length && schedule[slot] < power.level) {
      entries.push(blankPowerEntry());
      slot++;
    }
    entries.push(power.entry);
    slot++;
  }
  // Pad out the remaining picks so the run of level-up slots is the length Mids
  // expects before anything auto-granted starts.
  while (slot < schedule.length) {
    entries.push(blankPowerEntry());
    slot++;
  }
  return entries;
}

/**
 * Export a Sidekick Build to Mids Reborn .mbd JSON format.
 *
 * Returns the JSON alongside every enhancement that could not be named, so the
 * caller can say what the file is missing. Mids drops an unresolvable slot in
 * silence, so an unreported warning here is a build the user gets back with
 * holes and no explanation.
 *
 * `targetsHitValues` is Mids' per-power `VariableValue` — Siphon Speed's stack count, Follow
 * Up's, the number of corpses a Mire is standing in — keyed by `internalName`. It is passed in
 * rather than read off the build because the build does not hold it: the importer returns it
 * beside the build and the UI store owns it from there, which is the same key `InfoPanel`
 * writes the slider under. A caller with no slider state passes nothing and every power goes
 * out at 0, which is what an unset slider means (MBDEXPORT-19). Hand-ported from canonical;
 * the fixtures that measured it are canonical-only, so the argument above is the record here.
 */
export function exportToMidsWithReport(
  build: Build,
  levelUpMode: boolean,
  targetsHitValues: Record<string, number> = {},
): { json: string; warnings: MidsExportWarning[] } {
  const archetypeId = build.archetype.id || '';
  const warnings: MidsExportWarning[] = [];
  // The two file-level claims first, so a report that opens with "Mids has no Thunderspy
  // database" reads in the order the reader needs it: the fork, then the archetype, then the
  // sets and pieces that could not be named because of them.
  const databaseLabel = midsDatabaseForBuild(build, warnings);
  const midsClass = midsClassForBuild(build, warnings);
  // Outside Level Up mode a slot carries no real level (SLOT-3). Mids' own
  // .mbd format requires a Level per slot regardless, so this is a synthetic,
  // schedule-legal placement — not a claim about the build's actual leveling
  // history. The caller is expected to say so once in the UI, not per slot.
  const slotLevels = computeExportSlotLevels(build, levelUpMode);

  // Build PowerSets array: always 8 entries
  // [0]=primary, [1]=secondary, [2]="" (reserved), [3-6]=pools, [7]=epic
  // Each set resolved ONCE, so the header array and every `PowerName` under it are the
  // same string and an unpaired set is reported once rather than once per power.
  const blankSet: MidsSet = { ourKey: '', path: '' };
  const resolve = setResolver(warnings);
  const resolveSet = (id: string, category: SetCategory) =>
    resolve(ourPowersetKey(id, category), powersetLabel(id, category));

  const primary = build.primary.id ? resolveSet(build.primary.id, 'primary') : blankSet;
  const secondary = build.secondary.id ? resolveSet(build.secondary.id, 'secondary') : blankSet;
  const pools = build.pools.map((pool) => resolveSet(pool.id, 'pool'));
  const epic = build.epicPool ? resolveSet(build.epicPool.id, 'epic') : blankSet;

  const poolPaths = pools.map((pool) => pool.path);
  // Pad to exactly 4 pool slots
  while (poolPaths.length < 4) poolPaths.push('');

  // The two role headers name the BRANCH set once the build has specialised into one, and the
  // picks underneath keep their own — which is how Mids' own file spells it, and the whole of
  // what a VEAT's round trip used to lose (MBDEXPORT-16). Only the header moves: `collect`
  // below still hands each role its base set, so a base pick goes on writing the base path.
  const branch = branchTaken(build, warnings);
  const primaryHeader = branch?.primarySet ? resolveSet(branch.primarySet, 'primary') : primary;
  const secondaryHeader = branch?.secondarySet
    ? resolveSet(branch.secondarySet, 'secondary')
    : secondary;

  const powerSets = [primaryHeader.path, secondaryHeader.path, '', ...poolPaths, epic.path];

  // Collect the chosen powers, then lay them out along the pick schedule —
  // Mids reads this array positionally, so grouping by powerset scrambles the
  // grid even though every Level field is right.
  const chosen: { level: number; entry: MbdPowerEntry }[] = [];
  // Form sub-powers do not take a pick, and the file has to say so positionally: Mids
  // reads everything past `LastPower` as auto-granted, and its own Warshade file puts the
  // ten Nova and Dwarf attacks down there with the inherents (MBDEXPORT-5). Leaving them
  // in the level-up run costs ten picks and shifts every power after them, which is a
  // second, quieter way to hand back a build the author did not write.
  const granted: MbdPowerEntry[] = [];
  const collect = (
    powers: SelectedPower[],
    powersetId: string,
    set: MidsSet,
    category: SetCategory,
  ) => {
    for (const power of powers) {
      const owner = setForPower(power, powersetId, archetypeId, category, set, resolve);
      const powerName = buildPowerName(power, owner.powersetId, owner.set, category);
      const entry = buildPowerEntry(power, powerName, category, slotLevels, targetsHitValues, warnings);
      if (isFormSubPower(power)) granted.push(entry);
      else chosen.push({ level: power.level, entry });
    }
  };

  collect(build.primary.powers, build.primary.id || '', primary, 'primary');
  collect(build.secondary.powers, build.secondary.id || '', secondary, 'secondary');
  build.pools.forEach((pool, i) => collect(pool.powers, pool.id, pools[i], 'pool'));
  if (build.epicPool) collect(build.epicPool.powers, build.epicPool.id, epic, 'epic');

  const powerEntries: MbdPowerEntry[] = orderByPickSchedule(chosen);

  // `LastPower` is the COUNT of level-up slots, not the index of the last one —
  // Mids reads entry `LastPower` itself as the first auto-granted power. This was
  // written as `length - 1` under the other reading, which handed the level-49 pick
  // to the granted run: it came back at level 0, one pick short, in all eight corpus
  // files (MBDEXPORT-11). Mids' own files settle it — every one of them puts an
  // `Inherent.*` at index `LastPower` and a real pick at `LastPower - 1`, and there
  // is no separator entry between the two. Inherents and incarnates therefore follow
  // the level-up run, and the count has to be taken before they are added.
  const lastPower = powerEntries.length;

  // Inherents. Mids re-creates the roster itself, but only the build file
  // carries what the user slotted into them — and that is where a build keeps
  // Miracle, Panacea, Performance Shifter and Numina's.
  //
  // All seven go, always, even the empty ones. `CharacterBuildData.SortGridPowers`
  // sizes its scratch array by how many `eGridType.Inherent` powers the FILE
  // carries, then writes each one to a FIXED index — Brawl 0, Sprint 1, Rest 2,
  // Swift 3, Hurdle 4, Health 5, Stamina 6. Send only the four Fitness powers and
  // the array has four slots: Swift lands at [3] and Hurdle throws
  // IndexOutOfRange. That exception aborts `LoadBuild` wholesale, so the inherent
  // grid is never built and `Validate()` never runs — which reads, on screen, as
  // inherents that are present but have lost every slot.
  for (const name of MIDS_INHERENT_GRID) {
    const power = build.inherents.find((p) => (p.internalName || p.name) === name || p.name === name);
    const fullName = power ? inherentFullName(power, archetypeId) : null;
    if (!power || !fullName) {
      // A fork without this power still has to hold the index open.
      powerEntries.push({ ...blankPowerEntry(), PowerName: `Inherent.Inherent.${name}`, Level: 1 });
      continue;
    }
    powerEntries.push(buildPowerEntry(power, fullName, 'inherent', slotLevels, targetsHitValues, warnings));
  }

  // The archetype inherent, which is not one of the seven above: `SortGridPowers`
  // indexes those by position, and this one Mids addresses by name — its own files
  // put it in the middle of that run, and moving it makes no difference. It went
  // unwritten entirely until MBDEXPORT-20, and a build handed over without it is a
  // build Mids totals differently than we do: Defiance's damage floor, Supremacy's
  // pet buff, Dark Sustenance's mez protection. It carries the slider too, which is
  // how the gap surfaced — MBDEXPORT-19's grade had one survivor it could not
  // explain, because a census that diffs a FIELD cannot see a missing ROW.
  //
  // Mids' spelling is the export's own `Inherent.Inherent` row, never the
  // `Inherent.<Archetype>.<Name>` the roster synthesises: the export files Fury
  // under `Rage_Buff` and gives both Arachnos archetypes a `Conditioning`, so
  // neither the declared name nor an archetype prefix reaches the right row alone.
  // The name comes off the generated headline map rather than canonical's
  // `Inherent.Inherent` powerset, which this repo does not carry — the two are
  // graded against each other over all 60 archetype-fork pairs.
  //
  // The registry is the authority and the build's copy is a cache — same order
  // `midsClassForBuild` takes, and for the same reason: a stored build carries
  // whatever its fork said when it was saved.
  const declaredInherent = (archetypeId ? getArchetype(archetypeId)?.inherent?.name : undefined)
    ?? build.archetype.inherent?.name;
  const atInherent = declaredInherent
    ? build.inherents.find((p) => p.name === declaredInherent)
    : undefined;
  const atInherentName = archetypeId ? headlineArchetypeInherentName(archetypeId) : undefined;
  if (atInherent && atInherentName) {
    const segments = atInherentName.split('.');
    powerEntries.push(buildPowerEntry(
      atInherent,
      rotateFullName(atInherentName, `${segments[0]}.${segments[1]}`),
      'inherent',
      slotLevels,
      targetsHitValues,
      warnings,
    ));
  } else if (declaredInherent) {
    // The archetype declares an inherent and we cannot name the power behind it, so
    // the file goes out a power short. Said, not swallowed: the synthesised name
    // would resolve to nothing in Mids and read as a silent drop either way.
    warnings.push({
      power: declaredInherent,
      slot: 0,
      detail: atInherent
        ? `no Inherent.Inherent power this fork ships answers to ${declaredInherent} for this archetype, `
          + 'so the file carries no archetype inherent and Mids totals the build without it'
        : `this build carries no ${declaredInherent} row to write, so the file goes out without it`,
    });
  }

  // The form sub-powers, after the inherent grid and in the order they were collected —
  // which is Mids' own order in the file this was measured against.
  for (const entry of granted) powerEntries.push(entry);

  // Incarnates (DATA-GAP MBDEXPORT-7). `powerName` is our own slug — the last segment of
  // the roster's `fullName`, lower-cased — and a comment here used to claim it was already
  // Mids' `Incarnate.<Slot>.<Power>`. It has no dots, so Mids resolved nothing and the row
  // came back blank; the reason nobody noticed is that an incarnate carries no enhancement
  // slots, so a hunt that counted lost enhancements walked straight past it.
  //
  // The roster is where the three-segment name lives, and it goes out through the same two
  // lookups every other power does: the set path, and the power's own name in Mids'
  // spelling. `Incarnate.Alpha` and its 102 siblings are in the path table already.
  for (const slot of INCARNATE_SLOT_ORDER) {
    const chosen = build.incarnates?.[slot];
    if (!chosen?.powerName) continue;
    powerEntries.push({
      PowerName: incarnateFullName(chosen, slot, resolve, warnings),
      Level: 50,
      StatInclude: true,
      ProcInclude: false,
      // An incarnate is not addressed by `internalName` and our reader routes no slider to
      // one, so there is nothing to look up here. Mids writes none either — the MBDEXPORT-19
      // census diffs every entry by name and no incarnate is among the ten.
      VariableValue: 0,
      InherentSlotsUsed: 0,
      SubPowerEntries: [],
      SlotEntries: [],
    });
  }

  // Accolades (DATA-GAP MBDEXPORT-12), after the granted run for the same reason the
  // incarnates are: everything past `LastPower` is addressed by name, not by index.
  for (const entry of accoladeEntries(build, resolve, warnings)) powerEntries.push(entry);

  const mbdFile: MbdFile = {
    BuiltWith: {
      App: 'CoH Planner',
      Version: '1.0',
      Database: databaseLabel,
      DatabaseVersion: '27.2025.1127.1',
    },
    Level: String(build.level - 1), // 0-based
    Class: midsClass,
    Origin: build.settings?.origin || 'Science',
    Alignment: 'Hero',
    Name: build.name || 'Unnamed Build',
    Comment: '',
    PowerSets: powerSets,
    LastPower: lastPower,
    PowerEntries: powerEntries,
  };

  return { json: JSON.stringify(mbdFile, null, 2), warnings };
}

/**
 * Mids' `Incarnate.<Slot>.<Power>` for a chosen incarnate (DATA-GAP MBDEXPORT-7).
 *
 * `powerName` holds two different things depending on who wrote the build, which is how a
 * comment here claiming it was "already Mids' own" managed to be half true for months: the
 * `.skif` reader stores the full `Incarnate.Judgement.Mighty_Radial_Final_Judgement`, and
 * the `.mbd` importer stores `power.id` — `musculature_radial_paragon`, a slug with no dots
 * and nothing for Mids to resolve.
 *
 * A slug cannot be un-slugged: it does not say where its underscores were capitals. So the
 * roster answers first, `powerName` second where it is already a path, and an incarnate
 * neither can name is reported rather than written as a bare word.
 */
function incarnateFullName(
  chosen: SelectedIncarnatePower,
  slot: IncarnateSlotId,
  resolve: (ourKey: string, label: string) => MidsSet,
  warnings: MidsExportWarning[],
): string {
  const fromRoster = getIncarnatePower(slot, chosen.powerId || chosen.powerName)?.fullName;
  const fullName = fromRoster ?? (chosen.powerName.split('.').length >= 3 ? chosen.powerName : '');
  const segments = fullName.split('.');
  if (segments.length < 3) {
    warnings.push({
      power: chosen.displayName || chosen.powerName,
      slot: 0,
      detail: `no ${slot} power of that name in the roster, so Mids gets ${chosen.powerName}`,
    });
    return chosen.powerName;
  }
  const set = resolve(`${segments[0]}.${segments[1]}`, `${chosen.displayName} (${slot})`);
  const name = midsPowerSegment(set.ourKey, segments.slice(2).join('.'));
  return set.path ? `${set.path}.${name}` : name;
}

/**
 * The `Temporary_Powers.Accolades.*` entries for the accolades this build holds
 * (DATA-GAP MBDEXPORT-12).
 *
 * MBDIMPORT-1 is this hole on the reader's side: a blanket `Temporary_Powers.` skip took the
 * accolades out with the day-job temps, and a build lost The Atlas Medallion, Task Force
 * Commander, Portal Jockey and Freedom Phalanx Reserve — their +MaxHP and +MaxEnd with them —
 * while the summary reported nothing missing. The writer had the mirror of it: the roster was
 * read on the way in and never written on the way out, so a build that arrived with four left
 * with none.
 *
 * The name goes out through the same two lookups every other power name does — the set path,
 * then the power's own name in Mids' spelling — because a path COMPOSED rather than looked up
 * is MBDEXPORT-6's defect, and `Temporary_Powers.Accolades` is in the path table already.
 * Composing it here would be right on all four forks today and unguarded the day one spells it
 * differently.
 *
 * `StatInclude` is the mapping, not presence, and it is `true` for everything written: Mids
 * keeps "owned" and "counted" apart, the planner carries only the counted state, and the
 * reader on both sides reads the flag as that state. An accolade the planner does not hold is
 * simply not written, which is the same statement.
 *
 * `Level` is not information about the build. Mids' own corpus writes 50 on two files and 1 on
 * a third at the same character level, and neither reader looks at it for an accolade — so the
 * character's level goes out, as the honest reading of a permanent buff this character holds.
 *
 * The count reconciles the way the import's does (MBDIMPORT-5): every id in the build's roster
 * is either written or named in a warning. An accolade dropped in silence is the failure this
 * row exists to end, so the miss is REPORTED rather than skipped — a stored id with no power
 * behind it is a roster divergence, the same fact the reader's third arm surfaces.
 */
function accoladeEntries(
  build: Build,
  resolve: (ourKey: string, label: string) => MidsSet,
  warnings: MidsExportWarning[],
): MbdPowerEntry[] {
  const entries: MbdPowerEntry[] = [];
  for (const id of build.accolades ?? []) {
    const power = getAccolade(id);
    const segments = (power?.fullName ?? '').split('.');
    if (!power || segments.length < 3) {
      warnings.push({
        power: power?.name || id,
        slot: 0,
        detail: power
          ? `the accolade roster carries no full name for ${id}, so Mids gets no entry for it`
          : `no accolade in this dataset answers to ${id}, so it is not written`,
      });
      continue;
    }
    const set = resolve(`${segments[0]}.${segments[1]}`, power.name);
    const name = midsPowerSegment(set.ourKey, segments.slice(2).join('.'));
    entries.push({
      PowerName: set.path ? `${set.path}.${name}` : name,
      Level: build.level,
      StatInclude: true,
      ProcInclude: false,
      // Accolades come in through `processAccoladeEntry`, which reads no slider, so writing
      // one here would be inventing a value the reader cannot return. Symmetric and measured:
      // no accolade is among MBDEXPORT-19's ten.
      VariableValue: 0,
      InherentSlotsUsed: 0,
      SubPowerEntries: [],
      SlotEntries: [],
    });
  }
  return entries;
}

/**
 * Mids' full name for an inherent (`Inherent.Fitness.Stamina`).
 *
 * A stored inherent may not carry `fullName` — the .skif writer prunes it — so
 * fall back to the dataset's own inherent rosters by name.
 */
function inherentFullName(power: SelectedPower, archetypeId: string): string | null {
  const stored = (power as { fullName?: string }).fullName;
  const full = stored?.startsWith('Inherent.')
    ? stored
    : [...getInherentPowers(), ...getArchetypeInherentPowers(archetypeId || undefined)]
      .find((def) => def.internalName === (power.internalName || power.name) || def.name === power.name)
      ?.fullName;
  if (!full) return null;
  // Inherents rotate too — Thunderspy carries a row under `inherent.*` — and this path
  // never went through `buildPowerName`, so the rewrite has to be repeated here.
  const segments = full.split('.');
  return rotateFullName(full, segments.length >= 2 ? `${segments[0]}.${segments[1]}` : '');
}

/**
 * Export a Sidekick Build to Mids Reborn .mbd JSON format.
 * Returns the JSON string ready to save as a .mbd file.
 */
export function exportToMids(
  build: Build,
  levelUpMode: boolean,
  targetsHitValues: Record<string, number> = {},
): string {
  return exportToMidsWithReport(build, levelUpMode, targetsHitValues).json;
}
