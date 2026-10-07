//! The movement-replay harness: the whole movement chain against `fixtures/movement/`.
//!
//! WHAT THIS IS, and how it differs from [`oracle`](../oracle.rs). The `fixtures/oracle/` corpus
//! grades one APPLIER on one POWER and records `{scale, table}` — the answer one level above the
//! number. This corpus records whole CALLS: 148 lines of
//! `{fn, args: {dataset, combatMode, powers}, value: {runSpeed, flySpeed, jumpSpeed, jumpHeight}}`,
//! where the value is the RESOLVED percentage each travel axis ended up with. So it grades what
//! the oracle corpus structurally cannot — the archetype-table resolution, the suppress-group
//! selection, the additive stacking of ungrouped sources, and the self-directed slow — and it is
//! the only frozen statement about the movement resolve that exists anywhere in this repository.
//! Nothing read these records before this file: the emitter promised a
//! `movement_gate` would hold the fixture to its manifest and that gate was never written, the
//! same never-existed shape item 22 found in the three named export tests.
//!
//! WHO PRODUCED THEM. `scripts/emit-movement-fixtures.ts`, deleted with all of `src/` in
//! `6caffef34` and readable at `6caffef34^`. It called the TS
//! `applyMovementResolveForGate(powers, 'blaster', combatMode)` — an ISOLATED resolve: a fresh
//! `GlobalBonuses`, the active-power gather, then `resolveMovementTotals`, no set bonuses, no
//! procs, no incarnates, no Alpha, no strength buffs and no slots. It recorded the NON-ZERO
//! deltas of the four travel fields only. Three consequences this reader has to honour:
//!   * A field the record omits is a ZERO delta, not an unrecorded one, so the key SETS are
//!     compared and a fifth axis appearing on our side is a failure.
//!   * Every host is unslotted, so both sides run at enhancement multiplier 1.0. A record that
//!     ever carries a slot is refused rather than replayed unslotted.
//!   * `blaster` at level 50, whatever archetype actually owns the powerset. The sample is
//!     synthetic on purpose and both sides read the same synthetic build.
//!
//! THE ONE STRUCTURAL DIFFERENCE, and why it is checked rather than assumed. The TS gate took an
//! already-assembled power list; we go through [`coh_math::gather::gather_active_powers`], which
//! also runs MODE SUPPRESSION — a Kheldian form can switch another power off. The emitter knew,
//! and excluded form/stance powers from the sample for exactly this reason (`isModeDisruptor`).
//! That exclusion is a property of the fixture rather than of the code, so this harness asserts
//! it: the gather must hand back every power the record names, in the recorded order, with an
//! empty error channel. Order matters because float addition is not associative and both sides
//! sum in list order.
//!
//! THE ARCHETYPE FORK, the one place the two readings are allowed to differ by construction. The
//! TS `baseAtoms` DROPPED an archetype-forked atom outright — those readers had no build to
//! resolve it against — while `apply::apply_active_power_bonuses` calls `Power::for_caster_class`
//! and keeps the arm this build gets (AT-FORK-1). So a forked movement atom would read as a
//! value bug that is really a fixture the record cannot express, which is the same hole
//! `oracle.rs` handles with `as_bag_view`. No power in the sample carries a fork today; the
//! count is PRINTED rather than assumed, so the day a re-emit picks one the run says which.
//!
//! WHAT THIS CANNOT REACH. The travel CEILING. `apply_active_power_bonuses` collects
//! `MovementCapContribution`s beside the buffs, and `movement::resolve_cap_bumps` and
//! `movement::project_axis` turn them into the speed the dashboard shows — but the TS gate
//! returned `GlobalBonuses`, which carries the four buff PERCENTAGES and no ceiling. So the cap
//! bumps, the per-class floor and the mph/feet conversion are collected here and graded by
//! nothing. `fixtures/totals/` (stage 6) is where those live, along with the suppress-group and
//! combat branches the next section shows are out of reach here too.
//!
//! WHAT THESE 353 NUMBERS ACTUALLY GRADE. All 148 calls matched on the first run, which is the
//! shape of a vacuous check, so each half was perturbed before any of it was believed. The
//! `TypedValue` perturbation stages 2-4 used cannot reach movement (it returns `MovementValue`),
//! so these are its own. Reddened records, out of 148:
//!
//! | Perturbation | Red | What that settles |
//! | --- | --- | --- |
//! | movement-map contribution `value` + 0.001 | **143** | the buff map, its enhancement multiplier and the summation are graded |
//! | the movement map's table lookup forced to `None` | **143** | the ARCHETYPE-TABLE RESOLUTION is graded — the level the `{scale, table}` oracle corpus structurally could not reach, and the whole case for this stage |
//! | self-directed slow + 0.001 | **18** | the `× -100` self-penalty write is graded — Rebirth 8, Thunderspy 10, and NOT AT ALL on Homecoming or Brainstorm, which sampled no such power |
//! | suppress-group winner `>` → `<` | **0** | ungraded |
//! | suppress grouping removed, every source additive | **0** | ungraded |
//! | the combat-suppression drop removed | **0** | ungraded |
//! | replayed as `tanker` instead of `blaster` | **5** | per-archetype VARIATION is graded on one power only (Thunderspy's Increase Density, 5 vs 4); the movement tables are otherwise flat across classes, which is a fact about the game data, not a gap here |
//!
//! So this corpus grades the value, the table and the additive stacking, and it does NOT grade the
//! suppress-group selection or the combat drop — the two branches `resolve_movement_totals` exists
//! for. That is not bad luck in a 14-of-43 sample. It is ONE of the emitter's own two filters, and
//! the reason is structural: a travel power's suppress group and its mode metadata are carried by
//! the SAME powers, because `modesDisallowed: ['Disable_Travel', 'Disable_FlyToggles', …]` is how
//! the game switches travel off. So `isModeDisruptor`, which the emitter applied to keep mode
//! suppression out of a resolve that took its powers as-is, excludes the entire suppress-group
//! population as a side effect.
//!
//! Measured, not reasoned: of the 56 sampled (powerset, internal name) pairs, all 109 recorded
//! `movementBuffValue` entries carry `stackKey: null` and `suppressible: false`. Of the powers in
//! the wider population that DO carry one — 17 keyed and 11 suppressible on Homecoming, 0 and 10
//! on Rebirth, 0 and 11 on Thunderspy, 17 and 11 on Brainstorm — every single one is excluded by
//! `isModeDisruptor`, on all four forks, with no exception: Super Speed, Fly, Mighty Leap, Jetpack,
//! Speed of Sound, Freerunning, Long Jump, Mystic Flight, Group Fly, Combat Jumping, Combat
//! Flight, Turbo Boost and the Concealment pool's Invisibility all carry `modesDisallowed`, as do
//! the four Peacebringer `luminous-aura` flight powers and the Kheldian and Primalist forms. The
//! emitter's header claims the sample covers "any shared-stackKey suppress group"; it covers none.
//! Rebirth and Thunderspy have no keyed movement entry ANYWHERE, so on those two forks there was
//! nothing to miss.
//!
//! Both vacuous halves are held rather than noted. [`SUPPRESS_CENSUS`] asserts the two counts
//! measured above over the contributions our own resolve receives, and
//! [`grade_combat_vacuity`] holds the fixture to `manifest.json`'s stamped `suppressible` count —
//! the tripwire the emitter asked for and never got ("the day one lands the gate goes red instead
//! of staying quietly vacuous", FIXTURE-2's carried residual): while the count is 0, every
//! `combatMode: true` record must restate its twin, on both sides.
//!
//! EXACT f64, no tolerance. Both sides multiply the same JSON literal out of the same bundle, so
//! a recorded `13.650000095367432` is reachable exactly; an epsilon here would hide precisely the
//! ordering and accumulation bugs the corpus is being spent to find.
//!
//! Run: `cargo test -p coh_math --test movement_replay -- --nocapture`

use coh_data::{CharacterState, DatasetId, PowerDatabase, SelectedPower};
use coh_math::GlobalBonuses;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The archetype and level every record was emitted under (`emit-movement-fixtures.ts`'s
/// `ARCHETYPE` and the `level: 50` its Rust-identity twin carries).
const ARCHETYPE: &str = "blaster";
const LEVEL: i32 = 50;

/// The four fields the emitter read out of the returned `GlobalBonuses`, in its own order.
const AXES: [&str; 4] = ["runSpeed", "flySpeed", "jumpSpeed", "jumpHeight"];

/// The only `fn` this file knows how to replay. A record naming another one is refused rather
/// than skipped: a second function in the file would otherwise be silently ungraded.
const RECORDED_FN: &str = "applyMovementResolveForGate";

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

/// One power as a record names it. The pair is the address: `internalName` collides across
/// archetypes (`Build_Up` appears ×64), so neither half addresses a power alone.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PowerRef {
    powerset: String,
    internal_name: String,
}

/// One recorded call: the arguments to replay and the answer to replay them against.
#[derive(Debug)]
struct Record {
    /// 1-based line in `contributions.jsonl`, so a failure names the line to read.
    line: usize,
    dataset: String,
    combat_mode: bool,
    powers: Vec<PowerRef>,
    /// The non-zero travel deltas, axis → percentage. An absent axis is a zero delta.
    value: BTreeMap<String, f64>,
}

/// Panic on any field this reader does not know, in either direction.
///
/// The `allowed` half is the `oracle.rs` rule: a key the emitter wrote and this reader drops
/// would pass by omission. The `required` half is the other direction, which matters more here
/// than it does there — these records carry ARGUMENTS, and an argument silently defaulted is a
/// replay of a different call than the one that was recorded.
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

fn parse_record(line_no: usize, line: &str) -> Record {
    let where_ = format!("fixtures/movement/contributions.jsonl:{line_no}");
    let record: Value =
        serde_json::from_str(line).unwrap_or_else(|e| panic!("{where_}: not JSON: {e}"));
    let object = record
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: not an object"));
    exact_fields(object, &["fn", "args", "value"], &where_);
    let name = record["fn"]
        .as_str()
        .unwrap_or_else(|| panic!("{where_}: fn is not a string"));
    assert_eq!(
        name, RECORDED_FN,
        "{where_}: records a call to {name:?}, which this harness does not replay"
    );

    let args = record["args"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: args is not an object"));
    exact_fields(args, &["dataset", "combatMode", "powers"], &where_);
    let dataset = args["dataset"]
        .as_str()
        .unwrap_or_else(|| panic!("{where_}: dataset is not a string"))
        .to_string();
    let combat_mode = args["combatMode"]
        .as_bool()
        .unwrap_or_else(|| panic!("{where_}: combatMode is not a bool"));

    let powers = args["powers"]
        .as_array()
        .unwrap_or_else(|| panic!("{where_}: powers is not an array"))
        .iter()
        .map(|p| {
            let p = p
                .as_object()
                .unwrap_or_else(|| panic!("{where_}: a power is not an object"));
            exact_fields(
                p,
                &["powerset", "internalName", "isActive", "level", "slots"],
                &where_,
            );
            // Both of these are constant across all 148 records, and both change what gets
            // replayed if they ever stop being. `isActive: false` would contribute nothing but
            // for an auto power; a slot would make our side unenhanced where the record is not.
            assert_eq!(
                p["isActive"].as_bool(),
                Some(true),
                "{where_}: a power is recorded inactive — this replay switches every host on"
            );
            assert_eq!(
                p["level"].as_i64(),
                Some(i64::from(LEVEL)),
                "{where_}: a power is recorded at a level other than {LEVEL}"
            );
            assert_eq!(
                p["slots"].as_array().map(Vec::len),
                Some(0),
                "{where_}: a power is recorded with enhancement slots — the emitter states every \
                 host is unslotted so both sides run at multiplier 1.0, and replaying a slotted \
                 host unslotted would compare two different calls"
            );
            PowerRef {
                powerset: p["powerset"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{where_}: powerset is not a string"))
                    .to_string(),
                internal_name: p["internalName"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{where_}: internalName is not a string"))
                    .to_string(),
            }
        })
        .collect();

    let value = record["value"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: value is not an object"))
        .iter()
        .map(|(axis, v)| {
            assert!(
                AXES.contains(&axis.as_str()),
                "{where_}: value states axis {axis:?}, which is not one of {AXES:?}"
            );
            let n = v
                .as_f64()
                .unwrap_or_else(|| panic!("{where_}: {axis} is not a number"));
            assert!(
                n != 0.0,
                "{where_}: {axis} is recorded as {n} — the emitter writes only NON-ZERO deltas, \
                 so a recorded zero means the omission rule this reader relies on has changed"
            );
            (axis.clone(), n)
        })
        .collect();

    Record {
        line: line_no,
        dataset,
        combat_mode,
        powers,
        value,
    }
}

fn read_records() -> Vec<Record> {
    let path = repo().join("fixtures/movement/contributions.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| parse_record(i + 1, line))
        .collect()
}

/// `fixtures/movement/manifest.json`'s per-fork census: how many movement powers the emitter
/// discovered, and how many of them carry a combat-suppressible entry.
#[derive(Debug, Clone, Copy)]
struct Population {
    powers: u64,
    suppressible: u64,
}

fn read_manifest(records: usize) -> BTreeMap<String, Population> {
    let path = repo().join("fixtures/movement/manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let manifest: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let lines = manifest["contributions"]["lines"]
        .as_u64()
        .unwrap_or_else(|| panic!("{path:?}: no contributions.lines"));
    assert_eq!(
        lines as usize, records,
        "{path:?} states {lines} recorded lines and contributions.jsonl holds {records} — the \
         manifest and the fixture were not written by the same run"
    );
    manifest["population"]
        .as_object()
        .unwrap_or_else(|| panic!("{path:?}: no population"))
        .iter()
        .map(|(dataset, p)| {
            (
                dataset.clone(),
                Population {
                    powers: p["powers"]
                        .as_u64()
                        .unwrap_or_else(|| panic!("{dataset}: powers")),
                    suppressible: p["suppressible"]
                        .as_u64()
                        .unwrap_or_else(|| panic!("{dataset}: suppressible")),
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------- our side

/// What one replay produced besides its answer.
struct Replayed {
    /// The non-zero travel deltas, in the same shape the record states them.
    value: BTreeMap<String, f64>,
    /// Powers whose atoms fork by archetype. The record cannot express a fork (see the header),
    /// so these are named rather than silently compared.
    forked: Vec<String>,
    /// Anything the calc met and could not derive — the fail-loud channel. A recorded answer
    /// compared against a total with a hole in it is not a graded answer.
    errors: Vec<String>,
    /// Movement contributions this call resolved, and how many of them carry a suppress group or
    /// a combat-suppress flag. Counted on OUR side rather than read from the manifest, because
    /// these two counts are what decides whether the resolve's grouping and combat branches are
    /// graded at all — see [`SUPPRESS_CENSUS`].
    contributions: usize,
    keyed: usize,
    suppressible: usize,
}

/// Replay one record: build the state the arguments describe, run the gather, the active-power
/// pass and the movement resolve, and read the four travel fields back out.
///
/// The three vectors the TS gate had no equivalent for — `movement_cap_contribs`,
/// `res_self_debuffs`, `power_breakdown` — are collected and dropped, as the header states.
fn replay(db: &PowerDatabase, dataset: DatasetId, record: &Record) -> Replayed {
    let mut state = CharacterState::empty(dataset);
    state.level = LEVEL as u8;
    state.archetype.id = Some(ARCHETYPE.to_string());
    state.combat.in_combat = record.combat_mode;
    // Every host in ONE bucket, in the recorded order: `all_selected` walks primary first and
    // each pick carries its own owning set, so the powerset ids need not belong to the primary.
    // Order is load-bearing — both sides sum in list order over f64.
    state.primary.powers = record
        .powers
        .iter()
        .map(|p| {
            let mut pick = SelectedPower::picked(&p.internal_name, &p.powerset, 1);
            pick.is_active = true;
            // `picked` hands out one empty base slot; the record says `slots: []`.
            pick.slots = Vec::new();
            pick
        })
        .collect();

    let gathered = coh_math::gather::gather_active_powers(&state, db);
    let mut errors: Vec<String> = gathered
        .unresolved
        .iter()
        .map(|e| format!("gather: {} — {}", e.context, e.detail))
        .collect();
    // The gather must be the identity on this corpus. It is not on an arbitrary build (mode
    // suppression), and the emitter excluded the powers that would make it differ — a fixture
    // property, so it is checked and not trusted.
    let got: Vec<PowerRef> = gathered
        .powers
        .iter()
        .map(|p| PowerRef {
            powerset: p.power_set.to_string(),
            internal_name: p.def.ident().to_string(),
        })
        .collect();
    if got != record.powers {
        errors.push(format!(
            "the gather did not hand back the recorded power list: recorded {:?}, gathered {:?}",
            record
                .powers
                .iter()
                .map(|p| format!("{}/{}", p.powerset, p.internal_name))
                .collect::<Vec<_>>(),
            got.iter()
                .map(|p| format!("{}/{}", p.powerset, p.internal_name))
                .collect::<Vec<_>>(),
        ));
    }
    let forked: Vec<String> = gathered
        .powers
        .iter()
        .filter(|p| p.def.atoms.iter().any(|a| a.caster_archetypes.is_some()))
        .map(|p| format!("{}/{}", p.power_set, p.def.ident()))
        .collect();

    let mut g = GlobalBonuses::default();
    let before = GlobalBonuses::default();
    let mut movement = Vec::new();
    let mut caps = Vec::new();
    coh_math::apply::apply_active_power_bonuses(
        &gathered.powers,
        &mut g,
        ARCHETYPE,
        LEVEL,
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
    coh_math::movement::resolve_movement_totals(&movement, &mut g, record.combat_mode);

    errors.extend(
        g.errors
            .iter()
            .map(|e| format!("calc: {} — {}", e.context, e.detail)),
    );
    let keyed = movement.iter().filter(|c| c.stack_key.is_some()).count();
    let suppressible = movement.iter().filter(|c| c.suppressible).count();
    let value = AXES
        .iter()
        .filter_map(|axis| {
            let after = g
                .get(axis)
                .unwrap_or_else(|| panic!("GlobalBonuses has no {axis}"));
            let empty = before.get(axis).unwrap_or(0.0);
            let delta = after - empty;
            (delta != 0.0).then(|| ((*axis).to_string(), delta))
        })
        .collect();
    Replayed {
        value,
        forked,
        errors,
        contributions: movement.len(),
        keyed,
        suppressible,
    }
}

// ---------------------------------------------------------------- reporting

/// Axis values in the emitter's own field order, so two sides of a mismatch line up by eye.
fn describe(value: &BTreeMap<String, f64>) -> String {
    if value.is_empty() {
        return "{}".to_string();
    }
    let mut parts = Vec::new();
    for axis in AXES {
        if let Some(v) = value.get(axis) {
            parts.push(format!("{axis}={v}"));
        }
    }
    format!("{{{}}}", parts.join(", "))
}

/// Exact, with NaN equal to NaN. NaN is not reachable through a healthy table lookup — it is what
/// a missing one leaves behind — so treating it as equal here would turn an underivable total
/// into a pass. It is reported as a mismatch instead, which is why this is spelled out rather
/// than left to `f64`'s answer.
fn same_value(a: &BTreeMap<String, f64>, b: &BTreeMap<String, f64>) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|((ka, va), (kb, vb))| ka == kb && va == vb)
}

// ---------------------------------------------------------------- the run

/// What the replay resolved that the resolve's two conditional branches need, measured over every
/// record of a fork: contributions carrying a suppress group, and contributions carrying the
/// combat-suppress flag. Both are ZERO on all four forks, which is why `resolve_axis`'s
/// group-winner selection and its combat drop are graded by nothing here (see the header's
/// perturbation table).
///
/// Asserted rather than merely printed, and deliberately so. The numbers cannot be FIXED from
/// this side — they are a property of which powers the deleted emitter's discovery walk could
/// see — but they can silently stop being true, and a corpus that starts grading the grouping is
/// the one event that would change what this whole file is worth. The emitter asked for exactly
/// this ("the day one lands the gate goes red instead of staying quietly vacuous"); it asked for
/// it against `manifest.json`'s stamped count, and this is the stronger form, because it reads
/// the contributions our own resolve actually received rather than a number written beside them.
/// When it reddens: re-read the header's table, update these, and say which branch became live.
const SUPPRESS_CENSUS: (usize, usize) = (0, 0);

/// The tripwire the emitter asked for and never got: while the manifest says no movement atom in
/// this fork is combat-suppressible, every `combatMode: true` record must restate its
/// `combatMode: false` twin. Both sides are checked — theirs says the corpus is vacuous on this
/// axis, ours says our combat handling agrees that it is.
fn grade_combat_vacuity(
    records: &[&Record],
    ours: &[BTreeMap<String, f64>],
    population: Population,
    failures: &mut Vec<String>,
) {
    let mut pairs = 0usize;
    for (i, out) in records.iter().enumerate() {
        if out.combat_mode {
            continue;
        }
        let Some(j) = records
            .iter()
            .position(|r| r.combat_mode && r.powers == out.powers)
        else {
            continue;
        };
        pairs += 1;
        let recorded_same = same_value(&out.value, &records[j].value);
        let ours_same = same_value(&ours[i], &ours[j]);
        if population.suppressible == 0 && !(recorded_same && ours_same) {
            failures.push(format!(
                "lines {} / {}: the manifest states 0 suppressible movement powers on this fork, \
                 so combat mode cannot change a total — but {} changed. recorded {} vs {}, ours \
                 {} vs {}",
                out.line,
                records[j].line,
                if recorded_same { "ours" } else { "the record" },
                describe(&out.value),
                describe(&records[j].value),
                describe(&ours[i]),
                describe(&ours[j]),
            ));
        }
    }
    println!(
        "  combat half: {pairs} twin pair(s), manifest suppressible {} of {} movement powers{}",
        population.suppressible,
        population.powers,
        if population.suppressible == 0 {
            " — every combatMode:true record restates its twin, so the combat branch is GRADED BY \
             NOTHING here and is held to that"
        } else {
            " — suppressible powers exist on this fork, so the twins may legitimately differ"
        }
    );
}

fn grade(dataset: DatasetId) {
    let all = read_records();
    let manifest = read_manifest(all.len());
    let records: Vec<&Record> = all
        .iter()
        .filter(|r| r.dataset == dataset.as_str())
        .collect();
    assert!(
        !records.is_empty(),
        "fixtures/movement/contributions.jsonl holds no records for {}",
        dataset.as_str()
    );
    let population = *manifest
        .get(dataset.as_str())
        .unwrap_or_else(|| panic!("manifest.json has no population for {}", dataset.as_str()));

    let db = load(dataset);
    let mut failures: Vec<String> = Vec::new();
    let mut ours: Vec<BTreeMap<String, f64>> = Vec::new();
    let (mut graded, mut mismatched, mut values) = (0usize, 0usize, 0usize);
    let (mut contributions, mut keyed, mut suppressible) = (0usize, 0usize, 0usize);
    let mut forked: Vec<String> = Vec::new();

    println!(
        "\n=== {} — {} recorded calls over {} discovered movement powers",
        dataset.as_str(),
        records.len(),
        population.powers,
    );

    for record in &records {
        let replayed = replay(&db, dataset, record);
        for key in &replayed.forked {
            if !forked.contains(key) {
                forked.push(key.clone());
            }
        }
        for e in &replayed.errors {
            failures.push(format!("line {}: {e}", record.line));
        }
        contributions += replayed.contributions;
        keyed += replayed.keyed;
        suppressible += replayed.suppressible;
        // Every axis the record states is one frozen number spent; an axis only WE state is one
        // the record says is zero, and is counted too because it is graded either way.
        values += record
            .value
            .keys()
            .chain(replayed.value.keys())
            .collect::<std::collections::BTreeSet<_>>()
            .len();
        if same_value(&replayed.value, &record.value) {
            graded += 1;
        } else {
            mismatched += 1;
            // EVERY mismatch prints, unlike `oracle.rs`'s 25-per-applier cap. That cap is there
            // because a systemic failure over 3,887 powers is a wall of output; a fork here is 37
            // calls, so the whole of it fits on a screen, and truncating costs a re-run of a
            // corpus that is spent once. Measured rather than assumed: with the movement push
            // perturbed, Thunderspy reported 32 mismatches and printed 25, and the 7 it swallowed
            // were exactly the ones that identified the second code path.
            {
                println!(
                    "  MISMATCH line {} ({}, combat {})\n    powers:   {}\n    recorded: {}\n    ours:     {}",
                    record.line,
                    dataset.as_str(),
                    record.combat_mode,
                    record
                        .powers
                        .iter()
                        .map(|p| format!("{}/{}", p.powerset, p.internal_name))
                        .collect::<Vec<_>>()
                        .join(" + "),
                    describe(&record.value),
                    describe(&replayed.value),
                );
            }
        }
        ours.push(replayed.value);
    }

    println!(
        "  {:<30} graded {graded} of {} calls, {values} axis value(s) compared",
        RECORDED_FN,
        graded + mismatched,
    );
    if forked.is_empty() {
        println!("  0 archetype-forked hosts — the record's one blind spot is empty on this fork");
    } else {
        println!(
            "  {} archetype-forked host(s), which the record cannot express (see the header): {}",
            forked.len(),
            forked.join(", ")
        );
    }
    println!(
        "  {contributions} movement contribution(s) resolved: {keyed} keyed, {suppressible} \
         combat-suppressible"
    );
    if (keyed, suppressible) != SUPPRESS_CENSUS {
        failures.push(format!(
            "{}: {keyed} keyed / {suppressible} suppressible movement contribution(s), and this \
             harness was written against {:?}. That is not a calc bug — it means the corpus has \
             started (or stopped) grading `resolve_axis`'s suppress-group selection or its combat \
             drop, which changes what these 37 records are worth. Re-read this file's header \
             table, re-measure, and update SUPPRESS_CENSUS.",
            dataset.as_str(),
            SUPPRESS_CENSUS,
        ));
    }
    grade_combat_vacuity(&records, &ours, population, &mut failures);

    if mismatched > 0 {
        failures.push(format!(
            "{}/{RECORDED_FN}: {mismatched} of {} recorded calls disagree",
            dataset.as_str(),
            records.len()
        ));
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// One test per fork, for the reason `oracle.rs` gives: the window closes per fork, so when a
// server patches, that fork's records stale and the other three keep their value. A single test
// would make the first patch red the whole gate.
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
