//! The six presence disagreements the display census left unnamed.
//!
//! The display item's value work extends `window_slots` from "which keys does this power's atom
//! stream project" to "and what value does each carry". That inherits the router's presence
//! verdict wholesale, so a key the router already gets wrong would ship its value wrong too. The
//! census measured those disagreements against the AUTHORED bag — 12 of 47 `absorb`, plus single
//! digits on `summon`, `effectDuration`, `enduranceGain`, `stun`, and `regenBuffUnenhanced`
//! writing a key the bag doesn't — and named none of them.
//!
//! This prints, per key and per fork, the powers on both sides of the disagreement with the atoms
//! that could have routed there, so each one gets a cause rather than a count.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example display_residual_probe`

use coh_data::{DatasetId, EffectType, Power, PowerDatabase};
use serde_json::Value;
use std::path::PathBuf;

/// The keys under investigation, each with the atom families that could route to it. Printing
/// every atom of a disagreeing power buries the answer under forty resistance rows; printing the
/// families the router tests for this key is the actual evidence. `None` in the list means "print
/// all", for the keys whose routing is not family-scoped.
///
/// Deliberately a fixed list rather than "every key that disagrees": the execution stats and the
/// stacking metadata disagree by the hundreds and are a different job (they are def fields and
/// converter verdicts, projections of no atom).
const KEYS: [(&str, &[EffectType]); 8] = [
    ("absorb", &[EffectType::Absorb]),
    ("summon", &[EffectType::EntCreate]),
    ("effectDuration", &[EffectType::Mez]),
    (
        "enduranceGain",
        &[EffectType::Endurance, EffectType::Recovery],
    ),
    ("stun", &[EffectType::Mez]),
    ("regenBuffUnenhanced", &[EffectType::Regeneration]),
    ("immobilize", &[EffectType::Mez]),
    ("sleep", &[EffectType::Mez]),
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

/// One atom's routing-relevant axes, in the order the router tests them.
fn describe(a: &coh_data::AtomicEffect) -> String {
    format!(
        "type={:?} sub={:?} asp={:?} attrib={:?} scale={:?} mag={:?} dur={:?} table={:?} \
         toWho={:?} ignoreStr={:?} stacking={:?} stackKey={:?} gated={:?} fork={:?} deact={}",
        a.effect_type,
        a.sub_type,
        a.aspect,
        a.attrib_type,
        a.scale,
        a.magnitude,
        a.duration,
        a.modifier_table,
        a.to_who,
        a.ignore_strength,
        a.stacking,
        a.stack_key,
        a.gated,
        a.caster_archetypes,
        a.is_deactivation_burst(),
    )
}

fn main() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let classes: Vec<String> = db
            .archetypes()
            .expect("archetype catalog")
            .all()
            .iter()
            .filter_map(|at| db.class_name_of(&at.id).map(str::to_owned))
            .collect();
        let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();
        println!("\n================ {dataset:?} ================");
        for (key, families) in KEYS {
            let mut bag_only: Vec<(String, Vec<String>)> = Vec::new();
            let mut atom_only: Vec<(String, Vec<String>)> = Vec::new();
            for power in &corpus(&db) {
                let authored_value = power
                    .extra
                    .get("effects")
                    .and_then(Value::as_object)
                    .and_then(|o| o.get(key));
                let authored = authored_value.is_some();
                let projected = coh_math::window_slots::bag_slots(power, dataset, &class_refs)
                    .keys()
                    .contains(key);
                if authored == projected {
                    continue;
                }
                // Every atom of the family that could reach this key, whatever the bag subset
                // filter says: a disagreement is as often the FILTER dropping a row as the
                // routing branch declining it, and printing only the surviving atoms would hide
                // exactly that half.
                let rows: Vec<String> = power
                    .atoms
                    .iter()
                    .filter(|a| a.effect_type.is_some_and(|t| families.contains(&t)))
                    .map(describe)
                    .collect();
                let entry = (
                    format!(
                        "{} [{}]  bag={}",
                        power.name,
                        power.ident(),
                        authored_value
                            .map(|v| serde_json::to_string(v).unwrap_or_default())
                            .unwrap_or_else(|| "-".to_string())
                    ),
                    rows,
                );
                if authored {
                    bag_only.push(entry);
                } else {
                    atom_only.push(entry);
                }
            }
            if bag_only.is_empty() && atom_only.is_empty() {
                continue;
            }
            println!(
                "\n--- {key}: {} bag-only, {} atom-only ---",
                bag_only.len(),
                atom_only.len()
            );
            for (label, side) in [("BAG-ONLY", &bag_only), ("ATOM-ONLY", &atom_only)] {
                for (power, rows) in side.iter().take(14) {
                    println!("  {label} {power}");
                    for row in rows {
                        println!("      {row}");
                    }
                }
            }
        }
    }
}
