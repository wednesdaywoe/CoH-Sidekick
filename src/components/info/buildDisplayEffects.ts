/**
 * The display effects bag (PROD6C-3a).
 *
 * `RegistryEffectsDisplay` does not render a power's authored `effects` — it renders a bag
 * the surfaces BUILD at the render edge, and InfoPanel, PowerInfoTooltip and
 * CompareSlottingModal each built their own. This is that build, extracted once so there is a
 * single bag, and so the engine's `granted.rs` has one shape to mirror instead of three that
 * disagreed with each other.
 *
 * Everything here is a pure function of the power object — the transforms PROD6C-3 enumerates
 * as (2) the merged `stats`, (3) healing extracted from the damage array and (5) the flattened
 * `movement` object. Transform (4), `buffDuration` backfilled from a summon's duration, was
 * REMOVED 2026-09-07 and is described below. Two further transforms live below it rather than
 * inside it, because each runs on the built bag: (6) per-target scaling, which reads UI state, and (7) the pseudo-pet merge.
 * The one transform that stays at the call sites is the redirect / quick-snipe / conditional
 * merge that decides WHICH power this is.
 *
 * Two divergences between the surfaces are settled here rather than preserved (counts are
 * powers per fork over the deduped runtime corpus, homecoming / rebirth / thunderspy):
 *
 * * `arc` is converted to degrees from whichever partition carries it. Primary/secondary
 *   powers store the raw binary radians on `stats.arc` and pool/epic powers on
 *   `effects.arc` (`transformPoolPower` builds no `stats`), and the registry row is a
 *   `degrees` one — the tooltip read only `stats.arc` and so rendered a pool power's radian
 *   value into a degrees row (48 / 42 / 46 powers).
 * * `buffDuration` from a summon's duration was InfoPanel-only (297 / 273 / 280 powers). It is
 *   gone (2026-09-07). Its guard — "only where the power states no duration of its own" — read
 *   `effects.buffDuration`, which STRIP-1 removed from every power on every fork, so the guard
 *   became universally true and the backfill fired on every summon power: measured at 110 / 111 /
 *   120 / 119 powers per fork, and 100% of the bag's surviving `buffDuration` values. That put a
 *   pet's lifespan into the slot that means the power's own effect clock, where it collided with
 *   the engine's answer for the same key — the two are different quantities and the slot holds
 *   one (PROD6B-BETA-PARITY class 2). The lifespan has its own home on all three surfaces, via
 *   `formatSummonDuration`, which also states the 99999s "permanent" sentinel as words.
 *
 * The tooltip's six `flySpeed` / `runSpeed` / `jumpSpeed` / `jumpHeight` / `regeneration` /
 * `recovery` top-level mappings and its `effects.endurance` fallback are NOT carried over:
 * measured across all three forks, no power carries any of those keys at the top level of a
 * runtime bag (`transformPoolPower` destructures `endurance` away into the renamed
 * `enduranceCost`), so they are dead rather than divergent.
 */
import type { Power, PowerEffects } from '@/types';
import { arcToDegrees } from '@/data/proc-data';
import { extractHealingFromDamage } from '@/utils/calculations/healing';
import { synthesizePseudoPetEffects } from '@/utils/calculations/pet-damage';
import { carriesPerTarget, maxStackCap } from '@/data/core/atom-query';
import { perTargetCountCannotBeZero } from '@/utils/calculations/character-totals';

/** Toggle tick interval when the data omits one — end/s = endurance / activatePeriod. */
const DEFAULT_ACTIVATE_PERIOD = 0.5;

/**
 * Build the bag `RegistryEffectsDisplay` resolves for one power.
 *
 * `damage` defaults to the power's own, and is a parameter because each surface reaches its
 * effective damage array differently (InfoPanel merges conditionals onto the power itself;
 * the tooltip merges the stance-gated entries into the array) — the heal extraction must
 * see the merged array either way.
 */
export function buildDisplayEffects(
  power: Power,
  damage: unknown = power.damage ?? power.effects?.damage,
): PowerEffects {
  const effects = power.effects ?? ({} as PowerEffects);
  const stats = power.stats;
  const bag: Record<string, unknown> = { ...effects };

  // STRIP-1: the writer lifts summon out of the bag to the top level, so the
  // bag spread above no longer carries it. Mint it from the top level so the
  // display surfaces that read the bag (DamageBlock, the tooltip's pet block)
  // keep their contract.
  if (power.summon) bag.summon = power.summon;

  // Execution stats live on `stats` for primary/secondary powers and in the bag itself for
  // pool/epic ones. A zero is no stat (the surfaces' own `&&` truthiness), so it leaves any
  // authored bag value standing.
  if (stats?.endurance) {
    bag.enduranceCost = power.powerType === 'Toggle'
      ? stats.endurance / (stats.activatePeriod ?? DEFAULT_ACTIVATE_PERIOD)
      : stats.endurance;
  }
  if (stats?.recharge) bag.recharge = stats.recharge;
  if (stats?.accuracy) bag.accuracy = stats.accuracy;
  if (stats?.range) bag.range = stats.range;
  if (stats?.castTime) bag.castTime = stats.castTime;
  if (stats?.radius) bag.radius = stats.radius;
  if (stats?.maxTargets) bag.maxTargets = stats.maxTargets;

  const rawArc = stats?.arc ?? effects.arc;
  if (rawArc != null) bag.arc = arcToDegrees(rawArc);

  // A heal authored inside the damage array (Life Drain, Reconstruction) rather than as an
  // effect. An authored `healing` wins.
  if (!effects.healing) {
    const healing = extractHealingFromDamage(damage);
    if (healing) bag.healing = healing;
  }

  // The nested movement object (Super Jump, Fly, Sprint) carries the registry's per-axis
  // keys one level down.
  const movement = effects.movement as Record<string, unknown> | undefined;
  if (movement && typeof movement === 'object') {
    if (movement.flySpeed) bag.fly = movement.flySpeed;
    if (movement.runSpeed) bag.runSpeed = movement.runSpeed;
    if (movement.jumpSpeed) bag.jumpSpeed = movement.jumpSpeed;
    if (movement.jumpHeight) bag.jumpHeight = movement.jumpHeight;
    // The converter's `<axis>Unenhanced` split slots (FLYPOOL-1): a paired
    // IgnoreStrength half beside the enhanceable one, flattened to the flat
    // spellings the registry resolves through the base key (ENT-6) and the
    // engine's granted.rs emits from the atoms. Pool Fly carries both halves.
    if (movement.flySpeedUnenhanced) bag.flyUnenhanced = movement.flySpeedUnenhanced;
    if (movement.runSpeedUnenhanced) bag.runSpeedUnenhanced = movement.runSpeedUnenhanced;
    if (movement.jumpSpeedUnenhanced) bag.jumpSpeedUnenhanced = movement.jumpSpeedUnenhanced;
    if (movement.jumpHeightUnenhanced) bag.jumpHeightUnenhanced = movement.jumpHeightUnenhanced;
  }

  return bag as PowerEffects;
}

// ---------------------------------------------------------------------------
// the per-target transform (PROD6C-3b)
// ---------------------------------------------------------------------------
//
// Unlike everything above this is NOT a pure function of the power: it reads the power's own
// stacking-slider value out of the UI store. It lives here anyway because it transforms the
// same bag, immediately after it is built, and because the engine's `stacking.rs` mirrors the
// pair — the slider gate and the adjustment — as one step of `display_effects`.


/** `maxTargets` sentinel for an unbounded AoE: no bound, so no axis for a slider to drag. */
const UNBOUNDED_MAX_TARGETS = 255;

/**
 * Check if a value (or by-type sub-values) has a perTarget field.
 */
export function hasPerTargetField(value: unknown): boolean {
  if (typeof value !== 'object' || value === null) return false;
  // `maxHPFractionPerTarget` is the same field for an absorb whose magnitude is an
  // Expression rather than a scale — it grows the row the same way.
  if ('perTarget' in value || 'maxHPFractionPerTarget' in value) return true;
  // Check by-type sub-objects (defenseBuff, resistance, etc.)
  for (const subVal of Object.values(value as Record<string, unknown>)) {
    if (typeof subVal === 'object' && subVal !== null
      && ('perTarget' in subVal || 'maxHPFractionPerTarget' in subVal)) return true;
  }
  return false;
}

/**
 * The stacking slider a surface offers for this power, or `null` for one that offers none —
 * `has_stacking_slider` / `adjust_display_bag` in the engine's `stacking.rs` are the twin.
 *
 * Two mechanics share the one control. An AoE power whose effects carry a per-foe increment
 * slides on TARGETS HIT; a power whose atoms self-stack slides on the STACK COUNT. Per-target
 * wins where a power has both (Soul Drain self-stacks on a double-cast and grows per foe, and
 * the per-foe story is the dominant one for tooltip math), so the smaller stack cap cannot
 * shrink the targets axis.
 *
 * **Both arms read the ATOMS.** They read the authored `effects` object until STACKINFO-1, and
 * the function opened with `if (!power.effects) return null` — sound while that object was the
 * display bag's source, and starvation the moment it was not. STRIP-1 emptied it: no power on
 * any fork carries an authored `perTarget`, a `maxStacks` or a `stackInterval` any more, so this
 * returned `null` for the entire corpus and every surface stopped offering the slider. It read
 * as a corpus with no stacking powers rather than as a reader with no input, which is why the
 * one gate over it went green having graded nothing (`powerProjectionParity`).
 *
 * The atom-native reading is WIDER than the bag's on the stack arm and that is the correction,
 * not a side effect: a power states its own cap on its atoms, where the converter minted a
 * `maxStacks` slot for only some of them. It is NARROWER on four powers, which is the other half
 * of the same correction — ATOM-BAG-3's delay schedules, a `Replace` shield re-applied every few
 * seconds that the bag recorded as a stack of 7. Those no longer offer a slider for a shield that
 * never doubles, and the `Stacks (every Xs)` label that described their cadence went with them:
 * its `stackInterval` came from the same writer, and no other power ever had one.
 *
 * `minStacks` is where the axis STARTS, and two shapes cannot start it at zero. The floor is
 * {@link perTargetCountCannotBeZero}, the same predicate the engine and the totals path floor the
 * count with, rather than a term this function spells by hand:
 *
 *  - A power whose effects land on the caster as well as on whoever else is in radius fills the
 *    first of its `maxTargets` seats for as long as it is running. Phalanx Fighting gives its 5%
 *    melee/ranged/AoE defence solo, and a slider reading "Off" there described a state the power
 *    is never in (PERFOE-3).
 *  - A power the game refuses to fire without an entity in its sights reached at least one of
 *    them if it was used at all. Guarded Spin is the reported case: a Staff Fighting cone whose
 *    +Def(Melee, Lethal) the game stacks once per foe it lands on, so its growth is real — but the
 *    slider it needed opened at "Off", and toggling the power on moved no defence on the dashboard
 *    for anyone who did not know to look for it (PERFOE-4).
 *
 * The stacks arm keeps 0: N there counts casts, and a click the build has not fired is off.
 */
export function getStackingInfo(
  power: Power,
): { maxStacks: number; minStacks: number; label: string } | null {
  // AoE per-target powers (Soul Drain, Eclipse, Power Sink, …) — the slider's natural axis is
  // "targets hit", bounded by the power's own `maxTargets`. An absent or unbounded bound is no
  // axis to drag, so it is no slider rather than a one-target one.
  if (carriesPerTarget(power)) {
    const maxTargets = power.stats?.maxTargets;
    if (maxTargets && maxTargets > 1 && maxTargets !== UNBOUNDED_MAX_TARGETS) {
      const minStacks = perTargetCountCannotBeZero(power) ? 1 : 0;
      return { maxStacks: maxTargets, minStacks, label: 'Targets Hit' };
    }
  }

  // Linear self-stacking — the power's atoms state a depth greater than one on an effect they
  // land on the caster. Reached only where there is no per-target scaling, so the two never
  // compound (Siphon Speed's caster recharge buff is the plain case).
  const cap = maxStackCap(power);
  if (cap !== undefined) return { maxStacks: cap, minStacks: 0, label: 'Stacks' };

  return null;
}

/**
 * Adjust ScaledEffect scale values for per-target stacking.
 * At N targets: effective_scale = scale + perTarget × (N - 1)
 * Recursively handles by-type objects (defenseBuff, resistance).
 */
function adjustScaledValue(value: unknown, targetsHit: number): unknown {
  if (typeof value !== 'object' || value === null) return value;
  if ('scale' in value && 'perTarget' in value) {
    const se = value as { scale: number; table: string; perTarget?: number };
    if (se.perTarget && targetsHit > 1) {
      return { ...se, scale: se.scale + se.perTarget * (targetsHit - 1) };
    }
    return value;
  }
  // A MaxHP-fraction absorb has no `scale` for the increment to grow — its magnitude
  // rides an Expression, so the per-foe step arrives as `maxHPFractionPerTarget`.
  if ('maxHPFraction' in value && 'maxHPFractionPerTarget' in value) {
    const fr = value as { maxHPFraction: number; maxHPFractionPerTarget?: number };
    if (fr.maxHPFractionPerTarget && targetsHit > 1) {
      return { ...fr, maxHPFraction: fr.maxHPFraction + fr.maxHPFractionPerTarget * (targetsHit - 1) };
    }
    return value;
  }
  // By-type object — recurse into sub-entries
  const result: Record<string, unknown> = {};
  let changed = false;
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    const adjusted = adjustScaledValue(v, targetsHit);
    result[k] = adjusted;
    if (adjusted !== v) changed = true;
  }
  return changed ? result : value;
}

/**
 * Multiply the `scale` field of a ScaledEffect (or every leaf of a by-type
 * object) by `multiplier`. Used for linear self-stacking — every additional
 * stack just adds another instance of the base magnitude.
 */
function multiplyScale(value: unknown, multiplier: number): unknown {
  if (multiplier === 1) return value;
  if (typeof value !== 'object' || value === null) return value;
  if ('scale' in value && typeof (value as { scale: unknown }).scale === 'number') {
    const se = value as { scale: number; [k: string]: unknown };
    return { ...se, scale: se.scale * multiplier };
  }
  // By-type object — recurse
  const result: Record<string, unknown> = {};
  let changed = false;
  for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
    const adj = multiplyScale(v, multiplier);
    result[k] = adj;
    if (adj !== v) changed = true;
  }
  return changed ? result : value;
}

/**
 * Create an adjusted copy of effects with per-target scale values multiplied
 * (perTarget AoE math) and/or stacks-linear effects multiplied by stack count.
 */
function adjustEffectsForTargets(
  effects: PowerEffects,
  targetsHit: number
): PowerEffects {
  if (targetsHit <= 1) return effects;
  const stacksLinear = new Set(effects.stacksLinear || []);
  const adjusted: Record<string, unknown> = {};
  let changed = false;
  for (const [key, value] of Object.entries(effects)) {
    let adj = adjustScaledValue(value, targetsHit);
    // Linear self-stack multiply — but only for keys NOT already driven by
    // perTarget (adjustScaledValue above handles those); applying both would
    // double-scale. Honor a per-effect cap so a lower-cap key (Psychokinetic
    // Barrier's absorb, cap 2) doesn't over-multiply when the slider (maxStacks
    // 3) is dragged past its own limit.
    if (stacksLinear.has(key) && !hasPerTargetField(value)) {
      const cap = effects.stackCaps?.[key] ?? effects.maxStacks;
      const n = cap ? Math.min(targetsHit, cap) : targetsHit;
      adj = multiplyScale(adj, n);
    }
    adjusted[key] = adj;
    if (adj !== value) changed = true;
  }
  return (changed ? adjusted : effects) as PowerEffects;
}

/**
 * The display bag a surface renders for one power at its current slider setting: the built bag
 * when the power shows no stacking slider or the slider is at 0/1, else the adjusted copy.
 *
 * The `targetsHit > 1` identity is the DISPLAY rule, and deliberately not the totals one — the
 * accumulator reads an absent count as zero foes and zeroes a `perTarget` buff (PROD6B-2d).
 * A display row is only ever grown here, never zeroed.
 *
 * **It currently transforms nothing, and that is stated rather than assumed.** Both of
 * {@link adjustEffectsForTargets}'s inputs are authored — a `perTarget` on a bag VALUE, and the
 * power's `stacksLinear` list — and STRIP-1 left neither in the corpus, so the copy it returns is
 * always the bag it was given. Nothing user-visible depends on that any more: ENGLAG-1 re-pointed
 * the rendered rows at the engine's projection and all three `RegistryEffectsDisplay` call sites
 * pass `rows`, so this bag reaches the surface for its `durations` / `buffDuration` annotations
 * alone, and the slider's whole effect runs through the engine. Kept, not deleted, because it is
 * the shape `coh_math::stacking::adjust_display_bag` mirrors and because the day the beta bag
 * grows an atom seed is the day it starts mattering again. `powerProjectionParity`'s targets-hit
 * body reports its two counters every run instead of flooring them, so that day is visible.
 */
export function withTargetsHit(power: Power, effects: PowerEffects, targetsHit: number): PowerEffects {
  if (!getStackingInfo(power) || targetsHit <= 1) return effects;
  return adjustEffectsForTargets(effects, targetsHit);
}

// ---------------------------------------------------------------------------
// the pseudo-pet merge (PROD6C-3c)
// ---------------------------------------------------------------------------

/**
 * Merge a summon power's pseudo-pet debuffs UNDER its own bag — the parent's keys win, the
 * pet fills in keys the parent doesn't carry. Powers like Glue Arrow deliver their
 * enhanceable debuffs entirely through a non-commandable pseudo-pet, so without this the
 * player's enhancements never reach the rows they scale.
 *
 * Applied AFTER `withTargetsHit`, as both surfaces do: the synthesized fragment carries no
 * `perTarget` metadata for the slider to grow.
 */
export function withPseudoPetEffects(power: Power, effects: PowerEffects): PowerEffects {
  const pseudo = synthesizePseudoPetEffects(power.summon);
  return pseudo ? { ...pseudo, ...effects } : effects;
}
