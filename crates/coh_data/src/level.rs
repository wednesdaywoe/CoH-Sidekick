//! Character, enhancement and exemplar levels.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// A level: a positive integer.
///
/// `0` is not a level. It is the wire format's *second* spelling of "unset",
/// inherited from the beta's `slot.level || globalIOLevel` idiom where a falsy
/// `0` and an absent field mean the same thing. Carried into an `Option<u8>`
/// that is the two encodings of absence, and each read site has to remember to
/// decode both — a read site that trusts the `Option` and forgets the `0`
/// treats "unset" as a real level `0`, which reads at the curve floor and
/// skips exemplar scaling instead of falling back to the build level.
///
/// Rejecting `0` at construction leaves `Option<Level>` one spelling of absent,
/// so no read site can be written that way. The wire keeps its legacy encoding:
/// [`deserialize_optional`] folds an incoming `0` to `None`.
///
/// No upper bound is enforced. The level ceiling belongs to the dataset's
/// leveling schedule, not to this type (Rule 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Level(u8);

impl Level {
    /// The level, or `None` for the `0` sentinel.
    pub const fn new(value: u8) -> Option<Level> {
        match value {
            0 => None,
            level => Some(Level(level)),
        }
    }

    /// A level known at compile time. The `0` check runs during const
    /// evaluation, so a bad constant fails the build rather than a run.
    pub const fn constant(value: u8) -> Level {
        assert!(value != 0, "0 is the unset sentinel, not a level");
        Level(value)
    }

    /// The level, or `None` when the value is the `0` sentinel or too large to
    /// be one.
    pub fn from_i64(value: i64) -> Option<Level> {
        u8::try_from(value).ok().and_then(Level::new)
    }

    /// The level, or `None` when the value is not one: fractional, infinite,
    /// `NaN`, the `0` sentinel, or out of range. Levels are integers in-game,
    /// so a fractional one is a caller bug and must not silently truncate.
    pub fn from_f64(value: f64) -> Option<Level> {
        if !value.is_finite() || value.fract() != 0.0 {
            return None;
        }
        Level::from_i64(value as i64)
    }

    /// The underlying number.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl From<Level> for u8 {
    fn from(level: Level) -> u8 {
        level.0
    }
}

impl From<Level> for i64 {
    fn from(level: Level) -> i64 {
        i64::from(level.0)
    }
}

impl From<Level> for f64 {
    fn from(level: Level) -> f64 {
        f64::from(level.0)
    }
}

impl From<Level> for usize {
    fn from(level: Level) -> usize {
        usize::from(level.0)
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Level, D::Error> {
        let value = u8::deserialize(deserializer)?;
        Level::new(value).ok_or_else(|| {
            serde::de::Error::custom("0 is the unset sentinel, not a level; use Option<Level>")
        })
    }
}

/// Deserialize a level that may be absent, where the wire spells "absent" as
/// either `null`/omitted or the legacy `0`. Both land on `None`, so the
/// sentinel dies at the boundary instead of at every read site.
pub fn deserialize_optional<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Level>, D::Error> {
    Ok(Option::<u8>::deserialize(deserializer)?.and_then(Level::new))
}
