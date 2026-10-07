//! Endurance Modification enhances every Endurance and Recovery mod a power carries: the boost
//! is one `Strength` template over both attribs (`boosts/crafted_recovery_50`), with no sign or
//! recipient filter. Rebirth Guardian Transference drains the foe (`enduranceDrain`) and its
//! pseudo-pet hands the endurance to allies (`enduranceGain`, `copyCreatorMods`). Both rows
//! rendered flat with the enhancement slotted, because neither registry entry named an aspect.

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

#[test]
fn transference_drain_and_gain_follow_endurance_modification() {
    let db = load(DatasetId::Rebirth);
    let mut state = CharacterState::empty(DatasetId::Rebirth);
    state.level = 50;
    state.archetype.id = Some("guardian".into());
    state.secondary.id = Some("guardian/energy-composition".into());
    let mut power = SelectedPower::picked("Transference", "guardian/energy-composition", 35);
    power.slots = vec![Some(Enhancement::generic_io(
        "EnduranceModification",
        None,
        0,
    ))];
    state.secondary.powers = vec![power];

    let result = coh_math::recalculate(&state, &db);
    let projection = result
        .power_projection
        .iter()
        .find(|p| p.power_internal_name == "Transference")
        .expect("Transference projected");
    for key in ["enduranceDrain", "enduranceGain"] {
        let row = projection
            .granted_magnitudes
            .iter()
            .find(|g| g.effect_key == key)
            .unwrap_or_else(|| panic!("no {key} row"));
        assert!(
            row.value.enhanced.abs() > row.value.base.abs() * 1.2,
            "{key}: {:?} ignores the slotted Endurance Modification",
            row.value
        );
    }
}
