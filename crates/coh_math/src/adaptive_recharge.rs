//! Adaptive recharge — a power whose recharge grows with the number of foes it hit.
//!
//! Homecoming's Cinders, Glacier, Flash, Synaptic Overload, Seeds of Confusion and their
//! siblings, Fiery Aura's Consume, and the Psionic/Radiation Armor redirect powers (Consume
//! Psyche, Devour Psyche, Radiation Therapy). Each carries one `RechargePower` row, aspect `Abs`,
//! aimed at the caster, which the game applies once per foe affected and which adds its scale in
//! seconds to the power's own timer (`AdjustTimer`). The authored help states the rule in words
//! ("a base recharge of 8 seconds and each affected foe will increase the recharge by 14.5").
//!
//! Recharge enhancement and buffs reduce the WHOLE adjusted timer, not only the base — a
//! developer statement, not something the data says on its own — so this module answers the
//! BASE seconds for a count, and the callers divide it exactly as they divide any base recharge.
//! Procs are the exception and do not read this: the game rolls them against the unadjusted base.
//!
//! A power with an over-cap ([`OverCap`]) counts foes past its trigger at a fraction:
//! Radiation Therapy's one 9.46s row adds 9.46s for the first foe and 2.84s for each after.
//!
//! The ceiling comes in two shapes:
//!
//! * A power that redirects carries two records, and the converter stamps the redirect's own
//!   recharge as `stats.redirectRecharge`. That one is the base; the outer record's
//!   `stats.recharge` is the ceiling (60s on all three, matching their help text).
//! * A direct power states only its base, and its ceiling is where the count stops:
//!   `base + increment × maxTargets` reproduces every stated maximum (8 + 16 × 14.5 = 240).

use crate::projection::{extra_object, object_number};
use crate::stacking::{weighted_count, OverCap};
use coh_data::atom::lands_on_caster;
use coh_data::{Aspect, EffectType, Power};

/// A power's adaptive-recharge rule, in base seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdaptiveRecharge {
    /// Recharge with nobody hit.
    pub base: f64,
    /// Seconds each affected foe adds.
    pub per_target: f64,
    /// The most the count can raise it to.
    pub ceiling: f64,
    /// The largest count that means anything — the power's `maxTargets`.
    pub max_targets: u32,
    /// How foes past the power's over-cap count — Radiation Therapy's second and later foes add
    /// 0.3 of the increment each.
    pub over_cap: Option<OverCap>,
}

impl AdaptiveRecharge {
    /// Base recharge after hitting `targets` foes. An absent count is nobody hit, the same
    /// reading the per-target path gives the shared targets-hit input.
    pub fn base_for(&self, targets: Option<u32>) -> f64 {
        let n = targets.unwrap_or(0).min(self.max_targets);
        (self.base + self.per_target * weighted_count(self.over_cap, n)).min(self.ceiling)
    }
}

/// The adaptive-recharge rule `power` carries, or `None` for an ordinary recharge.
///
/// A row qualifies only on a power whose count is bounded: the accolade temp powers carry a
/// `RechargePower` to Self too, on single-target records with no `maxTargets`, and that row is a
/// shared-timer mechanic rather than a per-foe one. A gated row is excluded for the same reason —
/// Brainstorm's Refraction Shield adds its seconds only when aimed at a friend.
pub fn adaptive_recharge(power: &Power) -> Option<AdaptiveRecharge> {
    let stats = extra_object(power, "stats");
    let max_targets =
        object_number(stats, "maxTargets").filter(|max| *max > 1.0 && *max != 255.0)? as u32;
    let per_target: f64 = power
        .atoms
        .iter()
        .filter(|atom| {
            atom.effect_type == Some(EffectType::RechargePower)
                && atom.aspect == Some(Aspect::Abs)
                && lands_on_caster(atom)
                && atom
                    .requires_expression
                    .as_ref()
                    .is_none_or(|gate| gate.is_empty())
                && atom.gated != Some(true)
        })
        .filter_map(|atom| atom.scale)
        .filter(|scale| *scale > 0.0)
        .sum();
    if per_target <= 0.0 {
        return None;
    }
    let over_cap = OverCap::of(power);
    let outer = object_number(stats, "recharge").filter(|r| *r > 0.0);
    let (base, ceiling) = match object_number(stats, "redirectRecharge").filter(|r| *r > 0.0) {
        Some(base) => (base, outer.unwrap_or(f64::INFINITY)),
        None => {
            let base = outer.unwrap_or(0.0);
            (
                base,
                base + per_target * weighted_count(over_cap, max_targets),
            )
        }
    };
    Some(AdaptiveRecharge {
        base,
        per_target,
        ceiling,
        max_targets,
        over_cap,
    })
}

/// The base recharge every non-proc reader should use: the adaptive value for `targets` when the
/// power has the rule, else `stats.recharge` as before.
pub fn base_recharge(power: &Power, targets: Option<u32>) -> Option<f64> {
    match adaptive_recharge(power) {
        Some(rule) => Some(rule.base_for(targets)).filter(|r| *r > 0.0),
        None => crate::projection::truthy_stat(power, "recharge", "recharge"),
    }
}
