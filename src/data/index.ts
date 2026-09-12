/**
 * Data layer barrel export
 *
 * Import data and accessors from here:
 * import { getArchetype, getIOSet, getSpecialRegistry } from '@/data';
 */

// Archetype data and accessors
export {
  ARCHETYPES,
  getArchetype,
  getArchetypeIds,
  getArchetypesByFaction,
  EPIC_ARCHETYPE_IDS,
  STANDARD_ARCHETYPE_IDS,
  isEpicArchetype,
  getEpicArchetypes,
  getStandardArchetypes,
} from './archetypes';

// Enhancement data (non-IO)
export {
  COMMON_IO_TYPES,
  ORIGIN_TIER_INFO,
  ORIGINS,
  DUAL_ORIGIN_COMBOS,
  isDualOriginValidForOrigin,
  ENHANCEMENT_CATEGORIES,
} from './enhancements';
export type { DualOriginCombo, EnhancementCategory } from './enhancements';

// Special-enhancement registries (per-dataset generated, SOURCE-1 item 9)
export { getSpecialEnhancements, getSpecialRegistry } from './special-enhancements';
export type { SpecialCategory, GeneratedSpecialEnhancementDef } from './special-enhancements';

// Enhancement registry (centralized mappings, factory functions, query functions)
export {
  // Stat icons
  STAT_ICON_MAP,
  getStatIconFilename,
  getGenericIOIconPath,
  getOriginIconPath,
  // Category mappings
  CATEGORY_PRIORITY,
  sortCategoriesByPriority,
  // Display config
  RARITY_DISPLAY,
  getRarityColor,
  TIER_DISPLAY,
  getTierTextColor,
  getTierBorderColor,
  // Factory functions
  isInherentlyAttuned,
  createIOSetEnhancement,
  createGenericIOEnhancement,
  createSpecialEnhancement,
  createOriginEnhancement,
  // Query functions
  getAvailableGenericIOs,
  getAvailableHamidons,
  getAvailableTitans,
  getAvailableHydras,
  getAvailableDSyncs,
  getAvailablePrestige,
} from './enhancement-registry';

// IO Set data and accessors
export {
  getAllIOSets,
  getMostCommonSetSize,
  getIOSet,
  getIOSetsByRarity,
  getIOSetsForCategory,
  getIOSetsForPower,
  getIOSetPiece,
  getSetBonusesAtCount,
  getAllIOSetTypes,
  IO_SET_RARITIES,
  getIOSetRarityInfo,
  ARCHETYPE_ATO_CATEGORY,
} from './io-sets';
export type { IOSetRarityInfo } from './io-sets';

// Powerset data and accessors
export {
  getAllPowersets,
  getPowerset,
  getPowersetsForArchetype,
  getPower,
  getPowersAvailableAtLevel,
} from './powersets';

// Unified power icon path resolution (flat /img/powers/ folder)
export { getPowerIconPath } from '@/utils/power-icons';
export type { PowersetRegistry } from './powersets';

// Power Pool data and accessors
export {
  getAllPowerPools,
  getPowerPool,
  getPowerPoolIds,
  getPoolPower,
  getPoolPowersAvailableAtLevel,
  getPoolEntryPowers,
  arePoolPrerequisitesMet,
  POOL_CATEGORIES,
  getPoolsByCategory,
  arePoolsUnlocked,
  isPowerAvailableInPool,
  getAvailablePoolPowers,
  getExcludedPools,
} from './power-pools';
export type { PowerPoolRegistry, PoolCategoryInfo } from './power-pools';

// Epic/Patron Pool data and accessors
export {
  getAllEpicPools,
  getEpicPool,
  getEpicPoolsForArchetype,
  areEpicPoolsUnlocked,
  isEpicPowerAvailable,
} from './epic-pools';
export type { EpicPool, EpicPoolRegistry } from './epic-pools';

// Level progression data and accessors
export {
  // Constants
  MAX_LEVEL,
  EPIC_POOL_LEVEL,
  POOL_UNLOCK_LEVEL,
  getPoolUnlockLevel,
  MAX_POWER_POOLS,
  getMaxPowerPools,
  MAX_POWER_PICKS,
  MAX_SLOTS_PER_POWER,
  TOTAL_SLOTS_AT_50,
  // Power picks
  POWER_PICK_LEVELS,
  isPowerPickLevel,
  getPowerPicksAtLevel,
  // Slot grants
  SLOT_GRANTS,
  getSlotGrants,
  getSlotsGrantedAtLevel,
  getTotalSlotsAtLevel,
  getNextGrantLevel,
  getPicksGrantedAtLevel,
  getProgressionLevel,
  // Enhancement availability
  ENHANCEMENT_AVAILABILITY,
  isEnhancementAvailable,
  // Pool requirements (pool gating is data-driven; epic still uses this table)
  EPIC_TIER_REQUIREMENTS,
  // Epic pools
  canAccessEpicPools,
  // Incarnate
  INCARNATE_LEVEL,
  INCARNATE_SLOTS,
  // Level info
  getLevelInfo,
  generateProgressionTable,
  // Inherent powers
  INHERENT_FITNESS_POWERS,
  BASIC_INHERENT_POWERS,
  PRESTIGE_SPRINT_POWERS,
  getInherentPowers,
  getInherentPowerDef,
  getArchetypeInherentPowers,
  getPickShadowingInherentPowers,
  createArchetypeInherentPower,
} from './levels';
export type { LevelInfo, InherentPowerDef } from './levels';

// Accolades — derived from the exported Accolades powerset (see ./accolades)
export { getAccolades, getAccolade, accoladeId, accoladeFaction, accoladeRequiredModes } from './accolades';
export type { AccoladePower, AccoladeFaction } from './accolades';

// Incarnate data and accessors
export {
  getAllIncarnateSlots,
  getIncarnateSlot,
  getIncarnateTrees,
  getIncarnateTree,
  getIncarnatePowersForTree,
  getIncarnatePowersByTier,
  getIncarnatePower,
  getIncarnateSlotIconPath,
  getSelectableIncarnateSlotIds,
} from './incarnates';

// Incarnate registry (centralized slot/tier metadata, layout config, display helpers)
export {
  // Slot config
  INCARNATE_SLOT_REGISTRY,
  getSlotConfig,
  getSlotColor,
  isSlotToggleable,
  getToggleableSlotIds,
  // Tier config
  INCARNATE_TIER_REGISTRY,
  getTierConfig,
  getTierColor,
  getTierDisplayName,
  // Tree descriptions
  TREE_DESCRIPTIONS,
  getTreeDescription,
  // Tree layout
  STANDARD_TREE_LAYOUT,
  resolveTreeRow,
  // Display helpers
  RARE_SORT_KEYWORDS,
  NAME_ABBREVIATION_RULES,
  abbreviatePowerName,
  sortRarePowers,
  // Backward-compatible derived constants
  INCARNATE_SLOT_COLORS,
  INCARNATE_TIER_COLORS,
  INCARNATE_TIER_NAMES,
} from './incarnate-registry';
export type {
  IncarnateEffectType,
  IncarnateSlotConfig,
  IncarnateTierConfig,
  TreeSlotDescriptor,
  TreeRowLayout,
  TreeLayoutConfig,
} from './incarnate-registry';

// Incarnate effects data
export {
  getAlphaEffects,
  getAlphaEdBypass,
  getDestinyEffects,
  getDestinyEffectsAtTime,
  getDestinyTimeline,
  getDestinyTotalDuration,
  getDestinySustainedFloorTime,
  getDestinyBoostsAllowed,
  applyAlphaToDestiny,
  getHybridEffects,
  getInterfaceEffects,
  getJudgementEffects,
  getLoreEffects,
  getLevelShiftGrants,
  getGenesisEffects,
  getIncarnateEffects,
  formatEffectPercent,
  formatEffectValue,
  normalizePowerId as normalizeIncarnatePowerId,
} from './incarnate-effects';
export type {
  AlphaEffects,
  DestinyEffects,
  DestinyTimeline,
  DestinyTimelineTier,
  HybridEffects,
  InterfaceEffects,
  JudgementEffects,
  LoreEffects,
  GenesisEffects,
  GenesisExemplarEffect,
  IncarnatePowerEffects,
} from './incarnate-effects';

// Incarnate salvage registry
export {
  SALVAGE_REGISTRY,
  SALVAGE_RARITY_COLORS,
  getSalvageDefinition,
  getSalvageDisplayName,
  getSalvageRarity,
  getSalvageRarityColor,
  getSalvageCost,
  parseSalvageString,
} from './incarnate-salvage';
export type { SalvageDefinition } from './incarnate-salvage';

// Invention (IO-crafting) salvage — binary-sourced from salvage.bin (108 items).
export { INVENTION_SALVAGE_REGISTRY } from './generated/invention-salvage.generated';
export type {
  InventionSalvageDefinition,
  InventionSalvageRarity,
} from './generated/invention-salvage.generated';

// Incarnate crafting recipes
export {
  INCARNATE_RECIPES,
  CRAFTING_CONVERSIONS,
  getTierRecipe,
  calculateCumulativeCost,
} from './incarnate-recipes';

// Incarnate crafting components
export {
  LORE_TREE_NAME_MAP,
  getComponentTreeKey,
  getTreeComponents,
  getVariantComponents,
  getCumulativeSalvage,
} from './incarnate-components';

// Proc enhancement data
export {
  PROC_DATABASE,
  findProcData,
  resolveProcPieceName,
  getProcEffects,
  procEffectSummary,
  getProcEffectLabel,
  getProcEffectColor,
  isProcAlwaysOn,
  resolveProcRollGeometry,
  powerFiresProcs,
  resolveProcRollSite,
  interpolateProcDamage,
  // PPM calculation functions
  getPPMAreaFactor,
  getPPMAreaDenominator,
  arcToDegrees,
  calculateProcChance,
  calculateProcsPerMinute,
  calculateProcDPS,
  calculateAutoToggleProcChance,
  calculateAutoToggleProcsPerMinute,
  calculateProcStats,
  AUTO_POWER_PSEUDO_RECHARGE,
  resolveProcRollSchedule,
  procRollsInPatch,
  calculateScheduledProcChance,
  // Variable-proc controls (per-proc toggles + stack / HP sliders)
  getProcControlType,
  isVariableProc,
  DEFAULT_STACK_COUNT,
  interpolateScalingValue,
  resolveProcContribution,
  procOverrideKey,
  isDefaultProcOverride,
  pruneProcOverridesForRemovedPowers,
  reindexProcOverridesForRemovedSlot,
  DEFAULT_PROC_OVERRIDE,
} from './proc-data';
export type { ProcData, ProcType, ProcEffectCategory, ParsedProcEffect, ProcEffect, PowerProcCalcData, ProcControlType, ProcRollSchedule } from './proc-data';

// Granted powers (sub-powers granted by parent powers like Adaptation)
export {
  GRANTED_POWER_GROUPS,
  hasGrantedPowers,
  getGrantedPowerGroup,
  getActiveDamageConversion,
} from './granted-powers';
export type { GrantedPowerGroup } from './granted-powers';

// Inherent power rules (per-server Fitness availability + auto-granted slots)
export {
  getInherentAvailabilityOverride,
  getInherentGrantLevel,
  getInherentAutoGrantedSlotLevels,
  getInherentAutoGrantedSlotCount,
} from './inherent-rules';

// Unified power lookup
export { lookupPower } from './power-lookup';
export type { PowerLookupResult } from './power-lookup';

// Stat/effect color palette (single source of truth)
export { STAT_COLORS } from './stat-colors';

// Global "stance" selector descriptors (Bio Armor Adaptation + Staff Fighting
// Form/Perfection) — one shared header control gives them identical treatment.
// The stance lives in the parent's `activeSubPower` (build-scoped); these are
// the descriptors + derive helpers all consumers share.
export {
  STANCE_GROUPS,
  stanceGroupForConditionalId,
  toStancePowers,
  findStanceParent,
  activeStanceOptionId,
  stanceAdjusterOverrides,
} from './stance-groups';
export type { StanceGroup, StanceOption, StancePowerLike } from './stance-groups';

// Effect registry for data-driven power effect display
export {
  EFFECT_REGISTRY,
  CATEGORY_CONFIG,
  groupEffectsByCategory,
  isByTypeObject,
  isMezEffect,
  formatMezValue,
  calculateEffectValue,
  formatEffectValue as formatRegistryEffectValue,
  getByTypeAbbreviations,
  getByTypeFirstValue,
  getRegisteredEffectKeys,
  isRegisteredEffect,
} from './effect-registry';
export type {
  EffectCategory,
  EffectFormat,
  EffectDisplayConfig,
  CategoryDisplayConfig,
  GroupedEffect,
  GroupedEffects,
} from './effect-registry';

// Adjudicated description notes — where a power's in-game text and the planner's data disagree.
export { DESCRIPTION_NOTES, getDescriptionNote } from './description-notes';
export type { DescriptionNote } from './description-notes';
