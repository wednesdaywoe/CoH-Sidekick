//! Accolade toggles — derived from the accolade-category powerset, never hand-authored.
//!
//! Accolades are ordinary auto-on Self powers (`Temporary_Powers.Accolades`), merged into
//! the contract like the inherents. The planner presents the *permanent stat* ones as
//! independent on/off toggles (DATA-GAP ACCOLADE-1); the click/travel/summon members
//! (Eye of the Magus, Long Range Teleport) grant a timed buff on use and are not offered —
//! folding one permanently into a build because it was *earned* would be wrong, and
//! [`coh_math`]'s gather likewise folds only `Auto` members.
//!
//! The toggle set is DERIVED, not a curated name list: a stat toggle is an `Auto` member
//! carrying a permanent +Max HP / +Max End atom — the same derivation the beta's
//! `getAccolades()` runs over its generated module, read atom-native here. Deriving is what
//! surfaced the four the beta's old hand silo dropped (Iron Man, Super Patriot, Labyrinth
//! Conqueror, Mazebreaker) and what keeps each buff's magnitude the def's own.
//!
//! Faction is read from the power's `activateRequires` gate (`… hero eq` / `… villain eq`)
//! and shown as a label only. There is no mutual exclusion: the real gates don't 1:1-pair
//! (End-5 has two hero powers against one villain; Iron Man is villain-only; the Labyrinth
//! pair is faction-less), so every toggle stands alone.

use crate::atom::{Aspect, EffectType};
use crate::database::PowerDatabase;
use crate::power::Power;

/// The powerset category the extracted `Temporary_Powers.Accolades` set carries. Accolades
/// live in their own category rather than under an archetype, which is what distinguishes
/// them from every other powerset in the db (the set *id* is emitter-local; the category is
/// the converted datum).
pub const ACCOLADE_CATEGORY: &str = "accolade";

/// The hero/villain gate an accolade's `activateRequires` names, as a display label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccoladeFaction {
    Hero,
    Villain,
    Any,
}

impl AccoladeFaction {
    pub fn label(self) -> &'static str {
        match self {
            AccoladeFaction::Hero => "Hero",
            AccoladeFaction::Villain => "Villain",
            AccoladeFaction::Any => "Any",
        }
    }
}

/// One offered accolade toggle: the def, its stored id, its faction label, and any mode its
/// buff is conditional on.
pub struct AccoladeToggle<'a> {
    pub power: &'a Power,
    /// The id [`crate::character::CharacterState::accolades`] stores — the internal name,
    /// lower-cased (the fold the gather resolves case-insensitively).
    pub id: String,
    pub faction: AccoladeFaction,
    /// Modes the game requires for this accolade to apply, as display labels. Empty for the
    /// permanent ones, which is nearly all of them.
    ///
    /// The gather folds every `Auto` accolade unconditionally, so a toggle with a requirement
    /// here DOES move the totals — it just only holds while that mode is live. The picker says
    /// so rather than the number lying. Read off `modesRequired`, so a fork that gates a
    /// different accolade, or stops gating these, needs no code change.
    pub requires_modes: Vec<String>,
}

impl PowerDatabase {
    /// Every member of the accolade-category powersets, toggles and clicks alike — the
    /// resolution universe (the gather resolves a stored id against this).
    pub fn accolade_powers(&self) -> impl Iterator<Item = &Power> {
        self.accolade_powers_with_set().map(|(_, power)| power)
    }

    /// The same walk, keeping each accolade's owning set id. Internal names are not unique
    /// across sets, so any consumer that must ADDRESS one accolade (rather than just read it)
    /// needs the pair — the per-source breakdown ledger being the first.
    pub fn accolade_powers_with_set(&self) -> impl Iterator<Item = (&str, &Power)> {
        self.powersets
            .iter()
            .filter(|set| set.category.as_deref() == Some(ACCOLADE_CATEGORY))
            .flat_map(|set| set.powers.iter().map(|power| (set.id.as_str(), power)))
    }

    /// The permanent stat-buff accolades the planner offers as toggles, in game order.
    pub fn accolade_toggles(&self) -> Vec<AccoladeToggle<'_>> {
        self.accolade_powers()
            .filter(|power| is_stat_toggle(power))
            .map(|power| AccoladeToggle {
                power,
                id: power.ident().to_ascii_lowercase(),
                faction: faction(power),
                requires_modes: required_modes(power),
            })
            .collect()
    }
}

/// A stat toggle is an `Auto` member carrying a permanent +Max HP / +Max End atom. The
/// non-stat `Auto` members this drops carry no `Max`-aspect atom at all (Portable
/// Workbench grants a crafting table; the legacy Vanguard challenge is a flag).
fn is_stat_toggle(power: &Power) -> bool {
    is_auto(power)
        && power.atoms.iter().any(|atom| {
            atom.aspect == Some(Aspect::Max)
                && matches!(
                    atom.effect_type,
                    Some(EffectType::MaxHp | EffectType::MaxEndurance)
                )
        })
}

/// The modes `power` names in `modesRequired`, as display labels.
///
/// The accolade converter is the fourth tree to call `assignModes` and was the last to get it,
/// which is why this field read empty on every accolade until 2026-08-21. Labelling goes
/// through [`crate::mode_label`] like every other mode control: the keys are the game's own
/// tokens and inventing a nicer name would be inventing.
fn required_modes(power: &Power) -> Vec<String> {
    power
        .extra
        .get("modesRequired")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(crate::mode_label)
        .collect()
}

fn is_auto(power: &Power) -> bool {
    power
        .extra
        .get("powerType")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case("auto"))
}

/// Read the faction off the `activateRequires` gate. The gates are uniform RPN
/// (`type char> hero eq`, optionally conjoined), so the faction token's presence IS the
/// datum — the same reading the beta's `accoladeFaction` does.
fn faction(power: &Power) -> AccoladeFaction {
    // Joined only to ask a question of it — a token boundary can neither create nor
    // destroy this clause. Never split the result back apart (COND-8).
    let gate = crate::expression_text(
        crate::expression_tokens(power.extra.get("activateRequires")).as_deref(),
    );
    if gate.contains("hero eq") {
        AccoladeFaction::Hero
    } else if gate.contains("villain eq") {
        AccoladeFaction::Villain
    } else {
        AccoladeFaction::Any
    }
}
