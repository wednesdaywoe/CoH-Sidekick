/**
 * Homecoming dataset — assembles per-server data into a single `Dataset`
 * object that satisfies the contract in `src/data/dataset.ts`.
 *
 * The pieces here close over Homecoming's own data records, so when this
 * dataset is active, calls like `getActiveDataset().getTableValue(...)`
 * resolve against HC's tables. Other datasets (Rebirth, …) live in
 * sibling folders and ship their own version of this file.
 */

import type { Dataset } from '../../dataset';
import {
  ARCHETYPES,
  EPIC_ARCHETYPE_IDS,
  STANDARD_ARCHETYPE_IDS,
  getArchetype,
} from './archetypes';
import {
  AT_TABLES,
  PET_TABLES,
  getTableValue,
  calculateEffectValue,
  calculateIncarnateDamage,
  getPetTableValue,
} from './at-tables';
import { getBaseToHit, getCombatModifier, getDefenseSoftcap } from './purple-patch';
import { GRANTED_POWER_GROUPS } from './granted-powers';
import { ENHANCEMENT_CURVES } from './generated/enhancement-curves';
import { SPECIAL_ENHANCEMENTS } from './generated/special-enhancements';
import { GENERATED_ARCHETYPE_INHERENTS, HEADLINE_ARCHETYPE_INHERENTS } from './generated/archetype-inherents';
import { PET_ENTITIES } from './pet-entities';
import { MODULAR_POWERSETS } from './powersets/index';
import { IO_SETS_RAW } from './io-sets-raw';
import { MIDS_UIDS } from './generated/mids-uids';
import { EPIC_POOLS_RAW } from './epic-pools-raw';
import type { LegacyEpicPoolRegistry } from '../../epic-pools';
import * as IncarnateGen from './generated/incarnate-effects';
import type { IncarnateEffectsRaw } from '../../incarnate-effects';
import { POWER_POOLS_RAW } from './power-pools-raw';
import type { LegacyPowerPoolRegistry } from '../../power-pools';

const dataset: Dataset = {
  id: 'homecoming',
  displayName: 'Homecoming',

  archetypes: {
    registry: ARCHETYPES,
    epicIds: EPIC_ARCHETYPE_IDS,
    standardIds: STANDARD_ARCHETYPE_IDS,
  },

  atTables: {
    archetypes: AT_TABLES,
    pets: PET_TABLES,
  },

  purplePatch: {
    getBaseToHit,
    getCombatModifier,
    getDefenseSoftcap,
  },

  grantedPowerGroups: GRANTED_POWER_GROUPS,

  // Homecoming uses the baseline inherent power configuration: Fitness
  // available at L1, no auto-granted enhancement slots.
  inherentRules: {
    availabilityOverrides: {},
    autoGrantedSlotLevels: {},
    archetypeInherents: GENERATED_ARCHETYPE_INHERENTS,
    headlineArchetypeInherents: HEADLINE_ARCHETYPE_INHERENTS,
  },

  petEntities: PET_ENTITIES,
  enhancementCurves: ENHANCEMENT_CURVES,
  specialEnhancements: SPECIAL_ENHANCEMENTS,

  powersetsRaw: MODULAR_POWERSETS,
  ioSetsRaw: IO_SETS_RAW,
  midsUids: MIDS_UIDS,
  epicPoolsRaw: EPIC_POOLS_RAW as unknown as LegacyEpicPoolRegistry,
  incarnateEffectsRaw: {
    alpha: IncarnateGen.GENERATED_ALPHA_EFFECTS,
    alphaEdBypass: IncarnateGen.GENERATED_ALPHA_ED_BYPASS,
    destiny: IncarnateGen.GENERATED_DESTINY_EFFECTS,
    destinyTimeline: IncarnateGen.GENERATED_DESTINY_TIMELINE,
    destinyBoosts: IncarnateGen.GENERATED_DESTINY_BOOSTS,
    hybrid: IncarnateGen.GENERATED_HYBRID_EFFECTS,
    interface: IncarnateGen.GENERATED_INTERFACE_EFFECTS,
    judgement: IncarnateGen.GENERATED_JUDGEMENT_EFFECTS,
    lore: IncarnateGen.GENERATED_LORE_EFFECTS,
    // This server's export carries a genesis table, but the slot was defined and
    // never enabled: 36/36 powers with authored help of literally "… text",
    // reused Interface icons, and no exemplar-grant linkage. Serving it would
    // apply a slot the picker rightly hides (DATASET_ONLY_SLOTS in
    // incarnates.ts) — see GENESIS-1.
    genesis: {},
  } as unknown as IncarnateEffectsRaw,
  powerPoolsRaw: POWER_POOLS_RAW as unknown as LegacyPowerPoolRegistry,

  getTableValue,
  calculateEffectValue,
  calculateIncarnateDamage,
  getPetTableValue,
  getArchetype,
};

export default dataset;
