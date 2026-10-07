//! Enhancement values, the twin of `src/utils/calculations/enhancement-values.ts`.
//! Two layers: the per-curve PRIMITIVES (ED, per-level strength, schedule
//! assignment, exemplar handicap, rarity) and the per-power slot AGGREGATION
//! ([`calculate_power_enhancement_bonuses`], the twin of the beta
//! `calculatePowerEnhancementBonuses`) that composes them over a power's
//! slotted enhancements into per-aspect [`EnhancementBonuses`]. The
//! aggregation runs sum-then-one-ED (accumulate every slot's raw per-aspect
//! contribution, then run ED once per aspect), pinned exact-f64 by the
//! enhancement gate's `calculatePowerEnhancementBonuses` sweep and end-to-end by
//! the slotted totals-gate builds.
//!
//! Alpha's ED-bypass split ([`combine_with_alpha_ed`] / [`filter_alpha_by_allowed_enhancements`],
//! the beta `combineWithAlphaED` / `filterAlphaByAllowedEnhancements`) layers on top: it
//! splits the Alpha incarnate's per-aspect buff into an ED-subject slice folded into the
//! raw IO sum before the single ED pass and an ED-bypass slice added after, gated by the
//! power's `allowed_enhancements` (INCARNATE-1). [`crate::apply`] picks it over the plain
//! aggregation whenever an active Alpha is equipped.
//!
//! SOURCE-1 SW6: the curve-reading lookups take the dataset's
//! [`EnhancementCurves`] (the contract's `enhancement-curves` section, loaded
//! by `coh_data`) as input. There are no local curve constants. A caller with
//! no curves has no enhancement math: surface the missing data as an error,
//! never a default. Since WS17 the exemplar handicap rides the same chain
//! (`exemplar_handicaps` in the curves section); the rarity tier -> multiplier
//! map is the last hand table, keyed on the binary tier rather than a name.

use crate::totals::CalcError;
use coh_data::enhancement_curves::EnhancementCurves;
pub use coh_data::enhancement_curves::Schedule;
use coh_data::io_sets::IoSetCatalog;
use coh_data::{Enhancement, EnhancementKind, Level};
use std::collections::BTreeMap;

/// Multiplier an enhancement's values gain at +`boost_level` combine boosts, read
/// off the `boosters` combine curve. `None` for a boost level outside the curve:
/// it doesn't exist in-game, so it must surface, not clamp.
pub fn boost_multiplier(boost_level: i64, curves: &EnhancementCurves) -> Option<f64> {
    let boosters = &curves.boost_effectiveness.boosters;
    usize::try_from(boost_level)
        .ok()
        .and_then(|i| boosters.get(i))
        .copied()
}

/// The two axes an enhancement's stored level offset can sit on.
///
/// These are DIFFERENT game mechanics read from DIFFERENT bins
/// (`boost_effect_boosters.bin` vs `boost_effect_above.bin` /
/// `boost_effect_below.bin`), and no enhancement has both:
///
///  - `Booster`: Enhancement Booster combines, IOs only, unsigned.
///  - `Relative`: the enhancement's level minus the character's combat level,
///    what an SO/DO/TO or a Hamidon-class special carries. Signed.
///
/// They were collapsed into one unsigned field on both sides of the twin
/// because on Homecoming the two curves are numerically IDENTICAL over 0..+3
/// (`[1, 1.05, 1.10, 1.15]` either way), the entire range that was
/// representable. They diverge only below even, and `below` falls at -10% per
/// level, not the -5% a symmetric reading of the booster curve would suggest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnhancementLevelAxis {
    Booster,
    Relative,
}

/// Which axis `kind`'s stored level offset sits on.
pub fn enhancement_level_axis(kind: &EnhancementKind) -> EnhancementLevelAxis {
    match kind {
        EnhancementKind::Special { .. } | EnhancementKind::Origin { .. } => {
            EnhancementLevelAxis::Relative
        }
        EnhancementKind::IoSet { .. } | EnhancementKind::GenericIo { .. } => {
            EnhancementLevelAxis::Booster
        }
    }
}

/// Effectiveness multiplier for a slot's stored level offset, off the dataset's
/// own curves. The twin of the beta `enhancementLevelMultiplier`.
///
/// Clamps to the end of the curve, a deliberate divergence from
/// [`boost_multiplier`]'s fail-loud contract: a Mids file can legitimately carry
/// `PlusFive` on an SO (past `above`'s four entries), and an older save can
/// carry anything, so an out-of-domain offset is ordinary input, not a data
/// defect. Dropping the slot would silently understate the build, which is the
/// failure mode this whole change exists to remove.
///
/// The datasets disagree about the domain: Homecoming's `below` stops at -3
/// (x0.70), Rebirth's runs to -9 (x0.10), and Thunderspy ships both curves flat
/// at 1.0, attenuation switched off entirely. Reading the curve keeps a
/// hardcoded rule from inventing a penalty Thunderspy doesn't apply.
pub fn enhancement_level_multiplier(slot: &Enhancement, curves: &EnhancementCurves) -> f64 {
    let offset = i64::from(slot.boost);
    let effectiveness = &curves.boost_effectiveness;
    // The booster axis is unsigned: there's no such thing as a negative combine,
    // so a negative there is a corrupt value, not a penalty to honour.
    let (curve, step) = match enhancement_level_axis(&slot.kind) {
        EnhancementLevelAxis::Booster => (&effectiveness.boosters, offset.max(0)),
        EnhancementLevelAxis::Relative if offset < 0 => (&effectiveness.below, -offset),
        EnhancementLevelAxis::Relative => (&effectiveness.above, offset),
    };
    if curve.is_empty() {
        return 1.0;
    }
    let index = usize::try_from(step).unwrap_or(0).min(curve.len() - 1);
    curve[index]
}

/// Enhancement Diversification: four diminishing-return tiers. Full value to t1,
/// then each slice beyond a threshold counts at that tier's effectiveness
/// (90% / 70% / 15% on all three committed datasets).
pub fn apply_ed(value: f64, schedule: Schedule, curves: &EnhancementCurves) -> f64 {
    let [t1, t2, t3] = curves.schedules.get(schedule).ed_thresholds;
    let [tier2_effectiveness, tier3_effectiveness, tier4_effectiveness] = curves.tier_effectiveness;
    if value <= t1 {
        value
    } else if value <= t2 {
        t1 + (value - t1) * tier2_effectiveness
    } else if value <= t3 {
        let tier2 = t1 + (t2 - t1) * tier2_effectiveness;
        tier2 + (value - t2) * tier3_effectiveness
    } else {
        let tier2 = t1 + (t2 - t1) * tier2_effectiveness;
        let tier3 = tier2 + (t3 - t2) * tier3_effectiveness;
        tier3 + (value - t3) * tier4_effectiveness
    }
}

/// `curve[level - 1]`, top-clamped past the last entry: boost.c indexes the
/// handicap curves by combat level with a size-1 clamp. [`Level`] guarantees the
/// floor; callers guarantee a non-empty curve.
fn handicap_curve_at(curve: &[f64], level: Level) -> f64 {
    let index = usize::from(level).min(curve.len()) - 1;
    curve[index]
}

/// The two levels an exemplar handicap reads, as a named pair.
///
/// A pair rather than two parameters because they share a type: passed
/// positionally, swapping them still compiles and fails silently. The swap
/// satisfies the `exemplar_level >= io_level` early-out and yields the magnitude
/// unscaled, wrong by exactly the handicap, in exactly the case (exemplared
/// below the IO level) the function exists to handle. Field names make that
/// mix-up something you have to write on purpose.
pub struct ExemplarLevels {
    /// The level the enhancement itself is read at.
    pub io_level: Level,
    /// The level the character is exemplared down to.
    pub exemplar_level: Level,
}

/// Exemplar magnitude handicap, a faithful boost.c `boost_HandicapExemplar`
/// over the dataset's exemplar_handicaps.bin curves, in the game's order:
/// clamp to `pre_clamp[exemplar]` (0.4167 through level 45), scale by
/// `weights[exemplar]/weights[io_level]` when the magnitude reaches
/// `limits[exemplar]` (the minor-bonus floor: 5%/10%/20% by level band), then
/// clamp to `post_clamp[exemplar]` (1.0 at every level). Empty pre/post-clamp
/// curves skip their clamp, as in boost.c. Procs never scale; no scaling at or
/// above the IO level.
///
/// Total, because levels are integers in-game and [`Level`] is the only way to
/// spell one: a fractional level is refused where it's parsed, so there's no
/// out-of-domain input left to reject.
pub fn apply_exemplar_scaling(
    raw_value: f64,
    levels: ExemplarLevels,
    is_proc: bool,
    curves: &EnhancementCurves,
) -> f64 {
    let ExemplarLevels {
        io_level,
        exemplar_level,
    } = levels;
    if is_proc {
        return raw_value;
    }
    if exemplar_level >= io_level {
        return raw_value;
    }
    let handicaps = &curves.exemplar_handicaps;
    let mut magnitude = raw_value;
    if !handicaps.pre_clamp.is_empty() {
        magnitude = magnitude.min(handicap_curve_at(&handicaps.pre_clamp, exemplar_level));
    }
    // limits/weights are guaranteed non-empty by the contract loader.
    if magnitude >= handicap_curve_at(&handicaps.limits, exemplar_level) {
        magnitude *= handicap_curve_at(&handicaps.weights, exemplar_level)
            / handicap_curve_at(&handicaps.weights, io_level);
    }
    if !handicaps.post_clamp.is_empty() {
        magnitude = magnitude.min(handicap_curve_at(&handicaps.post_clamp, exemplar_level));
    }
    magnitude
}

/// Enhancement strength of one boost at `level` for a schedule, read straight
/// off the dataset's per-level strength curve. Levels are integers in-game;
/// `None` for a fractional level (a caller bug that must surface).
///
/// The band is the crafted-record roster the dataset's boost index names
/// ([`EnhancementCurves::craft_levels`]), not the curve: it used to be a typed
/// 10..=53, which on Homecoming read three levels off a 105-entry class table
/// that no recipe produces, and read nothing on the forks, whose tables stop at
/// 50 (BOOST-6). A level past the roster clamps rather than failing — a stored
/// build from before the band was read off the export is ordinary input.
pub fn io_value_at_level(
    level: f64,
    schedule: Schedule,
    curves: &EnhancementCurves,
) -> Option<f64> {
    if level.fract() != 0.0 || !level.is_finite() {
        return None;
    }
    let curve = &curves.schedules.get(schedule).strength_by_boost_level;
    let min_level = i64::from(curves.craft_levels.first()?.get());
    let max_level = i64::from(curves.craft_levels.last()?.get()).min(curve.len() as i64);
    // The band and the curve are loaded from two different sections and nothing joins them:
    // the loader refuses an EMPTY strength curve, but a curve shorter than the first crafted
    // level leaves `min_level > max_level`, which `clamp` panics on rather than reporting —
    // a dead tab on the wasm build, where a panic aborts. `None` is
    // what the caller already surfaces for a curve it cannot read.
    if max_level < min_level {
        return None;
    }
    let clamped = (level as i64).clamp(min_level, max_level);
    curve.get((clamped - 1) as usize).copied()
}

/// Per-aspect scale for an IO set piece by aspect count, off the dataset's
/// multi-aspect ladder. Counts past the last rung keep its scale (pieces author
/// at most four aspect segments, but name-segment counting can exceed that).
/// `None` for a count below 1: an IO piece has at least one aspect, so a
/// non-positive count is malformed input that must surface.
pub fn multi_aspect_modifier(aspect_count: i64, curves: &EnhancementCurves) -> Option<f64> {
    if aspect_count < 1 {
        return None;
    }
    let ladder = &curves.multi_aspect_scale;
    let index = (aspect_count as usize).min(ladder.len()) - 1;
    Some(ladder[index])
}

/// Enhancement multiplier of the 25%-hot rarity tiers (purples + catalyzed Superior variants).
const HIGH_RARITY_MULTIPLIER: f64 = 1.25;

/// Enhancement multiplier per binary rarity tier (`boostsets.bin`): the
/// very-rare (purple) tiers and the catalyzed Superior variants run 25% hot;
/// everything else, including the Rebirth one-off event tiers, enhances at
/// standard values. `None` on a tier outside the vocabulary (TS twin throws).
/// Replaces the category=="purple" / `Superior*` name-prefix heuristic
/// (SOURCE-1 item 3).
pub fn set_rarity_multiplier(rarity: &str) -> Option<f64> {
    match rarity {
        "ECCommon"
        | "ECUncommon"
        | "ECRare"
        | "ECPvP"
        | "ECPVP"
        | "ECATO"
        | "ECATO2"
        | "ECWinter"
        | "ECHalloween"
        | "ECSummer"
        | "ECUniversalDamage"
        | "LibertysBelt"
        | "ImperialMight"
        | "ForcedIndoctrination"
        | "ECSpeedRun"
        | "" => Some(1.0),
        "ECVeryRare" | "ECUltraRare" | "ECSATO" | "ECSATO2" | "ECSWinter" | "ECSHalloween" => {
            Some(HIGH_RARITY_MULTIPLIER)
        }
        _ => None,
    }
}

/// Engine aspect key → `dim_returns` boost-type name. The outer `None` is an
/// aspect outside the engine vocabulary (rejected); the inner `None` marks
/// aspects whose boost type has no named dim_returns entry, so they take the
/// dataset's default bucket. The default entry IS the data, not a fallback.
/// Twin of the TS `ASPECT_BOOST_TYPE` map; the vocabulary-guard vitest pins
/// the named types against every dataset's export.
fn aspect_boost_type(normalized_aspect: &str) -> Option<Option<&'static str>> {
    match normalized_aspect {
        "absorb" | "accuracy" | "confuse" | "damage" | "defenseDebuff" | "endurance"
        | "enduranceMod" | "fear" | "fly" | "heal" | "hold" | "immobilize" | "intangible"
        | "jump" | "mezDuration" | "recharge" | "run" | "sleep" | "slow" | "stun" | "taunt" => {
            Some(None)
        }
        "defense" | "defenseBuff" => Some(Some("Buff_Defense")),
        "tohit" | "tohitBuff" => Some(Some("Buff_ToHit")),
        "tohitDebuff" => Some(Some("Debuff_ToHit")),
        "range" => Some(Some("Range")),
        "resistance" => Some(Some("Res_Damage")),
        "interrupt" => Some(Some("Interrupt")),
        "knockback" => Some(Some("Knockback")),
        _ => None,
    }
}

/// Aspect → ED schedule, mirroring the game's dim_returns lookup: a named
/// boost type reads its schedule from the dataset's map; a type the dataset
/// doesn't name (including every default-bucket aspect) takes the dataset's
/// default entry. `None` for an aspect outside the engine vocabulary: an
/// unknown aspect must surface, never silently enhance on the default
/// schedule (Rule 1). The enhancement gate asserts the out-of-domain probes
/// are rejected.
pub fn schedule_for_aspect(
    normalized_aspect: &str,
    curves: &EnhancementCurves,
) -> Option<Schedule> {
    let boost_type = aspect_boost_type(normalized_aspect)?;
    Some(match boost_type {
        None => curves.default_schedule,
        Some(name) => curves
            .boost_type_schedules
            .get(name)
            .copied()
            .unwrap_or(curves.default_schedule),
    })
}

/// TO/DO/SO enhancement percentage for one aspect, the SW8 grid read: the origin
/// families sit on flat Ones tables, so a tier's value depends only on the
/// aspect's ED schedule, per dataset. `None` on an unknown tier or an aspect
/// outside the engine vocabulary (TS twin throws).
pub fn origin_tier_value(
    tier: &str,
    normalized_aspect: &str,
    curves: &EnhancementCurves,
) -> Option<f64> {
    let scales = match tier {
        "TO" => &curves.origin_tiers.training,
        "DO" => &curves.origin_tiers.dual,
        "SO" => &curves.origin_tiers.single,
        _ => return None,
    };
    let schedule = schedule_for_aspect(normalized_aspect, curves)?;
    Some(scales.get(schedule) * 100.0)
}

// ============================================
// PER-POWER SLOT AGGREGATION
// ============================================

/// The six mez types a Controller/Dominator ATO's universal `Mez` aspect expands
/// to (the beta `UNIVERSAL_MEZ_KEYS`). In-game the set uses whichever type the
/// slotted power applies; the planner expands to all six at the same value and
/// the effect-matching layer keeps only the matching one.
const UNIVERSAL_MEZ_KEYS: [&str; 6] = ["hold", "stun", "immobilize", "sleep", "confuse", "fear"];

/// The keys the "Universal Travel" aspect expands to (the beta
/// `UNIVERSAL_TRAVEL_KEYS`). Winter's Gift and Blessing of the Zephyr carry ONE boost
/// that enhances every travel mode plus Range, so the game counts it as a single
/// aspect slot; its in-game piece name is literally "Run Speed, Jump, Flight
/// Speed, Range". Verified in `exported_powers/boosts/crafted_winters_gift_a`:
/// `['RunningSpeed','FlyingSpeed','JumpingSpeed']` and `['JumpHeight']` on
/// `Melee_Boosts_33` (Schedule A), `['Range']` on `Melee_Boosts_20` (Schedule B).
///
/// Unlike `Mez`, these do NOT share a schedule, so each key is looked up on its own.
/// `jump` is the single jump category covering both JumpingSpeed and JumpHeight.
///
/// The extractor labels the bundle "Move Speed", a name neither engine understood
/// until 2026-07-30, so both sets silently enhanced nothing through this aspect.
const UNIVERSAL_TRAVEL_KEYS: [&str; 4] = ["run", "fly", "jump", "range"];

/// Per-aspect enhancement fractions for one power's slotted enhancements, post-ED
/// (the beta `EnhancementBonuses`). Keyed by the normalized aspect names produced
/// by [`normalize_aspect_name`], a closed vocabulary, so the seams read fixed
/// literals (`enh.get("damage")`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EnhancementBonuses(BTreeMap<&'static str, f64>);

impl EnhancementBonuses {
    /// Build directly from `(aspect, fraction)` pairs, the shape the Alpha incarnate
    /// buff arrives in (the beta's `EnhancementBonuses` object literal). Aspect keys are
    /// the closed normalized vocabulary, so they're `&'static str` like the aggregation's.
    pub fn from_aspect_values(pairs: impl IntoIterator<Item = (&'static str, f64)>) -> Self {
        EnhancementBonuses(pairs.into_iter().collect())
    }

    /// The post-ED fraction for `aspect` (a normalized key like `"damage"`), or
    /// `0.0` if the power enhances it none (the beta's `enhBonuses.x || 0`).
    pub fn get(&self, aspect: &str) -> f64 {
        self.0.get(aspect).copied().unwrap_or(0.0)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `(aspect, fraction)` pairs, aspect-sorted, for gate serialization / display.
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, f64)> + '_ {
        self.0.iter().map(|(k, v)| (*k, *v))
    }
}

/// Normalize an enhancement aspect string to its internal key (the beta
/// `normalizeAspectName` over `ASPECT_NAME_MAP`). `None` for a string outside the
/// vocabulary, and the caller skips that contribution, matching the beta. Every
/// key this returns is in the schedule vocabulary ([`schedule_for_aspect`] never
/// rejects a normalized aspect).
///
/// A missing spelling is not an inert one: every caller skips the aspect entirely, so an
/// unmapped name is a piece that enhances nothing while its tooltip still reads a value.
/// `enhancement_vocabulary_gate` walks each dataset's own aspect strings so the next
/// unmapped one reds instead of silently zeroing.
pub fn normalize_aspect_name(aspect: &str) -> Option<&'static str> {
    Some(match aspect.trim() {
        "Acc" | "Accuracy" => "accuracy",
        "Dmg" | "Dam" | "Damage" => "damage",
        "Rech" | "Recharge" => "recharge",
        "EndRdx"
        | "EnduranceReduction"
        | "Endurance"
        | "End Reduction"
        | "Endurance Discount"
        | "Endurance Reduction" => "endurance",
        "EndMod" | "EnduranceModification" | "Endurance Modification" => "enduranceMod",
        "Range" | "Range Increase" => "range",
        "Heal" | "Healing" => "heal",
        "Def" | "Defense" => "defense",
        "DefBuff" | "Defense Buff" => "defenseBuff",
        "DefDeb" | "Defense Debuff" => "defenseDebuff",
        "Res" | "Resist Damage" | "Damage Resistance" | "Resistance" => "resistance",
        "ToHit" | "ToHit Buff" | "To Hit Buff" => "tohit",
        "ToHitDeb" | "ToHit Debuff" | "To Hit Debuff" => "tohitDebuff",
        "Hold" | "Hold Duration" => "hold",
        "Stun" | "Stun Duration" => "stun",
        "Immob" | "Immobilize" | "Immobilization Duration" => "immobilize",
        "Sleep" | "Sleep Duration" => "sleep",
        "Confuse" | "Confusion" | "Confuse Duration" => "confuse",
        // `Terrorize` is the set-piece spelling of the same aspect the powers call `Fear`
        // (47 pieces across the three datasets); `Threat` is the set-piece spelling of
        // `Taunt` (39). Neither had a mapping, so those pieces enhanced nothing.
        "Fear" | "Fear Duration" | "Terrorize" => "fear",
        "KB" | "Knockback" | "Knockback Distance" => "knockback",
        "Slow" | "Snare" => "slow",
        // `InterruptTime` is attested in the beta's `io-sets-raw` for all three forks; this
        // repo's IO-set catalogue is at an older vintage and doesn't carry it yet.
        "Interrupt" | "Interrupt Time" | "InterruptTime" | "Activation Acceleration" => "interrupt",
        "Absorb" => "absorb",
        "Intangible" => "intangible",
        "Mez Duration" => "mezDuration",
        "Run Speed" | "Run" => "run",
        "Fly" => "fly",
        "Jumping" | "Jump" => "jump",
        "Taunt" | "Threat" => "taunt",
        _ => return None,
    })
}

/// Effective aspect "slots" on an IO set piece for the multi-aspect scheduling
/// penalty (the beta `getEffectiveAspectCount`). An explicit `total_aspects` wins,
/// because the extractor recovers it by inverting the piece's own enhancement
/// scale — the game's authoritative dilution. Otherwise it is the aspect list,
/// plus one for the hidden segment a proc carries. Heal and Absorb share one
/// enhancement category, so the pair collapses to a single slot.
///
/// The piece's display name is not an input. Counting its slash segments would
/// put a display string inside the arithmetic, and the names are the game's own:
/// "Healing/Absorb" does not spell the pair this collapse looks for, so a
/// name-derived count would over-count precisely the pieces the collapse exists
/// for. Measured across all three forks, taking the larger of the two counts
/// moves 145 of the 3,653 pieces and every one of them moves upward: 114 are
/// the Heal/Absorb pieces, and 31 are pieces whose name segments outrun their
/// aspect list. The extractor emits `total_aspects` whenever the scale exceeds
/// the aspect list, so a piece without one already has its measured answer, and
/// a name count can only inflate it.
fn effective_aspect_count(aspects: &[String], is_proc: bool, total_aspects: Option<i64>) -> i64 {
    if let Some(total) = total_aspects {
        return total;
    }
    let has = |name: &str| aspects.iter().any(|a| a == name);
    let heal_absorb = has("Heal") && has("Absorb");
    aspects.len() as i64 + i64::from(is_proc) - i64::from(heal_absorb)
}

/// One boost's enhancement strength at `level` for a schedule, fail-loud: a
/// missing value (a fractional/out-of-domain level, a caller bug since
/// aggregation levels are integers) surfaces as a `CalcError` and contributes 0,
/// never a silent default (Rule 1).
fn io_value(
    level: i64,
    schedule: Schedule,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> f64 {
    match io_value_at_level(level as f64, schedule, curves) {
        Some(value) => value,
        None => {
            errors.push(CalcError::new(
                "enhancement",
                format!("no enhancement strength at level {level}"),
            ));
            0.0
        }
    }
}

/// Parse one IO-set piece's aspects into per-aspect fractions (the beta
/// `parseIOSetPieceValues`): each aspect gets its schedule's per-level strength
/// times the multi-aspect modifier, with the universal `Mez` aspect expanded to
/// the six mez keys.
fn parse_io_set_piece_values(
    aspects: &[String],
    level: i64,
    is_proc: bool,
    total_aspects: Option<i64>,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> BTreeMap<&'static str, f64> {
    let mut bonuses: BTreeMap<&'static str, f64> = BTreeMap::new();
    let count = effective_aspect_count(aspects, is_proc, total_aspects);
    let Some(modifier) = multi_aspect_modifier(count, curves) else {
        errors.push(CalcError::new(
            "enhancement",
            format!("io-set piece aspect count {count} out of range"),
        ));
        return bonuses;
    };
    for aspect in aspects {
        if aspect.trim() == "Mez" {
            for key in UNIVERSAL_MEZ_KEYS {
                let Some(schedule) = schedule_for_aspect(key, curves) else {
                    continue;
                };
                let value = io_value(level, schedule, curves, errors) * modifier;
                *bonuses.entry(key).or_insert(0.0) += value;
            }
            continue;
        }
        if aspect.trim() == "Move Speed" {
            for key in UNIVERSAL_TRAVEL_KEYS {
                let Some(schedule) = schedule_for_aspect(key, curves) else {
                    continue;
                };
                let value = io_value(level, schedule, curves, errors) * modifier;
                *bonuses.entry(key).or_insert(0.0) += value;
            }
            continue;
        }
        let Some(normalized) = normalize_aspect_name(aspect) else {
            continue;
        };
        let Some(schedule) = schedule_for_aspect(normalized, curves) else {
            continue;
        };
        // Overwrite (not accumulate) per aspect within one piece, mirroring the beta.
        bonuses.insert(
            normalized,
            io_value(level, schedule, curves, errors) * modifier,
        );
    }
    mirror_heal_to_absorb(&mut bonuses);
    bonuses
}

/// Every Heal-boosting enhancement boosts Absorb by the same amount: ONE boost applied to
/// two attribs, not two boosts. Verified in powers.bin: the generic Healing IO
/// `Boosts.Crafted_Heal_50` is a single `['Heal_Dmg','Absorb']`@Strength template.
///
/// IO-set pieces and most HOs list `Absorb` explicitly; generic IOs, origin enhancements and
/// a couple of specials (Titan Kyanite Shard, Positron Exposure) carry only "Healing", so
/// mirror it in one place per ENHANCEMENT, which is why a piece that already lists Absorb
/// is never counted twice. The beta twin is `mirrorHealToAbsorb` (enhancement-values.ts).
///
/// This lets [`crate::apply`] read the absorb magnitude's multiplier off the `absorb`
/// aspect: an Absorb-ONLY boost (the Cardiac/Resilient Radial Alpha) then lands without
/// also inflating heals.
fn mirror_heal_to_absorb(bonuses: &mut BTreeMap<&'static str, f64>) {
    if !bonuses.contains_key("absorb") {
        if let Some(heal) = bonuses.get("heal").copied() {
            bonuses.insert("absorb", heal);
        }
    }
}

/// Accumulate every slot's raw (pre-ED) per-aspect contribution across the four slot
/// kinds (IO set / generic IO / special / origin), the body shared by
/// [`calculate_power_enhancement_bonuses`] and [`combine_with_alpha_ed`].
///
/// A non-attuned IO-set piece is read at `min(slot.level ?? global, max)` for EVERY caller.
/// This used to be a per-caller flag: `combineWithAlphaED`'s copy of the beta loop never
/// picked up the `slot.level` fix, so an active Alpha silently re-levelled every slot to the
/// character's level (a level-30 Artillery triple paid 21.2% recharge instead of 17.4%), and
/// this port mirrored it bit-for-bit. Both sides now share one accumulator: an Alpha changes
/// what is ADDED to a power's aspects, never how its slots are read.
///
/// Fail-loud (Rule 1): a slot the interpreter can't resolve (an unknown stat, a
/// set/piece absent from the catalog, a missing catalog, an out-of-range boost or
/// rarity) surfaces as a `CalcError` and contributes nothing (numerically the
/// beta's skip), never a silent wrong value.
fn accumulate_raw_io_bonuses(
    slots: &[Option<Enhancement>],
    global_io_level: Level,
    io_sets: Option<&IoSetCatalog>,
    exemplar_level: Option<Level>,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> BTreeMap<&'static str, f64> {
    let mut raw: BTreeMap<&'static str, f64> = BTreeMap::new();

    for slot in slots.iter().flatten() {
        // Axis-aware and clamping. See `enhancement_level_multiplier`. An SO's
        // offset is a RELATIVE LEVEL off the above/below curves, not a booster
        // combine, and the previous unsigned `boosters`-only read could neither
        // represent nor apply the negative half.
        let boost = enhancement_level_multiplier(slot, curves);
        match &slot.kind {
            EnhancementKind::IoSet {
                set_id, piece_num, ..
            } => {
                let Some(catalog) = io_sets else {
                    errors.push(CalcError::new(
                        "enhancement",
                        format!("io-set {set_id} slotted but no set catalog is loaded"),
                    ));
                    continue;
                };
                let Some(set) = catalog.get(set_id) else {
                    errors.push(CalcError::new(
                        "enhancement",
                        format!("io-set {set_id} not in the catalog"),
                    ));
                    continue;
                };
                let Some(piece) = set.piece(*piece_num) else {
                    errors.push(CalcError::new(
                        "enhancement",
                        format!("io-set {set_id} has no piece #{piece_num}"),
                    ));
                    continue;
                };
                let Some(rarity) = set_rarity_multiplier(&set.rarity) else {
                    errors.push(CalcError::new(
                        "enhancement",
                        format!("io-set {set_id}: unknown rarity tier {:?}", set.rarity),
                    ));
                    continue;
                };
                // Attuned pieces (inherently, or individually catalyzed) scale with
                // character/exemplar level, uncapped by the set max; others fix at the
                // slot's craft level (the build level only when the slot carries none),
                // capped by the set max.
                let is_attuned = set.attuned_only || slot.attuned;
                let io_level = if is_attuned {
                    exemplar_level.unwrap_or(global_io_level)
                } else {
                    // An attuned-only set has no craft level to cap, and took the
                    // branch above; a `max_level` that isn't a real level imposes
                    // no cap here.
                    let craft = slot.level.unwrap_or(global_io_level);
                    craft.min(Level::from_i64(set.max_level).unwrap_or(craft))
                };
                let piece_bonuses = parse_io_set_piece_values(
                    &piece.aspects,
                    i64::from(io_level),
                    piece.proc,
                    piece.total_aspects,
                    curves,
                    errors,
                );
                let is_pure_proc = piece.proc && piece.aspects.is_empty();
                for (aspect, value) in piece_bonuses {
                    let mut scaled = value * rarity * boost;
                    if let Some(exemplar_level) = exemplar_level {
                        if !is_attuned && !is_pure_proc {
                            scaled = apply_exemplar_scaling(
                                scaled,
                                ExemplarLevels {
                                    io_level,
                                    exemplar_level,
                                },
                                false,
                                curves,
                            );
                        }
                    }
                    *raw.entry(aspect).or_insert(0.0) += scaled;
                }
            }
            EnhancementKind::GenericIo { stat, .. } => {
                let Some(normalized) = normalize_aspect_name(stat) else {
                    continue;
                };
                let Some(schedule) = schedule_for_aspect(normalized, curves) else {
                    continue;
                };
                // `slot.level || globalIOLevel`: an unset level falls to the build level.
                let io_level = slot.level.unwrap_or(global_io_level);
                let mut value = io_value(i64::from(io_level), schedule, curves, errors) * boost;
                if let Some(exemplar_level) = exemplar_level {
                    value = apply_exemplar_scaling(
                        value,
                        ExemplarLevels {
                            io_level,
                            exemplar_level,
                        },
                        false,
                        curves,
                    );
                }
                add_enhancement_bonus(&mut raw, normalized, value);
            }
            EnhancementKind::Special { aspects, .. } => {
                // Hamidon/Titan/etc.: each aspect carries its own percentage value.
                let mut slot_bonuses: BTreeMap<&'static str, f64> = BTreeMap::new();
                for aspect in aspects {
                    let Some(normalized) = normalize_aspect_name(&aspect.stat) else {
                        continue;
                    };
                    *slot_bonuses.entry(normalized).or_insert(0.0) +=
                        (aspect.value / 100.0) * boost;
                }
                mirror_heal_to_absorb(&mut slot_bonuses);
                for (aspect, value) in slot_bonuses {
                    *raw.entry(aspect).or_insert(0.0) += value;
                }
            }
            EnhancementKind::Origin { tier, stat, .. } => {
                // TO/DO/SO: value derived from tier + aspect schedule, not a stored number.
                let Some(normalized) = normalize_aspect_name(stat) else {
                    continue;
                };
                let Some(value) = origin_tier_value(tier, normalized, curves) else {
                    errors.push(CalcError::new(
                        "enhancement",
                        format!("origin tier {tier:?}/{normalized} not in the tier grid"),
                    ));
                    continue;
                };
                add_enhancement_bonus(&mut raw, normalized, (value / 100.0) * boost);
            }
        }
    }
    raw
}

/// Merge ONE single-aspect enhancement (generic IO, origin) into a power's running raw
/// totals, Heal→Absorb mirrored (see [`mirror_heal_to_absorb`]). IO-set pieces come
/// pre-mirrored out of [`parse_io_set_piece_values`]; specials mirror their whole aspect
/// list at once, since those DO list both.
fn add_enhancement_bonus(raw: &mut BTreeMap<&'static str, f64>, aspect: &'static str, value: f64) {
    *raw.entry(aspect).or_insert(0.0) += value;
    if aspect == "heal" {
        *raw.entry("absorb").or_insert(0.0) += value;
    }
}

/// Enhancement Diversification once per aspect (the sum-then-one-ED close of the beta
/// aggregation). Fail-loud on an aggregated aspect with no ED schedule (Rule 1).
fn apply_ed_per_aspect(
    raw: BTreeMap<&'static str, f64>,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> BTreeMap<&'static str, f64> {
    let mut ed: BTreeMap<&'static str, f64> = BTreeMap::new();
    for (aspect, raw_value) in raw {
        let Some(schedule) = schedule_for_aspect(aspect, curves) else {
            errors.push(CalcError::new(
                "enhancement",
                format!("aggregated aspect {aspect} has no ED schedule"),
            ));
            continue;
        };
        ed.insert(aspect, apply_ed(raw_value, schedule, curves));
    }
    ed
}

/// Aggregate a power's slotted enhancements into per-aspect [`EnhancementBonuses`]
/// (the beta `calculatePowerEnhancementBonuses`): accumulate every slot's raw
/// per-aspect contribution, then apply Enhancement Diversification once per aspect.
/// Non-attuned IO-set pieces read the slot's craft level (see
/// [`accumulate_raw_io_bonuses`]).
pub fn calculate_power_enhancement_bonuses(
    slots: &[Option<Enhancement>],
    global_io_level: Level,
    io_sets: Option<&IoSetCatalog>,
    exemplar_level: Option<Level>,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> EnhancementBonuses {
    let raw = accumulate_raw_io_bonuses(
        slots,
        global_io_level,
        io_sets,
        exemplar_level,
        curves,
        errors,
    );
    EnhancementBonuses(apply_ed_per_aspect(raw, curves, errors))
}

/// One enhancement's own strength per authored aspect, pre-ED — what the piece says on its
/// face, for a tooltip. Read through [`accumulate_raw_io_bonuses`] so it can never disagree
/// with what the aggregate adds for the same slot. Labelled by the authored aspect, not the
/// normalized key: a Heal piece's mirrored Absorb is one boost, not a second line. `Mez`
/// reads as one line (its six keys share a value); `Move Speed` splits per key, because its
/// keys sit on different schedules. `None` when the slot can't be resolved, never a guess.
pub fn single_enhancement_values(
    slot: &Enhancement,
    global_io_level: Level,
    io_sets: Option<&IoSetCatalog>,
    curves: &EnhancementCurves,
) -> Option<Vec<(String, f64)>> {
    let mut errors = Vec::new();
    let raw = accumulate_raw_io_bonuses(
        std::slice::from_ref(&Some(slot.clone())),
        global_io_level,
        io_sets,
        None,
        curves,
        &mut errors,
    );
    if !errors.is_empty() {
        return None;
    }
    let authored: Vec<&str> = match &slot.kind {
        EnhancementKind::IoSet { aspects, .. } => aspects.iter().map(String::as_str).collect(),
        EnhancementKind::GenericIo { stat, .. } | EnhancementKind::Origin { stat, .. } => {
            vec![stat.as_str()]
        }
        EnhancementKind::Special { aspects, .. } => {
            aspects.iter().map(|a| a.stat.as_str()).collect()
        }
    };
    let enh_type = |key: &str| {
        ASPECT_TO_ENH_TYPE
            .iter()
            .find(|(aspect, _)| *aspect == key)
            .map(|(_, label)| *label)
    };
    let mut lines = Vec::new();
    for aspect in authored {
        match aspect.trim() {
            "Mez" => {
                if let Some(value) = raw.get(UNIVERSAL_MEZ_KEYS[0]) {
                    lines.push(("Mez".to_string(), *value));
                }
            }
            "Move Speed" => {
                for key in UNIVERSAL_TRAVEL_KEYS {
                    if let (Some(value), Some(label)) = (raw.get(key), enh_type(key)) {
                        lines.push((label.to_string(), *value));
                    }
                }
            }
            other => {
                if let Some(value) = normalize_aspect_name(other).and_then(|key| raw.get(key)) {
                    lines.push((other.to_string(), *value));
                }
            }
        }
    }
    Some(lines)
}

/// Aspect key → the `EnhancementStatType` a power lists in `allowed_enhancements`. The
/// beta's `ASPECT_TO_ENH_TYPE`, the reverse of the aggregation's aspect vocabulary. Both
/// `defense` and `defenseBuff` map to `"Defense"` (either satisfies a `Defense`-accepting
/// power). Gates which Alpha aspects a power accepts ([`allowed_aspect_keys`]).
const ASPECT_TO_ENH_TYPE: &[(&str, &str)] = &[
    ("damage", "Damage"),
    ("accuracy", "Accuracy"),
    ("recharge", "Recharge"),
    ("endurance", "EnduranceReduction"),
    ("enduranceMod", "EnduranceModification"),
    ("range", "Range"),
    ("heal", "Healing"),
    ("defense", "Defense"),
    ("defenseBuff", "Defense"),
    ("resistance", "Resistance"),
    ("tohit", "ToHit"),
    ("tohitDebuff", "ToHit Debuff"),
    ("defenseDebuff", "Defense Debuff"),
    ("hold", "Hold"),
    ("stun", "Stun"),
    ("immobilize", "Immobilize"),
    ("sleep", "Sleep"),
    ("confuse", "Confuse"),
    ("fear", "Fear"),
    ("knockback", "Knockback"),
    ("slow", "Slow"),
    ("taunt", "Taunt"),
    ("interrupt", "Interrupt"),
    // Absorb has no enhancement category of its own. It's the second attrib of the HEALING
    // boost (see `mirror_heal_to_absorb`), so a power that accepts Healing is exactly the
    // power an Absorb buff reaches. Keying this to a nonexistent "Absorb" category filtered
    // the Cardiac/Resilient Radial Alpha's +33% Absorb out of every power in the build.
    ("absorb", "Healing"),
    ("intangible", "Intangible"),
    ("mezDuration", "Mez Duration"),
    ("run", "Run Speed"),
    ("fly", "Fly"),
    ("jump", "Jump"),
];

/// Intern an already-normalized aspect key (e.g. `"damage"`) to its canonical
/// `&'static str`, or `None` if it's outside the engine's aspect vocabulary.
/// Distinct from [`normalize_aspect_name`], which maps an *authored* spelling
/// (`"Damage"`, `"Dmg"`) to the normalized key: this interns a key *already* in
/// normalized form, the shape of [`EnhancementBonuses`] and the Alpha buff maps.
/// The vocabulary is the key column of [`ASPECT_TO_ENH_TYPE`], which is exactly
/// the set [`normalize_aspect_name`] emits.
pub fn intern_aspect_key(normalized: &str) -> Option<&'static str> {
    ASPECT_TO_ENH_TYPE
        .iter()
        .map(|(aspect, _)| *aspect)
        .find(|aspect| *aspect == normalized)
}

/// The set of aspect keys an Alpha buff may reach on a power, from its `allowed_enhancements`
/// list (the beta's `allowedAspectKeys`). `None` (the field is ABSENT on the wire) means
/// "accept every aspect" (the beta's `null` gate); `Some(set)` accepts only the listed
/// categories' aspects, and an EMPTY list yields an empty set: accept none. Absent ≠ empty is
/// load-bearing (127 field-less pseudo-pet powers accept everything, an `allowedEnhancements:
/// []` power accepts nothing), so the caller passes `Option<&[String]>`, not a flattened slice.
fn allowed_aspect_keys(
    allowed: Option<&[String]>,
) -> Option<std::collections::HashSet<&'static str>> {
    let allowed = allowed?;
    Some(
        ASPECT_TO_ENH_TYPE
            .iter()
            .filter(|(_, enh_type)| allowed.iter().any(|a| a == enh_type))
            .map(|(aspect, _)| *aspect)
            .collect(),
    )
}

/// Whether the Alpha buff for `aspect` reaches a power with these accepted keys (the beta's
/// `alphaAcceptsAspect`). `None` (field absent) accepts every aspect.
fn alpha_accepts_aspect(
    allowed_keys: &Option<std::collections::HashSet<&'static str>>,
    aspect: &str,
) -> bool {
    match allowed_keys {
        None => true,
        Some(keys) => keys.contains(aspect),
    }
}

/// Filter an Alpha buff to only the aspects a power accepts via `allowed_enhancements`, the
/// beta `filterAlphaByAllowedEnhancements`, used for a power with NO slots (Alpha is then the
/// only enhancement source, added without an ED split). Absent list (`None`) returns the buff
/// unchanged; a present list keeps only its accepted aspects (empty list ⇒ nothing).
pub fn filter_alpha_by_allowed_enhancements(
    alpha_bonuses: &EnhancementBonuses,
    allowed: Option<&[String]>,
) -> EnhancementBonuses {
    let allowed_keys = allowed_aspect_keys(allowed);
    EnhancementBonuses(
        alpha_bonuses
            .0
            .iter()
            .filter(|(aspect, _)| alpha_accepts_aspect(&allowed_keys, aspect))
            .map(|(aspect, value)| (*aspect, *value))
            .collect(),
    )
}

/// Combine a power's slotted-IO bonuses with an Alpha incarnate buff, honouring Alpha's
/// ED-bypass mechanic (the beta `combineWithAlphaED`, enhancement-values.ts:731). Alpha's
/// per-aspect value splits in two, gated by the power's `allowed_enhancements`:
///
/// 1. raw IO sum (before ED) from [`accumulate_raw_io_bonuses`], the same accumulator the
///    non-Alpha path uses, so a slot is read identically either way;
/// 2. + the ED-SUBJECT slice of Alpha (`value − bypass`) folded into that raw sum;
/// 3. one ED pass over the combined total;
/// 4. + the ED-BYPASS slice (`bypass`) added on top, after ED.
///
/// The bypass slice comes from the exported silent-grant data (`alpha_ed_bypass`, the
/// BoostIgnoreDiminishing / `Ones` templates), not a per-tier ratio, because Thunderspy
/// authors splits that diverge from the HC/Rebirth rarity pattern. A naïve post-ED add of the
/// whole Alpha value would overstate enhancement when a slotted aspect already sits at the
/// ED knee.
#[allow(clippy::too_many_arguments)]
pub fn combine_with_alpha_ed(
    slots: &[Option<Enhancement>],
    allowed: Option<&[String]>,
    global_io_level: Level,
    io_sets: Option<&IoSetCatalog>,
    alpha_bonuses: &EnhancementBonuses,
    alpha_ed_bypass: &EnhancementBonuses,
    exemplar_level: Option<Level>,
    curves: &EnhancementCurves,
    errors: &mut Vec<CalcError>,
) -> EnhancementBonuses {
    // Step 1: raw IO totals (before ED). Non-attuned pieces read their craft level.
    let mut raw = accumulate_raw_io_bonuses(
        slots,
        global_io_level,
        io_sets,
        exemplar_level,
        curves,
        errors,
    );

    let allowed_keys = allowed_aspect_keys(allowed);

    // Step 2: fold Alpha's ED-subject slice (total − bypass) into the raw sum, for the
    // aspects the power accepts. A zero Alpha value contributes nothing (the beta's
    // `value !== 0` guard); absent aspects are never iterated.
    for (aspect, value) in alpha_bonuses.0.iter() {
        if *value != 0.0 && alpha_accepts_aspect(&allowed_keys, aspect) {
            let ed_subject = value - alpha_ed_bypass.get(aspect);
            *raw.entry(*aspect).or_insert(0.0) += ed_subject;
        }
    }

    // Step 3: one ED pass over the combined totals.
    let mut result = apply_ed_per_aspect(raw, curves, errors);

    // Step 4: add Alpha's ED-bypass slice on top, same gate as Step 2.
    for (aspect, value) in alpha_bonuses.0.iter() {
        if *value != 0.0 && alpha_accepts_aspect(&allowed_keys, aspect) {
            let bypass = alpha_ed_bypass.get(aspect);
            *result.entry(*aspect).or_insert(0.0) += bypass;
        }
    }

    EnhancementBonuses(result)
}
