//! The set-bonus replay harness: `calculateSetBonuses` and its layer-2 routing against
//! `fixtures/set-bonuses/`.
//!
//! WHAT THIS IS. 356 whole-call records —
//! `{fn: "calculateSetBonuses", args: {dataset, powers, exemplarLevel, buildLevel}, value}` —
//! and the richest of the three stage-5 chains, because each `value` carries THREE answers:
//!
//!   * `aggregated` — layer 1, the Rule-of-5-capped per-stat totals (`Σ value × min(count, 5)`).
//!   * `tracking` — the Rule-of-5 bookkeeping, reduced to `{stat: {valueKey: {count, capped}}}`.
//!     The emitter dropped the `sources` / `rejectedSources` lists on purpose: those depend on
//!     build traversal ORDER while count and capped do not, so this grades the cap's OUTCOME
//!     without pinning the order it was reached in.
//!   * `global` — layer 2, the non-zero `GlobalBonuses` deltas `applySetBonusesToGlobal` routes
//!     the aggregate into. That is the internal-key → field routing in isolation: the
//!     `kbprotection` ×0.01 scale, the `debuffresistrecharge` → Slow pairing, and the
//!     paired/`resAll` expansions.
//!
//! The third of the three replay chains, after
//! [`movement_replay`](../movement_replay.rs) and [`proc_replay`](../proc_replay.rs).
//!
//! WHO PRODUCED THEM. `scripts/emit-set-bonus-fixtures.ts`, deleted with all of `src/` in
//! `6caffef34` and readable at `6caffef34^`. The Rust equivalent is one call to
//! [`coh_math::set_bonuses::calculate_set_bonuses`] with the recorded `exemplarLevel` and
//! `buildLevel` passed straight through — the TS passed `exemplarLevel ?? undefined` and
//! `buildLevel` in exactly that shape, and `effective_level` is `exemplar.unwrap_or(build)` on
//! both sides. `pvp` is `false` because the TS signature had no such parameter, which is also why
//! `repel_resistance` appears in the manifest's census as `pvp-only` rather than in a record.
//!
//! THE POWERS CARRY NO POWERSET, and that is a fact about set bonuses rather than a hole: the
//! aggregation reads nothing but the slots. Each recorded power is `{name, slots}` and becomes a
//! [`coh_data::SelectedPower`] whose powerset is the recorded name, which nothing in this chain
//! looks at. Order is preserved because the Rule-of-5 accepts the first five instances.
//!
//! THE ONE DECLARED DIVERGENCE, and it reaches all three layers. The beta collapses six mez
//! resistances into a single scalar `mezresist`; this crate keeps them apart
//! (`MezResistHold`/`Stun`/`Immobilize`/`Sleep`/`Confuse`/`Fear`), which is MEZRES-1 and which
//! `GlobalBonuses::mez_resist`'s own doc describes while naming the two gates that should declare
//! it — "the two gates declare the expansion as their one divergence". `proc_replay.rs` is the
//! other; neither existed until now. Handled by COLLAPSING our side into the beta's vocabulary
//! once, in [`collapse_mez`], rather than by three separate allowances — and the collapse is
//! conditional on all six being present with an identical value and identical buckets. A subset,
//! or six that disagree, is left uncollapsed so the comparison fails and names them.
//!
//! THE STAT CENSUS IS ITSELF GRADED, which is the gate the emitter named and never got
//! (`set_bonus_gate::fixture_population_matches_stat_census`). `manifest.json` declares which
//! stat keys the fixture covers and which the oracle could not stage at all, each uncovered one
//! carrying the engine key the contract vocabulary gives it. [`grade_stat_census`] holds the
//! three to one another: `covered` must be exactly the keys the fixture's own aggregates produce,
//! every `uncovered` key must resolve to a real [`SetBonusStat`] this engine models, and no key
//! may be in both. A census that quietly narrowed would otherwise read as full coverage.
//!
//! EXACT f64, no tolerance, for the reason `movement_replay.rs` gives. The `valueKey` strings are
//! compared as STRINGS: they are the Rule-of-5 bucket identity, a deliberate `toFixed(2)` replica,
//! so `"7.50"` versus `"7.5"` is a real difference and not formatting.
//!
//! WHAT THESE RECORDS GRADE. All 356 agreed on the first run — 1,439 aggregate values, 1,559
//! Rule-of-5 buckets and 1,443 routed fields — which is the shape of a vacuous check, so each part
//! was perturbed before any of it was believed. Reddened records, out of 356:
//!
//! | Perturbation | Red | What that settles |
//! | --- | --- | --- |
//! | the `valueKey` bucket granularity 2dp → 3dp | **352** | the `toFixed(2)` replica IS the dedup identity, not a display detail |
//! | each aggregate total + 0.001 | **352** | layer 1 and the layer-2 routing under it are graded (the 4 survivors are the records where exemplar suppression leaves the aggregate empty) |
//! | one of the six mez types shifted by 0.001 | **stops the run** | [`collapse_mez`] is checked by VALUE; the three-layer agreement assert names which layer diverged |
//! | `pvp` passed as `true` instead of `false` | **24** | the PvP-effect skip is graded, so `pvp: false` is the recorded call and not a guess |
//! | exemplar suppression disabled | **12** | `piece_bonuses_active` is graded |
//! | the `kbprotection` × 0.01 scale dropped | **9** | the one scaled route is graded |
//! | the attuned branch of `piece_bonuses_active` ignored | **8** | attuned-vs-level suppression is graded separately from suppression itself |
//! | the `toFixed(2)` odd-eighths tie branch removed (Rust round-to-even) | **7** | the half-away-from-zero rule for `x.125`/`x.375`/`x.625`/`x.875` is graded — seven records turn on it |
//! | the Rule-of-5 cap raised from 5 to 6 | **4** | the cap is graded, unlike in `proc_replay.rs`, where the record that would have graded it was never emitted |
//! | the `debuffresistrecharge` → Slow pairing dropped | **4** | the one paired route is graded |
//!
//! So this is the deepest of the three chains: every documented mechanism in
//! `coh_math::set_bonuses` has at least four records standing behind it, down to the rounding rule
//! for odd eighths. It is also the one corpus that grades the Rule of 5 at all.
//!
//! Run: `cargo test -p coh_math --test set_bonus_replay -- --nocapture`

use coh_data::{DatasetId, Enhancement, Level, PowerDatabase, SelectedPower};
use coh_math::set_bonuses::SetBonusStat;
use coh_math::GlobalBonuses;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// The only `fn` this file replays.
const RECORDED_FN: &str = "calculateSetBonuses";

/// The six mez resistances the beta collapses into one scalar, and the key it collapses them to.
/// See the header. `MezResistRepel` is deliberately NOT among them — the beta's scalar never
/// covered repel, and `mezResistKnockback` has its own internal key (`kbresistance`).
const MEZ_SCALAR_TYPES: [SetBonusStat; 6] = [
    SetBonusStat::MezResistHold,
    SetBonusStat::MezResistStun,
    SetBonusStat::MezResistImmobilize,
    SetBonusStat::MezResistSleep,
    SetBonusStat::MezResistConfuse,
    SetBonusStat::MezResistFear,
];
const MEZ_SCALAR_KEY: &str = "mezresist";

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

/// One tracked bucket, reduced the way the emitter reduced it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Bucket {
    count: u32,
    capped: bool,
}

/// `{stat: {valueKey: {count, capped}}}`, in the beta's stat vocabulary.
type Tracking = BTreeMap<String, BTreeMap<String, Bucket>>;

#[derive(Debug)]
struct Record {
    /// 1-based line in `aggregation.jsonl`.
    line: usize,
    dataset: String,
    powers: Vec<(String, Vec<Option<Enhancement>>)>,
    exemplar_level: Option<Level>,
    build_level: Level,
    aggregated: BTreeMap<String, f64>,
    tracking: Tracking,
    global: BTreeMap<String, f64>,
}

/// Panic on any field this reader does not know, in either direction — the twin in
/// `movement_replay.rs` says why.
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

fn number_map(value: &Value, what: &str) -> BTreeMap<String, f64> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("{what}: not an object"))
        .iter()
        .map(|(k, v)| {
            let n = v
                .as_f64()
                .unwrap_or_else(|| panic!("{what}: {k} is not a number"));
            (k.clone(), n)
        })
        .collect()
}

fn parse_record(line_no: usize, line: &str) -> Record {
    let where_ = format!("fixtures/set-bonuses/aggregation.jsonl:{line_no}");
    let record: Value =
        serde_json::from_str(line).unwrap_or_else(|e| panic!("{where_}: not JSON: {e}"));
    exact_fields(
        record
            .as_object()
            .unwrap_or_else(|| panic!("{where_}: not an object")),
        &["fn", "args", "value"],
        &where_,
    );
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
    exact_fields(
        args,
        &["dataset", "powers", "exemplarLevel", "buildLevel"],
        &where_,
    );
    let build_level = args["buildLevel"]
        .as_u64()
        .and_then(|n| u8::try_from(n).ok())
        .and_then(Level::new)
        .unwrap_or_else(|| panic!("{where_}: buildLevel is not a level"));
    // `null` is "not exemplared", which is the same thing `Level`'s 0 sentinel spells.
    let exemplar_level = match &args["exemplarLevel"] {
        Value::Null => None,
        other => Some(
            other
                .as_u64()
                .and_then(|n| u8::try_from(n).ok())
                .and_then(Level::new)
                .unwrap_or_else(|| panic!("{where_}: exemplarLevel {other} is not a level")),
        ),
    };

    let powers = args["powers"]
        .as_array()
        .unwrap_or_else(|| panic!("{where_}: powers is not an array"))
        .iter()
        .map(|p| {
            let object = p
                .as_object()
                .unwrap_or_else(|| panic!("{where_}: a power is not an object"));
            exact_fields(object, &["name", "slots"], &where_);
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
            (
                object["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{where_}: name is not a string"))
                    .to_string(),
                slots,
            )
        })
        .collect();

    let value = record["value"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: value is not an object"));
    exact_fields(value, &["aggregated", "tracking", "global"], &where_);
    let tracking = value["tracking"]
        .as_object()
        .unwrap_or_else(|| panic!("{where_}: tracking is not an object"))
        .iter()
        .map(|(stat, buckets)| {
            let buckets = buckets
                .as_object()
                .unwrap_or_else(|| panic!("{where_}: tracking.{stat} is not an object"))
                .iter()
                .map(|(key, b)| {
                    let b = b
                        .as_object()
                        .unwrap_or_else(|| panic!("{where_}: {stat}.{key} is not an object"));
                    exact_fields(b, &["count", "capped"], &format!("{where_}: {stat}.{key}"));
                    (
                        key.clone(),
                        Bucket {
                            count: b["count"]
                                .as_u64()
                                .and_then(|n| u32::try_from(n).ok())
                                .unwrap_or_else(|| panic!("{where_}: {stat}.{key}.count")),
                            capped: b["capped"]
                                .as_bool()
                                .unwrap_or_else(|| panic!("{where_}: {stat}.{key}.capped")),
                        },
                    )
                })
                .collect();
            (stat.clone(), buckets)
        })
        .collect();

    Record {
        line: line_no,
        dataset: args["dataset"]
            .as_str()
            .unwrap_or_else(|| panic!("{where_}: dataset is not a string"))
            .to_string(),
        powers,
        exemplar_level,
        build_level,
        aggregated: number_map(&value["aggregated"], &format!("{where_}: aggregated")),
        tracking,
        global: number_map(&value["global"], &format!("{where_}: global")),
    }
}

fn read_records() -> Vec<Record> {
    let path = repo().join("fixtures/set-bonuses/aggregation.jsonl");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(i, line)| parse_record(i + 1, line))
        .collect()
}

/// One entry of `manifest.json`'s `statCoverage.uncovered`: a corpus stat the oracle could not
/// put in a line, and the engine key it names for it.
#[derive(Debug)]
struct Uncovered {
    raw: String,
    key: String,
    reason: String,
}

struct Manifest {
    covered: Vec<String>,
    uncovered: Vec<Uncovered>,
}

fn read_manifest(records: usize) -> Manifest {
    let path = repo().join("fixtures/set-bonuses/manifest.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let manifest: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let lines = manifest["aggregation"]["lines"]
        .as_u64()
        .unwrap_or_else(|| panic!("{path:?}: no aggregation.lines"));
    assert_eq!(
        lines as usize, records,
        "{path:?} states {lines} recorded lines and aggregation.jsonl holds {records} — the \
         manifest and the fixture were not written by the same run"
    );
    let census = &manifest["statCoverage"];
    Manifest {
        covered: census["covered"]
            .as_array()
            .unwrap_or_else(|| panic!("{path:?}: no statCoverage.covered"))
            .iter()
            .map(|v| {
                v.as_str()
                    .unwrap_or_else(|| panic!("{path:?}: a covered entry is not a string"))
                    .to_string()
            })
            .collect(),
        uncovered: census["uncovered"]
            .as_array()
            .unwrap_or_else(|| panic!("{path:?}: no statCoverage.uncovered"))
            .iter()
            .map(|v| Uncovered {
                raw: v["raw"].as_str().unwrap_or_default().to_string(),
                key: v["key"].as_str().unwrap_or_default().to_string(),
                reason: v["reason"].as_str().unwrap_or_default().to_string(),
            })
            .collect(),
    }
}

// ---------------------------------------------------------------- our side

/// Collapse the six per-type mez resistances into the beta's single scalar, the one declared
/// divergence (see the header).
///
/// Conditional on the whole shape: all six present, all carrying the same value, and — for the
/// tracking — the same bucket map. Anything less is left alone, so a subset or a disagreement
/// among the six reaches the comparison and fails there instead of being collapsed into
/// agreement. Returns whether it collapsed, so the run can say how many records leaned on it.
fn collapse_mez<T: Clone + PartialEq>(map: &mut BTreeMap<String, T>) -> bool {
    let keys: Vec<String> = MEZ_SCALAR_TYPES
        .iter()
        .map(|s| s.as_beta_key().to_string())
        .collect();
    let present: Vec<&T> = keys.iter().filter_map(|k| map.get(k)).collect();
    if present.len() != keys.len() || present.windows(2).any(|w| w[0] != w[1]) {
        return false;
    }
    let collapsed = present[0].clone();
    for key in &keys {
        map.remove(key);
    }
    map.insert(MEZ_SCALAR_KEY.to_string(), collapsed);
    true
}

/// The same collapse on a `GlobalBonuses`: the six per-type fields fold into the scalar the beta
/// wrote. Value-conditional in the same way, and `mez_resist` must be untouched — this crate
/// never writes it, which is the whole reason the expansion exists.
fn collapse_mez_global(g: &mut GlobalBonuses) -> bool {
    let six = [
        g.mez_resist_hold,
        g.mez_resist_stun,
        g.mez_resist_immobilize,
        g.mez_resist_sleep,
        g.mez_resist_confuse,
        g.mez_resist_fear,
    ];
    if g.mez_resist != 0.0 || six[0] == 0.0 || six.iter().any(|v| *v != six[0]) {
        return false;
    }
    g.mez_resist = six[0];
    g.mez_resist_hold = 0.0;
    g.mez_resist_stun = 0.0;
    g.mez_resist_immobilize = 0.0;
    g.mez_resist_sleep = 0.0;
    g.mez_resist_confuse = 0.0;
    g.mez_resist_fear = 0.0;
    true
}

struct Replayed {
    aggregated: BTreeMap<String, f64>,
    tracking: Tracking,
    global: GlobalBonuses,
    errors: Vec<String>,
    collapsed: bool,
}

fn replay(db: &PowerDatabase, record: &Record) -> Replayed {
    let catalog = db
        .io_sets
        .as_ref()
        .expect("the bundle carries an io-sets section");
    let powers: Vec<SelectedPower> = record
        .powers
        .iter()
        .map(|(name, slots)| {
            // The chain reads slots only, so the powerset is the recorded name: there is no
            // powerset in the record and nothing here would look at one.
            let mut pick = SelectedPower::picked(name.clone(), name.clone(), 1);
            pick.slots = slots.clone();
            pick
        })
        .collect();

    let result = coh_math::set_bonuses::calculate_set_bonuses(
        powers.iter(),
        catalog,
        record.exemplar_level,
        record.build_level,
        false,
    );

    let mut aggregated: BTreeMap<String, f64> = result
        .aggregated
        .iter()
        .map(|(stat, v)| (stat.as_beta_key().to_string(), *v))
        .collect();
    let mut tracking: Tracking = result
        .tracking
        .iter()
        .map(|(stat, buckets)| {
            (
                stat.as_beta_key().to_string(),
                buckets
                    .iter()
                    .map(|(key, b)| {
                        (
                            key.clone(),
                            Bucket {
                                count: b.count,
                                capped: b.capped,
                            },
                        )
                    })
                    .collect(),
            )
        })
        .collect();

    let mut global = GlobalBonuses::default();
    coh_math::set_bonuses::apply_set_bonuses_to_global(&mut global, &result);
    let errors = global
        .errors
        .iter()
        .map(|e| format!("calc: {} — {}", e.context, e.detail))
        .collect();
    global.errors.clear();

    // One collapse, three layers — they must agree about whether it applied, or the three answers
    // are not describing the same build.
    let a = collapse_mez(&mut aggregated);
    let t = collapse_mez(&mut tracking);
    let gg = collapse_mez_global(&mut global);
    assert_eq!(
        (a, t),
        (gg, gg),
        "line {}: the mez collapse applied to {a:?}/{t:?}/{gg:?} of the three layers — a \
         divergence that reaches one layer and not another is not the declared one",
        record.line
    );

    Replayed {
        aggregated,
        tracking,
        global,
        errors,
        collapsed: gg,
    }
}

// ---------------------------------------------------------------- reporting

fn describe_numbers(map: &BTreeMap<String, f64>) -> String {
    if map.is_empty() {
        return "{}".to_string();
    }
    format!(
        "{{{}}}",
        map.iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn describe_tracking(t: &Tracking) -> String {
    if t.is_empty() {
        return "{}".to_string();
    }
    format!(
        "{{{}}}",
        t.iter()
            .map(|(stat, buckets)| {
                format!(
                    "{stat}: {}",
                    buckets
                        .iter()
                        .map(|(k, b)| format!(
                            "{k}×{}{}",
                            b.count,
                            if b.capped { " capped" } else { "" }
                        ))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    )
}

/// Which fields two `GlobalBonuses` disagree on, by diffing their serializations (snake_case).
fn differing_fields(ours: &GlobalBonuses, theirs: &GlobalBonuses) -> Vec<String> {
    let to_map = |g: &GlobalBonuses| -> serde_json::Map<String, Value> {
        match serde_json::to_value(g).expect("GlobalBonuses serializes") {
            Value::Object(map) => map,
            other => panic!("GlobalBonuses did not serialize as an object: {other}"),
        }
    };
    let (a, b) = (to_map(ours), to_map(theirs));
    a.iter()
        .filter(|(field, ours)| b.get(*field).unwrap_or(&Value::Null) != *ours)
        .map(|(field, ours)| {
            format!(
                "{field}: recorded {}, ours {ours}",
                b.get(field).unwrap_or(&Value::Null)
            )
        })
        .collect()
}

// ---------------------------------------------------------------- the run

/// A recorded `global` map as a `GlobalBonuses`, through the calc's own routing. See
/// `proc_replay.rs` for why the comparison is whole-struct rather than field-by-field.
fn expected_global(record: &Record) -> Result<GlobalBonuses, Vec<String>> {
    let mut g = GlobalBonuses::default();
    let mut bad = Vec::new();
    for (field, value) in &record.global {
        match g.add_by_camel_name(field, *value) {
            coh_math::totals::TypeRoute::Routed => {}
            other => bad.push(format!(
                "recorded global field {field:?} did not route ({other:?}) — the record states a \
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

/// The gate the emitter named and never wrote: hold the fixture BODY, the manifest CENSUS and the
/// ENGINE to one another.
///
/// Reads the whole fixture rather than one fork's slice, because `covered` is the union the
/// emitter accumulated across all four.
fn grade_stat_census(all: &[Record], manifest: &Manifest, failures: &mut Vec<String>) {
    // Every stat key the fixture's own aggregates produce — with the mez scalar re-expanded,
    // because `covered` is stated in the beta's vocabulary and the collapse is ours.
    let mut produced: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
    for record in all {
        produced.extend(record.aggregated.keys().map(String::as_str));
    }
    let declared: std::collections::BTreeSet<&str> =
        manifest.covered.iter().map(String::as_str).collect();
    let missing: Vec<&&str> = declared.difference(&produced).collect();
    let undeclared: Vec<&&str> = produced.difference(&declared).collect();
    if !missing.is_empty() || !undeclared.is_empty() {
        failures.push(format!(
            "manifest statCoverage.covered and the fixture body disagree: {} declared stat(s) \
             appear in no record's aggregate ({missing:?}), {} stat(s) appear in a record and are \
             not declared ({undeclared:?}). A census that narrows quietly reads as full coverage.",
            missing.len(),
            undeclared.len(),
        ));
    }
    for entry in &manifest.uncovered {
        if SetBonusStat::from_beta_key(&entry.key).is_none() {
            failures.push(format!(
                "manifest declares {:?} ({}) ungraded under the engine key {:?}, and this engine \
                 models no such stat. The census names what went ungraded in the ENGINE's \
                 spelling so it can be resolved to a real SetBonusStat; a key that resolves to \
                 nothing declares a gap in something that does not exist.",
                entry.raw, entry.reason, entry.key,
            ));
        }
        if declared.contains(entry.key.as_str()) {
            failures.push(format!(
                "manifest declares {:?} both covered and uncovered (key {:?})",
                entry.raw, entry.key,
            ));
        }
    }
    println!(
        "  stat census: {} stat(s) declared covered and produced by the body, {} declared \
         unstageable and each resolving to a real engine stat ({})",
        declared.len(),
        manifest.uncovered.len(),
        manifest
            .uncovered
            .iter()
            .map(|u| format!("{} [{}]", u.raw, u.reason))
            .collect::<Vec<_>>()
            .join(", "),
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
        "fixtures/set-bonuses/aggregation.jsonl holds no records for {}",
        dataset.as_str()
    );

    let db = load(dataset);
    let mut failures: Vec<String> = Vec::new();
    let (mut graded, mut mismatched) = (0usize, 0usize);
    let (mut values, mut buckets, mut fields) = (0usize, 0usize, 0usize);
    let (mut collapsed, mut capped, mut exemplared) = (0usize, 0usize, 0usize);

    println!(
        "\n=== {} — {} recorded calls",
        dataset.as_str(),
        records.len()
    );

    for record in &records {
        let replayed = replay(&db, record);
        for e in &replayed.errors {
            failures.push(format!("line {}: {e}", record.line));
        }
        let expected = match expected_global(record) {
            Ok(g) => g,
            Err(bad) => {
                for b in bad {
                    failures.push(format!("line {}: {b}", record.line));
                }
                continue;
            }
        };
        values += record.aggregated.len();
        fields += record.global.len();
        buckets += record.tracking.values().map(BTreeMap::len).sum::<usize>();
        if replayed.collapsed {
            collapsed += 1;
        }
        if record.exemplar_level.is_some() {
            exemplared += 1;
        }
        if record
            .tracking
            .values()
            .any(|b| b.values().any(|v| v.capped))
        {
            capped += 1;
        }

        let mut wrong: Vec<String> = Vec::new();
        if replayed.aggregated != record.aggregated {
            wrong.push(format!(
                "aggregated:\n      recorded: {}\n      ours:     {}",
                describe_numbers(&record.aggregated),
                describe_numbers(&replayed.aggregated),
            ));
        }
        if replayed.tracking != record.tracking {
            wrong.push(format!(
                "tracking:\n      recorded: {}\n      ours:     {}",
                describe_tracking(&record.tracking),
                describe_tracking(&replayed.tracking),
            ));
        }
        if replayed.global != expected {
            wrong.push(format!(
                "global:\n      {}",
                differing_fields(&replayed.global, &expected).join("\n      "),
            ));
        }
        if wrong.is_empty() {
            graded += 1;
            continue;
        }
        mismatched += 1;
        println!(
            "  MISMATCH line {} (exemplar {:?}, {} power(s))\n    {}",
            record.line,
            record.exemplar_level.map(Level::get),
            record.powers.len(),
            wrong.join("\n    "),
        );
    }

    println!(
        "  {RECORDED_FN:<22} graded {graded} of {} calls — {values} aggregate value(s), \
         {buckets} Rule-of-5 bucket(s), {fields} routed field(s)",
        graded + mismatched,
    );
    println!(
        "  of those: {capped} record(s) carry a CAPPED bucket, {exemplared} are exemplared, \
         {collapsed} lean on the declared mez collapse"
    );
    grade_stat_census(&all, &manifest, &mut failures);

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
