/**
 * InfoPanel - Displays detailed information about the currently hovered power
 * Renders inline within the Info Panel column (column headers are in PlannerPage)
 * Shows Base/Enhanced/Final values for enhanceable stats
 */

import { useMemo, useState } from 'react';
import { useUIStore, useBuildStore, useDominationActive, useScourgeActive, useFuryLevel, useContainmentActive, useCriticalHitsActive, useStalkerHidden, useStalkerTeamSize, useStalkerCritActive, useSentinelCritActive, useGlobalAdjuster } from '@/stores';
import { getBaseToHit } from '@/data/purple-patch';
import {
  lookupPower,
  getIOSet,
  getPowerIconPath,
  getAlphaEffects,
  getAlphaEdBypass,
  getDestinyEffects,
  getHybridEffects,
  getInterfaceEffects,
  getJudgementEffects,
  getLoreEffects,
  getGenesisEffects,
  formatEffectValue,
  isSlotToggleable,
  findProcData,
  getProcEffects,
  interpolateProcDamage,
  arcToDegrees,
  resolveProcRollGeometry,
  resolveProcRollSchedule,
  calculateScheduledProcChance,
  powerFiresProcs,
  getActiveDamageConversion,
} from '@/data';
import { useCharacterCalculation, useGlobalBonuses, usePowerProjection, usePowerDamageVsRank } from '@/hooks/useCalculatedStats';
import { calculatePowerEnhancementBonuses, combineWithAlphaED, calculatePowerDamage, getAlphaEnhancementBonuses, getAlphaEdBypassBonuses, abbreviateDamageType, calculateArcanaTime, dotTickCount, calculateDamageWithATTable, type EnhancementBonuses, type PowerDamageResult, isControllerPower, isCorruptorAttackPower, isBruteAttackPower, isScrapperAttackPower, isStalkerAttackPower, calculateFuryDamageBonus, calculateAssassinationDamageBonus, getContainmentInfo, getScourgeInfo, getFuryInfo, getEffectiveLevel, areIncarnatesSuppressed } from '@/utils/calculations';
import { resolveAtMechanic, applyAtMechanicBonus, critBranchSummary, critComponents, VS_HIGHER_RANK_SEGMENT, VS_MINION_RANK_SEGMENT } from '@/utils/calculations/power-at-mechanics';
import type { IOSetEnhancement } from '@/types';
import { isPermaEligible, recastVerdict } from '@/utils/calculations/perma';
import { getRechargeBounds } from '@/data/at-tables';
import { buildDisplayEffects, getStackingInfo, withPseudoPetEffects, withTargetsHit } from './buildDisplayEffects';
import { calculatePetDamage, calculateResolvedPseudoPetDamage, shouldApplyEnhancements, resolveProcAreaGeometry, resolveProcPatchDuration, type PetDamageResult, type PetAbilityDamage, type PetEffectComputed } from '@/utils/calculations/pet-damage';
import { getPetEntity, type PetAbility } from '@/data/pet-entities';
import { petUpgradeStatuses, resolveActiveUpgradeTiers, takenPowerNames, type PetUpgradeStatus } from '@/utils/calculations/pet-upgrades';
import { calculateIncarnateDamage } from '@/data/at-tables';
import { getBaselineHealth } from '@/utils/calculations/stats';
import type { GenesisExemplarEffect } from '@/data';
import { getActiveIncarnateDamageProcs, computeIncarnateProcContributions } from '@/data/incarnate-procs';
import { resolvePath } from '@/utils/paths';
import { useModeSuppression } from '@/hooks/useModeSuppression';
import { modeLabel } from '@/utils/mode-suppression';
import { EnhancementInfoContent } from './EnhancementInfoContent';
import { MechanicAdjusters } from './MechanicAdjusters';
import { BuffPetAuraToggle } from './BuffPetAuraToggle';
import { SlottedProcControls } from './SlottedProcControls';
import { DamageBlock } from './DamageBlock';
import { ShortHelpChips } from './TagsRow';
import { PowerMetaTags, AllowedEnhancementsBlock, GeneralStatsBlock } from './PowerInfoBlocks';
import type {
  ArchetypeId,
  Power,
  IncarnateSlotId,
  ToggleableIncarnateSlot,
  SummonEffect,
} from '@/types';
import { getSlotColor, getTierColor, getTierDisplayName } from '@/data';
import {
  convertGlobalBonusesToAspects,
  withStrengthBonuses,
  findSelectedPowerInBuild,
  calcThreeTier,
} from './powerDisplayUtils';
import { resolveEffectivePower, effectiveGlobalAdjusters, isCasterHidden, currentToHitFraction, isQuickSnipeActive } from './resolveEffectivePower';
import { magnitudesFromProjection } from './magnitudesFromProjection';
import {
  RegistryEffectsDisplay,
} from './SharedPowerComponents';
import {
  formatEffectArea,
  formatPetEffectValue,
  isPetStatCarrier,
  partitionPetEffects,
  petEffectColor,
  petEffectLabel,
  formatSummonDuration,
  PERMANENT_PSEUDOPET_DURATION,
} from './petEffectDisplay';

export function InfoPanel() {
  const infoPanel = useUIStore((s) => s.infoPanel);
  const unlockInfoPanel = useUIStore((s) => s.unlockInfoPanel);

  // If locked, show locked content; otherwise show hover content
  const content = infoPanel.locked ? infoPanel.lockedContent : infoPanel.content;

  if (!content) {
    return (
      <div className="text-slate-400 text-sm text-center py-8 italic">
        Hover over a power to see details
      </div>
    );
  }

  return (
    <div className="relative">
      {/* Lock indicator — locked: amber filled lock; unlocked: dim open lock as a discoverable affordance */}
      {infoPanel.locked ? (
        <button
          onClick={unlockInfoPanel}
          className="absolute top-0 right-0 text-amber-400 hover:text-amber-300 p-1 z-10"
          title="Locked — this panel won't change as you hover. Click to unlock and resume hover-to-preview."
          aria-label="Unlock info panel"
        >
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
            <path fillRule="evenodd" d="M5 9V7a5 5 0 0110 0v2a2 2 0 012 2v5a2 2 0 01-2 2H5a2 2 0 01-2-2v-5a2 2 0 012-2zm8-2v2H7V7a3 3 0 016 0z" clipRule="evenodd" />
          </svg>
        </button>
      ) : (
        <div
          className="absolute top-0 right-0 text-slate-500 p-1 z-10 pointer-events-auto cursor-help"
          title="Hover-preview mode — the panel updates as you hover. Right click any power to lock its details here."
          aria-label="Info panel: hover-preview mode"
        >
          <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 20 20">
            <path fillRule="evenodd" d="M10 2a5 5 0 00-5 5v2H4a2 2 0 00-2 2v5a2 2 0 002 2h12a2 2 0 002-2v-5a2 2 0 00-2-2H7V7a3 3 0 015.905-.75 1 1 0 001.937-.5A5.002 5.002 0 0010 2z" clipRule="evenodd" />
          </svg>
        </div>
      )}

      {content.type === 'power' && (
        <PowerInfo powerName={content.powerName} powerSet={content.powerSet} />
      )}

      {content.type === 'enhancement' && (
        <EnhancementInfo enhancementId={content.enhancementId} />
      )}

      {content.type === 'slotted-enhancement' && (
        <EnhancementInfoContent powerName={content.powerName} powerSet={content.powerSet} slotIndex={content.slotIndex} />
      )}

      {content.type === 'incarnate' && (
        <IncarnateInfo slotId={content.slotId as IncarnateSlotId} powerId={content.powerId} />
      )}
    </div>
  );
}

interface PowerInfoProps {
  powerName: string;
  powerSet: string;
}

function PowerInfo({ powerName, powerSet }: PowerInfoProps) {
  const build = useBuildStore((s) => s.build);
  const archetypeId = build.archetype.id;
  // Mode suppression — which build powers are currently suspended by an active
  // mode-setter (Granite Armor suspends the other Stone toggles, etc.).
  const modeSuppression = useModeSuppression();
  // Global "Storm Cell Active" / High Winds — when on, the storm pseudo-pet's
  // powered-up state applies (WindSpeed debuffs + lightning folded into damage).
  const stormCellActive = useGlobalAdjuster('stormblast_instormcell', false);
  const globalBonuses = useGlobalBonuses();
  // The damage-bar scale, off the engine's own totals rather than a second fold in TS. It used
  // to be `useBuildMaxAttackDamage`, a maximum over the build's PICKED powers — always attained,
  // so one power read full on every build no matter how hard it actually hit.
  const maxBuildDamage = useCharacterCalculation().damageCeiling ?? 0;
  const targetLevelOffset = useUIStore((s) => s.targetLevelOffset);
  const incarnateActive = useUIStore((s) => s.incarnateActive);
  const dominationActive = useDominationActive();
  const scourgeActive = useScourgeActive();
  const furyLevel = useFuryLevel();
  const containmentActive = useContainmentActive();
  const criticalHitsActive = useCriticalHitsActive();
  const stalkerHidden = useStalkerHidden();
  const stalkerTeamSize = useStalkerTeamSize();
  const stalkerCritActive = useStalkerCritActive();
  const sentinelCritActive = useSentinelCritActive();
  const effectiveHidden = isCasterHidden(build, stalkerHidden);
  const procSettings = useUIStore((s) => s.procSettings);
  const includeProcDamageToggle = useUIStore((s) => s.includeProcDamageInDPS) && procSettings.damage;
  const useArcanaTimeToggle = useUIStore((s) => s.useArcanaTime);
  const damageDisplayMode = useUIStore((s) => s.damageDisplayMode);
  const combatMode = useUIStore((s) => s.combatMode);

  // Unified power lookup across all categories
  const result = lookupPower(powerSet, powerName);
  let power: Power | undefined = result?.power;
  let powersetName = result?.powersetName || '';

  // Archetype inherent fallback (dynamically created, not in static data)
  if (!power && powerSet === 'Inherent') {
    powersetName = 'Inherent';
    const selectedInherent = build.inherents.find((p) => p.internalName === powerName);
    if (selectedInherent) {
      power = selectedInherent;
      if (selectedInherent.inherentCategory === 'archetype') {
        powersetName = `${build.archetype.name} Inherent`;
      }
    }
  }

  // Find the selected power from build to get its slots
  const selectedPower = findSelectedPowerInBuild(powerName, powerSet, build);

  // The engine's resolved non-DPS values for this power — execution three-tiers, ArcanaTime,
  // perma, and the magnitudes it grants (PROD6C). Covers a power the build has not picked too,
  // which is what the panel shows while you browse the picker.
  const projection = usePowerProjection(powerSet, powerName);

  // The effect rows themselves, taken from that projection rather than resolved here
  // (ENGLAG-1). The engine seeds its display bag from the power's ATOMS, so it resolves the
  // rows of a power whose authored `effects` STRIP-1 removed; resolving locally would render
  // only the keys `buildDisplayEffects` mints for itself. `null` until the dataset loads,
  // which is the one window the local path still covers.
  const effectRows = useMemo(
    () => (projection ? magnitudesFromProjection(projection.grantedMagnitudes) : undefined),
    [projection],
  );

  // Get Alpha incarnate enhancement bonuses (apply to all powers)
  const alphaBonuses = useMemo<EnhancementBonuses>(() => {
    return getAlphaEnhancementBonuses(build.incarnates, incarnateActive);
  }, [build.incarnates, incarnateActive]);

  const exemplarMode = useUIStore((s) => s.exemplarMode);
  const exemplarLevel = useUIStore((s) => s.exemplarLevel);

  // Calculate enhancement bonuses if power is slotted, plus Alpha bonuses.
  // Alpha bonuses interact with ED: a portion (based on tier) bypasses ED,
  // the rest is added to raw IO bonuses before ED is applied.
  const enhancementBonuses = useMemo<EnhancementBonuses>(() => {
    const hasAlpha = Object.values(alphaBonuses).some((v) => v !== undefined && v !== 0);
    const alphaEdBypass = getAlphaEdBypassBonuses(build.incarnates, incarnateActive);

    if (hasAlpha && selectedPower?.slots) {
      // Use combined calculation that properly splits alpha through/around ED
      return combineWithAlphaED(
        // `allowedEnhancements` gates Alpha to the aspects this power accepts — the game's own rule
        // (an Alpha's +Damage does not boost a power that only takes EndRdx and Recharge), which
        // `combineWithAlphaED` falls back to "apply everything" without. Omitting it here leaked
        // Alpha onto every aspect on this surface while the totals path and the engine both filtered
        // (PROD6C-3f).
        { name: selectedPower.name, slots: selectedPower.slots, allowedEnhancements: selectedPower.allowedEnhancements },
        build.level,
        getIOSet,
        alphaBonuses,
        alphaEdBypass,
        exemplarMode ? exemplarLevel : undefined
      );
    }

    // No alpha — standard ED-only calculation
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

  const globalBonusesWithStrength = useMemo(
    () => withStrengthBonuses(globalBonusesForCalc, globalBonuses),
    [globalBonusesForCalc, globalBonuses]
  );


  // When combatMode is active and power has quickSnipe, use Quick-cast stats/damage
  // The power this panel actually shows: the snipe's fast form, the mid-combat cast, and the
  // active mode-gated contributions merged in (drowning bonus, Disintegrating, Bio Armor
  // adaptation). Single-sourced as `resolveEffectivePower` so the picker tooltip and the engine
  // gate resolve the same power, and mirrored in the engine's `effective.rs` (PROD6C-3k).
  const mechanicAdjusters = useUIStore((s) => s.mechanicAdjusters);
  const globalAdjusters = useUIStore((s) => s.globalAdjusters);
  const effectivePowerState = useMemo(
    () => ({
      combatMode,
      hidden: effectiveHidden,
      globalAdjusters: effectiveGlobalAdjusters(build, globalAdjusters),
      mechanicAdjusters,
      atInherentState: { dominationActive },
      activeModes: build.activeModes,
      build,
      currentToHit: currentToHitFraction(archetypeId, globalBonuses),
    }),
    [combatMode, effectiveHidden, mechanicAdjusters, globalAdjusters, dominationActive, build, archetypeId, globalBonuses],
  );
  const conditionalMerge = useMemo(() => {
    if (!power) return { power, extraInstances: {} };
    return resolveEffectivePower(power, effectivePowerState);
  }, [power, effectivePowerState]);
  const effectivePower = conditionalMerge.power;
  const isQuickSnipe = !!power && isQuickSnipeActive(power, effectivePowerState);
  const extraInstances = conditionalMerge.extraInstances;

  // Incarnate damage procs (Interface DoTs, Hybrid Assault) fire on outgoing
  // ATTACKS, so they only belong on powers that can slot Damage enhancements
  // (TO/DO/SO/IO damage eligibility). Without this gate they were appended to
  // every clickable power — buffs, toggles, mez with no damage component.
  const powerCanSlotDamage = !!effectivePower?.allowedEnhancements?.includes('Damage');

  // Calculate actual damage using archetype modifiers and level
  const calculatedDamage = useMemo(() => {
    if (!effectivePower?.damage && !effectivePower?.effects?.damage) return null;

    // Determine if this is a primary or secondary powerset
    const isPrimary = powerSet === build.primary.id;
    const isSecondary = powerSet === build.secondary.id;

    // Derive the powerset category for melee vs ranged determination
    const powersetCategory = isPrimary ? 'PRIMARY' : isSecondary ? 'SECONDARY' : undefined;

    const result = calculatePowerDamage(
      effectivePower,
      {
        level: build.level,
        archetypeId: archetypeId as ArchetypeId | undefined,
        primaryName: powersetName,
        primaryCategory: powersetCategory,
      },
      { damage: enhancementBonuses.damage || 0 },
      globalBonusesForCalc.damage,
      0
    );

    // Apply damage type conversion (e.g., Dual Pistols ammo swap)
    if (result) {
      const powersetPowers = isPrimary ? build.primary.powers
        : isSecondary ? build.secondary.powers : [];
      const conversion = getActiveDamageConversion(powersetPowers);
      if (conversion && result.type === conversion.from) {
        result.type = conversion.to;
      }
    }

    return result;
  }, [effectivePower, build.level, archetypeId, powersetName, enhancementBonuses.damage, globalBonusesForCalc.damage, powerSet, build.primary.id, build.secondary.id, build.primary.powers, build.secondary.powers]);

  // Pseudo-pet damage synthesis. Powers like Blizzard, Caltrops, Ice Storm,
  // Bonfire, etc. carry no `damage[]` on the parent; they summon an
  // `isPseudoPet` entity that ticks the actual damage. Players think
  // "Blizzard does X damage", not "Blizzard summons a pseudo-pet that ticks
  // X". Surface the pet's lifetime damage in the standard Damage section by
  // synthesizing a PowerDamageResult and feeding the same DamageBlock as
  // every other attack — so DPS, enhancement tiers, and cap-relative bars
  // match the rest of the UI. Only pseudo-pets unify here; real summons
  // (Voltaic Sentinel, Phantom Army, MM henchmen) keep their separate
  // SummonsBlock — those are entities the player commands.
  const pseudoPetDamage = useMemo(() => {
    if (calculatedDamage) return null;
    const summon = effectivePower?.summon;
    if (!summon) return null;

    const entityList = summon.entities && summon.entities.length > 0
      ? summon.entities.map(e => ({ entityName: e.entity, count: e.count }))
      : summon.entity
        ? [{ entityName: summon.entity, count: summon.entityCount || 1 }]
        : [];
    const resolvedList = summon.resolvedEntities ?? [];
    // Conditional ("ignited") entities — Oil Slick Arrow's burn patch — fold into
    // the damage total only when their per-power toggle is on (off by default;
    // the patch does no damage unless ignited by a fire/energy power).
    for (const ce of summon.conditionalEntities ?? []) {
      if (mechanicAdjusters[`${effectivePower!.internalName}:${ce.toggleId}`]) {
        entityList.push({ entityName: ce.entity, count: 1 });
      }
    }
    if (entityList.length === 0 && resolvedList.length === 0) return null;

    // Skip commanded pets (MM henchmen, Lore incarnates, Phantom Army, etc.)
    // — the SummonsBlock is the better UI for those: they have multiple
    // abilities, long lifetimes, and the player commands them. Synthesizing
    // a per-cast damage number for "you summon a Phantom Army" doesn't
    // match how the player thinks about the power.
    for (const { entityName } of entityList) {
      const entity = getPetEntity(entityName);
      if (entity?.commandable) return null;
    }

    const enhBonus = enhancementBonuses.damage || 0;
    // globalBonusesForCalc.damage is already a decimal (convertGlobalBonusesToAspects
    // divides by 100). Passing it as-is to calculatePetDamage; previously this
    // line did `/ 100` again, which silently nuked Fury / Musculature / Build
    // Up contributions on every pseudo-pet display.
    const globalBonus = globalBonusesForCalc.damage || 0;

    // Collect PetDamageResults from both real PET_ENTITIES pets and synthesized
    // location pseudo-pets (resolvedEntities). Each carries its own count and
    // lifespan so the fires-per-spawn accounting is per-entity (Category Five's
    // 20s storm and 17s lightning Eye differ).
    const results: { result: PetDamageResult; count: number; duration: number }[] = [];
    const takenNames = takenPowerNames(build);
    for (const { entityName, count } of entityList) {
      const applyEnh = shouldApplyEnhancements(entityName, summon.copyBoosts);
      const result = calculatePetDamage(
        entityName, build.level, count, summon.duration,
        applyEnh ? enhBonus : 0, applyEnh, globalBonus,
        resolveActiveUpgradeTiers(getPetEntity(entityName), takenNames) ?? [],
      );
      if (result) results.push({ result, count, duration: summon.duration ?? 0 });
    }
    for (const resolved of resolvedList) {
      const applyEnh = resolved.copyCreatorMods;
      const result = calculateResolvedPseudoPetDamage(
        resolved, archetypeId ?? '', build.level,
        applyEnh ? enhBonus : 0, applyEnh, globalBonus, stormCellActive,
      );
      if (result) results.push({
        result, count: resolved.count ?? 1, duration: resolved.duration ?? summon.duration ?? 0,
      });
    }
    if (results.length === 0) return null;

    let base = 0, enhanced = 0, final = 0;
    const damageTypes = new Set<string>();
    for (const { result, count, duration } of results) {
      // Compute total per-cast damage by counting actual fires during the
      // summon window. The previous formula (`aggregateDps × duration`)
      // works for true continuous pseudo-pets (Blizzard, Tar Patch) but
      // collapses for one-shot attack-summons like Lightning Rod and
      // Shield Charge where `activatePeriod` is a 100s marker and the
      // pet despawns after 1-4s — the pet fires exactly once at spawn.
      //
      if (duration <= 0) continue;
      // A "permanent" pseudo-pet (the 99999s sentinel duration — a persistent
      // attack pet like Voltaic Sentinel with no bounded lifespan in the data)
      // has no meaningful per-cast lifetime TOTAL: fires-per-spawn over 99999s
      // would be astronomical. Show its PER-ACTIVATION damage (one tick/bolt)
      // instead — bounded and honest; the "(permanent)" label gives context, and
      // the DMG/DPA/DPS toggle still works off the per-activation value.
      const perActivation = duration >= PERMANENT_PSEUDOPET_DURATION;
      for (const ab of result.abilities) {
        const isAuto = ab.ability.type === 'Auto';
        const period = ab.ability.activatePeriod ?? 0;
        let firesPerSpawn: number;
        if (perActivation) {
          firesPerSpawn = 1;
        } else if (result.oneShot) {
          // A bomb detonates once and is destroyed by its own Self_Destruct
          // (trip mines, time bombs, seeker drones, Photon Seekers). Its
          // attack's recharge is not a repeat cadence — the pet is gone — so
          // the duration/cycleTime formula below is meaningless here: it read a
          // Blaster's Trip Mine as THIRTEEN detonations over the 260s the mine
          // merely sits armed (2894 damage instead of 195). Only the
          // Controller/Corruptor/MM mine escaped, because its shared entity's
          // 1000s attack recharge happened to round the same formula down to 1.
          firesPerSpawn = 1;
        } else if (isAuto && period > 0) {
          // Auto abilities fire immediately on spawn, then every `period`
          // seconds. dotTickCount counts the initial tick at t=0 (and is robust
          // to float32 period noise — see Freeze Ray).
          firesPerSpawn = dotTickCount(duration, period);
        } else if (ab.cycleTime > 0) {
          firesPerSpawn = dotTickCount(duration, ab.cycleTime);
        } else {
          firesPerSpawn = 1;
        }
        base += ab.damagePerHit * firesPerSpawn * count;
        enhanced += ab.damagePerHitEnhanced * firesPerSpawn * count;
        final += ab.damagePerHitFinal * firesPerSpawn * count;
        for (const dmg of ab.damageByType) damageTypes.add(dmg.type);
      }
    }
    if (base <= 0) return null;

    const typeLabel = damageTypes.size === 1
      ? Array.from(damageTypes)[0]
      : damageTypes.size > 1 ? 'Mixed' : 'Damage';

    // Bar retool for pseudo-pet DoT: `scale: 1` makes the cap-relative bar use
    // `base × cap` as its reference (base/scale = base), so it shows ENHANCEMENT
    // HEADROOM — base sits at 1/cap and grows toward the cap with enhancements/
    // buffs. (The default `(base/scale) × cap` reference assumes a per-activation
    // scale and pins a multi-tick lifetime total to max regardless of slotting.)
    return {
      base,
      enhanced,
      final,
      type: typeLabel,
      scale: 1,
      // The damage-strength cap bound at least one contributing entity, so the
      // Final tier gets the same amber cap mark a directly-cast power gets.
      capped: results.some(({ result }) => result.capped),
    } as PowerDamageResult;
  }, [calculatedDamage, effectivePower, build, enhancementBonuses.damage, globalBonusesForCalc.damage, archetypeId, stormCellActive, mechanicAdjusters]);

  // Resolved damage shown in the Damage block — direct first, pseudo-pet fallback.
  const resolvedDamage = calculatedDamage ?? pseudoPetDamage;

  // The Scrapper crit is rank-forked rows in the power's OWN data (its own scale, table
  // and probability — One Thousand Cuts crits for 2.21 against a 2.361 base; Sweeping
  // Strike rolls 15%, not 10%), so the column reads those rows resolved by the engine
  // against a target of the rank the column has always claimed ("vs Lt+"). The engine run
  // is a full totals pass, so it is gated to the one case that reads it.
  const wantEngineCrit = archetypeId === 'scrapper' && isScrapperAttackPower(powerSet) && criticalHitsActive;
  const damageVsHigher = usePowerDamageVsRank(wantEngineCrit ? powerSet : undefined, powerName, VS_HIGHER_RANK_SEGMENT);
  const damageVsMinion = usePowerDamageVsRank(wantEngineCrit ? powerSet : undefined, powerName, VS_MINION_RANK_SEGMENT);

  // Archetype hit-time damage mechanic (Containment, Scourge, crits, …).
  // ONLY hit-time mechanics that sit OUTSIDE the damage cap belong here.
  // Additive damage-strength buffs (Brute Fury, Defender Vigilance) are already
  // folded into globalBonuses.damage by calculateCharacterTotals (step 9.1), so
  // they're in the capped Final column — re-applying them would double-count.
  // Gating is shared with the Attack Chain builder via resolveAtMechanic; the
  // scrapper crit VALUE diverges from it deliberately (rows vs chance-average).
  const inherentInfo = useMemo(() => {
    if (!calculatedDamage) return null;

    const mech = resolveAtMechanic(powerSet, {
      archetypeId: archetypeId ?? undefined,
      containmentActive,
      scourgeActive,
      criticalHitsActive,
      stalkerCritActive,
      sentinelCritActive,
      effectiveHidden,
      stalkerTeamSize,
    }, effectivePower?.fromHideBonus);
    if (!mech) return null;

    // Scrapper crit: ADD the power's own crit rows (what a critical actually deals — the
    // same reading CoD lists), not the flat ×1.10 chance-average resolveAtMechanic still
    // carries for the Attack Chain's DPS model. No rows against this target — or the
    // engine not loaded yet — means no column, never a flat archetype constant.
    if (mech.kind === 'crit') {
      const critFinal = critComponents(damageVsHigher).reduce((sum, c) => sum + c.total.final, 0);
      if (critFinal <= 0) return null;
      return {
        header: 'w/ Crit',
        color: 'text-sk-magenta',
        applyBonus: (damage: number) => damage + critFinal,
        showContainment: false,
      };
    }

    const header = mech.kind === 'scourge' ? 'w/ Scourge'
      : mech.kind === 'assassination'
        ? (effectiveHidden ? (effectivePower?.fromHideBonus != null ? 'w/ Assassinate' : 'w/ Crit') : 'w/ Assassin')
      : mech.kind === 'containment' ? 'w/ Contain'
      : 'w/ Crit'; // opportunity

    return {
      header,
      color: 'text-sk-magenta',
      applyBonus: (damage: number, exempt = 0) => applyAtMechanicBonus(damage, mech.multiplier, exempt),
      showContainment: mech.kind === 'containment',
    };
  }, [calculatedDamage, archetypeId, powerSet, containmentActive, scourgeActive,
      criticalHitsActive, effectiveHidden, stalkerTeamSize, stalkerCritActive, sentinelCritActive,
      effectivePower?.fromHideBonus, damageVsHigher]);

  // Proc-only damage contributions for non-damaging powers. Powers like
  // Infrigidate, Siphon Speed, etc. carry no base damage but commonly host
  // damage procs (Annihilation, Achilles, Bombardment, etc.). When that's
  // the case we synthesize a Damage section so users can see what their
  // procs are actually doing on the cycle.
  const procDamageEntries = useMemo(() => {
    if (calculatedDamage) return null; // base damage already shown
    if (!selectedPower?.slots) return null;
    // ProcAllowed kNone (Fold Space, Whitecap, the pet summons): no PPM proc
    // rolls against this power, so there is no proc DPS to synthesize.
    if (!powerFiresProcs(effectivePower ?? selectedPower)) return null;
    const baseRecharge = effectivePower?.stats?.recharge ?? effectivePower?.effects?.recharge ?? 0;
    const castTime = effectivePower?.stats?.castTime ?? effectivePower?.effects?.castTime ?? 0;
    // AoE powers whose footprint lives on a summoned pseudo-pet/patch (Burn,
    // rains, Caltrops) carry radius 0 on the parent — resolveProcAreaGeometry
    // pulls the pet's radius so the proc area-factor isn't scored single-target.
    const directRadius = effectivePower?.stats?.radius ?? effectivePower?.effects?.radius ?? 0;
    const directArc = effectivePower?.stats?.arc ?? effectivePower?.effects?.arc;
    const { radius, arcDegrees } = resolveProcAreaGeometry(
      directRadius, arcToDegrees(directArc) || undefined, effectivePower?.summon);
    // Propel & co.: procs are main-target-only despite the power's AoE radius
    // (that radius is a secondary knockback), so score them single-target.
    const { radius: procRadius, arcDegrees: procArc } =
      resolveProcRollGeometry(effectivePower?.procsOnlyOnMainTarget, radius, arcDegrees);
    if (!baseRecharge && !castTime) return null;
    // A rain hands its rolls to the summoned patch: they are scored against the
    // proc's own 10s period, once every 10s the patch lives, so one cast of
    // Sleet is worth two ~18% rolls rather than one at the 90% ceiling.
    const schedule = resolveProcRollSchedule({
      powerType: effectivePower?.powerType,
      baseRecharge,
      castTime,
      patchDuration: resolveProcPatchDuration(directRadius, effectivePower?.summon),
    });

    // Cycle time uses the *enhanced* recharge (slotted + global) and
    // arcana-aware cast time, so proc DPS matches the headline Cycle Time and
    // the DamageBlock "+proc" annotation. Proc *chance* (below) reads both too,
    // but differently: global recharge cancels out of the window unless the power
    // carries slotted recharge, and then it softens that penalty rather than
    // adding one (DATA-GAP-REGISTER PPM-2).
    const enhancedRecharge = calcThreeTier('recharge', baseRecharge, enhancementBonuses, globalBonusesForCalc).final;
    const effectiveCast = useArcanaTimeToggle ? calculateArcanaTime(castTime) : castTime;
    const cycleTime = (enhancedRecharge + effectiveCast) || 1;

    const entries: { name: string; setName: string; type: string; chance: number; avgDamage: number; perActivation: number; dps: number }[] = [];
    for (const slot of selectedPower.slots) {
      if (!slot || slot.type !== 'io-set') continue;
      const ioSlot = slot as IOSetEnhancement;
      if (!ioSlot.isProc) continue;
      const procData = findProcData(ioSlot.name, ioSlot.setName);
      if (!procData || procData.ppm === null) continue;
      // A foe-damage proc is a Damage effect with a value..valueMax range
      // (Build Up's self-buff Damage has a duration and no valueMax — excluded).
      const effect = getProcEffects(procData).find(
        (e) => e.category === 'Damage' && e.value != null && e.valueMax != null,
      );
      if (!effect || effect.value == null || effect.valueMax == null) continue;

      // Proc DAMAGE scales with the CHARACTER's (combat) level, NOT the slotted
      // IO's crafted level — a level-21 and a level-50 proc deal identical damage
      // on a level-50 character ("slot the cheapest proc"). Mirrors DamageBlock's
      // proc-damage helper. interpolateProcDamage clamps to the proc's levelRange.
      const slotLevel = build.level;
      // Proc damage is FLAT in CoH — it is NOT modified by damage IOs
      // slotted in this power, nor by damage-strength buffs (Fury, Build
      // Up, Aim, Musculature). Procs fire at their fixed scale-table
      // value. Only AT-specific multipliers (Containment, Critical Hits,
      // Scourge) can multiply a proc's damage, and those are applied at
      // hit time by the game, not surfaced in the planner's averages.
      const avgDamage = interpolateProcDamage(effect.value, effect.valueMax, procData.levelRange, slotLevel);
      // A slotted Recharge IO lowers the firing window, which lowers PPM chance; the build's
      // global recharge dilutes that loss instead of deepening it. Neither applies on a patch,
      // whose window is the proc's own fixed period — calculateScheduledProcChance drops them.
      const chance = calculateScheduledProcChance(
        procData.ppm,
        schedule,
        procRadius,
        procArc,
        enhancementBonuses.recharge || 0,
        globalBonusesForCalc.recharge || 0,
      );
      const perActivation = chance * avgDamage * schedule.rolls;
      const dps = perActivation / cycleTime;
      entries.push({
        name: ioSlot.name,
        setName: ioSlot.setName,
        type: effect.effectType ?? 'Damage',
        chance,
        avgDamage,
        perActivation,
        dps,
      });
    }
    // Append incarnate damage procs (Hybrid Assault doublehit, Interface
    // DoTs) — they fire on every outgoing attack when their slot is
    // active, regardless of slotted enhancements. Their entries show up
    // alongside the IO-proc rows so the user sees the full proc landscape
    // for the cast.
    if (archetypeId && powerCanSlotDamage) {
      const incProcs = getActiveIncarnateDamageProcs(build.incarnates, {
        hybrid: !!incarnateActive?.hybrid,
        interface: !!incarnateActive?.interface,
      });
      // Mids multiplies incarnate-proc damage by the attack's maxTargets
      // for AoEs (and 1 for STs). Mirroring that so SK's "Energy 240ish"
      // line for Shield Charge matches Mids' "Energy 221ish" instead of
      // the per-target 15. Slotted IO procs keep their single-target
      // convention because Mids does too.
      const maxTargets = effectivePower?.stats?.maxTargets ?? 1;
      const expectedTargets = radius > 0 ? Math.max(1, maxTargets) : 1;
      const incContribs = computeIncarnateProcContributions(
        incProcs,
        archetypeId,
        build.level,
        baseRecharge,
        castTime,
        radius,
        arcDegrees,
        enhancementBonuses.recharge || 0,
        globalBonusesForCalc.recharge || 0,
        expectedTargets,
      );
      for (const c of incContribs) {
        entries.push({
          name: c.name,
          setName: 'Incarnate',
          type: c.damageType,
          chance: c.chance,
          avgDamage: c.damagePerActivation,
          perActivation: c.avgDamage,
          // Recompute DPS on the same enhanced cycle time as the IO-proc rows.
          // computeIncarnateProcContributions divides by base recharge, which
          // would otherwise leave incarnate rows inconsistent with the rest of
          // this table once the power has recharge slotted.
          dps: c.avgDamage / cycleTime,
        });
      }
    }

    return entries.length > 0 ? entries : null;
  }, [calculatedDamage, selectedPower, effectivePower, build.level, build.incarnates, archetypeId, incarnateActive?.hybrid, incarnateActive?.interface, enhancementBonuses.recharge, globalBonusesForCalc.damage, globalBonusesForCalc.recharge, useArcanaTimeToggle, powerCanSlotDamage]);

  // Damage delivered by a power-GRANTED DoT proc (Molten Embrace's Fire DoT,
  // Stalker Hidden Flame, Envenomed Blades, …). The damage lives in a hidden
  // Temporary_Powers power resolved at convert time (`grantedDamageProcs`); it
  // procs off the player's OWN attacks, so it isn't folded into this power's
  // own DPS — surfaced as its own informational block. `enhanceable` procs
  // (the I28P3 Molten Embrace change) scale with global damage strength + any
  // damage slotted in this power; non-enhanceable ones (IgnoreStrength) show
  // flat.
  const grantedProcDamage = useMemo(() => {
    const procs = effectivePower?.grantedDamageProcs;
    if (!procs || procs.length === 0 || !archetypeId) return null;
    // Enhanceable procs scale with this power's slotted damage + global damage
    // strength; non-enhanceable (IgnoreStrength) ones show flat. Use the damage
    // calc's AT-table helper (handles the proc's `Melee_PvPDamage`-named table,
    // which `calculateEffectValue` does not resolve for damage).
    const enhBonus = (enhancementBonuses.damage || 0);
    const dmgBuffs = (globalBonusesForCalc.damage || 0);
    const rows = procs.map((proc) => {
      const ticks = proc.period && proc.duration ? dotTickCount(proc.duration, proc.period) : 1;
      const comps = proc.damage.map((d) => {
        const perTick = calculateDamageWithATTable(
          d.scale, d.table, archetypeId, build.level,
          proc.enhanceable ? enhBonus : 0,
          proc.enhanceable ? dmgBuffs : 0,
        ) ?? 0;
        return { type: d.damageType, perTick };
      });
      const perTick = comps.reduce((s, c) => s + c.perTick, 0);
      return {
        name: proc.displayName,
        comps,
        ticks,
        perTick,
        total: perTick * ticks,
        chance: proc.tickChance,
        duration: proc.duration,
        enhanceable: proc.enhanceable,
      };
    });
    return rows.length > 0 ? rows : null;
  }, [effectivePower, archetypeId, build.level, enhancementBonuses.damage, globalBonusesForCalc.damage]);

  // Average incarnate-proc damage per cast for THIS attack. Fed to
  // DamageBlock so its "+X proc" annotation includes Hybrid/Interface
  // proc contributions in addition to slotted IO procs. Independent of
  // procDamageEntries (which gates on calculatedDamage being null);
  // direct-damage attacks need this number too.
  const incarnateProcDamage = useMemo(() => {
    if (!archetypeId || !effectivePower || !powerCanSlotDamage) return 0;
    const baseRecharge = effectivePower.stats?.recharge ?? effectivePower.effects?.recharge ?? 0;
    const castTime = effectivePower.stats?.castTime ?? effectivePower.effects?.castTime ?? 0;
    const directRadius = effectivePower.stats?.radius ?? effectivePower.effects?.radius ?? 0;
    const directArc = effectivePower.stats?.arc ?? effectivePower.effects?.arc;
    const { radius, arcDegrees } = resolveProcAreaGeometry(
      directRadius, arcToDegrees(directArc) || undefined, effectivePower.summon);
    if (!baseRecharge && !castTime) return 0;
    const procs = getActiveIncarnateDamageProcs(build.incarnates, {
      hybrid: !!incarnateActive?.hybrid,
      interface: !!incarnateActive?.interface,
    });
    if (procs.length === 0) return 0;
    const maxTargets = effectivePower.stats?.maxTargets ?? 1;
    const expectedTargets = radius > 0 ? Math.max(1, maxTargets) : 1;
    const contribs = computeIncarnateProcContributions(
      procs,
      archetypeId,
      build.level,
      baseRecharge,
      castTime,
      radius,
      arcDegrees,
      enhancementBonuses.recharge || 0,
      expectedTargets,
    );
    return contribs.reduce((sum, c) => sum + c.avgDamage, 0);
  }, [effectivePower, archetypeId, build.level, build.incarnates, incarnateActive?.hybrid, incarnateActive?.interface, enhancementBonuses.recharge, powerCanSlotDamage]);

  // Per-target buff scaling (stored in UI store so it persists across power switches)
  const stackingInfo = useMemo(() => power ? getStackingInfo(power) : null, [power]);
  // An untouched slider reads as its axis' FLOOR, not as zero: on a power whose caster fills one
  // of its own target seats there is no zero-target state to default to (PERFOE-3). The engine
  // floors the count the same way from the same two fields, so the control and the number agree.
  const storedTargetsHit = useUIStore((s) => s.targetsHitValues[powerName]);
  const targetsHit = storedTargetsHit ?? stackingInfo?.minStacks ?? 0;
  const setTargetsHit = useUIStore((s) => s.setTargetsHit);

  if (!power || !effectivePower) {
    return <div className="text-slate-400 text-sm">Power not found</div>;
  }

  // The bag the registry display resolves — the merged stats, the heal extracted from the
  // damage array, a summon's duration as the buff duration, and the flattened movement
  // object. Shared with PowerInfoTooltip, and mirrored by the engine's `granted.rs`
  // (PROD6C-3a).
  const effects = buildDisplayEffects(effectivePower);

  // Check if power has any enhancements
  const hasEnhancements = selectedPower && selectedPower.slots.some(s => s !== null);

  const getIconPath = (): string => getPowerIconPath(power.icon);

  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-start gap-2">
        <img
          src={getIconPath()}
          alt=""
          className="w-8 h-8 rounded flex-shrink-0"
          onError={(e) => {
            (e.target as HTMLImageElement).src = resolvePath('/img/Unknown.png');
          }}
        />
        <div className="min-w-0">
          <h3 className="text-sm font-bold text-blue-400 leading-tight">
            {power.name}
            {isQuickSnipe && (
              <span className="text-[11px] text-amber-400 ml-1 font-normal">(Quick)</span>
            )}
            {hasEnhancements && (
              <span className="text-[11px] text-green-500 ml-1 font-normal">(enhanced)</span>
            )}
          </h3>
        </div>
      </div>

      {/* Top tag cloud — Power Type / Target Type (+ mez-immunity flags) as
        * chips, followed by the chip-style render of shortHelp (range/area,
        * mez, debuff, buff, damage type) for at-a-glance power identity. */}
      <div className="flex flex-wrap gap-1">
        <PowerMetaTags power={power} />
        {power.shortHelp && <ShortHelpChips shortHelp={power.shortHelp} />}
      </div>

      {/* Mode gating — a "Requires: <mode>" note for powers only usable in a
        * combat state (Momentum, Domination, form attacks), and a "Suspended by
        * <power>" banner when an active mode-setter suspends this power's effects
        * (Granite Armor suspends the other Stone toggles). Annotation for the
        * former (build planners always slot these); the latter mirrors the
        * totals calc, which drops the suspended power's contribution. */}
      {power.modesRequired?.length ? (
        <div className="text-[11px] text-amber-400/90">
          Requires: {power.modesRequired.map(modeLabel).join(', ')}
        </div>
      ) : null}
      {(() => {
        const sup = modeSuppression.get(power.internalName);
        return sup ? (
          <div className="text-[11px] text-orange-300 bg-orange-500/10 border border-orange-500/30 rounded px-2 py-1">
            Suspended by <span className="font-semibold">{sup.by}</span> — effects not applied while active.
          </div>
        ) : null;
      })()}

      {/* Allowed Enhancements — its own wrapping section of chips so nothing
        * gets truncated. */}
      <AllowedEnhancementsBlock power={power} />

      {/* Summon/Pet Info with DPS. Pseudo-pet powers (Blizzard, Caltrops,
        * Ice Storm, Bonfire, etc.) surface their lifetime damage through
        * the unified Damage block above instead — players think of those
        * as the power's own damage, not a separate summon. Real summons
        * (Voltaic Sentinel, Phantom Army, MM henchmen, Lore pets) keep
        * the dedicated panel since the entity is something the player
        * commands. */}
      {effects?.summon && !(effects.summon.isPseudoPet && pseudoPetDamage) && (
        <PetDamageDisplay
          summon={effects.summon}
          level={build.level}
          enhancementDamageBonus={enhancementBonuses.damage || 0}
          globalDamageBonus={globalBonusesForCalc.damage || 0}
          archetypeId={archetypeId ?? undefined}
        />
      )}

      {/* Buff-pet aura toggle — for summonable buff drones (Force Field Generator,
        * Barrier Reef, Triage Beacon) whose AoE buffs the top-of-screen totals
        * couldn't previously show. Opt-in; folds into totals via Step 7.2. Renders
        * nothing when the summon isn't a buff-pet. */}
      {power && <BuffPetAuraToggle power={power} />}

      {/* Stacking slider (per-target or per-stack) */}
      {stackingInfo && (() => {
        // Disable slider when Toggle/Auto power is toggled off
        const isToggleOff = selectedPower &&
          (selectedPower.powerType === 'Toggle' || selectedPower.powerType === 'Auto') &&
          selectedPower.isActive === false;
        // Toggled off, the power is not running and even a caster-counted seat is empty.
        const effectiveTargets = isToggleOff ? 0 : Math.max(targetsHit, stackingInfo.minStacks);

        return (
          <div className={`flex items-center gap-2 bg-slate-800/50 rounded px-2 py-1.5 ${isToggleOff ? 'opacity-50' : ''}`}>
            <span className="text-xs text-slate-300 whitespace-nowrap">{stackingInfo.label}</span>
            <input
              type="range"
              min={stackingInfo.minStacks}
              max={stackingInfo.maxStacks}
              value={effectiveTargets}
              onChange={(e) => setTargetsHit(powerName, Number(e.target.value))}
              disabled={!!isToggleOff}
              className="flex-1 h-1 accent-blue-500 cursor-pointer disabled:opacity-50"
            />
            <span className="text-xs text-slate-200 font-mono w-10 text-right">
              {effectiveTargets === 0 ? 'Off' : `${effectiveTargets} / ${stackingInfo.maxStacks}`}
            </span>
          </div>
        );
      })()}

      {/* Mechanic Adjusters — toggles for power.conditionalEffects (drowning,
        * Disintegrating, Bio Armor adaptation, etc.). Toggle state is
        * persisted in uiStore; calc-pipeline integration is a follow-up. */}
      <MechanicAdjusters power={power} />

      {/* Slotted Procs — per-proc on/off + stack / %HP sliders for variable
        * procs (Might of the Tanker stacks, Reactive Defenses HP-scaling). */}
      <SlottedProcControls power={selectedPower} />

      {/* Registry-driven Power Effects display. Pseudo-pet enhanceable debuffs
          (Glue Arrow's slow, etc.) are merged in — parent effects win on key
          collisions; the pet fills in keys the parent power doesn't carry. */}
      <RegistryEffectsDisplay
        effects={withPseudoPetEffects(effectivePower, withTargetsHit(power, effects, targetsHit))}
        allowedEnhancements={power?.allowedEnhancements || []}
        enhancementBonuses={enhancementBonuses}
        globalBonuses={globalBonusesWithStrength}
        archetypeId={archetypeId ?? undefined}
        level={build.level}
        targetsAffected={power?.targetsAffected}
        rows={effectRows}
        categories={['execution', 'buff', 'debuff', 'control', 'protection', 'movement']}
        executionKeys={['accuracy', 'enduranceCost', 'recharge', 'healing']}
        dominationActive={dominationActive}
        header="Power Effects"
        duration={effects?.buffDuration}
        specialEffects={power.specialEffects}
        extraInstances={extraInstances}
        purplePatchInfo={{
          factor: Math.min(0.95, Math.max(0.05, getBaseToHit(targetLevelOffset - globalBonuses.levelShift) + globalBonuses.toHit / 100)) / 0.75,
          offset: targetLevelOffset,
          toHitBonus: globalBonuses.toHit,
          combatModifier: globalBonuses.combatModifier ?? 1,
        }}
      />

      {/* Damage with three-tier display - using actual damage calculation */}
      {/* Damage from slotted procs (only shown for non-damaging powers — base
       * damage already has its own section below). Useful for utility powers
       * like Infrigidate / Siphon Speed that get most/all of their damage
       * from procs the player slots. */}
      {procDamageEntries && procDamageEntries.length > 0 && (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Damage from Procs <span className="text-slate-500 font-normal">(Lvl {build.level})</span>
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-1">
            <div className="grid grid-cols-[1fr_auto_auto_auto_auto] gap-2 text-[11px] text-slate-400 uppercase border-b border-slate-700 pb-0.5">
              <span>Proc</span>
              <span className="text-right">Type</span>
              <span className="text-right">Chance</span>
              <span className="text-right">Avg/Cast</span>
              <span className="text-right">DPS</span>
            </div>
            {procDamageEntries.map((p, i) => (
              <div key={i} className="grid grid-cols-[1fr_auto_auto_auto_auto] gap-2 text-xs">
                <span className="text-slate-300 truncate" title={`${p.setName}: ${p.name}`}>{p.name}</span>
                <span className="text-right text-orange-400">{p.type}</span>
                <span className="text-right text-slate-300 tabular-nums">{(p.chance * 100).toFixed(0)}%</span>
                <span className="text-right text-amber-300 tabular-nums">{p.perActivation.toFixed(1)}</span>
                <span className="text-right text-amber-400 tabular-nums">{p.dps.toFixed(2)}</span>
              </div>
            ))}
            <div className="border-t border-slate-700 pt-0.5 flex justify-between text-[11px]">
              <span className="text-slate-400 uppercase">Total</span>
              <span className="text-amber-400 tabular-nums">
                {procDamageEntries.reduce((s, p) => s + p.perActivation, 0).toFixed(1)} avg / {' '}
                {procDamageEntries.reduce((s, p) => s + p.dps, 0).toFixed(2)} DPS
              </span>
            </div>
          </div>
        </div>
      )}

      {/* Granted DoT procs — damage this power delivers via a hidden
       * Temporary_Powers grant (Molten Embrace's Fire DoT, Hidden Flame, …).
       * Procs off the player's own attacks, so it's shown informationally
       * rather than summed into this power's DPS. */}
      {grantedProcDamage && grantedProcDamage.length > 0 && (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Granted Damage Proc <span className="text-slate-500 font-normal">(Lvl {build.level})</span>
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-1">
            {grantedProcDamage.map((g, i) => (
              <div key={i} className="text-xs space-y-0.5">
                <div className="flex justify-between items-baseline">
                  <span className="text-slate-300 truncate" title={g.name}>{g.name}</span>
                  <span className="text-amber-300 tabular-nums">
                    {g.comps.map((c) => `${abbreviateDamageType(c.type)} ${c.perTick.toFixed(2)}`).join(' + ')}
                    {g.ticks > 1 ? <span className="text-slate-400">{` ×${g.ticks} = ${g.total.toFixed(1)}`}</span> : null}
                  </span>
                </div>
                <div className="text-[11px] text-slate-500">
                  {g.duration ? `over ${g.duration}s` : 'instant'}
                  {g.chance != null ? ` · ${(g.chance * 100).toFixed(0)}% per tick` : ''}
                  {' · '}
                  <span className={g.enhanceable ? 'text-emerald-400' : 'text-slate-500'}>
                    {g.enhanceable ? 'enhanced by damage' : 'unaffected by enhancements'}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {resolvedDamage && (
        <DamageBlock
          calculatedDamage={resolvedDamage}
          effects={{
            recharge: effects?.recharge,
            castTime: effects?.castTime,
            enduranceCost: effects?.enduranceCost,
            radius: effects?.radius,
            arc: effects?.arc,
            summon: effectivePower.summon,
          }}
          archetypeId={archetypeId ?? undefined}
          buildLevel={build.level}
          inherentInfo={inherentInfo}
          globalCombatModifier={globalBonuses.combatModifier ?? 1}
          targetLevelOffset={targetLevelOffset}
          selectedPower={selectedPower}
          damageDisplayMode={damageDisplayMode}
          arcanaTimeEnabled={useArcanaTimeToggle}
          includeProcDamage={includeProcDamageToggle}
          enhancementBonuses={enhancementBonuses}
          globalBonusesForCalc={globalBonusesForCalc}
          incarnateProcDamage={incarnateProcDamage}
          maxBuildDamage={maxBuildDamage}
        />
      )}

      {/* Inherent AT damage bonus info panels */}
      {inherentInfo && calculatedDamage && (
        <>
          {/* Scourge (Corruptor) */}
          {archetypeId === 'corruptor' && isCorruptorAttackPower(powerSet) && (() => {
            const scourgeInfo = getScourgeInfo();
            return (
              <div className={`text-xs rounded px-2 py-1.5 border ${
                scourgeActive
                  ? 'bg-sk-magenta/10 border-sk-magenta/30'
                  : 'bg-slate-800/50 border-slate-700/30'
              }`}>
                <div className="flex items-center justify-between">
                  <span className={`font-medium ${scourgeActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                    Scourge
                  </span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                    scourgeActive ? 'bg-sk-magenta/20 text-sk-magenta' : 'bg-slate-700 text-slate-300'
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

          {/* Fury (Brute) */}
          {archetypeId === 'brute' && isBruteAttackPower(powerSet) && (() => {
            const furyInfo = getFuryInfo();
            const currentBonus = calculateFuryDamageBonus(furyLevel);
            return (
              <div className={`text-xs rounded px-2 py-1.5 border ${
                furyLevel > 0
                  ? 'bg-sk-magenta/10 border-sk-magenta/30'
                  : 'bg-slate-800/50 border-slate-700/30'
              }`}>
                <div className="flex items-center justify-between">
                  <span className={`font-medium ${furyLevel > 0 ? 'text-sk-magenta' : 'text-slate-300'}`}>
                    Fury
                  </span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                    furyLevel > 0 ? 'bg-sk-magenta/20 text-sk-magenta' : 'bg-slate-700 text-slate-300'
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

          {/* Critical Hits (Scrapper) — the power's OWN crit rows per rank branch: the
              export states the chance and the crit's own damage, and they vary by power
              (Sweeping Strike rolls 15%; One Thousand Cuts' crit is smaller than its
              base), so no archetype-constant table can stand in for them. */}
          {archetypeId === 'scrapper' && isScrapperAttackPower(powerSet) && (() => {
            const minion = critBranchSummary(damageVsMinion);
            const higher = critBranchSummary(damageVsHigher);
            return (
              <div className={`text-xs rounded px-2 py-1.5 border ${
                criticalHitsActive
                  ? 'bg-sk-magenta/10 border-sk-magenta/30'
                  : 'bg-slate-800/50 border-slate-700/30'
              }`}>
                <div className="flex items-center justify-between">
                  <span className={`font-medium ${criticalHitsActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                    Critical Hits
                  </span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                    criticalHitsActive ? 'bg-sk-magenta/20 text-sk-magenta' : 'bg-slate-700 text-slate-300'
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

          {/* Assassination (Stalker) */}
          {archetypeId === 'stalker' && isStalkerAttackPower(powerSet) && (() => {
            const currentBonus = calculateAssassinationDamageBonus(effectiveHidden, stalkerTeamSize, effectivePower?.fromHideBonus);
            return (
              <div className={`text-xs rounded px-2 py-1.5 border ${
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
                    effectiveHidden ? 'bg-sk-magenta/25 text-sk-magenta' : 'bg-sk-magenta/20 text-sk-magenta'
                  }`}>
                    {effectiveHidden ? 'ALPHA STRIKE' : `${stalkerTeamSize === 0 ? 'Solo' : `+${stalkerTeamSize}`}`}
                  </span>
                </div>
                <div className="mt-1 text-[11px]">
                  {effectiveHidden ? (
                    <span className="text-sk-magenta/70 font-medium">
                      {effectivePower?.fromHideBonus != null
                        ? `Assassination from Hide — +${(currentBonus * 100).toFixed(0)}% damage on opening attack`
                        : '100% critical from Hide — +100% avg damage on opening attack'}
                    </span>
                  ) : (
                    <span className="text-sk-magenta/70">+{(currentBonus * 100).toFixed(0)}% avg from Assassination</span>
                  )}
                </div>
              </div>
            );
          })()}

          {/* Containment (Controller) */}
          {archetypeId === 'controller' && isControllerPower(powerSet) && (() => {
            const containmentInfo = getContainmentInfo();
            return (
              <div className={`text-xs rounded px-2 py-1.5 border ${
                containmentActive
                  ? 'bg-sk-magenta/15 border-sk-magenta/40'
                  : 'bg-slate-800/50 border-slate-700/30'
              }`}>
                <div className="flex items-center justify-between">
                  <span className={`font-medium ${containmentActive ? 'text-sk-magenta' : 'text-slate-300'}`}>
                    Containment
                  </span>
                  <span className={`text-[10px] px-1.5 py-0.5 rounded ${
                    containmentActive ? 'bg-sk-magenta/20 text-sk-magenta' : 'bg-slate-700/50 text-slate-300'
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
        </>
      )}

      {/* Perma Tracker. The numbers come from the engine's projection for this power
        * (PROD6C) — the same pass the totals ran, rather than a second TS calc. */}
      {isPermaEligible(power, getRechargeBounds(archetypeId)) && (() => {
        const permaTracked = useUIStore.getState().permaTrackedPowers.includes(power.internalName);
        const togglePerma = useUIStore.getState().togglePermaTracked;
        const permaInfo = projection?.perma ?? null;

        return (
          <div className="border-t border-slate-700 pt-2 mt-2">
            <div className="flex items-center justify-between">
              <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide">
                Perma Tracker
              </h4>
              <button
                onClick={() => togglePerma(power.internalName)}
                className={`text-[11px] px-2 py-0.5 rounded border transition-colors ${
                  permaTracked
                    ? 'bg-sk-magenta/20 border-sk-magenta/50 text-sk-magenta'
                    : 'bg-slate-800 border-slate-600 text-slate-300 hover:border-slate-500'
                }`}
              >
                {permaTracked ? 'Tracking' : 'Track'}
              </button>
            </div>
            {permaInfo && (
              <div className="mt-1.5 bg-slate-800/50 rounded p-2 space-y-1">
                {/* Progress bar */}
                <div className="flex items-center gap-2">
                  <div className="flex-1 h-1.5 bg-slate-700 rounded-full overflow-hidden">
                    <div
                      className="h-full rounded-full transition-all bg-sk-magenta"
                      style={{ width: `${permaInfo.permaPercent}%` }}
                    />
                  </div>
                  <span className="text-xs font-mono font-semibold min-w-[3rem] text-right text-sk-magenta">
                    {permaInfo.isPerma ? 'PERMA' : `${permaInfo.permaPercent.toFixed(1)}%`}
                  </span>
                </div>
                {/* Stats */}
                <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 text-xs">
                  <div className="flex justify-between">
                    <span className="text-slate-400">+Recharge</span>
                    <span className={permaInfo.totalRecharge > 0 ? 'text-green-400' : 'text-slate-300'}>
                      {(permaInfo.totalRecharge * 100).toFixed(0)}% / {(permaInfo.rechargeNeeded * 100).toFixed(0)}%
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">Eff. Rchg</span>
                    <span className={permaInfo.effectiveRecharge < permaInfo.baseRecharge ? 'text-green-400' : 'text-slate-300'}>
                      {permaInfo.effectiveRecharge.toFixed(1)}s
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">Base / Duration</span>
                    <span className="text-slate-300">{permaInfo.baseRecharge}s / {permaInfo.duration}s</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-slate-400">Gap</span>
                    <span className={permaInfo.isPerma ? 'text-green-400' : 'text-red-400'}>
                      {permaInfo.isPerma ? 'None' : `${(permaInfo.effectiveRecharge - permaInfo.duration).toFixed(1)}s`}
                    </span>
                  </div>
                  {/* What an early re-fire buys: nothing extra (the running copy restarts
                    * its clock) or a second stacked copy for the overlap. Engine verdict
                    * when the artifact carries it; the TS twin answers for older artifacts.
                    * No row rather than a guess when the atoms don't state a single answer. */}
                  {(() => {
                    const recast = permaInfo.recast ?? recastVerdict(power, permaInfo.duration);
                    if (!recast) return null;
                    return (
                      <div className="flex justify-between col-span-2">
                        <span className="text-slate-400">Recast</span>
                        <span className="text-slate-300">
                          {recast === 'refreshes'
                            ? 'Refreshes (recasting early never overlaps)'
                            : 'Stacks (recasting early overlaps copies)'}
                        </span>
                      </div>
                    );
                  })()}
                </div>
              </div>
            )}
          </div>
        );
      })()}

      {/* General Stats Block — non-enhanceable execution metadata
        * (Activation / Pwr Range / Effect Area / Attack Type). Accuracy /
        * End Cost / Recharge are scoped into RegistryEffectsDisplay above
        * via executionKeys so they get the three-column layout. */}
      <GeneralStatsBlock
        // The EFFECTIVE power, like every other block here and like the `effects` this same
        // call passes. Reading the base record left this block describing a form the rest of
        // the panel wasn't showing: with Power Boost live, Stun's redirect moved Rech Time to
        // 90s while Effect Area still read Single Target off the unredirected record.
        power={effectivePower ?? power}
        effects={effects}
        enhancementBonuses={enhancementBonuses}
        globalRechargeBonus={globalBonusesForCalc.recharge || 0}
        projection={projection}
        damageType={calculatedDamage?.type}
        useArcanaTime={useArcanaTimeToggle}
        selectedPower={selectedPower}
      />

      {/* Enhancement Bonuses Summary */}
      {hasEnhancements && Object.keys(enhancementBonuses).length > 0 && (() => {
        const hasAlpha = Object.values(alphaBonuses).some((v) => v !== undefined && v !== 0);
        return (
          <div className="border-t border-slate-700 pt-2 mt-2">
            <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
              Enhancement Bonuses (after ED)
            </h4>
            <div className="grid grid-cols-2 gap-x-4 gap-y-0.5 text-sm">
              {Object.entries(enhancementBonuses).map(([aspect, value]) => {
                const total = (value || 0) * 100;
                const alphaVal = (alphaBonuses[aspect] || 0) * 100;
                const hasAlphaForAspect = alphaVal > 0;
                const ioOnly = total - alphaVal;
                return (
                  <div key={aspect} className="flex justify-between">
                    <span className="text-slate-300 capitalize">{aspect}</span>
                    <span className="text-green-400">
                      +{total.toFixed(2)}%
                      {hasAlpha && hasAlphaForAspect && (
                        <span className="text-slate-400 text-xs"> ({ioOnly.toFixed(0)}%)</span>
                      )}
                    </span>
                  </div>
                );
              })}
            </div>
            {hasAlpha && (
              <p className="text-[11px] text-slate-500 mt-1">Includes Alpha incarnate</p>
            )}
          </div>
        );
      })()}

      {/* Description (at bottom - least important info) */}
      <p
        className="text-sm text-slate-300 leading-relaxed"
        dangerouslySetInnerHTML={{
          __html: power.description
            .replace(/<br\s*\/?>/gi, ' ')
            .replace(/<[^>]+>/g, '')
            .replace(/NOTE:\s*(.*?)(?:\.|$)/g, '<span class="block mt-1 text-amber-400 font-semibold">NOTE: $1.</span>')
        }}
      />
    </div>
  );
}

interface EnhancementInfoProps {
  enhancementId: string;
}

function EnhancementInfo({ enhancementId }: EnhancementInfoProps) {
  // TODO: Implement enhancement info display
  return (
    <div className="text-slate-400 text-sm">
      Enhancement: {enhancementId}
    </div>
  );
}

// ============================================
// INCARNATE INFO
// ============================================

interface IncarnateInfoProps {
  slotId: IncarnateSlotId;
  powerId: string;
}

/** Render a Genesis below-45 exemplar power's details, by kind. */
function renderGenesisExemplar(ex: GenesisExemplarEffect, archetypeId: string, level: number) {
  const row = (label: string, value: string, color = 'text-slate-300') => (
    <div className="flex justify-between text-sm" key={label}>
      <span className="text-slate-300">{label}</span>
      <span className={color}>{value}</span>
    </div>
  );
  const pct = (v: number) => `+${(v * 100).toFixed(0)}%`;
  switch (ex.kind) {
    case 'attack': {
      const dmg = calculateIncarnateDamage(ex.damageScale, ex.tableName, archetypeId, level);
      return (
        <>
          {row(`Damage (${ex.damageType})`, dmg !== null ? dmg.toFixed(1) : `${ex.damageScale} scale`, 'text-red-400')}
          {row('Effect Area', ex.effectArea, 'text-cyan-400')}
          {ex.radius > 0 && row('Radius', `${ex.radius} ft`)}
          {ex.maxTargets > 0 && row('Max Targets', `${ex.maxTargets}`)}
          {row('Recharge', `${ex.recharge}s`)}
          {ex.secondaryEffects.length > 0 && (
            <div className="flex flex-wrap gap-1 pt-0.5" key="sec">
              {ex.secondaryEffects.map((s, i) => (
                <span key={i} className="text-[10px] px-1.5 py-0.5 bg-slate-700/50 rounded text-amber-400">{s}</span>
              ))}
            </div>
          )}
        </>
      );
    }
    case 'proc': {
      const dot = ex.dotType && ex.dotDamage && ex.dotTableName
        ? calculateIncarnateDamage(ex.dotDamage, ex.dotTableName, archetypeId, level) : null;
      return (
        <>
          {row('Effect', ex.label, 'text-cyan-400')}
          {ex.debuffMagnitude !== undefined && !ex.dotType && row('Magnitude', `${ex.debuffMagnitude}`)}
          {ex.dotType && row(`DoT (${ex.dotType})`, dot !== null ? dot.toFixed(1) : `${ex.dotDamage} scale`, 'text-red-400')}
          {ex.duration > 0 && row('Duration', `${ex.duration}s`)}
          {row('Triggers', `every ${ex.procPeriod}s`)}
        </>
      );
    }
    case 'summon':
      return (
        <>
          {row('Faction', ex.faction, 'text-cyan-400')}
          {row('Pets', ex.pets.join(', '))}
          {row('Duration', `${ex.duration}s (${Math.round(ex.duration / 60)}m)`)}
          {row('Recharge', `${ex.recharge}s (${Math.round(ex.recharge / 60)}m)`)}
        </>
      );
    case 'buff':
      return (
        <>
          {ex.stats.recharge ? row('Recharge', pct(ex.stats.recharge), 'text-cyan-400') : null}
          {ex.stats.recovery ? row('Recovery', pct(ex.stats.recovery), 'text-blue-400') : null}
          {ex.stats.endurance ? row('Endurance', pct(ex.stats.endurance), 'text-blue-400') : null}
          {ex.mezProtection ? row('Mez Protection', `Mag ${ex.mezProtection}`, 'text-purple-400') : null}
          {row('Radius', `${ex.radius} ft`)}
        </>
      );
  }
}

function IncarnateInfo({ slotId, powerId }: IncarnateInfoProps) {
  const build = useBuildStore((s) => s.build);
  const globalBonuses = useGlobalBonuses();
  const archetypeId = build.archetype.id;
  const incarnateActive = useUIStore((s) => s.incarnateActive);
  const exemplarMode = useUIStore((s) => s.exemplarMode);
  const exemplarLevel = useUIStore((s) => s.exemplarLevel);
  // Below 45, incarnate abilities are suppressed (Genesis swaps to its exemplar
  // power). Drives which Genesis block is emphasized.
  const exemplarEffectiveLevel = getEffectiveLevel(build.level, exemplarMode, exemplarLevel);
  const incarnatesSuppressed = areIncarnatesSuppressed(exemplarEffectiveLevel);

  const selectedPower = build.incarnates?.[slotId];
  if (!selectedPower) {
    return (
      <div className="text-slate-400 text-sm">
        No incarnate power selected for this slot.
      </div>
    );
  }

  const slotColor = getSlotColor(slotId);
  const tierColor = getTierColor(selectedPower.tier);
  const tierName = getTierDisplayName(selectedPower.tier);
  const isToggleable = isSlotToggleable(slotId);
  const isActive = isToggleable ? incarnateActive[slotId as ToggleableIncarnateSlot] : false;
  const iconPath = getPowerIconPath(selectedPower.icon);

  // Get the effects based on slot type
  const alphaEffects = slotId === 'alpha' ? getAlphaEffects(powerId) : null;
  // Per-aspect ED-bypass slice authored in the silent grant power (read via
  // getAlphaEdBypass). Feeds the "ED Bypass" column beside each total.
  const alphaEdBypass = slotId === 'alpha' ? getAlphaEdBypass(powerId) : null;
  const destinyEffects = slotId === 'destiny' ? getDestinyEffects(powerId) : null;
  const hybridEffects = slotId === 'hybrid' ? getHybridEffects(powerId) : null;
  const interfaceEffects = slotId === 'interface' ? getInterfaceEffects(powerId) : null;
  const judgementEffects = slotId === 'judgement' ? getJudgementEffects(powerId) : null;
  const loreEffects = slotId === 'lore' ? getLoreEffects(powerId) : null;
  const genesisEffects = slotId === 'genesis' ? getGenesisEffects(powerId) : null;
  const genesisPct = genesisEffects ? `+${(genesisEffects.tierPercent * 100).toFixed(1)}%` : '';

  // A slotted + active Genesis amplifies its partner slot's display. When the
  // user is viewing Judgement/Lore, fold in the matching Verdict/Data boost.
  const slottedGenesis = build.incarnates?.genesis && (incarnateActive?.genesis ?? true)
    ? getGenesisEffects(build.incarnates.genesis.powerId)
    : null;
  const verdictGenesisPct = slottedGenesis?.tree === 'verdict' ? slottedGenesis.tierPercent : 0;
  const dataGenesis = slottedGenesis?.tree === 'data' ? slottedGenesis : null;

  return (
    <div className="space-y-2">
      {/* Header */}
      <div className="flex items-start gap-2">
        <div
          className="w-10 h-10 rounded-md overflow-hidden flex-shrink-0"
          style={{
            boxShadow: `0 0 6px ${tierColor}60`,
            border: `2px solid ${tierColor}`,
          }}
        >
          <img
            src={iconPath}
            alt={selectedPower.displayName}
            className="w-full h-full object-cover"
            onError={(e) => {
              (e.target as HTMLImageElement).src = resolvePath('/img/Unknown.png');
            }}
          />
        </div>
        <div className="min-w-0 flex-1">
          <h3 className="text-sm font-bold leading-tight" style={{ color: slotColor }}>
            {selectedPower.displayName}
          </h3>
          <div className="flex items-center gap-2 mt-0.5">
            <span className="text-xs text-slate-300 capitalize">{slotId}</span>
            <span className="text-xs" style={{ color: tierColor }}>
              {tierName}
            </span>
          </div>
          {isToggleable && (
            <div className={`text-[11px] mt-0.5 ${isActive ? 'text-green-400' : 'text-gray-500'}`}>
              {isActive ? 'Active - bonuses applied' : 'Inactive - bonuses not applied'}
            </div>
          )}
        </div>
      </div>

      {/* Tree info */}
      <div className="text-xs text-slate-300">
        Tree: <span className="text-slate-300">{selectedPower.treeName}</span>
      </div>

      {/* Alpha Effects (Enhancement bonuses) */}
      {alphaEffects && (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Enhancement Bonuses
          </h4>
          <p className="text-[11px] text-slate-400 mb-1">
            Applies to all powers that accept these enhancement types.
          </p>
          <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
            {/* Header row for Alpha effects */}
            <div className="grid grid-cols-[5rem_1fr_1fr] gap-1 text-[11px] text-slate-400 uppercase mb-0.5 border-b border-slate-700 pb-0.5">
              <span>Aspect</span>
              <span>Total</span>
              <span>ED Bypass</span>
            </div>
            {alphaEffects.damage !== undefined && (
              <AlphaEffectRow label="Damage" value={alphaEffects.damage} edBypass={alphaEdBypass?.damage} colorClass="text-red-400" />
            )}
            {alphaEffects.accuracy !== undefined && (
              <AlphaEffectRow label="Accuracy" value={alphaEffects.accuracy} edBypass={alphaEdBypass?.accuracy} colorClass="text-yellow-400" />
            )}
            {alphaEffects.recharge !== undefined && (
              <AlphaEffectRow label="Recharge" value={alphaEffects.recharge} edBypass={alphaEdBypass?.recharge} colorClass="text-cyan-400" />
            )}
            {alphaEffects.enduranceReduction !== undefined && (
              <AlphaEffectRow label="End Reduc" value={alphaEffects.enduranceReduction} edBypass={alphaEdBypass?.enduranceReduction} colorClass="text-blue-400" />
            )}
            {alphaEffects.heal !== undefined && (
              <AlphaEffectRow label="Heal" value={alphaEffects.heal} edBypass={alphaEdBypass?.heal} colorClass="text-green-400" />
            )}
            {alphaEffects.defense !== undefined && (
              <AlphaEffectRow label="Defense" value={alphaEffects.defense} edBypass={alphaEdBypass?.defense} colorClass="text-purple-400" />
            )}
            {alphaEffects.resistance !== undefined && (
              <AlphaEffectRow label="Resistance" value={alphaEffects.resistance} edBypass={alphaEdBypass?.resistance} colorClass="text-orange-400" />
            )}
            {alphaEffects.range !== undefined && (
              <AlphaEffectRow label="Range" value={alphaEffects.range} edBypass={alphaEdBypass?.range} colorClass="text-slate-300" />
            )}
            {alphaEffects.hold !== undefined && (
              <AlphaEffectRow label="Hold" value={alphaEffects.hold} edBypass={alphaEdBypass?.hold} colorClass="text-purple-400" />
            )}
            {alphaEffects.stun !== undefined && (
              <AlphaEffectRow label="Stun" value={alphaEffects.stun} edBypass={alphaEdBypass?.stun} colorClass="text-purple-400" />
            )}
            {alphaEffects.immobilize !== undefined && (
              <AlphaEffectRow label="Immobilize" value={alphaEffects.immobilize} edBypass={alphaEdBypass?.immobilize} colorClass="text-purple-400" />
            )}
            {alphaEffects.sleep !== undefined && (
              <AlphaEffectRow label="Sleep" value={alphaEffects.sleep} edBypass={alphaEdBypass?.sleep} colorClass="text-purple-400" />
            )}
            {alphaEffects.fear !== undefined && (
              <AlphaEffectRow label="Fear" value={alphaEffects.fear} edBypass={alphaEdBypass?.fear} colorClass="text-purple-400" />
            )}
            {alphaEffects.confuse !== undefined && (
              <AlphaEffectRow label="Confuse" value={alphaEffects.confuse} edBypass={alphaEdBypass?.confuse} colorClass="text-purple-400" />
            )}
            {alphaEffects.slow !== undefined && (
              <AlphaEffectRow label="Slow" value={alphaEffects.slow} edBypass={alphaEdBypass?.slow} colorClass="text-cyan-400" />
            )}
            {alphaEffects.toHitBuff !== undefined && (
              <AlphaEffectRow label="ToHit Buff" value={alphaEffects.toHitBuff} edBypass={alphaEdBypass?.toHitBuff} colorClass="text-yellow-400" />
            )}
            {alphaEffects.toHitDebuff !== undefined && (
              <AlphaEffectRow label="ToHit Debuff" value={alphaEffects.toHitDebuff} edBypass={alphaEdBypass?.toHitDebuff} colorClass="text-yellow-400" />
            )}
            {alphaEffects.defenseDebuff !== undefined && (
              <AlphaEffectRow label="Def Debuff" value={alphaEffects.defenseDebuff} edBypass={alphaEdBypass?.defenseDebuff} colorClass="text-purple-400" />
            )}
            {alphaEffects.taunt !== undefined && (
              <AlphaEffectRow label="Taunt" value={alphaEffects.taunt} edBypass={alphaEdBypass?.taunt} colorClass="text-slate-300" />
            )}
            {alphaEffects.runSpeed !== undefined && (
              <AlphaEffectRow label="Run Speed" value={alphaEffects.runSpeed} edBypass={alphaEdBypass?.runSpeed} colorClass="text-cyan-400" />
            )}
            {alphaEffects.jumpSpeed !== undefined && (
              <AlphaEffectRow label="Jump Speed" value={alphaEffects.jumpSpeed} edBypass={alphaEdBypass?.jumpSpeed} colorClass="text-cyan-400" />
            )}
            {alphaEffects.flySpeed !== undefined && (
              <AlphaEffectRow label="Fly Speed" value={alphaEffects.flySpeed} edBypass={alphaEdBypass?.flySpeed} colorClass="text-cyan-400" />
            )}
            {alphaEffects.absorb !== undefined && (
              <AlphaEffectRow label="Absorb" value={alphaEffects.absorb} edBypass={alphaEdBypass?.absorb} colorClass="text-blue-400" />
            )}
          </div>
          {/* Level Shift and ED Bypass info */}
          <div className="mt-1.5 space-y-0.5">
            {alphaEffects.levelShift !== undefined && alphaEffects.levelShift > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-amber-400">Level Shift</span>
                <span className="text-amber-400">+{alphaEffects.levelShift}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Destiny Effects (Direct stat bonuses) */}
      {destinyEffects && (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Buff Effects <span className="text-slate-500 font-normal">(initial values)</span>
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
            {destinyEffects.defenseAll !== undefined && (
              <IncarnateEffectRow label="Defense (All)" value={destinyEffects.defenseAll} colorClass="text-purple-400" />
            )}
            {destinyEffects.resistanceAll !== undefined && (
              <IncarnateEffectRow label="Resistance (All)" value={destinyEffects.resistanceAll} colorClass="text-orange-400" />
            )}
            {destinyEffects.debuffResistance !== undefined && (
              <IncarnateEffectRow label="Debuff Resistance" value={destinyEffects.debuffResistance} colorClass="text-cyan-400" />
            )}
            {destinyEffects.statusResistance !== undefined && (
              <IncarnateEffectRow label="Status Resistance" value={destinyEffects.statusResistance} colorClass="text-cyan-300" />
            )}
            {destinyEffects.regeneration !== undefined && (
              <IncarnateEffectRow label="Regeneration" value={destinyEffects.regeneration} colorClass="text-green-400" />
            )}
            {destinyEffects.recovery !== undefined && (
              <IncarnateEffectRow label="Recovery" value={destinyEffects.recovery} colorClass="text-blue-400" />
            )}
            {destinyEffects.damage !== undefined && (
              <IncarnateEffectRow label="Damage" value={destinyEffects.damage} colorClass="text-red-400" />
            )}
            {destinyEffects.toHit !== undefined && (
              <IncarnateEffectRow label="ToHit" value={destinyEffects.toHit} colorClass="text-yellow-400" />
            )}
            {destinyEffects.recharge !== undefined && (
              <IncarnateEffectRow label="Recharge" value={destinyEffects.recharge} colorClass="text-cyan-400" />
            )}
            {destinyEffects.healPercent !== undefined && (
              <IncarnateEffectRow label="Heal" value={destinyEffects.healPercent} colorClass="text-green-400" />
            )}
            {destinyEffects.healReceived !== undefined && (
              <IncarnateEffectRow label="Healing Received" value={destinyEffects.healReceived} colorClass="text-green-400" />
            )}
            {/* Rebirth's click heal: raw scale × AT table → HP, shown vs the
                build's (buffed) Max HP. */}
            {destinyEffects.healScale !== undefined && destinyEffects.healScale > 0 && (() => {
              const hp = calculateIncarnateDamage(
                destinyEffects.healScale,
                destinyEffects.healTable ?? 'Ranged_Tempdamage',
                archetypeId ?? '',
                build.level,
              );
              if (!hp) return null;
              const { baseHealth } = getBaselineHealth(archetypeId as ArchetypeId | undefined, build.level);
              const buffedMax = baseHealth * (1 + (globalBonuses.maxHP ?? 0) / 100);
              const pct = buffedMax > 0 ? Math.round((hp / buffedMax) * 100) : 0;
              return (
                <div className="flex justify-between text-sm">
                  <span className="text-green-400">Heal</span>
                  <span className="text-green-400">~{Math.round(hp)} HP{pct > 0 ? ` (${pct}% Max HP)` : ''}</span>
                </div>
              );
            })()}
            {destinyEffects.maxHP !== undefined && (
              <IncarnateEffectRow label="Max HP" value={destinyEffects.maxHP} colorClass="text-red-300" />
            )}
            {destinyEffects.mezProtection !== undefined && (
              <div className="flex justify-between text-sm">
                <span className="text-purple-400">Mez Protection</span>
                <span className="text-purple-400">Mag {destinyEffects.mezProtection}</span>
              </div>
            )}
            {destinyEffects.kbProtection !== undefined && (
              <div className="flex justify-between text-sm">
                <span className="text-purple-400">Knockback Protection</span>
                <span className="text-purple-400">Mag {destinyEffects.kbProtection}</span>
              </div>
            )}
            {destinyEffects.runSpeed !== undefined && (
              <IncarnateEffectRow label="Run/Jump/Fly Speed" value={destinyEffects.runSpeed} colorClass="text-sky-300" />
            )}
            {destinyEffects.levelShift !== undefined && destinyEffects.levelShift > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-amber-400">Level Shift</span>
                <span className="text-amber-400">+{destinyEffects.levelShift}</span>
              </div>
            )}
          </div>
          {destinyEffects.initialDuration !== undefined && (
            <div className="text-[11px] text-slate-400 mt-1">
              Duration: {destinyEffects.initialDuration}s peak / {destinyEffects.totalDuration}s total
            </div>
          )}
        </div>
      )}

      {/* Hybrid Effects */}
      {hybridEffects && (
        <div>
          {/* Passive bonuses (always-on) */}
          {Object.keys(hybridEffects.passive).length > 0 && (
            <>
              <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
                Passive (Always-On)
              </h4>
              <div className="bg-slate-800/50 rounded p-2 space-y-0.5 mb-2">
                {Object.entries(hybridEffects.passive).map(([stat, value]) => (
                  <IncarnateEffectRow key={stat} label={formatHybridStatName(stat)} value={value} colorClass={hybridStatColor(stat)} />
                ))}
              </div>
            </>
          )}
          {/* Front-loaded toggle bonuses */}
          {Object.keys(hybridEffects.frontLoaded).length > 0 && (
            <>
              <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
                Toggle (Baseline)
              </h4>
              <div className="bg-slate-800/50 rounded p-2 space-y-0.5 mb-2">
                {Object.entries(hybridEffects.frontLoaded).map(([stat, value]) => (
                  <IncarnateEffectRow key={stat} label={formatHybridStatName(stat)} value={value} colorClass={hybridStatColor(stat)} />
                ))}
              </div>
            </>
          )}
          {/* Per-target stacking */}
          {Object.keys(hybridEffects.perTarget).length > 0 && (
            <>
              <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
                Per Enemy (max {hybridEffects.maxTargets})
              </h4>
              <div className="bg-slate-800/50 rounded p-2 space-y-0.5 mb-2">
                {Object.entries(hybridEffects.perTarget).map(([stat, value]) => (
                  <IncarnateEffectRow key={stat} label={formatHybridStatName(stat)} value={value} colorClass={hybridStatColor(stat)} />
                ))}
              </div>
            </>
          )}
          <div className="text-[11px] text-slate-400 mt-1">
            Duration: {hybridEffects.duration}s / Recharge: {hybridEffects.recharge}s
          </div>
        </div>
      )}

      {/* Interface Effects (Proc-based) */}
      {interfaceEffects && (() => {
        const dotDmg = interfaceEffects.dotDamage && interfaceEffects.dotTableName
          ? calculateIncarnateDamage(interfaceEffects.dotDamage, interfaceEffects.dotTableName, build.archetype.id ?? '', build.level)
          : null;
        return (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Proc Effects
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
            {interfaceEffects.debuffType && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">Debuff</span>
                <span className="text-orange-400">{interfaceEffects.debuffType}</span>
              </div>
            )}
            {interfaceEffects.debuffMagnitude !== undefined && (
              <IncarnateEffectRow label="Magnitude" value={interfaceEffects.debuffMagnitude} colorClass="text-orange-400" />
            )}
            {interfaceEffects.dotType && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">DoT ({interfaceEffects.dotType})</span>
                <span className="text-red-400">
                  {dotDmg !== null ? `${dotDmg.toFixed(1)}` : `${((interfaceEffects.dotDamage || 0) * 100).toFixed(0)}% scale`}
                </span>
              </div>
            )}
            {interfaceEffects.procChance !== undefined && (
              <IncarnateEffectRow label="Proc Chance" value={interfaceEffects.procChance} colorClass="text-cyan-400" />
            )}
          </div>
        </div>
        ); })()}

      {/* Judgement Effects (Click attack) */}
      {judgementEffects && (() => {
        const baseJudgementDamage = calculateIncarnateDamage(
          judgementEffects.damageScale, judgementEffects.tableName,
          build.archetype.id ?? '', build.level
        );
        // Verdict Genesis amplifies the Judgement attack's damage.
        const judgementDamage = baseJudgementDamage !== null && verdictGenesisPct > 0
          ? baseJudgementDamage * (1 + verdictGenesisPct)
          : baseJudgementDamage;
        return (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Attack Details
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Damage ({judgementEffects.damageType})</span>
              <span className="text-red-400">
                {judgementDamage !== null ? `${judgementDamage.toFixed(1)}` : `${judgementEffects.damageScale} scale`}
                {verdictGenesisPct > 0 && (
                  <span className="text-[10px] text-lime-400 ml-1">(+{(verdictGenesisPct * 100).toFixed(1)}% Genesis)</span>
                )}
              </span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Effect Area</span>
              <span className="text-cyan-400">{judgementEffects.effectArea}</span>
            </div>
            {judgementEffects.range > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">Range</span>
                <span className="text-slate-300">{judgementEffects.range} ft</span>
              </div>
            )}
            {judgementEffects.radius > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">Radius</span>
                <span className="text-slate-300">{judgementEffects.radius} ft</span>
              </div>
            )}
            {judgementEffects.arc > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">Arc</span>
                <span className="text-slate-300">{judgementEffects.arc}&deg;</span>
              </div>
            )}
            {judgementEffects.maxTargets > 0 && (
              <div className="flex justify-between text-sm">
                <span className="text-slate-300">Max Targets</span>
                <span className="text-slate-300">{judgementEffects.maxTargets}</span>
              </div>
            )}
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Activation</span>
              <span className="text-slate-300">{judgementEffects.activationTime}s</span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Recharge</span>
              <span className="text-slate-300">{judgementEffects.rechargeTime}s</span>
            </div>
          </div>
          {judgementEffects.secondaryEffects.length > 0 && (
            <div className="mt-1">
              <div className="text-[11px] text-slate-400 mb-0.5">Secondary Effects:</div>
              <div className="flex flex-wrap gap-1">
                {judgementEffects.secondaryEffects.map((effect, i) => (
                  <span key={i} className="text-[11px] px-1.5 py-0.5 bg-slate-700/50 rounded text-amber-400">
                    {effect}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
        ); })()}

      {/* Lore Effects (Pet summon) */}
      {loreEffects && (
        <div>
          <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
            Summon Details
          </h4>
          <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Faction</span>
              <span className="text-cyan-400">{loreEffects.faction}</span>
            </div>
            <div className="text-sm mt-1">
              <span className="text-slate-300">Pets:</span>
              <div className="flex flex-wrap gap-1 mt-0.5">
                {loreEffects.pets.map((pet, i) => (
                  <span key={i} className="text-[11px] px-1.5 py-0.5 bg-slate-700/50 rounded text-green-400">
                    {pet}
                  </span>
                ))}
              </div>
            </div>
            <div className="flex justify-between text-sm mt-1">
              <span className="text-slate-300">Duration</span>
              <span className="text-slate-300">{loreEffects.duration}s ({Math.round(loreEffects.duration / 60)}m)</span>
            </div>
            <div className="flex justify-between text-sm">
              <span className="text-slate-300">Recharge</span>
              <span className="text-slate-300">{loreEffects.rechargeTime}s ({Math.round(loreEffects.rechargeTime / 60)}m)</span>
            </div>
            {dataGenesis && (
              <div className="mt-1 pt-1 border-t border-slate-700/60 text-[11px] text-lime-400">
                Data Genesis: +{(dataGenesis.tierPercent * 100).toFixed(1)}% pet damage
                {dataGenesis.loreMaxHP !== undefined && `, +${dataGenesis.loreMaxHP} pet Max HP`}
              </div>
            )}
          </div>
        </div>
      )}

      {/* Genesis Effects — 45+ amplifier + below-45 exemplar power. The block
          matching the current effective level is emphasized; the other is dimmed. */}
      {genesisEffects && (
        <div className="space-y-2">
          {/* 45+ amplifier */}
          <div className={incarnatesSuppressed ? 'opacity-50' : ''}>
            <h4 className="text-[11px] font-semibold text-slate-400 uppercase tracking-wide mb-1">
              Enhances {genesisEffects.enhancesSlot.charAt(0).toUpperCase() + genesisEffects.enhancesSlot.slice(1)}
              <span className="text-slate-500 font-normal normal-case"> (45+{incarnatesSuppressed ? ', inactive' : ''})</span>
            </h4>
            <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
              {genesisEffects.tree === 'data' && (
                <>
                  <div className="flex justify-between text-sm">
                    <span className="text-slate-300">Lore Pet Damage</span>
                    <span className="text-red-400">{genesisPct}</span>
                  </div>
                  {genesisEffects.loreMaxHP !== undefined && (
                    <div className="flex justify-between text-sm">
                      <span className="text-slate-300">Lore Pet Max HP</span>
                      <span className="text-green-400">+{genesisEffects.loreMaxHP}</span>
                    </div>
                  )}
                </>
              )}
              {genesisEffects.tree === 'fate' && (
                <div className="flex justify-between text-sm">
                  <span className="text-slate-300">Destiny Effects</span>
                  <span className="text-cyan-400">{genesisPct}</span>
                </div>
              )}
              {genesisEffects.tree === 'socket' && (
                <>
                  <div className="flex justify-between text-sm">
                    <span className="text-slate-300">Interface Proc Chance</span>
                    <span className="text-cyan-400">{genesisPct}</span>
                  </div>
                  <div className="flex justify-between text-sm">
                    <span className="text-slate-300">Your Max HP</span>
                    <span className="text-green-400">{genesisPct}</span>
                  </div>
                  <div className="flex justify-between text-sm">
                    <span className="text-slate-300">Your Max Endurance</span>
                    <span className="text-blue-400">{genesisPct}</span>
                  </div>
                </>
              )}
              {genesisEffects.tree === 'verdict' && (
                <div className="flex justify-between text-sm">
                  <span className="text-slate-300">Judgement Damage</span>
                  <span className="text-red-400">{genesisPct}</span>
                </div>
              )}
            </div>
          </div>

          {/* Below-45 exemplar power */}
          {genesisEffects.exemplarEffect && (
            <div className={incarnatesSuppressed ? '' : 'opacity-60'}>
              <h4 className="text-[11px] font-semibold text-amber-400/90 uppercase tracking-wide mb-1">
                Below Level 45
                <span className="text-slate-500 font-normal normal-case"> (exemplar{incarnatesSuppressed ? ', active' : ''})</span>
              </h4>
              <div className="bg-slate-800/50 rounded p-2 space-y-0.5">
                {renderGenesisExemplar(genesisEffects.exemplarEffect, build.archetype.id ?? '', build.level)}
              </div>
            </div>
          )}

          <p className="text-[11px] text-slate-400 mt-1">
            {incarnatesSuppressed
              ? `Exemplared to ${exemplarEffectiveLevel}: the exemplar power is active; the 45+ amplifier is suppressed.`
              : 'Active at level 45+. Below 45 it grants the exemplar power shown above.'}
          </p>
        </div>
      )}
    </div>
  );
}

/** Convert hybrid stat keys (e.g. 'resSmashing', 'defMelee', 'regeneration') to display names */
function formatHybridStatName(stat: string): string {
  const map: Record<string, string> = {
    damage: 'Damage', regeneration: 'Regeneration', recovery: 'Recovery',
    enduranceDiscount: 'End Discount', statusResistance: 'Status Resistance',
    resSmashing: 'Res Smashing', resLethal: 'Res Lethal', resFire: 'Res Fire',
    resCold: 'Res Cold', resEnergy: 'Res Energy', resNegative: 'Res Negative',
    resPsionic: 'Res Psionic', resToxic: 'Res Toxic',
    defMelee: 'Def Melee', defRanged: 'Def Ranged', defAoE: 'Def AoE',
    defSmashing: 'Def Smashing', defLethal: 'Def Lethal', defFire: 'Def Fire',
    defCold: 'Def Cold', defEnergy: 'Def Energy', defNegative: 'Def Negative',
    defPsionic: 'Def Psionic', defToxic: 'Def Toxic',
    protHold: 'Prot Hold', protStun: 'Prot Stun', protImmobilize: 'Prot Immobilize',
    protSleep: 'Prot Sleep', protConfuse: 'Prot Confuse', protFear: 'Prot Fear',
  };
  return map[stat] || stat;
}

function hybridStatColor(stat: string): string {
  if (stat.startsWith('res')) return 'text-orange-400';
  if (stat.startsWith('def')) return 'text-purple-400';
  if (stat.startsWith('prot')) return 'text-purple-400';
  if (stat === 'regeneration') return 'text-green-400';
  if (stat === 'damage') return 'text-red-400';
  if (stat === 'statusResistance') return 'text-purple-400';
  if (stat === 'enduranceDiscount') return 'text-blue-400';
  return 'text-slate-300';
}

function IncarnateEffectRow({
  label,
  value,
  colorClass,
  suffix = '',
}: {
  label: string;
  value: number;
  colorClass: string;
  suffix?: string;
}) {
  return (
    <div className="flex justify-between text-sm">
      <span className="text-slate-300">{label}</span>
      <span className={colorClass}>{formatEffectValue(value)}{suffix}</span>
    </div>
  );
}

/**
 * Alpha effect row with ED bypass calculation
 * Shows total bonus and the portion that bypasses ED
 */
function AlphaEffectRow({
  label,
  value,
  edBypass,
  colorClass,
}: {
  label: string;
  value: number;
  /** Data-authored portion of `value` that bypasses ED (absent = none bypasses). */
  edBypass?: number;
  colorClass: string;
}) {
  return (
    <div className="grid grid-cols-[5rem_1fr_1fr] gap-1 items-baseline text-sm">
      <span className="text-slate-300">{label}</span>
      <span className={colorClass}>{formatEffectValue(value)}</span>
      <span className="text-green-400">
        {edBypass !== undefined ? formatEffectValue(edBypass) : '—'}
      </span>
    </div>
  );
}

// ============================================
// PET DAMAGE DISPLAY
// ============================================

interface PetDamageDisplayProps {
  summon: SummonEffect;
  level: number;
  enhancementDamageBonus: number;
  globalDamageBonus: number;
  /** Power's archetype id — used to label the aggregate row "Henchmen"
   *  (Mastermind) vs the more neutral "Pet" for everyone else. */
  archetypeId?: string;
}

/** Expandable row for a single pet ability with damage */
function PetAbilityRow({ ad }: { ad: PetAbilityDamage }) {
  const [open, setOpen] = useState(false);
  const ability = ad.ability;
  const abilityHasEnh = ad.damagePerHitEnhanced !== ad.damagePerHit;
  const abilityHasBuff = ad.damagePerHitFinal !== ad.damagePerHitEnhanced;

  return (
    <div>
      <div
        role="button"
        aria-expanded={open}
        title={open ? `Hide details for ${ability.displayName}` : `Show damage breakdown by type for ${ability.displayName}`}
        className="grid grid-cols-[1fr_3rem_3rem_3rem_3rem] gap-0.5 text-[11px] items-baseline cursor-pointer select-none hover:bg-slate-800/40 rounded px-0.5"
        onClick={() => setOpen(!open)}
      >
        <span className="text-slate-300 truncate flex items-center gap-0.5">
          <span className="text-[10px] text-slate-500">{open ? '▼' : '▶'}</span>
          {ability.displayName}
          {ability.type === 'Auto' && <span className="text-slate-400 text-[9px]">auto</span>}
        </span>
        <span className="text-red-400 text-right">{ad.damagePerHit.toFixed(1)}</span>
        <span className={`text-right ${abilityHasEnh ? 'text-green-400' : 'text-slate-500'}`}>
          {abilityHasEnh ? ad.damagePerHitEnhanced.toFixed(1) : '—'}
        </span>
        <span className={`text-right ${abilityHasBuff ? 'text-amber-400' : 'text-slate-500'}`}>
          {abilityHasBuff ? ad.damagePerHitFinal.toFixed(1) : '—'}
        </span>
        <span className="text-slate-300 text-right">{ad.dpsFinal.toFixed(1)}</span>
      </div>
      {open && <PetAbilityDetails ability={ability} cycleTime={ad.cycleTime} damageByType={ad.damageByType} />}
    </div>
  );
}

/** One titled block of the effects list — the pet's own stats, or what it
 *  applies to others. Renders nothing when empty. */
function PetEffectGroup({ title, hint, effects, showTitle = true }: {
  title: string;
  hint: string;
  effects: PetEffectComputed[];
  showTitle?: boolean;
}) {
  if (effects.length === 0) return null;
  return (
    <div>
      {showTitle && (
        <div className="text-[10px] uppercase tracking-wide text-slate-500 mt-0.5" title={hint}>
          {title}
        </div>
      )}
      {/* Two-column flow — each effect is a compact label↔value pair so the
          block packs tightly instead of one tall column with a wide gap. */}
      <div className="grid grid-cols-2 gap-x-4 gap-y-0.5">
        {effects.map((eff, i) => {
          const chanceLabel = eff.chance && eff.chance < 1 ? ` (${(eff.chance * 100).toFixed(0)}%)` : '';
          return (
            <div key={`${eff.type}-${i}`} className="flex items-baseline justify-between gap-1.5 text-[11px]">
              <span className={`${petEffectColor(eff)} truncate`} title={petEffectLabel(eff)}>
                {petEffectLabel(eff)}{chanceLabel}
                {eff.conditional && (
                  <span className="text-amber-500/70 italic ml-1" title="Only while the power is in its empowered/triggered state (e.g. Storm Cell's High Winds)">
                    ⚡
                  </span>
                )}
                {eff.ignoreStrength && (
                  <span className="text-slate-500 italic ml-1" title="Ignores buffs and enhancements">
                    (unenh.)
                  </span>
                )}
              </span>
              <span className="text-slate-300 tabular-nums shrink-0">
                {formatPetEffectValue(eff)}
              </span>
            </div>
          );
        })}
      </div>
    </div>
  );
}

/** Deduped, human-readable list of the effect KINDS an ability carries. */
function abilityEffectSummary(ability: PetAbility): string {
  const seen: string[] = [];
  for (const eff of ability.effects ?? []) {
    const label = petEffectLabel(eff);
    if (!seen.includes(label)) seen.push(label);
  }
  return seen.join(', ');
}

/** Expandable row for a pet ability with effects only (no damage) */
function PetEffectAbilityRow({ ability }: { ability: PetAbility }) {
  const [open, setOpen] = useState(false);

  return (
    <div>
      <div
        role="button"
        aria-expanded={open}
        title={open ? `Hide effects for ${ability.displayName}` : `Show effect details for ${ability.displayName}`}
        className="grid grid-cols-[1fr_3rem_3rem_3rem_3rem] gap-0.5 text-[11px] items-baseline cursor-pointer select-none hover:bg-slate-800/40 rounded px-0.5"
        onClick={() => setOpen(!open)}
      >
        <span className="text-slate-300 truncate flex items-center gap-0.5">
          <span className="text-[10px] text-slate-500">{open ? '▼' : '▶'}</span>
          {ability.displayName}
          {ability.type === 'Auto' && <span className="text-slate-400 text-[9px]">auto</span>}
        </span>
        {/* A summary, not a transcript: the same effect type recurs across an
            ability's templates (a Soldier's Resistance carries two separate mez
            protections), and repeating "SelfMezProtection, SelfMezProtection"
            in a 9rem gutter told the reader nothing. Values live in the
            expanded detail below. */}
        <span className="text-slate-500 text-right col-span-3 truncate" title={abilityEffectSummary(ability)}>
          {abilityEffectSummary(ability)}
        </span>
        <span className="text-slate-500 text-right">—</span>
      </div>
      {open && <PetAbilityDetails ability={ability} />}
    </div>
  );
}

/** Detail panel shown when a pet ability row is expanded */
function PetAbilityDetails({ ability, cycleTime, damageByType }: {
  ability: PetAbility;
  cycleTime?: number;
  damageByType?: { type: string; base: number; enhanced: number; final: number }[];
}) {
  const stats: { label: string; value: string }[] = [];

  if (ability.type !== 'Auto' || !ability.activatePeriod) {
    if (ability.castTime) stats.push({ label: 'Cast', value: `${ability.castTime}s` });
    if (ability.recharge) stats.push({ label: 'Recharge', value: `${ability.recharge}s` });
  }
  if (ability.activatePeriod) stats.push({ label: 'Period', value: `${ability.activatePeriod}s` });
  if (cycleTime) stats.push({ label: 'Cycle', value: `${cycleTime.toFixed(2)}s` });
  if (ability.effectArea && ability.effectArea !== 'Character') {
    stats.push({ label: 'Area', value: formatEffectArea(ability.effectArea) });
  }
  if (ability.range) stats.push({ label: 'Range', value: `${ability.range}ft` });
  if (ability.radius) stats.push({ label: 'Radius', value: `${ability.radius}ft` });
  if (ability.maxTargets && ability.maxTargets > 1) stats.push({ label: 'Max Targets', value: `${ability.maxTargets}` });
  if (ability.attackTypes?.length) stats.push({ label: 'Attack', value: ability.attackTypes.join(', ') });

  return (
    <div className="ml-3 mb-1 mt-0.5 pl-2 border-l border-slate-700/50">
      {/* Stats grid */}
      {stats.length > 0 && (
        <div className="grid grid-cols-2 gap-x-3 gap-y-0 text-[11px]">
          {stats.map((s) => (
            <div key={s.label} className="flex justify-between">
              <span className="text-slate-400">{s.label}</span>
              <span className="text-slate-300">{s.value}</span>
            </div>
          ))}
        </div>
      )}
      {/* Damage type breakdown */}
      {damageByType && damageByType.length > 0 && (
        <div className="mt-0.5">
          {damageByType.map((d) => (
            <div key={d.type} className="flex items-center gap-2 text-[11px]">
              <span className="text-red-400 w-12">{abbreviateDamageType(d.type)}</span>
              <span className="text-slate-300">{d.base.toFixed(1)}</span>
              {d.enhanced !== d.base && <span className="text-green-400">→ {d.enhanced.toFixed(1)}</span>}
              {d.final !== d.enhanced && <span className="text-amber-400">→ {d.final.toFixed(1)}</span>}
            </div>
          ))}
        </div>
      )}
      {/* Effects. One chip per template, each carrying its own sub-types —
          two SelfMezProtections are two different protections (Placate mag 4,
          Confuse mag 2) and are only distinguishable by what they name. */}
      {ability.effects && ability.effects.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-0.5">
          {ability.effects.map((eff, i) => {
            const magLabel = eff.magnitude ? ` ${eff.magnitude}` : '';
            const chanceLabel = eff.chance && eff.chance < 1 ? ` ${(eff.chance * 100).toFixed(0)}%` : '';
            return (
              <span key={`${eff.type}-${i}`} className={`text-[10px] px-1 py-0.5 rounded bg-slate-800/60 ${petEffectColor(eff)}`}>
                {petEffectLabel(eff)}{magLabel}{chanceLabel}
              </span>
            );
          })}
        </div>
      )}
    </div>
  );
}

/** Single entity DPS display (used inside PetDamageDisplay) */
function SingleEntityDisplay({
  petDamage,
  entityCount,
  label,
  showHeader,
  isPseudoPet,
  duration,
  hideDamage = false,
}: {
  petDamage: PetDamageResult | null;
  entityCount: number;
  label: string;
  showHeader: boolean;
  isPseudoPet: boolean;
  duration?: number;
  /** Suppress the DPS rows — used for synthesized location pseudo-pets whose
   *  damage is already unified into the headline Damage block; here we only want
   *  the informational applied-effects list. */
  hideDamage?: boolean;
}) {
  const [expanded, setExpanded] = useState(false);
  // Default-expand the effects list for synthesized location pseudo-pets — those
  // debuffs/mez ARE the power's content (Storm Cell is all -rech/-spd/-tohit).
  const [effectsExpanded, setEffectsExpanded] = useState(hideDamage);

  const durationLabel = formatSummonDuration(duration);
  const countLabel = entityCount > 1 ? ` x${entityCount}` : '';

  const hasDamage = !hideDamage && petDamage && petDamage.aggregateDpsBase > 0;
  const hasEnh = petDamage && petDamage.aggregateDpsEnhanced !== petDamage.aggregateDpsBase;
  const hasBuff = petDamage && petDamage.aggregateDpsFinal !== petDamage.aggregateDpsEnhanced;
  const hasEffects = petDamage && petDamage.allEffects.length > 0;
  const hasContent = hasDamage || hasEffects;
  const petEffects = useMemo(
    () => partitionPetEffects(petDamage?.allEffects ?? []),
    [petDamage],
  );

  return (
    <div>
      {/* Entity header */}
      {showHeader && (
        <div className="flex items-center gap-2">
          <span className="text-indigo-400 text-sm font-medium">
            {isPseudoPet ? '⚡ Creates' : '🐾 Summons'}
          </span>
          <span className="text-slate-200 text-sm">
            {label}{countLabel}
          </span>
          <span className="text-slate-400 text-xs">({durationLabel})</span>
        </div>
      )}

      {/* Pet DPS + Effects */}
      {hasContent && (
        <div className="mt-1">
          {hasDamage && (
            <>
              <div
                role="button"
                aria-expanded={expanded}
                title={expanded ? 'Hide per-ability damage breakdown' : 'Show per-ability damage breakdown'}
                className="flex items-center gap-1 cursor-pointer select-none"
                onClick={() => setExpanded(!expanded)}
              >
                <span className="text-[11px] text-slate-400">{expanded ? '▼' : '▶'}</span>
                <span className="text-xs text-slate-300 font-medium">
                  {entityCount > 1 ? 'Total' : 'Pet'} DPS:
                </span>
                <span className="text-xs text-red-400 font-medium">
                  {petDamage!.aggregateDpsBase.toFixed(1)}
                </span>
                {hasEnh && (
                  <span className="text-xs text-green-400">
                    → {petDamage!.aggregateDpsEnhanced.toFixed(1)}
                  </span>
                )}
                {hasBuff && (
                  <span className="text-xs text-amber-400">
                    → {petDamage!.aggregateDpsFinal.toFixed(1)}
                  </span>
                )}
              </div>

              {entityCount > 1 && (
                <div className="text-[11px] text-slate-400 ml-3">
                  Per pet: {petDamage!.totalDpsBase.toFixed(1)}
                  {hasEnh && ` → ${petDamage!.totalDpsEnhanced.toFixed(1)}`}
                  {hasBuff && ` → ${petDamage!.totalDpsFinal.toFixed(1)}`}
                </div>
              )}
            </>
          )}

          {/* Expanded ability breakdown */}
          {expanded && (
            <div className="mt-1.5 ml-1 space-y-0">
              <div className="grid grid-cols-[1fr_3rem_3rem_3rem_3rem] gap-0.5 text-[10px] text-slate-400 font-medium border-b border-slate-700/50 pb-0.5 mb-0.5">
                <span>Ability</span>
                <span className="text-right">Base</span>
                <span className="text-right">Enh</span>
                <span className="text-right">Final</span>
                <span className="text-right">DPS</span>
              </div>
              {petDamage!.abilities.map((ad) => (
                <PetAbilityRow key={ad.ability.name} ad={ad} />
              ))}
              {petDamage!.effectOnlyAbilities
                // The pet's own always-on stat carriers are not things it does —
                // their whole content is the "Its own defenses" block below.
                .filter((ability) => !isPetStatCarrier(ability))
                .map((ability) => (
                  <PetEffectAbilityRow key={ability.name} ability={ability} />
                ))}
            </div>
          )}

          {/* Effects expandable section */}
          {hasEffects && (
            <div className="mt-1">
              <div
                role="button"
                aria-expanded={effectsExpanded}
                title={effectsExpanded ? 'Hide pet effect list' : 'Show full list of effects this pet applies'}
                className="flex items-center gap-1 cursor-pointer select-none"
                onClick={() => setEffectsExpanded(!effectsExpanded)}
              >
                <span className="text-[11px] text-slate-400">{effectsExpanded ? '▼' : '▶'}</span>
                <span className="text-xs text-slate-300 font-medium">Effects</span>
              </div>
              {effectsExpanded && (
                // Split into the pet's OWN stats and what it puts on somebody
                // else. A summon is a second character (COH-DATA-MODEL §6), so
                // its resistance and mez protection describe the thing standing
                // there; the debuffs describe what it does to enemies. Rendered
                // as one flat list they read as a single undifferentiated pile.
                <div className="mt-0.5 ml-3 space-y-0.5">
                  <PetEffectGroup
                    title="Its own defenses"
                    hint="The summon's own stats — its resistance, defense and mez protection. These belong to the pet and are never added to your character totals."
                    effects={petEffects.self}
                  />
                  <PetEffectGroup
                    title="Applies to targets"
                    hint="What this pet puts on the things it hits (or on allies in range)."
                    effects={petEffects.applied}
                    // Only worth naming when there is something to contrast it
                    // with — a pet with nothing but debuffs needs no heading.
                    showTitle={petEffects.self.length > 0}
                  />
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * The henchman-upgrade row.
 *
 * Reads as build state, not as a what-if: a Mastermind who has taken Equip
 * Mercenary has equipped henchmen, so the boxes come pre-ticked from the build
 * and the labels name the actual powers. It stays clickable — comparing an
 * un-upgraded pet against an upgraded one is a real question — but a click is
 * then flagged as an override, with a way back.
 *
 * When the dataset can't say which power grants which tier (Thunderspy's export
 * doesn't resolve grant targets), the generic labels return and nothing is
 * pre-ticked. Better a plain "Upgrade 1" the user drives than a named power the
 * data can't actually vouch for.
 */
function PetUpgradeRow({
  statuses, available, activeTiers, derivable, overridden, onToggle, onFollowBuild,
}: {
  statuses: PetUpgradeStatus[];
  available: ReadonlySet<number>;
  activeTiers: ReadonlySet<number>;
  derivable: boolean;
  overridden: boolean;
  onToggle: (tier: number) => void;
  onFollowBuild: () => void;
}) {
  // One entry per selectable tier. Named where we know the granting power,
  // positional where we don't.
  const rows = derivable
    ? statuses.map((s) => ({ tier: s.tier, label: s.powerName.replace(/_/g, ' '), taken: s.taken }))
    : [...available].sort().map((tier, i) => ({ tier, label: `Upgrade ${i + 1}`, taken: false }));

  return (
    <div className="flex items-center gap-3 mb-2 pb-1.5 border-b border-indigo-500/20 flex-wrap">
      <span className="text-xs text-indigo-400 font-medium">Upgrades:</span>
      {rows.map((row) => (
        <label key={row.tier} className="flex items-center gap-1 cursor-pointer">
          <input
            type="checkbox"
            checked={activeTiers.has(row.tier)}
            onChange={() => onToggle(row.tier)}
            className="w-3 h-3 accent-indigo-500"
          />
          <span
            className={`text-xs ${activeTiers.has(row.tier) ? 'text-indigo-300' : 'text-slate-400'}`}
            title={derivable
              ? (row.taken ? `${row.label} is in this build` : `${row.label} is not in this build`)
              : 'This dataset does not record which power grants this upgrade'}
          >
            {row.label}
            {derivable && !row.taken && <span className="text-slate-500"> (not taken)</span>}
          </span>
        </label>
      ))}
      {overridden && (
        <button
          type="button"
          onClick={onFollowBuild}
          className="text-[10px] text-indigo-400 underline hover:text-indigo-300"
          title="Stop overriding and follow the powers this build has taken"
        >
          follow build
        </button>
      )}
    </div>
  );
}

export function PetDamageDisplay({ summon, level, enhancementDamageBonus, globalDamageBonus, archetypeId }: PetDamageDisplayProps) {
  const build = useBuildStore((s) => s.build);
  // Manual override of the derived tier set. Null = follow the build, which is
  // what a Mastermind wants: taking Equip Mercenary equips the henchmen, and a
  // panel defaulting to un-equipped shows a pet the character doesn't have.
  const [tierOverride, setTierOverride] = useState<Set<number> | null>(null);
  // "Storm Cell Active" / High Winds — empowered pseudo-pet display (WindSpeed + lightning).
  const stormCellActive = useGlobalAdjuster('stormblast_instormcell', false);

  // Build entity list from either entities array or single entity. Filter out
  // entries we don't have damage data for AND that look visual-only (e.g. Rain
  // of Arrows summons a `P4047293352` visual entity alongside its real damage
  // pseudopet `Pets_RainofArrows`). The visual marker has no abilities and
  // surfaces no useful info to the user — drop it. We keep entries that lack
  // PET_ENTITIES data IF they at least look like a real entity name (e.g.
  // `Pets_*`, `MastermindPets_*`) so unmodeled-but-real pets still show up.
  const entityList = useMemo(() => {
    const raw = summon.entities
      ? summon.entities.map(e => ({ entityName: e.entity, count: e.count }))
      : summon.entity
        ? [{ entityName: summon.entity, count: summon.entityCount || 1 }]
        : [];
    return raw.filter(({ entityName }) => {
      // Has model data → keep (tolerates un-prefixed names → Pets_<name>, e.g. Sleet)
      if (getPetEntity(entityName)) return true;
      // Looks like a real (un-modeled) pet name → keep so we can still surface it
      if (/^(Pets_|MastermindPets_|Villain_Pets_|VillainPets_)/i.test(entityName)) return true;
      // P-hashes and other opaque identifiers without model data are
      // almost always FX / visual placeholders. Drop.
      return false;
    });
  }, [summon.entities, summon.entity, summon.entityCount]);

  const entities = useMemo(() => entityList.map(e => getPetEntity(e.entityName)), [entityList]);

  // Which upgrades the BUILD has taken, and their status for the label row.
  // Undefined = this dataset's export doesn't record what grants each tier
  // (Thunderspy), so the tier can't be read off the build and the checkboxes
  // stay as the only answer.
  const upgrades = useMemo(() => {
    const taken = takenPowerNames(build);
    const statuses = petUpgradeStatuses(entities, taken);
    const derived = new Set<number>();
    let derivable = false;
    for (const entity of entities) {
      const tiers = resolveActiveUpgradeTiers(entity, taken);
      if (!tiers) continue;
      derivable = true;
      for (const t of tiers) derived.add(t);
    }
    const available = new Set<number>();
    for (const entity of entities) for (const t of entity?.upgradeTiers ?? []) available.add(t.tier);
    return { statuses, derived, derivable, available };
  }, [entities, build]);

  const activeTiers = tierOverride ?? upgrades.derived;

  // Calculate pet damage for each entity
  const petResults = useMemo(() => {
    return entityList.map(e => {
      const applyEnh = shouldApplyEnhancements(e.entityName, summon.copyBoosts);
      const enhBonus = applyEnh ? enhancementDamageBonus : 0;
      return {
        entityName: e.entityName,
        count: e.count,
        result: calculatePetDamage(
          e.entityName, level, e.count, summon.duration,
          enhBonus, applyEnh, globalDamageBonus, activeTiers
        ),
      };
    });
  }, [entityList, level, enhancementDamageBonus, globalDamageBonus, activeTiers, summon.copyBoosts, summon.duration]);

  // Synthesized location pseudo-pets (Storm Cell, Category Five, …). Their DAMAGE
  // is already unified into the headline Damage block (pseudoPetDamage), so here
  // we render only the informational applied-effects list (the IgnoreStrength
  // debuffs + mez the player can't enhance) — `hideDamage` suppresses the DPS row.
  const resolvedResults = useMemo(() => {
    return (summon.resolvedEntities ?? []).map((re, i) => {
      const applyEnh = re.copyCreatorMods;
      return {
        key: `${re.displayName}-${i}`,
        count: re.count ?? 1,
        duration: re.duration,
        label: re.displayName,
        result: calculateResolvedPseudoPetDamage(
          re, archetypeId ?? '', level,
          applyEnh ? enhancementDamageBonus : 0, applyEnh, globalDamageBonus, stormCellActive,
        ),
      };
    }).filter(r => r.result && r.result.allEffects.length > 0);
  }, [summon.resolvedEntities, archetypeId, level, enhancementDamageBonus, globalDamageBonus, stormCellActive]);

  // Conditional ("ignited") entities — Oil Slick Arrow's burn patch. Always shown
  // here (flagged via the label) as the triggered-state damage; it only folds into
  // the headline Damage block when the per-power toggle is on (see pseudoPetDamage).
  const conditionalResults = useMemo(() => {
    return (summon.conditionalEntities ?? []).map((ce, i) => {
      const entity = getPetEntity(ce.entity);
      const applyEnh = entity?.copyCreatorMods ?? false;
      return {
        key: `cond-${ce.entity}-${i}`,
        label: `${ce.label} (when triggered)`,
        result: calculatePetDamage(
          ce.entity, level, 1, summon.duration,
          applyEnh ? enhancementDamageBonus : 0, applyEnh, globalDamageBonus,
        ),
      };
    }).filter(r => r.result);
  }, [summon.conditionalEntities, level, enhancementDamageBonus, globalDamageBonus, summon.duration]);

  // Aggregate totals across all entities
  const totals = useMemo(() => {
    let base = 0, enhanced = 0, final_ = 0;
    for (const r of petResults) {
      if (r.result) {
        base += r.result.aggregateDpsBase;
        enhanced += r.result.aggregateDpsEnhanced;
        final_ += r.result.aggregateDpsFinal;
      }
    }
    return { base, enhanced, final: final_ };
  }, [petResults]);

  const isMultiEntity = entityList.length > 1;
  const isSingleEntity = entityList.length === 1;
  // Mutually-exclusive variants (Soul Extraction's tier ghosts): exactly ONE
  // materializes, so the per-variant cards are alternatives — never an additive
  // group. Label them and suppress the summed "Total … DPS" (which would imply
  // you get all three at once).
  const mutuallyExclusive = summon.mutuallyExclusive === true;
  const hasDamage = totals.base > 0 && !mutuallyExclusive;
  const hasEnh = totals.enhanced !== totals.base;
  const hasBuff = totals.final !== totals.enhanced;
  const durationLabel = formatSummonDuration(summon.duration);

  // Nothing to show (e.g. a location shell with only an unresolved entity).
  if (entityList.length === 0 && resolvedResults.length === 0 && conditionalResults.length === 0) return null;

  return (
    <div className="bg-indigo-900/30 rounded p-2 border border-indigo-500/30">
      {/* Upgrades. Named after the powers that grant them where the data says
          which those are, and pre-set from the build rather than from zero. */}
      {upgrades.available.size > 0 && (
        <PetUpgradeRow
          statuses={upgrades.statuses}
          available={upgrades.available}
          activeTiers={activeTiers}
          derivable={upgrades.derivable}
          overridden={tierOverride !== null}
          onToggle={(tier) => {
            const next = new Set(activeTiers);
            if (next.has(tier)) next.delete(tier); else next.add(tier);
            setTierOverride(next);
          }}
          onFollowBuild={() => setTierOverride(null)}
        />
      )}

      {/* Single entity display (same as before) */}
      {isSingleEntity && (
        <SingleEntityDisplay
          petDamage={petResults[0]?.result ?? null}
          entityCount={entityList[0].count}
          label={petResults[0]?.result?.displayName || summon.displayName || summon.entity || 'Entity'}
          showHeader={true}
          isPseudoPet={summon.isPseudoPet}
          duration={summon.duration}
        />
      )}

      {/* Multi-entity display */}
      {isMultiEntity && (
        <div className="space-y-2">
          {mutuallyExclusive && (
            <div className="text-xs text-indigo-300 font-medium">
              Summons 1 of (tier matches the henchman you sacrifice):
            </div>
          )}
          {petResults.map((pr) => (
            <SingleEntityDisplay
              key={pr.entityName}
              petDamage={pr.result}
              entityCount={pr.count}
              label={pr.result?.displayName || pr.entityName.replace(/^(Pets_|MastermindPets_)/i, '').replace(/_/g, ' ')}
              showHeader={true}
              isPseudoPet={summon.isPseudoPet}
              duration={summon.duration}
            />
          ))}

          {/* Aggregate total across all entity types. "Henchmen" only fits
              Mastermind primary summons; everything else (Lore incarnates,
              Phantom Army, summons from epic pools, etc.) gets a neutral label. */}
          {hasDamage && (
            <div className="border-t border-indigo-500/30 pt-1.5 mt-1.5">
              <div className="flex items-center gap-1">
                <span className="text-xs text-slate-300 font-medium">
                  {archetypeId === 'mastermind' ? 'Total Henchmen DPS:' : 'Total Pet DPS:'}
                </span>
                <span className="text-xs text-red-400 font-medium">{totals.base.toFixed(1)}</span>
                {hasEnh && <span className="text-xs text-green-400">→ {totals.enhanced.toFixed(1)}</span>}
                {hasBuff && <span className="text-xs text-amber-400">→ {totals.final.toFixed(1)}</span>}
              </div>
            </div>
          )}
        </div>
      )}

      {/* Synthesized location pseudo-pets (Storm Cell, Category Five, …): the
          informational applied-effects list (IgnoreStrength debuffs + mez). The
          DAMAGE is already unified into the headline Damage block, so hideDamage. */}
      {resolvedResults.length > 0 && (
        <div className="space-y-2">
          {resolvedResults.map((rr) => (
            <SingleEntityDisplay
              key={rr.key}
              petDamage={rr.result}
              entityCount={rr.count}
              label={rr.label}
              showHeader={true}
              isPseudoPet={true}
              duration={rr.duration ?? summon.duration}
              hideDamage={true}
            />
          ))}
        </div>
      )}

      {/* Conditional ("ignited") entities — Oil Slick Arrow's burn patch. Shown
          with its Fire DPS as the triggered-state potential; folds into the
          headline Damage block when the "Oil Slick Ignited" toggle is on. */}
      {conditionalResults.length > 0 && (
        <div className="space-y-2">
          {conditionalResults.map((cr) => (
            <SingleEntityDisplay
              key={cr.key}
              petDamage={cr.result}
              entityCount={1}
              label={cr.label}
              showHeader={true}
              isPseudoPet={true}
              duration={summon.duration}
            />
          ))}
        </div>
      )}

      {/* No entity display (pseudopets with no data) */}
      {entityList.length === 0 && resolvedResults.length === 0 && conditionalResults.length === 0 && (
        <div className="flex items-center gap-2">
          <span className="text-indigo-400 text-sm font-medium">
            {summon.isPseudoPet ? '⚡ Creates' : '🐾 Summons'}
          </span>
          <span className="text-slate-200 text-sm">
            {summon.displayName || 'Entity'}
          </span>
          <span className="text-slate-400 text-xs">({durationLabel})</span>
        </div>
      )}

      {/* Fallback: show powers list if no pet data at all */}
      {entityList.length === 0 && resolvedResults.length === 0 && conditionalResults.length === 0 && summon.powers && summon.powers.length > 0 && (
        <div className="text-xs text-slate-400 mt-0.5">
          Powers: {summon.powers.map(p => p.split('.').pop()).join(', ')}
        </div>
      )}
    </div>
  );
}
