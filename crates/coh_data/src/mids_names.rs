//! Mids' internal name for a power → this dataset's, per powerset (DATA-GAP MBDIMPORT-2).
//!
//! A `.mbd` identifies a power by internal name alone, and that namespace has drifted from the
//! game's. Homecoming rotates internal names underneath stable display names: Tactical Arrow's
//! `Gymnastics` is Oil Slick Arrow in the export now, and the power the game shows as
//! "Gymnastics" is internally `Quickness`. Stalker Shield Defense is a three-cycle.
//!
//! **That makes an exact internal-name match the least reliable matcher rather than the most.**
//! The name exists, so nothing fails — it resolves to a different power, takes that power's
//! slots, and the entry that rightfully owned them is deduped away in silence. Matching on the
//! name alone lands five enhancements on the wrong Tactical Arrow power and reports nothing.
//!
//! Reading Mids for this is not a Rule 0 breach: the question is what MIDS calls a power, and
//! only Mids can answer it. The tables are DERIVED by `scripts/convert-mids-name-map.cjs`,
//! joining Mids' own power list to the export on DISPLAY name — the identity that survived the
//! rotation — so they are a measurement rather than a curated list of known breakages. The four
//! rows a bug report surfaced are 8 of the 115 Homecoming carries.
//!
//! **Both directions ride in one section on purpose.** The reader and the writer sharing one
//! table is what MBDEXPORT-4 cost: the import half read Mids' grade names correctly while the
//! export half wrote ours, and nobody had run the two in sequence until a real Mids refused the
//! whole build. Either direction can rot the other, so they are not allowed to drift apart in
//! separate files.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// The raw section as the contract carries it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MidsNameSection {
    /// OUR `group.powerset` (lower-cased) -> Mids' internal name (lower-cased) -> ours.
    #[serde(default)]
    name_map: HashMap<String, HashMap<String, String>>,
    /// Mids' `group.powerset` -> ours, where the group segment itself drifted
    /// (`Guardian_Composition` for our `Guardian_Comp`, MBDIMPORT-7).
    #[serde(default)]
    powerset_alias: HashMap<String, String>,
    /// The same join backwards, for the writer (MBDEXPORT-3).
    #[serde(default)]
    name_reverse: HashMap<String, HashMap<String, String>>,
    /// Our powerset key -> the path Mids writes, for the writer (MBDEXPORT-6).
    #[serde(default)]
    powerset_path: HashMap<String, String>,
}

/// The dataset's Mids power-name tables, both directions.
#[derive(Debug, Clone, PartialEq)]
pub struct MidsNames {
    name_map: HashMap<String, HashMap<String, String>>,
    powerset_alias: HashMap<String, String>,
    name_reverse: HashMap<String, HashMap<String, String>>,
    powerset_path: HashMap<String, String>,
    /// [`Self::powerset_path`] read backwards — Mids' literal path, folded, to our key
    /// (DATA-GAP MBDIMPORT-12). Built here rather than emitted because it is the same
    /// measurement: the generator pairs every set it can and states the pairing one way, and
    /// a reader needs the other.
    path_reverse: HashMap<String, String>,
}

impl MidsNames {
    /// Parse the contract's `mids-names` section. Absent ≠ malformed, per the sibling readers.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let raw: MidsNameSection = serde_json::from_value(section.clone())
            .map_err(|e| format!("mids-names section: {e}"))?;
        Ok(Some(Self {
            path_reverse: invert_paths(&raw.powerset_path),
            name_map: raw.name_map,
            powerset_alias: raw.powerset_alias,
            name_reverse: raw.name_reverse,
            powerset_path: raw.powerset_path,
        }))
    }

    /// This dataset's powerset key for a path a `.mbd` spelled, lower-cased.
    ///
    /// Mids' path and ours are usually the same string, so an unlisted path is returned
    /// unchanged. That is a fallthrough, not a guess: the caller still has to resolve the
    /// result against real powersets, and an unknown one fails there rather than here.
    ///
    /// **Two doors, because the alias table answers for a narrower population than the
    /// question** (DATA-GAP MBDIMPORT-12). It is written only for a pair that also carries a
    /// rotated power name, so a set Mids merely RESPELLS — `Epic.Fire_Mastery_Guardian` for
    /// our `Epic.Guardian_Fire_Mastery` — has no row, and a reader loses every pick from it in
    /// silence: 86 sets on Homecoming, 20 on Rebirth, 3 on Thunderspy. The path table pairs
    /// every set the generator could join and states that pairing the writer's way round, so
    /// reading it backwards answers the rest with no new data.
    pub fn powerset_key(&self, mids_path: &str) -> String {
        let key = fold_path(mids_path);
        if let Some(ours) = self.powerset_alias.get(&key) {
            return ours.clone();
        }
        self.path_reverse.get(&key).cloned().unwrap_or(key)
    }

    /// This dataset's internal name for a power a `.mbd` named, within one powerset.
    ///
    /// `None` means the map has no row for that pair, which is the COMMON case and means the
    /// name did not rotate — the caller falls through to matching the name it was given. It
    /// does NOT mean the power is absent; only a lookup against the powerset can say that.
    pub fn power_name(&self, powerset_key: &str, mids_name: &str) -> Option<&str> {
        self.name_map
            .get(&powerset_key.to_lowercase())?
            .get(&mids_name.to_lowercase())
            .map(String::as_str)
    }

    /// Whether a Mids name is one this powerset ROTATED AWAY from — a name Mids still lists
    /// that now belongs to a different power here, or to none.
    ///
    /// The distinction matters because it separates the two halves of MBDIMPORT-2. Mids' Stalker
    /// Willpower `Reconstruction` has no counterpart at all, and falling through to the
    /// same-named Regeneration power would bind the wrong one — so the reader must refuse it,
    /// loudly, WITH the pieces it was holding. That refusal is MBDIMPORT-5's territory.
    pub fn is_rotated_away(&self, powerset_key: &str, mids_name: &str) -> bool {
        let Some(rows) = self.name_map.get(&powerset_key.to_lowercase()) else {
            return false;
        };
        let name = mids_name.to_lowercase();
        // The name is claimed by the map (it maps somewhere) — not rotated away.
        if rows.contains_key(&name) {
            return false;
        }
        // Nothing maps TO it either, yet a sibling in this set maps somewhere: the set rotated
        // and this name was not carried across.
        !rows.is_empty() && rows.values().any(|ours| ours.to_lowercase() == name)
    }

    /// Mids' internal name for one of ours, for the writer.
    pub fn mids_power_name(&self, powerset_key: &str, our_name: &str) -> Option<&str> {
        self.name_reverse
            .get(&powerset_key.to_lowercase())?
            .get(&our_name.to_lowercase())
            .map(String::as_str)
    }

    /// The path Mids writes for one of our powersets, for the writer.
    pub fn mids_powerset_path(&self, powerset_key: &str) -> Option<&str> {
        self.powerset_path
            .get(&powerset_key.to_lowercase())
            .map(String::as_str)
    }

    /// Powersets carrying at least one rotated name, for a census that states its population.
    pub fn rotated_powerset_count(&self) -> usize {
        self.name_map.len()
    }

    /// Does the alias table itself answer for this Mids path? The census's discriminator: a set
    /// it covers was never lost, and a set only the inverted path table reaches was.
    pub fn alias_answers(&self, mids_path: &str) -> bool {
        self.powerset_alias.contains_key(&fold_path(mids_path))
    }

    /// Sets the alias table answers for, for the census beside [`Self::path_reverse_only_count`].
    pub fn alias_count(&self) -> usize {
        self.powerset_alias.len()
    }

    /// Sets Mids RESPELLS that only the path table answers for — the population MBDIMPORT-12
    /// recovered, for a census that states it rather than assuming it.
    ///
    /// A path Mids spells exactly as we do needs no door at all: [`Self::powerset_key`]
    /// returns it unchanged and the set-path index answers. So the count is of the pairs that
    /// DIFFER and carry no alias row, which is the population that resolved to nothing before.
    ///
    /// It counts every paired set, most of which no build can hold — 71 of Homecoming's 86 are
    /// incarnate pet sets. The census beside it is the one that
    /// narrows to what a `.mbd` can name.
    pub fn path_reverse_only_count(&self) -> usize {
        self.path_reverse
            .iter()
            .filter(|(mids, ours)| *mids != *ours && !self.powerset_alias.contains_key(*mids))
            .count()
    }
}

/// `ours -> Mids' literal path` read backwards, folded to the spelling lookups compare on.
///
/// **A Mids path claimed by two of our sets is dropped rather than resolved to one of them.**
/// The pairing is one-to-one on all four forks today (0 collisions across 3,596 / 3,470 /
/// 3,453 / 3,582 pairs), and a fork that broke that would be handing this table a choice it
/// has no grounds to make — the same refusal [`crate::mids_uids`] makes for the same reason.
fn invert_paths(paths: &HashMap<String, String>) -> HashMap<String, String> {
    let mut inverted: HashMap<String, String> = HashMap::new();
    let mut collided: Vec<String> = Vec::new();
    for (ours, mids) in paths {
        let key = fold_path(mids);
        match inverted.insert(key.clone(), ours.to_lowercase()) {
            Some(other) if other != ours.to_lowercase() => collided.push(key),
            _ => {}
        }
    }
    for key in collided {
        inverted.remove(&key);
    }
    inverted
}

/// A Mids path folded the way a `.mbd`'s own is by the time it reaches a lookup: lower-cased,
/// with each segment trimmed.
///
/// **The trim is load-bearing, and it is the path table's literal spelling that makes it so.**
/// That table keeps Mids' exact string because the WRITER needs it — Mids resolves a path with
/// an ordinal `==`, and four of Rebirth's Guardian secondaries are spelled
/// `Guardian_Composition.Dark_Composition ` with a trailing space it will not forgive
/// (MBDEXPORT-3's `"Shukuchi "` is the same shape one layer down). A reader trims, because Mids
/// writes that space into the file too — `Energy_Composition .Kinetic_Shield` — so keying the
/// inverse on the literal would leave exactly those sets unreachable through the door built to
/// reach them.
fn fold_path(path: &str) -> String {
    path.split('.')
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(".")
        .to_lowercase()
}
