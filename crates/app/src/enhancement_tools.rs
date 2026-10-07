//! The enhancement tools — one edit, every slot.
//!
//! A finished build's last chore is mechanical: forty pieces to lift to level 50, a dozen to
//! attune, every one of them to boost. Doing it in the picker is forty right-clicks. This is
//! that chore as one act — the beta's `EnhancementToolsModal`, which replaced its own one-shot
//! "Maximize Enhancements" confirm dialog for the same reason.
//!
//! Four axes, each independently opt-in: leave a row switched off and that axis of every piece
//! in the build is untouched. Apply commits all of them as one [`BuildSession::commit`], so one
//! undo puts the build back.
//!
//! # A piece is re-slotted by the rules that slotted it
//!
//! Nothing here decides for itself which pieces can take a booster or carry a craft level. The
//! picker already answers that when it builds a piece ([`Enhancement::io_set`]), and this asks
//! the same predicates of the built piece — [`Enhancement::takes_craft_level`],
//! [`Enhancement::takes_booster`], [`Enhancement::takes_attunement`]. A second copy of those
//! rules here is a second copy that can disagree with the picker about the same build, which is
//! the failure this codebase keeps paying for.
//!
//! The same holds for the fourth axis. `boost` is a booster combine on an IO and a SIGNED
//! RELATIVE LEVEL on an SO or a Hamidon — one field, two mechanics off two different curves —
//! and `coh_math`'s [`enhancement_level_axis`] is where that split is stated. This asks it
//! rather than re-deriving it from the kind, because the enhancement list has already been bitten
//! once by a surface that summed the two as if they were one number.
//!
//! # Two bands, both the dataset's
//!
//! A set piece's craft level is clamped into that set's OWN `[min_level, max_level]`, not into
//! a global one: a set that tops out at 30 cannot hold a level-50 piece, and writing 50 into it
//! would be the planner inventing an item. A set the loaded dataset does not carry has no band
//! to clamp into, so its pieces are left alone and counted as skipped rather than clamped
//! against a guess (Rule 1).
//!
//! The relative-level band is [`RelativeLevelBand`], read off the fork's own `above`/`below`
//! curves — Homecoming −3..+3, Rebirth −9..+4, and Thunderspy nothing at all, where the row is
//! absent rather than offering steps that all mean the same thing. Same rule the picker header
//! follows, from the same function.
//!
//! # What the beta's fourth control was, and why this one is different
//!
//! The beta offers "Special enhancement level", an ABSOLUTE 47–53, because its model gives a
//! Hamidon a level. This model does not: a special carries no craft level ([`Enhancement::special`]
//! leaves it `None`) and its strength comes from the relative level in `boost`. So the axis this
//! row edits is the relative level, and it reaches origin enhancements too — the beta excludes
//! SOs from its tools entirely, but SOs sit on exactly the same axis here, and excluding them
//! would be a rule about nothing.
//!
//! # The preview is the same walk
//!
//! Each row says how many pieces it would change before you press anything, and the count comes
//! from [`Retune::preview`] — the same per-piece function [`Retune::apply`] runs, over the same
//! walk. A preview computed separately from the edit is a preview that can lie about the edit;
//! here they cannot disagree, and the gate below is the proof.
//!
//! It also makes "nothing would change" a state the surface can show: Apply is dead when the
//! build is already what the plan asks for, instead of the beta's silent no-op.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::picker_defaults::{CraftBand, RelativeLevelBand, BOOST_MAX};
use crate::shell::Db;
use coh_data::io_sets::IoSetCatalog;
use coh_data::{CharacterState, Enhancement, Level, PowerDatabase};
use coh_math::enhancement::{enhancement_level_axis, EnhancementLevelAxis};
use dioxus::prelude::*;

// ============================================================
// The plan.
// ============================================================

/// What the user asked for. Every axis is opt-in: `None` (or `false`) leaves that axis of every
/// piece in the build exactly as it is, which is what makes four independent switches safe to
/// commit as one edit.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Retune {
    /// Craft level for non-attuned IOs, clamped per set.
    pub craft_level: Option<u8>,
    /// Attune every set piece that is not already attuned.
    pub attune_sets: bool,
    /// Enhancement Booster level for pieces that can take one.
    pub booster: Option<u8>,
    /// Relative level for origin and special enhancements.
    pub relative_level: Option<i8>,
}

/// How many pieces one axis of a plan actually moves. The tally the rows show, and the tally
/// Apply is enabled by.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Touched {
    pub craft_level: usize,
    pub attuned: usize,
    pub booster: usize,
    pub relative_level: usize,
    /// Pieces the craft-level axis passed over because they name an IO set the loaded dataset
    /// does not carry, so the band to clamp into is unknown. Surfaced, never guessed.
    pub unresolved_sets: usize,
}

impl Touched {
    fn add(&mut self, other: Touched) {
        self.craft_level += other.craft_level;
        self.attuned += other.attuned;
        self.booster += other.booster;
        self.relative_level += other.relative_level;
        self.unresolved_sets += other.unresolved_sets;
    }

    /// Pieces this plan would move, across every axis. A piece moved on two axes counts twice —
    /// this is the figure that answers "is there anything to do", not "how many pieces change".
    fn edits(&self) -> usize {
        self.craft_level + self.attuned + self.booster + self.relative_level
    }
}

impl Retune {
    /// Whether the user switched anything on. Distinct from [`Touched::edits`] being zero: a
    /// plan with nothing enabled has nothing to say, where an enabled plan with no edits is
    /// saying the build already looks like this.
    fn is_empty(&self) -> bool {
        self.craft_level.is_none()
            && !self.attune_sets
            && self.booster.is_none()
            && self.relative_level.is_none()
    }

    /// What this plan would change, without changing it.
    pub fn preview(
        &self,
        build: &CharacterState,
        database: &PowerDatabase,
        band: Option<RelativeLevelBand>,
    ) -> Touched {
        let craft = CraftBand::from_boost_index(database.boost_index.as_ref());
        let mut total = Touched::default();
        for power in build.all_selected() {
            for piece in power.slots.iter().flatten() {
                let mut copy = piece.clone();
                total.add(self.retune_piece(
                    &mut copy,
                    database.io_sets.as_ref(),
                    craft.as_ref(),
                    band,
                ));
            }
        }
        total
    }

    /// Apply this plan to every slotted piece in the build, returning what moved.
    ///
    /// Walks [`CharacterState::all_selected_mut`] rather than naming the buckets, so a piece in
    /// a granted inherent's slot is re-slotted like any other — the beta's five-bucket walk is
    /// the shape that leaves pockets of a build silently untouched.
    pub fn apply(
        &self,
        build: &mut CharacterState,
        database: &PowerDatabase,
        band: Option<RelativeLevelBand>,
    ) -> Touched {
        let catalog = database.io_sets.as_ref();
        let craft = CraftBand::from_boost_index(database.boost_index.as_ref());
        let mut total = Touched::default();
        for power in build.all_selected_mut() {
            for piece in power.slots.iter_mut().flatten() {
                total.add(self.retune_piece(piece, catalog, craft.as_ref(), band));
            }
        }
        total
    }

    /// One piece, re-slotted. The single place any axis is decided — `preview` and `apply` are
    /// the same walk over this, differing only in whether they keep the result.
    ///
    /// Attunement runs first because it REMOVES the two things the next steps write: a catalyst
    /// takes the piece's craft level away (it scales with the character now) and burns off any
    /// boosters already combined in. Running the level or booster step against the pre-attuned
    /// state would write a value the attunement then contradicts.
    fn retune_piece(
        &self,
        piece: &mut Enhancement,
        catalog: Option<&IoSetCatalog>,
        craft: Option<&CraftBand>,
        band: Option<RelativeLevelBand>,
    ) -> Touched {
        let mut touched = Touched::default();

        if self.attune_sets && piece.takes_attunement() && !piece.attuned {
            piece.attuned = true;
            piece.level = None;
            piece.boost = 0;
            touched.attuned += 1;
        }

        // No roster, no levels to re-craft to: a dataset that states none leaves the axis
        // alone rather than writing a level invented here.
        if let (Some(requested), Some(craft)) = (self.craft_level, craft) {
            match craft_level_for(piece, requested, craft, catalog) {
                CraftLevel::Skip => {}
                CraftLevel::Unresolved => touched.unresolved_sets += 1,
                CraftLevel::At(level) => {
                    if piece.level != Some(level) {
                        piece.level = Some(level);
                        // Re-crafting below the booster floor burns the boosters with it,
                        // the way attuning does — the sub-50 copy cannot hold them.
                        if piece.holds_refused_booster() {
                            piece.boost = 0;
                        }
                        touched.craft_level += 1;
                    }
                }
            }
        }

        if let Some(requested) = self.booster {
            // The booster axis is unsigned in the game and signed in the field, because the
            // field is shared with the relative level. The cast is the widening, not a clamp.
            let target = i8::try_from(requested.min(BOOST_MAX)).unwrap_or(i8::MAX);
            if piece.takes_booster() && piece.boost != target {
                piece.boost = target;
                touched.booster += 1;
            }
        }

        if let (Some(requested), Some(band)) = (self.relative_level, band) {
            let target = requested.clamp(band.min, band.max);
            let relative = enhancement_level_axis(&piece.kind) == EnhancementLevelAxis::Relative;
            if relative && piece.boost != target {
                piece.boost = target;
                touched.relative_level += 1;
            }
        }

        touched
    }
}

/// What the craft-level axis has to say about one piece.
enum CraftLevel {
    /// This piece carries no craft level to write — attuned, or an SO or a Hamidon.
    Skip,
    /// A set piece whose set is not in the loaded dataset, so its band is unknown.
    Unresolved,
    /// The level to write, already inside every band that applies.
    At(Level),
}

/// The level `requested` becomes for this piece.
///
/// Two clamps, in order. The picker's own IO band first, which is the domain of the control the
/// number came from; then, for a set piece, that SET's band — a level-50 piece of a set that
/// tops out at 30 is not an item, and clamping is the game's answer here rather than a
/// convenience (the picker clamps the same way at pick time).
fn craft_level_for(
    piece: &Enhancement,
    requested: u8,
    band: &CraftBand,
    catalog: Option<&IoSetCatalog>,
) -> CraftLevel {
    if !piece.takes_craft_level() {
        return CraftLevel::Skip;
    }
    let requested = i64::from(requested.clamp(band.min(), band.max()));

    let banded = match &piece.kind {
        coh_data::EnhancementKind::IoSet { set_id, .. } => {
            let Some(set) = catalog.and_then(|catalog| catalog.get(set_id)) else {
                return CraftLevel::Unresolved;
            };
            requested.min(set.max_level).max(set.min_level)
        }
        // A common IO exists only at the roster's own levels, so it lands on the step at or
        // below the request rather than between two recipes (BOOST-6).
        coh_data::EnhancementKind::GenericIo { .. } => {
            i64::from(band.crafted_at_or_below(requested as u8))
        }
        _ => requested,
    };

    // A catalog row of `0` would spell the build's global IO level, which an explicit re-level
    // is not asking for; there is no level to write, so nothing is written.
    match Level::from_i64(banded) {
        Some(level) => CraftLevel::At(level),
        None => CraftLevel::Skip,
    }
}

// ============================================================
// Under-level: the same rule, read rather than written.
// ============================================================

/// The level this piece would reach if the build were finished, when that is ABOVE the level it
/// carries now. `Some(level)` is the slot badge's whole condition; `None` says the piece is
/// already as high as it goes, or carries no craft level to raise.
///
/// This calls [`craft_level_for`] rather than restating what it decides. The badge and the tool
/// that clears it are then the same rule seen twice: a piece cannot be marked as raisable and
/// then left alone by Apply, or cleaned by Apply while its slot never said anything. That
/// pairing is the point — a build sitting at 47 is invisible until something says so, and a
/// mark that disagreed with the fix would be worse than no mark.
///
/// Three states deliberately go unmarked:
///
///  - **A set the loaded dataset does not carry** ([`CraftLevel::Unresolved`]). There is no band
///    to measure against, and a badge is a claim. The tools count the same pieces as
///    `unresolved_sets` rather than clamping them to a guess.
///  - **A piece with no craft level at all** — attuned, an SO, a Hamidon. Attuned pieces scale
///    with the character and the other two ride the relative-level axis, so "below 50" is not a
///    thing that can be true of them.
///  - **A piece whose `level` is `None`.** That is not an unstated level, it is a stated
///    deferral: `coh_math`'s `slot.level.unwrap_or(global_io_level)` resolves it to the build's
///    own level, so the piece is already worth as much as this character can make it.
pub fn under_craft_level(
    piece: &Enhancement,
    band: &CraftBand,
    catalog: Option<&IoSetCatalog>,
) -> Option<Level> {
    let CraftLevel::At(ceiling) = craft_level_for(piece, band.max(), band, catalog) else {
        return None;
    };
    piece.level.filter(|now| *now < ceiling).map(|_| ceiling)
}

/// How many slotted pieces in the build [`under_craft_level`] marks — the figure the quickbar
/// action carries, so the count and the badges are one walk's worth of the same answer.
///
/// Walks [`CharacterState::all_selected`] for the reason [`Retune::apply`] does: naming the
/// buckets is how a piece in a granted inherent's slot goes uncounted.
pub fn under_level_count(build: &CharacterState, database: &PowerDatabase) -> usize {
    let Some(band) = CraftBand::from_boost_index(database.boost_index.as_ref()) else {
        // No roster, no ceiling to be short of — a badge would be a claim about a band the
        // dataset never stated.
        return 0;
    };
    let catalog = database.io_sets.as_ref();
    build
        .all_selected()
        .flat_map(|power| power.slots.iter().flatten())
        .filter(|piece| under_craft_level(piece, &band, catalog).is_some())
        .count()
}

// ============================================================
// The modal.
// ============================================================

/// The tools' open state, held at the shell root for the containment reason every modal here
/// shares: a `fixed` backdrop is contained by the grid's `transform`ed surfaces (see
/// [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct EnhancementToolsOpen(pub Signal<bool>);

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn EnhancementToolsHost(database: Option<Db>) -> Element {
    let mut open = use_context::<EnhancementToolsOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Enhancement tools".to_string(),
            size: ModalSize::Md,
            on_close: move |_| open.set(false),
            EnhancementToolsBody { database }
        }
    }
}

#[component]
fn EnhancementToolsBody(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let mut open = use_context::<EnhancementToolsOpen>().0;

    // The four rows' state. Levels default to the top of each band because that is the chore
    // this surface exists for — a build is finished at 50, and the tool's job is getting there
    // in one act. Attunement defaults off: it is the only one of the four that costs a real
    // in-game consumable, so it is opted into rather than out of.
    let mut craft_level_on = use_signal(|| true);
    // `0` is the unset sentinel (coh_data's own convention for a level): the top of the band
    // is not knowable until the dataset resolves, so it is read from the roster below rather
    // than typed in here.
    let mut craft_level = use_signal(|| 0u8);
    let mut attune_on = use_signal(|| false);
    let mut booster_on = use_signal(|| false);
    let mut booster = use_signal(|| BOOST_MAX);
    let mut relative_on = use_signal(|| false);
    let mut relative = use_signal(|| 0i8);

    // Every hook runs before the early return below: a `use_memo` reached only on some renders
    // shifts the hook index of everything after it, and this branch flips during a session when
    // the dataset resolves under an already-open modal.
    let loading = database.is_none();
    let band = use_memo(use_reactive!(|database| database
        .as_ref()
        .and_then(|db| RelativeLevelBand::from_curves(db.0.enhancement_curves.as_ref()))
        .filter(|band| !band.is_even_only())));
    // The crafting levels the dataset's boost index names — the Craft level row's whole domain,
    // and its default (BOOST-6).
    let craft_band = use_memo(use_reactive!(|database| database
        .as_ref()
        .and_then(|db| CraftBand::from_boost_index(db.0.boost_index.as_ref()))));
    let craft_floor = craft_band().as_ref().map_or(0, CraftBand::min);
    let craft_ceiling = craft_band().as_ref().map_or(0, CraftBand::max);
    let craft_target = match craft_level() {
        0 => craft_ceiling,
        stated => stated,
    };

    let plan = use_memo(move || Retune {
        craft_level: if craft_level_on() && craft_target > 0 {
            Some(craft_target)
        } else {
            None
        },
        attune_sets: attune_on(),
        booster: if booster_on() { Some(booster()) } else { None },
        relative_level: if relative_on() && band().is_some() {
            Some(relative())
        } else {
            None
        },
    });

    // Cloned for Apply before the memo takes it: `use_reactive!` moves its captures, and the
    // handler needs the same database the preview read.
    let applies_to = database.clone();
    let touched = use_memo(use_reactive!(|(database, plan)| match &database {
        Some(db) => plan().preview(&session.build.read(), &db.0, band()),
        None => Touched::default(),
    }));

    if loading {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    }

    let nothing_enabled = plan().is_empty();
    let nothing_to_do = touched().edits() == 0;

    rsx! {
        div { class: "enh-tools",
            p { class: "enh-tools__lede",
                "Each switch edits one thing about every enhancement in the build. \
                 Leave one off and that part of the build is untouched. Apply is one undo."
            }

            ToolRow {
                label: "Craft level",
                description: "Every non-attuned IO, common or set. A set piece lands inside its own set's level range — a level-50 piece of a set that stops at 30 is not an item.",
                enabled: craft_level_on(),
                on_toggle: move |on| craft_level_on.set(on),
                changed: touched().craft_level,
                note: match touched().unresolved_sets {
                    0 => None,
                    n => Some(format!(
                        "{n} left alone — their set is not in this dataset, so its level range is unknown",
                    )),
                },
                Stepper {
                    label: "Craft level",
                    value: i64::from(craft_target),
                    min: i64::from(craft_floor),
                    max: i64::from(craft_ceiling),
                    enabled: craft_level_on() && craft_ceiling > 0,
                    display: format!("Level {craft_target}"),
                    on_change: move |next: i64| craft_level.set(next as u8),
                }
            }

            ToolRow {
                label: "Attune every set piece",
                description: "What a catalyst does: the piece scales with your level instead of a fixed one. It also takes the piece's craft level and any boosters already in it, so those are cleared here too.",
                enabled: attune_on(),
                on_toggle: move |on| attune_on.set(on),
                changed: touched().attuned,
                note: None,
            }

            ToolRow {
                label: "Enhancement boosters",
                description: "Applied to every IO that can take one. Attuned pieces and pure procs cannot — an attuned piece burns its boosters, and a proc has no magnitude to raise.",
                enabled: booster_on(),
                on_toggle: move |on| booster_on.set(on),
                changed: touched().booster,
                note: None,
                Stepper {
                    label: "Booster level",
                    value: i64::from(booster()),
                    min: 0,
                    max: i64::from(BOOST_MAX),
                    enabled: booster_on(),
                    display: match booster() {
                        0 => "None".to_string(),
                        n => format!("+{n}"),
                    },
                    on_change: move |next: i64| booster.set(next as u8),
                }
            }

            // Absent, not disabled, on a fork whose curves are flat: there is no band to step
            // through there, and a control over a one-value domain is a control that lies about
            // having an effect.
            match band() {
                Some(band) => rsx! {
                    ToolRow {
                        label: "Relative level",
                        description: "Origin and special enhancements — how far above or below your combat level they sit. This fork attenuates to {band.min} and rewards to +{band.max}.",
                        enabled: relative_on(),
                        on_toggle: move |on| relative_on.set(on),
                        changed: touched().relative_level,
                        note: None,
                        Stepper {
                            label: "Relative level",
                            value: i64::from(relative()),
                            min: i64::from(band.min),
                            max: i64::from(band.max),
                            enabled: relative_on(),
                            display: match relative() {
                                0 => "Even".to_string(),
                                n if n > 0 => format!("+{n}"),
                                n => format!("{n}"),
                            },
                            on_change: move |next: i64| relative.set(next as i8),
                        }
                    }
                },
                None => rsx! {
                    p { class: "enh-tools__absent",
                        "This fork applies no penalty or bonus for an enhancement's level relative to \
                         yours, so there is nothing to set for origin and special enhancements."
                    }
                },
            }

            div { class: "enh-tools__actions",
                span { class: "enh-tools__verdict",
                    if nothing_enabled {
                        "Switch something on."
                    } else if nothing_to_do {
                        "The build is already like this."
                    } else if touched().edits() == 1 {
                        "1 change across the build."
                    } else {
                        "{touched().edits()} changes across the build."
                    }
                }
                button {
                    class: "seg",
                    r#type: "button",
                    onclick: move |_| open.set(false),
                    "Cancel"
                }
                button {
                    class: "seg is-primary",
                    r#type: "button",
                    disabled: nothing_enabled || nothing_to_do,
                    onclick: move |_| {
                        let Some(db) = applies_to.clone() else { return };
                        let plan = plan();
                        let band = band();
                        session.commit(|build| {
                            plan.apply(build, &db.0, band);
                        });
                        open.set(false);
                    },
                    "Apply to the build"
                }
            }
        }
    }
}

/// One axis: a switch, what it does, and how many pieces it would move. The count is the row's
/// own answer before you press anything, which is the difference between a bulk edit you can
/// see the size of and one you find out about afterwards.
#[component]
fn ToolRow(
    label: String,
    description: String,
    enabled: bool,
    on_toggle: EventHandler<bool>,
    changed: usize,
    /// Anything the axis has to report about pieces it could not act on.
    note: Option<String>,
    children: Element,
) -> Element {
    rsx! {
        div { class: if enabled { "enh-tools__row" } else { "enh-tools__row is-off" },
            label { class: "enh-tools__switch",
                input {
                    r#type: "checkbox",
                    checked: enabled,
                    onchange: move |evt| on_toggle.call(evt.checked()),
                }
                span { class: "enh-tools__label", "{label}" }
            }
            p { class: "enh-tools__description", "{description}" }
            div { class: "enh-tools__control",
                {children}
                if enabled {
                    span { class: if changed == 0 { "enh-tools__changed is-none" } else { "enh-tools__changed" },
                        match changed {
                            0 => "nothing to change".to_string(),
                            1 => "1 piece".to_string(),
                            n => format!("{n} pieces"),
                        }
                    }
                }
            }
            if let Some(note) = note {
                p { class: "enh-tools__note", "{note}" }
            }
        }
    }
}

/// A range plus its reading. The number beside the track is the whole point on a band that runs
/// negative: `-3` and `+3` are three steps apart on the track and opposite in meaning, and a
/// bare slider position says neither.
#[component]
fn Stepper(
    label: String,
    value: i64,
    min: i64,
    max: i64,
    enabled: bool,
    display: String,
    on_change: EventHandler<i64>,
) -> Element {
    rsx! {
        div { class: "slider enh-tools__slider",
            input {
                r#type: "range",
                min: "{min}",
                max: "{max}",
                step: "1",
                value: "{value}",
                disabled: !enabled,
                "aria-label": "{label}",
                oninput: move |evt| {
                    if let Ok(next) = evt.value().parse::<i64>() {
                        on_change.call(next);
                    }
                },
            }
            span { class: "enh-tools__reading", "{display}" }
        }
    }
}
