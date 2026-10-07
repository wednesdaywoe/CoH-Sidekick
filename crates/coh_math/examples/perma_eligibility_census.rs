//! PERMA-2: the oracle half of the perma-eligibility census.
//!
//! Emits one JSONL row per power per fork carrying `coh_math::perma::is_perma_eligible`'s verdict
//! and the three inputs it turns on — the recharge, the caster-side window the atoms open, and
//! whether the power damages. The TS half (`scripts/keys/perma2-ts-census.cjs`) emitted the same
//! keys off the same bundle, so the two joined row-for-row and the diff was the population whose
//! eligibility MOVES when the bag-bound predicate is ported. That half read `src/` and was deleted
//! on 2026-09-25, so this example now emits one side of a join with nothing to join against; the
//! verdict it was measured against is in `scripts/keys/README.md`. PERMA-2's port is closed, so
//! nothing is owed unless the row is reopened.
//!
//! Both verdicts are reported: `rust` under the archetype recharge clamp every player class of
//! every dataset carries (floor 0.25 / cap 5.0, the constant `perma_window_corpus` pins), and
//! `rust_unbounded` with the reachability arm left out entirely. The pair separates a disagreement
//! about caster state from one about the ceiling, which the TS side spells as its own
//! `PRACTICAL_RECHARGE_CAP` rather than as a clamp.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example perma_eligibility_census`

use coh_data::{DatasetId, Power, PowerDatabase};
use coh_math::projection::StrengthBounds;
use coh_math::window_slots::window_slots;
use std::path::PathBuf;

/// Every player class of every dataset: the −75% recharge-debuff floor and the +400% cap.
/// Same constant the window corpus grades the rings under.
const RECHARGE: StrengthBounds = StrengthBounds {
    floor: 0.25,
    cap: 5.0,
};

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn row(dataset: DatasetId, set_id: &str, source: &str, index: usize, power: &Power) {
    let slots = window_slots(power, dataset);
    let window = coh_math::perma::self_state_window_from_atoms(power, dataset);
    let recharge = power
        .extra
        .get("stats")
        .and_then(|s| s.get("recharge"))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0);
    println!(
        "{}",
        serde_json::json!({
            "ds": dataset.as_str(),
            "set": set_id,
            "src": source,
            "i": index,
            "ident": power.ident(),
            "name": power.name,
            "type": power.extra.get("powerType").and_then(serde_json::Value::as_str),
            "rech": recharge,
            "win": window,
            "dmg": power.extra.get("damage").is_some_and(|d| !d.is_null()),
            "slots": slots.durations.len(),
            "rust": coh_math::perma::is_perma_eligible(power, Some(RECHARGE), dataset),
            "rust_unbounded": coh_math::perma::is_perma_eligible(power, None, dataset),
        })
    );
}

/// `--detail <ident>`: the per-key window slots behind one power's verdict, which is what an
/// adjudication of a candidate/oracle divergence needs — the census row alone says the two
/// disagree, not which key carried the window.
fn detail(dataset: DatasetId, power: &Power) {
    let slots = window_slots(power, dataset);
    println!(
        "{}/{} type={:?} targetType={:?} targetsAffected={:?} rech={} window={}",
        dataset.as_str(),
        power.ident(),
        power
            .extra
            .get("powerType")
            .and_then(serde_json::Value::as_str),
        power
            .extra
            .get("targetType")
            .and_then(serde_json::Value::as_str),
        power.targets_affected(),
        power
            .extra
            .get("stats")
            .and_then(|s| s.get("recharge"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        coh_math::perma::self_state_window_from_atoms(power, dataset),
    );
    println!("  present: {:?}", slots.present);
    println!(
        "  self_marked: {:?} slow_self={}",
        slots.self_marked, slots.slow_self
    );
    for (key, seconds) in &slots.durations {
        println!(
            "  duration {key}={seconds} self_buff_key={}",
            coh_math::perma::SELF_BUFF_KEYS.contains(key),
        );
    }
    for atom in &power.atoms {
        if atom.duration.is_none_or(|d| d <= 0.0) && atom.summon_window.is_none() {
            continue;
        }
        println!(
            "  atom {:?}/{:?} aspect={:?} scale={:?} table={:?} dur={:?} summon={:?} to={:?} gated={:?} owner={:?}",
            atom.effect_type,
            atom.sub_type,
            atom.aspect,
            atom.scale,
            atom.modifier_table,
            atom.duration,
            atom.summon_window,
            atom.to_who,
            atom.gated,
            atom.owner_targets,
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--detail") {
        let want = args[pos + 1].clone();
        for dataset in DatasetId::ALL {
            let db = load(dataset);
            for power in db.all_powers().chain(db.inherent_powers.iter()) {
                if power.ident() == want {
                    detail(dataset, power);
                }
            }
        }
        return;
    }
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        for powerset in &db.powersets {
            for (i, power) in powerset.powers.iter().enumerate() {
                row(dataset, &powerset.id, "powerset", i, power);
            }
        }
        for (i, partition) in db.pool_powers.iter().enumerate() {
            row(dataset, &partition.set_id, "pool", i, &partition.power);
        }
        for (i, partition) in db.epic_powers.iter().enumerate() {
            row(dataset, &partition.set_id, "epic", i, &partition.power);
        }
        for (i, power) in db.inherent_powers.iter().enumerate() {
            row(dataset, "", "inherent", i, power);
        }
    }
}
