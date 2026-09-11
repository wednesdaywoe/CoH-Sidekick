/**
 * Dataset plumbing — the single point of indirection that lets the planner
 * support multiple CoH server datasets (Homecoming, Rebirth, …).
 *
 * Each dataset folder under `src/data/datasets/<id>/` ships a `Dataset`
 * object as its default export. Exactly one dataset is "active" at a
 * time. App boot calls `loadDataset()` to choose the active one based on
 * the current build's `serverId`; data files at `src/data/*.ts` are thin
 * facades that read from `getActiveDataset()`, so every consumer keeps
 * its existing import paths and the active dataset transparently swaps
 * at runtime.
 *
 * See MULTI_DATASET_PLAN.md for the broader plan.
 */

import type { ArchetypeId, ArchetypeRegistry, Archetype, Powerset } from '@/types';
import type { LegacyIOSetRegistry } from './io-sets';
import type { MidsUidTable } from './mids-uids';
import type { LegacyEpicPoolRegistry } from './epic-pools';
import type { IncarnateEffectsRaw } from './incarnate-effects';
import type { LegacyPowerPoolRegistry } from './power-pools';
// The canonical EnhancementCurvesData shape is authored by the SW3 converter
// (scripts/convert-enhancement-curves.cjs) into every dataset's generated
// module; the homecoming copy is imported type-only as the contract's
// reference declaration. The staleness guard asserts all three modules stay
// structurally identical to the binary export.
import type { EnhancementCurvesData } from './datasets/homecoming/generated/enhancement-curves';
import type { SpecialEnhancementsData } from './datasets/homecoming/generated/special-enhancements';

export type { EnhancementCurvesData, EnhancementSchedule, OriginTier } from './datasets/homecoming/generated/enhancement-curves';
export type { GeneratedSpecialEnhancementDef, SpecialEnhancementsData } from './datasets/homecoming/generated/special-enhancements';
// Same rationale as above: the archetype-inherent shape is authored in the
// homecoming levels module and imported type-only as the contract's reference
// declaration. A dataset that populates `archetypeInherents` satisfies it.
import type { InherentPowerDef } from './datasets/homecoming/levels';

// ============================================
// DATA SHAPES SHARED ACROSS DATASETS
// ============================================
// Defined here (not in a per-dataset file) because they're part of the
// `Dataset` interface contract — every dataset implementation must satisfy
// the same shape, and consumer code reads through that shape.

export interface ATTableData {
  primaryCategory: string;
  secondaryCategory: string;
  /** RechargeTime ClampStrength interval — the bounds on NET recharge strength
   *  (floor 0.25 = the −75% debuff floor, cap 5 = +400%). Absent when the
   *  dataset's export didn't carry it. */
  rechargeBounds?: { floor: number; cap: number };
  tables: Record<string, number[]>;
}

/** Scalar for a base, per-level array for a cap. */
export interface PetMovementAttrib {
  run_speed?: number | number[];
  fly_speed?: number | number[];
  jump_speed?: number | number[];
  jump_height?: number | number[];
}

/**
 * A pet class's own character stats.
 *
 * A summon is a second character, and these are the numbers that make it one:
 * its hit points and the ceilings it lives against. They come from the PET's
 * class row (`Class_Minion_Pets`, `Class_Henchman_Boss`, …), never from the
 * caster's archetype — a Bruiser's 90% resistance cap is not its Mastermind's,
 * and neither is its HP.
 */
export interface PetClassAttribs {
  /** Base max HP per level (index = level − 1). */
  hitPoints: number[];
  hpCap?: number[];
  absorbCap?: number[];
  /** Damage-resistance ceiling as a fraction (0.9 = 90%). */
  resistanceCap?: number;
  /** Damage strength ceiling as a multiplier (4 = +300%). */
  damageCap?: number;
  baseThreat?: number;
  rechargeFloor?: number;
  rechargeCap?: number;
  enduranceFloor?: number;
  enduranceCap?: number;
  movementBase?: PetMovementAttrib;
  movementCap?: PetMovementAttrib;
}

export interface PetTableData {
  tables: Record<string, number[]>;
  /** The class's villain-rank enum. Player classes are 0; the classes the game
   *  treats as summons are 10 — including `henchman_boss`, so this does NOT
   *  separate a henchman minion from a henchman boss. Useful only for telling a
   *  pet class apart from a repurposed NPC one (grunt 2, elite 6, AV 7). */
  villainRank?: number;
  /** Absent when the dataset's export didn't carry an attribs block. */
  attribs?: PetClassAttribs;
}

/** Shape for granted-power groups (parent → auto-granted children). */
export interface GrantedPowerGroup {
  parentPower: string;
  grantedPowers: string[];
  mutuallyExclusive: boolean;
  description?: string;
  slottable?: boolean;
  /** Sibling powers (internalNames) that must ALSO be selected in the same
   *  pool/powerset for the grant to fire (conjunctive grants, e.g. Rebirth's
   *  Group Fly needs both Aerobatics and Fly). See the granted-powers modules. */
  alsoRequires?: readonly string[];
  damageConversion?: Record<string, { from: string; to: string }>;
}

/** Pet damage / effect data for damage calculation. */
export interface PetDamageEntry {
  damageType: string;
  scale: number;
  table: string;
  /** Sub-1.0 hit chance from the effect group (Trip Mine's third Fire
   *  template lands 50% of the time). Absent = guaranteed. Damage layers
   *  must weight by this; summing at face value overstates the power. */
  chance?: number;
}

export interface PetEffect {
  type: string;
  magnitude?: number;
  chance?: number;
  scale?: number;
  table?: string;
  /** Mez-family rows: which of the mod's two numbers `scale × table` computes, carried so the
   *  pseudo-pet merge hands the display reader the same discriminator a parent power's mez row
   *  carries. Read by the control merge; a knock row's quantity is a distance either way. See
   *  {@link MezEffect.attribType}. */
  attribType?: string;
  /** The movement axis a Slow / MovementCapDebuff row applies to, spelled the
   *  way a parent power's `slow[axis]` spells it. A power states several axes
   *  at different scales, so the merge holds one value per axis rather than
   *  one per key. Absent on every other type. */
  axis?: string;
  /** Ally-buff auras (buff-pets like Force Field Generator / Barrier Reef).
   *  A DefenseBuff/ResistanceBuff aura buffs every listed sub-type at the
   *  same scale/table; absorbAspect distinguishes MaxHP-fraction (Maximum)
   *  from flat (Absolute) absorb. Folded into character totals when the
   *  summon's buff-pet toggle is enabled. */
  defenseTypes?: string[];
  resistanceTypes?: string[];
  absorbAspect?: string;
  /** The pet's OWN defensive profile (`target: Self` templates) —
   *  `SelfResistance` / `SelfDefense` / `SelfMezProtection` /
   *  `SelfMezResistance` / `SelfDebuffResistance`. Deliberately NOT the
   *  ally-aura type names above: those fold into the PLAYER's totals, and a
   *  pet's own resistance is not the player's. `scale` stays SIGNED on
   *  SelfResistance (a negative is a real vulnerability). */
  mezTypes?: string[];
  debuffTypes?: string[];
  /** The source template carried IgnoreStrength: the summoner's slotting does
   *  not reach this effect even though the pet inherits it via CopyBoosts. */
  ignoreStrength?: boolean;
}

export interface PetAbility {
  name: string;
  displayName: string;
  type: 'Click' | 'Auto' | 'Toggle';
  damage: PetDamageEntry[];
  effects?: PetEffect[];
  recharge: number;
  castTime: number;
  activatePeriod?: number;
  effectArea: string;
  /** EntsAffected — which entity categories this ability's effects can land on.
   *  Every effect row above is authored `target: AnyAffected`, and that word names
   *  whoever the ability affects, so this is the only field that separates a
   *  protection the pet grants its SUMMONER (Force Field Generator's Dispersion
   *  Bubble, `['Friend','Self']`) from one it inflicts on the foe it just held
   *  (Singularity's Gravity Distortion, `['Foe']`) — identical type names, identical
   *  scales. The pet is the caster here, so the polarity inverts against a player
   *  power: `Self` is the pet alone and the summoner arrives as `Friend`,
   *  `MyOwner` or `Teammate` (register ENT-12; the power-converter twin is
   *  MEZRES-3). Omitted when the export states nothing, so absent stays
   *  distinguishable from an authored empty list. */
  targetsAffected?: string[];
  range?: number;
  radius?: number;
  maxTargets?: number;
  attackTypes?: string[];
  rechargeUnaffected?: boolean;
}

export interface PetUpgradeTier {
  tier: number;
  abilities: PetAbility[];
  /** The player power(s) that turn this tier on, by internal name
   *  (`Equip_Mercenary`, `Tactical_Upgrade`). Derived from which powerset the
   *  grant targets — the `_2`/`_3` suffix IS the tier — so it holds whatever a
   *  server names its upgrades. Absent when the export carries no resolved
   *  grant targets (Thunderspy), in which case a consumer cannot tell from the
   *  build alone whether the tier is active. */
  grantedBy?: string[];
  /** Abilities this tier TAKES AWAY, by name. An upgrade replaces rather than
   *  adds: Equip Mercenary revokes the Soldier's base Resistance as it grants
   *  Equip, and Enchant Undead revokes the Skeletal Warrior's base Hack and
   *  Slash as it grants its own. Appending a tier without applying these
   *  double-counts the attacks and leaves the stale passive on top. */
  revokes?: string[];
}

export interface PetEntity {
  name: string;
  displayName: string;
  characterClass: string;
  commandable: boolean;
  copyCreatorMods: boolean;
  abilities: PetAbility[];
  /** Pet lifespan in seconds (from bundled Self_Destruct power's Silent_Kill delay).
   *  Omitted for permanent pets (mastermind primaries, etc.) that despawn only
   *  when killed or unsummoned. Used by convert-powerset to populate
   *  `summon.duration` for summoning powers whose EntCreate Duration is 0. */
  lifespan?: number;
  /** This pet detonates ONCE: it is destroyed by its own bundled
   *  Self_Destruct the moment it fires (trip mines, time bombs, seeker
   *  drones, high explosives). Its attack's recharge is therefore not a
   *  repeat cadence — damage layers must cap fires-per-spawn at 1 rather
   *  than dividing the summon window by the cycle time. */
  oneShot?: boolean;
  /** Entity defs this pet's own powers create IN PLACE (an `EntCreate` at
   *  `target: Self`), in walk order. A pet's payload can be one summon deeper —
   *  Poison Trap's gas cloud, Oil Slick's fire, the Mu Guardian's Voltaic Sentinel —
   *  so the consumers that merge a pseudo-pet's kit into its summoning power follow
   *  this chain. Each name is a key into this same record and keeps its OWN class row,
   *  commandability and lifespan, which is why the link travels instead of the child's
   *  abilities being folded into `abilities` here: pet damage resolves against
   *  `characterClass`, and Oil Slick's burn is a different class from its oil.
   *  A name with no record means the export doesn't carry that entity (NPC-only);
   *  no player-summonable pet reaches one. */
  createsEntities?: string[];
  upgradeTiers?: PetUpgradeTier[];
}

// ============================================
// INHERENT POWER RULES (per-server)
// ============================================

/**
 * Per-server overrides applied to the shared inherent powers list.
 *
 * The four Fitness powers (Swift, Hurdle, Health, Stamina) plus Brawl,
 * Sprint, and Rest are universal across servers, but availability
 * level and auto-granted enhancement slots vary:
 *
 *   - HC:      Fitness available at L1, no auto-granted slots.
 *   - Rebirth: Fitness available at L2, Health gets +1 slot at L8 / L16,
 *              Stamina +1 at L12 / L22.
 *   - Future servers will hook in here with their own variations.
 *
 * Keys are inherent power `internalName`s. Anything not listed inherits
 * the InherentPowerDef defaults baked into the shared list.
 */
export interface InherentRules {
  /**
   * `available` field override (0-indexed: -1 = L1, 0 = L1, 1 = L2, ...).
   * Use -1 for "always granted at L1" (the default for HC). Apply only
   * to powers whose grant level genuinely differs from the shared default.
   */
  availabilityOverrides: Record<string, number>;

  /**
   * Levels at which the named inherent power receives auto-granted
   * enhancement slots, in slot order. These slots come outside the
   * 67-slot user budget — they're freebies. Empty array (or absent
   * key) means no auto-grants.
   */
  autoGrantedSlotLevels: Record<string, readonly number[]>;

  /**
   * Extra archetype-gated inherents this server grants that the shared
   * hand-written list in `datasets/homecoming/levels.ts` doesn't carry, keyed
   * by archetype id. Merged on top of that list (which wins on a name clash,
   * so a server that simply also has Energy Flight doesn't get it twice).
   *
   * The case that forced this hook: Thunderspy moved the Stalker's **Hide**
   * and **Placate** out of the powersets into `Inherent.Inherent` and reused
   * the vacated powerset name slots for other powers, so both were reachable
   * from no screen at all (AUTOISSUE-2). Homecoming and Rebirth grant them
   * from powersets and so contribute nothing here.
   *
   * OPTIONAL because the two repos close AUTOISSUE-2 at different layers. A
   * dataset built for the TypeScript planner populates it from a generated
   * `archetype-inherents.ts`. The Rust engine needs no list: it admits a grant
   * by GATE SHAPE (`granted_powers::inherent_scope` keeps a member whose
   * `requires` names player archetype classes and nothing else) and dedupes
   * against the archetype catalogue's own declared inherent names, so its
   * datasets leave this absent and lose nothing.
   */
  archetypeInherents?: Record<string, readonly InherentPowerDef[]>;

  /**
   * Archetype id → the `Inherent.Inherent` full name of its HEADLINE inherent —
   * Defiance, Fury, Dark Sustenance. The NAME only: no stats, no atoms, so there is
   * nothing here that can drift into a wrong number.
   *
   * Deliberately NOT the same list as `archetypeInherents` above, which is the powers
   * that reach the build from nowhere else. A headline inherent reaches it through the
   * archetype record's own `inherent:` field, so it is absent there on purpose — and
   * `createArchetypeInherentPower` synthesises `Inherent.<Archetype>.<Name>` for it,
   * which is OUR spelling and resolves to nothing in another planner. The `.mbd`
   * writer needs the export's, and wrote no archetype inherent at all until it had one
   * (MBDEXPORT-20, 8 of 8 corpus builds, both forks).
   *
   * OPTIONAL for the same reason `archetypeInherents` is: the two repos reach this
   * name at different layers. The beta populates it from a generated map, derived by
   * `convert-archetype-inherents.cjs` from the declared inherent name and the export's
   * `@Class_` gate. Canonical leaves it absent and reads the same name off the whole
   * `Inherent.Inherent` Power in `archetypeInherentPowerset`, which the beta does not
   * carry. The two are graded against each other over all 60 archetype-fork pairs, in
   * the beta's `archetype-inherent-oracle.test.ts`.
   *
   * An archetype may be absent from a populated map: a fork that ships no single power
   * for a declared inherent leaves it out, and the caller reports rather than guessing
   * (INHERENT-10, Thunderspy's Primalist).
   */
  headlineArchetypeInherents?: Record<string, string>;
}

// ============================================
// DATASET INTERFACE
// ============================================

/**
 * Every dataset the planner ships, as a runtime value.
 *
 * A bare union can't be iterated, so anything needing the whole set had to restate it, and three
 * copies (the server picker, the `?serverId=` parser, the per-server build store) were still
 * three-dataset after Brainstorm shipped. The picker one meant Brainstorm was unselectable.
 * Derive `DatasetId` from this rather than writing the union out again.
 */
export const DATASET_IDS = ['homecoming', 'rebirth', 'thunderspy', 'brainstorm'] as const;

export type DatasetId = (typeof DATASET_IDS)[number];

/**
 * Is this string one of the ids the planner ships?
 *
 * The one home for the question, because every place that asked it inline wrote the roster out
 * again and the fourth copy (`hydrateBuild`'s serverId fallback) was still three-dataset after
 * Brainstorm shipped: a Brainstorm save re-stamped itself Homecoming on open, so the engine
 * computed the build against live while the header badge read the loaded dataset and said
 * Brainstorm. Answers about the id ALONE — whether the dataset is loaded is a different
 * question, and `getActiveDataset` owns it.
 */
export function isDatasetId(value: unknown): value is DatasetId {
  return typeof value === 'string' && (DATASET_IDS as readonly string[]).includes(value);
}

/**
 * Is this the Homecoming game, on either of its rings?
 *
 * The TypeScript twin of `DatasetId::is_homecoming()` in `crates/coh_data/src/database.rs`.
 * Homecoming and Brainstorm are one game two shards apart; Rebirth and Thunderspy are separate
 * forks. A fact that follows from WHICH GAME the bytes are — the Labyrinth accolades, the
 * suspended pool modes — holds on both rings, and `id === 'homecoming'` silently answers "no"
 * for the other one.
 */
export function isHomecomingGame(id: DatasetId): boolean {
  return id === 'homecoming' || id === 'brainstorm';
}

export interface Dataset {
  id: DatasetId;
  displayName: string;

  archetypes: {
    registry: ArchetypeRegistry;
    epicIds: readonly ArchetypeId[];
    standardIds: readonly ArchetypeId[];
  };

  atTables: {
    archetypes: Record<string, ATTableData>;
    pets: Record<string, PetTableData>;
  };

  // Purple patch — combat scaling tables for level differences (damage,
  // base ToHit). Scoped to the dataset because values are tunable per
  // server even though HC's are the de-facto standard.
  purplePatch: {
    getBaseToHit: (levelDiff: number) => number;
    getCombatModifier: (levelDiff: number) => number;
    getDefenseSoftcap: (
      levelDiff: number,
      contentMode?: 'standard' | 'incarnate',
    ) => number;
  };

  // Granted powers — parent → auto-granted children mappings (e.g.
  // Adaptation stances, Fly → Afterburner, Dual Pistols ammo swap).
  // Curated per server; sub-power names are server-specific data.
  grantedPowerGroups: Record<string, GrantedPowerGroup>;

  // Inherent power rules — per-server overrides for the universal inherent
  // powers (Swift, Hurdle, Health, Stamina, Brawl, Sprint, Rest, etc.).
  // Each server tunes Fitness differently: HC grants the four Fitness
  // powers at L1; Rebirth shifts them to L2 and adds auto-granted
  // enhancement slots at fixed levels. Future datasets (Thunderspy,
  // Unity, etc.) will plug in their own variations through this hook
  // rather than hard-coding each new wrinkle in the planner.
  inherentRules: InherentRules;

  // Pet entities — pet abilities + upgrade tiers used for pet damage
  // calculation (Mastermind summons, Voltaic Sentinel, Lore pets, etc.).
  petEntities: Record<string, PetEntity>;

  // Enhancement curves — ED thresholds, boost-type→schedule assignment,
  // per-boost-level strength curves, multi-aspect scale, relative-level
  // attenuation, and the exemplar magnitude handicaps, all generated from the
  // dataset's binary export (SOURCE-1 SW3). The enhancement engine reads these
  // through `src/data/enhancement-curves.ts`.
  enhancementCurves: EnhancementCurvesData;

  // Special-enhancement registries (Hamidon/Titan/Hydra/D-Sync/prestige),
  // generated from the dataset's boost-piece templates (SOURCE-1 item 9).
  // The forks carry no D-Sync pieces and only the classic 11 Hamidons —
  // per-dataset by necessity, read through `src/data/special-enhancements.ts`.
  specialEnhancements: SpecialEnhancementsData;

  // Raw powerset registry (INCLUDES dormant sets). Lives on the dataset — and
  // therefore in this dataset's chunk — rather than being reached by a static
  // cross-dataset import, so a bundling consumer pulls only the active server's
  // ~13-20 MB of powerset data instead of welding all three into the eager
  // entry bundle. The `powersets` facade filters dormant sets lazily per active
  // dataset; the raw is kept whole for a possible future "show unreleased sets"
  // toggle.
  powersetsRaw: Record<string, Powerset>;

  // The archetype-inherent powerset (Domination, Fury, Containment, …) as the
  // export carries it: generated by `convert-inherents.cjs`, and the same
  // literal `emit-contract.cjs` merges into the contract under id "Inherent".
  //
  // `ARCHETYPES[at].inherent` NAMES an archetype's inherent; this is the power
  // it names. Kept out of `powersetsRaw` because it is nobody's pick — no
  // archetype lists it as a primary/secondary, and the picker must not offer it.
  //
  // Optional because it states a capability, not a requirement: a repo that has
  // not generated the artifact does not carry it, and the beta has not (see
  // PARTSTAT-2's carried beta residual). `getArchetypeInherentPower` answers
  // `undefined` there and the card shows an inherent with no numbers, which is
  // the gap made visible rather than filled. Canonical's four datasets all set
  // it, and `domination-perma-classification.test.ts` reds per fork if one stops.
  archetypeInherentPowerset?: Powerset;

  // Raw (untransformed) IO-set registry. Same rationale as `powersetsRaw`, at
  // ~600 KB per server. The `io-sets` facade transforms it to the runtime
  // `IOSetRegistry` lazily.
  ioSetsRaw: LegacyIOSetRegistry;

  // Mids Reborn's enhancement-UID namespace for this server, generated from the
  // matching EnhDB.mhd. Only the .mbd export path reads it; see
  // `src/data/mids-uids.ts` for why the UIDs are read rather than derived.
  midsUids?: MidsUidTable;

  // Raw epic-pool registry. Same rationale again. The per-dataset generated
  // literal is structurally looser than the facade's `LegacyEpicPoolRegistry`,
  // so each dataset asserts the type at assignment (the cast that formerly
  // lived in the epic-pools facade).
  epicPoolsRaw: LegacyEpicPoolRegistry;

  // Raw incarnate effect tables (alpha/destiny/hybrid/interface/judgement/lore/
  // genesis + destiny timeline & boosts) for this server. Same rationale. The
  // `incarnate-effects` facade reads each slot on demand — including the
  // decision that a dormant slot is served as `{}` rather than as the
  // placeholder table the export carries, which is now stated by each dataset's
  // own index rather than by a fork branch in the facade.
  incarnateEffectsRaw: IncarnateEffectsRaw;

  // Raw power-pool registry (INCLUDES dormant pools). Same rationale/shape as
  // `epicPoolsRaw`; the `power-pools` facade drops dormant pools and transforms
  // lazily per active dataset. The generated literal is structurally looser than
  // `LegacyPowerPoolRegistry`, so each dataset asserts the type at assignment.
  powerPoolsRaw: LegacyPowerPoolRegistry;

  // Helpers closed over this dataset's own data records.
  getTableValue: (archetype: string, tableName: string, level: number) => number | undefined;
  calculateEffectValue: (archetype: string, tableName: string, scale: number, level?: number) => number | undefined;
  calculateIncarnateDamage: (scale: number, tableName: string, archetype: string, level?: number) => number | null;
  getPetTableValue: (petClass: string, tableName: string, level: number) => number | undefined;

  getArchetype: (id: ArchetypeId) => Archetype | undefined;
}

// ============================================
// ACTIVE DATASET STATE
// ============================================

let active: Dataset | null = null;
const cache = new Map<DatasetId, Promise<Dataset>>();

/**
 * Synchronous accessor used by every data-layer facade. Throws if no
 * dataset has been loaded yet — app boot must `await loadDataset(...)`
 * before mounting the React tree.
 */
export function getActiveDataset(): Dataset {
  if (!active) {
    throw new Error(
      'No dataset loaded. Call `await loadDataset(id)` before any data access ' +
      '(e.g. before mounting the React tree).',
    );
  }
  return active;
}

/**
 * Sanity-check archetype data for known footgun patterns. Runs once per
 * dataset on first load. Throws (in dev) or warns (in prod) so the bug
 * surfaces early rather than as a baffling user-facing symptom.
 *
 * Currently checks:
 * - **Branch primary === secondary**: a branch whose `secondarySet` ID
 *   equals its `primarySet` is always a bug. It surfaces in the UI as
 *   "the secondary picker shows primary powers" — exactly what
 *   triggered the 2026-05-05 Rebirth Arachnos Soldier bug
 *   ([REBIRTH_ARACHNOS_SOLDIER_BUG.md](../../REBIRTH_ARACHNOS_SOLDIER_BUG.md)).
 *   Almost always means the secondary powerset wasn't extracted and
 *   was filled in with placeholder data.
 * - **Enhancement-curves dataset id**: the generated curves module stamps
 *   its dataset id; a mismatch means the index wired another dataset's
 *   module (copy-paste hazard across the three near-identical index files).
 */
function validateDataset(ds: Dataset): void {
  const errors: string[] = [];
  if (ds.enhancementCurves.dataset !== ds.id) {
    errors.push(
      `enhancementCurves carries dataset id "${ds.enhancementCurves.dataset}" — the index ` +
      `wired another dataset's generated module.`,
    );
  }
  if (ds.specialEnhancements.dataset !== ds.id) {
    errors.push(
      `specialEnhancements carries dataset id "${ds.specialEnhancements.dataset}" — the index ` +
      `wired another dataset's generated module.`,
    );
  }
  for (const [atId, at] of Object.entries(ds.archetypes.registry)) {
    if (!at.branches) continue;
    for (const [branchId, branch] of Object.entries(at.branches)) {
      if (!branch) continue;
      if (branch.primarySet === branch.secondarySet) {
        errors.push(
          `Archetype "${atId}" branch "${branchId}" has primarySet === secondarySet ` +
          `("${branch.primarySet}"). This is always a bug — the secondary powerset ` +
          `is likely missing from extracted data.`,
        );
      }
    }
  }
  if (errors.length === 0) return;
  const message = `Dataset "${ds.id}" failed validation:\n  - ${errors.join('\n  - ')}`;
  if (import.meta.env?.DEV) {
    throw new Error(message);
  }
  // In prod, log loudly but don't crash the app — the user can still
  // use ATs that aren't affected.
  console.error(message);
}

/**
 * Validate an already-constructed dataset and set it active, synchronously.
 * The app goes through `loadDataset`; this entry exists for Node tooling
 * (fixture emitters) that loads dataset modules through a CJS require hook,
 * where `loadDataset`'s dynamic import cannot resolve.
 */
export function activateDataset(ds: Dataset): Dataset {
  validateDataset(ds);
  active = ds;
  return ds;
}

/**
 * Lazy-loads a dataset (each lives in its own chunk via dynamic import)
 * and sets it as active. Idempotent per id — repeated calls return the
 * cached promise.
 */
export async function loadDataset(id: DatasetId): Promise<Dataset> {
  let promise = cache.get(id);
  if (!promise) {
    promise = (async () => {
      switch (id) {
        case 'homecoming':
          return (await import('./datasets/homecoming')).default;
        case 'rebirth':
          return (await import('./datasets/rebirth')).default;
        case 'thunderspy':
          return (await import('./datasets/thunderspy')).default;
        case 'brainstorm':
          return (await import('./datasets/brainstorm')).default;
        default: {
          const _exhaustive: never = id;
          throw new Error(`Unknown dataset: ${_exhaustive}`);
        }
      }
    })();
    cache.set(id, promise);
  }
  const ds = await promise;
  return activateDataset(ds);
}

/**
 * Static metadata for all known datasets. Does not force any dataset to
 * load — safe to call from boot/UI code that needs to render a picker
 * before committing to a dataset.
 */
export function getAllDatasetMetadata(): Array<{ id: DatasetId; displayName: string }> {
  return DATASET_IDS.map((id) => ({ id, displayName: DATASET_DISPLAY_NAMES[id] }));
}

/** Typed `Record<DatasetId, …>`, so a new dataset fails the build until it's named here. */
const DATASET_DISPLAY_NAMES: Record<DatasetId, string> = {
  homecoming: 'Homecoming',
  rebirth: 'Rebirth',
  thunderspy: 'Thunderspy',
  // Named for the server, so a user reading the picker knows which shard they're
  // planning against rather than only that it's "beta".
  brainstorm: 'HC Brainstorm',
};
