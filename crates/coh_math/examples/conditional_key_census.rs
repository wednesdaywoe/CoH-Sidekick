//! Does the atom-side projection reproduce the key set the conditional adjusters read off the
//! bag? Measured over every per-power `conditionalEffects` entry of all three bundles.
//!
//! [`crate::adjusters`] asks the bag two questions and never asks it for a value: which keys the
//! power's BASE bag carries (the collision test that splits `adds` from `extra_instances`), and
//! which keys one conditional ENTRY's own `effects` object carries. Both are key sets, so the
//! grade here is a set difference rather than a number — there is no totals oracle behind this
//! surface, and no arithmetic to drift.
//!
//! The atom side answers from [`coh_math::window_slots::slots_over`], the converter's own
//! projection run over a chosen subset of the power's atoms: the bag's subset for the base, and
//! the atoms an entry claims (`AtomicEffect::conditional_id`) for each entry.
//!
//! Both directions are reported, because they fail differently and only one of them is loud:
//!   * BAG-ONLY — the bag has a key the atoms do not project. This is the migration's real
//!     failure: the adjuster would stop naming a contribution it names today, silently, as an
//!     empty set rather than an error (Rule 1).
//!   * ATOM-ONLY — the atoms project a key the bag never wrote. Not automatically a defect: the
//!     bag drops what a single-valued slot cannot hold, which is the asymmetry the whole atom
//!     migration exists to correct. Reported separately so it is adjudicated, never averaged in.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example conditional_key_census`

use coh_data::{AtomicEffect, DatasetId, Power, PowerDatabase};
use coh_math::window_slots::slots_over;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Wire fields inside an entry's `effects` object that name no effect of their own: a duration
/// belongs to the effect it times, and the adjuster skips it for that reason.
const NOT_A_CONTRIBUTION: [&str; 1] = ["durations"];

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The keys a bag-shaped object carries, minus the ones that time an effect rather than being one.
fn bag_keys(effects: Option<&Value>) -> BTreeSet<String> {
    effects
        .and_then(Value::as_object)
        .map(|map| {
            map.keys()
                .filter(|k| !NOT_A_CONTRIBUTION.contains(&k.as_str()))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn atom_keys(
    power: &Power,
    dataset: DatasetId,
    select: impl FnMut(&AtomicEffect) -> bool,
) -> BTreeSet<String> {
    let slots = slots_over(power, dataset, select);
    let mut keys: BTreeSet<String> = slots
        .present
        .iter()
        .filter(|k| !NOT_A_CONTRIBUTION.contains(*k))
        .map(|k| k.to_string())
        .collect();
    // `buffDuration` and `effectDuration` ARE bag keys, and the mirror holds them as fields of
    // their own rather than in `present` because the window consumers read them as fallbacks.
    // Mapped back here so the comparison is against the bag's actual key set and not against
    // the mirror's internal shape.
    if slots.buff_duration.is_some() {
        keys.insert("buffDuration".to_string());
    }
    if slots.effect_duration.is_some() {
        keys.insert("effectDuration".to_string());
    }
    keys
}

#[derive(Default)]
struct Tally {
    subjects: usize,
    agree: usize,
    bag_only: BTreeMap<String, Vec<String>>,
    atom_only: BTreeMap<String, Vec<String>>,
}

impl Tally {
    fn record(&mut self, subject: &str, bag: &BTreeSet<String>, atoms: &BTreeSet<String>) {
        self.subjects += 1;
        if bag == atoms {
            self.agree += 1;
            return;
        }
        for k in bag.difference(atoms) {
            self.bag_only
                .entry(k.clone())
                .or_default()
                .push(subject.to_string());
        }
        for k in atoms.difference(bag) {
            self.atom_only
                .entry(k.clone())
                .or_default()
                .push(subject.to_string());
        }
    }

    fn report(&self, label: &str) {
        println!(
            "  {label}: {}/{} agree, {} diverge",
            self.agree,
            self.subjects,
            self.subjects - self.agree
        );
        for (dir, map) in [("BAG-ONLY", &self.bag_only), ("ATOM-ONLY", &self.atom_only)] {
            if map.is_empty() {
                continue;
            }
            println!("    {dir}:");
            for (key, subjects) in map {
                let sample: Vec<&str> = subjects.iter().take(4).map(String::as_str).collect();
                println!(
                    "      {key} × {} — {}{}",
                    subjects.len(),
                    sample.join(", "),
                    if subjects.len() > sample.len() {
                        ", …"
                    } else {
                        ""
                    }
                );
            }
        }
    }
}

fn main() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let mut base = Tally::default();
        let mut entries = Tally::default();
        let mut unclaimed = 0usize;
        let mut orphan_ids: BTreeMap<String, usize> = BTreeMap::new();

        // Powersets, pools AND epics — the three partitions a power lives in. Walking
        // `db.powersets` alone leaves out ~490 powers per fork, which is COND-2's hole moved
        // up into the measurement (`adjuster_corpus::the_sweep_reaches_every_partition` is the
        // standing guard on the same point).
        let powersets = db.powersets.iter().flat_map(|set| set.powers.iter());
        let pools = db.pool_powers.iter().map(|held| &held.power);
        let epics = db.epic_powers.iter().map(|held| &held.power);
        {
            for power in powersets.chain(pools).chain(epics) {
                let name = power.internal_name.as_deref().unwrap_or("(unnamed)");
                let conditionals = power
                    .extra
                    .get("conditionalEffects")
                    .and_then(Value::as_array)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let per_power: Vec<&Value> = conditionals
                    .iter()
                    .filter(|e| e.get("scope").and_then(Value::as_str) != Some("global"))
                    .collect();
                if per_power.is_empty() {
                    continue;
                }

                // The base collision surface, on the bag's own atom subset.
                base.record(
                    name,
                    &bag_keys(power.extra.get("effects")),
                    &atom_keys(power, dataset, |a| {
                        !a.is_gated() && a.caster_archetypes.is_none() && !a.is_deactivation_burst()
                    }),
                );

                let claimed: BTreeSet<&str> = power
                    .atoms
                    .iter()
                    .filter_map(|a| a.conditional_id.as_deref())
                    .collect();
                for entry in &per_power {
                    let Some(id) = entry.get("id").and_then(Value::as_str) else {
                        continue;
                    };
                    if !claimed.contains(id) {
                        unclaimed += 1;
                        println!("  UNCLAIMED {name}:{id} — entry joins no atom");
                    }
                    entries.record(
                        &format!("{name}:{id}"),
                        &bag_keys(entry.get("effects")),
                        &atom_keys(power, dataset, |a| a.conditional_id.as_deref() == Some(id)),
                    );
                }
                // An id on an atom that no per-power entry carries: either a global-scoped
                // entry's (legitimate — those are caster state, not adjusters) or a stamp
                // pointing at nothing.
                let entry_ids: BTreeSet<&str> = conditionals
                    .iter()
                    .filter_map(|e| e.get("id").and_then(Value::as_str))
                    .collect();
                for id in claimed.difference(&entry_ids) {
                    *orphan_ids.entry((*id).to_string()).or_default() += 1;
                }
            }
        }

        println!("{}:", dataset.as_str());
        base.report("base key set");
        entries.report("entry key set");
        println!("  entries joining no atom: {unclaimed}");
        if !orphan_ids.is_empty() {
            println!("  atom ids with no entry: {orphan_ids:?}");
        }
    }
}
