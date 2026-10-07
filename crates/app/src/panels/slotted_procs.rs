//! The slotted-proc controls — one row per proc piece in a power, beside the power holding it
//! (the beta `SlottedProcControls`).
//!
//! Three controls, all writing [`coh_data::CharacterState::proc_overrides`] at the key
//! [`coh_math::procs::slotted_procs`] composes: an on/off switch that removes the piece from
//! every proc pass, a stack pin for a self-stacking buff (Might of the Tanker), and a %HP pin
//! for an HP-scaling global (Reactive Defenses). Auto is the default on both pins and it is not
//! a zero — a stacking buff contributes one DISCRETE stack, an HP-scaling one its always-on
//! floor. That is what makes "Auto" a real position rather than a way of saying "off".
//!
//! **What a row is contributing is measured, never claimed.** The labels come from the proc
//! breakdown the calc emitted for THIS build, so a row cannot describe an arithmetic that did
//! not happen — which matters most for the pieces the beta's own list drops: a chance-gated
//! damage proc feeds no dashboard total, and a PPM Performance Shifter feeds recovery even
//! though it is neither always-on nor variable (the beta's filter offers no row for it at all).
//!
//! Every piece the proc database resolves gets a row whatever it is contributing, because a row
//! that disappeared when switched off would be a control you could turn off and not back on.

use crate::build_session::BuildSession;
use crate::panels::stat_registry;
use coh_data::ProcOverride;
use coh_math::procs::{ChanceWorking, ProcBreakdownSource, ProcDamage, ProcRoll, SlottedProc};
use dioxus::prelude::*;

/// One row: the piece, its controls, and the stats it was measured contributing.
#[derive(Clone, PartialEq)]
pub struct SlottedProcRow {
    pub proc: SlottedProc,
    /// Dashboard labels for what this piece contributed on the last recalculate, deduped.
    /// Empty means it moved nothing — which is a fact about this build, not about the control.
    pub contributing: Vec<&'static str>,
}

/// Pair each slotted piece with what the calc measured it contributing.
///
/// Matched on `(set, proc, power)` rather than on the override key, because a breakdown source
/// records which piece produced it and not which slot held it. Two copies of one unique IO in
/// one power would therefore share a reading — a state the game's uniqueness rules make
/// unreachable, and the alternative is a slot index on the wire that nothing else needs.
pub fn rows(procs: Vec<SlottedProc>, breakdown: &[ProcBreakdownSource]) -> Vec<SlottedProcRow> {
    procs
        .into_iter()
        .map(|proc| {
            let contributing = breakdown
                .iter()
                .filter(|src| src.set_name == proc.set_name && src.proc_name == proc.io_name)
                .filter_map(|src| stat_registry::label_for_ledger_key(&src.breakdown_key))
                .fold(Vec::new(), |mut named, label| {
                    if !named.contains(&label) {
                        named.push(label);
                    }
                    named
                });
            SlottedProcRow { proc, contributing }
        })
        .collect()
}

#[component]
pub fn SlottedProcBlock(rows: Vec<SlottedProcRow>) -> Element {
    rsx! {
        div { class: "info-block",
            h4 { "Slotted procs" }
            for row in rows.iter() {
                SlottedProcRowView { key: "{row.proc.key}", row: row.clone() }
            }
        }
    }
}

#[component]
fn SlottedProcRowView(row: SlottedProcRow) -> Element {
    let session = use_context::<BuildSession>();
    let proc = row.proc.clone();
    let switch_key = proc.key.clone();
    let switch_mode = proc.mode.clone();
    let switch_stacks = proc.stacks;
    let switch_hp = proc.hp_pct;
    let damage_counted = proc_damage_counted(&row);

    rsx! {
        div {
            class: "proc-slot",
            class: if !proc.enabled { "is-off" },
            div { class: "proc-slot__head",
                span { class: "proc-slot__label",
                    "{proc.io_name}"
                    span { class: "proc-slot__set", "{proc.set_name}" }
                    SlottedProcNote { row: row.clone(), damage_counted }
                }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: proc.enabled,
                        onchange: move |evt: Event<FormData>| {
                            let on = evt.checked();
                            let key = switch_key.clone();
                            let mode = switch_mode.clone();
                            session
                                .commit(move |state| {
                                    state
                                        .proc_overrides
                                        .insert(
                                            key.clone(),
                                            ProcOverride {
                                                enabled: on,
                                                mode: mode.clone(),
                                                stacks: switch_stacks,
                                                hp_pct: switch_hp,
                                            },
                                        );
                                });
                        },
                    }
                    span { class: "switch-track" }
                }
            }
            div { class: "proc-slot__facts",
                ProcChance { roll: proc.roll.clone() }
                if let Some(damage) = &proc.damage {
                    ProcDamageLine { damage: damage.clone(), counted: damage_counted }
                }
                if let Some(mechanics) = &proc.mechanics {
                    span { class: "proc-slot__mechanics", "{mechanics}" }
                }
            }
            if proc.enabled {
                if let Some(max) = proc.max_stacks {
                    StackPin { proc: proc.clone(), max }
                }
                if proc.scaling {
                    HpPin { proc: proc.clone() }
                }
            }
        }
    }
}

/// How often the piece fires in this power (the beta's Proc Chance row, per piece). The hover
/// states the arithmetic with this power's own numbers, so a reader can check it by hand; the
/// rule a reader most often gets wrong rides with it: global recharge makes the power fire more
/// often, never the proc more likely.
#[component]
fn ProcChance(roll: ProcRoll) -> Element {
    let pct = |chance: f64| format!("{:.0}%", chance * 100.0);
    let (text, title, warn) = match roll {
        ProcRoll::AlwaysOn => ("always on".to_string(), "Granted by slotting it; nothing is rolled.".to_string(), false),
        ProcRoll::PerActivation { ppm, chance, rolls, fixed_period, working } => {
            let text = match rolls > 1.0 {
                true => format!("{} per roll × {} rolls per cast", pct(chance), rolls),
                false => format!("{} per activation", pct(chance)),
            };
            (text, chance_working(ppm, chance, rolls, fixed_period, working), false)
        }
        ProcRoll::PerCheck { ppm, chance, period } => (
            format!("{} every {}s", pct(chance), period),
            format!(
                "{ppm} PPM. An auto or toggle power checks its procs every {period}s while it \
                 runs.\n{ppm} × {period}s ÷ 60 = {:.1}%, clamped to {:.1}%–90% → {:.1}%",
                ppm * period / 60.0 * 100.0,
                (0.05 + ppm * 0.015) * 100.0,
                chance * 100.0,
            ),
            false,
        ),
        ProcRoll::Never { via_pet: true } => (
            "rolls on the pet, not this cast".to_string(),
            "This power is flagged to fire no procs itself; its summon copies the slotting and rolls it instead.".to_string(),
            true,
        ),
        ProcRoll::Never { via_pet: false } => (
            "never fires here".to_string(),
            "This power is flagged to fire no procs, so the piece only counts for its set bonuses.".to_string(),
            true,
        ),
        ProcRoll::Unrated => (
            "no proc rate in the data".to_string(),
            "The proc data names no PPM for this piece, so no chance is stated rather than a guessed one.".to_string(),
            true,
        ),
        ProcRoll::Unknown(why) => ("chance unknown".to_string(), why, true),
    };
    rsx! {
        span {
            class: "proc-slot__chance",
            class: if warn { "is-warn" },
            title: "{title}",
            "{text}"
        }
    }
}

/// The PPM formula with this power's numbers in it, one term per line, ending on the chance shown.
fn chance_working(
    ppm: f64,
    chance: f64,
    rolls: f64,
    fixed_period: bool,
    working: ChanceWorking,
) -> String {
    let ChanceWorking {
        start,
        slotted_recharge,
        window,
        cast_time,
        area_factor,
    } = working;
    let mut lines = vec![format!("{ppm} PPM")];
    lines.push(match fixed_period {
        true => format!(
            "Window: {start:.2}s, the piece's own period. The summoned patch rolls every {start:.0}s \
             while it lives, so recharge does not move it."
        ),
        false => format!(
            "Window: {start:.2}s base recharge ÷ (1 + {:.1}% slotted recharge) = {window:.2}s",
            slotted_recharge * 100.0
        ),
    });
    if cast_time > 0.0 {
        lines.push(format!("Cast time: {cast_time:.2}s"));
    }
    lines.push(match area_factor == 1.0 {
        true => "Area factor: 1.00 (single target)".to_string(),
        false => format!("Area factor: {area_factor:.3}"),
    });
    let raw = ppm * (window + cast_time) / (60.0 * area_factor);
    lines.push(format!(
        "{ppm} × ({window:.2} + {cast_time:.2}) ÷ (60 × {area_factor:.3}) = {:.1}%",
        raw * 100.0
    ));
    let floor = 0.05 + ppm * 0.015;
    if raw < floor || raw > 0.9 {
        lines.push(format!(
            "Clamped to {:.1}%–90% → {:.1}%",
            floor * 100.0,
            chance * 100.0
        ));
    }
    if rolls > 1.0 {
        lines.push(format!("{rolls} rolls per cast"));
    }
    if !fixed_period {
        lines.push(
            "Global recharge (set bonuses, Hasten) does not change the chance; it only makes \
             the power fire more often."
                .to_string(),
        );
    }
    lines.join("\n")
}

/// A damage proc's hit, and what it adds to one cast on average.
#[component]
fn ProcDamageLine(damage: ProcDamage, counted: bool) -> Element {
    let kind = damage.damage_type.as_deref().unwrap_or("damage");
    let text = match damage.per_cast {
        Some(per_cast) => format!(
            "{:.2} {kind} per hit · avg {:.2} per cast",
            damage.per_hit, per_cast
        ),
        None => format!("{:.2} {kind} per hit", damage.per_hit),
    };
    let title = match (damage.per_cast, counted) {
        (None, _) => {
            "Proc damage at your level. This power rolls on a clock rather than per cast, \
                      so the hit is not added to its damage."
                .to_string()
        }
        (Some(_), true) => "Proc damage at your level, times the chance per cast. Flat: damage \
                            enhancements and damage buffs do not raise it."
            .to_string(),
        (Some(_), false) => "Not added to this power's damage: proc damage is switched off on the \
                             Damage heading, or this piece or its category is switched off."
            .to_string(),
    };
    rsx! {
        span {
            class: "proc-slot__damage",
            class: if !counted { "is-off" },
            title: "{title}",
            "{text}"
        }
    }
}

/// What this row is doing right now, in the fewest words that stay true.
///
/// The category note comes first because it names the OTHER control: a piece switched on whose
/// category is off looks broken from here, and the fix is two surfaces away in Proc Settings.
///
/// A damage proc feeds the power's own damage rather than a dashboard total, so it says that
/// instead. A piece that only rolls a short effect (Force Feedback's 5s of recharge, a chance to
/// hold) says why it moves no total, because "nothing" reads as broken.
#[component]
fn SlottedProcNote(row: SlottedProcRow, damage_counted: bool) -> Element {
    let withheld = row.proc.disabled_categories.join(", ");
    let per_cast = row.proc.damage.as_ref().and_then(|damage| damage.per_cast);
    let rolled = matches!(
        row.proc.roll,
        ProcRoll::PerActivation { .. } | ProcRoll::PerCheck { .. }
    );
    rsx! {
        if !withheld.is_empty() {
            span { class: "proc-slot__note is-unshown", "{withheld} procs are switched off in Proc settings" }
        } else if !row.contributing.is_empty() {
            span { class: "proc-slot__note", "adds {row.contributing.join(\", \")}" }
        } else if let (true, Some(per_cast)) = (damage_counted, per_cast) {
            span { class: "proc-slot__note", "adds {per_cast:.1} damage per cast on average" }
        } else if row.proc.enabled && per_cast.is_some() {
            span { class: "proc-slot__note is-unshown", "proc damage is switched off on the Damage heading" }
        } else if row.proc.enabled && rolled {
            span { class: "proc-slot__note is-unshown", "fires a short effect on a roll; not counted in your totals" }
        } else if row.proc.enabled {
            span { class: "proc-slot__note is-unshown", "no steady contribution to your totals" }
        }
    }
}

/// Whether this piece's damage is in the power's damage right now: the reader's proc-damage
/// switch, the piece's own switch, and the Damage category in Proc settings all have to allow it.
fn proc_damage_counted(row: &SlottedProcRow) -> bool {
    let switched_on = try_use_context::<crate::damage_metric_store::ProcDamagePref>()
        .is_none_or(|pref| (pref.0)());
    switched_on && coh_math::procs::proc_damage_per_cast(std::slice::from_ref(&row.proc)) > 0.0
}

/// The discrete stack pin for a self-stacking buff.
///
/// Auto and 1 are different positions even where they resolve to the same number: Auto follows
/// the honest default if that default ever changes, and a pinned 1 is the reader saying they
/// have one. Zero is reachable through the pin and means "stacked none", which is not the same
/// as switching the piece off — the piece is still slotted and still counts for set bonuses.
#[component]
fn StackPin(proc: SlottedProc, max: u32) -> Element {
    let session = use_context::<BuildSession>();
    let pinned = (proc.mode == "stacks").then_some(proc.stacks).flatten();

    let write = move |proc: SlottedProc, stacks: Option<u32>| {
        session.commit(move |state| {
            state.proc_overrides.insert(
                proc.key.clone(),
                ProcOverride {
                    enabled: true,
                    mode: match stacks.is_some() {
                        true => "stacks".to_string(),
                        false => "auto".to_string(),
                    },
                    stacks,
                    hp_pct: proc.hp_pct,
                },
            );
        });
    };

    rsx! {
        div { class: "proc-slot__pin",
            span { class: "proc-slot__pin-label", "Stacks" }
            div { class: "combat-choices",
                button {
                    class: "combat-choice",
                    "aria-pressed": pinned.is_none(),
                    onclick: {
                        let proc = proc.clone();
                        move |_| write(proc.clone(), None)
                    },
                    "Auto"
                }
                for stacks in 0..=max {
                    button {
                        key: "{stacks}",
                        class: "combat-choice",
                        "aria-pressed": pinned == Some(stacks),
                        onclick: {
                            let proc = proc.clone();
                            move |_| write(proc.clone(), Some(stacks))
                        },
                        "{stacks}"
                    }
                }
            }
        }
    }
}

/// The %HP pin for an HP-scaling global. 100% is the always-on floor and near-0 the cap, which
/// is why the slider reads left-to-right as "hurt → healthy" rather than as a magnitude.
#[component]
fn HpPin(proc: SlottedProc) -> Element {
    let session = use_context::<BuildSession>();
    let pinned = (proc.mode == "hp").then_some(proc.hp_pct).flatten();
    let shown = pinned.unwrap_or(FULL_HEALTH);

    let write = move |proc: SlottedProc, hp_pct: Option<f64>| {
        session.commit(move |state| {
            state.proc_overrides.insert(
                proc.key.clone(),
                ProcOverride {
                    enabled: true,
                    mode: match hp_pct.is_some() {
                        true => "hp".to_string(),
                        false => "auto".to_string(),
                    },
                    stacks: proc.stacks,
                    hp_pct,
                },
            );
        });
    };

    rsx! {
        div { class: "proc-slot__pin",
            span { class: "proc-slot__pin-label", "Health" }
            button {
                class: "combat-choice",
                "aria-pressed": pinned.is_none(),
                onclick: {
                    let proc = proc.clone();
                    move |_| write(proc.clone(), None)
                },
                "Auto"
            }
            input {
                r#type: "range",
                min: "0",
                max: "100",
                step: "1",
                value: "{shown}",
                disabled: pinned.is_none(),
                oninput: {
                    let proc = proc.clone();
                    move |evt: Event<FormData>| {
                        if let Ok(pct) = evt.value().parse::<f64>() {
                            write(proc.clone(), Some(pct));
                        }
                    }
                },
            }
            button {
                class: "combat-choice",
                "aria-pressed": pinned.is_some(),
                onclick: {
                    let proc = proc.clone();
                    move |_| write(proc.clone(), Some(shown))
                },
                "{shown:.0}%"
            }
        }
    }
}

/// The %HP an unpinned HP-scaling global is read at — full health, its always-on floor.
const FULL_HEALTH: f64 = 100.0;
