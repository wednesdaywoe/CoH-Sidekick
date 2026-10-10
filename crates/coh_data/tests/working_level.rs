//! The guided planner's working level is the earliest pick slot the build has not filled (GP3).
//!
//! The app shows it beside the character's level as `50 (14)`, and it is
//! `next_pick_level(taken, 1)`. Graded on Homecoming's own pick schedule, where level 1 grants
//! two picks: the case most likely to be miscounted.

use coh_data::{DatasetId, LevelingSchedule, PowerDatabase};
use std::path::PathBuf;

fn homecoming_schedule() -> LevelingSchedule {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(DatasetId::Homecoming.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes)
        .unwrap_or_else(|e| panic!("load homecoming: {e}"))
        .leveling_schedule
        .expect("homecoming ships a leveling schedule")
}

fn working(schedule: &LevelingSchedule, taken: &[u8]) -> Option<u8> {
    schedule.next_pick_level(taken, 1)
}

#[test]
fn a_new_build_works_at_level_one() {
    assert_eq!(working(&homecoming_schedule(), &[]), Some(1));
}

#[test]
fn level_one_holds_two_picks() {
    let schedule = homecoming_schedule();
    assert_eq!(working(&schedule, &[1]), Some(1));
    assert_eq!(working(&schedule, &[1, 1]), Some(2));
}

/// A level 32 power as the third pick fills the 32 slot and leaves the working level at 2.
#[test]
fn an_out_of_level_pick_leaves_the_gap_behind_it() {
    assert_eq!(working(&homecoming_schedule(), &[1, 1, 32]), Some(2));
}

#[test]
fn a_full_build_has_no_working_level() {
    let schedule = homecoming_schedule();
    assert_eq!(working(&schedule, &schedule.pick_levels()), None);
}
