//! The inherent powers the game grants a character — the ones it never asks them to pick.
//!
//! Three universal families (basic, fitness, prestige sprints) plus the one power the
//! archetype itself grants (Defiance, Fury, Gauntlet …). They cost no power pick, they
//! cannot be removed, and several of them take enhancement slots, which is why they live in
//! the build at all: [`CharacterState::picked_powers`](crate::CharacterState::picked_powers)
//! skips them while
//! [`all_selected`](crate::CharacterState::all_selected) does not.
//!
//! **Provenance.** The three universal lists come from the contract's `levels` section, and
//! the basic and prestige halves are now read per fork from that fork's own export
//! (`scripts/convert-basic-inherents.cjs`): a power absent from a fork's list is one that
//! fork does not have. It read the same on every fork until 2026-08-13, because `levels.ts`
//! exists only under `datasets/homecoming/` and every bundle got Homecoming's copy — which
//! is how Thunderspy characters came to be offered a Ninja Run, a Beast Run, an Athletic Run
//! and five prestige sprints that fork has never shipped (INHERENT-4). The counts now differ
//! (15 / 14 / 7), and each is pinned.
//!
//! The FITNESS half is still the beta's hand-authored membership, copied per bundle. Its
//! four powers exist on every fork, so nothing is known to be wrong with it — but nothing
//! proves it either, and that is the residual.
//!
//! The sourced half of that section has been superseded once already: pick levels, slot
//! grants, pool/epic unlock levels and the pool cap now come from
//! [`crate::LevelingSchedule`] (read from `schedules.bin`), and nothing here reads the
//! `levels` copies of those. This module reads only names, categories and slot ceilings.
//!
//! Nothing here is a name table: the families come from the section's own keys, the
//! archetype's own inherent comes from matching the archetype's declared inherent NAME
//! against the `Inherent` powerset — no archetype ever appears in an `if`.

use crate::character::{InherentCategory, SelectedPower};
use serde::Deserialize;
use serde_json::Value;

/// The synthetic powerset every granted inherent is tagged with. Inherents have no owning
/// powerset of their own, and the whole engine — gather, the def lookup fallback, the
/// display resolver — addresses them through this one id.
pub const INHERENT_SET: &str = "Inherent";

/// One power the game grants outright.
#[derive(Debug, Clone, PartialEq)]
pub struct InherentGrant {
    /// The `internalName` the build stores and the def lookup resolves.
    pub internal_name: String,
    /// Display name.
    pub name: String,
    pub category: InherentCategory,
    /// Enhancement slots this power accepts. `0` marks the ones the game gives no slots at
    /// all (the archetype inherents and the prestige travel powers) — they render, they
    /// contribute, they cannot be slotted.
    pub max_slots: u8,
}

/// Every universal inherent grant, in grant order (archetype inherent first once the caller
/// supplies one, then basic, fitness, prestige).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InherentGrants {
    pub grants: Vec<InherentGrant>,
}

/// The `levels` section's three inherent lists. Deserialized by shape, so a section that
/// renames or drops a list is a decode error rather than a silently shorter grant list.
#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
struct LevelsSection {
    basic_inherent_powers: Vec<WireInherent>,
    inherent_fitness_powers: Vec<WireInherent>,
    prestige_sprint_powers: Vec<WireInherent>,
}

#[derive(Deserialize)]
struct WireInherent {
    name: String,
    #[serde(rename = "internalName")]
    internal_name: String,
    category: InherentCategory,
    #[serde(rename = "maxSlots", default)]
    max_slots: u8,
}

impl InherentGrants {
    /// Read the universal grants from the contract's `levels` section.
    ///
    /// An ABSENT section yields `None` — a hand-built database carries no `levels`, and a
    /// caller must surface "this dataset lists no inherents" rather than quietly granting
    /// none. A PRESENT section that does not parse is an error (Rule 1), matching the
    /// sibling readers.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let wire: LevelsSection = serde_json::from_value(section.clone())
            .map_err(|error| format!("levels section: {error}"))?;

        let grants: Vec<InherentGrant> = [
            wire.basic_inherent_powers,
            wire.inherent_fitness_powers,
            wire.prestige_sprint_powers,
        ]
        .into_iter()
        .flatten()
        .map(|entry| InherentGrant {
            internal_name: entry.internal_name,
            name: entry.name,
            category: entry.category,
            max_slots: entry.max_slots,
        })
        .collect();

        if grants.is_empty() {
            return Err("levels section: no inherent powers".into());
        }
        Ok(Some(InherentGrants { grants }))
    }

    /// Whether `internal_name` is granted outright — which is also why a powerset that
    /// republishes one of these (the legacy Fitness pool still ships its four powers) has
    /// nothing pickable in it. That exclusion needs no rule of its own: each of those
    /// powers carries `Inherent.Fitness.<name> !` as its own `requires`, so
    /// [`crate::pick_rules`] closes it the moment the inherent is granted.
    pub fn is_granted(&self, internal_name: &str) -> bool {
        self.grants
            .iter()
            .any(|grant| grant.internal_name == internal_name)
    }
}

/// Build the build's inherent list: the archetype's own inherent (when the dataset has a
/// power for it) followed by every universal grant.
///
/// `archetype_inherent` is the archetype's inherent power identity, resolved by the caller
/// from the dataset — `None` when no archetype is chosen, or when the dataset ships no
/// power for the one it declares, which is a visible gap rather than a substitution.
///
/// `auto_granted_slot_levels` is [`LevelingSchedule::auto_granted_slot_levels`](crate::LevelingSchedule::auto_granted_slot_levels):
/// the levels at which a fork hands a named inherent extra slots OUTSIDE the user budget
/// (Rebirth does this for two of them; the others' map is empty). Those slots are counted
/// into [`SelectedPower::inherent_slot_count`], which is what keeps them off the budget.
///
/// Existing selections are preserved by identity: a rebuilt list carries over each power's
/// slots and toggle state, so re-granting after a level change or an archetype swap does not
/// wipe the user's slotting.
pub fn granted_inherents(
    grants: &InherentGrants,
    archetype_inherent: Option<&InherentGrant>,
    auto_granted_slot_levels: &std::collections::BTreeMap<String, Vec<u8>>,
    level: u8,
    existing: &[SelectedPower],
) -> Vec<SelectedPower> {
    // The names identity itself answers for. An unstamped entry under one of these is already
    // being rebuilt by the map below — it is a universal inherent a reader just handed over, not
    // a grant — so carrying it through as well would put the power in the list twice.
    //
    // DEFENSIVE, and mutation-tested as such (2026-09-26): removing this term leaves
    // `grant_survives_a_read.rs` green, including the leg that counts copies with every inherent
    // switched on. The duplicate cannot survive today because `granted_powers::apply_to_inherents`
    // drops an unstamped entry no grant names, and every production caller runs that pass in the
    // same edit as this one (`build_io`, `shell`, `level_control`, `identity`). The term is here so
    // this function is right on its own rather than only right in that pair — a caller that ever
    // runs it alone would otherwise double every universal inherent a file carries — but it
    // should not be read as proven.
    let by_identity: std::collections::BTreeSet<&str> = archetype_inherent
        .into_iter()
        .chain(grants.grants.iter())
        .map(|grant| grant.internal_name.as_str())
        .collect();
    archetype_inherent
        .into_iter()
        .chain(grants.grants.iter())
        .map(|grant| {
            let auto_slots =
                auto_granted_slot_count(auto_granted_slot_levels, &grant.internal_name, level);
            // Through [`crate::current_inherent_name`], so a build saved before
            // the universal inherents were sourced from the export carries its
            // slotting across the eight names that changed.
            let carried = existing.iter().find(|power| {
                crate::current_inherent_name(&power.internal_name) == grant.internal_name
            });
            select(grant, auto_slots, carried)
        })
        // The reconcile's pick-gated grants ([`crate::granted_powers`]) share this list but
        // not this rebuild: carrying them through whole preserves their slotting, and their
        // own reconcile — which always runs right after this sync — drops any whose gate no
        // longer holds. Rebuilding them here from identity would be wrong twice: identity
        // doesn't know the picks, and dropping them would strip the user's slots.
        //
        // An UNSTAMPED entry rides through on the same terms, and for the same reason one step
        // earlier: a `.skif` read stores no category (`skif.rs` drops it deliberately and names
        // this reconcile pair as what re-derives it, rule 6), so the read hands over a grant
        // with `None` where the stamp should be. Keeping only the stamped ones dropped it here,
        // and since THIS sync runs first on the load path (`build_io::adopt_as`) the grant
        // reconcile then had nothing to carry state from — it re-created the power from its def,
        // toggle off and enhancements gone. Eleven powers across four Thunderspy archetypes,
        // Hide among them (GRANTLOCK-1). Carrying it through hands it to the pass that can
        // actually judge it: `granted_powers::apply_to_inherents` claims it when a gate still
        // names it and drops it when none does, so nothing unaccounted-for survives the pair.
        .chain(
            existing
                .iter()
                .filter(|power| match power.inherent_category {
                    Some(InherentCategory::Granted) => true,
                    None => {
                        !by_identity.contains(crate::current_inherent_name(&power.internal_name))
                    }
                    Some(_) => false,
                })
                .cloned(),
        )
        .collect()
}

/// How many slots the fork has auto-granted a named inherent by `level` — the count that
/// lands in [`SelectedPower::inherent_slot_count`] and so keeps those slots off the user's
/// budget.
///
/// Shared with the `.skif` reader rather than inlined, because [`select`] separates the
/// user's slots from the granted ones by SUBTRACTING this count. A reader that restored a
/// build without it would hand every granted slot back as a user-placed one, and the build
/// would gain a slot every time it loaded.
pub fn auto_granted_slot_count(
    auto_granted_slot_levels: &std::collections::BTreeMap<String, Vec<u8>>,
    internal_name: &str,
    level: u8,
) -> u8 {
    auto_granted_slot_levels
        .get(internal_name)
        .map_or(0, |levels| {
            levels.iter().filter(|&&at| level >= at).count() as u8
        })
}

/// One granted power as a build selection, reusing the user's slotting when there is one.
///
/// Slot shape: a power the game gives no slots to gets none; every other gets its free base
/// slot, plus however many the user placed, plus the fork's auto-granted ones. `level` is
/// left at 0 — the game grants these, so there is no pick level to record, and
/// `picked_powers` excludes them from the ladder.
///
/// The three parts have to be counted separately, not measured off the slot vector's length:
/// a user-placed slot is build state whether or not an enhancement sits in it (it is already
/// spending budget), so a re-grant that rebuilt the shape from "base + auto" alone would give
/// those slots back every time the build loaded.
fn select(grant: &InherentGrant, auto_slots: u8, carried: Option<&SelectedPower>) -> SelectedPower {
    let mut power = SelectedPower::picked(&grant.internal_name, INHERENT_SET, 0);
    power.is_locked = true;
    power.inherent_category = Some(grant.category);
    power.inherent_slot_count = auto_slots;
    power.slots = if grant.max_slots == 0 {
        Vec::new()
    } else {
        vec![None; 1 + usize::from(auto_slots)]
    };

    let Some(carried) = carried else { return power };
    power.is_active = carried.is_active;
    power.active_sub_power = carried.active_sub_power.clone();
    power.targets_hit = carried.targets_hit;
    if grant.max_slots == 0 {
        return power;
    }

    // What the user placed, read off the carried selection the same way the budget count
    // does: everything that is neither the free base slot nor an auto-granted one.
    let user_placed = carried
        .slots
        .len()
        .saturating_sub(1 + usize::from(carried.inherent_slot_count));
    let want = 1 + user_placed + usize::from(auto_slots);

    let mut slots = carried.slots.clone();
    slots.resize(want.max(filled_extent(&slots)), None);
    power.slots = slots;
    power
}

/// One past the last slot holding an enhancement — the length a shrink may not cut below,
/// so a fork granting FEWER slots at a lower level can never silently discard a piece the
/// user placed in one of them.
fn filled_extent(slots: &[Option<crate::Enhancement>]) -> usize {
    slots
        .iter()
        .rposition(|slot| slot.is_some())
        .map_or(0, |index| index + 1)
}
