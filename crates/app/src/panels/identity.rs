//! Build identity — the build's spine: name, archetype, and primary/secondary powerset
//! selection, plus the archetype's inherent card. This is *who the character is*; picking
//! and slotting individual powers lives on its own surface (`panels/powers.rs`), mirroring
//! the beta's split of build identity from the power grid. Every edit goes through
//! [`BuildSession::commit`] so it is undoable and persisted (see `build_session.rs`).
//!
//! Rendered in the header's Build Identity popover (`shell::IdentityPopover`), not on the
//! grid — it is a short form the user dips into and back out of, and a whole always-visible
//! surface was grid width the loadout needed more (the beta `BuildIdentityPopover`).

use crate::build_session::BuildSession;
use crate::panels::powers::BuildRole;
use crate::panels::powerset_select::PowersetSelect;
use crate::shell::Db;
use coh_data::{ArchetypeSelection, DatasetId, PowersetSelection};
use dioxus::prelude::*;

/// `(stored value, display label)`. The stored value is `CharacterState::origin`'s own
/// vocabulary — lowercase, matching `icons::origin_frame_prefix` — and the display label is
/// the only place a capital appears.
const ORIGINS: [(&str, &str); 5] = [
    ("magic", "Magic"),
    ("mutation", "Mutation"),
    ("natural", "Natural"),
    ("science", "Science"),
    ("technology", "Technology"),
];

#[component]
pub fn IdentityPanel(database: Db, dataset: Signal<DatasetId>) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    // The archetype catalog is UI-only and parsed on demand, so memoize the one parse per
    // dataset. `Err` surfaces as a visible node rather than a panic (Rule 1's UI edge).
    let catalog = use_memo(use_reactive!(|database| database.archetypes()));

    let catalog_read = catalog.read();
    let Ok(archetypes) = &*catalog_read else {
        let err = catalog_read.as_ref().err().cloned().unwrap_or_default();
        return rsx! {
            div { class: "build",
                div { class: "load-state error", "Archetype data failed to load: {err}" }
            }
        };
    };

    let current_archetype = build.read().archetype.id.clone().unwrap_or_default();
    let has_archetype = !current_archetype.is_empty();

    rsx! {
        div { class: "build",
            // "Which game is this a build of" leads the form: the fork decides which
            // definitions every surface below reads, and a build carried to another fork
            // is a different build — so the switch belongs beside the archetype rather
            // than with the reader's set-and-forget preferences (HM2: moved out of the
            // main menu's former Options group).
            label { class: "build-field",
                span { class: "field-label", "Dataset" }
                DatasetSwitcher { dataset }
            }

            // The build name is metadata, not the task — a quiet title that recedes so
            // the archetype control (the decision that gates everything) leads.
            input {
                class: "build-name",
                r#type: "text",
                "aria-label": "Build name",
                placeholder: "Name this build",
                value: "{build.read().name}",
                oninput: move |evt| session.commit(|state| state.name = evt.value()),
            }

            // Origin — a build fact like Dataset and the name above, not a preference: it
            // decides which DO/SO overlay frame an origin enhancement wears
            // (`icons::origin_overlay_url`) and nothing else, so it sits with the other
            // identity facts rather than in the app's set-and-forget preferences.
            label { class: "build-field",
                span { class: "field-label", "Origin" }
                select {
                    class: "build-select",
                    value: build.read().origin.clone().unwrap_or_default(),
                    onchange: move |evt: Event<FormData>| {
                        let chosen = evt.value();
                        session.commit(move |state| {
                            state.origin = (!chosen.is_empty()).then(|| chosen.clone());
                        });
                    },
                    option { value: "", "— choose origin —" }
                    for (value, label) in ORIGINS {
                        option { value: "{value}", "{label}" }
                    }
                }
            }

            // Archetype is the focal decision: it gates the sets, pools, and inherent
            // below it. The `focal` class arc-colors its label (arc = "live" in the weld
            // theme) and enlarges the control, so the eye lands here first.
            label { class: "build-field focal",
                span { class: "field-label", "Archetype" }
                select {
                    class: "build-select hero",
                    onchange: {
                        let archetypes = archetypes.all().to_vec();
                        let database = database.clone();
                        move |evt: Event<FormData>| {
                            let id = evt.value();
                            let name = archetypes
                                .iter()
                                .find(|a| a.id == id)
                                .map(|a| a.name.clone())
                                .unwrap_or_default();
                            let database = database.clone();
                            session.commit(move |state| {
                                set_archetype(state, id, name);
                                // The new archetype's inherents and grants land in the same
                                // edit as the archetype itself, so one user action is one
                                // undo step.
                                crate::inherents::sync(state, &database);
                                crate::granted_powers::sync(state, &database);
                            });
                        }
                    },
                    // Selection is expressed per-option (`selected`), not via the select's
                    // `value`: primary/secondary options are derived from the archetype, and a
                    // `value` set before those options exist (e.g. on a restore render) does not
                    // stick. Per-option `selected` is applied with the option, so it always does.
                    option { value: "", selected: current_archetype.is_empty(), "— choose archetype —" }
                    for at in archetypes.all() {
                        option { value: "{at.id}", selected: at.id == current_archetype, "{at.name}" }
                    }
                }
            }

            // Primary + secondary are dependent peers to each other and subordinate to
            // the archetype above — a tight pair, one visual tier down.
            //
            // The control is the Available rail's own (LAY4): one roster, one gate, one commit,
            // two frames. This frame is the labelled form field; the rail's is a column head.
            div { class: "build-pair",
                label { class: "build-field",
                    span { class: "field-label", "Primary" }
                    PowersetSelect {
                        database: database.clone(),
                        role: BuildRole::Primary,
                        class: "build-select".to_string(),
                        placeholder: "— choose primary —".to_string(),
                    }
                }

                label { class: "build-field",
                    span { class: "field-label", "Secondary" }
                    PowersetSelect {
                        database: database.clone(),
                        role: BuildRole::Secondary,
                        class: "build-select".to_string(),
                        placeholder: "— choose secondary —".to_string(),
                    }
                }
            }

            if has_archetype {
                if let Some(at) = archetypes.get(&current_archetype) {
                    // Static info card — the capsule marker only, no interactive/selected state.
                    div { class: "power-card",
                        p { class: "power-card__title", "{at.inherent.name}" }
                        p { class: "power-card__body", "{at.inherent.description}" }
                    }
                }
            } else {
                p { class: "hint", "Choose an archetype to begin." }
            }
        }
    }
}

/// Which fork's data every surface is reading.
///
/// It decides which game this build is a build OF — a property of the character, not of the
/// reader — so it sits in the identity form beside the archetype rather than with the
/// set-and-forget preferences. Writes the shell's dataset signal: switching the fork is a
/// dataset switch before it is anything else, and the build that lands is the one saved
/// for that fork.
#[component]
fn DatasetSwitcher(mut dataset: Signal<DatasetId>) -> Element {
    rsx! {
        nav { class: "dataset-switcher",
            for id in DatasetId::ALL {
                button {
                    class: if dataset() == id { "seg active" } else { "seg" },
                    onclick: move |_| dataset.set(id),
                    "{id.display_name()}"
                }
            }
        }
    }
}

/// Set the archetype and clear every set-dependent selection — a different archetype offers
/// different primary/secondary sets and pools, so the old picks no longer apply.
fn set_archetype(state: &mut coh_data::CharacterState, id: String, name: String) {
    state.archetype = if id.is_empty() {
        ArchetypeSelection::default()
    } else {
        ArchetypeSelection { id: Some(id), name }
    };
    state.primary = PowersetSelection::default();
    state.secondary = PowersetSelection::default();
    state.pools.clear();
    state.epic_pool = None;
    state.inherents.clear();
}
