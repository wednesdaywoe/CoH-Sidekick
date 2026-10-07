//! A buff-pet's ally aura takes the summoner's slotting when the summon is `CopyBoosts`.
//!
//! Prismatic Shield (`Sanctuary_of_Light`) and Force Field Generator both create their pet with
//! `CopyBoosts`, so the game runs the pet's aura with the summoning power's enhancements. Every
//! buff-pet summon on Homecoming carries the flag. The fold used to resolve these auras base,
//! so slotting Defense or Resistance into the summon moved nothing.

use coh_data::{CharacterState, DatasetId, Enhancement, PowerDatabase, SelectedPower};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn totals(
    db: &PowerDatabase,
    set: &str,
    power: &str,
    stats: &[&str],
) -> coh_math::totals::GlobalBonuses {
    let mut state = CharacterState::empty(DatasetId::Homecoming);
    state.level = 50;
    state.archetype.id = Some("mastermind".into());
    state.secondary.id = Some(set.into());
    let mut pick = SelectedPower::picked(power, set, 30);
    pick.slots = stats
        .iter()
        .map(|stat| Some(Enhancement::generic_io(*stat, None, 0)))
        .collect();
    state.secondary.powers = vec![pick];
    state
        .combat
        .power_state
        .insert(coh_math::buff_pets::buff_pet_toggle_key(set, power), true);
    let g = coh_math::recalculate(&state, db).bonuses;
    assert!(g.errors.is_empty(), "{:?}", g.errors);
    g
}

#[test]
fn prismatic_shield_aura_takes_the_summoners_defense_and_resistance() {
    let db = load(DatasetId::Homecoming);
    let set = "mastermind/light-affinity";
    let base = totals(&db, set, "Sanctuary_of_Light", &[]);
    let slotted = totals(
        &db,
        set,
        "Sanctuary_of_Light",
        &[
            "Defense",
            "Defense",
            "Defense",
            "Resistance",
            "Resistance",
            "Resistance",
        ],
    );
    assert!(base.defense_melee > 0.0, "aura folded nothing");
    assert!(base.resistance_smashing > 0.0, "aura folded nothing");
    // Three level-50 IOs sit past the first ED knee, so well over +40% on each face.
    assert!(
        slotted.defense_melee > base.defense_melee * 1.4,
        "defense {} -> {} ignores the slotted Defense",
        base.defense_melee,
        slotted.defense_melee
    );
    assert!(
        slotted.resistance_smashing > base.resistance_smashing * 1.4,
        "resistance {} -> {} ignores the slotted Resistance",
        base.resistance_smashing,
        slotted.resistance_smashing
    );
}

#[test]
fn force_field_generator_aura_takes_the_summoners_defense() {
    let db = load(DatasetId::Homecoming);
    let set = "mastermind/traps";
    let base = totals(&db, set, "Force_Field_Generator", &[]);
    let slotted = totals(
        &db,
        set,
        "Force_Field_Generator",
        &["Defense", "Defense", "Defense"],
    );
    assert!(base.defense_ranged > 0.0, "aura folded nothing");
    assert!(
        slotted.defense_ranged > base.defense_ranged * 1.4,
        "defense {} -> {} ignores the slotted Defense",
        base.defense_ranged,
        slotted.defense_ranged
    );
}
