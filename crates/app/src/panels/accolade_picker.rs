//! The accolade picker — the beta `AccoladesModal`'s structure: a flat list of
//! independent on/off toggles with a hero/villain faction label, no grouping and
//! no mutual exclusion (the real `activateRequires` gates don't 1:1-pair —
//! DATA-GAP ACCOLADE-1).
//!
//! Everything rendered here is read from [`coh_data::accolades`] — the derivation
//! over the accolade-category powerset the contract ships, never a hand list. A
//! toggle writes [`coh_data::character::CharacterState::accolades`] through
//! [`BuildSession::commit`], so each flip is one undo step and the totals' gather
//! folds the accolade's own atoms on the next recalculate with no further wiring.
//!
//! A row also carries any mode the accolade's buff is conditional on (`modesRequired`,
//! DATA-GAP ACCOLADE-2). The gather folds a gated accolade like any other, so the warning
//! is the only thing distinguishing a buff you always have from one you have in one zone.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::AccoladeFaction;
use dioxus::prelude::*;

/// The picker's open state, provided at the shell root and hosted there, outside
/// the free grid's `transform`ed surfaces, for the same containment reason as
/// [`IncarnatePickerOpen`](crate::panels::incarnate_picker::IncarnatePickerOpen).
#[derive(Clone, Copy)]
pub struct AccoladePickerOpen(pub Signal<bool>);

/// The one accolade picker for the whole app. Renders nothing while closed.
#[component]
pub fn AccoladePickerHost(database: Db) -> Element {
    let mut open = use_context::<AccoladePickerOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        AccoladePickerModal { database, on_close: move |_| open.set(false) }
    }
}

#[component]
fn AccoladePickerModal(database: Db, on_close: EventHandler<()>) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    let toggles = database.accolade_toggles();
    if toggles.is_empty() {
        return rsx! {
            Modal { title: "Accolades".to_string(), size: ModalSize::Sm, on_close,
                p { class: "hint", "This dataset carries no accolade data." }
            }
        };
    }

    let enabled_count = build
        .read()
        .accolades
        .iter()
        .filter(|id| toggles.iter().any(|t| t.id.eq_ignore_ascii_case(id)))
        .count();

    rsx! {
        Modal { title: "Accolades".to_string(), size: ModalSize::Sm, on_close,
            div { class: "accolade-picker",
                p { class: "accolade-picker__lede",
                    "Accolades are permanent passive powers that grant stat bonuses. "
                    span { class: "accolade-picker__count", "{enabled_count} enabled" }
                }
                div { class: "accolade-picker__list",
                    for toggle in toggles.iter() {
                        {
                            let id = toggle.id.clone();
                            let enabled = build
                                .read()
                                .accolades
                                .iter()
                                .any(|a| a.eq_ignore_ascii_case(&id));
                            let name = toggle.power.name.clone();
                            let help = toggle
                                .power
                                .extra
                                .get("shortHelp")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let faction = toggle.faction;
                            // A mode requirement means the buff is real but conditional: the
                            // gather folds it into the totals either way, so the row says what
                            // the number can't. Read off the def, so no accolade is named here.
                            let requires = toggle.requires_modes.join(", ");
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: if enabled {
                                        "accolade-row is-enabled"
                                    } else {
                                        "accolade-row"
                                    },
                                    onclick: move |_| {
                                        let id = id.clone();
                                        session.commit(move |state| {
                                            match state
                                                .accolades
                                                .iter()
                                                .position(|a| a.eq_ignore_ascii_case(&id))
                                            {
                                                Some(at) => {
                                                    state.accolades.remove(at);
                                                }
                                                None => state.accolades.push(id.clone()),
                                            }
                                        });
                                    },
                                    span {
                                        class: "accolade-row__check",
                                        if enabled { "✓" }
                                    }
                                    span { class: "accolade-row__body",
                                        span { class: "accolade-row__name",
                                            "{name}"
                                            if faction != AccoladeFaction::Any {
                                                span { class: "accolade-row__faction",
                                                    "{faction.label()}"
                                                }
                                            }
                                        }
                                        if !help.is_empty() {
                                            span { class: "accolade-row__help", "{help}" }
                                        }
                                        if !requires.is_empty() {
                                            span { class: "accolade-row__requires",
                                                "Requires {requires}. Counted in totals regardless."
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "accolade-picker__footer",
                    button {
                        class: "accolade-picker__clear",
                        disabled: enabled_count == 0,
                        onclick: move |_| {
                            session.commit(|state| state.accolades.clear());
                        },
                        "Clear all"
                    }
                    button {
                        class: "accolade-picker__done",
                        onclick: move |_| on_close.call(()),
                        "Done"
                    }
                }
            }
        }
    }
}

/// The Available-panel strip that opens the picker: one button carrying the enabled count, in
/// the shared `picker-strip` chrome the pool triggers further down the rail sit in.
///
/// Second from the top, under the incarnate sockets: both are loadouts the build carries rather
/// than powers it picks, and neither is browsed the way the lists below them are.
#[component]
pub fn AccoladeStrip(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut picker = use_context::<AccoladePickerOpen>().0;

    let toggles = database.accolade_toggles();
    if toggles.is_empty() {
        return rsx! {};
    }

    let enabled_count = build
        .read()
        .accolades
        .iter()
        .filter(|id| toggles.iter().any(|t| t.id.eq_ignore_ascii_case(id)))
        .count();
    let summary = if enabled_count == 0 {
        "none".to_string()
    } else {
        format!("{enabled_count} of {}", toggles.len())
    };

    rsx! {
        div { class: "picker-strip",
            span { class: "picker-strip__title", "Accolades" }
            button {
                class: "picker-strip__open",
                title: "Toggle the build's earned accolades",
                onclick: move |_| picker.set(true),
                span {
                    class: if enabled_count == 0 {
                        "picker-strip__power is-empty"
                    } else {
                        "picker-strip__power"
                    },
                    "{summary}"
                }
            }
        }
    }
}
