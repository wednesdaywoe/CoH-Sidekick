//! The totals-replay harness: `recalculate` against `fixtures/totals/`, whole build for whole
//! build.
//!
//! WHAT THIS IS. 143 whole-build records — `{build, totals, stats}` — where `totals` is the
//! COMPLETE 86-field dump of the TS `globalBonuses` accumulator, zeros included, not a delta
//! list. The highest value per record, and the only frozen thing that
//! reaches the 26 public readers in `appliers/` with no per-power record at all. Deliberately
//! last, because a whole-build mismatch says a number is wrong across forty readers and not
//! which one — the appliers had to be graded first (stages 2-4, 6,908 of 6,908) for these to be
//! diagnosable. 37 homecoming, 37 brainstorm, 35 rebirth, 34 thunderspy.
//!
//! WHO PRODUCED THEM. `scripts/emit-totals-fixtures.ts`, deleted with all of `src/` in
//! `6caffef34` and readable at `6caffef34^`. It called the TS `calculateCharacterTotals` over a
//! hand-curated corpus of synthetic builds and dumped the accumulator whole. Its own header says
//! why they are synthetic rather than sampled from the game: the per-power appliers are already
//! corpus-gated, so "this tier proves the PASS COMPOSITION (gather → strength/ED → apply →
//! inherents → caps), which needs whole builds, not every game power". The Rust equivalent is
//! ONE call to [`coh_math::recalculate`] — every pass, in the order the app runs them, with
//! nothing switched off. That is the difference from stage 5's three files, each of which
//! replayed an isolated chain.
//!
//! THE GATE THE EMITTER NAMED NEVER EXISTED. Its header describes a `totals_gate` asserting "only
//! the `GlobalBonuses` fields M3 implements (a growing allow-list)", with the list called
//! `M3_FIELDS`. No such gate is in this tree, in Rust or anywhere else — the same never-existed
//! shape item 22 found in three named export tests and stage 5 found in `movement_gate`. The
//! allow-list it describes is what the whole-struct comparison below replaces, so the field set
//! can no longer grow or shrink without the run saying so.
//!
//! THE BUILD DESERIALIZES DIRECTLY, and then has to survive coming back. The emitter wrote
//! `build` as "the exact `coh_data::CharacterState` serde shape the Rust gate deserializes", so
//! this parses it with `serde_json` rather than rebuilding it field by field. That alone is not
//! strict enough: `CharacterState` does not deny unknown fields, so a key nested inside
//! `primary` or `combat` would be read past in silence and the replay would run a different
//! build. [`recorded_survives`] closes it — deserialize, re-serialize, and require every value
//! the record states to come back unchanged, comparing numbers as numbers so a recorded `0`
//! matches an `f64` `0.0`. The other direction is reported rather than failed: 22 fields our
//! `CharacterState` carries are absent from the record, every one `#[serde(default)]` and added
//! after the freeze, and their defaults ARE the reading the TS had. They print once per fork, so
//! what the frozen corpus does not pin is visible instead of merely true.
//!
//! WHY THE COMPARISON IS WHOLE-STRUCT. `GlobalBonuses` has ~150 fields and the records name 86,
//! so a field-by-field reader would be a hand-kept second copy of [`GlobalBonuses::get`]'s match
//! arms — the kind of list that drifts silently. Instead BOTH sides are written into a fresh
//! `GlobalBonuses` through the calc's own routing and compared with `PartialEq`: the recorded
//! value through [`GlobalBonuses::add_by_camel_name`], and ours through `get` and then the same
//! router. Every one of the 86 is covered in both directions with no list to keep, an unroutable
//! recorded name is a hard failure rather than a silent skip, and the ~64 fields the record
//! never stated are zero on both sides by construction — which is the honest reading, since the
//! TS dump did not state them either. This is the piece stage 5 was run partly to prove
//! (`proc_replay.rs` established it on 34 fields); the failure report names the differing fields
//! in Rust snake_case, because that is what the struct serializes as, so `runSpeed` reads as
//! `run_speed`.
//!
//! A SHARED ROUTER COULD CANCEL A MIS-ROUTE, so the mapping is measured rather than read.
//! Both sides go through the same door, so a name routed to the wrong field would land the same
//! way twice and pass. It cannot cancel while the mapping is INJECTIVE: two names sharing one
//! field is the only way a wrong value hides. [`grade_router_injectivity`] routes all 84 routable
//! names with a distinct sentinel each and requires 84 distinct fields to come out non-zero.
//! Distinct sentinels rather than a shared one because [`ASSIGN_ONLY`]'s seven arms are hand
//! written in this file and ASSIGN rather than add: a collision there overwrites instead of
//! summing, and a shared sentinel would not notice. Proved able to fire from both directions —
//! an add-collision and an assign-collision each take it to 82.
//!
//! SEVEN RECORDED NAMES ARE ASSIGNED, NOT ADDED, which is why `add_by_camel_name` has no arm for
//! them: the two stealth radii are the output of a grouped-max resolve, and `toggleEndCost`,
//! `netEndPerSec`, `baseToHit`, `hitChance` and `combatModifier` are written once by passes that
//! run after the walk. Building a total from a recorded dump starts at zero, where assignment and
//! addition coincide. [`ASSIGN_ONLY`] is a closed list carrying each one's reason; any other name
//! the router rejects stays a hard failure. `proc_replay.rs` found the first two of these.
//!
//! TWO RECORDED NAMES REACH NO FIELD AT ALL. `threatLevel` and `enduranceDiscount` are stated
//! as zero by all 143 builds, and for `enduranceDiscount` the beta agrees it is vestigial —
//! `GlobalBonuses::toggle_end_cost`'s doc says the beta field of that name "is never
//! accumulated" and `endurance` IS the EndDisc sum. Both are declared in [`UNMODELLED`] and
//! re-checked by value: a non-zero would be a real drop and fails.
//!
//! THERE WAS A THIRD, AND IT IS NOW GRADED. The first run of this file found `protRepel`: the
//! beta routes a `repel` protection atom to it and this crate had no field to hold it, on five
//! builds stating 10. It was declared as an allowed drop, written to redden the moment the field
//! appeared. The field appeared — `GlobalBonuses::protection_repel`, with a `repel` entry in
//! `apply::MEZ_PROT_TYPES` reading through `kb_protection_value` — and the allowance is gone.
//! Those five builds are now graded on all 86 names like the other 138, and the fix is graded by
//! the frozen oracle that found the gap.
//!
//! `stats` IS RECORDED AND NOT GRADED HERE, and the reason is held so it cannot go stale. Those
//! 34 fields are the beta `CharacterStats`, the capped combine-by-max projection, and four of
//! them reach no [`CharacterStats::get`] arm — grading the block today would mean hand-writing
//! the bridge this file exists to avoid. Part of Pass 8 IS graded: `baseToHit`, `hitChance` and
//! `combatModifier` are written onto the accumulator by the purple-patch projection and turn all
//! 143 builds red when it is switched off. The resistance/HP caps and the S-L / F-C / E-N
//! combine-by-max are reached by nothing. [`STATS_UNBRIDGED`] reddens the day that changes.
//!
//! EXACT f64, no tolerance, for the reason `movement_replay.rs` gives.
//!
//! WHAT THESE 12,298 NUMBERS GRADE. All 143 agreed on the first run, which is the shape of a
//! vacuous check, so thirty perturbations were run before any of it was believed. Reddened
//! builds, out of 143:
//!
//! | Perturbation | Red | What that settles |
//! | --- | --- | --- |
//! | `toggleEndCost` + 0.001 | **143** | the Pass 9.7 toggle-drain sum, and the `netEndPerSec` close computed over it |
//! | the Pass 8 purple-patch projection not run | **143** | `baseToHit` / `hitChance` / `combatModifier`, on every build |
//! | every applier's `TypedValue` scale + 0.001 | **115** | the whole per-power applier family, at the one chokepoint the apply walk pulls them through |
//! | the archetype-table value × 1.001 | **92** | the AT-table resolution — the level `{scale, table}` records structurally cannot reach |
//! | every build replayed as `blaster` | **61** | the per-archetype FORK of that resolution |
//! | the mez-protection magnitude + 0.001 | **24** | the curated-armor protection fold |
//! | the debuff-resistance router + 0.001 | **21** | `add_debuff_resistance`'s arms |
//! | the ED curve disabled | **16** | enhancement diminishing returns, on every one of the 16 slotted builds |
//! | the SO origin-tier value × 1.001 | **16** | the slotted magnitude ED is applied to |
//! | the mez-resistance router + 0.001 | **16** | `add_mez_resistance`'s arms |
//! | the stealth PvE / PvP radii swapped | **15** | the split [`ASSIGN_ONLY`]'s first two entries exist for |
//! | Pass 6 (incarnates) not run | **14** | the Destiny / Hybrid / Genesis stat blocks |
//! | Pass 1 (strength) zeroed | **12** | the +Strength self-buffs and the multipliers they feed |
//! | the self-resistance-debuff resolve not run | **12** | the mitigated self-penalty (Bio Offensive Adaptation) |
//! | the per-power targets-hit slider ignored | **12** | per-foe stacking |
//! | the knockback/knockup fold `max` → `+=` | **10** | the pairing that would otherwise double KB protection |
//! | Pass 3 (archetype inherents) not run | **8** | Brute Fury and Defender Vigilance |
//! | the incarnate level shift + 0.001 | **8** | `apply_level_shift`'s ceiling spend |
//! | the level shift dropped from the con-level diff | **8** | that shift's feed INTO the purple patch |
//! | the enemy con-level offset ignored | **8** | the purple patch's other input |
//! | Pass 7 (movement resolve) not run | **5** | the travel-buff suppress-group resolve |
//! | the self-directed movement debuff not applied | **4** | the self-slow axes, which that resolve does not carry |
//! | incarnate exemplar suppression disabled | **4** | the below-45 gate |
//! | the flat absorb × 1.001 | **4** | the absorb applier |
//! | both proc passes not run | **0** | UNGRADED |
//! | the set-bonus layer-2 routing not run | **0** | UNGRADED |
//! | the gather's mode suppression not run | **0** | UNGRADED |
//! | the MaxHP-fraction absorb resolve × 1.001 | **0** | UNGRADED |
//! | the Alpha virtual-enhancement split not fed | **0** | UNGRADED |
//! | the toggle-off gate ignored | **0** | UNGRADED |
//!
//! SIX MECHANISMS ARE GRADED BY NOTHING, and in every case the cause is a property of the
//! sampled builds rather than sampling luck — the recurring shape of this whole item. Each is
//! measured from both ends: the perturbation above reddens zero, and [`Census`] says why, held
//! at zero so the day the corpus starts reaching one the run says so instead of leaving the
//! claim quietly false. Every build carries only plain SO pieces, so the set-bonus layers and
//! every proc pass contribute nothing (those are graded in `set_bonus_replay.rs` and
//! `proc_replay.rs`, 540 calls between them). 12 builds equip an Alpha and 16 slot something,
//! and the two sets are DISJOINT on every fork, so the Alpha virtual-enhancement split — which
//! only ever feeds a per-power ED aggregation — has nothing to feed. The four absorb builds all
//! use the flat applier, so the MaxHP-fraction resolve is unreached. No build reads in combat,
//! no build reads teamed, no picked power is recorded switched OFF, and the gather drops nothing
//! and leaves nothing unresolved. So combat suppression, the Vigilance team-size taper, the
//! toggle gate and mode suppression are all ungraded here. The emitter's own header is out of
//! date in both directions and worth reading against this list: it claims "ZERO set pieces, no
//! incarnates, no procs", and the corpus has since grown 17 incarnate builds and 16 slotted ones
//! while set pieces and procs stayed at zero.
//!
//! Run: `cargo test -p coh_math --test totals_replay -- --nocapture`

use coh_data::{CharacterState, DatasetId, PowerDatabase};
use coh_math::totals::TypeRoute;
use coh_math::{CharacterStats, GlobalBonuses};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The three keys every record carries.
const RECORD_KEYS: [&str; 3] = ["build", "totals", "stats"];

/// The `CharacterState` fields the emitter wrote. Every OTHER field on the struct is
/// `#[serde(default)]` and lands at its pre-field reading, which is the reading the TS had.
const BUILD_KEYS: [&str; 13] = [
    "name",
    "dataset",
    "archetype",
    "level",
    "primary",
    "secondary",
    "pools",
    "epic_pool",
    "inherents",
    "accolades",
    "incarnates",
    "slot_order",
    "combat",
];

/// The `CombatContext` keys the emitter wrote. `exemplar_level` is optional by construction
/// (`JSON.stringify` drops `undefined`), so it is allowed but not required; the other four are
/// required, because a combat input silently defaulted is a different call.
const COMBAT_REQUIRED: [&str; 4] = [
    "in_combat",
    "enemy_level_offset",
    "fury_level",
    "vigilance_team_size",
];
const COMBAT_OPTIONAL: [&str; 1] = ["exemplar_level"];

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = repo()
        .join("contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

// ---------------------------------------------------------------- their side

/// One recorded build: the arguments to replay, and the 86-field answer to replay them against.
struct Record {
    /// 1-based line in this fork's `synthetic.jsonl`, so a failure names the line to read.
    line: usize,
    /// The emitter's own probe name, which says what the build was curated to reach.
    name: String,
    state: CharacterState,
    /// All 86 recorded fields, zeros included — this is a whole dump, not a delta list.
    totals: BTreeMap<String, f64>,
    /// The recorded `stats` key set. Not graded here; see [`grade_stats_are_ungradeable`].
    stats_fields: Vec<String>,
}

/// Panic on any field this reader does not know, in either direction. The twin in
/// `movement_replay.rs` / `proc_replay.rs`: a key the emitter wrote and this reader drops would
/// pass by omission, and an argument silently defaulted replays a different call.
fn exact_fields(object: &serde_json::Map<String, Value>, allowed: &[&str], what: &str) {
    for field in object.keys() {
        assert!(
            allowed.contains(&field.as_str()),
            "{what}: unknown field {field:?} — the fixture states something this replay drops, \
             which would pass by omission"
        );
    }
    for field in allowed {
        assert!(
            object.contains_key(*field),
            "{what}: no {field:?} — a missing argument would be replayed at this reader's \
             default, which is a different call than the one recorded"
        );
    }
}

/// Two JSON numbers are the same number. The recorded `fury_level: 0` is a JSON integer and our
/// re-serialized `f64` is `0.0`; `Value`'s own `PartialEq` calls those different.
fn same_number(a: &Value, b: &Value) -> Option<bool> {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) if a.is_number() && b.is_number() => Some(x == y),
        _ => None,
    }
}

/// Every place the recorded build does NOT survive a round trip through `CharacterState`.
///
/// The emitter's header calls `build` "the exact `coh_data::CharacterState` serde shape the Rust
/// gate deserializes", so this holds it to that: deserialize, re-serialize, and require every
/// value the record states to come back unchanged. A field serde no longer knows, a type that
/// has changed, or an array that lost an element is then a hard failure at the point it happens
/// rather than a wrong total forty readers away. Fields OURS carries that the record does not
/// state are the other direction and are reported by [`beyond_the_record`], not failed: those
/// are the `#[serde(default)]` fields added after the freeze, and their defaults ARE the
/// behaviour the TS had.
fn recorded_survives(recorded: &Value, ours: &Value, path: &str, lost: &mut Vec<String>) {
    match (recorded, ours) {
        (Value::Object(want), Value::Object(got)) => {
            for (key, value) in want {
                let child = format!("{path}.{key}");
                match got.get(key) {
                    Some(mine) => recorded_survives(value, mine, &child, lost),
                    None => lost.push(format!(
                        "{child}: the record states {value} and the round trip dropped the field \
                         entirely"
                    )),
                }
            }
        }
        (Value::Array(want), Value::Array(got)) => {
            if want.len() != got.len() {
                lost.push(format!(
                    "{path}: the record states {} element(s) and the round trip produced {}",
                    want.len(),
                    got.len()
                ));
                return;
            }
            for (i, (value, mine)) in want.iter().zip(got).enumerate() {
                recorded_survives(value, mine, &format!("{path}[{i}]"), lost);
            }
        }
        (a, b) => {
            let equal = same_number(a, b).unwrap_or(a == b);
            if !equal {
                lost.push(format!(
                    "{path}: the record states {a} and we read it back as {b}"
                ));
            }
        }
    }
}

/// The paths OUR `CharacterState` carries that the record never states — the fields added since
/// the freeze, each sitting at its `#[serde(default)]`. Collected and printed once per fork
/// rather than per record: what the frozen corpus does not pin is exactly the list a reader of
/// this harness needs, and it is invisible if nothing prints it.
fn beyond_the_record(recorded: &Value, ours: &Value, path: &str, extra: &mut Vec<String>) {
    match (recorded, ours) {
        (Value::Object(want), Value::Object(got)) => {
            for (key, mine) in got {
                let child = format!("{path}.{key}");
                match want.get(key) {
                    Some(value) => beyond_the_record(value, mine, &child, extra),
                    None => extra.push(format!("{child} = {mine}")),
                }
            }
        }
        (Value::Array(want), Value::Array(got)) if want.len() == got.len() => {
            // Indexless, because these paths are deduplicated across 143 builds and a per-element
            // path would print the same defaulted field once per power.
            for (value, mine) in want.iter().zip(got) {
                beyond_the_record(value, mine, &format!("{path}[]"), extra);
            }
        }
        _ => {}
    }
}

fn parse_record(dataset: DatasetId, line_no: usize, line: &str, extra: &mut Vec<String>) -> Record {
    let where_ = format!(
        "fixtures/totals/{}/synthetic.jsonl:{line_no}",
        dataset.as_str()
    );
    let record: Value =
        serde_json::from_str(line).unwrap_or_else(|e| panic!("{where_}: not JSON: {e}"));
    let object = record
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: not an object"));
    exact_fields(object, &RECORD_KEYS, &where_);

    let build = record["build"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: build is not an object"));
    exact_fields(build, &BUILD_KEYS, &where_);
    let combat = build["combat"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: build.combat is not an object"));
    for field in combat.keys() {
        assert!(
            COMBAT_REQUIRED.contains(&field.as_str()) || COMBAT_OPTIONAL.contains(&field.as_str()),
            "{where_}: build.combat states {field:?}, which this replay drops"
        );
    }
    for field in COMBAT_REQUIRED {
        assert!(
            combat.contains_key(field),
            "{where_}: build.combat has no {field:?} — a combat input silently defaulted is a \
             different call than the one recorded"
        );
    }

    let state: CharacterState = serde_json::from_value(record["build"].clone())
        .unwrap_or_else(|e| panic!("{where_}: build is not a coh_data::CharacterState: {e}"));
    assert_eq!(
        state.dataset,
        dataset,
        "{where_}: the record names dataset {:?} and sits in the {} file",
        state.dataset,
        dataset.as_str()
    );

    let round_tripped = serde_json::to_value(&state).expect("CharacterState serializes");
    let mut lost = Vec::new();
    recorded_survives(&record["build"], &round_tripped, "build", &mut lost);
    assert!(
        lost.is_empty(),
        "{where_}: the recorded build does not survive a round trip through CharacterState, so \
         the replay would run a DIFFERENT build than the one recorded:\n  {}",
        lost.join("\n  ")
    );
    beyond_the_record(&record["build"], &round_tripped, "build", extra);

    let totals = record["totals"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: totals is not an object"))
        .iter()
        .map(|(field, v)| {
            (
                field.clone(),
                v.as_f64()
                    .unwrap_or_else(|| panic!("{where_}: totals.{field} is not a number")),
            )
        })
        .collect();

    let stats_fields = record["stats"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: stats is not an object"))
        .keys()
        .cloned()
        .collect();

    Record {
        line: line_no,
        name: state.name.clone(),
        state,
        totals,
        stats_fields,
    }
}

fn read_records(dataset: DatasetId, extra: &mut Vec<String>) -> Vec<Record> {
    let path = repo()
        .join("fixtures/totals")
        .join(dataset.as_str())
        .join("synthetic.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| parse_record(dataset, i + 1, line, extra))
        .collect()
}

// ---------------------------------------------------------------- the bridge

/// The recorded names this crate ASSIGNS rather than accumulates, each with why
/// [`GlobalBonuses::add_by_camel_name`] has no arm for it. Building a total from a recorded dump
/// starts from zero, where assignment and addition coincide, so these are assigned on both
/// sides. The list is CLOSED: any other name the router rejects stays a hard failure.
const ASSIGN_ONLY: [(&str, &str); 7] = [
    (
        "stealthRadiusPvE",
        "the stealth resolve returns a total (grouped max plus additive sum) rather than \
         accumulating, so adding into the field would double what it already decided",
    ),
    ("stealthRadiusPvP", "the PvP half of the same resolve"),
    (
        "toggleEndCost",
        "Pass 9.7 sums the projected per-power endurance costs after the projection and writes \
         the sum once",
    ),
    (
        "netEndPerSec",
        "the net-endurance close, written once beside it",
    ),
    (
        "baseToHit",
        "Pass 8 Step 9.5 writes the purple-patch projection onto the accumulator",
    ),
    ("hitChance", "the same projection"),
    ("combatModifier", "the same projection"),
];

fn assign_by_camel_name(g: &mut GlobalBonuses, field: &str, value: f64) -> bool {
    match field {
        "stealthRadiusPvE" => g.stealth_radius_pve = value,
        "stealthRadiusPvP" => g.stealth_radius_pvp = value,
        "toggleEndCost" => g.toggle_end_cost = value,
        "netEndPerSec" => g.net_end_per_sec = value,
        "baseToHit" => g.base_to_hit = value,
        "hitChance" => g.hit_chance = value,
        "combatModifier" => g.combat_modifier = value,
        _ => return false,
    }
    true
}

/// Recorded names this crate does not model AT ALL — no field, so neither
/// [`GlobalBonuses::get`] nor [`GlobalBonuses::add_by_camel_name`] can reach them. Each must be
/// stated as ZERO by every record, which is the only reading under which not modelling it costs
/// nothing. A non-zero is a hard failure that names the field: the frozen answer then holds a
/// number this crate structurally cannot produce, which is a real drop and not a vocabulary gap.
///
/// Both entries are measured at zero across all 143 builds on all four forks. A third name once
/// belonged beside them and no longer does: `protRepel`, which the corpus states non-zero on five
/// builds, now has `GlobalBonuses::protection_repel` behind it and is graded like any other.
const UNMODELLED: [(&str, &str); 2] = [
    (
        "threatLevel",
        "no GlobalBonuses field; the beta's threat total has no reader in this crate",
    ),
    (
        "enduranceDiscount",
        "vestigial in the beta too — `GlobalBonuses::toggle_end_cost`'s doc states it: the beta \
         field of that name is never accumulated, and `endurance` IS the EndDisc sum",
    ),
];

/// Write one recorded field into a fresh accumulator through the calc's OWN routing, or say why
/// it could not be. Used for BOTH sides: the recorded value, and our own value read back out by
/// [`GlobalBonuses::get`]. Both go through the same door, which is what makes the comparison
/// cover all 86 fields with no list kept beside it — and the reason [`grade_router_injectivity`]
/// exists is that a shared door could otherwise hide a mis-route.
fn route(g: &mut GlobalBonuses, field: &str, value: f64) -> Result<(), String> {
    match g.add_by_camel_name(field, value) {
        TypeRoute::Routed => Ok(()),
        TypeRoute::Unspent(why) => Err(format!(
            "recorded field {field:?} is deliberately not totalled by this crate ({why}), so the \
             record states a number nothing here can hold"
        )),
        TypeRoute::Unknown if ASSIGN_ONLY.iter().any(|(name, _)| *name == field) => {
            assert!(
                assign_by_camel_name(g, field, value),
                "{field:?} is declared in ASSIGN_ONLY and assign_by_camel_name has no arm for it"
            );
            Ok(())
        }
        TypeRoute::Unknown => Err(format!(
            "recorded field {field:?} reaches no GlobalBonuses arm and is not in ASSIGN_ONLY — \
             the record states a total this crate cannot express, which a field-by-field \
             comparison would have skipped in silence"
        )),
    }
}

/// The recorded dump as a `GlobalBonuses`, and OUR dump projected onto the same 86 names.
///
/// Both are fresh structs written through [`route`], so `PartialEq` over the whole struct is a
/// comparison of exactly the 86 recorded fields and nothing else. The ~64 fields the record does
/// not name are zero on both sides by construction, which is the honest reading: the TS dump
/// never stated them, so this corpus cannot grade them either way.
struct BothSides {
    mine: GlobalBonuses,
    theirs: GlobalBonuses,
}

fn both_sides(record: &Record, ours: &GlobalBonuses) -> Result<BothSides, Vec<String>> {
    let mut theirs = GlobalBonuses::default();
    let mut mine = GlobalBonuses::default();
    let mut bad = Vec::new();
    for (field, recorded) in &record.totals {
        if let Some((_, why)) = UNMODELLED.iter().find(|(name, _)| name == field) {
            if *recorded != 0.0 {
                bad.push(format!(
                    "the record states {field} {recorded} and this crate has no field for it \
                     ({why}). A zero would have cost nothing; a number is a real drop."
                ));
            }
            continue;
        }
        let Some(is_ours) = ours.get(field) else {
            bad.push(format!(
                "recorded field {field:?} reaches no GlobalBonuses::get arm and is not declared \
                 in UNMODELLED — this replay cannot read our own answer for it"
            ));
            continue;
        };
        if let Err(why) = route(&mut theirs, field, *recorded) {
            bad.push(why);
            continue;
        }
        if let Err(why) = route(&mut mine, field, is_ours) {
            bad.push(why);
        }
    }
    if bad.is_empty() {
        Ok(BothSides { mine, theirs })
    } else {
        Err(bad)
    }
}

// ---------------------------------------------------------------- our side

struct Replayed {
    g: GlobalBonuses,
    errors: Vec<String>,
}

/// Replay one build: `recalculate`, whole. Every pass, in the order the app runs them.
fn replay(db: &PowerDatabase, record: &Record) -> Replayed {
    let result = coh_math::recalculate(&record.state, db);
    let mut g = result.bonuses;
    let errors = g
        .errors
        .iter()
        .map(|e| format!("calc: {} — {}", e.context, e.detail))
        .collect();
    // Cleared so the whole-struct compare is over the NUMBERS. Nothing is lost: the channel is
    // reported above and any entry in it fails the run.
    g.errors.clear();
    // `result.stats` is deliberately not read: see `grade_stats_are_ungradeable`.
    Replayed { g, errors }
}

// ---------------------------------------------------------------- reporting

/// Which fields two `GlobalBonuses` disagree on, by diffing their serializations. Snake_case,
/// because that is what the struct serializes as: `runSpeed` reads as `run_speed`.
fn differing_fields(ours: &GlobalBonuses, theirs: &GlobalBonuses) -> Vec<String> {
    let to_map = |g: &GlobalBonuses| -> serde_json::Map<String, Value> {
        match serde_json::to_value(g).expect("GlobalBonuses serializes") {
            Value::Object(map) => map,
            other => panic!("GlobalBonuses did not serialize as an object: {other}"),
        }
    };
    let (a, b) = (to_map(ours), to_map(theirs));
    let mut out = Vec::new();
    for (field, ours) in &a {
        let theirs = b.get(field).unwrap_or(&Value::Null);
        if ours != theirs {
            out.push(format!("{field}: recorded {theirs}, ours {ours}"));
        }
    }
    out
}

// ---------------------------------------------------------------- the floors

/// The 86 recorded names must claim 86 DISTINCT `GlobalBonuses` fields.
///
/// Both sides of every comparison are written through the same [`route`], so a name routed to
/// the wrong field would cancel and pass. It cannot cancel if the mapping is injective: two
/// names sharing one field is the only way a wrong value hides, and this measures that directly
/// rather than trusting a reading of the match arms. The `UNMODELLED` names route nowhere and
/// are excluded by count, so the expected number is 86 minus that list.
fn grade_router_injectivity(record: &Record, failures: &mut Vec<String>) {
    let mut g = GlobalBonuses::default();
    let mut routed = 0usize;
    for (i, field) in record.totals.keys().enumerate() {
        if UNMODELLED.iter().any(|(name, _)| name == field) {
            continue;
        }
        // A DISTINCT sentinel per name. Two names that ADD into one field leave it holding
        // their sum and one field short; two that ASSIGN into one field leave it holding the
        // later value and one field short as well. Counting NON-ZERO FIELDS rather than
        // matching a shared sentinel catches both, and [`ASSIGN_ONLY`] is seven hand-written
        // arms in this file — exactly where an assign-collision would live.
        if route(&mut g, field, (i + 1) as f64).is_ok() {
            routed += 1;
        }
    }
    let Value::Object(map) = serde_json::to_value(&g).expect("GlobalBonuses serializes") else {
        panic!("GlobalBonuses did not serialize as an object");
    };
    let claimed = map
        .values()
        .filter(|v| v.as_f64().is_some_and(|n| n != 0.0))
        .count();
    if claimed != routed {
        failures.push(format!(
            "line {}: {routed} recorded names routed and they claimed only {claimed} distinct \
             GlobalBonuses fields. Two recorded names share a field, so a wrong value in one of \
             them is invisible: both sides of this comparison go through the same router, and a \
             collision is the one way that cancels.",
            record.line
        ));
    }
}

/// `stats` is recorded and NOT graded here, and this holds the reason so it cannot go stale.
///
/// The record's 34 `stats` fields are the beta `CharacterStats` — the capped, combine-by-max
/// projection. Four of them (`runspeed`, `flyspeed`, `jumpspeed`, `jumpheight`) reach no
/// [`CharacterStats::get`] arm, so grading that block today would mean hand-writing the bridge
/// this file exists to avoid. The 86 totals DO reach part of Pass 8 — `baseToHit`, `hitChance`
/// and `combatModifier` are written onto the accumulator by the purple-patch projection and are
/// graded on all 143 builds — but the resistance/HP caps and the S-L, F-C, E-N combine-by-max
/// are reached by nothing here.
///
/// Asserted rather than noted: the day `CharacterStats::get` grows the four movement arms, this
/// reddens and says the block became gradeable, instead of leaving the omission quietly
/// permanent.
const STATS_UNBRIDGED: [&str; 4] = ["runspeed", "flyspeed", "jumpspeed", "jumpheight"];

fn grade_stats_are_ungradeable(record: &Record, failures: &mut Vec<String>) {
    let unbridged: Vec<&String> = record
        .stats_fields
        .iter()
        .filter(|f| CharacterStats::default().get(f).is_none())
        .collect();
    let expected: Vec<&str> = STATS_UNBRIDGED.to_vec();
    let actual: Vec<&str> = unbridged.iter().map(|s| s.as_str()).collect();
    if actual != expected {
        failures.push(format!(
            "line {}: the recorded `stats` block has {:?} with no CharacterStats::get arm, and \
             this harness was written against {expected:?}. That is not a calc bug — it means \
             the capped-projection block this file deliberately leaves ungraded has become \
             gradeable (or has drifted). Re-read STATS_UNBRIDGED and say what changed.",
            record.line, actual
        ));
    }
}

/// What the corpus STRUCTURALLY cannot reach, held to the facts that make it unreachable.
///
/// Six mechanisms `recalculate` runs are graded by nothing here, and in every case the cause is
/// a property of the sampled builds rather than sampling luck. Each is measured from both ends:
/// the perturbation in this file's header reddens zero records, and the census below says why.
/// Held rather than noted, for the reason `movement_replay.rs`'s `SUPPRESS_CENSUS` gives — a
/// corpus that starts reaching one of these is exactly the event that changes what this file is
/// worth, and it can stop being true without anyone looking.
///
/// Every count is zero. They are spelled as counts rather than as booleans so a failure says
/// HOW MANY builds crossed the line.
#[derive(Debug, Default, PartialEq)]
struct Census {
    /// Builds carrying an enhancement that is not a plain origin (TO/DO/SO) piece. Zero, so the
    /// set-bonus layers and every proc pass contribute nothing: `fixtures/set-bonuses/` and
    /// `fixtures/procs/` are where those are graded, and neither is reachable from here.
    set_or_proc_pieces: usize,
    /// Builds that equip an Alpha incarnate AND slot at least one enhancement. Zero, and that
    /// is the whole reason the Alpha virtual-enhancement split is ungraded: Alpha feeds the
    /// per-power ED aggregation, 16 builds slot something, 12 equip an Alpha, and the two sets
    /// are disjoint on every fork.
    alpha_with_slots: usize,
    /// Builds carrying an accolade. Zero, so the accolade fold is ungraded.
    accolades: usize,
    /// Builds reading IN COMBAT. Zero, so combat suppression — the branch that drops a
    /// suppressible defense or travel buff — is ungraded, the same hole `movement_replay.rs`
    /// found from the other side.
    in_combat: usize,
    /// Builds reading at a team size other than solo. Zero, so Defender Vigilance is graded
    /// only at its solo maximum: the team-size taper is ungraded.
    teamed: usize,
    /// Picked powers recorded as switched OFF. Zero, so the toggle gate is ungraded — every
    /// recorded pick contributes, and an engine that ignored `is_active` entirely would pass.
    inactive_picks: usize,
    /// Picks the gather DROPPED (mode suppression) or could not resolve. Zero on both counts,
    /// which is the gather identity `movement_replay.rs` asserts for the same reason: this
    /// replay runs mode suppression and the TS gate did not, so the two agree only while the
    /// gather changes nothing.
    gather_dropped: usize,
}

fn census_of(db: &PowerDatabase, records: &[Record]) -> Census {
    let mut census = Census::default();
    for record in records {
        let state = &record.state;
        let slots: Vec<&coh_data::Enhancement> = state
            .all_selected()
            .flat_map(|pick| pick.slots.iter().flatten())
            .collect();
        if slots
            .iter()
            .any(|e| !matches!(e.kind, coh_data::EnhancementKind::Origin { .. }))
        {
            census.set_or_proc_pieces += 1;
        }
        if state.incarnates.alpha.is_some() && !slots.is_empty() {
            census.alpha_with_slots += 1;
        }
        if !state.accolades.is_empty() {
            census.accolades += 1;
        }
        if state.combat.in_combat {
            census.in_combat += 1;
        }
        if state.combat.vigilance_team_size != 1 {
            census.teamed += 1;
        }
        census.inactive_picks += state.all_selected().filter(|p| !p.is_active).count();
        let picked = state.all_selected().count();
        let gathered = coh_math::gather::gather_active_powers(state, db);
        census.gather_dropped +=
            !gathered.unresolved.is_empty() as usize + picked.saturating_sub(gathered.powers.len());
    }
    census
}

// ---------------------------------------------------------------- the run

fn read_manifest(dataset: DatasetId, records: usize, fields: usize) {
    let path = repo().join("fixtures/totals/manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let manifest: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let entry = manifest
        .get(dataset.as_str())
        .unwrap_or_else(|| panic!("{path:?}: no entry for {}", dataset.as_str()));
    let builds = entry["builds"]
        .as_u64()
        .unwrap_or_else(|| panic!("{path:?}: no {}.builds", dataset.as_str()));
    assert_eq!(
        builds as usize,
        records,
        "{path:?} states {builds} builds for {} and synthetic.jsonl holds {records} — the \
         manifest and the fixture were not written by the same run",
        dataset.as_str()
    );
    let declared = entry["totalsFields"]
        .as_u64()
        .unwrap_or_else(|| panic!("{path:?}: no {}.totalsFields", dataset.as_str()));
    assert_eq!(
        declared as usize,
        fields,
        "{path:?} states {declared} totals fields for {} and every record carries {fields}",
        dataset.as_str()
    );
}

/// `COH_WRITE_TOTALS=1`: bring this fork's recorded `totals` up to the current engine after a
/// game patch, as `oracle.rs`'s `COH_WRITE_ORACLE` does for the per-power records. Only the
/// fields that moved are rewritten, on only the builds that moved; the build, the `stats` dump
/// (never graded, see [`grade_stats_are_ungradeable`]) and every agreeing line stay byte-for-byte.
/// Rule every diff line against the patch notes before committing.
fn rewrite(dataset: DatasetId) {
    let path = repo()
        .join("fixtures/totals")
        .join(dataset.as_str())
        .join("synthetic.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let records = read_records(dataset, &mut Vec::new());
    let db = load(dataset);
    for record in &records {
        let ours = replay(&db, record).g;
        let mut value: Value = serde_json::from_str(&lines[record.line - 1]).expect("record");
        let mut moved = false;
        for (field, recorded) in &record.totals {
            if UNMODELLED.iter().any(|(name, _)| name == field) {
                continue;
            }
            let Some(now) = ours.get(field) else { continue };
            if now != *recorded {
                let n = if now.fract() == 0.0 && now.abs() < 1e15 {
                    Value::from(now as i64)
                } else {
                    Value::from(now)
                };
                value["totals"][field.as_str()] = n;
                moved = true;
            }
        }
        if moved {
            lines[record.line - 1] = value.to_string();
            println!("  rewrote line {} — {}", record.line, record.name);
        }
    }
    let mut out = lines.join("\n");
    if text.ends_with('\n') {
        out.push('\n');
    }
    std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
}

fn grade(dataset: DatasetId) {
    if std::env::var_os("COH_WRITE_TOTALS").is_some() {
        rewrite(dataset);
    }
    let mut beyond: Vec<String> = Vec::new();
    let records = read_records(dataset, &mut beyond);
    assert!(
        !records.is_empty(),
        "fixtures/totals/{}/synthetic.jsonl holds no records",
        dataset.as_str()
    );
    let widths: Vec<usize> = records.iter().map(|r| r.totals.len()).collect();
    let fields = widths[0];
    assert!(
        widths.iter().all(|w| *w == fields),
        "{}: the records do not all carry the same number of totals fields ({:?})",
        dataset.as_str(),
        widths
    );
    read_manifest(dataset, records.len(), fields);

    let db = load(dataset);
    let census = census_of(&db, &records);
    let mut failures: Vec<String> = Vec::new();
    if census != Census::default() {
        failures.push(format!(
            "{}: the corpus census is {census:?} and every count in it is held at zero. A \
             non-zero one is not a calc bug — it means this corpus has started reaching a \
             mechanism this file's header says it reaches by NOTHING. Re-measure that row's \
             perturbation, then update the header and this struct's docs.",
            dataset.as_str(),
        ));
    }
    let (mut graded, mut mismatched) = (0usize, 0usize);

    println!(
        "\n=== {} — {} recorded builds × {fields} fields",
        dataset.as_str(),
        records.len()
    );

    for record in &records {
        grade_router_injectivity(record, &mut failures);
        grade_stats_are_ungradeable(record, &mut failures);
        let replayed = replay(&db, record);
        for e in &replayed.errors {
            failures.push(format!("line {}: {e}", record.line));
        }
        let both = match both_sides(record, &replayed.g) {
            Ok(both) => both,
            Err(bad) => {
                for b in bad {
                    failures.push(format!("line {} ({}): {b}", record.line, record.name));
                }
                continue;
            }
        };
        if both.mine == both.theirs {
            graded += 1;
            continue;
        }
        mismatched += 1;
        println!(
            "  MISMATCH line {} — {}\n    {}",
            record.line,
            record.name,
            differing_fields(&both.mine, &both.theirs).join("\n    "),
        );
    }

    println!(
        "  recalculate  graded {graded} of {} builds, {} field value(s)",
        graded + mismatched,
        (graded + mismatched) * fields,
    );
    beyond.sort();
    beyond.dedup();
    if !beyond.is_empty() {
        println!(
            "  the record states none of these, so they sit at the serde default the TS had: {}",
            beyond.join(", ")
        );
    }

    if mismatched > 0 {
        failures.push(format!(
            "{}/recalculate: {mismatched} of {} recorded builds disagree",
            dataset.as_str(),
            records.len()
        ));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// One test per fork, for the reason `oracle.rs` gives: the window closes per fork.
#[test]
fn homecoming() {
    grade(DatasetId::Homecoming);
}

#[test]
fn rebirth() {
    grade(DatasetId::Rebirth);
}

#[test]
fn thunderspy() {
    grade(DatasetId::Thunderspy);
}

#[test]
fn brainstorm() {
    grade(DatasetId::Brainstorm);
}
