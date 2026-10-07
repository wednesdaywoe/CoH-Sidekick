//! Mids Reborn's `.mbd` file — the document, and the fork it names.
//!
//! **There is no decode step.** `.mbd` is plain uncompressed JSON, CRLF from Mids and LF from
//! us, and the top-level keys are identical between a file Mids wrote and one our own exporter
//! did. That is the good news and the warning at once: the beta's ~3,254-line reader is entirely
//! name and UID resolution, and **nothing in it can fail loudly the way a bad byte does.** Every
//! failure this format has is a name that resolved to the wrong thing, or to nothing, quietly.
//!
//! So this module holds only the document and the one question that must be answered before any
//! of it is read: which fork's data was the author planning against. Resolution lives next door.
//!
//! **A power is named by its INTERNAL name and nothing else**, which is what makes
//! [`crate::mids_names`] load-bearing rather than a nicety — Homecoming rotates internal names
//! underneath stable display names, so the name in the file is the least reliable matcher there
//! is.
//!
//! **What is deliberately typed as opaque.** `SubPowerEntries` is non-empty in none of the eight
//! corpus files and `FlippedEnhancement` is null on all 829 of their slots. Neither is read by
//! anything here, and both are held as raw [`Value`] rather than modelled — a shape guessed from
//! zero examples is a shape that will be wrong, and holding the bytes means a later reader gets
//! the real thing rather than a re-derivation. They round-trip.
//!
//! **`BuiltWith.App` is provenance, never a discriminator.** Mids 3.7.5.21 writes `Mids' Reborn`
//! with an apostrophe and the 3.8 builds write `Mids Reborn` without one. Nothing may branch on
//! it. Our own exporter writes `CoH Planner`, which is likewise not a licence to read the file
//! differently.

use crate::DatasetId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// What the author was planning with. Provenance only — see the module note on `App`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MbdBuiltWith {
    #[serde(default)]
    pub app: String,
    #[serde(default)]
    pub version: String,
    /// The fork stamp — the only thing in the file that says which game's data this build was
    /// authored against. [`probe_dataset`] is the only thing that may interpret it.
    #[serde(default)]
    pub database: String,
    #[serde(default)]
    pub database_version: String,
}

/// One slotted enhancement, as Mids addresses it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MbdEnhancement {
    /// Mids' UID for the piece. Resolved by lookup, never by reading its parts.
    #[serde(default)]
    pub uid: String,
    /// `None` / `TrainingO` / `DualO` / `SingleO`.
    ///
    /// **Never `TO` / `DO` / `SO`** — those are OUR tokens, and testing for them is MBDIMPORT-6:
    /// the corpus Widow imported 10 of its 89 enhancements and warned about the other 79, with
    /// nothing noticing because the only origin-graded pieces in the Homecoming arm are three
    /// Hamidons that a different branch owns. Writing ours back is MBDEXPORT-4, which made a
    /// real Mids refuse an entire build with `Requested value 'SO' was not found`.
    #[serde(default)]
    pub grade: String,
    /// 0-based: 49 means level 50.
    #[serde(default)]
    pub io_level: i32,
    /// `MinusThree` … `Even` … `PlusFive`, relative to the character's level.
    #[serde(default)]
    pub relative_level: String,
    #[serde(default)]
    pub obtained: bool,
}

/// One slot on a power.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MbdSlotEntry {
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub is_inherent: bool,
    #[serde(default)]
    pub enhancement: Option<MbdEnhancement>,
    /// Null on all 829 corpus slots and read by nothing. Held raw so it round-trips.
    #[serde(default)]
    pub flipped_enhancement: Value,
}

/// One power the build carries — a pick, an auto-granted inherent, an accolade or an incarnate.
///
/// Which of those it is, is not stated: the file says only a name and a level, and
/// **the list is read POSITIONALLY** — an entry past `LastPower` is auto-granted rather than
/// picked. A reader that fixes a name without honouring the position turns granted powers into
/// picks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MbdPowerEntry {
    /// The power's INTERNAL name, fully qualified: `Blaster_Ranged.Assault_Rifle.Aim`.
    #[serde(default)]
    pub power_name: String,
    #[serde(default)]
    pub level: i32,
    #[serde(default)]
    pub stat_include: bool,
    #[serde(default)]
    pub proc_include: bool,
    /// The per-power stack / targets-hit slider.
    #[serde(default)]
    pub variable_value: i32,
    #[serde(default)]
    pub inherent_slots_used: i32,
    /// Empty on all 46 entries of the corpus Kheldian, on all 45 of its slots-only twin, and on
    /// every other corpus file. Held raw rather than modelled from zero examples.
    #[serde(default)]
    pub sub_power_entries: Value,
    #[serde(default)]
    pub slot_entries: Vec<MbdSlotEntry>,
}

impl MbdPowerEntry {
    /// Every enhancement this entry arrived holding.
    ///
    /// The reconciliation reads this rather than re-walking the slots at each refusal site,
    /// because **a count each refusal path has to remember to reach is a count the next path
    /// added will miss** — which is exactly how MBDIMPORT-5 shipped: a refused power took six
    /// slotted enhancements with it and the summary reported `enhancementsFailed: 0`.
    pub fn enhancement_count(&self) -> usize {
        self.slot_entries
            .iter()
            .filter(|slot| slot.enhancement.as_ref().is_some_and(|e| !e.uid.is_empty()))
            .count()
    }
}

/// A Mids `.mbd` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MbdFile {
    pub built_with: MbdBuiltWith,
    /// A string in the file, not a number.
    #[serde(default)]
    pub level: String,
    /// `Class_Blaster`.
    #[serde(default)]
    pub class: String,
    #[serde(default)]
    pub origin: String,
    #[serde(default)]
    pub alignment: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub comment: String,
    /// The picked powersets in slot order. **An empty string is a real entry** — the unpicked
    /// slot — not a hole to be filtered out, because the positions are what identify primary,
    /// secondary, pools and epic.
    #[serde(default)]
    pub power_sets: Vec<String>,
    /// The index past which entries are auto-granted rather than picked.
    #[serde(default)]
    pub last_power: i32,
    #[serde(default)]
    pub power_entries: Vec<MbdPowerEntry>,
}

/// Mids serialises with PascalCase keys; serde's `rename_all` cannot express `IoLevel` and
/// `PowerName` from one rule, so the mapping is stated per struct via this module's
/// `#[serde(rename)]`-free approach: we read case-insensitively by normalising the document
/// first. See [`from_str`].
fn normalize_keys(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(k, v)| (pascal_to_snake(&k), normalize_keys(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(normalize_keys).collect()),
        other => other,
    }
}

/// `PowerName` -> `power_name`, `IoLevel` -> `io_level`, `Uid` -> `uid`.
fn pascal_to_snake(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for (index, ch) in key.char_indices() {
        if ch.is_ascii_uppercase() {
            if index != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

/// Why a `.mbd` could not be read as a document at all.
///
/// Distinct from anything the file NAMES that this fork cannot resolve — those are retained and
/// reported per entry (rule 8), never a failure of the read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MbdError {
    #[error("not JSON: {0}")]
    NotJson(String),
    #[error("JSON, but not a Mids build: {0}")]
    NotAnMbd(String),
}

/// Whether a text is meant to be read as a `.mbd`.
///
/// The whole discriminator, and it has to be a key rather than a shape because **a `.mbd` and a
/// `.skif` are both JSON objects**: the app's one document probe splits on the first character
/// ([`crate::skif`]'s door and the game client's export are the other two arms), which puts
/// these two on the same side of it. The key is `BuiltWith`, which is the one field [`MbdFile`]
/// requires and the one a `.skif` has no reason to carry.
///
/// **A probe, not a read.** It answers whether this text BELONGS to this reader, and says
/// nothing about whether the reader will accept it — a JSON object naming `BuiltWith` and
/// nothing else routes here and is refused by [`from_str`] in words that name the format. The
/// alternative, probing with `from_str(text).is_ok()`, would hand a malformed `.mbd` to the
/// `.skif` reader and get back a receipt about a missing `version`, which is a diagnosis of the
/// wrong file.
pub fn is_mbd_document(text: &str) -> bool {
    let Ok(raw) = serde_json::from_str::<serde_json::Map<String, Value>>(text) else {
        return false;
    };
    // Through the module's own normalizer, so the probe and the read agree on what counts as
    // this key — Mids writes `BuiltWith`, and a hand-edited file writing `builtWith` reads.
    raw.keys().any(|key| pascal_to_snake(key) == "built_with")
}

/// Read a `.mbd` document.
pub fn from_str(text: &str) -> Result<MbdFile, MbdError> {
    let raw: Value = serde_json::from_str(text).map_err(|e| MbdError::NotJson(e.to_string()))?;
    serde_json::from_value(normalize_keys(raw)).map_err(|e| MbdError::NotAnMbd(e.to_string()))
}

/// The fork a `.mbd` was authored against, or `None` where it names no fork of ours.
///
/// **A database identifies a fork only where some fork carries it as its OWN.** `Generic` is a
/// real Mids database in its own right, so a build authored in it is a plain CoH build rather
/// than evidence of a fork — it answers `None` and the caller reads the file under whatever
/// dataset is loaded. The same goes for a database name we have never heard of, which is how
/// Mids' third-party databases would arrive.
///
/// That asymmetry is why this is not simply the inverse of what the writer stamps. Thunderspy
/// WRITES `Generic`, because that is the database its UID table is cut from and the only string
/// measured to survive — in Mids 3.8.6 under Wine a build naming `Thunderspy` dies in a .NET
/// error box with no build at all — but a file naming `Generic` cannot be read back as
/// Thunderspy, because a genuine Generic build names it too. See DATA-GAP MBDEXPORT-2.
///
/// Brainstorm is the same shape one step over: it writes `Homecoming` and a file naming
/// `Homecoming` reads as Homecoming, which is the fork a Brainstorm build is authored in anyway.
pub fn probe_dataset(text: &str) -> Option<DatasetId> {
    let file = from_str(text).ok()?;
    dataset_for_database(&file.built_with.database)
}

/// The fork that carries this Mids database as its own, if any.
pub fn dataset_for_database(database: &str) -> Option<DatasetId> {
    // Stated as data, not as a chain of string comparisons in logic: the pairing is a fact about
    // which fork OWNS a Mids database, and the two non-owning forks (Brainstorm on Homecoming's,
    // Thunderspy on Generic) are absent from it by construction rather than by an exception.
    const OWNED: [(&str, DatasetId); 2] = [
        ("Homecoming", DatasetId::Homecoming),
        ("Rebirth", DatasetId::Rebirth),
    ];
    OWNED
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(database))
        .map(|(_, id)| id)
}
