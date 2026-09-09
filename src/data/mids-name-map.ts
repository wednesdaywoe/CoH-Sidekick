/**
 * Mids' internal name for a power → this dataset's, per powerset (DATA-GAP MBDIMPORT-2).
 *
 * A `.mbd` identifies a power by internal name alone, and that namespace has drifted from
 * the game's. HC has rotated internal names underneath stable display names: Tactical
 * Arrow's `Gymnastics` is Oil Slick Arrow in the export now, and the power the game shows
 * as "Gymnastics" is internally `Quickness`. Stalker Shield Defense is a three-cycle.
 *
 * That makes an exact internal-name match the *least* reliable matcher rather than the
 * most: the name exists, so nothing fails — it just resolves to a different power, takes
 * that power's slots, and the entry that rightfully owned them is deduped away in silence.
 *
 * The tables are DERIVED (`scripts/convert-mids-name-map.cjs`) by joining Mids' own power
 * list to the export on display name. They are not a curated list of known breakages, and
 * that matters: the four that a bug report surfaced are 8 of the 115 rows Homecoming
 * carries. Reading Mids for this is not a Rule 0 breach — the question is what MIDS calls
 * a power, and only Mids can answer it. [[derive-dont-invent]]
 */

import { getActiveDataset, type DatasetId } from './dataset';
import {
  MIDS_NAME_MAP as HOMECOMING_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as HOMECOMING_MIDS_SETS,
  MIDS_NAME_REVERSE as HOMECOMING_MIDS_REVERSE,
  MIDS_POWERSET_PATH as HOMECOMING_MIDS_PATHS,
} from './datasets/homecoming/generated/mids-name-map';
import {
  MIDS_NAME_MAP as REBIRTH_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as REBIRTH_MIDS_SETS,
  MIDS_NAME_REVERSE as REBIRTH_MIDS_REVERSE,
  MIDS_POWERSET_PATH as REBIRTH_MIDS_PATHS,
} from './datasets/rebirth/generated/mids-name-map';
import {
  MIDS_NAME_MAP as THUNDERSPY_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as THUNDERSPY_MIDS_SETS,
  MIDS_NAME_REVERSE as THUNDERSPY_MIDS_REVERSE,
  MIDS_POWERSET_PATH as THUNDERSPY_MIDS_PATHS,
} from './datasets/thunderspy/generated/mids-name-map';
import {
  MIDS_NAME_MAP as BRAINSTORM_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as BRAINSTORM_MIDS_SETS,
  MIDS_NAME_REVERSE as BRAINSTORM_MIDS_REVERSE,
  MIDS_POWERSET_PATH as BRAINSTORM_MIDS_PATHS,
} from './datasets/brainstorm/generated/mids-name-map';

type NameMap = Readonly<Record<string, Readonly<Record<string, string>>>>;
type SetAlias = Readonly<Record<string, string>>;

/**
 * One entry per dataset and no `default` arm, for the reason `accolades.ts` records: a
 * fall-through to Homecoming reads live's data on a fork that has its own, and reads it
 * silently. A Record typed on `DatasetId` cannot compile with a dataset missing.
 */
const MAP_BY_DATASET: Record<DatasetId, NameMap> = {
  homecoming: HOMECOMING_MIDS_NAMES,
  rebirth: REBIRTH_MIDS_NAMES,
  thunderspy: THUNDERSPY_MIDS_NAMES,
  brainstorm: BRAINSTORM_MIDS_NAMES,
};

/**
 * The same tables backwards — ours → Mids' — for the .mbd WRITER (MBDEXPORT-3).
 *
 * A second generated table rather than an inversion of the one above, because the forward
 * table is lossy in the direction the writer needs: it folds Mids' spelling to lower case
 * and trims it, and Mids resolves a `PowerName` by ordinal `==` against its own string.
 * Both halves are minted by one pass of `convert-mids-name-map.cjs` over one join, so
 * they cannot drift; `name-map.test.ts` holds them to being mutual inverses.
 */
const REVERSE_BY_DATASET: Record<DatasetId, NameMap> = {
  homecoming: HOMECOMING_MIDS_REVERSE,
  rebirth: REBIRTH_MIDS_REVERSE,
  thunderspy: THUNDERSPY_MIDS_REVERSE,
  brainstorm: BRAINSTORM_MIDS_REVERSE,
};

/**
 * Ours → Mids' literal `group.set`, for the .mbd WRITER (MBDEXPORT-3's sibling, MBDEXPORT-6).
 *
 * The path half of a `PowerName`. `MIDS_NAME_REVERSE` above answers the third segment and
 * this answers the first two, and they are separate tables because they cover different
 * populations: a rotation table holds only the names that MOVED, while the writer needs a
 * path for every set a build can hold.
 */
const PATH_BY_DATASET: Record<DatasetId, SetAlias> = {
  homecoming: HOMECOMING_MIDS_PATHS,
  rebirth: REBIRTH_MIDS_PATHS,
  thunderspy: THUNDERSPY_MIDS_PATHS,
  brainstorm: BRAINSTORM_MIDS_PATHS,
};

/** Mids' spelling of a powerset key → ours, for the pairs where the group segment drifted. */
const ALIAS_BY_DATASET: Record<DatasetId, SetAlias> = {
  homecoming: HOMECOMING_MIDS_SETS,
  rebirth: REBIRTH_MIDS_SETS,
  thunderspy: THUNDERSPY_MIDS_SETS,
  brainstorm: BRAINSTORM_MIDS_SETS,
};

/**
 * Separators and case folded away, so one spelling of a key reaches its row.
 *
 * Mids writes a powerset segment with a trailing space and sometimes a space where we
 * write an underscore (`Guardian_Composition.Stone composition`), and this table is read
 * from both namespaces — see `rowsFor`.
 */
function normalizeKey(key: string): string {
  return key.trim().replace(/[\s_-]+/g, '_').toLowerCase();
}

/**
 * This powerset's rows, whichever namespace spelled the key.
 *
 * The map is keyed by OUR `group.powerset`, because its main reader — the matcher in
 * `mids-import/mappers.ts` — builds the key from the candidate powers' own paths. But the
 * importer's retired-name check holds the .mbd's own path instead, and Mids' group segment
 * has drifted from ours (`Guardian_Composition` for `Guardian_Comp`, MBDIMPORT-7). Two
 * keyings of one table is how a fix lands on one reader and leaves the other wrong
 * (METHOD-7), so the alias is resolved here rather than at either call site.
 */
function rowsFor(powersetKey: string): Readonly<Record<string, string>> | undefined {
  return lookupRows(MAP_BY_DATASET[getActiveDataset().id], powersetKey);
}

/** The same resolution against the reverse table, so one keying serves both directions. */
function reverseRowsFor(powersetKey: string): Readonly<Record<string, string>> | undefined {
  return lookupRows(REVERSE_BY_DATASET[getActiveDataset().id], powersetKey);
}

function lookupRows(map: NameMap, powersetKey: string): Readonly<Record<string, string>> | undefined {
  const key = normalizeKey(powersetKey);
  if (map[key]) return map[key];
  const ours = ALIAS_BY_DATASET[getActiveDataset().id][key];
  return ours ? map[ours] : undefined;
}

/**
 * This dataset's internal name for the power Mids calls `midsInternalName` inside
 * `powersetKey` (`blaster_support.tactical_arrow`), or undefined when the two agree —
 * which is every name but a hundred-odd, so undefined is the overwhelmingly common answer.
 */
export function midsNameRemap(powersetKey: string, midsInternalName: string): string | undefined {
  return rowsFor(powersetKey)?.[midsInternalName.trim().toLowerCase()];
}

/**
 * Which Mids name owns each of THIS dataset's powers in `powersetKey`, as
 * `ourInternalName` (lower) → the Mids name that resolves to it.
 *
 * The inverse view exists because a rotation leaves a second way to bind the wrong power:
 * Stalker Willpower's `Resurgence` remaps to `Reconstruction`, and Mids' OWN `Reconstruction`
 * — a power HC has since removed — still exact-matches ours and takes the slot first, purely
 * because it is listed earlier in the file. An exact hit on a name that is demonstrably
 * another power's is not evidence, so the owner named here outranks it and the squatter
 * falls through to a warning.
 */
export function midsNameOwners(powersetKey: string): ReadonlyMap<string, string> {
  const owners = new Map<string, string>();
  for (const [midsName, ourName] of Object.entries(rowsFor(powersetKey) ?? {})) {
    owners.set(ourName.toLowerCase(), midsName);
  }
  return owners;
}

/**
 * Mids' internal name for THIS dataset's `ourInternalName` inside `powersetKey`, or
 * undefined when the two agree — which is every name but a hundred-odd.
 *
 * The writer's half of the rotation (DATA-GAP MBDEXPORT-3). Undefined is the normal
 * answer and means "write ours"; it does NOT mean "Mids has no such power". This table
 * carries only names that moved, so it cannot tell those two apart, and a caller that
 * reported every undefined would report the whole build.
 *
 * The returned string is Mids' literal spelling — its case and any inner or trailing
 * whitespace are part of the identity, because `PiDFromUidPower` compares with `==`.
 * Write it through unaltered.
 */
export function midsNameForExport(
  powersetKey: string,
  ourInternalName: string,
): string | undefined {
  return reverseRowsFor(powersetKey)?.[ourInternalName.trim().toLowerCase()];
}

/**
 * Mids' own spelling of OUR `group.powerset` (`guardian_comp.atmospheric_composition` →
 * `Guardian_Composition.Atmospheric_Composition`), or undefined when nothing pairs them.
 *
 * Undefined does NOT mean "write ours" here, and that is the difference from
 * `midsNameForExport` above. This table covers every paired set rather than only the ones
 * that drifted, so a miss is a set the pairing could not reach — and Mids answers a
 * `group.set` it cannot resolve with a blank row that keeps the power's slots. The caller
 * says so.
 */
export function midsPowersetPathForExport(ourPowersetKey: string): string | undefined {
  return PATH_BY_DATASET[getActiveDataset().id][normalizeKey(ourPowersetKey)];
}

/**
 * Whether this fork's Mids namespace was read at all.
 *
 * False where the names dump this dataset was generated from predates MBDEXPORT-6 and
 * carries powerset keys folded to lower case — Mids' own spelling is not recoverable from
 * it, so `midsPowersetPathForExport` answers undefined for every set on the fork. That is
 * a different fact from "Mids has no such powerset", and a warning that conflates the two
 * sends the reader after the wrong thing.
 */
export function midsPowersetPathsKnown(): boolean {
  return Object.keys(PATH_BY_DATASET[getActiveDataset().id]).length > 0;
}

/** The active dataset's whole table — for gates and audits, not for resolution. */
export function midsNameMap(): NameMap {
  return MAP_BY_DATASET[getActiveDataset().id];
}

/** The active dataset's whole reverse table — for gates and audits, not for resolution. */
export function midsNameReverseMap(): NameMap {
  return REVERSE_BY_DATASET[getActiveDataset().id];
}
