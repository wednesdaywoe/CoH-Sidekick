//! Compare Slotting — alternative enhancement configurations for one power, read against the
//! live build and applied in one commit (the beta `CompareSlottingModal`, in its engine-backed
//! form).
//!
//! The modal owns no arithmetic. Every number on its stats side comes from one
//! [`coh_math::recalculate`] over a HYPOTHETICAL build — the live build with the target power's
//! slots swapped for the row under the cursor — read through the same
//! [`PowerView`](crate::view::power_view::PowerView) the Info panel renders and the same stat
//! registry the dashboard reads. A comparison can therefore only rank slottings by the numbers
//! the rest of the app would show for them; it has no arithmetic of its own to disagree with.
//!
//! Row 0 ("Current") mirrors the build's actual slotting and is never stored: scratch edits to
//! it survive only while the build's real slotting stays what the scratch was seeded from, so
//! the row can never silently disagree with the build it names. The saved rows are
//! session-scoped on purpose (the beta's reasoning, kept): a persisted copy would outlive the
//! build it describes and surface against a dataset whose enhancements do not resolve.
//! Surviving close/reopen and power switching is the part that matters.

use std::collections::BTreeMap;

use coh_data::{power_address, CharacterState, Enhancement, EnhancementKind};
use dioxus::prelude::*;

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::panels::adjusters::PowerAdjusters;
use crate::panels::dashboards::DashboardConfig;
use crate::panels::info::PowerViewCard;
use crate::panels::powers::{
    build_set_bonus_block, enhancement_type_class, slot_chip_label, slot_icon_url,
    slot_indicator_style, slot_overlay_url, PickerDestination, PickerOpen, PickerTarget,
    SetBonusBlock, SetBonusTierList,
};
use crate::panels::stat_registry;
use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use crate::view::power_view;

/// Row 0 — the live mirror of the build's actual slotting. Never stored.
pub const CURRENT_COPY_ID: u32 = 0;

/// One comparison row: a full slot loadout for the target power. Row 0 is derived
/// ([`CompareSlottingState::rows`]); saved rows carry ids from 1 up.
#[derive(Clone, PartialEq)]
pub struct ComparisonCopy {
    pub id: u32,
    pub slots: Vec<Option<Enhancement>>,
}

/// The power under comparison, addressed the way the build addresses a pick.
#[derive(Clone, PartialEq)]
pub struct CompareTarget {
    pub powerset_id: String,
    pub internal_name: String,
}

/// Row 0's unsaved edits. Valid only while the build's real slotting still equals
/// `seeded_from` — any build edit to the power (applying a row, slotting it on the card,
/// undo) invalidates the scratch and the mirror re-seeds, which is what keeps "Current"
/// honest about the name it wears.
struct CurrentScratch {
    address: String,
    seeded_from: Vec<Option<Enhancement>>,
    slots: Vec<Option<Enhancement>>,
}

/// The modal's whole state: the open flag, the power under comparison, the saved rows per
/// power address, row 0's scratch, and the hover/applied row marks.
#[derive(Default)]
pub struct CompareSlottingState {
    pub open: bool,
    pub target: Option<CompareTarget>,
    saved: BTreeMap<String, Vec<ComparisonCopy>>,
    scratch: Option<CurrentScratch>,
    pub hovered: Option<u32>,
    pub applied: Option<u32>,
}

/// Context handle, provided once at the shell root (like
/// [`crate::panels::powers::PickerOpen`]).
#[derive(Clone, Copy)]
pub struct CompareSlottingStore(pub Signal<CompareSlottingState>);

/// Force a row to the power's current slot count. A row is a fixed-length loadout captured
/// when it was made; the user can add or remove slots on the real power afterwards, and a
/// mismatched length would otherwise render the wrong number of circles and let Apply write
/// past the end of the power.
pub fn reconcile_length(
    slots: &[Option<Enhancement>],
    slot_count: usize,
) -> Vec<Option<Enhancement>> {
    let mut out: Vec<Option<Enhancement>> = slots.iter().take(slot_count).cloned().collect();
    out.resize(slot_count, None);
    out
}

/// The live build with the target power's slots swapped for `slots` — everything the stats
/// side reads is a [`coh_math::recalculate`] over this. Pure so the swap's reach is gradeable:
/// exactly one power moves, every other selection is untouched.
pub fn hypothetical_build(
    build: &CharacterState,
    target: &CompareTarget,
    slots: &[Option<Enhancement>],
) -> CharacterState {
    let mut hypothetical = build.clone();
    if let Some(power) = hypothetical.selected_power_mut(&target.powerset_id, &target.internal_name)
    {
        let count = power.slots.len();
        power.slots = reconcile_length(slots, count);
    }
    hypothetical
}

/// Land a row on the real power. The row is reconciled to the power's own slot count, so an
/// out-of-date row can never grow or shrink the power it lands on. Called inside one
/// [`BuildSession::commit`], so applying is one undo step.
pub fn apply_copy(
    state: &mut CharacterState,
    target: &CompareTarget,
    slots: &[Option<Enhancement>],
) {
    if let Some(power) = state.selected_power_mut(&target.powerset_id, &target.internal_name) {
        let count = power.slots.len();
        power.slots = reconcile_length(slots, count);
    }
}

impl CompareSlottingState {
    /// Open the modal on whatever target it last held (the quickbar's way in — the selector
    /// inside picks the power). Row 0's scratch is dropped so the mirror starts each visit at
    /// what the build actually holds.
    pub fn open(&mut self) {
        self.open = true;
        self.scratch = None;
        self.hovered = None;
        self.applied = None;
    }

    /// Aim the modal at one power. Hover and applied marks are about rows of the old power,
    /// so they mean nothing across the switch; the saved rows are keyed per power and keep.
    pub fn set_target(&mut self, target: CompareTarget) {
        self.target = Some(target);
        self.scratch = None;
        self.hovered = None;
        self.applied = None;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.scratch = None;
        self.hovered = None;
        self.applied = None;
    }

    /// The rows as rendered: "Current" first (scratch if still valid, else the build's own
    /// slots), then the saved rows — every one forced to the power's present slot count, so
    /// each consumer below sees correct-length loadouts.
    pub fn rows(&self, address: &str, build_slots: &[Option<Enhancement>]) -> Vec<ComparisonCopy> {
        let current = self
            .scratch
            .as_ref()
            .filter(|scratch| scratch.address == address && scratch.seeded_from == build_slots)
            .map(|scratch| scratch.slots.clone())
            .unwrap_or_else(|| build_slots.to_vec());
        let mut rows = vec![ComparisonCopy {
            id: CURRENT_COPY_ID,
            slots: reconcile_length(&current, build_slots.len()),
        }];
        if let Some(saved) = self.saved.get(address) {
            rows.extend(saved.iter().map(|copy| ComparisonCopy {
                id: copy.id,
                slots: reconcile_length(&copy.slots, build_slots.len()),
            }));
        }
        rows
    }

    /// Write one slot of one row. Row 0 goes to the scratch (seeded from the build's real
    /// slots first, if stale); a saved row is edited in place — and every saved row is
    /// reconciled on the same write, so a stale-length row is repaired the first time the
    /// list is touched rather than written back short.
    pub fn edit_slot(
        &mut self,
        address: &str,
        build_slots: &[Option<Enhancement>],
        copy_id: u32,
        slot_index: usize,
        piece: Option<Enhancement>,
    ) {
        if copy_id == CURRENT_COPY_ID {
            let stale = self.scratch.as_ref().is_none_or(|scratch| {
                scratch.address != address || scratch.seeded_from != build_slots
            });
            if stale {
                self.scratch = Some(CurrentScratch {
                    address: address.to_string(),
                    seeded_from: build_slots.to_vec(),
                    slots: build_slots.to_vec(),
                });
            }
            if let Some(slot) = self
                .scratch
                .as_mut()
                .and_then(|scratch| scratch.slots.get_mut(slot_index))
            {
                *slot = piece;
            }
            return;
        }
        let Some(saved) = self.saved.get_mut(address) else {
            return;
        };
        for copy in saved.iter_mut() {
            copy.slots = reconcile_length(&copy.slots, build_slots.len());
        }
        if let Some(slot) = saved
            .iter_mut()
            .find(|copy| copy.id == copy_id)
            .and_then(|copy| copy.slots.get_mut(slot_index))
        {
            *slot = piece;
        }
    }

    /// Append an empty row.
    pub fn add_copy(&mut self, address: &str, slot_count: usize) {
        let id = self.next_id(address);
        self.saved
            .entry(address.to_string())
            .or_default()
            .push(ComparisonCopy {
                id,
                slots: vec![None; slot_count],
            });
    }

    /// Duplicate a row directly below itself. Duplicating "Current" has no predecessor among
    /// the saved rows, so it lands at their front — still directly below the row it came from.
    pub fn duplicate_copy(
        &mut self,
        address: &str,
        build_slots: &[Option<Enhancement>],
        copy_id: u32,
    ) {
        let Some(source) = self
            .rows(address, build_slots)
            .into_iter()
            .find(|copy| copy.id == copy_id)
        else {
            return;
        };
        let id = self.next_id(address);
        let saved = self.saved.entry(address.to_string()).or_default();
        for copy in saved.iter_mut() {
            copy.slots = reconcile_length(&copy.slots, build_slots.len());
        }
        let insert_at = match copy_id == CURRENT_COPY_ID {
            true => 0,
            false => saved
                .iter()
                .position(|copy| copy.id == copy_id)
                .map_or(saved.len(), |index| index + 1),
        };
        saved.insert(
            insert_at,
            ComparisonCopy {
                id,
                slots: source.slots,
            },
        );
    }

    /// Remove a saved row (never the live "Current" row); the power's entry goes with its
    /// last row, so an untouched power holds nothing here.
    pub fn remove_copy(&mut self, address: &str, copy_id: u32) {
        if copy_id == CURRENT_COPY_ID {
            return;
        }
        let Some(saved) = self.saved.get_mut(address) else {
            return;
        };
        saved.retain(|copy| copy.id != copy_id);
        if saved.is_empty() {
            self.saved.remove(address);
        }
    }

    /// Ids only have to be unique within one power's list — max over the saved rows plus one,
    /// never a counter that resets and collides with rows that outlived a close.
    fn next_id(&self, address: &str) -> u32 {
        self.saved
            .get(address)
            .into_iter()
            .flatten()
            .map(|copy| copy.id)
            .max()
            .unwrap_or(CURRENT_COPY_ID)
            + 1
    }
}

/// The one Compare Slotting modal, hosted at the shell root — OUTSIDE the free grid's
/// `transform`ed surfaces — for the same containment reason as every other overlay, and
/// BEFORE the enhancement picker host in the DOM, so the picker a row opens paints above
/// this modal. Renders nothing while closed.
#[component]
pub fn CompareSlottingHost(database: Db) -> Element {
    let mut store = use_context::<CompareSlottingStore>().0;
    if !store.read().open {
        return rsx! {};
    }
    let title = match &store.read().target {
        Some(target) => format!(
            "Compare Slotting — {}",
            crate::naming::power_label(Some(&database), &target.powerset_id, &target.internal_name)
        ),
        None => "Compare Slotting".to_string(),
    };
    rsx! {
        Modal { title, size: ModalSize::Xl, on_close: move |_| store.write().close(),
            CompareSlottingBody { database }
        }
    }
}

/// One row of the Stats Impact table, precomputed so the grid markup stays flat. `delta` is
/// `None` below the display epsilon — shown as an em dash, matching the beta.
#[derive(Clone, PartialEq)]
struct ImpactRow {
    label: &'static str,
    current: String,
    hypothetical: String,
    delta: Option<f64>,
}

/// One set represented in the active row with enough same-set pieces for a bonus.
#[derive(Clone, PartialEq)]
struct CopySetBlock {
    set_name: String,
    block: SetBonusBlock,
}

/// One entry of the power selector.
struct SelectorEntry {
    value: String,
    label: String,
}

#[component]
fn CompareSlottingBody(database: Db) -> Element {
    let mut store = use_context::<CompareSlottingStore>().0;
    let session = use_context::<BuildSession>();
    let live_totals = use_context::<BuildTotals>().0;
    let stat_config = use_context::<DashboardConfig>().0;

    // The hypothetical build and its totals for the row under the cursor. One engine run
    // serves the effect rows, the Stats Impact deltas and the set-bonus tiers below — the
    // same single-run rule the shell's `BuildTotals` memo follows, one hover deep.
    let hypothetical: Memo<Option<(u32, CharacterState, coh_math::CalculatedTotals)>> =
        use_memo(use_reactive!(|database| {
            let state = store.read();
            let target = state.target.clone()?;
            let build = session.build.read();
            let selected = build.selected_power(&target.powerset_id, &target.internal_name)?;
            let address = power_address(&target.powerset_id, &target.internal_name);
            let rows = state.rows(&address, &selected.slots);
            let active = state
                .hovered
                .and_then(|id| rows.iter().find(|copy| copy.id == id))
                .unwrap_or(&rows[0]);
            let hypothetical = hypothetical_build(&build, &target, &active.slots);
            let totals = coh_math::recalculate(&hypothetical, &database);
            Some((active.id, hypothetical, totals))
        }));

    let state = store.read();
    let build = session.build.read();

    // The selector lists every picked power, grouped the way the build groups them. It is
    // always shown — it is both the way in from the quickbar and the way to switch powers.
    let mut groups: Vec<(String, Vec<SelectorEntry>)> = Vec::new();
    {
        let mut push_group = |name: &str, powers: &[coh_data::SelectedPower]| {
            let entries: Vec<SelectorEntry> = powers
                .iter()
                .map(|power| SelectorEntry {
                    value: format!("{}::{}", power.powerset, power.internal_name),
                    label: crate::naming::power_label(
                        Some(&database),
                        &power.powerset,
                        &power.internal_name,
                    ),
                })
                .collect();
            if !entries.is_empty() {
                groups.push((name.to_string(), entries));
            }
        };
        push_group(&build.primary.name, &build.primary.powers);
        push_group(&build.secondary.name, &build.secondary.powers);
        for pool in &build.pools {
            push_group(&pool.name, &pool.powers);
        }
        if let Some(epic) = &build.epic_pool {
            push_group(&epic.name, &epic.powers);
        }
        let slotted_inherents: Vec<coh_data::SelectedPower> = build
            .inherents
            .iter()
            .filter(|power| !power.slots.is_empty())
            .cloned()
            .collect();
        push_group("Inherent", &slotted_inherents);
    }
    let selector_value = state
        .target
        .as_ref()
        .map(|target| format!("{}::{}", target.powerset_id, target.internal_name))
        .unwrap_or_default();
    let selector = rsx! {
        div { class: "cmp-picker",
            label { class: "cmp-picker__label", "Power" }
            select {
                class: "cmp-picker__select",
                value: "{selector_value}",
                onchange: move |evt| {
                    let value = evt.value();
                    if let Some((powerset_id, internal_name)) = value.split_once("::") {
                        store.write().set_target(CompareTarget {
                            powerset_id: powerset_id.to_string(),
                            internal_name: internal_name.to_string(),
                        });
                    }
                },
                option { value: "", disabled: true, selected: selector_value.is_empty(), "Select a power…" }
                for (group_name , entries) in &groups {
                    optgroup { label: "{group_name}",
                        for entry in entries {
                            option { value: "{entry.value}", "{entry.label}" }
                        }
                    }
                }
            }
        }
    };

    let Some(target) = state.target.clone() else {
        return rsx! {
            div { class: "cmp-slotting cmp-slotting--empty",
                {selector}
                p { class: "hint", "Pick a power to compare slotting configurations for it." }
            }
        };
    };
    // A target the build no longer holds (removed while open, or a dataset switch) is said,
    // not silently blanked (Rule 1) — the selector above is the way back.
    let Some(selected) = build.selected_power(&target.powerset_id, &target.internal_name) else {
        return rsx! {
            div { class: "cmp-slotting cmp-slotting--empty",
                {selector}
                p { class: "load-state error", "This power is no longer in the build." }
            }
        };
    };

    let address = power_address(&target.powerset_id, &target.internal_name);
    let rows = state.rows(&address, &selected.slots);
    let slot_count = selected.slots.len();
    let hovered = state.hovered;
    let applied = state.applied;

    // Everything the stats side shows, derived from the one hypothetical run.
    let hypothetical_value = hypothetical.read();
    let stats_side = match &*hypothetical_value {
        Some((active_id, hypothetical_build, hypothetical_totals)) => {
            let showing = match rows.iter().position(|copy| copy.id == *active_id) {
                Some(0) | None => "Current".to_string(),
                Some(index) => format!("Copy {index}"),
            };
            let view = power_view::view_for(
                &database,
                hypothetical_build,
                hypothetical_totals,
                &target.powerset_id,
                &target.internal_name,
            );

            let visible = stat_config.read();
            let current_totals = live_totals.read();
            let impact: Vec<ImpactRow> = stat_registry::ALL
                .iter()
                .filter(|def| visible.shows(def.id))
                .map(|def| {
                    let delta = (def.read)(hypothetical_totals) - (def.read)(&current_totals);
                    ImpactRow {
                        label: def.label,
                        current: def.resolve(&current_totals).text,
                        hypothetical: def.resolve(hypothetical_totals).text,
                        delta: (delta.abs() >= 0.005).then_some(delta),
                    }
                })
                .collect();

            // The sets the active row slots, graded against the HYPOTHETICAL totals' Rule-of-5
            // tracking — the tiers say what the build would get with this row applied, not what
            // the current build happens to cap.
            let mut set_blocks: Vec<CopySetBlock> = Vec::new();
            if let (Some(catalog), Some(power)) = (
                database.io_sets.as_ref(),
                hypothetical_build.selected_power(&target.powerset_id, &target.internal_name),
            ) {
                let mut seen: Vec<&str> = Vec::new();
                for piece in power.slots.iter().flatten() {
                    if let EnhancementKind::IoSet { set_id, .. } = &piece.kind {
                        if !seen.contains(&set_id.as_str()) {
                            seen.push(set_id);
                        }
                    }
                }
                for set_id in seen {
                    let Some(set) = catalog.get(set_id) else {
                        continue;
                    };
                    let block = build_set_bonus_block(
                        set,
                        power,
                        set_id,
                        &hypothetical_totals.set_bonus_tracking,
                    );
                    // A lone piece reaches no tier — the compare table only lists sets whose
                    // bonuses are in play.
                    if block.slotted >= 2 {
                        set_blocks.push(CopySetBlock {
                            set_name: set.name.clone(),
                            block,
                        });
                    }
                }
            }

            rsx! {
                div { class: "cmp-stats",
                    div { class: "cmp-stats__showing", "Showing: {showing}" }
                    if !impact.is_empty() {
                        div { class: "cmp-stats__section", "Stats impact" }
                        div { class: "cmp-impact",
                            for row in &impact {
                                span { key: "{row.label}", class: "cmp-impact__label", "{row.label}" }
                                span { class: "cmp-impact__current", "{row.current}" }
                                span { class: "cmp-impact__arrow", "→" }
                                span { class: "cmp-impact__next", "{row.hypothetical}" }
                                match row.delta {
                                    None => rsx! {
                                        span { class: "cmp-impact__delta", "—" }
                                    },
                                    Some(delta) if delta > 0.0 => rsx! {
                                        span { class: "cmp-impact__delta cmp-impact__delta--up", "+{delta:.2}" }
                                    },
                                    Some(delta) => rsx! {
                                        span { class: "cmp-impact__delta cmp-impact__delta--down", "{delta:.2}" }
                                    },
                                }
                            }
                        }
                    }
                    if !set_blocks.is_empty() {
                        div { class: "cmp-stats__section", "Set bonuses" }
                        for entry in &set_blocks {
                            div { key: "{entry.set_name}", class: "cmp-set",
                                div { class: "cmp-set__head",
                                    "{entry.set_name} ({entry.block.slotted}/{entry.block.total_pieces})"
                                }
                                SetBonusTierList { tiers: entry.block.tiers.clone() }
                            }
                        }
                    }
                    match view {
                        Some(view) => rsx! {
                            PowerViewCard { view, adjusters: PowerAdjusters::default() }
                        },
                        None => rsx! {
                            div { class: "load-state error",
                                "This power is not in the current dataset."
                            }
                        },
                    }
                }
            }
        }
        None => rsx! {
            div { class: "cmp-stats",
                div { class: "load-state error", "This power is no longer in the build." }
            }
        },
    };

    rsx! {
        div { class: "cmp-slotting",
            {selector}
            div { class: "cmp-slotting__panes",
                div { class: "cmp-rows",
                    for (index , row) in rows.iter().enumerate() {
                        CompareCopyRow {
                            key: "{row.id}",
                            database: database.clone(),
                            target: target.clone(),
                            copy: row.clone(),
                            index,
                            hovered,
                            applied,
                        }
                    }
                    button {
                        class: "cmp-add",
                        r#type: "button",
                        onclick: {
                            let address = address.clone();
                            move |_| store.write().add_copy(&address, slot_count)
                        },
                        "+ Add configuration"
                    }
                }
                {stats_side}
            }
        }
    }
}

/// One comparison row: its label, its actions and its slot circles. Hovering it aims the
/// stats side at this row; the handlers write the store (rows) or the session (Apply), never
/// both in one gesture.
#[component]
fn CompareCopyRow(
    database: Db,
    target: CompareTarget,
    copy: ComparisonCopy,
    index: usize,
    hovered: Option<u32>,
    applied: Option<u32>,
) -> Element {
    let mut store = use_context::<CompareSlottingStore>().0;
    let session = use_context::<BuildSession>();

    let copy_id = copy.id;
    let label = match copy_id == CURRENT_COPY_ID {
        true => "Current".to_string(),
        false => format!("Copy {index}"),
    };
    let class = if hovered == Some(copy_id) {
        "cmp-copy cmp-copy--active"
    } else if applied == Some(copy_id) {
        "cmp-copy cmp-copy--applied"
    } else {
        "cmp-copy"
    };
    let address = power_address(&target.powerset_id, &target.internal_name);

    // The build's REAL slots at gesture time — duplication reconciles against them, and they
    // are read at the event rather than captured, so a row edit between render and click can
    // never act on a stale mirror.
    let build_slots_now = {
        let target = target.clone();
        move || {
            session
                .build
                .peek()
                .selected_power(&target.powerset_id, &target.internal_name)
                .map(|power| power.slots.clone())
                .unwrap_or_default()
        }
    };

    let duplicate = {
        let address = address.clone();
        let build_slots_now = build_slots_now.clone();
        move |_| {
            store
                .write()
                .duplicate_copy(&address, &build_slots_now(), copy_id);
        }
    };
    let apply = {
        let target = target.clone();
        let slots = copy.slots.clone();
        move |_| {
            let target = target.clone();
            let slots = slots.clone();
            session.commit(move |state| apply_copy(state, &target, &slots));
            let mut state = store.write();
            state.applied = Some(copy_id);
            state.scratch = None;
        }
    };
    let remove = {
        let address = address.clone();
        move |_| store.write().remove_copy(&address, copy_id)
    };

    rsx! {
        div {
            class,
            onmouseenter: move |_| store.write().hovered = Some(copy_id),
            onmouseleave: move |_| {
                let mut state = store.write();
                if state.hovered == Some(copy_id) {
                    state.hovered = None;
                }
            },
            div { class: "cmp-copy__head",
                span { class: "cmp-copy__name", "{label}" }
                if applied == Some(copy_id) {
                    span { class: "cmp-copy__applied-mark", "applied" }
                }
                div { class: "cmp-copy__actions",
                    button {
                        class: "cmp-btn",
                        r#type: "button",
                        title: "Duplicate this configuration",
                        onclick: duplicate,
                        "Copy"
                    }
                    button {
                        class: "cmp-btn cmp-btn--apply",
                        r#type: "button",
                        title: "Apply this slotting to the build (one undo step)",
                        onclick: apply,
                        "Apply"
                    }
                    if copy_id != CURRENT_COPY_ID {
                        button {
                            class: "cmp-btn cmp-btn--remove",
                            r#type: "button",
                            title: "Remove this configuration",
                            onclick: remove,
                            "✕"
                        }
                    }
                }
            }
            div { class: "cmp-slots",
                for (slot_index , slot) in copy.slots.iter().enumerate() {
                    CompareSlotCell {
                        key: "{slot_index}",
                        database: database.clone(),
                        target: target.clone(),
                        copy_id,
                        slot_index,
                        enhancement: slot.clone(),
                    }
                }
            }
        }
    }
}

/// One slot circle of a comparison row. Click opens the shared enhancement picker aimed at
/// this row ([`PickerDestination::CompareCopy`]); right-click clears the row's slot. Neither
/// gesture can reach the build — that is Apply's job alone.
#[component]
fn CompareSlotCell(
    database: Db,
    target: CompareTarget,
    copy_id: u32,
    slot_index: usize,
    enhancement: Option<Enhancement>,
) -> Element {
    let mut store = use_context::<CompareSlottingStore>().0;
    let session = use_context::<BuildSession>();
    let picker_open = use_context::<PickerOpen>().0;

    let icon_url = enhancement
        .as_ref()
        .and_then(|piece| slot_icon_url(piece, &database));
    let indicator = enhancement
        .as_ref()
        .and_then(|piece| slot_indicator_style(piece, &database.procs));
    // The frame over the base icon — the same stack the build's slot cells and the picker
    // draw, so a piece reads the same rarity in the comparison rows.
    let character_origin = session.build.read().origin.clone();
    let frame_url = enhancement
        .as_ref()
        .map(|piece| slot_overlay_url(piece, &database, character_origin.as_deref()));

    let open_picker = {
        let target = target.clone();
        move |_| {
            let mut open = picker_open;
            open.set(Some(PickerTarget {
                powerset_id: target.powerset_id.clone(),
                power_internal_name: target.internal_name.clone(),
                slot_index: slot_index as u8,
                destination: PickerDestination::CompareCopy { copy_id },
            }));
        }
    };
    let clear = {
        let target = target.clone();
        move |evt: Event<MouseData>| {
            evt.prevent_default();
            let address = power_address(&target.powerset_id, &target.internal_name);
            let build_slots = session
                .build
                .peek()
                .selected_power(&target.powerset_id, &target.internal_name)
                .map(|power| power.slots.clone())
                .unwrap_or_default();
            store
                .write()
                .edit_slot(&address, &build_slots, copy_id, slot_index, None);
        }
    };

    match enhancement {
        Some(piece) => {
            let name = crate::naming::enhancement_name(&piece).to_string();
            let qualifiers = crate::naming::enhancement_qualifiers(&piece).join(", ");
            let title = match qualifiers.is_empty() {
                true => format!("{name} — right-click to remove"),
                false => format!("{name} ({qualifiers}) — right-click to remove"),
            };
            rsx! {
                div { class: "enhancement-slot",
                    button {
                        class: "slot-cell slot--{enhancement_type_class(&piece.kind)}",
                        r#type: "button",
                        title: "{title}",
                        onclick: open_picker,
                        oncontextmenu: clear,
                        if let Some(url) = &icon_url {
                            span { class: "slot-chip",
                                img { class: "slot-chip-img", src: "{url}", alt: "{name}" }
                                if let Some(frame) = &frame_url {
                                    img { class: "slot-chip-frame", src: "{frame}", alt: "", aria_hidden: "true" }
                                }
                            }
                        } else {
                            span { class: "slot-chip-label", "{slot_chip_label(&piece)}" }
                        }
                    }
                    if let Some(background) = &indicator {
                        span { class: "slot-dot", style: "background: {background};" }
                    }
                    if piece.boost > 0 {
                        span { class: "slot-boost", "{piece.boost}" }
                    }
                }
            }
        }
        None => rsx! {
            div { class: "enhancement-slot",
                button {
                    class: "slot-cell slot-cell--empty",
                    r#type: "button",
                    title: "Slot {slot_index + 1} — click to add an enhancement",
                    onclick: open_picker,
                    oncontextmenu: move |evt: Event<MouseData>| evt.prevent_default(),
                    "{slot_index + 1}"
                }
            }
        },
    }
}
