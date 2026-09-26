/**
 * SPIKE4 — betaBuild → CharacterState adapter.
 *
 * Translates the beta's build model (`Build` from buildStore) plus the calc-context
 * inputs (uiStore options + `incarnateActive`) into the rebuild engine's
 * `CharacterState` wire shape, which `coh_wasm.recalculate(build_json)` deserializes.
 *
 * This is a DATA-SHAPE TRANSLATION, not a calc. It never derives a stat, set bonus, ED
 * result, or level curve — the engine owns all of that. Every field lands in one of the
 * five SPIKE3 buckets (map / rename / restructure / drop / fail-loud); see
 * docs/SPIKE3-CONTRACT-DIFF.md in the rebuild repo for the full table.
 *
 * Fail-loud (the rebuild mandate): inputs the engine has no equivalent for do NOT get
 * silently dropped. `destinyTime` passes through to `combat.destiny_time` (the engine scrubs
 * the diminishing Destiny timeline itself; null → its sustained-floor default). A disabled
 * `incarnateLevelShift` passes through to `combat.incarnate_level_shift` as a CEILING the engine
 * spends down the loadout's earned shifts (null → all of them). It used to warn and drop, because
 * the engine applied every shift unconditionally; LSHIFT-1 gave it the ceiling. Conditional adjusters (`globalAdjusters` /
 * `mechanicAdjusters`) reach the engine two ways: for the per-power DISPLAY projection they are
 * forwarded as `combat.global_conditionals` / `per_power_conditionals` / `power_state` — the last
 * two a partition of the beta's one mechanic map, re-addressed per pick (PROD6C-3k), and for
 * the TOTALS they ride build state the engine already reads — stances via `active_sub_power`,
 * out-of-combat via `combatMode`→`in_combat`. Only the totals half can silently drop a toggle, so
 * that is where this still fails loud: a global toggle carrying an unmodeled caster dashboard buff
 * (see the PROD3 classification below). A silent drop here would be exactly the field-loss class
 * the rebuild exists to kill.
 */

import type { Build, PowersetSelection, PoolSelection, ProcOverride } from '@/types/build';
import type { SelectedPower, ConditionalEffect } from '@/types/power';
import type { Enhancement } from '@/types/enhancement';
import type { IncarnateActiveState } from '@/types/incarnate';
import type { DatasetId } from '@/data/dataset';
import { getPowerset } from '@/data/powersets';
import { getPowerPool } from '@/data/power-pools';
import { getEpicPool } from '@/data/epic-pools';
import { effectiveGlobalAdjusters, isCasterHidden } from '@/components/info/resolveEffectivePower';
import { BUFF_PET_TOGGLE_ID } from '@/utils/calculations/buff-pet-auras';

// ============================================
// OUTPUT WIRE SHAPE (mirrors crates/coh_data/src/character.rs serde)
// ============================================

/** `EnhancementKind` — the flattened, `type`-tagged payload half of a slot. */
export type CharacterStateEnhancementKind =
  | { type: 'io-set'; set_id: string; set_name: string; piece_num: number; aspects: string[]; is_proc: boolean; is_unique: boolean }
  | { type: 'io-generic'; stat: string; value: number }
  | { type: 'special'; category: string; aspects: { stat: string; value: number }[] }
  | { type: 'origin'; tier: string; origin: string | null; secondary_origin: string | null; stat: string; value: number };

export type CharacterStateEnhancement = {
  id: string;
  name: string;
  icon: string;
  level: number | null;
  attuned: boolean;
  /**
   * Stored level offset — SIGNED. Booster combines on the IO kinds, relative
   * level on origin/special, where a negative is an under-level enhancement.
   * The Rust side is `i8` for this reason; it was `u8`, which could not carry
   * the negative half across the wire at all.
   */
  boost: number;
} & CharacterStateEnhancementKind;

export interface CharacterStateSelectedPower {
  internal_name: string;
  power_set: string;
  level: number;
  slots: (CharacterStateEnhancement | null)[];
  is_active: boolean;
  active_sub_power: string | null;
  inherent_slot_count: number;
  is_locked: boolean;
  inherent_category: 'fitness' | 'basic' | 'prestige' | 'archetype' | null;
  targets_hit: number | null;
}

export interface CharacterStatePowerset {
  id: string | null;
  name: string;
  powers: CharacterStateSelectedPower[];
}

export interface CharacterStatePool {
  id: string;
  name: string;
  powers: CharacterStateSelectedPower[];
}

export interface CharacterStateIncarnateSlot {
  power_name: string;
  active: boolean;
}

export interface CharacterStateIncarnateLoadout {
  alpha: CharacterStateIncarnateSlot | null;
  judgement: CharacterStateIncarnateSlot | null;
  interface: CharacterStateIncarnateSlot | null;
  destiny: CharacterStateIncarnateSlot | null;
  lore: CharacterStateIncarnateSlot | null;
  hybrid: CharacterStateIncarnateSlot | null;
  genesis: CharacterStateIncarnateSlot | null;
}

export interface CharacterStateSlotOrderEntry {
  power_name: string;
  slot_index: number;
  category: string | null;
  level: number | null;
}

export interface CharacterStateCombatContext {
  in_combat: boolean;
  enemy_level_offset: number;
  fury_level: number;
  vigilance_team_size: number;
  hit_points_percent: number;
  exemplar_level: number | null;
  destiny_time: number | null;
  /** How many foes are inside an active Melee Hybrid's sphere. `null` reads as none, which
   *  applies nothing — the behaviour the pass had before the layer was wired. */
  hybrid_targets_hit: number | null;
  /** The incarnate level-shift ceiling: how many of the loadout's EARNED shifts to spend.
   *  `null` = all of them. The engine spends it down the grants (Alpha first) rather than
   *  clamping the sum, so each slot's breakdown row still states what it contributed. */
  incarnate_level_shift: number | null;
  hidden: boolean;
  global_conditionals: Record<string, boolean>;
  per_power_conditionals: Record<string, boolean>;
  /** Per-power switches the player threw on their own character, keyed by the pick's ADDRESS
   *  (`"<powerset>:<internalName>:<id>"`). The engine keeps these apart from the target-state
   *  map above because the two answer to opposite rules at the file boundary; see
   *  `partitionMechanicAdjusters`. */
  power_state: Record<string, boolean>;
  active_modes: string[];
  /** The what-if TEAM-BUFF layer, keyed by the `GlobalBonuses` field each entry lands in. The
   *  engine injects it into the accumulators before projection — the one live input here that
   *  moves the totals rather than choosing which power a surface describes. */
  what_if_buffs: Record<string, number>;
}

export interface CharacterState {
  name: string;
  dataset: DatasetId;
  archetype: { id: string | null; name: string };
  level: number;
  primary: CharacterStatePowerset;
  secondary: CharacterStatePowerset;
  pools: CharacterStatePool[];
  epic_pool: CharacterStatePool | null;
  inherents: CharacterStateSelectedPower[];
  accolades: string[];
  incarnates: CharacterStateIncarnateLoadout;
  /** Per-slotted-proc control overrides, keyed by the pick's ADDRESS plus the slot index
   *  (`"<powerset>:<internalName>:<slot index>"`), so the engine's proc pass honours the
   *  "Slotted Procs" enable / stacks / %HP controls instead of always running the defaults.
   *  The build stores them under the power's DISPLAY name; `procOverridesFrom` re-addresses
   *  them here. */
  proc_overrides: Record<string, ProcOverride>;
  slot_order: CharacterStateSlotOrderEntry[];
  combat: CharacterStateCombatContext;
}

// ============================================
// INPUT: the uiStore-derived calc context (the options bag + incarnateActive)
// ============================================

/**
 * The subset of uiStore the engine consumes, gathered exactly as
 * `useCharacterCalculation` gathers it. Names/defaults match uiStore. `procSettings`
 * is intentionally omitted: it gates per-power proc DPS (engine M5, out of the totals
 * slice); proc set-bonus globals (LotG +Recharge, …) are always-on in the engine.
 */
export interface AdapterCalcContext {
  exemplarMode: boolean;
  exemplarLevel: number;
  incarnateActive: IncarnateActiveState;
  /** How many of the loadout's earned incarnate level shifts to read the build with; `null` =
   *  all of them. A ceiling, not a magnitude — the engine spends it down the earned grants. */
  incarnateLevelShift: number | null;
  targetsHitValues: Record<string, number>;
  targetLevelOffset: number;
  vigilanceTeamSize: number;
  furyLevel: number;
  combatMode: boolean;
  destinyTime: number | null;
  /** Foes inside the Melee Hybrid's sphere, 0-N by tier. A live preview input like
   *  `destinyTime`, never persisted with the build. */
  hybridTargetsHit: number | null;
  globalAdjusters: Record<string, boolean>;
  mechanicAdjusters: Record<string, boolean>;
  /** Header Domination toggle — an AT-inherent mechanic the conditional `domination` id reads
   *  instead of `globalAdjusters` (PROD6C-3k). */
  dominationActive: boolean;
  /** Header alpha-strike toggle. Combined with the build's Hide power into the engine's
   *  `hidden`, which gates the mid-combat cast. */
  stalkerHidden: boolean;
  /** The what-if TEAM-BUFF layer: how much of each stat to pretend a teammate is handing the
   *  build, keyed by the `GlobalBonuses` field name the buff lands in (`damage`, `toHit`,
   *  `recharge`, …). A live preview input like `destinyTime`, never persisted with the build —
   *  a shared build carrying a hidden +damage would read as the build's own number. The engine
   *  injects it into the accumulators before projection, so every archetype ceiling binds
   *  against it and the fast-snipe ToHit threshold sees it. */
  whatIfBuffs: Record<string, number>;
}

const INCARNATE_SLOTS = ['alpha', 'judgement', 'interface', 'destiny', 'lore', 'hybrid', 'genesis'] as const;

// ============================================
// MAPPERS
// ============================================

function mapEnhancement(e: Enhancement): CharacterStateEnhancement {
  const base = {
    id: e.id,
    name: e.name,
    icon: e.icon ?? '',
    level: e.level ?? null,
    attuned: e.attuned ?? false,
    boost: e.boost ?? 0,
  };
  switch (e.type) {
    case 'io-set':
      return { ...base, type: 'io-set', set_id: e.setId, set_name: e.setName, piece_num: e.pieceNum, aspects: e.aspects, is_proc: e.isProc, is_unique: e.isUnique };
    case 'io-generic':
      return { ...base, type: 'io-generic', stat: e.stat, value: e.value ?? 0 };
    case 'special':
      return { ...base, type: 'special', category: e.category, aspects: e.aspects.map((a) => ({ stat: a.stat, value: a.value })) };
    case 'origin':
      return { ...base, type: 'origin', tier: e.tier, origin: e.origin ?? null, secondary_origin: e.secondaryOrigin ?? null, stat: e.stat, value: e.value ?? 0 };
    default: {
      // Exhaustiveness guard — a new enhancement kind must fail loud here, not drop.
      const never: never = e;
      throw new Error(`characterStateAdapter: unmapped enhancement kind ${JSON.stringify(never)}`);
    }
  }
}

/**
 * What `inherent_category` MEANS to the engine, which is not what it means to the planner's
 * inherents panel.
 *
 * The engine reads `'archetype'` as "this power's contribution is derived by its own
 * combat-context pass; skip it in the ordinary gather" (`coh_math::gather`, the Vigilance/Fury
 * skip) — a statement about the calculation. The planner's `inherentCategory` is a DISPLAY group,
 * and eleven powers claim `'archetype'` to land in the expanded "<AT> Inherent" section instead of
 * the collapsed Basic one.
 *
 * Forwarding one as the other made the engine skip all eleven, so they reached no total at all:
 * Peacebringer Energy/Combat Flight and Warshade Shadow Step/Recall on every fork, and on
 * Thunderspy the Stalker's Hide and Placate, the Mastermind's Hold Ground, Group Energy Flight,
 * Quantum Acceleration, Starless Step and Shadow Slip. Hide's toggle moved no defense whatever the
 * user did with it, which is the report that found this.
 *
 * So the engine is told `'archetype'` only for the power that actually is one — the archetype's own
 * declared mechanic, marked `derivedMechanic` where it is built. Everything else in that display
 * group travels as the ordinary granted power it is. `null` rather than a substitute category:
 * the engine uses this field for nothing but the skip, and inventing a group would be claiming
 * something about a power the planner has not been told.
 */
function engineInherentCategory(p: SelectedPower): NonNullable<SelectedPower['inherentCategory']> | null {
  if (p.derivedMechanic) return 'archetype';
  return p.inherentCategory === 'archetype' ? null : p.inherentCategory ?? null;
}

function mapPower(p: SelectedPower, targetsHitValues: Record<string, number>): CharacterStateSelectedPower {
  return {
    internal_name: p.internalName,
    power_set: p.powerSet,
    level: p.level,
    slots: p.slots.map((slot) => (slot ? mapEnhancement(slot) : null)),
    is_active: p.isActive ?? false,
    active_sub_power: p.activeSubPower ?? null,
    inherent_slot_count: p.inherentSlotCount ?? 0,
    is_locked: p.isLocked ?? false,
    inherent_category: engineInherentCategory(p),
    // targetsHitValues is keyed by internalName (NOT unique across the build); a value
    // distributes to every pick with that name, inheriting the beta's own limitation.
    targets_hit: p.internalName in targetsHitValues ? targetsHitValues[p.internalName] : null,
  };
}

function mapPowerset(s: PowersetSelection, targetsHitValues: Record<string, number>): CharacterStatePowerset {
  return { id: s.id, name: s.name, powers: s.powers.map((p) => mapPower(p, targetsHitValues)) };
}

function mapPool(p: PoolSelection, targetsHitValues: Record<string, number>): CharacterStatePool {
  return { id: p.id, name: p.name, powers: p.powers.map((pw) => mapPower(pw, targetsHitValues)) };
}

function mapIncarnates(build: Build, active: IncarnateActiveState): CharacterStateIncarnateLoadout {
  const out = {} as CharacterStateIncarnateLoadout;
  for (const slot of INCARNATE_SLOTS) {
    const sel = build.incarnates[slot];
    out[slot] = sel ? { power_name: sel.powerName, active: active[slot] ?? false } : null;
  }
  return out;
}

// ============================================
// PER-PICK ADDRESSING
// ============================================

/**
 * The engine addresses one pick as `<powerset>:<internalName>` (`coh_data::power_address`),
 * because an internal name alone cannot tell an epic pick from its secondary twin — an
 * override set on one would land on both. Two of the maps below travel to the engine under
 * that address while the beta still stores them under a name, so they are re-addressed here.
 *
 * This is the whole reason a mismatch is dangerous rather than noisy: a key the engine cannot
 * find reads as ABSENT, and absent means "the player never threw this switch". A stale key
 * shape does not fail, it silently reverts every one of these controls to its default.
 */
function powerAddress(powerSet: string, internalName: string): string {
  return `${powerSet}:${internalName}`;
}

/** Every pick the engine's own `all_selected()` walks, in the same order. */
function allPicks(build: Build): SelectedPower[] {
  return [
    ...build.primary.powers,
    ...build.secondary.powers,
    ...build.pools.flatMap((pool) => pool.powers),
    ...(build.epicPool?.powers ?? []),
    ...build.inherents,
  ];
}

/**
 * Caster-side toggle ids that belong in `power_state` rather than `per_power_conditionals`.
 *
 * The engine splits the beta's one flat `mechanicAdjusters` map in two, and the split is not
 * cosmetic: a switch on your own character travels with a shared build, while a foe's state
 * (drowning, contaminated) is world state that must open at the planner's default. One map
 * cannot say which a key is. The beta writes both halves into `mechanicAdjusters`, so the
 * partition happens here — and a caster toggle added later must be listed here too, or it
 * lands in the target-state map and does nothing.
 */
const CASTER_POWER_STATE_TOGGLE_IDS: ReadonlySet<string> = new Set([BUFF_PET_TOGGLE_ID]);

/** Split `mechanicAdjusters` into the target-state half and the caster half, re-addressing
 *  the caster half onto the picks it names. An entry naming no pick in this build is dropped:
 *  the engine has no power to hang it on either. */
function partitionMechanicAdjusters(
  build: Build,
  mechanicAdjusters: Record<string, boolean>,
): { perPower: Record<string, boolean>; powerState: Record<string, boolean> } {
  const perPower: Record<string, boolean> = {};
  const powerState: Record<string, boolean> = {};
  const picksByInternalName = new Map<string, SelectedPower[]>();
  for (const pick of allPicks(build)) {
    const list = picksByInternalName.get(pick.internalName);
    if (list) list.push(pick);
    else picksByInternalName.set(pick.internalName, [pick]);
  }

  for (const [key, on] of Object.entries(mechanicAdjusters)) {
    const split = key.lastIndexOf(':');
    const id = split === -1 ? '' : key.slice(split + 1);
    if (!CASTER_POWER_STATE_TOGGLE_IDS.has(id)) {
      perPower[key] = on;
      continue;
    }
    // The beta keys on the internal name alone, so a name held twice sets both picks —
    // the same within-build collision the address exists to fix, carried over faithfully
    // rather than resolved here on a guess about which pick the player meant.
    for (const pick of picksByInternalName.get(key.slice(0, split)) ?? []) {
      powerState[`${powerAddress(pick.powerSet, pick.internalName)}:${id}`] = on;
    }
  }
  return { perPower, powerState };
}

/** Re-address the build's `procOverrides` (stored `"<display name>:<slot index>"`) onto the
 *  picks they name. Same faithful-collision rule as above: a display name held by two picks
 *  sets both, which is what the beta's own controls already do. */
function procOverridesFrom(build: Build): Record<string, ProcOverride> {
  const stored = build.procOverrides;
  if (!stored) return {};
  const picksByName = new Map<string, SelectedPower[]>();
  for (const pick of allPicks(build)) {
    const list = picksByName.get(pick.name);
    if (list) list.push(pick);
    else picksByName.set(pick.name, [pick]);
  }

  const out: Record<string, ProcOverride> = {};
  for (const [key, override] of Object.entries(stored)) {
    const split = key.lastIndexOf(':');
    if (split === -1) continue;
    const slotIndex = key.slice(split + 1);
    for (const pick of picksByName.get(key.slice(0, split)) ?? []) {
      out[`${powerAddress(pick.powerSet, pick.internalName)}:${slotIndex}`] = override;
    }
  }
  return out;
}

// ============================================
// CONDITIONAL-ADJUSTER CLASSIFICATION (PROD3)
// ============================================

/**
 * `ActivePowerEffect` keys that land on a CASTER dashboard total (defense, resistance, +damage,
 * movement, …) — as opposed to foe-facing control (hold/stun/…) or attack-damage entries, which
 * never move a Self total. A conditional whose `effects` carry one of these reaches the dashboard.
 */
const SELF_TOTAL_EFFECT_KEYS: ReadonlySet<string> = new Set([
  'tohitBuff', 'tohitBuffUnenhanced', 'accuracyBuff', 'damageBuff', 'rechargeBuff',
  'defense', 'defenseBuff', 'defenseBuffSuppressible', 'resistance', 'debuffResistance',
  'mezResistance', 'elusivity', 'absorb', 'protection', 'enduranceDiscount',
  'runSpeed', 'runSpeedUnenhanced', 'flySpeed', 'jumpHeight', 'jumpSpeed', 'movement',
  'regeneration', 'recovery', 'maxEndurance', 'maxHealth',
  'regenBuff', 'regenBuffUnenhanced', 'recoveryBuff', 'recoveryBuffUnenhanced',
  'maxHPBuff', 'maxHPBuffUnenhanced', 'maxEndBuff',
]);

/**
 * Global conditional-adjuster ids whose caster-side buff the engine already accounts for, so
 * forwarding the beta toggle changes no total and must not fail loud:
 *  - the Bio Armor adaptation stances flow via `active_sub_power`, and the engine re-admits their
 *    `Source.Mode?`-gated atoms (PROD3 increment 1);
 *  - out-of-combat rides `combatMode` → `in_combat` — the engine binds `kOutOfCombat` to `!in_combat`,
 *    the same signal that governs the suppressible out-of-combat defense;
 *  - the Primalist form stances ride the toggled form power's own `setsModes`, which is what
 *    `collect_source_modes` binds `kHunterMode` / `kProwlerMode` from. Thunderspy published no mode
 *    at all until TSPY-4 fixed the parser, which is why these two used to fail loud here.
 */
const ENGINE_MODELED_ADJUSTERS: ReadonlySet<string> = new Set([
  'defensiveadaptation', 'offensiveadaptation', 'restedadaptation',
  'outofcombat',
  'huntermode', 'prowlermode',
]);

/**
 * Global conditional-adjuster ids that carry a caster-side buff but gate a COMBAT-TRANSIENT rotation
 * state (Storm's clear skies, Dual Pistols ammo / status mode, Staff perfection, Storm Blast's
 * in-cell): a mid-combo pulse, not a sustained total. The engine leaves them Indeterminate and omits
 * them, so the dashboard shows the sustained value and the toggle is a no-op — the deliberate reading
 * of a totals dashboard, not a silent drop of a persistent buff.
 *
 * The last two are thunderspy's, and their data says the same thing in a different idiom: both gate
 * on `Temporary_Powers.… source.ownPower?` — a temp power a prior attack grants and the next
 * consumes — rather than on a mode. A build does not hold that temp, so it is rotation state.
 */
const TRANSIENT_UNMODELED_ADJUSTERS: ReadonlySet<string> = new Set([
  'clearskies', 'cryoammunition', 'dd_statusmode_2',
  'perfection_of_body_level_3', 'perfection_of_mind_level_3', 'stormblast_instormcell',
  'bulletcut', 'pale_self_buff_plaguebearer',
]);

/** Every `conditionalEffects` entry the build's selected powers carry, indexed by conditional id. */
function buildConditionalsById(build: Build): Map<string, ConditionalEffect[]> {
  const byId = new Map<string, ConditionalEffect[]>();
  const add = (conds?: ConditionalEffect[]) => {
    for (const c of conds ?? []) {
      const list = byId.get(c.id);
      if (list) list.push(c);
      else byId.set(c.id, [c]);
    }
  };
  const fromSet = (
    setId: string | null | undefined,
    powers: { internalName: string }[],
    lookup: (id: string) => { powers: { internalName: string; conditionalEffects?: ConditionalEffect[] }[] } | undefined,
  ) => {
    if (!setId) return;
    const def = lookup(setId);
    if (!def) return;
    for (const pick of powers) add(def.powers.find((p) => p.internalName === pick.internalName)?.conditionalEffects);
  };
  fromSet(build.primary.id, build.primary.powers, getPowerset);
  fromSet(build.secondary.id, build.secondary.powers, getPowerset);
  for (const pool of build.pools) fromSet(pool.id, pool.powers, getPowerPool);
  if (build.epicPool) fromSet(build.epicPool.id, build.epicPool.powers, getEpicPool);
  // Inherents carry their own hydrated conditionalEffects (no powerset def to consult).
  for (const p of build.inherents) add((p as { conditionalEffects?: ConditionalEffect[] }).conditionalEffects);
  return byId;
}

/** True when any `conditionalEffects` entry with this id contributes a caster dashboard total. */
function adjusterAffectsSelfTotals(byId: Map<string, ConditionalEffect[]>, id: string): boolean {
  return (byId.get(id) ?? []).some((c) => {
    const effects = (c as { effects?: Record<string, unknown> }).effects;
    return !!effects && Object.keys(effects).some((k) => SELF_TOTAL_EFFECT_KEYS.has(k));
  });
}

// ============================================
// MAIN ADAPTER
// ============================================

/**
 * Build a `CharacterState` from the beta build + calc context. Throws on any input the
 * engine cannot honor (the fail-loud rows of the SPIKE3 table).
 */
export function toCharacterState(build: Build, ctx: AdapterCalcContext): CharacterState {
  // --- Conditional adjusters (PROD3): the engine models them through build state, not these maps. ---
  // Stance modes flow via `active_sub_power`; out-of-combat rides `combatMode` → `in_combat`. Every
  // other active toggle is either target-state (mechanicAdjusters — per-power scope is foe-side, zero
  // Self total) or a combat-transient caster buff the engine deliberately omits from a sustained-totals
  // dashboard. So forwarding them changes no total and must NOT throw. The guard stays fail-loud for the
  // one case that WOULD be a silent drop: a global toggle carrying a caster dashboard buff that is
  // neither modeled nor a known transient — i.e. a future data drop added a totals-relevant conditional
  // without a decision here. (The mechanic map is target-side by construction, so it is not scanned.)
  const unclassifiedActive = Object.entries(ctx.globalAdjusters)
    .filter(([id, on]) => on && !ENGINE_MODELED_ADJUSTERS.has(id) && !TRANSIENT_UNMODELED_ADJUSTERS.has(id))
    .map(([id]) => id);
  if (unclassifiedActive.length > 0) {
    // Only an unknown active toggle reaches the powerset defs — the common no-adjuster path never does.
    const conditionalsById = buildConditionalsById(build);
    const unclassifiedTotalsAdjusters = unclassifiedActive.filter((id) => adjusterAffectsSelfTotals(conditionalsById, id));
    if (unclassifiedTotalsAdjusters.length > 0) {
      throw new Error(
        `characterStateAdapter: global adjuster(s) carry a caster dashboard buff the engine does not model and would be silently dropped: ${unclassifiedTotalsAdjusters.join(', ')} — classify in ENGINE_MODELED_ADJUSTERS or TRANSIENT_UNMODELED_ADJUSTERS (or model in the engine) before forwarding`,
      );
    }
  }
  const mechanics = partitionMechanicAdjusters(build, ctx.mechanicAdjusters);

  return {
    name: build.name,
    dataset: build.serverId,
    archetype: { id: build.archetype.id, name: build.archetype.name },
    level: build.level,
    primary: mapPowerset(build.primary, ctx.targetsHitValues),
    secondary: mapPowerset(build.secondary, ctx.targetsHitValues),
    pools: build.pools.map((p) => mapPool(p, ctx.targetsHitValues)),
    epic_pool: build.epicPool ? mapPool(build.epicPool, ctx.targetsHitValues) : null,
    inherents: build.inherents.map((p) => mapPower(p, ctx.targetsHitValues)),
    accolades: [...build.accolades],
    incarnates: mapIncarnates(build, ctx.incarnateActive),
    proc_overrides: procOverridesFrom(build),
    slot_order: build.slotOrder.map((s) => ({
      power_name: s.powerName,
      slot_index: s.slotIndex,
      category: s.category ?? null,
      level: s.level ?? null,
    })),
    combat: {
      in_combat: ctx.combatMode,
      enemy_level_offset: ctx.targetLevelOffset,
      fury_level: ctx.furyLevel,
      // Beta vigilanceTeamSize = teammate count (0 = solo); engine vigilance_team_size =
      // team SIZE (1 = solo, and it rejects 0). See the beta calc + rebuild inherents test.
      vigilance_team_size: ctx.vigilanceTeamSize + 1,
      // Not fed by the beta totals path; the engine defaults it to full health.
      hit_points_percent: 100,
      // (mode + level) → Option: only exemplared when the mode is on.
      exemplar_level: ctx.exemplarMode ? ctx.exemplarLevel : null,
      // Destiny-time scrub: null → engine resolves at the sustained-floor time (the perma
      // default); a number → that exact second. The engine reads the same timeline the beta's
      // getDestinyEffectsAtTime does, so null↔None and n↔Some(n) match the beta path exactly.
      destiny_time: ctx.destinyTime,
      // Melee Hybrid foe count: null → no foes, so the per-foe layer contributes nothing and
      // the totals read as they did while that layer was cut. The engine clamps the number to
      // the equipped tier's own ceiling, so a stale value from a bigger tier cannot overstate
      // a smaller one.
      hybrid_targets_hit: ctx.hybridTargetsHit,
      // Level-shift ceiling: null → the engine spends every shift the loadout earned (the
      // prior fixed behaviour), a number → it spends that much down the earned grants,
      // Alpha first. It cannot exceed them, so this never conjures a shift the build has
      // not earned. Forwarded rather than warned since LSHIFT-1.
      incarnate_level_shift: ctx.incarnateLevelShift,
      // The three DISPLAY-side inputs (PROD6C-3k): they resolve the effective power each
      // per-power projection describes and change no dashboard total, which is why forwarding
      // the toggle maps here does not disturb the classification above.
      hidden: isCasterHidden(build, ctx.stalkerHidden),
      // Stances win over stale UI toggles (the same overlay the surfaces and this calc apply),
      // and the one AT-inherent id the beta routes to the Header's own state is resolved into
      // the map here — the engine reads the data's `scope`, not a curated list of mechanic names.
      global_conditionals: { ...effectiveGlobalAdjusters(build, ctx.globalAdjusters), domination: ctx.dominationActive },
      per_power_conditionals: mechanics.perPower,
      // The caster half of the same map, addressed per pick — the buff-pet aura opt-ins.
      power_state: mechanics.powerState,
      // The caster modes the player switched on. Display only: the engine swaps the redirected
      // record into the projection, and the TOTALS pass reads the same states through the
      // powers' own `setsModes` gates, so no total moves (PROD6C-3l).
      active_modes: build.activeModes ?? [],
      // The what-if team-buff layer. Passed as a live input like `destiny_time`: the engine
      // strips it at its own persistence boundary, so nothing here can leak it into a saved
      // or shared build.
      what_if_buffs: ctx.whatIfBuffs,
    },
  };
}

/** Convenience: `toCharacterState` serialized to the JSON string `coh_wasm.recalculate` takes. */
export function toCharacterStateJson(build: Build, ctx: AdapterCalcContext): string {
  return JSON.stringify(toCharacterState(build, ctx));
}
