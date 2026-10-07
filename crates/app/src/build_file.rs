//! The `.skif` file edge: what a build is called on disk, which reader a file belongs to, and
//! the one write that leaves the app.
//!
//! The codec itself is [`coh_data::skif`] and knows nothing about files — it takes and returns
//! text. This module is the thin layer between that text and a filesystem, kept out of the
//! components so the two decisions with a right answer (the name, the reader) are gradeable
//! without a renderer.
//!
//! **Reading and writing are asymmetric on purpose.** Dioxus already gives `<input type="file">`
//! a native dialog on the desktop webview (`dioxus-desktop`'s `FileDialogRequest`) and the
//! browser's own on the web, so opening needs nothing here — a component takes the text off the
//! event. Writing has no such primitive, and the two targets do not share a mechanism: the
//! browser downloads a blob, and the desktop webview cannot, because `wry` registers a download
//! handler only when the app supplies one and `dioxus-desktop` supplies none. So [`write_file`]
//! is the one place in this crate with a per-target body, and both arms are real. That is the
//! opposite of the shape that killed the slot drag — a helper only `wasm32` could run, with a
//! `None` twin standing in for the platform that got nothing.

use coh_data::client_build::{self, ClientError};
use coh_data::game_export::{self, ExportError};
use coh_data::game_import;
use coh_data::incarnate_catalog::IncarnateTier;
use coh_data::mbd::{self, MbdError};
use coh_data::mbd_import;
use coh_data::mxd::{self, MxdError};
use coh_data::mxd_import;
use coh_data::skif::{self, Decoded, SkifError, Unresolved};
use coh_data::{CharacterState, DatasetId, PowerDatabase};

/// The extension, and the filter a save dialog offers.
pub const EXTENSION: &str = "skif";

/// What an unnamed build's file is called. Not a placeholder for the *build* — the build is
/// allowed to have no name — only for the file, which must be called something.
const UNNAMED: &str = "build";

/// Characters no filename may carry, on any of the three desktop platforms — the union rather
/// than this one's, since a file written here is meant to be sent to someone.
const RESERVED: [char; 9] = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Long enough for any build name a user would type. This is a readability cap and nothing
/// more — the limit a filesystem actually enforces is [`MAX_STEM_BYTES`], and this one cannot
/// stand in for it: 80 characters is 80 bytes of ASCII and 320 bytes of emoji.
const MAX_STEM_CHARS: usize = 80;

/// What one path component may weigh. ext4, APFS and NTFS all stop at 255 **bytes** — not
/// characters — and the extension and its separating dot are spent out of the same budget.
const NAME_MAX_BYTES: usize = 255;

/// The share of [`NAME_MAX_BYTES`] the stem may take.
const MAX_STEM_BYTES: usize = NAME_MAX_BYTES - EXTENSION.len() - 1;

/// Stems Windows resolves to a DEVICE rather than a file, with or without an extension:
/// `CON.skif` opens the console, it does not create a file. So this is not a name that fails
/// to write — it is a name that appears to write and leaves nothing behind, which is why it is
/// handled here beside the characters that merely fail.
///
/// Reasoned rather than measured: nothing in this repo runs on Windows, so this list is from
/// the platform's documented set and the behaviour above is not something the test suite can
/// demonstrate. The cost of being wrong is one underscore on a file nobody named after a
/// serial port.
const WINDOWS_DEVICES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// What to call this build's file: its own name where it has one, its archetype where it does
/// not, and [`UNNAMED`] where it has neither.
///
/// The archetype comes from the id rather than the display name because the display name is
/// not always there — [`skif::hydrate`] leaves it empty and lets the UI resolve it from the id
/// — so naming a file after it would make the name depend on how the build arrived rather than
/// on what it is.
pub fn file_name_for(build: &CharacterState) -> String {
    let stem = [
        build.name.as_str(),
        build.archetype.id.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .map(filename_safe)
    .find(|candidate| !candidate.is_empty())
    .unwrap_or_else(|| UNNAMED.to_string());
    format!("{stem}.{EXTENSION}")
}

/// `text` reduced to a stem this app is willing to write, changing as little as it can.
///
/// Deliberately not a slug. A build name is something the user typed and will look for later,
/// so capitals, spaces and accents survive; only what would make the write fail, hide the file,
/// or write somewhere that is not a file at all is touched. A leading dot goes because a
/// dotfile is not a refusal, it is a file the user cannot find.
///
/// **What it does NOT promise**, because the comment here used to and that is F48. It is not a
/// guarantee that any given filesystem will accept the result: it does not know the length of
/// the directory it will be written into, it does not normalise Unicode (so a name that is
/// already NFD stays NFD, and APFS will compare it equal to its NFC twin), and it leaves
/// bidirectional-override characters alone — those are not refused by any filesystem, but they
/// can make `build<RLO>fdp.skif` read as a PDF in a file listing. That last one is a display
/// concern rather than a write concern, and it is named here rather than half-handled.
///
/// The three things it does promise are the three that produce no file, or the wrong one: a
/// character the platform refuses, a stem over the byte budget, and a Windows device name.
fn filename_safe(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| {
            if c.is_control() || RESERVED.contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    // Collapse the runs the replacement above can create, so `a/\b` is `a b` and not `a   b`.
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_matches('.').trim();

    // Bounded by BOTH counts, and the byte one is the one a filesystem enforces. Built by
    // pushing whole `char`s rather than slicing at a byte index, which would panic mid-character
    // on exactly the names this exists for.
    let mut stem = String::new();
    for c in trimmed.chars().take(MAX_STEM_CHARS) {
        if stem.len() + c.len_utf8() > MAX_STEM_BYTES {
            break;
        }
        stem.push(c);
    }
    let stem = stem.trim().to_string();

    // The one place this function adds a character the user did not type. The alternative is
    // falling through to the archetype, which silently discards a name somebody chose; an
    // underscore keeps it and costs one byte, and device names are four ASCII characters at
    // most so the budget above is not at risk.
    if WINDOWS_DEVICES.iter().any(|d| stem.eq_ignore_ascii_case(d)) {
        return format!("{stem}_");
    }
    stem
}

/// Read a `.skif` of any version this build knows, choosing the reader by the version the file
/// declares.
///
/// The codec deliberately ships two strict doors and no router: [`skif::decode`] refuses
/// everything but v5 and [`skif::legacy::decode_legacy`] refuses everything but v2/v3/v4,
/// because v4's own reader fell *through* an unrecognized version into a mis-parse. This is the
/// router that was left to the caller, and it is one function rather than a decision taken at
/// each call site so there is one place to grade — a version it does not recognize is refused
/// here in the same words a reader would use, never handed to a reader to see what happens.
pub fn decode_any(text: &str, database: &PowerDatabase) -> Result<Decoded, SkifError> {
    match skif::probe_version(text) {
        Some(skif::VERSION) => skif::decode(text, database),
        Some(version) if skif::legacy::LEGACY_VERSIONS.contains(&version) => {
            skif::legacy::decode_legacy(text, database)
        }
        found => Err(SkifError::Version { found }),
    }
}

/// Why a build's text could not be read, named by whichever reader owns it.
///
/// Two error types rather than one flattened message, because the readers refuse different
/// kinds of thing and each says so precisely: a `.skif` names the version it found, a
/// `/buildsave` names the line it could not read. Flattening them here would leave the receipt
/// unable to tell a user which of the two formats they were even close to.
#[derive(Debug, PartialEq)]
pub enum ImportError {
    Skif(SkifError),
    Export(ExportError),
    Mids(MbdError),
    LegacyMids(MxdError),
    Client(ClientError),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::Skif(error) => error.fmt(f),
            ImportError::Export(error) => error.fmt(f),
            ImportError::Mids(error) => error.fmt(f),
            ImportError::LegacyMids(error) => error.fmt(f),
            ImportError::Client(error) => error.fmt(f),
        }
    }
}

/// Whether a build's text is JSON rather than the game client's plaintext export.
///
/// The first cut of the discriminator, and it is the document's first character rather than a
/// filename or a MIME type: a build reaches this app off a file input, out of a URL fragment and
/// out of a paste box, and only the first of those three carries a name.
/// [`coh_data::import_link`] splits its own two arms on `{`, so a link and a pasted file carrying
/// the same document cannot route to different readers. The two probes diverge below, on `[` and
/// `"`, and only in a direction that cannot open a gap: a fragment decoding to either falls to
/// that module's `game_export::parse`, which refuses it as `Unreadable` before any reader sees
/// it, so the link arm is stricter rather than differently routed.
///
/// **This is no longer the whole answer.** Two of the three formats are JSON objects, so a
/// `true` here says only that the text is not a `/buildsave`. [`document_kind`] is the question
/// callers want, and it splits this side again.
///
/// Three prefixes, not one, and not every character a JSON value may start with. `{` is the only
/// one any of the four formats is written in; `[` and `"` are here because the app re-serialises
/// values it did not author — a shared build's `build_json` is an untyped `serde_json::Value`
/// (`cloud/shared_builds.rs`), which `cloud/browser.rs` hands the planner as text, so a row
/// holding a string or an array arrives as a quoted or bracketed document. Under `starts_with('{')`
/// alone that text was not JSON at all, fell through to the plaintext side, and a `|MxDz;…|`
/// *inside the string* claimed the `.mxd` reader (F71).
///
/// The scalar literals — a leading digit, `-`, `t`, `f` or `n` — are deliberately left out, and
/// the reason is [`crate::build_io::read_pasted`]: a share fragment is base64url, whose alphabet
/// holds every one of those characters, so claiming them would take a pasted fragment for a
/// document and refuse it as a `.skif` of no known version. Nothing is lost by leaving them —
/// a bare scalar is no build in any of the four formats, and it carries no data block for the
/// plaintext side to claim either, so it lands on the `/buildsave` reader and is refused there.
pub fn is_json_document(text: &str) -> bool {
    text.trim_start().starts_with(['{', '[', '"'])
}

/// Which reader owns a text. See [`document_kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Document {
    /// A `.skif` of any version this app knows, or a JSON file claiming to be one.
    Skif,
    /// Mids Reborn's `.mbd`.
    Mbd,
    /// Mids' older plaintext `.mxd` — the forum post with the build in it.
    Mxd,
    /// The text a game client's `/buildsave` wrote.
    GameExport,
    /// The game client's JSON build, as the CoH-to-Sidekick importer sends it in an `/import#…`
    /// link — see [`coh_data::client_build`].
    ClientBuild,
}

/// Read which reader a text belongs to.
///
/// One probe, in one place, for the same reason [`decode_any`] is one router: the app has three
/// doors a build can arrive through and only one of them (the file input) carries a name, so the
/// format has to be read off the document every time. A second copy of this decision is a second
/// chance for a paste and a file of the same bytes to route differently.
///
/// The order is not arbitrary. `.skif` is the DEFAULT of the two JSON arms rather than a probe
/// of its own, because it is the format this app writes and the one whose reader can name a
/// version it does not recognize; `.mbd` claims its side on a key it requires
/// ([`mbd::is_mbd_document`]). A file that is neither — JSON with no `BuiltWith` and no
/// `version` — reaches the `.skif` reader and is refused there naming the version it found,
/// which is the same refusal it got before this split existed.
pub fn document_kind(text: &str) -> Document {
    if !is_json_document(text) {
        // The non-JSON side has two readers now, and `.mxd` claims its own on the one line every
        // spelling of that format carries. `/buildsave` keeps the fall-through for the reason
        // `.skif` keeps the other one: it is the reader that can name what it was expecting.
        match mxd::is_mxd_document(text) {
            true => Document::Mxd,
            false => Document::GameExport,
        }
    } else if mbd::is_mbd_document(text) {
        Document::Mbd
    } else if coh_data::client_build::is_client_document(text) {
        // Before the `.skif` fall-through, which would otherwise claim it for being JSON and
        // refuse it for having no version — the failure a real importer link hit on 2026-09-30.
        Document::ClientBuild
    } else {
        Document::Skif
    }
}

/// Read a build out of whatever the user handed the app: a `.skif` of any version, a Mids
/// `.mbd`, or the text a game client's `/buildsave` wrote.
///
/// `loaded` is the fork a document that names none is read against. A `.skif` names its own and
/// never reaches this argument; a `/buildsave` export names none and never will, because the
/// client that writes it has no idea other forks exist; and a `.mbd` names a Mids DATABASE,
/// which is not the same question — `Generic` is a real one, so a build authored in it names no
/// fork either. So the only honest answer for those two is the server the app is holding, with
/// everything that failed to resolve reported (rule 8) — which is also the evidence the user
/// needs to say it was the wrong server.
pub fn decode_import(
    text: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
) -> Result<Decoded, ImportError> {
    match document_kind(text) {
        Document::Skif => decode_any(text, database).map_err(ImportError::Skif),
        Document::Mbd => decode_mbd(text, database, loaded),
        Document::Mxd => decode_mxd(text, database, loaded),
        Document::GameExport => {
            let export = game_export::parse(text).map_err(ImportError::Export)?;
            let converted = game_import::to_skif_build(&export, database, loaded);
            let notes = converted
                .unresolved
                .into_iter()
                .map(|note| (note.context, note.detail));
            hydrate_converted(converted.build, notes, database)
        }
        Document::ClientBuild => decode_client_build(text, database, loaded),
    }
}

/// Read the game client's JSON build: reshape it into a `/buildsave` export and convert that the
/// way a `/buildsave` file is converted, then add the accolades and incarnates that format has no
/// line for (beta `importExternalBuild`, with its `augmentAccolades` and `augmentIncarnates`).
///
/// Every accolade or incarnate that does not resolve is a note in the report, not a silent drop.
fn decode_client_build(
    text: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
) -> Result<Decoded, ImportError> {
    let document = client_build::parse(text).map_err(ImportError::Client)?;
    let read = client_build::read(&document, database.boost_index.as_ref())
        .map_err(ImportError::Client)?;
    let converted = game_import::to_skif_build(&read.export, database, loaded);
    let mut build = converted.build;
    let mut notes: Vec<(String, String)> = converted
        .unresolved
        .into_iter()
        .map(|note| (note.context, note.detail))
        .collect();

    // Accolades: the client lists every one the character has earned. Only the ones this
    // dataset offers as a stat toggle go into the build — the click and travel accolades grant
    // a timed buff on use, and the `.mbd` reader draws the same line.
    let toggles = database.accolade_toggles();
    for name in &read.accolades {
        match toggles
            .iter()
            .find(|toggle| toggle.power.ident().eq_ignore_ascii_case(name))
        {
            Some(toggle) if !build.accolades.contains(&toggle.id) => {
                build.accolades.push(toggle.id.clone())
            }
            Some(_) => {}
            None if database
                .accolade_powers()
                .any(|power| power.ident().eq_ignore_ascii_case(name)) => {}
            None => notes.push((
                format!("Accolade {name}"),
                "this dataset has no accolade by that name".to_string(),
            )),
        }
    }

    // Incarnates: the client lists every one the character OWNS, several per slot. The highest
    // tier in each slot is the one slotted, the beta's rule.
    let mut best: std::collections::BTreeMap<String, (IncarnateTier, String)> =
        std::collections::BTreeMap::new();
    for (slot_name, power_name) in &read.incarnates {
        let slot = database.incarnate_catalog.slots.iter().find(|slot| {
            slot.id.eq_ignore_ascii_case(slot_name) || slot.key.eq_ignore_ascii_case(slot_name)
        });
        let Some(slot) = slot else {
            notes.push((
                format!("Incarnate {slot_name}.{power_name}"),
                format!("this dataset has no incarnate slot called {slot_name:?}"),
            ));
            continue;
        };
        let Some(power) = slot.find_power(power_name) else {
            notes.push((
                format!("Incarnate {slot_name}.{power_name}"),
                format!(
                    "this dataset's {} slot carries no power called {power_name:?}",
                    slot.id
                ),
            ));
            continue;
        };
        let tier = power.tier();
        let better = best.get(&slot.id).is_none_or(|(held, _)| tier > *held);
        if better {
            best.insert(slot.id.clone(), (tier, power.internal_name.clone()));
        }
    }
    for (slot, (_, power)) in best {
        build.incarnates.insert(
            slot,
            coh_data::skif::SkifIncarnate {
                power,
                active: true,
            },
        );
    }

    hydrate_converted(build, notes, database)
}

/// Read a `.mbd`, and say out loud everything about it that the file's own author would not
/// see here.
///
/// The level is a derivation here and nowhere else — a `.mbd` stores no character level
/// (MBDIMPORT-8) — so where the schedule raised it, or the placements exceed what the schedule
/// grants, the converter says so itself, in one note that reaches the report through
/// `unresolved` below. Nothing is added to that here: a second note saying the same thing in
/// different words is what the first cut of this function shipped, and the receipt read as two
/// separate problems.
///
/// What IS added is the one thing the converter has no channel for, and it is rule 1 rather
/// than courtesy — [`mbd_import::MbdSummary`] says so in its own words.
///
/// **The reconciliation.** Every enhancement the file holds is either imported or failed, and
/// a read where those two do not sum to what arrived has lost track of a piece rather than
/// declined it. MBDIMPORT-5 is that bug shipped: six pieces gone beside
/// `enhancementsFailed: 0`. It cannot be an assertion here
/// — the app has a user in front of it — so it is a note the report cannot omit.
fn decode_mbd(
    text: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
) -> Result<Decoded, ImportError> {
    let file = mbd::from_str(text).map_err(ImportError::Mids)?;
    let converted = mbd_import::to_skif_build(&file, database, loaded);
    let summary = converted.summary;

    let mut notes: Vec<(String, String)> = converted
        .unresolved
        .into_iter()
        .map(|note| (note.context, note.detail))
        .collect();
    if !summary.reconciles() {
        notes.push((
            "the file's enhancements".to_string(),
            format!(
                "{} in the file, {} imported and {} refused — the two do not add up to what \
                 arrived, so a piece is in no count on this receipt",
                summary.enhancements_in_file,
                summary.enhancements_imported,
                summary.enhancements_failed,
            ),
        ));
    }
    hydrate_converted(converted.build, notes, database)
}

/// Read a `.mxd`, Mids' older plaintext format, and say what of it this dataset could not name.
///
/// **Two readers, not one.** [`mxd::from_str`] gets the two halves of the document out and
/// proves they describe the same build; [`mxd_import::to_mbd_file`] turns what they name into
/// the document the `.mbd` reader already reads; and everything after that IS the `.mbd` reader,
/// unchanged. The alternative — a second converter — is how the two halves of the Mids surface
/// drifted apart before, and this is the third door onto one namespace.
///
/// **A `.mxd` names no fork**, because the format predates them: it carries an archetype class,
/// eight powerset paths and no database stamp at all. So it reads against the loaded dataset on
/// the same terms a `/buildsave` does, with everything that failed to resolve reported — which
/// is also the evidence the user needs to say it was the wrong server.
fn decode_mxd(
    text: &str,
    database: &PowerDatabase,
    loaded: DatasetId,
) -> Result<Decoded, ImportError> {
    let file = mxd::from_str(text).map_err(ImportError::LegacyMids)?;
    let reading = mxd_import::to_mbd_file(&file, database);
    let naming = reading.summary;
    let converted = mbd_import::to_skif_build(&reading.file, database, loaded);
    let summary = converted.summary;

    // The naming pass's refusals lead, on `hydrate_converted`'s own rule: they happened first and
    // they explain what follows, because a power this dataset could not name never reached the
    // `.mbd` reader at all.
    let mut notes: Vec<(String, String)> = reading
        .notes
        .into_iter()
        .chain(converted.unresolved)
        .map(|note| (note.context, note.detail))
        .collect();
    // **Two reconciliations, because there are two passes.** The naming pass can lose a power or
    // a piece before the `.mbd` reader ever sees it, and the `.mbd` reader's own counts are
    // against the document it was handed — so a piece dropped between them is in neither. Both
    // are rule 1's obligation on this module for MBDIMPORT-5's reason: the failure is silent, and
    // the note is the only thing that is not.
    if !naming.reconciles() {
        notes.push((
            "reading the file's names".to_string(),
            format!(
                "{} powers and {} enhancements in the file, {} and {} named — the counts do not \
                 add up to what arrived, so something the file held is in no count on this receipt",
                naming.powers_in_file,
                naming.enhancements_in_file,
                naming.powers_named,
                naming.enhancements_named,
            ),
        ));
    }
    if !summary.reconciles() {
        notes.push((
            "the file's enhancements".to_string(),
            format!(
                "{} in the file, {} imported and {} refused — the two do not add up to what \
                 arrived, so a piece is in no count on this receipt",
                summary.enhancements_in_file,
                summary.enhancements_imported,
                summary.enhancements_failed,
            ),
        ));
    }
    hydrate_converted(converted.build, notes, database)
}

/// Hydrate a build a converter produced, with the converter's own refusals joined onto the
/// reader's.
///
/// The tail both non-`.skif` readers share, and shared rather than written twice because the
/// JOIN is the part with a rule behind it: the converter's refusals lead, because they happened
/// first and they explain the ones under them — a power with no home in this fork never reached
/// `hydrate`, so neither did the slots hanging off it. Joining the two lists is the caller's job
/// by design (each converter's `ImportNote` says so), because only the caller has one report to
/// put them in. Two converters with two orderings would be two receipts that read differently
/// about the same failure.
fn hydrate_converted(
    build: coh_data::skif::SkifBuild,
    notes: impl IntoIterator<Item = (String, String)>,
    database: &PowerDatabase,
) -> Result<Decoded, ImportError> {
    let mut decoded = skif::hydrate(
        skif::SkifFile {
            version: skif::VERSION,
            authored_against: None,
            meta: None,
            build,
        },
        database,
    )
    .map_err(ImportError::Skif)?;

    let mut unresolved: Vec<Unresolved> = notes
        .into_iter()
        .map(|(context, detail)| Unresolved { context, detail })
        .collect();
    unresolved.append(&mut decoded.unresolved);
    decoded.unresolved = unresolved;
    Ok(decoded)
}

/// Where a write ended up, for the report — `None` where the platform owns the destination and
/// the app was never told it.
pub type WrittenTo = Option<String>;

/// Write `text` out as a file the user keeps, offering `suggested_name`.
///
/// Returns `Ok(None)` when the write succeeded somewhere the app cannot name (the browser's
/// download, wherever it is configured to land), `Ok(Some(path))` when it can, and `Err` with
/// what went wrong. A cancelled save dialog is `Ok(None)` too: the user declining is not a
/// failure, and nothing about it needs reporting.
#[cfg(target_arch = "wasm32")]
pub async fn write_file(suggested_name: &str, text: &str) -> Result<WrittenTo, String> {
    use dioxus::prelude::*;

    // Both values reach JS as JSON string literals rather than through `{:?}`: Rust's debug
    // escaping is close enough to JS's to be tempting and is not the same language, and this
    // payload is an arbitrary build name plus a whole file.
    let name = serde_json::to_string(suggested_name).map_err(|e| e.to_string())?;
    let payload = serde_json::to_string(text).map_err(|e| e.to_string())?;
    let js = format!(
        "try {{\
           const blob = new Blob([{payload}], {{ type: 'application/json' }});\
           const url = URL.createObjectURL(blob);\
           const link = document.createElement('a');\
           link.href = url;\
           link.download = {name};\
           document.body.appendChild(link);\
           link.click();\
           document.body.removeChild(link);\
           URL.revokeObjectURL(url);\
           return '';\
         }} catch (e) {{ return String(e); }}"
    );
    let value = document::eval(&js).await.map_err(|e| format!("{e:?}"))?;
    match value.as_str().unwrap_or_default() {
        "" => Ok(None),
        message => Err(message.to_string()),
    }
}

/// The native half: a real save dialog, through the same XDG portal `dioxus-desktop` opens the
/// file input with, so both halves of build I/O ask the desktop the same way.
#[cfg(not(target_arch = "wasm32"))]
pub async fn write_file(suggested_name: &str, text: &str) -> Result<WrittenTo, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .set_file_name(suggested_name)
        .add_filter("CoH Sidekick build", &[EXTENSION])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

/// Write `bytes` out as a PNG the user keeps, offering `suggested_name`.
///
/// The image twin of [`write_file`], and per-target for the same reason: the browser downloads a
/// blob and the desktop webview cannot. Same three outcomes — `Ok(None)` for a save the app
/// cannot name or a dialog the user cancelled, `Ok(Some(path))` where it can, `Err` otherwise.
#[cfg(target_arch = "wasm32")]
pub async fn write_png_file(suggested_name: &str, bytes: &[u8]) -> Result<WrittenTo, String> {
    use dioxus::prelude::*;

    let name = serde_json::to_string(suggested_name).map_err(|e| e.to_string())?;
    let payload =
        serde_json::to_string(&crate::clipboard::base64(bytes)).map_err(|e| e.to_string())?;
    let js = format!(
        "try {{\
           const bin = atob({payload});\
           const bytes = new Uint8Array(bin.length);\
           for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);\
           const blob = new Blob([bytes], {{ type: 'image/png' }});\
           const url = URL.createObjectURL(blob);\
           const link = document.createElement('a');\
           link.href = url;\
           link.download = {name};\
           document.body.appendChild(link);\
           link.click();\
           document.body.removeChild(link);\
           URL.revokeObjectURL(url);\
           return '';\
         }} catch (e) {{ return String(e); }}"
    );
    let value = document::eval(&js).await.map_err(|e| format!("{e:?}"))?;
    match value.as_str().unwrap_or_default() {
        "" => Ok(None),
        message => Err(message.to_string()),
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn write_png_file(suggested_name: &str, bytes: &[u8]) -> Result<WrittenTo, String> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .set_file_name(suggested_name)
        .add_filter("PNG image", &["png"])
        .save_file()
        .await
    else {
        return Ok(None);
    };
    let path = handle.path().to_path_buf();
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path.display().to_string()))
}
