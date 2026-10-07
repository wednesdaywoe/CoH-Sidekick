//! Per-power adjusters — the controls for state a power tracks about ITSELF.
//!
//! The build-wide half of this vocabulary is derived in [`coh_data::caster_state`] and decided
//! once for the whole character: which form it is in, which caster modes are live, which global
//! mechanic is running. What remains is the per-power half — a foe disintegrating under Beam
//! Rifle, drowning under Water Blast, contaminated by Radiation Melee. That state is not the
//! caster's, so two powers pointed at two different foes disagree about it, which is why it is
//! keyed `"<internalName>:<id>"` in [`CombatContext::per_power_conditionals`] and why its
//! control belongs beside the power that tracks it.
//!
//! Everything here is read out of the power's own `conditionalEffects` — an adjuster exists
//! exactly when the export declares one, and it is labelled as the export labels it (Rule 0).
//! What it contributes is named through [`crate::effect_registry`], the same vocabulary the
//! projection's rows are drawn from, so a control and the row it moves cannot describe the
//! same effect differently.
//!
//! [`CombatContext::per_power_conditionals`]: coh_data::CombatContext::per_power_conditionals

use crate::effect_registry;
use crate::window_slots::{slots_over, window_slots};
use coh_data::{AtomicEffect, CharacterState, DatasetId, Power};
use serde_json::Value;
use std::collections::BTreeSet;

/// One adjustable conditional on one power.
///
/// The contribution fields are what makes a control honest: the projection merges an active
/// conditional onto the power ([`crate::effective`]), and the merge has three outcomes a reader
/// would otherwise have to guess at — a value that appears, a value that supersedes the base
/// one, and a second simultaneous instance the merge deliberately refuses to fold into one
/// stronger row. The third shows no number at all, so it is named rather than left to look
/// like a toggle that does nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerAdjuster {
    /// The [`CombatContext::per_power_conditionals`] key this control writes.
    ///
    /// [`CombatContext::per_power_conditionals`]: coh_data::CombatContext::per_power_conditionals
    pub key: String,
    /// The conditional's own id, as the export spells it.
    pub id: String,
    /// The converter's own label for it.
    pub label: String,
    /// Whether it resolves on for this build — the build's toggle, or the entry's own
    /// `defaultActive` where the build has never touched it.
    pub active: bool,
    /// The mutual-exclusion group the export puts it in. Adjusters sharing one are a choice
    /// between states, not a set of independent switches — a target is at one combo level, not
    /// at all three.
    pub group: Option<String>,
    /// The effects it brings that the projection will show, in the registry's labels.
    pub adds: Vec<String>,
    /// Whether it adds a damage component. Damage is a wire field of its own rather than an
    /// `effects` key, so it has no registry label to answer for it.
    pub adds_damage: bool,
    /// Effects it casts a SECOND simultaneous instance of. The additive merge keeps the base
    /// value on a colliding key — two instances of a hold are not one stronger hold — so no
    /// projected row can carry these, and the control has to say so itself.
    pub extra_instances: Vec<String>,
    /// Effects it brings that the DISPLAY vocabulary has no label for, by their wire key.
    ///
    /// Not bookkeeping and not nothing: the `*Unenhanced` halves are read by the apply pass
    /// (`apply.rs`'s unenhanced-half fallback) and move real numbers, they simply have no row of
    /// their own in [`crate::effect_registry`] — they are summed into their enhanced sibling's.
    /// Naming the raw key is worse than naming a label and better than dropping a contribution
    /// the control does make (Rule 1).
    pub unlabelled: Vec<String>,
    /// Whether its values supersede the base ones they collide with (`mode: "replace"`) rather
    /// than standing beside them.
    pub replaces: bool,
}

impl PowerAdjuster {
    /// Whether the export declares this state but carries nothing to apply for it.
    ///
    /// Oil Slick Arrow's "ignited" is one on all three forks: an id, a label, and no payload at
    /// all. A control for it could not move a number, so the surface shows it as declared-but-
    /// inert rather than offering a switch that does nothing (Rule 1 — the gap is stated, not
    /// hidden).
    pub fn is_inert(&self) -> bool {
        !self.shows_something() && self.extra_instances.is_empty()
    }

    /// Whether turning it on puts something new in front of the reader — which is not the same
    /// as it being worth a control. A conditional whose whole contribution collides with the
    /// base casts a second simultaneous instance the merge refuses to fold into one row, so it
    /// shows nothing while still being a state worth recording.
    pub fn shows_something(&self) -> bool {
        !self.adds.is_empty() || self.adds_damage || !self.unlabelled.is_empty()
    }
}

/// The bag keys the power's BASE atoms project — the collision surface an additive conditional
/// is tested against.
///
/// The bag's own atom subset, as [`crate::window_slots`] defines it: ungated, unforked, minus
/// the `OnDeactivate` rows the converter's routing loop skips.
///
/// This answers a NARROWER question than the bag object it replaces, and deliberately so. A
/// power's `effects` also carries its execution stats — `castTime`, `recharge`, `radius`,
/// `maxStacks` and a dozen more — which are def fields sharing the map, not projections of any
/// atom. They are absent here, and the collision verdict is unchanged because no conditional on
/// any fork carries a key that is one of them: `summon` was the single overlap, and the mirror
/// projects it. `conditional_key_census` is the measurement, `adjuster_atom_bag_parity` the
/// standing guard.
fn base_keys(power: &Power, dataset: DatasetId) -> BTreeSet<&'static str> {
    // `window_slots` IS this subset (`_bagTemplates`, minus the deactivation bursts the routing
    // loop skips), and it carries the two Thunderspy target-trap guards, which belong to the
    // base bag: the converter runs them on the finished power's `effects` and never on an
    // entry's, so the base loses a trapped key here exactly as it does there.
    window_slots(power, dataset).keys()
}

/// The bag keys ONE conditional entry's atoms project.
///
/// The join is [`AtomicEffect::conditional_id`], stamped by the converter on the templates of
/// the group each entry was built from. An entry whose atoms are absent projects nothing, which
/// is the honest answer for the payload-less entries the export declares (Oil Slick Arrow's
/// "ignited" is one on every fork) and is what [`PowerAdjuster::is_inert`] already reports.
fn entry_keys(power: &Power, dataset: DatasetId, id: &str) -> BTreeSet<&'static str> {
    slots_over(power, dataset, |a: &AtomicEffect| {
        a.conditional_id.as_deref() == Some(id)
    })
    .keys()
}

/// Every per-power adjuster `power` declares, in the export's own order.
///
/// `scope: "global"` entries are skipped: those are caster state, decided once for the build
/// through [`coh_data::global_mechanics`] and its stance selectors. Offering them a second time
/// here would be two controls over one value that can disagree.
///
/// So are the entries an archetype fork puts outside this build's class — `caster_class` is the
/// build's own class token ([`coh_data::caster_class_name`]), and a control for a state the
/// caster's class cannot be in is a control that can only produce a wrong number (COND-4).
pub fn power_adjusters(
    power: &Power,
    state: &CharacterState,
    caster_class: Option<&str>,
) -> Vec<PowerAdjuster> {
    let internal_name = power.internal_name.as_deref().unwrap_or_default();
    let base = base_keys(power, state.dataset);

    conditional_entries(power)
        .filter(|entry| coh_data::conditional_for_class(entry, caster_class))
        .filter(|entry| entry.get("scope").and_then(Value::as_str) != Some("global"))
        .filter_map(|entry| {
            let id = entry.get("id").and_then(Value::as_str)?;
            let key = format!("{internal_name}:{id}");
            let replaces = entry.get("mode").and_then(Value::as_str) == Some("replace");

            let mut adds = Vec::new();
            let mut extra_instances = Vec::new();
            let mut unlabelled = Vec::new();
            for effect_key in entry_keys(power, state.dataset, id) {
                // The additive merge fills only keys the base LACKS, so a colliding one changes
                // nothing at all — which is a different answer from "it changes something the
                // panel can't name", and the two must not be conflated.
                let collides = !replaces && base.contains(effect_key);
                match (collides, effect_registry::lookup(effect_key)) {
                    (true, Some(config)) => extra_instances.push(config.label.clone()),
                    (true, None) => {}
                    (false, Some(config)) => adds.push(config.label.clone()),
                    (false, None) => unlabelled.push(effect_key.to_string()),
                }
            }

            Some(PowerAdjuster {
                active: state
                    .combat
                    .per_power_conditionals
                    .get(&key)
                    .copied()
                    .unwrap_or_else(|| {
                        entry
                            .get("defaultActive")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    }),
                key,
                id: id.to_string(),
                label: entry
                    .get("label")
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .to_string(),
                group: entry
                    .get("group")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                adds,
                adds_damage: entry.get("damage").is_some_and(|damage| match damage {
                    Value::Array(entries) => !entries.is_empty(),
                    Value::Null => false,
                    _ => true,
                }),
                extra_instances,
                unlabelled,
                replaces,
            })
        })
        .collect()
}

/// The [`CombatContext::per_power_conditionals`] writes that flipping `key` implies.
///
/// An ungrouped adjuster is one write. A grouped one is a CHOICE: turning it on turns its
/// siblings off in the same act, because the export puts them in a group to say the target is
/// in one of those states, not in several. Returned rather than applied — this crate computes,
/// it does not own build state — so a surface lands the whole choice in a single commit and one
/// undo step.
///
/// [`CombatContext::per_power_conditionals`]: coh_data::CombatContext::per_power_conditionals
pub fn toggle_writes(adjusters: &[PowerAdjuster], key: &str, on: bool) -> Vec<(String, bool)> {
    let Some(chosen) = adjusters.iter().find(|adjuster| adjuster.key == key) else {
        return Vec::new();
    };
    let Some(group) = chosen.group.as_deref().filter(|_| on) else {
        return vec![(key.to_string(), on)];
    };
    adjusters
        .iter()
        .filter(|adjuster| adjuster.group.as_deref() == Some(group))
        .map(|adjuster| (adjuster.key.clone(), adjuster.key == key))
        .collect()
}

fn conditional_entries(power: &Power) -> impl Iterator<Item = &Value> {
    power
        .extra
        .get("conditionalEffects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}
