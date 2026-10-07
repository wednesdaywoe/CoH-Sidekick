//! A power that hands out mutually exclusive forms — Dual Pistols' Swap Ammo, Staff Fighting's
//! Staff Mastery, Bio Armor's Adaptation — is a stance selector, and choosing a form has to move
//! what the form moves, on every fork that ships it.
//!
//! The selector was derived and its control existed, but choosing a Staff form moved nothing (its
//! Perfection conditionals fold to no name the form carries), Thunderspy's selectors moved nothing
//! the power text could read (it issues the parents from the inherent set, away from the attacks
//! whose conditionals name the forms), and Homecoming's Sky Splitter self-buff stayed out of the
//! totals behind an unanswered `enttype target>` clause. Graded through the real `stance_groups`
//! → `set_stance` → `recalculate` path, so a form that only changes the label fails here.

use coh_data::{CharacterState, DatasetId, PowerDatabase, SelectedPower};
use std::path::PathBuf;

const DATASETS: [DatasetId; 4] = [
    DatasetId::Homecoming,
    DatasetId::Rebirth,
    DatasetId::Thunderspy,
    DatasetId::Brainstorm,
];

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// A level-50 build holding `picks` (all switched on) from one set, with the grants reconciled the
/// way the app does after a pick.
fn build(
    dataset: DatasetId,
    db: &PowerDatabase,
    archetype: &str,
    set: &str,
    secondary: bool,
    picks: &[&str],
) -> CharacterState {
    let mut state = CharacterState::empty(dataset);
    state.level = 50;
    state.archetype.id = Some(archetype.into());
    let bucket = if secondary {
        &mut state.secondary
    } else {
        &mut state.primary
    };
    bucket.id = Some(set.into());
    bucket.powers = picks
        .iter()
        .filter(|pick| db.resolve_power(set, pick).is_some())
        .map(|pick| {
            let mut power = SelectedPower::picked(*pick, set, 1);
            power.is_active = true;
            power
        })
        .collect();
    coh_data::sync_granted_powers(&mut state, db);
    state
}

/// The one stance group in `state`, the form named `form` chosen.
fn with_form(state: &CharacterState, db: &PowerDatabase, form: &str) -> CharacterState {
    let groups = coh_data::stance_groups(state, db);
    let group = groups
        .iter()
        .find(|group| group.options.iter().any(|o| o.internal_name == form))
        .unwrap_or_else(|| panic!("no stance group offers {form}"));
    assert!(
        group
            .options
            .iter()
            .all(|option| !option.conditional_ids.is_empty()),
        "{}: a form answers to no conditional, so the power text cannot follow it: {:?}",
        group.parent,
        group.options
    );
    let mut chosen = state.clone();
    coh_data::set_stance(&mut chosen, group, Some(form));
    chosen
}

#[test]
fn every_ammo_changes_the_pistols_secondary_damage() {
    for dataset in DATASETS {
        let db = load(dataset);
        let state = build(
            dataset,
            &db,
            "blaster",
            "blaster/dual-pistols",
            false,
            &["Pistols", "Swap_Ammo"],
        );
        let types = |state: &CharacterState| -> Vec<String> {
            let totals = coh_math::recalculate(state, &db);
            let pistols = totals
                .power_projection
                .iter()
                .find(|p| p.power_internal_name == "Pistols")
                .expect("Pistols projected");
            let mut types: Vec<String> = pistols
                .damage
                .components
                .iter()
                .filter(|c| !matches!(c.application, coh_math::damage::DamageApplication::Dormant))
                .map(|c| c.damage_type.clone())
                .collect();
            types.sort();
            types.dedup();
            types
        };
        for (form, damage_type) in [
            ("Incendiary_Ammunition", "Fire"),
            ("Cryo_Ammunition", "Cold"),
            ("Chemical_Ammunition", "Toxic"),
        ] {
            let loaded = types(&with_form(&state, &db, form));
            assert!(
                loaded.iter().any(|t| t == damage_type),
                "{dataset:?}: {form} loaded, Pistols deals {loaded:?}"
            );
        }
    }
}

#[test]
fn incendiary_ammo_burns() {
    for dataset in DATASETS {
        let db = load(dataset);
        let state = build(
            dataset,
            &db,
            "blaster",
            "blaster/dual-pistols",
            false,
            &["Pistols", "Swap_Ammo"],
        );
        let burn = |state: &CharacterState| -> f64 {
            coh_math::recalculate(state, &db)
                .power_projection
                .iter()
                .find(|p| p.power_internal_name == "Pistols")
                .expect("Pistols projected")
                .damage
                .components
                .iter()
                .filter(|c| c.damage_type == "Fire" && c.over_time.is_some())
                .filter(|c| !matches!(c.application, coh_math::damage::DamageApplication::Dormant))
                .map(|c| c.total.base)
                .sum()
        };
        assert_eq!(burn(&state), 0.0, "{dataset:?}: standard rounds burn");
        let lit = burn(&with_form(&state, &db, "Incendiary_Ammunition"));
        assert!(lit > 0.0, "{dataset:?}: Incendiary Ammo loaded, no burn");
    }
}

#[test]
fn staff_forms_move_the_totals() {
    for dataset in DATASETS {
        let db = load(dataset);
        let state = build(
            dataset,
            &db,
            "scrapper",
            "scrapper/staff-fighting",
            false,
            &["Sky_Splitter", "Staff_Mastery"],
        );
        let base = coh_math::recalculate(&state, &db).stats;
        let body = coh_math::recalculate(&with_form(&state, &db, "Form_of_the_Body"), &db).stats;
        let mind = coh_math::recalculate(&with_form(&state, &db, "Form_of_the_Mind"), &db).stats;
        let soul = coh_math::recalculate(&with_form(&state, &db, "Form_of_the_Soul"), &db).stats;
        assert!(
            body.res_sl > base.res_sl,
            "{dataset:?}: Body moved no resistance"
        );
        assert!(
            mind.to_hit > base.to_hit,
            "{dataset:?}: Mind moved no to-hit"
        );
        assert!(
            soul.regeneration > base.regeneration,
            "{dataset:?}: Soul moved no regeneration"
        );
        assert_eq!(body.to_hit, base.to_hit, "{dataset:?}: Body moved to-hit");
    }
}

#[test]
fn bio_adaptations_move_the_totals() {
    for dataset in DATASETS {
        let db = load(dataset);
        let state = build(
            dataset,
            &db,
            "scrapper",
            "scrapper/bio-armor",
            true,
            &["Hardened_Carapace", "Evolution", "Inexhaustible"],
        );
        let base = coh_math::recalculate(&state, &db).stats;
        let defensive =
            coh_math::recalculate(&with_form(&state, &db, "Defensive_Adaptation"), &db).stats;
        let offensive =
            coh_math::recalculate(&with_form(&state, &db, "Offensive_Adaptation"), &db).stats;
        let efficient =
            coh_math::recalculate(&with_form(&state, &db, "Efficient_Adaptation"), &db).stats;
        assert!(defensive.res_sl > base.res_sl, "{dataset:?}: Defensive");
        assert!(offensive.damage > base.damage, "{dataset:?}: Offensive");
        assert!(
            efficient.regeneration > base.regeneration,
            "{dataset:?}: Efficient"
        );
    }
}
