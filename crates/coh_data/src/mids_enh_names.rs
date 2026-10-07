//! Mids Reborn's enhancement SHORT-NAME namespace, and its array order.
//!
//! The legacy `.mxd` names a slotted enhancement twice, and this is the table that answers both
//! spellings. Neither is a name our data owns, which is why this is a Mids table and not a
//! Rule 0 breach: it is Mids' answer about Mids, the one question the export cannot be asked.
//!
//! - **the post half writes a short code**, `Ags-ResDam/EndRdx` — the set's `ShortName` and the
//!   piece's, joined by a hyphen. It is a NAME, so it survives Mids reordering its database, and
//!   it is not derivable: Gaussian's set short name ends in a hyphen of its own, so
//!   `GssSynFr--Build%` is one code with three plausible splits and only the database can say
//!   which is real.
//! - **the compressed half writes an array index.** Mids reorders that array between releases,
//!   so an index names a piece only inside its own file. What survives the reordering is set
//!   MEMBERSHIP, and that is the whole reason the index is carried: where a file's code names a
//!   piece this database has since renamed, the index still says which set it is in.
//!
//! **Together they resolve what neither can.** [`MidsEnhNames::resolve`] takes the code first,
//! because a name beats a position; where the code names nothing, it falls back to the index and
//! accepts it ONLY where the index's set is the set the code named. Across the 1,929-file corpus
//! that residual pass answers 358 slots, all of them two Mids piece renames
//! (`LucoftheG-Rchg+` → `Def/Rchg+`, `SprBrtFur-Rech/Fury` → `Rech/Fury%`), and the set agrees
//! every single time. A residual that fired without the set agreeing would be the coin flip
//! MBDIMPORT-18 closed at the other door.

use crate::mids_enh_names::MidsEnhKind::{Generic, Origin, Set, Special};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// What Mids files a record as — its own `eType`, carried as data.
///
/// The kind decides how the post half SPELLS the record, and that spelling has changed: a D-Sync
/// is `DSyncO:Nucle` in the 1.01 post and `DS:…` in the 3.x one for the same records. A reader
/// keyed on the decoration would lose whichever family the other Mids wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MidsEnhKind {
    /// A piece of an enhancement set. The post writes `<setShort>-<short>`.
    Set,
    /// A crafted generic IO. The post writes `<short>-I`.
    Generic,
    /// A training / dual / single origin enhancement. The post writes `<short>`.
    Origin,
    /// Hamidon, Hydra, Titan or D-Sync. The post writes `<family>:<short>`.
    Special,
    /// A Mids type the generator had no kind for. Held rather than dropped so the reader can
    /// decline it by name instead of reporting an index nothing explains.
    #[serde(other)]
    Unclassified,
}

/// One record, as this table holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidsEnhRecord {
    /// The short name of the set this record belongs to, empty outside one.
    pub set_short: String,
    /// The record's own short name.
    pub short: String,
    /// Mids' UID — the spelling the `.mbd` reader already resolves.
    pub uid: String,
    pub kind: MidsEnhKind,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MidsEnhNameSection {
    #[serde(default)]
    enhancements: Vec<(String, String, String, MidsEnhKind)>,
    #[serde(default)]
    source_sha256: String,
}

/// The dataset's Mids short-name table, indexed both ways a `.mxd` needs it.
#[derive(Debug, Clone, PartialEq)]
pub struct MidsEnhNames {
    /// Mids' array, in Mids' order. The POSITION is the index a `.mxd` writes, so nothing here
    /// may sort it.
    by_index: Vec<MidsEnhRecord>,
    /// (kind, stem) → every index answering to it, the stem lower-cased.
    ///
    /// **Keyed on the kind as well as the name**, because the post's DECORATION is what
    /// separates two records Mids short-names identically: `Acc-I` is the crafted generic IO and
    /// `Acc` the origin enhancement, and both are short-named `Acc`. Folding the decoration away
    /// and looking up the stem alone made every generic IO in the corpus — 11,964 slots — a
    /// two-way tie that only the file's own index resolved, which is exactly the wrong thing to
    /// spend the index on.
    ///
    /// A `Vec` because Mids' database is still not unique inside one kind: Siren's Song has two
    /// pieces short-named `EndRdx` on every fork, and Rebirth's Return From the Grave has a
    /// duplicated sixth record. Collapsing those to one arm would be a coin flip dressed as a
    /// decode; carrying both lets the file's own index break the tie, and lets
    /// [`MidsEnhNames::resolve`] decline where it cannot.
    by_code: HashMap<(MidsEnhKind, String), Vec<usize>>,
    source_sha256: String,
}

/// Why a code and an index did not reach one record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidsEnhRefusal {
    /// Neither the code nor the index named anything this database carries.
    Unknown,
    /// The code names several records and nothing said which. Carries how many.
    Ambiguous(usize),
    /// The code names nothing here and the index names a record in a DIFFERENT set, so the two
    /// halves of the file point at different things. Carries the set the index landed in.
    SetDisagrees(String),
}

/// How a slot's enhancement was reached, so a caller can report the residual population rather
/// than discovering it in the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidsEnhRoute {
    /// The post's code named exactly one record.
    Code,
    /// The post's code named several and the file's own index picked one of them.
    CodeDisambiguatedByIndex,
    /// The post's code named nothing this database carries, and the index named a record in the
    /// set the code did name. A Mids piece rename, in other words.
    IndexWithinNamedSet,
}

impl MidsEnhNames {
    /// Parse the contract's `mids-enh-names` section.
    ///
    /// Absent is not malformed, on the sibling readers' terms: a hand-constructed
    /// `PowerDatabase` carries no table and consumers surface that as an error rather than
    /// treating it as an empty one. A PRESENT section that will not parse IS an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let raw: MidsEnhNameSection = serde_json::from_value(section.clone())
            .map_err(|error| format!("mids-enh-names section: {error}"))?;

        let by_index: Vec<MidsEnhRecord> = raw
            .enhancements
            .into_iter()
            .map(|(set_short, short, uid, kind)| MidsEnhRecord {
                set_short,
                short,
                uid,
                kind,
            })
            .collect();

        let mut by_code: HashMap<(MidsEnhKind, String), Vec<usize>> = HashMap::new();
        for (index, record) in by_index.iter().enumerate() {
            if let Some(code) = record.code() {
                by_code
                    .entry((record.kind, code.to_lowercase()))
                    .or_default()
                    .push(index);
            }
        }
        Ok(Some(Self {
            by_index,
            by_code,
            source_sha256: raw.source_sha256,
        }))
    }

    /// The record at one of Mids' array indices, where this database is long enough to have one.
    pub fn at_index(&self, index: i32) -> Option<&MidsEnhRecord> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.by_index.get(index))
    }

    /// Resolve a slot from both halves of the file: the post's code, and the compressed half's
    /// index where the file states one.
    ///
    /// The code is tried first and the index only rescues it, never overrides it — an index is a
    /// position in a database this reader does not have, and a position that happens to be in
    /// range is not evidence. See the module note for the measured populations.
    pub fn resolve(
        &self,
        code: &str,
        index: Option<i32>,
    ) -> Result<(&MidsEnhRecord, MidsEnhRoute), MidsEnhRefusal> {
        let (kind, stem) = Self::lookup_key(code);
        let named = self.by_code.get(&(kind, stem));
        let at_index = index.and_then(|index| self.at_index(index));

        match named.map(Vec::as_slice) {
            Some([only]) => Ok((&self.by_index[*only], MidsEnhRoute::Code)),
            Some(several) => {
                // Mids' own database names two records the same thing. The file's index is the
                // only thing that can say which, and it has to be one of them — an index
                // pointing somewhere else entirely is not a tie-break, it is a third answer.
                let index = index.and_then(|index| usize::try_from(index).ok());
                match index.filter(|index| several.contains(index)) {
                    Some(index) => Ok((
                        &self.by_index[index],
                        MidsEnhRoute::CodeDisambiguatedByIndex,
                    )),
                    None => Err(MidsEnhRefusal::Ambiguous(several.len())),
                }
            }
            None => {
                // The residual pass. The code named nothing, so the only thing left is the
                // index — admitted only where it lands in the SET the code named, because set
                // membership is what survives Mids reordering its array and a piece's position
                // in it is not.
                let Some(record) = at_index else {
                    return Err(MidsEnhRefusal::Unknown);
                };
                if kind != Set || record.kind != Set || record.set_short.is_empty() {
                    return Err(MidsEnhRefusal::Unknown);
                }
                if !record.names_the_set_in(code) {
                    return Err(MidsEnhRefusal::SetDisagrees(record.set_short.clone()));
                }
                Ok((record, MidsEnhRoute::IndexWithinNamedSet))
            }
        }
    }

    /// What kind of record the post's own spelling says this is, and the name under it.
    ///
    /// The decoration is Mids' and its spelling has changed between versions — a D-Sync is
    /// `DSyncO:` in the 1.01 post and `DS:` in the 3.x one — so what is read off it is the KIND
    /// and never the family. The kind is then half the lookup key, which is what keeps `Acc-I`
    /// and `Acc` apart where Mids short-names both records `Acc`.
    fn lookup_key(code: &str) -> (MidsEnhKind, String) {
        if let Some((_, stem)) = code.rsplit_once(':') {
            return (Special, stem.to_lowercase());
        }
        if let Some(stem) = code.strip_suffix("-I") {
            return (Generic, stem.to_lowercase());
        }
        match code.contains('-') {
            true => (Set, code.to_lowercase()),
            false => (Origin, code.to_lowercase()),
        }
    }

    /// SHA-256 of the EnhDB.mhd this table was read from, for the freshness gate.
    pub fn source_sha256(&self) -> &str {
        &self.source_sha256
    }

    /// Every record, in Mids' own order — for a census that wants the table's whole population.
    pub fn records(&self) -> &[MidsEnhRecord] {
        &self.by_index
    }

    /// How many distinct codes the table answers to, and how many of those name more than one
    /// record. A census wants both, because the second is the population the tie-break exists
    /// for and a table where it grows is a table getting less decodable.
    pub fn code_counts(&self) -> (usize, usize) {
        (
            self.by_code.len(),
            self.by_code
                .values()
                .filter(|indices| indices.len() > 1)
                .count(),
        )
    }
}

impl MidsEnhRecord {
    /// The code this record answers to, stripped of the decoration the post would add.
    ///
    /// `None` for a record this table has no kind for: a record nothing can spell is a record no
    /// lookup should be able to reach by accident.
    /// Whether a post code names THIS record's set — asked as a prefix rather than by splitting
    /// the code, because the split point is not decidable: Gaussian's set short name is
    /// `GssSynFr-`, so `GssSynFr--Build%` divides three ways and only a set already in hand can
    /// say which. Comparing forwards asks the question the residual pass actually has.
    fn names_the_set_in(&self, code: &str) -> bool {
        !self.set_short.is_empty()
            && code.len() > self.set_short.len()
            && code[..self.set_short.len()].eq_ignore_ascii_case(&self.set_short)
            && code.as_bytes()[self.set_short.len()] == b'-'
    }

    fn code(&self) -> Option<String> {
        match self.kind {
            Set => Some(format!("{}-{}", self.set_short, self.short)),
            Generic | Origin | Special => Some(self.short.clone()),
            MidsEnhKind::Unclassified => None,
        }
    }
}
