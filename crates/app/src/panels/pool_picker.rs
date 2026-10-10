//! The pool / epic-pool picker — a modal whose rows expand to reveal the pool's powers,
//! beside a pane that previews whichever one is under the cursor.
//!
//! It replaces the two `select` menus this surface shipped with (the 2026-07-26 inline
//! redesign), because a menu names pools and a pool is not what anyone is choosing: a build
//! takes Fighting because it wants Tough, Speed because it wants Hasten. The unit of the
//! decision is the power, so the surface has to show the powers — all of them, with their
//! numbers, before anything is committed. That is what the beta's `PoolPickerModal` got right
//! and what a `select` structurally cannot do.
//!
//! Two deliberate departures from that beta modal:
//!
//! - **The preview is inside the modal.** The beta drove the side info panel on hover, which
//!   its own overlay covered, and then bolted a second modal onto right-click to compensate.
//!   Here the right-hand pane renders [`crate::panels::info::PowerViewCard`] —
//!   the same component the info panel renders — so the preview is visible and single-source.
//! - **Reading comes before taking.** Hovering a power row previews it and clicking takes it,
//!   so the click lands on a power whose numbers are already on screen. The beta committed on
//!   a click with no preview at all. Touch has no hover, so there the first tap previews and a
//!   second tap (on the row or the pane's button) takes.
//!
//! A pool the build already holds keeps its row, marked. Filtering it away treated this as a
//! surface for adding POOLS, which is the framing the modal exists to reject — every power
//! after a pool's first was then reachable only from the available list, with no preview and
//! no gate reason. What a held aggregate loses is the "add it empty" control, which would
//! commit nothing, and the powers already picked out of it.
//!
//! Nothing here decides *whether* a power can be taken. That is
//! [`coh_data::pick_rules`](coh_data::requires_met) reading the power's own `requires`
//! expression, exactly as the available list reads it — this modal shows the verdict and the
//! reason, and shows the pools whose verdicts are all closed rather than hiding them, since
//! seeing why a pool is out of reach is the point of a browsing surface.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::panels::info::PowerViewCard;
use crate::panels::powers::{
    add_pool, add_power, pick_gate, powers_of_set, set_epic_pool, set_unlock_floor, PickGate,
    SetOrigin,
};
use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use crate::view::power_view::{resolve_power_def, view_for, PowerView};
use dioxus::prelude::*;

/// Which catalog the picker is browsing. The two differ in the aggregate they list, the cap
/// they respect, and whether choosing replaces a previous choice — not in how a power inside
/// them is gated, which is why they share one component.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PoolPickerMode {
    /// A standard power pool, added alongside any others the build already holds.
    Pool,
    /// The epic / patron pool, of which a build holds one.
    Epic,
}

impl PoolPickerMode {
    fn origin(self) -> SetOrigin {
        match self {
            PoolPickerMode::Pool => SetOrigin::Pool,
            PoolPickerMode::Epic => SetOrigin::Epic,
        }
    }

    /// Record the chosen aggregate on the build. The two catalogs differ here and nowhere
    /// else in this module: a pool joins the ones the build holds, an epic replaces the one
    /// it holds.
    fn add_to(self, state: &mut coh_data::CharacterState, id: &str, name: &str) {
        match self {
            PoolPickerMode::Pool => add_pool(state, id, name),
            PoolPickerMode::Epic => set_epic_pool(state, id, name),
        }
    }

    fn title(self) -> &'static str {
        match self {
            PoolPickerMode::Pool => "Add a Power Pool",
            PoolPickerMode::Epic => "Choose an Epic / Patron Pool",
        }
    }

    /// The accent token the rows carry, distinguishing the two catalogs at a glance (the
    /// beta's blue/purple, in this theme's vocabulary).
    fn accent_class(self) -> &'static str {
        match self {
            PoolPickerMode::Pool => "pool-row--pool",
            PoolPickerMode::Epic => "pool-row--epic",
        }
    }

    /// Whether an aggregate this build can take nothing from still earns a row.
    ///
    /// It does for pools and does not for epics, because a block means a different thing in
    /// each catalog. A power pool closed by its powers' own gates is closed *for now*, and
    /// the reason is what a build plans against.
    ///
    /// The epic catalog is not universal: every archetype republishes its own copy of each
    /// mastery under the same display name, so it holds one Dark Mastery per archetype and
    /// all but this character's are closed permanently by an archetype gate. Listing them
    /// would bury the handful that are real under a hundred that never will be, each labelled
    /// with a "yet" that is a lie. Their absence is not a hidden rule — it IS the archetype
    /// gate in the powers' own `requires`, surfacing through [`PoolEntry::blocked_reason`].
    ///
    /// This answers only the per-power case. A set the SET-level gate refuses permanently is
    /// dropped from both catalogs before this is consulted, which is the same distinction the
    /// game draws — `uiPowers.c:395` hides a refused specialization set and an epic, and
    /// shows every other refusal disabled with its message.
    fn lists_blocked(self) -> bool {
        match self {
            PoolPickerMode::Pool => true,
            PoolPickerMode::Epic => false,
        }
    }
}

/// The picker's open state, provided at the shell root beside the enhancement picker's
/// [`PickerOpen`](crate::panels::powers::PickerOpen) and hosted there for the same reason: a
/// `position: fixed` backdrop rendered inside a free-grid surface is contained by that
/// surface's `transform` and clipped to the panel. `None` = closed.
#[derive(Clone, Copy)]
pub struct PoolPickerOpen(pub Signal<Option<PoolPickerMode>>);

/// The one pool picker for the whole app. Rendered at the shell root, OUTSIDE the free grid's
/// transformed surfaces. Renders nothing while closed.
#[component]
pub fn PoolPickerHost(database: Db) -> Element {
    let mut open = use_context::<PoolPickerOpen>().0;
    match open() {
        None => rsx! {},
        Some(mode) => rsx! {
            PoolPickerModal {
                key: "{mode:?}",
                database,
                mode,
                on_close: move |_| open.set(None),
            }
        },
    }
}

/// One row in the pool list: the aggregate, and what this build can do with it.
#[derive(Clone, PartialEq)]
struct PoolEntry {
    id: String,
    name: String,
    description: String,
    /// Every power in the pool the build has NOT already picked, paired with the verdict of
    /// its own `requires` against this build. Resolved once per open so expanding a row costs
    /// no re-evaluation.
    powers: Vec<(coh_data::Power, PickGate)>,
    /// How many powers the pool holds in this dataset at all, before the picked filter — so
    /// an exhausted pool can say so instead of reading as one the dataset never carried.
    catalogued: usize,
    /// The build already holds this aggregate.
    held: bool,
    /// The level below which nothing in the pool can be taken.
    unlock_floor: u8,
    /// How many of its powers the build could take right now.
    open_count: usize,
    /// The aggregate's own set-level verdict ([`coh_data::set_gate`]) — the rule the powers'
    /// `requires` expressions do not carry, and which nothing consulted before SETGATE-1: the
    /// five Specialized pools exclude each other here and nowhere else, their entry powers
    /// having empty `requires`.
    ///
    /// Always `Open` for an aggregate the build already holds. The game tests this gate only
    /// on ACQUISITION — `character_net_server.c:1185` reaches it inside
    /// `character_OwnsPowerSet(…) == NULL` — so a set already in the build keeps offering its
    /// remaining powers whatever the gate says now.
    set_gate: Result<coh_data::SetGate, String>,
}

impl PoolEntry {
    /// Why nothing can be taken from this pool, or `None` if something can. A pool whose every
    /// power is closed is shown disabled with this reason rather than filtered away — the gates
    /// are the interesting part of a pool's story, and the build plans toward opening them.
    fn blocked_reason(&self) -> Option<String> {
        // The set-level verdict outranks the per-power ones, in the game's order: it refuses
        // the whole set before looking at what is inside (`character_base.c:1499`), and the
        // message is the game's own words rather than an inference drawn from which powers
        // happen to be shut.
        match &self.set_gate {
            Err(error) => return Some(format!("Prerequisite unreadable: {error}")),
            Ok(coh_data::SetGate::Closed { reason }) => return Some(reason.clone()),
            // Never listed at all — see the filter in `pool_entries`. Reachable only if that
            // filter stops working, and a wrong row beats a silently missing one (Rule 1).
            Ok(coh_data::SetGate::BranchClosed) => {
                return Some("This build no longer qualifies for this set".to_string())
            }
            // Not a block. The set is still a reachable choice and every power in it carries
            // its own level gate, which is how the game draws one (`uiLevelPower.c:243`).
            Ok(coh_data::SetGate::Open | coh_data::SetGate::NotYet { .. }) => {}
        }
        if self.catalogued == 0 {
            return Some("No powers in this dataset".to_string());
        }
        if self.powers.is_empty() {
            return Some("Every power in this pool is picked".to_string());
        }
        if self.open_count == 0 {
            return Some("No power available to this build yet".to_string());
        }
        None
    }

    /// Whether the build may ADD this aggregate right now — the question the commit button and
    /// the add-empty escape hatch ask, which is not the question `blocked_reason` answers.
    ///
    /// A set under its specialize level reads as browsable but is not yet buyable, and the
    /// game splits those two the same way: `uiLevelPower.c:243` lists it,
    /// `character_IsAllowedToHavePowerSetHypothetically` refuses it.
    fn acquirable(&self) -> bool {
        acquisition_blocker(&self.set_gate, &self.name).is_none()
    }
}

/// Why this aggregate cannot be ADDED to the build, or `None` if it can be.
///
/// Every route that commits an aggregate asks this one function — the row's own
/// [`PoolEntry::acquirable`], the commit button under the preview, and the add-empty escape
/// hatch. They used to answer it separately, which is the shape that lets two verdicts drift
/// apart, and only one of the three was reachable from a test.
///
/// Distinct from [`PoolEntry::blocked_reason`] on exactly one verdict, deliberately:
/// [`coh_data::SetGate::NotYet`] is not a reason to hide or disable a row (the game lists such
/// a set so it can be read and planned toward) but IS a reason to refuse the purchase
/// (`character_IsAllowedToHavePowerSetHypothetically` returns false below the level). A held
/// aggregate never reaches a refusal here because [`aggregate_set_gate`] answers `Open` for it.
fn acquisition_blocker(gate: &Result<coh_data::SetGate, String>, name: &str) -> Option<String> {
    match gate {
        Ok(coh_data::SetGate::Open) => None,
        Ok(coh_data::SetGate::Closed { reason }) => Some(reason.clone()),
        Ok(coh_data::SetGate::NotYet { at }) => Some(format!("{name} unlocks at level {at}")),
        Ok(coh_data::SetGate::BranchClosed) => Some(format!("{name} is not open to this build")),
        Err(error) => Some(format!("Prerequisite unreadable: {error}")),
    }
}

/// One aggregate's set-level verdict against this build, or the unreadable expression.
///
/// `held` short-circuits to `Open` rather than skipping the call, so the reason a held set is
/// never refused sits in one place instead of being spread across each caller.
fn aggregate_set_gate(
    pool: &coh_data::PoolDef,
    held: bool,
    state: &coh_data::CharacterState,
    sets: &coh_data::SetPaths,
) -> Result<coh_data::SetGate, String> {
    if held {
        return Ok(coh_data::SetGate::Open);
    }
    coh_data::set_gate(
        &pool.buy_requires,
        &pool.buy_requires_failed,
        pool.specialize_at,
        &pool.specialize_requires,
        state,
        state.archetype.id.as_deref(),
        sets,
    )
    .map_err(|error| error.to_string())
}

/// One row per aggregate in the catalog this mode browses, each carrying what the build can
/// still do with it. Pure over `(database, mode, state)`, and outside the component so the
/// two claims that make the list correct are gradeable: a held aggregate keeps its row, and a
/// power already picked out of one is not offered again.
fn pool_entries(
    database: &Db,
    mode: PoolPickerMode,
    state: &coh_data::CharacterState,
) -> Vec<PoolEntry> {
    let archetype_id = state.archetype.id.clone();
    let catalog = match mode {
        PoolPickerMode::Pool => &database.pool_catalog.pools,
        PoolPickerMode::Epic => &database.pool_catalog.epics,
    };
    // Powers already picked, by ident, so a second click on one the build owns cannot push a
    // duplicate — `add_power` appends unconditionally. `picked_powers`, not `all_selected`: a
    // granted inherent is not a pick, and counting one as picked would make a pool whose powers
    // the game hands over (Fitness) read "every power is picked" instead of naming the grant
    // gate that actually closed it.
    let picked: Vec<&str> = state
        .picked_powers()
        .map(|p| p.internal_name.as_str())
        .collect();
    let holds = |id: &str| match mode {
        PoolPickerMode::Pool => state.pools.iter().any(|pool| pool.id == id),
        PoolPickerMode::Epic => state.epic_pool.as_ref().is_some_and(|epic| epic.id == id),
    };
    catalog
        .iter()
        .map(|pool| {
            let catalogued = powers_of_set(database, &pool.id, mode.origin());
            let idents: Vec<&str> = catalogued.iter().map(coh_data::Power::ident).collect();
            let powers: Vec<(coh_data::Power, PickGate)> = catalogued
                .iter()
                .filter(|power| !picked.contains(&power.ident()))
                .map(|power| {
                    let siblings: Vec<&str> = idents
                        .iter()
                        .copied()
                        .filter(|i| *i != power.ident())
                        .collect();
                    let gate = pick_gate(
                        power,
                        &pool.id,
                        &siblings,
                        state,
                        archetype_id.as_deref(),
                        &database.set_paths,
                    );
                    (power.clone(), gate)
                })
                // A power the game grants is never picked, so it is not part of what this pool
                // offers.
                .filter(|(_, gate)| *gate != PickGate::Granted)
                .collect();
            let open_count = powers
                .iter()
                .filter(|(_, gate)| matches!(gate, PickGate::Open | PickGate::NeedsEarlier))
                .count();
            let held = holds(&pool.id);
            PoolEntry {
                id: pool.id.clone(),
                name: pool.name.clone(),
                description: pool.description.clone(),
                unlock_floor: set_unlock_floor(database, &pool.id, mode.origin()),
                held,
                catalogued: catalogued.len(),
                powers,
                open_count,
                set_gate: aggregate_set_gate(pool, held, state, &database.set_paths),
            }
        })
        // A set whose branch requirement this build fails is gone, not blocked: no level
        // reaches it, so a row saying "not yet" would be a lie. The game lists such a set on
        // no surface (`uiPowers.c:395` hides it; `uiLevelPower.c:243` lists a specialization
        // set only while `WillBeAllowedToSpecialize` holds). On Homecoming this is what
        // retires the Fitness pool once the build has been handed Swift and Hurdle.
        .filter(|entry| !matches!(entry.set_gate, Ok(coh_data::SetGate::BranchClosed)))
        // An aggregate offering this build nothing keeps its row only where a block is
        // temporary and informative — see `lists_blocked` — or where the build holds it, so the
        // row still reads as a standing choice.
        .filter(|entry| mode.lists_blocked() || entry.held || entry.blocked_reason().is_none())
        .collect()
}

/// The picker modal. Builds the whole catalog once, then renders the list beside a preview
/// pane driven by whichever power the user last pointed at.
#[component]
fn PoolPickerModal(database: Db, mode: PoolPickerMode, on_close: EventHandler<()>) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let totals = use_context::<BuildTotals>().0;

    let mut expanded = use_signal(|| Option::<String>::None);
    // The previewed power, as (set_id, ident). Cleared when a row collapses so the pane never
    // outlives the list it came from.
    let mut previewing = use_signal(|| Option::<(String, String)>::None);

    // The catalog, gated against the live build. `use_memo` would need the build's identity as
    // a dependency; the modal is short-lived and re-renders on each commit anyway (a commit
    // closes it), so this is a plain derivation.
    let entries = pool_entries(&database, mode, &build.read());

    // The preview, resolved through the same `view_for` the info panel uses, so a pool power
    // previews here exactly as it reads there — projected against this build (unslotted, since
    // it isn't held yet, but taking Alpha and the build-wide globals).
    let preview: Option<PowerView> = previewing().and_then(|(set_id, ident)| {
        view_for(&database, &build.read(), &totals.read(), &set_id, &ident)
    });

    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let target_slot = use_context::<crate::level_control::TargetSlot>();
    // A click on a power row takes it when it can be taken, and otherwise leaves it in the
    // preview, where the pane says why not. Same verdict as the Take button: both read
    // `take_check`.
    let take = {
        let database = database.clone();
        move |set_id: String, ident: String| {
            previewing.set(Some((set_id.clone(), ident.clone())));
            let check = take_check(
                &database,
                &build.read(),
                mode,
                &set_id,
                &ident,
                level_up_mode(),
                target_slot.steering(level_up_mode()),
            );
            let Some(TakeCheck {
                pool_name,
                pick_level: Some(level),
                blocker: None,
                ..
            }) = check
            else {
                return;
            };
            take_power(
                session,
                database.clone(),
                mode,
                set_id,
                ident,
                pool_name,
                level,
                target_slot,
            );
            on_close.call(());
        }
    };

    rsx! {
        Modal { title: mode.title().to_string(), size: ModalSize::Full, on_close,
            div { class: "pool-picker",
                div { class: "pool-picker__list",
                    p { class: "pool-picker__hint",
                        "Open a pool to read its powers. Point at one to preview it, click it to "
                        "take it. Taking a power adds its pool too."
                    }
                    if entries.is_empty() {
                        p { class: "hint",
                            match mode {
                                PoolPickerMode::Pool => "This dataset carries no power pools.",
                                PoolPickerMode::Epic => "No epic pools are open to this archetype.",
                            }
                        }
                    }
                    for entry in entries.iter() {
                        PoolRow {
                            key: "{entry.id}",
                            database: database.clone(),
                            entry: entry.clone(),
                            mode,
                            is_expanded: expanded() == Some(entry.id.clone()),
                            previewing: previewing(),
                            on_toggle: {
                                let id = entry.id.clone();
                                move |_| {
                                    let collapsing = expanded() == Some(id.clone());
                                    expanded.set((!collapsing).then(|| id.clone()));
                                    if collapsing {
                                        previewing.set(None);
                                    }
                                }
                            },
                            on_preview: {
                                let id = entry.id.clone();
                                move |ident: String| previewing.set(Some((id.clone(), ident)))
                            },
                            on_take: {
                                let id = entry.id.clone();
                                let mut take = take.clone();
                                move |ident: String| take(id.clone(), ident)
                            },
                        }
                    }
                }

                div { class: "pool-picker__preview",
                    match (previewing(), preview) {
                        // No adjusters in the preview: this pane is deciding whether to TAKE a
                        // power, and an adjuster writes build state keyed to a power the build
                        // does not own yet ([`crate::panels::adjusters`]).
                        (Some((set_id, ident)), Some(view)) => rsx! {
                            PowerViewCard { view, adjusters: Default::default() }
                            PoolPickerCommit {
                                database: database.clone(),
                                mode,
                                set_id,
                                power_ident: ident,
                                on_committed: move |_| on_close.call(()),
                            }
                        },
                        // A previewed power whose def won't resolve is a visible fault, not a
                        // blank pane and not a silent skip (Rule 1).
                        (Some((_, ident)), None) => rsx! {
                            div { class: "load-state error",
                                "“{ident}” has no definition in this dataset, so it cannot be previewed."
                            }
                        },
                        (None, _) => rsx! {
                            p { class: "hint", "Pick a power on the left to read it here." }
                        },
                    }
                }
            }
        }
    }
}

/// Everything the commit control's verdict depends on. A struct rather than nine arguments so
/// the caller cannot silently reorder two `&str`s past each other.
struct CommitInputs<'a> {
    set_id: &'a str,
    pool_name: &'a str,
    power_name: &'a str,
    unlock_level: u8,
    /// `None` means the id names no aggregate in this dataset — a fault, not a refusal.
    set_verdict: Option<&'a Result<coh_data::SetGate, String>>,
    gate: &'a PickGate,
    pick_level: Option<u8>,
    pool_room: bool,
    level_gated_pick: Option<u8>,
    /// Level Up mode refuses a [`PickGate::NeedsEarlier`] power; free-form takes it.
    level_up_mode: bool,
}

/// Why the Take button cannot commit this pick, or `None` if it can.
///
/// Pure, and outside the component for one reason: it is the last check between a click and a
/// build state the game would reject, and a verdict computed inside `rsx!` is reachable from no
/// test. Mutating the set-level line to ignore its verdict survived every behavioural gate in
/// this module while the derivation lived in the component.
///
/// The set-level verdict is asked FIRST because taking a power out of an unheld pool buys that
/// pool: where the game would refuse the pool, its refusal is the whole answer and no per-power
/// reason is reached. `character_net_server.c:1185` orders it the same way.
fn commit_blocker(inputs: CommitInputs<'_>) -> Option<String> {
    let CommitInputs {
        set_id,
        pool_name,
        power_name,
        unlock_level,
        set_verdict,
        gate,
        pick_level,
        pool_room,
        level_gated_pick,
        level_up_mode,
    } = inputs;

    let set_blocker = match set_verdict {
        None => Some(format!("“{set_id}” is not an aggregate in this dataset")),
        Some(verdict) => acquisition_blocker(verdict, pool_name),
    };

    set_blocker.or_else(|| match (gate, pick_level, pool_room, level_gated_pick) {
        (PickGate::Unreadable(error), _, _, _) => Some(format!("Prerequisite unreadable: {error}")),
        (PickGate::Closed, _, _, _) => Some(format!(
            "{power_name} needs prerequisites this build hasn't taken"
        )),
        (PickGate::NeedsEarlier, _, _, _) if level_up_mode => Some(format!(
            "{power_name} needs prerequisites this build hasn't taken"
        )),
        (PickGate::Granted, _, _, _) => Some(format!("{power_name} is granted, not picked")),
        (_, _, false, _) => Some("Every power pool slot is taken".to_string()),
        (_, None, _, _) => Some(format!(
            "No power pick left at or above level {unlock_level}"
        )),
        (_, _, _, Some(level)) => Some(format!(
            "This pick belongs to level {level}, which this character hasn't reached \
             (Level Up mode)"
        )),
        (PickGate::Open | PickGate::NeedsEarlier, Some(_), true, None) => None,
    })
}

/// Everything the Take control needs to know about one power: whether it can be taken, at
/// which pick level, and what taking it does to the build's pools. Computed in one place so the
/// button under the preview and a click on the power's own row cannot disagree.
struct TakeCheck {
    power_name: String,
    pool_name: String,
    pick_level: Option<u8>,
    already_held: bool,
    /// Taking this replaces the epic pool the build holds, discarding its picks.
    replacing_epic: bool,
    blocker: Option<String>,
}

/// Resolves the pick level the same way an available row does, so the level the game would
/// grant is what the pick carries. No such level left means the build has spent every pick at
/// or above the power's unlock level, and the blocker says that instead of substituting one.
/// `None` when the power has no definition in this dataset.
fn take_check(
    database: &Db,
    state: &coh_data::CharacterState,
    mode: PoolPickerMode,
    set_id: &str,
    power_ident: &str,
    level_up_mode: bool,
    target: Option<u8>,
) -> Option<TakeCheck> {
    let power = resolve_power_def(database, set_id, power_ident)?;
    let set_powers = powers_of_set(database, set_id, mode.origin());
    let siblings: Vec<&str> = set_powers
        .iter()
        .map(coh_data::Power::ident)
        .filter(|i| *i != power_ident)
        .collect();
    let gate = pick_gate(
        power,
        set_id,
        &siblings,
        state,
        state.archetype.id.as_deref(),
        &database.set_paths,
    );
    let taken_levels: Vec<u8> = state.picked_powers().map(|p| p.level).collect();
    let already_held = match mode {
        PoolPickerMode::Pool => state.pools.iter().any(|pool| pool.id == set_id),
        PoolPickerMode::Epic => state.epic_pool.as_ref().is_some_and(|e| e.id == set_id),
    };
    // Taking a power out of an unheld pool BUYS that pool, so the pool's own gate has to
    // pass before the pick can — the game orders it the same way
    // (`character_net_server.c:1185` buys the set first, and only if it is allowed).
    let set_verdict = database
        .pool_catalog
        .find(set_id)
        .map(|pool| aggregate_set_gate(pool, already_held, state, &database.set_paths));
    let cap = database
        .leveling_schedule
        .as_ref()
        .map_or(0, |schedule| schedule.max_power_pools as usize);
    let pool_room = mode == PoolPickerMode::Epic || already_held || state.pools.len() < cap;

    let unlock_level = power
        .unlock_level()
        .max(set_unlock_floor(database, set_id, mode.origin()));
    let pick_level = database
        .leveling_schedule
        .as_ref()
        .and_then(|schedule| schedule.pick_level_toward(&taken_levels, unlock_level, target));

    let pool_name = database
        .pool_catalog
        .find(set_id)
        .map(|pool| pool.name.clone())
        .unwrap_or_else(|| set_id.to_string());

    // Level-up mode refuses a pick belonging to a level the character hasn't reached, exactly
    // as the Available rows do — a pool power is a pick like any other.
    let level_gated_pick =
        crate::level_control::pick_beyond_level(level_up_mode, pick_level, state.level);

    let blocker = commit_blocker(CommitInputs {
        set_id,
        pool_name: &pool_name,
        power_name: &power.name,
        unlock_level,
        set_verdict: set_verdict.as_ref(),
        gate: &gate,
        pick_level,
        pool_room,
        level_gated_pick,
        level_up_mode,
    });

    Some(TakeCheck {
        power_name: power.name.clone(),
        pool_name,
        pick_level,
        already_held,
        // Switching the epic pool discards the picks the previous one held, so the note has
        // to say so before the click, not after.
        replacing_epic: mode == PoolPickerMode::Epic && !already_held && state.epic_pool.is_some(),
        blocker,
    })
}

/// Take the power, and its pool with it. One commit, so one undo step takes back both the pool
/// and the pick it was added for — and any grant the pick enables.
fn take_power(
    session: BuildSession,
    database: Db,
    mode: PoolPickerMode,
    set_id: String,
    power_ident: String,
    pool_name: String,
    level: u8,
    target_slot: crate::level_control::TargetSlot,
) {
    session.commit(move |state| {
        mode.add_to(state, &set_id, &pool_name);
        add_power(state, &set_id, &power_ident, level, None);
        crate::granted_powers::sync(state, &database);
    });
    // A clicked slot steers one pick, as on the Available rows.
    target_slot.clear();
}

/// The commit control under the preview: takes the power (and its pool), and — for a pool the
/// build doesn't hold yet — says so, since adding one spends a pool slot.
#[component]
fn PoolPickerCommit(
    database: Db,
    mode: PoolPickerMode,
    set_id: String,
    power_ident: String,
    on_committed: EventHandler<()>,
) -> Element {
    let session = use_context::<BuildSession>();
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let target_slot = use_context::<crate::level_control::TargetSlot>();

    let Some(check) = take_check(
        &database,
        &session.build.read(),
        mode,
        &set_id,
        &power_ident,
        level_up_mode(),
        target_slot.steering(level_up_mode()),
    ) else {
        return rsx! {};
    };
    let TakeCheck {
        power_name,
        pool_name,
        pick_level,
        already_held,
        replacing_epic,
        blocker,
    } = check;

    rsx! {
        div { class: "pool-picker__commit",
            if let Some(reason) = &blocker {
                p { class: "pool-picker__blocked", "{reason}" }
            } else {
                if !already_held {
                    if replacing_epic {
                        p { class: "pool-picker__note",
                            "Replaces the epic pool this build holds, discarding its picks."
                        }
                    } else {
                        p { class: "pool-picker__note", "Adds {pool_name} to the build." }
                    }
                }
                button {
                    class: "pool-picker__take",
                    onclick: {
                        let set_id = set_id.clone();
                        let power_ident = power_ident.clone();
                        let pool_name = pool_name.clone();
                        let database = database.clone();
                        move |_| {
                            let Some(level) = pick_level else { return };
                            take_power(
                                session,
                                database.clone(),
                                mode,
                                set_id.clone(),
                                power_ident.clone(),
                                pool_name.clone(),
                                level,
                                target_slot,
                            );
                            on_committed.call(());
                        }
                    },
                    "Take {power_name}"
                    if let Some(level) = pick_level {
                        span { class: "pool-picker__take-level", " · level {level} pick" }
                    }
                }
            }
        }
    }
}

/// One pool in the list: a disclosure header, and — when open — its description, its powers,
/// and the escape hatch for adding the pool with no pick.
#[component]
fn PoolRow(
    database: Db,
    entry: PoolEntry,
    mode: PoolPickerMode,
    is_expanded: bool,
    previewing: Option<(String, String)>,
    on_toggle: EventHandler<()>,
    on_preview: EventHandler<String>,
    on_take: EventHandler<String>,
) -> Element {
    let session = use_context::<BuildSession>();
    let blocked = entry.blocked_reason();
    // The level the pool's powers dim against, by the Available rows' own rule, so a power
    // reads as out of reach here exactly when it will on the rail it lands in.
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let target_slot = use_context::<crate::level_control::TargetSlot>();
    let target_slot = move || target_slot.steering(level_up_mode());
    let reach_level = {
        let build = session.build.read();
        let working_level = database.leveling_schedule.as_ref().and_then(|schedule| {
            crate::level_control::working_level(schedule, &build, target_slot())
        });
        crate::level_control::reach_level(level_up_mode(), working_level, build.level)
    };

    let mut classes = vec!["pool-row", mode.accent_class()];
    if is_expanded {
        classes.push("is-expanded");
    }
    if blocked.is_some() {
        classes.push("is-blocked");
    }
    if entry.held {
        classes.push("is-current");
    }

    rsx! {
        div { class: classes.join(" "),
            button {
                class: "pool-row__header",
                "aria-expanded": is_expanded,
                disabled: blocked.is_some(),
                onclick: move |_| on_toggle.call(()),
                span {
                    class: if is_expanded {
                        "pool-row__caret"
                    } else {
                        "pool-row__caret caret--folded"
                    },
                    "▼"
                }
                span { class: "pool-row__name", "{entry.name}" }
                if entry.held {
                    span { class: "pool-row__current",
                        match mode {
                            PoolPickerMode::Pool => "held",
                            PoolPickerMode::Epic => "current",
                        }
                    }
                }
                if let Some(reason) = blocked.clone() {
                    span { class: "pool-row__blocked", "{reason}" }
                } else if !is_expanded {
                    if entry.description.is_empty() {
                        span { class: "pool-row__meta", "{entry.open_count} available" }
                    } else {
                        span { class: "pool-row__meta", "{entry.description}" }
                    }
                }
            }

            if is_expanded && blocked.is_none() {
                div { class: "pool-row__body",
                    if !entry.description.is_empty() {
                        p { class: "pool-row__description", "{entry.description}" }
                    }
                    div { class: "power-rows",
                        for (power, gate) in entry.powers.iter() {
                            PoolPowerRow {
                                key: "{power.ident()}",
                                power: power.clone(),
                                gate: gate.clone(),
                                unlock_floor: entry.unlock_floor,
                                reach_level,
                                is_previewing: previewing.as_ref().is_some_and(|(set, ident)| {
                                    set == &entry.id && ident == power.ident()
                                }),
                                on_preview: {
                                    let ident = power.ident().to_string();
                                    move |_| on_preview.call(ident.clone())
                                },
                                on_take: {
                                    let ident = power.ident().to_string();
                                    move |_| on_take.call(ident.clone())
                                },
                            }
                        }
                    }
                    // The pool without a pick — the beta's escape hatch, kept because a build
                    // can legitimately want the pool reserved before it has a pick to spend.
                    // Epic mode uses it too: it IS how you switch pools without taking a power.
                    // An aggregate the build already holds has nothing to add, so it gets no
                    // control rather than one that commits an undo step and changes nothing.
                    //
                    // It commits directly, bypassing `PoolPickerCommit` and every check that
                    // lives there, so it asks the set-level question itself. A row can be
                    // browsable and still unbuyable — a set below its specialize level is
                    // exactly that — and this is the one control that could otherwise add one.
                    if !entry.held && entry.acquirable() {
                    button {
                        class: "pool-row__add-empty",
                        onclick: {
                            let id = entry.id.clone();
                            let name = entry.name.clone();
                            let database = database.clone();
                            move |_| {
                                let id = id.clone();
                                let name = name.clone();
                                let database = database.clone();
                                session.commit(move |state| {
                                    // Switching the epic pool discards its picks, and any
                                    // grants those picks were holding open go with them.
                                    mode.add_to(state, &id, &name);
                                    crate::granted_powers::sync(state, &database);
                                });
                            }
                        },
                        match mode {
                            PoolPickerMode::Pool => "Add the pool without picking a power",
                            PoolPickerMode::Epic => "Choose this pool without picking a power",
                        }
                    }
                    }
                }
            }
        }
    }
}

/// A power inside an expanded pool row. Hovering previews it and clicking takes it.
///
/// Touch has no hover, so there a tap on a power not yet in the preview only previews it, and
/// a second tap takes it. Taking straight off the first tap would spend a pick on a power the
/// user never got to read.
#[component]
fn PoolPowerRow(
    power: coh_data::Power,
    gate: PickGate,
    unlock_floor: u8,
    /// The level being picked for ([`crate::level_control::reach_level`]): an open power
    /// unlocking above it dims, and stays takeable.
    reach_level: u8,
    is_previewing: bool,
    on_preview: EventHandler<()>,
    on_take: EventHandler<()>,
) -> Element {
    // Decided on press, spent by the click that follows. The click handler can't ask
    // `is_previewing` itself: a tap's synthetic mouseenter previews the row first, and the
    // re-render lands before the click does.
    let mut preview_only = use_signal(|| false);
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let power_name = power.name.clone();
    let unlock_level = power.unlock_level().max(unlock_floor);

    // The same art the Available rail's rows wear, read off the same field — this picker offers
    // the very powers that rail will list, so a power it shows here has to be the one you then
    // recognize there.
    let icon_url = crate::view::icons::power_icon_url(
        power.extra.get("icon").and_then(|value| value.as_str()),
    );

    // A gated or unreadable power still previews — reading a power you can't take yet is how
    // a build gets planned toward it — so only the styling and the title differ.
    let (state_class, title) = match &gate {
        PickGate::Open => (
            if unlock_level > reach_level {
                " is-locked"
            } else {
                ""
            },
            format!("{power_name} — unlocks at level {unlock_level}"),
        ),
        PickGate::Closed => (
            " is-gated",
            format!("{power_name} needs prerequisites this build hasn't taken yet"),
        ),
        // Free-form takes it and flags the build; Level Up mode refuses it, so there it reads
        // as refused, the way the Available rail's row does in the same mode.
        PickGate::NeedsEarlier if level_up_mode() => (
            " is-gated",
            format!("{power_name} needs prerequisites this build hasn't taken yet"),
        ),
        PickGate::NeedsEarlier => (
            " is-needs-earlier",
            format!("{power_name} needs an earlier pick from this pool — add one before it"),
        ),
        PickGate::Granted => (
            " is-unpickable",
            format!("{power_name} is granted, not picked"),
        ),
        PickGate::Unreadable(error) => (" is-faulted", error.clone()),
    };
    let selected_class = if is_previewing { " is-previewing" } else { "" };

    rsx! {
        button {
            class: "power-row{state_class}{selected_class}",
            title: "{title}",
            onpointerdown: move |evt| {
                preview_only.set(evt.pointer_type() == "touch" && !is_previewing)
            },
            onclick: move |_| {
                if preview_only() {
                    preview_only.set(false);
                    on_preview.call(());
                } else {
                    on_take.call(());
                }
            },
            onmouseenter: move |_| on_preview.call(()),
            span { class: "power-row__level",
                if matches!(gate, PickGate::Unreadable(_)) { "!" } else { "{unlock_level}" }
            }
            // Art for a readable power; the fault glyph keeps the chip when the prerequisite
            // could not be read, because a row reporting a fault should lead with the fault and
            // not with a picture that says nothing is wrong (Rule 1).
            span { class: "power-row__icon",
                if matches!(gate, PickGate::Unreadable(_)) {
                    "⚠"
                } else {
                    img { class: "power-row__art", src: "{icon_url}", alt: "" }
                }
            }
            span { class: "power-row__name", "{power_name}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocker(gate: &PickGate, level_up_mode: bool) -> Option<String> {
        let open = Ok(coh_data::SetGate::Open);
        commit_blocker(CommitInputs {
            set_id: "fighting",
            pool_name: "Fighting",
            power_name: "Tough",
            unlock_level: 14,
            set_verdict: Some(&open),
            gate,
            pick_level: Some(14),
            pool_room: true,
            level_gated_pick: None,
            level_up_mode,
        })
    }

    #[test]
    fn free_form_takes_a_pool_power_that_needs_an_earlier_pick() {
        assert_eq!(blocker(&PickGate::NeedsEarlier, false), None);
    }

    #[test]
    fn level_up_mode_refuses_a_pool_power_that_needs_an_earlier_pick() {
        assert!(blocker(&PickGate::NeedsEarlier, true).is_some());
    }

    #[test]
    fn a_closed_pool_power_is_refused_in_both_modes() {
        assert!(blocker(&PickGate::Closed, false).is_some());
        assert!(blocker(&PickGate::Closed, true).is_some());
    }
}
