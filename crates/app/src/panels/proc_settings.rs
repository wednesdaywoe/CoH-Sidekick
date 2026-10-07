//! The proc-settings modal (the beta `ProcSettingsModal`): which proc effect categories
//! contribute to the build's totals.
//!
//! **The categories are derived, per dataset (Rule 0).** The beta wrote a nine-bucket table by
//! hand and mapped fourteen category names into it, `unknown ⇒ enabled` — against proc data
//! carrying twenty-one, so its own "Disable All" left Stealth, Absorb, MaxHP and the always-on
//! `+Damage` globals contributing, and its `BuildUp` key named a category the data never emits.
//! None of that is ported. [`coh_math::procs::proc_categories`] asks the router which categories
//! it spends and what each one feeds, so this component names no category and no stat.
//!
//! Rows group by the dashboard section their contributions land in, read through
//! [`stat_registry::stat_for_ledger_key`] — the same key→row lookup the buff-pet opt-in promises
//! against, so a switch and the panel it moves cannot disagree about where a proc lands.
//!
//! Each flip is one [`BuildSession::commit`]: the setting is build identity (it changes every
//! number the dashboard shows, so a shared build has to carry it), which also makes it one undo
//! step and persists it with everything else.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::panels::stat_registry::{self, StatSection};
use crate::shell::Db;
use coh_math::procs::ProcCategory;
use dioxus::prelude::*;

/// The modal's open state, held at the shell root above both layout roots — a `fixed` backdrop
/// is contained by the grid's `transform`ed surfaces (see [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct ProcSettingsOpen(pub Signal<bool>);

/// The one proc-settings modal for the whole app. Renders nothing while closed.
#[component]
pub fn ProcSettingsHost(database: Db) -> Element {
    let mut open = use_context::<ProcSettingsOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        ProcSettingsModal { database, on_close: move |_| open.set(false) }
    }
}

#[component]
fn ProcSettingsModal(database: Db, on_close: EventHandler<()>) -> Element {
    let session = use_context::<BuildSession>();
    let categories = coh_math::procs::proc_categories(&session.build.read(), &database.0);

    if categories.is_empty() {
        return rsx! {
            Modal { title: "Proc Settings".to_string(), size: ModalSize::Md, on_close,
                p { class: "hint", "This dataset carries no proc data." }
            }
        };
    }

    let (inert, gateable): (Vec<_>, Vec<_>) = categories
        .iter()
        .cloned()
        .partition(|c| c.inert_reason.is_some());
    let enabled = gateable.iter().filter(|c| c.enabled).count();
    let slotted = gateable.iter().filter(|c| c.slotted > 0).count();
    let all_on = enabled == gateable.len();

    // Section order is the dashboard's, so scanning this modal and scanning the grid walk the
    // stat vocabulary the same way. A category whose keys name no dashboard row falls to the
    // tail rather than being dropped — a switch that works has to be reachable even when
    // nothing on screen is the place it lands.
    let mut groups: Vec<(Option<StatSection>, Vec<ProcCategory>)> = StatSection::ALL
        .into_iter()
        .map(|section| (Some(section), Vec::new()))
        .collect();
    groups.push((None, Vec::new()));
    for category in gateable {
        let section = section_of(&category);
        let bucket = groups
            .iter_mut()
            .find(|(held, _)| *held == section)
            .expect("every section plus the tail is present");
        bucket.1.push(category);
    }
    groups.retain(|(_, members)| !members.is_empty());

    let set_all = move |on: bool, names: Vec<String>| {
        session.commit(move |state| {
            for name in &names {
                if on {
                    state.disabled_proc_categories.remove(name);
                } else {
                    state.disabled_proc_categories.insert(name.clone());
                }
            }
        });
    };
    let every_name: Vec<String> = groups
        .iter()
        .flat_map(|(_, members)| members.iter().map(|c| c.name.clone()))
        .collect();

    rsx! {
        Modal { title: "Proc Settings".to_string(), size: ModalSize::Md, on_close,
            div { class: "proc-settings",
                p { class: "proc-settings__lede",
                    "Procs and global IOs contribute to your totals by effect category. "
                    span { class: "proc-settings__count",
                        "{enabled} of {every_name.len()} contributing · {slotted} slotted in this build"
                    }
                }
                div { class: "proc-settings__actions",
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| set_all(!all_on, every_name.clone()),
                        if all_on { "Disable all" } else { "Enable all" }
                    }
                }

                for (section , members) in groups.iter() {
                    section { key: "{section_key(*section)}", class: "proc-settings__group",
                        h4 { class: "proc-settings__group-name", "{section_title(*section)}" }
                        for category in members.iter() {
                            CategoryRow { key: "{category.name}", category: category.clone() }
                        }
                    }
                }

                if !inert.is_empty() {
                    section { class: "proc-settings__group",
                        h4 { class: "proc-settings__group-name", "Not on your sheet" }
                        for category in inert.iter() {
                            InertRow { key: "{category.name}", category: category.clone() }
                        }
                    }
                }
            }
        }
    }
}

/// One category's switch, saying what it feeds and how much of it this build carries.
#[component]
fn CategoryRow(category: ProcCategory) -> Element {
    let session = use_context::<BuildSession>();
    let name = category.name.clone();

    rsx! {
        div {
            class: "proc-row",
            class: if category.slotted == 0 { "is-unslotted" },
            span { class: "proc-row__label",
                "{category.name}"
                span { class: "proc-row__note", "{feeds_text(&category)}" }
                span { class: "proc-row__examples", "{examples_text(&category)}" }
            }
            span {
                class: "proc-row__count mono",
                title: "Proc effects of this category among the build's slotted pieces",
                "{category.slotted}"
            }
            label { class: "switch",
                input {
                    r#type: "checkbox",
                    checked: category.enabled,
                    onchange: move |evt: Event<FormData>| {
                        let on = evt.checked();
                        let name = name.clone();
                        session
                            .commit(move |state| {
                                if on {
                                    state.disabled_proc_categories.remove(&name);
                                } else {
                                    state.disabled_proc_categories.insert(name.clone());
                                }
                            });
                    },
                }
                span { class: "switch-track" }
            }
        }
    }
}

/// A category the router cannot spend on the player's dashboard, shown with its reason and no
/// switch. Stated rather than hidden: it is the answer to "why does my slotted Impeded Swiftness
/// show up nowhere", and a switch that cannot move a number reads as a broken one.
#[component]
fn InertRow(category: ProcCategory) -> Element {
    let reason = category.inert_reason.clone().unwrap_or_default();
    rsx! {
        div { class: "proc-row is-inert",
            span { class: "proc-row__label",
                "{category.name}"
                span { class: "proc-row__note", "{reason}" }
            }
        }
    }
}

/// The dashboard section a category's contributions land in, or `None` when its keys name no
/// row on any panel.
///
/// The FIRST resolving key decides, because a category that reaches several rows reaches them
/// from one place — `Defense` feeds eleven positions across one panel, not eleven panels.
fn section_of(category: &ProcCategory) -> Option<StatSection> {
    category
        .breakdown_keys
        .iter()
        .find_map(|key| stat_registry::stat_for_ledger_key(key))
        .map(|stat| stat.section)
}

fn section_title(section: Option<StatSection>) -> &'static str {
    section.map_or("Elsewhere", StatSection::title)
}

fn section_key(section: Option<StatSection>) -> &'static str {
    section.map_or("elsewhere", |s| s.slug())
}

/// "feeds Melee Def, Ranged Def" — the rows this category moves, in the dashboard's own labels.
///
/// Deduped for the reason the buff-pet opt-in dedupes: several accumulator keys land on one row
/// (the dashboard pairs S/L, F/C and E/N), and naming a row twice reads as two separate gains.
fn feeds_text(category: &ProcCategory) -> String {
    let named = category
        .breakdown_keys
        .iter()
        .filter_map(|key| stat_registry::label_for_ledger_key(key))
        .fold(Vec::new(), |mut named, label| {
            if !named.contains(&label) {
                named.push(label);
            }
            named
        });
    match named.is_empty() {
        true => String::new(),
        false => format!("feeds {}", named.join(", ")),
    }
}

/// The sets a category comes from, capped at three plus a count — enough to recognise the
/// category without the row becoming a catalogue.
fn examples_text(category: &ProcCategory) -> String {
    const SHOWN: usize = 3;
    let total = category.examples.len();
    let shown = category
        .examples
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>();
    match total > SHOWN {
        true => format!("{} +{} more", shown.join(", "), total - SHOWN),
        false => shown.join(", "),
    }
}
