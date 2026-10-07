//! Rebirth's Boxing, Kick and Cross Punch each deal +15% damage per other Fighting attack the
//! caster owns: `@StdResult Pool.Fighting.Kick source.ownPowerNum? .15 * … 1 + *` in the damage
//! atom's magnitude expression. Owning a power is a fact the picks state, so the damage follows
//! the picks and no Mechanics toggle can move it.
//!
//! Before this, the converter's ownership toggles ("Kick" on Boxing, "Boxing" on Kick, "Cross
//! Punch" on both) were offered as controls and merged into ownership by MAX: one beside a
//! picked power did nothing, one beside an unpicked power credited it. The reporter saw toggles
//! that sometimes moved nothing and sometimes moved only one of the two partners.

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

fn build(picks: &[&str], toggles: &[(&str, bool)]) -> CharacterState {
    let mut state = CharacterState::empty(DatasetId::Rebirth);
    state.level = 50;
    state.archetype.id = Some("corruptor".into());
    state.pools = vec![PoolSelection {
        id: "fighting".into(),
        name: "fighting".into(),
        powers: picks
            .iter()
            .map(|p| SelectedPower::picked(p.to_string(), "fighting", 1))
            .collect(),
    }];
    for (id, on) in toggles {
        state.combat.global_conditionals.insert(id.to_string(), *on);
    }
    state
}

fn damage(db: &PowerDatabase, state: &CharacterState, power: &str) -> f64 {
    coh_math::recalculate(state, db)
        .power_projection
        .iter()
        .find(|p| p.power_internal_name == power)
        .unwrap_or_else(|| panic!("{power} not projected"))
        .damage
        .base
}

const TOGGLES: [&str; 3] = ["boxing", "kick", "cross_punch"];

#[test]
fn synergy_damage_follows_the_picks_and_no_toggle_moves_it() {
    let db = load(DatasetId::Rebirth);
    let lone = damage(&db, &build(&["Kick"], &[]), "Kick");
    for (picks, partners) in [
        (&["Boxing", "Kick", "Cross_Punch"][..], 2.0),
        (&["Boxing", "Kick"][..], 1.0),
        (&["Kick", "Cross_Punch"][..], 1.0),
    ] {
        let want = lone * (1.0 + 0.15 * partners);
        for on in [false, true] {
            let toggles: Vec<_> = TOGGLES.iter().map(|id| (*id, on)).collect();
            let got = damage(&db, &build(picks, &toggles), "Kick");
            assert!(
                (got - want).abs() < 1e-6,
                "{picks:?}, every toggle {on}: Kick {got} != {want}"
            );
        }
    }
}

#[test]
fn a_pickable_power_offers_no_ownership_toggle() {
    let db = load(DatasetId::Rebirth);
    let offered = coh_data::global_mechanics(&build(&["Boxing", "Kick"], &[]), &db);
    let leaked: Vec<_> = offered
        .iter()
        .filter(|m| TOGGLES.contains(&m.id.as_str()))
        .map(|m| &m.id)
        .collect();
    assert!(
        leaked.is_empty(),
        "ownership toggles still offered: {leaked:?}"
    );
}
