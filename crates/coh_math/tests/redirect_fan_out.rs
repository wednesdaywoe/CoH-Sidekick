//! A per-foe self-buff whose foes are counted by a REDIRECT's sphere, not the power's own.
//!
//! Fulcrum Shift (Kinetic Transfer on Controllers and Defenders) is a single-target shell: it
//! states no `maxTargets`, and its 10 lives in `Redirects.Kinetics.KineticTransfer`, which runs
//! the +damage buff once per foe it hits. Every buff stacks twice, so two casts can stand. The
//! slider used to fall through to a 0–2 "Stacks" count that the engine then read as foes, so
//! the reported shape — 2 × 40% for the casts and up to 10 × 20% per cast (Corruptor) — could
//! not be entered at all.
//!
//! Siphon Power is the base-only sibling: a `Stack`-to-2 +damage whose recipient is the
//! redirect's `Target`, and its stack slider moved nothing on either surface.

use coh_data::{CharacterState, DatasetId, PowerDatabase, SelectedPower};
use coh_math::stacking::{stacking_slider, SliderKind};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The build total's +damage and the power card's +damage row for a level-50 Corruptor holding
/// only this power, active, at `targets_hit`.
fn damage(db: &PowerDatabase, dataset: DatasetId, ident: &str, count: Option<u32>) -> (f64, f64) {
    let powerset = "corruptor/kinetics";
    let mut state = CharacterState::empty(dataset);
    let mut pick = SelectedPower::picked(ident.to_string(), powerset, 1);
    pick.is_active = true;
    pick.targets_hit = count;
    state.secondary.powers = vec![pick];
    state.level = 50;
    state.archetype.id = Some("corruptor".to_string());
    let request = coh_math::projection::PowerRef {
        powerset: powerset.to_string(),
        internal_name: ident.to_string(),
        targets_hit: count,
    };
    let totals = coh_math::recalculate_projecting(&state, db, std::slice::from_ref(&request));
    let projection = serde_json::to_value(&totals.power_projection).expect("projection");
    let card = find_row(&projection, "damageBuff");
    (
        totals.bonuses.damage,
        card.unwrap_or_else(|| panic!("{ident}: no +damage row on the card")),
    )
}

fn find_row(value: &serde_json::Value, key: &str) -> Option<f64> {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("row_key").and_then(|v| v.as_str()) == Some(key) {
                return map["value"]["final"].as_f64();
            }
            map.values().find_map(|v| find_row(v, key))
        }
        serde_json::Value::Array(items) => items.iter().find_map(|v| find_row(v, key)),
        _ => None,
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn fulcrum_shift_counts_foes_across_both_casts() {
    // The two Parse7 forks. The Parse6 forks deliver Fulcrum Shift through spawned helpers, whose
    // table is read under the helper's class — see `parse6_fulcrum_shift_reads_the_helpers_table`.
    for dataset in [DatasetId::Homecoming, DatasetId::Brainstorm] {
        let db = load(dataset);
        let power = db
            .powersets
            .iter()
            .find(|ps| ps.id == "corruptor/kinetics")
            .and_then(|ps| ps.powers.iter().find(|p| p.ident() == "Fulcrum_Shift"))
            .expect("Fulcrum Shift");
        let slider = stacking_slider(power).expect("Fulcrum Shift offers a count");
        assert_eq!(slider.kind, SliderKind::Targets, "{dataset:?}");
        assert_eq!(slider.per_cast, Some(10), "{dataset:?}: 10 foes per cast");
        assert_eq!(slider.max, 20, "{dataset:?}: two casts of 10");
        assert_eq!(
            slider.min, 1,
            "{dataset:?}: a foe-aimed click reached its foe"
        );

        // One foe is the base (+4) and that foe's buff (+2): 6 units. Ten foes is 4 + 2×10, and
        // twenty is two casts of that.
        let (one, one_card) = damage(&db, dataset, "Fulcrum_Shift", Some(1));
        let unit = one / 6.0;
        assert!(unit > 0.0, "{dataset:?}: one foe gives no +damage");
        for (count, units) in [
            (None, 6.0),
            (Some(2), 8.0),
            (Some(10), 24.0),
            (Some(12), 32.0),
            (Some(20), 48.0),
        ] {
            let (total, card) = damage(&db, dataset, "Fulcrum_Shift", count);
            assert!(
                close(total, unit * units),
                "{dataset:?} at {count:?}: total {total}, expected {}",
                unit * units
            );
            assert!(
                close(card, total),
                "{dataset:?} at {count:?}: card {card} vs total {total}"
            );
        }
        assert!(
            close(one_card, one),
            "{dataset:?}: card {one_card} vs total {one}"
        );
    }
}

#[test]
fn siphon_power_stacks_on_both_surfaces() {
    for dataset in [DatasetId::Homecoming, DatasetId::Brainstorm] {
        let db = load(dataset);
        let (one, one_card) = damage(&db, dataset, "Siphon_Power", Some(1));
        let (two, two_card) = damage(&db, dataset, "Siphon_Power", Some(2));
        assert!(one > 0.0, "{dataset:?}");
        assert!(
            close(two, 2.0 * one),
            "{dataset:?}: two casts {two} vs one {one}"
        );
        assert!(
            close(one_card, one) && close(two_card, two),
            "{dataset:?}: card disagrees"
        );
    }
}

/// Rebirth and Thunderspy deliver Fulcrum Shift through spawned helpers: the power's own record
/// is two `Create_Entity` rows (Rebirth) or an executed `Pets.*` power plus one (Thunderspy), and
/// the +damage lives in the helpers' powers. A helper is a second character, so its
/// `Melee_Buff_Dmg` reads `minion_pets` (0.1 at 50), not the Corruptor's 0.085.
///
/// Rebirth: a +4 base helper and a +2 helper per foe, both `minion_pets` — Homecoming's numbers
/// exactly, which is what separate `_Controller` and Defender helpers exist to produce.
///
/// Thunderspy: the +5 base is `KineticTransferPLAYER`, executed by the player rather than spawned,
/// so it reads the Corruptor's table (42.5%); the per-foe helper gives 1.6 × 0.1 (16%).
#[test]
fn parse6_fulcrum_shift_reads_the_helpers_table() {
    // dataset-absent: homecoming — its Fulcrum Shift runs `Redirects.*` powers as the player,
    // with no helper; `fulcrum_shift_counts_foes_across_both_casts` measures it.
    // dataset-absent: brainstorm — the same redirect shape as Homecoming, measured there too.
    for (dataset, base, per_foe) in [
        (DatasetId::Rebirth, 40.0, 20.0),
        (DatasetId::Thunderspy, 42.5, 16.0),
    ] {
        let db = load(dataset);
        let power = db
            .powersets
            .iter()
            .find(|ps| ps.id == "corruptor/kinetics")
            .and_then(|ps| ps.powers.iter().find(|p| p.ident() == "Fulcrum_Shift"))
            .expect("Fulcrum Shift");
        let slider = stacking_slider(power).expect("Fulcrum Shift offers a count");
        assert_eq!(slider.kind, SliderKind::Targets, "{dataset:?}");
        assert_eq!(slider.per_cast, Some(10), "{dataset:?}: 10 foes per cast");
        assert_eq!(slider.max, 20, "{dataset:?}: two casts of 10");

        for (count, casts, foes) in [
            (Some(1), 1.0, 1.0),
            (Some(10), 1.0, 10.0),
            (Some(12), 2.0, 12.0),
            (Some(20), 2.0, 20.0),
        ] {
            let expected = casts * base + foes * per_foe;
            let (total, card) = damage(&db, dataset, "Fulcrum_Shift", count);
            assert!(
                close(total, expected),
                "{dataset:?} at {count:?}: total {total}, expected {expected}"
            );
            assert!(
                close(card, total),
                "{dataset:?} at {count:?}: card {card} vs total {total}"
            );
        }
    }
}

/// Rebirth's Siphon Power is the base-only sibling: one `Create_Entity` row spawning a
/// `minion_pets` helper with +2 `Melee_Buff_Dmg` for its summoner, `Stack` to 2.
#[test]
fn rebirth_siphon_power_reads_the_helpers_table() {
    let db = load(DatasetId::Rebirth);
    let (one, one_card) = damage(&db, DatasetId::Rebirth, "Siphon_Power", Some(1));
    let (two, two_card) = damage(&db, DatasetId::Rebirth, "Siphon_Power", Some(2));
    assert!(close(one, 20.0), "one cast: {one}");
    assert!(close(two, 40.0), "two casts: {two}");
    assert!(
        close(one_card, one) && close(two_card, two),
        "card disagrees"
    );
}
