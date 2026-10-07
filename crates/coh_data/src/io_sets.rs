//! IO-set catalog — the per-dataset set/piece data the enhancement aggregation
//! consumes (the beta `getIOSet`, `src/data/io-sets.ts`). The typed view of the
//! contract's `io-sets` section (`emit-contract.cjs`), sourced from each
//! dataset's `pipeline/<id>/io-sets-raw.json` (itself `boostsets.bin` metadata +
//! the `boosts/**` piece aspects, plus Homecoming's hand-curated
//! `hand-data/homecoming/io-sets-raw.json` where the binary falls short). A slotted
//! piece already carries its own `aspects`/`proc`/`piece_num` on the build
//! (`character::EnhancementKind::IoSet`),
//! but the SET-level facts the aggregation needs — `max_level` (attuned + level
//! cap), `rarity` (rarity multiplier), and each piece's `total_aspects`/`name`
//! (the multi-aspect scheduling penalty) — live only here. The `bonuses` tiers
//! (the buffs granted at 2/3/4/5/6 pieces) also live here; `coh_math::set_bonuses`
//! reads them.
//!
//! Data/calc split (D2): this struct is pure DATA. The aggregation
//! (`calculate_power_enhancement_bonuses`) lives in `coh_math::enhancement` and
//! takes this catalog as input.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One craftable piece of an IO set. Only the fields the aggregation reads are
/// captured; the section also carries `unique` and display fields we ignore
/// (hence no `deny_unknown_fields`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct IoSetPiece {
    /// Piece number (1-based), matched against a slot's `piece_num`.
    pub num: u8,
    /// Display name — its slash segments drive the multi-aspect count
    /// (`effective_aspect_count`) where the explicit aspect list omits special
    /// segments (procs, power grants).
    #[serde(default)]
    pub name: String,
    /// Enhancement attributes this piece directly enhances.
    #[serde(default)]
    pub aspects: Vec<String>,
    /// Whether the piece carries a global proc (adds one to the aspect count).
    #[serde(default)]
    pub proc: bool,
    /// Explicit effective-aspect override for pieces with internal multi-attribute
    /// effects (e.g. +Critical Hit%), when present.
    #[serde(default, rename = "totalAspects")]
    pub total_aspects: Option<i64>,
    /// Whether only one copy of this piece may be slotted across the whole build.
    /// The picker carries it onto the slotted [`crate::EnhancementKind::IoSet`]
    /// (`is_unique`) so the uniqueness rule has the fact at hand.
    #[serde(default)]
    pub unique: bool,
}

/// One effect of a set-bonus tier — a single stat buff. `stat` is the contract's
/// snake-case display key (`damage_resistance_(cold)`, `accuracy`); the internal
/// stat-key mapping lives in `coh_math::set_bonuses`. `value` is a percentage as a
/// whole number (`4` = +4%). `pvp`-only effects don't apply in PvE.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SetBonusEffect {
    #[serde(default)]
    pub stat: String,
    #[serde(default)]
    pub value: f64,
    #[serde(default)]
    pub pvp: bool,
    /// The pre-rendered display string from the export (e.g. `"+4.0% Recovery"`), rounded
    /// at extraction time. The tooltip splices the precise [`value`](Self::value) back into
    /// its leading `+X.X%` (the beta `formatBonusDesc`); empty on hand-built fixtures, where
    /// the caller synthesizes `"<stat> +<value>%"`. Not read by the calc — display only —
    /// but exported, so captured rather than discarded.
    #[serde(default)]
    pub desc: String,
    /// For `mez_resistance_(all)`: the mez types the `(all)` label stands for, as the
    /// lowercase keys [`GlobalBonuses::add_mez_resistance`](coh_math) routes by. The bonus
    /// is one multi-attrib template in the binary; the converter collapses it to a single
    /// stat name and carries the attribs here, so the calc spends it into the same per-type
    /// accumulators a power's own mez resistance feeds instead of re-deriving what "all"
    /// covers. Empty on every other stat, and on the one hand-curated tier the binary has
    /// no counterpart for (DATA-GAP-REGISTER MEZRES-1) — which the calc reports rather than
    /// assuming the usual six.
    #[serde(default, rename = "mez_types")]
    pub mez_types: Vec<String>,
}

/// One set-bonus tier — the buff granted once `pieces` distinct pieces of the set
/// are slotted in a single power (cumulative: a 4-piece slotting fires the 2-, 3-,
/// and 4-piece tiers). The contract always uses the `effects` array; the beta's
/// legacy flat `{stat, value}` shape never appears in the export (verified 0 across
/// all datasets), so it is not modeled here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SetBonus {
    pub pieces: u8,
    #[serde(default)]
    pub effects: Vec<SetBonusEffect>,
}

/// One IO set — the `getIOSet` shape. No `deny_unknown_fields`: the export section may
/// carry display fields beyond those modeled here.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IoSet {
    #[serde(default)]
    pub name: String,
    /// The set's icon filename (e.g. `sAbsoluteamazement.png`), resolved against the
    /// `Enhancements/{folder}` tree by the UI (beta `SlottedEnhancementIcon`). Display only.
    #[serde(default)]
    pub icon: String,
    /// Mapped display tier (`purple`, `uncommon`, …) — distinct from `rarity`.
    #[serde(default)]
    pub category: String,
    /// Binary rarity tier (`ECVeryRare`, `ECRare`, …) — the key
    /// `coh_math::enhancement::set_rarity_multiplier` reads.
    #[serde(default)]
    pub rarity: String,
    /// The set's slotting category (`"Ranged Damage"`, `"Holds"`, …) — the key a
    /// power's `allowedSetCategories` filters on ([`IoSetCatalog::sets_for_power`]).
    /// The wire field is `type` (a Rust keyword), read into `set_type`.
    #[serde(default, rename = "type")]
    pub set_type: String,
    /// Lowest character level a piece of this set can be slotted at — the picker's
    /// level ordering and Level-Up gating read it.
    #[serde(default)]
    pub min_level: i64,
    /// The set's top level — it caps a non-attuned piece's IO level. It used to
    /// double as the attuned marker (`<= 1`); [`attuned_only`](Self::attuned_only)
    /// is that fact read off the export instead.
    pub max_level: i64,
    /// The game ships this set only attuned: its pieces name no `Crafted_*` boost
    /// record, so there is nothing to slot at a level. Stamped per dataset by
    /// `scripts/extract-rebirth-io-sets-v2.py` from the export's piece membership.
    ///
    /// `#[serde(default)]` for the hand-built catalogs in these tests and in
    /// `coh_math`, which state pieces and levels and nothing else. That a real
    /// contract section carries it for EVERY set is guarded on the beta side,
    /// where the field is emitted,
    /// since that is the layer that can tell a missing field from a false one.
    #[serde(default)]
    pub attuned_only: bool,
    pub pieces: Vec<IoSetPiece>,
    /// The set-bonus tiers granted at increasing piece counts. Read by
    /// `coh_math::set_bonuses`; absent on some hand-built test catalogs (default
    /// empty rather than required, so a piece-only fixture still parses).
    #[serde(default)]
    pub bonuses: Vec<SetBonus>,
}

impl IoSet {
    /// The piece with this 1-based number, or `None` if the set has no such piece.
    pub fn piece(&self, num: u8) -> Option<&IoSetPiece> {
        self.pieces.iter().find(|p| p.num == num)
    }
}

/// The dataset's IO sets, keyed by set id.
#[derive(Debug, Clone, PartialEq)]
pub struct IoSetCatalog {
    pub sets: BTreeMap<String, IoSet>,
}

impl IoSetCatalog {
    /// Parse the contract's `io-sets` section. Malformed ≠ absent (Rule 1,
    /// matching the sibling section readers): an ABSENT section yields `None`
    /// (a hand-constructed `PowerDatabase` carries no catalog, and consumers
    /// surface that as an error, not a default), but a PRESENT section that
    /// doesn't parse is an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let sets: BTreeMap<String, IoSet> =
            serde_json::from_value(section.clone()).map_err(|e| format!("io-sets section: {e}"))?;
        Ok(Some(IoSetCatalog { sets }))
    }

    /// The piece count most of this dataset's sets have (Homecoming: 6, on 169 of 227).
    ///
    /// The picker marks the sets that DIFFER from this, so the majority stays unmarked and
    /// the odd sizes are the only ink in a long list. Derived rather than written as `6`:
    /// a literal would be a game constant living in UI logic (Rule 0), and it would mark
    /// the wrong rows on a fork whose catalogue is shaped differently.
    ///
    /// `None` for an empty catalogue — with no majority there is nothing to be the
    /// exception to, and the caller marks nothing rather than marking everything.
    pub fn most_common_piece_count(&self) -> Option<usize> {
        let mut tally: BTreeMap<usize, usize> = BTreeMap::new();
        for set in self.sets.values() {
            *tally.entry(set.pieces.len()).or_default() += 1;
        }
        // Ties break toward the larger size, so the answer is stable rather than
        // dependent on iteration order.
        tally
            .into_iter()
            .max_by_key(|&(size, count)| (count, size))
            .map(|(size, _)| size)
    }

    /// Look up a set by id, mirroring the beta `getIOSet`: the id as given, then
    /// the hyphen-stripped id (a backward-compat fallback for set ids stored with
    /// hyphens where the catalog key has none).
    pub fn get(&self, set_id: &str) -> Option<&IoSet> {
        self.sets
            .get(set_id)
            .or_else(|| self.sets.get(&set_id.replace('-', "")))
    }

    /// The sets a power can slot, mirroring the beta `getIOSetsForPower`
    /// (`io-sets.ts`): every set whose `type` is one of the power's allowed set
    /// categories, keyed by set id, sorted by set name. An empty allow-list yields
    /// no sets — a power with no `allowedSetCategories` accepts no IO sets. This is
    /// DISTINCT from generic IOs, where an absent allow-list means "accept all".
    ///
    /// Both sides of the test are the same binary field, `BoostSet.GroupName`, so the
    /// comparison is a plain string containment and no table stands between them. The
    /// beta used to route `set.type` through an identity lookup that answered
    /// `undefined` for any heading it had not been told about, dropping a fork's sets
    /// from the picker in silence; it was deleted at BOOST-2. Verified against the
    /// export: no set type falls outside the map, so no set diverges.
    pub fn sets_for_power<'a>(
        &'a self,
        allowed_categories: &[String],
    ) -> Vec<(&'a str, &'a IoSet)> {
        let mut matched: Vec<(&str, &IoSet)> = self
            .sets
            .iter()
            .filter(|(_, set)| allowed_categories.iter().any(|c| c == &set.set_type))
            .map(|(id, set)| (id.as_str(), set))
            .collect();
        matched.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
        matched
    }
}
