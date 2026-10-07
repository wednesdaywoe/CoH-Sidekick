//! The living baseline: `recalculate` against **our own last recorded answer**, per fork.
//!
//! WHAT THIS IS, AND WHAT IT IS NOT. `totals_replay.rs` grades this calculator against the
//! frozen TS `globalBonuses` dumps in `fixtures/totals/`. Those are an ORACLE — an independent
//! statement of what a different implementation answered — and they are a ONE-TIME asset. They
//! were computed from the export that was on disk the day they were recorded, and the
//! consequence is plain: after the next patch the oracle and the data it was
//! computed from no longer agree, and no measurement afterwards can separate "the game changed"
//! from "we broke it". The window is open now and closes on patch day.
//!
//! This file is what is left standing on the other side of that day. It records what THIS
//! calculator answers, commits it, and compares every later run against it. That is a CHANGE
//! DETECTOR and not an oracle, and the distinction is the whole reason `defdiff.py`'s header
//! exists: a baseline regenerated from our own output catches drift and structurally cannot
//! catch a shared mistake. It is worth having anyway, for the reason a ratchet is worth having —
//! it makes every future movement of every number VISIBLE and adjudicated, instead of arriving
//! in a bug report.
//!
//! THE LOOP IT EXISTS FOR:
//!
//!   1. `npm run audit:shard-drift` says a fork's `.pigg` archives moved — the game patched.
//!   2. Re-export that fork, `npm run regen`, `npm run emit:contract`.
//!   3. `npm run baseline:check` goes red and prints every field that moved, per build.
//!   4. Rule each one GAME CHANGE or BUG. Fix the bugs.
//!   5. `npm run baseline:write` to record the adjudicated answer, and commit it.
//!
//! Step 3 is the only step this file performs; steps 4 and 5 are a person's. Nothing here
//! decides that a difference is acceptable, and that is deliberate — a gate that re-based itself
//! on new data would report a comfortable green while the thing it measures had changed
//! underneath, the failure this repository keeps finding in its own guards.
//!
//! WHICH INPUT MOVED IS RECORDED, BECAUSE IT IS THE FIRST QUESTION. A value that changed while
//! both inputs are byte-identical is a CODE change and has no data excuse; a value that changed
//! after a re-export is a candidate game change. The baseline therefore stamps both of the
//! calculator's inputs — `contract/<fork>/bundle.json.gz`, which is the entire power database
//! `recalculate` reads, and `fixtures/totals/<fork>/synthetic.jsonl`, which is the build corpus
//! replayed — and the failure message says which of the two moved before it prints a single
//! number. Same two-way idiom as `audit-atom-coverage.cjs`, whose `--write-baseline` is the
//! model this stage was specified against: one failure means a regression, the other means the
//! denominator changed and the old figure is not comparable.
//!
//! THE STAMP IS FNV-1a-64, NOT A CRYPTOGRAPHIC HASH, and the difference matters enough to name.
//! It answers "is this the same file" and nothing else. It does not answer "did someone forge
//! this file", which is `tools/export-integrity.py`'s question and is already asked there over
//! the same tree. A 64-bit non-cryptographic hash is chosen over pulling `sha2` into a crate
//! whose `Cargo.toml` carries a paragraph about not keeping dependencies it does not need.
//!
//! WHAT IT GRADES THAT THE FROZEN ORACLE CANNOT, measured rather than estimated. This records
//! the WHOLE `GlobalBonuses` (91 numeric fields plus the `errors` channel) and the WHOLE
//! `CharacterStats` (58 members, 62 leaf numbers — four of them are `ProjectedMovement`
//! structs carrying a value and its cap). **153 numbers per build, 21,879 over the corpus.**
//!
//! The frozen oracle reaches **84** of those 153: its 86 recorded names less the two that reach
//! no field at all. So **69 numbers per build are beyond it**, and the widening is not nominal —
//! **63 of the 69 take a non-zero value on at least one build**. Nineteen of the 153 are zero on
//! every build in the corpus; those are graded in one direction only, since the diff can fire
//! when one becomes non-zero and not otherwise.
//!
//! The seven `GlobalBonuses` fields the frozen corpus never states: `debuff_resist_accuracy`,
//! `debuff_resist_range`, `knockback_strength`, `mez_resist_repel`, `mez_resist_teleport`,
//! `movement_control`, `movement_friction`. The other sixty-two are the whole of `CharacterStats` —
//! the capped combine-by-max projection `totals_replay.rs` leaves ungraded on purpose, because
//! four of the recorded `stats` names reach no `CharacterStats::get` arm. **No bridge is needed
//! here.** Both sides are our own struct, serialized, so there is no camelCase name map to
//! hand-write and nothing to drift — and the four unbridgeable names are unbridgeable precisely
//! because they are not `f64` at all, which serializing does not care about.
//!
//! It is still change detection and not correctness. A wrong number recorded today is a wrong
//! number defended tomorrow. What says these numbers were right on the day the baseline was
//! first written is `totals_replay.rs`, grading the same builds against the frozen oracle.
//!
//! A `ProjectedMovement` is diffed as a WHOLE OBJECT rather than per member, so a moved cap
//! prints both numbers. Deliberate, and only tolerable because the struct has two fields: the
//! alternative is a recursive path walker whose output for a two-field struct is the same
//! information in more lines.
//!
//! WHAT THAT IS WORTH, BY PERTURBATION. Reddened builds out of 143, each measured against the
//! frozen replay in the same run so the widening is a measurement and not a claim:
//!
//! | Perturbation | This file | `totals_replay.rs` |
//! | --- | --- | --- |
//! | `toggle_end_cost` + 0.001 | **143** (74 values on homecoming alone) | 143 |
//! | the run-speed projection + 0.001 | **143** | **0** |
//! | a new `f64` field on `CharacterStats` | **143**, named as a SHAPE change | 0 |
//! | the E/N resistance combine `max` → `min` | **3** | **0** |
//! | the S/L defense combine `max` → `min` | 0 | 0 |
//!
//! The last row is a property of the corpus and not of this file, and it is worth stating
//! because it would otherwise read as coverage that exists. Five of the six combine-by-max pairs
//! are EQUAL on every build that states them — S/L defense on 22 builds, F/C on 25, E/N defense
//! on 26, S/L resistance on 25, F/C resistance on 21, all identical within the pair — so `max`,
//! `min` and "take the first" all coincide and no perturbation of the fold can be seen. Only
//! `resistance_energy` vs `resistance_negative` differs, on 3 builds, and that is the one axis
//! the fold is graded on at all. Writing a build whose paired typed defenses are unequal is the
//! cheapest coverage available anywhere in this corpus.
//!
//! WHY THE BUILDS ARE NOT RE-PARSED STRICTLY HERE. `totals_replay.rs` holds the strict reader:
//! exact field sets, and a deserialize/re-serialize round trip requiring every recorded value to
//! come back unchanged. Copying it would be a second copy free to drift from the first. This
//! file pins the corpus a different and independent way — the byte stamp above — so a build that
//! is edited, added or removed moves `buildsFnv1a64` and this gate says the corpus changed
//! whether or not the totals moved with it. The two guards do not depend on each other, which
//! matters because the frozen half is the half with an expiry date.
//!
//! NON-FINITE VALUES ARE A HARD FAILURE IN BOTH MODES. `serde_json` writes NaN and the
//! infinities as `null`, so a baseline could record one and then match it forever — two
//! different NaNs both read back as the same `null`. Any `null` in a serialized `GlobalBonuses`
//! or `CharacterStats` is therefore refused at the point it is produced. `CalcError`'s two
//! fields are both `Box<str>`, so no legitimate null exists anywhere in either struct.
//!
//! THE WRITER REFUSES A BUILD THAT ERRORED. `GlobalBonuses::errors` collects contributions the
//! interpreter met and could not derive. Recording a baseline over one would freeze a known gap
//! as the expected answer, which is precisely what stage 6's open item was closed to avoid. The
//! writer names the builds and exits without writing; the checker reports errors as an ordinary
//! diff, because a NEW error appearing is exactly the drift this file is for.
//!
//! EVERY GUARD IN THIS FILE WAS PROVED ABLE TO FIRE, because a green check nobody has seen
//! refuse anything is a wish. Beside the five value perturbations above: tampering the recorded
//! `bundleFnv1a64` reports the power database MOVED with no value differences at all, tampering
//! `buildsFnv1a64` reports the corpus moved, deleting one baseline row reports the count
//! mismatch and then names where the misalignment STARTS and counts the remaining 19, injecting
//! a NaN into `stats.resistance_cap` is refused as non-finite on all 37 homecoming builds
//! before any comparison happens — the first attempt put it in the S/L defense combine instead
//! and the baseline recorded `-100.0`, because `f64::max` returns the other operand when one
//! side is NaN and a downstream clamp had already swallowed it, so a NaN reaches this guard
//! only where nothing clamps behind it — pushing a `CalcError` makes the WRITER refuse rather than
//! record, and moving a baseline file aside produces the message that says to create it. Two
//! consecutive write runs are byte-identical, checked three times including once through the
//! npm script.
//!
//! Run:  `npm run baseline:check`            (or `cargo nextest run -p coh_math`)
//! Write: `npm run baseline:write`           (shard-drift preflight, then this file in write mode)

use coh_data::{CharacterState, DatasetId, PowerDatabase};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// Set to a non-empty, non-`0` value to RECORD the baseline instead of checking it. Named with
/// the repository's `COH_` prefix, like `COH_RAW_DEFS`. The npm script sets it inline for one
/// command so it cannot be left switched on in a shell.
const WRITE_ENV: &str = "COH_WRITE_BASELINE";

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn writing() -> bool {
    std::env::var(WRITE_ENV).is_ok_and(|v| !v.is_empty() && v != "0")
}

/// The self-describing note written into every baseline file, the idiom
/// `scripts/audit-atom-coverage-baseline.json` established: a data file in this tree says what
/// it is and how it is refreshed, so a reader who finds it first does not have to find its
/// consumer to understand it.
const NOTE: &str = "\
This is NOT an oracle. It is this repository's own calculator answering its own build corpus, \
recorded so that every later run can be diffed against it. It catches DRIFT and structurally \
cannot catch a shared mistake -- what says these numbers were right on the day this file was \
first written is crates/coh_math/tests/totals_replay.rs, which grades the same builds against \
the frozen TS oracle in fixtures/totals/. Checked by crates/coh_math/tests/totals_baseline.rs \
on every `cargo nextest run`; rewritten by `npm run baseline:write` AFTER a person has ruled \
each difference a game change or a bug. `inputs` stamps the two things the calculator read: if \
they are unchanged and a number moved, the cause is code and there is no data excuse.";

// ---------------------------------------------------------------- the input stamp

/// FNV-1a 64. Six lines and no dependency, stable across toolchains in a way
/// `std::collections::hash_map::DefaultHasher` explicitly is not — its docs reserve the right to
/// change the algorithm between releases, which would make every committed baseline read as a
/// moved input on a version bump.
fn fnv1a64(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// One of the calculator's two inputs, identified by path and content.
struct Stamp {
    path: String,
    hash: String,
    bytes: usize,
}

fn stamp(relative: &str) -> (Vec<u8>, Stamp) {
    let absolute = repo().join(relative);
    let bytes = std::fs::read(&absolute).unwrap_or_else(|e| panic!("read {absolute:?}: {e}"));
    let stamp = Stamp {
        path: relative.to_string(),
        hash: fnv1a64(&bytes),
        bytes: bytes.len(),
    };
    (bytes, stamp)
}

fn stamps_as_json(bundle: &Stamp, builds: &Stamp) -> Value {
    serde_json::json!({
        "bundle": bundle.path,
        "bundleFnv1a64": bundle.hash,
        "bundleBytes": bundle.bytes,
        "builds": builds.path,
        "buildsFnv1a64": builds.hash,
        "buildsBytes": builds.bytes,
    })
}

// ---------------------------------------------------------------- our side

/// One replayed build as the baseline records it: which line of the corpus it came from, the
/// emitter's probe name, and both output structs whole.
fn row(line: usize, name: &str, bonuses: &Value, stats: &Value) -> Value {
    serde_json::json!({
        "line": line,
        "name": name,
        "bonuses": bonuses,
        "stats": stats,
    })
}

/// Every path inside a serialized struct holding `null`, which is how `serde_json` writes NaN
/// and the infinities. See the header: a recorded `null` would match forever.
fn non_finite(value: &Value, path: &str, found: &mut Vec<String>) {
    match value {
        Value::Null => found.push(path.to_string()),
        Value::Object(map) => {
            for (key, child) in map {
                non_finite(child, &format!("{path}.{key}"), found);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                non_finite(child, &format!("{path}[{i}]"), found);
            }
        }
        _ => {}
    }
}

fn object_of(value: &Value, what: &str) -> Map<String, Value> {
    match value {
        Value::Object(map) => map.clone(),
        other => panic!("{what} did not serialize as an object: {other}"),
    }
}

// ---------------------------------------------------------------- the diff

/// What changed between one recorded struct and the one produced now, in three kinds. The kinds
/// are kept apart because they mean different things: a value that moved is a number to
/// adjudicate, while a field that appeared or vanished is the STRUCT changing shape, which is
/// always a code change and never a game change.
#[derive(Default)]
struct Diff {
    moved: Vec<String>,
    added: Vec<String>,
    removed: Vec<String>,
}

impl Diff {
    fn is_empty(&self) -> bool {
        self.moved.is_empty() && self.added.is_empty() && self.removed.is_empty()
    }
    fn shape_changed(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty()
    }
}

fn diff_into(was: &Map<String, Value>, now: &Map<String, Value>, block: &str, out: &mut Diff) {
    for (field, now_value) in now {
        match was.get(field) {
            None => out.added.push(format!("{block}.{field} = {now_value}")),
            Some(was_value) if was_value != now_value => out.moved.push(format!(
                "{block}.{field}: baseline {was_value}, now {now_value}"
            )),
            Some(_) => {}
        }
    }
    for field in was.keys() {
        if !now.contains_key(field) {
            out.removed
                .push(format!("{block}.{field} (baseline held {})", was[field]));
        }
    }
}

// ---------------------------------------------------------------- the run

/// How many misaligned rows the report names before it starts counting instead. See the use
/// site: one inserted build shifts every later row, and 140 copies of one fact is not a report.
const MISALIGNED_SHOWN: usize = 5;

fn baseline_path(dataset: DatasetId) -> PathBuf {
    repo()
        .join("baseline")
        .join(format!("totals-{}.json", dataset.as_str()))
}

fn grade(dataset: DatasetId) {
    let fork = dataset.as_str();
    let (bundle_bytes, bundle) = stamp(&format!("contract/{fork}/bundle.json.gz"));
    let (corpus_bytes, builds) = stamp(&format!("fixtures/totals/{fork}/synthetic.jsonl"));

    let db = PowerDatabase::from_gz_bytes(&bundle_bytes)
        .unwrap_or_else(|e| panic!("load {fork} bundle: {e}"));

    // Replay every build in the corpus. The strict reader is `totals_replay.rs`'s; see the
    // header for why it is not copied and what pins the corpus here instead.
    let corpus = String::from_utf8(corpus_bytes).unwrap_or_else(|e| panic!("{fork} corpus: {e}"));
    let mut rows: Vec<Value> = Vec::new();
    let mut errored: Vec<String> = Vec::new();
    let mut nulls: Vec<String> = Vec::new();
    for (index, line) in corpus.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_no = index + 1;
        let where_ = format!("fixtures/totals/{fork}/synthetic.jsonl:{line_no}");
        let record: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("{where_}: not JSON: {e}"));
        let state: CharacterState = serde_json::from_value(record["build"].clone())
            .unwrap_or_else(|e| panic!("{where_}: build is not a CharacterState: {e}"));

        let result = coh_math::recalculate(&state, &db);
        if !result.bonuses.errors.is_empty() {
            for e in &result.bonuses.errors {
                errored.push(format!(
                    "line {line_no} ({}): {} — {}",
                    state.name, e.context, e.detail
                ));
            }
        }
        let bonuses = serde_json::to_value(&result.bonuses).expect("GlobalBonuses serializes");
        let stats = serde_json::to_value(&result.stats).expect("CharacterStats serializes");
        non_finite(&bonuses, &format!("line {line_no}.bonuses"), &mut nulls);
        non_finite(&stats, &format!("line {line_no}.stats"), &mut nulls);
        rows.push(row(line_no, &state.name, &bonuses, &stats));
    }
    assert!(
        !rows.is_empty(),
        "{fork}: the build corpus holds no records"
    );
    assert!(
        nulls.is_empty(),
        "{fork}: {} value(s) are not finite, and serde_json writes NaN and the infinities as \
         `null` — a baseline that recorded one would match it forever:\n  {}",
        nulls.len(),
        nulls.join("\n  ")
    );

    let path = baseline_path(dataset);
    let shown = path
        .strip_prefix(repo())
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());

    if writing() {
        assert!(
            errored.is_empty(),
            "{fork}: {} build(s) produced calculator errors, and recording a baseline over one \
             would freeze a known gap as the expected answer. Fix them, then write:\n  {}",
            errored.len(),
            errored.join("\n  ")
        );
        let document = serde_json::json!({
            "_note": NOTE,
            "dataset": fork,
            "inputs": stamps_as_json(&bundle, &builds),
            "builds": rows,
        });
        std::fs::create_dir_all(path.parent().expect("baseline path has a parent"))
            .unwrap_or_else(|e| panic!("create baseline dir: {e}"));
        let text = serde_json::to_string_pretty(&document).expect("baseline serializes") + "\n";
        std::fs::write(&path, &text).unwrap_or_else(|e| panic!("write {path:?}: {e}"));
        println!(
            "\n  WROTE {shown} — {} builds, {} bytes.\n  \
             This file now says these answers are correct. Nothing checked that; a person did, \
             or should have.",
            rows.len(),
            text.len()
        );
        return;
    }

    let Ok(text) = std::fs::read_to_string(&path) else {
        panic!(
            "\n{fork}: no baseline at {shown}. This is the living baseline — the calculator's own \
             recorded answers, which every later run is diffed against. Create it with \
             `npm run baseline:write`, read what it wrote, and commit it."
        );
    };
    let recorded: Value =
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{shown}: not JSON: {e}"));

    let mut failures: Vec<String> = Vec::new();

    // Which of the calculator's two inputs moved. Printed before any number, because it decides
    // what the numbers below can possibly mean.
    let was = &recorded["inputs"];
    // `as_str()` rather than a `Value` comparison so a baseline whose stamp is missing or is
    // not a string reads as MOVED (`None != Some`), which is the safe reading: a baseline that
    // cannot say what it was computed from has not said it matches.
    let bundle_moved = was["bundleFnv1a64"].as_str() != Some(bundle.hash.as_str());
    let builds_moved = was["buildsFnv1a64"].as_str() != Some(builds.hash.as_str());
    let describe = |label: &str, moved: bool, was_hash: &Value, now: &Stamp| -> String {
        if moved {
            format!(
                "  {label:<18} MOVED     {} ({} -> {})",
                now.path,
                was_hash.as_str().unwrap_or("?"),
                now.hash
            )
        } else {
            format!("  {label:<18} unchanged {} ({})", now.path, now.hash)
        }
    };

    let recorded_rows = recorded["builds"]
        .as_array()
        .unwrap_or_else(|| panic!("{shown}: `builds` is not an array"));
    if recorded_rows.len() != rows.len() {
        failures.push(format!(
            "the baseline holds {} builds and the corpus now holds {}",
            recorded_rows.len(),
            rows.len()
        ));
    }

    let mut diff = Diff::default();
    let mut changed_builds = 0usize;
    let mut misaligned = 0usize;
    println!("\n=== {fork} — {} builds against {shown}", rows.len());
    for (was_row, now_row) in recorded_rows.iter().zip(&rows) {
        let mut one = Diff::default();
        if was_row["line"] != now_row["line"] || was_row["name"] != now_row["name"] {
            // Capped, because a corpus with one line inserted misaligns every row after it and
            // would otherwise print 140 variations of the same fact. The count line above is the
            // headline; these name where the drift starts.
            misaligned += 1;
            if misaligned <= MISALIGNED_SHOWN {
                failures.push(format!(
                    "baseline row {} {} is corpus row {} {} — the corpus was reordered or rewritten",
                    was_row["line"], was_row["name"], now_row["line"], now_row["name"]
                ));
            }
            continue;
        }
        for block in ["bonuses", "stats"] {
            diff_into(
                &object_of(&was_row[block], "recorded block"),
                &object_of(&now_row[block], "our block"),
                block,
                &mut one,
            );
        }
        if one.is_empty() {
            continue;
        }
        changed_builds += 1;
        println!("  CHANGED line {} — {}", now_row["line"], now_row["name"]);
        for line in one.moved.iter().chain(&one.added).chain(&one.removed) {
            println!("    {line}");
        }
        diff.moved.extend(one.moved);
        diff.added.extend(one.added);
        diff.removed.extend(one.removed);
    }

    if diff.is_empty() && failures.is_empty() && !bundle_moved && !builds_moved {
        println!("  baseline held — every field of every build is the recorded answer");
        return;
    }

    let mut report = vec![format!("\nBASELINE DIVERGED — {fork}")];
    report.push(describe(
        "the game data:",
        bundle_moved,
        &was["bundleFnv1a64"],
        &bundle,
    ));
    report.push(describe(
        "the build corpus:",
        builds_moved,
        &was["buildsFnv1a64"],
        &builds,
    ));
    report.push(String::new());
    match (bundle_moved, builds_moved) {
        (false, false) => report.push(
            "  Both inputs are byte-identical to the ones this baseline was recorded from, so \
             nothing outside this workspace's Rust changed the answer. Every difference below is \
             CODE. If it is an intended improvement, say so and rewrite the baseline; if it is \
             not, it is a regression."
                .to_string(),
        ),
        (true, _) => report.push(
            "  The power database MOVED — a re-export or a game patch landed. Every difference \
             below is a candidate GAME CHANGE and must be ruled game-or-bug one at a time. \
             `npm run audit:shard-drift` says whether the export itself is still current."
                .to_string(),
        ),
        (false, true) => report.push(
            "  The build corpus MOVED — a build was added, edited or removed. Differences below \
             belong to the builds that changed; a difference on a build that did NOT change is \
             still code."
                .to_string(),
        ),
    }
    if diff.shape_changed() {
        report.push(format!(
            "  {} field(s) appeared and {} vanished. That is the SHAPE of GlobalBonuses or \
             CharacterStats changing, which is always a code change and never a game change.",
            diff.added.len(),
            diff.removed.len()
        ));
    }
    if !diff.is_empty() {
        report.push(format!(
            "  {changed_builds} of {} builds differ, over {} field value(s). They are printed \
             above, per build.",
            rows.len(),
            diff.moved.len() + diff.added.len() + diff.removed.len()
        ));
    }
    for failure in &failures {
        report.push(format!("  !! {failure}"));
    }
    if misaligned > MISALIGNED_SHOWN {
        report.push(format!(
            "  !! ...and {} further row(s) misaligned, not printed: one inserted or deleted \
             build shifts every row after it, so the list says where the drift STARTS and not \
             how far it reaches.",
            misaligned - MISALIGNED_SHOWN
        ));
    }
    report.push(
        "\n  Rule each difference a GAME CHANGE or a BUG. Then `npm run baseline:write` and \
         commit the new baseline with the ruling in the message."
            .to_string(),
    );
    panic!("{}", report.join("\n"));
}

// One test per fork, for the reason `totals_replay.rs` and `oracle.rs` both give: a fork's data
// moves on its own patch day, so a fork's baseline diverges on its own.
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
