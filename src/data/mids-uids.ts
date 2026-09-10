/**
 * Mids Reborn's enhancement-UID namespace, read out of Mids' own EnhDB.
 *
 * Mids resolves a slotted enhancement by UID substring match
 * (`DatabaseAPI.GetEnhancementByUIDName`) and, when nothing matches, sets
 * `I9Slot.Enh = -1` and moves on. The slot comes up empty with no error and no
 * log, so a UID we get wrong doesn't look like a bug on either side — it looks
 * like the user never slotted anything.
 *
 * That rules out deriving the UID from a set's display name, which is what the
 * exporter used to do. The prefix is a per-set fact (`Crafted_` / `Attuned_` /
 * `Superior_Attuned_`, and the third keeps its own "Superior_" for ATOs but
 * drops it for winter sets), and Mids carries spellings no rule recovers:
 * `Numinas_Convalesence`, `ToHit_DeBuff`, `Gaussians_Synchronized_FireControl`.
 *
 * So the table is generated from EnhDB.mhd — Mids is the only authority on its
 * own namespace — by `tools/mids-oracle/emit_mids_uids.py`.
 *
 * WHOSE EnhDB, per fork, because two of the four are not their own (MBDEXPORT-2).
 * Homecoming and Rebirth read Mids' databases of those names. Brainstorm reads
 * Homecoming's, which is the database a Brainstorm build is authored in anyway.
 * Thunderspy reads Mids' GENERIC database — `Thunderspy/EnhDB.mhd` is byte-identical
 * to `Generic/EnhDB.mhd` — because Mids Reborn has never shipped a Thunderspy
 * database. It answers for 210 of that fork's 213 sets, since those are shared CoH
 * sets; the three it cannot are `kb` and the two Primalist ATOs, which exist in no
 * Mids database at all and are reported per slot by the export rather than dropped.
 * `sourceSha256` and the gate over it gauge freshness, and neither can see which
 * database the bytes are.
 */

import { getActiveDataset } from './dataset';

export interface MidsUidTable {
  /**
   * setId → piece UID, indexed by `pieceNum - 1`. Keys match the planner's
   * `IOSetRegistry` ids, which are the Mids set stem lowercased with
   * apostrophes and hyphens dropped — the same normalization the import path
   * applies in `parseIOSetUid`.
   *
   * A gap in the array (empty string) is a set whose EnhDB record is missing
   * that piece; callers must treat it as unresolvable rather than emit it.
   */
  ioSetPieces: Record<string, readonly string[]>;
  /** Every crafted generic IO UID Mids knows, e.g. `Crafted_Endurance_Discount`. */
  genericIO: readonly string[];
  /** Hamidon / Hydra / Titan / D-Sync and friends. */
  special: readonly string[];
  /** Origin (TO/DO/SO) enhancement UIDs. */
  origin: readonly string[];
  /**
   * SHA-256 of the EnhDB.mhd this table was read from.
   *
   * Vendoring a newer Mids database without re-running the emitter would leave
   * the table describing a file that is no longer there — and the failure mode
   * is a UID Mids has since renamed, which comes back as an empty slot rather
   * than an error. The staleness gate compares this against the file on disk.
   */
  sourceSha256: string;
}

const EMPTY: MidsUidTable = { ioSetPieces: {}, genericIO: [], special: [], origin: [], sourceSha256: '' };

/** The active dataset's UID table. */
export function getMidsUids(): MidsUidTable {
  return getActiveDataset().midsUids ?? EMPTY;
}

/**
 * The Mids UID for one piece of an IO set, or `null` when this dataset's Mids
 * database doesn't carry it.
 *
 * Returning `null` rather than a derived guess is the point: the caller has to
 * decide what to do about an enhancement Mids can't be told about, and a guess
 * would come back as a silently empty slot.
 */
export function getMidsIOSetPieceUid(setId: string, pieceNum: number): string | null {
  const pieces = getMidsUids().ioSetPieces;
  const entry = pieces[setId] ?? pieces[setId.replace(/-/g, '')];
  return entry?.[pieceNum - 1] || null;
}

/** Resolve a Mids UID suffix (`Endurance_Discount`) to its crafted generic IO UID. */
export function getMidsGenericIOUid(suffix: string): string | null {
  const wanted = `crafted_${suffix.toLowerCase()}`;
  return getMidsUids().genericIO.find((uid) => uid.toLowerCase() === wanted) ?? null;
}

/**
 * Mids' UID for an origin (TO/DO/SO) enhancement of this stat.
 *
 * Mids keeps one record per stat and takes the tier from the slot's `Grade` and
 * the origin from the character, so its whole roster is spelled `Magic_*` — the
 * prefix is a name, not a claim about the character's origin. Looked up rather
 * than assembled, so the day a fork's database spells them differently this
 * returns null instead of a UID that opens as an empty slot.
 */
export function getMidsOriginUid(craftedUid: string): string | null {
  const suffix = craftedUid.replace(/^Crafted_/, '').toLowerCase();
  return getMidsUids().origin.find((uid) => uid.toLowerCase().endsWith(`_${suffix}`)) ?? null;
}

/**
 * Mids' own spelling of an exotic (Hamidon/Titan/Hydra/D-Sync) UID, or `null`
 * if it has none.
 *
 * Matched case-insensitively because Mids is not consistent with itself: its
 * D-Sync roster is half `DSync_` and half `Dsync_`, and it spells the debuff
 * half `Debuff` there while the crafted IOs say `DeBuff`. Its own lookup is a
 * substring match that never sees the difference; ours has to be told.
 */
export function getMidsSpecialUid(uid: string): string | null {
  const wanted = uid.toLowerCase();
  return getMidsUids().special.find((known) => known.toLowerCase() === wanted) ?? null;
}

/**
 * The reverse index: a Mids UID → the set and piece it names.
 *
 * Reading the table both ways is what keeps the two directions honest. Deriving
 * the set from the UID's own text — lowercase the stem, take the trailing
 * letter — is right for most sets and wrong for the ones where Mids' UID is a
 * fossil: its Artillery set is spelled `Crafted_Shrapnel_*` after a rename, and
 * Exploit Weakness's third piece ends in a lowercase `_c`. Both parse to a set
 * we have never heard of, so the piece imports as nothing.
 *
 * Rebuilt when the active dataset changes; the table is per-server.
 */
let reverseIndex: { table: MidsUidTable; map: Map<string, { setId: string; pieceNum: number }> } | null = null;

function getReverseIndex(): Map<string, { setId: string; pieceNum: number }> {
  const table = getMidsUids();
  if (reverseIndex?.table === table) return reverseIndex.map;

  const map = new Map<string, { setId: string; pieceNum: number }>();
  for (const [setId, pieces] of Object.entries(table.ioSetPieces)) {
    pieces.forEach((uid, index) => {
      if (uid) map.set(uid, { setId, pieceNum: index + 1 });
    });
  }
  reverseIndex = { table, map };
  return map;
}

/** The set and piece a Mids IO-set UID names, or `null` if this dataset has no such UID. */
export function resolveMidsUid(uid: string): { setId: string; pieceNum: number } | null {
  return getReverseIndex().get(uid) ?? null;
}
