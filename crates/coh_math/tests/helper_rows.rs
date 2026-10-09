//! A spawned helper's atoms ([`coh_data::AtomicEffect::pet_class`]) resolve their table under the
//! helper's class, and only the damage-buff applier asks for that scope. Every other family
//! reads its tables under the build's archetype, which is right for a helper's row only while the
//! two classes agree on that table.
//!
//! So this pins the corpus to that state: a helper atom in any other family names a `*_Ones`
//! table, or a table that every archetype and the helper's class carry identically at every
//! level. Today that is Inertial Siphon's movement tables. A helper row on a table the classes
//! disagree on fails here instead of shipping the caster's number.
//!
//! `Endurance` is exempt: no totals family reads it, and the power card resolves a helper's row
//! under the helper's class already.

use coh_data::{DatasetId, EffectType, PowerDatabase, TableScope};
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
fn helper_rows_outside_damage_buff_read_tables_every_class_agrees_on() {
    let mut helper_atoms = 0;
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let archetypes: Vec<&str> = db.at_tables.archetype_names().collect();
        let powers = db
            .powersets
            .iter()
            .flat_map(|ps| ps.powers.iter())
            .chain(db.pool_powers.iter().map(|p| &p.power))
            .chain(db.epic_powers.iter().map(|p| &p.power));
        for power in powers {
            for atom in power.atoms.iter() {
                let Some(class) = atom.pet_class.as_deref() else {
                    continue;
                };
                helper_atoms += 1;
                if matches!(
                    atom.effect_type,
                    Some(EffectType::DamageBuff | EffectType::Endurance)
                ) {
                    continue;
                }
                let table = atom.modifier_table.as_deref().unwrap_or("");
                if table.to_ascii_lowercase().ends_with("_ones") {
                    continue;
                }
                for level in 1..=50 {
                    let helper = db.at_tables.value(TableScope::Pet(class), table, level);
                    for archetype in &archetypes {
                        let caster =
                            db.at_tables
                                .value(TableScope::Archetype(archetype), table, level);
                        assert_eq!(
                            helper, caster,
                            "{dataset:?} {}: {:?} on {table} reads {caster:?} for {archetype} \
                             at {level} where the {class} helper reads {helper:?}",
                            power.name, atom.effect_type
                        );
                    }
                }
            }
        }
    }
    assert!(
        helper_atoms > 0,
        "no helper atoms anywhere: the stamp stopped reaching the wire"
    );
}
