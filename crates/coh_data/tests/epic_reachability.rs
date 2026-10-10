//! An epic pool is reachable only by the archetype its openers name.
//!
//! Each archetype ships its own copy of every mastery. The openers carry the archetype gate
//! (`$archetype @Class_Blaster ==`); the later powers only `Epic ownPowerNum? 0 >`. Counting
//! the openers as siblings regardless of that gate opened every copy to every archetype, and
//! the epic picker listed all of them. Graded on Homecoming's own export.

use coh_data::{reachable_in_set, CharacterState, DatasetId, PowerDatabase};
use std::path::PathBuf;

fn load(fork: &str) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../contract/{fork}/bundle.json.gz"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {fork}: {e}"))
}

fn reachable(db: &PowerDatabase, set: &str, archetype: &str) -> Vec<String> {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.archetype.id = Some(archetype.to_string());
    let powers: Vec<&coh_data::Power> = db
        .epic_powers
        .iter()
        .filter(|p| p.set_id == set)
        .map(|p| &p.power)
        .collect();
    assert!(!powers.is_empty(), "no epic set {set}");
    reachable_in_set(&powers, set, &state, Some(archetype), &db.set_paths)
}

#[test]
fn blaster_dark_mastery_opens_to_blasters_only() {
    let db = load("homecoming");
    let blaster = reachable(&db, "blaster_dark_mastery", "blaster");
    assert_eq!(
        blaster.len(),
        5,
        "a Blaster reaches the whole pool: {blaster:?}"
    );
    for archetype in ["tanker", "controller", "brute", "scrapper"] {
        let other = reachable(&db, "blaster_dark_mastery", archetype);
        assert!(
            other.is_empty(),
            "{archetype} reaches {other:?} in Blaster Dark Mastery"
        );
    }
}

/// Every fork, every archetype it ships: the epics a fresh level-50 build can reach carry no
/// repeated display name — one Dark Mastery, not one per archetype.
#[test]
fn every_fork_offers_each_archetype_one_copy_of_each_epic() {
    for fork in ["homecoming", "rebirth", "thunderspy", "brainstorm"] {
        let db = load(fork);
        let archetypes = db.archetypes().expect("archetypes section reads");
        for archetype in archetypes.all().iter().map(|a| a.id.as_str()) {
            let mut names: Vec<&str> = db
                .pool_catalog
                .epics
                .iter()
                .filter(|pool| !reachable(&db, &pool.id, archetype).is_empty())
                .map(|pool| pool.name.as_str())
                .collect();
            println!("{fork} {archetype}: {} — {names:?}", names.len());
            names.sort_unstable();
            let total = names.len();
            names.dedup();
            assert_eq!(
                names.len(),
                total,
                "{fork}: {archetype} sees a repeated epic name"
            );
        }
    }
}
