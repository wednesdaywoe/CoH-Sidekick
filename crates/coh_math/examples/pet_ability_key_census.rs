//! Does a pet ability's own atoms state the display keys its `PetEffect` rows state?
//!
//! Job 4 of the display item is the pseudo-pet merge. A summoner's display bag carries a second
//! character's rows — `display_effects` merges `pseudo_pet_effects` in underneath the summoner's
//! own bag — and the summoner's own atoms have no reason to hold them, so the atom router reads
//! short against the built bag on nineteen effect keys (`slow` 124 of 124, `knockback` 100 of
//! 100). Not one of those misses is the router's. The atom-side answer is not a new projection:
//! it is running the PET's own atoms through the same router, which is what the converter half
//! (9209d2f13b) made possible by minting `atoms` on every pet ability.
//!
//! So this asks, per ability: does `bag_slots` over that ability's own atoms state the keys
//! `pseudo_pet_effects` states for it?
//!
//! Both sides are the real readers rather than a re-derivation. The atom side is `bag_slots`.
//! The current side is `pseudo_pet_effects`, reached by injecting the one ability into the pet
//! table under a probe name and pointing a synthetic summoner at it (the pattern is
//! `pet_faced_fold::inject_entity`). The alternative — restating `display_slot`'s match and its
//! four per-row drop conditions here, since `display_slot` is private — is exactly the second
//! copy of a rule that `granted.rs` refuses on the rest of its public surface, and it drifts.
//!
//! Three things net out before a number here means anything:
//!
//!   * **The two vocabularies differ by construction.** The merge writes 22 keys; `bag_slots`
//!     over the same abilities writes about 53. The surplus families are `OwnStat`, `AllyAura`
//!     and `Faced` verdicts — the pet's own stat sheet and the auras it projects, deliberately
//!     off the summoning power's card — plus `buffDuration`/`effectDuration`, which are
//!     `WindowSlots` fields rather than bag keys. The diff is therefore restricted to the
//!     vocabulary the merge can write, derived from the corpus's own effect types through the
//!     public `pet_effect_route` rather than hand-listed, so a route table that grows is picked
//!     up here instead of silently falling outside the comparison.
//!   * **All four forks carry pet atoms** since the 2026-08-24 regen. Until then only Homecoming
//!     did, and because `PowerWire::atoms` is `#[serde(default)]` the other three decoded to an
//!     empty atom list and would have reported a clean 0-of-0 rather than an absence — so the
//!     zero-coverage branch below names an uncovered fork out loud instead of printing a silent
//!     zero, and every covered fork gets a non-vacuity tripwire. Both stay: the mint is
//!     per-dataset, so a fork can go dark again on the next converter change.
//!   * **The entity table is not the whole merge.** `pseudo_pet_effects` has a second source —
//!     the inline `summon.resolvedEntities` block `convert-powerset.cjs` writes for the
//!     synthesized location pets (Storm Cell, Category Five, Freezing Rain) — and the converter
//!     half minted no atoms there. That population is counted in its own section rather than
//!     left outside scope, because a clean number over the entity table alone is a lie about
//!     the merge.
//!
//! Two reporting artifacts to read past. `pseudo_pet_effects` merges a whole entity chain, so it
//! has no per-ability caller in the app; per-ability is nonetheless the right grain for a
//! PRESENCE question, because a union of key sets aggregates to the summoner's without loss —
//! it is the VALUES that merge, and they are not compared here. And the census sweeps every
//! ability in the table, not only the ones a build can reach; reachability is reported beside
//! the diff so an unreachable residual is not read as a live defect.
//!
//! One divergence class is excused by name rather than counted, and the `ADJUDICATED` table below
//! carries the ground: `healing` reaches the merge by a single route whose player-side display
//! value comes from the `damage` def field rather than from any atom, so it is job 2's population
//! and not this one's. The excuse is scoped to the bag→atom direction because the key's other half
//! IS atom-projected, and it is asserted to fire.
//!
//! Reported, not asserted: the residual is a corpus fact and a work list. Set `PET_KEY_DETAIL=1`
//! for the per-ability divergence lines behind the summary (`bag_removal_census`'s `CARRIERS`
//! convention — the full list runs to hundreds of rows).
//!
//! Run: `cargo run -p coh_math --release --features census-probe --example pet_ability_key_census`

use coh_data::{DatasetId, Power, PowerDatabase};
use coh_math::granted::{
    pet_effect_route, pseudo_pet_effects, summon_entity_names, summoned_entity_chain,
    PetEffectRoute,
};
use coh_math::window_slots::bag_slots;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// Same rationale as `display_value_census.rs:15` — spelling the width as `DatasetId::ALL.len()`
/// makes a fork added to the enum a compile error here rather than a silently unswept fork.
const FORKS: [DatasetId; DatasetId::ALL.len()] = DatasetId::ALL;

/// The name the one-ability probe entity is injected under. Asserted absent from the real table
/// before use, so a fork that ever ships this name fails loudly instead of being overwritten.
const PROBE_ENTITY: &str = "__pet_ability_key_census_probe";

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// Every archetype this dataset states a class token for — the roster the fork restore in
/// `bag_slots` projects over. Asserted non-trivial for the reason `display_key_census` states:
/// a truncated roster makes the restoration vacuous instead of loud (AT-FORK-2).
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

/// Every power that can carry a summon: the powerset powers, both partitions, and the inherents.
fn corpus(db: &PowerDatabase) -> Vec<Power> {
    db.all_powers()
        .cloned()
        .chain(db.inherent_powers.iter().cloned())
        .collect()
}

/// One row of the census population: the entity that holds the ability, the ability's own name,
/// and the ability record itself.
struct Ability {
    entity: String,
    name: String,
    record: Value,
}

/// Every ability the fork's pet table holds, reachable or not. Wider than what any power summons
/// — see `reachable_entities` for the narrowing, which is reported rather than applied.
fn pet_abilities(db: &PowerDatabase) -> Vec<Ability> {
    let Some(entities) = db
        .sections
        .get("pet-entities")
        .and_then(|section| section.get("entities"))
        .and_then(Value::as_object)
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (entity, record) in entities {
        for ability in record
            .get("abilities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.push(Ability {
                entity: entity.clone(),
                name: ability
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("<unnamed>")
                    .to_string(),
                record: ability.clone(),
            });
        }
    }
    out
}

/// Every effect type either source of the merge ships on this fork.
///
/// Both sources, because the merge reads both. Deriving this from the entity table alone put
/// `resistanceDebuff` outside the vocabulary — it is a bag-bearing route key that no Homecoming
/// entity-table ability's types reach, only the inline block's — so it landed in the by-design
/// bucket and read as an atom-only surplus instead of as a key the comparison should cover.
fn collect_types(ability: &Value, out: &mut BTreeSet<String>) {
    for row in ability
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(effect_type) = row.get("type").and_then(Value::as_str) {
            out.insert(effect_type.to_string());
        }
    }
}

fn effect_types(abilities: &[Ability], powers: &[Power]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for ability in abilities {
        collect_types(&ability.record, &mut out);
    }
    for power in powers {
        for entity in inline_entities(power) {
            for ability in entity
                .get("abilities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                collect_types(ability, &mut out);
            }
        }
    }
    out
}

/// The display keys the merge can actually write, derived from those types through the public
/// `pet_effect_route`.
///
/// Not hand-listed: `PET_EFFECT_ROUTES` is private, and a hand copy of its bag-bearing keys is
/// the drift `granted.rs` keeps its router public to avoid. Deriving it also means a route table
/// that grows lands inside the comparison rather than outside it. The cost is stated rather than
/// hidden — a `Scalar`/`Control` route whose type appears nowhere in this fork contributes no
/// key, which is right for a presence census over this fork and would be wrong for a claim about
/// the route table itself.
fn merge_vocabulary(types: &BTreeSet<String>) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for effect_type in types {
        if let Some(PetEffectRoute::Scalar(key) | PetEffectRoute::Control(key)) =
            pet_effect_route(effect_type)
        {
            out.insert(key);
        }
    }
    out
}

/// The bag keys one ability's own `PetEffect` rows route to, before the merge's per-row drop
/// conditions are applied. The gap between this and what the merge published is what separates
/// "the atom router invented a key" from "the merge declined a row it routed".
fn routed_keys(ability: &Value) -> BTreeSet<&'static str> {
    let mut out = BTreeSet::new();
    for row in ability
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(effect_type) = row.get("type").and_then(Value::as_str) else {
            continue;
        };
        if let Some(PetEffectRoute::Scalar(key) | PetEffectRoute::Control(key)) =
            pet_effect_route(effect_type)
        {
            out.insert(key);
        }
    }
    out
}

/// The inline `summon.resolvedEntities` records on one power, if it carries the block.
fn inline_entities(power: &Power) -> &[Value] {
    power
        .summon()
        .and_then(|summon| summon.get("resolvedEntities"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The entities some power's summon actually delivers, through the real walk.
///
/// `summoned_entity_chain` applies the two rules that decide this — commandable pets and their
/// subtrees are skipped, and only in-place `createsEntities` summons are followed — and returns
/// the records rather than their names, so the names come back by matching each returned record
/// against the table it was borrowed from. That keeps the rule in one place; re-deriving the walk
/// here to get names out of it would be a second copy of it.
fn reachable_entities(db: &PowerDatabase, powers: &[Power]) -> BTreeSet<String> {
    let mut by_address: BTreeMap<usize, &str> = BTreeMap::new();
    if let Some(entities) = db
        .sections
        .get("pet-entities")
        .and_then(|section| section.get("entities"))
        .and_then(Value::as_object)
    {
        for (name, record) in entities {
            by_address.insert(record as *const Value as usize, name.as_str());
        }
    }

    let mut out = BTreeSet::new();
    for power in powers {
        let Some(summon) = power.summon() else {
            continue;
        };
        for root in summon_entity_names(summon) {
            for entity in summoned_entity_chain(db, &root) {
                if let Some(name) = by_address.get(&(entity as *const Value as usize)) {
                    out.insert((*name).to_string());
                }
            }
        }
    }
    out
}

/// The merge's OTHER source: `summon.resolvedEntities`, written into the powerset files by
/// `convert-powerset.cjs` for the synthesized location pets, and read at `granted.rs:1379` when
/// the entity table produced nothing. Returns (blocks, abilities, abilities with atoms, rows).
fn inline_population(powers: &[Power]) -> (usize, usize, usize, usize) {
    let (mut blocks, mut abilities, mut with_atoms, mut rows) = (0, 0, 0, 0);
    for power in powers {
        let resolved = inline_entities(power);
        if resolved.is_empty() {
            continue;
        }
        blocks += 1;
        for entity in resolved {
            for ability in entity
                .get("abilities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                abilities += 1;
                if ability
                    .get("atoms")
                    .and_then(Value::as_array)
                    .is_some_and(|atoms| !atoms.is_empty())
                {
                    with_atoms += 1;
                }
                rows += ability
                    .get("effects")
                    .and_then(Value::as_array)
                    .map_or(0, Vec::len);
            }
        }
    }
    (blocks, abilities, with_atoms, rows)
}

/// Give the probe entity this one ability, so `pseudo_pet_effects` reads exactly it.
///
/// `commandable: false` because `summoned_entity_chain` skips commandable pets and their
/// subtrees outright (`granted.rs:1571`), and no `characterClass`, which costs the rows their
/// `petClass` stamp — irrelevant to a presence census and load-bearing the moment this extends
/// to values, since the pet's class is the axis its magnitudes resolve against.
fn inject(db: &mut PowerDatabase, ability: &Value) {
    let entities = db
        .sections
        .get_mut("pet-entities")
        .and_then(|section| section.get_mut("entities"))
        .and_then(Value::as_object_mut)
        .expect("the bundle carries a pet-entities section");
    entities.insert(
        PROBE_ENTITY.to_string(),
        json!({
            "displayName": PROBE_ENTITY,
            "commandable": false,
            "abilities": [ability],
        }),
    );
}

/// Divergences excused by name, with the ground for each. One entry today.
///
/// `healing` is a HYBRID key and that is the whole reason this can be scoped honestly. It has two
/// producers that do not share a source: `convert-powerset.cjs:6861` writes it into the bag from
/// the `hitPoints` resource at a non-maximum aspect, which `window_slots.rs:2315` mirrors exactly;
/// and `display_effects` (`granted.rs:633-641`) derives one from the power's `damage` array for a
/// `Heal`-attrib heal, which is a display transform over a DEF FIELD and not a projection of any
/// atom. `damage` is one of the 19 keys `display_slot_presence_atom_bag_parity` subtracts as
/// `EXECUTION_STATS`, on exactly that ground.
///
/// The merge reaches `healing` by one route only — `("Heal", Scalar("healing"))`, `granted.rs:1077`
/// — so every `healing` the bag publishes here is the def-field half, and every `healing` the atom
/// path states is the `MaxHp` half. The two are disjoint on this corpus (bag 88, atom-only 20,
/// intersection 0), so excusing the bag→atom direction leaves the atom→bag direction fully graded
/// and blinds neither. Excusing the KEY instead of this direction would hide both.
///
/// ENT-19. Asserted to fire below, because an excuse that stops matching is a stale excuse.
const ADJUDICATED: &[(&str, &str)] = &[(
    "healing",
    "ENT-19 — the merge's only route to this key is a `Heal` row, whose player-side display value \
     comes from the `damage` def field rather than from any atom",
)];

fn adjudicated(key: &str) -> Option<&'static str> {
    ADJUDICATED
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, reason)| *reason)
}

#[derive(Default)]
struct KeyStat {
    /// Abilities with a `PetEffect` row routing to this key, before the merge's drop conditions.
    routed: usize,
    /// Abilities whose merged bag actually carries the key.
    bag: usize,
    /// …of those, the ones whose own atoms state it too.
    atom_has: usize,
    /// Abilities whose atoms state it where the merge does not.
    atom_only: usize,
    /// Bag-states-it-and-atoms-do-not, excused by an `ADJUDICATED` entry rather than counted.
    excused: usize,
    /// …of those, the ones that HAD a row routing to the key — so the merge dropped a routed row
    /// (a sub-1.0 chance, a missing scale or table) rather than the atom router inventing a key.
    dropped: usize,
}

fn main() {
    let detail = std::env::var("PET_KEY_DETAIL").is_ok();

    for dataset in FORKS {
        let mut db = load(dataset);
        let classes = roster(&db);
        let class_refs: Vec<&str> = classes.iter().map(String::as_str).collect();
        let abilities = pet_abilities(&db);
        let powers = corpus(&db);
        let reachable = reachable_entities(&db, &powers);
        let (inline_blocks, inline_abilities, inline_with_atoms, inline_rows) =
            inline_population(&powers);

        let entities = db
            .sections
            .get("pet-entities")
            .and_then(|section| section.get("entities"))
            .and_then(Value::as_object)
            .map_or(0, serde_json::Map::len);
        let with_atoms = abilities
            .iter()
            .filter(|ability| {
                ability
                    .record
                    .get("atoms")
                    .and_then(Value::as_array)
                    .is_some_and(|atoms| !atoms.is_empty())
            })
            .count();

        println!("\n=== {} ===", dataset.as_str());
        println!(
            "pet table: {entities} entities, {} abilities, {} reachable entities",
            abilities.len(),
            reachable.len(),
        );
        println!(
            "atom coverage: {with_atoms} of {} abilities carry atoms",
            abilities.len()
        );

        if with_atoms == 0 {
            println!(
                "  the converter half has not run for this fork — `convert-pet-entities.cjs` \
                 mints atoms per dataset, so a fork is covered only once it has been regenerated. \
                 Nothing to census here; this is a converter gap, not a clean result."
            );
            println!(
                "inline resolvedEntities: {inline_blocks} blocks, {inline_abilities} abilities, \
                 {inline_with_atoms} with atoms, {inline_rows} effect rows"
            );
            continue;
        }

        // Non-vacuity: a fork that carries atoms must carry them broadly, or a later run that
        // regressed the mint would report a small clean diff instead of a loss.
        assert!(
            with_atoms * 2 > abilities.len(),
            "{} carries atoms on only {with_atoms} of {} pet abilities — the mint regressed",
            dataset.as_str(),
            abilities.len(),
        );

        let vocab = merge_vocabulary(&effect_types(&abilities, &powers));
        println!(
            "merge vocabulary: {} keys reachable from this fork's effect types — {:?}",
            vocab.len(),
            vocab,
        );

        assert!(
            !db.sections["pet-entities"]["entities"]
                .as_object()
                .expect("entities object")
                .contains_key(PROBE_ENTITY),
            "{PROBE_ENTITY} is a real entity in {} — pick another probe name",
            dataset.as_str(),
        );

        let summoner = Power::from_value(json!({
            "name": "__pet_ability_key_census_summoner",
            "effects": { "summon": { "entity": PROBE_ENTITY } },
        }))
        .expect("the synthetic summoner decodes");

        let mut stats: BTreeMap<&'static str, KeyStat> = BTreeMap::new();
        let mut outside: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut divergences: Vec<String> = Vec::new();
        let mut excused_fired: BTreeSet<&'static str> = BTreeSet::new();
        let mut decode_errors: Vec<String> = Vec::new();
        let (mut atoms_empty, mut bag_empty, mut both_stated) = (0, 0, 0);

        for ability in &abilities {
            let power = match Power::from_value(ability.record.clone()) {
                Ok(power) => power,
                Err(err) => {
                    decode_errors.push(format!("{}/{}: {err}", ability.entity, ability.name));
                    continue;
                }
            };
            let atom_keys = bag_slots(&power, dataset, &class_refs).keys();

            inject(&mut db, &ability.record);
            let bag = pseudo_pet_effects(&summoner, &db);

            if atom_keys.is_empty() {
                atoms_empty += 1;
            }
            if bag.is_empty() {
                bag_empty += 1;
            }
            if !atom_keys.is_empty() && !bag.is_empty() {
                both_stated += 1;
            }

            let routed = routed_keys(&ability.record);
            for key in &routed {
                stats.entry(key).or_default().routed += 1;
            }

            for key in &atom_keys {
                if !vocab.contains(key) {
                    *outside.entry(key).or_insert(0) += 1;
                }
            }

            for key in bag.keys() {
                let Some(known) = vocab.get(key.as_str()) else {
                    // The merge wrote a key outside the vocabulary this fork's types derive.
                    // That means `pet_effect_route` grew a route the derivation missed, which is
                    // a fact about the derivation and has to be loud.
                    divergences.push(format!(
                        "{}/{}: merge wrote {key:?}, outside the derived vocabulary",
                        ability.entity, ability.name
                    ));
                    continue;
                };
                let stat = stats.entry(known).or_default();
                stat.bag += 1;
                if atom_keys.contains(known) {
                    stat.atom_has += 1;
                } else if let Some(reason) = adjudicated(known) {
                    stat.excused += 1;
                    excused_fired.insert(known);
                    let _ = reason;
                } else {
                    divergences.push(format!(
                        "{}/{} [{}]: bag states {known:?}, atoms do not",
                        ability.entity,
                        ability.name,
                        if reachable.contains(&ability.entity) {
                            "reachable"
                        } else {
                            "unreachable"
                        },
                    ));
                }
            }

            for key in &atom_keys {
                if vocab.contains(key) && !bag.contains_key(*key) {
                    let stat = stats.entry(key).or_default();
                    stat.atom_only += 1;
                    if routed.contains(key) {
                        stat.dropped += 1;
                    }
                }
            }
        }

        println!(
            "\nabilities: {} atoms-empty, {} bag-empty, {} stated by both",
            atoms_empty, bag_empty, both_stated,
        );
        if !decode_errors.is_empty() {
            println!("DECODE ERRORS: {}", decode_errors.len());
            for line in decode_errors.iter().take(10) {
                println!("  {line}");
            }
        }

        println!("\n-- per-key presence: the merged PetEffect bag vs the ability's own atoms --");
        println!(
            "{:<22} {:>7} {:>6} {:>9} {:>10} {:>8} {:>10} {:>8}",
            "key", "routed", "bag", "atom-has", "atom-miss", "excused", "atom-only", "dropped",
        );
        let mut graded = 0;
        let mut missed = 0;
        for (key, stat) in &stats {
            let miss = stat.bag - stat.atom_has - stat.excused;
            graded += stat.bag;
            missed += miss;
            println!(
                "{key:<22} {:>7} {:>6} {:>9} {:>10} {:>8} {:>10} {:>8}",
                stat.routed,
                stat.bag,
                stat.atom_has,
                miss,
                stat.excused,
                stat.atom_only,
                stat.dropped,
            );
        }
        println!("graded {graded} key-on-ability statements, {missed} not stated by the atom path");
        println!(
            "  `routed` counts abilities with a row routing to the key; `bag` the ones the merge \n               published. Their difference is the merge's own per-row drop conditions, and \n               `dropped` is the share of the atom-only column those explain."
        );

        for (key, reason) in ADJUDICATED {
            assert!(
                excused_fired.contains(key),
                "{} excuses {key:?} and nothing matched it — a stale excuse hides a regression \
                 instead of naming one ({reason})",
                dataset.as_str(),
            );
        }

        println!("\n-- atom keys outside the merge vocabulary (OwnStat / AllyAura / Faced verdicts, and the two WindowSlots fields) --");
        let mut outside_rows: Vec<_> = outside.iter().collect();
        outside_rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (key, count) in outside_rows.iter().take(20) {
            println!("  {key:<24} {count}");
        }
        if outside_rows.len() > 20 {
            println!("  … {} more", outside_rows.len() - 20);
        }

        println!("\n-- the merge's second source: inline resolvedEntities --");
        println!(
            "  {inline_blocks} blocks, {inline_abilities} abilities, {inline_with_atoms} with \
             atoms, {inline_rows} effect rows"
        );
        if inline_with_atoms == 0 && inline_abilities > 0 {
            println!(
                "  none carries atoms: `convert-powerset.cjs` writes this block and the converter \
                 half only taught `convert-pet-entities.cjs` to mint. These abilities have no \
                 atom side at all, so their share of the merge is uncensused here."
            );
        }

        if detail && !divergences.is_empty() {
            println!("\n-- divergences ({}) --", divergences.len());
            for line in &divergences {
                println!("  {line}");
            }
        } else if !divergences.is_empty() {
            println!(
                "\n{} per-ability divergences — set PET_KEY_DETAIL=1 for the list",
                divergences.len()
            );
            for line in divergences.iter().take(5) {
                println!("  {line}");
            }
        }
    }
}
