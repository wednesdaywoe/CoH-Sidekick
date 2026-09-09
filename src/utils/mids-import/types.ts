/**
 * Type definitions for Mids Reborn .mbd file format and import results
 */

import type { Build } from '@/types';

// ============================================
// .MBD FILE STRUCTURE
// ============================================

export interface MbdBuiltWith {
  App: string;
  Version: string;
  Database: string;
  DatabaseVersion: string;
}

export interface MbdEnhancement {
  Uid: string;
  Grade: string;
  IoLevel: number;
  RelativeLevel: string;
  Obtained: boolean;
}

export interface MbdSlotEntry {
  Level: number;
  IsInherent: boolean;
  Enhancement: MbdEnhancement | null;
  FlippedEnhancement: MbdEnhancement | null;
}

export interface MbdPowerEntry {
  PowerName: string;
  Level: number;
  StatInclude: boolean;
  ProcInclude: boolean;
  VariableValue: number;
  InherentSlotsUsed: number;
  SubPowerEntries: unknown[];
  SlotEntries: MbdSlotEntry[];
}

export interface MbdFile {
  BuiltWith: MbdBuiltWith;
  Level: string;
  Class: string;
  Origin: string;
  Alignment: string;
  Name: string;
  Comment: string;
  PowerSets: string[];
  LastPower: number;
  PowerEntries: MbdPowerEntry[];
}

// ============================================
// IMPORT RESULT
// ============================================

export type MidsWarningType = 'archetype' | 'powerset' | 'power' | 'pool' | 'epic' | 'enhancement' | 'general';

export interface MidsImportWarning {
  type: MidsWarningType;
  midsName: string;
  message: string;
}

export interface MidsImportSummary {
  /**
   * Power PICKS the finished build holds — derived from the build at the end of the
   * import, not tallied as entries resolve. See `countBudgetPowerPicks`: the tally it
   * replaced counted inherent slot-data entries, accolades and incarnate slots as
   * powers, and reported 31 for a build the dashboard then showed as 23 of 24 picks.
   */
  powersImported: number;
  powersFailed: number;
  /**
   * Enhancements the finished build holds. Every enhancement the file carries is in
   * this count or in `enhancementsFailed` — MBDIMPORT-5 was six that were in neither,
   * riding out of the build on a power the reassign guard was right to refuse.
   */
  enhancementsImported: number;
  /**
   * Enhancements the file carries that the build does not: a piece whose UID this
   * dataset cannot resolve, and every piece slotted into an entry the importer
   * declined — reported at the enhancement level whatever refused the entry holding
   * it, because a power-level warning does not say how much went with it.
   */
  enhancementsFailed: number;
  /**
   * Slot entries the file carries that the build took. Not the file's slot count —
   * a declined entry brings slots too, and those are `slotsSkipped`. Not the finished
   * build's either, which is larger: every power arrives holding a free first slot the
   * file never mentioned.
   */
  slotsImported: number;
  /**
   * Slot entries the file carries that the build did not take, so that the sentence
   * above is checkable rather than merely written: `slotsImported + slotsSkipped` is
   * the file's own slot count. Optional on the same terms as `accoladesImported`.
   */
  slotsSkipped?: number;
  /**
   * Accolade toggles switched on, counted apart from powers because they consume no
   * pick. Optional, and absent rather than 0 for an importer that does not report the
   * split — the .mxd path shares this result type and has no separate accolade pass, so
   * a hard 0 there would state a count it never took.
   */
  accoladesImported?: number;
  /** Incarnate slots filled, on the same terms. */
  incarnatesImported?: number;
}

export interface MidsImportResult {
  success: boolean;
  build: Build | null;
  warnings: MidsImportWarning[];
  summary: MidsImportSummary;
  /** For VEATs: the detected branch (e.g., 'crab-spider') so the UI can auto-set selectedBranch */
  detectedBranch?: string | null;
  /** Per-power stack / targets-hit slider values from Mids' `VariableValue`
   *  field, keyed by `internalName`. The caller applies these to the UI
   *  store after `applyMidsBuild` so the dashboard reproduces Mids' totals
   *  (e.g. Siphon Speed VariableValue=2 → 2 stacks of +Recharge). */
  targetsHit?: Record<string, number>;
}
