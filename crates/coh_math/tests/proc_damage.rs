//! Slotted damage procs: the hit at the character's level, and its average per cast.
//!
//! The expected figures are worked by hand from the proc data and the power's own stats, not read
//! back off the engine. Scirocco's Dervish's Lethal proc is 3.5 PPM with a 6.7–71.75 range, which
//! is 0.67 × `Melee_ProcDamage` at levels 1 and 50.

use coh_data::{CharacterState, DatasetId, Enhancement, PowerDatabase, SelectedPower};
use coh_math::procs::{proc_damage_per_cast, slotted_procs, ProcRoll};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn lethal_proc() -> Enhancement {
    serde_json::from_value(serde_json::json!({
        "type": "io-set", "id": "e31", "name": "Chance for Lethal Damage", "icon": "",
        "level": 50, "attuned": false, "boost": 0, "set_id": "sciroccos_dervish",
        "set_name": "Scirocco's Dervish", "piece_num": 6, "aspects": [],
        "is_proc": true, "is_unique": false
    }))
    .expect("slot shape")
}

fn blaster_with(power: &str, level: u8) -> CharacterState {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = level;
    state.archetype.id = Some("blaster".to_string());
    let mut pick = SelectedPower::picked(power, "blaster/fire-blast", 1);
    pick.slots = vec![Some(lethal_proc())];
    state.primary.powers = vec![pick];
    state
}

fn close(ours: f64, theirs: f64, what: &str) {
    assert!(
        (ours - theirs).abs() < 1e-3,
        "{what}: ours {ours}, expected {theirs}"
    );
}

#[test]
fn single_target_click_at_level_50() {
    let db = load(DatasetId::Homecoming);
    let state = blaster_with("Fire_Blast", 50);
    let rows = slotted_procs(&state, &db, "blaster/fire-blast", "Fire_Blast");
    let damage = rows[0].damage.as_ref().expect("a damage proc");
    close(damage.per_hit, 71.75, "hit at 50");
    // Fire Blast: 4s recharge, 1.2s cast, single target → 3.5 × 5.2 / 60.
    let chance = 3.5 * (4.0 + 1.2) / 60.0;
    let ProcRoll::PerActivation {
        chance: scored,
        working,
        ..
    } = rows[0].roll
    else {
        panic!("a click rolls per activation: {:?}", rows[0].roll);
    };
    close(scored, chance, "chance");
    close(working.window, 4.0, "window");
    close(working.area_factor, 1.0, "area factor");
    close(proc_damage_per_cast(&rows), 71.75 * chance, "per cast");
}

#[test]
fn hit_follows_the_table_at_lower_levels() {
    let db = load(DatasetId::Homecoming);
    let state = blaster_with("Fire_Blast", 30);
    let rows = slotted_procs(&state, &db, "blaster/fire-blast", "Fire_Blast");
    // 0.67 × Melee_ProcDamage[30] (72.9088), not a straight line between the ends.
    close(
        rows[0].damage.as_ref().unwrap().per_hit,
        0.67 * 72.9088,
        "hit at 30",
    );
}

#[test]
fn area_attack_pays_the_area_factor() {
    let db = load(DatasetId::Homecoming);
    let state = blaster_with("Fire_Ball", 50);
    let rows = slotted_procs(&state, &db, "blaster/fire-blast", "Fire_Ball");
    // Fire Ball: 16s recharge, 1s cast, 15ft sphere → 0.25 + 0.75 × (1 + 15 × 4500 / 30000).
    let area = 0.25 + 0.75 * (1.0 + 15.0 * (11.0 * 360.0 + 540.0) / 30000.0);
    let chance = 3.5 * (16.0 + 1.0) / (60.0 * area);
    close(proc_damage_per_cast(&rows), 71.75 * chance, "per cast");
}

#[test]
fn a_switched_off_piece_or_category_adds_nothing() {
    let db = load(DatasetId::Homecoming);
    let mut state = blaster_with("Fire_Blast", 50);
    state.disabled_proc_categories.insert("Damage".to_string());
    let rows = slotted_procs(&state, &db, "blaster/fire-blast", "Fire_Blast");
    assert_eq!(proc_damage_per_cast(&rows), 0.0);
}
