/**
 * Rebirth dataset — assembles per-server data into a single `Dataset`
 * object that satisfies the contract in `src/data/dataset.ts`.
 *
 * Mirrors `datasets/homecoming/index.ts`. Several pieces (purple-patch,
 * granted-powers) currently re-export HC's data as a first-pass
 * approximation; pet-entities is an empty placeholder pending a
 * `PC_Def_Entities.bin` parser. See MULTI_DATASET_PLAN.md.
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
  id: 'rebirth',
  displayName: 'Rebirth',

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

  // Rebirth tunes Fitness: Swift / Hurdle / Health / Stamina become
  // available at L2 (one level later than HC), and Health/Stamina each
  // receive two auto-granted enhancement slots at fixed levels —
  // outside the 67-slot user budget.
  inherentRules: {
    availabilityOverrides: {
      Swift: 1,
      Hurdle: 1,
      Health: 1,
      Stamina: 1,
    },
    autoGrantedSlotLevels: {
      Health: [8, 16],
      Stamina: [12, 22],
    },
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
    // Rebirth is the only server that finished the Genesis slot; the other two
    // export a byte-shaped-but-dormant table and serve `{}` — see GENESIS-1.
    genesis: IncarnateGen.GENERATED_GENESIS_EFFECTS,
  } as unknown as IncarnateEffectsRaw,
  powerPoolsRaw: POWER_POOLS_RAW as unknown as LegacyPowerPoolRegistry,

  getTableValue,
  calculateEffectValue,
  calculateIncarnateDamage,
  getPetTableValue,
  getArchetype,
};

export default dataset;
