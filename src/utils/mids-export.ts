/**
 * Export a Sidekick build to Mids Reborn .mbd (JSON) format.
 * This is the reverse of src/utils/mids-import/.
 */

import type { Build, SelectedPower, Powerset, Enhancement, IOSetEnhancement, GenericIOEnhancement, SpecialEnhancement, OriginEnhancement } from '@/types';
import { INCARNATE_SLOT_ORDER } from '@/types';
import type { MbdFile, MbdPowerEntry, MbdSlotEntry, MbdEnhancement } from '@/utils/mids-import/types';
import { getPowerset, getPowersetsForArchetype } from '@/data/powersets';
import { getPowerPool } from '@/data/power-pools';
import { getEpicPool } from '@/data/epic-pools';
import { getIOSet } from '@/data/io-sets';
import { getMidsGenericIOUid, getMidsIOSetPieceUid, getMidsOriginUid, getMidsSpecialUid } from '@/data/mids-uids';
import {
  midsNameForExport,
  midsPowersetPathForExport,
  midsPowersetPathsKnown,
} from '@/data/mids-name-map';
import { MIDS_STAT_MAP, MIDS_ORIGIN_TIER } from '@/utils/mids-import/mappers';
import { getInherentPowers, getArchetypeInherentPowers, POWER_PICK_LEVELS, getPicksGrantedAtLevel } from '@/data';
import { computeExportSlotLevels, type SlotLevel } from '@/utils/slot-levels';
import { powerKey, type PowerCategory } from '@/utils/power-key';

// ============================================
// REVERSE ARCHETYPE MAP (app ID → Mids Class_*)
// ============================================

const REVERSE_ARCHETYPE_MAP: Record<string, string> = {
  blaster: 'Class_Blaster',
  brute: 'Class_Brute',
  controller: 'Class_Controller',
  corruptor: 'Class_Corruptor',
  defender: 'Class_Defender',
  dominator: 'Class_Dominator',
  mastermind: 'Class_Mastermind',
  scrapper: 'Class_Scrapper',
  sentinel: 'Class_Sentinel',
  stalker: 'Class_Stalker',
  tanker: 'Class_Tanker',
  peacebringer: 'Class_Peacebringer',
  warshade: 'Class_Warshade',
  'arachnos-soldier': 'Class_Arachnos_Soldier',
  'arachnos-widow': 'Class_Arachnos_Widow',
};

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

/** The set a chosen power is written under, branch sets included. See `owningPowerset`. */
function setForPower(
  power: SelectedPower,
  powersetId: string,
  archetypeId: string,
  category: SetCategory,
  fallback: MidsSet,
  resolve: (ourKey: string, label: string) => MidsSet,
): { powersetId: string; set: MidsSet } {
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

/** One .mbd power entry, with its slots resolved. */
function buildPowerEntry(
  power: SelectedPower,
  powerName: string,
  category: PowerCategory,
  slotLevels: Map<string, SlotLevel[]>,
  warnings: MidsExportWarning[],
): MbdPowerEntry {
  const inherentSlots = power.inherentSlotCount ?? 0;
  const levels = slotLevels.get(powerKey(category, power.internalName || power.name));
  return {
    PowerName: powerName,
    Level: power.level,
    StatInclude: power.isActive !== false,
    ProcInclude: false,
    VariableValue: 0,
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

/** An unfilled pick. Mids writes one of these for a skipped slot and reads it back as `NIDPower < 0`. */
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
 */
export function exportToMidsWithReport(
  build: Build,
  levelUpMode: boolean,
): { json: string; warnings: MidsExportWarning[] } {
  const archetypeId = build.archetype.id || '';
  const midsClass = REVERSE_ARCHETYPE_MAP[archetypeId] || 'Class_Blaster';
  const warnings: MidsExportWarning[] = [];
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

  const powerSets = [primary.path, secondary.path, '', ...poolPaths, epic.path];

  // Collect the chosen powers, then lay them out along the pick schedule —
  // Mids reads this array positionally, so grouping by powerset scrambles the
  // grid even though every Level field is right.
  const chosen: { level: number; entry: MbdPowerEntry }[] = [];
  const collect = (
    powers: SelectedPower[],
    powersetId: string,
    set: MidsSet,
    category: SetCategory,
  ) => {
    for (const power of powers) {
      const owner = setForPower(power, powersetId, archetypeId, category, set, resolve);
      const powerName = buildPowerName(power, owner.powersetId, owner.set, category);
      chosen.push({
        level: power.level,
        entry: buildPowerEntry(power, powerName, category, slotLevels, warnings),
      });
    }
  };

  collect(build.primary.powers, build.primary.id || '', primary, 'primary');
  collect(build.secondary.powers, build.secondary.id || '', secondary, 'secondary');
  build.pools.forEach((pool, i) => collect(pool.powers, pool.id, pools[i], 'pool'));
  if (build.epicPool) collect(build.epicPool.powers, build.epicPool.id, epic, 'epic');

  const powerEntries: MbdPowerEntry[] = orderByPickSchedule(chosen);

  // `LastPower` is the index of the last CHOSEN power; Mids reads everything
  // past it as auto-granted. Inherents and incarnates therefore have to follow
  // the level-up run, and the marker has to be taken before they are added.
  const lastPower = powerEntries.length - 1;

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
    powerEntries.push(buildPowerEntry(power, fullName, 'inherent', slotLevels, warnings));
  }

  // Incarnates. `powerName` is already Mids' own `Incarnate.<Slot>.<Power>`.
  for (const slot of INCARNATE_SLOT_ORDER) {
    const chosen = build.incarnates?.[slot];
    if (!chosen?.powerName) continue;
    powerEntries.push({
      PowerName: chosen.powerName,
      Level: 50,
      StatInclude: true,
      ProcInclude: false,
      VariableValue: 0,
      InherentSlotsUsed: 0,
      SubPowerEntries: [],
      SlotEntries: [],
    });
  }

  // Database string mirrors what Mids Reborn writes for each server, so
  // round-tripping between us and Mids preserves the dataset on import.
  const databaseLabel = build.serverId === 'rebirth' ? 'Rebirth' : 'Homecoming';
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
export function exportToMids(build: Build, levelUpMode: boolean): string {
  return exportToMidsWithReport(build, levelUpMode).json;
}
