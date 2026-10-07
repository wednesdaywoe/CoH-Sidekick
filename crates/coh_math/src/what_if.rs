//! The what-if TEAM-BUFF layer — pretending a teammate is buffing the build, injected into the
//! accumulators before projection.
//!
//! **Why the injection is here and not at a display surface.** The chain modal's recharge
//! what-if works because [`crate::chain::ChainPower`] deliberately keeps `base_recharge` and
//! `recharge_enhancement` apart and re-folds them at schedule time against the archetype's own
//! `StrengthBounds`, so the what-if re-enters the math at exactly the point the build's real
//! global does and the clamp still binds. Damage has no such seam:
//! [`crate::chain_build`] hands the chain `component.total.final`, with enhancement, globals and
//! the per-power damage cap already folded. Scaling that by `(1 + buff)` in a UI would be wrong
//! twice — buffs are additive with enhancement inside the strength multiplier (`1 + enh +
//! global`), not multiplicative on the enhanced value, and it would walk straight past the cap
//! [`crate::damage`] binds. So the layer lands in [`GlobalBonuses`] at the same point the
//! build's own globals do: one injection, every surface consistent, every archetype ceiling
//! binding for free (decision 2026-08-01).
//!
//! **The vocabulary is asked for, not written down.** A what-if entry names a `GlobalBonuses`
//! field, and [`vocabulary`] derives the legal names by asking the accumulator two questions it
//! already answers — does `add_by_camel_name` route it, and is it an accumulation rather than a
//! baseline or a derived rate ([`GlobalBonuses::is_attributable`])? A stat with no accumulator
//! behind it is therefore ABSENT from the layer rather than present as a control that changes
//! nothing, and a stat the export grows arrives with no edit here.

use crate::totals::{CalcError, GlobalBonuses, TypeRoute};
use std::collections::BTreeMap;

/// The stat names a what-if entry may use, derived from the accumulator rather than listed.
///
/// Two filters, both of them existing accumulator vocabulary:
///
/// - `add_by_camel_name` must ROUTE the name. This is what makes an unmodelled stat absent
///   instead of inert — the plan's requirement, and the same posture `stat_registry` takes
///   toward its two deliberate absences.
/// - the name must be ATTRIBUTABLE. The unattributable keys are baselines (`baseToHit`, the
///   seven `strength*` multipliers) and derived rates (`netEndPerSec`) — quantities the calc
///   reads or computes rather than sums, so "add some" is a category error even where the
///   routing arm happens to exist.
///
/// Sorted, and rebuilt on each call from `BREAKDOWN_KEYS`; the caller is a UI that renders it
/// once per open, not a hot loop.
pub fn vocabulary() -> Vec<&'static str> {
    let mut probe = GlobalBonuses::default();
    let mut names: Vec<&'static str> = GlobalBonuses::BREAKDOWN_KEYS
        .iter()
        .copied()
        .filter(|key| GlobalBonuses::is_attributable(key))
        .filter(|key| matches!(probe.add_by_camel_name(key, 0.0), TypeRoute::Routed))
        .collect();
    names.sort_unstable();
    names
}

/// Inject `layer` into `bonuses`, returning one [`CalcError`] per entry that did not land.
///
/// Runs at the point every real global source has summed and nothing has yet been projected or
/// clamped, so a what-if is indistinguishable downstream from the equivalent real buff — which
/// is the property WIF9's gate grades.
///
/// A name the accumulator does not route is an ERROR, not a skip (Rule 1). The layer's keys
/// come from [`vocabulary`], so an unroutable one means the two have drifted apart — a surface
/// offering a control that reaches nothing. Zero-valued entries are dropped silently: a slider
/// resting at zero is the absence of a what-if, not a request to add nothing.
///
/// Returns [`Applied`] rather than a bare error list, because "which numbers on this screen are
/// simulated" is provenance the display needs and only this call knows.
pub fn apply(bonuses: &mut GlobalBonuses, layer: &BTreeMap<String, f64>) -> Applied {
    let mut applied = Applied::default();
    for (stat, magnitude) in layer {
        if *magnitude == 0.0 {
            continue;
        }
        match bonuses.add_by_camel_name(stat, *magnitude) {
            TypeRoute::Routed => {
                applied.moved.insert(stat.clone(), *magnitude);
            }
            TypeRoute::Unspent(reason) => applied.errors.push(CalcError::new(
                stat.clone(),
                format!("what-if buff not spent: {reason}"),
            )),
            TypeRoute::Unknown => applied.errors.push(CalcError::new(
                stat.clone(),
                "what-if buff names no GlobalBonuses field — the layer's vocabulary and the \
                 accumulator have drifted apart",
            )),
        }
    }
    applied
}

/// What the injection actually did: the entries that LANDED, and the errors for those that did
/// not.
///
/// `moved` is the provenance every display surface needs, and it is measured here rather than
/// re-derived from the build. A surface asking "is this number simulated?" by re-reading
/// `CombatContext::what_if_buffs` would be a second answer to the same question, free to drift
/// from the one the totals were actually computed with — a zero entry, an unroutable name, or a
/// totals snapshot taken before the last slider move would each make the marker lie.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct Applied {
    /// The accumulator keys the layer moved, and by how much.
    pub moved: BTreeMap<String, f64>,
    /// One error per entry that named nothing, or named something the calc does not spend.
    pub errors: Vec<CalcError>,
}

impl Applied {
    /// Whether any of `keys` was moved by the layer — the question a stat row asks about its own
    /// [`GlobalBonuses`] ledger keys before drawing a simulated marker.
    pub fn touches(&self, keys: &[&str]) -> bool {
        keys.iter().any(|key| self.moved.contains_key(*key))
    }

    /// Whether anything at all is being simulated.
    ///
    /// Deliberately NOT called `is_empty`: this struct holds two collections, so `is_empty`
    /// would read as "no errors" at half its call sites and mean the opposite — the silent
    /// inversion that turns a fail-loud check into an always-true one.
    pub fn simulates_nothing(&self) -> bool {
        self.moved.is_empty()
    }
}
