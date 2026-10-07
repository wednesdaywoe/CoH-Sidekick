/// Mobile reorder menu — toggle-overlay with drag-reorder and fallback ▲/▼ buttons.
/// Mirrors desktop's dual affordance (pointer-drag + buttons); sits below 900px viewport.
///
/// HM5: the open state is owned by the caller (the shell, which offers a Reorder entry in the
/// Display menu) rather than by a trigger button on the row, so the overlay is drivable from
/// row 1 and the loose “☰ Reorder” button is gone.
use crate::grid::model::GridItem;
use crate::grid::PanelKind;
use crate::mobile_order::MobileOrder;
use dioxus::prelude::*;

#[component]
pub fn ReorderMenu(
    mut open: Signal<bool>,
    mut mobile_order: Signal<MobileOrder>,
    layout: Signal<Vec<GridItem>>,
) -> Element {
    let dragging = use_signal(|| Option::<PanelKind>::None);
    let hover_index = use_signal(|| Option::<usize>::None);
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;

    if !open() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "reorder-overlay",
            onclick: move |_| open.set(false),
            div {
                class: "reorder-menu",
                role: "dialog",
                "aria-label": "Reorder panels",
                onclick: move |evt| evt.stop_propagation(),
                div { class: "reorder-head",
                    span { class: "reorder-title", "Reorder panels" }
                    button {
                        class: "reorder-close",
                        r#type: "button",
                        "aria-label": "Close",
                        onclick: move |_| open.set(false),
                        "✕"
                    }
                }

                    // Every panel is listed, hidden ones included and marked: the order is
                    // where a panel WILL sit, so a hidden one still holds its place for when
                    // it comes back, and a list that silently dropped it would read as the
                    // panel having been lost. Restoring it is the Panels modal's job — one
                    // surface owns visibility (see `crate::panel_visibility`).
                    {
                        let roster = crate::panels::dashboards::surfaces(&dashboards.read());
                        let hidden = crate::panel_visibility::hidden_panels(&layout.read(), &roster);
                        rsx! {
                            for (index, panel) in mobile_order().0.iter().enumerate() {
                                ReorderRow {
                                    key: "{index}",
                                    panel: *panel,
                                    index,
                                    hidden: hidden.contains(panel),
                                    mobile_order,
                                    dragging,
                                    hover_index,
                                }
                            }
                        }
                    }
            }
        }
    }
}

#[component]
fn ReorderRow(
    panel: PanelKind,
    index: usize,
    hidden: bool,
    mut mobile_order: Signal<MobileOrder>,
    mut dragging: Signal<Option<PanelKind>>,
    mut hover_index: Signal<Option<usize>>,
) -> Element {
    let mut dragging = dragging;
    let mut hover_index = hover_index;
    let is_dragging = dragging() == Some(panel);

    let mut classes = String::from("reorder-row");
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let title = crate::panels::dashboards::title_of(panel, &dashboards.read());
    if is_dragging {
        classes.push_str(" dragging");
    }
    if hidden {
        classes.push_str(" is-hidden");
    }

    rsx! {
        div {
            class: "{classes}",
            onpointerenter: move |_| {
                if dragging().is_some() && dragging() != Some(panel) {
                    hover_index.set(Some(index));
                }
            },
            onpointerup: move |_| {
                if let Some(dragged) = dragging() {
                    if dragged != panel {
                        mobile_order.with_mut(|o| *o = o.move_to_index(dragged, index));
                        crate::layout_store::persist_mobile(&mobile_order.peek());
                    }
                    dragging.set(None);
                    hover_index.set(None);
                }
            },

            // Grip handle for touch-drag reorder
            div {
                class: "reorder-handle",
                onpointerdown: move |_| {
                    dragging.set(Some(panel));
                },
                "☰"
            }

            // Panel name (non-interactive)
            span { class: "reorder-label", "{title}" }
            if hidden {
                span { class: "reorder-hidden mono", "hidden" }
            }

            // Accessible fallback buttons
            div { class: "reorder-buttons",
                button {
                    onclick: move |_| {
                        mobile_order.with_mut(|o| *o = o.move_by(panel, -1));
                        crate::layout_store::persist_mobile(&mobile_order.peek());
                    },
                    "▲"
                }
                button {
                    onclick: move |_| {
                        mobile_order.with_mut(|o| *o = o.move_by(panel, 1));
                        crate::layout_store::persist_mobile(&mobile_order.peek());
                    },
                    "▼"
                }
            }
        }
    }
}
