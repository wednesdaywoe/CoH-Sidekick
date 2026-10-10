//! A power aimed at an ally can still land on the caster. Vengeance is aimed at a dead
//! teammate, but its burst covers `["Teammate", "Self"]`, and the apply loop's ally-only skip read
//! the aim alone, so switching it on moved no total (report 2026-10-09). Electrical Affinity's
//! circuits are the opposite case: they list `Self` only for the Static stack they grant the
//! caster, and their chain buffs land on allies.

use coh_data::{
    CharacterState, DatasetId, PoolSelection, PowerDatabase, PowersetSelection, SelectedPower,
};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn picked(name: &str, set: &str, on: bool) -> SelectedPower {
    let mut p = SelectedPower::picked(name.to_string(), set, 49);
    p.is_active = on;
    p
}

fn vengeance(on: bool) -> CharacterState {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.archetype.id = Some("brute".into());
    state.pools = vec![PoolSelection {
        id: "leadership".into(),
        name: "leadership".into(),
        powers: vec![picked("Vengeance", "leadership", on)],
    }];
    state
}

fn circuit(name: &str, on: bool) -> CharacterState {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.archetype.id = Some("defender".into());
    let set = "defender/electrical-affinity";
    state.secondary = PowersetSelection {
        id: Some(set.into()),
        name: String::new(),
        powers: vec![picked(name, set, on)],
    };
    state
}

#[test]
fn vengeance_buffs_the_caster_when_switched_on() {
    let db = load(DatasetId::Homecoming);
    let off = coh_math::recalculate(&vengeance(false), &db).bonuses;
    let on = coh_math::recalculate(&vengeance(true), &db).bonuses;
    // Brute level 50: Melee_Buff_Def 0.085 × 2.5, Melee_Buff_Dmg / _ToHit 0.1 × 3.5.
    for (stat, before, after, want) in [
        ("defense_melee", off.defense_melee, on.defense_melee, 21.25),
        (
            "defense_psionic",
            off.defense_psionic,
            on.defense_psionic,
            21.25,
        ),
        ("damage", off.damage, on.damage, 35.0),
        ("to_hit", off.to_hit, on.to_hit, 35.0),
    ] {
        assert!(
            (after - before - want).abs() < 1e-3,
            "{stat}: off {before}, on {after}, want +{want}"
        );
    }
}

#[test]
fn circuits_do_not_buff_the_caster() {
    let db = load(DatasetId::Homecoming);
    for name in [
        "Energizing_Circuit",
        "Empowering_Circuit",
        "Insulating_Circuit",
    ] {
        let off = coh_math::recalculate(&circuit(name, false), &db).bonuses;
        let on = coh_math::recalculate(&circuit(name, true), &db).bonuses;
        assert_eq!(
            (off.recharge, off.damage, off.absorb),
            (on.recharge, on.damage, on.absorb),
            "{name} moved the caster's totals"
        );
    }
}
