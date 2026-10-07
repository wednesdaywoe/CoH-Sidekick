//! The census behind MXDIMPORT-1: every display name a `.mxd` states that this dataset cannot
//! place, and what was left in the scope the reader searched for it.
//!
//! The row asks for one split — **which of these a JOIN could reach, and which only a TABLE
//! could** — and a join here can only be MBDIMPORT-16's: after the exact matches, one leftover
//! on each side at the same level is a forced pairing, never a chosen one. So what this prints
//! per name is the evidence that pairing would rest on: the sets the files declared, the level
//! they filed the pick at, and the powers of those sets no entry in the same file claimed.
//!
//! **Nothing here is a gate, and nothing here decides.** It prints what the corpus holds; the
//! adjudication is written into the register's row, pair by pair.
//!
//! Run: `cargo run -p coh_data --release --features census-probe --example mxd_name_census -- <dir>`

use coh_data::mxd;
use coh_data::mxd_import::{self, UnplacedEntry};
use coh_data::{DatasetId, PowerDatabase};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

fn load(dataset: DatasetId) -> Option<PowerDatabase> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    PowerDatabase::from_gz_bytes(&std::fs::read(&path).ok()?).ok()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("mxd"))
        {
            out.push(path);
        }
    }
}

/// Everything the corpus says about one display name it could not place.
#[derive(Default)]
struct Name {
    entries: usize,
    files: BTreeSet<String>,
    levels: BTreeSet<i32>,
    /// Sets the declaring files held, by our id.
    scopes: BTreeSet<String>,
    /// Leftovers at this entry's own level, as `set id :: our display`, and how often each was
    /// the ONLY one — which is the whole of the forced-pairing test.
    at_level: BTreeMap<String, usize>,
    alone_at_level: BTreeMap<String, usize>,
    /// Occurrences where the scope held no unclaimed power at all at that level.
    none_at_level: usize,
    /// The candidate join: powers of the scope whose INTERNAL name is this display name.
    /// `hits[n]` counts the occurrences that found exactly `n` of them.
    hits: BTreeMap<usize, usize>,
    /// Where it found exactly one, which power, and whether its level is the picked one.
    internal: BTreeMap<String, usize>,
    internal_level_agrees: usize,
    /// Enhancement codes the post seats in this entry, by how often.
    codes: BTreeMap<String, usize>,
    /// Occurrences in a file that declares a powerset path this dataset has no set for, by path.
    unresolved: BTreeMap<String, usize>,
}

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: mxd_name_census <dir>");
        std::process::exit(2);
    };
    let Some(database) = load(DatasetId::Homecoming) else {
        eprintln!("no Homecoming bundle to census against");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();

    let mut reported: BTreeMap<String, Name> = BTreeMap::new();
    let mut silent: BTreeMap<String, usize> = BTreeMap::new();
    let (mut reported_entries, mut silent_entries) = (0usize, 0usize);
    let mut unresolved: BTreeMap<String, usize> = BTreeMap::new();
    let mut contended: BTreeMap<String, usize> = BTreeMap::new();
    let mut collides = 0usize;

    for path in &files {
        let stem = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if !mxd::is_mxd_document(&text) {
            continue;
        }
        let Ok(file) = mxd::from_str(&text) else {
            continue;
        };
        let report = mxd_import::unplaced(&file, &database);
        for (name, took, rival) in &report.display_join_contended {
            *contended
                .entry(format!("{name:?} -> {took} (rival {rival})"))
                .or_default() += 1;
        }
        collides += report.internal_join_collides;
        for path in &report.unresolved_paths {
            *unresolved.entry(path.clone()).or_default() += 1;
        }
        for entry in report.entries {
            if !entry.reported {
                silent_entries += 1;
                *silent.entry(entry.display.clone()).or_default() += 1;
                continue;
            }
            reported_entries += 1;
            record(
                reported.entry(entry.display.clone()).or_default(),
                &entry,
                &stem,
            );
        }
    }

    println!("files walked: {}", files.len());
    println!(
        "reported: {reported_entries} entries in {} names",
        reported.len()
    );
    println!(
        "dropped on the untouched-row rule: {silent_entries} entries in {} names",
        silent.len()
    );
    println!(
        "entries the display join placed with an internal-name rival: {} in {} shapes",
        contended.values().sum::<usize>(),
        contended.len(),
    );
    let mut rival: Vec<_> = contended.iter().collect();
    rival.sort_by(|a, b| b.1.cmp(a.1));
    for (shape, count) in &rival {
        println!("    {count:>4}x  {shape}");
    }
    println!("entries whose internal-name claimant is already placed: {collides}");
    println!("powerset paths this dataset has no set for:");
    for (path, count) in &unresolved {
        println!("    {count}x  {path}");
    }
    println!();

    let mut order: Vec<_> = reported.iter().collect();
    order.sort_by(|a, b| b.1.entries.cmp(&a.1.entries).then(a.0.cmp(b.0)));
    for (display, name) in order {
        println!(
            "{display:?} — {} entries in {} files, at level(s) {}",
            name.entries,
            name.files.len(),
            name.levels
                .iter()
                .map(i32::to_string)
                .collect::<Vec<_>>()
                .join("/"),
        );
        for (path, count) in &name.unresolved {
            println!("  ROSTER GAP — the file declares {path}, which this dataset has no set for ({count}x)");
        }
        let joined: usize = name.hits.get(&1).copied().unwrap_or(0);
        println!(
            "  internal-name join: {joined} of {} land on one power, level agreeing on {}",
            name.entries, name.internal_level_agrees,
        );
        for (candidate, count) in &name.internal {
            println!("    -> {candidate} ({count}x)");
        }
        let mut codes: Vec<_> = name.codes.iter().collect();
        codes.sort_by(|a, b| b.1.cmp(a.1));
        println!(
            "  slotted: {}",
            codes
                .iter()
                .take(9)
                .map(|(code, count)| format!("{code}x{count}"))
                .collect::<Vec<_>>()
                .join(" "),
        );
        for (n, count) in name.hits.iter().filter(|(n, _)| **n != 1) {
            println!("    {count} occurrence(s) found {n} candidates");
        }
        println!(
            "  scopes: {}",
            name.scopes.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        if name.none_at_level > 0 {
            println!(
                "  no unclaimed power at that level: {} of {}",
                name.none_at_level, name.entries
            );
        }
        let mut alone: Vec<_> = name.alone_at_level.iter().collect();
        alone.sort_by(|a, b| b.1.cmp(a.1));
        for (candidate, count) in &alone {
            println!("  ALONE at that level ({count}×): {candidate}");
        }
        let mut shared: Vec<_> = name
            .at_level
            .iter()
            .filter(|(key, _)| !name.alone_at_level.contains_key(*key))
            .collect();
        shared.sort_by(|a, b| b.1.cmp(a.1));
        for (candidate, count) in shared.iter().take(6) {
            println!("  also free at that level ({count}×): {candidate}");
        }
        if shared.len() > 6 {
            println!("  … and {} more free at that level", shared.len() - 6);
        }
        println!();
    }

    println!("dropped silently, by name:");
    let mut quiet: Vec<_> = silent.iter().collect();
    quiet.sort_by(|a, b| b.1.cmp(a.1));
    for (display, count) in quiet.iter().take(20) {
        println!("  {display}: {count}");
    }
    println!("  … {} distinct in all", silent.len());
}

fn record(name: &mut Name, entry: &UnplacedEntry, file: &str) {
    name.entries += 1;
    name.files.insert(file.to_string());
    name.levels.insert(entry.level);
    name.scopes.extend(entry.scope.iter().cloned());
    for code in &entry.slotted {
        if code != "Empty" {
            *name
                .codes
                .entry(code.split('-').next().unwrap_or(code).to_string())
                .or_default() += 1;
        }
    }
    let level = u8::try_from(entry.level).unwrap_or(u8::MAX);
    let here: Vec<String> = entry
        .residual
        .iter()
        .filter(|(_, _, available)| *available == level)
        .map(|(set, display, _)| format!("{set} :: {display}"))
        .collect();
    if here.is_empty() {
        name.none_at_level += 1;
    }
    for candidate in &here {
        *name.at_level.entry(candidate.clone()).or_default() += 1;
    }
    if let [only] = here.as_slice() {
        *name.alone_at_level.entry(only.clone()).or_default() += 1;
    }
    for path in &entry.unresolved_paths {
        *name.unresolved.entry(path.clone()).or_default() += 1;
    }
    *name.hits.entry(entry.by_internal.len()).or_default() += 1;
    if let [(set, display, available)] = entry.by_internal.as_slice() {
        *name
            .internal
            .entry(format!("{set} :: {display} (lvl {available})"))
            .or_default() += 1;
        if i32::from(*available) == entry.level {
            name.internal_level_agrees += 1;
        }
    }
}
