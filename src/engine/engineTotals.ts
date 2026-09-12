/**
 * SPIKE5 — output mapper: engine `CalculatedTotals` (snake_case) → beta
 * `CharacterCalculationResult` (camelCase). The mirror of the SPIKE4 input adapter.
 *
 * The engine owns the numbers; this only reshapes keys. Movement lives in the engine's
 * `GlobalBonuses` (not its `CharacterStats`), so the beta `stats.runspeed…` fields are
 * pulled from `bonuses`. A handful of beta fields have no engine source (`threatLevel`,
 * `protRepel`, `enduranceDiscount`) and are left 0 — none feed the totals
 * dashboard. `toggleEndCost`/`netEndPerSec` DID feed it (the Survival & Mobility card's END
 * COST and NET END) and were wrongly in that list: they read 0 for every build from the
 * engine swap until the engine grew them (Step 9.7, `projection::toggle_endurance_total`).
 * `bonusTracking` and `breakdown` are populated from the engine's own tracking: set-bonus
 * Rule-of-5 provenance (the `(x/5)` counters, capped rings/banner, per-stat tooltip set-bonus
 * rows) plus every per-source ledger the engine files — procs, buff-pet auras, travel and
 * stealth radii, the per-power walk (active powers, accolades, archetype inherents) and
 * incarnates. The engine's own reconciliation gate proves those ledgers are disjoint and
 * together account for each total, so they fold straight on top of one another with no dedupe.
 * `setBonuses` (AggregatedBonuses) stays empty — no UI reads it.
 */

import type { Build } from '@/types/build';
import type { SelectedPower } from '@/types/power';
import type { IncarnateActiveState } from '@/types/incarnate';
import { getAccolades, accoladeId } from '@/data/accolades';
import type { CharacterCalculationResult } from '@/utils/calculations';
import { toCharacterStateJson, type AdapterCalcContext } from './characterStateAdapter';
import { recalcJson } from './engine';
import { useEngineStore, EMPTY_CALC_ERRORS, type CalcErrorReport } from './engineStore';
import {
  mapStats,
  mapGlobal,
  mapBonusTracking,
  mapSetBonusBreakdown,
  addProcBreakdown,
  addBuffPetBreakdown,
  addMovementBreakdown,
  addStealthBreakdown,
  addPowerBreakdown,
  addIncarnateBreakdown,
  mapPowerProjection,
  type EngineTotals,
  type PowerProjection,
  type PowerNameResolver,
  type IncarnateNameResolver,
} from './engineTotalsMap';

/**
 * A resolver from an engine source ref back to the slotting power's DISPLAY name — the engine's
 * `SelectedPower` carries only `internal_name`/`power_set`, but the over-cap ring matches the
 * PowerRow's display name, so we rebuild it from the same build the engine read. Keyed by
 * `powerSet\0internalName` (internal names are not unique across sets). Unresolved refs fall
 * back to the internal name rather than dropping the source.
 *
 * Accolades need the second lookup: they are powers by the time the engine's ledger sees them,
 * but the build stores them as a list of ids rather than as picked powers, so they are in no
 * powerset the loop above walks. Their id IS the lower-cased internal name (the adapter's own
 * convention), which is what makes the registry answer.
 */
function powerNameResolver(build: Build): PowerNameResolver {
  const byKey = new Map<string, string>();
  const add = (powers: SelectedPower[] | undefined) => {
    for (const p of powers ?? []) byKey.set(`${p.powerSet}\0${p.internalName}`, p.name);
  };
  add(build.primary.powers);
  add(build.secondary.powers);
  for (const pool of build.pools) add(pool.powers);
  add(build.epicPool?.powers);
  add(build.inherents);
  const accoladeName = new Map(getAccolades().map((p) => [accoladeId(p), p.name]));
  // Second-chance lookup on the internal name alone, kept only where it is unambiguous
  // within this build. It answers the case the primary key cannot: the adapter addresses a
  // pool power by its POOL id while `powerSet` is stamped from the set the power was picked
  // from, so a build whose pool grouping is wrong misses `byKey` for every pool power — and
  // that build is precisely the one whose name needs printing in the calc-error banner.
  const byInternal = new Map<string, string | null>();
  for (const [key, name] of byKey) {
    const internal = key.slice(key.indexOf('\0') + 1);
    byInternal.set(internal, byInternal.has(internal) ? null : name); // null = ambiguous
  }
  return (ref) =>
    byKey.get(`${ref.power_set}\0${ref.power_internal_name}`) ??
    accoladeName.get(ref.power_internal_name.toLowerCase()) ??
    byInternal.get(ref.power_internal_name) ??
    ref.power_internal_name;
}

/**
 * The incarnate twin of {@link powerNameResolver}: the engine's ledger addresses a contributor
 * by slot + the internal name the loadout stores, and the loadout carries the display name
 * beside it. Read from the build rather than the incarnate catalog for the same reason the
 * power resolver is — it names what this build actually equipped.
 */
function incarnateNameResolver(build: Build): IncarnateNameResolver {
  return (src) => {
    const equipped = build.incarnates?.[src.slot as keyof Build['incarnates']];
    return equipped?.displayName ?? src.power_name;
  };
}

/**
 * `selected power <powerset>/<internalName> is not in this dataset — …`, the one calc error
 * a user can act on: a power they picked that this dataset cannot resolve, so it contributes
 * nothing to any total while still rendering as a normal card. Matched on the message
 * because `CalcError` carries no structured fields; a message that stops matching costs the
 * banner its power NAMES, never its warning — `classifyCalcErrors` falls back to counting.
 */
const MISSING_SELECTED_POWER = /^selected power (\S+?)\/(\S+?) is not in this dataset/;

/**
 * Split the engine's calc errors into the part worth naming and the part worth counting.
 * Exported for its own test — the regex above is the fragile joint.
 */
export function classifyCalcErrors(
  errors: { context: string; detail: string }[] | undefined,
  resolveName: PowerNameResolver
): CalcErrorReport {
  if (!errors?.length) return EMPTY_CALC_ERRORS;
  const missingPowers: string[] = [];
  let allMissingPowers = true;
  for (const e of errors) {
    const m = e.context === 'gather' ? MISSING_SELECTED_POWER.exec(e.detail) : null;
    if (m) {
      missingPowers.push(resolveName({ power_set: m[1], power_internal_name: m[2] }));
    } else {
      allMissingPowers = false;
    }
  }
  return {
    lines: errors.map((e) => `${e.context} — ${e.detail}`),
    missingPowers,
    allMissingPowers,
  };
}

/** Log every calc error (fail-loud) and hand the UI its banner copy. */
function reportCalcErrors(
  errors: { context: string; detail: string }[] | undefined,
  resolveName: PowerNameResolver
): void {
  for (const e of errors ?? []) console.error(`[engine] ${e.context}: ${e.detail}`);
  const report = classifyCalcErrors(errors, resolveName);
  // Deferred for the same reason as `setError` in `calculateCharacterTotals`: this runs
  // inside a useMemo, and a zustand write during render warns.
  queueMicrotask(() => useEngineStore.getState().setCalcErrors(report));
}

/**
 * Run the build through the engine and reshape to the beta result. Returns `null` when the
 * dataset isn't loaded yet (boot) — the caller substitutes an empty result until then.
 * Any engine `errors` are logged AND surfaced in the calc-error banner (fail-loud) but do
 * not blank the totals.
 */
export function engineCalculate(build: Build, ctx: AdapterCalcContext): CharacterCalculationResult | null {
  const stateJson = toCharacterStateJson(build, ctx);
  const out = recalcJson(build.serverId, stateJson);
  if (out === null) return null;

  const totals = JSON.parse(out) as EngineTotals;
  const resolveName = powerNameResolver(build);
  reportCalcErrors(totals.bonuses.errors, resolveName);
  const breakdown = mapSetBonusBreakdown(totals.set_bonus_tracking, resolveName);
  addProcBreakdown(breakdown, totals.proc_breakdown, resolveName);
  addBuffPetBreakdown(breakdown, totals.buff_pet_breakdown, resolveName);
  addMovementBreakdown(breakdown, totals.movement_breakdown);
  addStealthBreakdown(breakdown, totals.stealth_breakdown);
  addPowerBreakdown(breakdown, totals.power_breakdown, resolveName);
  addIncarnateBreakdown(breakdown, totals.incarnate_breakdown, incarnateNameResolver(build));
  return {
    stats: mapStats(totals.stats, totals.bonuses),
    globalBonuses: mapGlobal(totals.bonuses, totals.stats),
    breakdown,
    setBonuses: {},
    bonusTracking: mapBonusTracking(totals.set_bonus_tracking, resolveName),
    powerProjection: mapPowerProjection(totals.power_projection),
    damageCeiling: totals.damage_ceiling ?? null,
    // The accumulator keys the what-if layer moved. Carried through so a stat row can mark
    // itself SIMULATED from the engine's own record rather than by re-reading the sliders.
    whatIfMoved: totals.what_if?.moved ?? {},
    engineStateJson: stateJson,
  };
}

/** Re-export for the totals hook's context assembly. */
export type { AdapterCalcContext, IncarnateActiveState };
export type { PowerProjection };
