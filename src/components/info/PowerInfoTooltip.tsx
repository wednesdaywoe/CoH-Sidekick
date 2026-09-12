/**
 * PowerInfoTooltip - Floating tooltip that displays power or enhancement info on hover
 * Shows all power stats with Base/Enhanced/Final values
 * Remains responsive to hover changes even when info panel is locked
 */

import { useEffect, useLayoutEffect, useRef, useState, useCallback, useMemo } from 'react';
import { createPortal } from 'react-dom';
import { useUIStore, useBuildStore, useDominationActive, useScourgeActive, useFuryLevel, useSupremacyActive, useVigilanceTeamSize, useCriticalHitsActive, useStalkerHidden, useStalkerTeamSize, useStalkerCritActive, useContainmentActive, useSentinelCritActive, useGlobalAdjuster } from '@/stores';
import { getBaseToHit } from '@/data/purple-patch';
import { useCharacterCalculation, useGlobalBonuses, usePowerDamageVsRank, usePowerProjection } from '@/hooks/useCalculatedStats';
import { magnitudesFromProjection } from './magnitudesFromProjection';
import { critBranchSummary, critComponents, VS_HIGHER_RANK_SEGMENT, VS_MINION_RANK_SEGMENT } from '@/utils/calculations/power-at-mechanics';
import { lookupPower, getIOSet, getPowerset } from '@/data';
import type { Power } from '@/types';
import {
  calculatePowerEnhancementBonuses,
  combineWithAlphaED,
  calculatePowerDamage,
  calculateDefiance,
  getDominationInfo,
  getPowerDominationSummary,
  getScourgeInfo,
  calculateScourgeDamage,
  isCorruptorAttackPower,
  getFuryInfo,
  calculateFuryDamageBonus,
  isBruteAttackPower,
  getSupremacyInfo,
  getBodyguardInfo,
  isMastermindPower,
  getVigilanceInfo,
  calculateVigilanceDamageBonus,
  isDefenderPower,
  isScrapperAttackPower,
  getAssassinationInfo,
  calculateAssassinationDamageBonus,
  calculateAssassinationDamage,
  isStalkerAttackPower,
  getGauntletInfo,
  isTankerPower,
  getContainmentInfo,
  calculateContainmentDamage,
  isControllerPower,
  getOpportunityCritBonus,
  calculateOpportunityCritDamage,
  isSentinelAttackPower,
  getAlphaEnhancementBonuses,
  getAlphaEdBypassBonuses,
  type EnhancementBonuses,
} from '@/utils/calculations';
import { calculatePetDamage, calculateResolvedPseudoPetDamage, shouldApplyEnhancements, type PetDamageResult } from '@/utils/calculations/pet-damage';
import { buildDisplayEffects, withPseudoPetEffects } from './buildDisplayEffects';
import { formatPetEffectValue, petEffectLabel, formatSummonDuration } from './petEffectDisplay';
import { getPetEntity } from '@/data/pet-entities';
import { resolveActiveUpgradeTiers, takenPowerNames } from '@/utils/calculations/pet-upgrades';
import { resolveEffectivePower, effectiveGlobalAdjusters, isCasterHidden, currentToHitFraction } from './resolveEffectivePower';
import type { ArchetypeId } from '@/types';
import {
  convertGlobalBonusesToAspects,
  withStrengthBonuses,
  findSelectedPowerInBuild,
  getDamageCap,
} from './powerDisplayUtils';
import {
  RegistryEffectsDisplay,
} from './SharedPowerComponents';

interface PowerInfoContentProps {
  powerName: string;
  powerSet: string;
}

function PowerInfoContent({ powerName, powerSet }: PowerInfoContentProps) {
  const build = useBuildStore((s) => s.build);
  const archetypeId = build.archetype.id;
  const stormCellActive = useGlobalAdjuster('stormblast_instormcell', false);
  const mechanicAdjusters = useUIStore((s) => s.mechanicAdjusters);
  const globalAdjusters = useUIStore((s) => s.globalAdjusters);
  const globalBonuses = useGlobalBonuses();
  // Same scale the panel reads — see `InfoPanel`. Shared so the tooltip and the panel can never
  // draw the same power at two different lengths.
  const maxBuildDamage = useCharacterCalculation().damageCeiling ?? 0;
  const targetLevelOffset = useUIStore((s) => s.targetLevelOffset);
  const incarnateActive = useUIStore((s) => s.incarnateActive);
  const dominationActive = useDominationActive();
  const scourgeActive = useScourgeActive();
  const furyLevel = useFuryLevel();
  const supremacyActive = useSupremacyActive();
  const vigilanceTeamSize = useVigilanceTeamSize();
  const criticalHitsActive = useCriticalHitsActive();
  const stalkerHidden = useStalkerHidden();
  const combatMode = useUIStore((s) => s.combatMode);
  const stalkerTeamSize = useStalkerTeamSize();
  const stalkerCritActive = useStalkerCritActive();
  const effectiveHidden = isCasterHidden(build, stalkerHidden);
  const containmentActive = useContainmentActive();
  const sentinelCritActive = useSentinelCritActive();

  // The Scrapper crit read from the power's OWN rows, engine-resolved against the rank
  // each surface names (see InfoPanel — same source, same gating: the projection is a
  // full engine pass, so it only runs for the one case that reads it).
  const wantEngineCrit = archetypeId === 'scrapper' && isScrapperAttackPower(powerSet) && criticalHitsActive;
  const damageVsHigher = usePowerDamageVsRank(wantEngineCrit ? powerSet : undefined, powerName, VS_HIGHER_RANK_SEGMENT);
  const damageVsMinion = usePowerDamageVsRank(wantEngineCrit ? powerSet : undefined, powerName, VS_MINION_RANK_SEGMENT);

  // The effect rows, from the engine rather than resolved off the display bag (ENGLAG-1) — the
  // same swap the InfoPanel takes, and for the same reason: STRIP-1 left the bag holding only
  // the keys `buildDisplayEffects` mints, so the local path renders no authored effect at all.
  const projection = usePowerProjection(powerSet, powerName);
  const effectRows = useMemo(
    () => (projection ? magnitudesFromProjection(projection.grantedMagnitudes) : undefined),
    [projection],
  );

  // Check if Fiery Embrace is active in the build
  const isFieryEmbraceActive = useMemo(() => {
    // Check secondary powers for Fiery Embrace
    const fieryEmbrace = build.secondary.powers.find(p => p.internalName === 'Fiery_Embrace');
    if (fieryEmbrace && fieryEmbrace.isActive) return true;
    // Also check primary (some ATs might have it there)
    const primaryFE = build.primary.powers.find(p => p.internalName === 'Fiery_Embrace');
    if (primaryFE && primaryFE.isActive) return true;
    return false;
  }, [build.secondary.powers, build.primary.powers]);

  // Unified power lookup across all categories
  const lookupResult = lookupPower(powerSet, powerName);
  let basePower: Power | undefined = lookupResult?.power;

  // Archetype inherent fallback (dynamically created, not in static data)
  if (!basePower && powerSet === 'Inherent') {
    const selectedInherent = build.inherents.find((p) => p.internalName === powerName);
    if (selectedInherent) {
      basePower = { ...selectedInherent };
    }
  }

  // Find the selected power from build to get its slots
  const selectedPower = findSelectedPowerInBuild(powerName, powerSet, build);

  // Get Alpha incarnate enhancement bonuses (apply to all powers)
  const alphaBonuses = useMemo<EnhancementBonuses>(() => {
    return getAlphaEnhancementBonuses(build.incarnates, incarnateActive);
  }, [build.incarnates, incarnateActive]);

  const exemplarMode = useUIStore((s) => s.exemplarMode);
  const exemplarLevel = useUIStore((s) => s.exemplarLevel);

  // Calculate enhancement bonuses with proper Alpha ED bypass handling
  const enhancementBonuses = useMemo<EnhancementBonuses>(() => {
    const hasAlpha = Object.values(alphaBonuses).some((v) => v !== undefined && v !== 0);
    const alphaEdBypass = getAlphaEdBypassBonuses(build.incarnates, incarnateActive);

    if (hasAlpha && selectedPower?.slots) {
      return combineWithAlphaED(
        // Alpha applies only to the aspects the power accepts; without this list
        // `combineWithAlphaED` applies it to all of them (PROD6C-3f).
        { name: selectedPower.name, slots: selectedPower.slots, allowedEnhancements: selectedPower.allowedEnhancements },
        build.level,
        getIOSet,
        alphaBonuses,
        alphaEdBypass,
        exemplarMode ? exemplarLevel : undefined
      );
    }

    if (selectedPower?.slots) {
      return calculatePowerEnhancementBonuses(
        { name: selectedPower.name, slots: selectedPower.slots },
        build.level,
        getIOSet,
        exemplarMode ? exemplarLevel : undefined
      );
    }

    return { ...alphaBonuses };
  }, [selectedPower, build.level, alphaBonuses, exemplarMode, exemplarLevel, build.incarnates, incarnateActive]);

  // Convert global bonuses to enhancement-aspect-keyed decimals for three-tier display
  const globalBonusesForCalc = useMemo(
    () => convertGlobalBonusesToAspects(globalBonuses),
    [globalBonuses]
  );

  // The magnitude rows read the +Strength-augmented globals, same as InfoPanel's — the two
  // surfaces render the same rows from the same resolver and must not disagree on them.
  // The damage half below deliberately keeps the un-augmented `globalBonusesForCalc`:
  // strength carries no damage aspect.
  const globalBonusesWithStrength = useMemo(
    () => withStrengthBonuses(globalBonusesForCalc, globalBonuses),
    [globalBonusesForCalc, globalBonuses]
  );

  // Get powerset for determining damage type
  const powerset = getPowerset(powerSet);

  // The power this hover actually shows — the snipe's fast form, the mid-combat cast, and the
  // active mode-gated contributions merged onto `effects` and `damage`. Before PROD6C-3k this
  // surface hand-rolled two thirds of that (conditional damage for the heal row, a castTime patch
  // on the built bag) and applied neither the conditional EFFECTS merge nor the snipe form, so it
  // disagreed with the InfoPanel on 110 / 116 / 174 powers per fork. Both now call the same
  // resolver, which the engine's `effective.rs` mirrors.
  const effectivePower = useMemo(
    () =>
      basePower
        ? resolveEffectivePower(basePower, {
            combatMode,
            hidden: effectiveHidden,
            globalAdjusters: effectiveGlobalAdjusters(build, globalAdjusters),
            mechanicAdjusters,
            atInherentState: { dominationActive },
            activeModes: build.activeModes,
            build,
            currentToHit: currentToHitFraction(archetypeId, globalBonuses),
          }).power
        : undefined,
    [basePower, combatMode, effectiveHidden, mechanicAdjusters, globalAdjusters, dominationActive, build, archetypeId, globalBonuses],
  );

  // Calculate actual damage using archetype modifiers and level
  const calculatedDamage = useMemo(() => {
    if (!effectivePower?.damage && !effectivePower?.effects?.damage) return null;

    // Determine if this is a primary or secondary powerset
    const isPrimary = powerSet === build.primary.id;
    const isSecondary = powerSet === build.secondary.id;

    // Get the powerset category to help determine melee vs ranged
    const powersetCategory = isPrimary
      ? powerset?.category?.toUpperCase()
      : isSecondary
        ? powerset?.category?.toUpperCase()
        : undefined;

    // Get powerset display name
    const powersetName = powerset?.name || '';

    return calculatePowerDamage(
      effectivePower,
      {
        level: build.level,
        archetypeId: archetypeId as ArchetypeId | undefined,
        primaryName: powersetName,
        primaryCategory: powersetCategory,
      },
      { damage: enhancementBonuses.damage || 0 },
      globalBonusesForCalc.damage,
      0 // active buffs
    );
  }, [effectivePower, build.level, archetypeId, enhancementBonuses.damage, globalBonusesForCalc.damage, powerSet, build.primary.id, build.secondary.id, powerset]);

  // Calculate aggregate pet damage (supports multi-entity summons)
  const petDamageAggregate = useMemo<{ results: PetDamageResult[]; base: number; enhanced: number; final: number } | null>(() => {
    const summon = effectivePower?.summon;
    if (!summon) return null;

    // Build entity list
    const entityList: { entityName: string; count: number }[] = [];
    if (summon.entities) {
      for (const e of summon.entities) entityList.push({ entityName: e.entity, count: e.count });
    } else if (summon.entity) {
      entityList.push({ entityName: summon.entity, count: summon.entityCount || 1 });
    }
    const resolvedList = summon.resolvedEntities ?? [];
    // Conditional ("ignited") entities fold in only when their per-power toggle is on.
    for (const ce of summon.conditionalEntities ?? []) {
      if (mechanicAdjusters[`${effectivePower!.internalName}:${ce.toggleId}`]) {
        entityList.push({ entityName: ce.entity, count: 1 });
      }
    }
    if (entityList.length === 0 && resolvedList.length === 0) return null;

    const globalDmgBonus = globalBonusesForCalc.damage || 0;
    const results: PetDamageResult[] = [];
    let base = 0, enhanced = 0, final_ = 0;

    // Henchman upgrades come off the build, same as the InfoPanel. Omitting
    // them here (which is what this did) meant hovering a summon quoted the
    // un-upgraded DPS while the panel beside it quoted the upgraded one.
    const taken = takenPowerNames(build);

    for (const e of entityList) {
      const applyEnh = shouldApplyEnhancements(e.entityName, summon.copyBoosts);
      const enhBonus = applyEnh ? (enhancementBonuses.damage || 0) : 0;
      const tiers = resolveActiveUpgradeTiers(getPetEntity(e.entityName), taken) ?? [];
      const result = calculatePetDamage(e.entityName, build.level, e.count, summon.duration, enhBonus, applyEnh, globalDmgBonus, tiers);
      if (result) {
        results.push(result);
        base += result.aggregateDpsBase;
        enhanced += result.aggregateDpsEnhanced;
        final_ += result.aggregateDpsFinal;
      }
    }

    // Synthesized location pseudo-pets (Storm Cell, Category Five, …) — parity
    // with the InfoPanel. Damage off the summoner's AT; effects (with chance)
    // surface as the chips below.
    for (const re of resolvedList) {
      const applyEnh = re.copyCreatorMods;
      const enhBonus = applyEnh ? (enhancementBonuses.damage || 0) : 0;
      const result = calculateResolvedPseudoPetDamage(re, archetypeId ?? '', build.level, enhBonus, applyEnh, globalDmgBonus, stormCellActive);
      if (result) {
        results.push(result);
        base += result.aggregateDpsBase;
        enhanced += result.aggregateDpsEnhanced;
        final_ += result.aggregateDpsFinal;
      }
    }

    return results.length > 0 ? { results, base, enhanced, final: final_ } : null;
  }, [effectivePower?.summon, effectivePower?.internalName, build, enhancementBonuses.damage, globalBonusesForCalc.damage, archetypeId, stormCellActive, mechanicAdjusters]);

  // Calculate archetype-specific damage bonuses
  const damageDisplayInfo = useMemo(() => {
    if (!calculatedDamage) return null;

    // Check which archetype inherent applies
    const isCorruptor = archetypeId === 'corruptor';
    const isCorruptorPower = isCorruptorAttackPower(powerSet);
    const showScourge = isCorruptor && isCorruptorPower && scourgeActive;

    const isBrute = archetypeId === 'brute';
    const isBrutePower = isBruteAttackPower(powerSet);
    const showFury = isBrute && isBrutePower && furyLevel > 0;

    const isDefender = archetypeId === 'defender';
    const isDefenderAttackPower = isDefenderPower(powerSet);
    const vigilanceBonus = calculateVigilanceDamageBonus(build.level, vigilanceTeamSize);
    const showVigilance = isDefender && isDefenderAttackPower && vigilanceBonus > 0;

    const isScrapper = archetypeId === 'scrapper';
    const isScrapperPower = isScrapperAttackPower(powerSet);
    // The crit is the power's own rows added on top of the final — no rows against the
    // vs-higher target (or engine still loading) means no crit column, never a flat
    // archetype constant (the ×1.10 average this replaced).
    const critFinalAdd = critComponents(damageVsHigher).reduce((sum, c) => sum + c.total.final, 0);
    const showCriticalHits = isScrapper && isScrapperPower && criticalHitsActive && critFinalAdd > 0;

    const isStalker = archetypeId === 'stalker';
    const isStalkerPower = isStalkerAttackPower(powerSet);
    // Assassin's Strike carries a data-driven from-Hide multiplier (bigger than a
    // normal crit). When hidden it replaces the generic +100%; mid-combat it's
    // ignored and the Assassin's Focus crit chance applies.
    const assassinationBonus = calculateAssassinationDamageBonus(
      effectiveHidden,
      stalkerTeamSize,
      basePower?.fromHideBonus
    );
    const showAssassination = isStalker && isStalkerPower && stalkerCritActive;

    const isController = archetypeId === 'controller';
    const isControllerAttackPower = isControllerPower(powerSet);
    const showContainment = isController && isControllerAttackPower && containmentActive;

    const isSentinel = archetypeId === 'sentinel';
    const isSentinelPower = isSentinelAttackPower(powerSet);
    const showOpportunityCrit = isSentinel && isSentinelPower && sentinelCritActive;

    // Determine final column header. NOTE: this column is only for hit-time
    // multipliers OUTSIDE the damage cap (crits, Scourge, Containment). Fury
    // and Vigilance are additive damage-strength buffs already folded into the
    // capped Final by calculateCharacterTotals, so they're surfaced as info
    // chips below (showFury/showVigilance) rather than a column — applying them
    // here would double-count against the Final.
    const finalColumnHeader = showScourge ? 'w/ Scourge'
      : showCriticalHits ? 'w/ Crit'
      : showAssassination ? (effectiveHidden ? (basePower?.fromHideBonus != null ? 'w/ Assassinate' : 'w/ Crit') : 'w/ Assassin')
      : showContainment ? 'w/ Contain'
      : showOpportunityCrit ? 'w/ Crit'
      : 'Final';

    // Get color class for inherent-modified values
    const finalColumnColor = showScourge ? 'text-sk-magenta'
      : showCriticalHits ? 'text-sk-magenta'
      : showAssassination ? 'text-sk-magenta'
      : showContainment ? 'text-sk-magenta'
      : showOpportunityCrit ? 'text-sk-magenta'
      : 'text-amber-400';

    // Function to apply inherent bonus (hit-time mechanics only). The Scrapper crit is
    // ADDITIVE — the power's own crit rows, what a critical actually deals — while the
    // other mechanics still apply their multiplier model.
    const applyInherentBonus = (damage: number) => {
      if (showScourge) return calculateScourgeDamage(damage);
      if (showCriticalHits) return damage + critFinalAdd;
      if (showAssassination) return calculateAssassinationDamage(damage, effectiveHidden, stalkerTeamSize, basePower?.fromHideBonus);
      if (showContainment) return calculateContainmentDamage(damage, true);
      if (showOpportunityCrit) return calculateOpportunityCritDamage(damage);
      return damage;
    };

    return {
      finalColumnHeader,
      finalColumnColor,
      applyInherentBonus,
      showScourge,
      showFury,
      showVigilance,
      showCriticalHits,
      showAssassination,
      showContainment,
      showOpportunityCrit,
      furyBonus: showFury ? calculateFuryDamageBonus(furyLevel) : 0,
      vigilanceBonus,
      assassinationBonus,
      opportunityCritBonus: showOpportunityCrit ? getOpportunityCritBonus() : 0,
    };
  }, [calculatedDamage, archetypeId, powerSet, scourgeActive, furyLevel, vigilanceTeamSize,
      criticalHitsActive, effectiveHidden, stalkerTeamSize, stalkerCritActive, containmentActive,
      sentinelCritActive, build.level, damageVsHigher]);

  // All hooks are above this point — safe to early return
  if (!basePower) {
    return <div className="text-slate-400 text-xs">Power not found</div>;
  }

  // The bag the registry display resolves, shared with InfoPanel (PROD6C-3a). The pseudo-pet
  // effects sit under it — the parent power's own keys win on a collision.
  //
  // Assassin's Strike: fast Quick animation mid-combat, slow interruptible cast from Hide.
  // Mirror the from-Hide toggle (which drives its damage) so cast time matches the state —
  // not hidden → the fast mid-combat cast. That is UI state, so it stays here rather than in
  // the shared build (InfoPanel applies the same rule to the power's own stats).
  const effects = withPseudoPetEffects(effectivePower!, buildDisplayEffects(effectivePower!));

  // Check if power has any enhancements
  const hasEnhancements = selectedPower && selectedPower.slots.some(s => s !== null);

  return (
    <div className="space-y-1.5 max-w-[320px]">
      {/* Header */}
      <div>
        <h3 className="text-sm font-bold text-[var(--color-link)] leading-tight">
          {basePower.name}
          {hasEnhancements && (
            <span className="text-[10px] text-green-500 ml-1">(enhanced)</span>
          )}
        </h3>
        <span className="text-[11px] text-slate-300 capitalize">{basePower.powerType}</span>
      </div>

      {/* Short Help */}
      {basePower.shortHelp && (
        <div className="text-[11px] text-amber-400/80 italic">
          {basePower.shortHelp}
        </div>
      )}

      {/* Summon/Pet Info with DPS and Effects */}
      {effects?.summon && (
        <div className="bg-indigo-900/30 rounded p-1 border border-indigo-500/30 text-[11px]">
          {/* Entity labels — filter visual-only / opaque P-hash entities
              (Rain of Arrows pairs `Pets_RainofArrows` with a `P4047293352`
              visual marker that's just FX). Keep entries with PET_ENTITIES
              data OR a recognizable real pet-name prefix. */}
          {effects.summon.entities ? (
            <>
              {effects.summon.mutuallyExclusive && (
                <div className="text-slate-300 text-[10px] italic">Summons 1 of (tier matches henchman):</div>
              )}
              {effects.summon.entities
                .filter((e) =>
                  /^(Pets_|MastermindPets_|Villain_Pets_|VillainPets_)/i.test(e.entity),
                )
                .map((e) => {
                  const displayName = e.entity.replace(/^(Pets_|MastermindPets_)/i, '').replace(/_/g, ' ');
                  return (
                    <div key={e.entity} className="flex items-center gap-1">
                      <span className="text-indigo-400">🐾</span>
                      <span className="text-slate-200">{displayName}{!effects.summon!.mutuallyExclusive && e.count > 1 ? ` x${e.count}` : ''}</span>
                    </div>
                  );
                })}
            </>
          ) : (
            // Single entity
            <div className="flex items-center gap-1">
              <span className="text-indigo-400">
                {effects.summon.isPseudoPet ? '⚡' : '🐾'}
              </span>
              <span className="text-slate-200">
                {effects.summon.displayName || effects.summon.entity || 'Entity'}
                {(effects.summon.entityCount && effects.summon.entityCount > 1) ? ` x${effects.summon.entityCount}` : ''}
              </span>
              {effects.summon.duration && (
                <span className="text-slate-300">({formatSummonDuration(effects.summon.duration)})</span>
              )}
            </div>
          )}
          {petDamageAggregate && petDamageAggregate.base > 0 && (
            <div className="flex items-center gap-1 mt-0.5 ml-3">
              <span className="text-slate-300">{petDamageAggregate.results.length > 1 ? 'Total DPS:' : 'DPS:'}</span>
              <span className="text-red-400">{petDamageAggregate.base.toFixed(1)}</span>
              {petDamageAggregate.enhanced !== petDamageAggregate.base && (
                <span className="text-green-400">→ {petDamageAggregate.enhanced.toFixed(1)}</span>
              )}
              {petDamageAggregate.final !== petDamageAggregate.enhanced && (
                <span className="text-amber-400">→ {petDamageAggregate.final.toFixed(1)}</span>
              )}
            </div>
          )}
          {petDamageAggregate && petDamageAggregate.results.some(r => r.allEffects.length > 0) && (
            <div className="flex flex-wrap gap-0.5 mt-0.5 ml-3">
              {/* Unique effects across all entities. Keyed by the full LABEL, not
                  the bare type: two entities' "Protection" chips are different
                  chips when one is Placate and the other Confuse. */}
              {Array.from(new Map(
                petDamageAggregate.results.flatMap(r => r.allEffects).map(eff => [petEffectLabel(eff), eff])
              ).entries()).map(([label, eff]) => {
                const chanceLabel = eff.chance && eff.chance < 1 ? ` ${(eff.chance * 100).toFixed(0)}%` : '';
                return (
                  <span key={label} className="text-[10px] text-purple-300 bg-purple-900/30 px-0.5 rounded">
                    {label} {formatPetEffectValue(eff)}{chanceLabel}
                  </span>
                );
              })}
            </div>
          )}
        </div>
      )}

      {/* Consolidated Power Effects - registry-driven display */}
      <RegistryEffectsDisplay
        effects={effects}
        allowedEnhancements={basePower?.allowedEnhancements || []}
        enhancementBonuses={enhancementBonuses}
        globalBonuses={globalBonusesWithStrength}
        archetypeId={archetypeId ?? undefined}
        level={build.level}
        targetsAffected={basePower?.targetsAffected}
        rows={effectRows}
        categories={['execution', 'buff', 'debuff', 'control', 'protection', 'movement']}
        dominationActive={dominationActive}
        compact={true}
        header="Power Effects"
        duration={effects?.buffDuration}
        damage={calculatedDamage}
        finalColumnHeader={damageDisplayInfo?.finalColumnHeader}
        finalColumnColor={damageDisplayInfo?.finalColumnColor}
        applyInherentBonus={damageDisplayInfo?.applyInherentBonus}
        purplePatchInfo={{
          factor: Math.min(0.95, Math.max(0.05, getBaseToHit(targetLevelOffset - globalBonuses.levelShift) + globalBonuses.toHit / 100)) / 0.75,
          offset: targetLevelOffset,
          toHitBonus: globalBonuses.toHit,
          combatModifier: globalBonuses.combatModifier ?? 1,
        }}
      />

      {/* Damage bar - overlaid base/enhanced/final relative to AT cap */}
      {calculatedDamage && !calculatedDamage.unknown && calculatedDamage.scale && (() => {
        const damageCap = getDamageCap(archetypeId ?? '');

        // Include DoT total damage in the bar (direct + DoT×effectiveTicks).
        // effectiveTicks weights chance-gated / cancel-on-miss DoTs so the bar
        // matches the in-game average.
        const dot = calculatedDamage.dotDamage;
        const isPureDot = dot && Math.abs(calculatedDamage.base - dot.base) <= 0.001;
        const et = dot ? dot.effectiveTicks : 0;
        const totalBase = isPureDot
          ? dot.base * et
          : calculatedDamage.base + (dot ? dot.base * et : 0);
        const totalEnhanced = isPureDot
          ? dot.enhanced * et
          : calculatedDamage.enhanced + (dot ? dot.enhanced * et : 0);
        const totalFinal = isPureDot
          ? dot.final * et
          : calculatedDamage.final + (dot ? dot.final * et : 0);

        // Normalize to the build's highest-damage attack so bar length is
        // comparable across powers (Mids-style). Matches DamageBar; falls back
        // to the AT's scale-1.0 capped damage when no damaging powers exist.
        const referenceDamage = maxBuildDamage && maxBuildDamage > 0
          ? maxBuildDamage
          : (calculatedDamage.base / calculatedDamage.scale) * damageCap;
        const basePercent = Math.min((totalBase / referenceDamage) * 100, 100);
        const enhPercent = Math.min((totalEnhanced / referenceDamage) * 100, 100);
        const finalPercent = Math.min((totalFinal / referenceDamage) * 100, 100);

        return (
          <div className="relative h-2 bg-slate-700/30 rounded overflow-hidden" title={`Damage cap: ${(damageCap * 100).toFixed(0)}%`}>
            {/* Final / Enhanced / Base tiers — same red ramp as the InfoPanel bar */}
            <div
              className="absolute inset-y-0 left-0 bg-red-800 rounded-l transition-all duration-300"
              style={{ width: `${finalPercent}%` }}
            />
            <div
              className="absolute inset-y-0 left-0 bg-red-400 rounded-l transition-all duration-300"
              style={{ width: `${enhPercent}%` }}
            />
            <div
              className="absolute inset-y-0 left-0 bg-red-200 rounded-l transition-all duration-300"
              style={{ width: `${basePercent}%` }}
            />
          </div>
        );
      })()}

      {/* Fiery Embrace conditional damage (shown separately since it's conditional) */}
      {calculatedDamage?.fieryEmbraceDamage && damageDisplayInfo && (
        <div className="bg-slate-800/50 rounded p-1.5">
          {(() => {
            const fe = calculatedDamage.fieryEmbraceDamage;
            const hasEnh = Math.abs(fe.enhanced - fe.base) > 0.001;
            const feFinal = damageDisplayInfo.applyInherentBonus(fe.final);
            const hasGlobal = Math.abs(feFinal - fe.enhanced) > 0.001;
            const isActive = isFieryEmbraceActive;
            return (
              <>
                <div className={`grid grid-cols-[3rem_1fr_1fr_1fr] gap-1 items-baseline text-xs ${isActive ? '' : 'opacity-40'}`}>
                  <span className={isActive ? 'text-orange-400' : 'text-slate-400'}>{fe.type}</span>
                  <span className={isActive ? 'text-slate-300' : 'text-slate-400'}>{fe.base.toFixed(2)}</span>
                  <span className={isActive ? (hasEnh ? 'text-green-400' : 'text-slate-500') : 'text-slate-500'}>
                    {hasEnh ? `→ ${fe.enhanced.toFixed(2)}` : '—'}
                  </span>
                  <span className={isActive ? (hasGlobal ? damageDisplayInfo.finalColumnColor : 'text-slate-500') : 'text-slate-500'}>
                    {hasGlobal ? `→ ${feFinal.toFixed(2)}` : '—'}
                  </span>
                </div>
                <div className={`text-[10px] italic mt-0.5 ${isActive ? 'text-orange-400' : 'text-slate-400'}`}>
                  {isActive ? '✓ Fiery Embrace active' : '* Fire damage only with Fiery Embrace active'}
                </div>
              </>
            );
          })()}
        </div>
      )}

      {/* Inherent damage bonus info (Scourge, Fury, etc.) */}
      {damageDisplayInfo && calculatedDamage && (
        <>
          {damageDisplayInfo.showScourge && (
            <div className="text-[10px] text-sk-magenta">
              +{(getScourgeInfo().averageDamageBonus * 100).toFixed(0)}% avg from Scourge
            </div>
          )}
          {damageDisplayInfo.showFury && (
            <div className="text-[10px] text-sk-magenta">
              +{(damageDisplayInfo.furyBonus * 100).toFixed(0)}% from Fury ({furyLevel}/100)
            </div>
          )}
          {damageDisplayInfo.showVigilance && (
            <div className="text-[10px] text-sk-magenta">
              +{(damageDisplayInfo.vigilanceBonus * 100).toFixed(0)}% from Vigilance ({vigilanceTeamSize === 0 ? 'Solo' : `${vigilanceTeamSize} teammates`})
            </div>
          )}
          {damageDisplayInfo.showCriticalHits && (() => {
            // showCriticalHits already proved the vs-higher branch has rows.
            const crit = critBranchSummary(damageVsHigher)!;
            return (
              <div className="text-[10px] text-sk-magenta">
                +{crit.finalTotal.toFixed(2)} from Critical Hits ({crit.chanceLabel} chance vs Lt+)
              </div>
            );
          })()}
          {damageDisplayInfo.showAssassination && (
            <div className="text-[10px] text-sk-magenta">
              +{(damageDisplayInfo.assassinationBonus * 100).toFixed(0)}% avg from Assassination ({effectiveHidden ? 'Alpha Strike' : 'Sustained'})
            </div>
          )}
          {damageDisplayInfo.showOpportunityCrit && (
            <div className="text-[10px] text-sk-magenta">
              +{(damageDisplayInfo.opportunityCritBonus * 100).toFixed(0)}% from Opportunity Crit
            </div>
          )}
        </>
      )}

      {/* DoT from effects.dot (secondary DoT) */}
      {effects?.dot && effects.dot.scale != null && (
        <div className="text-xs">
          <span className="text-slate-300 text-[11px] uppercase">DoT: </span>
          <span className="text-orange-400">
            {effects.dot.type} {effects.dot.scale.toFixed(2)}/tick × {effects.dot.ticks} = {(effects.dot.scale * effects.dot.ticks).toFixed(2)} total
          </span>
        </div>
      )}

      {/* Blaster Defiance - show for Blaster primary/secondary attack powers */}
      {(() => {
        // Only show for Blaster archetype and blaster powersets
        if (archetypeId !== 'blaster' || !powerSet.startsWith('blaster/')) return null;

        const defianceInfo = calculateDefiance(effects, basePower.effectArea);
        if (!defianceInfo || defianceInfo.damageBonus <= 0) return null;

        return (
          <div className="text-xs mt-1 bg-orange-900/20 rounded px-1.5 py-1 border border-orange-700/30">
            <span className="text-orange-400 font-medium">Defiance: </span>
            <span className="text-orange-300">+{defianceInfo.damageBonus.toFixed(1)}% dmg</span>
            <span className="text-slate-300"> for </span>
            <span className="text-orange-300">{defianceInfo.duration.toFixed(1)}s</span>
          </div>
        );
      })()}

      {/* Dominator Domination — show only for powers that actually carry a
          `Tag "Domination"` mez bonus (per-power data), so it appears on tagged
          assault powers too and stays hidden on untagged holds. */}
      {(() => {
        if (archetypeId !== 'dominator') return null;
        const domSummary = getPowerDominationSummary(effects, basePower.conditionalEffects);
        if (!domSummary) return null;

        const dominationInfo = getDominationInfo();
        const fmt = (n: number) => (Number.isInteger(n) ? String(n) : parseFloat(n.toFixed(2)).toString());

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            dominationActive
              ? 'bg-pink-900/30 border-pink-700/50'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${dominationActive ? 'text-pink-400' : 'text-slate-300'}`}>
                Domination
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                dominationActive
                  ? 'bg-pink-600/50 text-pink-200'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {dominationActive ? 'ACTIVE' : 'Inactive'}
              </span>
            </div>
            {dominationActive && (
              <div className="mt-1 text-[11px]">
                <span className="text-pink-300">×{fmt(domSummary.magMultiplier)} Mag, </span>
                <span className="text-pink-300">×{fmt(domSummary.durMultiplier)} Dur</span>
                <span className="text-slate-400 ml-1">({dominationInfo.activeDuration}s)</span>
              </div>
            )}
            {!dominationActive && (
              <div className="text-[10px] text-slate-400 mt-0.5">
                Toggle Domination to see enhanced values
              </div>
            )}
          </div>
        );
      })()}

      {/* Corruptor Scourge - show when viewing Corruptor attack powers */}
      {(() => {
        // Only show for Corruptor archetype and corruptor powersets with damage
        if (archetypeId !== 'corruptor' || !isCorruptorAttackPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const scourgeInfo = getScourgeInfo();

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            scourgeActive
              ? 'bg-sk-magenta/10 border-sk-magenta/30'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${scourgeActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Scourge
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                scourgeActive
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {scourgeActive ? 'SHOWING AVG' : 'Hidden'}
              </span>
            </div>
            {scourgeActive && (
              <div className="mt-1 text-[11px]">
                <span className="text-sk-magenta/70">+{(scourgeInfo.averageDamageBonus * 100).toFixed(0)}% avg dmg</span>
                <span className="text-slate-400 ml-1">(×{(1 + scourgeInfo.averageDamageBonus).toFixed(2)} multiplier)</span>
              </div>
            )}
            {!scourgeActive && (
              <div className="text-[10px] text-slate-400 mt-0.5">
                {scourgeInfo.chanceFormula}
              </div>
            )}
          </div>
        );
      })()}

      {/* Brute Fury - show when viewing Brute attack powers */}
      {(() => {
        // Only show for Brute archetype and brute powersets with damage
        if (archetypeId !== 'brute' || !isBruteAttackPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const furyInfo = getFuryInfo();
        const currentBonus = calculateFuryDamageBonus(furyLevel);

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            furyLevel > 0
              ? 'bg-sk-magenta/10 border-sk-magenta/30'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${furyLevel > 0 ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Fury
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                furyLevel > 0
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {furyLevel}/{furyInfo.maxFury}
              </span>
            </div>
            <div className="mt-1 text-[11px]">
              <span className="text-sk-magenta/70">+{(currentBonus * 100).toFixed(0)}% damage</span>
              <span className="text-slate-400 ml-1">({furyInfo.damagePerFury * 100}% per fury)</span>
            </div>
          </div>
        );
      })()}

      {/* Mastermind Supremacy - show when viewing Mastermind powers */}
      {(() => {
        // Only show for Mastermind archetype and mastermind powersets
        if (archetypeId !== 'mastermind' || !isMastermindPower(powerSet)) return null;

        const supremacyInfo = getSupremacyInfo();
        const bodyguardInfo = getBodyguardInfo();

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            supremacyActive
              ? 'bg-sk-magenta/10 border-sk-magenta/30'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${supremacyActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Supremacy
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                supremacyActive
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {supremacyActive ? 'ACTIVE' : 'Inactive'}
              </span>
            </div>
            {supremacyActive && (
              <div className="mt-1 space-y-0.5">
                <div className="text-[11px]">
                  <span className="text-slate-400">Henchmen within {supremacyInfo.radius}ft:</span>
                </div>
                <div className="text-[11px] flex gap-3">
                  <span className="text-sk-magenta/70">+{(supremacyInfo.damageBonus * 100).toFixed(0)}% Damage</span>
                  <span className="text-sk-magenta/70">+{(supremacyInfo.toHitBonus * 100).toFixed(0)}% ToHit</span>
                </div>
                <div className="text-[11px] mt-1 pt-1 border-t border-sk-magenta/30">
                  <span className="text-slate-400">Bodyguard Mode:</span>
                  <span className="text-sk-magenta/70 ml-1">
                    {(bodyguardInfo.mastermindDamageShare * 100).toFixed(0)}% to MM, {(bodyguardInfo.henchmenDamageShare * 100).toFixed(0)}% to pets
                  </span>
                </div>
              </div>
            )}
            {!supremacyActive && (
              <div className="text-[10px] text-slate-400 mt-0.5">
                Enable to see henchmen buff values
              </div>
            )}
          </div>
        );
      })()}

      {/* Defender Vigilance - show when viewing Defender powers */}
      {(() => {
        // Only show for Defender archetype and defender powersets with damage
        if (archetypeId !== 'defender' || !isDefenderPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const vigilanceInfo = getVigilanceInfo();
        const currentBonus = calculateVigilanceDamageBonus(build.level, vigilanceTeamSize);
        const teamLabel = vigilanceTeamSize === 0 ? 'Solo' : `${vigilanceTeamSize} teammate${vigilanceTeamSize > 1 ? 's' : ''}`;

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            currentBonus > 0
              ? 'bg-sk-magenta/10 border-sk-magenta/30'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${currentBonus > 0 ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Vigilance
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                currentBonus > 0
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {teamLabel}
              </span>
            </div>
            <div className="mt-1 text-[11px]">
              {currentBonus > 0 ? (
                <span className="text-sk-magenta/70">+{(currentBonus * 100).toFixed(0)}% damage</span>
              ) : (
                <span className="text-slate-400">No bonus (3+ teammates)</span>
              )}
              <span className="text-slate-400 ml-1">(max {(vigilanceInfo.maxSoloDamageBonus * 100).toFixed(0)}% solo at Lv20+)</span>
            </div>
          </div>
        );
      })()}


      {/* Scrapper Critical Hits — the power's OWN crit rows per rank branch (the export
          states the chance and the crit's own damage, and both vary by power). */}
      {(() => {
        // Only show for Scrapper archetype and scrapper powersets with damage
        if (archetypeId !== 'scrapper' || !isScrapperAttackPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const minion = critBranchSummary(damageVsMinion);
        const higher = critBranchSummary(damageVsHigher);

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            criticalHitsActive
              ? 'bg-sk-magenta/10 border-sk-magenta/30'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${criticalHitsActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Critical Hits
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                criticalHitsActive
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700 text-slate-300'
              }`}>
                {criticalHitsActive ? 'IN FINAL (vs Lt+)' : 'Hidden'}
              </span>
            </div>
            {criticalHitsActive && (
              <div className="mt-1 space-y-0.5 text-[11px]">
                <div className="flex justify-between">
                  <span className="text-slate-300">vs Minions:</span>
                  <span className="text-sk-magenta/70">
                    {minion ? `${minion.chanceLabel} chance → +${minion.finalTotal.toFixed(2)}` : 'no crit'}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-300">vs Lt/Boss+:</span>
                  <span className="text-sk-magenta/70">
                    {higher ? `${higher.chanceLabel} chance → +${higher.finalTotal.toFixed(2)}` : 'no crit'}
                  </span>
                </div>
              </div>
            )}
            {!criticalHitsActive && (
              <div className="text-[10px] text-slate-400 mt-0.5">
                Crit chance and damage are per power — toggle on to see this power&apos;s rows
              </div>
            )}
          </div>
        );
      })()}

      {/* Stalker Assassination - show when viewing Stalker attack powers */}
      {(() => {
        // Only show for Stalker archetype and stalker powersets with damage
        if (archetypeId !== 'stalker' || !isStalkerAttackPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const assassinationInfo = getAssassinationInfo();
        const currentBonus = calculateAssassinationDamageBonus(effectiveHidden, stalkerTeamSize, basePower?.fromHideBonus);
        const critChance = effectiveHidden ? 1.0 : assassinationInfo.baseCritChance + (stalkerTeamSize * assassinationInfo.critChancePerTeammate);

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            effectiveHidden
              ? 'bg-sk-magenta/15 border-sk-magenta/40'
              : currentBonus > 0
                ? 'bg-sk-magenta/10 border-sk-magenta/30'
                : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${effectiveHidden || currentBonus > 0 ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Assassination
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                effectiveHidden
                  ? 'bg-sk-magenta/25 text-sk-magenta'
                  : 'bg-sk-magenta/20 text-sk-magenta'
              }`}>
                {effectiveHidden ? 'ALPHA STRIKE' : `${stalkerTeamSize === 0 ? 'Solo' : `+${stalkerTeamSize}`}`}
              </span>
            </div>
            <div className="mt-1 space-y-0.5 text-[11px]">
              {effectiveHidden ? (
                <>
                  <div className="flex justify-between">
                    <span className="text-slate-300">From Hide:</span>
                    <span className="text-sk-magenta/70 font-medium">
                      {basePower?.fromHideBonus != null ? 'guaranteed Assassination' : '100% critical chance'}
                    </span>
                  </div>
                  <div className="text-[10px] text-sk-magenta/60 mt-0.5">
                    +{(currentBonus * 100).toFixed(0)}% {basePower?.fromHideBonus != null ? 'damage (Assassin’s Strike)' : 'avg damage (guaranteed double damage)'}
                  </div>
                </>
              ) : (
                <>
                  <div className="flex justify-between">
                    <span className="text-slate-300">Base crit:</span>
                    <span className="text-sk-magenta/70">{(assassinationInfo.baseCritChance * 100).toFixed(0)}%</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-300">Team bonus:</span>
                    <span className="text-sk-magenta/70">+{(stalkerTeamSize * assassinationInfo.critChancePerTeammate * 100).toFixed(0)}% ({stalkerTeamSize} × 3%)</span>
                  </div>
                  <div className="flex justify-between font-medium">
                    <span className="text-slate-300">Total:</span>
                    <span className="text-sk-magenta/70">{(critChance * 100).toFixed(0)}% → +{(currentBonus * 100).toFixed(0)}% avg</span>
                  </div>
                </>
              )}
            </div>
            <div className="text-[10px] text-slate-400 mt-1 pt-1 border-t border-sk-magenta/30">
              Assassin's Focus: Primary attacks can stack +33.3% crit (×3) for Assassin's Strike
            </div>
          </div>
        );
      })()}

      {/* Tanker Gauntlet - show when viewing Tanker powers */}
      {(() => {
        // Only show for Tanker archetype and tanker powersets
        if (archetypeId !== 'tanker' || !isTankerPower(powerSet)) return null;

        const gauntletInfo = getGauntletInfo();

        return (
          <div className="text-xs mt-1 rounded px-1.5 py-1 border bg-yellow-900/30 border-yellow-700/50">
            <div className="flex items-center justify-between">
              <span className="font-medium text-yellow-400">
                Gauntlet
              </span>
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-yellow-600/50 text-yellow-200">
                PunchVoke
              </span>
            </div>
            <div className="mt-1 space-y-0.5 text-[11px]">
              <div className="text-yellow-300">
                +{(gauntletInfo.aoeRadiusBonus * 100).toFixed(0)}% AoE Radius/Range
              </div>
              <div className="text-yellow-300">
                +{(gauntletInfo.coneArcBonus * 100).toFixed(0)}% Cone Arc
              </div>
              <div className="text-yellow-300">
                ST attacks taunt target + {gauntletInfo.singleTargetTauntSplash} nearby
              </div>
              <div className="text-yellow-300">
                AoE attacks taunt all affected
              </div>
            </div>
            <div className="text-[10px] text-slate-400 mt-1 pt-1 border-t border-yellow-700/30">
              PBAoE hits bonus targets at {(gauntletInfo.bonusTargetDamageMultiplier * 100).toFixed(0)}% damage
            </div>
          </div>
        );
      })()}

      {/* Containment info panel - Controllers */}
      {(() => {
        // Only show for Controller archetype and controller powersets with damage
        if (archetypeId !== 'controller' || !isControllerPower(powerSet)) return null;
        if (!calculatedDamage) return null;

        const containmentInfo = getContainmentInfo();

        return (
          <div className={`text-xs mt-1 rounded px-1.5 py-1 border ${
            containmentActive
              ? 'bg-sk-magenta/15 border-sk-magenta/40'
              : 'bg-slate-800/50 border-slate-700/30'
          }`}>
            <div className="flex items-center justify-between">
              <span className={`font-medium ${containmentActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                Containment
              </span>
              <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                containmentActive
                  ? 'bg-sk-magenta/20 text-sk-magenta'
                  : 'bg-slate-700/50 text-slate-300'
              }`}>
                {containmentActive ? 'Active' : 'Inactive'}
              </span>
            </div>
            <div className="mt-1 space-y-0.5 text-[11px]">
              <div className={containmentActive ? 'text-sk-magenta/70' : 'text-slate-400'}>
                {containmentInfo.description}
              </div>
              {containmentActive && (
                <div className="text-sk-magenta font-medium">
                  +{((containmentInfo.damageMultiplier - 1) * 100).toFixed(0)}% damage bonus
                </div>
              )}
            </div>
            <div className="text-[10px] text-slate-400 mt-1 pt-1 border-t border-sk-magenta/30">
              Toggle in header to show damage vs controlled targets
            </div>
          </div>
        );
      })()}

      {/* Enhancement summary */}
      {hasEnhancements && Object.keys(enhancementBonuses).length > 0 && (
        <div className="border-t border-slate-700 pt-1 mt-1">
          <span className="text-[10px] text-slate-400 uppercase">Enhancement Bonuses (after ED):</span>
          <div className="flex flex-wrap gap-x-2 text-[11px]">
            {Object.entries(enhancementBonuses).map(([aspect, value]) => (
              <span key={aspect} className="text-green-400">
                {aspect}: +{((value || 0) * 100).toFixed(2)}%
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

// Enhancement info content for slotted enhancements
import { EnhancementInfoContent } from './EnhancementInfoContent';

// `powerSet` is required on BOTH variants: a power's identity is
// (powerSet, internalName) — see `findSelectedPowerInBuild`.
type TooltipContent =
  | { type: 'power'; powerName: string; powerSet: string }
  | { type: 'slotted-enhancement'; powerName: string; powerSet: string; slotIndex: number };

function TooltipBody({ content }: { content: TooltipContent }) {
  return (
    <div className="bg-slate-900/95 border border-slate-600 rounded-lg shadow-xl p-2">
      {content.type === 'power' && (
        <PowerInfoContent powerName={content.powerName} powerSet={content.powerSet} />
      )}
      {content.type === 'slotted-enhancement' && (
        <EnhancementInfoContent powerName={content.powerName} powerSet={content.powerSet} slotIndex={content.slotIndex} />
      )}
    </div>
  );
}

function sameContent(a: TooltipContent, b: TooltipContent): boolean {
  if (a.type !== b.type) return false;
  if (a.type === 'power' && b.type === 'power') {
    return a.powerName === b.powerName && a.powerSet === b.powerSet;
  }
  if (a.type === 'slotted-enhancement' && b.type === 'slotted-enhancement') {
    return a.powerName === b.powerName && a.slotIndex === b.slotIndex;
  }
  return false;
}

export function PowerInfoTooltip() {
  const tooltipEnabled = useUIStore((s) => s.infoPanel.tooltipEnabled);
  const infoPanelContent = useUIStore((s) => s.infoPanel.content);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [overSource, setOverSource] = useState(false);
  const [shiftHeld, setShiftHeld] = useState(false);
  const [pinned, setPinned] = useState<{ content: TooltipContent; position: { x: number; y: number } } | null>(null);
  const pinnedRef = useRef<HTMLDivElement>(null);
  // Measured width of the pinned tooltip so the live tooltip sits flush against it regardless of content size
  const [pinnedWidth, setPinnedWidth] = useState<number | null>(null);

  const handleMouseMove = useCallback((e: MouseEvent) => {
    // Position tooltip to the right and slightly below cursor
    const x = e.clientX + 15;
    const y = e.clientY + 10;

    // Keep tooltip within viewport
    const maxX = window.innerWidth - 300; // tooltip width estimate
    const maxY = window.innerHeight - 400; // tooltip height estimate

    setPosition({
      x: Math.min(x, maxX),
      y: Math.min(y, maxY),
    });

    // Track whether the cursor is over a tooltip-source element
    const target = e.target as Element | null;
    const onSource = !!target?.closest?.('[data-info-hover]');
    setOverSource(onSource);
  }, []);

  // Track Shift key state
  useEffect(() => {
    const down = (e: KeyboardEvent) => {
      if (e.key === 'Shift') setShiftHeld(true);
    };
    const up = (e: KeyboardEvent) => {
      if (e.key === 'Shift') {
        setShiftHeld(false);
        setPinned(null);
      }
    };
    window.addEventListener('keydown', down);
    window.addEventListener('keyup', up);
    return () => {
      window.removeEventListener('keydown', down);
      window.removeEventListener('keyup', up);
    };
  }, []);

  // When Shift is pressed while a tooltip is showing, pin the current content
  useEffect(() => {
    if (!shiftHeld) return;
    if (pinned) return;
    if (!infoPanelContent) return;
    if (infoPanelContent.type !== 'power' && infoPanelContent.type !== 'slotted-enhancement') return;
    if (!overSource) return;
    setPinned({
      content: infoPanelContent as TooltipContent,
      position: { ...position },
    });
    setPinnedWidth(null);
  }, [shiftHeld, pinned, infoPanelContent, overSource, position]);

  // Measure the pinned tooltip's rendered width so the live tooltip sits flush against it
  useLayoutEffect(() => {
    if (!pinned || !pinnedRef.current) return;
    const width = pinnedRef.current.getBoundingClientRect().width;
    if (width > 0 && width !== pinnedWidth) setPinnedWidth(width);
  }, [pinned, pinnedWidth]);

  // Determine if we should show the live (hover) tooltip
  const liveIsValid = !!infoPanelContent && (
    infoPanelContent.type === 'power' || infoPanelContent.type === 'slotted-enhancement'
  );
  // Hide the live tooltip when it duplicates the pinned one (so you don't see two identical cards)
  const liveDuplicate = !!pinned && liveIsValid && sameContent(pinned.content, infoPanelContent as TooltipContent);
  const showLive = tooltipEnabled && liveIsValid && overSource && !liveDuplicate;

  useEffect(() => {
    if (!tooltipEnabled) return;
    window.addEventListener('mousemove', handleMouseMove);
    return () => window.removeEventListener('mousemove', handleMouseMove);
  }, [tooltipEnabled, handleMouseMove]);

  if (!tooltipEnabled) return null;
  if (!showLive && !pinned) return null;

  // Position the pinned tooltip; place the live one beside it with a gap when both are shown
  const PIN_GAP = 8;
  const LIVE_WIDTH_EST = 320;
  // Use the measured pinned width so the live tooltip sits flush; fall back to the estimate until the first measurement lands
  const pinnedW = pinnedWidth ?? LIVE_WIDTH_EST;
  let liveLeft = position.x;
  let liveTop = position.y;
  if (pinned && showLive) {
    // Prefer placing the live tooltip to the right of the pinned; if off-screen, place to the left
    const preferredLeft = pinned.position.x + pinnedW + PIN_GAP;
    if (preferredLeft + LIVE_WIDTH_EST > window.innerWidth - 8) {
      liveLeft = Math.max(8, pinned.position.x - LIVE_WIDTH_EST - PIN_GAP);
    } else {
      liveLeft = preferredLeft;
    }
    liveTop = pinned.position.y;
  }

  return createPortal(
    <>
      {pinned && (
        <div
          className="fixed z-50 pointer-events-none"
          style={{ left: pinned.position.x, top: pinned.position.y }}
        >
          <div ref={pinnedRef} className="ring-2 ring-[var(--color-primary)]/70 rounded-lg inline-block">
            <TooltipBody content={pinned.content} />
          </div>
          <div className="text-[11px] text-[var(--color-link)] text-center mt-0.5">Pinned (Shift)</div>
        </div>
      )}
      {showLive && infoPanelContent && (
        <div
          className="fixed z-50 pointer-events-none"
          style={{ left: liveLeft, top: liveTop }}
        >
          <TooltipBody content={infoPanelContent as TooltipContent} />
        </div>
      )}
    </>,
    document.body
  );
}
