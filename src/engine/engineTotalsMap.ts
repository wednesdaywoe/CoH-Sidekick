/**
 * SPIKE5 — pure engine→beta key mapping (no engine/wasm import, so it runs anywhere:
 * browser, the totals hook, and the SPIKE6 Node parity harness). Reshapes the engine's
 * snake_case `CalculatedTotals` to the beta camelCase `CharacterStats` / `GlobalBonuses`.
 * See engineTotals.ts for the notes on movement (a GlobalBonuses field in the engine) and
 * the beta-only fields with no engine source (`threatLevel`, `protRepel`, …).
 */

import type { CharacterStats, BonusTracking, ValueTracking, StatSource, DashboardStatBreakdown } from '@/utils/calculations';
import type { GlobalBonuses } from '@/utils/calculations/character-totals';
import type { ThreeTierValues } from '@/components/info/powerDisplayUtils';
import type { PermaInfo } from '@/utils/calculations/perma';
import type { EnhancementBonuses } from '@/utils/calculations/enhancement-values';

export interface EngineStats {
  damage: number; accuracy: number; to_hit: number; recharge: number; endurance_reduction: number;
  def_melee: number; def_ranged: number; def_aoe: number;
  def_sl: number; def_fc: number; def_en: number; def_psionic: number; def_toxic: number;
  res_sl: number; res_fc: number; res_en: number; res_psionic: number; res_toxic: number;
  recovery: number; regeneration: number; max_hp: number; max_end: number;
  /** Absorb in absolute HP, already clamped to the archetype's per-level AttribMaxMax ceiling. */
  absorb: number;
  debuff_resist_slow: number; debuff_resist_defense: number; debuff_resist_recharge: number; debuff_resist_endurance: number;
  debuff_resist_recovery: number; debuff_resist_to_hit: number; debuff_resist_regeneration: number; debuff_resist_perception: number;
}

export interface EngineBonuses {
  damage: number; accuracy: number; to_hit: number; recharge: number; endurance: number; range: number;
  toggle_end_cost: number; net_end_per_sec: number;
  defense_melee: number; defense_ranged: number; defense_aoe: number;
  defense_smashing: number; defense_lethal: number; defense_fire: number; defense_cold: number;
  defense_energy: number; defense_negative: number; defense_psionic: number; defense_toxic: number;
  resistance_smashing: number; resistance_lethal: number; resistance_fire: number; resistance_cold: number;
  resistance_energy: number; resistance_negative: number; resistance_psionic: number; resistance_toxic: number;
  max_hp: number; max_endurance: number; absorb: number; regeneration: number; recovery: number;
  run_speed: number; jump_height: number; jump_speed: number; fly_speed: number;
  mez_resist: number;
  mez_resist_hold: number; mez_resist_stun: number; mez_resist_immobilize: number; mez_resist_sleep: number;
  mez_resist_confuse: number; mez_resist_fear: number; mez_resist_knockback: number;
  protection_hold: number; protection_stun: number; protection_immobilize: number; protection_sleep: number;
  protection_confuse: number; protection_fear: number; protection_knockback: number;
  debuff_resist_slow: number; debuff_resist_defense: number; debuff_resist_recharge: number; debuff_resist_endurance: number;
  debuff_resist_recovery: number; debuff_resist_to_hit: number; debuff_resist_regeneration: number; debuff_resist_perception: number;
  debuff_resist_accuracy: number; debuff_resist_range: number;
  heal_other: number; heal_received: number;
  stealth_radius_pve: number; stealth_radius_pvp: number; perception_radius: number;
  mez_resist_taunt: number; mez_resist_placate: number;
  level_shift: number;
  base_to_hit: number; hit_chance: number; combat_modifier: number;
  strength_defense: number; strength_to_hit: number; strength_heal: number; strength_absorb: number;
  strength_end_mod: number; strength_movement: number; strength_mez: number;
  immobilize_duration: number; hold_duration: number; stun_duration: number; sleep_duration: number;
  confuse_duration: number; terror_duration: number;
  errors: { context: string; detail: string }[];
}

/** One accepted/rejected set-bonus instance (engine `BonusSourceRef`). The engine owns the
 *  accept/reject decision; the display name is resolved here from the build. */
export interface EngineBonusSourceRef {
  power_internal_name: string;
  power_set: string;
  set_name: string;
  pieces: number;
}

/** One Rule-of-5 bucket for a `(stat, value)` pair (engine `ValueBucket`). */
export interface EngineValueBucket {
  value: number;
  count: number;
  capped: boolean;
  sources: EngineBonusSourceRef[];
  rejected_sources: EngineBonusSourceRef[];
}

/** One tracked stat's buckets + its beta routing (engine `SetBonusStatTracking`). */
export interface EngineSetBonusTracking {
  /** Beta internal stat key — the `bonusTracking` map key the UI looks up. */
  stat_key: string;
  /** camelCase dashboard breakdown-map key(s) the sources surface under. */
  breakdown_keys: string[];
  /** 2-dp value key → bucket. */
  buckets: Record<string, EngineValueBucket>;
}

/** One always-on proc contribution to the dashboard breakdown (engine `ProcBreakdownSource`). */
export interface EngineProcBreakdownSource {
  breakdown_key: string;
  set_name: string;
  proc_name: string;
  value: number;
  capped: boolean;
  /** Which proc pass emitted it — drives the label + whether it feeds the over-cap ring. */
  kind: 'always_on' | 'ppm' | 'build_up';
  /** "end" / "hp" reinterpretation tag → " (+End)" / " (+HP)" label suffix, else "". */
  note: string;
  power_internal_name: string;
  power_set: string;
}

/** A base→enhanced→final value (engine `ThreeTier`). `final` is a reserved word in Rust, so
 *  the engine emits the raw identifier — the JSON key is still `final`. */
interface EngineThreeTier {
  base: number;
  enhanced: number;
  final: number;
}

/** Per-power perma tracking (engine `PermaInfo`), snake_case. `recast` is the engine's
 *  `RecastBehavior` unit variant ("Refreshes"/"Stacks"), null when the atoms don't state a
 *  single answer; absent entirely on artifacts built before it existed. */
interface EnginePerma {
  base_recharge: number;
  duration: number;
  effective_recharge: number;
  recharge_needed: number;
  total_recharge: number;
  perma_percent: number;
  is_perma: boolean;
  recast?: 'Refreshes' | 'Stacks' | null;
}

/** One selected power's non-DPS execution + perma projection (engine `PowerProjection`,
 *  PROD6B-1). Each execution aspect is `null` when the power lacks that base stat; `perma` is
 *  `null` for a non-perma-eligible power. */
export interface EngineGrantedQuantity {
  /** All seven members of the engine's `GrantedQuantity`. This used to declare three, which
   *  made an exhaustive reader on this side accept four wire values it had no arm for
   *  (ATTRTYPE-1: widening a wire enum obliges every exhaustive reader). */
  kind:
    | 'value'
    | 'mez_duration'
    | 'mez_magnitude'
    | 'mez_expression'
    | 'mez_constant'
    | 'mez_unstated'
    | 'distance';
  /** Present only on `mez_duration`: the mez rank the effect grabs, shown beside the
   *  duration the tier carries. */
  magnitude?: number;
}

/** One resolved granted buff/debuff magnitude — a display row's worth of already-resolved
 *  numbers (PROD6B-2). A by-type effect contributes one entry per type. */
export interface EngineGrantedMagnitude {
  row_key: string;
  effect_key: string;
  label: string;
  category: string;
  format: string;
  priority: number | null;
  value: EngineThreeTier;
  quantity: EngineGrantedQuantity;
  by_type_label: string | null;
  /** The effective duration in seconds, from `GrantedMagnitude::duration`. `null` on a
   *  `mez_duration` row (its tier already IS the seconds) and on a row the display bag
   *  records nothing for — see the engine's `recorded_duration`. ENGLAG-2. */
  duration: number | null;
}

/** How a resolved damage component applies (engine `DamageApplication`) — serde's external
 *  tagging: the unit variants are bare strings, the chance variant an object. */
export type EngineDamageApplication = 'Always' | 'Dormant' | { Chance: number };

/** A component that ticks rather than landing at once (engine `DamageOverTime`). */
export interface EngineDamageOverTime {
  duration: number;
  period: number;
  nominal_ticks: number;
  tick_chance: number | null;
  cancel_on_miss: boolean;
  expected_ticks: number;
}

/** One damage atom resolved against the projected target (engine `DamageComponent`, RB5). */
export interface EngineDamageComponent {
  damage_type: string;
  table: string;
  /** Per-tick for an over-time component. */
  base: number;
  /** Whole-duration totals at the three enhancement tiers. */
  total: EngineThreeTier;
  application: EngineDamageApplication;
  over_time: EngineDamageOverTime | null;
  gate: string | null;
  /** The effect group's authored `Tag`s, verbatim (`CritLarge`, `ScrapperCrit_AoE`, …). */
  tags: string[];
}

/** A damage atom whose gate the projection context could not answer (engine
 *  `UnresolvedDamage`) — reported, never dropped (Rule 1). */
export interface EngineUnresolvedDamage {
  damage_type: string;
  gate: string;
  reason: string;
  tags: string[];
}

/** Every damage component of one power against one target (engine `PowerDamage`). The
 *  base/enhanced/final sums cover only the `Always` components — a chance component is a
 *  further hit, not part of the one these totals describe. */
export interface EngineDamage {
  components: EngineDamageComponent[];
  unresolved: EngineUnresolvedDamage[];
  base: number;
  enhanced: number;
  final: number;
  capped: boolean;
}

export interface EnginePowerProjection {
  power_internal_name: string;
  power_set: string;
  recharge: EngineThreeTier | null;
  endurance_cost: EngineThreeTier | null;
  accuracy: EngineThreeTier | null;
  cast_time: EngineThreeTier | null;
  arcana_time: number | null;
  range: EngineThreeTier | null;
  perma: EnginePerma | null;
  granted_magnitudes: EngineGrantedMagnitude[];
  enhancement_bonuses: Record<string, number>;
  damage: EngineDamage;
}

/** One buff-pet aura contribution — the engine's per-(pet, stat) row. The label is the
 *  SUMMONING power's display name, resolved here from the build, which is how the beta's own
 *  synthetic per-pet power labelled it. */
export interface EngineBuffPetBreakdownSource {
  breakdown_key: string;
  value: number;
  power_internal_name: string;
  power_set: string;
}

/** One travel-buff contribution — `suppressed` when it lost its suppress group (Combat Jumping
 *  beside Super Jump) or combat mode dropped it. Never `capped`: suppression is game mechanics,
 *  not a Rule-of-5 violation, so it must not feed the over-cap warning. */
export interface EngineMovementBreakdownSource {
  breakdown_key: string;
  value: number;
  suppressed: boolean;
  power_name: string;
}

/** One stealth-radius contribution — `superseded` when a larger radius in the same suppress
 *  group won the grouped-max resolve. The travel twin of {@link EngineMovementBreakdownSource},
 *  and `superseded` maps onto the same `suppressed` display state. */
export interface EngineStealthBreakdownSource {
  breakdown_key: string;
  value: number;
  superseded: boolean;
  power_name: string;
}

/** One row of the engine's per-power provenance ledger (engine `PowerBreakdownSource`) — the
 *  main active-power, accolade and archetype-inherent contributions, measured as the accumulator
 *  delta around each contributor rather than reported by the appliers. The ROW's `kind` names the
 *  source group, not the field it arrived in: by the time the apply pass runs, an accolade and a
 *  toggle are both auto-on Self powers. */
export interface EnginePowerBreakdownSource {
  breakdown_key: string;
  power_internal_name: string;
  power_set: string;
  /** Signed — a power's self-penalty (Granite's −Recharge) is a contribution like any other. */
  value: number;
  kind: EnginePowerSourceKind;
}

/** The engine's `PowerSourceKind` variants, serialized as their Rust identifiers. */
export type EnginePowerSourceKind = 'ActivePower' | 'Accolade' | 'Inherent';

/** One incarnate contribution (engine `IncarnateBreakdownSource`). Not a power: the loadout
 *  addresses it by slot, and one equipped power can contribute more than once (its stat block
 *  and its below-45 exemplar buff), so it carries its own ledger. */
export interface EngineIncarnateBreakdownSource {
  breakdown_key: string;
  /** Catalog slot id (`alpha` … `genesis`) — how the player finds the contributor. */
  slot: string;
  /** The equipped power's internal name; the display name is resolved here from the loadout. */
  power_name: string;
  /** The below-45 Genesis exemplar buff, a DIFFERENT contribution of the same equipped power. */
  exemplar: boolean;
  value: number;
}

export interface EngineTotals {
  stats: EngineStats;
  bonuses: EngineBonuses;
  set_bonus_tracking: EngineSetBonusTracking[];
  proc_breakdown: EngineProcBreakdownSource[];
  buff_pet_breakdown: EngineBuffPetBreakdownSource[];
  movement_breakdown: EngineMovementBreakdownSource[];
  stealth_breakdown: EngineStealthBreakdownSource[];
  power_breakdown: EnginePowerBreakdownSource[];
  incarnate_breakdown: EngineIncarnateBreakdownSource[];
  power_projection: EnginePowerProjection[];
  /** The hardest hit this build's CHOSEN POWERSETS can produce with a power's own slots filled
   *  for damage — the scale a damage bar reads against. Deliberately not the hardest hit the
   *  build has PICKED: a maximum over picks is attained by construction, so exactly one power
   *  read full on every build and the bar's range went to the gap between best and second-best.
   *  `null` when nothing in reach resolves to damage, including the ordinary no-target case. */
  damage_ceiling: number | null;
  /** What the what-if TEAM-BUFF layer moved in producing these totals. Measured by the engine's
   *  own injection, so a "simulated" marker cannot disagree with the numbers it marks. */
  what_if: { moved: Record<string, number> };
}

/** The beta-facing form of one resolved granted magnitude — {@link EngineGrantedMagnitude} with
 *  its keys reshaped. One entry per display row, so a by-type effect contributes one per type. */
export interface GrantedMagnitude {
  rowKey: string;
  effectKey: string;
  label: string;
  category: string;
  format: string;
  priority: number | null;
  value: ThreeTierValues;
  quantity: EngineGrantedQuantity;
  byTypeLabel: string | null;
  /** The effective duration in seconds — see [`EngineGrantedMagnitude.duration`]. ENGLAG-2. */
  duration: number | null;
}

/** The beta-facing per-power non-DPS projection — the engine's resolved values reshaped to the
 *  existing calculator output types ({@link ThreeTierValues} / {@link PermaInfo}) so the display
 *  surfaces read the engine instead of re-deriving via `calculatePowerEnhancementBonuses`
 *  + `calcThreeTier` + `calculatePermaInfo`. `cycleTime` stays a render-edge composition
 *  (`recharge.final + (useArcanaTime ? arcanaTime : castTime.base)`) because it depends on a UI
 *  toggle the engine never sees. */
export interface PowerProjection {
  recharge: ThreeTierValues | null;
  enduranceCost: ThreeTierValues | null;
  accuracy: ThreeTierValues | null;
  castTime: ThreeTierValues | null;
  arcanaTime: number | null;
  range: ThreeTierValues | null;
  perma: PermaInfo | null;
  /** The magnitudes this power GRANTS, each already resolved and three-tiered. One entry per
   *  display row, so a by-type effect contributes one per type (PROD6B-2). */
  grantedMagnitudes: GrantedMagnitude[];
  /** This power's post-ED slotted bonuses, aspect → fraction, with Alpha folded in — the same
   *  input every tier above was built from (PROD6D). A surface driving rows the projection does
   *  not itself resolve reads these instead of running `calculatePowerEnhancementBonuses`
   *  beside the engine. */
  enhancementBonuses: EnhancementBonuses;
  /** This power's own damage atoms resolved against the projection's target (RB5). The
   *  rank-forked rows — the Scrapper crit — only resolve when the `CharacterState` the
   *  projection ran on names a `combat.target_class`; without one they sit in
   *  `unresolved` rather than being guessed at. */
  damage: PowerDamage;
}

/** The beta-facing form of one resolved damage component. */
export interface PowerDamageComponent {
  damageType: string;
  table: string;
  /** Per-tick for an over-time component. */
  base: number;
  /** Whole-duration totals at the three enhancement tiers. */
  total: ThreeTierValues;
  /** `'always'` — part of every landed hit. `'dormant'` — inert until something outside the
   *  attack wakes it (Fiery Embrace). A number in (0,1) — the chance of a FURTHER hit, which
   *  is how the export states the Scrapper crit: its own row, own scale, own probability. */
  application: 'always' | 'dormant' | number;
  overTime: {
    duration: number;
    period: number;
    nominalTicks: number;
    tickChance: number | null;
    cancelOnMiss: boolean;
    expectedTicks: number;
  } | null;
  gate: string | null;
  /** The effect group's authored `Tag`s, verbatim — where the game NAMES the mechanic
   *  (`CritLarge`, `ScrapperCrit_AoE`). Never classified here (Rule 0). */
  tags: string[];
}

/** The beta-facing form of the engine's per-power damage ledger. */
export interface PowerDamage {
  components: PowerDamageComponent[];
  /** Components whose gate the projection could not answer — shown, never dropped. */
  unresolved: { damageType: string; gate: string; reason: string; tags: string[] }[];
  /** Sums of the `'always'` components only — a chance component is a further hit, not part
   *  of the hit these totals describe. */
  base: number;
  enhanced: number;
  final: number;
  capped: boolean;
}

/** External serde tagging → the beta union. Throws on an unknown variant: that is schema
 *  drift between the vendored wasm and this decoder, and a guessed application would ship a
 *  wrong number as authoritative (Rule 1). */
function mapDamageApplication(a: EngineDamageApplication): PowerDamageComponent['application'] {
  if (a === 'Always') return 'always';
  if (a === 'Dormant') return 'dormant';
  if (typeof a === 'object' && a !== null && typeof a.Chance === 'number') return a.Chance;
  throw new Error(`unrecognized engine DamageApplication: ${JSON.stringify(a)}`);
}

function mapDamageComponent(c: EngineDamageComponent): PowerDamageComponent {
  return {
    damageType: c.damage_type,
    table: c.table,
    base: c.base,
    total: { base: c.total.base, enhanced: c.total.enhanced, final: c.total.final },
    application: mapDamageApplication(c.application),
    overTime: c.over_time
      ? {
          duration: c.over_time.duration,
          period: c.over_time.period,
          nominalTicks: c.over_time.nominal_ticks,
          tickChance: c.over_time.tick_chance,
          cancelOnMiss: c.over_time.cancel_on_miss,
          expectedTicks: c.over_time.expected_ticks,
        }
      : null,
    gate: c.gate,
    tags: c.tags,
  };
}

function mapDamage(d: EngineDamage): PowerDamage {
  return {
    components: d.components.map(mapDamageComponent),
    unresolved: d.unresolved.map((u) => ({
      damageType: u.damage_type,
      gate: u.gate,
      reason: u.reason,
      tags: u.tags,
    })),
    base: d.base,
    enhanced: d.enhanced,
    final: d.final,
    capped: d.capped,
  };
}

/** The minimal power identity every engine source ref carries — resolved to a display name. */
interface PowerRef {
  power_internal_name: string;
  power_set: string;
}

/** Resolves an engine source ref to the slotting power's display name. Built from the same
 *  build the engine read; falls back to the internal name if a power can't be located. */
export type PowerNameResolver = (ref: PowerRef) => string;

/** The beta's source-string format (`collectAllSetBonuses`): `${set} (${pieces}pc in ${power})`. */
function sourceLabel(ref: EngineBonusSourceRef, powerName: string): string {
  return `${ref.set_name} (${ref.pieces}pc in ${powerName})`;
}

/**
 * Reshape the engine's set-bonus tracking into the beta `BonusTracking` — the `(x/5)` counters
 * and capped-strikethrough in the enhancement / set-bonus tooltips. Keyed by the engine's beta
 * stat key (which equals the UI's `normalizeStatName` output) then the 2-dp value key, matching
 * `{ [stat]: { [valueKey]: ValueTracking } }`.
 */
export function mapBonusTracking(
  tracking: EngineSetBonusTracking[],
  resolveName: PowerNameResolver,
): BonusTracking {
  const out: BonusTracking = {};
  const toTracked = (ref: EngineBonusSourceRef) => {
    const powerName = resolveName(ref);
    return { name: sourceLabel(ref, powerName), powerName };
  };
  for (const stat of tracking) {
    const byValue: Record<string, ValueTracking> = {};
    for (const [valueKey, bucket] of Object.entries(stat.buckets)) {
      byValue[valueKey] = {
        count: bucket.count,
        capped: bucket.capped,
        value: bucket.value,
        sources: bucket.sources.map(toTracked),
        rejectedSources: bucket.rejected_sources.map(toTracked),
      };
    }
    out[stat.stat_key] = byValue;
  }
  return out;
}

/**
 * Reshape the engine's set-bonus tracking into the dashboard `breakdown` map's set-bonus sources
 * — the per-stat tooltip rows, the over-cap ring (`powerName` + `capped`), and the Rule-of-5
 * banner. A faithful port of the beta's Step 3 + `buildStatBreakdown` (`legacy-totals.oracle.ts`):
 * each accepted instance is a `capped:false` source, each rejected instance a `capped:true` one,
 * the bucket total is `value × count`, and a stat fans out to every `breakdown_keys` entry
 * (`+Res(Recharge Debuff)` → both recharge and slow).
 */
export function mapSetBonusBreakdown(
  tracking: EngineSetBonusTracking[],
  resolveName: PowerNameResolver,
): Map<string, DashboardStatBreakdown> {
  const breakdown = new Map<string, DashboardStatBreakdown>();
  for (const stat of tracking) {
    const sources: StatSource[] = [];
    let cappedSources = 0;
    let total = 0;
    for (const bucket of Object.values(stat.buckets)) {
      for (const ref of bucket.sources) {
        const powerName = resolveName(ref);
        sources.push({ name: sourceLabel(ref, powerName), value: bucket.value, type: 'set-bonus', capped: false, powerName });
      }
      for (const ref of bucket.rejected_sources) {
        const powerName = resolveName(ref);
        sources.push({ name: sourceLabel(ref, powerName), value: bucket.value, type: 'set-bonus', capped: true, powerName });
        cappedSources++;
      }
      total += bucket.value * bucket.count;
    }
    for (const key of stat.breakdown_keys) {
      const existing = breakdown.get(key);
      if (existing) {
        existing.sources.push(...sources);
        existing.total += total;
        existing.cappedSources += cappedSources;
      } else {
        // Clone the source array so paired stats don't share a mutable reference.
        breakdown.set(key, { total, base: 0, sources: [...sources], cappedSources });
      }
    }
  }
  return breakdown;
}

/** Format the proc source's tooltip label, mirroring the beta's per-pass source strings. */
function procLabel(src: EngineProcBreakdownSource, resolveName: PowerNameResolver): string {
  const base = `${src.set_name}: ${src.proc_name}`;
  switch (src.kind) {
    case 'ppm':
      return `${base} (PPM)`;
    case 'build_up':
      return `${base} (in ${resolveName(src)})`;
    case 'always_on':
    default:
      return base + (src.note === 'end' ? ' (+End)' : src.note === 'hp' ? ' (+HP)' : '');
  }
}

/**
 * File one source under `key`, creating the entry if this is the first thing to reach it.
 *
 * `contributed` is separate from `source.value` because the two grouped-max families show a row
 * for a contribution that did not land: a suppressed travel buff or a superseded stealth radius
 * keeps its value on the row (what it WOULD have given) but adds 0 to the total, so the entry
 * matches the number the dashboard shows.
 */
function pushSource(
  breakdown: Map<string, DashboardStatBreakdown>,
  key: string,
  source: StatSource,
  contributed: number,
): void {
  const entry = breakdown.get(key);
  if (entry) {
    entry.sources.push(source);
    entry.total += contributed;
    if (source.capped) entry.cappedSources++;
  } else {
    breakdown.set(key, {
      total: contributed,
      base: 0,
      sources: [source],
      cappedSources: source.capped ? 1 : 0,
    });
  }
}

/**
 * Fold the engine's always-on proc contributions into the dashboard `breakdown` map as
 * `type:'proc'` sources — the beta's `applySingleProcEffect` / PPM / Build-Up `addToBreakdown`
 * calls. Mutates `breakdown` in place (procs land on top of the set-bonus sources, as in the
 * beta's ordering). Only the always-on kind carries a `powerName` (so the over-cap ring lights
 * for a Rule-of-5-capped proc); PPM / Build-Up rows omit it, matching the beta. Every source
 * adds its value to the stat total (the beta `addToBreakdown` semantics — including capped ones).
 */
export function addProcBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  procSources: EngineProcBreakdownSource[],
  resolveName: PowerNameResolver,
): void {
  for (const src of procSources) {
    pushSource(breakdown, src.breakdown_key, {
      name: procLabel(src, resolveName),
      value: src.value,
      type: 'proc',
      capped: src.capped,
      ...(src.kind === 'always_on' ? { powerName: resolveName(src) } : {}),
    }, src.value);
  }
}

/**
 * Fold the engine's buff-pet aura rows into the breakdown map — one `active-power` source per
 * (pet aura, stat), named after the summoning power (Force Field Generator, Triage Beacon).
 * The fold is opt-in per pet, so this is a no-op on a build that has enabled none.
 */
export function addBuffPetBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  sources: EngineBuffPetBreakdownSource[],
  resolveName: PowerNameResolver,
): void {
  for (const src of sources) {
    pushSource(breakdown, src.breakdown_key, {
      name: resolveName({ power_internal_name: src.power_internal_name, power_set: src.power_set }),
      value: src.value,
      type: 'active-power',
    }, src.value);
  }
}

/**
 * Fold the engine's travel-buff rows into the breakdown map. Each source keeps its own value
 * and carries `suppressed`, so the tooltip can show a suppress-group loser dimmed with what it
 * would have contributed.
 */
export function addMovementBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  sources: EngineMovementBreakdownSource[],
): void {
  for (const src of sources) {
    pushSource(breakdown, src.breakdown_key, {
      name: src.power_name,
      value: src.value,
      type: 'active-power',
      ...(src.suppressed ? { suppressed: true } : {}),
    }, src.suppressed ? 0 : src.value);
  }
}

/**
 * Fold the engine's stealth-radius rows into the breakdown map. The stealth twin of
 * {@link addMovementBreakdown} — both are grouped-max resolves, so a radius beaten by a larger
 * one in its suppress group is shown dimmed and contributes nothing.
 */
export function addStealthBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  sources: EngineStealthBreakdownSource[],
): void {
  for (const src of sources) {
    pushSource(breakdown, src.breakdown_key, {
      name: src.power_name,
      value: src.value,
      type: 'active-power',
      ...(src.superseded ? { suppressed: true } : {}),
    }, src.superseded ? 0 : src.value);
  }
}

/**
 * Which breakdown group each ledger `kind` renders under. A `Record` rather than a switch so a
 * new engine variant breaks the build here instead of silently landing in someone else's group
 * (CLAUDE.md Rule 1) — the same idiom `CAP_POOL` uses in over-cap-mute.ts.
 */
const POWER_SOURCE_TYPE: Record<EnginePowerSourceKind, StatSource['type']> = {
  ActivePower: 'active-power',
  Accolade: 'accolade',
  Inherent: 'inherent',
};

/**
 * Fold the engine's per-power provenance ledger into the breakdown map — the "Active Powers",
 * "Accolades" and "Inherent Powers" groups of every stat tooltip and of the Detailed Totals
 * sheet. This is the bulk of what a breakdown explains: an armour toggle's Defense and
 * Resistance, a passive's +Regen, a click's +MaxHP.
 *
 * The row's `kind` chooses the group, not the field it arrived in — see
 * {@link EnginePowerBreakdownSource}. Nothing here is Rule-of-5 tracked, so no row carries
 * `capped`/`powerName`: a power contribution can neither cap a bucket nor be rejected by one,
 * and giving it a `powerName` would enrol it in the over-cap ring's accounting.
 */
export function addPowerBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  sources: EnginePowerBreakdownSource[],
  resolveName: PowerNameResolver,
): void {
  for (const src of sources) {
    pushSource(breakdown, src.breakdown_key, {
      name: resolveName(src),
      value: src.value,
      type: POWER_SOURCE_TYPE[src.kind],
    }, src.value);
  }
}

/** Resolves an equipped incarnate's internal name to the display name the loadout stores. */
export type IncarnateNameResolver = (src: EngineIncarnateBreakdownSource) => string;

/**
 * Fold the engine's incarnate ledger into the breakdown map as `type:'incarnate'` sources.
 * Separate from {@link addPowerBreakdown} because an incarnate is addressed by slot rather than
 * by powerset, and one equipped power contributes more than once — the below-45 exemplar buff is
 * a different contribution of the same power, and carries the beta's `(exemplar)` suffix so the
 * two rows are tellable apart.
 */
export function addIncarnateBreakdown(
  breakdown: Map<string, DashboardStatBreakdown>,
  sources: EngineIncarnateBreakdownSource[],
  resolveName: IncarnateNameResolver,
): void {
  for (const src of sources) {
    const name = resolveName(src);
    pushSource(breakdown, src.breakdown_key, {
      name: src.exemplar ? `${name} (exemplar)` : name,
      value: src.value,
      type: 'incarnate',
    }, src.value);
  }
}

export function mapStats(s: EngineStats, b: EngineBonuses): CharacterStats {
  return {
    damage: s.damage, accuracy: s.accuracy, tohit: s.to_hit, recharge: s.recharge, endrdx: s.endurance_reduction,
    defMelee: s.def_melee, defRanged: s.def_ranged, defAoE: s.def_aoe,
    defSL: s.def_sl, defFC: s.def_fc, defEN: s.def_en, defPsionic: s.def_psionic, defToxic: s.def_toxic,
    resSL: s.res_sl, resFC: s.res_fc, resEN: s.res_en, resPsionic: s.res_psionic, resToxic: s.res_toxic,
    recovery: s.recovery, regeneration: s.regeneration, maxhp: s.max_hp, maxend: s.max_end,
    // Movement is a GlobalBonuses field in the engine, not CharacterStats.
    runspeed: b.run_speed, flyspeed: b.fly_speed, jumpspeed: b.jump_speed, jumpheight: b.jump_height,
    debuffResistSlow: s.debuff_resist_slow, debuffResistDefense: s.debuff_resist_defense,
    debuffResistRecharge: s.debuff_resist_recharge, debuffResistEndurance: s.debuff_resist_endurance,
    debuffResistRecovery: s.debuff_resist_recovery, debuffResistToHit: s.debuff_resist_to_hit,
    debuffResistRegeneration: s.debuff_resist_regeneration, debuffResistPerception: s.debuff_resist_perception,
  };
}

/**
 * The mirror of the movement note in {@link mapStats}: absorb is a `CharacterStats` field in the
 * engine but a `GlobalBonuses` field here, so it crosses the other way. The engine's `bonuses.absorb`
 * is the RAW accumulator; `stats.absorb` is that sum clamped to the archetype's per-level absorb
 * ceiling, which is what the game shows and therefore what the dashboard reads.
 */
export function mapGlobal(b: EngineBonuses, s: EngineStats): GlobalBonuses {
  return {
    damage: b.damage, accuracy: b.accuracy, toHit: b.to_hit, recharge: b.recharge, endurance: b.endurance, range: b.range,
    defMelee: b.defense_melee, defRanged: b.defense_ranged, defAoE: b.defense_aoe,
    defSmashing: b.defense_smashing, defLethal: b.defense_lethal, defFire: b.defense_fire, defCold: b.defense_cold,
    defEnergy: b.defense_energy, defNegative: b.defense_negative, defPsionic: b.defense_psionic, defToxic: b.defense_toxic,
    resSmashing: b.resistance_smashing, resLethal: b.resistance_lethal, resFire: b.resistance_fire, resCold: b.resistance_cold,
    resEnergy: b.resistance_energy, resNegative: b.resistance_negative, resPsionic: b.resistance_psionic, resToxic: b.resistance_toxic,
    maxHP: b.max_hp, maxEndurance: b.max_endurance, absorb: s.absorb, regeneration: b.regeneration, recovery: b.recovery,
    runSpeed: b.run_speed, jumpHeight: b.jump_height, jumpSpeed: b.jump_speed, flySpeed: b.fly_speed,
    mezResist: b.mez_resist,
    mezResistHold: b.mez_resist_hold, mezResistStun: b.mez_resist_stun, mezResistImmobilize: b.mez_resist_immobilize,
    mezResistSleep: b.mez_resist_sleep, mezResistConfuse: b.mez_resist_confuse, mezResistFear: b.mez_resist_fear,
    mezResistKnockback: b.mez_resist_knockback,
    protHold: b.protection_hold, protStun: b.protection_stun, protImmobilize: b.protection_immobilize,
    protSleep: b.protection_sleep, protConfuse: b.protection_confuse, protFear: b.protection_fear,
    protKnockback: b.protection_knockback,
    debuffResistSlow: b.debuff_resist_slow, debuffResistDefense: b.debuff_resist_defense,
    debuffResistRecharge: b.debuff_resist_recharge, debuffResistEndurance: b.debuff_resist_endurance,
    debuffResistRecovery: b.debuff_resist_recovery, debuffResistToHit: b.debuff_resist_to_hit,
    debuffResistRegeneration: b.debuff_resist_regeneration, debuffResistPerception: b.debuff_resist_perception,
    // DEBUFFRES-1. Accumulator-only on the engine side too — no `CharacterStats` twin, so unlike
    // the eight above these have no `mapStats` line.
    debuffResistAccuracy: b.debuff_resist_accuracy, debuffResistRange: b.debuff_resist_range,
    healOther: b.heal_other, healReceived: b.heal_received, threatLevel: 0,
    stealthRadiusPvE: b.stealth_radius_pve, stealthRadiusPvP: b.stealth_radius_pvp, perceptionRadius: b.perception_radius,
    protRepel: 0,
    mezResistTaunt: b.mez_resist_taunt, mezResistPlacate: b.mez_resist_placate,
    levelShift: b.level_shift,
    // `enduranceDiscount` stays 0 on BOTH sides: the beta field of that name is vestigial and
    // never accumulated — `endurance` is the EndDisc sum the toggle math and the dashboard read.
    toggleEndCost: b.toggle_end_cost, enduranceDiscount: 0, netEndPerSec: b.net_end_per_sec,
    baseToHit: b.base_to_hit, hitChance: b.hit_chance, combatModifier: b.combat_modifier,
    strengthDefense: b.strength_defense, strengthToHit: b.strength_to_hit, strengthHeal: b.strength_heal,
    strengthAbsorb: b.strength_absorb, strengthEndMod: b.strength_end_mod, strengthMovement: b.strength_movement,
    strengthMez: b.strength_mez,
    immobilizeDuration: b.immobilize_duration, holdDuration: b.hold_duration, stunDuration: b.stun_duration,
    sleepDuration: b.sleep_duration, confuseDuration: b.confuse_duration, terrorDuration: b.terror_duration,
  };
}

const mapTier = (t: EngineThreeTier | null): ThreeTierValues | null =>
  t ? { base: t.base, enhanced: t.enhanced, final: t.final } : null;

const mapPerma = (p: EnginePerma | null): PermaInfo | null =>
  p
    ? {
        baseRecharge: p.base_recharge,
        duration: p.duration,
        effectiveRecharge: p.effective_recharge,
        rechargeNeeded: p.recharge_needed,
        totalRecharge: p.total_recharge,
        permaPercent: p.perma_percent,
        isPerma: p.is_perma,
        recast:
          p.recast === 'Refreshes' ? 'refreshes' : p.recast === 'Stacks' ? 'stacks' : undefined,
      }
    : null;

/**
 * PROD6B-1 — reshape the engine's per-power projection into a lookup the display surfaces read.
 * Keyed by {@link projectionKey} (the same identity `powerNameResolver` uses; internal names
 * collide across sets, so the set disambiguates). The values are the existing calculator output
 * types, so a surface swaps its inline `calcThreeTier` / `calculatePermaInfo` call for a lookup
 * with no shape change — `usePowerProjection` is the read path (PROD6C).
 */
const mapGrantedMagnitude = (m: EngineGrantedMagnitude): GrantedMagnitude => ({
  rowKey: m.row_key,
  effectKey: m.effect_key,
  label: m.label,
  category: m.category,
  format: m.format,
  priority: m.priority,
  value: { base: m.value.base, enhanced: m.value.enhanced, final: m.value.final },
  quantity: m.quantity,
  byTypeLabel: m.by_type_label,
  duration: m.duration ?? null,
});

/**
 * The projection map's key. A power's identity is `(powerSet, internalName)` — `internalName`
 * alone is not unique (see `findSelectedPowerInBuild`) — and the separator is the same `\0` the
 * power-name resolver uses, so every key into an engine-keyed map is built one way.
 */
export function projectionKey(powerSet: string, internalName: string): string {
  return `${powerSet}\0${internalName}`;
}

/** One engine projection reshaped to the beta calculator output types. Also the shape the
 *  on-demand single-power projection comes back in (PROD6C), which is why it stands alone. */
export function mapOnePowerProjection(p: EnginePowerProjection): PowerProjection {
  return {
    recharge: mapTier(p.recharge),
    enduranceCost: mapTier(p.endurance_cost),
    accuracy: mapTier(p.accuracy),
    castTime: mapTier(p.cast_time),
    arcanaTime: p.arcana_time,
    range: mapTier(p.range),
    perma: mapPerma(p.perma),
    grantedMagnitudes: p.granted_magnitudes.map(mapGrantedMagnitude),
    enhancementBonuses: p.enhancement_bonuses,
    damage: mapDamage(p.damage),
  };
}

export function mapPowerProjection(projection: EnginePowerProjection[]): Map<string, PowerProjection> {
  const out = new Map<string, PowerProjection>();
  for (const p of projection) {
    out.set(projectionKey(p.power_set, p.power_internal_name), mapOnePowerProjection(p));
  }
  return out;
}
