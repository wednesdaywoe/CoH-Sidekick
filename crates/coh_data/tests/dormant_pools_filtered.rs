//! Dormant pools are dropped from the catalog but kept in the partition vectors.
//!
//! Gadgetry and Utility Belt are pre-shutdown Homecoming pools: their powers sit behind a
//! dev-only `accesslevel > 0` gate the client can't use, so the game hides them. The raw
//! `power-pools` section still carries both, stamped `dormant: true`, and the loader has to
//! honor that flag — a picker that listed them on Homecoming would offer pools the build can
//! never take.
//!
//! The catalog is the picker's view, so a dormant pool is filtered out of it. The partition
//! vectors are the calc's view and keep every power (the dormant powers still contribute to
//! nothing a build selects, and dropping them would break `verify_counts`, which reconciles
//! against the manifest). This gate pins both halves of that split.

use coh_data::{DatasetId, PowerDatabase};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The dormant pool ids this dataset's bins carry, keyed by what the game has released.
///
/// Ground truth (server devs, 2026-07-08): Homecoming has NEITHER, Rebirth has Gadgetry only,
/// Thunderspy has BOTH. Brainstorm is Homecoming's open beta and carries the same release
/// state, so it mirrors Homecoming.
fn dormant_ids(dataset: DatasetId) -> Vec<&'static str> {
    match dataset {
        DatasetId::Homecoming | DatasetId::Brainstorm => {
            vec!["gadgetry", "utility_belt"]
        }
        DatasetId::Rebirth => vec!["utility_belt"],
        DatasetId::Thunderspy => vec![],
    }
}

fn live_ids(dataset: DatasetId) -> Vec<&'static str> {
    match dataset {
        DatasetId::Homecoming | DatasetId::Brainstorm => {
            vec![
                "experimentation",
                "fighting",
                "fitness",
                "flight",
                "force_of_will",
                "invisibility",
                "leadership",
                "leaping",
                "presence",
                "medicine",
                "sorcery",
                "speed",
                "teleportation",
            ]
        }
        DatasetId::Rebirth => {
            vec![
                "experimentation",
                "fighting",
                "fitness",
                "flight",
                "force_of_will",
                "gadgetry",
                "invisibility",
                "leadership",
                "leaping",
                "presence",
                "medicine",
                "sorcery",
                "speed",
                "teleportation",
            ]
        }
        DatasetId::Thunderspy => {
            vec![
                "experimentation",
                "fighting",
                "fitness",
                "flight",
                "force_of_will",
                "gadgetry",
                "invisibility",
                "leadership",
                "leaping",
                "presence",
                "medicine",
                "sorcery",
                "speed",
                "teleportation",
                "utility_belt",
            ]
        }
    }
}

/// A dormant pool is absent from the catalog — the picker never lists it.
#[test]
fn dormant_pools_are_absent_from_the_catalog() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        for id in dormant_ids(dataset) {
            assert!(
                db.pool_catalog.find(id).is_none(),
                "{dataset:?}: {id} is dormant but still in the catalog"
            );
        }
    }
}

/// A released pool is present in the catalog — the picker still lists it.
#[test]
fn live_pools_are_present_in_the_catalog() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        for id in live_ids(dataset) {
            assert!(
                db.pool_catalog.find(id).is_some(),
                "{dataset:?}: {id} is released but missing from the catalog"
            );
        }
    }
}

/// The dormant powers still live in the partition vectors — dropping them would break the
/// manifest reconciliation, and nothing a build selects reaches them anyway.
#[test]
fn dormant_powers_survive_in_the_partition_vectors() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        for id in dormant_ids(dataset) {
            let count = db
                .pool_powers
                .iter()
                .filter(|entry| entry.set_id == id)
                .count();
            assert!(
                count > 0,
                "{dataset:?}: {id} is dormant but its powers are gone from the partition"
            );
        }
    }
}
