//! The two PPM inputs HC authored in 2026 (field 41b) and its chain area factor.
//!
//! Every expected figure is worked by hand from the power's own stats: an Obliteration damage
//! proc at 3.5 PPM, slotted with no recharge, so the window is the base recharge.

use coh_data::{CharacterState, DatasetId, Enhancement, PowerDatabase, SelectedPower};
use coh_math::procs::{slotted_procs, ProcRoll};
use std::path::PathBuf;

fn load() -> PowerDatabase {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contract/homecoming/bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).expect("load homecoming")
}

fn obliteration_proc() -> Enhancement {
    serde_json::from_value(serde_json::json!({
        "type": "io-set", "id": "e1", "name": "Chance for Smashing Damage", "icon": "",
        "level": 50, "attuned": false, "boost": 0, "set_id": "obliteration",
        "set_name": "Obliteration", "piece_num": 6, "aspects": [],
        "is_proc": true, "is_unique": false
    }))
    .expect("slot shape")
}

/// `(chance, area_factor, ppm_mod, fixed_period)` for the proc slotted in `power`.
fn roll(archetype: &str, powerset: &str, power: &str) -> (f64, f64, f64, bool) {
    let db = load();
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.archetype.id = Some(archetype.to_string());
    let mut pick = SelectedPower::picked(power, powerset, 1);
    pick.slots = vec![Some(obliteration_proc())];
    state.primary.powers = vec![pick];
    let rows = slotted_procs(&state, &db, powerset, power);
    match rows[0].roll {
        ProcRoll::PerActivation {
            chance,
            fixed_period,
            working,
            ..
        } => (chance, working.area_factor, working.ppm_mod, fixed_period),
        ref other => panic!("{power}: expected a per-activation roll, got {other:?}"),
    }
}

fn close(ours: f64, theirs: f64, what: &str) {
    assert!(
        (ours - theirs).abs() < 1e-6,
        "{what}: ours {ours}, expected {theirs}"
    );
}

#[test]
fn chain_scores_radius_times_targets() {
    // 10ft jumps, 5 targets: AF 1 + 0.15 × 10 × 5/10 = 1.75. A 10ft sphere would be 2.5.
    let (chance, area, _, _) = roll("brute", "brute/electrical-melee", "Chain_Induction");
    close(area, 0.25 + 0.75 * 1.75, "area factor");
    close(chance, 3.5 * (14.0 + 1.0) / (60.0 * area), "chance");
}

#[test]
fn chain_over_ten_targets_costs_more_than_a_sphere() {
    // 12ft, 16 targets: AF 1 + 0.15 × 12 × 1.6 = 3.88.
    let (_, area, _, _) = roll("blaster", "blaster/storm-blast", "Chain_Lightning");
    close(area, 0.25 + 0.75 * 3.88, "area factor");
}

#[test]
fn override_replaces_the_geometry() {
    // Trip Mine's parent authors AF 2.8 and rolls in its own 30s window.
    let (chance, area, _, _) = roll("defender", "defender/traps", "Trip_Mine");
    close(area, 0.25 + 0.75 * 2.8, "area factor");
    close(chance, 3.5 * (30.0 + 2.77) / (60.0 * area), "chance");
}

#[test]
fn sonic_boom_rolls_on_its_pet_with_the_pets_ppm_mod() {
    // The 15ft Auto pseudo-pet rolls on the piece's 10s period, at PPMMod 2.
    let (chance, area, ppm_mod, fixed) = roll("brute", "brute/sonic-aura", "Sonic_Boom");
    assert!(fixed, "a patch rolls on the piece's period");
    close(ppm_mod, 2.0, "ppm mod");
    close(area, 0.25 + 0.75 * (1.0 + 0.15 * 15.0), "area factor");
    close(chance, 3.5 * 2.0 * 10.0 / (60.0 * area), "chance");
}
