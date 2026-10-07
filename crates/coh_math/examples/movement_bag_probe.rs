//! One power's movement contributions, with its `effects` bag and without — the untruncated
//! form of the row `bag_removal_census` prints, for reading a single movement mover.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example movement_bag_probe -- <dataset> <power name>`

use coh_data::{CharacterState, DatasetId, PowerDatabase, SelectedPower};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn strip(mut db: PowerDatabase) -> PowerDatabase {
    for powerset in &mut db.powersets {
        for power in &mut powerset.powers {
            power.extra.remove("effects");
        }
    }
    db
}

fn probe(db: &PowerDatabase, dataset: DatasetId, name: &str, targets: u32) {
    for powerset in &db.powersets {
        for power in &powerset.powers {
            if power.name != name {
                continue;
            }
            let mut state = CharacterState::empty(dataset);
            let mut pick = SelectedPower::picked(power.ident(), &powerset.id, 1);
            pick.is_active = true;
            pick.targets_hit = Some(targets);
            state.primary.powers = vec![pick];
            state.level = 50;
            state.archetype.id = Some("blaster".to_string());

            let powers = coh_math::gather::gather_active_powers(&state, db).powers;
            let mut g = coh_math::GlobalBonuses::default();
            let (mut movement, mut caps) = (Vec::new(), Vec::new());
            coh_math::apply::apply_active_power_bonuses(
                &powers,
                &mut g,
                "blaster",
                50,
                &coh_math::strength::StrengthBuffs::default(),
                &state.combat,
                &coh_math::incarnates::AlphaEnhancement::default(),
                &mut Vec::new(),
                &mut Vec::new(),
                &mut movement,
                &mut caps,
                &mut Vec::new(),
                &mut Vec::new(),
                db,
            );
            println!("  [{}] movement:", powerset.id);
            for m in &movement {
                println!("     {m:?}");
            }
            println!("  caps:");
            for c in &caps {
                println!("     {c:?}");
            }
            return;
        }
    }
    println!("  (not found)");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dataset = args.next().expect("dataset");
    let name = args.next().expect("power name");
    let dataset = DatasetId::ALL
        .into_iter()
        .find(|d| d.as_str() == dataset)
        .expect("unknown dataset");

    for targets in [1u32, 4] {
        println!("=== {targets} target(s), with bag");
        probe(&load(dataset), dataset, &name, targets);
        println!("=== {targets} target(s), without bag");
        probe(&strip(load(dataset)), dataset, &name, targets);
    }
}
