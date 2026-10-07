//! Which surfaces are on the grid at all — the write path every route shares, the modal that
//! lists them, and the prompt that stands in for a grid with nothing left on it. The everyday
//! route is the quickbar's pill row ([`crate::quickbar`]); this module owns the mechanism.
//!
//! Folding a surface says "not right now"; hiding one says "not in this build". A build that
//! never reads Status Resistance would otherwise spend a cell on it forever, because the
//! stat-config modal can empty that panel but not remove it.
//!
//! Every row is read from [`PanelKind::ALL`] and titled by `PanelKind::title`, so this
//! component names no surface — a surface added to the enum appears here with no edit.
//!
//! Hidden lives on the [`GridItem`] rather than in a set of its own, next to `collapsed` and
//! for the same reason: it is a property of the surface, not of which arrangement is on
//! screen, so hiding a panel on the desktop grid holds in the mobile stack too. The whole
//! reflow is the pure [`set_hidden`]; this module calls it, persists the result, and scrolls a
//! surface it just showed back into view.
//!
//! Edits apply live, like the stat-config modal — the grid is right behind the card, so a
//! toggle shows its result immediately rather than being imagined behind an Apply button.

use crate::grid::collide::set_hidden;
use crate::grid::model::{GridItem, PanelKind};
use crate::layout_store;
use crate::modal::{Modal, ModalSize};
use dioxus::prelude::*;

/// The modal's open state, held at the shell root and provided as context so the quickbar's
/// menu entry and the empty-grid prompt can both open it. A bare flag: unlike the stat config's
/// per-section deep link, every opener here wants the same whole list.
///
/// Hosted above both layout roots for the containment reason every modal in the rebuild
/// shares — a `fixed` backdrop is contained by the grid's `transform`ed surfaces, so a modal
/// rendered inside a panel is clipped to that panel (see [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct PanelVisibilityOpen(pub Signal<bool>);

/// The surfaces currently off the grid, in roster order.
///
/// Reported in the roster's order rather than the layout's, so two callers comparing lists
/// compare the same list — the layout's own order is whatever the last drag left behind.
pub fn hidden_panels(layout: &[GridItem], roster: &[PanelKind]) -> Vec<PanelKind> {
    roster
        .iter()
        .copied()
        .filter(|panel| {
            layout
                .iter()
                .any(|item| item.panel == *panel && item.hidden)
        })
        .collect()
}

/// Take `panel` off the grid or put it back, persisting the reflowed layout. The one write
/// path — the panel header's own control, the modal's rows, the quickbar's pills and the
/// empty-grid prompt all come through here, so no caller can commit a hide without the push
/// and compaction that close the grid over it.
pub fn set_panel_hidden(
    mut layout: Signal<Vec<GridItem>>,
    columns: u32,
    panel: PanelKind,
    hidden: bool,
) {
    let mut items = layout.peek().clone();
    set_hidden(&mut items, panel, hidden);
    layout_store::persist_desktop(&items, columns);
    layout.set(items);
    if !hidden {
        scroll_into_view(panel);
    }
}

/// Bring a just-shown surface on screen. A hidden surface returns to the cell it was hidden
/// from, and `compact` floats it no higher than the surfaces above it — which for one hidden
/// from the foot of the grid means it comes back at the foot of the grid, off the bottom of a
/// 900px viewport. The control that showed it is up in the quickbar, so without this the click
/// reads as having done nothing.
///
/// Deferred two frames because the surface does not exist yet: this runs inside the same
/// handler that writes the layout, and Dioxus renders the node afterward. `block: "nearest"`
/// scrolls the least that puts it in view, so a surface already on screen doesn't jump.
///
/// Driven from JS rather than `web-sys` so it is one behaviour on both platforms — a
/// `wasm32`-gated DOM helper is a feature deleted on the desktop webview. The eval is
/// fire-and-forget, like the layout persist above it.
pub fn scroll_into_view(panel: PanelKind) {
    let js = format!(
        "requestAnimationFrame(() => requestAnimationFrame(() => {{\
           const el = document.querySelector('.surface[data-id={:?}]');\
           if (el) el.scrollIntoView({{ block: 'nearest', behavior: 'smooth' }});\
         }}));",
        panel.slug()
    );
    document::eval(&js);
}

/// Put every hidden surface back, as ONE layout edit rather than a call to
/// [`set_panel_hidden`] per surface.
///
/// Not just fewer writes — the per-surface loop is a borrow panic. Reading the layout to
/// learn what is hidden and writing it back to show one are the same signal, and a `for`
/// loop holds the guard from its iterator expression for the whole body, so the first
/// write lands while the read is still live. Binding the list first is what ends the read.
fn show_all(mut layout: Signal<Vec<GridItem>>, columns: u32, roster: &[PanelKind]) {
    let popped = consume_context::<crate::panel_popout::PoppedOut>();
    let mut items = layout.peek().clone();
    let hidden = hidden_panels(&items, roster);
    for panel in hidden {
        // A popped-out surface is hidden, but it is not hidden in the sense this button means —
        // showing it would draw it on the grid while its own window still has it. It comes back
        // by closing that window (`crate::panel_popout`).
        if popped.holds(panel) {
            continue;
        }
        set_hidden(&mut items, panel, false);
    }
    layout_store::persist_desktop(&items, columns);
    layout.set(items);
}

/// The modal, mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn PanelVisibilityHost(layout: Signal<Vec<GridItem>>) -> Element {
    let mut open = use_context::<PanelVisibilityOpen>().0;
    if !open() {
        return rsx! {};
    }

    rsx! {
        Modal {
            title: "Panels".to_string(),
            size: ModalSize::Md,
            on_close: move |_| open.set(false),
            PanelVisibilityBody { layout }
        }
    }
}

#[component]
fn PanelVisibilityBody(layout: Signal<Vec<GridItem>>) -> Element {
    let columns = use_context::<crate::grid::view::GridColumns>().0();
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let roster = crate::panels::dashboards::surfaces(&dashboards.read());
    let hidden = hidden_panels(&layout.read(), &roster);
    let showing = roster.len() - hidden.len();
    let total = roster.len();

    rsx! {
        div { class: "panel-visibility",
            div { class: "panel-visibility-summary",
                "{showing} of {total} panels on the grid."
            }

            div { class: "panel-visibility-actions",
                button {
                    class: "seg",
                    r#type: "button",
                    disabled: hidden.is_empty(),
                    onclick: {
                        let roster = roster.clone();
                        move |_| show_all(layout, columns, &roster)
                    },
                    "Show all"
                }
            }

            div { class: "panel-visibility-list",
                for panel in roster.iter().copied() {
                    PanelVisibilityRow { key: "{panel.slug()}", panel, layout }
                }
            }
        }
    }
}

/// One surface's row: a toggle carrying its own on/off state, plus the word for that state.
/// A row where the quickbar uses a pill, because the two are answering different questions. A
/// pill row is read as "which of these are on", which colour and dash carry at a glance; this
/// list is where you come when that was not enough, so each entry gets the width to say
/// "Hidden" or "On grid" in words.
#[component]
fn PanelVisibilityRow(panel: PanelKind, layout: Signal<Vec<GridItem>>) -> Element {
    let columns = use_context::<crate::grid::view::GridColumns>().0();
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let title = crate::panels::dashboards::title_of(panel, &dashboards.read());
    let hidden = layout
        .read()
        .iter()
        .any(|item| item.panel == panel && item.hidden);
    // Tracked, so the row restates itself the moment the surface leaves or returns.
    let popped = use_context::<crate::panel_popout::PoppedOut>()
        .0
        .read()
        .contains(&panel);

    rsx! {
        button {
            class: if popped {
                "panel-visibility-row is-popped"
            } else if hidden {
                "panel-visibility-row"
            } else {
                "panel-visibility-row is-on"
            },
            r#type: "button",
            // Out in its own window is a third state, not a darker shade of hidden, and the one
            // thing this control must not do with it is offer the grid back — the surface is
            // live somewhere else, so showing it would draw it twice. Inert rather than absent:
            // a row that vanished would read as the panel having been lost, which is the same
            // argument the reorder list already makes for listing hidden ones.
            disabled: popped,
            "aria-pressed": !hidden,
            onclick: move |_| set_panel_hidden(layout, columns, panel, !hidden),
            span { class: "panel-visibility-check", "aria-hidden": "true" }
            span { class: "panel-visibility-name", "{title}" }
            span { class: "panel-visibility-state mono",
                if popped { "Popped out" } else if hidden { "Hidden" } else { "On grid" }
            }
        }
    }
}

/// What a layout root draws once every surface is hidden. The quickbar's pill row is the way
/// back — it is chrome, so it survives an empty grid — and this is what stops the space under
/// it from reading as a failed load: an empty page states nothing, and "every panel is hidden"
/// is a different thing from "the dataset didn't come".
#[component]
pub fn AllPanelsHidden() -> Element {
    let mut open = use_context::<PanelVisibilityOpen>().0;

    rsx! {
        div { class: "panels-empty",
            p { "Every panel is hidden." }
            button {
                class: "seg",
                r#type: "button",
                onclick: move |_| open.set(true),
                "Choose panels…"
            }
        }
    }
}
