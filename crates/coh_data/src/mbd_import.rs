//! A `.mbd`'s slotted enhancements, resolved against a dataset.
//!
//! [`crate::mbd`] reads the document; this module turns one `Uid` into the piece a build slots.
//! The split is the same one [`crate::game_import`] makes and for the same reason — a file that
//! will not parse is broken, a record this fork has never heard of is a build to report on —
//! but the two formats fail differently enough that this is a sibling rather than a caller.
//!
//! **The UID belongs to Mids, and Mids resolves it by substring match.** A miss sets
//! `I9Slot.Enh = -1` and the slot comes up empty with no error and no log, so every failure this
//! format has is silent on Mids' side too. That is why a refusal here is returned and named
//! rather than dropped.
//!
//! # The two doors
//!
//! 1. [`BoostIndex::get`] — the game's own record namespace, which Rule 0 makes the authority.
//!    It answers 1,225 of Homecoming's 1,231 Mids UIDs and 1,137 of Rebirth's 1,266.
//! 2. [`MidsUids::set_piece`] — Mids' table, for the spellings where Mids has drifted from the
//!    game. It answers with a SET AND A PIECE, which is a coordinate and not a record: the game
//!    holds the crafted and the attuned forms of one piece under two names at the same
//!    coordinate. The Mids UID's own attunement prefix narrows the pair, and **anything still
//!    ambiguous is refused rather than guessed**.
//!
//! Twelve UIDs on one fork survive both doors — Rebirth's Rolling Barrage and Synapse's Agility,
//! six pieces each, spelled with no prefix at a coordinate holding two records (DATA-GAP
//! MBDIMPORT-9). Every other UID on every fork resolves: 1,225 + 6 on Homecoming and Brainstorm,
//! 1,137 + 117 on Rebirth, 1,137 + 0 on Thunderspy.
//!
//! In the corpus, door 2 is reached for 11 of Homecoming's 296 slotted enhancements and 66 of
//! Rebirth's 246; the 12 that refuse are Rolling Barrage, six in each of two builds. Nothing
//! falls past both doors.
//!
//! # What the record does NOT say
//!
//! **The origin tier is Mids' `Grade`, never the record's.** Mids carries one origin record per
//! stat — 26 of them, all spelled `Magic_<Stat>` — and the tier lives in the slot's `Grade`
//! field. The corpus Widow slots `Magic_Accuracy` at `TrainingO`, `DualO` and `SingleO` in one
//! file, and its character is Natural, so neither the tier nor the origin is in that name. The
//! boost index resolves `Magic_Accuracy` to the game's Magic SINGLE origin record, confidently
//! and wrongly for 43 of the corpus's 542 pieces, and taking its tier would be the port's own
//! version of the bug MBDIMPORT-6 closed. So the family comes from the index and the tier comes
//! from the grade.
//!
//! **`RelativeLevel` means two different things.** On an origin or special piece it is the
//! level relative to the character (`MinusThree` … `PlusFive`), which is what
//! [`SkifEnhancement::Origin`] and [`SkifEnhancement::Special`] carry. On an invention piece it
//! is the enhancement BOOSTER count, which is what [`SkifEnhancement::IoSet`] and
//! [`SkifEnhancement::IoGeneric`] carry — Mids has no separate booster field, and its own UI
//! prints a boosted level-50 IO as "50+5". Measured across the corpus the two populations do
//! not overlap: every invention piece is `Even` or `PlusFive` and every `Minus*` is on an
//! origin. A negative on an invention piece is a contradiction and is refused, not clamped.
//!
//! **An attuned piece's `IoLevel` is discarded.** Mids writes a real crafted level on one
//! (level 10 on the corpus's `Attuned_Command_of_the_Mastermind_A`); an attuned piece scales
//! with the character and has no level of its own, so carrying it would slot the piece at a
//! level the game does not give it.

use crate::boost_index::{BoostEntry, BoostIndex, OriginTier};
use crate::enhancements::EnhancementCatalog;
use crate::level::Level;
use crate::mbd::MbdEnhancement;
use crate::mids_uids::{MidsFamily, MidsUids};
use crate::skif::SkifEnhancement;

/// A slotted piece that could not be resolved, and the UID that named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedPiece {
    /// The UID as the `.mbd` spelled it, kept verbatim so a report can be searched for it and
    /// so a re-export can still write what the author had.
    pub uid: String,
    pub refusal: PieceRefusal,
}

/// Why a UID could not become a slotted piece.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PieceRefusal {
    #[error("the slot carries an enhancement with no Uid")]
    NoUid,
    #[error("neither this dataset's boost index nor its copy of Mids' table carries that UID{}", family_note(*.mids_family))]
    UnknownUid { mids_family: Option<MidsFamily> },
    #[error("Mids places this UID at {set} piece {piece}, where this dataset's boost index has no record")]
    SetPieceAbsent { set: String, piece: u8 },
    #[error("Mids places this UID at {set} piece {piece}, where the game has {} records ({}) and the UID's own spelling narrows to none of them", records.len(), records.join(", "))]
    Ambiguous {
        set: String,
        piece: u8,
        records: Vec<String>,
    },
    #[error("the boost index places this record in no family, so there is nothing to slot")]
    Unclassified,
    #[error("the record enhances {named:?}, of which this dataset's catalog carries {carried:?}; a generic piece needs exactly one")]
    Stat {
        named: Vec<String>,
        carried: Vec<String>,
    },
    #[error("the record is crafted at level {record}, but the file states IoLevel {stated}")]
    RecordLevelDisagrees { record: i64, stated: i32 },
    #[error("IoLevel {stated} is not a level a piece can be crafted at")]
    IoLevelOutOfRange { stated: i32 },
    #[error("{stated:?} is not one of Mids' relative levels")]
    RelativeLevel { stated: String },
    #[error("an invention piece is boosted, never levelled, so {stated:?} cannot be read as a booster count")]
    NegativeBooster { stated: String },
    #[error("the record is an origin enhancement and only Mids' grade states its tier, but the file grades it {stated:?}")]
    Grade { stated: String },
}

/// The family clause on [`PieceRefusal::UnknownUid`] — Mids can often say what KIND of thing a
/// UID is even where nothing can say which one, and naming it is the difference between a
/// report and an unrecognized string.
fn family_note(family: Option<MidsFamily>) -> &'static str {
    match family {
        Some(MidsFamily::GenericIo) => "; Mids files it as a generic IO",
        Some(MidsFamily::Special) => "; Mids files it as a special enhancement",
        Some(MidsFamily::Origin) => "; Mids files it as an origin enhancement",
        None => "",
    }
}

/// Resolve one `.mbd` slot's enhancement into the piece a build slots.
///
/// No character level is needed, unlike [`crate::game_import::resolve_enhancement`]: a
/// `/buildsave` states an origin piece's absolute level and leaves the subtraction to the
/// reader, while a `.mbd` states the relative level directly.
pub fn resolve_enhancement(
    enhancement: &MbdEnhancement,
    index: &BoostIndex,
    mids: &MidsUids,
    catalog: &EnhancementCatalog,
) -> Result<SkifEnhancement, UnresolvedPiece> {
    let refuse = |refusal| UnresolvedPiece {
        uid: enhancement.uid.clone(),
        refusal,
    };

    if enhancement.uid.is_empty() {
        return Err(refuse(PieceRefusal::NoUid));
    }
    let entry = locate(&enhancement.uid, index, mids).map_err(refuse)?;

    match entry {
        BoostEntry::IoSet {
            set,
            piece,
            attuned,
        } => Ok(SkifEnhancement::IoSet {
            set_id: set.clone(),
            piece_num: *piece,
            attuned: *attuned,
            level: match attuned {
                true => None,
                false => Some(crafted_level(enhancement).map_err(refuse)?),
            },
            booster: booster(enhancement).map_err(refuse)?,
        }),

        BoostEntry::CommonIo { level, stats } => {
            if let Some(record) = level {
                if *record != i64::from(enhancement.io_level) + 1 {
                    return Err(refuse(PieceRefusal::RecordLevelDisagrees {
                        record: *record,
                        stated: enhancement.io_level,
                    }));
                }
            }
            Ok(SkifEnhancement::IoGeneric {
                stat: catalog_stat(stats, catalog).map_err(refuse)?,
                level: Some(crafted_level(enhancement).map_err(refuse)?),
                booster: booster(enhancement).map_err(refuse)?,
            })
        }

        // `tier` is deliberately not read — see the module note. The record is the game's
        // single-origin spelling because that is the only origin spelling Mids has.
        BoostEntry::Origin { stats, .. } => Ok(SkifEnhancement::Origin {
            stat: catalog_stat(stats, catalog).map_err(refuse)?,
            tier: tier_name(origin_tier(&enhancement.grade).map_err(refuse)?).to_string(),
            relative_level: relative_level(enhancement).map_err(refuse)?,
        }),

        BoostEntry::Special { family, id } => Ok(SkifEnhancement::Special {
            category: family.clone(),
            id: id.clone(),
            relative_level: relative_level(enhancement).map_err(refuse)?,
        }),

        BoostEntry::Unclassified => Err(refuse(PieceRefusal::Unclassified)),
    }
}

/// Which boost record a Mids UID names, through the two doors.
fn locate<'a>(
    uid: &str,
    index: &'a BoostIndex,
    mids: &MidsUids,
) -> Result<&'a BoostEntry, PieceRefusal> {
    if let Some(entry) = index.get(uid) {
        return Ok(entry);
    }
    let Some(placed) = mids.set_piece(uid) else {
        return Err(PieceRefusal::UnknownUid {
            mids_family: mids.family(uid),
        });
    };

    let records = index.io_set_records(placed.set, placed.piece);
    if records.is_empty() {
        return Err(PieceRefusal::SetPieceAbsent {
            set: placed.set.to_string(),
            piece: placed.piece,
        });
    }

    // The narrowing. A stated prefix picks its record outright.
    let wanted = placed.prefix.attuned();
    let is_attuned = |name: &String| match index.get(name) {
        Some(BoostEntry::IoSet { attuned, .. }) => Some(*attuned),
        _ => None,
    };
    let matching: Vec<&String> = records
        .iter()
        .filter(|name| is_attuned(name).is_some_and(|a| wanted.is_none_or(|want| want == a)))
        .collect();

    let chosen = match matching.as_slice() {
        [only] => Some(*only),

        // MBDIMPORT-9. `Bare` still states nothing on its own — the doc on `MidsUidPrefix::Bare`
        // is why — so where the coordinate holds one record it takes it, attuned or not, and
        // Rebirth's 117 attuned-only bare pieces resolve as they always did.
        //
        // Where it holds BOTH, bare resolves to the crafted record. Mids marks attunement
        // positively and twice — an `Attuned_`/`Superior_Attuned_` prefix, or `recipe_name ==
        // "Alt"` — and neither marker ever appears on a crafted record (0 of 783 on Rebirth).
        // The marked sets are exactly the archetype-origin, Winter and Universal families, plus
        // 12 Rebirth customs; no set is spelled both bare and prefixed, so a bare set is one the
        // convention was never applied to rather than a second spelling of a marked one. An
        // unmarked set with a marked peer in the same family is therefore unmarked by choice.
        //
        // This is an inference from absence, which is normally exactly the mistake this module
        // avoids. What licenses it is the marked peers plus a render: Mids draws these pieces as
        // crafted IOs at a level, not in an attuned frame. That render is the author's
        // recollection rather than a captured artifact — to be verified against a live Mids, and
        // the row says so.
        //
        // The guard is the corpus census: the bare-at-a-two-record-coordinate
        // population is those twelve UIDs and nothing else. A set carrying the `Alt` marker
        // joining that population would make this preference wrong, and would go red first.
        _ if wanted.is_none() => {
            let mut crafted = matching
                .iter()
                .filter(|name| is_attuned(name) == Some(false));
            match (crafted.next(), crafted.next()) {
                (Some(one), None) => Some(*one),
                _ => None,
            }
        }

        _ => None,
    };

    match chosen {
        Some(name) => index.get(name).ok_or(PieceRefusal::SetPieceAbsent {
            set: placed.set.to_string(),
            piece: placed.piece,
        }),
        None => Err(PieceRefusal::Ambiguous {
            set: placed.set.to_string(),
            piece: placed.piece,
            records: records.to_vec(),
        }),
    }
}

/// The tier as the build spells it, matching [`crate::game_import`]'s.
fn tier_name(tier: OriginTier) -> &'static str {
    match tier {
        OriginTier::Training => "TO",
        OriginTier::Dual => "DO",
        OriginTier::Single => "SO",
    }
}

/// Mids' grade tokens, which are the ONLY place a `.mbd` states an origin piece's tier.
///
/// Never `TO`/`DO`/`SO` — those are ours, and testing for them is the dead branch MBDIMPORT-6
/// closed. `None` is a grade too, and on an origin record it is a contradiction rather than a
/// default: the file would be saying the piece has no tier.
fn origin_tier(grade: &str) -> Result<OriginTier, PieceRefusal> {
    match grade {
        "TrainingO" => Ok(OriginTier::Training),
        "DualO" => Ok(OriginTier::Dual),
        "SingleO" => Ok(OriginTier::Single),
        other => Err(PieceRefusal::Grade {
            stated: other.to_string(),
        }),
    }
}

/// Mids' `eEnhRelative`, as a signed offset.
///
/// Exhaustive on purpose. The beta's table reached the same nine through a `?? 0`, and the
/// fallback is what let its missing negative half read every red SO as fresh (MBDIMPORT-4) —
/// a token nobody has seen must fail here rather than arrive as "even".
///
/// `None` is in Mids' enum and carries no offset. It appears once in the corpus, on a slotted
/// origin piece, and is read as even on MBDIMPORT-4's terms.
const RELATIVE_LEVEL: [(&str, i8); 10] = [
    ("None", 0),
    ("MinusThree", -3),
    ("MinusTwo", -2),
    ("MinusOne", -1),
    ("Even", 0),
    ("PlusOne", 1),
    ("PlusTwo", 2),
    ("PlusThree", 3),
    ("PlusFour", 4),
    ("PlusFive", 5),
];

/// Mids' token for one of its own `eEnhRelative` ordinals, which is what a `.mxd` writes where
/// a `.mbd` writes the name. `None` for an ordinal Mids' enum has no member at — the same
/// refusal the token lookup makes, asked from the other side.
pub fn relative_level_token(ordinal: u8) -> Option<&'static str> {
    RELATIVE_LEVEL
        .get(usize::from(ordinal))
        .map(|(token, _)| *token)
}

fn relative_level(enhancement: &MbdEnhancement) -> Result<i8, PieceRefusal> {
    RELATIVE_LEVEL
        .into_iter()
        .find(|(token, _)| *token == enhancement.relative_level)
        .map(|(_, offset)| offset)
        .ok_or_else(|| PieceRefusal::RelativeLevel {
            stated: enhancement.relative_level.clone(),
        })
}

/// The same field, read as what it means on an invention piece.
fn booster(enhancement: &MbdEnhancement) -> Result<u8, PieceRefusal> {
    let offset = relative_level(enhancement)?;
    u8::try_from(offset).map_err(|_| PieceRefusal::NegativeBooster {
        stated: enhancement.relative_level.clone(),
    })
}

/// `IoLevel` is 0-based: 49 is level 50.
fn crafted_level(enhancement: &MbdEnhancement) -> Result<Level, PieceRefusal> {
    Level::from_i64(i64::from(enhancement.io_level) + 1).ok_or(PieceRefusal::IoLevelOutOfRange {
        stated: enhancement.io_level,
    })
}

/// The one thing a generic piece enhances, in the catalog's own vocabulary.
///
/// The twin of [`crate::game_import`]'s, and for its reason: a record may name more than the
/// catalog carries (every `_Heal` record names `["Healing", "Absorb"]` and the catalog has no
/// `Absorb`), so the intersection decides, and two survivors is a question rather than a tie to
/// break.
fn catalog_stat(named: &[String], catalog: &EnhancementCatalog) -> Result<String, PieceRefusal> {
    let carried: Vec<String> = named
        .iter()
        .filter(|stat| catalog.common_io_types.contains(stat))
        .cloned()
        .collect();
    match carried.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(PieceRefusal::Stat {
            named: named.to_vec(),
            carried,
        }),
    }
}

// ============================================================
// The build.
// ============================================================

use crate::accolades::ACCOLADE_CATEGORY;
use crate::build_sets::{archetype_for_class, bucket_of, set_powers, Bucket, Roles, SetLookup};
use crate::inherent_grants::auto_granted_slot_count;
use crate::leveling_schedule::LevelingSchedule;
use crate::mbd::{MbdFile, MbdPowerEntry};
use crate::mids_names::MidsNames;
use crate::skif::{SkifBuild, SkifIncarnate, SkifPower, SkifSelection};
use crate::{DatasetId, PowerDatabase};
use std::collections::HashMap;

/// Something the file named that this dataset has no record of, or a fact about the read the
/// user has to be told.
///
/// The twin of [`crate::game_import::ImportNote`] and kept beside the build for its reason: the
/// conversion happens BEFORE [`crate::skif::hydrate`], so anything refused here never reaches
/// the list `hydrate` keeps (rule 8). The two lists are joined by the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNote {
    /// What the `.mbd` called the thing, so the note can be matched to an entry in the file.
    pub context: String,
    pub detail: String,
}

/// What the read accounted for, reconciled against what the file holds.
///
/// **Every count is against the FILE, not against a running tally** (MBDIMPORT-5). A `.mbd`
/// has a dozen ways for an entry to be declined, and each one used to leave whatever that
/// entry was carrying in no number at all: the corpus Stalker held 92 enhancements, imported
/// 86, and reported zero failures — six pieces in the file and in no number the user was
/// shown. So the arithmetic here is done once, at the foot of the loop, over what the entry
/// arrived holding ([`MbdPowerEntry::enhancement_count`]) against what actually landed. A
/// refusal path added later cannot bypass a subtraction it never has to reach.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MbdSummary {
    /// Picks that reached a bucket. Granted entries are not picks and are not counted here.
    pub picks_imported: usize,
    /// Entries naming a power, that named nothing this dataset carries.
    pub powers_declined: usize,
    pub accolades_imported: usize,
    pub incarnates_imported: usize,
    /// Slots the file states, across every entry.
    pub slots_in_file: usize,
    /// Slots that reached a power in the build.
    pub slots_imported: usize,
    /// Enhancements the file states — the denominator every other enhancement count is
    /// reconciled against.
    pub enhancements_in_file: usize,
    pub enhancements_imported: usize,
    /// Everything the file held that did not land: a piece that refused, plus every piece
    /// carried out of the build by an entry that was declined after its slots were read.
    pub enhancements_failed: usize,
}

impl MbdSummary {
    /// Does the read account for every enhancement the file holds?
    ///
    /// The property MBDIMPORT-5 exists to make true. It is asserted rather than assumed
    /// because the interesting direction is the one the per-entry arithmetic cannot catch —
    /// a piece counted twice, which is what a dropped duplicate looked like before the claim
    /// guard handed its count back.
    pub fn reconciles(&self) -> bool {
        self.enhancements_imported + self.enhancements_failed == self.enhancements_in_file
    }
}

/// The character level a `.mbd` does not state, and the two readings it stands on
/// (MBDIMPORT-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DerivedLevel {
    /// The level the build imports at.
    pub level: u8,
    /// Mids' own `GetMaxLevel()` reconstruction — the sourced lower bound.
    pub floor: u8,
    /// Budget slots the finished build places, counted at [`Self::level`].
    pub placed_slots: usize,
    /// Slot budget at [`Self::level`] on this fork.
    pub slot_budget: usize,
    /// The schedule raised the level above Mids' floor — the corpus Warshade's case, and the
    /// reason the import can say the level was derived rather than read. Rule 1 makes
    /// surfacing it the caller's obligation on the same terms as its neighbour below:
    /// reporting only the overrun left a raise that fits saying nothing (MBDIMPORT-15).
    pub raised_from_floor: bool,
    /// The placements do not fit even at the cap. The level is still returned, and rule 1
    /// makes surfacing this the caller's obligation rather than its option.
    pub over_budget_at_cap: bool,
    /// Budget slots placed beyond what this server's schedule grants at [`Self::level`] — 0
    /// for any build that fits. Non-zero means Mids' own slot table allowed more than the game
    /// does (MBDIMPORT-11), and the COUNT is the reportable thing, not the fact.
    pub excess_over_server_budget: usize,
}

/// A `.mbd` turned into the shape [`crate::skif::hydrate`] reads, and everything that did not
/// survive the turning.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub build: SkifBuild,
    pub unresolved: Vec<ImportNote>,
    pub summary: MbdSummary,
    pub level: DerivedLevel,
}

/// Mids' `GetMaxLevel()`, recomputed from the file's own contents: the highest level any power
/// pick or slot placement occupies.
///
/// The file's stored `Level` is this minus one, so this is the same number Mids would show,
/// read from the data it is derived from rather than from the derived field. Power and slot
/// levels are written 1-based, so this is already a game level.
///
/// Negative levels are skipped rather than clamped: an unfilled row is written with `-1`, and
/// it is a hole in the list rather than a placement at a level.
pub fn mids_max_used_level(file: &MbdFile) -> u8 {
    let levels = file.power_entries.iter().flat_map(|entry| {
        std::iter::once(entry.level).chain(entry.slot_entries.iter().map(|slot| slot.level))
    });
    levels
        .filter_map(|level| u8::try_from(level).ok())
        .max()
        .unwrap_or(0)
}

/// Whether the file's stored `Level` still equals the value Mids derives it from.
///
/// The identity the whole level rule rests on: `LoadBuild` never reads the field, and
/// `Character.Level` is recomputed as `Build.GetMaxLevel()`, so the stored number is a
/// characterisation of the BUILD rather than a statement about the CHARACTER. It held on 8 of
/// 8 corpus files across two Mids versions fourteen months apart. A future Mids that stores
/// something else there is a reader taking its floor from a field that no longer means what
/// this says it means, and the gate that pins this is where that surfaces.
pub fn stated_level_matches_derivation(file: &MbdFile) -> bool {
    file.level
        .trim()
        .parse::<i64>()
        .is_ok_and(|stated| stated == i64::from(mids_max_used_level(file)) - 1)
}

/// Convert a `.mbd` into a build, against `database`.
///
/// **Nothing here parses a name.** A powerset is found by handing the path the file spells to
/// the dataset's own set-path index, through Mids' alias table where the two namespaces have
/// drifted apart (MBDIMPORT-7); a power is found by asking [`MidsNames`] what this dataset
/// calls the name Mids used, and a name that powerset ROTATED AWAY from is refused rather than
/// bound to the same-named power that now holds it (MBDIMPORT-2). A bucket is decided by which
/// catalogue carries the resolved set, never by the group token the path opens with — the
/// exception being the granted `Inherent` group, which names no set a build holds.
///
/// `dataset` is the fork this file is read INTO. [`crate::mbd::probe_dataset`] is what answers
/// whether the file names another one, and refusing on that is the caller's decision, not this
/// function's: a build authored in Mids' `Generic` database names no fork at all.
pub fn to_skif_build(file: &MbdFile, database: &PowerDatabase, dataset: DatasetId) -> Converted {
    let mut notes = Vec::new();
    let mut summary = MbdSummary::default();

    let names = database.mids_names.as_ref();
    if names.is_none() {
        notes.push(ImportNote {
            context: dataset.as_str().to_string(),
            detail: "this dataset ships no Mids name map, so every power name is matched as \
                     Mids spelled it; a name this fork has rotated onto another power binds \
                     that power instead"
                .to_string(),
        });
    }

    let sets = SetLookup::of(database);
    let archetype = archetype_id(&file.class, database, &mut notes);
    let roles = archetype.as_deref().and_then(|id| Roles::of(id, database));

    let mut build = SkifBuild {
        name: file.name.clone(),
        dataset: dataset.as_str().to_string(),
        archetype,
        origin: (!file.origin.is_empty()).then(|| file.origin.clone()),
        // Provisional. The level is settled below, once the finished build can be counted.
        level: 0,
        ..SkifBuild::default()
    };

    open_selections(file, &mut build, &sets, names, database, &mut notes);

    // Which Mids entry took each resolved power. A REPEAT of one Mids name is an ordinary
    // duplicate and stays silent — Mids files carry those — while two DIFFERENT names landing
    // on one power is a collision, and it was the quiet half of MBDIMPORT-2: the loser's slots
    // went with it under `warnings: []`.
    let mut claimed: HashMap<(String, String), String> = HashMap::new();

    for (index, entry) in file.power_entries.iter().enumerate() {
        if entry.power_name.trim().is_empty() {
            continue;
        }
        let held = entry.enhancement_count();
        let slots_held = entry.slot_entries.len();
        summary.enhancements_in_file += held;
        summary.slots_in_file += slots_held;

        let granted = usize::try_from(file.last_power).is_ok_and(|last| index >= last);
        let outcome = convert_entry(entry, granted, &sets, names, &build, database);
        notes.extend(outcome.notes);
        if outcome.declined {
            summary.powers_declined += 1;
        }

        let placed = place(
            outcome.placement,
            &mut build,
            &mut claimed,
            entry,
            roles.as_ref(),
            database,
            &mut summary,
            &mut notes,
        );

        // The accounting, once, over what the entry ARRIVED holding. Not at each refusal site
        // — a count a refusal path has to remember to reach is a count the next path added
        // will miss, which is the defect MBDIMPORT-5 closed.
        if placed {
            summary.enhancements_imported += outcome.pieces_resolved;
            summary.slots_imported += outcome.slots_read;
        }
        let landed = if placed { outcome.pieces_resolved } else { 0 };
        let lost = held.saturating_sub(landed);
        summary.enhancements_failed += lost;
        let unexplained = lost.saturating_sub(if placed { outcome.pieces_refused } else { 0 });
        if unexplained > 0 {
            notes.push(ImportNote {
                context: entry.power_name.clone(),
                detail: format!(
                    "{unexplained} slotted enhancement{} left the build with this entry",
                    if unexplained == 1 { "" } else { "s" }
                ),
            });
        }
    }

    let level = settle_level(file, &build, database);
    build.level = level.level;
    notes.extend(level_note(level, dataset));

    if !summary.reconciles() {
        notes.push(ImportNote {
            context: "reconciliation".to_string(),
            detail: format!(
                "the read accounted for {} enhancements and the file holds {}",
                summary.enhancements_imported + summary.enhancements_failed,
                summary.enhancements_in_file,
            ),
        });
    }

    Converted {
        build,
        unresolved: notes,
        summary,
        level,
    }
}

/// The archetype id whose class token the file states, with a miss said out loud.
fn archetype_id(
    class: &str,
    database: &PowerDatabase,
    notes: &mut Vec<ImportNote>,
) -> Option<String> {
    let found = archetype_for_class(class, database);
    if found.is_none() {
        notes.push(ImportNote {
            context: class.to_string(),
            detail: "no archetype in this dataset states that class token".to_string(),
        });
    }
    found
}

/// Open the buckets the file's `PowerSets` list names, before any entry is read.
///
/// **The list is positional and an empty string is a real entry** — the unpicked slot — so the
/// first two positions are the archetype's own roles whatever they hold, and everything past
/// them is a pool or an epic pool decided by which catalogue carries it. Opening them here
/// rather than on first use is what makes a powerset the author picked and never took a power
/// from survive the round trip.
fn open_selections(
    file: &MbdFile,
    build: &mut SkifBuild,
    sets: &SetLookup,
    names: Option<&MidsNames>,
    database: &PowerDatabase,
    notes: &mut Vec<ImportNote>,
) {
    for (position, path) in file.power_sets.iter().enumerate() {
        if path.trim().is_empty() {
            continue;
        }
        let Some(id) = resolve_set(path, sets, names) else {
            notes.push(ImportNote {
                context: path.clone(),
                detail: "no powerset, pool or epic pool in this dataset carries that set path"
                    .to_string(),
            });
            continue;
        };
        let selection = || SkifSelection {
            id: id.clone(),
            powers: Vec::new(),
        };
        match position {
            0 => build.primary = Some(selection()),
            1 => build.secondary = Some(selection()),
            _ => match bucket_of(&id, None, database) {
                // A pool named twice is one pool: Mids writes the same path into two positions
                // where a build re-picked one, and two selections carrying one id would render
                // as two pools and spend two of the build's pool allowance.
                Some(Bucket::Pool(_)) => {
                    if !build.pools.iter().any(|pool| pool.id == id) {
                        build.pools.push(selection());
                    }
                }
                Some(Bucket::Epic(_)) => build.epic_pool = Some(selection()),
                _ => notes.push(ImportNote {
                    context: path.clone(),
                    detail: format!(
                        "the file lists {id:?} past its two archetype sets, where only a pool \
                         or an epic pool can go"
                    ),
                }),
            },
        }
    }
}

/// This dataset's id for a set path a `.mbd` spelled.
///
/// Two hops, both lookups. Mids' path becomes ours through the alias table — which carries
/// only the pairs that DRIFTED, so an unlisted path passes through unchanged and fails at the
/// second hop rather than here (MBDIMPORT-7 is the drift this exists for: Mids spells the
/// Rebirth Guardian's group `Guardian_Composition` where the export says `Guardian_Comp`).
/// Then the dataset's own set-path index answers with the id a build stores.
fn resolve_set(mids_path: &str, sets: &SetLookup, names: Option<&MidsNames>) -> Option<String> {
    sets.resolve_path(&powerset_key(mids_path, names))
        .map(str::to_string)
}

/// The key both the alias table and the name map are keyed on: our spelling of Mids' path.
fn powerset_key(mids_path: &str, names: Option<&MidsNames>) -> String {
    let trimmed = trim_segments(mids_path);
    match names {
        Some(names) => names.powerset_key(&trimmed),
        None => trimmed.to_lowercase(),
    }
}

/// `Guardian_Composition.Energy_Composition .Kinetic_Shield` — Mids writes trailing whitespace
/// inside segments on some Rebirth exports, and every lookup below compares whole segments.
fn trim_segments(name: &str) -> String {
    name.split('.').map(str::trim).collect::<Vec<_>>().join(".")
}

/// Where one entry belongs, once it has been read.
enum Placement {
    /// A power that resolved to a set, with the set it belongs to. The bucket is decided at
    /// placement time, so one rule serves every entry.
    Power {
        set_id: String,
        power: SkifPower,
    },
    Accolade(String),
    Incarnate {
        slot: String,
        pick: SkifIncarnate,
    },
    /// Nothing to carry: a temporary power, a granted entry the dataset re-derives whole, or
    /// an entry that was declined (whose note is already in `notes`).
    Nothing,
}

/// One entry, read. Counts are of what the read REACHED, so the caller can reconcile them
/// against what the entry arrived holding without re-walking the slots.
struct EntryOutcome {
    placement: Placement,
    notes: Vec<ImportNote>,
    pieces_resolved: usize,
    pieces_refused: usize,
    slots_read: usize,
    /// The entry named something and this dataset could not answer, as against an entry with
    /// nothing to carry. Both leave the build without a pick and only one is worth counting,
    /// and keeping them apart here is what stops `powers_declined` reporting every Sprint row
    /// in every file as a failure.
    declined: bool,
}

impl EntryOutcome {
    fn nothing() -> EntryOutcome {
        EntryOutcome {
            placement: Placement::Nothing,
            notes: Vec::new(),
            pieces_resolved: 0,
            pieces_refused: 0,
            slots_read: 0,
            declined: false,
        }
    }

    fn declined(context: &str, detail: String) -> EntryOutcome {
        EntryOutcome {
            notes: vec![ImportNote {
                context: context.to_string(),
                detail,
            }],
            declined: true,
            ..EntryOutcome::nothing()
        }
    }
}

/// Read one `PowerEntries` row.
fn convert_entry(
    entry: &MbdPowerEntry,
    granted: bool,
    sets: &SetLookup,
    names: Option<&MidsNames>,
    build: &SkifBuild,
    database: &PowerDatabase,
) -> EntryOutcome {
    let full = trim_segments(&entry.power_name);
    let segments: Vec<&str> = full.split('.').collect();
    let [group, set, power_name] = segments[..] else {
        return EntryOutcome::declined(
            &full,
            "a power is named by three segments — group, set, power — and this names \
             something else"
                .to_string(),
        );
    };
    let group_path = format!("{group}.{set}");

    if let Some(slot) = incarnate_slot(&group_path, database) {
        return incarnate_pick(entry, slot, power_name, &full, database);
    }
    if let Some(set_id) = resolve_set(&group_path, sets, names) {
        if is_accolade_set(&set_id, database) {
            return accolade(entry, power_name, &full, database);
        }
    }
    // Everything else under the temporary-powers group is a day-job power or a mission temp,
    // and the planner models none of them. Silent, because a build carrying one is ordinary.
    if group.eq_ignore_ascii_case(TEMPORARY_GROUP) {
        return EntryOutcome::nothing();
    }

    // **Granted is a group as well as a position.** `LastPower` is Mids' own boundary and it is
    // kept loosely — the corpus Stalker states 25 against 24 picks, with its archetype inherent
    // sitting inside the picked range — so reading position alone spends a power pick on
    // `Assassination`.
    let granted_group = group.eq_ignore_ascii_case(INHERENT_GROUP);
    let granted = granted || granted_group;

    // A granted entry with nothing on it is not carried. The roster of what a build is granted
    // is the dataset's (rule 6), so the file is worth reading only for what the author ADDED to
    // one. That is what the older Mids exports' `_H` henchman shadow rows are
    // (`Mastermind_Summon.Mercenaries.Soldier_H`, three per pet, zero slots): bookkeeping
    // duplicates rather than picks, and reading them binds three powers no fork carries. The
    // rule is the data's, not a list of the names it catches.
    if granted && !carries_something(entry) {
        return EntryOutcome::nothing();
    }

    // The set the entry itself names, which is not always one the build listed: a VEAT names its
    // base set for some picks and its branch set for others, and both are its own.
    //
    // **The granted group is not resolved as a path at all.** `Inherent.Inherent` and
    // `Inherent.Fitness` are one group in the file and three homes in the dataset — the
    // archetype inherents in the synthetic inherent set, Fitness in the pool partition, and a
    // Kheldian form's attacks in the powerset that holds the form. Resolving the path finds the
    // Fitness POOL and spends one of the build's pool picks on powers the game hands over. So
    // the question for a granted entry is which of the build's OWN sets carries the power: one
    // answer places it there, none or several leave it a granted inherent under the synthetic
    // set `hydrate` resolves those against.
    let set_id = match granted_group {
        true => Some(
            carried_by_selection(power_name, build, database)
                .unwrap_or_else(|| crate::INHERENT_SET.to_string()),
        ),
        false => resolve_set(&group_path, sets, names),
    };
    let key = powerset_key(&group_path, names);

    let ours = match names {
        Some(names) => match names.power_name(&key, power_name) {
            Some(ours) => ours.to_string(),
            // The powerset gave this name to a DIFFERENT power and offers no counterpart for
            // the one Mids means. Ranging on would bind the same-named power the set now
            // holds — Willpower's `Reconstruction` finding Regeneration's — and a wrong
            // answer that looks deliberate is worse than none.
            None if names.is_rotated_away(&key, power_name) => {
                return EntryOutcome::declined(
                    &full,
                    format!(
                        "{power_name:?} names a different power in this dataset, and the one \
                         Mids means has no counterpart here"
                    ),
                )
            }
            None => power_name.to_string(),
        },
        None => power_name.to_string(),
    };

    let ours = separator_drift(&ours, set_id.as_deref(), database).unwrap_or(ours);

    let Some(set_id) = set_id else {
        return EntryOutcome::declined(
            &full,
            format!(
                "no set in this dataset carries {ours:?}, so the pick has no home in this build"
            ),
        );
    };

    let (slots, notes, resolved, refused) = read_slots(entry, &full, database);
    EntryOutcome {
        placement: Placement::Power {
            set_id,
            power: SkifPower {
                internal_name: ours,
                powerset: None,
                // `0` is the format's own spelling for granted-not-picked. An entry past
                // `LastPower` is auto-granted rather than picked — the list is positional, and
                // a reader that honours the name without the position turns grants into picks.
                level: match granted {
                    true => 0,
                    false => u8::try_from(entry.level).unwrap_or(0),
                },
                slots,
                // Mids' `StatInclude` is "this power is contributing to my totals", which is
                // the same question this field answers. Mirroring it is what makes a fresh
                // import reproduce the totals the author was looking at.
                is_active: entry.stat_include,
                active_sub_power: None,
            },
        },
        notes,
        pieces_resolved: resolved,
        pieces_refused: refused,
        slots_read: entry.slot_entries.len(),
        declined: false,
    }
}

/// The group a `.mbd` writes for the powers the game grants rather than sells. It names no set
/// a build holds — `Inherent.Inherent` and `Inherent.Fitness` are two different homes in the
/// dataset — which is why it is the one group token read here rather than resolved.
const INHERENT_GROUP: &str = "Inherent";

/// The group temporary powers ride in under, accolades included.
const TEMPORARY_GROUP: &str = "Temporary_Powers";

/// Is there anything on this entry the dataset does not already re-derive?
///
/// A granted power's roster is the dataset's (rule 6), so a build states one only where the
/// author put something on it: an enhancement, a slot beyond the base one, or the toggle left
/// running — a state the author switched on is not a default.
///
/// **An entry with no slots at all carries nothing either way.** That is the discriminator
/// between a power the build holds and a Mids bookkeeping row: Sprint, Rest and Brawl each
/// state their base slot, while `Special_Set_Bonuses`, `Defiance`, `Fast_Snipe` and
/// `Double_Jump` state none. Reading `StatInclude` alone would carry those four into the build
/// as powers no dataset has a record of, and report each one — a fail-loud channel crying wolf
/// on every file is one nobody reads.
fn carries_something(entry: &MbdPowerEntry) -> bool {
    entry.enhancement_count() > 0
        || entry.slot_entries.len() > 1
        || (entry.stat_include && !entry.slot_entries.is_empty())
}

/// Does this dataset file `set_id` as accolades?
///
/// Read off the powerset's own category rather than the path the file spells, so a fork that
/// moves the set says so in its data rather than needing a rule here.
fn is_accolade_set(set_id: &str, database: &PowerDatabase) -> bool {
    database
        .find_powerset(set_id)
        .and_then(|set| set.category.as_deref())
        == Some(ACCOLADE_CATEGORY)
}

/// The set's own spelling of a name that differs from Mids' only in its separators.
///
/// **The name map leaves this case open on purpose, and the Rust reader is what makes that a
/// gap** (DATA-GAP MBDIMPORT-13). `convert-mids-name-map.cjs` joins on the display name with
/// case and separators folded to a space, and it deliberately stops short of deleting them —
/// a row is for a name meaning a DIFFERENT power, and folding `Quick Sand` onto `Quicksand`
/// would mint rows for spelling drift the beta's own matcher ladder already resolved. That
/// ladder is what this reader does not have and does not want: it ends in a fuzzy display
/// match across the whole dataset, which is the mis-bind rule 8 exists to prevent.
///
/// So the drift is answered here instead, and answered narrowly. The comparison is against
/// the powers of the ONE set the pick already resolved to, the fold is separators and case
/// and nothing else, and two survivors is a question rather than a tie to break. Rebirth's
/// Guardian Dark Assault is the corpus case: Mids spells the power `Moon_Beam` and the export
/// spells it `Moonbeam`, and without this the pick is retained under a name the dataset has no
/// record of.
fn separator_drift(ours: &str, set_id: Option<&str>, database: &PowerDatabase) -> Option<String> {
    let set_id = set_id?;
    let powers = set_powers(database, set_id);
    if powers
        .iter()
        .any(|power| power.ident().eq_ignore_ascii_case(ours))
    {
        return None;
    }
    let wanted = fold_separators(ours);
    let mut matching = powers
        .iter()
        .filter(|power| fold_separators(power.ident()) == wanted);
    let only = matching.next()?;
    matching.next().is_none().then(|| only.ident().to_string())
}

/// A name with its separators and case folded away — the whole fold, stated once so both
/// halves of the comparison are the same rule.
pub(crate) fn fold_separators(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The one set among the build's own that carries a power of this name, where exactly one
/// does.
///
/// This is how a Kheldian form's attacks find their home. Mids exports them under
/// `Inherent.Inherent.Dark_Nova_Blast` while the dataset holds them in the primary powerset
/// with the form that grants them, so the group says nothing and the build's own picks are the
/// only scope in which the name is unambiguous. Several claimants means the name is not an
/// answer, and none means it is an ordinary granted inherent.
fn carried_by_selection(
    power_name: &str,
    build: &SkifBuild,
    database: &PowerDatabase,
) -> Option<String> {
    let held = build
        .primary
        .iter()
        .chain(build.secondary.iter())
        .chain(build.pools.iter())
        .chain(build.epic_pool.iter())
        .map(|selection| selection.id.as_str());
    let mut claimants = held.filter(|set_id| {
        database.find_power(set_id, power_name).is_some()
            || database.find_partition_power(set_id, power_name).is_some()
    });
    let only = claimants.next()?;
    claimants.next().is_none().then(|| only.to_string())
}

/// The incarnate slot whose export key this path names, if any.
fn incarnate_slot(group_path: &str, database: &PowerDatabase) -> Option<String> {
    database
        .incarnate_catalog
        .slots
        .iter()
        .find(|slot| slot.key.eq_ignore_ascii_case(group_path))
        .map(|slot| slot.id.clone())
}

/// One `Incarnate.<slot>.<power>` entry.
///
/// A name the slot does not carry is REPORTED rather than dropped. Mids enumerates some of its
/// own tree tiers under names no fork ships, and the beta's reader recognised those by pattern
/// and skipped them silently — which also silences the case that matters, a build carrying an
/// incarnate this fork has never heard of (Homecoming ships 54 Judgement to the forks' 45).
fn incarnate_pick(
    entry: &MbdPowerEntry,
    slot: String,
    power_name: &str,
    full: &str,
    database: &PowerDatabase,
) -> EntryOutcome {
    let found = database
        .incarnate_catalog
        .slot(&slot)
        .and_then(|catalog| catalog.find_power(power_name));
    let Some(power) = found else {
        return EntryOutcome::declined(
            full,
            format!("this dataset's {slot} slot carries no power called {power_name:?}"),
        );
    };
    EntryOutcome {
        placement: Placement::Incarnate {
            slot,
            pick: SkifIncarnate {
                power: power.internal_name.clone(),
                active: entry.stat_include,
            },
        },
        ..EntryOutcome::nothing()
    }
}

/// One `Temporary_Powers.Accolades.<name>` entry.
///
/// **`StatInclude` is the mapping, not mere presence.** Mids keeps "owned" and "counted"
/// apart; the planner has one state and it is the counted one, so an accolade the author
/// excluded from their Mids totals must not arrive switched on.
///
/// An accolade this dataset carries but does not OFFER as a toggle is silent by design: the
/// click and travel members grant a timed buff on use, and folding one into a build
/// permanently because it was earned would be wrong. A name in neither roster is a roster
/// divergence, and the note is the only thing that would surface it.
fn accolade(
    entry: &MbdPowerEntry,
    power_name: &str,
    full: &str,
    database: &PowerDatabase,
) -> EntryOutcome {
    let toggle = database
        .accolade_toggles()
        .into_iter()
        .find(|toggle| toggle.power.ident().eq_ignore_ascii_case(power_name))
        .map(|toggle| toggle.id);
    match toggle {
        Some(id) if entry.stat_include => EntryOutcome {
            placement: Placement::Accolade(id),
            ..EntryOutcome::nothing()
        },
        Some(_) => EntryOutcome::nothing(),
        None if database
            .accolade_powers()
            .any(|power| power.ident().eq_ignore_ascii_case(power_name)) =>
        {
            EntryOutcome::nothing()
        }
        None => EntryOutcome::declined(
            full,
            "this dataset's accolade roster carries no power of that name".to_string(),
        ),
    }
}

/// An entry's slots, positionally, with the pieces that refused named.
///
/// The slot list is kept at the length the file states, holes included: a slot's position is
/// its identity in this format, and closing a gap moves every piece after it.
fn read_slots(
    entry: &MbdPowerEntry,
    context: &str,
    database: &PowerDatabase,
) -> (Vec<Option<SkifEnhancement>>, Vec<ImportNote>, usize, usize) {
    let mut notes = Vec::new();
    let mut slots = Vec::with_capacity(entry.slot_entries.len());
    let mut resolved = 0;
    let mut refused = 0;

    let (Some(index), Some(catalog), Some(mids)) = (
        database.boost_index.as_ref(),
        database.enhancements.as_ref(),
        database.mids_uids.as_ref(),
    ) else {
        if entry.enhancement_count() > 0 {
            notes.push(ImportNote {
                context: context.to_string(),
                detail: "this dataset ships no boost index or Mids UID table, so no \
                         enhancement can be resolved"
                    .to_string(),
            });
        }
        return (
            entry.slot_entries.iter().map(|_| None).collect(),
            notes,
            0,
            0,
        );
    };

    for slot in &entry.slot_entries {
        let Some(enhancement) = slot.enhancement.as_ref().filter(|e| !e.uid.is_empty()) else {
            slots.push(None);
            continue;
        };
        match resolve_enhancement(enhancement, index, mids, catalog) {
            Ok(piece) => {
                resolved += 1;
                slots.push(Some(piece));
            }
            Err(unresolved) => {
                refused += 1;
                notes.push(ImportNote {
                    context: format!("{} in {context}", unresolved.uid),
                    detail: unresolved.refusal.to_string(),
                });
                slots.push(None);
            }
        }
    }
    (slots, notes, resolved, refused)
}

/// Put one read entry into the build, and say whether it landed.
///
/// The claim guard lives here rather than in the read, because only the build knows what is
/// already in it. A power the build already holds under the SAME Mids name is an ordinary
/// duplicate; a second Mids name resolving onto one power is a collision, and reporting it is
/// the difference between a bug report and a power missing from a build that imported cleanly.
#[allow(clippy::too_many_arguments)]
fn place(
    placement: Placement,
    build: &mut SkifBuild,
    claimed: &mut HashMap<(String, String), String>,
    entry: &MbdPowerEntry,
    roles: Option<&Roles>,
    database: &PowerDatabase,
    summary: &mut MbdSummary,
    notes: &mut Vec<ImportNote>,
) -> bool {
    match placement {
        Placement::Nothing => false,
        Placement::Accolade(id) => {
            if !build.accolades.contains(&id) {
                build.accolades.push(id);
                summary.accolades_imported += 1;
            }
            true
        }
        Placement::Incarnate { slot, pick } => {
            build.incarnates.insert(slot, pick);
            summary.incarnates_imported += 1;
            true
        }
        Placement::Power { set_id, power } => {
            let key = (set_id.clone(), power.internal_name.to_lowercase());
            if let Some(by) = claimed.get(&key) {
                if by != &entry.power_name {
                    notes.push(ImportNote {
                        context: entry.power_name.clone(),
                        detail: format!(
                            "resolves to {:?} in {set_id:?}, which {by:?} already claimed — \
                             this entry and its slots were dropped",
                            power.internal_name
                        ),
                    });
                }
                return false;
            }

            let Some(bucket) = bucket_of(&set_id, roles, database) else {
                notes.push(ImportNote {
                    context: entry.power_name.clone(),
                    detail: format!(
                        "{set_id:?} is not one of this archetype's sets, a pool or an epic \
                         pool, so {:?} has no bucket in this build",
                        power.internal_name
                    ),
                });
                summary.powers_declined += 1;
                return false;
            };

            claimed.insert(key, entry.power_name.clone());
            if power.level > 0 {
                summary.picks_imported += 1;
            }
            match bucket {
                Bucket::Inherent => build.inherents.push(power),
                Bucket::Primary => push(&mut build.primary, set_id, power),
                Bucket::Secondary => push(&mut build.secondary, set_id, power),
                Bucket::Epic(id) => push(&mut build.epic_pool, id, power),
                // A pool selection always carries the pool the pick resolved to — the bucket IS
                // that set — so there is never a second set for the pick to state.
                Bucket::Pool(id) => match build.pools.iter_mut().find(|pool| pool.id == id) {
                    Some(pool) => pool.powers.push(power),
                    None => build.pools.push(SkifSelection {
                        id,
                        powers: vec![power],
                    }),
                },
            }
            true
        }
    }
}

/// Add a pick to a selection, opening it at the pick's own set where the file listed none.
///
/// **A pick whose set is not the one its list names keeps its own**, which is the one thing
/// v4 could not say: a VEAT records some picks under its base set and some under the branch
/// set it specialised into, and the branch id is the only record that the branch was taken.
fn push(slot: &mut Option<SkifSelection>, id: String, mut pick: SkifPower) {
    match slot {
        Some(selection) => {
            if selection.id != id {
                pick.powerset = Some(id);
            }
            selection.powers.push(pick);
        }
        None => {
            *slot = Some(SkifSelection {
                id,
                powers: vec![pick],
            })
        }
    }
}

/// Settle the character level (MBDIMPORT-8).
///
/// Mids' own number is the floor: a character is at least as high as the highest level it has
/// spent a pick or a slot at. It understates by construction — the corpus Warshade is an
/// author-confirmed 50 whose last placement falls at 49, and Mids displays it as 49 — so the
/// export's own leveling schedule supplies the raise: a build cannot have placed more budget
/// slots than its level has been granted, and the lowest level whose budget covers the
/// placements is the answer. No constant anywhere, and a genuinely low-level build still
/// imports at its own level rather than being promoted to the cap.
///
/// **The export's schedule is the right table even though Mids planned on a different one**
/// (MBDIMPORT-11). `schedules.bin` holds one schedule and `NLevels.mhd` is byte-identical to
/// it on Homecoming and Rebirth, the two forks that was measured on (both 67); `RLevels.mhd` is
/// Mids' respec table and has no counterpart in the binary — nor in the game. Observed on a live
/// Homecoming server 2026-09-13 (MBDIMPORT-14): a respec hands out one flat pool of 67 with no
/// levels in it, and levelling grants a POWER at 47 and 49 and slots at 48 and 50, which is what
/// this table already said. `RLevels` is Mids' own `NLevels` with 3 slots added at each of those
/// two power levels, and 67 + 6 = 73 is the whole of the gap. Thunderspy is not covered — the
/// binary grants 71 there and Mids' table for it is its pre-fork `Generic` one (TSPY-12), so on
/// that fork the agreement is absent rather than confirmed. The question
/// here is what level the CHARACTER must be, and a character is granted slots by the game — so
/// grading against the looser table could only under-raise, letting a build that overspends
/// the server's budget import at a level that cannot hold it.
fn settle_level(file: &MbdFile, build: &SkifBuild, database: &PowerDatabase) -> DerivedLevel {
    let floor = mids_max_used_level(file).max(1);
    let Some(schedule) = database.leveling_schedule.as_ref() else {
        return DerivedLevel {
            level: floor,
            floor,
            ..DerivedLevel::default()
        };
    };
    let cap = schedule.max_level().unwrap_or(floor).max(floor);

    let mut placed = 0;
    let mut budget = 0;
    for candidate in floor..=cap {
        placed = placed_budget_slots(build, candidate, schedule);
        budget = schedule.total_slots_at_level(candidate);
        if placed <= budget {
            return DerivedLevel {
                level: candidate,
                floor,
                placed_slots: placed,
                slot_budget: budget,
                raised_from_floor: candidate > floor,
                over_budget_at_cap: false,
                excess_over_server_budget: 0,
            };
        }
    }
    DerivedLevel {
        level: cap,
        floor,
        placed_slots: placed,
        slot_budget: budget,
        raised_from_floor: cap > floor,
        over_budget_at_cap: true,
        excess_over_server_budget: placed.saturating_sub(budget),
    }
}

/// What the receipt has to say about a level the file never stated, or nothing where Mids' own
/// floor stood unmoved and the placements fit.
///
/// **One note, not two.** Both facts are about the same number, and a receipt that lists them
/// separately reads as two problems — the second note `decode_mbd` already refuses to add
/// beside this one.
///
/// **The raise is reported even where it lands inside budget** (MBDIMPORT-15). Only the
/// over-budget arm used to write anything, so a build re-levelled into a schedule it fits
/// arrived under a receipt reading "everything resolved": 3 of the 8 corpus files, each moved
/// from Mids' 49 to 50. Level is not cosmetic — it chooses a row of the archetype's hit-point
/// table, and the corpus Blaster reads 1621 Max HP at 50 against 1617 at 49 — so a silent move
/// leaves the user comparing this planner against Mids on two different characters.
fn level_note(level: DerivedLevel, dataset: DatasetId) -> Option<ImportNote> {
    let detail = match (level.raised_from_floor, level.over_budget_at_cap) {
        (false, false) => return None,
        (true, false) => format!(
            "Mids shows this build at level {} — the level of its last power or slot — and it \
             imports at {}, because no earlier level on {} grants the {} budget slots the \
             build places. Level chooses a row of the archetype's hit-point table, so the two \
             are not the same character.",
            level.floor,
            level.level,
            dataset.display_name(),
            level.placed_slots,
        ),
        (true, true) => format!(
            "Mids shows this build at level {} — the level of its last power or slot — and it \
             imports at {}, the highest level {}'s schedule reaches. Even there the schedule \
             grants {} of the {} budget slots the build places, {} short. Mids' own slot table \
             permits placements this server's schedule never grants, so the build is imported \
             whole and the extra slots are over budget.",
            level.floor,
            level.level,
            dataset.display_name(),
            level.slot_budget,
            level.placed_slots,
            level.excess_over_server_budget,
        ),
        (false, true) => format!(
            "this build places {} enhancement slots where {} grants {} by level {} — {} \
             more than the game allows. Mids' own slot table permits placements this \
             server's schedule never grants, so the build is imported whole and the extra \
             slots are over budget.",
            level.placed_slots,
            dataset.display_name(),
            level.slot_budget,
            level.level,
            level.excess_over_server_budget,
        ),
    };
    Some(ImportNote {
        context: format!("level {}", level.level),
        detail,
    })
}

/// Placeable slots the build has spent, at a level.
///
/// The twin of [`crate::leveling_schedule::placed_budget_slots`], which reads a hydrated
/// build; this reads the file's shape, before hydration, because the level it helps settle is
/// an input to hydration. A power's free base slot and its auto-granted inherent slots don't
/// count against the budget — and the auto-granted count is the DATASET's answer at this
/// level, not the file's `IsInherent` flags, because Mids' own tables disagree with the
/// server's about which slots those are (MBDIMPORT-11).
///
/// It is recomputed per candidate level rather than once because that dependency is real:
/// Rebirth grants Health and Stamina their extra slots at levels of their own, so how many of
/// a build's slots are free is a function of the level being tested.
fn placed_budget_slots(build: &SkifBuild, level: u8, schedule: &LevelingSchedule) -> usize {
    build
        .selections()
        .map(|power| {
            let free = 1 + usize::from(auto_granted_slot_count(
                &schedule.auto_granted_slot_levels,
                &power.internal_name,
                level,
            ));
            power.slots.len().saturating_sub(free)
        })
        .sum()
}
