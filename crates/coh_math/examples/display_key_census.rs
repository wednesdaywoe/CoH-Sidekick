//! Which display keys the authored `effects` bag is still the only source of.
//!
//! The atom migration's display item has to give ~38 bag keys an atom home before the bag can
//! leave the contract. Which keys those are is not readable off the code: `display_effects`
//! starts from the authored bag and then overwrites part of it from `stats`, the `damage` array
//! and the movement atoms, so a key that appears in the built bag may already have a non-bag
//! source. This asks the corpus instead — build every power's display bag twice, once as
//! shipped and once with the authored `effects` object deleted, and diff key by key.
//!
//! Three outcomes per key, and they are different work:
//!   * **unchanged** — some transform already writes it; the bag is not its source.
//!   * **presence lost** — the key vanishes without the bag. It needs an atom home.
//!   * **value drift** — the key survives but its value changes. The bag was supplying part of
//!     it (a scale, a table, a by-type object).
//!
//! For every key that needs a home, the census also reports whether `window_slots` already
//! projects that key's PRESENCE from atoms, since presence and value are separate halves: the
//! adjusters item bought the presence surface, and the display item is the values. That
//! comparison runs twice — against the BUILT display bag and against the AUTHORED one — because
//! the built bag additionally carries the execution stats, the stacking metadata and the
//! pseudo-pet merge, so a miss there could be any of three things and a miss against the
//! authored bag is the router's own. The third table splits the built-bag misses by whether the
//! power summons, which is what separates the two populations.
//!
//! One reporting artifact to read past: `durations` is a FIELD of `WindowSlots`, not a member of
//! its `keys()`, so it reports as a total miss on both tables while being fully projected.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example display_key_census`

use coh_data::{DatasetId, Power, PowerDatabase};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// Every power that reaches a display surface: the powerset powers, both partitions, and the
/// inherents (which carry no powerset and are the most likely to be bag-only).
fn corpus(db: &PowerDatabase) -> Vec<Power> {
    db.all_powers()
        .cloned()
        .chain(db.inherent_powers.iter().cloned())
        .collect()
}

#[derive(Default)]
struct KeyStat {
    carriers: usize,
    presence_lost: usize,
    value_drift: usize,
    examples: Vec<String>,
}

fn main() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let classes = roster(&db);
        let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();
        let powers = corpus(&db);
        let mut stats: BTreeMap<String, KeyStat> = BTreeMap::new();
        let mut atom_presence: BTreeSet<&'static str> = BTreeSet::new();

        let mut agree: BTreeMap<String, [usize; 3]> = BTreeMap::new();
        let mut raw_agree: BTreeMap<String, [usize; 3]> = BTreeMap::new();
        let mut pet_miss: BTreeMap<String, usize> = BTreeMap::new();

        for power in &powers {
            let with = coh_math::granted::display_effects(power, power, None, dataset, &db);
            let mut stripped = power.clone();
            stripped.extra.remove("effects");
            let without =
                coh_math::granted::display_effects(&stripped, &stripped, None, dataset, &db);
            let atoms = coh_math::window_slots::bag_slots(power, dataset, &class_refs).keys();
            for key in &atoms {
                atom_presence.insert(key);
            }

            // Per-power presence agreement, restricted to the keys the atom router has a
            // vocabulary for. A union over the corpus only says the router CAN write the key
            // somewhere; the display item needs it written on the same powers.
            for key in with.keys() {
                let slot = agree.entry(key.clone()).or_insert([0; 3]);
                slot[0] += 1;
                if atoms.contains(key.as_str()) {
                    slot[1] += 1;
                }
            }
            for key in &atoms {
                if !with.contains_key(*key) {
                    agree.entry(key.to_string()).or_insert([0; 3])[2] += 1;
                }
            }

            // The same comparison against the AUTHORED bag, which is what the atom router
            // mirrors. The built display bag additionally carries the execution stats, the
            // stacking metadata and the pseudo-pet merge, so a miss there can be any of three
            // things; a miss here is the router's own.
            let authored = power
                .extra
                .get("effects")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            for key in authored.keys() {
                let slot = raw_agree.entry(key.clone()).or_insert([0; 3]);
                slot[0] += 1;
                if atoms.contains(key.as_str()) {
                    slot[1] += 1;
                }
            }
            for key in &atoms {
                if !authored.contains_key(*key) {
                    raw_agree.entry(key.to_string()).or_insert([0; 3])[2] += 1;
                }
            }

            // Split the built-bag miss by whether the pseudo-pet merge contributed, to say
            // whether a miss is the router's or a second character's rows arriving.
            if !coh_math::granted::pseudo_pet_effects(power, &db).is_empty() {
                for key in with.keys() {
                    if !atoms.contains(key.as_str()) {
                        *pet_miss.entry(key.clone()).or_insert(0) += 1;
                    }
                }
            }

            for (key, value) in &with {
                let entry = stats.entry(key.clone()).or_default();
                entry.carriers += 1;
                match without.get(key) {
                    None => {
                        entry.presence_lost += 1;
                        if entry.examples.len() < 3 {
                            entry.examples.push(power.name.clone());
                        }
                    }
                    Some(other) if other != value => {
                        entry.value_drift += 1;
                        if entry.examples.len() < 3 {
                            entry.examples.push(format!("{}~", power.name));
                        }
                    }
                    Some(_) => {}
                }
            }
        }

        println!("\n=== {dataset:?} — {} powers ===", powers.len());
        println!(
            "{:<26} {:>8} {:>9} {:>7}  {:<10} examples",
            "key", "carriers", "no-bag→∅", "drift", "atom-pres"
        );
        for (key, s) in &stats {
            if s.presence_lost == 0 && s.value_drift == 0 {
                continue;
            }
            println!(
                "{:<26} {:>8} {:>9} {:>7}  {:<10} {}",
                key,
                s.carriers,
                s.presence_lost,
                s.value_drift,
                if atom_presence.contains(key.as_str()) {
                    "yes"
                } else {
                    "-"
                },
                s.examples.join(", ")
            );
        }
        println!("\n-- per-power presence agreement (display key vs window_slots) --");
        println!(
            "{:<26} {:>8} {:>10} {:>10} {:>9}",
            "key", "carriers", "atom-has", "atom-miss", "atom-only"
        );
        for (key, [carriers, has, only]) in &agree {
            if *carriers == *has && *only == 0 {
                continue;
            }
            println!(
                "{:<26} {:>8} {:>10} {:>10} {:>9}",
                key,
                carriers,
                has,
                carriers - has,
                only
            );
        }

        println!("\n-- AUTHORED bag vs window_slots (the router's own miss) --");
        println!(
            "{:<26} {:>8} {:>10} {:>10} {:>9}",
            "key", "carriers", "atom-has", "atom-miss", "atom-only"
        );
        for (key, [carriers, has, only]) in &raw_agree {
            if *carriers == *has && *only == 0 {
                continue;
            }
            println!(
                "{:<26} {:>8} {:>10} {:>10} {:>9}",
                key,
                carriers,
                has,
                carriers - has,
                only
            );
        }

        println!("\n-- of the built-bag misses, how many are on a pseudo-pet summoner --");
        for (key, n) in &pet_miss {
            let total = agree.get(key).map(|a| a[0] - a[1]).unwrap_or(0);
            println!("{key:<26} {n:>6} of {total}");
        }

        let needs: Vec<&String> = stats
            .iter()
            .filter(|(_, s)| s.presence_lost > 0 || s.value_drift > 0)
            .map(|(k, _)| k)
            .collect();
        let clean: Vec<&String> = stats
            .iter()
            .filter(|(_, s)| s.presence_lost == 0 && s.value_drift == 0)
            .map(|(k, _)| k)
            .collect();
        println!("bag-sourced keys: {} — {:?}", needs.len(), needs);
        println!("already non-bag: {} — {:?}", clean.len(), clean);
    }
}

/// Every archetype this dataset states a class token for — the roster `_addUnanimousForkedSlots`
/// projects over, read off the dataset rather than hand-listed and asserted non-trivial, because
/// a truncated roster would make the fork restoration vacuous instead of loud (AT-FORK-2).
fn roster(db: &PowerDatabase) -> Vec<String> {
    let archetypes = db.archetypes().expect("archetype catalog");
    let ids: Vec<String> = archetypes
        .all()
        .iter()
        .filter_map(|at| db.class_name_of(&at.id).map(str::to_owned))
        .collect();
    assert!(
        ids.len() >= 10,
        "archetype roster is {} — too small to have resolved the forks",
        ids.len()
    );
    ids
}
