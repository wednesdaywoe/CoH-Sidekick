//! PROD6B — per-power **non-DPS execution + perma** (6B-1) and **granted magnitudes** (6B-2)
//! projection. The engine already computes every ingredient of the execution half (per-power
//! enhancement bonuses, AT-table-free execution stats, perma recharge math) but `recalculate`
//! only ever summed them into the character totals. This re-walks every SELECTED power (active or
//! not — the info panel shows a picked toggle you have turned off) and emits the resolved
//! three-tier values the per-power display surfaces read, so the beta's duplicate
//! `calculatePowerEnhancementBonuses` / `calcThreeTier` / `getRecharge` / `calculatePermaInfo`
//! can retire (PROD6C–E). It owns the MATH; the adapter only reshapes keys (decision
//! 2026-07-24, user-chosen: engine owns the resolved values).
//!
//! The buff/debuff magnitudes a power GRANTS are resolved by [`crate::granted`] and ride along on
//! each projection (6B-2), and its own damage by [`crate::damage`] (RB5). No DPS value is here:
//! damage arrives per component with its rolls kept apart, and the ONE consumer allowed to
//! average them into a number is the attack chain ([`crate::chain_build`], RB5-c). Proc-DPS
//! stays out of scope.
//!
//! Fidelity: the three-tier composition ports the beta `calcThreeTier`
//! (`powerDisplayUtils.ts`) and ArcanaTime ports `calculateArcanaTime` (`damage.ts`)
//! formula-for-formula; the enhancement bonuses come from the already-gated
//! [`crate::apply::power_enhancement`] and perma from the already-tested [`crate::perma`].
//! The beta `powerProjectionParity` test grades the whole block against those TS calculators
//! over each fork's corpus.

use crate::apply::{power_enhancement, PowerBreakdownSource};
use crate::damage::{resolve_power_damage, PowerDamage};
use crate::expr::{SourceContext, TargetIdentity, CURRENT_TO_HIT, HIDE_METER};
use crate::gather::PowerSourceKind;
use crate::granted::{resolve_granted_magnitudes, GrantedMagnitude};
use crate::incarnates::AlphaEnhancement;
use crate::perma::{calculate_perma_info, is_perma_eligible, PermaInfo};
use crate::totals::{CalcError, GlobalBonuses};
use coh_data::{CharacterState, Power, PowerDatabase};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};

/// One tick of the server clock — the ArcanaTime quantum (beta `damage.ts` `SERVER_TICK`).
const SERVER_TICK: f64 = 0.132;

/// The `strengthsDisallowed` entry that blocks a build-wide +Range from reaching a power.
/// Schema vocabulary (an attribute name), not a game proper noun.
const RANGE_ENHANCEMENT: &str = "Range";

/// The enhancement aspects the game DIVIDES a base value by (its net strength) rather than
/// multiplying it. Schema vocabulary (aspect keys), not game proper nouns.
const RECHARGE_ASPECT: &str = "recharge";
const ENDURANCE_ASPECT: &str = "endurance";

/// The enhancement aspect a damage ceiling is slotted for. Schema vocabulary (an aspect key),
/// not a game proper noun.
const DAMAGE_ASPECT: &str = "damage";

/// Whether `aspect` reduces its base value (divides by net strength) rather than scaling it.
pub(crate) fn is_reduction_aspect(aspect: &str) -> bool {
    matches!(aspect, RECHARGE_ASPECT | ENDURANCE_ASPECT)
}

/// One aspect's `ClampStrength` interval — the range the game holds a net strength inside
/// (`Common/entity/character_attribs.c`, clamping between `StrengthMin` and
/// `StrengthMaxTable[level]`). Exported per archetype as `ArchetypeCaps::*_floor` / `*_cap`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct StrengthBounds {
    pub floor: f64,
    pub cap: f64,
}

/// The clamp bounds for both reduction aspects, resolved once per build from the archetype's
/// caps. Either side is `None` when the build has no archetype yet or the dataset ships no
/// caps for it: the divisor then goes UNCLAMPED rather than inventing a ceiling — the same
/// call [`crate::finalize::convert_to_character_stats`] makes for the resistance cap, and
/// `recalculate` raises the gap once rather than per power.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReductionClamps {
    pub recharge: Option<StrengthBounds>,
    pub endurance: Option<StrengthBounds>,
}

impl ReductionClamps {
    pub fn from_caps(caps: Option<&coh_data::ArchetypeCaps>) -> Self {
        let Some(caps) = caps else {
            return ReductionClamps::default();
        };
        ReductionClamps {
            recharge: Some(StrengthBounds {
                floor: caps.recharge_floor,
                cap: caps.recharge_cap,
            }),
            endurance: Some(StrengthBounds {
                floor: caps.endurance_floor,
                cap: caps.endurance_cap,
            }),
        }
    }

    /// The clamps a build divides by — its archetype's, or none until it has an archetype.
    pub fn for_build(state: &CharacterState, db: &PowerDatabase) -> Self {
        Self::from_caps(
            state
                .archetype
                .id
                .as_deref()
                .and_then(|archetype| db.archetype_stats.get(archetype)),
        )
    }

    /// The bounds governing `aspect`, or `None` for an aspect that multiplies (and for a
    /// reduction aspect whose archetype ships no caps).
    pub(crate) fn for_aspect(&self, aspect: &str) -> Option<StrengthBounds> {
        match aspect {
            RECHARGE_ASPECT => self.recharge,
            ENDURANCE_ASPECT => self.endurance,
            _ => None,
        }
    }
}

/// A power's chance to hit, and the level gap it was read at.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct HitChance {
    /// Fraction in `[0.05, 0.95]`: the base ToHit for `level_diff`, plus the build's ToHit
    /// buffs, times this power's final accuracy ([`crate::purple_patch::hit_chance`]).
    pub chance: f64,
    /// Signed levels the target sits above the caster after the level shift — the
    /// `effective_level_diff` the build-wide hit chance is read at.
    pub level_diff: i32,
}

/// A base → enhanced → final value, the beta `ThreeTierValues` (`powerDisplayUtils.ts`).
/// `enhanced` folds in this power's slotting; `final` additionally folds the build-wide
/// global. Reductions (recharge / endurance) divide by `1 + bonus`; every other aspect
/// multiplies by `1 + bonus`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ThreeTier {
    pub base: f64,
    pub enhanced: f64,
    // `final` is a reserved word; the raw identifier serializes as the beta key "final".
    pub r#final: f64,
}

impl ThreeTier {
    /// A value no enhancement aspect touches — flat across all three tiers.
    pub(crate) fn flat(base: f64) -> Self {
        ThreeTier {
            base,
            enhanced: base,
            r#final: base,
        }
    }

    /// Multiplicative aspect (accuracy, range, buff magnitudes): `base × (1 + bonus)`.
    pub(crate) fn multiplicative(base: f64, enhancement: f64, global: f64) -> Self {
        ThreeTier {
            base,
            enhanced: base * (1.0 + enhancement),
            r#final: base * (1.0 + enhancement + global),
        }
    }

    /// Reduction aspect (recharge, endurance): `base / (1 + bonus)`, with the net strength
    /// held inside the archetype's `ClampStrength` interval.
    ///
    /// The bound is data, never 1. The game divides by the net strength directly
    /// (`fEnduranceCost / (fEnduranceDiscount + 0.0001f)`, `character_tick.c`; the same shape
    /// for recharge), having already clamped that strength between the class's `StrengthMin`
    /// and `StrengthMaxTable[level]`. Recharge floors at 0.25 (a debuffed power takes at most
    /// 4× its base) and caps at 5.0; endurance floors at 0.0001 — a divide guard, not a floor,
    /// so an endurance debuff really does make a power cost more. Both beta paths predate
    /// this: the totals applied no bound at all and the display hardcoded `max(1, …)`, a
    /// leftover of the older subtractive `× max(0, 1 − bonus)` form where the guard was
    /// load-bearing. `None` leaves the divisor unclamped (see [`ReductionClamps`]).
    pub(crate) fn reduction(
        base: f64,
        enhancement: f64,
        global: f64,
        bounds: Option<StrengthBounds>,
    ) -> Self {
        let divide = |net: f64| match bounds {
            Some(bounds) => base / net.clamp(bounds.floor, bounds.cap),
            None => base / net,
        };
        ThreeTier {
            base,
            enhanced: divide(1.0 + enhancement),
            r#final: divide(1.0 + enhancement + global),
        }
    }
}

/// ArcanaTime for a cast time — `(ceil(cast / tick) + 1) × tick`, floored at one tick
/// (beta `calculateArcanaTime`, `damage.ts`). A non-positive cast is one tick.
fn arcana_time(cast_time: f64) -> f64 {
    if cast_time <= 0.0 {
        return SERVER_TICK;
    }
    ((cast_time / SERVER_TICK).ceil() + 1.0) * SERVER_TICK
}

/// The resolved per-power non-DPS execution + perma values, keyed to the slotting power by
/// the same `power_internal_name` / `power_set` ref the proc + set-bonus breakdowns use.
/// Each execution aspect is `Some` only when the power carries that base stat (the beta
/// shows the row only for a truthy value); `perma` is `Some` for any click power with a
/// recharge and a caster-side window — a wider set than the display gate, kept wide so the
/// beta's projection-parity harness can grade the numbers everywhere they exist.
/// `perma_eligible` carries the display gate ([`crate::perma::is_perma_eligible`])
/// alongside, so every render edge applies the same verdict the card's ring uses.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PowerProjection {
    pub power_internal_name: String,
    pub power_set: String,
    /// Recharge seconds (reduction). Base is `stats.recharge` else `effects.recharge`.
    pub recharge: Option<ThreeTier>,
    /// Endurance cost (reduction). A toggle's base is per-second
    /// (`stats.endurance / (activatePeriod ?? 0.5)`); a click's is the flat `stats.endurance`;
    /// else `effects.enduranceCost`.
    pub endurance_cost: Option<ThreeTier>,
    /// Accuracy multiplier (multiplicative). Base is `stats.accuracy` else `effects.accuracy`.
    pub accuracy: Option<ThreeTier>,
    /// The chance this power lands on the build's chosen target, read at the target's level net
    /// of the incarnate level shift. `None` when the power has no accuracy (it never rolls to
    /// hit) or the purple-patch tables are empty.
    pub hit_chance: Option<HitChance>,
    /// Activation / cast seconds (multiplicative). No enhancement aspect exists for cast time,
    /// so today base == enhanced == final; kept three-tier for uniformity with the beta.
    pub cast_time: Option<ThreeTier>,
    /// ArcanaTime derived from the base cast time (which carries no enhancement). `None` when
    /// the power has no cast time.
    pub arcana_time: Option<f64>,
    /// Range feet (multiplicative). Base is `stats.range` else `effects.range`.
    pub range: Option<ThreeTier>,
    /// Perma tracking — `None` for a toggle/auto or a power with no recharge/duration.
    pub perma: Option<PermaInfo>,
    /// The display gate for `perma` ([`crate::perma::is_perma_eligible`]): false when keeping
    /// the power up permanently is meaningless or unreachable, in which case a render edge
    /// shows no tracker even though the numbers above exist.
    pub perma_eligible: bool,
    /// The buff/debuff magnitudes this power GRANTS, each resolved against its AT modifier
    /// table and three-tiered (PROD6B-2). One entry per display row, so a by-type effect
    /// contributes one per type. Empty for a power whose effects bag holds no registered
    /// effect (a pure summon, a damage-only attack).
    pub granted_magnitudes: Vec<GrantedMagnitude>,
    /// This power's post-ED slotted bonuses, aspect → fraction, Alpha folded in — the input
    /// every tier above was built from (PROD6D). Exposed because a surface that re-slots a
    /// power hypothetically needs the same fractions to drive rows the projection does not
    /// itself resolve, and recomputing them beside the engine is how the two drift.
    pub enhancement_bonuses: BTreeMap<&'static str, f64>,
    /// The damage this power's own atoms deal against the build's chosen target (RB5), each
    /// component resolved separately and the certain ones totalled. Empty of components AND of
    /// unresolved rows for a power that deals none; components present but unresolved rows too
    /// when no target is chosen, because a damage atom's gate names who is being hit.
    pub damage: PowerDamage,
}

pub(crate) fn extra_object<'a>(power: &'a Power, key: &str) -> Option<&'a Map<String, Value>> {
    power.extra.get(key).and_then(Value::as_object)
}

pub(crate) fn object_number(object: Option<&Map<String, Value>>, key: &str) -> Option<f64> {
    object.and_then(|o| o.get(key)).and_then(Value::as_f64)
}

/// A base execution stat read the beta's `stats`-merged-over-`effects` way (InfoPanel
/// `:993`): the `stats.<stat_key>` value when truthy, else `effects.<effects_key>` when
/// truthy. A zero/absent value is no stat (the beta's `&&` truthiness), yielding `None` so
/// no row is projected.
pub(crate) fn truthy_stat(power: &Power, stat_key: &str, effects_key: &str) -> Option<f64> {
    let stats = extra_object(power, "stats");
    let effects = extra_object(power, "effects");
    object_number(stats, stat_key)
        .filter(|value| *value != 0.0)
        .or_else(|| object_number(effects, effects_key).filter(|value| *value != 0.0))
}

fn is_toggle(power: &Power) -> bool {
    power
        .extra
        .get("powerType")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("toggle"))
}

/// Base endurance cost the beta's way: a toggle's `stats.endurance` divided by its
/// `activatePeriod` (a per-second drain; `?? 0.5` when the period is absent), a click's flat
/// `stats.endurance`, else `effects.enduranceCost`.
pub(crate) fn base_endurance_cost(power: &Power) -> Option<f64> {
    let stats = extra_object(power, "stats");
    if let Some(endurance) = object_number(stats, "endurance").filter(|value| *value != 0.0) {
        if is_toggle(power) {
            let period = object_number(stats, "activatePeriod").unwrap_or(0.5);
            return Some(endurance / period);
        }
        return Some(endurance);
    }
    object_number(extra_object(power, "effects"), "enduranceCost").filter(|value| *value != 0.0)
}

/// How much of the build-wide +Range actually reaches this power.
///
/// The server copies the character's whole Strength set onto every power unconditionally
/// (`character_mods.c` `character_AccrueBoosts`: `ppow->attrStrength = p->attrStrength`) and
/// then zeroes the entries the power's own `StrengthsDisallowed` names. `BoostsAllowed` gates
/// something else entirely — which enhancement a slot will accept (`boost.c`
/// `power_IsValidBoost`) — so it says nothing about a global. A melee attack's range stays at
/// base because it carries `StrengthsDisallowed Range`, not because it rejects Range IOs.
///
/// The two are not interchangeable: Snow Storm, Dark Servant and ~490 other player powers
/// accept no Range enhancement yet disallow no Range Strength, so a global does extend them.
/// The slotted side needs no gate here — [`power_enhancement`] already filters on
/// `boostsAllowed`, which is the list that actually governs it.
///
/// `globalStrengthsDisallowed` is the HC-added narrower form: slotted enhancement still
/// applies, globals do not. Both block a global, so both are read here (see
/// [`crate::perma`], which applies the same pair to recharge).
fn reachable_global_range(power: &Power, global_range: f64) -> f64 {
    let disallows = |field: &str| {
        power
            .extra
            .get(field)
            .and_then(Value::as_array)
            .is_some_and(|list| {
                list.iter()
                    .any(|entry| entry.as_str() == Some(RANGE_ENHANCEMENT))
            })
    };
    if disallows("strengthsDisallowed") || disallows("globalStrengthsDisallowed") {
        return 0.0;
    }
    global_range
}

/// The context a build-relative gate is evaluated in — a damage atom's `requires`, and the
/// fast-form condition [`crate::effective`] reads. The build's chosen target answers the
/// target-side readers, the modes it satisfies answer the source-side ones (the same set
/// [`crate::gather::live_modes`] hands the conditional layer, so a mode-gated damage atom, a
/// mode-gated resistance atom and a snipe's form gate answer to one build state).
///
/// `cur.kToHit` is the one CHARACTER attribute here, and it can be answered only at this point in
/// the pipeline: it is the class's own `AttribBase` ToHit plus the finalized ToHit buff, so the
/// mid-totals contexts ([`crate::gather`], [`crate::inherents`]) genuinely cannot supply it.
/// Read UNCLAMPED, matching the beta's `permanentToHit`; against a +125.35pp CAPS-1 ceiling and a
/// +22pp threshold the clamp cannot change a verdict, but the day a fork authors one near the
/// ceiling it would.
///
/// With no target chosen the target readers stay Indeterminate, which is the honest answer: the
/// gates ask who is being hit and nobody has said. `owned_powers` answers the caster-ownership
/// gates from the build's own picks and from the transient states its conditional toggles declare
/// ([`crate::gather::owned_powers`]); a path neither producer carries is state this build is not
/// in, and absent still means not owned. Homecoming's fast snipe reads
/// `Set_Bonus.Global_Bonus.Experienced_Marksman source.ownPower?`, which stays false because
/// nothing in any fork's export GRANTS that power — 34 unique-IO boosts grant their
/// `Set_Bonus.Global_Bonus.*` marker and this one is granted by none of them, so its `kEngaged`
/// half is the only live route (DATA-GAP-REGISTER CHAIN-1).
///
/// `team_size` is the combat panel's team stepper — named `vigilance_team_size` for its first
/// consumer, but it is THE team size, and the fork crit programs
/// (`30 source.TeamSize> 0.03 * 0.07 + rand >= …`) read the same quantity. The game's own
/// reader counts team members within the operand radius INCLUDING the caster, solo = 1
/// (`chareval_TeamSizeHelper`); a positionless projection has no radius, so the stepper's
/// value stands for "teammates in range", exactly as Vigilance already reads it.
fn gate_context(
    state: &CharacterState,
    source_modes: &HashSet<String>,
    g: &GlobalBonuses,
    db: &PowerDatabase,
    owned: &HashMap<String, f64>,
) -> SourceContext {
    let mut source_attributes = HashMap::new();
    if let Some(caps) = state
        .archetype
        .id
        .as_deref()
        .and_then(|archetype| db.archetype_stats.get(archetype))
    {
        source_attributes.insert(
            CURRENT_TO_HIT.to_string(),
            caps.to_hit_base + g.to_hit / 100.0,
        );
    }
    // The hide meter, for the from-Hide openers' `kMeter source> .9 <` (CHAIN-1). Bound from
    // the build's own `hidden` input, and ONLY for a build whose powers publish that meter —
    // `kMeter` is one attribute ten mechanics drive, so an unconditional binding would tell a
    // Dominator's or a Mastermind's meter that it means "hidden"
    // ([`crate::gather::publishes_hide_meter`]).
    //
    // Full/empty rather than a scale: `hidden` is a boolean the user declares, and the gates
    // that read it are thresholds (`.9 <`, `0 >`) that only ask which side of the window the
    // caster is on. A fractional meter would be inventing a decay curve the build has no input
    // for. A build with no hide meter leaves the key ABSENT — Indeterminate, not zero, so a
    // gate says "unknown" rather than "not hidden".
    if crate::gather::publishes_hide_meter(state, db) {
        source_attributes.insert(
            HIDE_METER.to_string(),
            if state.combat.hidden { 1.0 } else { 0.0 },
        );
    }
    let (target_class, target_entity) = state.combat.target_identity();
    SourceContext {
        // Always a target, never `None` here: this is the per-power projection, the caller the
        // doc on [`SourceContext::target`] names as the one that HAS a target. The totals loop
        // is the caller that passes `None`, and it still does — a self-totals pass has no one
        // target, which is a different statement from "the rank is unstated".
        target: Some(TargetIdentity {
            archetype_class: target_class,
            entity_type: target_entity.to_string(),
        }),
        source_modes: crate::gather::live_modes(source_modes, state.combat.in_combat),
        source_attributes,
        owned_powers: owned.clone(),
        team_size: Some(f64::from(state.combat.vigilance_team_size)),
        caster_class: state
            .archetype
            .id
            .as_deref()
            .and_then(|archetype| db.class_name_of(archetype))
            .map(str::to_string),
        ..Default::default()
    }
}

/// A power the caller wants projected even though the build does not hold it — the info
/// tooltip renders three-tier values for a power you are only hovering in the picker, and an
/// unselected power still takes Alpha and the build-wide globals.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct PowerRef {
    pub powerset: String,
    pub internal_name: String,
    /// The stacking-slider input for this power, which the display bag's per-target transform
    /// reads (PROD6C-3b). A held power carries it on its own selection; an unheld one has no
    /// selection to carry it, and the surfaces keep the slider state by name regardless.
    #[serde(default)]
    pub targets_hit: Option<u32>,
}

/// Project ONE power against the finalized accumulator. `slots` is the build's slotting for
/// it, or empty for a power the build does not hold (Alpha and the globals still apply).
/// Returns `None` for a ref the dataset cannot resolve.
#[allow(clippy::too_many_arguments)]
fn project_one(
    powerset: &str,
    internal_name: &str,
    slots: &[Option<coh_data::Enhancement>],
    targets_hit: Option<u32>,
    state: &CharacterState,
    gate: &SourceContext,
    g: &GlobalBonuses,
    effective_level_diff: i32,
    alpha: &AlphaEnhancement,
    clamps: ReductionClamps,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> Option<PowerProjection> {
    let level = state.level as i32;
    // The build-wide globals as fractions, per aspect (the beta `convertGlobalBonusesToAspects`,
    // which divides each dashboard percent by 100). Cast time has no global.
    let global_recharge = g.recharge / 100.0;
    let global_endurance = g.endurance / 100.0;
    let global_accuracy = g.accuracy / 100.0;
    let global_range = g.range / 100.0;

    let power = crate::gather::resolve_power(db, powerset, internal_name)?;
    let enhancement = power_enhancement(slots, power, alpha, level, &state.combat, db, errors);

    // Every value below describes the power the surface is SHOWING, not the one the build holds:
    // a snipe fired in combat, an opener cast from cover rather than from Hide, a power whose
    // mode-gated contributions are merged in (PROD6C-3k). Only the slotting above and the perma
    // tracker below stay keyed to the base power, as both beta surfaces keep them.
    let effective = crate::effective::effective_power(power, state, gate, db);
    let shown = effective.as_ref();

    // An adaptive-recharge power's base depends on how many foes it hit; every other power's
    // is `stats.recharge` exactly as before.
    let recharge = crate::adaptive_recharge::base_recharge(shown, targets_hit).map(|base| {
        ThreeTier::reduction(
            base,
            enhancement.get(RECHARGE_ASPECT),
            global_recharge,
            clamps.for_aspect(RECHARGE_ASPECT),
        )
    });
    let endurance_cost = base_endurance_cost(shown).map(|base| {
        ThreeTier::reduction(
            base,
            enhancement.get(ENDURANCE_ASPECT),
            global_endurance,
            clamps.for_aspect(ENDURANCE_ASPECT),
        )
    });
    let accuracy = truthy_stat(shown, "accuracy", "accuracy")
        .map(|base| ThreeTier::multiplicative(base, enhancement.get("accuracy"), global_accuracy));
    // The build-wide hit chance uses only the global accuracy; a power's own roll uses its final
    // accuracy, slotting included, against the same base ToHit and ToHit buffs.
    let hit_chance = accuracy
        .zip(crate::purple_patch::get_base_to_hit(
            &db.purple_patch,
            effective_level_diff,
        ))
        .map(|(accuracy, base_to_hit)| HitChance {
            chance: crate::purple_patch::hit_chance(base_to_hit, g.to_hit, accuracy.r#final),
            level_diff: effective_level_diff,
        });
    let cast_base = truthy_stat(shown, "castTime", "castTime");
    let cast_time =
        cast_base.map(|base| ThreeTier::multiplicative(base, enhancement.get("castTime"), 0.0));
    let arcana = cast_base.map(arcana_time);
    let range = truthy_stat(shown, "range", "range").map(|base| {
        ThreeTier::multiplicative(
            base,
            enhancement.get("range"),
            reachable_global_range(shown, global_range),
        )
    });

    // Perma reads its OWN base recharge (effects-first — a deliberate beta divergence
    // from the stats-first execution recharge above) and the same slotted+global
    // recharge fractions. Deliberately ungated: the beta's projection-parity harness
    // grades these numbers on every power that has them, so the display gate rides
    // alongside as `perma_eligible` and the render edge applies it (the beta
    // InfoPanel's isPermaEligible guard) — a power that can never reach perma must
    // not show a tracker stuck at 0% while its card shows no ring.
    let perma = calculate_perma_info(
        power,
        targets_hit,
        enhancement.get(RECHARGE_ASPECT),
        global_recharge,
        clamps.for_aspect(RECHARGE_ASPECT),
        state.dataset,
    );
    let perma_eligible =
        is_perma_eligible(power, clamps.for_aspect(RECHARGE_ASPECT), state.dataset);

    // An unselected archetype misses every modifier-table lookup, which is exactly
    // what the beta's `archetypeId &&` guards do with an undefined archetype: the
    // table-resolved branches are skipped and the authored value stands.
    let archetype = state.archetype.id.as_deref().unwrap_or_default();
    // The bag comes from the shown power; the stacking slider reads the BASE one, because that
    // is the power the slider itself is offered for (the beta's `getStackingInfo(power)` and
    // `withTargetsHit(power, …)` both read the base while the bag comes from the merged one).
    let granted_magnitudes = resolve_granted_magnitudes(
        shown,
        power,
        archetype,
        level,
        &enhancement,
        g,
        db,
        targets_hit,
        state.dataset,
    );

    // Damage reads the SHOWN power for the same reason every tier above does — a snipe fired
    // in combat is a different attack — and the build's chosen target, which its atoms' gates
    // ask about by name. No target chosen leaves those gates Indeterminate, so the components
    // come back unresolved rather than partly summed (RB5).
    let damage = resolve_power_damage(
        shown,
        archetype,
        level,
        enhancement.get("damage"),
        g.damage / 100.0,
        gate,
        db,
        errors,
    );

    Some(PowerProjection {
        power_internal_name: internal_name.to_string(),
        power_set: powerset.to_string(),
        recharge,
        endurance_cost,
        accuracy,
        hit_chance,
        cast_time,
        arcana_time: arcana,
        range,
        perma,
        perma_eligible,
        granted_magnitudes,
        enhancement_bonuses: enhancement.iter().collect(),
        damage,
    })
}

/// Project every selected power's non-DPS execution + perma values, plus any `extra` ref the
/// caller asked for that the build does not hold. `g` is the FINALIZED accumulator (its
/// `recharge` / `endurance` / `accuracy` / `range` percents drive the "final" tier), so this
/// runs at the very end of [`crate::recalculate`]. `alpha` is the same equipped-Alpha input the
/// apply loop used, so a power's projected recharge matches the recharge that fed the totals.
/// Enhancement gaps surface into `errors` (fail-loud, Rule 1); real datasets carry curves for
/// every slotted power, so this is empty in practice.
///
/// An `extra` ref the build DOES hold is skipped rather than projected twice — the selected
/// walk already emitted it, with its slotting, which is the better answer.
/// Target types whose power buffs ALLIES ONLY — the beta's `ALLY_ONLY_TARGET_TYPES`
/// (legacy-totals.oracle.ts). Such a toggle costs the caster nothing on their own bar in the
/// beta's model, so it is excluded from the drain. Powers that auto-apply to the caster
/// (Recovery Aura, Farsight, …) are tagged `Self` and are unaffected.
const ALLY_ONLY_TARGET_TYPES: &[&str] = &[
    "ally",
    "ally (alive)",
    "teammate",
    "dead teammate",
    "friend",
    "deadplayerfriend",
    "deadoraliveleaguemate",
];

/// Endurance per second the build's ACTIVE toggles drain — the beta's `toggleEndCost`
/// (its Step 9.7 `applyToggleEndCosts`).
///
/// Reads each toggle's ALREADY-PROJECTED `endurance_cost.final`, rather than recomputing the
/// divisor here: that is the same number the power's own row shows, so the dashboard total is
/// by construction the sum of the rows a user can see. `final` is
/// `base / (1 + slotted EndRdx + global EndDisc)` with the base already per-second for a
/// toggle ([`base_endurance_cost`]) — the canonical CoH divisor, taken after every discount
/// source has aggregated (this runs at the end of the pipeline, which is exactly why the beta
/// moved its own copy to Step 9.7).
///
/// A power with no projected endurance cost contributes nothing: `None` there means the power
/// carries no endurance stat at all, which is the beta's `baseEndPerSec <= 0` skip.
pub fn toggle_endurance_total(
    state: &CharacterState,
    projections: &[PowerProjection],
    db: &PowerDatabase,
) -> ToggleEndurance {
    let mut breakdown = Vec::new();
    let total = state
        .all_selected()
        .filter(|selection| selection.is_active)
        .filter_map(|selection| {
            let power =
                crate::gather::resolve_power(db, &selection.powerset, &selection.internal_name)?;
            if !is_toggle(power) {
                return None;
            }
            let ally_only = power
                .extra
                .get("targetType")
                .and_then(Value::as_str)
                .is_some_and(|t| ALLY_ONLY_TARGET_TYPES.contains(&t.to_ascii_lowercase().as_str()));
            if ally_only {
                return None;
            }
            projections
                .iter()
                .find(|p| {
                    p.power_set == selection.powerset
                        && p.power_internal_name == selection.internal_name
                })
                .and_then(|p| p.endurance_cost.as_ref())
                .map(|cost| (selection, cost.r#final))
        })
        .fold(0.0, |total, (selection, cost)| {
            // A toggle projecting exactly 0.0 files no row — the ledger's no-zero-rows rule. A
            // "0.00/s" row would read as "this toggle is free" rather than as "it carries no
            // endurance stat", which is what a zero here actually means.
            if cost != 0.0 {
                breakdown.push(PowerBreakdownSource {
                    breakdown_key: "toggleEndCost".to_string(),
                    power_internal_name: selection.internal_name.clone(),
                    power_set: selection.powerset.clone(),
                    value: cost,
                    kind: PowerSourceKind::ActivePower,
                });
            }
            total + cost
        });
    // `fold(0.0)`, not `sum()`: std's float `Sum` folds from -0.0 (the additive identity that
    // preserves an addend's signed zero), so a build with no toggles would hand the dashboard
    // -0.0 and it would render "-0.00/s".
    ToggleEndurance { total, breakdown }
}

/// The toggle drain and the per-toggle rows it was summed from.
///
/// This is the one accumulator field whose provenance needs no snapshot bracket: it is built by
/// summing named powers rather than by accumulating into shared state, so each contributor is
/// already in hand at the point of addition. Diffing `GlobalBonuses` around it would re-derive
/// what the fold already knows.
pub struct ToggleEndurance {
    pub total: f64,
    pub breakdown: Vec<PowerBreakdownSource>,
}

#[allow(clippy::too_many_arguments)]
pub fn project_powers(
    state: &CharacterState,
    source_modes: &HashSet<String>,
    g: &GlobalBonuses,
    effective_level_diff: i32,
    alpha: &AlphaEnhancement,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
    extra: &[PowerRef],
    owned: &HashMap<String, f64>,
) -> Vec<PowerProjection> {
    // The reduction clamps and the gate context are properties of the build, not of a power:
    // resolve each once here rather than re-reading the caps for every projection.
    let clamps = ReductionClamps::for_build(state, db);
    let gate = gate_context(state, source_modes, g, db, owned);
    let mut projections: Vec<PowerProjection> = state
        .all_selected()
        .filter_map(|selection| {
            // A selection the dataset can't resolve is already reported by the totals gather;
            // skip it here rather than double-recording (Rule 1's marker already stands).
            project_one(
                &selection.powerset,
                &selection.internal_name,
                &selection.slots,
                selection.targets_hit,
                state,
                &gate,
                g,
                effective_level_diff,
                alpha,
                clamps,
                db,
                errors,
            )
        })
        .collect();

    for request in extra {
        let already = projections.iter().any(|p| {
            p.power_set == request.powerset && p.power_internal_name == request.internal_name
        });
        if already {
            continue;
        }
        if let Some(projection) = project_one(
            &request.powerset,
            &request.internal_name,
            &[],
            request.targets_hit,
            state,
            &gate,
            g,
            effective_level_diff,
            alpha,
            clamps,
            db,
            errors,
        ) {
            projections.push(projection);
        }
    }
    projections
}

/// The hardest hit this build's chosen powersets can produce — the scale a damage bar reads
/// against.
///
/// Deliberately NOT the hardest hit the build has PICKED. A maximum taken over picked powers is
/// attained by construction: something is always the biggest, so exactly one attack reads full on
/// every build — not because it hits hard but because it won a field of whatever you happened to
/// choose — and the bar spends its whole range on the gap between the best and second-best pick.
/// A full bar has to mean something a power can fail to be. So the ceiling is taken over every
/// power in the sets this build chose (primary, secondary, pools, epic), picked or not, each
/// asked what it would deal with its own slots filled for damage.
///
/// Every term is read rather than assumed: the slot count is the power's own exported `maxSlots`,
/// one boost's strength is the dataset's per-level curve, the diminishing returns are the
/// dataset's own ED thresholds, and the archetype's damage cap is applied by
/// [`resolve_power_damage`] itself rather than re-derived here. Mids computes this same quantity
/// in `GetBestDamageValues` and then throws it away — its info panel overwrites the scale with
/// `Math.Max(414f, thisPower)` every time it draws a power, so what its bar actually reads
/// against is that constant.
///
/// `None` when nothing in reach resolves to damage: no target chosen (every damage gate is then
/// Indeterminate, so no component sums and the bar does not draw either), a dataset carrying no
/// enhancement curves, or a build whose sets hold no attack.
pub fn damage_ceiling(
    state: &CharacterState,
    source_modes: &HashSet<String>,
    g: &GlobalBonuses,
    db: &PowerDatabase,
    owned: &HashMap<String, f64>,
) -> Option<f64> {
    let archetype = state.archetype.id.as_deref()?;
    let level = state.level as i32;
    let gate = gate_context(state, source_modes, g, db, owned);
    let global_damage = g.damage / 100.0;

    // A candidate that cannot resolve drops out of the maximum rather than filing a marker: these
    // are powers the build has not picked and the panel never shows, so a Rule 1 marker here
    // would point at nothing a reader could go look at. The cost of losing one is a ceiling that
    // sits too low, which the caller's own `max` against the shown power absorbs.
    let mut scratch: Vec<CalcError> = Vec::new();

    chosen_powerset_ids(state)
        .filter_map(|id| db.find_powerset(&id))
        .flat_map(|powerset| powerset.powers.iter())
        .filter_map(|power| {
            let enhancement = full_slot_damage_enhancement(power, level, db)?;
            let damage = resolve_power_damage(
                power,
                archetype,
                level,
                enhancement,
                global_damage,
                &gate,
                db,
                &mut scratch,
            );
            (damage.r#final > 0.0).then_some(damage.r#final)
        })
        .fold(None, |best: Option<f64>, hit| {
            Some(best.map_or(hit, |best| best.max(hit)))
        })
}

/// The powerset ids this build has chosen — the catalogue [`damage_ceiling`] measures. Every set
/// the build can still pick from counts, because a power you have not taken yet is one the
/// ceiling has to already cover: a scale that jumps the first time you pick your epic blast is
/// the moving-scale problem again, one set further out.
fn chosen_powerset_ids(state: &CharacterState) -> impl Iterator<Item = String> + '_ {
    state
        .primary
        .id
        .iter()
        .chain(state.secondary.id.iter())
        .cloned()
        .chain(state.pools.iter().map(|pool| pool.id.clone()))
        .chain(state.epic_pool.iter().map(|pool| pool.id.clone()))
}

/// The damage enhancement a power carries with its own slots full — `maxSlots` boosts at the
/// build's level, summed pre-ED and put through the dataset's diminishing returns.
///
/// This is the one hypothetical in the ceiling, and it is the one that makes a full bar
/// reachable: it asks what the power WOULD deal slotted out for damage, so filling the bar means
/// "you have built the hardest hit these sets offer" rather than "you picked this". `None` when
/// the dataset states no enhancement curves — then no ceiling can be stated at all, rather than
/// one standing on an invented slot value.
fn full_slot_damage_enhancement(power: &Power, level: i32, db: &PowerDatabase) -> Option<f64> {
    let curves = db.enhancement_curves.as_ref()?;
    if !power.accepts_enhancements() {
        return Some(0.0);
    }
    let slots = power.max_slots()?;
    let schedule = crate::enhancement::schedule_for_aspect(DAMAGE_ASPECT, curves)?;
    let per_boost = crate::enhancement::io_value_at_level(f64::from(level), schedule, curves)?;
    Some(crate::enhancement::apply_ed(
        per_boost * f64::from(slots),
        schedule,
        curves,
    ))
}
