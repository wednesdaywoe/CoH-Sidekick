//! The what-if team-buff modal — simulate what a teammate is handing the build.
//!
//! The layer itself lives on `CombatContext::what_if_buffs` and is injected into the
//! accumulators before projection ([`coh_math::what_if`]), so this component only reads and
//! writes magnitudes. Every consequence — the archetype ceilings binding, the fast snipe form,
//! the chain's DPS — falls out of that one injection rather than out of anything here.
//!
//! **No stat is named in a conditional.** The controls come from
//! [`coh_math::what_if::vocabulary`] (what the accumulator can take) narrowed to the keys some
//! [`stat_registry::StatDef`] actually renders, and each one's label, section and unit are read
//! off that row. A stat the engine grows arrives with no edit here; a stat no surface shows
//! never becomes a control that appears to do something.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::panels::stat_registry::{self, StatFormat, StatSection};
use dioxus::prelude::*;
use std::collections::BTreeSet;

/// Whether the modal is open — the shell provides it, matching the attack-chain trio.
#[derive(Clone, Copy)]
pub struct WhatIfOpen(pub Signal<bool>);

/// One control: a stat the layer can push, and the dashboard row it will move.
///
/// Shared with the attack-chain modal, which offers the same layer's chain-moving subset — one
/// description of a stat's control, so the two surfaces cannot label or step it differently.
#[derive(Clone, PartialEq)]
pub struct WhatIfControl {
    /// The `GlobalBonuses` key the layer is keyed by.
    pub stat: &'static str,
    pub label: String,
    pub section: StatSection,
    pub family_token: &'static str,
    pub format: StatFormat,
}

/// The controls, derived rather than listed.
///
/// Two narrowings on top of the engine's vocabulary, each using vocabulary that already exists
/// for its own reasons:
///
/// * a key must be named by some [`stat_registry::StatDef::ledger_keys`] — that is what "a
///   surface renders this number" means. It is the filter that keeps `mezResist` out: the
///   accumulator still routes it, but nothing in the calc spends it and its dashboard row was
///   retired (DATA-GAP-REGISTER MEZRES-1), so a slider for it would move nothing anywhere.
/// * the row supplies the label — but only when it names exactly ONE ledger key, so the label
///   is unambiguously about this stat. Where a row combines several (`defSL` is
///   `max(defSmashing, defLethal)`), the key is humanised instead of borrowing a label that
///   describes the pair.
fn controls() -> Vec<WhatIfControl> {
    coh_math::what_if::vocabulary()
        .into_iter()
        .filter_map(|stat| {
            let row = stat_registry::ALL
                .iter()
                .find(|def| def.ledger_keys.contains(&stat))?;
            let label = if row.ledger_keys.len() == 1 {
                row.label.to_string()
            } else {
                humanise(stat)
            };
            Some(WhatIfControl {
                stat,
                label,
                section: row.section,
                family_token: row.family.token(),
                format: row.ledger_format,
            })
        })
        .collect()
}

/// `defSmashing` → `Def Smashing`. A pure string transform over the accumulator's own key, used
/// only where a row's label would describe more than this one stat. Not a lookup table: a table
/// would be a second stat vocabulary to keep in sync (the PROD6A lesson).
fn humanise(key: &str) -> String {
    let mut out = String::with_capacity(key.len() + 4);
    for (index, character) in key.chars().enumerate() {
        if character.is_uppercase() && index > 0 {
            out.push(' ');
        }
        if index == 0 {
            out.extend(character.to_uppercase());
        } else {
            out.push(character);
        }
    }
    out
}

/// Write one entry of the shared what-if layer.
///
/// Zero REMOVES the entry rather than storing a `0.0`, which is what keeps "is anything
/// simulated?" a plain `is_empty()` on every surface instead of a scan for non-zero values.
/// Shared with the chain modal for that reason: two writers with two ideas of what zero means
/// would make the simulated markers disagree with the sliders.
pub fn set_buff(session: BuildSession, stat: &'static str, magnitude: f64) {
    session.commit(move |build| {
        if magnitude == 0.0 {
            build.combat.what_if_buffs.remove(stat);
        } else {
            build
                .combat
                .what_if_buffs
                .insert(stat.to_string(), magnitude);
        }
    });
}

/// The controls for `stats`, in the order given — how another surface offers part of the same
/// layer without a second idea of what a stat is called or how it steps.
///
/// A name with no control is DROPPED rather than rendered bare: the reasons a stat has no
/// control (nothing renders it, so nothing would visibly move) are reasons a chain-side slider
/// would be inert too. `chain_sensitivity_gate` is what keeps that from silently hiding a stat
/// the chain genuinely moves with.
pub fn controls_for(stats: &[&str]) -> Vec<WhatIfControl> {
    let all = controls();
    stats
        .iter()
        .filter_map(|stat| all.iter().find(|control| control.stat == *stat).cloned())
        .collect()
}

/// The step a magnitude control moves in, from the unit the row is already in. A magnitude is a
/// mez mag (whole-ish points), a percentage moves in fives the way the recharge slider does, and
/// everything else in ones.
pub fn step_for(format: StatFormat) -> &'static str {
    match format {
        StatFormat::Percent | StatFormat::SignedPercent | StatFormat::HastePercent => "5",
        StatFormat::Magnitude => "0.5",
        StatFormat::EndurancePerSecond | StatFormat::SignedEndurancePerSecond => "0.1",
        _ => "1",
    }
}

/// Which stats have a visible slider row, shared by this modal and the chain builder.
///
/// Active is DERIVED, not stored: any stat carrying a nonzero magnitude is active — including
/// one set on the other surface, since the layer is shared — plus the chips a user switched on
/// that are still resting at zero, which is the only part this holds. Switching a chip off
/// zeroes its entry, so the surfaces can never disagree about what is being simulated.
///
/// The zero-value chips are deliberately per-surface: they are a statement about which controls
/// you want in front of you, not about the build.
#[derive(Clone, Copy, PartialEq)]
pub struct WhatIfActivation(Signal<BTreeSet<&'static str>>);

pub fn use_what_if_activation() -> WhatIfActivation {
    WhatIfActivation(use_signal(BTreeSet::new))
}

impl WhatIfActivation {
    pub fn is_active(&self, session: &BuildSession, stat: &str) -> bool {
        self.0.read().contains(stat)
            || session
                .build
                .read()
                .combat
                .what_if_buffs
                .get(stat)
                .is_some_and(|magnitude| *magnitude != 0.0)
    }

    pub fn toggle(&mut self, session: BuildSession, stat: &'static str) {
        if self.is_active(&session, stat) {
            self.0.write().remove(stat);
            set_buff(session, stat, 0.0);
        } else {
            self.0.write().insert(stat);
        }
    }

    /// The companion to "clear all": drop the chips resting at zero too, not just the values.
    pub fn deactivate_all(&mut self) {
        self.0.write().clear();
    }
}

/// One stat as a chip — the compact form of a control. Tapping it opens (or closes) that stat's
/// slider row, and it carries its own magnitude so a closed row still reads at a glance.
#[component]
pub fn WhatIfChip(
    control: WhatIfControl,
    active: bool,
    magnitude: f64,
    on_toggle: EventHandler<()>,
) -> Element {
    let verb = if active {
        "Stop simulating"
    } else {
        "Simulate"
    };
    rsx! {
        button {
            class: "whatif-chip",
            class: if active { "is-active" },
            style: "--row-hue: {control.family_token};",
            r#type: "button",
            "aria-pressed": active,
            title: "{verb} a {control.label} team buff",
            onclick: move |_| on_toggle.call(()),
            span { class: "whatif-chip__label", "{control.label}" }
            if active && magnitude != 0.0 {
                span { class: "whatif-chip__value mono", "{control.format.render_delta(magnitude)}" }
            }
            if active {
                span { class: "whatif-chip__close", "aria-hidden": "true", "×" }
            }
        }
    }
}

/// Header entry — sits in the shell's control cluster beside the chain button.
#[component]
pub fn WhatIfButton() -> Element {
    let mut open = use_context::<WhatIfOpen>().0;
    let session = use_context::<BuildSession>();
    let active = !session.build.read().combat.what_if_buffs.is_empty();
    rsx! {
        button {
            class: "quickbar-action",
            class: if active { "is-simulated" },
            r#type: "button",
            title: "What-if team buffs, simulates buffs provided by teammates",
            "aria-pressed": active,
            onclick: move |_| open.set(true),
            {crate::view::marks::what_if()}
            span { "{crate::quickbar::model::ToolId::WhatIf.title()}" }
            if active {
                span { class: "whatif-dot", "aria-hidden": "true" }
            }
        }
    }
}

#[component]
pub fn WhatIfHost() -> Element {
    let mut open = use_context::<WhatIfOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "What-if team buffs".to_string(),
            size: ModalSize::Lg,
            on_close: move |_| open.set(false),
            WhatIfBody {}
        }
    }
}

#[component]
fn WhatIfBody() -> Element {
    let session = use_context::<BuildSession>();
    let controls = use_memo(controls);
    let mut activation = use_what_if_activation();

    let active_count = session.build.read().combat.what_if_buffs.len();
    // The sliders live here, one per ACTIVE stat, rather than one per stat — the vocabulary is
    // sixty-odd controls and a build simulates one or two, so the modal stays the size of what
    // is actually being simulated.
    let active: Vec<WhatIfControl> = controls()
        .into_iter()
        .filter(|control| activation.is_active(&session, control.stat))
        .collect();

    rsx! {
        div { class: "whatif",
            div { class: "whatif__intro",
                p {
                    "These buffs are "
                    strong { "simulated" }
                    ". They are injected where a teammate's real buff would land — into the \
                     build's globals before anything is projected — so every archetype ceiling \
                     binds against them and every surface agrees. They are never saved or \
                     shared with the build."
                }
                if active_count > 0 || !active.is_empty() {
                    button {
                        class: "seg",
                        r#type: "button",
                        onclick: move |_| {
                            session.commit(|build| build.combat.what_if_buffs.clear());
                            activation.deactivate_all();
                        },
                        "Clear all ({active_count})"
                    }
                }
            }
            if !active.is_empty() {
                section { class: "whatif__group whatif__group--active",
                    h3 { class: "whatif__group-title", "Simulated buffs" }
                    div { class: "whatif__rows",
                        for control in active {
                            WhatIfRow { key: "{control.stat}", control }
                        }
                    }
                }
            }
            for section in StatSection::ALL {
                WhatIfSection { section, controls: controls(), activation }
            }
            p { class: "whatif__hint",
                "Tap to add a slider for that buff; tap again to remove it."
            }
        }
    }
}

#[component]
fn WhatIfSection(
    section: StatSection,
    controls: Vec<WhatIfControl>,
    activation: WhatIfActivation,
) -> Element {
    let session = use_context::<BuildSession>();
    let rows: Vec<WhatIfControl> = controls
        .into_iter()
        .filter(|control| control.section == section)
        .collect();
    if rows.is_empty() {
        return rsx! {};
    }
    rsx! {
        section { class: "whatif__group",
            h3 { class: "whatif__group-title", "{section.title()}" }
            div { class: "whatif__chips",
                for control in rows {
                    WhatIfChip {
                        key: "{control.stat}",
                        control: control.clone(),
                        active: activation.is_active(&session, control.stat),
                        magnitude: magnitude_of(&session, control.stat),
                        on_toggle: {
                            let mut activation = activation;
                            move |_| activation.toggle(session, control.stat)
                        },
                    }
                }
            }
        }
    }
}

/// One stat's current magnitude in the shared layer, zero when it carries none.
fn magnitude_of(session: &BuildSession, stat: &str) -> f64 {
    session
        .build
        .read()
        .combat
        .what_if_buffs
        .get(stat)
        .copied()
        .unwrap_or(0.0)
}

#[component]
fn WhatIfRow(control: WhatIfControl) -> Element {
    let session = use_context::<BuildSession>();
    let stat = control.stat;
    let magnitude = session
        .build
        .read()
        .combat
        .what_if_buffs
        .get(stat)
        .copied()
        .unwrap_or(0.0);

    rsx! {
        div {
            class: "whatif__row",
            class: if magnitude != 0.0 { "is-simulated" },
            style: "--row-hue: {control.family_token};",
            span { class: "whatif__label", "{control.label}" }
            // The slider is a convenience reach, NOT a bound: the number beside it is
            // unbounded, and the engine binds the archetype's real ceiling against whatever
            // either one writes. (The chain modal's own rows span the exported ceilings,
            // because there each stat's ceiling is already resolved for the build.)
            div { class: "slider whatif__slider",
                input {
                    r#type: "range",
                    min: "-{GENERIC_REACH}",
                    max: "{GENERIC_REACH}",
                    step: step_for(control.format),
                    value: "{magnitude.clamp(-GENERIC_REACH, GENERIC_REACH)}",
                    "aria-label": "{control.label} what-if buff",
                    oninput: move |evt| {
                        let Ok(value) = evt.value().parse::<f64>() else { return };
                        set_buff(session, stat, value);
                    },
                }
            }
            input {
                class: "whatif__input mono",
                r#type: "number",
                step: step_for(control.format),
                value: "{magnitude}",
                "aria-label": "{control.label} what-if buff, exact value",
                oninput: move |evt| {
                    let Ok(value) = evt.value().parse::<f64>() else { return };
                    set_buff(session, stat, value);
                },
            }
            span { class: "whatif__unit mono", "{control.format.render_delta(magnitude)}" }
        }
    }
}

/// How far the modal's sliders reach either way. A UI affordance, not a game number: this
/// modal's stats have not been resolved against a build yet (a mez magnitude and a +damage
/// percentage share the control), so there is no per-stat ceiling to span here.
const GENERIC_REACH: f64 = 200.0;
