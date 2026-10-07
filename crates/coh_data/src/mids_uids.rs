//! Mids Reborn's enhancement-UID namespace, read out of Mids' own EnhDB.
//!
//! A `.mbd` names a slotted enhancement by a UID and nothing else, and that namespace is
//! Mids', not the game's. Mids resolves one by substring match and, when nothing matches,
//! sets the slot to -1 and moves on — no error, no log. So a UID either side gets wrong
//! does not look like a bug on either side; it looks like the user never slotted anything.
//!
//! **This table is the second door, not the first.** Most of what a `.mbd` names is a boost
//! record the game itself carries under the same spelling, and [`crate::boost_index`] is the
//! authority on those — the export owns the record, so Rule 0 sends the question there.
//! What this table adds is the set of spellings where MIDS has drifted from the game and only
//! Mids can say so: `Crafted_Shrapnel_*` is Mids' Artillery after a rename, and Rebirth's
//! `Liberty's_Belt` and `Superior_Witchcraft` are Mids' names for sets the boost index files
//! elsewhere. Asking Mids what Mids calls a thing is not a Rule 0 breach; deriving it from the
//! UID's own text is, and that is what the beta's `parseIOSetUid` does.
//!
//! **A set and a piece do not name a record.** The game holds the crafted and the attuned forms
//! of one piece under two names at the same set and piece number, so this table ships the
//! attunement prefix Mids spells each set with ([`MidsUidPrefix`]) alongside the UIDs. Without
//! it the second door reaches Artillery piece 1 and finds two records it cannot choose between,
//! which is where Homecoming's six `Crafted_Shrapnel_*` UIDs used to stop.
//!
//! Measured across the seven-file corpus on 2026-09-09: 195 of 201 distinct Homecoming UIDs
//! and 97 of 151 Rebirth ones resolve through the boost index, the other 6 and 54 through this
//! table, and **nothing through neither**. Where both answer — 4,723 overlapping records across
//! all four forks — they disagree zero times, which is why the order between them is a
//! preference rather than a risk. [`the_two_doors_never_disagree`] is that claim's tripwire.
//!
//! The table is generated from EnhDB.mhd by `tools/mids-oracle/emit_mids_uids.py`, per fork,
//! because two of the four forks read a database that is not their own (MBDEXPORT-2):
//! Brainstorm reads Homecoming's, and Thunderspy reads Mids' GENERIC database, since Mids has
//! never shipped a Thunderspy one. `source_sha256` gauges the vendored file's freshness and
//! cannot see which database the bytes are.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// One IO-set piece, as this table addresses it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MidsSetPiece<'a> {
    /// The [`crate::io_sets`] set id.
    pub set: &'a str,
    /// 1-based piece number, matching [`crate::boost_index::BoostEntry::IoSet`].
    pub piece: u8,
    /// The attunement prefix Mids spells this UID with.
    ///
    /// A set and a piece do not name a record: the game holds the crafted and the attuned
    /// forms of one piece at the same set and piece number, and this is what tells them apart.
    pub prefix: MidsUidPrefix,
}

/// The attunement prefix a Mids UID carries.
///
/// **`Bare` is an observation, not an absence.** Mids spells 23 of Rebirth's sets with no
/// prefix and 117 of those pieces are the game's ATTUNED records, so reading `Bare` as
/// "not attuned" binds the wrong one of two. It means Mids states nothing here, which is
/// why [`Self::attuned`] answers `None` rather than `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MidsUidPrefix {
    SuperiorAttuned,
    Attuned,
    Crafted,
    Bare,
}

impl MidsUidPrefix {
    /// What the prefix says about attunement, or `None` where it says nothing.
    ///
    /// `SuperiorAttuned` and `Attuned` both mean attuned; the two are different SETS, not
    /// different attunements, and the set id has already separated them by the time anyone
    /// asks this.
    pub fn attuned(self) -> Option<bool> {
        match self {
            MidsUidPrefix::SuperiorAttuned | MidsUidPrefix::Attuned => Some(true),
            MidsUidPrefix::Crafted => Some(false),
            MidsUidPrefix::Bare => None,
        }
    }
}

/// The raw section as the contract carries it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MidsUidSection {
    /// set id -> piece UIDs, indexed by `piece - 1`. An empty string is a piece whose
    /// EnhDB record is missing, and is unresolvable rather than a name.
    #[serde(default)]
    io_set_pieces: HashMap<String, Vec<String>>,
    /// set id -> the attunement prefix that set's piece UIDs carry.
    #[serde(default)]
    io_set_prefix: HashMap<String, MidsUidPrefix>,
    /// Spelled `genericIO` in the emitted table, which `rename_all = "camelCase"` renders
    /// `genericIo` — so this field silently deserialized EMPTY on all four forks until
    /// MBDIMPORT-10, and [`MidsUids::family`] could never answer `GenericIo`. `#[serde(default)]`
    /// is what made it silent, and it stays because an absent section is not an error here;
    /// the guard is `mids_tables_corpus`, which asserts each roster is populated.
    #[serde(default, rename = "genericIO")]
    generic_io: Vec<String>,
    #[serde(default)]
    special: Vec<String>,
    #[serde(default)]
    origin: Vec<String>,
    #[serde(default)]
    source_sha256: String,
}

/// The dataset's Mids UID table, indexed for the direction a reader needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct MidsUids {
    /// UID -> the set and piece it names, lower-cased on the key.
    ///
    /// The forward table is set -> UIDs and the reader needs the inverse. Inverting here
    /// rather than in the generator is safe only because a UID names exactly one piece;
    /// [`MidsUids::from_section`] refuses a table where that stops being true instead of
    /// letting one set silently win. Measured 0 collisions on all four forks, 2026-09-09.
    by_uid: HashMap<String, (String, u8, MidsUidPrefix)>,
    /// The families this table can only classify, not resolve: it says a UID IS a generic IO,
    /// a special or an origin piece, but not which one in the planner's own vocabulary. That
    /// second half is the boost index's, and these lists exist so a UID the boost index does
    /// not carry can still be REFUSED with the right reason rather than an anonymous one.
    generic_io: HashMap<String, String>,
    special: HashMap<String, String>,
    origin: HashMap<String, String>,
    source_sha256: String,
}

/// Which family this table places a UID in, when the boost index could not resolve it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidsFamily {
    GenericIo,
    Special,
    Origin,
}

impl MidsUids {
    /// Parse the contract's `mids-uids` section.
    ///
    /// Absent is not malformed, on the sibling readers' terms: a hand-constructed
    /// `PowerDatabase` carries no table and consumers surface that as an error rather than
    /// treating it as an empty one. A PRESENT section that will not parse IS an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let raw: MidsUidSection = serde_json::from_value(section.clone())
            .map_err(|e| format!("mids-uids section: {e}"))?;

        let mut by_uid: HashMap<String, (String, u8, MidsUidPrefix)> = HashMap::new();
        for (set, pieces) in &raw.io_set_pieces {
            if pieces.iter().all(String::is_empty) {
                continue;
            }
            // Required, not defaulted. A set whose pieces this table names but whose prefix it
            // does not would answer every piece of it with "Mids states nothing", and that
            // resolves to a REFUSAL indistinguishable from a set Mids has never heard of. An
            // older table missing the whole map has to fail here, loudly, rather than degrade.
            let prefix = *raw.io_set_prefix.get(set).ok_or_else(|| {
                format!("mids-uids section: {set} names pieces but no attunement prefix")
            })?;
            for (index, uid) in pieces.iter().enumerate() {
                if uid.is_empty() {
                    continue;
                }
                let piece = u8::try_from(index + 1).map_err(|_| {
                    format!("mids-uids section: {set} has more pieces than a set can hold")
                })?;
                let key = uid.to_lowercase();
                if let Some((prior_set, prior_piece, _)) = by_uid.get(&key) {
                    return Err(format!(
                        "mids-uids section: {uid} names both {prior_set} piece {prior_piece} and \
                         {set} piece {piece}; the reader inverts this table and cannot choose"
                    ));
                }
                by_uid.insert(key, (set.clone(), piece, prefix));
            }
        }

        let index = |uids: &[String]| -> HashMap<String, String> {
            uids.iter()
                .map(|uid| (uid.to_lowercase(), uid.clone()))
                .collect()
        };

        Ok(Some(Self {
            by_uid,
            generic_io: index(&raw.generic_io),
            special: index(&raw.special),
            origin: index(&raw.origin),
            source_sha256: raw.source_sha256,
        }))
    }

    /// The set and piece a Mids IO-set UID names, or `None` where this fork's Mids database
    /// carries no such UID.
    ///
    /// Matched case-insensitively because Mids is not consistent with itself — its D-Sync
    /// roster is half `DSync_` and half `Dsync_`, and it spells one debuff `Debuff` where the
    /// crafted IOs say `DeBuff`. Its own lookup is a substring match that never sees the
    /// difference; ours has to be told.
    pub fn set_piece(&self, uid: &str) -> Option<MidsSetPiece<'_>> {
        self.by_uid
            .get(&uid.to_lowercase())
            .map(|(set, piece, prefix)| MidsSetPiece {
                set: set.as_str(),
                piece: *piece,
                prefix: *prefix,
            })
    }

    /// Which non-set family Mids files this UID under, if any.
    ///
    /// Used only to make a refusal specific. A UID that lands here is one the boost index
    /// could not resolve, so the answer is still "this build loses a piece" — but it is the
    /// difference between naming the family and reporting an unrecognized string.
    pub fn family(&self, uid: &str) -> Option<MidsFamily> {
        let key = uid.to_lowercase();
        if self.generic_io.contains_key(&key) {
            Some(MidsFamily::GenericIo)
        } else if self.special.contains_key(&key) {
            Some(MidsFamily::Special)
        } else if self.origin.contains_key(&key) {
            Some(MidsFamily::Origin)
        } else {
            None
        }
    }

    /// SHA-256 of the EnhDB.mhd this table was read from, for the freshness gate.
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// How many IO-set UIDs the table carries, for a census that wants to state its population.
    pub fn set_piece_count(&self) -> usize {
        self.by_uid.len()
    }

    /// Every IO-set UID the table carries, lower-cased as the inversion stored them.
    ///
    /// For a census that has to run the table's OWN population through a reader, rather than a
    /// population sized from whatever a fixture happens to hold.
    pub fn set_piece_uids(&self) -> impl Iterator<Item = &str> {
        self.by_uid.keys().map(String::as_str)
    }

    /// The three non-set rosters' sizes: generic IOs, specials, origins.
    ///
    /// Exists so a census can see one of them go EMPTY. Each is `#[serde(default)]`, so a key
    /// this reader spells differently from the emitted table costs nothing at parse time and
    /// costs [`Self::family`] an answer at run time — which is how `genericIO` was read as
    /// `genericIo` and came up empty on all four forks (MBDIMPORT-10).
    pub fn family_counts(&self) -> (usize, usize, usize) {
        (self.generic_io.len(), self.special.len(), self.origin.len())
    }
}
