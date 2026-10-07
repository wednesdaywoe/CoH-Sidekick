//! What the calc still gets from the `effects` bag, measured against the corpus.
//!
//! Every family the ATOM1–15 migrations moved reads `atom_applier(power) ?? bag.slot()`. Which
//! side answered is not visible from the code — the bag can be serving a whole fork, one named
//! residual, or nothing at all. This asks the corpus instead: hold each power alone, run the real
//! gather + apply passes twice — once with the bundle as shipped, once with every power's
//! `effects` object deleted — and diff the totals accumulator field by field.
//!
//! A field that moves is a value the bag is still the only source of; the census names the power,
//! the field, and the bag keys that power carries, which is the work list for removing the slot
//! from the contract. A field that never moves anywhere means the `?? bag` arm is dead corpus-wide.
//!
//! Every probe runs once per archetype in the dataset's own roster, because the atom stream can
//! fork on the caster's class and the bag cannot (AT-FORK-1). Where a power's atoms are all
//! forked, the archetypes no arm names read atom-less and fall to the bag — so a sweep pinned to
//! one archetype reports that fork's own arm answering and calls the slot clean. A field only
//! some archetypes move is reported with those archetypes named.
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example bag_removal_census`

use coh_data::{CharacterState, DatasetId, PoolSelection, Power, PowerDatabase, SelectedPower};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The same database with every power's `effects` object removed — the contract as it would ship
/// once the bag is gone. Pets and inherents are stripped too: a summoning power's totals reach
/// through the pet's own record, so leaving pet bags in place would hide the pet-side reads.
fn without_bags(dataset: DatasetId) -> PowerDatabase {
    let mut db = load(dataset);
    for powerset in &mut db.powersets {
        for power in &mut powerset.powers {
            power.extra.remove("effects");
        }
    }
    for partition in db.pool_powers.iter_mut().chain(db.epic_powers.iter_mut()) {
        partition.power.extra.remove("effects");
    }
    for power in &mut db.inherent_powers {
        power.extra.remove("effects");
    }
    if let Some(pets) = db.sections.get_mut("pet-entities") {
        strip_effects(pets);
    }
    db
}

/// Delete every object-valued `effects` key in a raw section — the pet-entity powers, which the
/// summon merge reads straight off `sections` rather than through a typed `Power`. Array-valued
/// `effects` (an IO set's bonus list) is a different field and stays.
fn strip_effects(node: &mut Value) {
    match node {
        Value::Object(map) => {
            if map.get("effects").is_some_and(Value::is_object) {
                map.remove("effects");
            }
            for value in map.values_mut() {
                strip_effects(value);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(strip_effects),
        _ => {}
    }
}

/// Every power a build can hold, with the container it is held in.
fn pickable(db: &PowerDatabase) -> Vec<(String, String, String, bool)> {
    let mut out = Vec::new();
    for powerset in &db.powersets {
        for power in &powerset.powers {
            out.push((
                power.ident().to_string(),
                power.name.clone(),
                powerset.id.clone(),
                false,
            ));
        }
    }
    for pool in db
        .pool_catalog
        .pools
        .iter()
        .chain(db.pool_catalog.epics.iter())
    {
        for ident in &pool.power_idents {
            if let Some(power) = db.all_powers().find(|p| p.ident() == ident) {
                out.push((ident.clone(), power.name.clone(), pool.id.clone(), true));
            }
        }
    }
    // The basic and prestige inherents (Sprint, Ninja Run, Rest) are in no powerset or pool, and
    // they are the powers most likely to need the bag: several carry no atoms at all. A sweep
    // over the pickable corpus alone would report them as clean.
    for power in &db.inherent_powers {
        out.push((
            power.ident().to_string(),
            power.name.clone(),
            "inherent-probe".to_string(),
            false,
        ));
    }
    out
}

/// The totals a level-50 `archetype` holding exactly this one power, active, produces — the
/// real gather + apply passes, as `route_sweep::totals_for_power_alone` runs them.
///
/// The archetype is an INPUT rather than a pinned Blaster because the atom stream can fork on
/// it and the bag cannot. `Power::for_caster_class` drops every atom whose `casterArchetypes`
/// excludes the build's class, so a power whose atoms are all forked reads atom-less for the
/// archetypes no arm names — and that is precisely when a `?? bag` arm wakes up. A one-archetype
/// sweep cannot see it: it reports the fork's own arm answering and calls the slot clean
/// (AT-FORK-1, AT-FORK-2).
fn totals(
    db: &PowerDatabase,
    dataset: DatasetId,
    ident: &str,
    container: &str,
    is_pool: bool,
    targets_hit: u32,
    archetype: &str,
) -> Value {
    let mut state = CharacterState::empty(dataset);
    let mut pick = SelectedPower::picked(ident.to_string(), container, 1);
    pick.is_active = true;
    // The stacking metadata (`stacksLinear`, `maxStacks`, `stackCaps`) is bag-only and reachable
    // only through the targets-hit input, which doubles as the stack count. At one target
    // `adjust_for_stacking` is the identity, so a one-target sweep is blind to that whole slot.
    pick.targets_hit = Some(targets_hit);
    if is_pool {
        state.pools = vec![PoolSelection {
            id: container.to_string(),
            name: container.to_string(),
            powers: vec![pick],
        }];
    } else {
        state.primary.powers = vec![pick];
    }
    state.level = 50;
    state.archetype.id = Some(archetype.to_string());

    let powers = coh_math::gather::gather_active_powers(&state, db).powers;
    let mut g = coh_math::GlobalBonuses::default();
    // The five deferred collectors, kept rather than discarded. A positive movement buff, a
    // stealth radius and an absorb fraction do not land on the accumulator here — they are
    // resolved later, across all sources at once — so a sweep that dropped them would call
    // Sprint's whole effect clean.
    let mut stealth = Vec::new();
    let mut absorb_fractions = Vec::new();
    let mut movement = Vec::new();
    let mut movement_caps = Vec::new();
    let mut res_self_debuffs = Vec::new();
    coh_math::apply::apply_active_power_bonuses(
        &powers,
        &mut g,
        archetype,
        50,
        &coh_math::strength::StrengthBuffs::default(),
        &state.combat,
        &coh_math::incarnates::AlphaEnhancement::default(),
        &mut stealth,
        &mut absorb_fractions,
        &mut movement,
        &mut movement_caps,
        &mut res_self_debuffs,
        &mut Vec::new(),
        db,
    );
    let mut out = serde_json::Map::new();
    out.insert(
        "totals".to_string(),
        serde_json::to_value(&g).expect("totals serialize"),
    );
    for (name, debug) in [
        ("stealth_contribs", format!("{stealth:?}")),
        ("absorb_fraction_contribs", format!("{absorb_fractions:?}")),
        ("movement_contribs", format!("{movement:?}")),
        ("movement_cap_contribs", format!("{movement_caps:?}")),
        ("res_self_debuffs", format!("{res_self_debuffs:?}")),
    ] {
        out.insert(name.to_string(), Value::String(debug));
    }
    Value::Object(out)
}

/// The leaf paths where two totals objects disagree, as `field.sub` → `(with, without)`.
fn diff(with: &Value, without: &Value, prefix: &str, out: &mut Vec<(String, String, String)>) {
    match (with, without) {
        (Value::Object(a), Value::Object(b)) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                diff(
                    a.get(key).unwrap_or(&Value::Null),
                    b.get(key).unwrap_or(&Value::Null),
                    &path,
                    out,
                );
            }
        }
        _ if with != without => {
            out.push((prefix.to_string(), with.to_string(), without.to_string()))
        }
        _ => {}
    }
}

/// Every archetype this dataset states a class token for — the roster the atom fork resolves
/// against.
///
/// Read off the dataset rather than hand-listed, and filtered through `class_name_of`, because
/// that mapping is what `for_caster_class` consumes: an archetype with no class token resolves
/// no fork, so probing it would only re-run the unforked read under another name. The roster is
/// asserted non-trivial for the same reason AT-FORK-2's guards assert it — an empty or truncated
/// roster would make the widened sweep vacuously clean, which is the failure this probe exists
/// to rule out.
fn roster(db: &PowerDatabase) -> Vec<String> {
    let archetypes = db.archetypes().expect("archetype catalog");
    let ids: Vec<String> = archetypes
        .all()
        .iter()
        .filter(|at| db.class_name_of(&at.id).is_some())
        .map(|at| at.id.clone())
        .collect();
    assert!(
        ids.len() >= 10,
        "archetype roster is {} — too small to have resolved the forks; the sweep would be \
         vacuously clean",
        ids.len()
    );
    ids
}

/// The bag keys this power carries, for attributing a moved field to a slot.
fn bag_keys(power: &Power) -> Vec<String> {
    power
        .extra
        .get("effects")
        .and_then(Value::as_object)
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default()
}

fn main() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let stripped = without_bags(dataset);
        // field → (powers moved, one example)
        let mut by_field: BTreeMap<String, (usize, String)> = BTreeMap::new();
        let mut movers: Vec<(String, Vec<String>, Vec<String>)> = Vec::new();

        let classes = roster(&db);

        for (ident, name, container, is_pool) in pickable(&db) {
            // field → (the archetypes it moved under, one example of the move). Probed once per
            // archetype, and each archetype's probe runs at one target and then four: the second
            // is what reaches the stacking metadata and the per-target increments, so a field
            // that moves only there is named as such, since it is a different slot with a
            // different fix.
            let mut moved: BTreeMap<String, (Vec<String>, String, String)> = BTreeMap::new();
            for archetype in &classes {
                let mut fields: Vec<(String, String, String)> = Vec::new();
                let with = totals(&db, dataset, &ident, &container, is_pool, 1, archetype);
                let without = totals(
                    &stripped, dataset, &ident, &container, is_pool, 1, archetype,
                );
                diff(&with, &without, "", &mut fields);
                let single: Vec<String> = fields.iter().map(|(f, _, _)| f.clone()).collect();

                let with = totals(&db, dataset, &ident, &container, is_pool, 4, archetype);
                let without = totals(
                    &stripped, dataset, &ident, &container, is_pool, 4, archetype,
                );
                let mut multi = Vec::new();
                diff(&with, &without, "", &mut multi);
                for (field, a, b) in multi {
                    if !single.contains(&field) {
                        fields.push((format!("{field}@4targets"), a, b));
                    }
                }
                for (field, a, b) in fields {
                    moved
                        .entry(field)
                        .or_insert_with(|| (Vec::new(), a, b))
                        .0
                        .push(archetype.clone());
                }
            }
            // A field every archetype moves is reported bare; one only some archetypes move
            // NAMES them, because that is the fork-shaped case this widening exists to catch —
            // the bag answering for the builds whose arm the atom stream does not carry.
            let fields: Vec<(String, String, String)> = moved
                .into_iter()
                .map(|(field, (ats, a, b))| {
                    let field = if ats.len() == classes.len() {
                        field
                    } else {
                        format!("{field}@{}", ats.join("+"))
                    };
                    (field, a, b)
                })
                .collect();
            if fields.is_empty() {
                continue;
            }
            let keys = db
                .all_powers()
                .find(|p| p.ident() == ident)
                .or_else(|| db.inherent_powers.iter().find(|p| p.ident() == ident))
                .map(bag_keys)
                .unwrap_or_default();
            for (field, a, b) in &fields {
                let entry = by_field
                    .entry(field.clone())
                    .or_insert_with(|| (0, format!("{name} ({a} → {b})")));
                entry.0 += 1;
            }
            movers.push((
                format!("{name} [{container}]"),
                fields
                    .iter()
                    .map(|(f, a, b)| format!("{f} {a}→{b}"))
                    .collect(),
                keys,
            ));
        }

        // The archetype count rides the header because it is the probe's breadth, and a sweep
        // that silently narrowed to one class would report the same clean result as a wide one.
        println!(
            "\n=== {} — {} powers change when the bag is removed ({} archetypes probed)",
            dataset.as_str(),
            movers.len(),
            classes.len()
        );
        for (field, (count, example)) in &by_field {
            println!("  {field:<44} {count:>4}   e.g. {}", truncate(example, 110));
        }
        // Carriers are behind a flag: the field summary is the work list, and the full list runs
        // to several hundred powers once the stacking probe is included.
        if std::env::var("CARRIERS").is_ok() {
            println!("  --- carriers:");
            for (name, fields, keys) in &movers {
                println!(
                    "    {name}\n      moved: {}\n      bag:   {}",
                    truncate(&fields.join(", "), 400),
                    keys.join(",")
                );
            }
        }
    }
}

fn truncate(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((cut, _)) => format!("{}…", &text[..cut]),
        None => text.to_string(),
    }
}
