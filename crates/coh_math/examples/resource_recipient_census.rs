//! Who does a regen/recovery buff slot actually land on? — measured against the corpus.
//!
//! The bag's four caster-facing resource slots (`regenBuff`, `regenBuffUnenhanced`,
//! `recoveryBuff`, `recoveryBuffUnenhanced`) are written by `extractEffects` on a routing rule
//! that asks the aspect and the sign of `scale`, and never asks WHO the template lands on. The
//! atom carries that answer directly (`reaches_caster`, the TARGETS-2/3 reading), so the two can
//! be graded against each other over every power of all three forks.
//!
//! Two sections, because they answer different questions.
//!
//! **Section 1 — the recipient partition.** Per power per slot:
//!
//! - `PHANTOM-LIVE` — the bag credits the caster, no base atom of that effect type reaches him,
//!   and the power is not ally-only. These are live wrong numbers.
//! - `PHANTOM-inert` — the same, but `is_ally_only` skips the whole power in the apply loop, so
//!   the slot never resolves. Counted, not listed: the guard already answers.
//! - `AGREE` — the bag credits and at least one base atom reaches.
//! - `ATOM-ONLY` — an atom reaches and the bag has no slot.
//!
//! **Section 2 — the reader's own answer**, one line per power per slot, printed from the REAL
//! `resource_buff_value` rather than a re-implementation. Diff two runs of this section across a
//! change to that reader and the diff IS the blast radius; a census that re-derived the reader
//! could only ever agree with it.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example resource_recipient_census`

use coh_data::{reaches_caster, AtomicEffect, DatasetId, EffectType, Power, PowerDatabase};
use coh_math::appliers::resources::{
    recovery_buff_unenhanced_value, recovery_buff_value, regen_buff_unenhanced_value,
    regen_buff_value,
};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The apply loop's `ALLY_ONLY_TARGET_TYPES` gate, mirrored so the census can partition by it —
/// an ally-only power never reaches the resource blocks at all, so its bag slot is inert.
const ALLY_ONLY_TARGET_TYPES: [&str; 7] = [
    "ally",
    "ally (alive)",
    "teammate",
    "dead teammate",
    "friend",
    "deadplayerfriend",
    "deadoraliveleaguemate",
];

fn target_type(p: &Power) -> &str {
    p.extra
        .get("targetType")
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn is_ally_only(p: &Power) -> bool {
    let t = target_type(p).to_ascii_lowercase();
    ALLY_ONLY_TARGET_TYPES.contains(&t.as_str())
}

/// The bag slot's stated scale, or `None` when the power carries no such slot.
fn bag_slot(power: &Power, slot: &str) -> Option<f64> {
    let effects = power.extra.get("effects")?.as_object()?;
    match effects.get(slot)? {
        Value::Number(n) => n.as_f64(),
        Value::Object(o) => o.get("scale").and_then(Value::as_f64),
        _ => None,
    }
}

/// The base atoms `resource_buff_value` would consider for one half of one resource family —
/// its own filter, minus the recipient question this census is asking about.
fn candidate_atoms<'a>(
    power: &'a Power,
    effect_type: EffectType,
    want_ignore_strength: bool,
) -> Vec<&'a AtomicEffect> {
    power
        .atoms
        .iter()
        .filter(|a| {
            a.effect_type == Some(effect_type)
                && a.gated != Some(true)
                && a.aspect != Some(coh_data::Aspect::Res)
                && a.not_on_caster != Some(true)
                && !(a.scale.is_some_and(|s| s < 0.0)
                    || a.modifier_table
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains("debuff")))
                && (a.ignore_strength == Some(true)) == want_ignore_strength
        })
        .collect()
}

const SLOTS: [(&str, EffectType, bool); 4] = [
    ("regenBuff", EffectType::Regeneration, false),
    ("regenBuffUnenhanced", EffectType::Regeneration, true),
    ("recoveryBuff", EffectType::Recovery, false),
    ("recoveryBuffUnenhanced", EffectType::Recovery, true),
];

fn reader(power: &Power, slot: &str) -> Option<String> {
    let v = match slot {
        "regenBuff" => regen_buff_value(power),
        "regenBuffUnenhanced" => regen_buff_unenhanced_value(power),
        "recoveryBuff" => recovery_buff_value(power),
        _ => recovery_buff_unenhanced_value(power),
    }?;
    Some(format!(
        "{:.4}|{}|{:?}",
        v.scale,
        v.table.as_deref().unwrap_or(""),
        v.per_target
    ))
}

fn main() {
    let mut reader_lines: Vec<String> = Vec::new();

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
        let mut phantoms: Vec<String> = Vec::new();
        let mut atom_only: Vec<String> = Vec::new();

        for power in db.all_powers() {
            for (slot, effect_type, want_is) in SLOTS {
                if let Some(answer) = reader(power, slot) {
                    reader_lines.push(format!(
                        "{:<12} {:<28} {:<24} {answer}",
                        dataset.as_str(),
                        power.name,
                        slot
                    ));
                }

                let bag = bag_slot(power, slot);
                let atoms = candidate_atoms(power, effect_type, want_is);
                let reaching = atoms.iter().filter(|a| reaches_caster(a, power)).count();
                let ally_only = is_ally_only(power);
                let verdict = match (bag.is_some(), reaching > 0) {
                    (true, false) if ally_only => "PHANTOM-inert",
                    (true, false) => "PHANTOM-LIVE",
                    (true, true) => "AGREE",
                    (false, true) => "ATOM-ONLY",
                    (false, false) => continue,
                };
                *counts.entry((slot, verdict)).or_default() += 1;
                let line = format!(
                    "    {slot:<24} {name:<28} target={t:<22} bag={bag:?} atoms={n} reaching={reaching}",
                    name = power.name,
                    t = target_type(power),
                    n = atoms.len()
                );
                match verdict {
                    "PHANTOM-LIVE" => phantoms.push(line),
                    "ATOM-ONLY" => atom_only.push(line),
                    _ => {}
                }
            }
        }

        println!("\n=== {} ===", dataset.as_str());
        for ((slot, verdict), n) in &counts {
            println!("  {verdict:<14} {slot:<24} {n}");
        }
        for (title, mut rows) in [
            (
                "PHANTOM-LIVE (bag credits the caster, no atom reaches him)",
                phantoms,
            ),
            ("ATOM-ONLY (atom reaches, bag silent)", atom_only),
        ] {
            if rows.is_empty() {
                continue;
            }
            rows.sort();
            rows.dedup();
            println!("  -- {title} — {} distinct --", rows.len());
            for line in rows.iter().take(30) {
                println!("{line}");
            }
            if rows.len() > 30 {
                println!("    … {} more", rows.len() - 30);
            }
        }
    }

    reader_lines.sort();
    reader_lines.dedup();
    println!("\n=== READER ANSWERS ({} distinct) ===", reader_lines.len());
    for line in &reader_lines {
        println!("{line}");
    }
}
