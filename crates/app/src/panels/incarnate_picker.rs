//! The incarnate surface and its picker: a socket per offered slot on
//! [`PanelKind::Incarnates`](crate::grid::model::PanelKind::Incarnates), and a modal
//! shaped like the beta's `IncarnateModal` — slot tabs → tree sidebar → info panel →
//! the tree drawn as the game's craft ladder.
//!
//! Everything rendered here is read from [`coh_data::IncarnateCatalog`] — the dataset's
//! own export indices — and the slots OFFERED come from
//! [`coh_data::database::PowerDatabase::offered_incarnate_slots`], which gates on the
//! effects tables (GENESIS-1 dormancy) rather than a dataset-name list. Selection
//! writes [`coh_data::character::IncarnateLoadout`] through [`BuildSession::commit`],
//! so a pick is one undo step and the totals' Pass 6 sees it on the next recalculate
//! with no further wiring.
//!
//! The ladder's geometry is NOT decided here: [`IncarnateTreeView::grid_rows`] returns
//! the four rungs and their five columns, placed from each power's own derived
//! tier/branch/depth, and [`IncarnateTreeView::path_to`] returns what a power is crafted
//! through. This module only draws them, so no craft rule lives in the view (Rule 0).
//!
//! Two beta features are deliberately still absent:
//! - **The Very Rare pair cycler.** A T4 can be crafted from any two of its tree's four
//!   Rares; the beta cycled the six pairs on right-click. The tree here highlights the
//!   same-branch pair — the default [`super::incarnate_crafting`]'s tree also descends —
//!   and the game ships all six pairs as separate recipes now that `baserecipes.bin` is
//!   decoded, so the cycler is a data walk over
//!   [`coh_data::IncarnateCrafting::recipes_for`] whenever it lands.
//! - **The destiny-time slider.** The calc resolves Destiny at its sustained-floor decay
//!   time (`destinyTime: null`, the beta's default); a slider without the runtime input
//!   would be a control that moves nothing. It lands with the destiny-time input itself.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use crate::view::icons::incarnate_icon_url;
use coh_data::character::IncarnateSlot;
use coh_data::{IncarnateCatalogPower, IncarnateSlotCatalog, IncarnateTier, IncarnateTreeView};
use dioxus::prelude::*;

/// The picker's open state: `Some(slot id)` = open on that slot's tab. Provided
/// at the shell root and hosted there, outside the free grid's `transform`ed
/// surfaces, for the same containment reason as
/// [`PoolPickerOpen`](crate::panels::pool_picker::PoolPickerOpen).
#[derive(Clone, Copy)]
pub struct IncarnatePickerOpen(pub Signal<Option<String>>);

/// The one incarnate picker for the whole app. Renders nothing while closed.
#[component]
pub fn IncarnatePickerHost(database: Db) -> Element {
    let mut open = use_context::<IncarnatePickerOpen>().0;
    match open() {
        None => rsx! {},
        Some(slot_id) => rsx! {
            IncarnatePickerModal {
                database,
                initial_slot: slot_id,
                on_close: move |_| open.set(None),
            }
        },
    }
}

/// The equipped power's display name and tier for one slot, resolved against the
/// catalog. `None` when the slot is empty; the display name falls back to the stored
/// internal name when the catalog no longer carries the pick (a build carried across a
/// dataset switch), so a stale pick reads as a stale pick rather than as an empty slot.
struct EquippedPower {
    internal_name: String,
    display_name: String,
    tree_name: String,
    tier: Option<IncarnateTier>,
    active: bool,
}

fn equipped(slot: &IncarnateSlotCatalog, pick: Option<&IncarnateSlot>) -> Option<EquippedPower> {
    let pick = pick?;
    let found = slot.find_power(&pick.power_name);
    Some(EquippedPower {
        internal_name: pick.power_name.clone(),
        display_name: found
            .map(|p| p.display_name.clone())
            .unwrap_or_else(|| pick.power_name.clone()),
        tree_name: found.map(|p| p.tree_name()).unwrap_or_default(),
        tier: found.map(|p| p.tier()),
        active: pick.active,
    })
}

/// The tier token a rule keys its rarity colour on (`--tier-*` in tokens.css); an
/// unresolvable pick has no tier, so it gets none and stays on the neutral seam.
fn tier_class(prefix: &str, tier: Option<IncarnateTier>) -> String {
    match tier {
        Some(tier) => format!(" {prefix}--{}", tier.icon_token()),
        None => String::new(),
    }
}

/// The letter an empty socket carries — the slot's own initial (A J I D L H, plus G where
/// Genesis is live). Read off the display name rather than a slot-id table, so a dataset
/// that offers a slot this build has never heard of still labels its socket.
fn slot_initial(display_name: &str) -> String {
    display_name
        .chars()
        .next()
        .map(|first| first.to_uppercase().to_string())
        .unwrap_or_default()
}

/// Toggle whether a slot's pick feeds the totals. One write path, shared by the socket's
/// state dot and its right-click, so neither can commit a state the other wouldn't.
fn toggle_active(session: BuildSession, slot_id: String) {
    session.commit(move |state| {
        if let Some(pick) = state.incarnates.get(&slot_id).cloned() {
            let _ = state.incarnates.set(
                &slot_id,
                Some(IncarnateSlot {
                    active: !pick.active,
                    ..pick
                }),
            );
        }
    });
}

// ---------------------------------------------------------------------------
// The socket row
// ---------------------------------------------------------------------------

/// The incarnate loadout: one socket per offered slot, in the beta's `IncarnateSlotGrid`
/// scheme. An empty socket is a ring carrying the slot's initial and nothing else; slotting
/// a power puts its art in the ring. That's the whole surface — the slot name and tree name
/// the socket used to spell out are what the art and the tooltip say, and six sockets of
/// stacked labels was a panel-sized answer to a one-row question.
///
/// Clicking a socket opens the picker on that slot. Rarity rides the ring and the corner pip;
/// an equipped socket also carries the dot for whether it feeds the totals.
///
/// **It leads the [Available](crate::panels::powers::AvailablePanel) rail rather than owning a
/// grid panel** (2026-09-13, user-directed). `PanelKind::Incarnates` argued the other way —
/// that this is a second loadout, level-50 choices rather than part of choosing which powers to
/// pick, so it deserved its own surface. What that argument did not weigh is that the surface
/// it bought was hidden by default at every width below twelve columns, and that its content is
/// one row: a whole panel, its title bar, its drag handle and its seat in the band, spent on a
/// row of six rings. As a row at the head of the rail it is always present and costs the rail
/// nothing it was using, which is the compactness the panel could not offer at any width.
#[component]
pub fn IncarnateSockets(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut picker = use_context::<IncarnatePickerOpen>().0;

    let offered = database.offered_incarnate_slots();
    if offered.is_empty() {
        return rsx! {
            div { class: "incarnates",
                p { class: "hint", "This dataset carries no incarnate data." }
            }
        };
    }

    // The crafting checklist covers the slots the dataset ships recipes for;
    // its entry opens on the first equipped covered slot, else the first
    // covered one, so the button always lands somewhere meaningful.
    let mut crafting = use_context::<super::incarnate_crafting::IncarnateCraftingOpen>().0;
    let crafting_slot = {
        let has_recipes = |slot: &coh_data::IncarnateSlotCatalog| {
            slot.powers.iter().any(|p| {
                !database
                    .0
                    .incarnate_crafting
                    .recipes_for(&p.full_name)
                    .is_empty()
            })
        };
        let covered: Vec<&coh_data::IncarnateSlotCatalog> = offered
            .iter()
            .copied()
            .filter(|slot| has_recipes(slot))
            .collect();
        covered
            .iter()
            .find(|slot| build.read().incarnates.get(&slot.id).is_some())
            .or(covered.first())
            .map(|slot| slot.id.clone())
    };

    rsx! {
        div { class: "incarnates",
            div { class: "incarnate-sockets",
                for slot in offered.iter() {
                    {
                        let slot_id = slot.id.clone();
                        let pick = build.read().incarnates.get(&slot_id).cloned();
                        let equipped = equipped(slot, pick.as_ref());
                        let tier = equipped.as_ref().and_then(|e| e.tier);
                        // Equipped-and-off dims the whole socket: an incarnate that isn't
                        // feeding the totals is doing nothing, and the socket should read
                        // that way at a glance rather than only in its state dot.
                        let dimmed = equipped.as_ref().is_some_and(|e| !e.active);
                        let open_id = slot_id.clone();
                        let menu_id = slot_id.clone();
                        let toggle_id = slot_id.clone();
                        let title = match equipped.as_ref() {
                            Some(e) => format!(
                                "{} — {} ({})\nRight-click to toggle whether it feeds the totals",
                                slot.display_name, e.display_name, e.tree_name
                            ),
                            None => format!("Choose a {} power", slot.display_name),
                        };
                        rsx! {
                            div {
                                key: "{slot.id}",
                                class: "incarnate-socket{tier_class(\"incarnate-socket\", tier)}",
                                class: if dimmed { "is-dimmed" } else { "" },
                                class: if equipped.is_some() { "is-equipped" } else { "" },
                                // Right-click toggles active (the beta's `onContextMenu`).
                                // On an empty slot there is nothing to toggle, so this only
                                // suppresses the native menu there — `toggle_active` re-reads
                                // the loadout, so it is inert either way.
                                oncontextmenu: move |evt: Event<MouseData>| {
                                    evt.prevent_default();
                                    toggle_active(session, menu_id.clone());
                                },
                                button {
                                    class: "incarnate-socket__open",
                                    title: "{title}",
                                    onclick: move |_| picker.set(Some(open_id.clone())),
                                    match equipped.as_ref() {
                                        Some(e) => {
                                            let icon = incarnate_icon_url(
                                                &slot_id,
                                                &e.internal_name,
                                                tier.map(IncarnateTier::icon_token).unwrap_or("common"),
                                            );
                                            rsx! {
                                                img {
                                                    class: "incarnate-socket__icon",
                                                    src: "{icon}",
                                                    alt: "",
                                                }
                                            }
                                        }
                                        None => rsx! {
                                            span {
                                                class: "incarnate-socket__initial",
                                                "{slot_initial(&slot.display_name)}"
                                            }
                                        },
                                    }
                                }
                                if let Some(e) = equipped.as_ref() {
                                    button {
                                        class: if e.active {
                                            "incarnate-socket__state is-on"
                                        } else {
                                            "incarnate-socket__state"
                                        },
                                        title: if e.active {
                                            "Feeding the totals — click to disable"
                                        } else {
                                            "Not feeding the totals — click to enable"
                                        },
                                        onclick: move |_| toggle_active(session, toggle_id.clone()),
                                    }
                                }
                                if tier.is_some() {
                                    span { class: "incarnate-socket__tier-pip" }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(slot_id) = crafting_slot {
                button {
                    class: "incarnate-crafting-open",
                    onclick: move |_| {
                        crafting.set(Some(
                            super::incarnate_crafting::CraftingTab::Slot(slot_id.clone()),
                        ));
                    },
                    "Crafting checklist…"
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The modal
// ---------------------------------------------------------------------------

#[component]
fn IncarnatePickerModal(database: Db, initial_slot: String, on_close: EventHandler<()>) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    let mut active_slot = use_signal(|| initial_slot.clone());
    // The tree the sidebar has selected, per open. `None` = follow the pick
    // (the tree of the slot's current power, else the first tree).
    let mut chosen_tree = use_signal(|| Option::<String>::None);
    // The node under the pointer, which is what the info panel reads before anything is
    // hovered — the beta's `hoveredPower || selectedPower`. Held as an internal name so
    // it survives the tree being regrouped between renders.
    let mut hovered = use_signal(|| Option::<String>::None);

    let offered = database.offered_incarnate_slots();
    if offered.is_empty() {
        return rsx! {
            Modal { title: "Incarnate Powers".to_string(), size: ModalSize::Lg, on_close,
                p { class: "hint", "This dataset carries no incarnate data." }
            }
        };
    }
    let slot = offered
        .iter()
        .find(|s| s.id == active_slot())
        .copied()
        .unwrap_or(offered[0]);

    let current_pick: Option<IncarnateSlot> = build.read().incarnates.get(&slot.id).cloned();
    let trees = slot.trees();

    // Resolve which tree the main pane shows: the sidebar's choice, else the
    // tree holding the current pick, else the first.
    let picked_tree_name = current_pick
        .as_ref()
        .and_then(|pick| slot.find_power(&pick.power_name).map(|p| p.tree_name()));
    let shown_tree_name = chosen_tree()
        .filter(|name| trees.iter().any(|t| &t.name == name))
        .or(picked_tree_name.clone())
        .unwrap_or_else(|| trees.first().map(|t| t.name.clone()).unwrap_or_default());
    let shown_tree = trees.iter().find(|t| t.name == shown_tree_name);

    let selected_name = current_pick.as_ref().map(|pick| pick.power_name.clone());
    let selected_power = selected_name
        .as_deref()
        .and_then(|name| shown_tree.and_then(|tree| find_in_tree(tree, name)));

    // The rungs a selected power is crafted through, so the ladder shows the route and not
    // just the destination. Read from the tree, never assembled here.
    let path: Vec<String> = match (shown_tree, selected_power) {
        (Some(tree), Some(power)) => tree
            .path_to(power)
            .iter()
            .map(|p| p.internal_name.clone())
            .collect(),
        _ => Vec::new(),
    };

    // Hover wins over selection, and falls back to it — the info panel is about whichever
    // node the eye is on. A hover left over from another tree resolves to nothing here,
    // which is correct: it isn't on screen any more.
    let detail = hovered()
        .as_deref()
        .and_then(|name| shown_tree.and_then(|tree| find_in_tree(tree, name)))
        .or(selected_power);

    let slot_title = format!("Incarnate — {}", slot.display_name);

    rsx! {
        Modal { title: slot_title, size: ModalSize::Full, on_close,
            div { class: "incarnate-picker",
                // Slot tabs — one per offered slot, underlined when active and
                // rarity-pipped when equipped.
                div { class: "incarnate-picker__tabs",
                    for tab in offered.iter() {
                        {
                            let tab_pick = build.read().incarnates.get(&tab.id).cloned();
                            let tab_tier = equipped(tab, tab_pick.as_ref()).and_then(|e| e.tier);
                            let id = tab.id.clone();
                            rsx! {
                                button {
                                    key: "{tab.id}",
                                    class: "incarnate-tab{tier_class(\"incarnate-tab\", tab_tier)}",
                                    class: if tab.id == slot.id { "is-active" } else { "" },
                                    onclick: move |_| {
                                        active_slot.set(id.clone());
                                        chosen_tree.set(None);
                                        hovered.set(None);
                                    },
                                    span { "{tab.display_name}" }
                                    if tab_pick.is_some() {
                                        span { class: "incarnate-tab__equipped-dot" }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "incarnate-picker__body",
                    // Tree sidebar.
                    div { class: "incarnate-picker__trees",
                        span { class: "incarnate-picker__trees-title", "Trees" }
                        for tree in trees.iter() {
                            button {
                                key: "{tree.name}",
                                class: if tree.name == shown_tree_name {
                                    "incarnate-tree-row is-active"
                                } else {
                                    "incarnate-tree-row"
                                },
                                onclick: {
                                    let name = tree.name.clone();
                                    move |_| {
                                        chosen_tree.set(Some(name.clone()));
                                        hovered.set(None);
                                    }
                                },
                                span { "{tree.name}" }
                                if picked_tree_name.as_deref() == Some(tree.name.as_str()) {
                                    span { class: "incarnate-tab__equipped-dot" }
                                }
                            }
                        }
                    }

                    div { class: "incarnate-picker__main",
                        NodeDetail { power: detail.cloned() }
                        // The tree as the craft ladder: Very Rare at the top down to the
                        // Common root, each rung five columns wide.
                        div { class: "incarnate-tree",
                            if let Some(tree) = shown_tree {
                                for row in tree.grid_rows() {
                                    TreeRung {
                                        key: "{row.tier.icon_token()}",
                                        tier: row.tier,
                                        cells: row.cells.iter().map(|c| c.map(|p| (*p).clone())).collect::<Vec<_>>(),
                                        slot_id: slot.id.clone(),
                                        selected: selected_name.clone(),
                                        path: path.clone(),
                                        hovered,
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "incarnate-picker__footer",
                    if let Some(pick) = current_pick.as_ref() {
                        {
                            let picked_display = slot
                                .find_power(&pick.power_name)
                                .map(|p| p.display_name.clone())
                                .unwrap_or_else(|| pick.power_name.clone());
                            let slot_id = slot.id.clone();
                            rsx! {
                                span { class: "incarnate-picker__current", "Equipped: {picked_display}" }
                                button {
                                    class: "incarnate-picker__clear",
                                    onclick: move |_| {
                                        let slot_id = slot_id.clone();
                                        session.commit(move |state| {
                                            let _ = state.incarnates.set(&slot_id, None);
                                        });
                                    },
                                    "Clear selection"
                                }
                            }
                        }
                    } else {
                        span { class: "incarnate-picker__current is-empty", "Nothing equipped in this slot" }
                    }
                }
            }
        }
    }
}

/// The tree's own power with this internal name, case-insensitively — the loadout stores
/// what the export gave it, which is not always the catalog's casing.
fn find_in_tree<'a>(
    tree: &IncarnateTreeView<'a>,
    internal_name: &str,
) -> Option<&'a IncarnateCatalogPower> {
    tree.powers
        .iter()
        .copied()
        .find(|p| p.internal_name.eq_ignore_ascii_case(internal_name))
}

/// The info strip above the tree: whatever node the pointer is on (or, failing that, the
/// pick). A fixed-height block rather than one that appears on hover — a strip that
/// materialises under the pointer would move the tree out from under it.
#[component]
fn NodeDetail(power: Option<IncarnateCatalogPower>) -> Element {
    let Some(power) = power else {
        return rsx! {
            div { class: "incarnate-detail is-empty",
                span { "Hover or tap a power to read what it does." }
            }
        };
    };
    let tier = power.tier();

    rsx! {
        div { class: "incarnate-detail{tier_class(\"incarnate-detail\", Some(tier))}",
            span { class: "incarnate-detail__name", "{power.display_name}" }
            span { class: "incarnate-detail__tier", "{tier.label()}" }
            if !power.short_help.is_empty() {
                p { class: "incarnate-detail__help", "{power.short_help}" }
            }
        }
    }
}

/// One rung of the ladder: the tier's label over its five columns. An empty column draws a
/// spacer so the columns line up down the whole tree — that alignment is the only thing
/// that makes a branch read as one vertical line.
#[component]
fn TreeRung(
    tier: IncarnateTier,
    cells: Vec<Option<IncarnateCatalogPower>>,
    slot_id: String,
    selected: Option<String>,
    path: Vec<String>,
    hovered: Signal<Option<String>>,
) -> Element {
    let session = use_context::<BuildSession>();
    let tier_token = tier.icon_token();

    rsx! {
        div { class: "incarnate-rung incarnate-rung--{tier_token}",
            span { class: "incarnate-rung__label", "{tier.label()}" }
            div { class: "incarnate-rung__cells",
                for (column, cell) in cells.iter().enumerate() {
                    match cell {
                        None => rsx! { span { key: "{column}", class: "incarnate-node-gap" } },
                        Some(power) => {
                            let is_selected = selected.as_deref()
                                .is_some_and(|s| s.eq_ignore_ascii_case(&power.internal_name));
                            let on_path = path
                                .iter()
                                .any(|p| p.eq_ignore_ascii_case(&power.internal_name));
                            let icon = incarnate_icon_url(&slot_id, &power.internal_name, tier_token);
                            let internal = power.internal_name.clone();
                            let enter_name = power.internal_name.clone();
                            let slot_id = slot_id.clone();
                            rsx! {
                                button {
                                    key: "{power.internal_name}",
                                    class: "incarnate-node",
                                    class: if is_selected { "is-selected" } else { "" },
                                    class: if on_path { "is-on-path" } else { "" },
                                    title: "{power.short_help}",
                                    onmouseenter: move |_| {
                                        let mut hovered = hovered;
                                        hovered.set(Some(enter_name.clone()));
                                    },
                                    onmouseleave: move |_| {
                                        let mut hovered = hovered;
                                        hovered.set(None);
                                    },
                                    // Clicking the equipped node clears the slot — the same
                                    // select/deselect the beta's tree nodes carried.
                                    onclick: move |_| {
                                        let slot_id = slot_id.clone();
                                        let pick = (!is_selected).then(|| IncarnateSlot {
                                            power_name: internal.clone(),
                                            active: true,
                                        });
                                        session.commit(move |state| {
                                            let _ = state.incarnates.set(&slot_id, pick);
                                        });
                                    },
                                    img { class: "incarnate-node__icon", src: "{icon}", alt: "" }
                                    // `node_label` is the catalog's own shortening — the tree
                                    // prefix dropped and the tier/branch words initialled — so
                                    // the label can't disagree with the cell the node sits in.
                                    span { class: "incarnate-node__label", "{power.node_label()}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
