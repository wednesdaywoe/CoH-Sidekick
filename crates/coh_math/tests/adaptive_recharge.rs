//! Adaptive recharge, graded against the numbers each power's own help text states
//! ("a base recharge of 8 seconds and each affected foe will increase the recharge by 14.5
//! seconds for a maximum total of 240 seconds"). The help is authored separately from the
//! `Recharge_Power` rows, so agreement is a check from outside the pipeline.

use coh_data::{CharacterState, DatasetId, PowerDatabase, SelectedPower};
use coh_math::adaptive_recharge::adaptive_recharge;
use std::path::PathBuf;

fn homecoming() -> PowerDatabase {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../contract/homecoming/bundle.json.gz");
    PowerDatabase::from_gz_bytes(&std::fs::read(path).expect("read bundle")).expect("load bundle")
}

/// (powerset, power, base, per foe, stated maximum)
const STATED: &[(&str, &str, f64, f64, f64)] = &[
    ("controller/fire-control", "Cinders", 8.0, 14.5, 240.0),
    ("dominator/ice-control", "Glacier", 8.0, 14.5, 240.0),
    (
        "controller/electric-control",
        "Synaptic_Overload",
        6.0,
        6.5,
        110.0,
    ),
    (
        "controller/plant-control",
        "Seeds_of_Confusion",
        6.0,
        6.5,
        110.0,
    ),
    ("tanker/fiery-aura", "Consume", 10.0, 5.0, 60.0),
    ("tanker/psionic-armor", "Consume_Psyche", 5.0, 5.5, 60.0),
    ("scrapper/psionic-armor", "Devour_Psyche", 5.0, 5.5, 60.0),
];

#[test]
fn rule_matches_the_help_text() {
    let db = homecoming();
    for &(set, name, base, per_target, max) in STATED {
        let power = db
            .find_power(set, name)
            .unwrap_or_else(|| panic!("{set} {name}"));
        let rule = adaptive_recharge(power).unwrap_or_else(|| panic!("{name} has no rule"));
        assert_eq!(rule.base_for(None), base, "{name} base");
        assert_eq!(rule.base_for(Some(1)), base + per_target, "{name} one foe");
        assert_eq!(rule.base_for(Some(rule.max_targets)), max, "{name} maximum");
    }
}

/// Radiation Therapy's help text names two rates — "the first target hit will increase its
/// recharge by 9.5 with additional targets adding 2.8 seconds" — from ONE 9.46s row: its over-cap
/// (trigger 1, multiplier 0.3) scales every foe after the first to 2.838s.
#[test]
fn radiation_therapy_counts_through_its_over_cap() {
    let db = homecoming();
    let power = db
        .find_power("tanker/radiation-armor", "Radiation_Therapy")
        .expect("Radiation Therapy");
    let rule = adaptive_recharge(power).expect("rule");
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert!(close(rule.base_for(None), 25.0));
    assert!(close(rule.base_for(Some(1)), 25.0 + 9.46));
    assert!(close(rule.base_for(Some(2)), 25.0 + 9.46 + 9.46 * 0.3));
    assert!(close(rule.base_for(Some(10)), 60.0));
    // Not there before the tenth: the over-cap is what makes the 60s a ten-foe figure.
    assert!(rule.base_for(Some(9)) < 60.0);
}

#[test]
fn ordinary_recharge_has_no_rule() {
    let db = homecoming();
    let fireball = db
        .find_power("blaster/fire-blast", "Fire_Ball")
        .expect("Fire Ball");
    assert!(adaptive_recharge(fireball).is_none());
}

/// The count reaches the projected recharge of a held pick, and recharge enhancement still
/// divides the whole of it.
#[test]
fn projection_follows_the_count() {
    let db = homecoming();
    let recharge_at = |targets: Option<u32>| {
        let mut state = CharacterState::empty(DatasetId::Homecoming);
        state.level = 50;
        state.archetype.id = Some("controller".into());
        state.primary.id = Some("controller/fire-control".into());
        let mut pick = SelectedPower::picked("Cinders", "controller/fire-control", 1);
        pick.targets_hit = targets;
        state.primary.powers = vec![pick];
        let totals = coh_math::recalculate(&state, &db);
        let projection = totals
            .power_projection
            .iter()
            .find(|p| p.power_internal_name == "Cinders")
            .expect("Cinders projected");
        projection.recharge.expect("recharge").base
    };
    assert_eq!(recharge_at(None), 8.0);
    assert_eq!(recharge_at(Some(4)), 8.0 + 4.0 * 14.5);
}
