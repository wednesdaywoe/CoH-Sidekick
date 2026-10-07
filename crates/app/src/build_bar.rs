//! Build bar — the sticky <900px bar (beta `MobileBuildBar`) that keeps the level and the
//! budgets in reach while the mobile stack scrolls.
//!
//! Nothing here computes what the app doesn't already compute: the level is the header's own
//! [`LevelControl`] (level is one build input with one writer), and the slot counter reads
//! the same budget memo the Powers panel's slot counter reads ([`BuildBudget`]). The pick
//! counter is the beta's `useBuildBudget` arithmetic — picks granted by the level
//! (`total_power_picks_at_level`) against picks made (`picked_powers`, which skips the
//! locked auto-grants: the beta's `isAutoGranted` exclusion).

use crate::build_session::BuildSession;
use crate::level_control::{LevelControl, LevelUpControl, LevelUpMode, MIN_LEVEL};
use crate::panels::powers::BuildBudget;
use crate::shell::{Db, UndoRedo};
use dioxus::prelude::*;

#[component]
pub fn BuildBar(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let slot_budget = use_context::<BuildBudget>().0;
    let level_up_mode = use_context::<LevelUpMode>().0;

    // Everything is read before the markup, so the borrow of the build ends here. A dataset
    // without a schedule has no budget to compare against — the same fail-loud branch the
    // level control and the slot budget take: a dash, not an invented 0/0.
    let (picks, pick_budget, slots, next_pick) = {
        let build = session.build.read();
        let schedule = database
            .as_ref()
            .and_then(|database| database.leveling_schedule.as_ref());
        let pick_budget = schedule.map(|schedule| schedule.total_power_picks_at_level(build.level));
        // The header's next-pick readout, by the same rule (see `LevelUpControl`).
        let taken_levels: Vec<u8> = build.picked_powers().map(|power| power.level).collect();
        let next_pick =
            schedule.and_then(|schedule| schedule.next_pick_level(&taken_levels, MIN_LEVEL));
        (
            build.picked_powers().count(),
            pick_budget,
            slot_budget(),
            next_pick,
        )
    };

    rsx! {
        div { class: "build-bar",
            // Level Up and Undo/Redo ride here because the header they live in is not drawn
            // below 900px: the bottom nav replaced it, and these three have no tab to go to.
            LevelControl { database: database.clone() }
            LevelUpControl { database }
            UndoRedo { session }
            if let Some(budget) = pick_budget {
                BudgetCounter {
                    label: "Pwr",
                    used: picks,
                    available: budget,
                    low_at: 3,
                    title: "Power picks made",
                }
                // A phone has no hover text to read a pick's level from, so the bar says it.
                // Gone once every pick is made (the Pwr count already says so) and while Level
                // Up mode is on, as in the header: the mode's cluster names the level it owes.
                if let Some(level) = next_pick.filter(|_| !level_up_mode()) {
                    div {
                        class: "build-bar__budget",
                        title: "Your next power pick fills the level {level} slot",
                        span { class: "build-bar__label", "Next" }
                        span { class: "build-bar__value mono", "Lvl {level}" }
                    }
                }
                BudgetCounter {
                    label: "Slot",
                    used: slots.used,
                    available: slots.available,
                    low_at: 5,
                    title: "Enhancement slots placed",
                }
            } else {
                DashCounter { label: "Pwr" }
                DashCounter { label: "Slot" }
            }
        }
    }
}

/// One of the bar's two counters. The colour is the beta's threshold: over the budget is
/// danger, and the last few units are the warning heat — a budget about to run out must not
/// read the same as one barely started.
#[component]
fn BudgetCounter(
    label: &'static str,
    used: usize,
    available: usize,
    low_at: usize,
    title: &'static str,
) -> Element {
    let over = used > available;
    let low = !over && available.saturating_sub(used) <= low_at;
    rsx! {
        div {
            class: if over {
                "build-bar__budget is-over"
            } else if low {
                "build-bar__budget is-low"
            } else {
                "build-bar__budget"
            },
            title,
            span { class: "build-bar__label", "{label}" }
            span { class: "build-bar__value mono", "{used} / {available}" }
        }
    }
}

/// The no-schedule stand-in: the level control beside it shows the same dash for the same
/// reason, so the bar reads as one control family that has nothing to say.
#[component]
fn DashCounter(label: &'static str) -> Element {
    rsx! {
        div { class: "build-bar__budget is-unavailable",
            span { class: "build-bar__label", "{label}" }
            span { class: "build-bar__value mono", "—" }
        }
    }
}
