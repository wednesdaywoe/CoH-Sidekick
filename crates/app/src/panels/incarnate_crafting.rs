//! The incarnate crafting checklist: the craft tree behind each equipped
//! incarnate pick, and the consolidated salvage shopping list.
//!
//! Everything rendered here is read from [`coh_data::IncarnateCrafting`] — the
//! dataset's own `baserecipes.bin` recipes — through
//! [`coh_data::IncarnateCrafting::craft_tree`], so no craft rule lives in the
//! view (Rule 0): a Tier 4 consuming both same-branch Rares, each Rare its
//! branch's Tier 2, each Tier 2 the Common root, is all stated by the recipes'
//! own `PowerComponent` edges. The beta's `IncarnateCraftingModal` had a
//! hand-ported table for the same content, wrong in two documented ways this
//! source fixes (the Demons lore tree missing; the tier-3 radial pair swapped
//! in the hand-ported table).
//!
//! Checklist state rides the build ([`CharacterState::crafting_obtained`],
//! `crafting_salvage_checked`, `shopping_acquired`) through
//! [`BuildSession::commit`], so a checkbox is one undo step and persists with
//! the build. Node keys are parent-qualified ([`CraftNode::key`]) because a
//! Very Rare's tree holds the same Tier 2 twice — two separate crafts.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::character::CharacterState;
use coh_data::{CraftNode, CraftSalvage, IncarnateSlotCatalog, RecipeFamily};
use dioxus::prelude::*;
use std::collections::BTreeMap;

/// Which crafting surface is open: one slot's tree, or the all-slots list.
#[derive(Clone, PartialEq)]
pub enum CraftingTab {
    Slot(String),
    Shopping,
}

/// The modal's open state, provided at the shell root and hosted there —
/// outside the free grid's `transform`ed surfaces, like every other modal.
#[derive(Clone, Copy)]
pub struct IncarnateCraftingOpen(pub Signal<Option<CraftingTab>>);

/// The store's two currencies the summary lines total. These are display
/// VOCABULARY, not routing: the totalled options are whatever `buy` rows the
/// dataset's own conversion store states (the strings are those rows' currency
/// display names), and a salvage priced in neither simply prices as nothing.
const THREAD_CURRENCY: &str = "Incarnate Thread";
const EMPYREAN_CURRENCY: &str = "Empyrean Merit";

/// The slots this dataset's recipes cover (HC and Thunderspy author no Genesis
/// recipes, so the dormant slot never grows a crafting tab).
fn crafted_slots(database: &Db) -> Vec<&IncarnateSlotCatalog> {
    database
        .0
        .offered_incarnate_slots()
        .into_iter()
        .filter(|slot| {
            slot.powers.iter().any(|p| {
                database
                    .0
                    .incarnate_crafting
                    .recipes_for(&p.full_name)
                    .iter()
                    .any(|r| r.family == RecipeFamily::Current)
            })
        })
        .collect()
}

/// Threads-and-merits totals over a salvage need map. Each item counts toward
/// its own store row: the thread line where it has one, else the empyrean
/// line; an unpriced item (legacy drops) contributes to neither.
fn currency_totals(database: &Db, needed: &BTreeMap<String, u32>) -> (u32, u32) {
    let mut threads = 0;
    let mut empyrean = 0;
    for (id, count) in needed {
        let Some(salvage) = database.0.incarnate_crafting.salvage(id) else {
            continue; // unresolvable ids already fail loud in the rows
        };
        let price = |currency: &str| {
            salvage
                .buy
                .iter()
                .find(|b| b.currency == currency)
                .map(|b| b.amount * count)
        };
        if let Some(cost) = price(THREAD_CURRENCY) {
            threads += cost;
        } else if let Some(cost) = price(EMPYREAN_CURRENCY) {
            empyrean += cost;
        }
    }
    (threads, empyrean)
}

/// The equipped pick's catalog power for a slot, if both exist.
fn equipped_full_name(slot: &IncarnateSlotCatalog, build: &CharacterState) -> Option<String> {
    let pick = build.incarnates.get(&slot.id)?;
    slot.find_power(&pick.power_name)
        .map(|p| p.full_name.clone())
}

/// Salvage still needed across every equipped slot's tree, obtained nodes pruned.
fn all_slots_remaining(database: &Db, build: &CharacterState) -> BTreeMap<String, u32> {
    let mut needed = BTreeMap::new();
    for slot in crafted_slots(database) {
        let Some(full_name) = equipped_full_name(slot, build) else {
            continue;
        };
        if let Ok(tree) = database.0.incarnate_crafting.craft_tree(slot, &full_name) {
            tree.remaining_salvage(&|key| build.crafting_obtained.contains(key), &mut needed);
        }
        // An Err tree renders its marker in the slot view; the aggregate
        // simply doesn't include what cannot be assembled.
    }
    needed
}

/// Every node key in a tree — what "clear checklist" for one slot removes.
fn tree_keys(node: &CraftNode, out: &mut Vec<String>) {
    out.push(node.key.clone());
    for child in &node.children {
        tree_keys(child, out);
    }
}

/// The one crafting modal for the whole app. Renders nothing while closed.
#[component]
pub fn IncarnateCraftingHost(database: Db) -> Element {
    let open = use_context::<IncarnateCraftingOpen>().0;
    match open() {
        None => rsx! {},
        Some(tab) => rsx! {
            IncarnateCraftingModal { database, tab }
        },
    }
}

#[component]
fn IncarnateCraftingModal(database: Db, tab: CraftingTab) -> Element {
    let mut open = use_context::<IncarnateCraftingOpen>().0;
    let session = use_context::<BuildSession>();
    let build = session.build;

    let slots = crafted_slots(&database);
    if slots.is_empty() {
        return rsx! {
            Modal {
                title: "Incarnate Crafting".to_string(),
                size: ModalSize::Lg,
                on_close: move |_| open.set(None),
                p { class: "hint", "This dataset carries no incarnate crafting recipes." }
            }
        };
    }

    let tabs: Vec<(String, String, bool)> = slots
        .iter()
        .map(|slot| {
            let equipped = build.read().incarnates.get(&slot.id).is_some();
            (slot.id.clone(), slot.display_name.clone(), equipped)
        })
        .collect();
    let is_shopping = tab == CraftingTab::Shopping;

    rsx! {
        Modal {
            title: "Incarnate Crafting".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| open.set(None),
            div { class: "incarnate-craft",
                div { class: "incarnate-craft__tabs",
                    for (id, label, equipped) in tabs {
                        {
                            let active = tab == CraftingTab::Slot(id.clone());
                            let target = id.clone();
                            rsx! {
                                button {
                                    key: "{id}",
                                    class: "incarnate-craft__tab",
                                    class: if active { "is-active" } else { "" },
                                    onclick: move |_| open.set(Some(CraftingTab::Slot(target.clone()))),
                                    span { "{label}" }
                                    if equipped {
                                        span { class: "incarnate-tab__equipped-dot" }
                                    }
                                }
                            }
                        }
                    }
                    span { class: "incarnate-craft__tab-divider" }
                    button {
                        class: "incarnate-craft__tab",
                        class: if is_shopping { "is-active" } else { "" },
                        onclick: move |_| open.set(Some(CraftingTab::Shopping)),
                        "Shopping list"
                    }
                }
                match &tab {
                    CraftingTab::Shopping => rsx! {
                        ShoppingList { database: database.clone() }
                    },
                    CraftingTab::Slot(slot_id) => rsx! {
                        SlotCrafting { database: database.clone(), slot_id: slot_id.clone() }
                    },
                }
            }
        }
    }
}

#[component]
fn SlotCrafting(database: Db, slot_id: String) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut picker = use_context::<crate::panels::incarnate_picker::IncarnatePickerOpen>().0;
    let mut crafting_open = use_context::<IncarnateCraftingOpen>().0;

    let db = database.clone();
    let Some(slot) =
        db.0.offered_incarnate_slots()
            .into_iter()
            .find(|s| s.id == slot_id)
    else {
        return rsx! {
            div { class: "load-state error", "Slot {slot_id} is not offered by this dataset." }
        };
    };

    let Some(full_name) = equipped_full_name(slot, &build.read()) else {
        let open_slot = slot_id.clone();
        return rsx! {
            div { class: "incarnate-craft__empty",
                p { class: "hint", "Nothing equipped in this slot." }
                button {
                    class: "incarnate-craft__pick",
                    onclick: move |_| {
                        crafting_open.set(None);
                        picker.set(Some(open_slot.clone()));
                    },
                    "Choose a power"
                }
            }
        };
    };

    let tree = match db.0.incarnate_crafting.craft_tree(slot, &full_name) {
        Ok(tree) => tree,
        // Rule 1: an unassemblable tree is a visible defect for this pick, not
        // a blank pane — the modal keeps working for the other slots.
        Err(detail) => {
            return rsx! {
                div { class: "load-state error", "Craft tree for {full_name}: {detail}" }
            };
        }
    };

    let goal_display = slot
        .find_power(full_name.rsplit('.').next().unwrap_or(&full_name))
        .map(|p| p.display_name.clone())
        .unwrap_or_else(|| full_name.clone());

    let state = build.read();
    let obtained = state.crafting_obtained.clone();
    let mut node_only = BTreeMap::new();
    for s in &tree.salvage {
        *node_only.entry(s.id.clone()).or_insert(0) += s.amount;
    }
    let mut remaining = BTreeMap::new();
    tree.remaining_salvage(&|key| obtained.contains(key), &mut remaining);
    drop(state);

    let fully_crafted = remaining.is_empty();
    let clear_keys = {
        let mut keys = Vec::new();
        tree_keys(&tree, &mut keys);
        keys
    };

    rsx! {
        div { class: "incarnate-craft__body",
            div { class: "incarnate-craft__goal",
                span { class: "incarnate-craft__goal-label", "Crafting path to" }
                span { class: "incarnate-craft__goal-name", "{goal_display}" }
                if fully_crafted {
                    span { class: "incarnate-craft__done", "✓ fully crafted" }
                }
            }
            p { class: "incarnate-craft__hint",
                "A Very Rare consumes both of its branch's Rares, each with its own lower tiers. "
                "Marking a node you already own excludes it from costs calculations."
            }
            CostSummary {
                database: database.clone(),
                node_only,
                remaining,
            }
            CraftNodeView {
                database: database.clone(),
                slot_id: slot.id.clone(),
                node: tree,
                depth: 0,
                ancestor_obtained: false,
            }
            div { class: "incarnate-craft__footer",
                button {
                    class: "incarnate-craft__clear",
                    onclick: move |_| {
                        let keys = clear_keys.clone();
                        session.commit(move |state| {
                            for key in &keys {
                                state.crafting_obtained.remove(key);
                                state
                                    .crafting_salvage_checked
                                    .retain(|entry| !entry.starts_with(&format!("{key}:")));
                            }
                        });
                    },
                    "Clear this slot's checklist"
                }
            }
        }
    }
}

/// One rung of the craft tree: obtained checkbox, the power's name, its
/// salvage (collapsible), then the rungs it consumes.
#[component]
fn CraftNodeView(
    database: Db,
    slot_id: String,
    node: CraftNode,
    depth: usize,
    ancestor_obtained: bool,
) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut collapsed = use_signal(|| true);

    let state = build.read();
    let self_obtained = state.crafting_obtained.contains(&node.key);
    let checked: Vec<bool> = node
        .salvage
        .iter()
        .map(|s| {
            state
                .crafting_salvage_checked
                .contains(&format!("{}:{}", node.key, s.id))
        })
        .collect();
    drop(state);
    let obtained = ancestor_obtained || self_obtained;

    let db = database.0.clone();
    let slot = db
        .offered_incarnate_slots()
        .into_iter()
        .find(|s| s.id == slot_id);
    let internal = node.power.rsplit('.').next().unwrap_or(&node.power);
    let power = slot.and_then(|s| s.find_power(internal));
    let display = power
        .map(|p| p.display_name.clone())
        .unwrap_or_else(|| node.power.clone());
    let tier_token = power.map(|p| p.tier().icon_token()).unwrap_or("common");

    let toggle_key = node.key.clone();
    let has_salvage = !node.salvage.is_empty();
    let show_salvage = !collapsed() && !obtained && has_salvage;

    rsx! {
        div {
            class: "incarnate-craft-node incarnate-craft-node--{tier_token}",
            class: if depth > 0 { "is-nested" } else { "" },
            div {
                class: "incarnate-craft-node__row",
                class: if obtained { "is-obtained" } else { "" },
                input {
                    r#type: "checkbox",
                    class: "incarnate-craft-node__check",
                    checked: self_obtained,
                    disabled: ancestor_obtained,
                    title: if ancestor_obtained {
                        "Consumed by a higher rung you already own"
                    } else {
                        "Mark as already crafted — it and its ingredients drop from the costs"
                    },
                    onchange: move |_| {
                        let key = toggle_key.clone();
                        session.commit(move |state| {
                            if !state.crafting_obtained.remove(&key) {
                                state.crafting_obtained.insert(key);
                            }
                        });
                    },
                }
                button {
                    class: "incarnate-craft-node__name",
                    disabled: obtained || !has_salvage,
                    onclick: move |_| collapsed.set(!collapsed()),
                    span { "{display}" }
                    if ancestor_obtained {
                        span { class: "incarnate-craft-node__mark", "consumed" }
                    } else if self_obtained {
                        span { class: "incarnate-craft-node__mark", "owned" }
                    }
                }
            }
            if show_salvage {
                div { class: "incarnate-craft-node__salvage",
                    for (index, s) in node.salvage.iter().enumerate() {
                        SalvageRow {
                            key: "{s.id}",
                            database: database.clone(),
                            check_key: format!("{}:{}", node.key, s.id),
                            id: s.id.clone(),
                            amount: s.amount,
                            checked: checked[index],
                        }
                    }
                }
            }
            for child in node.children {
                CraftNodeView {
                    key: "{child.key}",
                    database: database.clone(),
                    slot_id: slot_id.clone(),
                    node: child,
                    depth: depth + 1,
                    ancestor_obtained: obtained,
                }
            }
        }
    }
}

/// One salvage requirement line: checkbox, count, name, its store price.
#[component]
fn SalvageRow(database: Db, check_key: String, id: String, amount: u32, checked: bool) -> Element {
    let session = use_context::<BuildSession>();
    let salvage = database.0.incarnate_crafting.salvage(&id).cloned();

    let (label, rarity_token, price) = match &salvage {
        Some(s) => (s.display_name.clone(), s.rarity.icon_token(), buy_text(s)),
        // Rule 1: an id the section can't resolve renders as itself with an
        // error mark, never a silent skip.
        None => (format!("{id} (unresolved)"), "common", String::new()),
    };
    let is_error = salvage.is_none();

    rsx! {
        label {
            class: "incarnate-craft-salvage incarnate-craft-salvage--{rarity_token}",
            class: if checked { "is-checked" } else { "" },
            class: if is_error { "load-state error" } else { "" },
            input {
                r#type: "checkbox",
                checked,
                onchange: move |_| {
                    let key = check_key.clone();
                    session.commit(move |state| {
                        if !state.crafting_salvage_checked.remove(&key) {
                            state.crafting_salvage_checked.insert(key);
                        }
                    });
                },
            }
            span { class: "incarnate-craft-salvage__name",
                if amount > 1 { "{amount} × " }
                "{label}"
            }
            if !price.is_empty() {
                span { class: "incarnate-craft-salvage__price", "{price}" }
            }
        }
    }
}

/// The store rows for one salvage, as authored: "20 Incarnate Threads each".
fn buy_text(salvage: &CraftSalvage) -> String {
    salvage
        .buy
        .iter()
        .map(|b| format!("{} {}", b.amount, b.currency))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The two-column cost block: the goal's own combine vs everything remaining.
#[component]
fn CostSummary(
    database: Db,
    node_only: BTreeMap<String, u32>,
    remaining: BTreeMap<String, u32>,
) -> Element {
    let columns = [("Final combine", &node_only), ("Remaining", &remaining)];
    rsx! {
        div { class: "incarnate-craft-summary",
            for (title, needed) in columns {
                {
                    let (threads, empyrean) = currency_totals(&database, needed);
                    let rows = sorted_needed(&database, needed);
                    rsx! {
                        div { key: "{title}", class: "incarnate-craft-summary__column",
                            span { class: "incarnate-craft-summary__title", "{title}" }
                            div { class: "incarnate-craft-summary__currency",
                                span { "{threads} Threads" }
                                span { "{empyrean} Empyrean Merits" }
                            }
                            for (id, count) in rows {
                                {
                                    let (label, rarity_token) = salvage_label(&database, &id);
                                    rsx! {
                                        span {
                                            key: "{id}",
                                            class: "incarnate-craft-summary__row incarnate-craft-salvage--{rarity_token}",
                                            "{count} × {label}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Needed salvage as (id, count), rarity-major then alphabetical — the buy
/// order a player runs the store in. Unresolvable ids sort last.
fn sorted_needed(database: &Db, needed: &BTreeMap<String, u32>) -> Vec<(String, u32)> {
    let mut rows: Vec<(String, u32)> = needed.iter().map(|(k, v)| (k.clone(), *v)).collect();
    rows.sort_by_key(|(id, _)| match database.0.incarnate_crafting.salvage(id) {
        Some(s) => (false, Some(s.rarity), s.display_name.clone()),
        None => (true, None, id.clone()),
    });
    rows
}

/// Display name + rarity colour token for a salvage id; an unresolvable id
/// labels as itself, marked (Rule 1's visible form).
fn salvage_label(database: &Db, id: &str) -> (String, &'static str) {
    match database.0.incarnate_crafting.salvage(id) {
        Some(s) => (s.display_name.clone(), s.rarity.icon_token()),
        None => (format!("{id} (unresolved)"), "common"),
    }
}

/// The consolidated list across every equipped slot. Left-click marks one
/// acquired, right-click un-marks; acquisition persists with the build.
#[component]
fn ShoppingList(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;

    let state = build.read();
    let needed = all_slots_remaining(&database, &state);
    let acquired_map = state.shopping_acquired.clone();
    drop(state);

    if needed.is_empty() {
        return rsx! {
            div { class: "incarnate-craft__empty",
                p { class: "hint",
                    "Nothing to buy. Equip incarnate powers (or mark nodes crafted) and the "
                    "required salvage list will appear."
                }
            }
        };
    }

    // Currency totals count what is still un-acquired.
    let mut outstanding = BTreeMap::new();
    for (id, count) in &needed {
        let acquired = acquired_map.get(id).copied().unwrap_or(0).min(*count);
        if *count > acquired {
            outstanding.insert(id.clone(), count - acquired);
        }
    }
    let (threads, empyrean) = currency_totals(&database, &outstanding);
    let rows = sorted_needed(&database, &needed);

    rsx! {
        div { class: "incarnate-craft__body",
            p { class: "incarnate-craft__hint",
                "Click an item to mark one acquired · right-click to undo"
            }
            div { class: "incarnate-craft-summary__currency",
                span { "{threads} Threads" }
                span { "{empyrean} Empyrean Merits" }
            }
            div { class: "incarnate-craft-shopping",
                for (id, count) in rows {
                    {
                        let acquired = acquired_map.get(&id).copied().unwrap_or(0).min(count);
                        let done = acquired >= count;
                        let (label, rarity_token) = salvage_label(&database, &id);
                        let click_id = id.clone();
                        let context_id = id.clone();
                        rsx! {
                            div {
                                key: "{id}",
                                class: "incarnate-craft-shopping__row incarnate-craft-salvage--{rarity_token}",
                                class: if done { "is-done" } else { "" },
                                onclick: move |_| {
                                    let id = click_id.clone();
                                    session.commit(move |state| {
                                        *state.shopping_acquired.entry(id).or_insert(0) += 1;
                                    });
                                },
                                oncontextmenu: move |evt| {
                                    evt.prevent_default();
                                    let id = context_id.clone();
                                    session.commit(move |state| {
                                        if let Some(n) = state.shopping_acquired.get_mut(&id) {
                                            *n = n.saturating_sub(1);
                                            if *n == 0 {
                                                state.shopping_acquired.remove(&id);
                                            }
                                        }
                                    });
                                },
                                span { class: "incarnate-craft-shopping__count",
                                    if done { "✓" } else { "{count - acquired} ×" }
                                }
                                span { class: "incarnate-craft-salvage__name", "{label}" }
                                if acquired > 0 && !done {
                                    span { class: "incarnate-craft-shopping__progress",
                                        "({acquired}/{count})"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "incarnate-craft__footer",
                button {
                    class: "incarnate-craft__clear",
                    onclick: move |_| {
                        session.commit(|state| state.shopping_acquired.clear());
                    },
                    "Reset acquired"
                }
            }
        }
    }
}
