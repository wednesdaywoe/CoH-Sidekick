//! Enhancement catalog — the pickable enhancement families the slotting UI
//! offers, read from the contract's `enhancements` section (`emit-contract.cjs`).
//! Its families and common-IO types are derived from the binary export; only the
//! character-origin list still comes from the frozen `src/data/enhancements`
//! oracle. Distinct from
//! [`crate::io_sets`] (the multi-piece IO sets) and
//! [`crate::enhancement_curves`] (the value/ED curves the calc applies): this is
//! the roster of what a player can slot, which a power's allow-lists filter.
//!
//! Captures every family the section carries: the generic ("common") IO types,
//! the character-origin list, and the special families (Hamidon, Synthetic Hamidon,
//! Titan, Hydra, D-Sync, Prestige), each surfaced to its picker tab. No `deny_unknown_fields` —
//! a family this reader doesn't model is preserved in the raw section, not a load
//! error.

use crate::character::AspectValue;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// One special-enhancement definition (a Hamidon, Synthetic Hamidon, Titan, Hydra,
/// D-Sync or Prestige origin), the beta `SpecialEnhancementDef`. Each `aspect` carries its own
/// authored percentage — the calc reads them straight (`coh_math::enhancement`),
/// no schedule derivation. Keyed by the export's internal id (e.g. `nucleolus`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SpecialEnhancementDef {
    pub name: String,
    /// The binary boost record this def was derived from (`Hamidon_Damage_Accuracy`) — the
    /// spelling the game client prints for a slotted piece, and so the join a game-client
    /// import matches on and an export writes back.
    #[serde(default)]
    pub boost: String,
    #[serde(default)]
    pub aspects: Vec<AspectValue>,
}

/// The dataset's enhancement catalog (the beta `src/data/enhancements`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnhancementCatalog {
    /// The generic ("common") IO types — one single-aspect enhancement each
    /// (`"Damage"`, `"Recharge"`, …), derived from the crafted boost family
    /// (`convert-boost-index.cjs`) rather than the beta's hand `COMMON_IO_TYPES`,
    /// which named 25 of the game's 26 (BOOST-1). A power accepts the subset named
    /// in its `allowedEnhancements` ([`crate::Power::allowed_enhancements`]).
    #[serde(default)]
    pub common_io_types: Vec<String>,

    /// The special-enhancement families the Special tab offers — Hamidon Origin,
    /// Synthetic Hamidon Origin, Titan, Hydra, D-Sync, and Prestige — each an id→def map. `BTreeMap` orders
    /// entries by id (deterministic); the export's authored order is not carried,
    /// which only affects the picker's within-family display order, not any value.
    /// A power accepts a family entry when one of its aspects matches the power's
    /// allow-list ([`Self::special_available`]).
    ///
    /// Read them through [`Self::special_families`], never by naming the fields — a
    /// family added here and forgotten at a call site is a family that silently
    /// stops being offered or graded.
    #[serde(default)]
    pub hamidon: BTreeMap<String, SpecialEnhancementDef>,
    /// The Synthetic Hamidon pieces. Their templates are byte-identical to the
    /// Hamidon originals, but they are separately purchasable enhancements with
    /// their own display names, so they are their own family rather than aliases —
    /// a build that slots one has to round-trip as the piece the player owns.
    #[serde(default, rename = "syntheticHamidon")]
    pub synthetic_hamidon: BTreeMap<String, SpecialEnhancementDef>,
    #[serde(default)]
    pub titan: BTreeMap<String, SpecialEnhancementDef>,
    #[serde(default)]
    pub hydra: BTreeMap<String, SpecialEnhancementDef>,
    #[serde(default)]
    pub dsync: BTreeMap<String, SpecialEnhancementDef>,
    #[serde(default)]
    pub prestige: BTreeMap<String, SpecialEnhancementDef>,

    /// The character-origin list (`Magic`, `Mutation`, …) the export owns. Carried
    /// for the Origin tab / SO flavor; the calc never keys enhancement value on it.
    #[serde(default)]
    pub origins: Vec<String>,
}

impl EnhancementCatalog {
    /// Parse the contract's `enhancements` section. Absent ≠ malformed (Rule 1,
    /// mirroring [`crate::io_sets::IoSetCatalog::from_section`]): an ABSENT section
    /// yields `None` (a hand-constructed `PowerDatabase` carries no catalog, and
    /// consumers surface that as an error, not a default), but a PRESENT section
    /// that doesn't parse is an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let catalog: EnhancementCatalog = serde_json::from_value(section.clone())
            .map_err(|e| format!("enhancements section: {e}"))?;
        Ok(Some(catalog))
    }

    /// The generic IOs a power can slot: its allowed common-IO types in catalog
    /// order, mirroring the beta `getAvailableGenericIOs`. The tri-state of
    /// `allowedEnhancements` is load-bearing (see [`crate::Power`]): `None`
    /// (127 pseudo-pet/temp powers) accepts EVERY type; a present list keeps only
    /// the named ones; an empty list accepts none.
    pub fn generic_ios_for_power<'a>(
        &'a self,
        allowed_enhancements: Option<&[String]>,
    ) -> Vec<&'a str> {
        match allowed_enhancements {
            None => self.common_io_types.iter().map(String::as_str).collect(),
            Some(allowed) => self
                .common_io_types
                .iter()
                .filter(|kind| allowed.iter().any(|a| a == *kind))
                .map(String::as_str)
                .collect(),
        }
    }

    /// The special-enhancement families in the picker's fixed order, each paired
    /// with its display label and the `category` tag a slotted piece carries (the
    /// beta `SPECIAL_SECTIONS`; D-Sync's tag is `d-sync` and Synthetic Hamidon's is
    /// `synthetic-hamidon`, hyphenated, though their export keys are `dsync` and
    /// `syntheticHamidon`). The UI filters each family with
    /// [`Self::special_available`] against the power's allow-list.
    pub fn special_families(
        &self,
    ) -> [(
        &'static str,
        &'static str,
        &BTreeMap<String, SpecialEnhancementDef>,
    ); 6] {
        [
            ("Hamidon Origin", "hamidon", &self.hamidon),
            (
                "Synthetic Hamidon Origin",
                "synthetic-hamidon",
                &self.synthetic_hamidon,
            ),
            ("Titan Origin", "titan", &self.titan),
            ("Hydra Origin", "hydra", &self.hydra),
            ("D-Sync Origin", "d-sync", &self.dsync),
            ("Prestige", "prestige", &self.prestige),
        ]
    }

    /// Whether a special def is slottable in a power: at least one of its aspects'
    /// stats appears in the power's allow-list (the beta `filterSpecialEnhancements`
    /// — "some aspect matches"). The `allowed_enhancements` tri-state carries the
    /// same meaning as for generic IOs: `None` (pseudo-pet/temp powers) accepts
    /// every entry; an empty list accepts none.
    pub fn special_available(
        def: &SpecialEnhancementDef,
        allowed_enhancements: Option<&[String]>,
    ) -> bool {
        match allowed_enhancements {
            None => true,
            Some(allowed) => def
                .aspects
                .iter()
                .any(|aspect| allowed.iter().any(|a| a == &aspect.stat)),
        }
    }
}
