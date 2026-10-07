//! The build's level, in the header.
//!
//! `CharacterState::level` was read in three load-bearing places — the level-gated slot
//! budget, the unlock badge on every available power, and the archetype-table context the
//! per-power projection resolves against — and written nowhere, so every build was silently
//! answering "what does this look like at 50?" while presenting itself as general. This is
//! that input's writer.
//!
//! It sits in the header rather than behind a popover (decision 2026-07-27, user-chosen)
//! because level is the slot counter's denominator: a value that moves a budget on screen
//! should not need a click to read.
//!
//! **Setting the level takes nothing away.** A power picked above it keeps its pick, a slot
//! placed past the budget keeps its enhancement, and the counter simply reads over
//! (`slot-counter--over`). That is both the beta's `setLevel` behaviour and the rule the
//! Available rows already state — a power that unlocks above the build's level is still
//! pickable, because a build is planned whole and then leveled into.
//!
//! Commit grain follows the combat panel's: the stepper commits per click, the slider on
//! drag release (`onchange`), so one gesture is one undo step rather than one per pixel. The
//! readout tracks the drag through a local signal — display only, no commit — because the
//! number is what the drag is aiming at.
//!
//! Beside the control sits [`LevelUpControl`] — level-up mode and its progression readout,
//! which turn the same level from a viewing lens into a walk the player takes one grant at a
//! time. See that component for what the mode gates.

use crate::build_session::BuildSession;
use crate::shell::Db;
use dioxus::prelude::*;

/// Level 1 is where every character starts; the ceiling is the dataset's own
/// ([`coh_data::LevelingSchedule::max_level`]), never written down here.
pub(crate) const MIN_LEVEL: u8 = 1;

/// Whether the planner is walking a character up through its levels rather than designing a
/// whole build at once. Lifted to the shell and provided as context because four surfaces read
/// it — this control, the available-power rows, the pool picker, and the enhancement picker —
/// and it is a UI mode, so it lives outside the build (see [`crate::level_up_store`]).
#[derive(Clone, Copy)]
pub struct LevelUpMode(pub Signal<bool>);

#[component]
pub fn LevelControl(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let level = session.build.read().level;
    // The level under the thumb mid-drag. `None` between gestures, so the committed level is
    // what shows; set on `oninput` and cleared by the `onchange` that commits it.
    let mut dragging = use_signal(|| Option::<u8>::None);

    // No dataset, no schedule, no ceiling — the same fail-loud branch the slot budget takes
    // (a dataset without a schedule blocks slot adds rather than inventing a bound).
    let ceiling = database
        .as_ref()
        .and_then(|database| database.leveling_schedule.as_ref())
        .and_then(|schedule| schedule.max_level());

    let (Some(database), Some(ceiling)) = (database, ceiling) else {
        return rsx! {
            div { class: "level-control is-unavailable",
                title: "This dataset has no leveling schedule, so the level range is unknown",
                span { class: "level-control__label", "Lvl" }
                span { class: "level-control__value mono", "—" }
            }
        };
    };

    let shown = dragging().unwrap_or(level);

    rsx! {
        div { class: "level-control",
            span { class: "level-control__label", "Lvl" }
            div { class: "stepper",
                button {
                    class: "step",
                    r#type: "button",
                    "aria-label": "Lower build level",
                    disabled: level <= MIN_LEVEL,
                    onclick: {
                        let database = database.clone();
                        move |_| set_level(session, &database, level.saturating_sub(1).max(MIN_LEVEL))
                    },
                    "−"
                }
                span { class: "level-control__value mono", "{shown}" }
                button {
                    class: "step",
                    r#type: "button",
                    "aria-label": "Raise build level",
                    disabled: level >= ceiling,
                    onclick: {
                        let database = database.clone();
                        move |_| set_level(session, &database, (level + 1).min(ceiling))
                    },
                    "+"
                }
            }
            div { class: "slider level-control__slider",
                input {
                    r#type: "range",
                    "aria-label": "Build level",
                    min: "{MIN_LEVEL}",
                    max: "{ceiling}",
                    step: "1",
                    value: "{shown}",
                    oninput: move |evt: Event<FormData>| {
                        if let Ok(next) = evt.value().parse::<u8>() {
                            dragging.set(Some(next.clamp(MIN_LEVEL, ceiling)));
                        }
                    },
                    onchange: {
                        let database = database.clone();
                        move |evt: Event<FormData>| {
                            dragging.set(None);
                            if let Ok(next) = evt.value().parse::<u8>() {
                                set_level(session, &database, next.clamp(MIN_LEVEL, ceiling));
                            }
                        }
                    },
                }
            }
        }
    }
}

/// Level-up mode and its progression readout — the header cluster that turns the level from a
/// lens you look through into a walk you take.
///
/// **What the mode gates** (chosen 2026-07-27, user-directed): a power pick whose schedule
/// level is above the build's level is refused; the enhancement picker withholds IO sets the
/// character couldn't craft yet; and switching the mode ON reads the build's level down to
/// [`coh_data::LevelingSchedule::progression_level`], so grants from levels the character
/// hasn't reached can't already be spent. Slots needed no new gate — the slot budget has been
/// level-gated since SE4, so `total_slots_at_level` already refuses a slot the level hasn't
/// granted.
///
/// **What it deliberately does NOT gate: the level control itself.** Forward motion stays free
/// (decision 2026-07-27, user-chosen) — the advance button is the sanctioned route, not the
/// only one, so someone who wants to jump ahead and come back keeps doing what the rest of the
/// app allows. The readout therefore informs rather than blocks: it says what this level still
/// owes, and offers the jump once nothing does.
///
/// The readout counts picks and slots by different means because the data differs — see
/// [`coh_data::level_progress`].
#[component]
pub fn LevelUpControl(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let mut mode = use_context::<LevelUpMode>().0;

    // A dataset with no schedule has no grants to walk, so the mode has nothing to mean — the
    // same fail-loud branch the level control and the slot budget take.
    let Some(database) = database else {
        return rsx! {};
    };
    let Some(schedule) = database.leveling_schedule.as_ref() else {
        return rsx! {
            div { class: "level-up is-unavailable",
                title: "This dataset has no leveling schedule, so there are no level grants",
                span { class: "level-up__label", "Level Up" }
            }
        };
    };

    // Every derived number, read before the markup so the borrow of the build ends here (the
    // click handlers below commit to it).
    let (level, progress, progression_target, next_pick) = {
        let build = session.build.read();
        let taken_levels: Vec<u8> = build.picked_powers().map(|power| power.level).collect();
        (
            build.level,
            coh_data::level_progress(schedule, &build),
            schedule.progression_level(
                build.picked_powers().count(),
                coh_data::placed_budget_slots(&build),
            ),
            // The rows' own rule with no unlock floor, so this is the pick a level-1 power would
            // take — the badge every Available row would wear if nothing held it back.
            schedule.next_pick_level(&taken_levels, MIN_LEVEL),
        )
    };

    // Switching ON reads the level down to what the build has actually spent; switching OFF
    // leaves the level alone, since nothing about the build was wrong — only the gates lift.
    let toggle = {
        let database = database.clone();
        move |_| {
            let turning_on = !*mode.peek();
            if turning_on {
                // Pin every slot the build already has to a real grant level before the level
                // below narrows the pool (the beta's order): a build planned freely has no
                // levelling order yet, and this is the moment one starts to matter.
                if let Some(schedule) = database.leveling_schedule.as_ref() {
                    let mut frozen = session.build.peek().clone();
                    if coh_data::slot_levels::freeze(schedule, &mut frozen) {
                        session.commit(move |state| *state = frozen);
                    }
                }
                if let Some(target) = progression_target {
                    if target < level {
                        set_level(session, &database, target);
                    }
                }
            }
            mode.set(turning_on);
        }
    };

    // Both readout tooltips, above the markup so the counts and their words agree in one place.
    let picks_title = format!(
        "Level {level} grants {} unspent power {}. Pick from the Available rows to spend {}.",
        progress.picks_owed,
        powers_word(progress.picks_owed),
        them_or_it(progress.picks_owed)
    );
    let slots_title = format!(
        "Level {level} grants {} unplaced enhancement {}. Add {} with a power card's +.",
        progress.slots_owed,
        slots_word(progress.slots_owed),
        them_or_it(progress.slots_owed)
    );

    if !mode() {
        return rsx! {
            button {
                class: "level-up level-up--off",
                r#type: "button",
                title: "Level Up mode: plan this character one level at a time: picks and \
                        enhancements are granted level by level",
                onclick: toggle,
                span { class: "level-up__glyph", "⇗" }
                span { class: "level-up__label", "Level Up" }
            }
            NextPickReadout { next_pick: next_pick }
        };
    }

    rsx! {
        div { class: "level-up level-up--on",
            button {
                class: "level-up__toggle",
                r#type: "button",
                title: "Turn off Level Up mode: picks and enhancements can be picked freely",
                onclick: toggle,
                span { class: "level-up__glyph", "⇗" }
                span { class: "level-up__label", "Level Up" }
            }
            span { class: "level-up__divider" }
            if progress.is_spent() {
                match progress.next_grant_level {
                    Some(next) => rsx! {
                        button {
                            class: "level-up__advance",
                            r#type: "button",
                            title: "Level {level} is fully spent; advance to level {next}",
                            onclick: {
                                let database = database.clone();
                                move |_| set_level(session, &database, next)
                            },
                            "→ Lvl {next}"
                        }
                    },
                    // The schedule grants nothing above this level: the walk is over.
                    None => rsx! {
                        span { class: "level-up__done", "At max level" }
                    },
                }
            } else {
                span { class: "level-up__owed-level", "Lvl {level}:" }
                if progress.picks_owed > 0 {
                    span {
                        class: "level-up__owed mono",
                        title: "{picks_title}",
                        "pick {progress.picks_owed} {powers_word(progress.picks_owed)}"
                    }
                }
                if progress.slots_owed > 0 {
                    span {
                        class: "level-up__owed mono",
                        title: "{slots_title}",
                        "place {progress.slots_owed} {slots_word(progress.slots_owed)}"
                    }
                }
            }
        }
    }
}

/// The level the next power pick lands at, beside Level Up while the mode is off.
///
/// An Available row's badge is the power's unlock level, so the slot a pick will fill was only
/// readable from a row's hover text, and players kept hovering a low-level power to find it.
/// With the mode on this stays hidden: the cluster already says what the level still owes.
#[component]
fn NextPickReadout(next_pick: Option<u8>) -> Element {
    match next_pick {
        Some(level) => rsx! {
            span {
                class: "next-pick",
                title: "Your next power pick fills the level {level} slot. A power that unlocks \
                        later takes the first free slot at or above its unlock level.",
                span { class: "next-pick__label", "Next pick" }
                span { class: "next-pick__value mono", "Lvl {level}" }
            }
        },
        None => rsx! {
            span {
                class: "next-pick is-done",
                title: "Every power pick the schedule grants is taken.",
                "All picks made"
            }
        },
    }
}

/// Whether level-up mode stands between this power and the pick it would take.
///
/// `Some(level)` = the pick lands on a level the character hasn't reached, so the mode refuses
/// it and the caller names that level in the reason. `None` = nothing in the way — the mode is
/// off, there is no pick to take (a separate refusal the caller already reports), or the pick
/// is at or below the build's level.
///
/// The gate is on the PICK's level, not the power's unlock level: those differ whenever an
/// earlier pick is already spent, and it is the pick being spent that the character has to have
/// earned. Both pickers route through here so the rule can't drift between them.
pub fn pick_beyond_level(mode_on: bool, pick_level: Option<u8>, current_level: u8) -> Option<u8> {
    pick_level
        .filter(|_| mode_on)
        .filter(|&level| level > current_level)
}

/// The readout says "pick 1 power" / "pick 2 powers" — the vocabulary the picker uses, not an
/// abbreviation of it (no domain abbreviations).
fn powers_word(count: u32) -> &'static str {
    if count == 1 {
        "power"
    } else {
        "powers"
    }
}

fn slots_word(count: u32) -> &'static str {
    if count == 1 {
        "slot"
    } else {
        "slots"
    }
}

/// What the tooltip's closing clause calls the thing it just counted.
fn them_or_it(count: u32) -> &'static str {
    if count == 1 {
        "it"
    } else {
        "them"
    }
}

/// Write the level as one undoable, persisted edit.
///
/// Both reconciles ride in the same commit because both read the build's level. One fork
/// hands Health and Stamina extra slots at fixed levels
/// ([`coh_data::LevelingSchedule::auto_granted_slot_levels`]), and `granted_inherents` reads
/// the level to decide how many. The grant rule's own level term
/// (`piAvailable[j] <= iLevel`) decides when the game hands a power over at all — a Kheldian
/// is issued the Dwarf attacks at 20 and not before — so a level change that skipped
/// [`crate::granted_powers::sync`] would leave the build holding, or missing, a form roster
/// its level no longer matches.
fn set_level(session: BuildSession, database: &Db, next: u8) {
    let database = database.clone();
    session.commit(move |state| {
        state.level = next;
        crate::inherents::sync(state, &database);
        crate::granted_powers::sync(state, &database);
    });
}
