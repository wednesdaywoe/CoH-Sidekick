//! `Power::rolls_to_hit_foe` — the Hit Chance Alert's gate — read off the real contracts.
//!
//! Accuracy says nothing here: self toggles carry one too. The answer is the export's
//! EntsAffected / EntsAutoHit pair, on the power or on a child it executes.

use coh_data::{DatasetId, Power, PowerDatabase};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// A powerset power by set id and ident; a pool power by its `fullName` (only the pool and
/// epic partitions carry one).
fn lookup<'a>(db: &'a PowerDatabase, set_id: &str, ident: &str) -> &'a Power {
    db.find_power(set_id, ident)
        .or_else(|| {
            db.all_powers()
                .find(|p| p.extra.get("fullName").and_then(|v| v.as_str()) == Some(ident))
        })
        .unwrap_or_else(|| panic!("{set_id} {ident} not in the contract"))
}

#[test]
fn homecoming_powers_roll_as_the_game_rolls_them() {
    let db = load(DatasetId::Homecoming);
    for (set_id, ident, rolls) in [
        // A plain attack.
        ("blaster/energy-blast", "Power_Blast", true),
        // A PBAoE damage aura: cast on Self, rolls against the foes around.
        ("brute/dark-armor", "Death_Shroud", true),
        // The same, with an authored EMPTY auto-hit list rather than `["None"]`.
        ("blaster/fire-manipulation", "Blazing_Aura", true),
        // Self buffs: an accuracy on the wire, never a roll.
        ("brute/dark-armor", "Dark_Embrace", false),
        ("", "Pool.Fighting.Tough", false),
        // The power teleports the caster; its executed child is the attack.
        ("", "Pool.Leaping.Spring_Attack", true),
        // The power auto-hits; its executed children roll.
        ("brute/stone-melee", "Fault", true),
    ] {
        assert_eq!(
            lookup(&db, set_id, ident).rolls_to_hit_foe(),
            rolls,
            "{set_id} {ident}"
        );
    }
}

/// Every converter carries EntsAutoHit wherever it carries EntsAffected — a partition that
/// dropped it would silently never raise the alert.
#[test]
fn auto_hit_travels_with_affected_on_every_fork() {
    for dataset in [
        DatasetId::Homecoming,
        DatasetId::Rebirth,
        DatasetId::Thunderspy,
        DatasetId::Brainstorm,
    ] {
        let db = load(dataset);
        let missing: Vec<&str> = db
            .all_powers()
            .filter(|p| p.targets_affected().is_some() && p.targets_auto_hit().is_none())
            .map(|p| p.name.as_str())
            .collect();
        assert!(
            missing.is_empty(),
            "{dataset:?}: {} powers carry targetsAffected without targetsAutoHit, e.g. {:?}",
            missing.len(),
            &missing[..missing.len().min(5)]
        );
    }
}
