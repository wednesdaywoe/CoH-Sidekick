//! A `StrengthsDisallowed RechargeTime` power keeps its authored recharge on the power card.
//!
//! Strength of Will is the reported case: it disallows recharge strength outright, so Hasten
//! and set bonuses must not shorten it. Perma and the attack chain already honoured the flag;
//! the card's recharge row added the build's global recharge anyway.

use coh_data::{CharacterState, DatasetId, PoolSelection, PowerDatabase, SelectedPower};
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
fn strength_of_will_ignores_global_recharge() {
    for dataset in [DatasetId::Rebirth, DatasetId::Homecoming] {
        let db = load(dataset);
        let mut state = CharacterState::empty(dataset);
        state.level = 50;
        state.archetype.id = Some("stalker".into());
        state.secondary.powers = vec![SelectedPower::picked(
            "Strength_of_Will",
            "stalker/willpower",
            1,
        )];
        let mut hasten = SelectedPower::picked("Hasten", "speed", 1);
        hasten.is_active = true;
        state.pools = vec![PoolSelection {
            id: "speed".into(),
            name: "speed".into(),
            powers: vec![hasten],
        }];

        let totals = coh_math::recalculate(&state, &db);
        assert!(
            totals.bonuses.recharge > 0.0,
            "{dataset:?}: Hasten gave no global recharge, so the test proves nothing"
        );
        let projection = totals
            .power_projection
            .iter()
            .find(|p| p.power_internal_name == "Strength_of_Will")
            .unwrap_or_else(|| panic!("{dataset:?}: Strength of Will not projected"));
        let row = projection
            .recharge
            .unwrap_or_else(|| panic!("{dataset:?}: Strength of Will has no recharge row"));
        // The card's recharge tile is a granted row, resolved by a separate path.
        let tile = projection
            .granted_magnitudes
            .iter()
            .find(|m| m.effect_key == "recharge")
            .unwrap_or_else(|| panic!("{dataset:?}: Strength of Will has no recharge tile"))
            .value;
        for (surface, tier) in [("row", row), ("tile", tile)] {
            assert_eq!(tier.base, 300.0, "{dataset:?} {surface}");
            assert_eq!(
                tier.r#final, tier.base,
                "{dataset:?} {surface}: global recharge leaked in"
            );
        }
    }
}
