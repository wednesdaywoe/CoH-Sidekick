/**
 * Incarnate damage procs — the chance-on-attack damage hits that Hybrid
 * Assault and Interface incarnate slots add to every attack.
 *
 * These are surfaced in the planner's per-attack damage breakdown so the
 * average-damage number matches what the player will see in-game (and what
 * Mids reports). The proc system here is parallel to slotted IO procs but
 * the trigger comes from the active Incarnate, not a slotted enhancement.
 *
 * Source: extracted from `exported_powers/incarnate/{hybrid,hybrid_silent,
 * interface,interface_silent}/*.json` for HC. Rebirth uses the same data.
 *
 * # Trigger semantics
 *
 * **Hybrid Assault Radial** — PPM-based. Grants a Doublehit power whose
 * inner PPM determines firing frequency on each outgoing attack (e.g.
 * Assault Radial Embodiment = 6 PPM Energy scale 0.4 of Melee_Tempdamage).
 * Subject to the same PPM area-factor / cooldown rules as slotted procs.
 *
 * **Hybrid Assault Core** — fixed chance for a self damage-strength buff
 * (e.g. Core Embodiment = 65% chance for +15% damage strength for 10s).
 * Not modeled here because the existing global-damage bonus already
 * captures it via the passive `damage` field in incarnate-effects.ts; the
 * proc-style "buff per attack" is closer to a Build Up uptime model than
 * a damage hit. Skipping for now — Mids treats this as a passive buff too.
 *
 * **Interface** — fixed chance per attack (T4 Flawless = 75%). Each
 * Interface tree has up to two procs (one secondary effect like -Res or
 * Confuse, plus a DoT). Only damage procs are listed here; debuff-only
 * Interfaces (Diamagnetic, Paralytic) have no entry.
 *
 * # DoT modeling
 *
 * Interface DoTs deal `scale × table` damage per tick over a duration
 * (typical: 4 ticks over 4.3s with period 1s). Total damage per cast =
 * scale × table × ticks. We surface the AVERAGE (chance × total) so a
 * line item like "Reactive Radial: 75% × 0.125 × table × 4 ticks" lands
 * in the same row format as slotted damage procs.
 */

import type { IncarnateBuildState } from '@/types';
import { calculateProcChance } from '@/data/proc-data';
import { getTableValue } from '@/data/at-tables';

export interface IncarnateDamageProc {
  /** Display name for the proc row (e.g. "Assault Radial: Doublehit"). */
  name: string;
  /** Damage type label shown in the proc row. */
  damageType: string;
  /** Per-attack trigger mode. PPM uses calculateProcChance; fixed-chance
   *  fires at the given `chance` regardless of cycle time. */
  trigger:
    | { kind: 'ppm'; ppm: number }
    | { kind: 'chance'; chance: number };
  /** Damage scale multiplied by the AT's `Melee_Tempdamage` table value
   *  at the build level. */
  scale: number;
  /** Number of ticks per proc activation. 1 for single-hit procs (Hybrid),
   *  4-5 for Interface DoTs. The actual game can have a `tick_chance` of
   *  0 (= guaranteed all ticks fire after the initial trigger); we use the
   *  nominal tick count for the average. */
  ticks: number;
  /** Damage table to look up via the AT's modifier tables. The aliasing
   *  in at-tables.getTableValue maps `Melee_Tempdamage` to Melee_Damage. */
  table: string;
  /** PvE-only / Friend-only / vs-critter etc. gates would go here. All
   *  current entries assume vs-foe usage; default behaviour. */
}

/** Map from selected incarnate power ID (the `powerId` on
 *  SelectedIncarnatePower) → list of damage procs that fire while that
 *  power is active. */
export const INCARNATE_DAMAGE_PROCS: Record<string, IncarnateDamageProc[]> = {
  // ============================================
  // HYBRID — Assault Radial doublehit procs
  // ============================================
  // T1 Genome (Common) — Smashing 2 PPM scale 0.2
  'assault_radial_genome': [
    { name: 'Assault Radial: Doublehit', damageType: 'Energy', trigger: { kind: 'ppm', ppm: 2 }, scale: 0.2, ticks: 1, table: 'Melee_Tempdamage' },
  ],
  // T2 Partial Radial Graft — Smashing 4 PPM scale 0.2
  'assault_partial_radial_graft': [
    { name: 'Assault Radial: Doublehit', damageType: 'Smashing', trigger: { kind: 'ppm', ppm: 4 }, scale: 0.2, ticks: 1, table: 'Melee_Tempdamage' },
  ],
  // T3 Total Radial Graft (Rare) — Energy 4 PPM scale 0.3
  'assault_total_radial_graft': [
    { name: 'Assault Radial: Doublehit', damageType: 'Energy', trigger: { kind: 'ppm', ppm: 4 }, scale: 0.3, ticks: 1, table: 'Melee_Tempdamage' },
  ],
  // T4 Radial Embodiment (Very Rare) — Energy 6 PPM scale 0.4
  'assault_radial_embodiment': [
    { name: 'Assault Radial: Doublehit', damageType: 'Energy', trigger: { kind: 'ppm', ppm: 6 }, scale: 0.4, ticks: 1, table: 'Melee_Tempdamage' },
  ],

  // Partial Core / Common Core / Uncommon — Smashing variants. The Core
  // path's later tiers grant damage-strength buffs (Damage_Boost_*) which
  // are handled via incarnate-effects passive damage rather than as
  // per-attack procs.
  'assault_partial_core_graft': [
    { name: 'Assault Core: Doublehit', damageType: 'Smashing', trigger: { kind: 'ppm', ppm: 4 }, scale: 0.1, ticks: 1, table: 'Melee_Tempdamage' },
  ],
  // (assault_core_embodiment / assault_core_genome / assault_total_core_graft
  //  use Damage_Boost_* — modeled as passive damage strength, no proc row.)

  // ============================================
  // INTERFACE — DoT procs (75% chance at T4 Flawless)
  // ============================================
  // Reactive (Fire DoT)
  'reactive_core_flawless_interface': [
    { name: 'Reactive Core: Fire DoT', damageType: 'Fire', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  'reactive_radial_flawless_interface': [
    { name: 'Reactive Radial: Fire DoT', damageType: 'Fire', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  // Degenerative (Toxic DoT — Core/Radial)
  'degenerative_core_flawless_interface': [
    { name: 'Degenerative Core: Toxic DoT', damageType: 'Toxic', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.1, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  'degenerative_radial_flawless_interface': [
    { name: 'Degenerative Radial: Toxic DoT', damageType: 'Toxic', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.1, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  // Cognitive (Psionic DoT — Core/Radial)
  'cognitive_core_flawless_interface': [
    { name: 'Cognitive Core: Psi DoT', damageType: 'Psionic', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.1, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  'cognitive_radial_flawless_interface': [
    { name: 'Cognitive Radial: Psi DoT', damageType: 'Psionic', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.1, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  // Preemptive (Energy DoT — Core/Radial)
  'preemptive_core_flawless_interface': [
    { name: 'Preemptive Core: Energy DoT', damageType: 'Energy', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  'preemptive_radial_flawless_interface': [
    { name: 'Preemptive Radial: Energy DoT', damageType: 'Energy', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  // Spectral (Negative DoT — Core/Radial)
  'spectral_core_flawless_interface': [
    { name: 'Spectral Core: Negative DoT', damageType: 'Negative', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  'spectral_radial_flawless_interface': [
    { name: 'Spectral Radial: Negative DoT', damageType: 'Negative', trigger: { kind: 'chance', chance: 0.75 }, scale: 0.125, ticks: 4, table: 'Melee_Tempdamage' },
  ],
  // Diamagnetic and Paralytic have no damage procs — they're debuff-only.
};

/**
 * Collect all active incarnate damage procs for the build. The caller
 * passes the `IncarnateBuildState` and the `IncarnateActiveState` toggle
 * map; procs only contribute when the slot is both selected and active.
 */
export function getActiveIncarnateDamageProcs(
  incarnates: IncarnateBuildState,
  active: { hybrid: boolean; interface: boolean },
): IncarnateDamageProc[] {
  const out: IncarnateDamageProc[] = [];
  if (active.hybrid && incarnates.hybrid?.powerId) {
    const procs = INCARNATE_DAMAGE_PROCS[incarnates.hybrid.powerId];
    if (procs) out.push(...procs);
  }
  if (active.interface && incarnates.interface?.powerId) {
    const procs = INCARNATE_DAMAGE_PROCS[incarnates.interface.powerId];
    if (procs) out.push(...procs);
  }
  return out;
}

/** Computed proc row for the per-attack damage display. */
export interface IncarnateProcContribution {
  /** Display name (source identifier). */
  name: string;
  /** Damage type label. */
  damageType: string;
  /** Trigger chance per cast (post-PPM/area math, 0..1). */
  chance: number;
  /** Total damage per proc activation (all ticks summed for DoTs). */
  damagePerActivation: number;
  /** chance × damagePerActivation. */
  avgDamage: number;
  /** Avg damage spread over the power's cycle time (seconds). */
  dps: number;
}

/**
 * Compute the per-cast contribution of every active incarnate damage proc
 * for the given attack. Mirrors the slotted-IO proc calc but uses the
 * AT's Melee_Tempdamage table and PPM/chance from the incarnate data.
 *
 * @param procs        From `getActiveIncarnateDamageProcs`.
 * @param archetypeId  Lowercase AT key for the damage-table lookup.
 * @param buildLevel   Character level (drives table lookup row).
 * @param baseRecharge Attack's base recharge in seconds (PPM math).
 * @param castTime     Attack's raw cast time (not ArcanaTime).
 * @param radius       AoE radius in feet (0 for ST). PPM area factor.
 * @param arcDegrees   Cone arc in degrees (360 for sphere/ST).
 * @param enhancedRechargeBonus  This power's own slotted recharge as decimal. Build-wide
 *   recharge is deliberately NOT a parameter: HC's PPM rule counts the power's own
 *   enhancements and Alpha only (DATA-GAP-REGISTER PPM-5).
 * @param expectedTargets  Number of foes the proc damage gets applied to.
 *   Unlike slotted IO procs (which Mids treats as single-target per row),
 *   Mids multiplies Hybrid / Interface proc damage by the attack's
 *   `maxTargets` so an AoE Shield Charge shows `0.4 × table × 16` for
 *   the Doublehit. Defaults to 1 (single-target). Callers pass the
 *   attack's expected target count for AoEs.
 */
export function computeIncarnateProcContributions(
  procs: IncarnateDamageProc[],
  archetypeId: string,
  buildLevel: number,
  baseRecharge: number,
  castTime: number,
  radius: number,
  arcDegrees: number,
  enhancedRechargeBonus: number,
  expectedTargets: number = 1,
): IncarnateProcContribution[] {
  if (procs.length === 0) return [];
  const cycleTime = (baseRecharge + castTime) || 1;
  const out: IncarnateProcContribution[] = [];
  const targets = Math.max(1, expectedTargets);
  for (const p of procs) {
    const tableValue = getTableValue(archetypeId, p.table, buildLevel);
    if (tableValue === undefined) continue;
    // Damage tables are stored negative (the engine treats outgoing damage
    // as a negative attribute mod); abs() gives the magnitude.
    const damagePerTick = Math.abs(tableValue) * p.scale;
    const damagePerActivation = damagePerTick * p.ticks * targets;
    const chance = p.trigger.kind === 'ppm'
      ? calculateProcChance(p.trigger.ppm, baseRecharge, castTime, radius, arcDegrees, enhancedRechargeBonus)
      : p.trigger.chance;
    const avgDamage = chance * damagePerActivation;
    out.push({
      name: p.name,
      damageType: p.damageType,
      chance,
      damagePerActivation,
      avgDamage,
      dps: avgDamage / cycleTime,
    });
  }
  return out;
}
