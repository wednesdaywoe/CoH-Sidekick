//! Boost index — every enhancement the game can name, keyed by the spelling the
//! game client prints for it.
//!
//! A `/buildsave` writes a slotted enhancement as its binary boost name
//! (`Crafted_Bonesnap_A`, `Magic_Accuracy`, `Synthetic_Hamidon_Damage_Accuracy`).
//! No other section carries that spelling: [`crate::io_sets`] names sets and
//! pieces by display slug, [`crate::enhancements`] names common IOs and origins
//! by planner stat. This section is the join between the two — the typed view of
//! the contract's `boost-index` section (`scripts/convert-boost-index.cjs`).
//!
//! An entry carries IDENTITY, never values: it says which section describes the
//! enhancement and how to find it there, and the reader follows the pointer. So
//! there is no second copy of an aspect or a percentage to drift.

use crate::Level;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// The tier of an origin enhancement, as the export's own boost namespace states
/// it: `Generic_*` is Training (all five origins), `<Origin>_*` is Single, and
/// `<OriginA>_<OriginB>_*` is Dual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum OriginTier {
    #[serde(rename = "TO")]
    Training,
    #[serde(rename = "DO")]
    Dual,
    #[serde(rename = "SO")]
    Single,
}

/// What one boost record is, and where the section describing it can be found.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BoostEntry {
    /// A piece of an IO set: the [`crate::io_sets`] set id and 1-based piece
    /// number. `attuned` distinguishes the two variants of one piece, which are
    /// identical in the binary apart from their names.
    IoSet {
        set: String,
        piece: u8,
        attuned: bool,
    },
    /// A common ("invention") IO. `level` is the level the piece is crafted at,
    /// or `None` for the level-scaling template. `stats` is what it enhances, in
    /// the [`crate::EnhancementCatalog::common_io_types`] vocabulary.
    CommonIo {
        #[serde(default)]
        level: Option<i64>,
        #[serde(default)]
        stats: Vec<String>,
    },
    /// A Training/Dual/Single origin enhancement. `origins` is the set of
    /// character origins allowed to slot it — five for Training, two for Dual,
    /// one for Single.
    Origin {
        tier: OriginTier,
        #[serde(default)]
        origins: Vec<String>,
        #[serde(default)]
        stats: Vec<String>,
    },
    /// A special (Hamidon/Synthetic/Titan/Hydra/D-Sync/Prestige) enhancement:
    /// the category tag a slotted piece carries, matched against
    /// [`crate::EnhancementCatalog::special_families`], and the entry id in it.
    Special { family: String, id: String },
    /// A record the converter could place in no family — the standalone -Regen
    /// proc IOs and the dev-only pieces. Emitted rather than dropped so a build
    /// naming one fails loud instead of resolving to something plausible.
    Unclassified,
}

/// The crafted common-IO levels named by the section's own records.
///
/// Every stat family must name the same levels. One that disagrees is export
/// news rather than something to reconcile, so it fails loud instead of being
/// unioned into a band no single IO is crafted at.
fn craft_levels(entries: &BTreeMap<String, BoostEntry>) -> Result<Vec<Level>, String> {
    let mut by_stat: BTreeMap<String, Vec<Level>> = BTreeMap::new();
    for entry in entries.values() {
        let BoostEntry::CommonIo {
            level: Some(level),
            stats,
        } = entry
        else {
            continue;
        };
        let level = Level::from_i64(*level).ok_or_else(|| {
            format!("boost-index section: a common IO is crafted at {level}, which is not a level")
        })?;
        by_stat.entry(stats.join("/")).or_default().push(level);
    }
    if by_stat.is_empty() {
        return Err("boost-index section: no crafted common IO, so no level band".into());
    }
    let mut spellings: BTreeSet<Vec<Level>> = BTreeSet::new();
    for mut levels in by_stat.into_values() {
        levels.sort_unstable();
        levels.dedup();
        spellings.insert(levels);
    }
    if spellings.len() > 1 {
        let shown: Vec<String> = spellings
            .iter()
            .map(|levels| {
                levels
                    .iter()
                    .map(|level| level.get().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        return Err(format!(
            "boost-index section: common IOs are crafted at different levels per stat — {}",
            shown.join(" | ")
        ));
    }
    Ok(spellings.into_iter().next().expect("one spelling"))
}

/// The dataset's boost index.
#[derive(Debug, Clone, PartialEq)]
pub struct BoostIndex {
    /// Entries under the canonical spelling — the boost record's own name.
    pub entries: BTreeMap<String, BoostEntry>,
    /// Lower-cased name -> canonical spelling. The game's name lookups are
    /// case-insensitive and its own data takes advantage: Rebirth's Exploit
    /// Weakness boostset names `Crafted_Exploit_Weakness_C` while the record
    /// calls itself `..._c`. A client-printed name is matched through here.
    by_lower: HashMap<String, String>,
    /// The levels a common ("generic") IO is crafted at, ascending: the nine
    /// `Crafted_Accuracy_10` … `Crafted_Accuracy_50` records, in steps of five
    /// on every dataset, and nothing above 50. The tenth record per stat, bare
    /// `Crafted_Accuracy`, is the level-scaling template and states no level.
    ///
    /// It is the band the enhancement curves may be read at, and this section is
    /// the only one that states it — the curves themselves say what a level
    /// pays, not which levels exist, and Homecoming's class tables run 105
    /// entries deep (BOOST-6).
    craft_levels: Vec<Level>,
    /// (set id, piece number) -> the records that ARE that piece, canonically
    /// spelled and in the entries' own order.
    ///
    /// A LIST, not an entry, and that is the whole point: one set piece is
    /// usually two records — the crafted form and the attuned one — identical
    /// but for their names. A reader arriving with a set and a piece has not
    /// yet named a record, and this makes that visible instead of handing back
    /// whichever came first.
    by_set_piece: HashMap<(String, u8), Vec<String>>,
}

impl BoostIndex {
    /// Parse the contract's `boost-index` section. Absent ≠ malformed (Rule 1,
    /// mirroring the sibling section readers): an ABSENT section yields `None`
    /// (a hand-constructed `PowerDatabase` carries no index, and consumers
    /// surface that as an error, not a default), but a PRESENT section that
    /// doesn't parse is an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let entries: BTreeMap<String, BoostEntry> = serde_json::from_value(section.clone())
            .map_err(|e| format!("boost-index section: {e}"))?;
        let mut by_lower = HashMap::with_capacity(entries.len());
        let mut by_set_piece: HashMap<(String, u8), Vec<String>> = HashMap::new();
        for (name, entry) in &entries {
            if let Some(prior) = by_lower.insert(name.to_lowercase(), name.clone()) {
                return Err(format!(
                    "boost-index section: \"{name}\" collides with \"{prior}\" case-insensitively"
                ));
            }
            if let BoostEntry::IoSet { set, piece, .. } = entry {
                by_set_piece
                    .entry((set.clone(), *piece))
                    .or_default()
                    .push(name.clone());
            }
        }
        let craft_levels = craft_levels(&entries)?;
        Ok(Some(Self {
            entries,
            by_lower,
            craft_levels,
            by_set_piece,
        }))
    }

    /// The levels a common IO is crafted at, ascending. Never empty — a section
    /// that names none fails at parse.
    pub fn craft_levels(&self) -> &[Level] {
        &self.craft_levels
    }

    /// The entry for a name the game client printed, matched case-insensitively.
    /// `None` means this dataset has no such enhancement at all — distinct from
    /// [`BoostEntry::Unclassified`], which means it exists but nothing models it.
    pub fn get(&self, printed_name: &str) -> Option<&BoostEntry> {
        let canonical = self.by_lower.get(&printed_name.to_lowercase())?;
        self.entries.get(canonical)
    }

    /// Every record that IS this set's piece `piece`, canonically spelled.
    ///
    /// The direction [`Self::get`] does not run. A `.mbd` names a piece by a
    /// Mids UID, and where Mids has drifted from the game the only thing left
    /// in common is the set and the piece — so a reader lands here holding a
    /// coordinate rather than a name, and has to be told how many records sit
    /// at it. Usually two (crafted and attuned), sometimes one, and the caller
    /// must refuse rather than pick when it cannot narrow to one.
    pub fn io_set_records(&self, set: &str, piece: u8) -> &[String] {
        self.by_set_piece
            .get(&(set.to_string(), piece))
            .map_or(&[], Vec::as_slice)
    }

    /// The canonical spelling of a printed name — what an export writes back.
    pub fn canonical(&self, printed_name: &str) -> Option<&str> {
        self.by_lower
            .get(&printed_name.to_lowercase())
            .map(String::as_str)
    }
}
