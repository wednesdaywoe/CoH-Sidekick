//! Grants — the powers the game hands a build instead of asking it to pick them.
//!
//! The whole rule is `character_GrantAutoIssuePowers` (`Common/entity/character_base.c:1952`):
//! for every powerset the character owns, a power is handed over when
//! `piAvailable[j] <= iLevel && bAutoIssue && !OwnsPower && IsAllowedToHavePower`. Every term
//! is read from the export — [`Power::is_auto_issued`] is the marker,
//! [`Power::unlock_level`] the level, the power's own `requires` the `IsAllowedToHavePower`
//! eval, and the bucket walk below is "owns the powerset". Two surfaces consume it:
//!
//! * A parent handing out [`MIN_STANCE_OPTIONS`] or more mutually exclusive sub-powers is a
//!   stance selector ([`crate::caster_state`]): the build records which form is live and the
//!   totals expand that one.
//! * Everything else is a lone grant: the build simply HAS the power while its gate holds
//!   (Thunderspy's Cruelty rides a Pain Domination pick; the travel pools hand out
//!   Afterburner, Jaunt, Double Jump). [`sync_granted_powers`] reconciles those into the
//!   owning bucket as locked selections, so the power's own `powerType` decides what it
//!   contributes — an Auto full-time, a Toggle when switched on, a Click nothing.
//!
//! Storing a grant inside the owning bucket's `powers` Vec is what keeps every other
//! surface unchanged: def resolution, `requires` ownership paths, the gather pass, the
//! slot budget and the panel grouping all already walk those Vecs. A grant is
//! distinguishable from a pick by [`SelectedPower::is_locked`] and spends no pick level
//! ([`CharacterState::picked_powers`] skips locked selections).

use crate::pick_rules::requires_met;
use crate::{CharacterState, Power, PowerDatabase, SelectedPower, SetPaths};
use std::collections::BTreeSet;

/// A selector needs alternatives. A parent that hands out ONE sub-power is granting an
/// extra power, not offering a choice between forms — [`sync_granted_powers`] materializes
/// those as locked selections honouring their own `powerType`, while a parent at or above
/// this threshold becomes a stance selector instead ([`crate::caster_state::stance_groups`]).
pub const MIN_STANCE_OPTIONS: usize = 2;

/// The `requires` operator joining alternative prerequisite paths. A grant gate may name
/// more than one enabling power (`<path> <path> ||`); anything else in the expression means
/// the gate is doing something other than "the caster holds that power".
const ALTERNATION: &str = "||";

/// Does `parent_ident` hand `sub` out?
///
/// The grant gate is a `requires` naming the enabling power by its dotted path, optionally
/// alternating between several (`<path> <path> ||`). Only that shape counts: an expression
/// carrying anything else — a negation, an attribute read, a count — is gating on something
/// other than holding the parent, and reading it as a grant would attach sub-powers to a
/// parent that never hands them out.
pub fn granted_by(sub: &Power, parent_ident: &str) -> bool {
    let Some(gate) = requires(sub) else {
        return false;
    };
    let mut names_parent = false;
    for token in gate.iter().map(|t| &**t) {
        if token == ALTERNATION {
            continue;
        }
        if !token.contains('.') {
            return false;
        }
        names_parent |= token.rsplit('.').next() == Some(parent_ident);
    }
    names_parent
}

/// The power's `requires`, absent when the wire omits it or leaves it blank.
pub fn requires(power: &Power) -> Option<Vec<Box<str>>> {
    power
        .extra
        .get("requires")?
        .as_array()?
        .iter()
        .map(|t| t.as_str().map(Box::from))
        .collect::<Option<Vec<Box<str>>>>()
        .filter(|gate| !gate.is_empty())
}

/// Reconcile every bucket's lone grants against the picks the build currently holds.
///
/// Per held bucket (primary, secondary, each pool, the epic pool): a grant is desired when
/// its scope's def carries the grant marker, the build has reached its level, no stance
/// selector owns it, and its own `requires` evaluates true against the build. Locked entries
/// whose gate no longer holds are dropped (their slotting with them); missing desired ones
/// are pushed with the slot shape the def itself states; entries already present are left
/// untouched, so a resync never disturbs the user's toggle state or slotting.
///
/// Rounds run until the grant set stops moving, because a grant's gate may name another
/// grant: Homecoming's Quantum Acceleration is gated on Energy Flight, which the game issues
/// to Peacebringers rather than selling, so the enabling grant has to materialize before the
/// dependent one's gate can read true. [`grant_candidates`] bounds the rounds — a chain
/// cannot be longer than the marked powers it runs through — and exhausting that budget is
/// reported as a fault rather than left as a silent partial reconcile (Rule 1).
///
/// Returns gate-evaluation faults (`"<set_id>/<ident>: <error>"`) for the caller to log; a
/// grant whose gate cannot be read is NOT materialized — the picker's gate row is the
/// visible half of that failure (Rule 1).
pub fn sync_granted_powers(state: &mut CharacterState, db: &PowerDatabase) -> Vec<String> {
    let budget = grant_candidates(state, db);
    for _ in 0..=budget {
        let before = materialized_idents(state);
        let faults = sync_round(state, db);
        if materialized_idents(state) == before {
            return faults;
        }
    }
    let mut faults = sync_round(state, db);
    faults.push(format!(
        "granted powers: still changing after {budget} rounds — a grant chain longer than the \
         marked powers it runs through, or a gate that flips its own precondition"
    ));
    faults
}

/// The locked selections a round produced, as a comparable set — the reconcile's whole
/// output, and so the fixpoint's own signal.
fn materialized_idents(state: &CharacterState) -> BTreeSet<String> {
    state
        .all_selected()
        .filter(|selection| selection.is_locked)
        .map(|selection| selection.internal_name.clone())
        .collect()
}

/// How many powers in the build's own sets carry the grant marker — the ceiling on the
/// length of a grant-enables-grant chain, and so on the rounds a reconcile can need.
fn grant_candidates(state: &CharacterState, db: &PowerDatabase) -> usize {
    let from_powersets = [&state.primary.id, &state.secondary.id]
        .into_iter()
        .flatten()
        .filter_map(|set_id| db.find_powerset(set_id))
        .flat_map(|set| set.powers.iter())
        .filter(|power| power.is_auto_issued())
        .count();
    let partition_ids: Vec<&str> = state
        .pools
        .iter()
        .map(|pool| pool.id.as_str())
        .chain(state.epic_pool.iter().map(|epic| epic.id.as_str()))
        .collect();
    let from_partitions = db
        .pool_powers
        .iter()
        .chain(db.epic_powers.iter())
        .filter(|entry| partition_ids.contains(&entry.set_id.as_str()))
        .filter(|entry| entry.power.is_auto_issued())
        .count();
    let from_inherents = inherent_scope(db)
        .iter()
        .filter(|power| power.is_auto_issued())
        .count();
    from_powersets + from_partitions + from_inherents
}

/// The inherent-set powers the reconcile manages: the PICK-GATED members of the one
/// powerset every character owns without holding a bucket for it (`Inherent.Inherent`
/// is also how the archetype inherents get issued — `character_GrantAutoIssuePowers`
/// walks it like any other owned set). Thunderspy authors the twenty Kheldian form
/// attacks and the Bane/Widow Placates there, gated on the enabling FORM pick.
///
/// Pick-gated — the gate names a power outside the inherent set itself — is also the
/// ownership boundary against the identity sync ([`crate::granted_inherents`]): a member
/// gated on identity alone (`$archetype … ==`, the archetype inherents) is the identity
/// route's, and a member whose only dotted references stay inside the set (the
/// account-product idiom, and the sub-powers a stance parent hands out) is never
/// materialized here — the first is account state, the second the parent's options.
fn inherent_scope(db: &PowerDatabase) -> Vec<&Power> {
    let declared = declared_inherent_names(db);
    db.find_powerset(crate::INHERENT_SET)
        .map(|set| {
            set.powers
                .iter()
                .filter(|power| pick_gated(power) || archetype_gated(power, &declared))
                .collect()
        })
        .unwrap_or_default()
}

/// The display names this dataset's archetypes DECLARE as their own inherent. Those members
/// of [`crate::INHERENT_SET`] are handed over by [`crate::inherent_grants`] against this
/// same list, so the reconcile has to leave them alone.
///
/// Read from the archetype catalog, not spelled here. A catalog that
/// fails to decode yields an empty set, which narrows [`archetype_gated`] to nothing and so
/// preserves the pre-AUTOISSUE-2 scope rather than widening it on a bad read.
///
/// Public because it is half of the reconcile's scope rule, and a guard that wants to
/// address the OTHER half — the headline inherents this leaves alone — needs the same set
/// rather than a re-derivation that could drift from it.
pub fn declared_inherent_names(db: &PowerDatabase) -> BTreeSet<String> {
    db.archetypes()
        .map(|archetypes| {
            archetypes
                .all()
                .iter()
                .map(|archetype| archetype.inherent.name.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// Does this power's gate read the build's ARCHETYPE rather than anything it had to pick
/// (DATA-GAP AUTOISSUE-2)?
///
/// A fork can move an archetype's signature power out of its powerset into
/// [`crate::INHERENT_SET`], auto-issue it, and gate it `$archetype @Class_<AT> ==`.
/// Thunderspy does that with the Stalker's Hide and Placate, the Mastermind's Hold Ground
/// and eight Kheldian travel powers — and then reuses the vacated powerset name slots for
/// OTHER powers, so those powers are reachable from nowhere else. An archetype gate names
/// no power token, so [`pick_gated`] reads false on every one of them and they were never
/// in scope for the reconcile to grant.
///
/// `declared` is what keeps this from granting anything twice: an archetype's own headline
/// inherent (Vigilance, Containment, Domination …) carries this very gate shape, and
/// [`crate::inherent_grants`] already hands those over.
pub fn archetype_gated(power: &Power, declared: &BTreeSet<String>) -> bool {
    !declared.contains(&power.name)
        && requires(power).is_some_and(|gate| gate.iter().any(|token| token.starts_with("@Class_")))
}

/// Does this power's gate read the build's own picks — name a power outside
/// [`crate::INHERENT_SET`]? A dotted token's set is its second-to-last segment (a
/// two-segment path names its first — the set-qualified and category-qualified forms
/// the gates use); reader sigils (`$…`, `@…`) are values, not paths.
pub fn pick_gated(power: &Power) -> bool {
    requires(power).is_some_and(|gate| {
        gate.iter()
            .map(|t| &**t)
            .filter(|token| {
                token.contains('.') && !token.starts_with('@') && !token.starts_with('$')
            })
            .any(|token| {
                let segments: Vec<&str> = token.split('.').collect();
                let set_segment = match segments.len() {
                    0 | 1 => return false,
                    2 => segments[0],
                    n => segments[n - 2],
                };
                !set_segment.eq_ignore_ascii_case(crate::INHERENT_SET)
            })
    })
}

fn sync_round(state: &mut CharacterState, db: &PowerDatabase) -> Vec<String> {
    let mut faults = Vec::new();

    if let Some(set_id) = state.primary.id.clone() {
        let desired = desired_in_powerset(state, db, &set_id, &mut faults);
        apply(&mut state.primary.powers, &set_id, desired);
    }
    if let Some(set_id) = state.secondary.id.clone() {
        let desired = desired_in_powerset(state, db, &set_id, &mut faults);
        apply(&mut state.secondary.powers, &set_id, desired);
    }
    for index in 0..state.pools.len() {
        let set_id = state.pools[index].id.clone();
        let scope: Vec<&Power> = partition_scope(&db.pool_powers, &set_id);
        let desired = desired_grants(&scope, &set_id, state, &db.set_paths, &mut faults);
        apply(&mut state.pools[index].powers, &set_id, desired);
    }
    if let Some(set_id) = state.epic_pool.as_ref().map(|epic| epic.id.clone()) {
        let scope: Vec<&Power> = partition_scope(&db.epic_powers, &set_id);
        let desired = desired_grants(&scope, &set_id, state, &db.set_paths, &mut faults);
        if let Some(epic) = state.epic_pool.as_mut() {
            apply(&mut epic.powers, &set_id, desired);
        }
    }

    {
        let scope = inherent_scope(db);
        let desired = desired_grants(
            &scope,
            crate::INHERENT_SET,
            state,
            &db.set_paths,
            &mut faults,
        );
        apply_to_inherents(&mut state.inherents, desired);
    }

    faults
}

/// A desired grant: the identity to store plus the slot shape its def states.
type Grant = (String, Vec<Option<crate::Enhancement>>);

fn desired_in_powerset(
    state: &CharacterState,
    db: &PowerDatabase,
    set_id: &str,
    faults: &mut Vec<String>,
) -> Vec<Grant> {
    let scope: Vec<&Power> = db
        .find_powerset(set_id)
        .map(|set| set.powers.iter().collect())
        .unwrap_or_default();
    desired_grants(&scope, set_id, state, &db.set_paths, faults)
}

fn partition_scope<'a>(
    partition: &'a [crate::database::PartitionPower],
    set_id: &str,
) -> Vec<&'a Power> {
    partition
        .iter()
        .filter(|entry| entry.set_id == set_id)
        .map(|entry| &entry.power)
        .collect()
}

fn desired_grants(
    scope: &[&Power],
    set_id: &str,
    state: &CharacterState,
    sets: &SetPaths,
    faults: &mut Vec<String>,
) -> Vec<Grant> {
    let stance_owned = stance_owned(scope);
    let archetype = state.archetype.id.as_deref();
    scope
        .iter()
        .filter(|power| power.is_auto_issued())
        // `piAvailable[j] <= iLevel`, the grant rule's own level term. Most marked powers
        // state no level requirement, but the Kheldian form attacks do not: Nova's arrive at
        // 4 and Dwarf's at 20, so without this a level-1 build holding nothing would be
        // handed the whole form roster.
        .filter(|power| power.unlock_level() <= state.level)
        .filter(|power| !stance_owned.contains(power.ident()))
        .filter_map(|power| {
            let gate = requires(power).unwrap_or_default();
            match requires_met(&gate, state, archetype, sets) {
                Ok(true) => Some((power.ident().to_string(), grant_slots(power))),
                Ok(false) => None,
                Err(error) => {
                    faults.push(format!("{set_id}/{}: {error}", power.ident()));
                    None
                }
            }
        })
        .collect()
}

/// The idents a stance selector owns in `scope`: every granted power some parent hands
/// out alongside [`MIN_STANCE_OPTIONS`]-or-more siblings. Those are never materialized —
/// the parent pick's `active_sub_power` is their surface, and materializing them too would
/// double-count the live form.
///
/// Ownership, not availability: a selector's options are the parent's whatever level the
/// build has reached, so the level term that gates materialization has no place here.
fn stance_owned<'a>(scope: &[&'a Power]) -> BTreeSet<&'a str> {
    let mut owned = BTreeSet::new();
    for parent in scope {
        let handed: Vec<&str> = scope
            .iter()
            .filter(|sub| sub.is_auto_issued() && granted_by(sub, parent.ident()))
            .map(|sub| sub.ident())
            .collect();
        if handed.len() >= MIN_STANCE_OPTIONS {
            owned.extend(handed);
        }
    }
    owned
}

/// The slot shape the def states: none when the export says the power takes no slots
/// (`maxSlots` 0, or an explicit empty `allowedEnhancements`), else the free base slot.
/// 32 of the corpus's 34 lone grants take none; Thunderspy's Hunter's Howl and Pounce say
/// `maxSlots` 6 and slot like any pick.
fn grant_slots(power: &Power) -> Vec<Option<crate::Enhancement>> {
    if power.max_slots() == Some(0) || !power.accepts_enhancements() {
        Vec::new()
    } else {
        vec![None]
    }
}

fn apply(powers: &mut Vec<SelectedPower>, set_id: &str, desired: Vec<Grant>) {
    powers.retain(|power| {
        !power.is_locked
            || desired
                .iter()
                .any(|(ident, _)| ident == &power.internal_name)
    });
    for (ident, slots) in desired {
        // An entry already under this name is CLAIMED, not skipped. A `.skif` read stores
        // neither `is_locked` nor `inherent_category` — the reader drops both on purpose and
        // says this reconcile re-derives them (`skif.rs`, rule 6) — so the entry it hands over
        // is a grant wearing a pick's clothes. Skipping it left it that way permanently: the
        // grant lost its granted mark, gained a remove button, and SPENT A POWER PICK
        // ([`CharacterState::picked_powers`] counts every unlocked selection). Every pool that
        // hands something out was affected on all four forks — Afterburner, Jaunt, Double Jump,
        // Translocation, Stomp, Turbo Boost, Athletics (GRANTLOCK-1).
        //
        // Claiming stamps only what the reader dropped. The slot shape and the toggle are the
        // user's and are left alone, exactly as they are on the resync of a grant that never
        // left the build (`user_toggle_and_slots_survive_a_resync`) — a read is that same
        // resync, reached through a file.
        if let Some(held) = powers.iter_mut().find(|power| power.internal_name == ident) {
            held.is_locked = true;
            continue;
        }
        let mut grant = SelectedPower::picked(ident, set_id, 0);
        grant.is_locked = true;
        grant.slots = slots;
        powers.push(grant);
    }
}

/// [`apply`] for the shared inherents list. The identity sync's entries (fitness, basic,
/// prestige, the archetype inherent) live here too, so the lapse-drop is scoped to the
/// entries this reconcile stamped — [`crate::InherentCategory::Granted`] is the ownership
/// marker — and new grants are stamped with it.
///
/// An UNSTAMPED entry is this pass's too, and is the reason [`crate::granted_inherents`]
/// carries one through rather than dropping it: a category of `None` is what a `.skif` read
/// leaves behind, so the entry is one nothing has claimed YET. This pass is the claim. After
/// it, every entry in the list carries a category — the unstamped state exists only between
/// the read and here, which is why an unstamped entry no grant names is dropped on the same
/// terms as a lapsed one rather than left in the list contributing to totals from a section
/// that renders none of them.
fn apply_to_inherents(powers: &mut Vec<SelectedPower>, desired: Vec<Grant>) {
    powers.retain(|power| {
        !matches!(
            power.inherent_category,
            Some(crate::InherentCategory::Granted) | None
        ) || desired
            .iter()
            .any(|(ident, _)| ident == &power.internal_name)
    });
    for (ident, slots) in desired {
        // Claimed, not skipped — see [`apply`]. Dropping the category cost more here than a
        // powerset grant's lost lock, because the identity sync runs FIRST on the load path
        // (`build_io::adopt_as`) and used to drop an unstamped entry outright: the grant was
        // then re-created from the def, so its toggle came back OFF and its ENHANCEMENTS were
        // gone. Eleven powers across four Thunderspy archetypes, Hide among them (GRANTLOCK-1).
        if let Some(held) = powers.iter_mut().find(|power| power.internal_name == ident) {
            held.is_locked = true;
            held.inherent_category = Some(crate::InherentCategory::Granted);
            continue;
        }
        let mut grant = SelectedPower::picked(ident, crate::INHERENT_SET, 0);
        grant.is_locked = true;
        grant.slots = slots;
        grant.inherent_category = Some(crate::InherentCategory::Granted);
        powers.push(grant);
    }
}
