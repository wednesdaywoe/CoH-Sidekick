//! Combat — the live [`coh_data::CombatContext`] controls: the build-level combat-state
//! inputs the totals pipeline reads (in-combat suppression, target-level purple patch, the
//! Fury/Vigilance additive-inherent inputs, the HP-scaling `kHitPoints%` runtime value, and
//! the exemplar-preview level), plus the caster-state controls that say which form of itself
//! the build is in — stances, caster modes, and the build-wide conditional mechanics. Every
//! control routes through [`BuildSession::commit`], so a change is one undoable, persisted
//! edit and the shared `recalculate` memo (`BuildTotals`, provided at the shell root)
//! recomputes from it.
//!
//! Rendered in the header's Combat popover (`shell::CombatPopover`), not on the grid, for the
//! same reason as build identity: a short form, dipped into and back out of. The trigger
//! carries a marker whenever any input here is off its baseline, since these silently move
//! every number the stat panels show.
//!
//! The caster-state controls are BUILD-DERIVED, not enumerated here: what a build can switch
//! between is read from the export by [`coh_data::caster_state`] and rendered as whatever it
//! finds, so no powerset, power or archetype is named on this surface (Rule 0). A build with
//! no stance to pick and no mode to enter shows neither section — a control that changes
//! nothing this build shows is a control that should not be on screen.
//!
//! Fury and Vigilance are archetype-specific mechanics, but this surfaces every control
//! unconditionally: which input a build consumes is a property of its archetype inherent's
//! expression (Fury reads `kRage`, Vigilance reads `source.TeamSize`), so gating visibility
//! is a data-derived predicate — never an archetype-name branch (Rule 0). Until that
//! predicate lands, moving an input a build ignores simply leaves the totals unchanged, which
//! is honest; the alternative (a proper-noun `if`) would be a defect.
//!
//! Sliders commit on `onchange` (drag release), not `oninput`: a range drag emits an event
//! per step, and every commit is an undo checkpoint, so `oninput` would bury a single gesture
//! under a hundred history entries. Discrete controls commit per click — the correct grain.

use crate::build_session::BuildSession;
use crate::shell::Db;
use coh_data::caster_state::{MechanicToggle, ModeToggle, StanceGroup};
use coh_data::{CombatContext, ContentMode, Level, PowerDatabase};
use dioxus::prelude::*;

/// Target-level offset bounds — the purple-patch lookup is defined over ±7 (beta
/// `targetLevelOffset` clamp).
const TARGET_LEVEL_MIN: i32 = -7;
const TARGET_LEVEL_MAX: i32 = 7;
/// Team size counts the character (1 = solo); [`coh_data::CharacterState::validate`] rejects
/// 0. The Vigilance table floors at four total, so eight is a generous ceiling.
const TEAM_SIZE_MIN: u32 = 1;
const TEAM_SIZE_MAX: u32 = 8;
/// Exemplar preview range (beta `exemplarLevel` 1..=50); toggling off is `None`.
const EXEMPLAR_MIN: u8 = 1;
const EXEMPLAR_MAX: u8 = 50;
const EXEMPLAR_DEFAULT: Level = Level::constant(50);

/// The caster-state controls this build can use, derived from the export.
#[derive(Clone, PartialEq, Default)]
struct CasterState {
    stances: Vec<StanceGroup>,
    modes: Vec<ModeToggle>,
    mechanics: Vec<MechanicToggle>,
}

impl CasterState {
    fn is_empty(&self) -> bool {
        self.stances.is_empty() && self.modes.is_empty() && self.mechanics.is_empty()
    }
}

/// One offerable target rank: the class token gates are evaluated against, and the reader-facing
/// name. Both come from the export — the name is the rank segment the class tokens are written
/// with, spelled out where the data abbreviates ([`coh_data::target_ranks`]).
#[derive(Clone, PartialEq)]
pub struct TargetOption {
    pub class: String,
    pub label: String,
}

/// The abbreviation the export writes the lieutenant rank with. Expanding it is presentation,
/// not data: the token stays `Class_Lt_*` everywhere it is evaluated.
const LIEUTENANT_SEGMENT: &str = "Lt";

/// The target ranks this dataset's gates distinguish, named for a reader.
///
/// Derived, not listed: a fork that ships a rank the others don't offers it without code, and
/// one whose archetype catalogue won't parse offers none rather than guessing — `target_ranks`
/// returns `Err` for that reason.
///
/// Shared, because the target is read in two places now: this panel sets it, and the powerset
/// comparison states which target its damage bars were resolved against. Two copies of the
/// abbreviation expansion is one of them going stale.
pub fn target_options(database: &PowerDatabase) -> Vec<TargetOption> {
    coh_data::target_ranks(database)
        .unwrap_or_default()
        .into_iter()
        .map(|rank| TargetOption {
            class: rank.representative().to_string(),
            label: if rank.segment == LIEUTENANT_SEGMENT {
                "Lieutenant".to_string()
            } else {
                rank.segment.clone()
            },
        })
        .collect()
}

/// The reader-facing name for a stored target class token, or the token itself when this
/// dataset's gates do not name it (a build carried across forks — rule 8's retain-and-report).
pub fn target_label(database: &PowerDatabase, class: &str) -> String {
    target_options(database)
        .into_iter()
        .find(|option| option.class == class)
        .map_or_else(|| class.to_string(), |option| option.label)
}

#[component]
pub fn Combat(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    // Exhaustive destructure: a new `CombatContext` field breaks this build until it gets a
    // control (fail-loud — no silently-unreachable calc input).
    let CombatContext {
        in_combat,
        enemy_level_offset,
        fury_level,
        vigilance_team_size,
        hit_points_percent,
        exemplar_level,
        // No Destiny-time scrubber in this panel yet; the totals default (sustained floor)
        // stands until one lands. Bound to `_` so a future field still breaks this destructure.
        destiny_time: _,
        // Same story, same panel: the Melee Hybrid's foe count has a control in the beta and
        // none here, so the totals read it as zero foes. It rides with `destiny_time` rather
        // than earning its own exemption — when one incarnate scrub gets a control here, both
        // should.
        hybrid_targets_hit: _,
        incarnate_level_shift,
        content_mode,
        // The from-Hide state used to drive one thing — which cast animation a single power's
        // display shows — so it was bound to `_` here with its control deferred to "beside the
        // power display". That control was never built, and RB5-d then gave the field a second
        // consumer that is whole-rotation rather than per-power: `chain_build` reads it as the
        // rotation's DECLARED opener state, the anchor the meter clock re-hides against. A
        // whole-rotation input belongs beside In combat, so it lands here. Worth knowing before
        // binding the next field to `_`: this destructure is the fail-loud guard against an
        // uncontrolled calc input, and an exemption with a rationale goes silent exactly when
        // the rationale stops being true — `hidden` was unreachable from the UI until RB5-d's
        // browser pass noticed.
        hidden,
        // The two caster-state maps ARE controlled here, but through the derived sections
        // below rather than field by field: what a build can switch between is read from the
        // export, so the controls are a list, not a fixed form. The two per-power maps are the
        // exception — both target state (a foe drowning, disintegrating) and the caster-side
        // opt-ins are tracked per power, so their controls live beside the power that tracks
        // them ([`crate::panels::adjusters`]).
        global_conditionals: _,
        active_modes: _,
        per_power_conditionals: _,
        power_state: _,
        target_class,
        target_is_player,
        pvp,
        // The what-if team-buff layer has its own modal ([`crate::panels::what_if`]) — it is a
        // whole vocabulary of controls rather than one, and keeping it out of this panel is
        // what stops a simulated number sitting among the build's real combat state. Bound to
        // `_` for the same reason as the fields above.
        what_if_buffs: _,
    } = session.build.read().combat.clone();

    // One derivation per (database, build) change: it walks the build's powers and their sets.
    let caster_state: Memo<CasterState> = use_memo(use_reactive!(|database| {
        let Some(database) = database else {
            return CasterState::default();
        };
        let build = session.build.read();
        CasterState {
            stances: coh_data::stance_groups(&build, &database),
            modes: coh_data::caster_modes(&build, &database),
            mechanics: coh_data::global_mechanics(&build, &database),
        }
    }));

    let target_options: Memo<Vec<TargetOption>> = use_memo(use_reactive!(|database| {
        database
            .as_ref()
            .map(|database| target_options(&database.0))
            .unwrap_or_default()
    }));

    // Whether the Hidden control appears at all — the same data-derived predicate the calc
    // binds `kMeter` through, never an archetype name (Rule 0). `kMeter` is one attribute ten
    // mechanics drive, so declaring "hidden" is only meaningful for a build whose own powers
    // publish the HIDE meter; for every other build the input moves nothing, and a control that
    // moves nothing should not be on screen (the convention the stance and mode sections below
    // already follow).
    let publishes_hide_meter: Memo<bool> = use_memo(use_reactive!(|database| {
        database.as_ref().is_some_and(|database| {
            coh_math::gather::publishes_hide_meter(&session.build.read(), &database.0)
        })
    }));

    // How far the equipped loadout can shift, and which slots pay for it — the same list Pass 6
    // spends ([`coh_math::incarnates::level_shift_grants`]), so the stepper below cannot offer a
    // step the calc refuses to spend.
    let level_shift_grants: Memo<Vec<coh_math::incarnates::LevelShiftGrant>> =
        use_memo(use_reactive!(|database| {
            database
                .as_ref()
                .map(|database| {
                    coh_math::incarnates::level_shift_grants(
                        &session.build.read().incarnates,
                        &database.0.incarnate_effects,
                    )
                })
                .unwrap_or_default()
        }));

    rsx! {
        div { class: "combat",
            // In combat — gates `suppressible` atoms (stealth/travel defense drops in combat).
            div { class: "combat-row",
                span { class: "combat-label", "In combat" }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: in_combat,
                        onchange: move |evt: Event<FormData>| {
                            let on = evt.checked();
                            session.commit(move |s| s.combat.in_combat = on);
                        },
                    }
                    span { class: "switch-track" }
                }
            }

            // Hidden — the declared from-Hide state. Two consumers, one input: the per-power
            // display picks the from-Hide cast animation over the mid-combat one, and a
            // rotation reads it as the opener's state, which the hide meter's own suppression
            // window then re-hides against between casts (RB5-d). Independent of In combat:
            // that one gates `suppressible` atoms, this one answers `kMeter source>` gates.
            if publishes_hide_meter() {
                div { class: "combat-row",
                    span { class: "combat-label", "Hidden" }
                    label { class: "switch",
                        input {
                            r#type: "checkbox",
                            checked: hidden,
                            onchange: move |evt: Event<FormData>| {
                                let on = evt.checked();
                                session.commit(move |s| s.combat.hidden = on);
                            },
                        }
                        span { class: "switch-track" }
                    }
                }
            }

            // Target level — enemy level relative to the character; feeds the purple-patch lookups.
            div { class: "combat-row",
                span { class: "combat-label", "Target level" }
                div { class: "stepper",
                    button {
                        class: "step",
                        "aria-label": "Lower target level",
                        disabled: enemy_level_offset <= TARGET_LEVEL_MIN,
                        onclick: move |_| session.commit(move |s| {
                            s.combat.enemy_level_offset =
                                (s.combat.enemy_level_offset - 1).max(TARGET_LEVEL_MIN);
                        }),
                        "−"
                    }
                    span { class: "combat-value mono", "{signed(enemy_level_offset)}" }
                    button {
                        class: "step",
                        "aria-label": "Raise target level",
                        disabled: enemy_level_offset >= TARGET_LEVEL_MAX,
                        onclick: move |_| session.commit(move |s| {
                            s.combat.enemy_level_offset =
                                (s.combat.enemy_level_offset + 1).min(TARGET_LEVEL_MAX);
                        }),
                        "+"
                    }
                }
            }

            // Level shift — how many of the loadout's earned incarnate shifts to read the build
            // with. Beside Target level because the two are one number to the purple patch
            // (`effective_level_diff = enemy_level_offset − level_shift`), and because they ask
            // the same question: what am I fighting, and as what.
            //
            // It exists because the earned shift and the granted shift are different things. A
            // full Alpha/Destiny/Lore loadout has earned +3, but only incarnate-flagged content
            // grants all three, so a build read at +3 everywhere overstates itself. Which content
            // grants what is a game rule the export does not carry — the planner must not guess
            // it (Rule 0) and must not quietly suppress shifts either, which would leave a number
            // moving for a reason nothing on screen states. So the player says.
            //
            // Absent when the loadout has earned nothing: a control that moves no total should
            // not be on screen, the convention the Hidden row and the caster-state sections keep.
            if !level_shift_grants().is_empty() {
                {
                    let grants = level_shift_grants();
                    let earned: f64 = grants.iter().map(|grant| grant.shift).sum();
                    // Unset READS as every earned shift, and stepping back up to every shift
                    // STORES unset again — so a build that later equips another shifting slot
                    // picks the new shift up instead of staying pinned by a ceiling set when it
                    // had fewer.
                    let applied = incarnate_level_shift.unwrap_or(earned).clamp(0.0, earned);
                    // The step is a GRANT, not a magnitude of one: the ceiling is spent down the
                    // grant list, so the reachable settings are its prefix sums and nothing
                    // between them. Stepping by 1.0 instead would assume every slot shifts by
                    // exactly one — true of all three forks today, and an assumption about the
                    // data rather than a reading of it.
                    let prefixes: Vec<f64> = std::iter::once(0.0)
                        .chain(grants.iter().scan(0.0, |sum, grant| {
                            *sum += grant.shift;
                            Some(*sum)
                        }))
                        .collect();
                    // Which prefix the stored ceiling sits on. `partition_point` lands on the
                    // last prefix at or below it, so a ceiling from a loadout that has since
                    // changed resolves to the nearest reachable setting rather than to nothing.
                    let index = prefixes.partition_point(|&prefix| prefix <= applied) - 1;
                    let lower = prefixes[index.saturating_sub(1)];
                    let upper = prefixes[(index + 1).min(prefixes.len() - 1)];
                    // The slots the current setting is spending, named, so "why is this +1 when I
                    // have three" is answered where it is asked and not only in the breakdown.
                    let spending = grants[..index]
                        .iter()
                        .map(|grant| grant.slot)
                        .collect::<Vec<_>>()
                        .join(", ");
                    rsx! {
                        div { class: "combat-row",
                            span { class: "combat-label", "Level shift" }
                            div { class: "stepper",
                                button {
                                    class: "step",
                                    "aria-label": "Read the build at a lower incarnate level shift",
                                    disabled: index == 0,
                                    onclick: move |_| session.commit(move |s| {
                                        s.combat.incarnate_level_shift = Some(lower);
                                    }),
                                    "−"
                                }
                                span { class: "combat-value mono", "+{applied}" }
                                button {
                                    class: "step",
                                    "aria-label": "Read the build at a higher incarnate level shift",
                                    disabled: index + 1 >= prefixes.len(),
                                    onclick: move |_| session.commit(move |s| {
                                        s.combat.incarnate_level_shift =
                                            (upper < earned).then_some(upper);
                                    }),
                                    "+"
                                }
                            }
                        }
                        div { class: "combat-row combat-row-sub",
                            span { class: "combat-hint",
                                if spending.is_empty() {
                                    "none of +{earned} earned"
                                } else {
                                    "{spending} — of +{earned} earned"
                                }
                            }
                        }
                    }
                }
            }

            // Incarnate content — the purple-patch defense-softcap lookup (Pass 8). Beside
            // Level shift on the same criterion: which content the build is being read
            // against is a live reading the player picks, not a build-design fact.
            // Engine-complete since purple_patch.rs shipped; this was the missing control.
            div { class: "combat-row",
                span { class: "combat-label", "Incarnate content" }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: content_mode == ContentMode::Incarnate,
                        onchange: move |evt: Event<FormData>| {
                            let mode = if evt.checked() {
                                ContentMode::Incarnate
                            } else {
                                ContentMode::Standard
                            };
                            session.commit(move |s| s.combat.content_mode = mode);
                        },
                    }
                    span { class: "switch-track" }
                }
            }

            // Target rank — which KIND of enemy the per-power damage rows are read against.
            // Unlike every other input here it moves no total: the totals have no one target, so
            // this reaches only the per-power projection, whose damage atoms are each gated on
            // who is being hit.
            //
            // It is only half the target. The other half — critter or player — is the switch
            // below, and it is answered whether or not a rank is chosen, which is why a fresh
            // build reads its damage instead of reporting it unresolved.
            if !target_options().is_empty() {
                div { class: "combat-row",
                    span { class: "combat-label", "Target" }
                    select {
                        class: "select-compact",
                        value: target_class.clone().unwrap_or_default(),
                        onchange: move |evt: Event<FormData>| {
                            let chosen = evt.value();
                            session.commit(move |s| {
                                s.combat.target_class =
                                    (!chosen.is_empty()).then(|| chosen.clone());
                            });
                        },
                        // An unstated rank is a real state, not a placeholder — but it is
                        // narrower than it used to be. It no longer means "no target": the
                        // PvE/PvP fork is still answered, and only the 137 powers whose numbers
                        // actually fork on rank (the crit tables) report themselves unresolved.
                        option { value: "", "Unspecified" }
                        for option in target_options() {
                            option { value: "{option.class}", "{option.label}" }
                        }
                    }
                }
            }

            // PvP — the `enttype target>` fork. Every fork's attacks carry a separate player-facing
            // scale; Homecoming carries separate `*_PvPDamage` tables for it too.
            div { class: "combat-row",
                span { class: "combat-label", "Target is a player" }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: target_is_player,
                        onchange: move |evt: Event<FormData>| {
                            let on = evt.checked();
                            session.commit(move |s| s.combat.target_is_player = on);
                        },
                    }
                    span { class: "switch-track" }
                }
            }

            // On a PvP map — the `isPVPMap?` a set-bonus tier's `Requires` asks about, and a
            // different question from the target fork above: a PvP set states a second bonus at
            // each piece count that applies by where you are, not by who you are hitting.
            div { class: "combat-row",
                span { class: "combat-label", "On a PvP map" }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: pvp,
                        onchange: move |evt: Event<FormData>| {
                            let on = evt.checked();
                            session.commit(move |s| s.combat.pvp = on);
                        },
                    }
                    span { class: "switch-track" }
                }
            }

            // Fury — Brute additive-damage meter (0–100); only builds whose inherent reads `kRage` respond.
            div { class: "combat-row",
                span { class: "combat-label", "Fury" }
                div { class: "slider combat-slider",
                    input {
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{fury_level}",
                        onchange: move |evt: Event<FormData>| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                session.commit(move |s| s.combat.fury_level = v.clamp(0.0, 100.0));
                            }
                        },
                    }
                    span { class: "combat-value mono", "{fury_level:.0}" }
                }
            }

            // Team size — Vigilance input; counts the character (1 = solo).
            div { class: "combat-row",
                span { class: "combat-label", "Team size" }
                div { class: "stepper",
                    button {
                        class: "step",
                        "aria-label": "Smaller team",
                        disabled: vigilance_team_size <= TEAM_SIZE_MIN,
                        onclick: move |_| session.commit(move |s| {
                            s.combat.vigilance_team_size =
                                s.combat.vigilance_team_size.saturating_sub(1).max(TEAM_SIZE_MIN);
                        }),
                        "−"
                    }
                    span { class: "combat-value mono", "{vigilance_team_size}" }
                    button {
                        class: "step",
                        "aria-label": "Larger team",
                        disabled: vigilance_team_size >= TEAM_SIZE_MAX,
                        onclick: move |_| session.commit(move |s| {
                            s.combat.vigilance_team_size =
                                (s.combat.vigilance_team_size + 1).min(TEAM_SIZE_MAX);
                        }),
                        "+"
                    }
                }
            }

            // Health % — the `kHitPoints%` runtime input for HP-scaling regen/recovery (Gamma Boost, Reactive Defenses).
            div { class: "combat-row",
                span { class: "combat-label", "Health %" }
                div { class: "slider combat-slider",
                    input {
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{hit_points_percent}",
                        onchange: move |evt: Event<FormData>| {
                            if let Ok(v) = evt.value().parse::<f64>() {
                                session.commit(move |s| {
                                    s.combat.hit_points_percent = v.clamp(0.0, 100.0)
                                });
                            }
                        },
                    }
                    span { class: "combat-value mono", "{hit_points_percent:.0}" }
                }
            }

            // Exemplar — preview level; below 45 it suppresses incarnate contributions (Pass 6). Off = not exemplared.
            div { class: "combat-row",
                span { class: "combat-label", "Exemplar" }
                label { class: "switch",
                    input {
                        r#type: "checkbox",
                        checked: exemplar_level.is_some(),
                        onchange: move |evt: Event<FormData>| {
                            let on = evt.checked();
                            session.commit(move |s| {
                                s.combat.exemplar_level = on.then_some(EXEMPLAR_DEFAULT);
                            });
                        },
                    }
                    span { class: "switch-track" }
                }
            }
            if let Some(level) = exemplar_level {
                div { class: "combat-row combat-row-sub",
                    span { class: "combat-label", "Exemplar level" }
                    div { class: "slider combat-slider",
                        input {
                            r#type: "range",
                            min: "{EXEMPLAR_MIN}",
                            max: "{EXEMPLAR_MAX}",
                            step: "1",
                            value: "{level}",
                            onchange: move |evt: Event<FormData>| {
                                if let Ok(v) = evt.value().parse::<u8>() {
                                    let v = v.clamp(EXEMPLAR_MIN, EXEMPLAR_MAX);
                                    session.commit(move |s| s.combat.exemplar_level = Level::new(v));
                                }
                            },
                        }
                        span { class: "combat-value mono", "{level}" }
                    }
                }
            }

            CasterStateSections { state: caster_state() }
        }
    }
}

/// The build-derived half of the panel: the stances it can take, the modes it can enter, and
/// the mechanics it can switch on. Absent entirely for a build that has none of them, and each
/// section absent on its own — an empty heading reads as a broken control, not as "none".
#[component]
fn CasterStateSections(state: CasterState) -> Element {
    if state.is_empty() {
        return rsx! {};
    }
    rsx! {
        if !state.stances.is_empty() {
            h4 { class: "combat-section", "Stance" }
            for group in state.stances.iter() {
                StanceRow { key: "{group.powerset}/{group.parent}", group: group.clone() }
            }
        }
        if !state.modes.is_empty() {
            h4 { class: "combat-section", "Form" }
            for mode in state.modes.iter() {
                ModeRow { key: "{mode.key}", mode: mode.clone() }
            }
        }
        if !state.mechanics.is_empty() {
            h4 { class: "combat-section", "Mechanics" }
            // A mechanic the export groups is a state the caster is IN, one of several, so it
            // renders as the same choice strip a stance does. Independent switches would let a
            // build claim every combo level at once — which the display would then merge.
            for (group , members) in grouped(&state.mechanics) {
                MechanicChoice {
                    key: "{group}",
                    group,
                    members,
                    siblings: state.mechanics.clone(),
                }
            }
            for mechanic in state.mechanics.iter().filter(|m| m.group.is_none()) {
                MechanicRow {
                    key: "{mechanic.id}",
                    mechanic: mechanic.clone(),
                    siblings: state.mechanics.clone(),
                }
            }
        }
    }
}

/// One parent power's mutually exclusive forms, as a pressed-state choice strip. The cleared
/// state leads because it is the form a build is in until it picks another, and some sets name
/// it themselves (an unloaded pistol still fires a standard round).
#[component]
fn StanceRow(group: StanceGroup) -> Element {
    rsx! {
        div { class: "combat-row combat-row-stack",
            span { class: "combat-label", "{group.parent_name}" }
            StanceChoices { group }
        }
    }
}

/// The choice strip itself, shared by this popover and the parent power's own card
/// ([`crate::panels::powers::PickedPowerCard`]), so the two surfaces write the one stance
/// through the one call and cannot disagree about which form the build is in.
#[component]
pub fn StanceChoices(group: StanceGroup) -> Element {
    let session = use_context::<BuildSession>();
    let select = move |group: StanceGroup, option: Option<String>| {
        session.commit(move |state| {
            coh_data::set_stance(state, &group, option.as_deref());
        });
    };
    let cleared = group.clone();
    rsx! {
        div {
            class: "combat-choices",
            role: "group",
            "aria-label": "{group.parent_name}",
            button {
                class: "combat-choice",
                r#type: "button",
                "aria-pressed": group.active.is_none(),
                onclick: move |_| select(cleared.clone(), None),
                "{group.cleared_label}"
            }
            for option in group.options.iter() {
                {
                    let chosen = group.clone();
                    let name = option.internal_name.clone();
                    rsx! {
                        button {
                            key: "{option.internal_name}",
                            class: "combat-choice",
                            r#type: "button",
                            "aria-pressed": group.active.as_deref() == Some(option.internal_name.as_str()),
                            onclick: move |_| select(chosen.clone(), Some(name.clone())),
                            "{option.name}"
                        }
                    }
                }
            }
        }
    }
}

/// One caster mode. The publishing power is named under it where the build holds one, since a
/// mode reads as an abstraction until it is tied to the power that turns it on.
#[component]
fn ModeRow(mode: ModeToggle) -> Element {
    let session = use_context::<BuildSession>();
    let key = mode.key.clone();
    rsx! {
        div { class: "combat-row",
            span { class: "combat-label",
                "{mode.label}"
                if let Some(source) = &mode.source {
                    span { class: "combat-hint", "{source}" }
                }
            }
            label { class: "switch",
                input {
                    r#type: "checkbox",
                    checked: mode.active,
                    onchange: move |evt: Event<FormData>| {
                        let on = evt.checked();
                        let key = key.clone();
                        session
                            .commit(move |state| {
                                if on {
                                    state.combat.active_modes.insert(key);
                                } else {
                                    state.combat.active_modes.remove(&key);
                                }
                            });
                    },
                }
                span { class: "switch-track" }
            }
        }
    }
}

/// The mechanics that belong to a mutual-exclusion group, in the export's own order — the first
/// member's position decides where its group sits among the ungrouped switches.
fn grouped(mechanics: &[MechanicToggle]) -> Vec<(String, Vec<MechanicToggle>)> {
    let mut groups: Vec<(String, Vec<MechanicToggle>)> = Vec::new();
    for mechanic in mechanics {
        let Some(group) = mechanic.group.clone() else {
            continue;
        };
        match groups.iter_mut().find(|(held, _)| *held == group) {
            Some((_, members)) => members.push(mechanic.clone()),
            None => groups.push((group, vec![mechanic.clone()])),
        }
    }
    groups
}

/// One group of mutually exclusive mechanics, as a pressed-state choice strip.
///
/// The cleared state leads and is always offered: a caster at no combo level is the state every
/// build starts in, and the group has no member that means "none".
#[component]
fn MechanicChoice(
    group: String,
    members: Vec<MechanicToggle>,
    siblings: Vec<MechanicToggle>,
) -> Element {
    let session = use_context::<BuildSession>();
    let select = move |siblings: Vec<MechanicToggle>, id: String, on: bool| {
        session.commit(move |state| coh_data::set_global_mechanic(state, &siblings, &id, on));
    };
    let active = members.iter().find(|member| member.active);
    let cleared = (siblings.clone(), active.map(|member| member.id.clone()));

    rsx! {
        div { class: "combat-row combat-row-stack",
            span { class: "combat-label", "{coh_data::group_label(&group)}" }
            div { class: "combat-choices",
                button {
                    class: "combat-choice",
                    "aria-pressed": active.is_none(),
                    onclick: move |_| {
                        let (siblings, live) = cleared.clone();
                        if let Some(live) = live {
                            select(siblings, live, false);
                        }
                    },
                    "None"
                }
                for member in members.iter() {
                    {
                        let siblings = siblings.clone();
                        let id = member.id.clone();
                        rsx! {
                            button {
                                key: "{member.id}",
                                class: "combat-choice",
                                "aria-pressed": member.active,
                                onclick: move |_| select(siblings.clone(), id.clone(), true),
                                "{member.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One build-wide conditional mechanic, labelled as the export labels it.
///
/// Takes the whole derived list, not just its own entry: a mechanic the export groups is one of
/// several states the caster can be in, so switching it on switches its siblings off in the same
/// commit ([`coh_data::set_global_mechanic`]).
#[component]
fn MechanicRow(mechanic: MechanicToggle, siblings: Vec<MechanicToggle>) -> Element {
    let session = use_context::<BuildSession>();
    let id = mechanic.id.clone();
    rsx! {
        div { class: "combat-row",
            span { class: "combat-label", "{mechanic.label}" }
            label { class: "switch",
                input {
                    r#type: "checkbox",
                    checked: mechanic.active,
                    onchange: move |evt: Event<FormData>| {
                        let on = evt.checked();
                        let id = id.clone();
                        let siblings = siblings.clone();
                        session.commit(move |state| {
                            coh_data::set_global_mechanic(state, &siblings, &id, on);
                        });
                    },
                }
                span { class: "switch-track" }
            }
        }
    }
}

/// Signed offset with an explicit `+` for positives (target-level display: "+3" / "0" / "-2").
fn signed(n: i32) -> String {
    if n > 0 {
        format!("+{n}")
    } else {
        format!("{n}")
    }
}

/// The names of the combat inputs that are off their baselines, in form order.
///
/// HM3: this is what the Build Settings trigger shows in place of a bare `•`.
/// It is always derived by diffing against [`CombatContext::default`] — never a
/// hand-maintained list — so it cannot disagree with what the form shows, and a
/// new field added to [`CombatContext`] appears here the moment it is off-baseline.
pub fn drift_summary(combat: &CombatContext) -> Vec<String> {
    let default = CombatContext::default();
    let mut parts: Vec<String> = Vec::new();

    if combat.in_combat != default.in_combat {
        parts.push("In combat".to_string());
    }
    if combat.enemy_level_offset != default.enemy_level_offset {
        parts.push(format!("Target {}", signed(combat.enemy_level_offset)));
    }
    if combat.fury_level != default.fury_level {
        parts.push("Fury".to_string());
    }
    if combat.vigilance_team_size != default.vigilance_team_size {
        parts.push(format!("Team {}", combat.vigilance_team_size));
    }
    if combat.hit_points_percent != default.hit_points_percent {
        parts.push(format!("Health {}", combat.hit_points_percent));
    }
    if combat.exemplar_level != default.exemplar_level {
        parts.push("Exemplar".to_string());
    }
    if combat.incarnate_level_shift != default.incarnate_level_shift {
        parts.push("Level shift".to_string());
    }
    if combat.content_mode != default.content_mode {
        parts.push("Incarnate content".to_string());
    }
    if combat.hidden != default.hidden {
        parts.push("Hidden".to_string());
    }
    if combat.pvp != default.pvp {
        parts.push("PvP map".to_string());
    }
    if combat.target_class != default.target_class {
        parts.push("Target".to_string());
    }
    if combat.target_is_player != default.target_is_player {
        parts.push("Player target".to_string());
    }

    parts
}
