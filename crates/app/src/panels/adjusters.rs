//! The per-power adjusters — the controls for state a power tracks about itself, rendered
//! beside the power that tracks it.
//!
//! Two families, rendered together and stored apart — because they answer to opposite rules
//! at the file boundary, and one map could not have said which a key was. Both are derived
//! from the export rather than enumerated here (Rule 0):
//!
//! * **Target state** — the conditionals a power declares about what it is hitting: a foe
//!   disintegrating under Beam Rifle, drowning under Water Blast, contaminated by Radiation
//!   Melee. These move the power's own projection, so they sit above the rows they move. Keyed
//!   `"<internalName>:<id>"` in [`coh_data::CombatContext::per_power_conditionals`]. World
//!   state: a shared build opens at the planner's default for them.
//! * **The buff-pet opt-in** — whether the caster is standing in the aura of a drone it
//!   summons. This one moves the BUILD TOTALS, which is why it says which ones. Keyed
//!   `"<powerset>:<internalName>:buffpet"` in [`coh_data::CombatContext::power_state`]. A
//!   switch thrown on the character, so it travels with a shared build.
//!
//! * **Targets hit** — how many foes an AoE landed on, or how many times a self-stacking buff
//!   has been applied. Consume Psyche's +Regen grows per foe; Build Up doubles. One count serves
//!   both, as in the engine ([`coh_math::stacking`]), and it is stored on the pick itself
//!   ([`coh_data::SelectedPower::targets_hit`]) because internal names are not unique.
//!
//! The build-wide half of the same vocabulary (stances, forms, global mechanics) is not here:
//! it is one decision for the whole character and it lives in the Combat popover
//! ([`crate::panels::combat`]). Two controls over one value can disagree; one cannot.

use crate::build_session::BuildSession;
use crate::panels::stat_registry;
use coh_math::adjusters::{toggle_writes, PowerAdjuster};
use coh_math::buff_pets::BuffPetSource;
use coh_math::stacking::{SliderKind, StackingSlider};
use dioxus::prelude::*;

/// Everything adjustable about one power, resolved for the live build.
///
/// Assembled by the caller rather than derived here, because the two halves answer to different
/// scopes: the conditionals describe whatever power is being READ (held or previewed), while the
/// buff-pet opt-in folds into totals that walk the build's own selections, so offering it for a
/// power the build does not hold would be a switch that moves nothing.
#[derive(Clone, PartialEq, Default)]
pub struct PowerAdjusters {
    pub conditionals: Vec<PowerAdjuster>,
    pub buff_pets: Vec<BuffPetSource>,
    /// The key the buff-pet opt-in writes, and whether it is on. `None` when the build does not
    /// hold this power.
    pub buff_pet_toggle: Option<(String, bool)>,
    /// The targets-hit count. `None` when the count moves nothing on this power, or when the
    /// build does not hold it — the count is stored on the pick, so a previewed power has
    /// nowhere to keep it.
    pub targets_hit: Option<TargetsHit>,
}

impl PowerAdjusters {
    pub fn is_empty(&self) -> bool {
        self.conditionals.is_empty() && self.buff_pets.is_empty() && self.targets_hit.is_none()
    }
}

/// One held pick's targets-hit input: which pick it writes, its range, and what it holds now.
#[derive(Clone, PartialEq)]
pub struct TargetsHit {
    pub powerset: String,
    pub internal_name: String,
    pub slider: StackingSlider,
    pub value: Option<u32>,
}

#[component]
pub fn AdjusterBlock(adjusters: PowerAdjusters) -> Element {
    // Grouped conditionals are a choice between target states and render as one choice strip;
    // ungrouped ones are independent switches. Grouping preserves the export's order — the
    // first member's position decides where its group sits.
    let mut groups: Vec<(String, Vec<PowerAdjuster>)> = Vec::new();
    let mut singles: Vec<PowerAdjuster> = Vec::new();
    for adjuster in &adjusters.conditionals {
        let Some(group) = adjuster.group.clone() else {
            singles.push(adjuster.clone());
            continue;
        };
        match groups.iter_mut().find(|(held, _)| *held == group) {
            Some((_, members)) => members.push(adjuster.clone()),
            None => groups.push((group, vec![adjuster.clone()])),
        }
    }

    rsx! {
        div { class: "info-block",
            h4 { "Adjusters" }
            for (name , members) in groups.iter() {
                AdjusterChoice {
                    key: "{name}",
                    name: name.clone(),
                    members: members.clone(),
                    siblings: adjusters.conditionals.clone(),
                }
            }
            for adjuster in singles.iter() {
                AdjusterRow {
                    key: "{adjuster.key}",
                    adjuster: adjuster.clone(),
                    siblings: adjusters.conditionals.clone(),
                }
            }
            if let Some(targets_hit) = &adjusters.targets_hit {
                TargetsHitRow { targets_hit: targets_hit.clone() }
            }
            if let Some((key , active)) = &adjusters.buff_pet_toggle {
                for pet in adjusters.buff_pets.iter() {
                    BuffPetRow {
                        key: "{pet.pet}",
                        pet: pet.clone(),
                        toggle: key.clone(),
                        active: *active,
                    }
                }
            }
        }
    }
}

/// One independent target-state switch, plus what it changes about the rows below it.
#[component]
fn AdjusterRow(adjuster: PowerAdjuster, siblings: Vec<PowerAdjuster>) -> Element {
    let session = use_context::<BuildSession>();
    let key = adjuster.key.clone();

    // An adjuster the export declares with nothing to apply cannot move a number. It is shown
    // as declared-and-inert rather than offered as a switch, because a control that does
    // nothing reads as a broken one, and hiding it would hide a gap in the export (Rule 1).
    if adjuster.is_inert() {
        return rsx! {
            div { class: "adjuster-row is-inert",
                span { class: "adjuster-label",
                    "{adjuster.label}"
                    span { class: "adjuster-note", "declared with nothing to apply" }
                }
            }
        };
    }

    rsx! {
        div { class: "adjuster-row",
            span { class: "adjuster-label",
                "{adjuster.label}"
                AdjusterContribution { adjuster: adjuster.clone() }
            }
            label { class: "switch",
                input {
                    r#type: "checkbox",
                    checked: adjuster.active,
                    onchange: move |evt: Event<FormData>| {
                        let on = evt.checked();
                        let key = key.clone();
                        let siblings = siblings.clone();
                        session
                            .commit(move |state| {
                                for (key, on) in toggle_writes(&siblings, &key, on) {
                                    state.combat.per_power_conditionals.insert(key, on);
                                }
                            });
                    },
                }
                span { class: "switch-track" }
            }
        }
    }
}

/// One group of mutually exclusive target states, as a pressed-state choice strip — the shape
/// the stance selector uses for the same reason: the states are alternatives, and a row of
/// switches would let a build claim several at once.
///
/// The cleared state leads, because a target is in none of them until something says otherwise.
#[component]
fn AdjusterChoice(
    name: String,
    members: Vec<PowerAdjuster>,
    siblings: Vec<PowerAdjuster>,
) -> Element {
    let session = use_context::<BuildSession>();
    let select = move |siblings: Vec<PowerAdjuster>, key: String, on: bool| {
        session.commit(move |state| {
            for (key, on) in toggle_writes(&siblings, &key, on) {
                state.combat.per_power_conditionals.insert(key, on);
            }
        });
    };
    let active = members.iter().find(|member| member.active);
    let cleared = (members.clone(), active.map(|member| member.key.clone()));

    rsx! {
        div { class: "adjuster-row adjuster-row-stack",
            span { class: "adjuster-label", "{name}" }
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
                        let key = member.key.clone();
                        rsx! {
                            button {
                                key: "{member.key}",
                                class: "combat-choice",
                                "aria-pressed": member.active,
                                title: contribution_text(member),
                                onclick: move |_| select(siblings.clone(), key.clone(), true),
                                "{member.label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// What flipping an adjuster on changes about the power, in the same words the rows use.
///
/// The two trailing lines are the ones that earn their place, and both exist because a switch
/// that moves no visible number reads as a broken switch. An additive conditional whose effect
/// collides with the power's own casts a SECOND simultaneous instance, and the merge keeps the
/// base row rather than folding two into one stronger one. An effect outside the display
/// vocabulary moves the calc but has no row of its own to move, so it is named by its wire key
/// rather than dropped (Rule 1).
#[component]
fn AdjusterContribution(adjuster: PowerAdjuster) -> Element {
    let shown = contribution_text(&adjuster);
    rsx! {
        if !shown.is_empty() {
            span { class: "adjuster-note", "{shown}" }
        }
        if !adjuster.extra_instances.is_empty() {
            span { class: "adjuster-note is-unshown",
                "second instance of {adjuster.extra_instances.join(\", \")}, not shown below"
            }
        }
        if !adjuster.unlabelled.is_empty() {
            span { class: "adjuster-note is-unshown",
                "also brings {adjuster.unlabelled.join(\", \")}, which has no row of its own"
            }
        }
    }
}

/// The "adds X, Y" phrase for one adjuster, empty when everything it brings is an unshowable
/// second instance.
fn contribution_text(adjuster: &PowerAdjuster) -> String {
    let mut named: Vec<String> = adjuster.adds.clone();
    if adjuster.adds_damage {
        named.push(DAMAGE_LABEL.to_string());
    }
    if named.is_empty() {
        return String::new();
    }
    let verb = match adjuster.replaces {
        true => "replaces",
        false => "adds",
    };
    format!("{verb} {}", named.join(", "))
}

/// A conditional's damage component has no registry entry to name it — `damage` is a wire field
/// of its own, not an `effects` key — so the block that shows it names it, as the card's own
/// damage block does.
const DAMAGE_LABEL: &str = "Damage";

/// The opt-in for one summoned buff-pet's aura.
///
/// Off by default and named as a stance the character takes: the drone is temporary and
/// positional, so counting its buff is a claim about where the character is standing, not a
/// property of the build. It says which totals it moves because it moves the DASHBOARD — unlike
/// every other control here, its effect is off-screen from the power that owns it.
#[component]
fn BuffPetRow(pet: BuffPetSource, toggle: String, active: bool) -> Element {
    let session = use_context::<BuildSession>();
    // Deduped, because the promise is about ROWS and a row may read several accumulator keys:
    // naming one row twice would read as two separate gains.
    let stats = pet
        .breakdown_keys
        .iter()
        .fold(Vec::new(), |mut named, key| {
            if let Some(label) = stat_registry::label_for_ledger_key(key) {
                if !named.contains(&label) {
                    named.push(label);
                }
            }
            named
        });

    rsx! {
        div { class: "adjuster-row",
            span { class: "adjuster-label",
                "Standing in {pet.pet}’s aura"
                if !stats.is_empty() {
                    span { class: "adjuster-note", "adds {stats.join(\", \")} to your totals" }
                }
            }
            label { class: "switch",
                input {
                    r#type: "checkbox",
                    checked: active,
                    onchange: move |evt: Event<FormData>| {
                        let on = evt.checked();
                        let toggle = toggle.clone();
                        session
                            .commit(move |state| {
                                state.combat.power_state.insert(toggle, on);
                            });
                    },
                }
                span { class: "switch-track" }
            }
        }
    }
}

/// The targets-hit count for one held pick, as a slider with its value beside it.
///
/// It shows the count the engine APPLIES, not the raw input: an untouched per-foe power reads
/// zero foes (its buff does not fire) and an untouched stacking buff reads one stack, and a
/// power whose aim or own sphere guarantees a target never reads below one. Showing the raw
/// absent value would put a slider at a position the numbers below it disagree with.
#[component]
fn TargetsHitRow(targets_hit: TargetsHit) -> Element {
    let session = use_context::<BuildSession>();
    let slider = targets_hit.slider;
    let shown = slider.effective(targets_hit.value);
    let label = match slider.kind {
        SliderKind::Targets => "Targets hit",
        SliderKind::Stacks => "Stacks",
    };
    // A count spanning more than one cast (Fulcrum Shift: 10 foes a cast, two casts deep) says
    // how many casts it stands for, since that is what the base buff multiplies with.
    let note = match slider.per_cast {
        Some(per_cast) if shown > 0 => {
            let casts = shown.div_ceil(per_cast);
            let noun = if casts == 1 { "cast" } else { "casts" };
            format!("{shown} of {} · {casts} {noun}", slider.max)
        }
        _ => format!("{shown} of {}", slider.max),
    };

    rsx! {
        div { class: "adjuster-row adjuster-row-stack",
            span { class: "adjuster-label",
                "{label}"
                span { class: "adjuster-note mono", "{note}" }
            }
            div { class: "slider",
                input {
                    r#type: "range",
                    min: "{slider.min}",
                    max: "{slider.max}",
                    step: "1",
                    value: "{shown}",
                    "aria-label": "{label}",
                    oninput: move |evt| {
                        let Ok(count) = evt.value().parse::<u32>() else {
                            return;
                        };
                        let powerset = targets_hit.powerset.clone();
                        let internal_name = targets_hit.internal_name.clone();
                        session
                            .commit(move |state| {
                                if let Some(pick) = state.selected_power_mut(&powerset, &internal_name) {
                                    pick.targets_hit = Some(count);
                                }
                            });
                    },
                }
            }
        }
    }
}
