/**
 * Slotted damage-proc contribution per cast — the single source for the
 * "+proc" damage shown on the DamageBlock and folded into the Attack Chain
 * builder's DPS. Extracted from DamageBlock so the two can't diverge.
 *
 * Proc damage is FLAT in CoH: it is NOT modified by damage IOs slotted in the
 * power, nor by damage-strength buffs (Fury/Build Up/Aim/Musculature). A proc
 * fires at its fixed scale-table value. Proc *chance* uses the power's base
 * recharge shrunk by its own slotted recharge and widened back out by the
 * build's global — `base × (1 + global) / (1 + global + local)`. Global alone
 * cancels, which is why the older reading "global recharge never affects PPM
 * rolls" held for as long as nobody slotted recharge in the power; once they do,
 * global softens the penalty. Measured 2026-08-02 (DATA-GAP-REGISTER PPM-2).
 */

import type { Enhancement, IOSetEnhancement } from '@/types';
import type { ProcRollSite } from '@/types/power';
import {
  findProcData,
  getProcEffects,
  interpolateProcDamage,
  arcToDegrees,
  resolveProcRollGeometry,
  resolveProcRollSchedule,
  resolveProcRollSite,
  calculateScheduledProcChance,
  powerFiresProcs,
} from '@/data';

export interface SlottedProcDamageInput {
  slots: (Enhancement | null)[];
  /** Base recharge (s) — the window is built from this plus both recharge terms. */
  baseRecharge: number;
  /** Raw cast/activation time (s) — NOT ArcanaTime. */
  castTime: number;
  /** AoE radius (ft); 0 for single-target. */
  radius: number;
  /** Cone arc already in DEGREES (callers convert from raw arc themselves). */
  arcDegrees: number;
  /** Local slotted recharge enhancement as a fraction (e.g. 0.95). */
  rechargeEnh: number;
  /** Build-wide recharge as a fraction. Bites only when `rechargeEnh` is non-zero,
   *  where it softens the penalty rather than adding one. Defaults to 0. */
  globalRechargeEnh?: number;
  /** Character level used to interpolate proc damage. */
  buildLevel: number;
  /** The power's `ProcMainTargetOnly` flag — when set, its procs roll
   *  single-target despite an AoE radius; see resolveProcRollGeometry. */
  procsOnlyOnMainTarget?: boolean;
  /** The power's `ProcAllowed` flag. `false` means the power's own recharge is
   *  not a proc window; see powerFiresProcs. */
  procsAllowed?: boolean;
  /** The executed children that roll in a `ProcAllowed kNone` power's place,
   *  each with its own geometry; see `Power.procRollSites`. The WINDOW stays
   *  this power's own `baseRecharge`/`castTime` — measured, not assumed. */
  procRollSites?: ProcRollSite[];
  /** Click / Toggle / Auto. Auto and Toggle roll on the proc's own 10s period
   *  rather than a recharge window; see resolveProcRollSchedule. */
  powerType?: string;
  /** Lifetime (s) of the summoned patch that owns the rolls (a rain, Bonfire,
   *  Tar Patch), from resolveProcPatchDuration. A patch rolls every 10s for as
   *  long as it lives, so one cast of Sleet is worth two rolls — but each is
   *  scored against that 10s period, not against the parent's 60s recharge. */
  patchDuration?: number;
}

/**
 * Sum of `rolls × chance × damage` over every slotted foe-damage proc in the
 * power — the expected proc damage added to one cast. Returns 0 when nothing
 * procs.
 *
 * `rolls` is 1 for everything except a summoned patch, where the parent's cast
 * buys several independent 10s-period rolls instead of one recharge-scored one.
 * For a 15s rain that is 2 rolls at ~18% rather than 1 at 90% — a large drop,
 * and a measured one (`resolveProcRollSchedule`).
 */
export function calculateSlottedProcDamagePerCast(input: SlottedProcDamageInput): number {
  const { slots, baseRecharge, castTime, radius, arcDegrees, rechargeEnh, buildLevel } = input;
  // ProcAllowed kNone (Spring Attack, every pet summon): the power's recharge
  // window is not a proc window, so a damage proc slotted here adds nothing to
  // this cast. Pet summons are the subtle case — the proc does reach the pet
  // via CopyBoosts, but what it deals there is pet damage fired on the pet's
  // schedule, not damage on this activation. Fault and its kin are the other
  // subtle case, and they survive this check: their `procRollSites` children
  // roll in the shell's place.
  if (!powerFiresProcs(input)) return 0;
  // A kNone shell is not a proc window of its own, so a proc slotted here must
  // find a site or pay nothing.
  const shellRollsNothing = input.procsAllowed === false;
  // In a main-target-only power every proc — damage or not — rolls
  // single-target: the AoE radius belongs to a secondary effect (the knockback
  // splash) that the proc never rolls against.
  const shellGeometry =
    resolveProcRollGeometry(input.procsOnlyOnMainTarget, radius, arcDegrees);
  // ONE schedule for every piece, sites or not. A site changes where the proc
  // rolls, never when: the window is the recharge of the power the player
  // pressed. Measured 2026-08-09 on two delegating powers whose children have
  // very different recharges from their parents — Fault paid 37/45 where the
  // parent's window predicts 0.820 and the child's 0.301, Spring Attack 26/28
  // where the parent's caps at 0.9 and the child's predicts 0.434.
  const schedule = resolveProcRollSchedule({
    powerType: input.powerType,
    baseRecharge,
    castTime,
    patchDuration: input.patchDuration,
  });
  let total = 0;
  for (const slot of slots) {
    if (!slot || slot.type !== 'io-set') continue;
    const io = slot as IOSetEnhancement;
    if (!io.isProc) continue;
    const procData = findProcData(io.name, io.setName);
    if (!procData || procData.ppm === null) continue;
    // A foe-damage proc is a Damage effect with a value..valueMax range (Build
    // Up's self-buff Damage carries a duration and no valueMax — excluded).
    const dmg = getProcEffects(procData).find(
      (e) => e.category === 'Damage' && e.value !== undefined && e.valueMax !== undefined,
    );
    if (!dmg || dmg.value === undefined || dmg.valueMax === undefined) continue;
    // Proc DAMAGE scales with the CHARACTER's (combat) level, never the slotted
    // IO's crafted level: a level-21 and a level-50 Touch of Lady Grey deal
    // identical damage on a level-50 character (the "slot the cheapest proc"
    // rule). The IO level governs enhancement *values* and exemplar floor, not
    // the proc payload. interpolateProcDamage clamps to the proc's own
    // levelRange. (@Redlynne report, 2026-06-12 — was using io.level for
    // non-attuned, so Global-IO-Level builds under-counted 10×.)
    const procDmg = interpolateProcDamage(dmg.value, dmg.valueMax, procData.levelRange, buildLevel);
    // Which geometry this piece rolls against: routed by the piece's own
    // `boostsAllowed` — the key `CopyBoosts` filters by when the shell hands
    // its slotting to a child. A piece TWO children could hold (the multi-mez
    // ATO procs in Hypnotizing Lights) has no single roll: it pays nothing
    // here, and the Info panel carries the loud marker (DATA-GAP-REGISTER
    // HC-4).
    let site: ProcRollSite | null;
    try {
      site = resolveProcRollSite(input.procRollSites, procData.boostsAllowed);
    } catch {
      continue;
    }
    // A kNone shell with no site for this piece rolls it nowhere: no child's
    // `BoostsAllowed` can hold it, so `CopyBoosts` hands it to no one.
    if (!site && shellRollsNothing) continue;
    const { radius: areaRadius, arcDegrees: areaArc } = site
      ? resolveProcRollGeometry(
        site.procsOnlyOnMainTarget, site.radius, arcToDegrees(site.arc) || undefined)
      : shellGeometry;
    const procChance = calculateScheduledProcChance(
      procData.ppm, schedule, areaRadius, areaArc, rechargeEnh,
    );
    total += procDmg * procChance * schedule.rolls;
  }
  return total;
}
