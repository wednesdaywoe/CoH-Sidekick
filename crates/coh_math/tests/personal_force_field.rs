//! Personal Force Field on the two Homecoming forks: an always-on passive the toggle doesn't carry,
//! and a toggle whose +Def survives In-Combat.
//!
//! The passive is `Temporary_Powers.Temporary_Powers.Personal_Force_Field_Auto`, auto-issued while
//! the build holds any of the four Force Field copies. The toggle restates it only as a `Display`
//! group gated on a literal `0`, which the game never applies; the grant reconcile is what hands
//! the real one over. The toggle's only suppress event is `MissionObjectClick` — clicking a glowie,
//! not combat — so In-Combat must leave its +Def standing beside its +Res.

use coh_data::{granted_powers, CharacterState, DatasetId, PowerDatabase, SelectedPower};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

struct Totals {
    defense: [f64; 3],
    resistance: f64,
    max_endurance: f64,
}

fn totals(
    db: &PowerDatabase,
    dataset: DatasetId,
    archetype: &str,
    set: &str,
    toggled: bool,
    in_combat: bool,
) -> Totals {
    let mut state = CharacterState::empty(dataset);
    state.level = 50;
    state.archetype.id = Some(archetype.into());
    let mut pick = SelectedPower::picked("Personal_Force_Field", set, 1);
    pick.is_active = toggled;
    if set.ends_with("/primary") || archetype == "defender" {
        state.primary.id = Some(set.into());
        state.primary.powers = vec![pick];
    } else {
        state.secondary.id = Some(set.into());
        state.secondary.powers = vec![pick];
    }
    state.combat.in_combat = in_combat;
    let faults = granted_powers::sync_granted_powers(&mut state, db);
    assert!(faults.is_empty(), "grant reconcile faults: {faults:?}");
    let g = coh_math::recalculate(&state, db).bonuses;
    assert!(g.errors.is_empty(), "{:?}", g.errors);
    Totals {
        defense: [g.defense_melee, g.defense_ranged, g.defense_psionic],
        resistance: g.resistance_smashing,
        max_endurance: g.max_endurance,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn the_passive_applies_untoggled_and_the_toggle_holds_its_defense_in_combat() {
    for dataset in [DatasetId::Brainstorm, DatasetId::Homecoming] {
        let db = load(dataset);
        for (archetype, set) in [
            ("defender", "defender/force-field"),
            ("controller", "controller/force-field"),
            ("corruptor", "corruptor/force-field"),
            ("mastermind", "mastermind/force-field"),
        ] {
            let off = totals(&db, dataset, archetype, set, false, false);
            let on = totals(&db, dataset, archetype, set, true, false);
            let on_fighting = totals(&db, dataset, archetype, set, true, true);
            let tag = format!("{dataset:?} {archetype}");

            // Passive: owning the power, toggle off.
            assert!(
                off.defense.iter().all(|d| *d > 0.0),
                "{tag}: passive +Def missing {:?}",
                off.defense
            );
            assert!(off.resistance > 0.0, "{tag}: passive +Res missing");
            assert!(
                close(off.max_endurance, 5.0),
                "{tag}: passive +MaxEnd {}",
                off.max_endurance
            );

            // Toggle adds on top of the passive, on both faces.
            for (o, n) in off.defense.iter().zip(on.defense) {
                assert!(n > *o, "{tag}: toggle adds no +Def ({o} -> {n})");
            }
            assert!(on.resistance > off.resistance, "{tag}: toggle adds no +Res");

            // In combat changes nothing: the only suppress event is MissionObjectClick.
            for (n, f) in on.defense.iter().zip(on_fighting.defense) {
                assert!(close(*n, f), "{tag}: +Def dropped in combat ({n} -> {f})");
            }
            assert!(
                close(on.resistance, on_fighting.resistance),
                "{tag}: +Res moved in combat"
            );
        }
    }
}

/// The passive belongs to the build only while it holds the power: a Force Field build that
/// hasn't picked Personal Force Field gets nothing.
#[test]
fn no_passive_without_the_power() {
    let db = load(DatasetId::Brainstorm);
    let mut state = CharacterState::empty(DatasetId::Brainstorm);
    state.level = 50;
    state.archetype.id = Some("defender".into());
    state.primary.id = Some("defender/force-field".into());
    state.primary.powers = vec![SelectedPower::picked(
        "Deflection_Shield",
        "defender/force-field",
        1,
    )];
    granted_powers::sync_granted_powers(&mut state, &db);
    assert!(state
        .inherents
        .iter()
        .all(|p| p.internal_name != "Personal_Force_Field_Auto"));
    let g = coh_math::recalculate(&state, &db).bonuses;
    assert_eq!(g.defense_melee, 0.0);
    assert_eq!(g.max_endurance, 0.0);
}
