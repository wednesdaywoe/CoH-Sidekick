/**
 * The pre-engine TypeScript totals calculation — TEST ONLY. Nothing in the app imports this.
 *
 * It is the independent oracle `src/engine/serverParity.test.ts` diffs the Rust engine
 * against on all three datasets. Unlike a recorded baseline it recomputes from the exported
 * data, so an honest re-export moves both sides together and only a real engine divergence
 * surfaces. That is the whole reason it outlived its callers.
 *
 * It is frozen: it gains no features the engine gains. When the two legitimately part ways,
 * the gate's UNMAPPED set records it — do not "fix" this file to agree.
 */

import type { Build, Enhancement, IncarnateActiveState, IncarnateBuildState, IOSetEnhancement } from '@/types';
import type { ProcSettings } from '@/stores/uiStore';
import { isSelfDirectedEffect } from '@/types';
import { AT_INHERENT_CONDITIONAL_IDS } from '@/utils/conditional-effects';
import { getBuffPetSources, BUFF_PET_TOGGLE_ID, type BuffPetSource } from './buff-pet-auras';
import type { SummonEffect } from '@/types/power';
import { withoutIllegalSlots } from '@/utils/build-enhancement-validation';
import { stanceAdjusterOverrides, STANCE_GROUPS, activeStanceOptionId, getAccolades, accoladeId } from '@/data';
import type { AccoladePower } from '@/data';
import { getIOSet, getAlphaEffects, getDestinyEffects, getDestinyEffectsAtTime, getDestinySustainedFloorTime, getDestinyBoostsAllowed, applyAlphaToDestiny, getHybridEffects, getLoreEffects, getGenesisEffects, findProcData, getProcEffects, isProcAlwaysOn, calculateAutoToggleProcsPerMinute, calculateProcChance, arcToDegrees, getProcControlType, DEFAULT_STACK_COUNT, resolveProcContribution, procOverrideKey } from '@/data';
import type { DestinyEffects, GenesisEffects } from '@/data';
import type { ProcEffect } from '@/data/proc-data';
import { getTableValue } from '@/data/at-tables';
import { getBaseToHit, getCombatModifier } from '@/data/purple-patch';
import { getPowerPool } from '@/data/power-pools';
import { getEpicPool } from '@/data/epic-pools';
import { getPowerset } from '@/data/powersets';
import { getArchetype } from '@/data/archetypes';
import { repelProtectionValue } from './repel-protection';
import type { ArchetypeId } from '@/types';
import { calculateSetBonuses, getStatBreakdown, trackBonus, createBonusTracking, type AggregatedBonuses, type StatBreakdownItem, type BuildPowers } from './set-bonuses';
import { createEmptyStats, getBaselineHealth, type CharacterStats } from './stats';
import { combineWithAlphaED, filterAlphaByAllowedEnhancements, BASE_RECOVERY_RATE, BASE_REGEN_RATE, type EnhancementBonuses } from './enhancement-values';
import { toHitBuffValue, damageBuffValue, resistanceBuffValue, resistanceSelfDebuffValue, defenseBuffValue, defenseBuffSuppressibleValue, defenseBuffIsTeamOnly, maxHPBuffValue, regenBuffValue, recoveryBuffValue, movementBuffValue, kbProtectionValue, accuracyBuffValue, rechargeBuffValue, rangeBuffValue, perceptionBuffValue, enduranceDiscountValue, maxEndBuffValue, elusivityValue, mezSlotValue, mezResistanceValue, tauntPlacateValue, debuffResistanceValue, selfSlowValue, selfMovementCapDebuffValue, movementAxisSubType, damageBuffIsDefianceOnly, selfDamageDebuffValue, selfRechargeDebuffValue, stealthValue, absorbValue, absorbMaxHPFractionValue, stackCapOf, buffStack, DEBUFF_RESISTANCE_STACK } from '@/data/core/atom-query';
import type { MovementBuffEntry, StackFamily } from '@/data/core/atom-query';
import type { EncodedAtom } from '@/data/core/atomic-effect';
import { calculateVigilanceDamageBonus, calculateFuryDamageBonus } from './inherents';
import { getEffectiveLevel, areIncarnatesSuppressed } from './effective-level';
import { computeModeSuppression, type ModeCarrier } from '@/utils/mode-suppression';
import { isCalcDebugEnabled, debugBuildContext, debugSetBonuses, debugAlphaBonuses, debugGroup, debugGroupEnd, debugFormula, debugAccolade, debugHitChance, debugFinalStats, debugNetEndurance, debugEnd } from '@/utils/calc-debug';
import type { ActivePowerEffect, CalculationOptions, CharacterCalculationResult, DashboardStatBreakdown, GlobalBonuses, MezScaled, PowerWithToggle, ScalarOrScaled, StatSource, StrengthBuffs } from './character-totals';
import { adjustForStackCap, carries_combat_debuff, collectStrengthBuffs, createEmptyGlobalBonuses, emptyStrengthBuffs, getAlphaEdBypassBonuses, getAlphaEnhancementBonuses, mezSourceFor, resolveScaledEffect, syntheticEffects } from './character-totals';

// ============================================
// STAT NAME MAPPING
// ============================================

/**
 * Map set bonus stat names to our global bonus property names
 */
const STAT_TO_GLOBAL: Record<string, keyof GlobalBonuses> = {
  // Offense
  damage: 'damage',
  accuracy: 'accuracy',
  tohit: 'toHit',
  recharge: 'recharge',
  endrdx: 'endurance',
  range: 'range',

  // Defense positional
  defmelee: 'defMelee',
  defranged: 'defRanged',
  defaoe: 'defAoE',

  // Defense typed
  defsmashing: 'defSmashing',
  deflethal: 'defLethal',
  deffire: 'defFire',
  defcold: 'defCold',
  defenergy: 'defEnergy',
  defnegative: 'defNegative',
  defpsionic: 'defPsionic',
  deftoxic: 'defToxic',

  // Combined defense (S/L, F/C, E/N)
  defsl: 'defSmashing', // Will apply to both
  deffc: 'defFire',
  defen: 'defEnergy',

  // Resistance
  ressmashing: 'resSmashing',
  reslethal: 'resLethal',
  resfire: 'resFire',
  rescold: 'resCold',
  resenergy: 'resEnergy',
  resnegative: 'resNegative',
  respsionic: 'resPsionic',
  restoxic: 'resToxic',

  // Combined resistance
  ressl: 'resSmashing',
  resfc: 'resFire',
  resen: 'resEnergy',

  // Recovery & Health
  maxhp: 'maxHP',
  maxend: 'maxEndurance',
  regeneration: 'regeneration',
  recovery: 'recovery',

  // Movement
  runspeed: 'runSpeed',
  jumpheight: 'jumpHeight',
  flyspeed: 'flySpeed',

  // Special
  mezresist: 'mezResist',
  healother: 'healOther',
  threatlevel: 'threatLevel',

  // Offensive mez/control duration (normalized keys are lowercased+alpha-only)
  immobilizeduration: 'immobilizeDuration',
  holdduration: 'holdDuration',
  stunduration: 'stunDuration',
  sleepduration: 'sleepDuration',
  confuseduration: 'confuseDuration',
  terrorduration: 'terrorDuration',

  // Mez Protection (from IO set bonuses — value is stored as %, divide by 100 for mag)
  kbprotection: 'protKnockback',

  // Debuff Resistance (from IO set bonuses)
  debuffresistrecharge: 'debuffResistRecharge',
  debuffresistslow: 'debuffResistSlow',

  // Knockback Resistance (from IO set bonuses)
  kbresistance: 'mezResistKnockback',

  // Perception radius buff (from IO set bonuses — Rectified Reticle), same
  // % global as in-power +Perception buffs.
  perceptionradius: 'perceptionRadius',
};

/**
 * Stats that should apply to paired types (S/L, F/C, E/N)
 */
const PAIRED_STATS: Record<string, string[]> = {
  defsl: ['defSmashing', 'defLethal'],
  deffc: ['defFire', 'defCold'],
  defen: ['defEnergy', 'defNegative'],
  ressl: ['resSmashing', 'resLethal'],
  resfc: ['resFire', 'resCold'],
  resen: ['resEnergy', 'resNegative'],
  // +Res(Recharge Debuff) set bonuses also provide Slow resistance
  debuffresistrecharge: ['debuffResistRecharge', 'debuffResistSlow'],
};

// ============================================
// SET BONUS PROCESSING
// ============================================

/**
 * Convert set bonus aggregated values to global bonuses
 */
function applySetBonusesToGlobal(
  aggregated: AggregatedBonuses,
  global: GlobalBonuses
): void {
  for (const [stat, value] of Object.entries(aggregated)) {
    const normalizedStat = stat.toLowerCase().replace(/[^a-z]/g, '');

    // Check for paired stats (e.g., smashing/lethal resistance)
    if (PAIRED_STATS[normalizedStat]) {
      const paired = PAIRED_STATS[normalizedStat];
      for (const pairStat of paired) {
        const key = pairStat as keyof GlobalBonuses;
        if (key in global) {
          global[key] += value;
        }
      }
    } else {
      // Direct mapping
      const key = STAT_TO_GLOBAL[normalizedStat];
      if (key && key in global) {
        // IO set KB protection is stored as percentage (400 = Mag 4.0)
        const scale = normalizedStat === 'kbprotection' ? 0.01 : 1;
        global[key] += value * scale;
      }
    }
  }
}

/**
 * Build stat breakdown from set bonus tracking
 */
function buildStatBreakdown(
  breakdownItems: StatBreakdownItem[]
): DashboardStatBreakdown {
  const sources: StatSource[] = [];
  let cappedCount = 0;
  let total = 0;

  for (const item of breakdownItems) {
    // Counted sources (first 5) — these are active and NOT capped
    for (const tracked of item.sources) {
      sources.push({
        name: tracked.name,
        value: item.value,
        type: 'set-bonus',
        capped: false,
        powerName: tracked.powerName,
      });
    }

    // Rejected sources (6th+) — these exceeded the Rule of 5 and are NOT counted
    for (const tracked of item.rejectedSources) {
      sources.push({
        name: tracked.name,
        value: item.value,
        type: 'set-bonus',
        capped: true,
        powerName: tracked.powerName,
      });
      cappedCount++;
    }

    total += item.total;
  }

  return {
    total,
    base: 0,
    sources,
    cappedSources: cappedCount,
  };
}

/** targetType values where the power cannot be cast on self — the buff goes
 *  to allies only and must not contribute to the caster's totals. Covers
 *  every ally-target string we've seen in HC and Rebirth bin exports plus
 *  the legacy "Ally"/"Ally (Alive)"/"Teammate" labels Mids data uses.
 *  Powers that auto-apply to the caster (Recovery Aura, Regeneration Aura,
 *  Farsight, etc.) are tagged Self and are unaffected. */
const ALLY_ONLY_TARGET_TYPES = new Set([
  'ally',
  'ally (alive)',
  'teammate',
  'dead teammate',
  'friend',
  'deadplayerfriend',
  'deadoraliveleaguemate',
]);

/**
 * Sum per-second endurance cost from active toggles using the canonical CoH
 * divisor formula:
 *
 *     actualCost = baseEndPerSec / (1 + slotEndRdx + globalEndDisc/100)
 *
 * Runs AFTER all global EndDisc sources have been aggregated (set bonuses,
 * active-power discounts like Conserve Power, Hybrid Support T4) so the
 * divisor sees the full sum in one shot. This replaces the older two-step
 * pattern (per-toggle cost using only slot enhancement, then a post-hoc
 * `cost *= (1 - global/100)` rescale), which used the wrong linear formula
 * and silently dropped any discount sources that hadn't been aggregated yet.
 */
function applyToggleEndCosts(
  powers: PowerWithToggle[],
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  buildLevel: number,
  alphaBonuses: EnhancementBonuses = {},
  alphaEdBypass: EnhancementBonuses = {},
  exemplarLevel?: number,
): void {
  const globalEndDiscDecimal = (global.endurance || 0) / 100;
  for (const power of powers) {
    if (power.powerType?.toLowerCase() !== 'toggle' || !power.isActive) continue;
    if (power.targetType && ALLY_ONLY_TARGET_TYPES.has(power.targetType.toLowerCase())) continue;

    // `effects.enduranceCost` RETIRED BPORT11: 0 powers on any of the four forks carry it, so
    // the branch never fired and `stats.endurance / activatePeriod` has always been the source.
    let baseEndPerSec = 0;
    if (power.stats?.endurance) {
      const activatePeriod = power.stats.activatePeriod ?? 0.5;
      baseEndPerSec = activatePeriod > 0 ? power.stats.endurance / activatePeriod : 0;
    }
    if (baseEndPerSec <= 0) continue;

    let enhBonuses: EnhancementBonuses;
    if (power.slots && power.slots.length > 0) {
      enhBonuses = combineWithAlphaED(
        { name: power.name, slots: power.slots, allowedEnhancements: power.allowedEnhancements },
        buildLevel,
        getIOSet,
        alphaBonuses,
        alphaEdBypass,
        exemplarLevel
      );
    } else {
      enhBonuses = filterAlphaByAllowedEnhancements(alphaBonuses, power.allowedEnhancements);
    }
    const slotEndRdx = enhBonuses.endurance || 0;

    const divisor = 1 + slotEndRdx + globalEndDiscDecimal;
    const actualCost = baseEndPerSec / divisor;
    global.toggleEndCost += actualCost;
    addToBreakdown(breakdown, 'toggleEndCost', {
      name: power.name,
      value: actualCost,
      type: 'active-power',
    });
  }
}

/**
 * Apply bonuses from active toggle powers
 * Enhancement bonuses are now factored in to boost the base power values
 * Alpha incarnate bonuses are added to the enhancement bonuses for applicable aspects
 */
/**
 * One stealth-radius contribution, gathered across active powers and procs and
 * resolved together by resolveStealthRadius once every source is known.
 *
 * `stackKey` is the binary stealth-stacking group. Contributions sharing a
 * non-null key mutually suppress — only the largest radius in that key applies
 * (all "NictusFX" today: Stealth, Super Speed, Shinobi-Iri, the cloak toggles).
 * A null key stacks additively (Stalker Hide, IO procs, and Rebirth stealth,
 * whose Parse6 export can't resolve the key — a documented cross-server gap).
 */
interface StealthContribution {
  stackKey: string | null;
  /** PvE radius (feet); 0 if this source has no PvE component. */
  pve: number;
  /** PvP radius (feet); 0 if this source has no PvP component. */
  pvp: number;
  sourceName: string;
  type: 'active-power' | 'proc';
  powerName?: string;
}

/**
 * Commit the gathered stealth contributions to global.stealthRadiusPvE/PvP.
 * Per CoH mechanics each keyed (suppress) group contributes only its largest
 * radius; null-key contributions stack additively; the grand total is the sum.
 * Every contributor is recorded in the breakdown — suppressed (non-winning)
 * keyed entries are flagged `capped` so the tooltip explains why the displayed
 * total isn't the naive sum.
 */
function resolveStealthRadius(
  contribs: StealthContribution[],
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
): void {
  const axes = [
    { axis: 'pve' as const, key: 'stealthRadiusPvE' as const },
    { axis: 'pvp' as const, key: 'stealthRadiusPvP' as const },
  ];
  for (const { axis, key } of axes) {
    // Largest radius per keyed (suppress) group.
    const groupMax = new Map<string, number>();
    for (const c of contribs) {
      const v = c[axis];
      if (v <= 0 || !c.stackKey) continue;
      const cur = groupMax.get(c.stackKey);
      if (cur === undefined || v > cur) groupMax.set(c.stackKey, v);
    }
    let total = 0;
    for (const v of groupMax.values()) total += v;        // one winner per group
    for (const c of contribs) {                            // + additive (null-key)
      const v = c[axis];
      if (v > 0 && !c.stackKey) total += v;
    }
    global[key] = total;
    // Breakdown: list every contributor; mark suppress-group losers as
    // `suppressed` (NOT `capped`) — stealth mutual suppression is normal game
    // mechanics, not a Rule of 5 violation. Using `capped` here previously
    // flagged the losing stealth power with a spurious Rule-of-5 warning ring.
    for (const c of contribs) {
      const v = c[axis];
      if (v <= 0) continue;
      const suppressed = !!c.stackKey && v < (groupMax.get(c.stackKey) ?? 0);
      addToBreakdown(breakdown, key, {
        name: c.sourceName,
        value: v,
        type: c.type,
        powerName: c.powerName,
        ...(suppressed ? { suppressed: true } : {}),
      });
    }
  }
}

/**
 * One movement-percent contribution (run/fly/jump speed, jump height),
 * gathered across active powers and resolved together by
 * resolveMovementTotals once every source is known.
 *
 * `stackKey` is the binary suppress group (StackType kSuppress +
 * StackByAttribAndKey): active powers sharing a key mutually suppress per
 * stat — only the strongest applies. kTravelBuff covers Combat Jumping /
 * Super Jump / Super Speed's momentum effects / Fly / Ninja Run etc., which
 * previously all stacked additively. A null key stacks (Sprint, Swift,
 * Hurdle, set bonuses — and all of Rebirth, whose i25-era data predates the
 * travel suppress groups).
 *
 * `suppressible` marks buffs the game shuts off in combat (`Suppress
 * ActivateAttackClick` — Super Speed's run buff, Super Jump's jump buffs,
 * Fly's speed). Combat Jumping / Hover carry no suppress events and persist,
 * which is their whole point.
 */
interface MovementContribution {
  stat: 'runSpeed' | 'flySpeed' | 'jumpSpeed' | 'jumpHeight';
  /** Resolved buff percent (post AT-table, post enhancement). */
  value: number;
  stackKey: string | null;
  suppressible: boolean;
  sourceName: string;
  type: 'active-power';
}

/**
 * Commit the gathered movement contributions to global run/fly/jump totals.
 * In combat mode, suppressible buffs contribute nothing. Each keyed
 * (suppress) group contributes only its strongest member; null-key
 * contributions stack additively. Every contributor is recorded in the
 * breakdown — suppressed entries (combat-suppressed or group losers) are
 * flagged `capped` so the tooltip explains why the total isn't the naive sum.
 */
function resolveMovementTotals(
  contribs: MovementContribution[],
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  combatMode?: boolean,
): void {
  const stats = ['runSpeed', 'flySpeed', 'jumpSpeed', 'jumpHeight'] as const;
  for (const stat of stats) {
    const all = contribs.filter((c) => c.stat === stat && c.value !== 0);
    if (all.length === 0) continue;
    const eligible = all.filter((c) => !(combatMode && c.suppressible));
    // Strongest per keyed (suppress) group; null-key entries stack.
    const groupMax = new Map<string, number>();
    for (const c of eligible) {
      if (!c.stackKey) continue;
      const cur = groupMax.get(c.stackKey);
      if (cur === undefined || c.value > cur) groupMax.set(c.stackKey, c.value);
    }
    let total = 0;
    for (const v of groupMax.values()) total += v;
    for (const c of eligible) {
      if (!c.stackKey) total += c.value;
    }
    global[stat] += total;
    for (const c of all) {
      const combatSuppressed = !!(combatMode && c.suppressible);
      const groupLoser =
        !combatSuppressed && !!c.stackKey && c.value < (groupMax.get(c.stackKey) ?? 0);
      addToBreakdown(breakdown, stat, {
        name: c.sourceName,
        value: c.value,
        type: c.type,
        // `suppressed`, NOT `capped`: travel-buff mutual suppression / combat
        // suppression is normal game mechanics, not a Rule of 5 violation — so
        // the row dims but no Rule-of-5 warning fires (see StatSource.suppressed).
        ...(combatSuppressed || groupLoser ? { suppressed: true } : {}),
      });
    }
  }
}

/** Read the travel-suppression metadata off a scaled movement effect. */
function movementMeta(effect: ScalarOrScaled | undefined): { stackKey: string | null; suppressible: boolean } {
  if (typeof effect !== 'object' || effect === null) return { stackKey: null, suppressible: false };
  const o = effect as { stackKey?: string; suppressible?: boolean };
  return { stackKey: o.stackKey ?? null, suppressible: !!o.suppressible };
}

function applyActivePowerBonuses(
  powers: PowerWithToggle[],
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  buildLevel: number,
  archetypeId: string,
  alphaBonuses: EnhancementBonuses = {},
  alphaEdBypass: EnhancementBonuses = {},
  targetsHitValues: Record<string, number> = {},
  exemplarLevel?: number,
  combatMode?: boolean,
  strengthBuffs: StrengthBuffs = emptyStrengthBuffs(),
  stealthContribs: StealthContribution[] = [],
  // MaxHP-fraction absorb contributions (Wild Bastion etc.) are collected here
  // and resolved to absolute HP by the caller once the build's final Max HP —
  // including accolades and incarnates — is known. Flat-HP absorb is added to
  // global.absorb inline below.
  absorbFractionContribs: { name: string; fraction: number }[] = [],
  // Movement contributions are gathered here and committed by
  // resolveMovementTotals once every active-power pass has run — travel
  // suppress groups (kTravelBuff) take their strongest member instead of
  // summing, and combat mode drops suppressible buffs.
  movementContribs: MovementContribution[] = [],
  // Resistible self-directed -Res debuffs (Offensive Adaptation) — collected
  // here and applied by the caller AFTER powers + set bonuses are fully summed,
  // because each is reduced by the caster's own same-type resistance.
  resSelfDebuffContribs: { name: string; type: string; nominal: number; resistible: boolean }[] = []
): void {
  for (const power of powers) {
    // Auto powers are always active; others require explicit isActive toggle
    const isAuto = power.powerType?.toLowerCase() === 'auto';
    if (!(isAuto || power.isActive)) continue;

    // Skip ally-only buffs — the caster does not benefit from these
    // (e.g. Speed Boost, Fortitude — "you cannot use this power on yourself")
    if (power.targetType && ALLY_ONLY_TARGET_TYPES.has(power.targetType.toLowerCase())) continue;

    // Calculate enhancement bonuses for this power. The Alpha slot's
    // tier-specific ED-bypass mechanic only meaningfully changes the result
    // when both slotted IOs *and* alpha buff the same aspect — in that case
    // a naive post-ED add overstates enhancement (the slotted portion is
    // already at the ED cap, and alpha's full value lands on top instead
    // of being partially diminished). combineWithAlphaED splits alpha into
    // an ED-subject portion that joins the raw IO total before ED and a
    // bypass portion that lands after.
    let enhBonuses: EnhancementBonuses;
    if (power.slots && power.slots.length > 0) {
      enhBonuses = combineWithAlphaED(
        { name: power.name, slots: power.slots, allowedEnhancements: power.allowedEnhancements },
        buildLevel,
        getIOSet,
        alphaBonuses,
        alphaEdBypass,
        exemplarLevel
      );
    } else {
      // No slots → only Alpha contributes. Same gate as combineWithAlphaED
      // applies: don't apply Alpha bonuses for aspects the power doesn't
      // accept. Without this filter, e.g. Tactical Training: Assault
      // (allowedEnhancements: EndRdx + Recharge only) would still receive
      // Alpha Intuition's +33% Damage, inflating the displayed +Damage
      // toggle bonus.
      enhBonuses = filterAlphaByAllowedEnhancements(alphaBonuses, power.allowedEnhancements);
    }

    // Toggle endurance cost calc is now handled by applyToggleEndCosts, which
    // runs after all global EndDisc sources (set bonuses, active powers,
    // incarnates) have been aggregated — so the divisor formula sees the
    // complete picture in one pass instead of needing a post-hoc rescale.

    // BPORT13 retired an `if (!power.effects) continue;` that stood here.
    //
    // The deref it guarded is gone: absorb was the last family in this pass still reading the
    // bag and BPORT11's carry took it (BPORT7 A2). Every arm below now reads the atom-native
    // readers, or the named synthetic channel ({@link syntheticEffects}) for a contribution the
    // totals built for itself. The guard outlived its subject, and once BPORT7 emptied the bag
    // it stopped being a skip for atom-less legacy powers and became a skip for ALL of them —
    // this oracle contributed nothing to any total, and `serverParity` diffed the engine
    // against zeros on all four forks. A frozen oracle is still allowed to lose a dead guard;
    // what it may not do is gain a feature, and removing this adds none.
    // BPORT11's stacking selector, for the families that have crossed. The atoms carry the
    // depth a family self-stacks to, so `stackCapOf` answers with one number what the retired
    // `stacksLinear` / `maxStacks` / `stackCaps` triple answered from three bag slots:
    // membership AND depth. Measured over 14,249 powers before the first carry — wherever
    // both sides say a family stacks they name the same depth, 0 disagreements at any site.
    // Applied on the far side of an `atom ?? synthetic` seam, so a synthetic contribution
    // stacks on the same terms a real power's does.
    const stack = (value: ScalarOrScaled, family: StackFamily): ScalarOrScaled =>
      adjustForStackCap(value, targetsHitValues[power.internalName], stackCapOf(power, family), power);
    // The build's `Class_*` token, and the power as this build's class sees it. A protection
    // atom can fork on `casterArchetypes` (AT-FORK-1), and a build-agnostic read returns
    // undefined exactly where the build's own arm exists — which the bag used to paper over,
    // because the converter wrote one slot per power with no fork in it. Absent when no
    // archetype is selected, which is the raw view and the pre-fork behaviour.
    // `archetypeId` is a bare string on this signature (it is whatever the build carries), and
    // the registry lookup answers `undefined` for anything it does not hold — which is the
    // no-archetype case already handled, so the cast widens nothing.
    const playerClassToken = archetypeId
      ? getArchetype(archetypeId as ArchetypeId)?.stats?.className
      : undefined;
    const mezSource = mezSourceFor(power, playerClassToken);
    const _debugEnabled = isCalcDebugEnabled();
    // Snapshot global bonuses before this power for diff logging
    const _debugBefore = _debugEnabled ? { ...global } : null;

    // ToHit buff (stored as decimal, convert to percentage)
    // Enhanced by ToHit enhancements.
    // Plan B Slice 1: sourced from atoms (scale + perTarget reconstructed by
    // `toHitBuffValue`, verified bag-equal by scripts/planb-shadow-pertarget.cjs);
    // BPORT11: atom-native, synthetic arm kept (25 credited mints — Dual Blades' Blinding
    // Feint in DD Status Mode 2). All 874 carriers agree with the bag.
    const tohitBuff = toHitBuffValue(power) ?? syntheticEffects(power)?.tohitBuff;
    if (tohitBuff !== undefined) {
      const enhMultiplier = 1 + (enhBonuses.tohit || 0) + strengthBuffs.toHit;
      const adjustedBuff = stack(tohitBuff as ScalarOrScaled, buffStack('ToHit', 'enhanceable'));
      const value = resolveScaledEffect(adjustedBuff, archetypeId, buildLevel) * 100 * enhMultiplier;
      global.toHit += value;
      addToBreakdown(breakdown, 'toHit', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // ToHit buff that ignores strength (IgnoreStrength) — NOT boosted by ToHit
    // enh or global +ToHit (e.g. Bio Armor Environmental Adaptation's +ToHit).
    // Synthetic arm kept: 37 credited mints (Bio Armor's Environmental Modification in
    // Offensive Adaptation). All 48 real carriers agree.
    const tohitBuffUnenhanced = toHitBuffValue(power, { ignoreStrength: true })
      ?? syntheticEffects(power)?.tohitBuffUnenhanced;
    if (tohitBuffUnenhanced !== undefined) {
      const adjusted = stack(tohitBuffUnenhanced as ScalarOrScaled, buffStack('ToHit', 'unenhanced'));
      const value = resolveScaledEffect(adjusted, archetypeId, buildLevel) * 100;
      global.toHit += value;
      addToBreakdown(breakdown, 'toHit', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Accuracy buff (a flat +Accuracy self-buff like Focused Accuracy, stored as
    // a decimal). Additive into global.accuracy alongside set bonuses. Not
    // enhanced — accuracy enhancements boost attack-roll accuracy, not the
    // buff power's own +Accuracy (these powers don't slot accuracy-buff enh).
    // BPORT11: atom-native, with no synthetic arm behind it — `accuracyBuffValue` mirrors the
    // converter's own gate and agrees with the bag on all 56 carriers of all four forks, and no
    // reachable conditional or buff-pet aura mints the slot, so there is nothing left to fall
    // back to.
    const accuracyBuffSlot = accuracyBuffValue(power);
    if (accuracyBuffSlot !== undefined) {
      const adjustedBuff = stack(accuracyBuffSlot as ScalarOrScaled, buffStack('Accuracy'));
      const value = resolveScaledEffect(adjustedBuff, archetypeId, buildLevel) * 100;
      global.accuracy += value;
      addToBreakdown(breakdown, 'accuracy', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Damage buff (e.g. Assault, Build Up, Fulcrum Shift).
    // NOT enhanced: a +Damage buff is a fixed buff to the target's Damage
    // strength — Damage enhancements and global +Damage raise the OUTPUT of
    // attack powers, not the magnitude of a buff, and no "Damage Buff"
    // enhancement exists in CoH. (Same as accuracyBuff / tohitBuffUnenhanced.)
    // Plan B Slice 2: sourced from atoms (`damageBuffValue` collapses the
    // per-damage-type explosion and reconstructs perTarget — Soul Drain's per-foe
    // slider, Fulcrum Shift's redirect increment); `?? effects.damageBuff` keeps
    // an atom-less legacy power on the bag. Verified bag-equal by
    // scripts/planb-shadow-pertarget.cjs.
    //
    // BPORT11: synthetic arm kept (48 credited mints — Devices' Targeting Drone out of combat).
    // A power whose whole +damage buff is DEFIANCE takes neither path. The atom read already
    // rejects the tagged atoms, but the bag slot held the same value, and an absent atom read
    // is indistinguishable from the atom-less case the fallback served — so the rejection has
    // to be spoken here too or it is undone one line later. Measured: 68 powers carry a
    // `damageBuff` the reader declines and every single one is defiance-only, all of them
    // Blaster secondaries whose Defiance the bag was crediting as a permanent +damage buff.
    const damageBuff = damageBuffIsDefianceOnly(power)
      ? undefined
      : (damageBuffValue(power) ?? syntheticEffects(power)?.damageBuff);
    if (damageBuff !== undefined) {
      const adjustedBuff = stack(damageBuff as ScalarOrScaled, buffStack('DamageBuff'));
      const value = resolveScaledEffect(adjustedBuff, archetypeId, buildLevel) * 100;
      global.damage += value;
      addToBreakdown(breakdown, 'damage', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Damage debuff (self-penalty, e.g. Granite Armor -30% damage)
    // Only applied when the value is self-directed (toWho:'Self') — most
    // damageDebuff effects target enemies. Unenhanceable — self-debuffs are not
    // boosted by slotted enhancements.
    // Skip crash debuffs: if a power also has damageBuff, the debuff is a crash effect
    // (e.g., Rage: 120s buff + 10s crash) and should not count as sustained damage
    // BPORT11: atom-native (`selfDamageDebuffValue` mirrors the converter's self arm, which is
    // the only arm this pass ever read). All 43 self-tagged carriers agree, and no conditional
    // mints a self-directed one, so there is no synthetic arm to keep.
    const selfDmgDebuff = selfDamageDebuffValue(power);
    if (selfDmgDebuff !== undefined && damageBuff === undefined) {
      const value = resolveScaledEffect(selfDmgDebuff as ScalarOrScaled, archetypeId, buildLevel) * -100;
      global.damage += value;
      addToBreakdown(breakdown, 'damage', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Defense from active powers
    // Enhanced by Defense enhancements
    // Power data uses either "defense" or "defenseBuff" key for defense effects
    // Plan B Slice 4: the "defenseBuff" half is sourced from atoms
    // (`defenseBuffValue` — always-on Defense atoms, `suppressible:false`, restricted
    // to the 11 standard globals; Invincibility's per-foe +Def via the shared perTarget
    // stamp); `?? effects.defenseBuff` keeps an atom-less legacy power on the bag. The
    // BPORT11: atom-native, fork-resolved, synthetic arm kept (64 credited mints — Bio
    // Armor's Environmental Modification in Defensive Adaptation). 1,143 carrier views agree.
    //
    // The pet-aura channel moved WITH the arm rather than beside it. `effects.defense` had one
    // supplier and it was not the converter — 0 powers on any fork carry the slot, and the
    // only writer is `buffPetAuraEffects` a few hundred lines down. So the fold now mints
    // `defenseBuff` (canonical re-keyed its own copy the same way) and the two spellings
    // collapse into the branch that already had to exist for the conditionals.
    //
    // A team-only power (Grant Cover) has to skip the synthetic arm as well as the atom half,
    // or the fallback hands back the number the applier just declined to give. This reads the
    // `target ≠ source` clause off the atoms; it used to read a hand-written
    // `defenseBuffExcludesSelf` override flag, which existed on two of the three forks and so
    // answered wrong on the third (TEAMBUFF-1). It abstains on an atom-less power, so a
    // synthetic contribution still reaches the gate.
    // Verified bag-equal by scripts/planb-shadow-defense.cjs.
    const defenseEffects = defenseBuffIsTeamOnly(power)
      ? undefined
      : (defenseBuffValue(mezSource) ?? syntheticEffects(power)?.defenseBuff);
    if (defenseEffects && typeof defenseEffects === 'object') {
      const enhMultiplier = 1 + (enhBonuses.defense || enhBonuses.defenseBuff || 0) + strengthBuffs.defense;
      for (const [type, value] of Object.entries(defenseEffects)) {
        const adjustedDef = stack(value, buffStack('Defense', 'either', type));
        const percentage = resolveScaledEffect(adjustedDef, archetypeId, buildLevel) * 100 * enhMultiplier;
        const key = `def${capitalizeFirst(type)}` as keyof GlobalBonuses;
        if (key in global) {
          global[key] += percentage;
          addToBreakdown(breakdown, key, {
            name: power.name,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // Suppressible defense from stealth/travel powers (skipped in combat mode)
    // Plan B Slice 4: sourced from atoms (`defenseBuffSuppressibleValue` — the
    // `suppressible:true` complement of the always-on half above). BPORT11: atom-only — no
    // conditional or pet aura mints the slot. 97 carrier views agree and 12 are gains, all of
    // them Personal Force Field on rebirth and Thunderspy, whose +7.5 suppressible defence the
    // bag on those forks never carried at all. Verified bag-equal by
    // scripts/planb-shadow-defense.cjs.
    const suppressibleDefense = defenseBuffSuppressibleValue(power);
    if (!combatMode && suppressibleDefense && typeof suppressibleDefense === 'object') {
      const enhMultiplier = 1 + (enhBonuses.defense || enhBonuses.defenseBuff || 0) + strengthBuffs.defense;
      for (const [type, value] of Object.entries(suppressibleDefense)) {
        const adjustedDef = stack(value, buffStack('Defense', 'either', type));
        const percentage = resolveScaledEffect(adjustedDef, archetypeId, buildLevel) * 100 * enhMultiplier;
        const key = `def${capitalizeFirst(type)}` as keyof GlobalBonuses;
        if (key in global) {
          global[key] += percentage;
          addToBreakdown(breakdown, key, {
            name: `${power.name} (suppressible)`,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // Resistance from active powers
    // Enhanced by Resistance enhancements
    // Plan B Slice 3: sourced from atoms (`resistanceBuffValue` rebuilds each
    // damage type's { scale, perTarget } — Bio Armor's per-foe Evolving Armor —
    // restricted to the 8 standard resistance globals). BPORT11: fork-resolved through
    // {@link mezSource}, so rebirth's unanimous-fork Tough reads its 1.5 smashing/lethal from
    // the build's own arm instead of falling through; synthetic arm kept (65 credited mints —
    // Temporal Manipulation's Time Lord). 1,606 carrier views agree with no divergence.
    // Verified bag-equal by scripts/planb-shadow-resistance.cjs.
    const res = resistanceBuffValue(mezSource) ?? syntheticEffects(power)?.resistance;
    if (res && typeof res === 'object') {
      const enhMultiplier = 1 + (enhBonuses.resistance || 0);
      for (const [type, value] of Object.entries(res)) {
        const adjustedRes = stack(value, buffStack('Resistance', 'either', type));
        const percentage = resolveScaledEffect(adjustedRes, archetypeId, buildLevel) * 100 * enhMultiplier;
        const key = `res${capitalizeFirst(type)}` as keyof GlobalBonuses;
        if (key in global) {
          global[key] += percentage;
          addToBreakdown(breakdown, key, {
            name: power.name,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // Self-directed -Resistance penalty (toWho:'Self') — Bio Armor Offensive
    // Adaptation's -7.5% Res(all) trade-off. Most resistanceDebuff entries are
    // enemy-facing (they don't touch the caster's totals); only the self-tagged
    // ones subtract from the player's own resistance. Unenhanceable, and stored
    // as a positive magnitude by the converter (makeEffect uses Math.abs), so we
    // negate here.
    //
    // CoH mitigates a RESISTIBLE -Res debuff by the caster's own resistance to
    // that type: effective = nominal × (1 − R_type). Verified in-game — a -7.5%
    // Smashing debuff drops a 39.75% total by only 4.52% (= 7.5 × (1 − 0.3975)),
    // and the breakdown still shows the nominal -7.5%. Because R must include
    // EVERY resistance source (powers + IO set bonuses, both summed across all
    // active-power passes), we can't mitigate inline here — defer to the caller
    // via `resSelfDebuffContribs` and apply once the totals are complete. An
    // IgnoreResistance (`resistible === false`) self-debuff applies flat.
    // Plan B Slice 3: sourced from atoms (`resistanceSelfDebuffValue` — the
    // self-directed −Res atoms only, per standard type). BPORT11: fork-resolved, synthetic arm
    // kept (8 credited mints — rebirth's Pain Absorption levels), and the `isSelfDirectedEffect`
    // filter below still separates the self penalty from co-slotted foe debuffs on either arm.
    // All 17 self-tagged carrier views agree.
    const resSelfDebuff = resistanceSelfDebuffValue(mezSource)
      ?? syntheticEffects(power)?.resistanceDebuff;
    if (resSelfDebuff && typeof resSelfDebuff === 'object') {
      for (const [type, value] of Object.entries(resSelfDebuff)) {
        if (!isSelfDirectedEffect(value)) continue;
        const key = `res${capitalizeFirst(type)}` as keyof GlobalBonuses;
        if (!(key in global)) continue;
        const nominal = resolveScaledEffect(value as ScalarOrScaled, archetypeId, buildLevel) * 100 * -1;
        // The atom path carries the `resistible` flag; a SYNTHETIC contribution ({scale,toWho})
        // does not — default resistible (the only known self -Res, Offensive Adaptation, is
        // resistible, and CoH resists -Res by default). The default outlives the data seam
        // because the conditional half still reaches here without the flag.
        const resistible = (typeof value === 'object' && value !== null && 'resistible' in value)
          ? (value as { resistible?: boolean }).resistible !== false
          : true;
        resSelfDebuffContribs.push({ name: power.name, type: type.toLowerCase(), nominal, resistible });
      }
    }

    // Debuff Resistance from active powers
    // Defense Debuff Resistance is enhanced by Defense enhancements
    // BPORT11: atom-native, fork-resolved (rebirth Acrobatics' movement debuff-resistance rides
    // the same unanimous fork), synthetic arm kept (8 credited mints — Time Lord again).
    // 1,319 carrier views agree with no divergence.
    const debuffResSlot = debuffResistanceValue(mezSource) ?? syntheticEffects(power)?.debuffResistance;
    if (debuffResSlot && typeof debuffResSlot === 'object') {
      const debuffRes = debuffResSlot;
      // Map debuff resistance types to global bonus keys
      const debuffResMapping: Record<string, keyof GlobalBonuses> = {
        movement: 'debuffResistSlow',
        defense: 'debuffResistDefense',
        recharge: 'debuffResistRecharge',
        endurance: 'debuffResistEndurance',
        recovery: 'debuffResistRecovery',
        tohit: 'debuffResistToHit',
        regeneration: 'debuffResistRegeneration',
        perception: 'debuffResistPerception',
      };
      // Enhancement type that boosts each debuff resistance type
      const debuffResEnhMapping: Record<string, string> = {
        defense: 'defense',
      };

      for (const [type, value] of Object.entries(debuffRes)) {
        const typeLower = type.toLowerCase();
        const enhKey = debuffResEnhMapping[typeLower];
        const enhMultiplier = enhKey ? 1 + (enhBonuses[enhKey] || 0) : 1;
        const stackedValue = stack(value, DEBUFF_RESISTANCE_STACK);
        const percentage = resolveScaledEffect(stackedValue, archetypeId, buildLevel) * 100 * enhMultiplier;
        const key = debuffResMapping[typeLower];
        if (key && key in global) {
          global[key] += percentage;
          addToBreakdown(breakdown, key, {
            name: power.name,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // Mez Resistance from active powers (e.g., Acrobatics Hold resistance)
    // Enhanced by the corresponding mez type enhancement (Hold enhancements for Hold resistance, etc.)
    // Stored as mezResistance: { hold: { scale, table }, ... }
    // BPORT11: atom-native, read through the fork-resolved {@link mezSource}, synthetic arm
    // kept (2 credited mints — Super Reflexes' Focused Fighting under Master Brawler).
    // Measured over 213,735 power×class views on the keys this block actually routes to a
    // global: 8,760 agreements, 0 divergences, and 2 gains — rebirth Weave's immobilize
    // resistance for the two Kheldian classes, which only the fork-resolved read can see.
    // The `taunt`/`placate`/`teleport` keys ride the same bag map and are NOT compared here
    // because `mezResMapping` never routed them; the taunt/placate block below spends those.
    const mezResSlot = mezResistanceValue(mezSource) ?? syntheticEffects(power)?.mezResistance;
    if (mezResSlot && typeof mezResSlot === 'object') {
      const mezResMapping: Record<string, keyof GlobalBonuses> = {
        hold: 'mezResistHold',
        stun: 'mezResistStun',
        immobilize: 'mezResistImmobilize',
        sleep: 'mezResistSleep',
        confuse: 'mezResistConfuse',
        fear: 'mezResistFear',
        knockback: 'mezResistKnockback',
      };
      for (const [type, value] of Object.entries(mezResSlot as Record<string, ScalarOrScaled>)) {
        const typeLower = type.toLowerCase();
        // Mez resistance is enhanced by the matching mez enhancement type
        const enhMultiplier = 1 + (enhBonuses[typeLower] || 0);
        const percentage = resolveScaledEffect(value, archetypeId, buildLevel) * 100 * enhMultiplier;
        const key = mezResMapping[typeLower];
        if (key && key in global) {
          global[key] += percentage;
          addToBreakdown(breakdown, key, {
            name: power.name,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // Elusivity (Defense Debuff Resistance)
    // Super Reflexes, Shield Defense, etc. — stored as elusivity.all or per-type
    // Enhanced by Defense enhancements
    // BPORT11: atom-native. BPORT1 filed this slot as zero-supply and the carry confirms it
    // from both sides — no power on any of the four forks carries an `elusivity` bag entry, and
    // `elusivityValue` returns nothing for any of them either. The block is kept rather than
    // deleted because the reader is the honest one: the day a fork ships an Elusivity atom this
    // answers, where a deletion would have to be noticed first.
    const elusivitySlot = elusivityValue(power);
    if (elusivitySlot && typeof elusivitySlot === 'object') {
      const enhMultiplier = 1 + (enhBonuses.defense || enhBonuses.defenseBuff || 0);
      const elusivity = elusivitySlot as Record<string, ScalarOrScaled>;
      for (const [, value] of Object.entries(elusivity)) {
        // Both 'all' and specific types (smashing, lethal, etc.) contribute to defense debuff resistance
        const percentage = resolveScaledEffect(value, archetypeId, buildLevel) * 100 * enhMultiplier;
        if (percentage > 0) {
          global.debuffResistDefense += percentage;
          addToBreakdown(breakdown, 'debuffResistDefense', {
            name: power.name,
            value: percentage,
            type: 'active-power',
          });
        }
      }
    }

    // The five top-level movement scalars — `runSpeed`, `runSpeedUnenhanced`, `flySpeed`,
    // `jumpHeight`, `jumpSpeed` — RETIRED BPORT11. Not one power on any of the four forks
    // carries any of them, so all five blocks were iterating nothing. The comment they carried
    // named Sprint, Ninja Run and Beast Run as the powers reaching the calc this way; those
    // hand-authored inherents carry no bag at all, and their movement is atom-native through
    // the map below. BPORT1 had already filed `flySpeed` as zero-supply; the other four read
    // `leave` because the supply census could not see that their only supplier was a slot no
    // converter writes.

    // Movement buffs (new format — e.g., Lightning Reflexes, Reaction Time)
    // Skip when the power also has tohitDebuff or damageDebuff — those indicate
    // enemy-targeting debuff auras (e.g., Time's Juncture) where movement is a
    // foe slow, not a self-buff
    //
    // Plan B Slice 7: sourced from the atom list (`movementBuffValue`, verified bag-equal
    // corpus-wide by scripts/planb-shadow-movement.cjs). An axis can hold MORE than one entry
    // (the reader keys by axis + ignoreStrength + suppressible), so this is a list, not a map.
    //
    // BPORT11 fork-resolves it, and that is what let the data arm go. The reader returns an
    // array — often an EMPTY one — for any power with a movement atom, and `??` keeps an empty
    // array, so the bag branch only ever fired where the reader answered `undefined`. Across
    // all four forks that was ONE power: rebirth Acrobatics, whose atoms fork by class
    // (AT-FORK-1) so a build-agnostic read saw none of them. Reading through {@link mezSource}
    // makes the reader answer it, and the data branch is left with no carrier at all.
    //
    // The synthetic arm stays for the 17 credited conditional mints (Stone Armor's Rooted in
    // Granite Root).
    //
    // The combat-debuff gate moved with it. It was `effects.tohitDebuff === undefined &&
    // effects.damageDebuff === undefined` — a question about two sibling SLOTS standing in for
    // a question about the power — and `carries_combat_debuff` asks the atoms directly, on the
    // discriminators that decide it (`aspect` separates a ToHit debuff from ToHit-debuff
    // resistance; `Str` is what makes a DamageBuff row a debuff to the caster's own output).
    // The two agree on all 18,239 power×class views, so this is a swap of instrument, not of
    // verdict.
    const movementEntries: MovementBuffEntry[] | undefined = movementBuffValue(mezSource)
      ?? (() => {
        const m = syntheticEffects(power)?.movement;
        if (!m) return undefined;
        return Object.entries(m).map(([axis, val]) => ({
          axis,
          ...(typeof val === 'number' ? { scale: val, table: '' } : (val as object)),
        }) as MovementBuffEntry);
      })();
    if (movementEntries && !carries_combat_debuff(power)) {
      // NOTE: the `fly` entry (the kFly attrib) is deliberately NOT mapped.
      // It's the flight-mode grant (magnitude > 0 = "can fly"), not a speed
      // buff — mapping it into flySpeed double-counted Fly (+200% from the
      // grant on top of the real +160.9% FlyingSpeed buff). The kFly scale
      // (Hover 4.0, Fly 2.0/1.0) is a mode magnitude, not a percentage.
      const movementKeyMap: Record<string, MovementContribution['stat']> = {
        runSpeed: 'runSpeed',
        flySpeed: 'flySpeed',
        jumpHeight: 'jumpHeight',
        jumpSpeed: 'jumpSpeed',
      };
      // Enhancement aspect per global movement key (runSpeed→run, fly→fly,
      // jump→jump). Same enhBonuses-multiplier treatment as the scalar path.
      const movementAspectMap: Record<string, keyof EnhancementBonuses> = {
        runSpeed: 'run',
        flySpeed: 'fly',
        jumpHeight: 'jump',
        jumpSpeed: 'jump',
      };
      for (const entry of movementEntries) {
        const key = movementKeyMap[entry.axis];
        if (key && key in global) {
          // An `ignoreStrength` entry is the half of a two-template axis the
          // caster's Run/Fly/Jump enhancements do not touch — the same rule the
          // `runSpeedUnenhanced` scalar slot above encodes by skipping this
          // multiplier outright. Only where the axis actually HOLDS a pair: a
          // lone `ignoreStrength` entry has been multiplied by the aspect since
          // the map existed, and 26 / 20 / 10 of them ship across the three
          // forks. Reading the flag for them too is a different change. MOVEMAP-1.
          const paired = movementEntries.filter((e) => e.axis === entry.axis).length > 1;
          const enhMultiplier = entry.ignoreStrength && paired ? 1 : 1 + (enhBonuses[movementAspectMap[key]] || 0);
          const val = entry as unknown as ScalarOrScaled;
          // STACK-4: the axis stacks to the depth its OWN sub type reaches, not the movement
          // family's. Time Wall states Run `Stack` (cap 2) while its Fly and Jump are
          // `Replace`, and one family-wide cap multiplied all three by 2. An axis naming no
          // sub type falls through as itself, which no atom names — so the row lands unstacked
          // at its base value rather than borrowing a sibling axis' depth.
          const axisSub = movementAxisSubType(entry.axis) ?? entry.axis;
          const adjusted = stack(val, buffStack('Movement', 'either', axisSub));
          const value = resolveMovementPercent(adjusted, key, archetypeId, buildLevel) * enhMultiplier;
          movementContribs.push({ stat: key, value, sourceName: power.name, type: 'active-power', ...movementMeta(val) });
        }
      }
    }

    // Movement debuffs / slow (self-penalty, e.g. Granite Armor -70% run speed)
    // Applied PER ENTRY when that entry is self-directed (toWho:'Self') — most
    // slow effects target enemies, and a foe slow can sit in the same `slow` map
    // as a self slow. Gating per-entry keeps the foe half off the caster. Unenhanceable.
    //
    // BPORT11 reads BOTH halves of the penalty here, which the bag arm did not. `slow` is the
    // Current-face debuff and `movementCapDebuff` is the Maximum-face one — Granite's jump
    // CAP as against its jump SPEED — and they were one slot until ENT-5 split them so a cap
    // debuff would stop overwriting the speed debuff on the same axis. This block never grew
    // the second read, so 312 powers carry a `movementCapDebuff` the calc has never spent.
    // Only 4 of those views are self-tagged and both arms agree on all 4, so nothing moves
    // today; what changes is that the axis now has a reader on both faces.
    //
    // The atom arms move 11 views, and every one is a self penalty the CONVERTER's `toWho`
    // tagging lost rather than a value the reader invented. Rebirth and Thunderspy's Granite
    // Armor and Rooted state their jump root as `JumpHeight -500 toWho:Target` on a
    // Self-target toggle — the "target" of a self-cast toggle IS the caster, which is why
    // `reachesCaster` consults the power's recipients — and the bag's untagged entry was
    // dropped by the `isSelfDirectedEffect` gate below. So the root those two powers are named
    // for was reaching no total on two of the four forks.
    //
    // `fly` is in the key map and the atom arm never produces it, and that absence is the fix
    // rather than an oversight: `fly` is the kFly flight-MODE grant, so a grounding power's
    // mode kill was being spent as a flight-SPEED percentage.
    const slowKeyMap: Record<string, keyof GlobalBonuses> = {
      runSpeed: 'runSpeed',
      flySpeed: 'flySpeed',
      fly: 'flySpeed',
      jumpHeight: 'jumpHeight',
      jumpSpeed: 'jumpSpeed',
    };
    const asSelfMap = (rows: MovementBuffEntry[] | undefined) =>
      rows && rows.length
        ? Object.fromEntries(rows.map((e) => [e.axis, { ...e, toWho: 'Self' }]))
        : undefined;
    const slowSlots: Array<Record<string, unknown> | undefined> = [
      asSelfMap(selfSlowValue(power)),
      asSelfMap(selfMovementCapDebuffValue(power)),
      // The conditional half: Rooted's Granite Root mode mints `slow` on a synthetic with no
      // atoms, 12 credited contributions the atom arms cannot answer for.
      syntheticEffects(power)?.slow as Record<string, unknown> | undefined,
    ];
    for (const slot of slowSlots) {
      if (!slot || typeof slot !== 'object') continue;
      for (const [type, val] of Object.entries(slot)) {
        if (!isSelfDirectedEffect(val)) continue;
        const key = slowKeyMap[type];
        if (key && key in global) {
          const value = resolveScaledEffect(val as ScalarOrScaled, archetypeId, buildLevel) * -100;
          global[key] += value;
          addToBreakdown(breakdown, key, {
            name: power.name,
            value,
            type: 'active-power',
          });
        }
      }
    }

    // Recharge buff
    // NOT enhanced by Recharge enhancements — recharge enhancements reduce the
    // power's own recharge time, they don't boost the recharge speed buff value
    // BPORT11: atom-native. Agrees with the bag on all 309 shared carriers, and answers for 25
    // more the Thunderspy bag never held (Time Wall's +20% self recharge, Resurgence's +100%) —
    // the direction the migration exists to recover, and the direction that closes an existing
    // oracle-vs-engine gap, because Rust has read `recharge_buff_value` since ATOM9.
    // The synthetic arm STAYS: 20 reachable conditional mints (Time Lord's Temporal Selection)
    // and the buff-pet aura fold both write this slot, and neither carries an atom to read.
    const rechargeBuffSlot = rechargeBuffValue(power) ?? syntheticEffects(power)?.rechargeBuff;
    if (rechargeBuffSlot !== undefined) {
      const adjusted = stack(rechargeBuffSlot, buffStack('RechargeTime'));
      const value = extractScaleValue(adjusted) * 100;
      global.recharge += value;
      addToBreakdown(breakdown, 'recharge', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Recharge debuff (self-penalty, e.g. Granite Armor -65% recharge)
    // Only applied when the value is self-directed (toWho:'Self') — most
    // rechargeDebuff effects target enemies. Unenhanceable.
    // BPORT11: atom-native. All 8 self-tagged carriers agree (Granite Armor's -65% among
    // them), and the 134 conditional mints of the slot are every one of them foe-facing, which
    // the self-directed gate has always dropped — so there is no synthetic arm to keep.
    const selfRechargeDebuff = selfRechargeDebuffValue(power);
    if (selfRechargeDebuff !== undefined) {
      const value = resolveScaledEffect(selfRechargeDebuff as ScalarOrScaled, archetypeId, buildLevel) * -100;
      global.recharge += value;
      addToBreakdown(breakdown, 'recharge', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Regeneration buff
    // Enhanced by Healing enhancements (slotted) AND by +Healing Strength
    // set bonuses (Numina 4pc, Panacea 6pc, Miracle 4pc, etc.). HC's
    // heal-strength buffs increase the heal_enh multiplier applied to
    // regen-producing powers, post-ED. The `global.healOther` accumulator
    // is populated by set-bonuses.ts from `healing_strength` bonuses; we
    // read it here so it actually contributes to per-power regen.
    // Skip Res_Boolean tables — those are regen debuff resistance, not regen buffs
    //
    // Plan B Slice 6: sourced from the atom list (`regenBuffValue`), falling back to
    // the bag for an atom-less power AND for the two shapes the helper deliberately
    // PUNTS on — an Expression-typed resource template, and the StackByAttribAndKey
    // burst/tail family whose bag value is a suspected latent bug (see atom-query.ts).
    // Each half falls back independently; the shadow gate proves every value the
    // helper DOES return equals the bag's, so a mixed atom/bag pair still sums right.
    // BPORT11: synthetic arms kept on BOTH halves, and the unenhanced one is the larger —
    // 95 credited mints against the enhanceable half's 20, every one a Bio Armor stance
    // (Inexhaustible in Rested Adaptation). Canonical dropped that half and lost all 95.
    //
    // Retiring the DATA arm drops 43 + 76 credits and every one is a correction: Adrenalin
    // Boost, Painbringer, Temporal Selection, Speed Boost and their siblings are ALLY buffs
    // whose toWho-blind bag slot projected the ally's +regen/+recovery onto the caster, which
    // the game never grants and `reachesCaster` declines.
    const regenSlot = regenBuffValue(power) ?? syntheticEffects(power)?.regenBuff;
    const regenUnenhSlot = regenBuffValue(power, { ignoreStrength: true })
      ?? syntheticEffects(power)?.regenBuffUnenhanced;
    if (regenSlot !== undefined) {
      const regenVal = regenSlot as ScalarOrScaled;
      const regenTable = (typeof regenVal === 'object' && regenVal !== null && 'table' in regenVal)
        ? (regenVal as { table?: string }).table ?? ''
        : '';
      if (!regenTable.toLowerCase().includes('res_boolean')) {
        const enhMultiplier = 1 + (enhBonuses.heal || 0) + ((global.healOther || 0) / 100);
        const adjustedRegen = stack(regenVal, buffStack('Regeneration', 'enhanceable'));
        const value = resolveScaledEffect(adjustedRegen, archetypeId, buildLevel) * 100 * enhMultiplier;
        // If the power also has an unenhanced portion, combine into one breakdown entry
        const adjustedRegenUnenh = regenUnenhSlot !== undefined
          ? stack(regenUnenhSlot as ScalarOrScaled, buffStack('Regeneration', 'unenhanced'))
          : undefined;
        const unenhValue = adjustedRegenUnenh !== undefined
          ? resolveScaledEffect(adjustedRegenUnenh, archetypeId, buildLevel) * 100
          : 0;
        const totalValue = value + unenhValue;
        global.regeneration += totalValue;
        addToBreakdown(breakdown, 'regeneration', {
          name: power.name,
          value: totalValue,
          type: 'active-power',
        });
      }
    } else if (regenUnenhSlot !== undefined) {
      // Power only has unenhanceable regen (no enhanceable portion)
      const adjustedUnenhOnly = stack(regenUnenhSlot as ScalarOrScaled, buffStack('Regeneration', 'unenhanced'));
      const value = resolveScaledEffect(adjustedUnenhOnly, archetypeId, buildLevel) * 100;
      global.regeneration += value;
      addToBreakdown(breakdown, 'regeneration', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Recovery buff
    // Enhanced by Endurance Modification enhancements
    // Skip Res_Boolean tables — those are endurance drain resistance, not recovery buffs
    // Plan B Slice 6: atom-sourced with the same per-half bag fallback as regen above.
    // Same shape as regen above, and the same split: 9 credited mints on this half, 110 on the
    // unenhanced twin below — the biggest synthetic population in the whole carry.
    const recoverySlot = recoveryBuffValue(power) ?? syntheticEffects(power)?.recoveryBuff;
    if (recoverySlot !== undefined) {
      const recBuff = recoverySlot as ScalarOrScaled;
      const table = (typeof recBuff === 'object' && recBuff !== null && 'table' in recBuff)
        ? (recBuff as { table?: string }).table ?? ''
        : '';
      if (!table.toLowerCase().includes('res_boolean')) {
        const enhMultiplier = 1 + (enhBonuses.enduranceMod || 0);
        const adjustedRecovery = stack(recBuff, buffStack('Recovery', 'enhanceable'));
        const value = resolveScaledEffect(adjustedRecovery, archetypeId, buildLevel) * 100 * enhMultiplier;
        global.recovery += value;
        addToBreakdown(breakdown, 'recovery', {
          name: power.name,
          value,
          type: 'active-power',
        });
      }
    }

    // Recovery buff that ignores strength (IgnoreStrength) — NOT boosted by End
    // Mod enh or global +recovery (e.g. Bio Armor adaptation's ride-along recovery).
    const recoveryUnenhSlot = recoveryBuffValue(power, { ignoreStrength: true })
      ?? syntheticEffects(power)?.recoveryBuffUnenhanced;
    if (recoveryUnenhSlot !== undefined) {
      const adjusted = stack(recoveryUnenhSlot as ScalarOrScaled, buffStack('Recovery', 'unenhanced'));
      const value = resolveScaledEffect(adjusted, archetypeId, buildLevel) * 100;
      global.recovery += value;
      addToBreakdown(breakdown, 'recovery', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Max HP buff
    // Enhanced by Healing enhancements.
    //
    // MaxHP buffs use a flat 10% per scale point. The bin's
    // `Melee_HealSelf` reference is used by the engine for absolute-HP
    // bookkeeping (it's literally `baseMaxHP/10` for each AT) but the
    // displayed/applied buff is a percentage. Verified across the
    // canonical +HP powers:
    //   • Tanker  HPT: scale=2 → +20% (matches in-game)
    //   • Brute   Dull Pain (Second Wind): scale=2 → +20%
    //   • Brute   Earth's Embrace: scale=4 → +40%
    // The previous 5%/scale formula produced exactly half of each of
    // those numbers, which is what users were comparing against Mids
    // and seeing too low.
    // Plan B Slice 5: sourced from atoms (`maxHPBuffValue` — the MaxHP buff atoms,
    // aspect Max, split on `ignoreStrength`, restricted to the base set; verified
    // bag-equal by scripts/planb-shadow-maxhp.cjs). `?? effects.maxHPBuff` keeps an
    // atom-less legacy power on the bag. Read `.scale` directly (×10, no table
    // resolution) — the atom value carries no perTarget for any MaxHP power.
    // BPORT11: synthetic arms kept on both halves — 1 credited mint on this one (rebirth
    // Essence Boost in Peacebringer Tanker Mode) and 43 on the unenhanced twin (Bio Armor's
    // Inexhaustible in Defensive Adaptation). All 293 + 162 real carriers agree with the bag.
    const maxHPBuff = maxHPBuffValue(power) ?? syntheticEffects(power)?.maxHPBuff;
    if (maxHPBuff !== undefined) {
      const enhMultiplier = 1 + (enhBonuses.heal || 0);
      const scale = typeof maxHPBuff === 'number'
        ? maxHPBuff
        : maxHPBuff.scale;
      const value = scale * 10 * enhMultiplier;
      global.maxHP += value;
      addToBreakdown(breakdown, 'maxHP', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Unenhanceable half of a +MaxHP twin (IgnoreStrength). Same base formula as
    // maxHPBuff but with NO +Healing multiplier — the game flags this half so it
    // ignores enhancement strength. Both halves co-apply (Inexhaustible, High
    // Pain Tolerance, Dull Pain, …); the atom list carries the IgnoreStrength half
    // as its own atom (`ignoreStrength:true`), which is exactly the twin split the
    // bag re-materialized as a parallel `maxHPBuffUnenhanced` slot.
    const maxHPBuffUnenhanced = maxHPBuffValue(power, { ignoreStrength: true })
      ?? syntheticEffects(power)?.maxHPBuffUnenhanced;
    if (maxHPBuffUnenhanced !== undefined) {
      const scale = typeof maxHPBuffUnenhanced === 'number'
        ? maxHPBuffUnenhanced
        : maxHPBuffUnenhanced.scale;
      const value = scale * 10;
      global.maxHP += value;
      addToBreakdown(breakdown, 'maxHP', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Max Endurance buff
    // Enhanced by Endurance Modification enhancements
    // Scale values are already in absolute endurance points (e.g., scale 10 = +10 end)
    // BPORT11: atom-native, synthetic arm kept (6 reachable mints — Bio Armor's Genomic
    // Evolution in Rested Adaptation). `maxEndBuffValue` needed the recipient filter its Rust
    // twin already had before it could carry: without it Soul Consumption's foe drain was
    // folded into the caster's gain and the reader answered 2 where the bag says 1. With it,
    // 48/48 carriers agree.
    const maxEndBuffSlot = maxEndBuffValue(power) ?? syntheticEffects(power)?.maxEndBuff;
    if (maxEndBuffSlot !== undefined) {
      const enhMultiplier = 1 + (enhBonuses.enduranceMod || 0);
      const value = resolveScaledEffect(maxEndBuffSlot, archetypeId, buildLevel) * enhMultiplier;
      global.maxEndurance += value;
      addToBreakdown(breakdown, 'maxEndurance', {
        name: power.name,
        value,
        type: 'active-power',
      });
    }

    // Absorb shield
    // Mirrors the per-power display (SharedPowerComponents): a Heal-table
    // {scale,table} resolves to absolute HP; a `_ones` table or the recovered
    // `maxHPFraction` is a fraction of the caster's current Max HP, resolved to
    // HP later against the final build Max HP (accolades included — which is
    // why Wild Bastion grows with +HP accolades). Boosted like healing:
    // slotted Heal enhancement + +Strength(Absorb) from Power Boost / Clarion.
    // BPORT7 A2 carries the arm BPORT11 declined, on the shape canonical's own strip settled on:
    // the MaxHP-fraction probe answers the fraction carriers first, the flat fold the rest, and
    // the pet-aura mint is the last resort. What the decline got right, and what the carry keeps
    // named rather than silent: neither reader returns `perTarget`, so the per-foe increment no
    // longer reaches this slot.
    //
    // The per-foe ABSORB channel is the third item of the ABSORB-4 residual. The engine stays
    // atom-fed for every per-foe family, not just absorb — measured with the bag deleted, absorb
    // reads 0 / 133.86208 / 669.3104 at N = absent / 1 / 5, and Soul Drain's tohit 0 / 12 / 20 and
    // damage 0 / 60 / 100, all unchanged — so the channel survives the regen on the engine side.
    // This oracle reads it flat: Parasitic Aura's +10% MaxHP/foe (the atom's `perTarget` stamp)
    // and Parasitic Leech's +14.3%/foe (derived from the bundle's `stats.maxTargets`, which the
    // legacy power does not carry) both move on the engine and not here. `serverParity` excludes
    // the absorb key for those powers and logs them unverified, so the residual is watched, not
    // hidden.
    //
    // Two more named edges of the same carry. The 18 repeated-row powers state one shield as
    // several byte-identical rows: this fold sums them (N×) where the engine's `absorb_flat_value`
    // dedupes to one, so the oracle reads high and the engine the deduped value — ungraded. And
    // the 9 probe-undefined fraction carriers (Tough Hide, Unstoppable, Reinforced Exoskeleton,
    // Thunderspy) read 0 on both sides post-regen, the mirrored probe gate answering nothing with
    // the bag gone — both wrong against the bag, named, ungraded.
    const absorbFrac = absorbMaxHPFractionValue(power);
    const absorbSlot = absorbFrac != null
      ? { maxHPFraction: absorbFrac }
      : (absorbValue(power) ?? syntheticEffects(power)?.absorb);
    if (absorbSlot !== undefined && absorbSlot !== null) {
      // The slot is flat now — neither reader returns `perTarget` — so the targets-hit slider
      // moves nothing here; the per-foe channel is the residual above, not this path.
      // adjustForStackCap only ever adjusts `scale`, so maxHPFraction/table survive the spread.
      const ab = adjustForStackCap(
        absorbSlot as ScalarOrScaled,
        targetsHitValues[power.internalName],
        stackCapOf(power, buffStack('Absorb')),
        power,
      ) as { scale?: number; table?: string; perTarget?: number; maxHPFraction?: number; appliesStrength?: boolean };
      // The MaxHP-fraction form is scaled by strength unless it opts out
      // (appliesStrength:false — ATO procs, which don't reach this path).
      const applyStrength = ab.maxHPFraction == null || ab.appliesStrength !== false;
      const enhMultiplier = applyStrength
        ? 1 + (enhBonuses.heal || 0) + strengthBuffs.absorb
        : 1;
      const isOnesTable = (ab.table || '').toLowerCase().endsWith('_ones');
      if (ab.maxHPFraction != null || isOnesTable) {
        const baseFraction = ab.maxHPFraction != null
          ? ab.maxHPFraction
          : resolveScaledEffect(ab as ScalarOrScaled, archetypeId, buildLevel);
        const fraction = baseFraction * enhMultiplier;
        if (fraction > 0) absorbFractionContribs.push({ name: power.name, fraction });
      } else {
        const hp = resolveScaledEffect(ab as ScalarOrScaled, archetypeId, buildLevel) * enhMultiplier;
        if (hp > 0) {
          global.absorb += hp;
          addToBreakdown(breakdown, 'absorb', {
            name: power.name,
            value: hp,
            type: 'active-power',
          });
        }
      }
    }

    // Endurance Discount (e.g., Conserve Power — reduces end costs by a percentage)
    // Stored as a scaled effect; the resolved value is a decimal (e.g., 0.6 = 60% discount)
    // Routed to global.endurance (canonical EndDisc accumulator) so toggle cost
    // and the dashboard "End Disc" stat see a single unified sum.
    // BPORT11: atom-native, synthetic arm kept — 29 reachable conditional mints, every one a
    // Bio Armor stance (Hardened Carapace in Rested Adaptation), and a stance conditional has
    // no atoms of its own for the reader to answer from. 103/103 real carriers agree.
    const endDiscountSlot = enduranceDiscountValue(power) ?? syntheticEffects(power)?.enduranceDiscount;
    if (endDiscountSlot !== undefined) {
      const discount = resolveScaledEffect(endDiscountSlot, archetypeId, buildLevel) * 100;
      if (discount > 0) {
        global.endurance += discount;
        addToBreakdown(breakdown, 'endurance', {
          name: power.name,
          value: discount,
          type: 'active-power',
        });
      }
    }

    // `effects.protection` RETIRED BPORT11. BPORT1 filed the slot as zero-supply and the carry
    // confirms it from the data: not one power on any of the four forks carries the object, so
    // this loop was iterating nothing. Its whole subject is the six-MEZ fold below, which reads
    // the same protection off the atoms and folds Knockback/Knockup on the same rule.

    // Mez Protection from curated armor powers (effects.hold/stun/etc. with Res_Boolean tables)
    // When mez effects use Res_Boolean tables, they represent protection, not offensive mez
    const mezProtTypes: Array<{ field: keyof ActivePowerEffect; key: keyof GlobalBonuses }> = [
      { field: 'hold', key: 'protHold' },
      { field: 'stun', key: 'protStun' },
      { field: 'immobilize', key: 'protImmobilize' },
      { field: 'sleep', key: 'protSleep' },
      { field: 'confuse', key: 'protConfuse' },
      { field: 'fear', key: 'protFear' },
      { field: 'knockback', key: 'protKnockback' },
      // Repel protection joins the fold at BPORT11 rather than keeping its own block: it is
      // read on the same terms (a self-directed protection atom, `|scale| x table`) and it is
      // its own stat, not knockback — the continuous push rather than the impulse.
      { field: 'repel', key: 'protRepel' },
      // Knockup is mechanically the same as knockback for protection purposes
      // — powers like Evasive Maneuvers list `kKnockup kKnockback` together in
      // the .powers source and the bin emits separate `knockup`/`knockback`
      // templates with the same scale. Both feed the same protKnockback stat.
      { field: 'knockup', key: 'protKnockback' },
    ];

    // Knockback and Knockup protection are the SAME physical stat in CoH: a
    // single power grants both at an equal magnitude, and the bin emits paired
    // Knockup/Knockback templates with identical scale (see the {field:'knockup'}
    // entry above). Summing them via `+=` would DOUBLE this power's KB protection
    // (e.g. Bo Ryaku showing ~14 instead of ~7, Unyielding ~20 instead of ~10).
    // Accumulate the pair's contribution into a single per-power value (take the
    // max — they are always equal) and add it once after the loop. Different
    // powers still stack because each power runs this block separately.
    let kbProtFromThisPower = 0;
    for (const { field, key } of mezProtTypes) {
      const isKb = field === 'knockback' || field === 'knockup';
      // KB/KU protection is atom-native (ATOM15 / PASS2B-1): kbProtectionValue accumulates ONLY the
      // power's SELF-directed KB protection atoms (foe-attack knockback excluded, MezResist included),
      // so a self-atom result is caster protection by construction — the old effectArea+powerType proxy
      // (which miscredited SingleTarget foe attacks like Battle Axe Gash) is retired.
      //
      // BPORT11 takes the six MEZ types the same way (`mezSlotValue` mirrors the converter's
      // makeMezEffect arm and its prefer-PvE / larger-magnitude pick), and reads BOTH through
      // the fork-resolved {@link mezSource}. Measured over 213,735 power×class views: the six
      // agree with the bag on all 36,780 carrier views with no divergence and nothing lost on
      // either side, and the KB/KU bag arm never fired once — every view where it could have
      // was one the atom arm had already answered, which is what retires it rather than a
      // decision to drop it.
      //
      // The SYNTHETIC branch stays, and it is not the same branch. Quantum Maneuvers grants
      // its immobilize and knockback protection from a Flight-Active conditional, and a
      // conditional carries no atoms — 12 credited contributions across the fold that an
      // atom-only arm answers `undefined` for. It is gated by `isResBoolean` below exactly as
      // the retired data read was, because a synthetic is not a self-atom.
      let mez: MezScaled | undefined;
      let isSelfAtom = false;
      const atomVal = isKb
        ? kbProtectionValue(mezSource, field as 'knockback' | 'knockup')
        : field === 'repel'
          ? repelProtectionValue(mezSource)
          : mezSlotValue(mezSource, field as 'hold' | 'stun' | 'sleep' | 'immobilize' | 'confuse' | 'fear');
      if (atomVal) {
        mez = atomVal as unknown as MezScaled;
        isSelfAtom = true;
      } else {
        const synthVal = syntheticEffects(power)?.[field];
        if (synthVal !== undefined && typeof synthVal !== 'number') mez = synthVal as MezScaled;
      }
      if (!mez || !mez.table) continue;
      const tableLower = mez.table.toLowerCase();
      const isResBoolean = tableLower.includes('res_boolean');
      if (!(isResBoolean || isSelfAtom)) continue;
      // Read at the build level, like every other table read in this pass. This used to be
      // pinned to 50 on the claim that protection magnitude doesn't scale while leveling;
      // both oracles say otherwise — `Res_Boolean` appears nowhere in the game source (no
      // branch special-cases it, so `mod_Fill` resolves it at `iEffCombatLevel` like any
      // other template), and the tables vary by level (`melee_res_boolean` runs 0.120 → 0.277
      // across 1–50), so the pin roughly doubled a level-10 build's protection (PROD6B-2c).
      const tableValue = getTableValue(archetypeId, tableLower, buildLevel);
      if (tableValue === undefined) continue;
      let mag = Math.abs(mez.scale) * tableValue;
      // Knockback enhancements boost non-Res_Boolean self-KB protection (per Acrobatics
      // description). Narrowed to the KB arm when the six MEZ joined it: a non-Res_Boolean
      // hold-protection atom is not boosted by Knockback enhancement.
      if (isSelfAtom && isKb && !isResBoolean) {
        mag *= (1 + (enhBonuses.knockback || 0));
      }
      if (key === 'protKnockback') {
        // Fold the Knockback/Knockup pair into one contribution (see note above).
        kbProtFromThisPower = Math.max(kbProtFromThisPower, mag);
      } else {
        global[key] += mag;
        addToBreakdown(breakdown, key, {
          name: power.name,
          value: mag,
          type: 'active-power',
        });
      }
    }
    if (kbProtFromThisPower > 0) {
      global.protKnockback += kbProtFromThisPower;
      addToBreakdown(breakdown, 'protKnockback', {
        name: power.name,
        value: kbProtFromThisPower,
        type: 'active-power',
      });
    }

    // `protRepel` is folded in above (`repelProtectionValue`), and the standalone
    // `effects.repel` block that used to stand here is gone with it. That read was backwards on
    // the example it named: `effects.repel` holds the repel a power INFLICTS, so Ki Push, Jet
    // Stream, Hurricane and Repulsion Field credited the caster with protection equal to the
    // push they deal out — 71 powers — while Increase Density, the comment's own example, was
    // one of 15 real repel-protection powers the slot never carried at all.

    // `protTeleport` RETIRED BPORT11 (user decision), stat and all. The read was
    // `effects.teleport` x 100, and that slot holds the teleport a power PERFORMS: all 127
    // credited carriers are teleport powers — Burst of Speed, Lightning Rod, Shield Charge,
    // Wormhole at +410% — being credited with protection equal to their own range. No power on
    // any fork carries a protection-spelled (negative Cur) `Mez/Teleport` row, so there was
    // nothing for a corrected read to answer with. The honest family is the caster-facing
    // `MezResist/Teleport` rows (98 of them: Static Shield, Personal Force Field, Increase
    // Density, Entropic Aura), which is RESISTANCE rather than protection and belongs with the
    // mez-resistance family, not here. The engine never had a source for the stat either
    // (engineTotals.ts left it 0), so the live dashboard row has shown 0 since the engine swap
    // and only this oracle ever produced the wrong number. Canonical retired it the same way.

    // Taunt Resistance (e.g., Leadership: Assault) — BPORT11: atom-native. `tauntPlacateValue`
    // reads the caster-facing `MezResist/Taunt` rows the converter parked in the same
    // `mezResistance` bag map the block above spends, under a key that map never routed. All 4
    // carriers agree, and no conditional or pet aura mints the slot.
    {
      const mezVal = tauntPlacateValue(power, 'Taunt');
      if (mezVal !== undefined) {
        const mez = mezVal as MezScaled;
        if (mez.table && mez.table.toLowerCase().includes('res_boolean')) {
          const tableValue = getTableValue(archetypeId, mez.table.toLowerCase(), buildLevel);
          if (tableValue !== undefined) {
            const mag = Math.abs(mez.scale) * tableValue * 100;
            global.mezResistTaunt += mag;
            addToBreakdown(breakdown, 'mezResistTaunt', {
              name: power.name,
              value: mag,
              type: 'active-power',
            });
          }
        }
      }
    }

    // Placate Resistance (e.g., Leadership: Assault) — atom-native, see the Taunt arm above.
    // All 15 carriers agree.
    {
      const mezVal = tauntPlacateValue(power, 'Placate');
      if (mezVal !== undefined) {
        const mez = mezVal as MezScaled;
        if (mez.table && mez.table.toLowerCase().includes('res_boolean')) {
          const tableValue = getTableValue(archetypeId, mez.table.toLowerCase(), buildLevel);
          if (tableValue !== undefined) {
            const mag = Math.abs(mez.scale) * tableValue * 100;
            global.mezResistPlacate += mag;
            addToBreakdown(breakdown, 'mezResistPlacate', {
              name: power.name,
              value: mag,
              type: 'active-power',
            });
          }
        }
      }
    }

    // Stealth Radius — only COLLECTED here; the grouped max+sum is committed to
    // global.stealthRadius* by resolveStealthRadius once procs are gathered too.
    // Powers in a shared suppress group (binary stack_key, e.g. "NictusFX":
    // Stealth, Super Speed, Shinobi-Iri, the cloak toggles) don't stack — only
    // the largest applies; everything else stacks additively. That's why a
    // Stealth IO (its own group) lands on top of a stealth power toward the
    // invisibility cap, while Super Speed + pool Stealth (same group) don't add.
    // BPORT11: atom-native (`stealthValue` mirrors the converter's Current-face arm), synthetic
    // arm kept for the one credited mint (Thunderspy's Primalist's Cloak in Prowler Mode).
    // All 341 real carriers agree. 106 more carry a bag `stealth` the reader declines, and 105
    // of those are the teleport family's `{translucency: …}` authoring artifact under a key
    // this block never reads — 0 credited. The 106th is named in the verify test.
    const stealthSlot = stealthValue(power) ?? syntheticEffects(power)?.stealth;
    if (stealthSlot) {
      const pve = stealthSlot.stealthPvE !== undefined
        ? resolveScaledEffect(stealthSlot.stealthPvE, archetypeId, buildLevel) : 0;
      const pvp = stealthSlot.stealthPvP !== undefined
        ? resolveScaledEffect(stealthSlot.stealthPvP, archetypeId, buildLevel) : 0;
      if (pve > 0 || pvp > 0) {
        stealthContribs.push({
          stackKey: stealthSlot.stackKey ?? null,
          pve,
          pvp,
          sourceName: power.name,
          type: 'active-power',
        });
      }
    }

    // Perception Buff
    // BPORT11: atom-native, synthetic arm kept (2 reachable mints — Bio Armor's Rebuild DNA in
    // Offensive Adaptation). 256/256 real carriers agree.
    const perceptionBuffSlot = perceptionBuffValue(power) ?? syntheticEffects(power)?.perceptionBuff;
    if (perceptionBuffSlot !== undefined) {
      const val = resolveScaledEffect(perceptionBuffSlot, archetypeId, buildLevel) * 100;
      if (val > 0) {
        global.perceptionRadius += val;
        addToBreakdown(breakdown, 'perceptionRadius', {
          name: power.name,
          value: val,
          type: 'active-power',
        });
      }
    }

    // Range Buff (Boost Range, Aim's +Range, …) → global +Range.
    // Only aggregate Self-targeted buffs: the same `rangeBuff` field on a
    // Foe-targeted attack (Blazing Bolt, Moonbeam, every snipe) is the per-power
    // Fast Snipe range bump — gated on a ≥22% ToHit buff in game — not a
    // persistent caster buff, so it must not feed the character's Range total.
    // This mirrors the shouldShowToggle exclusion in power-row-utils.
    // BPORT11: atom-native, synthetic arm kept (2 reachable mints — Genomic Evolution in
    // Offensive Adaptation). 45/45 real carriers agree under the same `targetType: Self` gate.
    const rangeBuffSlot = rangeBuffValue(power) ?? syntheticEffects(power)?.rangeBuff;
    if (rangeBuffSlot !== undefined && power.targetType?.toLowerCase() === 'self') {
      const adjusted = stack(rangeBuffSlot, buffStack('Range'));
      const val = resolveScaledEffect(adjusted, archetypeId, buildLevel) * 100;
      if (val > 0) {
        global.range += val;
        addToBreakdown(breakdown, 'range', {
          name: power.name,
          value: val,
          type: 'active-power',
        });
      }
    }

    // Debug: log per-power diff with enhancement and alpha detail
    if (_debugEnabled && _debugBefore) {
      const diffs: { stat: string; value: number }[] = [];
      for (const key of Object.keys(global) as (keyof GlobalBonuses)[]) {
        const delta = global[key] - _debugBefore[key];
        if (Math.abs(delta) > 0.0001) {
          diffs.push({ stat: key, value: delta });
        }
      }
      // Only log powers that actually contributed something, or have enhancements worth showing
      const hasEnhBonuses = Object.values(enhBonuses).some(v => v !== undefined && Math.abs(v) > 0.0001);
      if (diffs.length > 0 || hasEnhBonuses) {
        debugGroup(`${power.name} (${power.powerType || 'unknown'}${power.isActive ? ', active' : ''})`);

        // Show enhancement bonuses applied to this power
        if (hasEnhBonuses) {
          debugGroup('Enhancement Bonuses (post-ED + Alpha)');
          for (const [aspect, val] of Object.entries(enhBonuses)) {
            if (val === undefined || Math.abs(val) < 0.0001) continue;
            const fromAlpha = alphaBonuses[aspect] ?? 0;
            const fromSlots = val - fromAlpha;
            let detail = `${aspect}: +${formatDebugNum(val * 100)}%`;
            if (fromAlpha > 0 && fromSlots > 0) {
              detail += ` (slots: ${formatDebugNum(fromSlots * 100)}% + alpha: ${formatDebugNum(fromAlpha * 100)}%)`;
            } else if (fromAlpha > 0) {
              detail += ` (alpha only)`;
            }
            debugFormula(detail);
          }
          debugGroupEnd();
        }

        // Show stat contributions
        if (diffs.length > 0) {
          debugGroup('Global Bonus Contributions');
          for (const d of diffs) {
            debugFormula(`${d.stat}: ${d.value > 0 ? '+' : ''}${formatDebugNum(d.value)}`);
          }
          debugGroupEnd();
        }

        debugGroupEnd();
      }
    }
  }
}

function formatDebugNum(n: number): string {
  return Number.isInteger(n) ? n.toString() : n.toFixed(4).replace(/0+$/, '').replace(/\.$/, '');
}

function extractScaleValue(effect: ScalarOrScaled | undefined): number {
  if (effect === undefined) return 0;
  if (typeof effect === 'number') return effect;
  return effect.scale || 0;
}

/**
 * Resolve a movement effect to its displayed percentage (the value added to
 * global.runSpeed/flySpeed/jumpSpeed/jumpHeight).
 *
 * ALL movement attribs — run/fly/jump SPEED *and* jump HEIGHT — scale by their
 * AT modifier table, like every other buff: `scale × AT-table × 100`. E.g.
 * Super Speed `RunningSpeed 1.0 × Melee_SpeedRunning` (3.5 @50) = +350%; Ninja
 * Run jumpHeight `0.25 × Melee_Leap` (27.8 @50) = +695%. The bare-scale reading
 * (×100, no table) gave absurdly slow travel powers (Super Speed ≈ 28 mph) and
 * an order-of-magnitude-too-small jump height (Ninja Run +25% → ~5 ft vs the
 * in-game ~25-30 ft onto a rooftop ledge — Redlynne, Rebirth, 2026-06-16).
 *
 * Jump height was previously special-cased to bare scale, which collapsed a
 * deliberate bin distinction: Ninja Run / Beast Run use the BIG `Melee_Leap`
 * table (27.8) while Sprint / the prestige sprints use flat `Melee_Ones` (1.0,
 * → +10% either way). That table choice is only meaningful if the table is
 * applied — so jump height is table-aware too. Verified vs Rebirth/HC
 * `powers.bin`: Ninja Run `0.25×27.8` → +695%, Hurdle `0.06×27.8` → +167%,
 * Sprint `0.1×Melee_Ones` → +10%.
 */
function resolveMovementPercent(
  effect: ScalarOrScaled | undefined,
  _movementKey: string,
  archetypeId: string,
  level: number,
): number {
  return resolveScaledEffect(effect, archetypeId, level) * 100;
}

function capitalizeFirst(str: string): string {
  if (str.toLowerCase() === 'aoe') return 'AoE';
  return str.charAt(0).toUpperCase() + str.slice(1).toLowerCase();
}

function addToBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  stat: string,
  source: StatSource
): void {
  if (!breakdown.has(stat)) {
    breakdown.set(stat, {
      total: 0,
      base: 0,
      sources: [],
      cappedSources: 0,
    });
  }
  const entry = breakdown.get(stat)!;
  entry.sources.push(source);
  entry.total += source.value;
  if (source.capped) entry.cappedSources++;
}

// ============================================
// FITNESS POWER PROCESSING
// ============================================

/**
 * Fitness power effects derived from INHERENT_FITNESS_POWERS scale data (levels.ts).
 *
 * Movement stats (runSpeed, flySpeed, jumpSpeed, jumpHeight) are resolved at
 * runtime through their AT table — Swift's RunningSpeed is `scale 0.1 ×
 * Melee_SpeedRunning` (3.5 at L50) = +35%, not the bare +10%; Hurdle's jumpHeight
 * is `0.06 × Melee_Leap` (27.8) = +167% — so the `value` below is only a fallback
 * (see the FITNESS_MOVEMENT_STATS branch in applyFitnessPowerBonuses). The
 * Melee_Ones buffs (regen, recovery) equal scale × 100, so their `value` is
 * authoritative.
 *
 *   Swift:   runSpeed  { scale: 0.1,  table: 'Melee_SpeedRunning' } → table-resolved (~35% @50)
 *            flySpeed  { scale: 0.1,  table: 'Melee_SpeedFlying' }  → table-resolved (~14% @50)
 *   Hurdle:  jumpHeight { scale: 0.06, table: 'Melee_Leap' }        → table-resolved (~167% @50)
 *            jumpSpeed  { scale: 0.5,  table: 'Melee_SpeedJumping' } → table-resolved (~124.5% @50)
 *   Health:  regenBuff  { scale: 0.4,  table: 'Melee_Ones' }        → 40%
 *   Stamina: recoveryBuff { scale: 0.25, table: 'Melee_Ones' }      → 25%
 */
interface FitnessEffect {
  stat: keyof GlobalBonuses;
  value: number;
  enhancementType: string;
}

/** Fitness stats whose `value` is a fallback — resolved via AT table at runtime. */
const FITNESS_MOVEMENT_STATS = new Set<keyof GlobalBonuses>(['runSpeed', 'flySpeed', 'jumpSpeed', 'jumpHeight']);

const FITNESS_POWER_EFFECTS: Record<string, FitnessEffect[]> = {
  'Swift': [
    { stat: 'runSpeed', value: 10, enhancementType: 'run' },
    { stat: 'flySpeed', value: 10, enhancementType: 'fly' },
  ],
  'Hurdle': [
    { stat: 'jumpHeight', value: 6, enhancementType: 'jump' },
    { stat: 'jumpSpeed', value: 50, enhancementType: 'jump' },
  ],
  'Health': [
    { stat: 'regeneration', value: 40, enhancementType: 'heal' },
  ],
  'Stamina': [
    { stat: 'recovery', value: 25, enhancementType: 'enduranceMod' },
  ],
};

/**
 * Log this silo's contributions. It lived in `calc-debug.ts` until FORK-2, which is
 * how a beta-only debug hook came to fork a file both repos share: the rebuild has no
 * legacy entry point, so it never had the caller, so the twelve lines never ported.
 * Nothing outside this file calls it — it belongs to the silo, not to the debug hub.
 */
function debugFitnessPower(
  name: string,
  effects: { stat: string; base: number; enhanced: number; enhBonus: number }[]
): void {
  if (!isCalcDebugEnabled()) return;
  debugGroup(`${name}`);
  for (const e of effects) {
    debugFormula(`${e.stat}: ${formatDebugNum(e.base)}% base × (1 + ${formatDebugNum(e.enhBonus * 100)}% enh) = ${formatDebugNum(e.enhanced)}%`);
  }
  debugGroupEnd();
}

/**
 * Apply bonuses from inherent fitness powers
 * Fitness powers provide base stats that can be enhanced with slotted enhancements
 */
function applyFitnessPowerBonuses(
  build: Build,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  globalIOLevel: number,
  alphaBonuses: EnhancementBonuses = {},
  alphaEdBypass: EnhancementBonuses = {},
  exemplarLevel?: number
): void {
  const fitnessPowers = (build.inherents || []).filter(
    (p) => p.inherentCategory === 'fitness'
  );

  for (const power of fitnessPowers) {
    const effects = FITNESS_POWER_EFFECTS[power.internalName];
    if (!effects) continue;

    // Calculate enhancement bonus for this power (see comment on
    // applyActivePowerBonuses for why we use combineWithAlphaED here).
    const enhBonuses = combineWithAlphaED(
      power,
      globalIOLevel,
      getIOSet,
      alphaBonuses,
      alphaEdBypass,
      exemplarLevel
    );

    const _fitnessDebugEffects: { stat: string; base: number; enhanced: number; enhBonus: number }[] = [];
    for (const effect of effects) {
      // Get the enhancement multiplier for this effect's type
      const enhMultiplier = 1 + (enhBonuses[effect.enhancementType] || 0);

      // Movement stats resolve through the AT table at the build level (Swift's
      // RunningSpeed scales by Melee_SpeedRunning, etc.) just like active-power
      // movement — see resolveMovementPercent. Non-movement stats (regen,
      // recovery) keep their fixed Melee_Ones value. Falls back to the hardcoded
      // value if the inherent def somehow lacks the effect.
      let baseValue = effect.value;
      if (FITNESS_MOVEMENT_STATS.has(effect.stat)) {
        const movEffect = (power.effects as Record<string, ScalarOrScaled> | undefined)?.[effect.stat];
        if (movEffect !== undefined) {
          baseValue = resolveMovementPercent(movEffect, effect.stat, build.archetype.id || '', globalIOLevel);
        }
      }

      // Calculate final value: base * (1 + enhancement%)
      const finalValue = baseValue * enhMultiplier;

      // Apply to global bonuses
      global[effect.stat] += finalValue;

      // Track in breakdown
      addToBreakdown(breakdown, effect.stat, {
        name: power.name,
        value: finalValue,
        type: 'inherent',
      });

      if (isCalcDebugEnabled()) {
        _fitnessDebugEffects.push({ stat: effect.stat, base: effect.value, enhanced: finalValue, enhBonus: enhBonuses[effect.enhancementType] || 0 });
      }
    }
    if (isCalcDebugEnabled() && _fitnessDebugEffects.length > 0) {
      debugFitnessPower(power.name, _fitnessDebugEffects);
    }
  }
}

// ============================================
// ACCOLADE PROCESSING
// ============================================

/**
 * Apply bonuses from accolades
 * Accolades provide flat or percentage bonuses to HP and endurance
 */
// ============================================
// PROC PROCESSING
// ============================================

interface SlottedProc {
  procName: string;
  setName: string;
  powerName: string;
  powerType: string;
  isActive: boolean;
  /** Slot array index — keys per-proc overrides (`${powerName}:${slotIndex}`). */
  slotIndex: number;
}

/** Minimal power interface for proc collection */
interface PowerForProcScan {
  name: string;
  powerType?: string;
  isActive?: boolean;
  slots?: (Enhancement | null)[];
  // Primary/secondary powers carry execution stats under `stats`; pool/epic
  // powers carry them under `effects` (transformPoolPower never builds a
  // `stats` object). Both fields are read with stats winning when present.
  stats?: { recharge?: number; castTime?: number; radius?: number; arc?: number };
  effects?: { recharge?: number; castTime?: number; radius?: number; arc?: number };
}

/**
 * Collect all "always-on" procs from the build
 * These are Global and Proc120s enhancements slotted in Auto or active Toggle powers
 */
function collectAlwaysOnProcs(build: Build): SlottedProc[] {
  const procs: SlottedProc[] = [];

  const looksLikeLegacyProcSlot = (slotName: string, procData: NonNullable<ReturnType<typeof findProcData>>): boolean => {
    const slot = (slotName || '').toLowerCase();
    const io = (procData.ioName || '').toLowerCase();
    if (!slot || !io) return false;
    if (slot === io) return true;
    // Legacy extractor names often prepend aspect text, e.g.
    // "Recharge/Resistance Bonus" for ioName "Resistance Bonus".
    if (slot.includes(io)) return true;
    // Placeholder names emitted for unresolved proc pieces.
    if (slot === 'chance' || slot === 'recharge/chance') return true;
    return false;
  };

  const processPower = (power: PowerForProcScan) => {
    if (!power.slots) return;

    const powerType = power.powerType?.toLowerCase() || '';
    const isAlwaysActive = powerType === 'auto' || (powerType === 'toggle' && power.isActive);

    for (let slotIndex = 0; slotIndex < power.slots.length; slotIndex++) {
      const slot = power.slots[slotIndex];
      if (!slot || slot.type !== 'io-set') continue;
      const ioSlot = slot as IOSetEnhancement;

      // Look up proc data
      const procData = findProcData(ioSlot.name, ioSlot.setName);
      if (!procData) continue;

      // Primary path: explicit proc flag.
      // Legacy safety net: some old extracted pieces shipped with proc:false
      // despite being real always-on globals. Accept only when the slot name
      // still clearly identifies the proc entry to avoid broad false positives.
      if (!ioSlot.isProc && !looksLikeLegacyProcSlot(ioSlot.name, procData)) continue;

      // Only include if it's an always-on proc type
      if (!isProcAlwaysOn(procData)) continue;

      // For Global procs, they're always active regardless of power type
      // For Proc120s, they need to be in an Auto or active Toggle power
      if (procData.type === 'Global' || isAlwaysActive) {
        procs.push({
          procName: ioSlot.name,
          setName: ioSlot.setName,
          powerName: power.name,
          powerType: powerType,
          isActive: true,
          slotIndex,
        });
      }
    }
  };

  // Process all power categories
  for (const power of build.primary?.powers || []) {
    processPower(power);
  }
  for (const power of build.secondary?.powers || []) {
    processPower(power);
  }
  for (const pool of build.pools || []) {
    for (const power of pool.powers) {
      processPower(power);
    }
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) {
      processPower(power);
    }
  }
  for (const power of build.inherents || []) {
    processPower(power);
  }

  return procs;
}

/**
 * Helper to apply a single proc effect category to global bonuses
 */
function applySingleProcEffect(
  category: string,
  value: number | undefined,
  valueMax: number | undefined,
  effectType: string | undefined,
  sourceName: string,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  powerName: string | undefined,
  stealthContribs: StealthContribution[]
): void {
  if (value === undefined) return;
  if (isCalcDebugEnabled()) {
    debugFormula(`${sourceName}: ${category}${effectType ? ` (${effectType})` : ''} +${formatDebugNum(value)}%`);
  }

  switch (category) {
    case 'Recovery':
      global.recovery += value;
      addToBreakdown(breakdown, 'recovery', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Regeneration':
      global.regeneration += value;
      addToBreakdown(breakdown, 'regeneration', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Endurance':
      // Endurance procs grant a % of max end - treat as recovery boost for calculations
      // Note: This is a one-time grant when the proc fires, not a sustained buff
      // For PPM procs like Performance Shifter, this doesn't apply to steady-state calculations
      // But for Panacea's secondary effect in the combined parsing, we include it for display
      global.recovery += value;
      addToBreakdown(breakdown, 'recovery', {
        name: `${sourceName} (+End)`,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Heal':
      // Heal procs grant a % of max HP per proc — treat as effective regeneration
      // For always-on procs (Proc120s), the heal fires every 10s, so it's a sustained regen contribution
      global.regeneration += value;
      addToBreakdown(breakdown, 'regeneration', {
        name: `${sourceName} (+HP)`,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'MaxHP':
      global.maxHP += value;
      addToBreakdown(breakdown, 'maxHP', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Defense':
      // Apply typed defense when provided; "All" expands to all entries.
      if (effectType?.toLowerCase() === 'all') {
        const defTypes: (keyof GlobalBonuses)[] = [
          'defMelee', 'defRanged', 'defAoE',
          'defSmashing', 'defLethal', 'defFire', 'defCold',
          'defEnergy', 'defNegative', 'defPsionic', 'defToxic'
        ];
        for (const defType of defTypes) {
          global[defType] += value;
          addToBreakdown(breakdown, defType as string, {
            name: sourceName,
            value,
            type: 'proc',
            powerName,
          });
        }
      } else {
        const specificDefMap: Record<string, keyof GlobalBonuses> = {
          melee: 'defMelee', ranged: 'defRanged', aoe: 'defAoE', area: 'defAoE',
          smashing: 'defSmashing', lethal: 'defLethal', fire: 'defFire', cold: 'defCold',
          energy: 'defEnergy', negative: 'defNegative', psionic: 'defPsionic', toxic: 'defToxic',
        };
        const defKey = specificDefMap[effectType?.toLowerCase() || ''];
        if (defKey) {
          global[defKey] += value;
          addToBreakdown(breakdown, defKey as string, {
            name: sourceName,
            value,
            type: 'proc',
            powerName,
          });
        }
      }
      break;

    case 'Absorb':
      global.absorb += value;
      addToBreakdown(breakdown, 'absorb', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Resistance': {
      const resLower = effectType?.toLowerCase() || '';
      if (resLower === 'all') {
        const resTypes: (keyof GlobalBonuses)[] = [
          'resSmashing', 'resLethal', 'resFire', 'resCold',
          'resEnergy', 'resNegative', 'resPsionic', 'resToxic'
        ];
        for (const resType of resTypes) {
          global[resType] += value;
          addToBreakdown(breakdown, resType as string, {
            name: sourceName,
            value,
            type: 'proc',
            powerName,
          });
        }
      } else {
        // Specific resistance type (e.g., "Psionic", "Fire")
        const specificResMap: Record<string, keyof GlobalBonuses> = {
          smashing: 'resSmashing', lethal: 'resLethal',
          fire: 'resFire', cold: 'resCold',
          energy: 'resEnergy', negative: 'resNegative',
          psionic: 'resPsionic', toxic: 'resToxic',
        };
        const resKey = specificResMap[resLower];
        if (resKey) {
          global[resKey] += value;
          addToBreakdown(breakdown, resKey as string, {
            name: sourceName,
            value,
            type: 'proc',
            powerName,
          });
        }
      }
      break;
    }

    case 'ToHit':
      global.toHit += value;
      addToBreakdown(breakdown, 'toHit', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Recharge':
      global.recharge += value;
      addToBreakdown(breakdown, 'recharge', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    // Always-on global +Damage (e.g. Liberty's Belt: Resistance/Global Damage
    // Bonus) — the LotG +Recharge analogue for damage. Rule-of-5 tracked under
    // the 'damage' stat via PROC_CATEGORY_TO_STAT.
    case 'Damage':
      global.damage += value;
      addToBreakdown(breakdown, 'damage', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'RunSpeed':
      global.runSpeed += value;
      addToBreakdown(breakdown, 'runSpeed', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'MezResist':
      global.mezResist += value;
      addToBreakdown(breakdown, 'mezResist', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'SlowResistance':
      global.debuffResistSlow += value;
      addToBreakdown(breakdown, 'debuffResistSlow', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'RechargeResistance':
      global.debuffResistRecharge += value;
      addToBreakdown(breakdown, 'debuffResistRecharge', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    // One named effect covering both drain axes: the source auto power carries
    // same-scale templates on Recovery and Endurance (Synapse's Agility's 6th
    // piece), so the category fans out to both debuff resists.
    case 'EnduranceDrainResistance':
      global.debuffResistEndurance += value;
      global.debuffResistRecovery += value;
      addToBreakdown(breakdown, 'debuffResistEndurance', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      addToBreakdown(breakdown, 'debuffResistRecovery', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'KnockbackProtection':
      global.protKnockback += value;
      addToBreakdown(breakdown, 'protKnockback', {
        name: sourceName,
        value,
        type: 'proc',
        powerName,
      });
      break;

    case 'Stealth': {
      // Stealth IO procs (Celerity, Unbounded Leap, Freebird, …) are their own
      // additive group — they land on top of any stealth power toward the
      // invisibility cap. A stealth ProcEffect carries the PvE radius in `value`
      // and the PvP radius in `valueMax`, but stealth IOs split these across two
      // effects:
      //   { value: 30 }                 → PvE-only  (30 ft)
      //   { value: 300, valueMax: 300 } → PvP-only  (value duplicates valueMax)
      // Guard the PvE side on `value !== valueMax` so the duplicated PvP
      // magnitude never leaks into the PvE radius. Collected with a null
      // stackKey (additive) and resolved by resolveStealthRadius.
      const pvp = valueMax !== undefined ? valueMax : 0;
      const pve = valueMax === undefined ? value : (value !== valueMax ? value : 0);
      stealthContribs.push({
        stackKey: null,
        pve,
        pvp,
        sourceName,
        type: 'proc',
        powerName,
      });
      break;
    }

    // Other categories (Damage, Control, Debuff, etc.) are not "always-on" stats
    default:
      break;
  }
}

/**
 * Maps proc effect categories to the stat key used in Rule of 5 tracking.
 * Null means the category does not contribute a trackable stat bonus.
 */
const PROC_CATEGORY_TO_STAT: Record<string, string | null> = {
  Recovery:          'recovery',
  Regeneration:      'regeneration',
  Heal:              'regeneration', // Heal procs contribute to effective regen rate
  Endurance:         'recovery',     // Treated as recovery in calculations
  Absorb:            'absorb',
  Recharge:          'recharge',
  RunSpeed:          'runspeed',
  Damage:            'damage',       // Always-on +Damage globals (Liberty's Belt), Rule-of-5 capped
};

/** Maps proc effect categories to procSettings keys */
const PROC_CATEGORY_TO_SETTING: Record<string, keyof ProcSettings> = {
  Recovery: 'recovery',
  Endurance: 'recovery',
  Regeneration: 'regeneration',
  Heal: 'regeneration',
  Recharge: 'recharge',
  ToHit: 'toHit',
  Defense: 'defense',
  Resistance: 'resistance',
  BuildUp: 'buildUp',
  RunSpeed: 'movement',
  KnockbackProtection: 'movement',
  MezResist: 'movement',
  SlowResistance: 'movement',
  RechargeResistance: 'movement',
};

/** Check if a proc category is enabled in procSettings */
function isProcCategoryEnabled(category: string, procSettings?: ProcSettings): boolean {
  if (!procSettings) return true; // All enabled by default
  const settingKey = PROC_CATEGORY_TO_SETTING[category];
  if (!settingKey) return true; // Unknown categories are always enabled
  return procSettings[settingKey];
}

/**
 * Apply bonuses from always-on procs (Global and Proc120s in Auto/Toggle powers).
 * Rule of 5 is enforced by sharing the same BonusTracking as set bonuses.
 */
function applyProcBonuses(
  build: Build,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  procSettings: ProcSettings | undefined,
  stealthContribs: StealthContribution[],
): void {
  // Procs use their own Rule of 5 tracking, separate from set bonuses.
  // In CoH, unique IO procs (e.g., LotG +Recharge) and set bonuses have independent stacking limits.
  const tracking = createBonusTracking();
  const procs = collectAlwaysOnProcs(build);

  for (const proc of procs) {
    const procData = findProcData(proc.procName, proc.setName);
    if (!procData) {
      if (isCalcDebugEnabled()) {
        console.warn(`[proc] No proc data found for "${proc.procName}" (set: ${proc.setName})`);
      }
      continue;
    }

    const sourceName = `${proc.setName}: ${proc.procName}`;

    // Binary-sourced structured effects (falls back to the mechanics parse).
    // Each effect goes through the same category-filter + Rule-of-5 gate.
    const overrideKey = procOverrideKey(proc.powerName, proc.slotIndex);
    const override = build.procOverrides?.[overrideKey];

    for (const eff of getProcEffects(procData)) {
      // Variable procs (self-stacking buffs, HP-scaling) are owned exclusively by
      // applyVariableProcBonuses — skip them here so they aren't double-counted.
      if (getProcControlType(eff) !== 'toggle') continue;
      // Per-proc override wins; the global category toggle is the default gate
      // for a proc the user hasn't explicitly touched.
      const enabled = override ? override.enabled : isProcCategoryEnabled(eff.category, procSettings);
      if (eff.value === undefined || !enabled) continue;
      // Skip pet/ally buffs (MM auras) and chance-gated procs — they don't
      // contribute a steady bonus to the PLAYER's dashboard.
      if ('target' in eff && eff.target === 'pets') continue;
      if ('chance' in eff && eff.chance !== undefined && eff.chance < 1) continue;
      const stat = eff.category ? PROC_CATEGORY_TO_STAT[eff.category] : undefined;
      const allowed = stat === undefined
        ? true  // No stat mapping: not subject to Rule of 5 (e.g., KB protection)
        : stat === null
          ? false // Explicitly excluded
          : trackBonus(tracking, stat, eff.value, sourceName, proc.powerName);

      if (allowed) {
        applySingleProcEffect(eff.category, eff.value, eff.valueMax, eff.effectType, sourceName, global, breakdown, proc.powerName, stealthContribs);
      } else if (stat) {
        // Rule of 5 rejected — add a capped entry so it appears in the tooltip
        addToBreakdown(breakdown, stat, {
          name: sourceName,
          value: eff.value,
          type: 'proc',
          capped: true,
          powerName: proc.powerName,
        });
      }
    }
  }

  // Also collect PPM procs from Auto/Toggle powers and calculate their effective contribution
  applyPPMProcBonuses(build, global, breakdown, procSettings);
}

/**
 * Apply bonuses from PPM-based procs in Auto/Toggle powers
 * These procs fire with a calculable frequency, contributing a rate-based bonus
 */
function applyPPMProcBonuses(
  build: Build,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  procSettings?: ProcSettings,
): void {
  // BASE_RECOVERY_RATE imported from enhancement-values.ts

  const processPower = (power: PowerForProcScan) => {
    if (!power.slots) return;

    const powerType = power.powerType?.toLowerCase() || '';
    const isAutoOrToggle = powerType === 'auto' || powerType === 'toggle';
    const isActive = powerType === 'auto' || (powerType === 'toggle' && power.isActive);

    // Only process active Auto/Toggle powers for PPM procs
    if (!isAutoOrToggle || !isActive) return;

    for (const slot of power.slots) {
      if (!slot || slot.type !== 'io-set') continue;
      const ioSlot = slot as IOSetEnhancement;
      if (!ioSlot.isProc) continue;

      const procData = findProcData(ioSlot.name, ioSlot.setName);
      if (!procData) continue;

      // Skip if not a PPM proc (Global and Proc120s are handled elsewhere)
      if (procData.type !== 'Proc' || procData.ppm === null) continue;

      const effects = getProcEffects(procData);
      const procsPerMin = calculateAutoToggleProcsPerMinute(procData.ppm);
      const sourceName = `${procData.setName}: ${ioSlot.name} (PPM)`;

      // Helper to apply a single PPM effect contribution
      const applyPPMEffect = (category: string | undefined, value: number | undefined, duration: number | undefined, suffix?: string) => {
        if (!category || value === undefined) return;
        if (!isProcCategoryEnabled(category, procSettings)) return;
        const label = suffix ? `${sourceName} (${suffix})` : sourceName;

        switch (category) {
          case 'Endurance': {
            // Endurance proc: X% of max end per proc
            // Convert to recovery equivalent: (value% × procsPerMin) / 60 / BASE_RECOVERY_RATE × 100
            const endPerSec = (value * procsPerMin) / 60;
            const recoveryEquivalent = (endPerSec / BASE_RECOVERY_RATE) * 100;
            global.recovery += recoveryEquivalent;
            addToBreakdown(breakdown, 'recovery', { name: label, value: recoveryEquivalent, type: 'proc' });
            break;
          }
          case 'Heal': {
            // PPM heal procs (e.g. Panacea) grant a chunk of HP on each proc fire.
            // The game does NOT count these as steady-state regeneration in Combat Attributes.
            // Only percentage-based regen buffs (Proc120s like Numina's) contribute to regen rate.
            // Skip adding to global.regeneration to match game behavior.
            break;
          }
          case 'Recovery': {
            const recoveryVal = (value * procsPerMin) / 60;
            const recoveryPct = (recoveryVal / BASE_RECOVERY_RATE) * 100;
            global.recovery += recoveryPct;
            addToBreakdown(breakdown, 'recovery', { name: label, value: recoveryPct, type: 'proc' });
            break;
          }
          case 'Regeneration': {
            if (duration && duration > 0) {
              // Stacking DURATION regen BUFF (e.g. Unrelenting Fury: +15% /
              // Superior +20% Regeneration for ~10.25s, StackType=Stack). `value`
              // is the per-stack regen %, NOT a one-shot HP grant — do NOT run it
              // through the instant-grant rate conversion (that inflates +20% into
              // ~+430%). The steady-state contribution is the per-stack value ×
              // the expected number of concurrently-active stacks, which by
              // Little's law is arrivalRate × buff lifetime. procsPerMin is already
              // area/rate-clamped (≤ ~5.4 for toggles), so this lands near the low
              // end of the buff's transient 1–5-stack range — the honest average.
              const avgStacks = (procsPerMin / 60) * duration;
              const effectivePct = value * avgStacks;
              global.regeneration += effectivePct;
              addToBreakdown(breakdown, 'regeneration', { name: label, value: effectivePct, type: 'proc' });
            } else {
              // Genuine instant per-proc regen grant (no buff duration): convert
              // the one-shot to an equivalent sustained regen rate.
              const regenVal = (value * procsPerMin) / 60;
              const regenPct = (regenVal / BASE_REGEN_RATE) * 100;
              global.regeneration += regenPct;
              addToBreakdown(breakdown, 'regeneration', { name: label, value: regenPct, type: 'proc' });
            }
            break;
          }
          // Other PPM categories (Damage, Control, etc.) don't contribute to dashboard stats
        }
      };

      // Apply each structured effect (e.g., Panacea's +HP + +End)
      for (const e of effects) {
        applyPPMEffect(e.category, e.value, e.duration, effects.length > 1 ? e.category : undefined);
      }
    }
  };

  // Process all power categories
  for (const power of build.primary?.powers || []) {
    processPower(power);
  }
  for (const power of build.secondary?.powers || []) {
    processPower(power);
  }
  for (const pool of build.pools || []) {
    for (const power of pool.powers) {
      processPower(power);
    }
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) {
      processPower(power);
    }
  }
  for (const power of build.inherents || []) {
    processPower(power);
  }
}

// ============================================
// BUILD UP PROC PROCESSING
// ============================================

/**
 * Apply average contributions from Build Up procs (Decimation, Gaussian's) in click powers.
 * These are PPM-based procs that grant a temporary +Damage and +ToHit buff.
 * We calculate the expected uptime across all click powers and add the average contribution.
 */
function applyBuildUpProcBonuses(
  build: Build,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
): void {
  // Collect all click powers that have Build Up procs
  const buildUpProcs: { procName: string; setName: string; ppm: number; damage: number; toHit: number; duration: number; powerName: string; baseRecharge: number; castTime: number; radius: number; arcDegrees: number }[] = [];

  const processPower = (power: PowerForProcScan) => {
    if (!power.slots) return;
    const powerType = power.powerType?.toLowerCase() || '';
    // Build Up procs only fire in click powers (not auto/toggle)
    if (powerType === 'auto' || powerType === 'toggle') return;
    if (!power.isActive) return;

    // Fall back to `effects` so pool/epic click powers (Wall of Force,
    // Spring Attack, etc.) contribute Build Up proc averages instead of
    // silently using the radius=0 / recharge=4 defaults. arcToDegrees
    // handles the radians-stored arc from both sources.
    const baseRecharge = power.stats?.recharge ?? power.effects?.recharge ?? 4;
    const castTime = power.stats?.castTime ?? power.effects?.castTime ?? 1;
    const radius = power.stats?.radius ?? power.effects?.radius ?? 0;
    const rawArc = power.stats?.arc ?? power.effects?.arc;
    const arcDegrees = radius > 0 ? (arcToDegrees(rawArc) || 360) : 360;

    for (const slot of power.slots) {
      if (!slot || slot.type !== 'io-set') continue;
      const ioSlot = slot as IOSetEnhancement;
      if (!ioSlot.isProc) continue;

      const procData = findProcData(ioSlot.name, ioSlot.setName);
      if (!procData || procData.type !== 'Proc' || procData.ppm === null) continue;

      // A Build Up proc is a self-buff Damage effect with a duration (regular
      // damage procs carry a valueMax range and no duration), plus a ToHit buff.
      // SELF-buff is the operative word: Soulbound Allegiance is a pet-set piece
      // slotted in a summon power, so its Build Up lands on the PET (target 'pets')
      // and must not reach the player's dashboard — the same skip the always-on and
      // variable proc passes already apply.
      const effects = getProcEffects(procData);
      const isSelf = (e: ProcEffect) => e.target === undefined || e.target === 'self';
      const dmgE = effects.find((e) => e.category === 'Damage' && e.duration !== undefined && isSelf(e));
      if (!dmgE) continue;
      const toHitE = effects.find((e) => e.category === 'ToHit' && isSelf(e));

      buildUpProcs.push({
        procName: ioSlot.name,
        setName: procData.setName,
        ppm: procData.ppm,
        damage: dmgE.value || 0,
        toHit: toHitE?.value || 0,
        duration: dmgE.duration || 10,
        powerName: power.name,
        baseRecharge,
        castTime,
        radius,
        arcDegrees,
      });
    }
  };

  // Process all power categories
  for (const power of build.primary?.powers || []) processPower(power);
  for (const power of build.secondary?.powers || []) processPower(power);
  for (const pool of build.pools || []) {
    for (const power of pool.powers) processPower(power);
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) processPower(power);
  }
  for (const power of build.inherents || []) processPower(power);

  if (buildUpProcs.length === 0) return;

  // For each Build Up proc, calculate the expected uptime
  // Build Up procs are unique — only one instance can be active at a time
  // We calculate the average contribution across all host powers
  // Simplified model: highest single-proc uptime (since buff doesn't stack with itself)
  let bestDamageContrib = 0;
  let bestToHitContrib = 0;
  let bestSourceName = '';

  for (const proc of buildUpProcs) {
    const procChance = calculateProcChance(proc.ppm, proc.baseRecharge, proc.castTime, proc.radius, proc.arcDegrees);
    // Expected uptime: assume power fires on recharge. Rough cycle = recharge + castTime.
    // Average buff uptime ≈ procChance × min(duration / cycleTime, 1)
    // For a rough estimate, use: avgDamage = procChance × damageValue
    // This assumes the buff duration covers most of the next cycle
    const avgDamage = procChance * proc.damage;
    const avgToHit = procChance * proc.toHit;
    const sourceName = `${proc.setName}: ${proc.procName} (in ${proc.powerName})`;

    if (avgDamage > bestDamageContrib) {
      bestDamageContrib = avgDamage;
      bestToHitContrib = avgToHit;
      bestSourceName = sourceName;
    }
  }

  // Apply the best single Build Up proc contribution
  if (bestDamageContrib > 0) {
    global.damage += bestDamageContrib;
    addToBreakdown(breakdown, 'damage', {
      name: bestSourceName,
      value: bestDamageContrib,
      type: 'proc',
    });
  }
  if (bestToHitContrib > 0) {
    global.toHit += bestToHitContrib;
    addToBreakdown(breakdown, 'toHit', {
      name: bestSourceName,
      value: bestToHitContrib,
      type: 'proc',
    });
  }
}

/**
 * Apply variable-proc bonuses — self-stacking buffs (Might of the Tanker) and
 * HP-scaling globals (Reactive Defenses). These effects are skipped by
 * applyProcBonuses / applyPPMProcBonuses and owned exclusively here so their
 * per-proc override (enable + stack / %HP slider) drives the contribution.
 *
 * Magnitude resolution:
 *  - `scaleTable` effects ("By the Slotted Power", e.g. MotT) are `value ×
 *    getTableValue(archetype, scaleTable, level)` — the AT modifier the generator
 *    can't apply (MotT: 50 × 0.10 Tanker Melee_Res_Dmg = 5%/stack).
 *  - plain effects (Reactive Defenses) use `value` as the resolved literal.
 * Expected stacks (Auto default) use the host power's base recharge/geometry, the
 * same rough model as applyBuildUpProcBonuses (enhanced recharge not yet folded in).
 */
function applyVariableProcBonuses(
  build: Build,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  procSettings: ProcSettings | undefined,
  stealthContribs: StealthContribution[],
): void {
  const archetype = build.archetype?.id || '';
  const level = build.level ?? 50;

  const resolveMagnitude = (raw: number | undefined, scaleTable: string | undefined): number => {
    if (raw === undefined) return 0;
    if (!scaleTable) return raw; // already a resolved literal
    const mod = getTableValue(archetype, scaleTable, level) ?? 0;
    return raw * mod;
  };

  const processPower = (power: PowerForProcScan) => {
    if (!power.slots) return;
    // A Proc-type variable buff contributes when its host is in use: auto is
    // always on; a toggle only while toggled on; a click ATTACK is assumed
    // in-rotation (click powers carry isActive === undefined — see buildStore's
    // add-power default — and their expected-stacks model already accounts for
    // cast frequency). Only an explicitly toggled-OFF host (isActive === false)
    // suppresses it. Global scaling procs (Reactive Defenses) are always on.
    const hostSuppressed = power.isActive === false;

    for (let slotIndex = 0; slotIndex < power.slots.length; slotIndex++) {
      const slot = power.slots[slotIndex];
      if (!slot || slot.type !== 'io-set') continue;
      const ioSlot = slot as IOSetEnhancement;
      if (!ioSlot.isProc) continue;

      const procData = findProcData(ioSlot.name, ioSlot.setName);
      if (!procData) continue;

      const overrideKey = procOverrideKey(power.name, slotIndex);
      const override = build.procOverrides?.[overrideKey];

      for (const eff of getProcEffects(procData)) {
        const controlType = getProcControlType(eff);
        if (controlType === 'toggle') continue; // handled by applyProcBonuses
        if (procData.type !== 'Global' && hostSuppressed) continue;

        // Per-proc override wins; otherwise the global category toggle gates.
        const effOverride =
          override ?? { enabled: isProcCategoryEnabled(eff.category, procSettings), mode: 'auto' as const };
        if (!effOverride.enabled) continue;

        const perUnitValue = resolveMagnitude(eff.value, eff.scaleTable);
        const capValue = eff.valueMax !== undefined ? resolveMagnitude(eff.valueMax, eff.scaleTable) : undefined;

        const contribution = resolveProcContribution({
          controlType,
          perUnitValue,
          capValue,
          maxStacks: eff.maxStacks,
          override: effOverride,
        });
        if (contribution === 0) continue;

        // Annotate the breakdown row with the resolved discrete stacks / %HP so
        // the dashboard tooltip explains the number (stacks default to 1).
        let detail = '';
        if (controlType === 'stacks') {
          const stacks =
            effOverride.mode === 'stacks'
              ? Math.max(0, Math.min(eff.maxStacks ?? 1, effOverride.stacks ?? 0))
              : Math.min(DEFAULT_STACK_COUNT, eff.maxStacks ?? 1);
          detail = ` (${stacks} stack${stacks === 1 ? '' : 's'})`;
        } else if (controlType === 'hp' && override) {
          detail = ` (@${effOverride.mode === 'hp' ? effOverride.hpPct ?? 100 : 100}% HP)`;
        }
        const sourceName = `${procData.setName}: ${ioSlot.name}${detail}`;
        applySingleProcEffect(
          eff.category,
          contribution,
          undefined,
          eff.effectType,
          sourceName,
          global,
          breakdown,
          power.name,
          stealthContribs,
        );
      }
    }
  };

  for (const power of build.primary?.powers || []) processPower(power);
  for (const power of build.secondary?.powers || []) processPower(power);
  for (const pool of build.pools || []) {
    for (const power of pool.powers) processPower(power);
  }
  if (build.epicPool) {
    for (const power of build.epicPool.powers) processPower(power);
  }
  for (const power of build.inherents || []) processPower(power);
}

// ============================================
// ACCOLADE PROCESSING
// ============================================

// Accolades are ordinary auto-on Self powers (`Temporary_Powers.Accolades`). Their +Max HP /
// +Max End contribution is READ FROM THE POWER via the same resolvers processPower uses —
// never a hand-authored bonus table (the removed `src/data/accolades.ts` silo mis-transcribed
// these; DATA-GAP ACCOLADE-1). A focused apply (rather than routing through processPower)
// keeps the dashboard's dedicated 'accolade' breakdown grouping. Accolades carry no slots, so
// no enhancement multiplier applies.
function applyAccoladeStats(
  accolades: AccoladePower[],
  archetypeId: string,
  buildLevel: number,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>
): void {
  for (const power of accolades) {
    // +Max HP — a flat 10% per scale point (the same rule as processPower). The enhanceable
    // and IgnoreStrength (flat, no +Healing) halves resolve through disjoint atom queries;
    // accolade MaxHP atoms are all IgnoreStrength, but read both to stay faithful to the data.
    for (const half of [maxHPBuffValue(power), maxHPBuffValue(power, { ignoreStrength: true })]) {
      if (half === undefined) continue;
      const scale = typeof half === 'number' ? half : half.scale;
      const value = scale * 10;
      global.maxHP += value;
      addToBreakdown(breakdown, 'maxHP', { name: power.name, value, type: 'accolade' });
      if (isCalcDebugEnabled()) debugAccolade(power.name, 'maxHP', value);
    }

    // +Max Endurance — flat endurance points (scale already absolute, e.g. scale 5 = +5 end).
    // BPORT11: atom-native, like the accolade's +MaxHP beside it. All 28 accolade carriers
    // across the four datasets agree with the bag exactly.
    const accoladeMaxEnd = maxEndBuffValue(power);
    if (accoladeMaxEnd !== undefined) {
      const value = resolveScaledEffect(accoladeMaxEnd, archetypeId, buildLevel);
      global.maxEndurance += value;
      addToBreakdown(breakdown, 'maxEndurance', { name: power.name, value, type: 'accolade' });
      if (isCalcDebugEnabled()) debugAccolade(power.name, 'maxEndurance', value);
    }
  }
}

/**
 * Apply a block of Hybrid stat bonuses (passive, frontLoaded, or perTarget) to global bonuses.
 * Stat keys use GlobalBonuses field names (e.g. 'regeneration', 'resSmashing', 'defMelee').
 * Special keys: 'statusResistance' applies to all mez resist types, 'enduranceDiscount' applies to end discount.
 */
function applyHybridStatBlock(
  stats: Record<string, number>,
  sourceName: string,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
): void {
  for (const [stat, decimal] of Object.entries(stats)) {
    // Status resistance from Control passive applies to all mez types
    if (stat === 'statusResistance') {
      const value = decimal * 100;
      const mezTypes: (keyof GlobalBonuses)[] = [
        'mezResistHold', 'mezResistStun', 'mezResistImmobilize',
        'mezResistSleep', 'mezResistConfuse', 'mezResistFear',
      ];
      for (const mezKey of mezTypes) {
        global[mezKey] += value;
        addToBreakdown(breakdown, mezKey, { name: sourceName, value, type: 'incarnate' });
      }
      continue;
    }

    // Endurance discount (e.g., Hybrid Support T4 passive) flows into the
    // canonical EndDisc accumulator (global.endurance) — same bucket set
    // bonuses use — so the divisor formula in applyToggleEndCosts and the
    // dashboard "End Disc" stat both pick it up.
    if (stat === 'enduranceDiscount') {
      const value = decimal * 100;
      global.endurance += value;
      addToBreakdown(breakdown, 'endurance', { name: sourceName, value, type: 'incarnate' });
      continue;
    }

    // Mez protection uses raw magnitude (not percentage)
    if (stat.startsWith('prot')) {
      const key = stat as keyof GlobalBonuses;
      if (key in global) {
        global[key] += decimal; // Already magnitude, not percentage
        addToBreakdown(breakdown, key, { name: sourceName, value: decimal, type: 'incarnate' });
      }
      continue;
    }

    // All other stats: decimal → percentage (× 100)
    const value = decimal * 100;

    // Handle "All" defense/resistance keys by expanding to individual types
    if (stat === 'defenseAll') {
      const defKeys: (keyof GlobalBonuses)[] = [
        'defMelee', 'defRanged', 'defAoE',
        'defSmashing', 'defLethal', 'defFire', 'defCold',
        'defEnergy', 'defNegative', 'defPsionic', 'defToxic',
      ];
      for (const k of defKeys) {
        global[k] += value;
        addToBreakdown(breakdown, k, { name: sourceName, value, type: 'incarnate' });
      }
      continue;
    }
    if (stat === 'resistanceAll') {
      const resKeys: (keyof GlobalBonuses)[] = [
        'resSmashing', 'resLethal', 'resFire', 'resCold',
        'resEnergy', 'resNegative', 'resPsionic', 'resToxic',
      ];
      for (const k of resKeys) {
        global[k] += value;
        addToBreakdown(breakdown, k, { name: sourceName, value, type: 'incarnate' });
      }
      continue;
    }

    const key = stat as keyof GlobalBonuses;
    if (key in global) {
      global[key] += value;
      addToBreakdown(breakdown, key, { name: sourceName, value, type: 'incarnate' });
    }
  }
}

/**
 * The active Genesis amplifier (Rebirth-only), or null. Genesis is toggleable,
 * so it only contributes when its slot toggle is on.
 */
function getActiveGenesis(
  incarnates: IncarnateBuildState | undefined,
  active: { genesis?: boolean },
): GenesisEffects | null {
  if (!incarnates?.genesis || active.genesis === false) return null;
  return getGenesisEffects(incarnates.genesis.powerId);
}

/**
 * Scale a Destiny effect block by a factor (used by Fate Genesis, which
 * amplifies Destiny slot ability effects). Only the numeric stat fields are
 * scaled — level shift and the duration metadata are left untouched.
 */
function scaleDestinyEffects(fx: DestinyEffects, factor: number): DestinyEffects {
  const skip = new Set(['levelShift', 'initialDuration', 'totalDuration']);
  const scaled: DestinyEffects = { ...fx };
  for (const [k, v] of Object.entries(fx)) {
    if (skip.has(k) || typeof v !== 'number') continue;
    (scaled as Record<string, number>)[k] = v * factor;
  }
  return scaled;
}

/**
 * Below level 45, a slotted+active Fate Genesis grants its exemplar buff (a
 * PBAoE ally buff: +Recharge / +Recovery, applied to self here as peak values
 * like Destiny). It's the only incarnate contribution that survives below 45 —
 * Verdict/Socket/Data exemplar powers are attacks/procs/pets (display-only).
 * The exemplar Fate buff feeds only +Recharge/+Recovery here; its mez protection
 * stays display-only (the full Destiny slot DOES wire mez protection into the
 * status-protection totals — see the Destiny block in applyIncarnateBonuses).
 */
function applyGenesisExemplarBuff(
  incarnates: IncarnateBuildState | undefined,
  active: { genesis?: boolean },
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
): void {
  const genesis = getActiveGenesis(incarnates, active);
  if (!genesis || genesis.tree !== 'fate') return;
  const ex = genesis.exemplarEffect;
  if (!ex || ex.kind !== 'buff') return;

  const sourceName = `${genesis.displayName} (exemplar)`;
  if (ex.stats.recharge) {
    const value = ex.stats.recharge * 100;
    global.recharge += value;
    addToBreakdown(breakdown, 'recharge', { name: sourceName, value, type: 'incarnate' });
  }
  if (ex.stats.recovery) {
    const value = ex.stats.recovery * 100;
    global.recovery += value;
    addToBreakdown(breakdown, 'recovery', { name: sourceName, value, type: 'incarnate' });
  }
}

/**
 * Apply bonuses from incarnate powers
 * Alpha provides enhancement bonuses, Destiny/Hybrid provide direct stat bonuses
 * Interface is proc-based and doesn't provide direct stats
 *
 * Genesis (Rebirth-only) amplifies a partner slot rather than granting flat
 * buffs: Socket adds player Max HP / Max End here; Fate scales the Destiny block
 * applied below. Verdict (Judgement damage) and Data (Lore pets) are display-only
 * and handled in the Info panel, not on the dashboard.
 */
function applyIncarnateBonuses(
  incarnates: IncarnateBuildState | undefined,
  incarnateActive: IncarnateActiveState | undefined,
  global: GlobalBonuses,
  breakdown: Map<string, DashboardStatBreakdown>,
  levelShiftActive = true,
  incarnatesSuppressed = false,
  destinyTime: number | null | undefined = undefined,
): void {
  if (!incarnates) return;
  const _debugBefore = isCalcDebugEnabled() ? { ...global } : null;

  // Default to all active if no active state provided
  const active = incarnateActive || {
    alpha: true,
    destiny: true,
    hybrid: true,
    interface: true,
    judgement: true,
    lore: true,
    genesis: true,
  };

  // Exemplared below 45: all normal incarnate bonuses are off. The ONLY
  // contribution that survives is Genesis's below-45 exemplar power — and of
  // the four trees, only Fate's grants a self stat-buff. The attack/proc/summon
  // are display-only (handled in the Info panel), so we return after it.
  if (incarnatesSuppressed) {
    applyGenesisExemplarBuff(incarnates, active, global, breakdown);
    return;
  }

  // Fate Genesis amplifies the Destiny block applied below.
  const genesis = getActiveGenesis(incarnates, active);
  const fateMultiplier = genesis?.tree === 'fate' ? genesis.tierPercent : 0;

  // Alpha - Enhancement bonuses are handled separately via getAlphaEnhancementBonuses()
  // They are applied in applyActivePowerBonuses() to boost power effectiveness
  // The dashboard doesn't show enhancement % directly, but the effect shows in toggle power stats

  // Destiny - Direct stat bonuses (defense, resistance, regen, recovery, etc.)
  // Note: These are initial/peak values since effects diminish over time
  if (incarnates.destiny && active.destiny) {
    // null → resolve at this power's sustained floor (the conservative default);
    // a number → that exact time; undefined → legacy flat peak.
    const effectiveDestinyTime = destinyTime === null
      ? getDestinySustainedFloorTime(incarnates.destiny.powerId)
      : destinyTime;
    let destinyEffects = getDestinyEffectsAtTime(incarnates.destiny.powerId, effectiveDestinyTime);
    if (destinyEffects) {
      // Alpha enhances Destiny buffs the game says accept its aspects — gated by
      // the power's boosts_allowed (Cardiac's resistance boosts Barrier's res,
      // etc.). Something Mids doesn't model. Only when the Alpha slot is active.
      let alphaEnhanced = false;
      if (incarnates.alpha && active.alpha) {
        const before = destinyEffects;
        destinyEffects = applyAlphaToDestiny(
          destinyEffects,
          getDestinyBoostsAllowed(incarnates.destiny.powerId),
          getAlphaEffects(incarnates.alpha.powerId),
        );
        alphaEnhanced = destinyEffects !== before;
      }

      // Fate Genesis (Rebirth) boosts Destiny ability effects by its tier %.
      if (fateMultiplier > 0) {
        destinyEffects = scaleDestinyEffects(destinyEffects, 1 + fateMultiplier);
      }
      const powerName =
        (fateMultiplier > 0
          ? `${incarnates.destiny.displayName} (+${(fateMultiplier * 100).toFixed(1)}% Genesis)`
          : incarnates.destiny.displayName) + (alphaEnhanced ? ' (+Alpha)' : '');

      // Defense All
      if (destinyEffects.defenseAll !== undefined) {
        const value = destinyEffects.defenseAll * 100;
        const defKeys: (keyof GlobalBonuses)[] = [
          'defMelee', 'defRanged', 'defAoE',
          'defSmashing', 'defLethal', 'defFire', 'defCold',
          'defEnergy', 'defNegative', 'defPsionic', 'defToxic',
        ];
        for (const key of defKeys) {
          global[key] += value;
          addToBreakdown(breakdown, key, { name: powerName, value, type: 'incarnate' });
        }
      }

      // Resistance All
      if (destinyEffects.resistanceAll !== undefined) {
        const value = destinyEffects.resistanceAll * 100;
        const resKeys: (keyof GlobalBonuses)[] = [
          'resSmashing', 'resLethal', 'resFire', 'resCold',
          'resEnergy', 'resNegative', 'resPsionic', 'resToxic',
        ];
        for (const key of resKeys) {
          global[key] += value;
          addToBreakdown(breakdown, key, { name: powerName, value, type: 'incarnate' });
        }
      }

      // Regeneration
      if (destinyEffects.regeneration !== undefined) {
        const value = destinyEffects.regeneration * 100;
        global.regeneration += value;
        addToBreakdown(breakdown, 'regeneration', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Recovery
      if (destinyEffects.recovery !== undefined) {
        const value = destinyEffects.recovery * 100;
        global.recovery += value;
        addToBreakdown(breakdown, 'recovery', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Damage
      if (destinyEffects.damage !== undefined) {
        const value = destinyEffects.damage * 100;
        global.damage += value;
        addToBreakdown(breakdown, 'damage', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // ToHit
      if (destinyEffects.toHit !== undefined) {
        const value = destinyEffects.toHit * 100;
        global.toHit += value;
        addToBreakdown(breakdown, 'toHit', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Recharge
      if (destinyEffects.recharge !== undefined) {
        const value = destinyEffects.recharge * 100;
        global.recharge += value;
        addToBreakdown(breakdown, 'recharge', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Max HP
      if (destinyEffects.maxHP !== undefined) {
        const value = destinyEffects.maxHP * 100;
        global.maxHP += value;
        addToBreakdown(breakdown, 'maxHP', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Max Endurance
      if (destinyEffects.maxEndurance !== undefined) {
        const value = destinyEffects.maxEndurance * 100;
        global.maxEndurance += value;
        addToBreakdown(breakdown, 'maxEndurance', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Healing Received (Res(Heal) buff, e.g. Incandescence). Stored positive
      // = more healing received. Its own total — NOT damage resistance (the
      // pre-fix `resistanceAll` mapping wrongly subtracted this from all 8 res
      // types).
      if (destinyEffects.healReceived !== undefined) {
        const value = destinyEffects.healReceived * 100;
        global.healReceived += value;
        addToBreakdown(breakdown, 'healReceived', {
          name: powerName,
          value,
          type: 'incarnate',
        });
      }

      // Mez Protection (Clarion) — a flat magnitude to all six control types.
      // Raw Mag points, NOT ×100 (that scaling is for percent stats only). This
      // was previously dropped by the converter and treated as display-only;
      // Clarion's whole purpose is mez protection, so it now feeds the
      // status-protection totals like any protection source.
      if (destinyEffects.mezProtection !== undefined) {
        const mag = destinyEffects.mezProtection;
        const protKeys: (keyof GlobalBonuses)[] = [
          'protHold', 'protStun', 'protImmobilize', 'protSleep', 'protConfuse', 'protFear',
        ];
        for (const key of protKeys) {
          (global[key] as number) += mag;
          addToBreakdown(breakdown, key, { name: powerName, value: mag, type: 'incarnate' });
        }
      }

      // Knockback/Knockup protection (Clarion) — its own total, raw Mag.
      if (destinyEffects.kbProtection !== undefined) {
        global.protKnockback += destinyEffects.kbProtection;
        addToBreakdown(breakdown, 'protKnockback', {
          name: powerName,
          value: destinyEffects.kbProtection,
          type: 'incarnate',
        });
      }

      // Run/Jump/Fly speed (Incandescence Radial) — one buff over all three
      // movement axes (decimal → percent).
      if (destinyEffects.runSpeed !== undefined) {
        const value = destinyEffects.runSpeed * 100;
        global.runSpeed += value;
        global.flySpeed += value;
        global.jumpHeight += value;
        addToBreakdown(breakdown, 'runSpeed', { name: powerName, value, type: 'incarnate' });
        addToBreakdown(breakdown, 'flySpeed', { name: powerName, value, type: 'incarnate' });
        addToBreakdown(breakdown, 'jumpHeight', { name: powerName, value, type: 'incarnate' });
      }
    }
  }

  // Hybrid — three-layer model: passive (always-on), frontLoaded (toggle baseline), perTarget (future slider)
  if (incarnates.hybrid) {
    const hybridEffects = getHybridEffects(incarnates.hybrid.powerId);
    if (hybridEffects) {
      const powerName = incarnates.hybrid.displayName;

      // Layer 1: Passive bonuses — always-on just by equipping
      applyHybridStatBlock(hybridEffects.passive, `${powerName} (passive)`, global, breakdown);

      // Layer 2: Front-loaded bonuses — active when toggle is on, no enemies required
      if (active.hybrid) {
        applyHybridStatBlock(hybridEffects.frontLoaded, powerName, global, breakdown);
      }

      // Layer 3: Per-target bonuses — deliberately NOT applied here. The engine gained this
      // layer with HYBRID-PT-1 and reads a foe count off the combat context; this file is the
      // frozen oracle and gains no feature the engine gains, so it stays at the pre-slider
      // behaviour. The two agree wherever the foe count is absent, which is every fixture.
    }
  }

  // Genesis (Rebirth) — Socket is the only tree with a direct player-stat effect:
  // +Max HP and +Max Endurance (both at the tier %). Fate is handled above
  // (Destiny scaling); Verdict (Judgement) and Data (Lore pets) are display-only.
  if (genesis?.tree === 'socket' && incarnates.genesis) {
    const powerName = incarnates.genesis.displayName;
    const value = genesis.tierPercent * 100;
    global.maxHP += value;
    addToBreakdown(breakdown, 'maxHP', { name: powerName, value, type: 'incarnate' });
    global.maxEndurance += value;
    addToBreakdown(breakdown, 'maxEndurance', { name: powerName, value, type: 'incarnate' });
  }

  // Interface - These are proc effects that debuff enemies, not player stats
  // We don't add them to global bonuses, but they could be displayed in tooltips
  //

  // Level Shift from incarnate slots (Alpha, Destiny, and Lore T3+)
  // Controlled by the independent levelShiftActive flag, NOT by per-slot stat toggles
  if (levelShiftActive) {
    if (incarnates.alpha) {
      const alphaEffects = getAlphaEffects(incarnates.alpha.powerId);
      if (alphaEffects?.levelShift) {
        global.levelShift += alphaEffects.levelShift;
        addToBreakdown(breakdown, 'levelShift', {
          name: incarnates.alpha.displayName,
          value: alphaEffects.levelShift,
          type: 'incarnate',
        });
      }
    }
    if (incarnates.destiny) {
      const destinyEffects = getDestinyEffects(incarnates.destiny.powerId);
      if (destinyEffects?.levelShift) {
        global.levelShift += destinyEffects.levelShift;
        addToBreakdown(breakdown, 'levelShift', {
          name: incarnates.destiny.displayName,
          value: destinyEffects.levelShift,
          type: 'incarnate',
        });
      }
    }
    if (incarnates.lore) {
      const loreEffects = getLoreEffects(incarnates.lore.powerId);
      if (loreEffects?.levelShift) {
        global.levelShift += loreEffects.levelShift;
        addToBreakdown(breakdown, 'levelShift', {
          name: incarnates.lore.displayName,
          value: loreEffects.levelShift,
          type: 'incarnate',
        });
      }
    }
  }

  // Debug: log incarnate diff
  if (_debugBefore && isCalcDebugEnabled()) {
    for (const key of Object.keys(global) as (keyof GlobalBonuses)[]) {
      const delta = global[key] - _debugBefore[key];
      if (Math.abs(delta) > 0.0001) {
        debugFormula(`${key}: ${delta > 0 ? '+' : ''}${formatDebugNum(delta)}`);
      }
    }
  }
}

// ============================================
// CONVERT TO CHARACTER STATS
// ============================================

/**
 * Convert global bonuses to character stats format for dashboard
 */
function convertToCharacterStats(global: GlobalBonuses): CharacterStats {
  const stats = createEmptyStats();

  // Offense
  stats.damage = global.damage;
  stats.accuracy = global.accuracy;
  stats.tohit = global.toHit;
  stats.recharge = global.recharge;
  stats.endrdx = global.endurance;

  // Defense positional
  stats.defMelee = global.defMelee;
  stats.defRanged = global.defRanged;
  stats.defAoE = global.defAoE;

  // Defense typed (combined S/L, F/C, E/N)
  stats.defSL = Math.max(global.defSmashing, global.defLethal);
  stats.defFC = Math.max(global.defFire, global.defCold);
  stats.defEN = Math.max(global.defEnergy, global.defNegative);
  stats.defPsionic = global.defPsionic;
  stats.defToxic = global.defToxic;

  // Resistance typed (combined)
  stats.resSL = Math.max(global.resSmashing, global.resLethal);
  stats.resFC = Math.max(global.resFire, global.resCold);
  stats.resEN = Math.max(global.resEnergy, global.resNegative);
  stats.resPsionic = global.resPsionic;
  stats.resToxic = global.resToxic;

  // Recovery & Health
  stats.regeneration = global.regeneration;
  stats.recovery = global.recovery;
  stats.maxhp = global.maxHP;
  stats.maxend = global.maxEndurance;

  // Movement
  stats.runspeed = global.runSpeed;
  stats.flyspeed = global.flySpeed;
  stats.jumpheight = global.jumpHeight;
  stats.jumpspeed = global.jumpSpeed;

  // Debuff Resistance
  stats.debuffResistSlow = global.debuffResistSlow;
  stats.debuffResistDefense = global.debuffResistDefense;
  stats.debuffResistRecharge = global.debuffResistRecharge;
  stats.debuffResistEndurance = global.debuffResistEndurance;
  stats.debuffResistRecovery = global.debuffResistRecovery;
  stats.debuffResistToHit = global.debuffResistToHit;
  stats.debuffResistRegeneration = global.debuffResistRegeneration;
  stats.debuffResistPerception = global.debuffResistPerception;

  return stats;
}

// ============================================
// MAIN CALCULATION
// ============================================

/**
 * Collect all powers from build for bonus calculation
 */
function collectAllPowers(build: Build): PowerWithToggle[] {
  const powers: PowerWithToggle[] = [];

  // Stored powers carry only the user's selections (slots, isActive, etc.).
  // Enrich with the current powerset definition so downstream filters can
  // see targetType/powerType/effectArea — without these the ally-only
  // filter in applyActivePowerBonuses sees undefined targetType and lets
  // ally buffs (Speed Boost, Fortitude, Grant Invisibility, etc.) bleed
  // into the caster's totals.
  const enrich = (
    power: { internalName?: string; isActive?: boolean; slots?: unknown },
    def?: { targetType?: string; powerType?: string; effectArea?: string; effects?: unknown; conditionalEffects?: unknown; setsModes?: string[]; modesSuspended?: string[]; atoms?: EncodedAtom[] },
  ): PowerWithToggle => {
    if (!def) return power as unknown as PowerWithToggle;
    return {
      ...power,
      // Definition wins for static metadata — stored copy may be missing or stale.
      targetType: def.targetType,
      powerType: def.powerType,
      effectArea: def.effectArea,
      effects: def.effects ?? (power as { effects?: unknown }).effects,
      // Atom list is generated static data — always take the definition's (the
      // stored build copy is trimmed and carries none). Atom-native appliers
      // (Plan B) read it; an atom-less power falls back to the bag.
      atoms: def.atoms ?? (power as { atoms?: EncodedAtom[] }).atoms,
      // Carry mode-/state-gated contributions so the calc can apply the
      // active ones (Bio Armor adaptation modes, …) — see expandActiveConditionals.
      conditionalEffects: def.conditionalEffects ?? (power as { conditionalEffects?: unknown }).conditionalEffects,
      // Carry the mode flags so mode-suppression (Granite → other Stone toggles)
      // can be resolved from the enriched active-power list.
      setsModes: def.setsModes,
      modesSuspended: def.modesSuspended,
    } as unknown as PowerWithToggle;
  };

  const primaryDef = build.primary.id ? getPowerset(build.primary.id) : undefined;
  for (const power of build.primary.powers) {
    powers.push(enrich(power, primaryDef?.powers.find((p) => p.internalName === power.internalName)));
  }

  const secondaryDef = build.secondary.id ? getPowerset(build.secondary.id) : undefined;
  for (const power of build.secondary.powers) {
    powers.push(enrich(power, secondaryDef?.powers.find((p) => p.internalName === power.internalName)));
  }

  for (const pool of build.pools) {
    const poolDef = getPowerPool(pool.id);
    for (const power of pool.powers) {
      powers.push(enrich(power, poolDef?.powers.find((p) => p.internalName === power.internalName)));
    }
  }

  if (build.epicPool) {
    const epicDef = getEpicPool(build.epicPool.id);
    for (const power of build.epicPool.powers) {
      powers.push(enrich(power, epicDef?.powers.find((p) => p.internalName === power.internalName)));
    }
  }

  // Inherent powers carry real self-buffs that must flow through the active-power
  // loop — most importantly the movement toggles (Sprint, Ninja Run, Beast Run,
  // prestige sprints), which otherwise contribute nothing to run/jump/fly speed.
  // Skip categories that already have dedicated handling so they aren't counted
  // twice: `fitness` (Swift/Hurdle/Health/Stamina → applyFitnessPowerBonuses) and
  // `archetype` (Supremacy, Vigilance, … → the AT-specific inherent calcs). The
  // stored inherent already carries effects/powerType from hydration, so it needs
  // no powerset-def enrichment.
  for (const power of build.inherents || []) {
    const cat = (power as { inherentCategory?: string }).inherentCategory;
    if (cat === 'fitness' || cat === 'archetype') continue;
    powers.push(power as unknown as PowerWithToggle);
  }

  // Active stance-toggle base effects. Bio Armor's stance is stored build-scoped
  // on the parent's `activeSubPower`, and the granted stance toggles
  // (Offensive/Defensive/Efficient Adaptation) are NOT persisted as their own
  // build powers — so their OWN base effects (Offensive's -7.5% Res(all) self
  // penalty, Defensive's -Damage) never reached the totals. Materialize the
  // active stance's sub-power as a synthetic ACTIVE power so its base effects
  // flow through applyActivePowerBonuses like any toggle. (The mode-gated
  // conditionals it enables on OTHER powers are handled separately by
  // expandActiveConditionals; those live on the base powers, not here.)
  const stanceDefs = [primaryDef, secondaryDef];
  for (const group of STANCE_GROUPS) {
    const activeId = activeStanceOptionId(powers as { internalName: string; activeSubPower?: string }[], group);
    const opt = group.options.find((o) => o.id === activeId);
    if (!opt?.subPower) continue;
    // Skip if a live copy of the toggle is already active in the list (avoids
    // double-counting an imported/persisted active sub-power).
    if (powers.some((p) => p.internalName === opt.subPower && p.isActive)) continue;
    let subDef: { effects?: unknown; powerType?: string; targetType?: string; effectArea?: string } | undefined;
    for (const def of stanceDefs) {
      const found = def?.powers.find((p) => p.internalName === opt.subPower);
      if (found) { subDef = found; break; }
    }
    if (!subDef?.effects || Object.keys(subDef.effects as object).length === 0) continue;
    powers.push({
      name: opt.label,
      internalName: opt.subPower,
      powerType: subDef.powerType,
      targetType: subDef.targetType,
      effectArea: subDef.effectArea,
      isActive: true,
      effects: subDef.effects,
    } as unknown as PowerWithToggle);
  }

  return powers;
}

/**
 * Expand the *active* mode-/state-gated conditionalEffects of the build's
 * active powers into synthetic active-power contributions.
 *
 * Bio Armor's adaptation modes (and similar mechanics) layer extra effects on
 * top of a power's always-on base — e.g. Environmental Modification's base
 * +Def(Fire) 1.5 plus an *additional* +Def(Fire) 0.45 while Defensive
 * Adaptation is active (confirmed against the raw `.powers` def: the base mod
 * has no `Requires`; the mode mod is `Requires kDefensiveAdaptation source.Mode?`).
 *
 * Rather than merge the conditional into the base power (which would force a
 * lossy replace-or-separate-row choice on colliding keys like `defenseBuff`),
 * each active conditional becomes its own synthetic active power. Feeding it
 * through the same `applyActivePowerBonuses` machinery makes collisions **sum**
 * naturally at the global-totals level (base 1.5 + mode 0.45 = 1.95).
 *
 * The synthetic carries **no slots** and is applied with **no Alpha / no
 * strength buffs** (see the call site) — these mode bonuses are flagged
 * IgnoreStrength in the binary ("these special bonuses are unenhanceable"), so
 * they must bypass enhancement and Power Boost.
 *
 * Default-safe: a build with no adjuster toggles selected (the common case)
 * produces zero synthetics, so the dashboard totals are byte-identical to
 * before this pass existed. Behavior only appears once the user picks a mode.
 *
 * AT-inherent gated conditionals (Domination, …) are skipped — they already
 * have dedicated total handling and would otherwise double-count.
 */
function expandActiveConditionals(
  powers: PowerWithToggle[],
  globalAdjusters: Record<string, boolean>,
  mechanicAdjusters: Record<string, boolean>,
): PowerWithToggle[] {
  const synthetics: PowerWithToggle[] = [];
  // Stance conditionals (Bio Armor adaptation, Staff Perfection) read their
  // on/off from `globalAdjusters` here, but the caller has already overlaid the
  // build's `activeSubPower`-derived state onto that map (see
  // `stanceAdjusterOverrides`), so the stance is build-scoped and the
  // enabler-taken gate is implicit (no parent → no activeSubPower → off).
  for (const power of powers) {
    // The parent must itself be active for its gated effects to apply.
    const isAuto = power.powerType?.toLowerCase() === 'auto';
    if (!(isAuto || power.isActive)) continue;

    const conds = power.conditionalEffects;
    if (!conds || conds.length === 0) continue;

    for (const c of conds) {
      // Driven elsewhere in the calc — skip to avoid double-counting.
      if (AT_INHERENT_CONDITIONAL_IDS.has(c.id)) continue;
      // Damage-only conditionals affect attack output, not dashboard totals.
      if (!c.effects || Object.keys(c.effects).length === 0) continue;

      const def = !!c.defaultActive;
      let on: boolean;
      if (c.scope === 'global') {
        const v = globalAdjusters[c.id];
        on = v === undefined ? def : v;
      } else {
        const v = mechanicAdjusters[`${power.internalName}:${c.id}`];
        on = v === undefined ? def : v;
      }
      if (!on) continue;

      synthetics.push({
        name: `${power.name} (${c.label})`,
        internalName: power.internalName,
        powerType: power.powerType,
        targetType: power.targetType,
        effectArea: power.effectArea,
        isActive: true,
        effects: c.effects as unknown as ActivePowerEffect,
        // This bag is THIS function's output, not converted data — the conditional carries
        // no atoms of its own, so the arms below reach it through `syntheticEffects` and
        // BPORT11's data-seam retirements leave it standing.
        syntheticContribution: true,
        // Intentionally no slots / allowedEnhancements → unenhanced.
      });
    }
  }
  return synthetics;
}

/**
 * Translate a buff-pet's aura PetEffects into the ActivePowerEffect shape the
 * totals loop consumes (defense/resistance/absorb/regen/…). Deduped so a pet
 * that lists the same aura on more than one ability contributes it once.
 *
 * Scalars use ScalarOrScaled {scale,table} so the loop level-resolves them just
 * like a player buff; ToHit uses the *Unenhanced* channel (pet ToHit auras don't
 * copy the player's slotted ToHit). The synthetic carries the summon power's
 * slots only when copyBoosts is set (see expandBuffPetAuras), so Defense IOs
 * slotted in the Force Field Generator power enhance its bubble — and nothing
 * else does, because it is applied with empty strength / no Alpha.
 */
function buffPetAuraEffects(sources: BuffPetSource[]): ActivePowerEffect {
  const effects: ActivePowerEffect = {};
  const seen = new Set<string>();
  for (const src of sources) {
    for (const a of src.auras) {
      const sub = (a.defenseTypes ?? a.resistanceTypes ?? []).join(',');
      const key = `${a.type}|${a.scale ?? ''}|${a.table ?? ''}|${sub}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const sc: ScalarOrScaled = { scale: a.scale ?? 0, table: a.table ?? '' };
      switch (a.type) {
        case 'DefenseBuff':
          // `defenseBuff`, not `defense`: BPORT11 retired the second spelling, whose only
          // supplier on any fork was this fold (0 converted powers carry `effects.defense`).
          // One key means a pet aura and a mode conditional reach the arm the same way, which
          // is what let the defence read drop its data branch without dropping the pet.
          effects.defenseBuff = effects.defenseBuff ?? {};
          for (const t of a.defenseTypes ?? []) effects.defenseBuff[t] = sc;
          break;
        case 'ResistanceBuff':
          effects.resistance = effects.resistance ?? {};
          for (const t of a.resistanceTypes ?? []) effects.resistance[t] = sc;
          break;
        case 'Absorb':
          // Flat-HP absorb off a Heal table (the game folds a pet's aspect=Maximum
          // Absorb to a flat amount; the MaxHP-fraction form is an Expression the
          // pet parser doesn't carry). Resolved to HP by the flat-absorb path.
          effects.absorb = { scale: a.scale, table: a.table };
          break;
        case 'RegenBuff':
          effects.regenBuff = sc;
          break;
        case 'RecoveryBuff':
          effects.recoveryBuff = sc;
          break;
        case 'ToHitBuff':
          effects.tohitBuffUnenhanced = sc;
          break;
        case 'RechargeBuff':
          effects.rechargeBuff = sc;
          break;
      }
    }
  }
  return effects;
}

/**
 * Expand the *toggled-on* buff-pet auras of the build's summon powers into
 * synthetic active-power contributions (Step 7.2), mirroring
 * `expandActiveConditionals`. Each buff-pet (Force Field Generator, Barrier Reef,
 * Triage Beacon, …) whose per-pet toggle is enabled becomes one synthetic Auto
 * power carrying the pet's aura effects, so they SUM into the dashboard totals
 * with a per-pet breakdown row.
 *
 * OPT-IN: the toggle is off by default (`?? false`), so a build that hasn't
 * enabled any buff-pet produces zero synthetics and totals are byte-identical to
 * before this pass existed. Taking the summon power is enough to see the toggle;
 * flipping it is what folds the aura in (a click summon has no persistent
 * isActive, so the toggle — not the parent's active state — is the gate).
 */
function expandBuffPetAuras(
  powers: PowerWithToggle[],
  mechanicAdjusters: Record<string, boolean>,
): PowerWithToggle[] {
  const synthetics: PowerWithToggle[] = [];
  for (const power of powers) {
    const summon = (power.effects as unknown as { summon?: SummonEffect } | undefined)?.summon;
    if (!summon) continue;
    const sources = getBuffPetSources(summon);
    if (sources.length === 0) continue;

    const on = mechanicAdjusters[`${power.internalName}:${BUFF_PET_TOGGLE_ID}`] ?? false;
    if (!on) continue;

    const effects = buffPetAuraEffects(sources);
    if (Object.keys(effects).length === 0) continue;

    synthetics.push({
      name: power.name,
      internalName: power.internalName,
      // Force active so applyActivePowerBonuses processes it regardless of the
      // click summon's (absent) isActive; Self target so it's never ally-skipped.
      powerType: 'Auto',
      targetType: 'Self',
      effectArea: power.effectArea,
      isActive: true,
      effects,
      // Minted by `buffPetAuraEffects` one line up, on a synthetic with no atoms — the same
      // named handoff the conditionals use, and the reason BPORT11 could retire the data
      // seams underneath it without zeroing every buff-pet aura.
      syntheticContribution: true,
      // copyBoosts → the pet inherits the summon power's slotted enhancements, so
      // carry them for the enhancement calc; otherwise the aura is unenhanced.
      slots: summon.copyBoosts ? power.slots : undefined,
      allowedEnhancements: summon.copyBoosts ? power.allowedEnhancements : undefined,
    });
  }
  return synthetics;
}

/**
 * Convert Build to BuildPowers format for set bonus calculation
 */
function buildToBuildPowers(build: Build): BuildPowers {
  return {
    primary: { powers: build.primary.powers },
    secondary: { powers: build.secondary.powers },
    pools: build.pools.map((pool) => ({ powers: pool.powers })),
    epicPool: build.epicPool ? { powers: build.epicPool.powers } : undefined,
    inherents: build.inherents,
  };
}

/**
 * Legacy TS totals calc — retained ONLY for the SPIKE6 parity diff (engine vs old numbers);
 * deleted after. No live caller.
 * @param build - The current build state
 * @param exemplarMode - When true, respects build level for set bonus suppression.
 * @param incarnateActive - Which incarnate slots are active for stat calculations
 * @param options - Additional calculation options
 */
export function legacyCalculateCharacterTotals(
  build: Build,
  exemplarMode = false,
  incarnateActive?: IncarnateActiveState,
  options?: CalculationOptions
): CharacterCalculationResult {
  // Enhancements the game wouldn't allow in their host power (mis-routed by an
  // import, a share-link, or a permuted-internal-name powerset) must not
  // contribute enhancement values or set bonuses. Compute against a copy with
  // those slots nulled; the store's build is left untouched (the UI flags them
  // separately). No-op — same reference — when the build is clean.
  build = withoutIllegalSlots(build);

  const breakdown = new Map<string, DashboardStatBreakdown>();
  const globalBonuses = createEmptyGlobalBonuses();
  const _debug = isCalcDebugEnabled();

  // Step 1: Calculate set bonuses with Rule of 5
  // effectiveLevel drives HP, fitness, and toggle scaling — always use build.level
  // Set bonus suppression only applies in exemplar mode (don't suppress at low build levels)
  const exemplarLevel = options?.exemplarLevel;
  const effectiveLevel = getEffectiveLevel(build.level, exemplarMode, exemplarLevel);
  // Incarnate abilities turn off entirely below level 45 (Genesis swaps to its
  // exemplar power). Gates the two incarnate calc entry points below.
  const incarnatesSuppressed = areIncarnatesSuppressed(effectiveLevel);

  // Debug: build context
  if (_debug) {
    debugBuildContext(
      build.archetype?.id || 'unknown',
      build.level,
      effectiveLevel,
      exemplarLevel,
      {
        exemplarMode,
        combatMode: options?.combatMode,
        targetLevelOffset: options?.targetLevelOffset,
        vigilanceTeamSize: options?.vigilanceTeamSize,
        furyLevel: options?.furyLevel,
        incarnateLevelShift: options?.incarnateLevelShift,
      }
    );
  }

  const buildPowers = buildToBuildPowers(build);
  const { bonuses: setBonusAggregated, tracking } = calculateSetBonuses(
    buildPowers,
    getIOSet,
    exemplarMode ? effectiveLevel : undefined,
    exemplarMode ? effectiveLevel : 50
  );


  // Step 2: Apply set bonuses to global bonuses
  applySetBonusesToGlobal(setBonusAggregated, globalBonuses);

  // Debug: set bonus results
  if (_debug) {
    const trackingStats: { stat: string; count: number; capped: number }[] = [];
    for (const stat of Object.keys(setBonusAggregated)) {
      const items = getStatBreakdown(tracking, stat);
      let count = 0;
      let capped = 0;
      for (const item of items) {
        count += item.sources.length;
        capped += item.rejectedSources.length;
      }
      if (count > 0 || capped > 0) trackingStats.push({ stat, count, capped });
    }
    debugSetBonuses(setBonusAggregated, trackingStats);
  }

  // Step 3: Build detailed breakdown from set bonus tracking
  for (const stat of Object.keys(setBonusAggregated)) {
    const statBreakdownItems = getStatBreakdown(tracking, stat);
    if (statBreakdownItems.length > 0) {
      const built = buildStatBreakdown(statBreakdownItems);
      // Normalize stat key to match breakdownKey casing (e.g. 'maxhp' → 'maxHP')
      const normalizedStat = stat.toLowerCase().replace(/[^a-z]/g, '');
      // Mirror PAIRED_STATS expansion from applySetBonusesToGlobal so the
      // breakdown shows up under both halves of a paired bonus (e.g.
      // Ice Mistral's Torment's +Res(Recharge Debuff) also grants Slow Res
      // — the source must surface under both stats, not just Recharge).
      const targetStats = PAIRED_STATS[normalizedStat] ?? [STAT_TO_GLOBAL[normalizedStat] || stat];
      for (const breakdownStat of targetStats) {
        const existing = breakdown.get(breakdownStat);
        if (existing) {
          existing.sources.push(...built.sources);
          existing.total += built.total;
          existing.cappedSources += built.cappedSources;
        } else {
          // Clone so paired stats don't share mutable source arrays.
          breakdown.set(breakdownStat, {
            ...built,
            sources: [...built.sources],
          });
        }
      }
    }
  }

  // Step 4: Collect all powers
  const collectedPowers = collectAllPowers(build);

  // Step 4.1: Mode suppression. When an active power sets a mode that suspends
  // other active powers (Granite Armor suspends the other Stone Armor toggles;
  // Kheldian forms suspend human toggles; Granite suspends travel toggles), the
  // suppressed powers' own effects must NOT be summed — the game runs them but
  // their armor/buff is off. Their slotted set bonuses still apply (that's a
  // separate path via buildToBuildPowers), matching the game. Default-safe: no
  // active mode-setter → empty map → identical totals.
  const suppressedPowers = computeModeSuppression(
    collectedPowers as unknown as ModeCarrier[],
  );
  const allPowers = suppressedPowers.size === 0
    ? collectedPowers
    : collectedPowers.filter((p) => !suppressedPowers.has(p.internalName));

  // Step 5: Get Alpha incarnate enhancement bonuses (apply to all powers including fitness)
  const alphaBonuses = getAlphaEnhancementBonuses(build.incarnates, incarnateActive, incarnatesSuppressed);
  // Per-aspect slice of the alpha bonus that bypasses ED, read from the same
  // silent grant data (BoostIgnoreDiminishing / `Ones` templates). Passed
  // alongside alphaBonuses so combineWithAlphaED splits the buff correctly
  // against the per-power IO ED total.
  const alphaEdBypass = getAlphaEdBypassBonuses(build.incarnates, incarnateActive, incarnatesSuppressed);
  if (_debug) debugAlphaBonuses(alphaBonuses);

  // Step 6: Apply inherent power bonuses (Fitness powers, with Alpha bonuses)
  if (_debug) debugGroup('Step 6: Fitness Powers');
  applyFitnessPowerBonuses(build, globalBonuses, breakdown, effectiveLevel, alphaBonuses, alphaEdBypass, exemplarLevel);
  if (_debug) debugGroupEnd();

  // Step 7: Apply active toggle power bonuses (with enhancement multipliers + Alpha bonuses)
  if (_debug) debugGroup('Step 7: Active Power Bonuses');
  // Collect active +Strength self-buffs (Power Boost family) FIRST, so each
  // boosted power's defense/tohit/etc. output is multiplied by the strength
  // when applyActivePowerBonuses runs. Stored on globalBonuses (as fractions)
  // for the Power Info per-power display to read.
  const strengthBuffs = collectStrengthBuffs(allPowers, build.archetype.id || '', effectiveLevel, options?.targetsHitValues ?? {});
  globalBonuses.strengthDefense = strengthBuffs.defense;
  globalBonuses.strengthToHit = strengthBuffs.toHit;
  globalBonuses.strengthHeal = strengthBuffs.heal;
  globalBonuses.strengthAbsorb = strengthBuffs.absorb;
  globalBonuses.strengthEndMod = strengthBuffs.endMod;
  globalBonuses.strengthMovement = strengthBuffs.movement;
  globalBonuses.strengthMez = strengthBuffs.mez;
  // Stealth radius is gathered from active powers AND procs, then committed
  // together by resolveStealthRadius (suppress-group max + additive sum).
  const stealthContribs: StealthContribution[] = [];
  // MaxHP-fraction absorb (Wild Bastion etc.) — resolved to HP after accolades
  // and incarnates land on global.maxHP (see the absorb resolution below).
  const absorbFractionContribs: { name: string; fraction: number }[] = [];
  // Movement percents are gathered across all three active-power passes and
  // committed together by resolveMovementTotals (travel suppress-group max +
  // additive sum + combat-mode suppression).
  const movementContribs: MovementContribution[] = [];
  // Resistible self -Res debuffs (Offensive Adaptation) — mitigated by same-type
  // resistance after every pass has summed the totals (see resolution below).
  const resSelfDebuffContribs: { name: string; type: string; nominal: number; resistible: boolean }[] = [];
  applyActivePowerBonuses(allPowers, globalBonuses, breakdown, effectiveLevel, build.archetype.id || '', alphaBonuses, alphaEdBypass, options?.targetsHitValues ?? {}, exemplarLevel, options?.combatMode, strengthBuffs, stealthContribs, absorbFractionContribs, movementContribs, resSelfDebuffContribs);

  // Step 7.1: Apply active mode-/state-gated conditional contributions (Bio
  // Armor adaptation modes, …). Each active conditional is a synthetic active
  // power so its effects SUM onto the base power's at the totals level (e.g.
  // Defensive Adaptation's +Def adds to Environmental Modification's base +Def).
  // Applied with NO Alpha / NO strength buffs and no slots → unenhanced, because
  // these mode bonuses are IgnoreStrength ("special bonuses are unenhanceable").
  // Default-safe: no toggles selected → no synthetics → totals unchanged.
  // Overlay the build's `activeSubPower`-derived stance state onto the UI
  // global adjusters so Bio Armor adaptation / Staff Perfection are driven by
  // the build-scoped stance (single source of truth), with `activeSubPower`
  // winning over any stale UI toggle.
  const effectiveGlobalAdjusters = {
    ...(options?.globalAdjusters ?? {}),
    ...stanceAdjusterOverrides(allPowers),
  };
  const conditionalPowers = expandActiveConditionals(
    allPowers,
    effectiveGlobalAdjusters,
    options?.mechanicAdjusters ?? {},
  );
  if (conditionalPowers.length > 0) {
    applyActivePowerBonuses(conditionalPowers, globalBonuses, breakdown, effectiveLevel, build.archetype.id || '', {}, {}, options?.targetsHitValues ?? {}, exemplarLevel, options?.combatMode, emptyStrengthBuffs(), stealthContribs, absorbFractionContribs, movementContribs, resSelfDebuffContribs);
  }

  // Step 7.2: Fold toggled-on buff-pet auras (Force Field Generator, Barrier
  // Reef, Triage Beacon, …) into the totals. Each is a synthetic Auto power whose
  // aura effects SUM onto the caster's totals with a per-pet breakdown row.
  // Applied with NO Alpha / NO strength buffs (pets don't inherit those); the
  // aura is enhanced only by the summon power's own slots when copyBoosts is set.
  // Default-safe: no buff-pet toggled → no synthetics → totals unchanged.
  const buffPetPowers = expandBuffPetAuras(allPowers, options?.mechanicAdjusters ?? {});
  if (buffPetPowers.length > 0) {
    applyActivePowerBonuses(buffPetPowers, globalBonuses, breakdown, effectiveLevel, build.archetype.id || '', {}, {}, options?.targetsHitValues ?? {}, exemplarLevel, options?.combatMode, emptyStrengthBuffs(), stealthContribs, absorbFractionContribs, movementContribs, resSelfDebuffContribs);
  }
  if (_debug) debugGroupEnd();

  // Step 7.3: Commit movement totals now that every active-power source is
  // gathered — travel suppress groups (kTravelBuff: CJ / SJ / SS momentum /
  // Fly / Ninja Run …) contribute only their strongest member, the rest sum,
  // and combat mode drops in-combat-suppressible buffs (SS run, SJ jump, Fly
  // speed — but not Combat Jumping / Hover, which have no suppress events).
  resolveMovementTotals(movementContribs, globalBonuses, breakdown, options?.combatMode);

  // Step 7.5: Apply always-on proc bonuses (Global and Proc120s in Auto/Toggle powers)
  // Procs have their own Rule of 5 tracking, separate from set bonuses
  const procSettings = options?.procSettings;
  const anyProcEnabled = !procSettings || Object.values(procSettings).some(v => v);
  if (_debug) debugGroup('Step 7.5-7.6: Proc Bonuses');
  if (anyProcEnabled) {
    applyProcBonuses(build, globalBonuses, breakdown, procSettings, stealthContribs);
  }

  // Step 7.5c: Variable procs (stacking buffs / HP-scaling globals). Runs
  // regardless of anyProcEnabled so an explicit per-proc enable still applies
  // when the global category toggles are all off — the pass gates per-proc.
  applyVariableProcBonuses(build, globalBonuses, breakdown, procSettings, stealthContribs);

  // Step 7.5b: Commit stealth radius now that every source (active powers +
  // procs) is gathered — suppress groups contribute their max, the rest add.
  resolveStealthRadius(stealthContribs, globalBonuses, breakdown);

  // Step 7.6: Apply Build Up proc average contributions (PPM click procs)
  if (!procSettings || procSettings.buildUp) {
    applyBuildUpProcBonuses(build, globalBonuses, breakdown);
  }
  if (_debug) debugGroupEnd();

  // Step 8: Apply accolade bonuses. Selected ids resolve to their powers in the active
  // dataset's Accolades powerset; stats are read off the power, not a stored bonus list.
  const selectedAccolades = build.accolades?.length
    ? getAccolades().filter((power) => build.accolades.includes(accoladeId(power)))
    : [];
  if (_debug && selectedAccolades.length > 0) debugGroup('Step 8: Accolades');
  if (selectedAccolades.length > 0) {
    applyAccoladeStats(selectedAccolades, build.archetype.id || '', effectiveLevel, globalBonuses, breakdown);
  }
  if (_debug && selectedAccolades.length > 0) debugGroupEnd();

  // Step 9: Apply incarnate bonuses (Destiny, Hybrid - direct stats)
  // Note: Alpha bonuses were already applied in Step 7 as enhancement bonuses
  if (_debug) debugGroup('Step 9: Incarnate Bonuses');
  // This file is frozen, so it never learned LSHIFT-1's ceiling: it applies every earned shift
  // or none. The two settings it CAN express map onto its boolean; anything between them throws
  // rather than quietly answering the nearest one it knows — a lossy map here would make the
  // parity gate green while the two sides read different level shifts, which is the exact class
  // of silent divergence this oracle exists to catch.
  const levelShiftCeiling = options?.incarnateLevelShift ?? null;
  if (levelShiftCeiling !== null && levelShiftCeiling !== 0) {
    throw new Error(
      `legacy-totals.oracle: incarnateLevelShift=${levelShiftCeiling} is a partial ceiling this frozen oracle cannot express (it knows only all-or-nothing). Compare against the engine with null or 0.`,
    );
  }
  applyIncarnateBonuses(build.incarnates, incarnateActive, globalBonuses, breakdown, levelShiftCeiling === null, incarnatesSuppressed, options?.destinyTime);
  if (_debug) debugGroupEnd();

  // Step 9.1: Apply archetype inherent damage bonuses (Vigilance, Fury)
  const archetypeId = build.archetype?.id;
  if (archetypeId === 'defender' && options?.vigilanceTeamSize !== undefined) {
    const vigBonus = calculateVigilanceDamageBonus(effectiveLevel, options.vigilanceTeamSize);
    if (vigBonus > 0) {
      const vigValue = vigBonus * 100;
      globalBonuses.damage += vigValue;
      addToBreakdown(breakdown, 'damage', {
        name: 'Vigilance',
        value: vigValue,
        type: 'inherent',
      });
    }
  }
  if (archetypeId === 'brute' && options?.furyLevel !== undefined && options.furyLevel > 0) {
    const furyBonus = calculateFuryDamageBonus(options.furyLevel);
    if (furyBonus > 0) {
      const furyValue = furyBonus * 100;
      globalBonuses.damage += furyValue;
      addToBreakdown(breakdown, 'damage', {
        name: 'Fury',
        value: furyValue,
        type: 'inherent',
      });
    }
  }

  // Step 9.2: Resolve MaxHP-fraction absorb (Wild Bastion etc.) to absolute HP.
  // Runs after accolades (Step 8) and incarnates (Step 9) have landed on
  // global.maxHP, so the fraction is taken against the build's final, capped
  // Max HP — matching how the game scales these shields off current Max HP
  // (which is why +HP accolades increase them).
  if (absorbFractionContribs.length > 0) {
    const { baseHealth, maxHealth } = getBaselineHealth(build.archetype?.id ?? undefined, build.level);
    const buffedHP = baseHealth * (1 + globalBonuses.maxHP / 100);
    const actualHP = maxHealth > 0 ? Math.min(buffedHP, maxHealth) : buffedHP;
    for (const { name, fraction } of absorbFractionContribs) {
      const hp = fraction * actualHP;
      if (hp <= 0) continue;
      globalBonuses.absorb += hp;
      addToBreakdown(breakdown, 'absorb', { name, value: hp, type: 'active-power' });
    }
  }

  // Step 9.3: Apply resistible self -Res debuffs (Bio Offensive Adaptation)
  // now that every resistance source — powers, IO set bonuses, procs,
  // accolades, incarnates — has summed. CoH reduces such a debuff by the
  // caster's own resistance to that type: effective = nominal × (1 − R), while
  // the breakdown still shows the nominal magnitude. Snapshot R per type BEFORE
  // applying any deferred debuff so multiple debuffs of one type all resist
  // against the same pre-debuff total (no cascade order-dependence). An
  // IgnoreResistance (`resistible:false`) debuff applies flat.
  if (resSelfDebuffContribs.length > 0) {
    const resSnapshot: Partial<Record<keyof GlobalBonuses, number>> = {};
    for (const { type } of resSelfDebuffContribs) {
      const key = `res${capitalizeFirst(type)}` as keyof GlobalBonuses;
      if (key in globalBonuses && resSnapshot[key] === undefined) {
        resSnapshot[key] = (globalBonuses[key] as number) || 0;
      }
    }
    for (const { name, type, nominal, resistible } of resSelfDebuffContribs) {
      const key = `res${capitalizeFirst(type)}` as keyof GlobalBonuses;
      if (!(key in globalBonuses)) continue;
      const R = resSnapshot[key] ?? 0;
      // nominal is already negative (e.g. -7.5). Mitigation clamps the factor to
      // ≥0 so an over-100% raw resistance can't flip the debuff into a buff.
      const factor = resistible ? Math.max(0, 1 - R / 100) : 1;
      (globalBonuses[key] as number) = ((globalBonuses[key] as number) || 0) + nominal * factor;
      addToBreakdown(breakdown, key, { name, value: nominal, type: 'active-power' });
    }
  }

  // Step 9.5: Compute hit chance against target level (purple patch)
  const targetOffset = options?.targetLevelOffset ?? 0;
  const effectiveLevelDiff = targetOffset - globalBonuses.levelShift;
  const ppBaseToHit = getBaseToHit(effectiveLevelDiff);
  const finalToHit = Math.min(0.95, Math.max(0.05, ppBaseToHit + globalBonuses.toHit / 100));
  const accuracyMult = 1 + globalBonuses.accuracy / 100;
  globalBonuses.baseToHit = ppBaseToHit;
  globalBonuses.hitChance = Math.min(0.95, Math.max(0.05, finalToHit * accuracyMult));
  globalBonuses.combatModifier = getCombatModifier(effectiveLevelDiff);

  if (_debug) {
    debugHitChance(targetOffset, globalBonuses.levelShift, effectiveLevelDiff, ppBaseToHit, globalBonuses.toHit, globalBonuses.accuracy, globalBonuses.hitChance);
  }

  // Step 9.7: Compute toggle endurance costs now that every global EndDisc
  // source (set bonuses, active-power discounts, Hybrid Support T4) has been
  // aggregated into global.endurance. Uses the divisor formula
  // `cost = base / (1 + slotEndRdx + global.endurance/100)` — the same math
  // the game and the per-power Power Info panel use. Replaces the earlier
  // pattern that summed per-toggle costs in Step 7 and then post-hoc scaled
  // by `(1 - global/100)` (wrong formula, and missed any discount source
  // that hadn't been aggregated by the time Step 7 ran, e.g. Hybrid T4 in
  // Step 9).
  if (_debug) debugGroup('Step 9.7: Toggle End Costs');
  applyToggleEndCosts(allPowers, globalBonuses, breakdown, effectiveLevel, alphaBonuses, alphaEdBypass, exemplarLevel);
  if (_debug) debugGroupEnd();

  // Step 10: Convert to character stats format
  const stats = convertToCharacterStats(globalBonuses);

  // Compute net endurance per second (recovery minus toggle costs)
  // MaxEndurance bonuses are flat values (accolades +5, power effects in absolute points)
  const totalMaxEnd = 100 + globalBonuses.maxEndurance;
  const recoveryEndPerSec = (totalMaxEnd / 60) * (1 + globalBonuses.recovery / 100);
  globalBonuses.netEndPerSec = recoveryEndPerSec - globalBonuses.toggleEndCost;

  if (_debug) {
    debugNetEndurance(totalMaxEnd, globalBonuses.maxEndurance, globalBonuses.recovery, recoveryEndPerSec, globalBonuses.toggleEndCost, globalBonuses.endurance, globalBonuses.netEndPerSec);
    debugFinalStats(globalBonuses as unknown as Record<string, number>);
    debugEnd();
  }

  // Update breakdown totals from final values (exclude capped/Rule-of-5 sources
  // AND suppress-group losers — both are present in the source list for display
  // but neither counts toward the total).
  for (const [, bd] of breakdown) {
    bd.total = bd.sources.reduce((sum, s) => (s.capped || s.suppressed) ? sum : sum + s.value, 0);
  }

  return {
    stats,
    globalBonuses,
    breakdown,
    setBonuses: setBonusAggregated,
    bonusTracking: tracking,
    // The legacy calc has no engine behind it, so it projects nothing. Its only caller is the
    // parity diff, which reads the dashboard totals.
    powerProjection: new Map(),
    engineStateJson: null,
    // The quarantined legacy calculator has no what-if layer: it is an ORACLE, graded against
    // the engine over builds with nothing simulated (PROD7). An entry here would be a second
    // implementation of the injection, which is exactly what the quarantine exists to prevent.
    whatIfMoved: {},
  };
}
