//! Slot levels — the character level each enhancement slot was granted at.
//!
//! The port of the beta's `slot-levels.ts`. Every power's first slot comes free with the pick,
//! so it sits at the pick's own level. Every slot after that draws one grant from the dataset's
//! [`LevelingSchedule::slot_grants`], and its level is the level of the grant it drew. A
//! power's trailing inherent auto-slots (Rebirth's Health and Stamina) sit at the fixed levels
//! the schedule names for them and draw nothing.
//!
//! **One solver answers every question.** A slot may only take a grant at or above its power's
//! pick level, and each grant serves one slot, so which slot gets which grant is a bipartite
//! matching rather than a walk down a list ([`assign_grants`]). The display, the placement probe
//! and the freeze all run it over the same demands, because a probe answered by a different walk
//! is how the beta's display and its placement came to disagree (SLOT-1).
//!
//! **Stored levels are preferences.** [`crate::CharacterState::slot_order`] records each slot
//! placement, and the level it was placed at. The solver honours a stored level wherever the
//! schedule still can, so removing one slot leaves its peers where they were, as in Mids. It
//! gives one up only where keeping it would leave another slot with no grant at all.
//!
//! **A slot the schedule cannot serve has no level** (`None`), never a plausible stand-in. The
//! pick level in particular is not a safe fallback: on a power taken at 38 it names a level
//! that grants no slots.
//!
//! Slot indices here follow this model's layout — base slot, the user's slots, then the
//! trailing inherent auto-slots ([`crate::SelectedPower::inherent_slot_count`]) — so an entry
//! addresses the same slot the powers panel draws.

use crate::character::{CharacterState, SelectedPower, SlotLevelSource, SlotOrderEntry};
use crate::leveling_schedule::LevelingSchedule;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

/// Which bucket of the build a power sits in — the beta's `PowerCategory`, and the spelling
/// [`SlotOrderEntry::category`] carries so a `.skif` round-trips through both planners.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotCategory {
    Inherent,
    Primary,
    Secondary,
    Pool,
    Epic,
}

impl SlotCategory {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            SlotCategory::Inherent => "inherent",
            SlotCategory::Primary => "primary",
            SlotCategory::Secondary => "secondary",
            SlotCategory::Pool => "pool",
            SlotCategory::Epic => "epic",
        }
    }

    fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "inherent" => SlotCategory::Inherent,
            "primary" => SlotCategory::Primary,
            "secondary" => SlotCategory::Secondary,
            "pool" => SlotCategory::Pool,
            "epic" => SlotCategory::Epic,
            _ => return None,
        })
    }

    /// The bucket that holds the pick `powerset_id` + `internal_name`, the way the rest of the
    /// app addresses a pick. `None` when the build holds no such pick.
    pub fn of(state: &CharacterState, powerset_id: &str, internal_name: &str) -> Option<Self> {
        let holds =
            |powers: &[SelectedPower]| powers.iter().any(|p| p.internal_name == internal_name);
        if state.primary.id.as_deref() == Some(powerset_id) && holds(&state.primary.powers) {
            return Some(SlotCategory::Primary);
        }
        if state.secondary.id.as_deref() == Some(powerset_id) && holds(&state.secondary.powers) {
            return Some(SlotCategory::Secondary);
        }
        if state
            .pools
            .iter()
            .any(|pool| pool.id == powerset_id && holds(&pool.powers))
        {
            return Some(SlotCategory::Pool);
        }
        if state
            .epic_pool
            .as_ref()
            .is_some_and(|pool| pool.id == powerset_id && holds(&pool.powers))
        {
            return Some(SlotCategory::Epic);
        }
        if powerset_id == crate::INHERENT_SET {
            return holds(&state.inherents).then_some(SlotCategory::Inherent);
        }
        // A VEAT branch pick sits in a role list under its own set id, not the list's.
        let carries = |powers: &[SelectedPower]| {
            powers
                .iter()
                .any(|p| p.powerset == powerset_id && p.internal_name == internal_name)
        };
        if carries(&state.primary.powers) {
            Some(SlotCategory::Primary)
        } else if carries(&state.secondary.powers) {
            Some(SlotCategory::Secondary)
        } else {
            None
        }
    }
}

/// A power's address for slot levels: its bucket plus its internal name (the beta `powerKey`).
pub type SlotKey = (SlotCategory, String);

/// Every power's slot levels, parallel to its `slots`. Index 0 is the pick level; `None` is a
/// slot the schedule has no grant left for.
pub type SlotLevels = BTreeMap<SlotKey, Vec<Option<u8>>>;

/// The level a pick's slots may start at. Auto-granted inherents carry `0` ("unset"), which
/// the game treats as present from level 1.
fn pick_level(power: &SelectedPower) -> u8 {
    power.level.max(1)
}

/// The slots on a power that the user placed and that draw from the grant pool: after the free
/// base slot, before the trailing inherent auto-slots.
fn user_band(power: &SelectedPower) -> Range<usize> {
    let end = power
        .slots
        .len()
        .saturating_sub(usize::from(power.inherent_slot_count));
    1..end.max(1)
}

/// Every power in the build with its bucket, ordered by pick level then bucket.
fn placed_powers(state: &CharacterState) -> Vec<(SlotCategory, &SelectedPower)> {
    let mut all: Vec<(SlotCategory, &SelectedPower)> = state
        .inherents
        .iter()
        .map(|p| (SlotCategory::Inherent, p))
        .chain(
            state
                .primary
                .powers
                .iter()
                .map(|p| (SlotCategory::Primary, p)),
        )
        .chain(
            state
                .secondary
                .powers
                .iter()
                .map(|p| (SlotCategory::Secondary, p)),
        )
        .chain(
            state
                .pools
                .iter()
                .flat_map(|pool| pool.powers.iter().map(|p| (SlotCategory::Pool, p))),
        )
        .chain(
            state
                .epic_pool
                .iter()
                .flat_map(|pool| pool.powers.iter().map(|p| (SlotCategory::Epic, p))),
        )
        .collect();
    all.sort_by_key(|(category, power)| (pick_level(power), *category));
    all
}

/// Which bucket an entry addresses. An entry written before `category` existed falls back to
/// searching the build in the beta's order.
fn entry_category(state: &CharacterState, entry: &SlotOrderEntry) -> Option<SlotCategory> {
    if let Some(category) = entry.category.as_deref().and_then(SlotCategory::parse) {
        return Some(category);
    }
    let name = entry.power_name.as_str();
    let holds = |powers: &[SelectedPower]| powers.iter().any(|p| p.internal_name == name);
    if holds(&state.primary.powers) {
        Some(SlotCategory::Primary)
    } else if holds(&state.secondary.powers) {
        Some(SlotCategory::Secondary)
    } else if state.pools.iter().any(|pool| holds(&pool.powers)) {
        Some(SlotCategory::Pool)
    } else if state
        .epic_pool
        .as_ref()
        .is_some_and(|pool| holds(&pool.powers))
    {
        Some(SlotCategory::Epic)
    } else if holds(&state.inherents) {
        Some(SlotCategory::Inherent)
    } else {
        None
    }
}

/// Every grant the schedule has issued by `level`, one entry per slot, ascending.
fn grant_pool(schedule: &LevelingSchedule, level: u8) -> Vec<u8> {
    schedule
        .slot_grants
        .range(..=level)
        .flat_map(|(&grant, &count)| std::iter::repeat_n(grant, count as usize))
        .collect()
}

/// One placed slot that needs a grant. `pick` is its floor; `preferred` is the level its
/// `slot_order` entry stored, honoured where the matching allows.
#[derive(Debug, Clone)]
struct Demand {
    key: Option<SlotKey>,
    slot: usize,
    pick: u8,
    preferred: Option<u8>,
}

/// Give each demand a distinct grant, serving as many demands as the schedule can. Returns the
/// grant index per demand, or `None` for a demand nothing can serve.
///
/// Stored levels are seeded first, so an untouched build keeps every slot where it was put.
/// Kuhn's augmenting paths then extend that seed to a maximum matching, moving a stored level
/// only when leaving it would cost another slot its grant entirely. A free grant is always
/// tried before an owned one: displacing an owner when a free grant was available rewrites a
/// level the user placed, which reversed the levelling order in the beta (SLOT-2).
fn assign_grants(demands: &[Demand], pool: &[u8]) -> Vec<Option<usize>> {
    let mut demand_grant: Vec<Option<usize>> = vec![None; demands.len()];
    let mut grant_owner: Vec<Option<usize>> = vec![None; pool.len()];

    for (d, demand) in demands.iter().enumerate() {
        let Some(preferred) = demand.preferred.filter(|&level| level >= demand.pick) else {
            continue;
        };
        if let Some(g) = (0..pool.len()).find(|&g| pool[g] == preferred && grant_owner[g].is_none())
        {
            demand_grant[d] = Some(g);
            grant_owner[g] = Some(d);
        }
    }

    fn augment(
        d: usize,
        demands: &[Demand],
        pool: &[u8],
        seen: &mut [bool],
        demand_grant: &mut [Option<usize>],
        grant_owner: &mut [Option<usize>],
    ) -> bool {
        let pick = demands[d].pick;
        for g in 0..pool.len() {
            if pool[g] < pick || seen[g] || grant_owner[g].is_some() {
                continue;
            }
            seen[g] = true;
            demand_grant[d] = Some(g);
            grant_owner[g] = Some(d);
            return true;
        }
        for g in 0..pool.len() {
            if pool[g] < pick || seen[g] {
                continue;
            }
            seen[g] = true;
            let rehoused = match grant_owner[g] {
                None => true,
                Some(owner) => augment(owner, demands, pool, seen, demand_grant, grant_owner),
            };
            if rehoused {
                demand_grant[d] = Some(g);
                grant_owner[g] = Some(d);
                return true;
            }
        }
        false
    }

    let mut unseeded: Vec<usize> = (0..demands.len())
        .filter(|&d| demand_grant[d].is_none())
        .collect();
    unseeded.sort_by_key(|&d| demands[d].pick);
    for d in unseeded {
        let mut seen = vec![false; pool.len()];
        augment(
            d,
            demands,
            pool,
            &mut seen,
            &mut demand_grant,
            &mut grant_owner,
        );
    }
    demand_grant
}

/// Every user slot in the build, carrying the stored level from its entry where it has one.
///
/// An entry that addresses no real user slot contributes nothing, and a placed slot with no
/// entry still needs a grant — the two rules the beta once split across two callers (SLOT-1).
/// With an empty `slot_order` this is simply every user slot with no preference, which is the
/// beta's "respec mode".
fn collect_demands(state: &CharacterState) -> Vec<Demand> {
    let powers = placed_powers(state);
    let shape: BTreeMap<SlotKey, (u8, Range<usize>)> = powers
        .iter()
        .map(|(category, power)| {
            (
                (*category, power.internal_name.clone()),
                (pick_level(power), user_band(power)),
            )
        })
        .collect();

    let mut demands = Vec::new();
    let mut claimed = BTreeSet::new();
    for entry in &state.slot_order {
        let Some(category) = entry_category(state, entry) else {
            continue;
        };
        let key = (category, entry.power_name.clone());
        let Some((pick, band)) = shape.get(&key) else {
            continue;
        };
        let slot = usize::from(entry.slot_index);
        if !band.contains(&slot) || !claimed.insert((key.clone(), slot)) {
            continue;
        }
        demands.push(Demand {
            key: Some(key),
            slot,
            pick: *pick,
            preferred: entry.level,
        });
    }
    for (key, (pick, band)) in &shape {
        for slot in band.clone() {
            if claimed.contains(&(key.clone(), slot)) {
                continue;
            }
            demands.push(Demand {
                key: Some(key.clone()),
                slot,
                pick: *pick,
                preferred: None,
            });
        }
    }
    demands
}

/// Slot levels with only the fixed parts filled in: each base slot at its pick level, and each
/// inherent auto-slot at the level the schedule grants it.
fn fixed_levels(schedule: &LevelingSchedule, state: &CharacterState) -> SlotLevels {
    placed_powers(state)
        .into_iter()
        .map(|(category, power)| {
            let mut levels = vec![None; power.slots.len()];
            if let Some(first) = levels.first_mut() {
                *first = Some(pick_level(power));
            }
            let granted = usize::from(power.inherent_slot_count);
            if granted > 0 {
                let fixed = schedule
                    .auto_granted_slot_levels
                    .get(&power.internal_name)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let start = power.slots.len().saturating_sub(granted).max(1);
                for (offset, slot) in (start..power.slots.len()).enumerate() {
                    levels[slot] = fixed.get(offset).copied();
                }
            }
            ((category, power.internal_name.clone()), levels)
        })
        .collect()
}

fn solve(schedule: &LevelingSchedule, state: &CharacterState) -> SlotLevels {
    let mut levels = fixed_levels(schedule, state);
    let demands = collect_demands(state);
    let pool = grant_pool(schedule, state.level);
    for (demand, grant) in demands.iter().zip(assign_grants(&demands, &pool)) {
        let Some(key) = &demand.key else { continue };
        if let Some(slot) = levels
            .get_mut(key)
            .and_then(|power| power.get_mut(demand.slot))
        {
            *slot = grant.map(|g| pool[g]);
        }
    }
    levels
}

/// Every slot's level: stored levels honoured where the schedule allows, every other slot
/// solved around them. What the powers panel draws and the forum export prints.
pub fn slot_levels(schedule: &LevelingSchedule, state: &CharacterState) -> SlotLevels {
    solve(schedule, state)
}

/// The level a new slot on a power picked at `pick` would be placed at, or `None` when every
/// grant it could legally take is already spoken for. `None` is a real answer: the caller
/// refuses the placement rather than inventing a level.
pub fn next_grant_level(
    schedule: &LevelingSchedule,
    state: &CharacterState,
    pick: u8,
) -> Option<u8> {
    let mut demands = collect_demands(state);
    demands.push(Demand {
        key: None,
        slot: 0,
        pick: pick.max(1),
        preferred: None,
    });
    let pool = grant_pool(schedule, state.level);
    let assigned = assign_grants(&demands, &pool);
    assigned.last().copied().flatten().map(|g| pool[g])
}

/// One pick's levels out of `levels`, found by the pick itself rather than by an address — for a
/// caller walking [`CharacterState::all_selected`] that has the power but not its bucket.
pub fn levels_of<'a>(
    levels: &'a SlotLevels,
    state: &CharacterState,
    power: &SelectedPower,
) -> Option<&'a [Option<u8>]> {
    let (category, _) = placed_powers(state)
        .into_iter()
        .find(|(_, placed)| std::ptr::eq(*placed, power))?;
    levels
        .get(&(category, power.internal_name.clone()))
        .map(Vec::as_slice)
}

/// Drop entries that address no user slot — a power since removed, or an index past its row.
fn prune(state: &mut CharacterState) -> bool {
    let live: BTreeMap<SlotKey, Range<usize>> = placed_powers(state)
        .into_iter()
        .map(|(category, power)| ((category, power.internal_name.clone()), user_band(power)))
        .collect();
    let categories: Vec<Option<SlotCategory>> = state
        .slot_order
        .iter()
        .map(|entry| entry_category(state, entry))
        .collect();
    let before = state.slot_order.len();
    let mut index = 0;
    state.slot_order.retain(|entry| {
        let category = categories[index];
        index += 1;
        category.is_some_and(|category| {
            live.get(&(category, entry.power_name.clone()))
                .is_some_and(|band| band.contains(&usize::from(entry.slot_index)))
        })
    });
    state.slot_order.len() != before
}

/// Write the build's solved levels into `slot_order`, so the next edit moves nothing it did
/// not touch. Safe to run repeatedly; returns whether anything changed.
///
/// The beta's four load-time migrations, in its order:
/// - a stored level the schedule never issues at all is cleared (it could never be honoured);
/// - every user slot without an entry gains one at its solved level;
/// - every entry whose stored level the solver could not honour takes the level it was given.
///
/// Levels written here are the solver's, so they are marked [`SlotLevelSource::Packed`]: the
/// `.mbd` writer must not read them as the author's own history (MBDEXPORT-21). A slot the
/// schedule cannot serve is left without a level, so a later solve can re-house it.
pub fn freeze(schedule: &LevelingSchedule, state: &mut CharacterState) -> bool {
    let mut changed = prune(state);

    for entry in &mut state.slot_order {
        let issued = entry
            .level
            .is_some_and(|level| schedule.slot_grants.get(&level).copied().unwrap_or(0) > 0);
        if entry.level.is_some() && !issued {
            entry.level = None;
            entry.level_source = None;
            changed = true;
        }
    }

    let levels = slot_levels(schedule, state);
    let recorded: BTreeSet<(SlotKey, usize)> = state
        .slot_order
        .iter()
        .filter_map(|entry| {
            entry_category(state, entry).map(|category| {
                (
                    (category, entry.power_name.clone()),
                    usize::from(entry.slot_index),
                )
            })
        })
        .collect();

    let categories: Vec<Option<SlotCategory>> = state
        .slot_order
        .iter()
        .map(|entry| entry_category(state, entry))
        .collect();
    for (entry, category) in state.slot_order.iter_mut().zip(categories) {
        let Some(category) = category else { continue };
        let solved = levels
            .get(&(category, entry.power_name.clone()))
            .and_then(|power| power.get(usize::from(entry.slot_index)))
            .copied()
            .flatten();
        if let Some(level) = solved {
            if entry.level != Some(level) {
                entry.level = Some(level);
                entry.level_source = Some(SlotLevelSource::Packed);
                changed = true;
            }
        }
    }

    let mut missing = Vec::new();
    for (category, power) in placed_powers(state) {
        let key = (category, power.internal_name.clone());
        for slot in user_band(power) {
            if recorded.contains(&(key.clone(), slot)) {
                continue;
            }
            let Some(level) = levels
                .get(&key)
                .and_then(|power| power.get(slot))
                .copied()
                .flatten()
            else {
                continue;
            };
            missing.push(SlotOrderEntry {
                power_name: power.internal_name.clone(),
                slot_index: slot as u8,
                category: Some(category.as_str().to_string()),
                level: Some(level),
                level_source: Some(SlotLevelSource::Packed),
            });
        }
    }
    changed |= !missing.is_empty();
    state.slot_order.extend(missing);
    changed
}

/// Record a slot the user just placed at `slot_index`. `level` is the level
/// [`next_grant_level`] gave it in level-up mode — the author's own placement, so
/// [`SlotLevelSource::Authored`] — or `None` outside it, where only the click order is kept for
/// a later switch into level-up mode to fill in.
pub fn record_added_slot(
    state: &mut CharacterState,
    category: SlotCategory,
    power_name: &str,
    slot_index: usize,
    level: Option<u8>,
) {
    prune(state);
    state.slot_order.push(SlotOrderEntry {
        power_name: power_name.to_string(),
        slot_index: slot_index as u8,
        category: Some(category.as_str().to_string()),
        level,
        level_source: level.map(|_| SlotLevelSource::Authored),
    });
}

/// Forget the slot removed from `slot_index`, and shift the same power's later entries down one
/// so each still addresses the slot it did. Call after the slot itself is gone.
pub fn forget_removed_slot(
    state: &mut CharacterState,
    category: SlotCategory,
    power_name: &str,
    slot_index: usize,
) {
    let categories: Vec<Option<SlotCategory>> = state
        .slot_order
        .iter()
        .map(|entry| entry_category(state, entry))
        .collect();
    let mut index = 0;
    state.slot_order.retain_mut(|entry| {
        let same_power = categories[index] == Some(category) && entry.power_name == power_name;
        index += 1;
        if !same_power {
            return true;
        }
        let slot = usize::from(entry.slot_index);
        if slot == slot_index {
            return false;
        }
        if slot > slot_index {
            entry.slot_index -= 1;
        }
        true
    });
    prune(state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::character::PowersetSelection;

    fn schedule(grants: &[(u8, u32)]) -> LevelingSchedule {
        LevelingSchedule {
            dataset: "test".into(),
            slot_grants: grants.iter().copied().collect(),
            total_slots: grants.iter().map(|(_, n)| n).sum(),
            power_picks: [(1, 2)].into_iter().collect(),
            max_power_picks: 2,
            pool_unlock_level: 4,
            max_power_pools: 4,
            epic_pool_level: 35,
            auto_granted_slot_levels: BTreeMap::new(),
        }
    }

    fn power(name: &str, level: u8, slots: usize) -> SelectedPower {
        let mut power = SelectedPower::picked(name, "Primary.Set", level);
        power.slots = vec![None; slots];
        power
    }

    fn build(level: u8, powers: Vec<SelectedPower>) -> CharacterState {
        let mut state = CharacterState::empty(crate::DatasetId::Homecoming);
        state.level = level;
        state.primary = PowersetSelection {
            id: Some("Primary.Set".into()),
            name: "Set".into(),
            powers,
        };
        state
    }

    fn levels_of(levels: &SlotLevels, name: &str) -> Vec<Option<u8>> {
        levels[&(SlotCategory::Primary, name.to_string())].clone()
    }

    #[test]
    fn a_branch_pick_is_found_under_its_own_set() {
        // A VEAT branch pick lives in the primary list under the branch set's id, which no
        // bucket names. Every slot edit addresses it that way, so a lookup that only asks the
        // bucket's id finds nothing and the edit silently does nothing.
        let mut state = build(50, vec![power("Slash", 1, 1)]);
        state
            .primary
            .powers
            .push(SelectedPower::picked("FRT_Aim", "Branch.Set", 24));
        assert_eq!(
            SlotCategory::of(&state, "Branch.Set", "FRT_Aim"),
            Some(SlotCategory::Primary)
        );
        assert!(state.selected_power("Branch.Set", "FRT_Aim").is_some());
        assert!(state.selected_power_mut("Branch.Set", "FRT_Aim").is_some());
        // Named under the wrong set, it is still not found.
        assert_eq!(SlotCategory::of(&state, "Other.Set", "FRT_Aim"), None);
        assert!(state.selected_power("Other.Set", "FRT_Aim").is_none());
    }

    #[test]
    fn a_late_power_takes_a_grant_an_early_one_can_give_up() {
        // One grant at 3, one at 40. The early power's slot is solved first; a first-come walk
        // would give it the 3 and then the late power the 40, which is also what a matching
        // gives — but seed the early power at 40 and the late power must still get served.
        let schedule = schedule(&[(3, 1), (40, 1)]);
        let mut state = build(50, vec![power("Early", 1, 2), power("Late", 38, 2)]);
        state.slot_order.push(SlotOrderEntry {
            power_name: "Early".into(),
            slot_index: 1,
            category: Some("primary".into()),
            level: Some(40),
            level_source: Some(SlotLevelSource::Authored),
        });
        let levels = slot_levels(&schedule, &state);
        assert_eq!(levels_of(&levels, "Early"), vec![Some(1), Some(3)]);
        assert_eq!(levels_of(&levels, "Late"), vec![Some(38), Some(40)]);
    }

    #[test]
    fn a_slot_no_grant_can_serve_has_no_level() {
        let schedule = schedule(&[(3, 1)]);
        let state = build(50, vec![power("Late", 38, 2)]);
        assert_eq!(
            levels_of(&slot_levels(&schedule, &state), "Late"),
            vec![Some(38), None]
        );
        assert_eq!(next_grant_level(&schedule, &state, 38), None);
    }

    #[test]
    fn removing_a_slot_leaves_its_peers_where_they_were() {
        let schedule = schedule(&[(3, 1), (5, 1), (7, 1)]);
        let mut state = build(50, vec![power("Attack", 1, 1)]);
        for _ in 0..3 {
            let level = next_grant_level(&schedule, &state, 1);
            let index = state.primary.powers[0].slots.len();
            state.primary.powers[0].slots.push(None);
            record_added_slot(&mut state, SlotCategory::Primary, "Attack", index, level);
        }
        assert_eq!(
            levels_of(&slot_levels(&schedule, &state), "Attack"),
            vec![Some(1), Some(3), Some(5), Some(7)]
        );

        state.primary.powers[0].slots.remove(1);
        forget_removed_slot(&mut state, SlotCategory::Primary, "Attack", 1);
        assert_eq!(
            levels_of(&slot_levels(&schedule, &state), "Attack"),
            vec![Some(1), Some(5), Some(7)]
        );
        // The freed 3 is what the next placement draws.
        assert_eq!(next_grant_level(&schedule, &state, 1), Some(3));
    }

    #[test]
    fn freezing_writes_every_slot_once_and_then_changes_nothing() {
        let schedule = schedule(&[(3, 2), (5, 1)]);
        let mut state = build(50, vec![power("A", 1, 2), power("B", 2, 3)]);
        assert!(freeze(&schedule, &mut state));
        assert_eq!(state.slot_order.len(), 3);
        assert!(state
            .slot_order
            .iter()
            .all(|entry| entry.level_source == Some(SlotLevelSource::Packed)));
        let before = slot_levels(&schedule, &state);
        assert!(!freeze(&schedule, &mut state));
        assert_eq!(slot_levels(&schedule, &state), before);
    }

    #[test]
    fn a_stored_level_the_schedule_never_issues_is_cleared() {
        let schedule = schedule(&[(3, 1)]);
        let mut state = build(50, vec![power("A", 1, 2)]);
        state.slot_order.push(SlotOrderEntry {
            power_name: "A".into(),
            slot_index: 1,
            category: None,
            level: Some(38),
            level_source: None,
        });
        freeze(&schedule, &mut state);
        assert_eq!(state.slot_order[0].level, Some(3));
    }

    #[test]
    fn only_grants_up_to_the_build_level_are_spent() {
        let schedule = schedule(&[(3, 1), (40, 1)]);
        let state = build(20, vec![power("A", 1, 3)]);
        assert_eq!(
            levels_of(&slot_levels(&schedule, &state), "A"),
            vec![Some(1), Some(3), None]
        );
    }
}
