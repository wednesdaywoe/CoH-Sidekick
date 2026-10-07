//! The enhancement picker's global slotting defaults — the crafting level,
//! attunement, and catalyst booster the picker stamps onto a piece at pick time
//! (the beta's `useUIStore` `globalIOLevel` / `attunementEnabled` /
//! `globalBoostLevel`). Held above the picker so they persist across opens: the
//! beta deliberately keeps the IO level between picks rather than resetting it to
//! the character level. Provided once by the shell; the picker reads and edits them.

use coh_data::Level;
use dioxus::prelude::*;

/// The relative-level offsets worth offering for one dataset, read off its own
/// `boost_effectiveness` curves.
///
/// `coh_math::enhancement::enhancement_level_multiplier` indexes `above` by the offset and
/// `below` by its negation, then clamps to the curve's end — so an offset past the point where
/// the curve stops moving is indistinguishable from the last one that did. The band is
/// therefore the range over which each curve actually varies, which is what the fork is saying
/// about the mechanic: Homecoming attenuates to −3 and rewards to +3, Rebirth runs −9 to +4,
/// and Thunderspy ships both curves flat at 1.0 — relative level does nothing there, and the
/// band collapses to even rather than offering 99 steps that all mean the same thing.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct RelativeLevelBand {
    pub min: i8,
    pub max: i8,
}

impl RelativeLevelBand {
    /// The band a dataset's curves define. `None` when the dataset ships no curves at all —
    /// the caller then has no domain to offer and shows no control.
    pub fn from_curves(curves: Option<&coh_data::EnhancementCurves>) -> Option<Self> {
        let effectiveness = &curves?.boost_effectiveness;
        // Offset 0 is the curve's own even-level entry; the reach is the last offset that
        // still differs from it.
        let reach = |curve: &[f64]| {
            let Some((even, rest)) = curve.split_first() else {
                return 0;
            };
            let last_varying = rest.iter().rposition(|value| value != even);
            last_varying.map_or(0, |i| i8::try_from(i + 1).unwrap_or(i8::MAX))
        };
        Some(RelativeLevelBand {
            min: -reach(&effectiveness.below),
            max: reach(&effectiveness.above),
        })
    }

    /// No offset in either direction changes anything — the control has nothing to step through.
    pub fn is_even_only(&self) -> bool {
        self.min == 0 && self.max == 0
    }
}

/// The levels an IO is crafted at, as the dataset's boost index names them — nine per stat,
/// `Crafted_Accuracy_10` through `Crafted_Accuracy_50`, in steps of five.
///
/// It replaces a typed `IO_LEVEL_MIN`/`IO_LEVEL_MAX` of 10..=53 and the separate
/// `IO_CRAFT_CEILING` of 50 that had to sit beside it, because 51-53 were never recipes: that
/// is the pre-booster spelling of a level-50 IO with three combines, which the picker already
/// carries on its own axis as the Boost dial, and reading it as a LEVEL paid a bigger number on
/// Homecoming's 105-entry class tables while paying nothing on the forks, whose tables stop at
/// 50. With the band read from the export the two constants collapse into one: the top of the
/// roster IS "as high as this piece goes" (BOOST-6).
#[derive(Clone, PartialEq, Eq)]
pub struct CraftBand {
    levels: Vec<Level>,
}

impl CraftBand {
    /// The band a dataset's boost index states. `None` when the database carries no index at
    /// all — the caller then has no domain to offer, the way [`RelativeLevelBand`] treats a
    /// dataset with no curves.
    pub fn from_boost_index(index: Option<&coh_data::BoostIndex>) -> Option<Self> {
        let levels = index?.craft_levels();
        (!levels.is_empty()).then(|| CraftBand {
            levels: levels.to_vec(),
        })
    }

    /// The lowest level anything is crafted at.
    pub fn min(&self) -> u8 {
        self.levels.first().expect("non-empty band").get()
    }

    /// The highest — the craft ceiling, which is what "as high as this piece goes" means.
    pub fn max(&self) -> u8 {
        self.levels.last().expect("non-empty band").get()
    }

    /// The roster level at or below `level`, and the floor for anything under it. A generic IO
    /// exists only at these levels, so a level carried in from the set tab — where every
    /// integer inside a set's own range is a real recipe — crafts at the step below it.
    pub fn crafted_at_or_below(&self, level: u8) -> u8 {
        let mut crafted = self.min();
        for candidate in &self.levels {
            if candidate.get() > level {
                break;
            }
            crafted = candidate.get();
        }
        crafted
    }

    /// One roster entry away from `from`, in the direction of `delta`. Stops at both ends.
    pub fn stepped(&self, from: u8, delta: i8) -> u8 {
        let at = self.crafted_at_or_below(from);
        let index = self
            .levels
            .iter()
            .position(|level| level.get() == at)
            .unwrap_or(0);
        let next = index
            .saturating_add_signed(delta as isize)
            .min(self.levels.len() - 1);
        self.levels[next].get()
    }
}
/// Catalyst boosters stack up to +5…
pub const BOOST_MAX: u8 = 5;
/// …and combine only into a level-50+ IO. Defined beside its enforcing reader
/// ([`coh_data::Enhancement::takes_booster`], upstream of the UI); re-exported here so the
/// picker's named game constants stay one family.
pub use coh_data::BOOSTER_LEVEL_FLOOR;

/// Shared picker defaults. `Copy` (every field is a `Copy` `Signal`), so panels
/// pull it from context and pass it by value.
#[derive(Clone, Copy, PartialEq)]
pub struct PickerDefaults {
    /// Crafting level for non-attuned IOs (clamped to the dataset's [`CraftBand`]).
    pub io_level: Signal<u8>,
    /// Attuned mode — set pieces scale with character level instead of a fixed craft level.
    pub attuned: Signal<bool>,
    /// Catalyst booster level (`0..=BOOST_MAX`).
    pub boost: Signal<u8>,
    /// Relative level for origin and special enhancements — the piece's level minus the
    /// character's, signed. A DIFFERENT game mechanic from the booster combine above
    /// (`coh_math::enhancement::EnhancementLevelAxis`), read from different curves, so it
    /// gets its own stored value: switching picker tabs must not rewrite the other axis.
    pub relative_level: Signal<i8>,
}

impl PickerDefaults {
    /// A fresh set of defaults: level 50 (the planner's design-at-cap default),
    /// unattuned, no booster, even relative level.
    pub fn new() -> Self {
        PickerDefaults {
            io_level: Signal::new(50),
            attuned: Signal::new(false),
            boost: Signal::new(0),
            relative_level: Signal::new(0),
        }
    }

    /// Nudge the crafting level by `delta`, clamped to the band the dataset's boost index
    /// states. `steps` walks the roster entry by entry instead of by one level — what the
    /// generic tab needs, where the only levels that exist are the roster's.
    pub fn adjust_io_level(mut self, delta: i8, band: &CraftBand, steps: bool) {
        let now = *self.io_level.peek();
        let next = if steps {
            band.stepped(now, delta)
        } else {
            now.saturating_add_signed(delta)
                .clamp(band.min(), band.max())
        };
        self.io_level.set(next);
    }

    /// Nudge the booster level by `delta`, clamped to `0..=BOOST_MAX`.
    pub fn adjust_boost(mut self, delta: i8) {
        let next = (self.boost.peek().saturating_add_signed(delta)).min(BOOST_MAX);
        self.boost.set(next);
    }

    /// Nudge the relative level by `delta`, clamped to the band the dataset's own
    /// `above`/`below` curves define. The two directions are separate curves of
    /// independent reach — Rebirth attenuates to −9 where it rewards only to +4 — so
    /// the band is asked for, never assumed symmetric.
    pub fn adjust_relative_level(mut self, delta: i8, band: RelativeLevelBand) {
        let next = self
            .relative_level
            .peek()
            .saturating_add(delta)
            .clamp(band.min, band.max);
        self.relative_level.set(next);
    }

    /// The relative level to stamp onto an origin or special piece.
    pub fn relative_level(&self) -> i8 {
        *self.relative_level.peek()
    }

    /// The slotting bag to stamp onto a set piece (the beta's `{ attuned, level, boost }`).
    pub fn slotting(&self) -> coh_data::IoSlotting {
        coh_data::IoSlotting {
            attuned: *self.attuned.peek(),
            io_level: *self.io_level.peek(),
            boost: *self.boost.peek(),
        }
    }
}

impl Default for PickerDefaults {
    fn default() -> Self {
        Self::new()
    }
}
