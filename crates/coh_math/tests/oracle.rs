//! The oracle harness: the atom appliers against the frozen `fixtures/oracle/` records.
//!
//! WHAT THIS IS. `fixtures/oracle/` holds the answers the DELETED TypeScript calculator gave,
//! one line per (power, applier), recorded before the rewrite and never read since —
//! `coh_data`'s own header says this crate never reads them. They are the only independent
//! statement of what the shipped app used to answer, so they can grade the Rust port. They are
//! also a ONE-TIME asset, for the reason `tools/defdiff.py` gives about this repo's other
//! fixtures: an answer key generated from your own implementation catches drift and
//! structurally cannot catch a shared mistake. Spend it while the inputs still match, then
//! promote our own output to a living baseline regenerated per re-export.
//! `scripts/keys/prov8-shard-drift.cjs` says whether the window is still open.
//!
//! THE DEADLINE IS PER FORK, which is why there is one test per fork rather than one test.
//! A record stores `{scale, table}` — one level ABOVE the number — so a patch that changes a
//! power's scale stales it and a patch that changes an archetype's modifier table does not.
//! When a fork patches, that fork's test reddens and the other three keys are untouched.
//! The `APPLIERS` table below is the inventory, and its order is the order the corpus is spent in.
//!
//! WHAT IS WIRED. [`APPLIERS`] is the whole of it: one row per recorded applier, naming the
//! fixture file, the shape its values take, and the reader to call. 14 appliers were recorded
//! against 40 public readers in `appliers/`, so the frozen corpus can never reach more than a
//! third of them — the rest need `fixtures/totals/`, or the game.
//!
//! WHICH VIEW OF A POWER IS COMPARED, and why not the plain one. A record holds ONE value per
//! power and names no archetype, so a power whose atoms FORK by archetype has no single value
//! for it to hold. Rebirth authors three pool powers that way — Combat Jumping, Acrobatics
//! (internal name `Leap`) and Tough — with a Kheldian arm and an everyone-else arm, and ZERO
//! such atoms exist on the other three forks. The app never reads the unnarrowed power:
//! `apply::apply_active_power_bonuses` calls `Power::for_caster_class` first and hands the
//! appliers the one arm this build gets. So the comparable reading is
//! [`coh_data::Power::as_bag_view`], whose own doc names this exact case — "without it
//! Rebirth's Acrobatics reads as a phantom, an atom value the bag 'lost', when in truth neither
//! side is wrong and only one of them can express a fork". Reading the raw power instead
//! reported both as value bugs on this harness's first run. Forked powers are COUNTED and
//! NAMED, because what the fixture cannot grade is a hole in the corpus and not a pass.
//!
//! WHAT THE ORACLE NEVER RECORDED, and why that is a fact rather than a gap. The bundle carries
//! two powersets the fixture walker could not have seen: `Inherent` and `Accolades`.
//! `forEachComposedPower` walks the three composed surfaces only, and those two are merged into
//! the bundle's `powersets` section by the emitter afterwards, on purpose
//! (`emit-contract.cjs:370` and `:387` — the inherents must reach the Rust calc because Pass 3
//! derives Vigilance from their atoms). They are named in [`UNRECORDED_SETS`] and counted per
//! fork. The direction is checked both ways: a key the fixture holds and the bundle does not is
//! always a failure, and so is a bundle key from any OTHER set — a third synthetic set cannot
//! widen this hole in silence.
//!
//! THE KEY IS ITSELF A CHECK. `scripts/collect-composed-powers.cjs` built each fixture key by
//! walking the composed powers in partition order and numbering same-name twins `@0`, `@1`, …;
//! its comment promises "the Rust gate derives the identical key while iterating the bundle in
//! the same partition order". That gate was never written. This is it, and it compares the two
//! KEY SETS before it compares any value — if the walk orders had drifted, the epic twins would
//! land on each other's numbers and a value comparison alone would report those as value bugs.
//!
//! Run: `cargo test -p coh_math --test oracle -- --nocapture`

use coh_data::slot_value::MovementValue;
use coh_data::{DatasetId, Power, PowerDatabase};
use coh_math::appliers::TypedValue;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

// ---------------------------------------------------------------- the corpus walk

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

/// `identity.replace(/\s+/g, '_')` from `collect-composed-powers.cjs:225`, run on the same
/// identity the JS reads (`internalName`, else the tail of `fullName`, else `name` — which is
/// what [`Power::ident`] returns once `normalize_legacy_power` has lifted the pool and epic
/// partitions out of their legacy shape).
fn fixture_ident(power: &Power) -> String {
    let mut out = String::with_capacity(power.ident().len());
    let mut in_space = false;
    for c in power.ident().chars() {
        if c.is_whitespace() {
            if !in_space {
                out.push('_');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// Every composed power with the fixture key the JS walker gave it.
///
/// Order is the contract: powersets in registry order, then the pool partition, then the epic
/// partition, mirroring `forEachComposedPower`. The occurrence counter is shared across all
/// three, exactly as the JS `occurrences` map is.
fn keyed_powers(db: &PowerDatabase) -> Vec<(String, &Power)> {
    let mut occurrences: BTreeMap<String, usize> = BTreeMap::new();
    let mut out: Vec<(String, &Power)> = Vec::new();

    let mut emit = |source: &str, container: &str, power: &'_ Power| {
        let base = format!("{source}/{container}/{}", fixture_ident(power));
        let occ = occurrences.entry(base.clone()).or_insert(0);
        let key = format!("{base}@{occ}");
        *occ += 1;
        key
    };

    for powerset in &db.powersets {
        for power in &powerset.powers {
            let key = emit("powerset", &powerset.id, power);
            out.push((key, power));
        }
    }
    for entry in &db.pool_powers {
        let key = emit("pool", "pool", &entry.power);
        out.push((key, &entry.power));
    }
    for entry in &db.epic_powers {
        let key = emit("epic", "epic", &entry.power);
        out.push((key, &entry.power));
    }
    out
}

/// The two powersets in the bundle that the frozen oracle never recorded, because the emitter
/// merges them in after the walker it shared with the fixture run has finished. See the module
/// header. A bundle power outside these two with no recorded answer fails the run.
const UNRECORDED_SETS: [&str; 2] = ["Inherent", "Accolades"];

fn is_unrecorded_by_construction(key: &str) -> bool {
    UNRECORDED_SETS
        .iter()
        .any(|set| key.starts_with(&format!("powerset/{set}/")))
}

// ---------------------------------------------------------------- the graded answer

/// One graded value, in the shape both sides reduce to.
///
/// Deliberately ONE struct across all three record shapes rather than three. A field neither
/// side states sits at its default on both and compares equal, and the alternative — a shape
/// per applier — is how two readers of the same fixture come to disagree about what a missing
/// field means. `scale` may be NaN (our readers' stand-in for an atom with no scale at all), so
/// [`same_num`] treats NaN as equal to NaN rather than using `f64`'s answer, which would report
/// every such value as a mismatch forever.
#[derive(Debug, Default, Clone)]
struct Cell {
    scale: f64,
    table: Option<String>,
    per_target: Option<f64>,
    /// Movement only: the binary suppress group.
    stack_key: Option<String>,
    /// Movement only: drops in combat.
    suppressible: bool,
    /// Movement only: the caster's enhancements do not multiply this entry.
    ignore_strength: bool,
}

/// A whole recorded answer. `Absent` is a recorded `null` — "this power states no atoms of
/// this kind" — and stays distinct from an empty [`Answer::Keyed`], which is "it states some
/// and none of them route to this slot". Collapsing the two would stop the routing chains
/// being graded at all, and routing is most of what the appliers decide.
#[derive(Debug)]
enum Answer {
    Absent,
    One(Cell),
    Keyed(Vec<(String, Cell)>),
}

fn same_num(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a == b
}

fn same_opt_num(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same_num(x, y),
        _ => false,
    }
}

fn same_cell(a: &Cell, b: &Cell) -> bool {
    same_num(a.scale, b.scale)
        && a.table == b.table
        && same_opt_num(a.per_target, b.per_target)
        && a.stack_key == b.stack_key
        && a.suppressible == b.suppressible
        && a.ignore_strength == b.ignore_strength
}

fn same_answer(a: &Answer, b: &Answer) -> bool {
    match (a, b) {
        (Answer::Absent, Answer::Absent) => true,
        (Answer::One(x), Answer::One(y)) => same_cell(x, y),
        (Answer::Keyed(x), Answer::Keyed(y)) => {
            x.len() == y.len()
                && x.iter()
                    .zip(y)
                    .all(|((kx, vx), (ky, vy))| kx == ky && same_cell(vx, vy))
        }
        _ => false,
    }
}

// ---------------------------------------------------------------- our side

fn cell_of_typed(v: &TypedValue) -> Cell {
    Cell {
        scale: v.scale,
        table: v.table.as_deref().map(str::to_string),
        per_target: v.per_target,
        ..Cell::default()
    }
}

fn from_typed(v: Option<TypedValue>) -> Answer {
    match v {
        None => Answer::Absent,
        Some(v) => Answer::One(cell_of_typed(&v)),
    }
}

#[allow(dead_code)]
fn from_typed_map(v: Option<Vec<(String, TypedValue)>>) -> Answer {
    match v {
        None => Answer::Absent,
        Some(list) => Answer::Keyed(
            list.iter()
                .map(|(k, v)| (k.clone(), cell_of_typed(v)))
                .collect(),
        ),
    }
}

fn from_movement(v: Option<Vec<(&'static str, MovementValue)>>) -> Answer {
    match v {
        None => Answer::Absent,
        Some(list) => Answer::Keyed(
            list.iter()
                .map(|(axis, m)| {
                    (
                        (*axis).to_string(),
                        Cell {
                            scale: m.scale,
                            table: m.table.as_deref().map(str::to_string),
                            per_target: m.per_target,
                            stack_key: m.stack_key.as_deref().map(str::to_string),
                            suppressible: m.suppressible,
                            ignore_strength: m.ignore_strength,
                        },
                    )
                })
                .collect(),
        ),
    }
}

// ---------------------------------------------------------------- the table

/// Which shape a recorded value takes. Declared per applier rather than sniffed from the JSON,
/// so a fixture that changes shape fails loudly instead of being re-interpreted.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    /// `{scale, table, perTarget?}` — one scaled value.
    One,
    /// `{smashing: {scale, table}, …}` — a damage-type map.
    TypeMap,
    /// `[{axis, scale, table, …}]` — a movement axis list, ORDER-SIGNIFICANT.
    AxisList,
}

struct Applier {
    /// The `applier` field in the fixture record.
    recorded: &'static str,
    /// The `fixtures/oracle/<fork>/<file>.jsonl` the records live in.
    file: &'static str,
    shape: Shape,
    read: fn(&Power) -> Answer,
}

/// Every applier this harness grades. One row is the whole cost of adding one.
///
/// Fourteen were recorded. The nine `Shape::One` and four `Shape::TypeMap` rows are stages 2
/// and are not wired yet — deliberately, because the stage that adds
/// them reports its divergences before anything is fixed.
const APPLIERS: &[Applier] = &[
    Applier {
        recorded: "movementBuffValue",
        file: "movement",
        shape: Shape::AxisList,
        read: |p| from_movement(coh_math::appliers::movement::movement_buff_value(p)),
    },
    Applier {
        recorded: "damageBuffValue",
        file: "damage",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::damage::damage_buff_value(p)),
    },
    Applier {
        recorded: "toHitBuffValue",
        file: "tohit",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::to_hit::to_hit_buff_value(p)),
    },
    Applier {
        recorded: "toHitBuffUnenhancedValue",
        file: "tohit",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::to_hit::to_hit_buff_unenhanced_value(p)),
    },
    Applier {
        recorded: "maxHPBuffValue",
        file: "maxhp",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::maxhp::max_hp_buff_value(p)),
    },
    Applier {
        recorded: "maxHPBuffUnenhancedValue",
        file: "maxhp",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::maxhp::max_hp_buff_unenhanced_value(p)),
    },
    Applier {
        recorded: "regenBuffValue",
        file: "resources",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::resources::regen_buff_value(p)),
    },
    Applier {
        recorded: "regenBuffUnenhancedValue",
        file: "resources",
        shape: Shape::One,
        read: |p| {
            from_typed(coh_math::appliers::resources::regen_buff_unenhanced_value(
                p,
            ))
        },
    },
    Applier {
        recorded: "recoveryBuffValue",
        file: "resources",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::resources::recovery_buff_value(p)),
    },
    Applier {
        recorded: "recoveryBuffUnenhancedValue",
        file: "resources",
        shape: Shape::One,
        read: |p| from_typed(coh_math::appliers::resources::recovery_buff_unenhanced_value(p)),
    },
    Applier {
        recorded: "defenseBuffValue",
        file: "defense",
        shape: Shape::TypeMap,
        read: |p| from_typed_map(coh_math::appliers::defense::defense_buff_value(p)),
    },
    Applier {
        recorded: "defenseBuffSuppressibleValue",
        file: "defense",
        shape: Shape::TypeMap,
        read: |p| from_typed_map(coh_math::appliers::defense::defense_buff_suppressible_value(p)),
    },
    Applier {
        recorded: "resistanceBuffValue",
        file: "resistance",
        shape: Shape::TypeMap,
        read: |p| from_typed_map(coh_math::appliers::resistance::resistance_buff_value(p)),
    },
    Applier {
        recorded: "resistanceSelfDebuffValue",
        file: "resistance",
        shape: Shape::TypeMap,
        read: |p| from_typed_map(coh_math::appliers::resistance::resistance_self_debuff_value(p)),
    },
];

/// Where we answer DIFFERENTLY ON PURPOSE, by (applier, fixture key), and the only difference
/// allowed.
///
/// Thunderspy's Aging Touch — display name "Dangerous Acceleration" — is the sole movement atom
/// in any fork carrying a `perTarget` increment: its run-speed self-buff grows by 0.1 per extra
/// target the cone hits. The converter stamps that (MOVEMAP-5, `convert-powerset.cjs:7679`) and
/// `MovementValue::per_target` carries it; the frozen record predates the whole field.
///
/// Allowlisted by KEY but not by value: the comparison re-runs with `per_target` cleared and
/// still demands an exact match on every other field, so this cannot hide a second disagreement
/// on the same power. If the increment stops being emitted, or another power gains one, the run
/// goes red and says which.
const KNOWN_DIVERGENCE: [(&str, &str); 1] = [(
    "movementBuffValue",
    "powerset/blaster/temporal-manipulation/Aging_Touch@0",
)];

/// The same answer with every per-target increment dropped — the one transformation
/// [`KNOWN_DIVERGENCE`] permits.
fn without_per_target(answer: &Answer) -> Answer {
    let strip = |c: &Cell| Cell {
        per_target: None,
        ..c.clone()
    };
    match answer {
        Answer::Absent => Answer::Absent,
        Answer::One(c) => Answer::One(strip(c)),
        Answer::Keyed(list) => {
            Answer::Keyed(list.iter().map(|(k, c)| (k.clone(), strip(c))).collect())
        }
    }
}

fn states_per_target(answer: &Answer) -> bool {
    match answer {
        Answer::Absent => false,
        Answer::One(c) => c.per_target.is_some(),
        Answer::Keyed(list) => list.iter().any(|(_, c)| c.per_target.is_some()),
    }
}

// ---------------------------------------------------------------- their side

/// One fixture entry object as a [`Cell`]. Panics rather than skipping on a field it does not
/// know: a key the emitter wrote and this reader drops would be a silent pass.
fn cell_of_json(value: &Value, shape: Shape, key: &str, what: &str) -> Cell {
    let object = value
        .as_object()
        .unwrap_or_else(|| panic!("{key}: {what} is not an object: {value}"));
    let allowed: &[&str] = match shape {
        Shape::One => &["scale", "table", "perTarget"],
        Shape::TypeMap => &["scale", "table", "perTarget", "toWho"],
        Shape::AxisList => &[
            "axis",
            "scale",
            "table",
            "perTarget",
            "stackKey",
            "suppressible",
            "ignoreStrength",
        ],
    };
    for field in object.keys() {
        assert!(
            allowed.contains(&field.as_str()),
            "{key}: unknown field {field:?} in a recorded {shape:?} value — the fixture states \
             something this comparison drops, which would pass by omission"
        );
    }
    let string = |name: &str| {
        object.get(name).filter(|v| !v.is_null()).map(|v| {
            v.as_str()
                .unwrap_or_else(|| panic!("{key}: {name} not a string"))
                .to_string()
        })
    };
    let number = |name: &str| {
        object.get(name).filter(|v| !v.is_null()).map(|v| {
            v.as_f64()
                .unwrap_or_else(|| panic!("{key}: {name} not a number"))
        })
    };
    // `toWho` is CHECKED here and then dropped, rather than compared. Only
    // `resistanceSelfDebuffValue` records it, its reader's filter is `reaches_caster(...)` so
    // every entry it returns is self-directed by construction, and all 136 recorded inner
    // values across all four forks say `Self` — one power, Bio Armor's Offensive Adaptation
    // (Organic Armor on Thunderspy). A constant grades nothing, so `TypedValue` does not carry
    // it and the comparison cannot. What CAN go wrong is the constant stopping being constant,
    // and that is what this asserts: the day a record says anything else, the run stops and
    // names the power. `resistance_self_debuff_resistible`'s doc states the same split from the
    // other side — "that function's shape is what the TS-fixture oracle gate diffs against".
    if let Some(who) = string("toWho") {
        assert_eq!(
            who, "Self",
            "{key}: a recorded self-debuff entry states toWho {who:?}, not \"Self\". This \
             comparison drops the field BECAUSE it has only ever been the one value; a second \
             value means the reader has to carry it."
        );
    }
    Cell {
        scale: number("scale").unwrap_or(f64::NAN),
        table: string("table"),
        per_target: number("perTarget"),
        stack_key: string("stackKey"),
        suppressible: object
            .get("suppressible")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        ignore_strength: object
            .get("ignoreStrength")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

fn answer_of_json(value: &Value, shape: Shape, key: &str) -> Answer {
    if value.is_null() {
        return Answer::Absent;
    }
    match shape {
        Shape::One => Answer::One(cell_of_json(value, shape, key, "value")),
        Shape::TypeMap => {
            let object = value
                .as_object()
                .unwrap_or_else(|| panic!("{key}: a TypeMap value is not an object: {value}"));
            Answer::Keyed(
                object
                    .iter()
                    .map(|(t, v)| (t.clone(), cell_of_json(v, shape, key, t)))
                    .collect(),
            )
        }
        Shape::AxisList => {
            let items = value
                .as_array()
                .unwrap_or_else(|| panic!("{key}: an AxisList value is not an array: {value}"));
            Answer::Keyed(
                items
                    .iter()
                    .map(|v| {
                        let cell = cell_of_json(v, shape, key, "entry");
                        let axis = v["axis"]
                            .as_str()
                            .unwrap_or_else(|| panic!("{key}: entry has no axis"))
                            .to_string();
                        (axis, cell)
                    })
                    .collect(),
            )
        }
    }
}

/// One fixture file, as applier -> key -> raw recorded value. Read once per file rather than
/// once per applier: `resources.jsonl` holds four appliers and is 1.5 MB per fork.
fn recorded_file(dataset: DatasetId, file: &str) -> BTreeMap<String, BTreeMap<String, Value>> {
    let path = repo()
        .join("fixtures/oracle")
        .join(dataset.as_str())
        .join(format!("{file}.jsonl"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    let mut out: BTreeMap<String, BTreeMap<String, Value>> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let record: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("{path:?}:{}: {e}", n + 1));
        let applier = record["applier"]
            .as_str()
            .unwrap_or_else(|| panic!("{path:?}:{}: record has no applier", n + 1))
            .to_string();
        let key = record["key"]
            .as_str()
            .unwrap_or_else(|| panic!("{path:?}:{}: record has no key", n + 1))
            .to_string();
        let previous = out
            .entry(applier.clone())
            .or_default()
            .insert(key.clone(), record["value"].clone());
        assert!(
            previous.is_none(),
            "{applier}/{key}: recorded twice in {path:?}"
        );
    }
    out
}

// ---------------------------------------------------------------- reporting

fn describe(answer: &Answer) -> String {
    let cell = |c: &Cell| {
        let mut s = format!("{}", c.scale);
        if let Some(t) = &c.table {
            s.push_str(&format!(" [{t}]"));
        }
        if let Some(p) = c.per_target {
            s.push_str(&format!(" perTarget:{p}"));
        }
        if let Some(k) = &c.stack_key {
            s.push_str(&format!(" stack:{k}"));
        }
        if c.suppressible {
            s.push_str(" suppressible");
        }
        if c.ignore_strength {
            s.push_str(" ignoreStrength");
        }
        s
    };
    match answer {
        Answer::Absent => "null".to_string(),
        Answer::One(c) => cell(c),
        Answer::Keyed(list) => format!(
            "[{}]",
            list.iter()
                .map(|(k, c)| format!("{k}={}", cell(c)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// How many mismatching powers to print per applier before summarizing the rest. Enough to see
/// whether a failure is one family or a systemic one, short of a wall of output.
const SHOW_AT_MOST: usize = 25;

/// One applier's result on one fork.
#[derive(Default)]
struct Tally {
    /// Both sides said the power states no atoms of this kind.
    absent: usize,
    /// Both sides produced an empty map — atoms exist, none route to this slot.
    empty: usize,
    /// Both sides produced a value and it matched. THE population that grades anything.
    graded: usize,
    mismatched: usize,
    /// Powers whose answer changes once the archetype fork is resolved. The fixture holds one
    /// value and names no archetype, so it grades the fork-free reading and says nothing about
    /// the arms.
    forked: Vec<String>,
    diverged_as_expected: Vec<String>,
}

// ---------------------------------------------------------------- the rewrite

/// A whole number as the JS emitter wrote it (`1`, not `1.0`), anything else as-is.
fn json_num(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}

fn json_of_cell(cell: &Cell, shape: Shape, applier: &str) -> serde_json::Map<String, Value> {
    let mut out = serde_json::Map::new();
    if !cell.scale.is_nan() {
        out.insert("scale".into(), json_num(cell.scale));
    }
    if let Some(t) = &cell.table {
        out.insert("table".into(), Value::from(t.as_str()));
    }
    if let Some(p) = cell.per_target {
        out.insert("perTarget".into(), json_num(p));
    }
    if shape == Shape::AxisList {
        if let Some(k) = &cell.stack_key {
            out.insert("stackKey".into(), Value::from(k.as_str()));
        }
        if cell.suppressible {
            out.insert("suppressible".into(), Value::Bool(true));
        }
        if cell.ignore_strength {
            out.insert("ignoreStrength".into(), Value::Bool(true));
        }
    }
    // The reader only ever sees "Self" here; see `cell_of_json`.
    if applier == "resistanceSelfDebuffValue" {
        out.insert("toWho".into(), Value::from("Self"));
    }
    out
}

fn json_of_answer(answer: &Answer, shape: Shape, applier: &str) -> Value {
    match (answer, shape) {
        (Answer::Absent, _) => Value::Null,
        (Answer::One(c), _) => Value::Object(json_of_cell(c, shape, applier)),
        (Answer::Keyed(list), Shape::AxisList) => Value::Array(
            list.iter()
                .map(|(axis, c)| {
                    let mut m = serde_json::Map::new();
                    m.insert("axis".into(), Value::from(axis.as_str()));
                    m.extend(json_of_cell(c, shape, applier));
                    Value::Object(m)
                })
                .collect(),
        ),
        (Answer::Keyed(list), _) => Value::Object(
            list.iter()
                .map(|(k, c)| (k.clone(), Value::Object(json_of_cell(c, shape, applier))))
                .collect(),
        ),
    }
}

/// `COH_WRITE_ORACLE=1`: bring this fork's records up to the current engine after a game patch.
///
/// The records are living, not frozen: when the server patches, the powers it changed get new
/// answers and the powers it added get their first. Every line whose answer still agrees is
/// kept byte-for-byte, so the git diff is exactly the set of answers that moved — read it and
/// attribute each one to the patch notes before committing, the same ruling `baseline:write`
/// asks for. Files are rebuilt in walk order, which is the order they were recorded in.
fn rewrite(dataset: DatasetId) {
    let db = load(dataset);
    let powers = keyed_powers(&db);
    let mut files: Vec<&str> = APPLIERS.iter().map(|a| a.file).collect();
    files.dedup();
    for file in files {
        let path = repo()
            .join("fixtures/oracle")
            .join(dataset.as_str())
            .join(format!("{file}.jsonl"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let mut lines: BTreeMap<(String, String), String> = BTreeMap::new();
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let record: Value = serde_json::from_str(line).expect("fixture line");
            let applier = record["applier"].as_str().expect("applier").to_string();
            let key = record["key"].as_str().expect("key").to_string();
            lines.insert((applier, key), line.to_string());
        }
        let mut out = String::new();
        for (key, power) in &powers {
            if is_unrecorded_by_construction(key) {
                continue;
            }
            for applier in APPLIERS.iter().filter(|a| a.file == file) {
                let ours = (applier.read)(&power.as_bag_view());
                let kept = lines
                    .get(&(applier.recorded.to_string(), key.clone()))
                    .filter(|line| {
                        let record: Value = serde_json::from_str(line).expect("fixture line");
                        let theirs = answer_of_json(&record["value"], applier.shape, key);
                        same_answer(&ours, &theirs)
                            || KNOWN_DIVERGENCE.contains(&(applier.recorded, key.as_str()))
                    });
                match kept {
                    Some(line) => out.push_str(line),
                    None => {
                        let mut record = serde_json::Map::new();
                        record.insert("key".into(), Value::from(key.as_str()));
                        record.insert("applier".into(), Value::from(applier.recorded));
                        record.insert(
                            "value".into(),
                            json_of_answer(&ours, applier.shape, applier.recorded),
                        );
                        out.push_str(&Value::Object(record).to_string());
                    }
                }
                out.push('\n');
            }
        }
        std::fs::write(&path, out).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
        println!("  rewrote {}", path.display());
    }
}

// ---------------------------------------------------------------- the run

fn grade(dataset: DatasetId) {
    if std::env::var_os("COH_WRITE_ORACLE").is_some() {
        rewrite(dataset);
    }
    let db = load(dataset);
    let powers = keyed_powers(&db);
    let mut failures: Vec<String> = Vec::new();

    let mut files: BTreeMap<&str, BTreeMap<String, BTreeMap<String, Value>>> = BTreeMap::new();
    for applier in APPLIERS {
        files
            .entry(applier.file)
            .or_insert_with(|| recorded_file(dataset, applier.file));
    }

    println!(
        "\n=== {} — {} powers walked",
        dataset.as_str(),
        powers.len()
    );

    for applier in APPLIERS {
        let recorded = files[applier.file]
            .get(applier.recorded)
            .unwrap_or_else(|| {
                panic!(
                    "{}: fixtures/oracle/{}/{}.jsonl records no applier named {:?}",
                    dataset.as_str(),
                    dataset.as_str(),
                    applier.file,
                    applier.recorded
                )
            });

        // Key sets first. A drift in walk order shows up here as two unmatched sets, and
        // reporting it as a value bug instead would send the reader into the applier.
        let ours_keys: BTreeSet<&str> = powers.iter().map(|(k, _)| k.as_str()).collect();
        let theirs_keys: BTreeSet<&str> = recorded.keys().map(String::as_str).collect();
        let unexplained: Vec<&str> = ours_keys
            .difference(&theirs_keys)
            .copied()
            .filter(|k| !is_unrecorded_by_construction(k))
            .collect();
        let vanished: Vec<&str> = theirs_keys.difference(&ours_keys).copied().collect();
        let ungraded = ours_keys.difference(&theirs_keys).count() - unexplained.len();
        if !unexplained.is_empty() || !vanished.is_empty() {
            failures.push(format!(
                "{}/{}: the two walks disagree about which powers exist. {} bundle key(s) have \
                 no recorded answer and are not in {:?} (e.g. {:?}); {} recorded key(s) are not \
                 in the bundle at all (e.g. {:?})",
                dataset.as_str(),
                applier.recorded,
                unexplained.len(),
                UNRECORDED_SETS,
                unexplained.iter().take(10).collect::<Vec<_>>(),
                vanished.len(),
                vanished.iter().take(10).collect::<Vec<_>>(),
            ));
            continue;
        }

        let mut tally = Tally::default();
        let mut shown = 0usize;

        for (key, power) in &powers {
            // Only the two sets named above reach this, and the check above proved it.
            let Some(raw) = recorded.get(key.as_str()) else {
                continue;
            };
            let ours = (applier.read)(&power.as_bag_view());
            if !same_answer(&ours, &(applier.read)(power)) {
                tally.forked.push(key.clone());
            }
            let theirs = answer_of_json(raw, applier.shape, key);

            match (&ours, &theirs) {
                (Answer::Absent, Answer::Absent) => tally.absent += 1,
                (Answer::Keyed(a), Answer::Keyed(b)) if a.is_empty() && b.is_empty() => {
                    tally.empty += 1
                }
                _ if same_answer(&ours, &theirs) => tally.graded += 1,
                _ => {}
            }
            let mut agrees = same_answer(&ours, &theirs);

            // The one allowed difference, re-checked field by field rather than waved through:
            // drop the increment the record predates and everything else must still match.
            if !agrees
                && KNOWN_DIVERGENCE.contains(&(applier.recorded, key.as_str()))
                && states_per_target(&ours)
                && same_answer(&without_per_target(&ours), &theirs)
            {
                tally.diverged_as_expected.push(key.clone());
                tally.graded += 1;
                agrees = true;
            }

            if !agrees {
                tally.mismatched += 1;
                if shown < SHOW_AT_MOST {
                    shown += 1;
                    println!(
                        "  MISMATCH {} {key}\n    recorded: {}\n    ours:     {}",
                        applier.recorded,
                        describe(&theirs),
                        describe(&ours),
                    );
                }
            }
        }

        println!(
            "  {:<30} {} recorded, {ungraded} ungraded, graded {} of {}  (absent {}, empty {})",
            applier.recorded,
            recorded.len(),
            tally.graded,
            tally.graded + tally.mismatched,
            tally.absent,
            tally.empty,
        );
        if !tally.forked.is_empty() {
            println!(
                "    {} archetype-forked, graded fork-free only: {}",
                tally.forked.len(),
                tally.forked.join(", ")
            );
        }
        for key in &tally.diverged_as_expected {
            println!("    known divergence, allowed: {key}");
        }
        if tally.mismatched > 0 {
            failures.push(format!(
                "{}/{}: {} of {} recorded powers disagree",
                dataset.as_str(),
                applier.recorded,
                tally.mismatched,
                recorded.len()
            ));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

// One test per fork, not one test, because the frozen key expires per fork: when a server
// patches, that fork's records stale and the other three stay valid. A single test would make
// the first patch red the whole gate and hide the three keys still worth something.
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
