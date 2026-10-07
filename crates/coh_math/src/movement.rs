//! `resolveMovementTotals` (`character-totals.ts:1009`) — the travel-buff commit.
//!
//! Like stealth ([`crate::stealth`]), a movement buff's contribution depends on the OTHER
//! buffs: active powers sharing a binary suppress group (`stack_key`, the `kTravelBuff` group
//! covering Super Speed / Super Jump / Fly / Combat Jumping / Ninja Run momentum) do not stack —
//! only the strongest per axis applies — while a null-key source (Sprint, Swift, Hurdle, set
//! bonuses, and all of Rebirth, whose i25-era data predates the travel groups) stacks additively.
//! So the apply loop only COLLECTS a [`MovementContribution`] per power, and this module commits
//! the grouped totals once every source is known.
//!
//! Two differences from the stealth resolve:
//!   * **Combat mode.** Buffs the game shuts off in combat (`suppressible` — Super Speed's run
//!     buff, Super Jump's jump buffs, Fly's speed) contribute nothing while `combat_mode`; Combat
//!     Jumping / Hover carry no suppress event and persist. Stealth has no such axis.
//!   * **Accumulate, not assign.** The beta does `global[stat] += total` (`:1032`), because set
//!     bonuses have already written these fields before the resolve runs. So this takes
//!     `&mut GlobalBonuses` and adds, unlike [`crate::stealth::resolve_stealth_radius`] which
//!     returns totals to assign.
//!
//! # Ordering is load-bearing for exact-f64
//!
//! The beta sums each suppress group's winner by JS `Map` iteration order (first-insertion of each
//! key among the contributions), then adds the ungrouped ones in contribution order. Float
//! addition is not associative, so the winners are summed with an insertion-ordered `Vec` and the
//! ungrouped sources in contribution order, exactly as [`crate::stealth`] does.

use crate::totals::route_closed;
use crate::GlobalBonuses;
use coh_data::{at_level, ArchetypeCaps, MovementAxes, MovementAxisTables};

/// Feet per second at scale 1.0 — `BASE_PLAYER_FORWARDS_SPEED`, 0.7 feet per tick
/// (`Common/entity/entity.c`) over the 30 Hz tick, which `setSpeed` multiplies the character's
/// `fSpeedRunning`/`fSpeedFlying`/`fSpeedJumping` by (`MapServer/src/entity/entGameActions.c`).
/// The same 21 the client's own readout uses.
const FEET_PER_SECOND_AT_UNIT_SCALE: f64 = 21.0;
const SECONDS_PER_HOUR: f64 = 3600.0;
const FEET_PER_MILE: f64 = 5280.0;
/// Feet of jump height at scale 1.0. JumpHeight is the client's one `kAttribStyle_Distance`
/// attribute with a scale factor of its own (`uiCombatNumbers.c:196`, `fVal *= 4`), which is why
/// it reads in feet where the other three read in mph.
const FEET_PER_UNIT_JUMP_HEIGHT: f64 = 4.0;

/// One of the four travel axes a movement buff can target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MovementStat {
    RunSpeed,
    FlySpeed,
    JumpSpeed,
    JumpHeight,
}

impl MovementStat {
    /// Every axis, in the order the panels show them.
    pub const ALL: [MovementStat; 4] = [
        MovementStat::RunSpeed,
        MovementStat::FlySpeed,
        MovementStat::JumpSpeed,
        MovementStat::JumpHeight,
    ];

    /// The `GlobalBonuses` (beta camelCase) field this axis commits to.
    fn global_field(self) -> &'static str {
        match self {
            MovementStat::RunSpeed => "runSpeed",
            MovementStat::FlySpeed => "flySpeed",
            MovementStat::JumpSpeed => "jumpSpeed",
            MovementStat::JumpHeight => "jumpHeight",
        }
    }

    /// A game-unit scale in the unit the client shows for this axis — miles per hour for the
    /// three speeds (`uiCombatNumbers.c:224`, `kAttribStyle_Speed`), feet for jump height
    /// (`:193`, `kAttribStyle_Distance`).
    pub fn to_display_unit(self, scale: f64) -> f64 {
        match self {
            MovementStat::RunSpeed | MovementStat::FlySpeed | MovementStat::JumpSpeed => {
                scale * FEET_PER_SECOND_AT_UNIT_SCALE * SECONDS_PER_HOUR / FEET_PER_MILE
            }
            MovementStat::JumpHeight => scale * FEET_PER_UNIT_JUMP_HEIGHT,
        }
    }

    /// This axis's slot out of a per-axis scalar set — the unbuffed base, or the floor.
    fn axis_scale(self, axes: &MovementAxes) -> f64 {
        match self {
            MovementStat::RunSpeed => axes.run_speed,
            MovementStat::FlySpeed => axes.fly_speed,
            MovementStat::JumpSpeed => axes.jump_speed,
            MovementStat::JumpHeight => axes.jump_height,
        }
    }

    /// This axis's per-level ceiling row.
    fn cap_row(self, tables: &MovementAxisTables) -> &[f64] {
        match self {
            MovementStat::RunSpeed => &tables.run_speed,
            MovementStat::FlySpeed => &tables.fly_speed,
            MovementStat::JumpSpeed => &tables.jump_speed,
            MovementStat::JumpHeight => &tables.jump_height,
        }
    }
}

/// One movement-buff source, gathered during the apply loop and resolved with every other source
/// by [`resolve_movement_totals`].
///
/// `value` is the resolved buff PERCENT (post AT-table, post enhancement). `stack_key` is the
/// binary suppress group (`None` = stacks additively). `suppressible` marks a buff dropped in
/// combat. The beta pushes a contribution for every non-zero movement effect (`:1017` filters
/// `value !== 0`), so both signs reach the resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementContribution {
    pub stat: MovementStat,
    pub value: f64,
    pub stack_key: Option<String>,
    pub suppressible: bool,
    /// The DISPLAY name of the power this came from — what the breakdown row is labelled
    /// with, exactly as the beta labels it (`power.name`). Carried directly rather than
    /// resolved from a (set, internal name) pair: a conditional/pet synthetic has no selection
    /// of its own to resolve against.
    pub power_name: String,
}

/// One movement source as the dashboard breakdown shows it — the value it WOULD contribute,
/// plus whether it lost its suppress group (or was dropped by combat mode).
///
/// `suppressed` is not `capped`: mutual suppression among travel powers is ordinary game
/// mechanics, while `capped` drives the Rule-of-5 warning ring. Conflating them put a spurious
/// over-cap warning on nearly every build that ran Combat Jumping beside Super Jump.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MovementBreakdownSource {
    pub breakdown_key: String,
    pub value: f64,
    pub suppressed: bool,
    pub power_name: String,
}

/// The grouped total for one axis: the sum of each suppress group's LARGEST value, plus every
/// ungrouped value. `combat_mode` drops `suppressible` sources entirely (they win no group and add
/// nothing). Mirrors `resolveMovementTotals`'s per-stat body (`:1016-1032`).
fn resolve_axis(contributions: &[MovementContribution], combat_mode: bool) -> f64 {
    // Winners in first-insertion key order — see the module doc on f64 ordering.
    let mut group_winners: Vec<(&str, f64)> = Vec::new();
    let eligible = || {
        contributions
            .iter()
            .filter(|c| c.value != 0.0 && !(combat_mode && c.suppressible))
    };
    for c in eligible() {
        let Some(key) = c.stack_key.as_deref() else {
            continue;
        };
        match group_winners.iter_mut().find(|(k, _)| *k == key) {
            Some((_, best)) if c.value > *best => *best = c.value,
            Some(_) => {}
            None => group_winners.push((key, c.value)),
        }
    }

    let mut total = 0.0;
    for (_, best) in &group_winners {
        total += best; // one winner per suppress group
    }
    for c in eligible() {
        if c.stack_key.is_none() {
            total += c.value; // ungrouped sources stack
        }
    }
    total
}

/// Commit the gathered movement contributions to the travel totals, ADDING each axis's grouped
/// total to its `GlobalBonuses` field (the beta's `global[stat] += total`).
pub fn resolve_movement_totals(
    contributions: &[MovementContribution],
    global: &mut GlobalBonuses,
    combat_mode: bool,
) -> Vec<MovementBreakdownSource> {
    let mut breakdown = Vec::new();
    for stat in [
        MovementStat::RunSpeed,
        MovementStat::FlySpeed,
        MovementStat::JumpSpeed,
        MovementStat::JumpHeight,
    ] {
        let axis: Vec<MovementContribution> = contributions
            .iter()
            .filter(|c| c.stat == stat)
            .cloned()
            .collect();
        if axis.is_empty() {
            continue;
        }
        let total = resolve_axis(&axis, combat_mode);
        route_closed(
            global.add_by_camel_name(stat.global_field(), total),
            stat.global_field(),
        );
        // One row per source, flagged with whether it actually reached the total: combat mode
        // drops a suppressible source outright, and within a suppress group only the largest
        // applies. The row still carries its own value so the tooltip can show what it would
        // have contributed.
        let winners = group_winner_values(&axis, combat_mode);
        // Which contribution IS each group's winner, by position among the eligible ones.
        // Identity, not value: two sources tied at a group's maximum are both "not less than the
        // best", so a value compare leaves both rows live and the breakdown sums to DOUBLE what
        // the group contributed (`resolve_axis` adds each group exactly once). The winner is the
        // first eligible member to reach the maximum, matching `group_winner_values`, whose `>`
        // never displaces an equal incumbent.
        let eligible =
            |c: &MovementContribution| c.value != 0.0 && !(combat_mode && c.suppressible);
        let winning_index = |key: &str, best: f64| {
            axis.iter()
                .position(|c| eligible(c) && c.stack_key.as_deref() == Some(key) && c.value == best)
        };
        for (index, c) in axis.iter().enumerate().filter(|(_, c)| c.value != 0.0) {
            let suppressed = match c.stack_key.as_deref() {
                Some(key) => {
                    (combat_mode && c.suppressible)
                        || winners
                            .iter()
                            .find(|(k, _)| k == key)
                            .is_none_or(|(_, best)| winning_index(key, *best) != Some(index))
                }
                None => combat_mode && c.suppressible,
            };
            breakdown.push(MovementBreakdownSource {
                breakdown_key: stat.global_field().to_string(),
                value: c.value,
                suppressed,
                power_name: c.power_name.clone(),
            });
        }
    }
    breakdown
}

/// One active travel power's CEILING raise, gathered during the apply loop beside the ordinary
/// [`MovementContribution`]s. These come from the power's `aspect=Maximum` movement templates
/// (`effects.movementCapBump`) and are in movement SCALE units, not percentages: Super Speed's
/// +1.938 run and Fly's +2.0475 raise how fast the character is ALLOWED to go, which is a
/// different question from how fast the buffs make them.
#[derive(Debug, Clone, PartialEq)]
pub struct MovementCapContribution {
    pub stat: MovementStat,
    pub scale: f64,
    pub stack_key: Option<String>,
    pub suppressible: bool,
}

/// The scale added to each axis's class ceiling by the build's active travel powers.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct MovementCapBumps {
    pub run_speed: f64,
    pub fly_speed: f64,
    pub jump_speed: f64,
    pub jump_height: f64,
}

impl MovementCapBumps {
    fn get(&self, stat: MovementStat) -> f64 {
        match stat {
            MovementStat::RunSpeed => self.run_speed,
            MovementStat::FlySpeed => self.fly_speed,
            MovementStat::JumpSpeed => self.jump_speed,
            MovementStat::JumpHeight => self.jump_height,
        }
    }

    fn set(&mut self, stat: MovementStat, scale: f64) {
        match stat {
            MovementStat::RunSpeed => self.run_speed = scale,
            MovementStat::FlySpeed => self.fly_speed = scale,
            MovementStat::JumpSpeed => self.jump_speed = scale,
            MovementStat::JumpHeight => self.jump_height = scale,
        }
    }
}

/// Resolve the gathered ceiling raises per axis (`getEffectiveMovementCaps`,
/// `movement-constants.ts:73`). Within a suppress group only the strongest bump applies; distinct
/// groups ADD (Fly's `kTravelMaxBuff` plus Afterburner's `kTravelTurboMaxBuff`), and an unkeyed
/// bump is its own group so it always adds. Combat mode drops the suppressible ones — Super
/// Jump's and Afterburner's go, Super Speed's and Fly's persist.
///
/// Only positive bumps count: a NEGATIVE `aspect=Maximum` movement template is a cap DEBUFF,
/// which the converter routes to `movementCapDebuff` instead, so one arriving here would be
/// double-counted.
pub fn resolve_cap_bumps(
    contributions: &[MovementCapContribution],
    combat_mode: bool,
) -> MovementCapBumps {
    let mut bumps = MovementCapBumps::default();
    for stat in MovementStat::ALL {
        // Winners in first-insertion key order, as [`resolve_axis`] does — the sum is over f64.
        let mut group_winners: Vec<(&str, f64)> = Vec::new();
        let mut total = 0.0;
        let eligible = contributions
            .iter()
            .filter(|c| c.stat == stat && c.scale > 0.0 && !(combat_mode && c.suppressible));
        for c in eligible {
            let Some(key) = c.stack_key.as_deref() else {
                total += c.scale; // unkeyed: its own group of one
                continue;
            };
            match group_winners.iter_mut().find(|(k, _)| *k == key) {
                Some((_, best)) if c.scale > *best => *best = c.scale,
                Some(_) => {}
                None => group_winners.push((key, c.scale)),
            }
        }
        for (_, best) in &group_winners {
            total += best;
        }
        bumps.set(stat, total);
    }
    bumps
}

/// One axis as the dashboard shows it: the speed (or height) the character actually reaches, and
/// the ceiling it is measured against — both already in display units.
#[derive(Debug, Default, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ProjectedMovement {
    pub value: f64,
    /// `0.0` when the archetype carries no ceiling row, meaning UNKNOWN — never "capped at zero".
    pub cap: f64,
}

/// Project one axis's buff percentage onto the speed the character reaches.
///
/// The server applies movement as a MULTIPLY: `attrCur.fSpeedRunning *= pattrMod->fSpeedRunning`
/// where the mod accumulates around a ModBase of 1.0 (`character_attribs.c:800`), so a +150% total
/// takes the class's base scale to 2.5× it — and `ClampCur` then bounds the result BETWEEN the
/// class's AttribMin scalar and `attrMax`, the AttribMaxTable row plus whatever the active travel
/// powers raised it by. Fly's base scale is 1.5 rather than 0, which is why a flying character
/// starts at 21.5 mph and not at nothing.
///
/// The floor is the half that used to be missing (MOVEMIN-1). A grounding power writes "you cannot
/// move on this axis" as a saturating magnitude — Granite Armor and Rooted state `JumpHeight
/// −500 × Melee_Ones`, Rest states `−1000` on all four axes — so without it the multiply reports a
/// negative speed. It is read per class rather than assumed zero because it ISN'T zero: run and
/// fly floor at 0.1 on every player class, which is the crawl the game leaves you.
///
/// Floor and ceiling are applied in that order. On player data the order is unobservable (every
/// class floors far below its own ceiling); it is stated so the behaviour is defined if an NPC
/// class, which authors floors as high as 1.95 run, is ever projected here.
pub fn project_axis(
    stat: MovementStat,
    buff_percent: f64,
    caps: &ArchetypeCaps,
    bumps: &MovementCapBumps,
    level: i32,
) -> ProjectedMovement {
    let floor = stat.axis_scale(&caps.movement_floor);
    let scale = stat.axis_scale(&caps.movement_base) * (1.0 + buff_percent / 100.0);
    let Some(class_cap) = at_level(stat.cap_row(&caps.movement_cap_table), level) else {
        // No ceiling row for this archetype: report the floored speed and leave the cap unknown
        // rather than clamping to a fabricated one (Rule 1). The floor is its own row, so a
        // missing ceiling does not withdraw it.
        return ProjectedMovement {
            value: stat.to_display_unit(scale.max(floor)),
            cap: 0.0,
        };
    };
    let cap = class_cap + bumps.get(stat);
    ProjectedMovement {
        value: stat.to_display_unit(scale.max(floor).min(cap)),
        cap: stat.to_display_unit(cap),
    }
}

/// The winning value of each suppress group on one axis — the same selection `resolve_axis`
/// sums, exposed so the breakdown can mark the losers.
fn group_winner_values(
    contributions: &[MovementContribution],
    combat_mode: bool,
) -> Vec<(String, f64)> {
    let mut winners: Vec<(String, f64)> = Vec::new();
    for c in contributions
        .iter()
        .filter(|c| c.value != 0.0 && !(combat_mode && c.suppressible))
    {
        let Some(key) = c.stack_key.as_deref() else {
            continue;
        };
        match winners.iter_mut().find(|(k, _)| k == key) {
            Some((_, best)) if c.value > *best => *best = c.value,
            Some(_) => {}
            None => winners.push((key.to_string(), c.value)),
        }
    }
    winners
}
