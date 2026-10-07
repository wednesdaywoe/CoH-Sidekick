//! Enhancement curves — the per-dataset curve data the enhancement engine
//! consumes (ED thresholds + tier effectiveness, boost-type→schedule
//! assignment, per-boost-level strength curves, multi-aspect scale ladder,
//! relative-level/boost combine curves). SOURCE-1 SW6: the typed view of the
//! contract's `enhancement-curves` section, which `emit-contract.cjs` sources
//! from the generated per-dataset module (itself staleness-guarded against
//! the binary export both directions), so this chain is
//! export == module == contract == this struct.
//!
//! Data/calc split (D2): this struct is pure DATA. The lookups (`apply_ed`,
//! `io_value_at_level`, `schedule_for_aspect`, …) live in
//! `coh_math::enhancement` and take these curves as input.

use crate::Level;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// An ED schedule — dim_returns keys each threshold set by boost type, and the
/// letters are the project-wide names for the four tier-start triples.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Schedule {
    A,
    B,
    C,
    D,
}

impl Schedule {
    pub fn from_wire(s: &str) -> Option<Schedule> {
        match s {
            "A" => Some(Schedule::A),
            "B" => Some(Schedule::B),
            "C" => Some(Schedule::C),
            "D" => Some(Schedule::D),
            _ => None,
        }
    }

    pub fn as_wire(self) -> &'static str {
        match self {
            Schedule::A => "A",
            Schedule::B => "B",
            Schedule::C => "C",
            Schedule::D => "D",
        }
    }
}

/// One schedule's curve data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScheduleCurve {
    /// ED tier boundaries (t1, t2, t3) as pre-ED enhancement fractions.
    pub ed_thresholds: [f64; 3],
    /// Named class-modifier table the strength curve was read from.
    pub source_table: String,
    /// Enhancement strength of one boost: `strength_by_boost_level[boost_level - 1]`.
    pub strength_by_boost_level: Vec<f64>,
}

/// The four schedules' curves, keyed by letter on the wire.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleCurves {
    #[serde(rename = "A")]
    pub a: ScheduleCurve,
    #[serde(rename = "B")]
    pub b: ScheduleCurve,
    #[serde(rename = "C")]
    pub c: ScheduleCurve,
    #[serde(rename = "D")]
    pub d: ScheduleCurve,
}

impl ScheduleCurves {
    pub fn get(&self, schedule: Schedule) -> &ScheduleCurve {
        match schedule {
            Schedule::A => &self.a,
            Schedule::B => &self.b,
            Schedule::C => &self.c,
            Schedule::D => &self.d,
        }
    }
}

/// An origin tier's TO/DO/SO enhancement fraction per ED schedule (SW8). The
/// origin boost families sit on flat Ones class tables, so a tier's value
/// depends only on the aspect's schedule.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TierScales {
    #[serde(rename = "A")]
    pub a: f64,
    #[serde(rename = "B")]
    pub b: f64,
    #[serde(rename = "C")]
    pub c: f64,
    #[serde(rename = "D")]
    pub d: f64,
}

impl TierScales {
    pub fn get(&self, schedule: Schedule) -> f64 {
        match schedule {
            Schedule::A => self.a,
            Schedule::B => self.b,
            Schedule::C => self.c,
            Schedule::D => self.d,
        }
    }
}

/// The three origin tiers, keyed TO/DO/SO on the wire (`do` is a Rust
/// keyword, hence the spelled-out field names).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginTiers {
    #[serde(rename = "TO")]
    pub training: TierScales,
    #[serde(rename = "DO")]
    pub dual: TierScales,
    #[serde(rename = "SO")]
    pub single: TierScales,
}

/// Relative-level attenuation (above/below) and +boost combine curves.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BoostEffectiveness {
    pub above: Vec<f64>,
    pub below: Vec<f64>,
    pub boosters: Vec<f64>,
}

/// Exemplar magnitude-handicap curves (exemplar_handicaps.bin), applied in
/// boost.c `boost_HandicapExemplar` order: clamp to `pre_clamp`, scale by
/// `weights[combat]/weights[io]` when the magnitude reaches `limits`, then
/// clamp to `post_clamp`. Indexed by 1-based level - 1, top-clamped to each
/// curve's length.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExemplarHandicaps {
    pub limits: Vec<f64>,
    pub weights: Vec<f64>,
    pub pre_clamp: Vec<f64>,
    pub post_clamp: Vec<f64>,
}

/// One dataset's enhancement-curve data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnhancementCurves {
    /// Dataset id stamped by the converter; checked against the bundle's
    /// manifest at load so a cross-wired section fails loud.
    pub dataset: String,
    pub schedules: ScheduleCurves,
    /// ED effectiveness of the value beyond each threshold (t1-t2, t2-t3, past t3).
    pub tier_effectiveness: [f64; 3],
    /// dim_returns boost-type vocabulary → its ED schedule.
    pub boost_type_schedules: BTreeMap<String, Schedule>,
    /// Schedule for every boost type not listed above (the dim_returns default entry).
    pub default_schedule: Schedule,
    /// Standard crafted-piece per-aspect scale, indexed by aspect count - 1 (1..4 aspects).
    pub multi_aspect_scale: [f64; 4],
    /// TO/DO/SO enhancement fraction per ED schedule.
    pub origin_tiers: OriginTiers,
    pub boost_effectiveness: BoostEffectiveness,
    pub exemplar_handicaps: ExemplarHandicaps,
    /// The levels an IO is crafted at, ascending — joined from the bundle's
    /// `boost-index` section at load, not read from this one.
    ///
    /// The curves say what a level PAYS; only the boost roster says which levels
    /// exist, and the difference is not academic: Homecoming's class tables run
    /// 105 entries deep, so a read at 51-53 paid a level the game ships no
    /// recipe for, while the forks' tables stop at 50 and paid nothing for the
    /// same three (BOOST-6). It lives here because every reader of a curve
    /// already holds the curves and nothing else tells it where to stop.
    #[serde(skip)]
    pub craft_levels: Vec<Level>,
}

impl EnhancementCurves {
    /// Parse the contract's `enhancement-curves` section. Malformed ≠ absent
    /// (matching the sibling section readers): an ABSENT
    /// section yields `None` (a hand-constructed `PowerDatabase` carries no
    /// curves, and consumers must surface that as an error, not a default),
    /// but a PRESENT section that doesn't parse — or whose curves are
    /// structurally vacuous — is an error.
    pub fn from_section(
        section: Option<&Value>,
        craft_levels: &[Level],
    ) -> Result<Option<Self>, String> {
        let Some(section) = section else {
            return Ok(None);
        };
        let mut curves: EnhancementCurves = serde_json::from_value(section.clone())
            .map_err(|e| format!("enhancement-curves section: {e}"))?;
        // A present curves section with no band is a bundle missing its
        // boost-index section, not a dataset whose IOs have no levels: reading
        // the curve with nothing to clamp to is what this join exists to stop.
        if craft_levels.is_empty() {
            return Err(
                "enhancement-curves section: no crafted IO levels — the boost-index section \
                 states the band these curves may be read at, and it named none"
                    .into(),
            );
        }
        curves.craft_levels = craft_levels.to_vec();
        for schedule in [Schedule::A, Schedule::B, Schedule::C, Schedule::D] {
            if curves
                .schedules
                .get(schedule)
                .strength_by_boost_level
                .is_empty()
            {
                return Err(format!(
                    "enhancement-curves section: schedule {} has an empty strength curve",
                    schedule.as_wire()
                ));
            }
        }
        if curves.boost_effectiveness.boosters.is_empty() {
            return Err("enhancement-curves section: empty boosters curve".into());
        }
        // boost.c skips exemplar scaling entirely on empty Limits/Weights —
        // that would be a real data change, so it fails loud here.
        if curves.exemplar_handicaps.limits.is_empty()
            || curves.exemplar_handicaps.weights.is_empty()
        {
            return Err("enhancement-curves section: empty exemplar limits/weights curve".into());
        }
        Ok(Some(curves))
    }
}
