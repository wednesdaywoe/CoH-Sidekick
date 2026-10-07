//! The proc-replay harness: the three isolated proc passes against `fixtures/procs/`.
//!
//! WHAT THIS IS. 184 whole-call records —
//! `{fn: "applyProcBonusesForGate", args: {dataset, powers}, value}` — where `value` is the
//! NON-ZERO delta of EVERY numeric `GlobalBonuses` field the proc passes moved, not a fixed
//! handful. The second of the three replay chains, and the sibling of
//! [`movement_replay`](../movement_replay.rs); `oracle.rs` grades single appliers against
//! `{scale, table}` records and reaches none of this.
//!
//! WHO PRODUCED THEM. `scripts/emit-proc-fixtures.ts`, deleted with all of `src/` in `6caffef34`
//! and readable at `6caffef34^`. It called the TS `applyProcBonusesForGate(build)`, which runs
//! `applyProcBonuses` (always-on globals + Proc120s + PPM), `applyVariableProcBonuses`,
//! `resolveStealthRadius` and `applyBuildUpProcBonuses` into a fresh `GlobalBonuses` and returns
//! it. The Rust equivalent is [`replay`]: `procs::apply_procs` (whose body is the first two),
//! then `stealth::resolve_stealth_radius`, then `procs::apply_build_up_procs` — the same four in
//! the same order `lib.rs::recalculate` runs them in, minus every other pass.
//!
//! THE ARGUMENTS NAME NO ARCHETYPE, which is a fact about the recorded call and not an omission
//! to paper over. The TS gate took a `Build` with one `primary.powers` list and no archetype id,
//! so the one archetype-dependent proc site — `apply_variable_procs`'s scale-table resolution —
//! resolved against nothing. This replay leaves `state.archetype.id` at `None`, which is that
//! same reading (`db.at_tables.get_table_value("", …)` misses and the magnitude stays raw). Had
//! the TS silently defaulted to an archetype instead, these records would disagree and say so.
//!
//! THE SLOTS DESERIALIZE DIRECTLY. The emitter's `toRustSlot` was written against
//! `coh_data::Enhancement`'s serde shape (`set_id`, `set_name`, `piece_num`, `is_proc`,
//! `is_unique`, `type: "io-set"`), so each recorded slot is parsed by `serde_json` into an
//! `Enhancement` rather than rebuilt field by field. That is the strictest available reading: a
//! field the shape has since lost, or a type that has changed, fails at parse instead of being
//! quietly defaulted into a different call.
//!
//! WHY THE COMPARISON IS WHOLE-STRUCT, not field-by-field. `GlobalBonuses` has ~150 fields, the
//! records name 34 of them across the corpus, and any of the rest could hold a value we produce
//! and the record does not. Naming them all would mean hand-keeping a second copy of
//! `GlobalBonuses::get`'s match arms, which is the kind of list that drifts silently. So the
//! recorded deltas are ADDED into a fresh `GlobalBonuses` through
//! [`GlobalBonuses::add_by_camel_name`] — the function the calc itself routes through — and the
//! two structs are compared with `PartialEq`. Every field is then covered in both directions with
//! no list to keep, an unroutable recorded field name is a hard failure rather than a silent
//! skip, and the mismatch report names the differing fields by diffing the two serializations.
//! The field names in a failure read as Rust snake_case, because that is what the struct
//! serializes as; `runSpeed` appears as `run_speed`.
//!
//! 87 OF THE 184 RECORDS HAVE AN EMPTY `value`, which is a claim and not a hole: a proc the TS
//! scored at nothing must score at nothing here too, and a routing bug that mints a bonus where
//! the oracle minted none lands on exactly those records. They are counted separately so the
//! graded total is never read as 184 non-trivial answers.
//!
//! WHAT THE RECORDS REACH. Every one agreed on the run after two harness faults were fixed (below),
//! which is the shape of a vacuous check, so each part was perturbed before any of it was believed.
//! Reddened records, out of 184:
//!
//! | Perturbation | Red | What that settles |
//! | --- | --- | --- |
//! | every routed always-on effect value + 0.001 | **71** | the always-on / Proc120s router and its Rule-of-5 bookkeeping path are graded |
//! | the Build-Up pass not run | **24** | the PPM × geometry × damage/toHit chain is graded |
//! | the all-six mez expansion + 0.001 | **19** | [`EXPANDED_SCALAR`] is checked by VALUE, not waved through by shape |
//! | the stealth PvE / PvP radii swapped | **13** | the split the beta's duplicated-magnitude guard exists for is graded |
//! | the PPM area denominator forced to 1.0 | **8** | the AoE proc-chance penalty is graded |
//! | `ProcMainTargetOnly` ignored | **8** | the flag that cancels that penalty is graded — the host PAIR earns its place |
//! | the variable-proc pass not run | **5** | HP-scaling / self-stacking procs are graded |
//! | the toggle gate ignored (a Proc120s applies with its host off) | **4** | the Global-vs-Proc120s distinction is graded |
//! | the archetype set to `blaster` instead of left unset | **3** | the recorded call really passed NO archetype — see above; three records say so |
//! | the auto/toggle PPM-per-minute conversion + 0.001 | **2** | graded, but thinly |
//! | the Rule-of-5 cap raised from 5 to 6 | **0** | UNGRADED |
//!
//! THE RULE OF 5 IS NOT GRADED, and the fixture's own header says it is: "the proc Rule of 5 (one
//! global in 6 separate powers → cap 5)". No such record exists. The emitter's case is guarded by
//! `alwaysOn.find(hasTrackedGlobal)` and emitted nothing on any fork — the corpus is 180
//! single-host records plus 4 five-host ones, and across all 184 no proc piece is slotted into
//! more than ONE host, so a per-(stat, value) bucket never reaches depth 2 let alone 5. Measured
//! twice, from opposite directions: the deepest same-piece repeat in the fixture is 1, and raising
//! the cap to 6 changes no answer. [`RULE_OF_FIVE_DEPTH`] holds that, so the day a re-emit
//! supplies the record the claim is revisited instead of staying quietly false.
//!
//! TWO HARNESS FAULTS THIS CORPUS FOUND, both worth naming because each would have read as a calc
//! bug. `stealthRadiusPvE` / `stealthRadiusPvP` have no `add_by_camel_name` arm — they are ASSIGNED
//! by the resolve, not accumulated — so 13 records had to be built through [`ASSIGN_ONLY`] rather
//! than the router. And `mezResist` is answered here by an expansion into the per-type fields
//! rather than by writing the scalar: a declared divergence `GlobalBonuses::mez_resist`'s own doc
//! states and names two gates for, of which this is one and neither existed until now. See
//! [`EXPANDED_SCALAR`].
//!
//! THE AREA-FACTOR PAIR, the one coverage floor the emitter asked for. A proc's chance in an AoE
//! scales with the host's radius, and `ProcMainTargetOnly` cancels exactly that — so the emitter
//! slotted the same Build-Up procs into two click hosts differing only in that flag, and stamped
//! the flagged host's name per fork into `manifest.json`. A flagged host alone would pass against
//! an engine that never penalises an AoE at all, so the PAIR is the test and its presence is
//! [`grade_main_target_floor`]: the named host must still appear in that fork's records.
//!
//! EXACT f64, no tolerance, for the reason `movement_replay.rs` gives.
//!
//! Run: `cargo test -p coh_math --test proc_replay -- --nocapture`

use coh_data::{CharacterState, DatasetId, Enhancement, PowerDatabase, SelectedPower};
use coh_math::totals::TypeRoute;
use coh_math::GlobalBonuses;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The level every host was recorded at (`emit-proc-fixtures.ts`'s Rust-identity twin).
const LEVEL: u8 = 50;

/// The deepest any one proc piece is slotted across the hosts of a single record — 1, meaning no
/// piece is ever in two hosts at once, on all four forks.
///
/// That number is why the proc Rule of 5 is graded by nothing (see the header): the cap buckets
/// per (stat, value) and bites at the sixth hit, and nothing here reaches the second. Asserted
/// rather than printed for the reason [`movement_replay`](../movement_replay.rs)'s
/// `SUPPRESS_CENSUS` gives: it cannot be fixed from this side, but it can silently stop being
/// true, and a corpus that starts grading the cap is exactly the event that changes what this
/// file is worth. When it reddens: re-measure the perturbation table, update this, and say so.
const RULE_OF_FIVE_DEPTH: usize = 1;

/// The only `fn` this file replays; another one in the file is refused, not skipped.
const RECORDED_FN: &str = "applyProcBonusesForGate";

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

/// One recorded host power: its address, its toggle state, and the pieces slotted in it.
#[derive(Debug)]
struct HostPower {
    powerset: String,
    internal_name: String,
    is_active: bool,
    slots: Vec<Option<Enhancement>>,
}

/// One recorded call.
#[derive(Debug)]
struct Record {
    /// 1-based line in `contributions.jsonl`, so a failure names the line to read.
    line: usize,
    dataset: String,
    powers: Vec<HostPower>,
    /// The non-zero field deltas, beta (camelCase) name → value. Empty is a real answer.
    value: BTreeMap<String, f64>,
}

/// Panic on any field this reader does not know, in either direction. See the twin in
/// `movement_replay.rs`: an argument silently defaulted is a replay of a different call.
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
    let where_ = format!("fixtures/procs/contributions.jsonl:{line_no}");
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
    exact_fields(args, &["dataset", "powers"], &where_);
    let dataset = args["dataset"]
        .as_str()
        .unwrap_or_else(|| panic!("{where_}: dataset is not a string"))
        .to_string();

    let powers = args["powers"]
        .as_array()
        .unwrap_or_else(|| panic!("{where_}: powers is not an array"))
        .iter()
        .map(|p| {
            let object = p
                .as_object()
                .unwrap_or_else(|| panic!("{where_}: a power is not an object"));
            exact_fields(
                object,
                &["powerset", "internalName", "isActive", "level", "slots"],
                &where_,
            );
            assert_eq!(
                object["level"].as_u64(),
                Some(u64::from(LEVEL)),
                "{where_}: a host is recorded at a level other than {LEVEL}"
            );
            // Parsed into the real `Enhancement`, not a hand-built stand-in. The emitter wrote
            // this shape for exactly this reader.
            let slots = object["slots"]
                .as_array()
                .unwrap_or_else(|| panic!("{where_}: slots is not an array"))
                .iter()
                .map(|s| {
                    Some(
                        serde_json::from_value::<Enhancement>(s.clone()).unwrap_or_else(|e| {
                            panic!("{where_}: a recorded slot is not a coh_data::Enhancement: {e}")
                        }),
                    )
                })
                .collect();
            HostPower {
                powerset: object["powerset"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{where_}: powerset is not a string"))
                    .to_string(),
                internal_name: object["internalName"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{where_}: internalName is not a string"))
                    .to_string(),
                is_active: object["isActive"]
                    .as_bool()
                    .unwrap_or_else(|| panic!("{where_}: isActive is not a bool")),
                slots,
            }
        })
        .collect();

    let value = record["value"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: value is not an object"))
        .iter()
        .map(|(field, v)| {
            let n = v
                .as_f64()
                .unwrap_or_else(|| panic!("{where_}: {field} is not a number"));
            assert!(
                n != 0.0,
                "{where_}: {field} is recorded as {n} — the emitter writes only NON-ZERO deltas, \
                 so a recorded zero means the omission rule this reader relies on has changed"
            );
            (field.clone(), n)
        })
        .collect();

    Record {
        line: line_no,
        dataset,
        powers,
        value,
    }
}

fn read_records() -> Vec<Record> {
    let path = repo().join("fixtures/procs/contributions.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| parse_record(i + 1, line))
        .collect()
}

/// `fixtures/procs/manifest.json`: the line count, and the per-fork host that carries
/// `ProcMainTargetOnly` — half of the area-factor pair (see the header).
fn read_manifest(records: usize) -> BTreeMap<String, Option<String>> {
    let path = repo().join("fixtures/procs/manifest.json");
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
    manifest["mainTargetHost"]
        .as_object()
        .unwrap_or_else(|| panic!("{path:?}: no mainTargetHost"))
        .iter()
        .map(|(dataset, host)| (dataset.clone(), host.as_str().map(str::to_string)))
        .collect()
}

// ---------------------------------------------------------------- our side

/// The two fields this crate ASSIGNS rather than accumulates, which is why
/// [`GlobalBonuses::add_by_camel_name`] has no arm for either: a stealth radius is the output of
/// [`coh_math::stealth::resolve_stealth_radius`] over every contribution at once (grouped max
/// plus additive sum), so adding into the field would double whatever the resolve already
/// decided. Building an EXPECTED total from a recorded delta starts from zero, where assignment
/// and addition coincide, so these two are assigned here. The list is closed: any OTHER name
/// `add_by_camel_name` rejects stays a hard failure.
const ASSIGN_ONLY: [&str; 2] = ["stealthRadiusPvE", "stealthRadiusPvP"];

fn assign_by_camel_name(g: &mut GlobalBonuses, field: &str, value: f64) -> bool {
    match field {
        "stealthRadiusPvE" => g.stealth_radius_pve = value,
        "stealthRadiusPvP" => g.stealth_radius_pvp = value,
        _ => return false,
    }
    true
}

/// The beta field name this crate answers by EXPANDING rather than by writing, and the prefix it
/// expands into.
///
/// `GlobalBonuses::mez_resist`'s own doc states the divergence and names the gate that should
/// declare it: "the beta's SCALAR mez-resist-all (`mezResist`), which the beta writes and never
/// reads. NOTHING in this calc writes it: both paths that claim an all-types mez resistance (set
/// bonuses and always-on procs) expand into the per-type `mez_resist_*` fields above, from the
/// types the export states (MEZRES-1) … the two gates declare the expansion as their one
/// divergence." This is one of those two gates, and until now neither existed.
///
/// Declared by SHAPE and re-checked by VALUE, the same standing `oracle.rs` gives its one movement
/// divergence. The expansion is not allowlisted: every per-type field we wrote must equal the
/// recorded scalar exactly, at least one must be non-zero (an expansion into nothing would
/// otherwise turn a dropped bonus into a pass), our own scalar must be untouched, and every other
/// field in the struct is still compared exactly. Which types the expansion covers is left to the
/// data, because that is where it comes from — today six of them (hold, stun, immobilize, sleep,
/// confuse, fear), and a seventh appearing is not a failure while it carries the same value.
const EXPANDED_SCALAR: (&str, &str) = ("mezResist", "mez_resist_");

/// Every per-type mez-resistance field we wrote, as (name, value), read back out of the
/// serialization so the set comes from the struct rather than from a list kept beside it.
fn mez_resist_expansion(g: &GlobalBonuses) -> Vec<(String, f64)> {
    let Value::Object(map) = serde_json::to_value(g).expect("GlobalBonuses serializes") else {
        panic!("GlobalBonuses did not serialize as an object");
    };
    map.into_iter()
        .filter(|(field, _)| field.starts_with(EXPANDED_SCALAR.1))
        .filter_map(|(field, v)| v.as_f64().filter(|n| *n != 0.0).map(|n| (field, n)))
        .collect()
}

/// A recorded answer as a `GlobalBonuses`: a fresh one with each delta added through the routing
/// the calc itself uses. A name no arm claims is a failure and not a skip — see the header.
///
/// `ours` is read for one purpose only: the [`EXPANDED_SCALAR`] divergence, where the recorded
/// scalar has to be matched against the per-type fields this crate writes instead. Every value
/// copied across is asserted equal to the recorded one first, so this cannot launder a wrong
/// number into an expected total.
fn expected_of(record: &Record, ours: &GlobalBonuses) -> Result<GlobalBonuses, Vec<String>> {
    let mut g = GlobalBonuses::default();
    let mut bad = Vec::new();
    for (field, value) in &record.value {
        if field == EXPANDED_SCALAR.0 {
            let expansion = mez_resist_expansion(ours);
            if expansion.is_empty() {
                bad.push(format!(
                    "the record states {field} {value} and this crate wrote no                      {}* field at all. The declared divergence is an EXPANSION; an expansion                      into nothing is a dropped bonus.",
                    EXPANDED_SCALAR.1
                ));
                continue;
            }
            let wrong: Vec<String> = expansion
                .iter()
                .filter(|(_, v)| v != value)
                .map(|(f, v)| format!("{f}={v}"))
                .collect();
            if !wrong.is_empty() {
                bad.push(format!(
                    "the record states {field} {value} and this crate's expansion disagrees on                      {}. The expansion is declared by shape, never by value.",
                    wrong.join(", ")
                ));
                continue;
            }
            for (f, v) in expansion {
                assert!(
                    assign_mez_resist_type(&mut g, &f, v),
                    "{f}: no setter for a mez-resistance type the struct serializes"
                );
            }
            continue;
        }
        match g.add_by_camel_name(field, *value) {
            TypeRoute::Routed => {}
            TypeRoute::Unspent(why) => bad.push(format!(
                "recorded field {field:?} is deliberately not totalled by this crate ({why}), so \
                 the record states a number nothing here can hold"
            )),
            TypeRoute::Unknown if ASSIGN_ONLY.contains(&field.as_str()) => {
                assign_by_camel_name(&mut g, field, *value);
            }
            TypeRoute::Unknown => bad.push(format!(
                "recorded field {field:?} reaches no GlobalBonuses arm — the record states a \
                 total this crate cannot express, which a field-by-field comparison would have \
                 skipped in silence"
            )),
        }
    }
    if bad.is_empty() {
        Ok(g)
    } else {
        Err(bad)
    }
}

/// One per-type mez-resistance field, by its serialized (snake_case) name. Spelled out rather
/// than reflected because a struct cannot be written through serde: the arms are the whole of
/// [`EXPANDED_SCALAR`]'s permitted target set, and a type the struct grows without an arm here
/// fails loudly in [`expected_of`] instead of being silently excluded from the comparison.
fn assign_mez_resist_type(g: &mut GlobalBonuses, field: &str, value: f64) -> bool {
    match field {
        "mez_resist_hold" => g.mez_resist_hold = value,
        "mez_resist_stun" => g.mez_resist_stun = value,
        "mez_resist_immobilize" => g.mez_resist_immobilize = value,
        "mez_resist_sleep" => g.mez_resist_sleep = value,
        "mez_resist_confuse" => g.mez_resist_confuse = value,
        "mez_resist_fear" => g.mez_resist_fear = value,
        "mez_resist_knockback" => g.mez_resist_knockback = value,
        "mez_resist_repel" => g.mez_resist_repel = value,
        "mez_resist_teleport" => g.mez_resist_teleport = value,
        "mez_resist_taunt" => g.mez_resist_taunt = value,
        "mez_resist_placate" => g.mez_resist_placate = value,
        _ => return false,
    }
    true
}

/// Where we answer DIFFERENTLY ON PURPOSE, and the only difference allowed.
///
/// The TS always-on proc switch has no `Perception` arm at all — its `default: break` carries the
/// comment "Other categories (Control, Debuff, etc.) are not 'always-on' stats" — and the only TS
/// write to `perceptionRadius` anywhere is the per-POWER applier. So a Rectified Reticle
/// +Perception piece contributed nothing to the frozen answer. This crate routes it, deliberately:
/// `apply_single_proc_effect`'s `"Perception"` arm states "the same % global the power-side
/// applier feeds … Rectified Reticle and Warp are the shipping instances
/// (DATA-GAP-REGISTER PROC-PERCEPTION-1)", and `ProcRoute`'s own doc names this as one of three
/// real drops the beta's `_ => {}` hid. The record is the PRE-FIX answer.
///
/// Allowed by SHAPE and re-checked by VALUE, the standing `oracle.rs` gives its movement
/// divergence. Four clauses, all required: `perception_radius` must be the ONLY differing field;
/// the record must state nothing for it (a recorded value we disagreed with would be a real bug);
/// ours must be non-zero; and the record's own slots must actually hold a piece whose proc data
/// states a `Perception` effect — which is what stops this allowance from covering a perception
/// total minted from somewhere else. The comparison then re-runs with the field cleared and every
/// other field must still match exactly.
const KNOWN_DIVERGENCE_FIELD: &str = "perception_radius";
const KNOWN_DIVERGENCE_CATEGORY: &str = "Perception";

/// Does this record actually slot a piece whose proc data states the diverging category? Read out
/// of the shipped proc database, not from a list of piece names, so a set that gains or loses a
/// +Perception piece needs no edit here.
fn slots_diverging_category(db: &PowerDatabase, record: &Record) -> bool {
    record.powers.iter().any(|host| {
        host.slots.iter().flatten().any(|enh| {
            let coh_data::EnhancementKind::IoSet { set_name, .. } = &enh.kind else {
                return false;
            };
            db.procs.find(&enh.name, set_name).is_some_and(|proc| {
                proc.effects
                    .iter()
                    .any(|e| e.category == KNOWN_DIVERGENCE_CATEGORY)
            })
        })
    })
}

struct Replayed {
    g: GlobalBonuses,
    errors: Vec<String>,
}

/// Replay one record: the four proc passes into a fresh `GlobalBonuses`, nothing else.
fn replay(db: &PowerDatabase, dataset: DatasetId, record: &Record) -> Replayed {
    let mut state = CharacterState::empty(dataset);
    state.level = LEVEL;
    // Deliberately no archetype — see the header. `CharacterState::empty` already leaves it unset.
    state.primary.powers = record
        .powers
        .iter()
        .map(|host| {
            let mut pick = SelectedPower::picked(&host.internal_name, &host.powerset, 1);
            pick.is_active = host.is_active;
            pick.slots = host.slots.clone();
            pick
        })
        .collect();

    // The proc passes resolve each pick's def with `filter_map`, so a host that stopped existing
    // would be dropped silently and its procs would contribute nothing — which reads as a value
    // bug several lines away from its cause. Checked here instead.
    let mut errors: Vec<String> = record
        .powers
        .iter()
        .filter(|host| {
            db.resolve_power(&host.powerset, &host.internal_name)
                .is_none()
        })
        .map(|host| {
            format!(
                "recorded host {}/{} is not in this dataset — the proc passes would drop it \
                 silently",
                host.powerset, host.internal_name
            )
        })
        .collect();

    let mut g = GlobalBonuses::default();
    let mut stealth_contributions = Vec::new();
    let alpha = coh_math::incarnates::AlphaEnhancement::default();
    let mut proc_errors = Vec::new();
    coh_math::procs::apply_procs(
        &state,
        db,
        &mut g,
        &mut stealth_contributions,
        &alpha,
        &mut proc_errors,
    );
    // The stealth radii are ASSIGNED, not added, exactly as `recalculate` assigns them — the proc
    // pass pushes stealth-IO contributions into the same resolve the apply pass feeds, and the
    // resolve returns totals rather than accumulating.
    let stealth_totals = coh_math::stealth::resolve_stealth_radius(&stealth_contributions);
    g.stealth_radius_pve = stealth_totals.pve;
    g.stealth_radius_pvp = stealth_totals.pvp;
    coh_math::procs::apply_build_up_procs(&state, db, &mut g, &alpha, &mut proc_errors);

    errors.extend(
        proc_errors
            .iter()
            .map(|e| format!("calc: {} — {}", e.context, e.detail)),
    );
    errors.extend(
        g.errors
            .iter()
            .map(|e| format!("calc: {} — {}", e.context, e.detail)),
    );
    // Cleared so the whole-struct compare is over the NUMBERS. Nothing is lost: the channel is
    // reported above and any entry in it fails the run.
    g.errors.clear();
    Replayed { g, errors }
}

// ---------------------------------------------------------------- reporting

/// Which fields two `GlobalBonuses` disagree on, by diffing their serializations. Snake_case,
/// because that is what the struct serializes as (see the header).
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

// ---------------------------------------------------------------- the run

/// The coverage floor the emitter asked for: the AoE host pair that makes the proc area factor
/// observable. `manifest.json` names the `ProcMainTargetOnly` half per fork; if that host has
/// left the fixture, the area factor is no longer graded and the run says so rather than passing
/// on the unflagged half alone.
fn grade_main_target_floor(
    records: &[&Record],
    host: Option<&str>,
    dataset: DatasetId,
    failures: &mut Vec<String>,
) {
    let Some(host) = host else {
        println!(
            "  area factor: manifest names no ProcMainTargetOnly host on this fork, so the pair \
             that makes it observable is absent"
        );
        return;
    };
    let hosting = records
        .iter()
        .filter(|r| r.powers.iter().any(|p| p.internal_name == host))
        .count();
    if hosting == 0 {
        failures.push(format!(
            "{}: manifest names {host:?} as the ProcMainTargetOnly host and no record slots a \
             proc into it. The area factor is then graded by nothing: a flagged host alone \
             passes against an engine that never penalises an AoE, so the PAIR is the test.",
            dataset.as_str(),
        ));
        return;
    }
    println!("  area factor: {hosting} record(s) host the ProcMainTargetOnly power {host:?}");
}

/// The deepest same-piece stack across one record's hosts — the input the Rule-of-5 cap buckets
/// on, read off the recorded arguments rather than out of the crate's own tracking, so it says
/// what the FIXTURE offers and not what we happened to do with it.
fn deepest_same_piece_stack(record: &Record) -> usize {
    let mut counts: BTreeMap<(&str, &str), usize> = BTreeMap::new();
    for host in &record.powers {
        for enh in host.slots.iter().flatten() {
            let coh_data::EnhancementKind::IoSet { set_name, .. } = &enh.kind else {
                continue;
            };
            *counts
                .entry((set_name.as_str(), enh.name.as_str()))
                .or_default() += 1;
        }
    }
    counts.into_values().max().unwrap_or(0)
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
        "fixtures/procs/contributions.jsonl holds no records for {}",
        dataset.as_str()
    );

    let db = load(dataset);
    let mut failures: Vec<String> = Vec::new();
    let (mut graded, mut mismatched, mut empty, mut values) = (0usize, 0usize, 0usize, 0usize);
    let mut diverged_as_expected: Vec<String> = Vec::new();

    println!(
        "\n=== {} — {} recorded calls",
        dataset.as_str(),
        records.len()
    );

    for record in &records {
        let replayed = replay(&db, dataset, record);
        for e in &replayed.errors {
            failures.push(format!("line {}: {e}", record.line));
        }
        let expected = match expected_of(record, &replayed.g) {
            Ok(g) => g,
            Err(bad) => {
                for b in bad {
                    failures.push(format!("line {}: {b}", record.line));
                }
                continue;
            }
        };
        values += record.value.len();
        if replayed.g == expected {
            if record.value.is_empty() {
                empty += 1;
            } else {
                graded += 1;
            }
            continue;
        }
        // The one allowed difference, re-checked field by field rather than waved through.
        let differing = differing_fields(&replayed.g, &expected);
        if differing.len() == 1
            && differing[0].starts_with(&format!("{KNOWN_DIVERGENCE_FIELD}:"))
            && expected.perception_radius == 0.0
            && replayed.g.perception_radius != 0.0
            && slots_diverging_category(&db, record)
        {
            let mut cleared = replayed.g.clone();
            cleared.perception_radius = 0.0;
            if cleared == expected {
                graded += 1;
                diverged_as_expected.push(format!(
                    "line {} — {} {} that the frozen answer drops",
                    record.line, KNOWN_DIVERGENCE_FIELD, replayed.g.perception_radius
                ));
                continue;
            }
        }
        mismatched += 1;
        println!(
            "  MISMATCH line {}\n    powers:   {}\n    fields:\n      {}",
            record.line,
            record
                .powers
                .iter()
                .map(|p| format!(
                    "{}/{}{} [{}]",
                    p.powerset,
                    p.internal_name,
                    if p.is_active { "" } else { " (off)" },
                    p.slots
                        .iter()
                        .flatten()
                        .map(|e| e.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ))
                .collect::<Vec<_>>()
                .join(" + "),
            differing.join("\n      "),
        );
    }

    println!(
        "  {RECORDED_FN:<26} graded {graded} of {} non-empty calls, {values} field value(s); \
         {empty} recorded-empty call(s) also agree",
        graded + mismatched,
    );
    for line in &diverged_as_expected {
        println!("    known divergence, allowed: {line}");
    }
    let depth = records
        .iter()
        .map(|r| deepest_same_piece_stack(r))
        .max()
        .unwrap_or(0);
    println!(
        "  Rule of 5: deepest same-piece stack across one record's hosts is {depth}, so the cap \
         (which bites at 6) is graded by nothing"
    );
    if depth != RULE_OF_FIVE_DEPTH {
        failures.push(format!(
            "{}: the deepest same-piece stack in this fork's records is {depth} and this harness \
             was written against {RULE_OF_FIVE_DEPTH}. That is not a calc bug — it means the \
             corpus has started (or stopped) being able to reach the proc Rule-of-5 cap, which \
             changes what these records are worth. Re-read this file's header table, re-measure, \
             and update RULE_OF_FIVE_DEPTH.",
            dataset.as_str(),
        ));
    }
    grade_main_target_floor(
        &records,
        manifest
            .get(dataset.as_str())
            .unwrap_or_else(|| {
                panic!(
                    "manifest.json has no mainTargetHost for {}",
                    dataset.as_str()
                )
            })
            .as_deref(),
        dataset,
        &mut failures,
    );

    if mismatched > 0 {
        failures.push(format!(
            "{}/{RECORDED_FN}: {mismatched} of {} recorded calls disagree",
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
