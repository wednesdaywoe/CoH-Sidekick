//! The `.mxd` document reader against every posted file the corpus holds.
//!
//! This asks the one question the format's own shape makes answerable: **a `.mxd` carries the
//! same build twice, so how often do its two renderings disagree?** Every file is a free oracle
//! pair, and [`coh_data::mxd::pair`] refuses on disagreement — so a sweep that reads clean is
//! evidence the reader has both halves right, and a single disagreement is a place to look.
//!
//! **Nothing here is a gate.** A sweep over files the repo does not own cannot assert a count
//! without pinning the tree it walks. The gate runs over the fixtures this repo does
//! own; this is the wide net that tells that gate what to pin.
//!
//! Run: `cargo run -p coh_data --release --features census-probe --example mxd_corpus_sweep -- <dir>`

use coh_data::mbd_import;
use coh_data::mxd::{self, MxdError, MxdShape};
use coh_data::mxd_import::{self, MxdSummary};
use coh_data::{DatasetId, PowerDatabase};
use std::collections::BTreeMap;
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

fn shape_name(shape: MxdShape) -> String {
    format!(
        "middle {} / {} slot level / {} flipped",
        shape.entry_middle,
        if shape.wide_slot_level {
            "wide"
        } else {
            "narrow"
        },
        if shape.flipped_enhancement {
            "with"
        } else {
            "no"
        },
    )
}

fn main() {
    let Some(root) = std::env::args().nth(1) else {
        eprintln!("usage: mxd_corpus_sweep <dir>");
        std::process::exit(2);
    };
    let mut files = Vec::new();
    walk(Path::new(&root), &mut files);
    files.sort();

    let (mut read, mut unreadable, mut not_a_document, mut without_prose) = (0, 0, 0, 0);
    let (mut powers, mut slots, mut filled, mut flipped) = (0usize, 0usize, 0usize, 0usize);
    let mut by_version: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_shape: BTreeMap<String, usize> = BTreeMap::new();
    let mut shape_disagrees_with_version: Vec<String> = Vec::new();
    let mut refusals: Vec<(String, MxdError)> = Vec::new();
    // A `.mxd` names no fork, so the whole corpus is read against one. Homecoming is what the
    // posted builds are: every fork-specific set in them resolves there.
    let database = load(DatasetId::Homecoming);
    let mut totals = MxdSummary::default();
    let mut non_reconciling = 0usize;
    let mut unnamed_powers: BTreeMap<String, usize> = BTreeMap::new();
    let mut unnamed_pieces: BTreeMap<String, usize> = BTreeMap::new();
    let mut declined_by_mbd = 0usize;

    for path in &files {
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let Ok(text) = std::fs::read_to_string(path) else {
            unreadable += 1;
            continue;
        };
        if !mxd::is_mxd_document(&text) {
            not_a_document += 1;
            continue;
        }
        match mxd::from_str(&text) {
            Ok(file) => {
                read += 1;
                *by_version
                    .entry(format!("{:.2}", file.binary.format_version))
                    .or_default() += 1;
                *by_shape.entry(shape_name(file.binary.shape)).or_default() += 1;
                if MxdShape::for_version(file.binary.format_version)
                    .is_some_and(|named| named != file.binary.shape)
                {
                    shape_disagrees_with_version.push(name.clone());
                }
                if file.prose.is_none() {
                    without_prose += 1;
                }
                if let Some(database) = &database {
                    let reading = mxd_import::to_mbd_file(&file, database);
                    let s = reading.summary;
                    totals.powersets_in_file += s.powersets_in_file;
                    totals.powersets_resolved += s.powersets_resolved;
                    totals.powers_in_file += s.powers_in_file;
                    totals.powers_named += s.powers_named;
                    totals.powers_unnamed += s.powers_unnamed;
                    totals.enhancements_in_file += s.enhancements_in_file;
                    totals.enhancements_named += s.enhancements_named;
                    totals.enhancements_unnamed += s.enhancements_unnamed;
                    totals.by_code += s.by_code;
                    totals.by_code_and_index += s.by_code_and_index;
                    totals.by_index_within_set += s.by_index_within_set;
                    if !s.reconciles() {
                        non_reconciling += 1;
                    }
                    // Which bucket a note belongs in is a structural question, not a phrasing
                    // one: a power refusal's context IS a row's power name and a piece refusal's
                    // is that name plus its code. Reading the detail text for a keyword put every
                    // refusal the internal-name join added under "pieces" (MXDIMPORT-1).
                    let named_here = |context: &str| {
                        file.prose
                            .as_ref()
                            .is_some_and(|prose| prose.rows.iter().any(|row| row.power == context))
                    };
                    for note in &reading.notes {
                        let bucket = if named_here(&note.context) {
                            &mut unnamed_powers
                        } else {
                            &mut unnamed_pieces
                        };
                        *bucket.entry(note.context.clone()).or_default() += 1;
                    }
                    let converted =
                        mbd_import::to_skif_build(&reading.file, database, DatasetId::Homecoming);
                    declined_by_mbd += converted.summary.powers_declined;
                }
                for record in file.binary.powers() {
                    powers += 1;
                    slots += record.slots.len();
                    filled += record
                        .slots
                        .iter()
                        .filter(|s| s.enhancement.is_some())
                        .count();
                    flipped += record.slots.iter().filter(|s| s.flipped.is_some()).count();
                }
            }
            Err(error) => refusals.push((name, error)),
        }
    }

    println!("files found: {}", files.len());
    println!("  read as a paired document: {read}");
    println!("  refused: {}", refusals.len());
    println!("  carried no data block: {not_a_document}");
    println!("  could not be read off disk: {unreadable}");
    println!("  read with no post half: {without_prose}");
    println!("powers: {powers}, slots: {slots}, filled: {filled}, flipped: {flipped}");
    println!("declared format versions:");
    for (version, count) in &by_version {
        println!("  {version}: {count}");
    }
    println!("record shapes that fit:");
    for (shape, count) in &by_shape {
        println!("  {shape}: {count}");
    }
    println!(
        "files whose shape is not the one their version names: {}",
        shape_disagrees_with_version.len()
    );
    for name in shape_disagrees_with_version.iter().take(10) {
        println!("  {name}");
    }
    println!(
        "resolved against Homecoming: {} of {} powers named, {} of {} enhancements named",
        totals.powers_named,
        totals.powers_in_file,
        totals.enhancements_named,
        totals.enhancements_in_file,
    );
    println!(
        "  by code {}, by code+index {}, by index inside the named set {}",
        totals.by_code, totals.by_code_and_index, totals.by_index_within_set,
    );
    println!(
        "  powerset paths: {} of {} resolved",
        totals.powersets_resolved, totals.powersets_in_file,
    );
    println!("  files whose counts do not reconcile: {non_reconciling}");
    println!("  entries the .mbd reader then declined: {declined_by_mbd}");
    let mut top: Vec<_> = unnamed_powers.iter().collect();
    top.sort_by(|a, b| b.1.cmp(a.1));
    println!("unnamed powers ({} distinct):", unnamed_powers.len());
    for (name, count) in top.iter().take(15) {
        println!("  {name}: {count}");
    }
    let mut pieces: Vec<_> = unnamed_pieces.iter().collect();
    pieces.sort_by(|a, b| b.1.cmp(a.1));
    println!("unnamed pieces ({} distinct):", unnamed_pieces.len());
    for (name, count) in pieces.iter().take(15) {
        println!("  {name}: {count}");
    }
    if !refusals.is_empty() {
        println!("refusals:");
        for (name, error) in refusals.iter().take(40) {
            println!("  {name}: {error}");
        }
    }
}
