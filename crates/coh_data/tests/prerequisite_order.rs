//! A held power's prerequisite counts only when it was picked at a lower level (GP1).
//!
//! `requires_met` answers "does the build own Boxing?", which is right for a power about to be
//! picked and wrong for one already placed: Boxing at 30 says nothing about whether Tough could
//! have been taken at 20. The guided free-form planner lets a build go out of order and marks it,
//! so the mark needs the question the game asked at the pick level.
//!
//! Graded against Homecoming's own Fighting pool `requires`, not a hand-written expression, so a
//! re-export that rewrites the gate moves this test with it.

use coh_data::{
    requires_met_in_order, CharacterState, DatasetId, PoolSelection, PowerDatabase, SelectedPower,
};
use std::path::PathBuf;

fn homecoming() -> PowerDatabase {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contract/homecoming/bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load homecoming: {e}"))
}

fn fighting_requires(db: &PowerDatabase, ident: &str) -> Vec<Box<str>> {
    let power = db
        .pool_powers
        .iter()
        .find(|p| p.set_id == "fighting" && p.power.ident() == ident)
        .unwrap_or_else(|| panic!("homecoming has no Fighting {ident}"));
    coh_data::granted_powers::requires(&power.power)
        .unwrap_or_else(|| panic!("Fighting {ident} carries no requires"))
}

/// A level-50 build holding only Fighting pool picks, at the given levels.
fn with_fighting(picks: &[(&str, u8)]) -> CharacterState {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.pools.push(PoolSelection {
        id: "fighting".into(),
        name: "Fighting".into(),
        powers: picks
            .iter()
            .map(|&(ident, level)| SelectedPower::picked(ident, "fighting", level))
            .collect(),
    });
    state
}

fn in_order(db: &PowerDatabase, ident: &str, level: u8, state: &CharacterState) -> bool {
    requires_met_in_order(
        &fighting_requires(db, ident),
        state,
        level,
        None,
        &db.set_paths,
    )
    .expect("Fighting gates evaluate")
}

#[test]
fn a_prerequisite_picked_later_does_not_open_tough() {
    let db = homecoming();
    let build = with_fighting(&[("Tough", 20), ("Boxing", 30)]);
    assert!(!in_order(&db, "Tough", 20, &build));
}

#[test]
fn a_prerequisite_picked_earlier_opens_tough() {
    let db = homecoming();
    let build = with_fighting(&[("Kick", 18), ("Tough", 20)]);
    assert!(in_order(&db, "Tough", 20, &build));
}

#[test]
fn tough_with_neither_prerequisite_is_out_of_order() {
    let db = homecoming();
    let build = with_fighting(&[("Tough", 20)]);
    assert!(!in_order(&db, "Tough", 20, &build));
}

/// Weave needs two earlier Fighting picks. One before and one after is not two before.
#[test]
fn weave_counts_only_the_picks_below_it() {
    let db = homecoming();
    let split = with_fighting(&[("Boxing", 10), ("Weave", 22), ("Tough", 30)]);
    assert!(!in_order(&db, "Weave", 22, &split));
    let both_before = with_fighting(&[("Boxing", 10), ("Tough", 20), ("Weave", 22)]);
    assert!(in_order(&db, "Weave", 22, &both_before));
}
