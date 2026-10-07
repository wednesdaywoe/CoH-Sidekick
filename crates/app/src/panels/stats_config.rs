//! The dashboard organizer: the user's panels with their stats in order, and the whole stat
//! vocabulary grouped by family underneath to pick from.
//!
//! It was a stat-SELECTION modal — one section per fixed panel, a pill per stat, and the only
//! question was which stats to show. A panel is a container the user fills now, so there are
//! two more questions: which panel a stat is in, and where in it. The browse grid below is the
//! old modal almost unchanged, because the question it answered is still one of the three; what
//! is new is the cards above it, which are where the other two are answered.
//!
//! No colour key (removed 2026-07-26, user-chosen — it cost more vertical space than the
//! sections could spare). The pills carry their stat's own family hue, so the modal still
//! teaches the hue code by BEING it: you pick "Melee" out of a purple row and it lands purple
//! on the panel. What the key alone documented — the meaning of the dotted underline and the
//! warning ring — now lives only in each value's `title` tooltip on the dashboard.
//!
//! Every label, pill colour, and family grouping is read from
//! [`crate::panels::stat_registry`] — this component names no stat and no family, so a stat
//! added to the registry appears here with no edit.
//!
//! Edits apply live rather than behind an Apply button. The beta buffers into local state and
//! commits on Apply, which made sense when the dashboard was a separate scrolling surface; here
//! the panels are on the grid behind the modal, so a change shows its result immediately and
//! the decision is made by looking rather than by imagining. Cancel is therefore not offered —
//! Reset to defaults is the undo that matters, and the modal has one.

use crate::grid::model::DashboardId;
use crate::modal::{Modal, ModalSize};
use crate::panels::dashboards::{DashboardConfig, Dashboards, NAME_MAX};
use crate::panels::stat_registry::{self, StatSection};
use dioxus::prelude::*;

/// Where the organizer opens.
///
/// [`Self::Panel`] opens it scrolled to one panel's card and flashes it — how a dashboard's gear
/// deep-links into its own group, the beta `statsConfigScrollTo`. [`Self::All`] opens it at the
/// top, naming nothing.
///
/// `All` exists because the roster can be empty. The quickbar's Configure Stats used to pass
/// `StatSection::ALL[0]`, with a doc saying "there is no 'opened at no particular place' to
/// represent" — true while every opener had a fixed section to name, and false the moment a
/// panel is something the user can delete. Without `All`, an entry point that has no panel in
/// mind would have to invent one, and the way back from a dashboard with no panels left would
/// be to have a panel already.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OrganizerTarget {
    All,
    Panel(DashboardId),
}

/// The one organizer's open state, held at the shell root. `None` is closed.
///
/// Held above both layout roots for the containment reason every modal here shares: a `fixed`
/// backdrop is contained by the grid's `transform`ed surfaces, so a modal rendered inside a
/// panel is clipped to that panel (see [`crate::modal`]).
#[derive(Clone, Copy, PartialEq)]
pub struct StatsConfigOpen(pub Signal<Option<OrganizerTarget>>);

/// Which panel a pill click lands in, and which stat row is mid-drag. Modal-local rather than
/// context, because neither outlives the modal and both are meaningless outside it.
#[derive(Clone, Copy, PartialEq)]
struct Organizing {
    /// The panel the browse grid adds to. `None` only while the roster is empty.
    adding_to: Option<DashboardId>,
    /// The stat id being dragged, if a drag is live.
    dragging: Option<&'static str>,
}

/// Scroll the deep-linked card into view and flash it, once the modal has mounted its scroll
/// container. Runs against the card's own DOM id rather than a Rust-side ref because the
/// scroll and the flash are both presentational and the element outlives neither.
const SCROLL_TO_SECTION: &str = "\
const el = document.getElementById(SLUG);\
if (el) {\
  el.scrollIntoView({ behavior: 'smooth', block: 'start' });\
  el.classList.add('is-flashed');\
  setTimeout(() => el.classList.remove('is-flashed'), 1200);\
}";

/// The modal, mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn StatsConfigHost() -> Element {
    let mut open = use_context::<StatsConfigOpen>().0;
    let Some(target) = open() else {
        return rsx! {};
    };

    rsx! {
        Modal {
            title: "Dashboard".to_string(),
            size: ModalSize::Xl,
            on_close: move |_| open.set(None),
            StatsConfigBody { target }
        }
    }
}

#[component]
fn StatsConfigBody(target: OrganizerTarget) -> Element {
    let mut dashboards = use_context::<DashboardConfig>().0;
    let mut organizing = use_signal(|| Organizing {
        adding_to: match target {
            OrganizerTarget::Panel(id) => Some(id),
            OrganizerTarget::All => dashboards.peek().first(),
        },
        dragging: None,
    });
    // The rename in flight: which panel, and the text typed so far. One signal rather than
    // two, because a draft with no panel to land on is not a state — the same shape
    // `attack_chain`'s `naming` uses, and for the same reason.
    let mut renaming = use_signal(|| Option::<(DashboardId, String)>::None);

    // Defer one frame so the modal's scroll container exists before we scroll inside it.
    // A target naming a panel the user deleted between opening and this running finds no
    // element; the script's own `if (el)` makes that a no-op rather than an error.
    use_effect(move || {
        let OrganizerTarget::Panel(id) = target else {
            return;
        };
        document::eval(&format!(
            "requestAnimationFrame(() => {{ const SLUG = \"stats-config-dashboard-{}\"; {SCROLL_TO_SECTION} }});",
            id.0
        ));
    });

    // A drag that ends anywhere but on a row still has to end. Without this, releasing over the
    // modal's padding leaves the roster believing a drag is live, and the next hover moves a
    // stat the user is no longer holding.
    let end_drag = move |_| organizing.write().dragging = None;

    let panels = dashboards.read().panels().to_vec();
    let placed = dashboards.read().placed_count();
    let total = stat_registry::ALL.len();
    // A panel the user deleted is still named by `adding_to` until they pick another. Resolving
    // it against the live roster each render, rather than clearing it on delete, keeps the two
    // states — "deleted" and "no longer the target" — from needing to be kept in step.
    let adding_to = organizing
        .read()
        .adding_to
        .filter(|id| dashboards.read().panel(*id).is_some());

    rsx! {
        div { class: "stats-config", onpointerup: end_drag, onpointercancel: end_drag,
            div { class: "stats-config-summary",
                if panels.is_empty() {
                    "No panels. Add one, then choose what it shows."
                } else {
                    "{placed} of {total} stats placed across {panels.len()} panels."
                }
            }

            div { class: "dash-cards",
                for panel in panels.iter() {
                    DashboardCard { key: "{panel.id.0}", id: panel.id, organizing, renaming }
                }
                button {
                    class: "dash-add",
                    r#type: "button",
                    onclick: move |_| {
                        let fresh = dashboards.write().add_panel();
                        organizing.write().adding_to = Some(fresh);
                        let minted = dashboards
                            .peek()
                            .name_of(fresh)
                            .unwrap_or_default()
                            .to_string();
                        renaming.set(Some((fresh, minted)));
                    },
                    "+ New panel"
                }
            }

            div { class: "stats-config-actions",
                // Which card a pill click lands in. A select rather than "whichever you touched
                // last", because a pill click gives no other signal about where it should go and
                // guessing wrong puts a stat in a panel that may be off screen.
                if panels.len() > 1 {
                    label { class: "dash-target",
                        span { "Add to" }
                        select {
                            value: adding_to.map(|id| id.0.to_string()).unwrap_or_default(),
                            onchange: move |evt| {
                                if let Ok(id) = evt.value().parse::<u32>() {
                                    organizing.write().adding_to = Some(DashboardId(id));
                                }
                            },
                            for panel in panels.iter() {
                                option { key: "{panel.id.0}", value: "{panel.id.0}", "{panel.name}" }
                            }
                        }
                    }
                }
                button {
                    class: "seg",
                    r#type: "button",
                    title: "Split the stats shown into one panel per category",
                    onclick: move |_| {
                        dashboards.write().regroup_by_category();
                        organizing.write().adding_to = dashboards.peek().first();
                    },
                    "Panels by category"
                }
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| dashboards.set(Dashboards::defaults()),
                    "Reset to defaults"
                }
            }

            for section in StatSection::ALL {
                StatsConfigSection { section, organizing }
            }
        }
    }
}

/// One panel's card: its name, its stats in order, and the controls that change both.
#[component]
fn DashboardCard(
    id: DashboardId,
    organizing: Signal<Organizing>,
    renaming: Signal<Option<(DashboardId, String)>>,
) -> Element {
    let mut dashboards = use_context::<DashboardConfig>().0;
    let mut organizing = organizing;
    let mut renaming = renaming;

    let Some(panel) = dashboards.read().panel(id).cloned() else {
        return rsx! {};
    };
    let is_target = organizing.read().adding_to == Some(id);
    let draft = renaming()
        .filter(|(held, _)| *held == id)
        .map(|(_, text)| text);
    let rows: Vec<&'static stat_registry::StatDef> = panel
        .stats
        .iter()
        .filter_map(|held| stat_registry::by_id(held))
        .collect();
    let name = panel.name.clone();

    rsx! {
        section {
            class: if is_target { "dash-card is-target" } else { "dash-card" },
            id: "stats-config-dashboard-{id.0}",
            // Dragging onto the CARD rather than onto one of its rows appends. That is what an
            // empty card needs — it has no row to aim at — and it is the only sensible reading
            // of the gap under a short list.
            onpointerenter: move |_| {
                let held = organizing.read().dragging;
                let Some(held) = held else { return };
                if dashboards.peek().home_of(held) == Some(id) {
                    return;
                }
                if let Some(stat) = stat_registry::by_id(held) {
                    dashboards.write().place_last(stat, id);
                }
            },

            header { class: "dash-card__head",
                if let Some(draft) = draft {
                    input {
                        class: "dash-card__rename",
                        r#type: "text",
                        autofocus: true,
                        maxlength: NAME_MAX as i64,
                        value: "{draft}",
                        // Select the existing name on open, so the first keystroke replaces it.
                        // `autofocus` alone leaves the caret at position 0 with the old name
                        // still there, and typing then PREPENDS — "Survivability" over
                        // "Dashboard 2" reads back as "SurvivabilityDashboard 2", which looks
                        // like the field ignoring the edit rather than honouring it.
                        onmounted: move |evt| async move {
                            let _ = evt.set_focus(true).await;
                            document::eval(
                                "const el = document.querySelector('.dash-card__rename'); \
                                 if (el) el.select();",
                            );
                        },
                        oninput: move |evt| renaming.set(Some((id, evt.value()))),
                        // Blur commits too, so clicking away from a typed name keeps it. The
                        // alternative is a rename that silently discards on every miss-click,
                        // and a name is a thing you look at rather than a form you submit.
                        onblur: move |_| {
                            let typed = renaming.peek().clone();
                            if let Some((held, text)) = typed {
                                if held == id {
                                    dashboards.write().rename(id, &text);
                                }
                            }
                            renaming.set(None);
                        },
                        onkeydown: move |evt: Event<KeyboardData>| match evt.key() {
                            Key::Enter => {
                                let typed = renaming.peek().clone();
                                if let Some((held, text)) = typed {
                                    if held == id {
                                        dashboards.write().rename(id, &text);
                                    }
                                }
                                renaming.set(None);
                            }
                            // Stopped here so Escape ends the rename rather than reaching the
                            // backdrop and closing the whole modal over an unfinished edit.
                            Key::Escape => {
                                evt.stop_propagation();
                                renaming.set(None);
                            }
                            _ => {}
                        },
                    }
                } else {
                    button {
                        class: "dash-card__name",
                        r#type: "button",
                        title: "Rename this panel",
                        onclick: {
                            let name = name.clone();
                            move |_| {
                                organizing.write().adding_to = Some(id);
                                renaming.set(Some((id, name.clone())));
                            }
                        },
                        "{name}"
                    }
                }
                span { class: "dash-card__count mono", "{rows.len()}" }
                button {
                    class: "dash-card__delete",
                    r#type: "button",
                    "aria-label": "Delete {name}",
                    title: "Delete this panel, its stats go back to the list below",
                    onclick: move |_| {
                        dashboards.write().remove_panel(id);
                        if organizing.peek().adding_to == Some(id) {
                            let next = dashboards.peek().first();
                            organizing.write().adding_to = next;
                        }
                    },
                    {crate::view::marks::close()}
                }
            }

            if rows.is_empty() {
                p { class: "dash-card__empty", "Nothing here yet — pick from the list below." }
            }
            div { class: "dash-rows",
                for (index, stat) in rows.iter().copied().enumerate() {
                    StatRow { key: "{stat.id}", stat, panel: id, index, organizing }
                }
            }
        }
    }
}

/// One placed stat: a drag handle, its name in its family's hue, and the keyboard-reachable ways
/// to move it.
///
/// The drag is the mobile reorder menu's shape — a handle seeds a shared signal on pointerdown,
/// every row commits on pointerenter, the release just clears it — and it reuses that menu's
/// `.reorder-handle` class deliberately, not out of convenience. Touch pointers take implicit
/// capture on pointerdown, which pins every later event to the element the gesture began on; the
/// capture-phase shim in `main.rs` releases it for exactly three selectors, and
/// `.reorder-handle` is one of them. A new class name would drag fine on a desktop and do
/// nothing at all on a phone.
#[component]
fn StatRow(
    stat: &'static stat_registry::StatDef,
    panel: DashboardId,
    index: usize,
    organizing: Signal<Organizing>,
) -> Element {
    let mut dashboards = use_context::<DashboardConfig>().0;
    let mut organizing = organizing;
    let held = organizing.read().dragging == Some(stat.id);

    rsx! {
        div {
            class: if held { "dash-row is-dragging" } else { "dash-row" },
            style: "--stat-hue: {stat.family.token()};",
            // The drop target is the row under the pointer, and `place` reads the index against
            // the list with the dragged stat already lifted out of it — the same convention
            // `grid::move_to_index` uses, so a stat and a panel answer a drop the same way.
            onpointerenter: move |_| {
                let dragging = organizing.read().dragging;
                let Some(dragging) = dragging else { return };
                if dragging == stat.id {
                    return;
                }
                if let Some(moving) = stat_registry::by_id(dragging) {
                    dashboards.write().place(moving, panel, index);
                }
            },
            div {
                class: "reorder-handle",
                onpointerdown: move |_| organizing.write().dragging = Some(stat.id),
                "☰"
            }
            span { class: "dash-row__label", "{stat.label}" }
            div { class: "dash-row__buttons",
                button {
                    class: "dash-row__move",
                    r#type: "button",
                    "aria-label": "Move {stat.label} up",
                    disabled: index == 0,
                    onclick: move |_| dashboards.write().place(stat, panel, index.saturating_sub(1)),
                    "▲"
                }
                button {
                    class: "dash-row__move",
                    r#type: "button",
                    "aria-label": "Move {stat.label} down",
                    onclick: move |_| dashboards.write().place(stat, panel, index + 1),
                    "▼"
                }
                button {
                    class: "dash-row__remove",
                    r#type: "button",
                    "aria-label": "Remove {stat.label}",
                    onclick: move |_| dashboards.write().unplace(stat.id),
                    "✕"
                }
            }
        }
    }
}

/// One family's worth of the vocabulary: a header that places or clears the whole family, then a
/// pill per stat.
///
/// Grouped by [`StatSection`] — the taxonomy — and NOT by panel, which is the distinction the
/// whole change rests on. A family is what a stat IS; a panel is where the user put it. The
/// browse list has to be the first of those, because you come here looking for a number by what
/// kind of number it is, and the panels are the thing being built out of what you find.
#[component]
fn StatsConfigSection(section: StatSection, organizing: Signal<Organizing>) -> Element {
    let mut dashboards = use_context::<DashboardConfig>().0;
    let stats: Vec<_> = stat_registry::in_section(section).collect();

    let config = dashboards.read();
    let shown = config.count_shown(stats.iter().copied());
    let total = stats.len();
    drop(config);
    let target = organizing
        .read()
        .adding_to
        .filter(|id| dashboards.read().panel(*id).is_some());

    // The header's own check state has three readings, and a plain checked/unchecked pair can
    // only carry two: none, some, all.
    let group_state = match shown {
        0 => "",
        n if n == total => "is-all",
        _ => "is-some",
    };

    rsx! {
        section {
            class: "stats-config-section",
            id: "stats-config-{section.slug()}",
            button {
                class: "stats-config-head {group_state}",
                r#type: "button",
                "aria-label": "Toggle every {section.title()} stat",
                disabled: target.is_none(),
                onclick: move |_| {
                    let Some(panel) = target else { return };
                    let stats: Vec<_> = stat_registry::in_section(section).collect();
                    dashboards.write().toggle_all(stats.iter().copied(), panel);
                },
                span { class: "stats-config-check", "aria-hidden": "true" }
                span { class: "stats-config-name", "{section.title()}" }
                span { class: "stats-config-count mono", "{shown}/{total}" }
            }
            div { class: "stats-config-pills",
                for stat in stats {
                    button {
                        key: "{stat.id}",
                        class: "stat-pill",
                        class: if dashboards.read().shows(stat.id) { "is-on" },
                        r#type: "button",
                        "aria-pressed": dashboards.read().shows(stat.id),
                        disabled: target.is_none(),
                        style: "--stat-hue: {stat.family.token()};",
                        // A pill that is on says WHERE it is on, because "on" stopped being the
                        // whole answer the moment there was more than one place to be on.
                        title: match dashboards.read().home_of(stat.id) {
                            Some(home) => match dashboards.read().name_of(home) {
                                Some(name) => format!("On {name} — click to remove"),
                                None => "Click to remove".to_string(),
                            },
                            None => "Not shown — click to add".to_string(),
                        },
                        onclick: move |_| {
                            if let Some(panel) = target {
                                dashboards.write().toggle(stat, panel);
                            }
                        },
                        "{stat.label}"
                    }
                }
            }
        }
    }
}
