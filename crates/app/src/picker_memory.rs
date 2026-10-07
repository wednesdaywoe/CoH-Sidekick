//! Where the enhancement picker opens — the navigation half of "slotting a whole power is fast".
//!
//! Re-opening the picker on a power you are part-way through slotting should land where you were,
//! not at the top of a 47-category catalogue. There are three answers to "where", and they are
//! ranked rather than blended, because the more specific one is always the better guess:
//!
//! 1. **The slot already holds a piece** → that piece's own section, with its set scrolled to.
//!    Opening a filled slot is a CHANGE, and the thing being changed is the best answer to what
//!    the user is looking at. It beats any remembered choice because it is about this slot, not
//!    about the power.
//! 2. **This power was slotted before** → wherever it was left. One power is one slotting job.
//! 3. **Neither** → the IO Sets tab, unfiltered. Sets are what a build is made of; the picker
//!    used to open on Generic IO, which is the tab a finished build touches least.
//!
//! **The scope is per power, and that is a choice with alternatives.** Per SET would lose the
//! rarity and slot-category the user navigated to; per SESSION would carry one power's answer
//! onto the next, which is wrong the moment the next power is a different kind of thing — a
//! travel power has nothing to learn from a Hold. Per power matches the unit the user is actually
//! working on, and matches the beta (`lastPickerFilterByPower`, keyed on the power's name).
//! [`the_last_place_is_kept_per_power`] is the test that says so, so a later session can tell this
//! apart from a default nobody chose.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Which tab, and how its sidebar was narrowed. Stored as wire strings rather than the picker's
/// own enums so a persisted location outlives a rename of either.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct PickerLocation {
    /// The tab's token (`"io-sets"`, `"generic"`, `"special"`, `"origin"`).
    pub tab: String,
    /// The slot-category facet (the export's set `type`), if one was chosen.
    #[serde(default)]
    pub set_type: Option<String>,
    /// The rarity facet (the export's set `category`), if one was chosen.
    #[serde(default)]
    pub rarity: Option<String>,
}

/// Every power's last picker location, keyed by [`coh_data::power_address`].
///
/// Keyed on the ADDRESS, not the internal name: a power's internal name is not unique across sets
/// (the beta keys on the display name and inherits that collision), so two powers sharing a name
/// would otherwise share one memory and drag each other around the catalogue.
#[derive(Clone, PartialEq, Debug, Default, Serialize, Deserialize)]
pub struct PickerMemory {
    #[serde(default)]
    places: BTreeMap<String, PickerLocation>,
}

impl PickerMemory {
    /// Where this power was left, if it has been slotted before.
    pub fn place(&self, address: &str) -> Option<&PickerLocation> {
        self.places.get(address)
    }

    /// Remember where the user navigated to for this power. Replaces rather than merges — the
    /// location is one answer, and half of a previous one is not a place.
    pub fn remember(&mut self, address: &str, place: PickerLocation) {
        self.places.insert(address.to_string(), place);
    }

    /// How many powers are remembered — the bound the persistence cares about.
    ///
    /// **Nothing calls this, so that bound is not enforced.** The remembered set grows without
    /// limit. Annotated rather than deleted 2026-09-26: the doc names a real intention and
    /// deleting the accessor would delete the only record of it. Either apply the bound at the
    /// persist site or drop the claim.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.places.len()
    }

    #[allow(dead_code)] // same non-caller as `len`.
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }
}

/// The tab tokens, as the persisted [`PickerLocation`] spells them. One spelling, used by the
/// store, the opening rule and the tab buttons, so a rename cannot desynchronise them.
pub mod tab {
    pub const GENERIC: &str = "generic";
    pub const IO_SETS: &str = "io-sets";
    pub const SPECIAL: &str = "special";
    pub const ORIGIN: &str = "origin";
}

/// Where a freshly opened picker points.
#[derive(Clone, PartialEq, Debug)]
pub struct Opening {
    pub place: PickerLocation,
    /// The set to scroll into view, when the opening is a change to an existing set piece.
    pub jump_to_set: Option<String>,
}

/// Where the picker should open, by the ranked rule this module's header states: the slot's own
/// piece, else this power's remembered place, else the IO Sets tab unfiltered.
///
/// `catalog` resolves a slotted piece's set to the facets that would show it. A piece whose set
/// the catalog does not carry still lands on the IO Sets tab — the tab is right even when the
/// narrowing cannot be worked out, and a wrong narrowing would hide the very piece being changed.
pub fn opening_location(
    filled: Option<&coh_data::Enhancement>,
    catalog: Option<&coh_data::IoSetCatalog>,
    remembered: Option<&PickerLocation>,
) -> Opening {
    if let Some(enhancement) = filled {
        return section_for(enhancement, catalog);
    }
    if let Some(place) = remembered {
        return Opening {
            place: place.clone(),
            jump_to_set: None,
        };
    }
    Opening {
        place: PickerLocation {
            tab: tab::IO_SETS.to_string(),
            set_type: None,
            rarity: None,
        },
        jump_to_set: None,
    }
}

/// The section an already-slotted piece lives in, so "change this" opens on the thing being
/// changed.
fn section_for(
    enhancement: &coh_data::Enhancement,
    catalog: Option<&coh_data::IoSetCatalog>,
) -> Opening {
    let bare = |tab: &str| Opening {
        place: PickerLocation {
            tab: tab.to_string(),
            set_type: None,
            rarity: None,
        },
        jump_to_set: None,
    };
    match &enhancement.kind {
        coh_data::EnhancementKind::GenericIo { .. } => bare(tab::GENERIC),
        coh_data::EnhancementKind::Special { .. } => bare(tab::SPECIAL),
        coh_data::EnhancementKind::Origin { .. } => bare(tab::ORIGIN),
        coh_data::EnhancementKind::IoSet { set_id, .. } => {
            let set = catalog.and_then(|catalog| catalog.get(set_id));
            Opening {
                place: PickerLocation {
                    tab: tab::IO_SETS.to_string(),
                    set_type: set.map(|set| set.set_type.clone()),
                    rarity: set.map(|set| set.category.clone()),
                },
                jump_to_set: Some(set_id.clone()),
            }
        }
    }
}
