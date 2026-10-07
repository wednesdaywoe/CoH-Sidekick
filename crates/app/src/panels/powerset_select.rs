//! The primary/secondary powerset choice — the control, its roster, and the one write path
//! both surfaces that offer it go through.
//!
//! Two surfaces offer the choice: the header's Build Identity popover, where it sits with the
//! archetype that gates it, and the Available rail's column heads, where the set being chosen
//! is the one being read (LAY4). They are two frames around one control, never two controls:
//! the roster, the set-level gate, the name resolution and the commit all live here, and a
//! caller supplies only which role it is asking for and what class the `<select>` wears.
//!
//! That is not tidiness. `identity::set_archetype` clears `primary` and `secondary` together
//! because a different archetype offers different sets, and a second control writing those
//! fields on its own path would keep offering the old archetype's roster after the switch —
//! the desync this module exists to make unrepresentable.

use crate::build_session::BuildSession;
use crate::panels::powers::BuildRole;
use crate::shell::Db;
use coh_data::PowersetSelection;
use dioxus::prelude::*;

/// One `<option>`-worth of powerset choice: its id (the `<select>` value), display name, and
/// the set-level refusal that makes it unchoosable, if there is one.
#[derive(Clone, PartialEq)]
pub struct SetChoice {
    pub id: String,
    pub name: String,
    /// The game's own `SetBuyRequiresFailedText`, or the fault text of a gate that would not
    /// read. `None` where the set is choosable.
    pub blocked: Option<String>,
}

/// The choosable sets from one of an archetype's rosters, with each one's set-level verdict.
///
/// No primary or secondary set on any of the three forks carries a set-level gate today, so
/// this changes nothing anyone can see — it is here because a set the game refuses must not be
/// silently offered (Rule 1). Graded by a synthetic gate, since the corpus holds no violating
/// case — see the tests below.
///
/// The sets that DO carry the gate are the VEAT branch pairs, and they are not chosen here:
/// nothing declares a branch, the game confers the set when a power is bought out of it, so
/// they are offered on the picking surface instead (`powers::branch_offers`).
///
/// The listed-vs-disabled split is the game's (`Game/src/UI/uiPowers.c:395`): a refused
/// specialization set is dropped from the list, and any other refusal is drawn disabled with
/// its message.
pub fn set_choices(
    database: &Db,
    set_ids: &[String],
    state: &coh_data::CharacterState,
) -> Vec<SetChoice> {
    let mut choices: Vec<SetChoice> = set_ids
        .iter()
        .filter_map(|set_id| {
            let Some(powerset) = database.find_powerset(set_id) else {
                // A roster naming a set the dataset does not ship is a fault worth seeing, not
                // a row to drop — the option renders disabled saying so.
                return Some(SetChoice {
                    id: set_id.clone(),
                    name: set_id.clone(),
                    blocked: Some("not in this dataset".to_string()),
                });
            };
            let verdict = coh_data::set_gate(
                &powerset.buy_requires,
                &powerset.buy_requires_failed,
                powerset.specialize_at,
                &powerset.specialize_requires,
                state,
                state.archetype.id.as_deref(),
                &database.set_paths,
            );
            let blocked = match verdict {
                Ok(coh_data::SetGate::Open) => None,
                Ok(coh_data::SetGate::Closed { reason }) => Some(reason),
                // Specialization sets: refused means not listed at all.
                Ok(coh_data::SetGate::BranchClosed | coh_data::SetGate::NotYet { .. }) => {
                    return None
                }
                Err(error) => Some(error.to_string()),
            };
            Some(SetChoice {
                id: set_id.clone(),
                name: powerset.name.clone(),
                blocked,
            })
        })
        .collect();
    // The roster comes in the data's order: internal names ("Devices" is Gadgets), with
    // later additions like Atomic Manipulation appended. The menu reads by display name.
    choices.sort_by(|left, right| left.name.cmp(&right.name));
    choices
}

/// One powerset `<option>`. A set the game refuses is still shown, disabled and carrying the
/// refusal — a select that silently omits a set says nothing about why, which is the whole
/// content of a gate.
#[component]
fn SetOption(choice: SetChoice, current: String) -> Element {
    let selected = choice.id == current;
    match &choice.blocked {
        None => rsx! {
            option { value: "{choice.id}", selected, "{choice.name}" }
        },
        // Still `selected` when it is what the build holds: a set can be refused AFTER it was
        // chosen, and dropping the selection would silently rewrite the build to explain a
        // rule to the user.
        Some(reason) => rsx! {
            option { value: "{choice.id}", selected, disabled: true, "{choice.name} — {reason}" }
        },
    }
}

/// Build a [`PowersetSelection`] for the chosen set id (empty id ⇒ cleared selection),
/// resolving its display name from the choices list or the dataset.
pub fn resolve_set(choices: &[SetChoice], database: &Db, set_id: &str) -> PowersetSelection {
    if set_id.is_empty() {
        return PowersetSelection::default();
    }
    let name = choices
        .iter()
        .find(|choice| choice.id == set_id)
        .map(|choice| choice.name.clone())
        .or_else(|| database.find_powerset(set_id).map(|ps| ps.name.clone()))
        .unwrap_or_else(|| set_id.to_string());
    PowersetSelection {
        id: Some(set_id.to_string()),
        name,
        powers: Vec::new(),
    }
}

/// The `<select>` itself — the whole of the powerset choice, wrapped by whatever frame the
/// calling surface wants around it.
///
/// It reads its own roster rather than taking one, which is what makes it the single write
/// path rather than a shared widget two callers feed: a caller that assembled the roster could
/// assemble a stale one, and the desync in this module's header is exactly that bug.
///
/// `class` is the frame's, because the two frames are genuinely different objects — a labelled
/// field in a form, and the head of a column in the rail — and nothing but the styling differs.
#[component]
pub fn PowersetSelect(
    database: Db,
    role: BuildRole,
    /// The `<select>`'s class, so a caller can frame the control without owning it.
    class: String,
    /// What an empty selection reads as. The rail's heads say it in fewer words than the
    /// popover's fields, because a column head is not a form label.
    placeholder: String,
) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    // The archetype catalog is UI-only and parsed on demand, so memoize the one parse per
    // dataset. An `Err` surfaces as a visible node rather than a panic (Rule 1's UI edge).
    let catalog = use_memo(use_reactive!(|database| database.archetypes()));

    // The roster, with each set's set-level verdict. Recomputed when the build or the dataset
    // changes — the verdict reads what the build holds, so it cannot be memoized on the
    // archetype alone.
    let roster_db = database.clone();
    let choices = use_memo(use_reactive!(|(roster_db, role)| {
        let state = build.read();
        let (Some(id), Ok(catalog)) = (state.archetype.id.clone(), &*catalog.read()) else {
            return Vec::<SetChoice>::new();
        };
        let Some(at) = catalog.get(&id) else {
            return Vec::new();
        };
        let roster = match role {
            BuildRole::Primary => &at.primary_sets,
            BuildRole::Secondary => &at.secondary_sets,
        };
        set_choices(&roster_db, roster, &state)
    }));

    if let Err(error) = &*catalog.read() {
        return rsx! {
            div { class: "load-state error", "Archetype data failed to load: {error}" }
        };
    }

    let current = match role {
        BuildRole::Primary => build.read().primary.id.clone(),
        BuildRole::Secondary => build.read().secondary.id.clone(),
    }
    .unwrap_or_default();
    let has_archetype = build.read().archetype.id.is_some();
    let choices = choices();
    // A `<select>` clips rather than ellipsises, and the rail's heads are ~220px against set
    // names like "Darkness Manipulation" — so the full name lives in the tooltip, which is the
    // only place a native select will carry it.
    let current_name = choices
        .iter()
        .find(|choice| choice.id == current)
        .map(|choice| choice.name.clone())
        .unwrap_or_else(|| placeholder.clone());

    rsx! {
        select {
            class,
            title: "{current_name}",
            disabled: !has_archetype,
            "aria-label": match role {
                BuildRole::Primary => "Primary powerset",
                BuildRole::Secondary => "Secondary powerset",
            },
            onchange: {
                let choices = choices.clone();
                let database = database.clone();
                move |evt: Event<FormData>| {
                    let selection = resolve_set(&choices, &database, &evt.value());
                    let database = database.clone();
                    session.commit(move |state| {
                        match role {
                            BuildRole::Primary => state.primary = selection,
                            BuildRole::Secondary => state.secondary = selection,
                        }
                        crate::granted_powers::sync(state, &database);
                    });
                }
            },
            // Selection is expressed per-option (`selected`), not via the select's `value`:
            // the options are derived from the archetype, and a `value` set before those
            // options exist (e.g. on a restore render) does not stick. Per-option `selected`
            // is applied with the option, so it always does.
            option { value: "", selected: current.is_empty(), "{placeholder}" }
            for choice in choices.iter().cloned() {
                SetOption { choice, current: current.clone() }
            }
        }
    }
}
