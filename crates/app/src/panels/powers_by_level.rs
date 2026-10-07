//! The Powers panel's by-level layout — the build's picks laid out over the levels the
//! game grants them at, instead of grouped by the powerset they came from (the beta's
//! `ChronologicalPowerView`, its "By Level" mode). Same cards, same data, different
//! arrangement: [`crate::panels::powers::PickedPowerCard`] renders every occupied cell,
//! so a power looks and behaves identically in both layouts.
//!
//! Every level in the grid comes from the dataset's own
//! [`LevelingSchedule::pick_levels`](coh_data::LevelingSchedule::pick_levels) — one cell
//! per pick the character earns, in ascending order — never a hand-written list of pick
//! levels (the beta hardcodes three arrays of eight).
//!
//! A pick moves by dragging its level badge onto another cell: onto an unspent pick it
//! takes that level, onto another power the two trade levels (the beta's drag-and-swap).
//! Only the stored `level` changes, so slots, enhancements and toggles travel with the
//! power, and the slot-level solver re-houses any slot the new level can no longer serve.

use crate::build_session::BuildSession;
use crate::panels::power_group::PowerGroup;
use crate::panels::powers::{set_unlock_floor, PickedPowerCard, SetOrigin};
use crate::shell::Db;
use crate::view::power_view::resolve_power_def;
use coh_data::{CharacterState, SelectedPower};
use dioxus::prelude::*;

/// Columns the picks are dealt into, left to right. Three is the beta's shape and what
/// the ~24 picks of every shipped dataset divide into evenly; the column *contents* are
/// derived, so a dataset granting a different number of picks still fills them.
const COLUMNS: usize = 3;

/// Below this much travel a press on the badge is not a drag.
const PICK_DRAG_THRESHOLD_PX: f64 = 4.0;

/// One cell of the grid: a pick the character earns at `level`, and the power occupying
/// it (`None` = the pick is unspent).
#[derive(Clone, PartialEq)]
struct PickCell {
    level: u8,
    power: Option<SelectedPower>,
}

/// Deal the build's picked powers onto the schedule's picks.
///
/// Powers are placed lowest-level first (ties in `picked_powers` traversal order:
/// primary → secondary → pools → epic), each into the first free cell whose level is its
/// own, then into the first free cell at a higher level. A power that fits nowhere is
/// returned separately rather than being forced into a cell the game wouldn't grant it —
/// the caller shows it as a visible fault (Rule 1), which is how a build carrying levels
/// this schedule never grants announces itself.
fn deal_picks(
    pick_levels: &[u8],
    mut powers: Vec<SelectedPower>,
) -> (Vec<PickCell>, Vec<SelectedPower>) {
    let mut cells: Vec<PickCell> = pick_levels
        .iter()
        .map(|&level| PickCell { level, power: None })
        .collect();
    let mut unplaced = Vec::new();

    powers.sort_by_key(|power| power.level);
    for power in powers {
        let free_at = |cells: &[PickCell], level: u8| {
            cells
                .iter()
                .position(|cell| cell.power.is_none() && cell.level == level)
        };
        let free_above = |cells: &[PickCell], level: u8| {
            cells
                .iter()
                .position(|cell| cell.power.is_none() && cell.level > level)
        };
        match free_at(&cells, power.level).or_else(|| free_above(&cells, power.level)) {
            Some(index) => cells[index].power = Some(power),
            None => unplaced.push(power),
        }
    }

    (cells, unplaced)
}

/// A column's heading — the band of levels its cells cover. Derived from the cells
/// themselves so it can never drift from what the column shows.
fn column_title(cells: &[PickCell]) -> String {
    match (cells.first(), cells.last()) {
        (Some(first), Some(last)) if first.level != last.level => {
            format!("Levels {}–{}", first.level, last.level)
        }
        (Some(only), _) => format!("Level {}", only.level),
        _ => String::new(),
    }
}

/// One pick, addressed by the set it came from and its internal name. Both halves, because
/// `internalName` repeats across sets (Build_Up ×64), and the set on the pick itself rather
/// than the bucket's id, because a VEAT branch pick sits in a role list no bucket id names.
#[derive(Clone, PartialEq, Debug)]
struct PickKey {
    powerset: String,
    internal_name: String,
}

impl PickKey {
    fn of(power: &SelectedPower) -> Self {
        PickKey {
            powerset: power.powerset.clone(),
            internal_name: power.internal_name.clone(),
        }
    }
}

/// A pick about to be moved: who it is, the level it holds now, and the lowest level the game
/// would let it be taken at.
#[derive(Clone, PartialEq, Debug)]
struct MovingPick {
    key: PickKey,
    /// The power's display name, for the refusal text.
    name: String,
    level: u8,
    floor: u8,
}

/// The pick-spending power `key` names, wherever the build keeps it.
fn pick_mut<'a>(state: &'a mut CharacterState, key: &PickKey) -> Option<&'a mut SelectedPower> {
    state
        .primary
        .powers
        .iter_mut()
        .chain(state.secondary.powers.iter_mut())
        .chain(
            state
                .pools
                .iter_mut()
                .flat_map(|pool| pool.powers.iter_mut()),
        )
        .chain(
            state
                .epic_pool
                .iter_mut()
                .flat_map(|epic| epic.powers.iter_mut()),
        )
        .filter(|power| !power.is_locked)
        .find(|power| power.powerset == key.powerset && power.internal_name == key.internal_name)
}

/// The lowest level `power` can be taken at: its own unlock level, raised to its set's floor
/// (pools and epics open as a whole before any power in them does). The same two terms the
/// picker's rows floor on, so a drag can never place a power where a click couldn't have.
/// `None` when the def can't be resolved — a move the planner can't check is refused.
fn pick_floor(database: &Db, state: &CharacterState, power: &SelectedPower) -> Option<u8> {
    let def = resolve_power_def(database, &power.powerset, &power.internal_name)?;
    let origin = if state.pools.iter().any(|pool| pool.id == power.powerset) {
        SetOrigin::Pool
    } else if state
        .epic_pool
        .as_ref()
        .is_some_and(|epic| epic.id == power.powerset)
    {
        SetOrigin::Epic
    } else {
        SetOrigin::Powerset
    };
    Some(
        def.unlock_level()
            .max(set_unlock_floor(database, &power.powerset, origin)),
    )
}

/// The name a player knows `power` by, falling back to its internal name when the def is gone.
fn display_name(database: &Db, power: &SelectedPower) -> String {
    resolve_power_def(database, &power.powerset, &power.internal_name)
        .map_or_else(|| power.internal_name.clone(), |def| def.name.clone())
}

/// Give `dragged` the level `to_level`, and — when the target cell is occupied — hand the
/// occupant the level `dragged` held. Nothing else on either pick changes.
fn apply_move(
    state: &mut CharacterState,
    dragged: &MovingPick,
    to_level: u8,
    occupant: Option<&MovingPick>,
) {
    if let Some(occupant) = occupant {
        if let Some(power) = pick_mut(state, &occupant.key) {
            power.level = dragged.level;
        }
    }
    if let Some(power) = pick_mut(state, &dragged.key) {
        power.level = to_level;
    }
}

/// Why dropping `dragged` on a cell at `to_level` (holding `occupant`, if any) is refused, or
/// `None` when the move is legal.
///
/// The rules are the picker's own, applied to both picks the move touches:
/// - each power lands at or above its floor ([`pick_floor`]);
/// - level 1 holds one primary pick and one secondary pick, never two of either
///   (`bucket_owns_level_one` in the picker);
/// - in level-up mode, nothing lands above the character's current level.
fn move_refusal(
    state: &CharacterState,
    dragged: &MovingPick,
    to_level: u8,
    occupant: Option<&MovingPick>,
    level_cap: Option<u8>,
) -> Option<String> {
    if to_level == dragged.level {
        return Some("Already at this level".to_string());
    }
    if to_level < dragged.floor {
        return Some(format!(
            "{} unlocks at level {}",
            dragged.name, dragged.floor
        ));
    }
    if let Some(occupant) = occupant {
        if dragged.level < occupant.floor {
            return Some(format!(
                "{} unlocks at level {}, so it can't move down to {}",
                occupant.name, occupant.floor, dragged.level
            ));
        }
    }
    if let Some(cap) = level_cap.filter(|&cap| to_level > cap) {
        return Some(format!(
            "Level Up mode is on — the character is level {cap}"
        ));
    }
    // Trial the move and count, rather than reasoning about which bucket each pick is in: a
    // VEAT branch pick sits in a role list under its own set id, and the count over the role
    // list is the one `bucket_owns_level_one` takes.
    let mut trial = state.clone();
    apply_move(&mut trial, dragged, to_level, occupant);
    let crowded = |powers: &[SelectedPower]| {
        powers
            .iter()
            .filter(|power| !power.is_locked && power.level == 1)
            .count()
            > 1
    };
    if crowded(&trial.primary.powers) || crowded(&trial.secondary.powers) {
        return Some("Level 1 holds one primary pick and one secondary pick".to_string());
    }
    None
}

/// A drag in flight: which cell it started from, where the pointer is, and what dropping on
/// each cell would do — decided once at the press, because nothing in the build changes until
/// release.
#[derive(Clone, PartialEq)]
struct PickDrag {
    from_cell: usize,
    origin: (f64, f64),
    moved: bool,
    /// Per cell, `None` when a drop there is legal, else why not.
    refusals: Vec<Option<String>>,
    /// The cells' on-screen boxes `(index, left, top, right, bottom)`, measured once the press's
    /// DOM round trip lands. Empty until then, so a drop that beats the measurement does nothing.
    rects: Vec<(usize, f64, f64, f64, f64)>,
    hover: Option<usize>,
}

impl PickDrag {
    fn cell_at(&self, x: f64, y: f64) -> Option<usize> {
        self.rects
            .iter()
            .find(|(_, left, top, right, bottom)| {
                x >= *left && x <= *right && y >= *top && y <= *bottom
            })
            .map(|(index, ..)| *index)
    }
}

/// Capture the pointer on the pressed badge and measure every cell of the visible grid.
///
/// Through `eval`, as `arm_slot_drag` does it, so the desktop webview gets the same gesture as
/// the browser. Capture is what keeps `pointermove`/`pointerup` arriving on the badge once the
/// pointer leaves it, and on touch it is what stops the finger's drag scrolling the panel. With
/// the pointer captured no cell sees it pass, so the drop target is found by hit-testing these
/// boxes. Both layout roots are in the document at once, so only cells that render are kept.
async fn arm_pick_drag(cell: usize, pointer_id: i32) -> Vec<(usize, f64, f64, f64, f64)> {
    let js = format!(
        r#"
        const grips = document.querySelectorAll('[data-pick-grip="{cell}"]');
        const grip = Array.from(grips).find(g => g.getBoundingClientRect().width > 0);
        if (grip) {{ try {{ grip.setPointerCapture({pointer_id}); }} catch (_) {{}} }}
        return Array.from(document.querySelectorAll('[data-pick-cell]'))
            .map(c => [Number(c.dataset.pickCell), c.getBoundingClientRect()])
            .filter(([_, r]) => r.width > 0)
            .map(([i, r]) => [i, r.left, r.top, r.right, r.bottom]);
        "#
    );
    let Ok(value) = document::eval(&js).await else {
        return Vec::new();
    };
    value
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let n = |i: usize| row.get(i).and_then(|v| v.as_f64());
                    Some((n(0)? as usize, n(1)?, n(2)?, n(3)?, n(4)?))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The by-level grid: one cell per power pick, dealt into columns.
#[component]
pub fn PowersByLevel(database: Db) -> Element {
    let session = use_context::<BuildSession>();
    let level_up_mode = use_context::<crate::level_control::LevelUpMode>().0;
    let drag = use_signal(|| Option::<PickDrag>::None);

    // No schedule ⇒ no picks to lay out, and inventing a level ladder here is exactly the
    // hardcode Rule 0 forbids. Say so instead (Rule 1).
    let Some(schedule) = database.leveling_schedule.as_ref() else {
        return rsx! {
            div { class: "load-state error",
                "This dataset carries no leveling schedule, so the levels powers are picked at are unknown."
            }
        };
    };

    let picked: Vec<SelectedPower> = session.build.read().picked_powers().cloned().collect();
    let (cells, unplaced) = deal_picks(&schedule.pick_levels(), picked);
    // Ceil-divide so the last column is the short one, never an extra column of one.
    let per_column = cells.len().div_ceil(COLUMNS).max(1);

    let active = drag().filter(|drag| drag.moved);
    // What release would do, said under the grid while a drag is live — the one place a refusal
    // reason can be read mid-gesture.
    let drag_status = active.as_ref().map(|drag| match drag.hover {
        None => "Drop on another level to move this power there".to_string(),
        Some(target) => match drag.refusals.get(target).cloned().flatten() {
            Some(reason) => reason,
            None => match &cells[target].power {
                Some(occupant) => {
                    format!("Swap levels with {}", display_name(&database, occupant))
                }
                None => format!("Move to level {}", cells[target].level),
            },
        },
    });

    rsx! {
        div { class: "power-columns",
            for (column_index, column) in cells.chunks(per_column).enumerate() {
                PowerGroup {
                    key: "col-{column_index}",
                    title: column_title(column),
                    // The picks in the band, spent or not — the count a title like
                    // "Levels 1–4" names, and the number of cells folding away.
                    count: column.len(),
                    for (cell_offset, cell) in column.iter().enumerate() {
                        {
                            let index = column_index * per_column + cell_offset;
                            let cell_class = match &active {
                                Some(drag) if drag.from_cell == index => "pick-cell pick-cell--source",
                                Some(drag) if drag.refusals.get(index).is_some_and(Option::is_none) => {
                                    if drag.hover == Some(index) {
                                        "pick-cell pick-cell--target pick-cell--hover"
                                    } else {
                                        "pick-cell pick-cell--target"
                                    }
                                }
                                Some(_) => "pick-cell pick-cell--refused",
                                None => "pick-cell",
                            };
                            rsx! {
                                div {
                                    key: "pick-{index}",
                                    class: cell_class,
                                    "data-pick-cell": "{index}",
                                    if let Some(power) = &cell.power {
                                        PickedPowerCard {
                                            database: database.clone(),
                                            power: power.clone(),
                                            powerset_id: power.powerset.clone(),
                                            level_badge: rsx! {
                                                PickGrip {
                                                    database: database.clone(),
                                                    cells: cells.clone(),
                                                    index,
                                                    drag,
                                                    level_cap: level_up_mode().then(|| session.build.read().level),
                                                }
                                            },
                                        }
                                    } else {
                                        div { class: "power-pick--empty",
                                            span { class: "power-level", "L{cell.level}" }
                                            span { class: "power-pick-hint", "Unspent pick" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some(status) = drag_status {
            div { class: "pick-drag-status", "{status}" }
        }
        if !unplaced.is_empty() {
            div { class: "load-state error",
                "{unplaced.len()} power(s) sit at a level this dataset grants no pick at: "
                {unplaced.iter().map(|p| p.internal_name.as_str()).collect::<Vec<_>>().join(", ")}
            }
        }
    }
}

/// A picked card's level badge in the by-level grid, doubling as the handle that moves the
/// pick. Press and drag it onto another cell; release commits the move as one undo step.
#[component]
fn PickGrip(
    database: Db,
    cells: Vec<PickCell>,
    index: usize,
    drag: Signal<Option<PickDrag>>,
    level_cap: Option<u8>,
) -> Element {
    let session = use_context::<BuildSession>();
    let Some(power) = cells.get(index).and_then(|cell| cell.power.clone()) else {
        return rsx! {};
    };
    let mut drag = drag;

    rsx! {
        span {
            class: "power-level pick-grip",
            title: "Drag to another level to move this power there, or onto another power to swap their levels",
            "data-pick-grip": "{index}",
            onpointerdown: {
                let database = database.clone();
                let cells = cells.clone();
                let power = power.clone();
                move |evt: Event<PointerData>| {
                    evt.prevent_default();
                    let state = session.build.read();
                    // Every cell's verdict is fixed for the whole gesture, so it is decided here
                    // once rather than on every pointer move.
                    let dragged = pick_floor(&database, &state, &power).map(|floor| MovingPick {
                        key: PickKey::of(&power),
                        name: display_name(&database, &power),
                        level: power.level,
                        floor,
                    });
                    let refusals = cells
                        .iter()
                        .enumerate()
                        .map(|(target, cell)| {
                            if target == index {
                                return Some(String::new());
                            }
                            let Some(dragged) = &dragged else {
                                return Some(format!(
                                    "{} isn't in this dataset, so its unlock level is unknown",
                                    display_name(&database, &power)
                                ));
                            };
                            let occupant = match &cell.power {
                                Some(other) => match pick_floor(&database, &state, other) {
                                    Some(floor) => Some(MovingPick {
                                        key: PickKey::of(other),
                                        name: display_name(&database, other),
                                        level: other.level,
                                        floor,
                                    }),
                                    None => {
                                        return Some(format!(
                                            "{} isn't in this dataset, so its unlock level is unknown",
                                            display_name(&database, other)
                                        ))
                                    }
                                },
                                None => None,
                            };
                            // A swap trades stored levels; a move onto an unspent pick takes
                            // the cell's.
                            let to_level = occupant.as_ref().map_or(cell.level, |o| o.level);
                            move_refusal(&state, dragged, to_level, occupant.as_ref(), level_cap)
                        })
                        .collect();
                    drop(state);
                    let coordinates = evt.client_coordinates();
                    drag.set(Some(PickDrag {
                        from_cell: index,
                        origin: (coordinates.x, coordinates.y),
                        moved: false,
                        refusals,
                        rects: Vec::new(),
                        hover: None,
                    }));
                    // Returned as a future for Dioxus to drive, not spawned: see `arm_slot_drag`.
                    let pointer_id = evt.pointer_id();
                    async move {
                        let rects = arm_pick_drag(index, pointer_id).await;
                        drag.with_mut(|active| {
                            if let Some(active) = active.as_mut() {
                                active.rects = rects;
                            }
                        });
                    }
                }
            },
            onpointermove: move |evt: Event<PointerData>| {
                let point = evt.client_coordinates();
                drag.with_mut(|active| {
                    let Some(active) = active.as_mut() else { return };
                    let travel = (point.x - active.origin.0).hypot(point.y - active.origin.1);
                    if !active.moved && travel > PICK_DRAG_THRESHOLD_PX {
                        active.moved = true;
                    }
                    active.hover = active.cell_at(point.x, point.y);
                });
            },
            onpointerup: {
                let database = database.clone();
                let cells = cells.clone();
                move |_| {
                    let Some(finished) = drag.take() else { return };
                    let Some(target) = finished.hover.filter(|_| finished.moved) else { return };
                    if finished.refusals.get(target).is_none_or(Option::is_some) {
                        return;
                    }
                    let Some(dragged) = cells[index].power.clone() else { return };
                    let occupant = cells[target].power.clone();
                    let to_level = occupant.as_ref().map_or(cells[target].level, |o| o.level);
                    let database = database.clone();
                    session.commit(move |state| {
                        let moving = |power: &SelectedPower| MovingPick {
                            key: PickKey::of(power),
                            name: String::new(),
                            level: power.level,
                            // Floors were checked at the press; the move itself needs none.
                            floor: 0,
                        };
                        apply_move(
                            state,
                            &moving(&dragged),
                            to_level,
                            occupant.as_ref().map(moving).as_ref(),
                        );
                        // A grant gated on a pick's level follows the pick in the same edit.
                        crate::granted_powers::sync(state, &database);
                    });
                }
            },
            onpointercancel: move |_| drag.set(None),
            onlostpointercapture: move |_| drag.set(None),
            "L{power.level}"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coh_data::DatasetId;

    fn pick(set: &str, name: &str, level: u8) -> SelectedPower {
        SelectedPower::picked(name, set, level)
    }

    fn moving(power: &SelectedPower, floor: u8) -> MovingPick {
        MovingPick {
            key: PickKey::of(power),
            name: power.internal_name.clone(),
            level: power.level,
            floor,
        }
    }

    /// A build with a primary taking 1 and 2, a secondary taking 1 and 4, and a pool power at 6.
    fn build() -> CharacterState {
        let mut state = CharacterState::empty(DatasetId::Homecoming);
        state.primary.id = Some("Prim".into());
        state.primary.powers = vec![pick("Prim", "P1", 1), pick("Prim", "P2", 2)];
        state.secondary.id = Some("Sec".into());
        state.secondary.powers = vec![pick("Sec", "S1", 1), pick("Sec", "S2", 4)];
        state.pools.push(coh_data::PoolSelection {
            id: "Pool".into(),
            name: "Pool".into(),
            powers: vec![pick("Pool", "Hover", 6)],
        });
        state
    }

    fn level_of(state: &CharacterState, set: &str, name: &str) -> u8 {
        state
            .picked_powers()
            .find(|p| p.powerset == set && p.internal_name == name)
            .unwrap()
            .level
    }

    #[test]
    fn swap_trades_the_two_levels_and_nothing_else() {
        let mut state = build();
        state.secondary.powers[1].slots = vec![None, None, None];
        let s2 = moving(&state.secondary.powers[1], 2);
        let hover = moving(&state.pools[0].powers[0], 4);
        assert_eq!(move_refusal(&state, &s2, 6, Some(&hover), None), None);
        apply_move(&mut state, &s2, 6, Some(&hover));
        assert_eq!(level_of(&state, "Sec", "S2"), 6);
        assert_eq!(level_of(&state, "Pool", "Hover"), 4);
        assert_eq!(state.secondary.powers[1].slots.len(), 3);
    }

    #[test]
    fn move_to_an_unspent_pick_takes_its_level() {
        let mut state = build();
        let s2 = moving(&state.secondary.powers[1], 2);
        assert_eq!(move_refusal(&state, &s2, 10, None, None), None);
        apply_move(&mut state, &s2, 10, None);
        assert_eq!(level_of(&state, "Sec", "S2"), 10);
    }

    #[test]
    fn a_power_cannot_land_below_its_floor_either_way_round() {
        let state = build();
        // Hover's pool floor is 4: dragging it to 2 is refused…
        let hover = moving(&state.pools[0].powers[0], 4);
        let p2 = moving(&state.primary.powers[1], 2);
        assert!(move_refusal(&state, &hover, 2, Some(&p2), None).is_some());
        // …and so is dragging P2 onto it, which would send Hover down to 2.
        assert!(move_refusal(&state, &p2, 6, Some(&hover), None).is_some());
    }

    #[test]
    fn level_one_keeps_one_primary_and_one_secondary() {
        let state = build();
        // P2 into level 1 by swapping with S1 would give the primary two level-1 picks.
        let p2 = moving(&state.primary.powers[1], 1);
        let s1 = moving(&state.secondary.powers[0], 1);
        assert!(move_refusal(&state, &p2, 1, Some(&s1), None).is_some());
        // Swapping within the primary only trades which primary power holds level 1.
        let p1 = moving(&state.primary.powers[0], 1);
        assert_eq!(move_refusal(&state, &p2, 1, Some(&p1), None), None);
    }

    #[test]
    fn level_up_mode_refuses_levels_above_the_character() {
        let state = build();
        let s2 = moving(&state.secondary.powers[1], 2);
        assert!(move_refusal(&state, &s2, 10, None, Some(8)).is_some());
        assert_eq!(move_refusal(&state, &s2, 8, None, Some(8)), None);
    }

    #[test]
    fn same_level_is_not_a_move() {
        let state = build();
        let p1 = moving(&state.primary.powers[0], 1);
        let s1 = moving(&state.secondary.powers[0], 1);
        assert!(move_refusal(&state, &p1, 1, Some(&s1), None).is_some());
    }
}
