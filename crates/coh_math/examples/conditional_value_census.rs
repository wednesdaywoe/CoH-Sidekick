//! Each conditional entry's authored `effects` block against the same block projected from the
//! atoms the entry claims — the measurement behind moving
//! [`coh_math::effective::with_active_conditionals`] off the wire's nested bags.
//!
//! The join is `conditionalId`, the converter stamp the adjusters half already leans on
//! (`adjuster_atom_bag_parity` grades the KEY set from it; this grades the VALUES).

use coh_data::{AtomicEffect, DatasetId, Power, PowerDatabase};
use coh_math::window_slots::slots_over;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const FORKS: [DatasetId; DatasetId::ALL.len()] = DatasetId::ALL;

const NOT_PROJECTED: &[&str] = &[
    "accuracy",
    "activatePeriod",
    "activationTime",
    "arc",
    "castTime",
    "damage",
    "effectArea",
    "endurance",
    "enduranceCost",
    "interruptTime",
    "maxStacks",
    "maxTargets",
    "radius",
    "range",
    "recharge",
    "stackCaps",
    "stackInterval",
    "stacksLinear",
    "summon",
];

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn corpus(db: &PowerDatabase) -> Vec<Power> {
    db.all_powers()
        .cloned()
        .chain(db.inherent_powers.iter().cloned())
        .collect()
}

fn patched(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(k, v)| k == "perTarget" || k == "maxHPFractionPerTarget" || patched(v)),
        Value::Array(items) => items.iter().any(patched),
        _ => false,
    }
}

fn diff(path: &str, bag: &Value, atom: &Value, out: &mut Vec<String>) {
    match (bag, atom) {
        (Value::Object(b), Value::Object(a)) => {
            let mut keys: Vec<&String> = b.keys().chain(a.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                match (b.get(key), a.get(key)) {
                    (Some(bv), Some(av)) => diff(&format!("{path}.{key}"), bv, av, out),
                    (Some(_), None) => out.push(format!("{path}.{key} bag-only")),
                    (None, Some(_)) => out.push(format!("{path}.{key} atom-only")),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(b), Value::Array(a)) if b.len() == a.len() => {
            for (i, (bv, av)) in b.iter().zip(a).enumerate() {
                diff(&format!("{path}[{i}]"), bv, av, out);
            }
        }
        (Value::Number(b), Value::Number(a)) => {
            let (b, a) = (
                b.as_f64().unwrap_or(f64::NAN),
                a.as_f64().unwrap_or(f64::NAN),
            );
            if (b - a).abs() > 1e-6 * b.abs().max(a.abs()).max(1.0) {
                out.push(format!("{path} {b} vs {a}"));
            }
        }
        _ if bag == atom => {}
        _ => out.push(format!("{path} shape: {bag} vs {atom}")),
    }
}

fn main() {
    let mut entries = 0usize;
    let mut with_effects = 0usize;
    let mut joined = 0usize;
    let mut graded = 0usize;
    let mut clean = 0usize;
    let mut skipped_patched = 0usize;
    let mut by_leaf: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut unjoined: Vec<String> = Vec::new();

    for dataset in FORKS {
        let db = load(dataset);
        for power in corpus(&db) {
            let Some(list) = power
                .extra
                .get("conditionalEffects")
                .and_then(Value::as_array)
            else {
                continue;
            };
            let ident = power.ident().to_string();
            for entry in list {
                entries += 1;
                let Some(authored) = entry.get("effects").and_then(Value::as_object) else {
                    continue;
                };
                with_effects += 1;
                let Some(id) = entry.get("id").and_then(Value::as_str) else {
                    continue;
                };
                let claims = power
                    .atoms
                    .iter()
                    .any(|a: &AtomicEffect| a.conditional_id.as_deref() == Some(id));
                if !claims {
                    unjoined.push(format!("{dataset:?} {ident} {id}"));
                    continue;
                }
                joined += 1;
                let projected = slots_over(&power, dataset, |a: &AtomicEffect| {
                    a.conditional_id.as_deref() == Some(id)
                });
                let mut bag = projected.values.clone();
                if !projected.durations.is_empty() {
                    let durations: serde_json::Map<String, Value> = projected
                        .durations
                        .iter()
                        .map(|(k, d)| ((*k).to_owned(), Value::from(*d)))
                        .collect();
                    bag.insert("durations".into(), Value::Object(durations));
                }
                if let Some(d) = projected.buff_duration {
                    bag.insert("buffDuration".into(), Value::from(d));
                }
                if let Some(d) = projected.effect_duration {
                    bag.insert("effectDuration".into(), Value::from(d));
                }
                for (key, bag_value) in authored {
                    if NOT_PROJECTED.contains(&key.as_str()) {
                        continue;
                    }
                    if patched(bag_value) {
                        skipped_patched += 1;
                        continue;
                    }
                    let Some(atom_value) = bag.get(key) else {
                        by_leaf
                            .entry(format!("{key} slot missing"))
                            .or_insert((0, format!("{dataset:?} {ident} {id}")))
                            .0 += 1;
                        graded += 1;
                        continue;
                    };
                    graded += 1;
                    let mut out = Vec::new();
                    diff(key, bag_value, atom_value, &mut out);
                    if out.is_empty() {
                        clean += 1;
                        continue;
                    }
                    for leaf in out {
                        let (path, detail) = leaf
                            .split_once(' ')
                            .map_or((leaf.clone(), String::new()), |(p, d)| {
                                (p.to_owned(), d.to_owned())
                            });
                        let generic = path
                            .split('.')
                            .map(|seg| if seg.contains('[') { "[i]" } else { seg })
                            .collect::<Vec<_>>()
                            .join(".");
                        let detail = detail.split(':').next().unwrap_or("").to_owned();
                        by_leaf
                            .entry(format!("{generic} {detail}"))
                            .or_insert((0, format!("{dataset:?} {ident} {id}")))
                            .0 += 1;
                    }
                }
            }
        }
    }

    println!("{entries} entries, {with_effects} with an effects block, {joined} joined to atoms");
    println!(
        "{graded} values graded, {clean} identical, {skipped_patched} skipped (stacking pass)"
    );
    println!("\n--- unjoined entries ({}) ---", unjoined.len());
    for line in unjoined.iter().take(30) {
        println!("   {line}");
    }
    println!("\n--- divergent leaves ---");
    let mut leaves: Vec<_> = by_leaf.iter().collect();
    leaves.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (leaf, (n, example)) in leaves.iter().take(40) {
        println!("{n:6}  {leaf}   e.g. {example}");
    }
}
