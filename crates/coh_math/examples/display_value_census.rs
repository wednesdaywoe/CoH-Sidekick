//! The value half's census: for every power of all three bundles, the authored `effects` bag
//! against [`bag_slots`]'s projected value objects, key by key and leaf by leaf.
//!
//! Prints the divergence population per key and per leaf path, with a worked example each, so
//! the causes get named before the gate pins them. Presence is already gated
//! (`display_slot_presence_atom_bag_parity`); everything here is a value that disagrees on a key
//! BOTH sides state.

use coh_data::{DatasetId, Power, PowerDatabase};
use coh_math::window_slots::bag_slots;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

const FORKS: [DatasetId; DatasetId::ALL.len()] = DatasetId::ALL;

/// Def fields sharing the `effects` map — no atom router writes them (see the presence gate).
const EXECUTION_STATS: &[&str] = &[
    "accuracy",
    "activatePeriod",
    "activationTime",
    "arc",
    "castTime",
    "damage",
    "durations",
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
    // `summon`'s value is `extractSummon`'s, built from pet parameters outside the projection.
    "summon",
    // Both are typed fields of the projection rather than members of `values`.
    "buffDuration",
    "effectDuration",
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

fn roster(db: &PowerDatabase) -> Vec<String> {
    db.archetypes()
        .expect("archetype catalog")
        .all()
        .iter()
        .filter_map(|at| db.class_name_of(&at.id).map(str::to_owned))
        .collect()
}

/// Deep value comparison, with the float tolerance a JS→Rust arithmetic round trip needs.
/// Reports the leaf paths that disagree rather than a whole-object verdict.
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
        _ => out.push(format!("{path} shape")),
    }
}

/// Does the authored value carry a `perTarget` anywhere in it — i.e. did `mergeStackingPatches`
/// rebuild this slot after `projectAtomsToEffects` wrote it?
fn mentions_per_target(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(k, v)| {
            k == "perTarget" || k == "maxHPFractionPerTarget" || mentions_per_target(v)
        }),
        Value::Array(items) => items.iter().any(mentions_per_target),
        _ => false,
    }
}

/// Did the stacking post-pass run over this power at all? `computeAoePerTargetPatches` stamps
/// the increment it found on the templates it read, and the wire carries that stamp, so an atom
/// with `per_target` says the pass produced patches — and a patch REBUILDS its slot as
/// `{scale, table, perTarget}`, dropping whatever marks `projectAtomsToEffects` had written and
/// substituting the pass's own scale. Where the increment came out undefined the rebuilt object
/// carries no `perTarget` at all, which is why the mark-only divergences look causeless until
/// this is asked.
fn stacking_reshaped(power: &Power) -> bool {
    power.atoms.iter().any(|a| a.per_target.is_some())
}

fn main() {
    let mut by_leaf: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut by_key: BTreeMap<String, usize> = BTreeMap::new();
    let mut graded = 0usize;
    let mut clean = 0usize;
    // The divergences that are NOT the per-foe post-pass rebuilding the slot, which is the
    // population a value mirror of `projectAtomsToEffects` is answerable for.
    let mut off_pass: Vec<String> = Vec::new();

    for dataset in FORKS {
        let db = load(dataset);
        let classes = roster(&db);
        let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();

        for power in corpus(&db) {
            let Some(authored) = power
                .extra
                .get("effects")
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };
            let projected = bag_slots(&power, dataset, &class_refs);
            let ident = power.ident().to_string();
            for (key, bag_value) in authored {
                if EXECUTION_STATS.contains(&key.as_str()) {
                    continue;
                }
                let Some(atom_value) = projected.values.get(key) else {
                    continue; // a presence question, and the presence gate owns it
                };
                graded += 1;
                let mut out = Vec::new();
                diff(key, bag_value, atom_value, &mut out);
                if out.is_empty() {
                    clean += 1;
                    continue;
                }
                *by_key.entry(key.clone()).or_default() += 1;
                if !mentions_per_target(bag_value) && !stacking_reshaped(&power) {
                    off_pass.push(format!("{dataset:?} {ident} {key}: {}", out.join("; ")));
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
                    let entry = by_leaf
                        .entry(format!("{generic} {detail}"))
                        .or_insert((0, format!("{dataset:?} {ident}")));
                    entry.0 += 1;
                }
            }
        }
    }

    println!(
        "{} divergences on slots the per-foe pass never touched:",
        off_pass.len()
    );
    for line in &off_pass {
        println!("   {line}");
    }
    println!(
        "graded {graded} slot values, {clean} identical, {} divergent",
        graded - clean
    );
    println!("\n--- by key ---");
    let mut keys: Vec<_> = by_key.iter().collect();
    keys.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (key, n) in keys {
        println!("{n:6}  {key}");
    }
    println!("\n--- by leaf ---");
    let mut leaves: Vec<_> = by_leaf.iter().collect();
    leaves.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (leaf, (n, example)) in leaves.iter().take(60) {
        println!("{n:6}  {leaf}   e.g. {example}");
    }
}
