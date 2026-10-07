//! Leveling schedule — the level-gated enhancement-slot and power-pick budget a
//! character earns as it levels. SE2 (SLOT-ECONOMY-PLAN): the typed view of the
//! contract's `leveling-schedule` section, which `emit-contract.cjs` sources from
//! the generated per-dataset module (itself derived from `schedules.bin` —
//! `AssignableBoost` / `Power` — via `convert-leveling-schedule.cjs`, WS17), so
//! this chain is export == module == contract == this struct.
//!
//! This is the sourced replacement for the beta's hand-authored `SLOT_GRANTS` /
//! `POWER_PICK_LEVELS` — sourced from the binary, not a second-hand table.
//!
//! Data/calc split (D2): this struct is pure DATA. Budget queries that combine it
//! with a build (`placed_budget_slots`, remaining) live beside it as pure fns; the
//! two cumulative lookups here are pure over the schedule alone.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The per-dataset leveling grant schedule. Grant maps are keyed by 1-based
/// character level; the count of grants at levels ≤ a level is how many of that
/// thing the character has at that level (`CountForLevel`, power_system.c).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LevelingSchedule {
    /// Dataset id, guarded against the bundle's manifest at load (Rule 0).
    pub dataset: String,
    /// Placeable enhancement slots granted at each 1-based level (level → count).
    /// Excludes each power's free base slot and any inherent auto-slots.
    pub slot_grants: BTreeMap<u8, u32>,
    /// Total placeable slots by level 50 — a stored cross-check on `slot_grants`
    /// (67 Homecoming/Rebirth, 71 Thunderspy).
    pub total_slots: u32,
    /// Power picks granted at each 1-based level (level → count; level 1 grants 2
    /// for primary + secondary).
    pub power_picks: BTreeMap<u8, u32>,
    /// Total power picks by level 50.
    pub max_power_picks: u32,
    /// 1-based level of the first pool-powerset pick.
    pub pool_unlock_level: u8,
    /// Total pool-powerset picks — the power-pool cap.
    pub max_power_pools: u32,
    /// 1-based level of the first epic-powerset pick.
    pub epic_pool_level: u8,
    /// 1-based levels at which a named inherent power receives auto-granted bonus
    /// slots (Rebirth Health/Stamina), OUTSIDE the `total_slots` user budget. `{}`
    /// on Homecoming/Thunderspy. Read by [`crate::granted_inherents`], which counts
    /// the entries at or below the build's level into `inherent_slot_count`.
    pub auto_granted_slot_levels: BTreeMap<String, Vec<u8>>,
}

impl LevelingSchedule {
    /// Parse the contract's `leveling-schedule` section. Malformed ≠ absent
    /// (Rule 1, matching the sibling section readers): an ABSENT section yields
    /// `None` (a hand-constructed `PowerDatabase` carries no schedule, and
    /// consumers must surface that as an error, not a default), but a PRESENT
    /// section that doesn't parse — or whose grants are vacuous or disagree with
    /// their own stored total — is an error.
    pub fn from_section(section: Option<&Value>) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let schedule: LevelingSchedule = serde_json::from_value(section.clone())
            .map_err(|e| format!("leveling-schedule section: {e}"))?;
        if schedule.slot_grants.is_empty() {
            return Err("leveling-schedule section: empty slot grants".into());
        }
        if schedule.power_picks.is_empty() {
            return Err("leveling-schedule section: empty power picks".into());
        }
        // The stored total must equal the sourced grants it summarizes — a drift
        // here is a converter/export mismatch, not data (distrust counts).
        let summed_slots: u32 = schedule.slot_grants.values().sum();
        if summed_slots != schedule.total_slots {
            return Err(format!(
                "leveling-schedule section: slot grants sum to {summed_slots} but totalSlots is {}",
                schedule.total_slots
            ));
        }
        let summed_picks: u32 = schedule.power_picks.values().sum();
        if summed_picks != schedule.max_power_picks {
            return Err(format!(
                "leveling-schedule section: power picks sum to {summed_picks} but maxPowerPicks is {}",
                schedule.max_power_picks
            ));
        }
        Ok(Some(schedule))
    }

    /// Cumulative placeable enhancement slots available at `level` — the beta
    /// `getTotalSlotsAtLevel`. Σ of grants at every level ≤ `level`.
    pub fn total_slots_at_level(&self, level: u8) -> usize {
        self.slot_grants
            .range(..=level)
            .map(|(_, &count)| count as usize)
            .sum()
    }

    /// Cumulative power picks available at `level` — the beta
    /// `getPowerPicksAtLevel`. Σ of picks granted at every level ≤ `level`.
    pub fn total_power_picks_at_level(&self, level: u8) -> usize {
        self.power_picks
            .range(..=level)
            .map(|(_, &count)| count as usize)
            .sum()
    }

    /// The highest level this schedule grants anything at — the ceiling a level control
    /// offers. Read from the grants rather than written down as 50 (Rule 0): a fork that
    /// moved the cap would move this on its own.
    ///
    /// `None` only for a hand-constructed schedule with empty grant maps, which
    /// [`from_section`](Self::from_section) already rejects; a caller holding a parsed
    /// schedule can treat it as the same "no schedule, no ceiling" case it already handles.
    pub fn max_level(&self) -> Option<u8> {
        self.slot_grants
            .keys()
            .chain(self.power_picks.keys())
            .copied()
            .max()
    }

    /// Every power pick as its own entry, ascending — a level that grants two picks
    /// appears twice (level 1: primary + secondary). `max_power_picks` entries long,
    /// which `from_section` has already cross-checked against the grant map.
    ///
    /// This is the sourced replacement for the beta's hand-authored
    /// `POWER_PICK_LEVELS` (Rule 0), and it is what the by-level power view lays its
    /// slots out from — one entry, one slot.
    pub fn pick_levels(&self) -> Vec<u8> {
        self.power_picks
            .iter()
            .flat_map(|(&level, &count)| std::iter::repeat_n(level, count as usize))
            .collect()
    }

    /// Power picks granted at exactly `level` — the beta `getPicksGrantedAtLevel`, which
    /// hardcoded "2 at level 1, 1 everywhere else"; here it is the schedule's own word.
    pub fn picks_granted_at_level(&self, level: u8) -> u32 {
        self.power_picks.get(&level).copied().unwrap_or(0)
    }

    /// Placeable slots granted at exactly `level` — the beta `getSlotsGrantedAtLevel`.
    pub fn slots_granted_at_level(&self, level: u8) -> u32 {
        self.slot_grants.get(&level).copied().unwrap_or(0)
    }

    /// The next level above `level` that grants a pick or a slot — where level-up mode's
    /// advance button goes, since the levels between grants ask nothing of the player.
    ///
    /// `None` = nothing is granted above `level`, which the caller reads as "at max level"
    /// rather than substituting a ceiling (the beta's `getNextGrantLevel` returned
    /// `MAX_LEVEL` here, making "no grant left" indistinguishable from "a grant at 50").
    pub fn next_grant_level(&self, level: u8) -> Option<u8> {
        self.slot_grants
            .range(level.saturating_add(1)..)
            .map(|(&grant_level, _)| grant_level)
            .chain(
                self.power_picks
                    .range(level.saturating_add(1)..)
                    .map(|(&grant_level, _)| grant_level),
            )
            .min()
    }

    /// The level a build of this usage is actually working on — the lowest level whose
    /// cumulative grants exceed what has been spent (the beta `getProgressionLevel`). This
    /// is what level-up mode reads the build's level down to when it turns on, so grants
    /// from levels the build hasn't reached can't already be spent.
    ///
    /// Saturates at the schedule's own ceiling: a build that has spent everything is at max
    /// level, not past it. `None` mirrors [`max_level`](Self::max_level)'s `None`.
    pub fn progression_level(&self, picks_used: usize, slots_used: usize) -> Option<u8> {
        let ceiling = self.max_level()?;
        let mut picks_granted = 0usize;
        let mut slots_granted = 0usize;
        for level in 1..=ceiling {
            picks_granted += self.picks_granted_at_level(level) as usize;
            slots_granted += self.slots_granted_at_level(level) as usize;
            if picks_used < picks_granted || slots_used < slots_granted {
                return Some(level);
            }
        }
        Some(ceiling)
    }

    /// The level a new power lands at: the earliest [`pick_levels`](Self::pick_levels)
    /// entry that no already-picked power occupies and that is at or above `min_level`
    /// (the power's own unlock level). `None` = the build has no pick left this power
    /// could legally take, which the caller surfaces rather than substituting a level
    /// the game wouldn't grant.
    ///
    /// `taken` is the levels of the powers already picked, in any order; a level
    /// granting two picks is only full once two powers sit on it. Entries that match no
    /// pick level (a build carrying a level the schedule doesn't grant) occupy nothing.
    pub fn next_pick_level(&self, taken: &[u8], min_level: u8) -> Option<u8> {
        let mut unclaimed = taken.to_vec();
        self.pick_levels().into_iter().find(|&level| {
            match unclaimed.iter().position(|&t| t == level) {
                // This pick is spent by an already-picked power.
                Some(index) => {
                    unclaimed.swap_remove(index);
                    false
                }
                None => level >= min_level,
            }
        })
    }
}

/// Placeable enhancement slots the build has spent — the beta
/// `countPlacedBudgetSlots`. A power's free base slot (index 0) and its inherent
/// auto-slots don't count against the user budget, so each power contributes
/// `slots.len() - 1 - inherent_slot_count` (saturating; a power with only its base
/// slot contributes 0). Pure over the build; the schedule sets the ceiling.
pub fn placed_budget_slots(state: &crate::CharacterState) -> usize {
    state
        .all_selected()
        .map(|power| {
            power
                .slots
                .len()
                .saturating_sub(1 + power.inherent_slot_count as usize)
        })
        .sum()
}

/// Slots left in the build's budget at its level — the beta `slotRemaining`.
/// `budget.saturating_sub(used)`: over-budget reads as 0 remaining (the UI flags
/// the overage separately from this count).
pub fn slots_remaining(budget: usize, used: usize) -> usize {
    budget.saturating_sub(used)
}

/// What the build still owes at the level it currently holds, and where the next grant sits
/// — the readout level-up mode puts in the header ("Lvl 12: pick 1 pwr · place 2 slots", or
/// "→ Lvl 14" once nothing is owed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LevelProgress {
    /// Power picks granted at this level that no picked power has taken.
    pub picks_owed: u32,
    /// Placeable slots granted at this level that the build hasn't placed.
    pub slots_owed: u32,
    /// The next level granting anything ([`LevelingSchedule::next_grant_level`]); `None` when
    /// this level is the schedule's last word.
    pub next_grant_level: Option<u8>,
}

impl LevelProgress {
    /// Nothing left to spend at this level, so advancing is the only move left.
    pub fn is_spent(&self) -> bool {
        self.picks_owed == 0 && self.slots_owed == 0
    }
}

/// Measure [`LevelProgress`] for a build against its dataset's schedule.
///
/// **The two halves count differently, because the data differs.** A pick carries the level
/// the game granted it ([`LevelingSchedule::next_pick_level`] assigns it), so picks owed at a
/// level is that level's grant minus the powers actually sitting on it — exact even when a
/// power's own unlock level forced it past a free earlier pick, which the beta's
/// cumulative-subtraction count read as satisfying the earlier level. Slots are counted in
/// total rather than by their levels ([`crate::slot_levels`]): total placed, less everything
/// granted below this level.
pub fn level_progress(schedule: &LevelingSchedule, state: &crate::CharacterState) -> LevelProgress {
    let level = state.level;
    let picks_taken_here = state
        .picked_powers()
        .filter(|power| power.level == level)
        .count();
    let slots_placed_below = schedule.total_slots_at_level(level.saturating_sub(1));
    let slots_placed_here = placed_budget_slots(state).saturating_sub(slots_placed_below);

    LevelProgress {
        picks_owed: schedule
            .picks_granted_at_level(level)
            .saturating_sub(picks_taken_here as u32),
        slots_owed: schedule
            .slots_granted_at_level(level)
            .saturating_sub(slots_placed_here as u32),
        next_grant_level: schedule.next_grant_level(level),
    }
}
