//! Attack-Chain builder modal (RB5-c) — the beta `AttackChainModal`, rebuilt over the engine.
//!
//! Every number renders from [`coh_math::chain_build::chain_inputs`], which reads the SAME
//! per-power projection the info panel shows — no calc runs here. The modal owns only
//! working state: the pick-order sequence (as stable chain-power ids, so a build edit
//! mid-session reconciles instead of mis-indexing), zoom, metric, and the saved-chain
//! selection. The team-buff simulation's CONTROLS are here but their VALUES are not — each is
//! one entry in the shared what-if layer on the build's `CombatContext`, so the same adjustment
//! moves every other surface too ([`crate::panels::what_if`]). Which stats get a control here is
//! measured, not chosen: the rows arrive on `ChainInputs::what_if`, already narrowed to the set
//! THIS build's chain moves with — a rotation holding no to-hit-gated fast form is offered no
//! to-hit control — and `chain_sensitivity_gate` holds that set to what it measures. Saved chains live on the
//! build ([`coh_data::CharacterState::attack_chains`]) and travel with it.
//!
//! Structure preserved from the beta, top to bottom: saved-chains strip · toolbar
//! (windows toggle, zoom, clear) · team-buff simulation · power palette ranked by
//! metric · per-power timeline lanes (activation bars, recharge bars, waiting hatch, DoT
//! ticks, effect-window bands, loop boundary, activity bar, ruler) · endurance sawtooth ·
//! rotation order · stats grid. Not yet ported: drag-to-reorder on the timeline (remove +
//! re-add covers the edit today), timeline panning, and clipboard copy.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::shell::Db;
use coh_data::AttackChain;
use coh_math::chain::{
    compute_chain_in_lanes, effective_recharge, nominal_damage, power_metric_value, Activation,
    ChainPower, ChainPowerKind, ChainResult, EffectWindowKind, PowerMetric,
};
use coh_math::chain_build::{
    chain_inputs, ids_to_sequence, schedule_chain, sequence_to_ids, CastNote, ChainInputs,
    ScheduledChain,
};
use coh_math::projection::StrengthBounds;
use dioxus::prelude::*;

/// Modal open flag, provided at the shell root beside the other overlay hosts.
#[derive(Clone, Copy)]
pub struct AttackChainOpen(pub Signal<bool>);

/// Header entry — sits in the shell's control cluster.
#[component]
pub fn AttackChainButton() -> Element {
    let mut open = use_context::<AttackChainOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Attack Chain Builder; pack a rotation and read its DPS and endurance",
            onclick: move |_| open.set(true),
            {crate::view::marks::chains()}
            span { "{crate::quickbar::model::ToolId::AttackChain.title()}" }
        }
    }
}

#[component]
pub fn AttackChainHost(database: Db) -> Element {
    let mut open = use_context::<AttackChainOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Attack Chain Builder".to_string(),
            size: ModalSize::Full,
            on_close: move |_| open.set(false),
            AttackChainBody { database }
        }
    }
}

/// The form the chain is scheduled inside. Absent when the build can enter no form that
/// changes what it can cast, which is every build without a Kheldian-style form toggle.
#[component]
fn ChainFormStrip(forms: Memo<Vec<String>>, form: Signal<Option<String>>) -> Element {
    if forms.read().is_empty() {
        return rsx! {};
    }
    let mut form = form;
    rsx! {
        div { class: "chain-toolbar",
            span { class: "chain-section-label", "Form" }
            button {
                class: "seg",
                class: if form.read().is_none() { "active" },
                r#type: "button",
                "aria-pressed": form.read().is_none(),
                onclick: move |_| form.set(None),
                "Default"
            }
            for mode in forms.read().iter().cloned() {
                button {
                    key: "{mode}",
                    class: "seg",
                    class: if form.read().as_deref() == Some(mode.as_str()) { "active" },
                    r#type: "button",
                    "aria-pressed": form.read().as_deref() == Some(mode.as_str()),
                    onclick: {
                        let mode = mode.clone();
                        move |_| form.set(Some(mode.clone()))
                    },
                    "{coh_data::mode_label(&mode)}"
                }
            }
        }
    }
}

/// What the inline naming form is naming.
#[derive(Clone, Copy, PartialEq)]
enum NamingMode {
    New,
    SaveAs,
    Rename,
}

#[component]
fn AttackChainBody(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let totals = use_context::<crate::panels::stats::BuildTotals>().0;
    let mut what_if_open = use_context::<crate::panels::what_if::WhatIfOpen>().0;
    // Which team-buff chips have a slider open here. Only the chips resting at ZERO are local:
    // a stat carrying a magnitude is active on every surface, because that is read off the
    // shared layer rather than stored per modal.
    let activation = crate::panels::what_if::use_what_if_activation();

    // The forms this build can fight in — offered only when entering one changes what is
    // castable, so a build with no form (or with a stance that only changes numbers) gets no
    // control at all.
    let db_for_forms = database.0.clone();
    let forms: Memo<Vec<String>> =
        use_memo(move || coh_data::form_modes(&session.build.read(), &db_for_forms));
    let form = use_signal(|| Option::<String>::None);
    // The Info card's proc-damage switch, so a power's damage reads the same here as there.
    let local_procs = use_signal(|| true);
    let include_procs = try_use_context::<crate::damage_metric_store::ProcDamagePref>()
        .map_or(local_procs, |pref| pref.0);

    // The engine-derived inputs, recomputed when the build (and so the totals) change.
    //
    // A chain is scheduled inside ONE form, so the state it reads must hold exactly that form
    // and no other: the form redirects the attacks it replaces and suspends the human toggles,
    // and the totals carry both. Only a form that differs from the build's own live modes
    // costs a recalculation; the default form reuses the shared totals memo.
    let db_for_inputs = database.0.clone();
    // The inputs travel WITH the state they were derived from: the per-cast walk
    // (`schedule_chain`) re-resolves positions against that same state, and handing it a
    // different one would resolve forms against a build the rows don't describe.
    let inputs: Memo<(ChainInputs, coh_data::CharacterState)> = use_memo(move || {
        let build = session.build.read();
        let chosen = form.read().clone();
        let procs = include_procs();
        let mut state = build.clone();
        state
            .combat
            .active_modes
            .retain(|mode| !forms.read().contains(mode));
        if let Some(mode) = &chosen {
            state.combat.active_modes.insert(mode.clone());
        }
        if state.combat.active_modes == build.combat.active_modes {
            let state = build.clone();
            return (
                chain_inputs(
                    &build,
                    &totals.read(),
                    &db_for_inputs,
                    chosen.as_deref(),
                    procs,
                ),
                state,
            );
        }
        let form_totals = coh_math::recalculate(&state, &db_for_inputs);
        (
            chain_inputs(
                &state,
                &form_totals,
                &db_for_inputs,
                chosen.as_deref(),
                procs,
            ),
            state,
        )
    });

    // Working state. The sequence is stored as stable ids so a build edit mid-session drops
    // exactly the departed power instead of shifting every later index.
    let mut sequence_ids = use_signal(Vec::<String>::new);
    let mut pixels_per_second = use_signal(|| 18.0_f64);
    let mut metric = use_signal(|| PowerMetric::Damage);
    let mut show_windows = use_signal(|| true);
    let mut selected_chain = use_signal(|| Option::<String>::None);
    let mut naming = use_signal(|| Option::<(NamingMode, String)>::None);

    // The per-cast schedule (RB5-d): each position resolved against its own walk state —
    // banked charges, the hide-meter clock — rather than once against the whole-rotation
    // toggles. Memoized because a position that visits a new context runs a full
    // recalculation; a rotation visits a handful, and only a sequence/build/form edit can
    // change the answer. `None` while the archetype's bounds are unknown (the body's early
    // return renders that state).
    let db_for_schedule = database.0.clone();
    let schedule: Memo<Option<ScheduledChain>> = use_memo(move || {
        let inputs_read = inputs.read();
        let (chain_in, walk_state) = &*inputs_read;
        let bounds = chain_in.bounds?;
        let sequence = ids_to_sequence(&chain_in.powers, &sequence_ids.read());
        Some(schedule_chain(
            walk_state,
            &db_for_schedule,
            chain_in,
            form.read().as_deref(),
            &sequence,
            chain_in.build_global_recharge_pct,
            bounds,
        ))
    });

    let inputs_guard = inputs.read();
    let (inputs_read, _walk_state) = &*inputs_guard;
    let Some(bounds) = inputs_read.bounds else {
        return rsx! {
            div { class: "chain-empty",
                "No archetype caps are loaded for this build — the recharge clamp is unknown, so the chain cannot be scheduled. Pick an archetype first."
            }
        };
    };
    if inputs_read.powers.is_empty() {
        // The form strip renders above the notice, not instead of it: a form the build holds
        // no attacks for is a legitimate state, and without the control the only way out
        // would be to close the modal.
        return rsx! {
            ChainFormStrip { forms, form }
            div { class: "chain-empty",
                if form.read().is_some() {
                    "No click powers with a cast time can be cast in this form — pick some of its attacks, or switch back."
                } else {
                    "No click powers with a cast time are picked yet — pick some attacks first."
                }
            }
        };
    }

    let powers = inputs_read.powers.clone();
    let endurance_params = inputs_read.endurance;
    // The chain's recharge divisor. It ALREADY carries whatever the what-if layer injected —
    // the injection lands in the accumulator before projection, so the totals this chain reads
    // are already simulated and adding the layer again here would double-count it.
    let global_pct = inputs_read.build_global_recharge_pct;
    let what_if_rows = inputs_read.what_if.clone();
    drop(inputs_guard);

    // How many of the chain's own stats are being simulated — the count the reset button clears,
    // deliberately not the whole layer: a reset here should not silently undo a defense buff set
    // in the what-if modal, which this modal never showed.
    let simulated_count = {
        let build = session.build.read();
        what_if_rows
            .iter()
            .filter(|row| build.combat.what_if_buffs.contains_key(row.stat))
            .count()
    };

    let sequence = ids_to_sequence(&powers, &sequence_ids.read());
    // The walked schedule: `powers` above stays the BASE candidate rows (palette, rotation
    // order, saved-chain ids), while the schedule's own row list appends one row per alternate
    // form some position resolved to, and its activations point into THAT list.
    let walked = schedule
        .read()
        .clone()
        .expect("bounds were checked above, so the schedule memo produced a walk");
    let activations = walked.activations.clone();
    let result = compute_chain_in_lanes(
        &walked.powers,
        &walked.lanes,
        &activations,
        global_pct,
        bounds,
        endurance_params,
        metric(),
    );

    // Rule-1 walk notices: state the walk could not model, beside the numbers it did.
    let walk_notices: Vec<String> = {
        let mut notices = Vec::new();
        if let Some(problem) = &walked.meter_error {
            notices.push(format!("The hide-meter clock could not be read: {problem}"));
        }
        for problem in &walked.edge_errors {
            notices.push(format!("Grant-edge state skipped: {problem}"));
        }
        if walked.edges_indeterminate > 0 {
            notices.push(format!(
                "{} grant edge(s) have conditions this context can't answer — their state \
                 changes are NOT modelled.",
                walked.edges_indeterminate
            ));
        }
        if walked.edges_probabilistic > 0 {
            notices.push(format!(
                "{} grant edge(s) roll below certainty — a schedule holds no fractional \
                 charge, so they are left out rather than averaged.",
                walked.edges_probabilistic
            ));
        }
        notices
    };

    // Palette order + intensity scale: stable per-power metric values.
    let mut palette: Vec<usize> = (0..powers.len()).collect();
    let metric_of = |index: usize| power_metric_value(&powers[index], metric(), global_pct, bounds);
    palette.sort_by(|a, b| metric_of(*b).total_cmp(&metric_of(*a)));
    let max_metric = palette.first().map(|&i| metric_of(i)).unwrap_or(0.0);

    // Rule-1 honesty strip: target-gated damage this context cannot answer reads as MISSING,
    // not zero — say so once, at the top, instead of shipping a quietly low number.
    let unresolved_total: usize = powers.iter().map(|p| p.unresolved_damage).sum();

    // The floor notice: every non-fixed power pinned at the archetype floor means the slider
    // has gone past what a debuff can actually do.
    let all_floored = {
        let mut any = false;
        let mut floored = true;
        for power in powers.iter().filter(|p| !p.fixed_recharge) {
            any = true;
            if 1.0 + power.recharge_enhancement + global_pct / 100.0 > bounds.floor {
                floored = false;
            }
        }
        any && floored
    };

    let saved_chains = session.build.read().attack_chains.clone();
    let modified = selected_chain().is_some_and(|id| {
        saved_chains
            .iter()
            .find(|chain| chain.id == id)
            .is_some_and(|chain| chain.powers != *sequence_ids.read())
    });

    let commit_chains = {
        move |mutate: Box<dyn FnOnce(&mut Vec<AttackChain>)>| {
            session.commit(move |build| mutate(&mut build.attack_chains));
        }
    };

    let confirm_naming = {
        let powers = powers.clone();
        move |_| {
            let Some((mode, value)) = naming() else {
                return;
            };
            let name = value.trim().to_string();
            if name.is_empty() {
                return;
            }
            let ids = sequence_to_ids(&powers, &ids_to_sequence(&powers, &sequence_ids.read()));
            match mode {
                NamingMode::Rename => {
                    if let Some(id) = selected_chain() {
                        commit_chains(Box::new(move |chains| {
                            if let Some(chain) = chains.iter_mut().find(|c| c.id == id) {
                                chain.name = name;
                            }
                        }));
                    }
                }
                NamingMode::New | NamingMode::SaveAs => {
                    let id = mint_chain_id(&session.build.read().attack_chains);
                    selected_chain.set(Some(id.clone()));
                    commit_chains(Box::new(move |chains| {
                        chains.push(AttackChain {
                            id,
                            name,
                            powers: ids,
                        });
                    }));
                }
            }
            naming.set(None);
        }
    };

    rsx! {
        div { class: "chain-body",

            // ---- Saved chains strip -------------------------------------------------
            div { class: "chain-saved",
                span { class: "chain-section-label", "Chains" }
                for chain in saved_chains.iter().cloned() {
                    button {
                        key: "{chain.id}",
                        class: "chain-saved__chip",
                        class: if selected_chain() == Some(chain.id.clone()) { "is-active" },
                        r#type: "button",
                        title: if modified && selected_chain() == Some(chain.id.clone()) { "unsaved changes" } else { "" },
                        onclick: {
                            let chain = chain.clone();
                            move |_| {
                                sequence_ids.set(chain.powers.clone());
                                selected_chain.set(Some(chain.id.clone()));
                                naming.set(None);
                            }
                        },
                        "{chain.name}"
                        if modified && selected_chain() == Some(chain.id.clone()) {
                            span { class: "chain-saved__dot", "•" }
                        }
                    }
                }
                button {
                    class: "chain-saved__chip chain-saved__chip--new",
                    r#type: "button",
                    onclick: move |_| {
                        sequence_ids.set(Vec::new());
                        selected_chain.set(None);
                        naming.set(None);
                    },
                    "+ New"
                }
                div { class: "chain-saved__actions",
                    if let Some((_, value)) = naming() {
                        input {
                            class: "chain-saved__name",
                            r#type: "text",
                            value: "{value}",
                            autofocus: true,
                            placeholder: "Chain name",
                            oninput: move |evt| {
                                if let Some((mode, _)) = naming() {
                                    naming.set(Some((mode, evt.value())));
                                }
                            },
                            onkeydown: {
                                let mut confirm = confirm_naming.clone();
                                move |evt: Event<KeyboardData>| {
                                    if evt.key() == Key::Enter {
                                        confirm(());
                                    } else if evt.key() == Key::Escape {
                                        naming.set(None);
                                    }
                                }
                            },
                        }
                        button {
                            class: "seg",
                            r#type: "button",
                            onclick: {
                                let mut confirm = confirm_naming.clone();
                                move |_| confirm(())
                            },
                            "Save"
                        }
                        button { class: "seg", r#type: "button", onclick: move |_| naming.set(None), "Cancel" }
                    } else {
                        if !sequence_ids.read().is_empty() {
                            if let Some(id) = selected_chain() {
                                if modified {
                                    button {
                                        class: "seg",
                                        r#type: "button",
                                        title: "Update this saved chain with the current rotation",
                                        onclick: {
                                            let powers = powers.clone();
                                            move |_| {
                                                let id = id.clone();
                                                let ids = sequence_to_ids(&powers, &ids_to_sequence(&powers, &sequence_ids.read()));
                                                commit_chains(Box::new(move |chains| {
                                                    if let Some(chain) = chains.iter_mut().find(|c| c.id == id) {
                                                        chain.powers = ids;
                                                    }
                                                }));
                                            }
                                        },
                                        "Save"
                                    }
                                }
                                button {
                                    class: "seg",
                                    r#type: "button",
                                    onclick: move |_| naming.set(Some((NamingMode::SaveAs, String::new()))),
                                    "Save as…"
                                }
                                button {
                                    class: "seg",
                                    r#type: "button",
                                    onclick: {
                                        let saved = saved_chains.clone();
                                        move |_| {
                                            let current = selected_chain()
                                                .and_then(|id| saved.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                                                .unwrap_or_default();
                                            naming.set(Some((NamingMode::Rename, current)));
                                        }
                                    },
                                    "Rename"
                                }
                                button {
                                    class: "seg chain-saved__delete",
                                    r#type: "button",
                                    onclick: move |_| {
                                        if let Some(id) = selected_chain() {
                                            selected_chain.set(None);
                                            commit_chains(Box::new(move |chains| {
                                                chains.retain(|chain| chain.id != id);
                                            }));
                                        }
                                    },
                                    "Delete"
                                }
                            } else {
                                button {
                                    class: "seg",
                                    r#type: "button",
                                    onclick: move |_| naming.set(Some((NamingMode::New, String::new()))),
                                    "Save"
                                }
                            }
                        }
                    }
                }
            }

            // ---- Honesty strip ------------------------------------------------------
            if unresolved_total > 0 {
                div { class: "chain-notice",
                    "Some damage is unresolved against the current combat context "
                    "({unresolved_total} rows, usually no target chosen). Those components are "
                    "missing from these numbers: pick a target in the Combat menu."
                }
            }
            for notice in walk_notices.iter() {
                div { class: "chain-notice", "{notice}" }
            }

            // ---- Form ---------------------------------------------------------------
            ChainFormStrip { forms, form }

            // ---- Toolbar ------------------------------------------------------------
            div { class: "chain-toolbar",
                button {
                    class: "stat-pill",
                    class: if show_windows() { "is-on" },
                    r#type: "button",
                    "aria-pressed": show_windows(),
                    onclick: move |_| show_windows.set(!show_windows()),
                    "Buff/debuff windows"
                }
                div { class: "chain-toolbar__spacer" }
                span { class: "chain-section-label", "Zoom" }
                button { class: "seg", r#type: "button", onclick: move |_| pixels_per_second.set((pixels_per_second() / 1.3).max(4.0)), "−" }
                button { class: "seg", r#type: "button", onclick: move |_| pixels_per_second.set((pixels_per_second() * 1.3).min(80.0)), "+" }
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| sequence_ids.set(Vec::new()),
                    "Clear"
                }
            }

            // ---- What-if simulation -------------------------------------------------
            // The controls are HERE because these are the stats a rotation's timing, damage and
            // endurance actually move with — but their VALUES are not this modal's: each is one
            // entry in the shared what-if layer, so the same adjustment moves the dashboard, the
            // info panel and every other surface at once. Which stats those are is measured, not
            // chosen here — the engine narrows them per build on `ChainInputs::what_if`.
            div { class: "chain-whatif",
                div { class: "chain-whatif__head",
                    span { class: "chain-section-label", "Team-buff simulation" }
                    span { class: "chain-whatif__spacer" }
                    span { class: "chain-whatif__hint", "tap a chip to add its slider" }
                    if simulated_count > 0 {
                        button {
                            class: "seg",
                            r#type: "button",
                            onclick: {
                                let stats: Vec<&'static str> =
                                    what_if_rows.iter().map(|row| row.stat).collect();
                                let mut activation = activation;
                                move |_| {
                                    let stats = stats.clone();
                                    session.commit(move |build| {
                                        for stat in stats {
                                            build.combat.what_if_buffs.remove(stat);
                                        }
                                    });
                                    activation.deactivate_all();
                                }
                            },
                            "Reset all ({simulated_count})"
                        }
                    }
                    button {
                        class: "seg",
                        r#type: "button",
                        title: "Every other stat a teammate can buff",
                        onclick: move |_| what_if_open.set(true),
                        "All team buffs…"
                    }
                }
                // One chip per chain-moving stat; only an activated chip takes up a slider row,
                // so the block stays the size of what is actually being simulated. A stat
                // buffed from the what-if modal arrives here already active — the layer is
                // shared, and activation is derived from it rather than stored twice.
                div { class: "whatif__chips chain-whatif__chips",
                    for row in what_if_rows.iter().copied() {
                        ChainWhatIfChip { key: "{row.stat}", row, activation }
                    }
                }
                div { class: "chain-whatif__rows",
                    for row in what_if_rows.iter().copied() {
                        if activation.is_active(&session, row.stat) {
                            ChainWhatIfRow { key: "{row.stat}", row }
                        }
                    }
                }
                if all_floored {
                    div { class: "chain-notice chain-notice--floor",
                        "Every power is at the recharge floor ({floor_pct(bounds)}% net): a "
                        "power slows to at most {1.0 / bounds.floor:.0}× its base recharge."
                    }
                }
            }

            // ---- Power palette ------------------------------------------------------
            div { class: "chain-palette",
                div { class: "chain-palette__head",
                    span { class: "chain-section-label", "Available powers — tap to add" }
                    span { class: "chain-palette__rank",
                        "Rank by"
                        select {
                            class: "select-compact",
                            onchange: move |evt| {
                                metric.set(match evt.value().as_str() {
                                    "dpa" => PowerMetric::DamagePerActivation,
                                    "dps" => PowerMetric::DamagePerSecond,
                                    _ => PowerMetric::Damage,
                                });
                            },
                            option { value: "damage", selected: metric() == PowerMetric::Damage, "Damage" }
                            option { value: "dpa", selected: metric() == PowerMetric::DamagePerActivation, "DPA" }
                            option { value: "dps", selected: metric() == PowerMetric::DamagePerSecond, "DPS" }
                        }
                    }
                }
                div { class: "chain-palette__chips",
                    for &index in palette.iter() {
                        {
                            let power = &powers[index];
                            let intensity = if max_metric > 0.0 { (metric_of(index) / max_metric).clamp(0.0, 1.0) } else { 0.0 };
                            let id = power.id.clone();
                            rsx! {
                                button {
                                    key: "{power.id}",
                                    class: "chain-chip {kind_class(power.kind)}",
                                    r#type: "button",
                                    style: "--chain-intensity: {intensity};",
                                    title: chip_title(power, metric(), global_pct, bounds),
                                    onclick: move |_| sequence_ids.write().push(id.clone()),
                                    span { class: "chain-chip__name", "{power.name}" }
                                    span { class: "chain-chip__cast mono", "{power.cast:.2}s" }
                                }
                            }
                        }
                    }
                }
            }

            // ---- Timeline -----------------------------------------------------------
            if let Some(result) = result.as_ref() {
                ChainTimeline {
                    powers: walked.powers.clone(),
                    lanes: walked.lanes.clone(),
                    notes: walked.notes.clone(),
                    activations: activations.clone(),
                    result: result.clone(),
                    pixels_per_second: pixels_per_second(),
                    global_recharge_pct: global_pct,
                    bounds,
                    show_windows: show_windows(),
                    metric: metric(),
                    max_metric,
                    on_remove: move |sequence_index: usize| {
                        let mut ids = sequence_ids.write();
                        if sequence_index < ids.len() {
                            ids.remove(sequence_index);
                        }
                    },
                }

                // ---- Rotation order -------------------------------------------------
                div { class: "chain-order",
                    span { class: "chain-section-label", "Rotation order" }
                    span { class: "chain-order__flow",
                        for (position, &power_index) in sequence.iter().enumerate() {
                            if position > 0 {
                                span { class: "chain-order__arrow", " → " }
                            }
                            span { class: "chain-order__name {kind_class(powers[power_index].kind)}",
                                "{powers[power_index].name}"
                            }
                        }
                    }
                }

                // ---- Stats grid -----------------------------------------------------
                ChainStats { result: result.clone(), endurance_params, }
            } else {
                div { class: "chain-empty", "Tap powers above to build a rotation." }
            }
        }
    }
}

/// The floor as a signed net percent for the notice (0.25 → −75).
fn floor_pct(bounds: StrengthBounds) -> f64 {
    (bounds.floor - 1.0) * 100.0
}

/// The chip form of a chain-side control — the same chip the what-if modal draws, over the
/// chain's own row data. Rendering the row itself is [`ChainWhatIfRow`]; this only opens it.
#[component]
fn ChainWhatIfChip(
    row: coh_math::chain_build::ChainWhatIf,
    activation: crate::panels::what_if::WhatIfActivation,
) -> Element {
    let session = use_context::<BuildSession>();
    let stat = row.stat;
    let Some(control) = crate::panels::what_if::controls_for(&[stat])
        .into_iter()
        .next()
    else {
        // The row itself renders the Rule-1 error; a chip for a control that does not exist
        // would just be a second place to say the same thing.
        return rsx! {};
    };
    let magnitude = session
        .build
        .read()
        .combat
        .what_if_buffs
        .get(stat)
        .copied()
        .unwrap_or(0.0);
    rsx! {
        crate::panels::what_if::WhatIfChip {
            control,
            active: activation.is_active(&session, stat),
            magnitude,
            on_toggle: {
                let mut activation = activation;
                move |_| activation.toggle(session, stat)
            },
        }
    }
}

/// One chain-side what-if control: a stat the chain's numbers move with.
///
/// Everything about it is read rather than chosen — the label, unit and step from the dashboard
/// row that renders the stat ([`crate::panels::what_if`]), and the slider's reach from the
/// archetype's own exported ceiling for it. A hand-set range would be a guess about a number the
/// export owns, and would differ per surface the moment one of them was updated and the other
/// was not.
#[component]
fn ChainWhatIfRow(row: coh_math::chain_build::ChainWhatIf) -> Element {
    let session = use_context::<BuildSession>();
    let stat = row.stat;
    let Some(control) = crate::panels::what_if::controls_for(&[stat])
        .into_iter()
        .next()
    else {
        // Fail loud rather than drop it: the chain measurably moves with this stat, so a
        // missing control is a surface that cannot simulate something it says it can.
        return rsx! {
            div { class: "chain-notice chain-notice--error",
                "No control is defined for {stat}, which this chain's numbers depend on."
            }
        };
    };

    let simulated = session
        .build
        .read()
        .combat
        .what_if_buffs
        .get(stat)
        .copied()
        .unwrap_or(0.0);
    let format = control.format;

    rsx! {
        div {
            class: "chain-whatif__row",
            class: if simulated != 0.0 { "is-simulated" },
            style: "--row-hue: {control.family_token};",
            span { class: "chain-whatif__label", "{control.label}" }
            span { class: "mono chain-whatif__build", "Build {format.render_delta(row.from_build)}" }
            if let Some(ceiling) = row.ceiling {
                div { class: "slider chain-whatif__slider",
                    input {
                        r#type: "range",
                        // Symmetric around zero: a teammate's debuff is the same layer with the
                        // sign flipped, and the game caps the buff side only.
                        min: "{-ceiling}",
                        max: "{ceiling}",
                        step: crate::panels::what_if::step_for(format),
                        value: "{simulated}",
                        "aria-label": "{control.label} team-buff simulation",
                        oninput: move |evt| {
                            if let Ok(value) = evt.value().parse::<f64>() {
                                crate::panels::what_if::set_buff(session, stat, value);
                            }
                        },
                    }
                }
            }
            input {
                class: "chain-whatif__input mono",
                r#type: "number",
                step: crate::panels::what_if::step_for(format),
                value: "{simulated}",
                "aria-label": "{control.label} team-buff simulation, exact value",
                oninput: move |evt| {
                    if let Ok(value) = evt.value().parse::<f64>() {
                        crate::panels::what_if::set_buff(session, stat, value);
                    }
                },
            }
            if simulated == 0.0 {
                span { class: "chain-whatif__state", "—" }
            } else {
                span {
                    class: "chain-whatif__state",
                    class: if simulated > 0.0 { "is-buff" } else { "is-debuff" },
                    "{format.render_delta(simulated)} simulated → {format.render_delta(row.current)}"
                }
            }
        }
    }
}

/// One scheduled cast's hover text: the resolved row plus what the walk knew at that
/// position — the meter state, an alternate form, the charges banked or spent.
fn cast_title(row: &ChainPower, activation: &Activation, note: Option<&CastNote>) -> String {
    let mut title = format!(
        "{} at {:.1}s · cast {:.2}s · {:.1} dmg",
        row.name, activation.start, row.cast, row.damage
    );
    if let Some(note) = note {
        if let Some(hidden) = note.hidden {
            title.push_str(if hidden {
                " · from Hide"
            } else {
                " · not hidden"
            });
        }
        if note.form_changed {
            title.push_str(" · alternate form for this position");
        }
        for path in &note.banked {
            title.push_str(&format!(" · banks {}", short_path(path)));
        }
        for path in &note.spent {
            title.push_str(&format!(" · spends {}", short_path(path)));
        }
    }
    title
}

/// A grant path's display tail (`redirects.energy_melee.energy_store` → `energy_store`) —
/// naming only; the full path stays in the data.
fn short_path(path: &str) -> &str {
    path.rsplit('.').next().unwrap_or(path)
}

fn kind_class(kind: ChainPowerKind) -> &'static str {
    match kind {
        ChainPowerKind::Attack => "is-attack",
        ChainPowerKind::Buff => "is-buff",
        ChainPowerKind::Utility => "is-utility",
    }
}

fn chip_title(
    power: &ChainPower,
    metric: PowerMetric,
    global_pct: f64,
    bounds: StrengthBounds,
) -> String {
    let recharge = effective_recharge(power, global_pct, bounds);
    let value = power_metric_value(power, metric, global_pct, bounds);
    let unresolved = if power.unresolved_damage > 0 {
        format!(" · {} damage rows unresolved", power.unresolved_damage)
    } else {
        String::new()
    };
    format!(
        "{} — {:.1} dmg/cast · cast {:.2}s · recharge {:.1}s · {:.1} end · metric {:.1}{}",
        power.name,
        nominal_damage(power),
        power.cast,
        recharge,
        power.endurance_cost,
        value,
        unresolved,
    )
}

#[component]
fn ChainTimeline(
    powers: Vec<ChainPower>,
    lanes: Vec<usize>,
    notes: Vec<Option<CastNote>>,
    activations: Vec<Activation>,
    result: ChainResult,
    pixels_per_second: f64,
    global_recharge_pct: f64,
    bounds: StrengthBounds,
    show_windows: bool,
    metric: PowerMetric,
    max_metric: f64,
    on_remove: EventHandler<usize>,
) -> Element {
    let px = pixels_per_second;
    let cycle = result.cycle_seconds;
    // A little air past the loop line so the boundary is visible.
    let display_seconds = cycle * 1.04 + 1.0;
    let track_width = display_seconds * px;

    // Lane order: first-cast order, one lane per POWER that appears in the chain. A power's
    // alternate forms are separate rows but share its lane (and its one recharge timer), so
    // a from-Hide opener and its mid-combat repeats draw on one line (RB5-d).
    let mut lane_powers: Vec<usize> = Vec::new();
    for activation in &activations {
        let lane = lanes[activation.power_index];
        if !lane_powers.contains(&lane) {
            lane_powers.push(lane);
        }
    }

    // Ruler step: coarser as the window grows.
    let step = if display_seconds > 60.0 {
        10.0
    } else if display_seconds > 30.0 {
        5.0
    } else if display_seconds > 12.0 {
        2.0
    } else {
        1.0
    };
    let tick_count = (display_seconds / step).floor() as usize;

    let efficiency_tight = result.efficiency >= 95.0;

    rsx! {
        div { class: "chain-timeline",
            div { class: "chain-timeline__head",
                span { class: "chain-section-label", "Chain timeline · ✕ removes a cast" }
                span { class: "chain-legend",
                    span { class: "chain-legend__item chain-legend__item--active", "active" }
                    span { class: "chain-legend__item chain-legend__item--recharge", "recharging" }
                    span { class: "chain-legend__item chain-legend__item--waiting", "ready, waiting" }
                    span { class: "chain-legend__item chain-legend__item--dead", "dead time" }
                    span { class: "chain-legend__item chain-legend__item--dot", "DoT tick" }
                    if efficiency_tight {
                        span { class: "chain-badge is-good", "Tight loop" }
                    } else {
                        span { class: "chain-badge is-warn", "{result.dead_time:.1}s dead" }
                    }
                }
            }
            div { class: "chain-timeline__scroll",
                div { class: "chain-timeline__lanes", style: "width: {track_width}px;",
                    for &lane in lane_powers.iter() {
                        {
                            let base = &powers[lane];
                            let mine: Vec<&Activation> = activations.iter().filter(|a| lanes[a.power_index] == lane).collect();
                            let count = mine.len();
                            let intensity = if max_metric > 0.0 {
                                (power_metric_value(base, metric, global_recharge_pct, bounds) / max_metric).clamp(0.0, 1.0)
                            } else { 0.0 };
                            rsx! {
                                div { class: "chain-lane", key: "{base.id}",
                                    span { class: "chain-lane__label", title: "{base.name}",
                                        "{base.name}"
                                        if count > 1 { span { class: "mono chain-lane__count", " ×{count}" } }
                                    }
                                    div { class: "chain-lane__track",
                                        // Effect window bands first (lowest layer). Each cast's own
                                        // resolved row supplies its window, dots and bar below —
                                        // the form actually fired at that position.
                                        if show_windows {
                                            for activation in mine.iter() {
                                                if let Some(window) = powers[activation.power_index].effect_window {
                                                    div {
                                                        class: match window.kind {
                                                            EffectWindowKind::Buff => "chain-window chain-window--buff",
                                                            EffectWindowKind::Debuff => "chain-window chain-window--debuff",
                                                        },
                                                        style: "left: {activation.start * px}px; width: {window.duration * px}px;",
                                                        title: match window.kind {
                                                            EffectWindowKind::Buff => "self-buff window",
                                                            EffectWindowKind::Debuff => "foe-debuff window",
                                                        },
                                                    }
                                                }
                                            }
                                        }
                                        // Recharge bars + waiting hatch — each cast recharges at
                                        // ITS OWN row's rate (the form fired decides the timer).
                                        for (position, activation) in mine.iter().enumerate() {
                                            {
                                                let recharge = effective_recharge(&powers[activation.power_index], global_recharge_pct, bounds);
                                                let ready_at = activation.end + recharge;
                                                let next_start = mine.get(position + 1).map(|a| a.start);
                                                rsx! {
                                                    div {
                                                        class: "chain-recharge",
                                                        style: "left: {activation.end * px}px; width: {recharge * px}px;",
                                                        title: "recharging {recharge:.1}s",
                                                    }
                                                    if let Some(next_start) = next_start {
                                                        if next_start > ready_at + 0.01 {
                                                            div {
                                                                class: "chain-waiting",
                                                                style: "left: {ready_at * px}px; width: {(next_start - ready_at) * px}px;",
                                                                title: "ready, waiting {next_start - ready_at:.1}s",
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // DoT tick marks, faded by landing probability.
                                        for activation in mine.iter() {
                                            for dot in powers[activation.power_index].dots.iter() {
                                                for t in 1..=(dot.ticks as u32) {
                                                    {
                                                        let tick_time = activation.start + powers[activation.power_index].cast + f64::from(t) * dot.period;
                                                        let probability = dot.tick_probability(t);
                                                        rsx! {
                                                            if tick_time <= display_seconds {
                                                                div {
                                                                    class: "chain-dot-tick",
                                                                    style: "left: {tick_time * px}px; opacity: {0.25 + 0.75 * probability};",
                                                                    title: "{dot.per_tick:.1} dmg · {probability * 100.0:.0}% to land",
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        // Activation bars (top layer) with per-bar remove. A cast
                                        // whose position resolved an alternate form is marked, and
                                        // its title says what the walk knew there.
                                        for activation in mine.iter() {
                                            {
                                                let row = &powers[activation.power_index];
                                                let note = notes.get(activation.sequence_index).and_then(Option::as_ref);
                                                let alt = note.is_some_and(|n| n.form_changed);
                                                rsx! {
                                                    div {
                                                        class: "chain-bar {kind_class(row.kind)}",
                                                        class: if alt { "chain-bar--alt" },
                                                        style: "left: {activation.start * px}px; width: {(activation.end - activation.start) * px:.1}px; --chain-intensity: {intensity};",
                                                        title: cast_title(row, activation, note),
                                                        if alt {
                                                            span { class: "chain-bar__form", title: "this position resolved an alternate form", "◆" }
                                                        }
                                                        button {
                                                            class: "chain-bar__remove",
                                                            r#type: "button",
                                                            title: "Remove this cast",
                                                            onclick: {
                                                                let sequence_index = activation.sequence_index;
                                                                move |_| on_remove.call(sequence_index)
                                                            },
                                                            "✕"
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
                    // Loop boundary.
                    div { class: "chain-loop-line", style: "left: {cycle * px}px;", title: "loop boundary — {cycle:.1}s" }

                    // Activity bar: active vs dead.
                    div { class: "chain-activity",
                        div { class: "chain-activity__active", style: "width: {cycle * px}px;" }
                        for gap in result.dead_gaps.iter() {
                            div {
                                class: "chain-activity__dead",
                                style: "left: {gap.start * px}px; width: {(gap.end - gap.start) * px}px;",
                                title: "dead {gap.end - gap.start:.1}s",
                            }
                        }
                    }

                    // Ruler.
                    div { class: "chain-ruler",
                        for i in 0..=tick_count {
                            {
                                let t = i as f64 * step;
                                rsx! {
                                    span { class: "chain-ruler__tick mono", style: "left: {t * px}px;", "{t:.0}s" }
                                }
                            }
                        }
                    }

                    // Endurance sawtooth (from a full bar, across the sim's loops).
                    if let Some(endurance) = result.endurance.as_ref() {
                        div { class: "chain-endurance",
                            span { class: "chain-section-label", "Endurance (from full)" }
                            svg {
                                class: "chain-endurance__chart",
                                width: "{track_width}",
                                height: "48",
                                view_box: "0 0 {track_width} 48",
                                preserve_aspect_ratio: "none",
                                // Cycle-boundary grid lines.
                                for loop_index in 1..=endurance.track_loops {
                                    line {
                                        x1: "{f64::from(loop_index) * cycle * px}",
                                        x2: "{f64::from(loop_index) * cycle * px}",
                                        y1: "0",
                                        y2: "48",
                                        class: "chain-endurance__grid",
                                    }
                                }
                                polyline {
                                    class: "chain-endurance__line",
                                    points: endurance_points(&endurance.track, px),
                                    fill: "none",
                                }
                                if let Some(stall) = endurance.stall_time {
                                    line {
                                        x1: "{stall * px}",
                                        x2: "{stall * px}",
                                        y1: "0",
                                        y2: "48",
                                        class: "chain-endurance__stall",
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

fn endurance_points(track: &[coh_math::chain::EndurancePoint], px: f64) -> String {
    track
        .iter()
        .map(|point| {
            format!(
                "{:.1},{:.1}",
                point.time * px,
                (1.0 - point.fraction) * 46.0 + 1.0
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[component]
fn ChainStats(
    result: ChainResult,
    endurance_params: Option<coh_math::chain::EnduranceParams>,
) -> Element {
    let endurance = result.endurance.clone();
    // The bar the sim drained, for the recovery card's help text. Present whenever the sim ran,
    // since the result only carries endurance when the params did.
    let max_endurance = endurance_params.map_or(0.0, |params| params.max_endurance);
    let click_gain_per_second = endurance
        .as_ref()
        .map(|e| {
            // Recovery clicks show as their own card only when the chain has one.
            (e.per_loop_delta
                - (e.recovery_per_second - e.toggle_per_second) * result.cycle_seconds
                + e.attack_per_second * result.cycle_seconds)
                / result.cycle_seconds
        })
        .filter(|gain| *gain > 0.01);

    rsx! {
        div { class: "chain-stats",
            ChainStatCard { label: "Cycle", value: format!("{:.1}", result.cycle_seconds), unit: "s", tone: StatTone::Neutral,
                help: "Full loop time: every power recharged, ready to repeat." }
            ChainStatCard { label: "Total dmg", value: format!("{:.0}", result.total_damage), unit: "", tone: StatTone::Neutral,
                help: "Damage landed per cycle, averaged over rolls (crits at p × damage) and DoT ticks inside the loop." }
            ChainStatCard { label: "DPS", value: format!("{:.1}", result.damage_per_second), unit: "", tone: StatTone::Neutral,
                help: "Total damage ÷ cycle." }
            ChainStatCard {
                label: "Efficiency",
                value: format!("{:.0}", result.efficiency),
                unit: "%",
                tone: tone_for(result.efficiency),
                help: "Share of the cycle spent animating. Below 100 means idle gaps.",
            }
            if let Some(compactness) = result.compactness {
                ChainStatCard {
                    label: "Compactness",
                    value: format!("{compactness:.0}"),
                    unit: "%",
                    tone: tone_for(compactness),
                    help: "Damage-weighted recharge utilization: when a power is ready, is it fired immediately?",
                }
            }
            if let Some(endurance) = endurance.as_ref() {
                ChainStatCard { label: "Recovery", value: format!("{:.2}", endurance.recovery_per_second), unit: "/s", tone: StatTone::Good,
                    help: "Passive endurance recovery ({max_endurance:.0} max end)." }
                if let Some(gain) = click_gain_per_second {
                    ChainStatCard { label: "Click +End", value: format!("{gain:.2}"), unit: "/s", tone: StatTone::Good,
                        help: "Endurance paid back by recovery clicks in the chain, averaged per second." }
                }
                ChainStatCard {
                    label: "Spend",
                    value: format!("{:.2}", endurance.attack_per_second + endurance.toggle_per_second),
                    unit: "/s",
                    tone: StatTone::Neutral,
                    help: "Attack costs ({endurance.attack_per_second:.2}/s) plus toggle drain ({endurance.toggle_per_second:.2}/s).",
                }
                ChainStatCard {
                    label: "Net end",
                    value: format!("{:+.2}", endurance.net_per_second),
                    unit: "/s",
                    tone: if endurance.net_per_second >= -0.001 { StatTone::Good } else { StatTone::Warn },
                    help: "Recovery − toggles − attack spend. Negative drains the bar.",
                }
                ChainStatCard {
                    label: "Sustain",
                    value: match (endurance.sustainable, endurance.stall_time, endurance.time_to_empty) {
                        (true, _, _) => "yes".to_string(),
                        (false, Some(stall), _) => format!("stall {stall:.0}s"),
                        (false, None, Some(empty)) => format!("{empty:.0}s"),
                        (false, None, None) => "declining".to_string(),
                    },
                    unit: "",
                    tone: if endurance.sustainable { StatTone::Good } else { StatTone::Warn },
                    help: "Read from the clamped simulation from a full bar — overfilled recovery clicks don't count twice.",
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum StatTone {
    Good,
    Warn,
    Bad,
    Neutral,
}

fn tone_for(percent: f64) -> StatTone {
    if percent >= 95.0 {
        StatTone::Good
    } else if percent >= 80.0 {
        StatTone::Warn
    } else {
        StatTone::Bad
    }
}

#[component]
fn ChainStatCard(
    label: String,
    value: String,
    unit: String,
    tone: StatTone,
    help: String,
) -> Element {
    let tone_class = match tone {
        StatTone::Good => "is-good",
        StatTone::Warn => "is-warn",
        StatTone::Bad => "is-bad",
        StatTone::Neutral => "",
    };
    rsx! {
        div { class: "chain-stat {tone_class}", title: "{help}",
            span { class: "chain-stat__label", "{label}" }
            span { class: "chain-stat__value mono",
                "{value}"
                if !unit.is_empty() {
                    span { class: "chain-stat__unit", "{unit}" }
                }
            }
        }
    }
}

/// A fresh chain id, unique within this build's saved chains.
fn mint_chain_id(existing: &[AttackChain]) -> String {
    let next = existing
        .iter()
        .filter_map(|chain| chain.id.strip_prefix("chain-")?.parse::<u64>().ok())
        .max()
        .unwrap_or(0)
        + 1;
    format!("chain-{next}")
}
