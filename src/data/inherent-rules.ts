/**
 * Inherent power rules facade.
 *
 * Each dataset describes the per-server quirks of the universal
 * inherent powers (Fitness L1 vs L2, Rebirth's auto-granted Health /
 * Stamina slots, future server variations). This file forwards reads
 * to the active dataset so call sites don't have to thread it through.
 */

import { getActiveDataset } from './dataset';

/**
 * Returns the `available` override for a given inherent power
 * `internalName` on the active server, or `undefined` if the dataset
 * doesn't override it. The shared InherentPowerDef default applies
 * when this returns undefined.
 */
export function getInherentAvailabilityOverride(internalName: string): number | undefined {
  return getActiveDataset().inherentRules.availabilityOverrides[internalName];
}

/**
 * Returns the auto-granted slot levels for a given inherent power
 * `internalName` on the active server. These slots come outside the
 * 67-slot user budget. Empty array (the default) means the power has
 * no auto-grants — same as HC for everything.
 */
export function getInherentAutoGrantedSlotLevels(internalName: string): readonly number[] {
  return getActiveDataset().inherentRules.autoGrantedSlotLevels[internalName] ?? [];
}

/**
 * Returns the cumulative count of auto-granted slots a power should
 * have at the given character level. Replaces the Rebirth-specific
 * helper that hard-coded Health/Stamina; now handled generically per
 * dataset.
 */
export function getInherentAutoGrantedSlotCount(internalName: string, level: number): number {
  const levels = getInherentAutoGrantedSlotLevels(internalName);
  let count = 0;
  for (const lvl of levels) {
    if (level >= lvl) count++;
  }
  return count;
}

/**
 * The `Inherent.Inherent` full name of an archetype's HEADLINE inherent — the
 * power it is built around, by the name the EXPORT files it under.
 *
 * Needed wherever this power has to be named for another program. Our own roster
 * carries `Inherent.<Archetype>.<Name>`, synthesised by
 * `createArchetypeInherentPower` from the archetype's display name, and no other
 * planner resolves that: Mids reads `Inherent.Inherent.Rage_Buff` where we would
 * say `Inherent.Brute.Fury`. The `.mbd` writer wrote no archetype inherent at all
 * until it had this (MBDEXPORT-20).
 *
 * `undefined` when this fork ships no single power for the archetype's declared
 * inherent — a real gap the caller reports rather than fills (INHERENT-10).
 */
export function headlineArchetypeInherentName(archetypeId: string): string | undefined {
  return getActiveDataset().inherentRules.headlineArchetypeInherents?.[archetypeId];
}
