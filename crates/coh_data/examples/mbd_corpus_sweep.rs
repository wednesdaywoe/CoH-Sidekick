//! The `.mbd` reader against a corpus three orders of magnitude larger than the one that closed it.
//!
//! The import gate grades eight files Mids itself wrote, and those
//! eight opened nine register rows in two days (MBDIMPORT-5 through -13). The gates that stand on
//! them are pinned to that roster by design — the corpus IS the assertion, so a ninth file reds
//! them. That makes the suite exact and makes it narrow, and this asks the other question: over a
//! few thousand builds nobody curated, what does the reader still not account for?
//!
//! **Nothing here is a gate.** A sweep over files the repo does not own cannot assert a count
//! without pinning the tree it walks, so this reports and the reader's own reconciliation does
//! the judging: [`MbdSummary::reconciles`] is the property MBDIMPORT-5 exists to make true, and a
//! file that breaks it is a defect whatever the corpus. Everything else printed is a population
//! to adjudicate, not a failure — a declined power may be a build naming a set this fork retired,
//! which is a real outcome and not a bug.
//!
//! Run: `cargo run -p coh_data --release --features census-probe --example mbd_corpus_sweep -- <dir>`

use coh_data::mbd_import::to_skif_build;
use coh_data::skif::{self, SkifFile};
use coh_data::{mbd, DatasetId, PowerDatabase};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn load(dataset: DatasetId) -> Option<PowerDatabase> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).ok()?;
    PowerDatabase::from_gz_bytes(&bytes).ok()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mbd"))
        {
            out.push(p);
        }
    }
}

#[derive(Default)]
struct Tally {
    files: usize,
    unreadable: usize,
    not_mbd_document: usize,
    parse_failed: usize,
    no_dataset: usize,
    no_bundle: usize,
    converted: usize,
    hydrate_failed: usize,
    non_reconciling: usize,
    powers_declined: usize,
    enhancements_in_file: usize,
    enhancements_imported: usize,
    enhancements_failed: usize,
    slots_in_file: usize,
    slots_imported: usize,
    over_budget_at_cap: usize,
    raised_from_floor: usize,
    excess_over_budget: usize,
}

fn main() {
    let dir = std::env::args()
        .nth(1)
        .expect("usage: mbd_corpus_sweep <dir>");
    let mut files = Vec::new();
    walk(Path::new(&dir), &mut files);
    files.sort();

    let mut dbs: BTreeMap<DatasetId, Option<PowerDatabase>> = BTreeMap::new();
    let mut t = Tally::default();
    let mut by_dataset: BTreeMap<String, usize> = BTreeMap::new();
    let mut parse_errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut hydrate_errors: BTreeMap<String, usize> = BTreeMap::new();
    let mut notes: BTreeMap<String, usize> = BTreeMap::new();
    // Which THING each note fired on, not just how often. A population is only adjudicable if
    // you can see whether it is one repeated case or a spread.
    let mut booster_ctx: BTreeMap<String, usize> = BTreeMap::new();
    let mut powerset_ctx: BTreeMap<String, usize> = BTreeMap::new();
    let mut nopower_ctx: BTreeMap<String, usize> = BTreeMap::new();
    let mut broken: Vec<String> = Vec::new();

    t.files = files.len();
    for path in &files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string();
        let Ok(text) = std::fs::read_to_string(path) else {
            t.unreadable += 1;
            continue;
        };
        if !mbd::is_mbd_document(&text) {
            t.not_mbd_document += 1;
            continue;
        }
        let file = match mbd::from_str(&text) {
            Ok(f) => f,
            Err(e) => {
                t.parse_failed += 1;
                *parse_errors
                    .entry(format!("{e}").chars().take(70).collect())
                    .or_default() += 1;
                continue;
            }
        };
        let Some(dataset) = mbd::probe_dataset(&text) else {
            t.no_dataset += 1;
            continue;
        };
        *by_dataset.entry(dataset.as_str().to_string()).or_default() += 1;
        let db = dbs.entry(dataset).or_insert_with(|| load(dataset));
        let Some(db) = db.as_ref() else {
            t.no_bundle += 1;
            continue;
        };

        let converted = to_skif_build(&file, db, dataset);
        t.converted += 1;
        let s = converted.summary;
        t.powers_declined += s.powers_declined;
        t.enhancements_in_file += s.enhancements_in_file;
        t.enhancements_imported += s.enhancements_imported;
        t.enhancements_failed += s.enhancements_failed;
        t.slots_in_file += s.slots_in_file;
        t.slots_imported += s.slots_imported;
        if !s.reconciles() {
            t.non_reconciling += 1;
            broken.push(format!(
                "{name}: in_file={} imported={} failed={} (off by {})",
                s.enhancements_in_file,
                s.enhancements_imported,
                s.enhancements_failed,
                s.enhancements_in_file as i64
                    - (s.enhancements_imported as i64 + s.enhancements_failed as i64)
            ));
        }
        if converted.level.over_budget_at_cap {
            t.over_budget_at_cap += 1;
        }
        if converted.level.raised_from_floor {
            t.raised_from_floor += 1;
        }
        t.excess_over_budget += converted.level.excess_over_server_budget;
        for n in &converted.unresolved {
            *notes
                .entry(n.detail.chars().take(80).collect())
                .or_default() += 1;
            if n.detail.contains("past its two archetype sets") {
                *powerset_ctx.entry(n.context.clone()).or_default() += 1;
            }
        }

        match skif::hydrate(
            SkifFile {
                version: skif::VERSION,
                authored_against: None,
                meta: None,
                build: converted.build,
            },
            db,
        ) {
            Ok(decoded) => {
                for n in &decoded.unresolved {
                    *notes
                        .entry(format!("[hydrate] {}", n.detail).chars().take(80).collect())
                        .or_default() += 1;
                    if n.detail.contains("booster on a piece that cannot take one") {
                        *booster_ctx.entry(n.context.clone()).or_default() += 1;
                    }
                    if n.detail.contains("carries no such power") {
                        *nopower_ctx.entry(n.context.clone()).or_default() += 1;
                    }
                }
            }
            Err(e) => {
                t.hydrate_failed += 1;
                *hydrate_errors
                    .entry(format!("{e}").chars().take(70).collect())
                    .or_default() += 1;
            }
        }
    }

    println!("\n================ .mbd corpus sweep ================");
    println!("dir: {dir}");
    println!("files found ................ {}", t.files);
    println!("  unreadable ............... {}", t.unreadable);
    println!("  not an .mbd document ..... {}", t.not_mbd_document);
    println!("  parse failed ............. {}", t.parse_failed);
    println!("  no dataset probed ........ {}", t.no_dataset);
    println!("  no bundle for dataset .... {}", t.no_bundle);
    println!("  converted ................ {}", t.converted);
    println!("  hydrate failed ........... {}", t.hydrate_failed);
    println!("\nby dataset: {by_dataset:?}");
    println!("\n--- the property MBDIMPORT-5 makes true ---");
    println!(
        "files whose enhancements do NOT reconcile: {}",
        t.non_reconciling
    );
    for b in broken.iter().take(20) {
        println!("    {b}");
    }
    if broken.len() > 20 {
        println!("    ... and {} more", broken.len() - 20);
    }
    println!("\n--- populations to adjudicate ---");
    println!("powers declined ............ {}", t.powers_declined);
    println!(
        "slots      in file/imported  {} / {}",
        t.slots_in_file, t.slots_imported
    );
    println!(
        "enhancements in file/imported/failed  {} / {} / {}  ({:.3}% failed)",
        t.enhancements_in_file,
        t.enhancements_imported,
        t.enhancements_failed,
        if t.enhancements_in_file == 0 {
            0.0
        } else {
            t.enhancements_failed as f64 * 100.0 / t.enhancements_in_file as f64
        }
    );
    println!("level raised from floor .... {}", t.raised_from_floor);
    println!("over budget at cap ......... {}", t.over_budget_at_cap);
    println!("excess slots over budget ... {}", t.excess_over_budget);
    if !parse_errors.is_empty() {
        println!("\n--- parse errors ---");
        let mut v: Vec<_> = parse_errors.iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        for (e, c) in v.iter().take(10) {
            println!("  {c:5}  {e}");
        }
    }
    if !hydrate_errors.is_empty() {
        println!("\n--- hydrate errors ---");
        let mut v: Vec<_> = hydrate_errors.iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        for (e, c) in v.iter().take(10) {
            println!("  {c:5}  {e}");
        }
    }
    println!("\n--- top unresolved notes ---");
    let mut v: Vec<_> = notes.iter().collect();
    v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
    for (n, c) in v.iter().take(30) {
        println!("  {c:5}  {n}");
    }
    println!("distinct notes: {}", notes.len());
    for (label, map) in [
        ("+5 booster refused, by piece", &booster_ctx),
        (
            "powerset past the two archetype sets, by path",
            &powerset_ctx,
        ),
        ("no such power, by context", &nopower_ctx),
    ] {
        if map.is_empty() {
            continue;
        }
        println!("\n--- {label} ({} distinct) ---", map.len());
        let mut v: Vec<_> = map.iter().collect();
        v.sort_by_key(|(_, c)| std::cmp::Reverse(**c));
        for (k, c) in v.iter().take(20) {
            println!("  {c:5}  {k}");
        }
    }
    println!("==================================================\n");
}
