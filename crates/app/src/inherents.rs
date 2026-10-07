//! Keeping the build's granted inherents in step with the build's identity.
//!
//! Inherents are not picked, so nothing in the picking UI ever adds one: they follow from
//! the archetype, the dataset, and the level. But they cannot be a `use_memo` either — they
//! carry the user's enhancement slotting, which is build state — so they are reconciled into
//! the build at the two moments the facts behind them change: when the archetype is set, and
//! when a build is loaded (a dataset switch resolves the same identity against a different
//! dataset's grants).
//!
//! The reconcile is idempotent and slot-preserving; [`coh_data::granted_inherents`] owns
//! both properties, and this is the thin layer that resolves the dataset's half of its
//! arguments.

use coh_data::{CharacterState, InherentGrant, PowerDatabase};

/// Rebuild `state.inherents` from the dataset's grants and the build's archetype.
///
/// A build with no archetype gets no inherents — the universal grants are real, but showing
/// them beside an empty identity would put a slottable surface in front of a build that has
/// not started. A dataset that lists no grants at all leaves the list untouched rather than
/// clearing it, so a hand-built or partial database cannot silently wipe a real build's
/// slotting.
pub fn sync(state: &mut CharacterState, database: &PowerDatabase) {
    if state.archetype.id.is_none() {
        state.inherents.clear();
        return;
    }
    let Ok(Some(grants)) = database.inherent_grants() else {
        return;
    };
    let archetype_inherent = archetype_inherent(state, database);
    let auto_granted = database
        .leveling_schedule
        .as_ref()
        .map(|schedule| schedule.auto_granted_slot_levels.clone())
        .unwrap_or_default();

    state.inherents = coh_data::granted_inherents(
        &grants,
        archetype_inherent.as_ref(),
        &auto_granted,
        state.level,
        &state.inherents,
    );
}

/// The archetype's own inherent as a grant, when the dataset ships a power for the one the
/// archetype declares. `None` covers both "no archetype" and an archetype naming an inherent
/// its dataset has no power for — the Inherents section marks that rather than substituting a
/// stand-in (Rule 1).
///
/// The corpus's one case is Thunderspy's Primalist, and it is the export's answer, not a
/// defect: the fork backs `Primal Energy` with a meter and a dampen only, and grants and
/// spends the resource from the archetype's own powersets (INHERENT-10).
pub fn archetype_inherent(
    state: &CharacterState,
    database: &PowerDatabase,
) -> Option<InherentGrant> {
    let name = declared_inherent_name(state, database)?;
    let power = database.archetype_inherent(&name)?;
    Some(InherentGrant {
        internal_name: power.ident().to_string(),
        name: power.name.clone(),
        category: coh_data::InherentCategory::Archetype,
        // Read, not assumed. The archetype inherents omit `maxSlots` entirely, so a bare
        // `unwrap_or(0)` would be inventing the answer; what the export actually states about
        // them is an empty `allowedEnhancements` — accepts no enhancement category, therefore
        // takes no slots. A dataset that ever did state a ceiling for one would be believed.
        max_slots: match power.max_slots() {
            Some(stated) => stated,
            None if power.accepts_enhancements() => coh_data::MAX_USER_SLOTS_PER_POWER as u8,
            None => 0,
        },
    })
}

/// The inherent NAME the build's archetype declares, from the dataset's archetype catalog.
pub fn declared_inherent_name(state: &CharacterState, database: &PowerDatabase) -> Option<String> {
    let archetype_id = state.archetype.id.as_deref()?;
    let catalog = database.archetypes().ok()?;
    Some(catalog.get(archetype_id)?.inherent.name.clone())
}
