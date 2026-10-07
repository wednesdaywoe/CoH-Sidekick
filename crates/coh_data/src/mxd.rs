//! Mids' legacy `.mxd` file — one build written twice, in one document.
//!
//! **The format is two renderings of the same build, and that is the whole reason this reader
//! exists in the shape it does.** The top of the file is the forum post Mids generates: power
//! DISPLAY names, enhancement SHORT codes, and slot levels in prose. The bottom is a
//! `|MxDz;…;HEX;|` block — zlib over Mids' own `BinaryWriter` stream, carrying canonical
//! powerset PATHS, the archetype class, exact slot and IO levels, and integer indices into the
//! Mids database that wrote it.
//!
//! Neither half can be read alone:
//!
//! - **the binary names nothing.** `nIDPower` is an array index into Mids' powers database, and
//!   that array has been reshuffled out of all recognition between the versions the corpus
//!   spans — index 299 is War Mace's Pulverize in a 2019 file and `Temporal_Healing` in the
//!   database this repo vendors. An index is portable only inside its own file.
//! - **the prose has no structure.** It states no powerset path, no archetype, no attunement,
//!   and — in the 3.x spelling — no IO level either.
//!
//! So the binary is the spine and the prose is the naming, and **each is the other's oracle**.
//! That is not a hope: across the 1,929 posted `.mxd` files in the corpus the two halves agree
//! on the row count, every power's level, every power's slot count and every slot's IO level,
//! with zero disagreements. [`pair`] therefore refuses on disagreement rather than preferring a
//! half, because a file whose two renderings differ has been edited and neither half is
//! evidence for the other.
//!
//! **What this module does NOT do is resolve anything.** No name here reaches our data: that is
//! [`crate::mxd_import`]'s job, and it goes through the `.mbd` door rather than growing a second
//! resolver. This module's whole responsibility is to get the two halves out of the document
//! faithfully and to state where they disagree.
//!
//! ## The record shapes
//!
//! Mids bumped the save format three times inside this corpus and the float at offset 4 names
//! which: `1.01` (Mids' Villain/Hero Designer 1.9x), `3.1` (Mids Reborn 3.2.x) and `3.2` (Mids
//! Reborn 3.4.x). They differ in two places — how many bytes a power entry carries between its
//! level and its slot count, and whether a slot's level is one byte or two.
//!
//! **The version does not settle the shape on its own**, which is the one thing about this
//! format that had to be measured rather than assumed. One corpus file declares `1.01` and
//! carries records with no `FlippedEnhancement` field at all, in a header byte-identical to its
//! 955 siblings that do. So [`MxdShape`] is chosen by reading the stream to its end under each
//! known shape and keeping the one that consumes the buffer EXACTLY: across all 1,929 files,
//! against ten candidate shapes, exactly one fits every time and never two. A file where none
//! fits, or more than one does, is refused — a shape that merely *starts* parsing is what turns
//! a wrong layout into plausible data.

use std::io::Read;

/// The magic the compressed half opens with, and the only one this reader accepts.
///
/// Mids' own writer has an uncompressed spelling too (`MxDu`); no posted file in the corpus uses
/// it, so it is refused by name rather than guessed at from zero examples.
const COMPRESSED_MAGIC: &str = "MxDz";

/// The body encoding the header declares. Same reasoning as [`COMPRESSED_MAGIC`].
const HEX_ENCODING: &str = "HEX";

/// The most a data block may declare it inflates to.
///
/// `inflated_len` was the one declared length nothing checked. Its two siblings are each checked
/// against the body that actually arrived (`hex_len` against `collect_hex`'s output,
/// `compressed_len` against the decoded bytes), and this one went straight to
/// `Vec::with_capacity` having been compared to nothing at all. A 41-byte paste declaring
/// `18446744073709551615` is a `capacity overflow` panic, reproduced in this file's tests; a
/// value in the middle band is worse, because an infallible allocator *aborts* there and no
/// `catch_unwind` reaches an abort.
///
/// **The number is measured, with the headroom stated.** A real Mids build inflates to about two
/// kilobytes: across the eight corpus fixtures in `fixtures/mids/mxd/` the declared lengths run
/// 1,581 to 2,131 bytes, at compression ratios of 2.07 to 2.85. One mebibyte is roughly 490 times
/// the largest build anyone has posted, so refusing above it cannot refuse a real document, and a
/// document declaring more than this is not a build whatever else it is.
///
/// It bounds both the allocation and the read. Bounding only the declared length would leave
/// F16's other half open, because `read_to_end` never looked at `inflated_len` in the first
/// place: a small body that inflates to gigabytes was materialised in full and only then
/// compared against what it said it would be.
const MAX_INFLATED_BYTES: usize = 1 << 20;

/// The magic the inflated stream opens with — three raw bytes, no length prefix.
const STREAM_MAGIC: &[u8] = b"MxD";

/// A level no character, power or enhancement in this game reaches, used to reject a shape whose
/// records are landing on the wrong bytes.
///
/// Deliberately generous rather than 50: the incarnate levels and the shifted IO bands push past
/// the level cap, and a bound tight enough to be interesting here would be a second source of
/// truth for something [`crate::leveling_schedule`] owns. Its only job is to be smaller than the
/// noise a misaligned read produces.
const IMPLAUSIBLE_LEVEL: i32 = 60;

/// How a power entry and a slot are laid out in one version of the stream.
///
/// The two fields are the only things that move between versions, and they are held as data
/// rather than as a branch per version so that [`MxdShape::CANDIDATES`] can be tried
/// mechanically — a shape is a thing the reader measures against the file, not a thing it
/// decides from the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MxdShape {
    /// Bytes between a power entry's level and its sub-power count: `StatInclude`,
    /// `ProcInclude`, and the two `int` fields whose width changed with the format.
    pub entry_middle: usize,
    /// Whether a slot's level is a two-byte field. Mids widened it at format 3.2.
    pub wide_slot_level: bool,
    /// Whether a slot carries a `FlippedEnhancement` after its own enhancement.
    pub flipped_enhancement: bool,
}

impl MxdShape {
    /// The shape Mids' 1.9x Designer writes.
    pub const V1_01: Self = Self {
        entry_middle: 5,
        wide_slot_level: false,
        flipped_enhancement: true,
    };
    /// The shape Mids Reborn 3.2.x writes — one more byte in the middle (`ProcInclude` arrives).
    pub const V3_1: Self = Self {
        entry_middle: 6,
        wide_slot_level: false,
        flipped_enhancement: true,
    };
    /// The shape Mids Reborn 3.4.x writes — two `int`s in the middle, and a two-byte slot level.
    pub const V3_2: Self = Self {
        entry_middle: 10,
        wide_slot_level: true,
        flipped_enhancement: true,
    };
    /// `1.01`'s records without a `FlippedEnhancement` field.
    ///
    /// Exactly one posted file in the corpus is written this way, under a header byte-identical
    /// to the 955 that are not, which is why the header cannot be trusted to name the shape.
    pub const V1_01_UNFLIPPED: Self = Self {
        entry_middle: 5,
        wide_slot_level: false,
        flipped_enhancement: false,
    };

    /// Every shape [`read_binary`] will try, widest net first so that a file fitting two is
    /// caught rather than resolved by ordering.
    ///
    /// The four beyond the named ones are not speculation about Mids versions nobody has seen —
    /// they are the near neighbours a misread would land on, and they are here so that "exactly
    /// one shape fits" is a claim about a real search rather than about a list of one.
    pub const CANDIDATES: [Self; 10] = [
        Self::V1_01,
        Self::V3_1,
        Self::V3_2,
        Self::V1_01_UNFLIPPED,
        Self {
            entry_middle: 4,
            wide_slot_level: false,
            flipped_enhancement: false,
        },
        Self {
            entry_middle: 6,
            wide_slot_level: false,
            flipped_enhancement: false,
        },
        Self {
            entry_middle: 10,
            wide_slot_level: true,
            flipped_enhancement: false,
        },
        Self {
            entry_middle: 4,
            wide_slot_level: false,
            flipped_enhancement: true,
        },
        Self {
            entry_middle: 5,
            wide_slot_level: true,
            flipped_enhancement: true,
        },
        Self {
            entry_middle: 6,
            wide_slot_level: true,
            flipped_enhancement: true,
        },
    ];

    /// The shape a declared format version names, where this reader knows one.
    ///
    /// Advisory only — [`read_binary`] chooses by what fits and uses this to say so when the two
    /// disagree. A version with no entry here is not a refusal: the shape search is the
    /// authority either way, and an unknown version whose records fit a known shape exactly is
    /// better evidence than a table that has never seen it.
    pub fn for_version(version: f32) -> Option<Self> {
        const NAMED: [(f32, MxdShape); 3] = [
            (1.01, MxdShape::V1_01),
            (3.1, MxdShape::V3_1),
            (3.2, MxdShape::V3_2),
        ];
        NAMED
            .into_iter()
            .find(|(declared, _)| (version - declared).abs() < 0.001)
            .map(|(_, shape)| shape)
    }
}

/// One slotted enhancement, as the binary half addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdEnhancementRef {
    /// Mids' own array index into the enhancement database that wrote this file.
    ///
    /// **Not portable.** Mids reorders that array between releases, and the corpus proves it:
    /// 1,699 slots across the corpus land on a different piece of the same set when read against
    /// the database this repo vendors. It is evidence about which SET a slot names — set
    /// membership survives the reordering — and the prose is what names the piece.
    pub index: i32,
    /// The two bytes Mids writes after the index, in order.
    ///
    /// **What they mean depends on the kind of record the index names**, and this document
    /// cannot know that — only an enhancement database can say whether index 60 is an invention
    /// piece or a Hamidon. So they are carried as written and read through [`Self::as_invention`]
    /// or [`Self::as_graded`], each of which names the reading it is taking.
    ///
    /// The union is measured, not assumed. On an invention the pair is (crafted level, relative
    /// level): 12 paired builds saved both as `.mxd` and as `.mbd` put `9` against
    /// `RelativeLevel: PlusFive` and `8` against `PlusFour`, and the crafted level agrees with
    /// the post half on 57,222 slots with no exceptions. On a Hamidon the pair is (relative
    /// level, grade) — `(4, 3)` on 5,538 corpus slots and `(4, 0)` on 123, which is `Even` with
    /// `SingleO` and `Even` with `None`, and matches the 91%/9% split of `Grade` on the 2,611
    /// special slots of the 2,187 `.mbd` files beside them. Read the other way round it would
    /// make 98% of the corpus's Hamidons `MinusOne`, a value those 2,187 files do not contain
    /// once.
    pub fields: [u8; 2],
}

impl MxdEnhancementRef {
    /// Read as an invention piece: the crafted level, 0-based exactly as
    /// [`crate::mbd::MbdEnhancement::io_level`] (49 means level 50), and Mids' `eEnhRelative`,
    /// which on an invention counts boosters (4 is Even, 9 is PlusFive — five boosters).
    pub fn as_invention(&self) -> (u8, u8) {
        (self.fields[0], self.fields[1])
    }

    /// Read as a Hamidon, Hydra, Titan, D-Sync or origin piece: Mids' `eEnhRelative`, then its
    /// `eEnhGrade`, which is the only place an origin piece's tier is stated.
    pub fn as_graded(&self) -> (u8, u8) {
        (self.fields[0], self.fields[1])
    }
}

/// One slot on a power, as the binary half records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdSlot {
    /// 0-based, and `-1` where Mids states none. The first slot of a power usually carries the
    /// power's own level, but not always — one corpus slot carries 22 on a power taken at 9 —
    /// so it is read rather than derived.
    pub level: i16,
    /// `None` is an empty slot, which Mids writes as an index of `-1` and the prose spells
    /// `Empty`.
    pub enhancement: Option<MxdEnhancementRef>,
    /// Mids' `Obtained` — "this piece is in my inventory". Read by nothing here, and held
    /// rather than dropped because it is the only field of Mids' slot record left over.
    ///
    /// **The two formats do not agree about it.** It is `true` on 192,777 of the corpus's
    /// 192,880 slots and `false` on 103, all of them in one file — while Mids' own `.mbd` writes
    /// `false` on all 364 slots of the builds saved both ways. Nothing here depends on which is
    /// right, and saying so is cheaper than a name that quietly asserts one.
    pub obtained: bool,
    /// Mids' alternate-enhancement slot — 64 of the corpus's 192,880, and read by nothing here
    /// yet; held rather than dropped so a later reader gets the real thing rather than a
    /// re-derivation, on the same terms as `.mbd`'s `FlippedEnhancement`, which is null on all
    /// 829 slots of ITS corpus. Between the two formats, this is the only sighting of the
    /// feature anywhere.
    pub flipped: Option<MxdEnhancementRef>,
}

/// One entry in the binary half's power list.
///
/// The list is read POSITIONALLY, the same way [`crate::mbd::MbdFile`]'s is: an entry past
/// `last_power` is auto-granted rather than picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MxdEntry {
    /// A power the build carries.
    Power(MxdPowerRecord),
    /// An entry naming no power. Mids writes `-1` for the index and skips the rest of the
    /// record, so an unassigned entry has no level and cannot be given one — which is why this
    /// is a variant rather than a `None` field on the record below.
    ///
    /// The prose half omits these entirely, and that omission is what aligns the two lists.
    Unassigned,
}

/// One power the build carries, as the binary half records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdPowerRecord {
    /// Mids' own array index into the powers database that wrote this file. See
    /// [`MxdEnhancementRef::index`] — this one is worse, because the powers array was reshuffled
    /// wholesale between the versions the corpus spans, so it identifies a power only within
    /// this one file.
    pub power_id: i32,
    /// 0-based, and `-1` for the entries Mids files at no level — every accolade in the corpus.
    pub level: i8,
    pub stat_include: bool,
    /// Absent before format 3.1, where it reads `false`.
    pub proc_include: bool,
    /// The per-power stack / targets-hit slider.
    pub variable_value: i32,
    pub slots: Vec<MxdSlot>,
}

/// The compressed half of a `.mxd`.
#[derive(Debug, Clone, PartialEq)]
pub struct MxdBinary {
    /// The float at offset 4. Provenance and a cross-check on [`Self::shape`], never the thing
    /// that chose it.
    pub format_version: f32,
    /// The record shape this stream actually turned out to have.
    pub shape: MxdShape,
    /// Mids' archetype class token, e.g. `Class_Brute`.
    pub class_name: String,
    /// The origin, e.g. `Magic`.
    pub origin: String,
    /// Mids' alignment enum.
    pub alignment: i32,
    pub character_name: String,
    /// Eight canonical powerset paths, blanks included, in Mids' own fixed order: primary,
    /// secondary, the slot most archetypes leave empty, four pools, ancillary. Held with the
    /// blanks because the position is what says which is which.
    pub powersets: Vec<String>,
    /// How many of the entries below are picks rather than grants. A COUNT, not the index of the
    /// last one — the same convention MBDEXPORT-11 pinned on the `.mbd` side.
    pub last_power: i32,
    pub entries: Vec<MxdEntry>,
}

impl MxdBinary {
    /// The entries that name a power, in order — the list the prose half aligns with.
    pub fn powers(&self) -> impl Iterator<Item = &MxdPowerRecord> {
        self.entries.iter().filter_map(|entry| match entry {
            MxdEntry::Power(record) => Some(record),
            MxdEntry::Unassigned => None,
        })
    }
}

/// One enhancement as the prose half spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdProseEnhancement {
    /// Mids' short code: `Ags-ResDam/EndRdx`, `RechRdx-I`, `HO:Nucle`, or `Empty`.
    ///
    /// Held whole rather than split into set and piece, because the split is not decidable here:
    /// Gaussian's Synchronized Fire-Control's set code ENDS in a hyphen (`GssSynFr-`), so
    /// `GssSynFr--Build%` has three plausible splits and only the enhancement database can say
    /// which is real. [`crate::mxd_import`] splits it against that database.
    pub code: String,
    /// The `:50` the 1.01 prose writes after the code, 1-based. Mids Reborn's prose dropped it,
    /// so `None` means the spelling states no level, never level zero.
    pub io_level: Option<u8>,
    /// The `(7)`, 1-based. `None` is the `(A)` every power's first slot carries, which states
    /// that the slot came with the power rather than stating a level.
    pub slot_level: Option<u8>,
}

/// One power as the prose half spells it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdProseRow {
    /// 1-based, as written.
    pub level: u8,
    /// Mids' DISPLAY name for the power, which is the only name the prose carries.
    pub power: String,
    pub enhancements: Vec<MxdProseEnhancement>,
}

/// The prose half of a `.mxd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MxdProse {
    /// Everything above the first `Level n:` line, one entry per line, with the markup taken
    /// out. Provenance: the Mids version, the character summary, the powerset display names.
    pub header: Vec<String>,
    pub rows: Vec<MxdProseRow>,
}

/// A `.mxd` read as a document: both halves, and the agreement between them established.
#[derive(Debug, Clone, PartialEq)]
pub struct MxdFile {
    pub binary: MxdBinary,
    /// `None` for a file that carries only the `|MxDz;…|` block. One corpus file is written that
    /// way, and a build pasted out of a planner's clipboard is the same shape, so an absent
    /// prose half is a document this reader accepts and reports on — not a malformed one.
    pub prose: Option<MxdProse>,
}

/// Why a `.mxd` could not be read as a document at all.
///
/// Distinct from anything the file NAMES that this fork cannot resolve — those are retained and
/// reported per entry, never a failure of the read, exactly as on the `.mbd` side.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MxdError {
    #[error("no Mids data block: the text carries no |MxDz;…;HEX;| header")]
    NoDataBlock,
    #[error("the data block declares {found:?} where this reader knows only {COMPRESSED_MAGIC}/{HEX_ENCODING}")]
    UnknownEncoding { found: String },
    #[error("the data block's header is not four numbers and an encoding: {0:?}")]
    MalformedHeader(String),
    #[error("the data block declares {declared} hex characters and carries {found}")]
    HexLengthMismatch { declared: usize, found: usize },
    #[error("the data block is not hexadecimal at character {at}")]
    NotHexadecimal { at: usize },
    #[error("the data block declares {declared} compressed bytes and carries {found}")]
    CompressedLengthMismatch { declared: usize, found: usize },
    #[error("the data block will not decompress: {0}")]
    Inflate(String),
    #[error("the decompressed block declares {declared} bytes and is {found}")]
    InflatedLengthMismatch { declared: usize, found: usize },
    #[error(
        "the data block says it inflates to {declared} bytes, past the {limit} this reader will \
         read; the largest build in the corpus inflates to about two thousand"
    )]
    InflatedLengthImplausible { declared: usize, limit: usize },
    #[error("the decompressed block does not open with MxD and a format version")]
    NotAMidsStream,
    #[error(
        "the decompressed block's records match no layout this reader knows \
         (format version {version}); it is neither a {candidates}-layout Mids save nor anything \
         near one"
    )]
    NoShapeFits { version: f32, candidates: usize },
    #[error(
        "the decompressed block's records match {fits} different layouts, so which one the file \
         is written in cannot be read off the file"
    )]
    AmbiguousShape { fits: usize },
    #[error(
        "the two halves of the file disagree: {what}. One build written twice is what makes this \
         format readable, so a document whose renderings differ has been edited and neither half \
         is evidence for the other"
    )]
    HalvesDisagree { what: String },
}

// ============================================================
// The document probe.
// ============================================================

/// Whether a text is meant to be read as a `.mxd`.
///
/// The key is the data block's own header, which is the one thing every spelling of this format
/// carries and nothing else in the app's four document formats does. A probe and not a read: a
/// text carrying the header and a corrupt body routes here and is refused by [`from_str`] in
/// words that name this format, rather than reaching the `/buildsave` reader and being refused
/// for having no `Build:` line — which is a diagnosis of the wrong file.
pub fn is_mxd_document(text: &str) -> bool {
    find_block_header(text).is_some()
}

/// Where the `|MxDz;…;HEX;|` header sits, as `(start, end)` byte offsets of the header line's
/// content.
fn find_block_header(text: &str) -> Option<(usize, usize)> {
    let mut search = 0usize;
    while let Some(offset) = text[search..].find("|MxD") {
        let start = search + offset + 1;
        let end = text[start..].find('|').map(|len| start + len)?;
        let line = &text[start..end];
        if line.split(';').count() >= 5 {
            return Some((start, end));
        }
        search = end;
    }
    None
}

// ============================================================
// The document.
// ============================================================

/// Read a `.mxd` document: both halves, paired.
pub fn from_str(text: &str) -> Result<MxdFile, MxdError> {
    let (header_start, header_end) = find_block_header(text).ok_or(MxdError::NoDataBlock)?;
    let stream = inflate_block(text, header_start, header_end)?;
    let binary = read_binary(&stream)?;
    let prose = read_prose(&text[..header_start]);
    pair(binary, prose)
}

/// Establish that the two halves describe the same build, and hand back the pair.
///
/// Every check here is a claim measured over the 1,929-file corpus and found to hold with zero
/// exceptions, which is what makes refusing on it honest rather than brittle. What is NOT
/// checked is the one place the two halves legitimately differ: a power's first slot carries its
/// own level in the binary and the prose spells it `(A)`, stating nothing, so there is nothing
/// to compare.
pub fn pair(binary: MxdBinary, prose: Option<MxdProse>) -> Result<MxdFile, MxdError> {
    let Some(prose) = prose else {
        return Ok(MxdFile {
            binary,
            prose: None,
        });
    };
    let records: Vec<&MxdPowerRecord> = binary.powers().collect();
    if records.len() != prose.rows.len() {
        return Err(MxdError::HalvesDisagree {
            what: format!(
                "the data block names {} powers and the post lists {}",
                records.len(),
                prose.rows.len()
            ),
        });
    }
    for (record, row) in records.iter().zip(&prose.rows) {
        disagreement(record, row).map_or(Ok(()), |what| Err(MxdError::HalvesDisagree { what }))?;
    }
    Ok(MxdFile {
        binary,
        prose: Some(prose),
    })
}

/// What the two halves say differently about one power, in words, or `None` where they agree.
fn disagreement(record: &MxdPowerRecord, row: &MxdProseRow) -> Option<String> {
    // The binary's level is 0-based and the prose's is 1-based; `-1` is the level Mids files
    // accolades at, and the prose writes those as `Level 0:`, so both spellings mean "no level".
    let stated = i32::from(record.level) + 1;
    if stated != i32::from(row.level) && !(record.level < 0 && row.level == 0) {
        return Some(format!(
            "the data block has {:?} at level {stated} and the post has it at level {}",
            row.power, row.level
        ));
    }
    if record.slots.len() != row.enhancements.len() {
        return Some(format!(
            "the data block gives {:?} {} slots and the post gives it {}",
            row.power,
            record.slots.len(),
            row.enhancements.len()
        ));
    }
    for (index, (slot, spelled)) in record.slots.iter().zip(&row.enhancements).enumerate() {
        // The post states a level only for an invention piece — a Hamidon is written `HO:Nucle`
        // with no level at all — so this comparison only ever reads the pair the invention way,
        // which is the reading that is right when a level is there to compare.
        if let (Some(reference), Some(level)) = (&slot.enhancement, spelled.io_level) {
            if u32::from(reference.as_invention().0) + 1 != u32::from(level) {
                return Some(format!(
                    "the data block puts {:?}'s slot {} at IO level {} and the post at {level}",
                    row.power,
                    index + 1,
                    u32::from(reference.as_invention().0) + 1,
                ));
            }
        }
        if slot.enhancement.is_none() != (spelled.code == EMPTY_SLOT_CODE) {
            return Some(format!(
                "the data block and the post disagree about whether {:?}'s slot {} is empty",
                row.power,
                index + 1
            ));
        }
        // Slot 0 is the `(A)` the prose states nothing about; every other slot's level is stated
        // twice and has never differed.
        if index > 0 {
            if let Some(level) = spelled.slot_level {
                if i32::from(slot.level) + 1 != i32::from(level) {
                    return Some(format!(
                        "the data block puts {:?}'s slot {} at level {} and the post at {level}",
                        row.power,
                        index + 1,
                        i32::from(slot.level) + 1,
                    ));
                }
            }
        }
    }
    None
}

/// The code the prose writes for a slot holding nothing.
pub const EMPTY_SLOT_CODE: &str = "Empty";

// ============================================================
// The data block.
// ============================================================

/// Pull the `|MxDz;…|` block out of the document and inflate it.
fn inflate_block(text: &str, header_start: usize, header_end: usize) -> Result<Vec<u8>, MxdError> {
    let header = &text[header_start..header_end];
    let fields: Vec<&str> = header.split(';').collect();
    let [magic, inflated_len, compressed_len, hex_len, encoding, ..] = fields.as_slice() else {
        return Err(MxdError::MalformedHeader(header.to_string()));
    };
    if *magic != COMPRESSED_MAGIC || *encoding != HEX_ENCODING {
        return Err(MxdError::UnknownEncoding {
            found: format!("{magic};…;{encoding}"),
        });
    }
    let declared = |field: &str| {
        field
            .parse::<usize>()
            .map_err(|_| MxdError::MalformedHeader(header.to_string()))
    };
    let (inflated_len, compressed_len, hex_len) = (
        declared(inflated_len)?,
        declared(compressed_len)?,
        declared(hex_len)?,
    );

    let body = collect_hex(&text[header_end..]);
    if body.len() != hex_len {
        return Err(MxdError::HexLengthMismatch {
            declared: hex_len,
            found: body.len(),
        });
    }
    let compressed = decode_hex(&body)?;
    if compressed.len() != compressed_len {
        return Err(MxdError::CompressedLengthMismatch {
            declared: compressed_len,
            found: compressed.len(),
        });
    }
    // The third declared length, checked at last, and checked before it is used rather than
    // after — its two siblings above are each measured against what actually arrived, and this
    // one was reaching the allocator compared against nothing. See [`MAX_INFLATED_BYTES`].
    if inflated_len > MAX_INFLATED_BYTES {
        return Err(MxdError::InflatedLengthImplausible {
            declared: inflated_len,
            limit: MAX_INFLATED_BYTES,
        });
    }

    // `take` is the half that closes the bomb, and it is separate from the check above on
    // purpose: the check bounds what may be *claimed*, this bounds what may be *read*, and
    // neither implies the other. `read_to_end` ignored `inflated_len` entirely, so a body
    // declaring two thousand bytes and inflating to two gigabytes was inflated in full and only
    // then found to disagree. One byte past the declared length is enough to tell a stream that
    // is too long from one that is exactly right, and the mismatch below reports it.
    let mut stream = Vec::with_capacity(inflated_len);
    flate2::read::ZlibDecoder::new(compressed.as_slice())
        .take(inflated_len as u64 + 1)
        .read_to_end(&mut stream)
        .map_err(|error| MxdError::Inflate(error.to_string()))?;
    if stream.len() != inflated_len {
        return Err(MxdError::InflatedLengthMismatch {
            declared: inflated_len,
            found: stream.len(),
        });
    }
    Ok(stream)
}

/// The hexadecimal body, with the `|` gutters and the block's closing rule taken out.
///
/// Split on the gutter rather than on lines, because the two are not the same: Mids wraps the
/// body at 65 characters a line, and a build exported to the clipboard puts the whole block —
/// header and body — on one. Splitting on lines read the single-line spelling as no body at all.
///
/// Accumulation STOPS at the first non-empty segment that is not hexadecimal, which is the
/// `|-----|` rule Mids draws under the block. That matters beyond tidiness: a forum signature
/// below the rule can be spelled entirely in hex digits (`beef`, `added`, `face`), and a
/// collector that skipped what it did not like instead of stopping would swallow them. The
/// declared length is checked against what comes back, so a body cut short says so.
fn collect_hex(text: &str) -> String {
    let mut body = String::new();
    for segment in text.split(['|', '\n', '\r']).map(str::trim) {
        if segment.is_empty() {
            continue;
        }
        if !segment.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            break;
        }
        body.push_str(segment);
    }
    body
}

fn decode_hex(body: &str) -> Result<Vec<u8>, MxdError> {
    body.as_bytes()
        .chunks(2)
        .enumerate()
        .map(|(index, pair)| {
            let [high, low] = pair else {
                return Err(MxdError::NotHexadecimal { at: index * 2 });
            };
            let digit = |byte: u8| {
                char::from(byte)
                    .to_digit(16)
                    .ok_or(MxdError::NotHexadecimal { at: index * 2 })
            };
            Ok((digit(*high)? * 16 + digit(*low)?) as u8)
        })
        .collect()
}

// ============================================================
// The binary half.
// ============================================================

/// Mids' `BinaryReader` stream, as a cursor that refuses rather than panics at the end.
struct Stream<'a> {
    bytes: &'a [u8],
    at: usize,
}

/// Running off the end of the stream, which is how a wrong shape usually announces itself. Not
/// an [`MxdError`]: the shape search expects these and only the last one turns into a refusal.
struct Overrun;

impl<'a> Stream<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], Overrun> {
        let slice = self.bytes.get(self.at..self.at + count).ok_or(Overrun)?;
        self.at += count;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, Overrun> {
        Ok(self.take(1)?[0])
    }

    fn i8(&mut self) -> Result<i8, Overrun> {
        Ok(self.u8()? as i8)
    }

    fn u16(&mut self) -> Result<u16, Overrun> {
        let bytes: [u8; 2] = self.take(2)?.try_into().map_err(|_| Overrun)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, Overrun> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| Overrun)?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn f32(&mut self) -> Result<f32, Overrun> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| Overrun)?;
        Ok(f32::from_le_bytes(bytes))
    }

    /// A .NET `BinaryWriter` string: a 7-bit-encoded length, then that many UTF-8 bytes.
    fn string(&mut self) -> Result<String, Overrun> {
        let mut length = 0usize;
        let mut shift = 0u32;
        loop {
            let byte = self.u8()?;
            length |= usize::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err(Overrun);
            }
        }
        Ok(String::from_utf8_lossy(self.take(length)?).into_owned())
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }
}

/// Read the inflated stream, choosing the record shape by which one fits.
///
/// "Fits" is: every record parses, every level and IO level is inside [`IMPLAUSIBLE_LEVEL`], and
/// the last record ends on the last byte. The exact-consumption half is what makes this a
/// measurement rather than a guess — a wrong shape over a ~1.5 KB stream of variable-length
/// records does not land on the final byte by luck, and across the corpus none ever has.
pub fn read_binary(stream: &[u8]) -> Result<MxdBinary, MxdError> {
    if !stream.starts_with(STREAM_MAGIC) {
        return Err(MxdError::NotAMidsStream);
    }
    // Through the cursor, not off a fixed offset. The magic is THREE bytes and the version sits
    // at four, so `starts_with` admits a stream a `&stream[4..]` then panics on — a block that
    // inflates to exactly `MxD` is the whole of it. Every other read in this file goes through
    // `Stream` for this reason; this one did not, and it was the one an attacker could reach
    // (F17).
    let mut header = Stream::new(stream);
    let version = header
        .take(STREAM_MAGIC.len() + 1)
        .and_then(|_| header.f32())
        .map_err(|_| MxdError::NotAMidsStream)?;

    let fits: Vec<MxdBinary> = MxdShape::CANDIDATES
        .into_iter()
        .filter_map(|shape| read_binary_as(stream, shape).ok())
        .collect();
    match fits.len() {
        1 => Ok(fits.into_iter().next().expect("one fit, just counted")),
        0 => Err(MxdError::NoShapeFits {
            version,
            candidates: MxdShape::CANDIDATES.len(),
        }),
        fits => Err(MxdError::AmbiguousShape { fits }),
    }
}

/// Read the stream under one candidate shape, or say it does not fit.
fn read_binary_as(stream: &[u8], shape: MxdShape) -> Result<MxdBinary, Overrun> {
    let mut cursor = Stream::new(stream);
    cursor.take(STREAM_MAGIC.len())?;
    let _format_revision = cursor.u8()?;
    let format_version = cursor.f32()?;
    let _reserved = cursor.take(2)?;

    let class_name = cursor.string()?;
    let origin = cursor.string()?;
    let alignment = cursor.i32()?;
    let character_name = cursor.string()?;

    // Mids writes the index of the last element rather than the count for its fixed-length
    // arrays, and the count for the power list. Both conventions are in the same header, so
    // neither can be inferred from the other.
    let last_powerset = cursor.i32()?;
    if !(0..64).contains(&last_powerset) {
        return Err(Overrun);
    }
    let powersets = (0..=last_powerset)
        .map(|_| cursor.string())
        .collect::<Result<Vec<_>, _>>()?;

    let last_power = cursor.i32()?;
    let last_entry = cursor.i32()?;
    if !(0..4096).contains(&last_entry) {
        return Err(Overrun);
    }
    let entries = (0..=last_entry)
        .map(|_| read_entry(&mut cursor, shape))
        .collect::<Result<Vec<_>, _>>()?;

    if cursor.remaining() != 0 {
        return Err(Overrun);
    }
    Ok(MxdBinary {
        format_version,
        shape,
        class_name,
        origin,
        alignment,
        character_name,
        powersets,
        last_power,
        entries,
    })
}

fn read_entry(cursor: &mut Stream<'_>, shape: MxdShape) -> Result<MxdEntry, Overrun> {
    let power_id = cursor.i32()?;
    if power_id < 0 {
        // Mids skips the whole middle of an unassigned entry and writes only its slot count,
        // which is always "none". Reading the middle anyway is what turns one such entry into a
        // stream of garbage records.
        let slots = read_slots(cursor, shape)?;
        return if slots.is_empty() {
            Ok(MxdEntry::Unassigned)
        } else {
            Err(Overrun)
        };
    }
    let level = cursor.i8()?;
    if !(-1..=IMPLAUSIBLE_LEVEL as i8).contains(&level) {
        return Err(Overrun);
    }
    let middle = cursor.take(shape.entry_middle)?;
    let stat_include = middle[0] != 0;
    // `ProcInclude` arrives at format 3.1. Before it, the byte at this offset is the low byte of
    // `VariableValue`, so reading it as a flag would make every targets-hit slider a proc toggle.
    let proc_include = shape.entry_middle >= MxdShape::V3_1.entry_middle && middle[1] != 0;
    let variable_value = variable_value(middle, shape);
    let _sub_powers = cursor.i8()?;
    let slots = read_slots(cursor, shape)?;
    Ok(MxdEntry::Power(MxdPowerRecord {
        power_id,
        level,
        stat_include,
        proc_include,
        variable_value,
        slots,
    }))
}

/// The per-power slider, out of whichever bytes of the entry's middle this shape puts it in.
///
/// Its width moved with the format: one `int` after `StatInclude` at 1.01, after
/// `StatInclude`/`ProcInclude` from 3.1. Reading the wrong four bytes is silent — the corpus's
/// sliders are 0, 1, 2, 5 and 100, all of which fit in the low byte either way — so the offset
/// is derived from the shape rather than shared between the two.
fn variable_value(middle: &[u8], shape: MxdShape) -> i32 {
    let offset = if shape.entry_middle >= MxdShape::V3_1.entry_middle {
        2
    } else {
        1
    };
    middle
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map_or(0, i32::from_le_bytes)
}

fn read_slots(cursor: &mut Stream<'_>, shape: MxdShape) -> Result<Vec<MxdSlot>, Overrun> {
    let last_slot = cursor.i8()?;
    (0..=i32::from(last_slot))
        .map(|_| read_slot(cursor, shape))
        .collect()
}

fn read_slot(cursor: &mut Stream<'_>, shape: MxdShape) -> Result<MxdSlot, Overrun> {
    let level = if shape.wide_slot_level {
        cursor.u16()? as i16
    } else {
        i16::from(cursor.i8()?)
    };
    if !(-1..=IMPLAUSIBLE_LEVEL as i16).contains(&level) {
        return Err(Overrun);
    }
    let enhancement = read_enhancement(cursor)?;
    let obtained = cursor.u8()? != 0;
    let flipped = if shape.flipped_enhancement {
        read_enhancement(cursor)?
    } else {
        None
    };
    Ok(MxdSlot {
        level,
        enhancement,
        obtained,
        flipped,
    })
}

/// Mids' `I9Slot`: an index, and the two level fields it writes only when the index names
/// something.
fn read_enhancement(cursor: &mut Stream<'_>) -> Result<Option<MxdEnhancementRef>, Overrun> {
    let index = cursor.i32()?;
    if index < 0 {
        return Ok(None);
    }
    let fields = [cursor.u8()?, cursor.u8()?];
    if i32::from(fields[0]) > IMPLAUSIBLE_LEVEL {
        return Err(Overrun);
    }
    Ok(Some(MxdEnhancementRef { index, fields }))
}

// ============================================================
// The prose half.
// ============================================================

/// Read the forum-post half: everything above the data block.
///
/// Returns `None` where the text carries no `Level n:` line at all, which is a real document
/// rather than a malformed one — one corpus file is the bare data block and nothing else.
pub fn read_prose(text: &str) -> Option<MxdProse> {
    let mut header = Vec::new();
    let mut rows = Vec::new();
    for line in unmarkup(text).lines() {
        match prose_row(line) {
            Some(row) => rows.push(row),
            None => {
                let line = line.trim();
                if !line.is_empty() && rows.is_empty() {
                    header.push(line.to_string());
                }
            }
        }
    }
    (!rows.is_empty()).then_some(MxdProse { header, rows })
}

/// The post's markup taken out, and nothing else changed.
///
/// Mids writes the 1.01 post as HTML — `<br />` for the newline and `&nbsp;` runs for the column
/// gutter — and the 3.x post as plain text with tabs. Both spellings survive a forum's round
/// trip, so both arrive here. The non-breaking space becomes an ordinary one because it is doing
/// the job of one; `<br />` becomes a newline because the whole post is otherwise a single line.
fn unmarkup(text: &str) -> String {
    text.replace("<br />", "\n")
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace("&nbsp;", " ")
        .replace('\u{a0}', " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// One `Level n:` line, or `None` for anything else in the post.
fn prose_row(line: &str) -> Option<MxdProseRow> {
    let rest = line.trim_start().strip_prefix("Level ")?;
    let (level, rest) = rest.split_once(':')?;
    let level: u8 = level.trim().parse().ok()?;

    // The post separates the power's name from its enhancements with a run of gutter, which is
    // tabs in the 3.x spelling and two-or-more spaces in the 1.01 one. A single space is inside
    // a name ("Battle Agility"), so the run is what the split has to be on.
    let mut columns = rest
        .split('\t')
        .flat_map(|column| column.split("  "))
        .map(str::trim)
        .filter(|column| !column.is_empty());
    let power = columns.next()?.to_string();
    let enhancements = columns
        .collect::<Vec<_>>()
        .join(", ")
        .split(',')
        .map(str::trim)
        .filter(|code| !code.is_empty())
        .map(prose_enhancement)
        .collect();
    Some(MxdProseRow {
        level,
        power,
        enhancements,
    })
}

/// One `Ags-ResDam/EndRdx:50(7)` off the post.
///
/// Everything is optional but the code itself, because the three spellings in the corpus each
/// leave something out: the 3.x post writes no `:50`, every power's first slot is `(A)` rather
/// than a level, and a power with no slots at all writes nothing.
fn prose_enhancement(code: &str) -> MxdProseEnhancement {
    let (head, slot_level) = match code.rsplit_once('(') {
        Some((head, tail)) => {
            let slot = tail.trim_end_matches(')');
            (head, slot.parse::<u8>().ok())
        }
        None => (code, None),
    };
    // Only a trailing `:<digits>` is the IO level. `HO:Nucle` carries a colon that is part of the
    // code, so splitting on the first one would turn every Hamidon in the corpus into a level.
    let (code, io_level) = match head.rsplit_once(':') {
        Some((stem, tail)) if !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()) => {
            (stem, tail.parse::<u8>().ok())
        }
        _ => (head, None),
    };
    MxdProseEnhancement {
        code: code.trim().to_string(),
        io_level,
        slot_level,
    }
}
