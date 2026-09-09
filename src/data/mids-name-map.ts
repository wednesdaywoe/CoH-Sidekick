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
} from './datasets/homecoming/generated/mids-name-map';
import {
  MIDS_NAME_MAP as REBIRTH_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as REBIRTH_MIDS_SETS,
} from './datasets/rebirth/generated/mids-name-map';
import {
  MIDS_NAME_MAP as THUNDERSPY_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as THUNDERSPY_MIDS_SETS,
} from './datasets/thunderspy/generated/mids-name-map';
import {
  MIDS_NAME_MAP as BRAINSTORM_MIDS_NAMES,
  MIDS_POWERSET_ALIAS as BRAINSTORM_MIDS_SETS,
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
  const dataset = getActiveDataset().id;
  const key = normalizeKey(powersetKey);
  const map = MAP_BY_DATASET[dataset];
  if (map[key]) return map[key];
  const ours = ALIAS_BY_DATASET[dataset][key];
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

/** The active dataset's whole table — for gates and audits, not for resolution. */
export function midsNameMap(): NameMap {
  return MAP_BY_DATASET[getActiveDataset().id];
}
