//! Proc / global-IO contributions to the character totals, a faithful port of the
//! beta's proc pass (`applyProcBonuses` / `applyPPMProcBonuses` /
//! `applyBuildUpProcBonuses` / `applyVariableProcBonuses`, `character-totals.ts`).
//! Reads the per-dataset [`coh_data::ProcDatabase`] and the build's slotted IO
//! pieces; writes the always-on/PPM/Build-Up/variable proc bonuses into
//! [`GlobalBonuses`], with the proc Rule of 5 (separate from set bonuses).
//!
//! Scope (M4 "Procs/PPM in calc"): the TOTALS contributions only. Always-on
//! globals (LotG +Recharge, Steadfast +Def), Proc120s (Numina, Miracle), PPM
//! procs in auto/toggle powers (Performance Shifter → recovery), averaged Build-Up
//! procs (→ damage/toHit), and variable procs (Reactive Defenses HP-scaling +Res,
//! Might of the Tanker stacks). DAMAGE-proc DPS is per-power (M5), NOT a totals
//! stat: `apply_single_proc_effect` drops the `Damage` range category exactly as
//! the beta does (only the always-on `+Damage` global, Liberty's Belt, lands).
//!
//! User-facing proc controls: two switches over every pass, both read off the build.
//! [`CharacterState::disabled_proc_categories`] (per category) and
//! [`CharacterState::proc_overrides`] (per slotted piece, plus its stack / %HP pin). The
//! piece's own switch wins where the build set one, and the category switch is the default for
//! a piece the build has never touched, so the two can't disagree about one proc. A fresh
//! build disables nothing and overrides nothing, exactly the beta's default state.
//!
//! The offered categories are derived, not enumerated. See [`proc_categories`].

use crate::gather;
use crate::incarnates::AlphaEnhancement;
use crate::projection::{ReductionClamps, StrengthBounds};
use crate::stealth::StealthContribution;
use crate::totals::{route_closed, CalcError, GlobalBonuses};
use coh_data::{
    CharacterState, Enhancement, EnhancementKind, Power, PowerDatabase, ProcData, ProcEffect,
    ProcOverride, ProcType, SelectedPower,
};
use serde_json::Value;
use std::collections::BTreeMap;

/// Base recovery rate (end/sec) used to convert an instant endurance/recovery
/// grant into a steady-state recovery %. Beta `enhancement-values.ts`.
const BASE_RECOVERY_RATE: f64 = 1.667;
/// Base regeneration rate (%HP/sec) for the instant-heal → regen conversion.
const BASE_REGEN_RATE: f64 = 100.0 / 240.0;
/// The discrete "auto" default stack count for a self-stacking buff proc.
const DEFAULT_STACK_COUNT: f64 = 1.0;

// ============================================================
// PPM math (beta proc-data.ts): pure functions, data in / number out.
// ============================================================

/// The geometry `basepower_CalculateAreaFactor` scores a power's PPM procs against. Carried as
/// one value rather than three parameters because `radius` and `arc_degrees` are same-typed
/// neighbours at every call site, and because `only_main_target` short-circuits both of them.
/// Passing it separately invites a call site that reads the flag and then forgets it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProcArea {
    pub radius: f64,
    pub arc_degrees: f64,
    /// `BasePower.bUseNonBoostTemplatesOnMainTarget`: the power's non-boost templates apply to
    /// the main target only, so its procs take no area penalty however wide it is.
    pub only_main_target: bool,
}

impl ProcArea {
    /// Single target: the geometry every power with no radius resolves to.
    fn single_target() -> Self {
        Self {
            radius: 0.0,
            arc_degrees: 360.0,
            only_main_target: false,
        }
    }
}

/// AoE penalty denominator: `0.25 + 0.75 × (1 + radius × (11·arc + 540) / 30000)`.
/// Single target (radius 0) → 1.0.
///
/// Faithful to `basepower_CalculateAreaFactor` (`powers.c:2806`), whose cone branch is
/// `(1 + radius×0.15) − (radius×0.00036667 × (360 − arc°))`, the same polynomial, with the
/// sphere branch recovered as the `arc = 360` case that callers substitute.
///
/// The main-target branch comes FIRST because the game's does: it returns 1.0 before it looks at
/// radius at all. Passing a zero radius instead would land on the same number today and lie
/// about why. The power still has its radius, and a later reader of that argument would be
/// reading a fabricated geometry.
fn ppm_area_denominator(area: ProcArea) -> f64 {
    if area.only_main_target || area.radius <= 0.0 {
        return 1.0;
    }
    0.25 + 0.75 * (1.0 + area.radius * (11.0 * area.arc_degrees + 540.0) / 30000.0)
}

/// Convert a raw arc (radians when ≤ 2π, else already degrees) to degrees.
pub(crate) fn arc_to_degrees(raw_arc: f64) -> f64 {
    if raw_arc == 0.0 {
        return 0.0;
    }
    if raw_arc <= 2.0 * std::f64::consts::PI {
        raw_arc * (180.0 / std::f64::consts::PI)
    } else {
        raw_arc
    }
}

/// Minimum proc chance: `5% + PPM × 1.5%`.
fn ppm_min_chance(ppm: f64) -> f64 {
    0.05 + ppm * 0.015
}

/// Clamp a raw chance to `[5% + PPM×1.5%, 90%]`.
fn clamp_proc_chance(raw: f64, ppm: f64) -> f64 {
    raw.max(ppm_min_chance(ppm)).min(0.9)
}

/// A click power's recharge as the proc window sees it: `base / (1 + local)`, where `local` is
/// the post-ED recharge slotted in this power, Alpha included. The build's global recharge — set
/// bonuses, Hasten, Ageless — does not enter, however much of it there is.
///
/// That exclusion is Homecoming's published PPM rule, which names the sources it excludes:
/// *"'Enhanced Recharge Time' includes reductions from enhancements and Alpha slotting, but not
/// from Luck of the Gambler bonuses, other enhancement set bonuses, Hasten, or other recharge
/// buffs."* It is also what a per-minute rate has to mean: global recharge makes the power fire
/// more often, so holding the per-activation chance fixed is what keeps procs *per minute* near
/// the piece's PPM.
///
/// This replaced a global-diluted window, `base × (1 + global) / (1 + global + local)`, which was
/// read off the 2012 server's STRUCTURE rather than measured (DATA-GAP-REGISTER PPM-2). The two
/// agree exactly whenever the power carries no slotting of its own — which is why the divergence
/// went unreported for so long, and why the bug report that found it had to slot recharge first.
///
/// `StrengthsDisallowed('RechargeTime')` drops the local term: a power that takes no recharge
/// strength has no window to shift. `GlobalStrengthsDisallowed` no longer has anything to drop
/// here. `net` holds the divisor inside the archetype's `ClampStrength` interval, the same one the
/// perma ring and the projection divide by; slotting alone cannot reach that ceiling, so it does
/// not bind today and is kept for fidelity to the game's clamp rather than for any effect.
fn proc_recharge_window(
    def: &Power,
    base_recharge: f64,
    slotted_recharge: f64,
    bounds: Option<StrengthBounds>,
) -> f64 {
    let local = if crate::perma::disallows_recharge(def, "strengthsDisallowed") {
        0.0
    } else {
        slotted_recharge
    };
    let net = |strength: f64| match bounds {
        Some(bounds) => strength.clamp(bounds.floor, bounds.cap),
        None => strength,
    };
    base_recharge / net(1.0 + local)
}

/// Proc chance per activation for a click power (the PPM formula).
///
/// `recharge_window` is [`proc_recharge_window`], not the power's base recharge. Which POWER's
/// recharge it comes off was HC-4, and it's settled: the host's own, not the child of an
/// `ExecutePower` wrapper it delegates to. Measured in game 2026-08-02.
fn calculate_proc_chance(ppm: f64, recharge_window: f64, cast_time: f64, area: ProcArea) -> f64 {
    let area_denom = ppm_area_denominator(area);
    let raw = (ppm * (recharge_window + cast_time)) / (60.0 * area_denom);
    clamp_proc_chance(raw, ppm)
}

/// Proc chance per check for an auto/toggle power: the same formula as a click's, with the
/// piece's activate period standing in for the host's `recharge + cast`.
///
/// `activate_period` is the PROC PIECE's own `fActivatePeriod`, not the toggle's: the engine reads
/// `ptemplate->ppowBase->fActivatePeriod` (`character_combat.c` `CalculateModChance`), and
/// `ptemplate->ppowBase` is "the power which contains this AttribModTemplate". For a proc that's
/// the BOOST's own power, which the host toggle only borrows templates from
/// (`character_combat.c:2932`).
fn calculate_auto_toggle_proc_chance(ppm: f64, activate_period: f64, area: ProcArea) -> f64 {
    let area_denom = ppm_area_denominator(area);
    let raw = (ppm * activate_period) / (60.0 * area_denom);
    clamp_proc_chance(raw, ppm)
}

/// Expected procs per minute for an auto/toggle power. The same field re-arms the piece
/// (`powers.c` `power_IncrementBoostTimers`: `fTimer += fActivatePeriod`), so the check rate is
/// `60 / period`. One field, not a second fact.
fn calculate_auto_toggle_procs_per_minute(ppm: f64, activate_period: f64, area: ProcArea) -> f64 {
    calculate_auto_toggle_proc_chance(ppm, activate_period, area) * (60.0 / activate_period)
}

/// The proc's control type (beta `getProcControlType`): a self-stacking buff
/// (`stacks`), an HP-scaling global (`hp`), or a plain always-on effect (`toggle`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlType {
    Stacks,
    Hp,
    Toggle,
}

fn control_type(eff: &ProcEffect) -> ControlType {
    if eff.max_stacks.is_some() {
        ControlType::Stacks
    } else if eff.scaling {
        ControlType::Hp
    } else {
        ControlType::Toggle
    }
}

/// One slotted proc's control override, keyed by the pick's own address plus the slot index
/// ([`proc_override_key`]).
///
/// The beta keys this on the power's DISPLAY name (`procOverrideKey`), which can't address one
/// pick: two picks in one build can share a display name, so an override set on the epic copy
/// also lands on the secondary one.
fn proc_override<'a>(
    state: &'a CharacterState,
    selection: &SelectedPower,
    slot_index: usize,
) -> Option<&'a ProcOverride> {
    state.proc_overrides.get(&proc_override_key(
        &selection.powerset,
        &selection.internal_name,
        slot_index,
    ))
}

/// The [`CharacterState::proc_overrides`] key one slotted piece answers to. Minted in one place
/// so the control that writes it and the five passes that read it can't drift.
pub fn proc_override_key(powerset: &str, internal_name: &str, slot_index: usize) -> String {
    let address = coh_data::power_address(powerset, internal_name);
    format!("{address}:{slot_index}")
}

/// Interpolate an HP-scaling proc's magnitude: full HP (100%) ⇒ `floor`, ~0 HP ⇒ `cap`, linear
/// in %HP. The beta `interpolateScalingValue`.
fn interpolate_scaling_value(floor: f64, cap: f64, hp_pct: f64) -> f64 {
    let clamped = hp_pct.clamp(0.0, 100.0);
    floor + (cap - floor) * (1.0 - clamped / 100.0)
}

/// Resolve a variable proc's steady-state contribution under its override, the beta
/// `resolveProcContribution`. With no override (or `auto`) this is the honest default: a
/// stacking buff contributes one DISCRETE stack (never a fractional expected-uptime average, a
/// moment the player is never in), an HP-scaling global contributes its always-on
/// floor, a plain proc its full value. A disabled proc contributes nothing.
fn resolve_proc_contribution(
    control: ControlType,
    per_unit: f64,
    cap_value: Option<f64>,
    max_stacks: Option<u32>,
    override_: Option<&ProcOverride>,
) -> f64 {
    if override_.is_some_and(|o| !o.enabled) {
        return 0.0;
    }
    let mode = override_.map_or("auto", |o| o.mode.as_str());
    match control {
        ControlType::Stacks => {
            let cap = f64::from(max_stacks.unwrap_or(1));
            let stacks = if mode == "stacks" {
                f64::from(override_.and_then(|o| o.stacks).unwrap_or(0)).clamp(0.0, cap)
            } else {
                DEFAULT_STACK_COUNT.min(cap)
            };
            per_unit * stacks
        }
        ControlType::Hp => {
            let floor = per_unit;
            let cap = cap_value.unwrap_or(floor);
            if mode == "hp" {
                interpolate_scaling_value(
                    floor,
                    cap,
                    override_.and_then(|o| o.hp_pct).unwrap_or(100.0),
                )
            } else {
                floor
            }
        }
        ControlType::Toggle => per_unit,
    }
}

// ============================================================
// Power def geometry: the beta reads `power.stats?.X ?? power.effects?.X`.
// ============================================================

fn def_num(def: &Power, subobject: &str, key: &str) -> Option<f64> {
    def.extra
        .get(subobject)
        .and_then(Value::as_object)
        .and_then(|o| o.get(key))
        .and_then(Value::as_f64)
}

/// `power.stats?.X ?? fallback`, over a power as the DATABASE holds it.
///
/// The `effects` half is gone. Every `def` that reaches this module comes
/// from `gather::resolve_power` via [`selected_with_def`] — the contract power, never the
/// effective one [`crate::effective::with_active_conditionals`] writes a bag onto. The
/// contract has carried no power-level bag on any fork since 2026-09-03, so the fallback
/// read an object that was never there.
fn stat_or_default(def: &Power, key: &str, fallback: f64) -> f64 {
    def_num(def, "stats", key).unwrap_or(fallback)
}

fn power_type_lower(def: &Power) -> String {
    def.extra
        .get("powerType")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// One executed child that rolls a `ProcAllowed kNone` power's procs in its place, the contract's
/// `procRollSites`, stamped by `collectProcRollSites`. See the field doc on the beta's
/// `Power.procRollSites`; the short version is that a `kNone` shell with a `CopyBoosts`
/// `kExecutePower` child has handed that child its slotting, so the child rolls, in the SHELL's
/// window, against the CHILD's geometry. A site therefore carries a routing key and geometry,
/// nothing else.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcRollSite {
    /// Full name of the child, for provenance in an error or a breakdown row.
    pub power: String,
    /// The child's own `BoostsAllowed`, the routing key `CopyBoosts` filters by.
    #[serde(default)]
    pub boosts_allowed: Vec<String>,
    pub radius: f64,
    /// Raw radians, as the power's own `stats.arc` carries it.
    pub arc: f64,
    #[serde(default)]
    pub procs_only_on_main_target: bool,
}

/// A power's roll sites, or an empty list when it rolls its own window.
///
/// A PRESENT but malformed list is an error, never an empty list: silently reading it as "no
/// sites" would put the ten powers back to firing nothing, the exact wrong number this field
/// exists to fix.
fn proc_roll_sites(def: &Power) -> Result<Vec<ProcRollSite>, CalcError> {
    let Some(raw) = def.extra.get("procRollSites") else {
        return Ok(Vec::new());
    };
    serde_json::from_value(raw.clone()).map_err(|e| {
        CalcError::new(
            "Proc",
            format!("{}: procRollSites did not parse — {e}", def.name),
        )
    })
}

/// The site a proc piece reaches: the one child whose `BoostsAllowed` intersects the piece's own.
///
/// Zero intersections is a real answer: `CopyBoosts` hands the child nothing it can't hold, so
/// the piece fires nowhere. Two is a shape the one-roll model can't express, and a piece with no
/// `boostsAllowed` in the proc data is an extractor gap the routing can't answer around; both
/// fail loud rather than picking a geometry. Sibling sites may share boost types no proc carries
/// (Fault's children both list Range and Accuracy), so the collision is per PIECE and lives
/// here, not in the converter.
fn site_for_piece<'a>(
    def: &Power,
    sites: &'a [ProcRollSite],
    proc: &ProcData,
    set_name: &str,
    io_name: &str,
) -> Result<Option<&'a ProcRollSite>, CalcError> {
    let piece_boosts = match proc.boosts_allowed.as_deref() {
        Some(b) if !b.is_empty() => b,
        _ => {
            return Err(CalcError::new(
                "Proc",
                format!(
                    "{set_name} \"{io_name}\": no boostsAllowed in the proc data, and {} \
                     routes its procs by it",
                    def.name
                ),
            ));
        }
    };
    let mut hits = sites
        .iter()
        .filter(|s| s.boosts_allowed.iter().any(|b| piece_boosts.contains(b)));
    let first = hits.next();
    if let (Some(a), Some(b)) = (first, hits.next()) {
        return Err(CalcError::new(
            "Proc",
            format!(
                "{set_name} \"{io_name}\" in {}: sites {} and {} both accept it — a proc \
                 slotted here has no single roll",
                def.name, a.power, b.power
            ),
        ));
    }
    Ok(first)
}

/// Whether a proc piece's template FIRES in this host power, and if so in which window.
///
/// `ProcAllowed kNone` (contract `procsAllowed: false`) is the game's statement that procs don't
/// fire against a power's OWN recharge, authored beside `BoostsAllowed`. The power keeps its
/// set categories, so a build can still slot one and does: 66 of the flagged Homecoming powers
/// accept a category some `Proc` piece belongs to, dominated by the pet summons where Recharge
/// Intensive Pets and Pet Damage go.
///
/// On the powers carrying `procRollSites` the flag is only half the story: a `CopyBoosts` executed
/// child rolls in the shell's place, and the piece reaches the one child whose `BoostsAllowed`
/// intersects its own. `Ok(Some(None))` means "fires, against this power's own geometry";
/// `Ok(Some(Some(site)))` means "fires, against that child's"; `Ok(None)` means it fires nowhere.
/// The WINDOW is the shell's either way: a site carries geometry, never a schedule.
///
/// Scoped to `ProcType::Proc`, which is the mechanism the flag names. `Global` and `Proc120s`
/// are granted by slotting the piece: the always-on pass never asks the host power to fire, and
/// applies a `Global` whatever the host is doing. Gating them on this flag would be a wider rule
/// than the evidence carries, and would strip real globals off 65 flagged powers.
fn proc_fires_in(
    def: &Power,
    proc: &ProcData,
    set_name: &str,
    io_name: &str,
) -> Result<Option<Option<ProcRollSite>>, CalcError> {
    if proc.proc_type != ProcType::Proc {
        return Ok(Some(None));
    }
    let allowed = def
        .extra
        .get("procsAllowed")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    if allowed {
        return Ok(Some(None));
    }
    let sites = proc_roll_sites(def)?;
    // No sites means no delegation: the flag stands alone and nothing fires. Answered before
    // the routing so the flagged-but-siteless population (every pet summon) never demands a
    // `boostsAllowed` it has no use for.
    if sites.is_empty() {
        return Ok(None);
    }
    Ok(site_for_piece(def, &sites, proc, set_name, io_name)?
        .cloned()
        .map(Some))
}

/// The geometry a piece is scored against: the routed child's when a site took it, the host's
/// own otherwise.
///
/// NOT `clamp`. `.max(0.0).min(360.0)` maps a NaN arc to 0.0, because `f64::max` returns the
/// non-NaN operand; `f64::clamp` propagates the NaN instead. The stage 7 baseline work found a
/// live NaN swallowed by this exact shape, so the two are not interchangeable and the current one
/// is the behaviour graded.
#[allow(clippy::manual_clamp)]
fn roll_area(site: Option<&ProcRollSite>, own: ProcArea) -> ProcArea {
    match site {
        Some(s) => ProcArea {
            radius: if s.procs_only_on_main_target {
                0.0
            } else {
                s.radius
            },
            arc_degrees: arc_to_degrees(s.arc).max(0.0).min(360.0),
            only_main_target: s.procs_only_on_main_target,
        },
        None => own,
    }
}

/// The summon block a power's pseudo-pet facts come off, or `None` when it summons nothing.
fn power_summon(def: &Power) -> Option<&Value> {
    def.summon()
}

/// Every ability of the pseudo-pets a power summons, under [`crate::granted::pseudo_pet_effects`]'
/// precedence: the entity table first, the inline `resolvedEntities` block only when the table
/// produced nothing.
///
/// That precedence is load-bearing, not a preference. Homecoming's Sentinel Whirlpool is the
/// one power in any fork carrying both blocks for the SAME pet, and they're identical row for row
/// (ENT-8), so walking both would count the pet twice, which a max-radius read survives and a roll
/// count does not.
///
/// [`crate::granted::summoned_entity_chain`] does two things the beta's flat lookup can't: it
/// stops at commandable pets (a Mastermind henchman is nobody's patch) and it follows
/// `createsEntities` one summon deeper (ENT-3).
fn pseudo_pet_abilities<'a>(def: &'a Power, db: &'a PowerDatabase) -> Vec<&'a Value> {
    let Some(summon) = power_summon(def) else {
        return Vec::new();
    };
    let mut out: Vec<&Value> = Vec::new();
    for name in crate::granted::summon_entity_names(summon) {
        for entity in crate::granted::summoned_entity_chain(db, &name) {
            out.extend(crate::granted::entity_abilities(entity));
        }
    }
    if out.is_empty() {
        for resolved in summon
            .get("resolvedEntities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            out.extend(crate::granted::entity_abilities(resolved));
        }
    }
    out
}

/// An ability's own AoE footprint, or `None` when it has none the area factor should read. A
/// radius on a `SingleTarget` or `Self` ability is not a footprint. It's the reach of something
/// that lands on one recipient.
fn ability_footprint(ability: &Value) -> Option<f64> {
    let radius = ability.get("radius").and_then(Value::as_f64)?;
    if radius <= 0.0 {
        return None;
    }
    match ability.get("effectArea").and_then(Value::as_str) {
        Some("SingleTarget" | "Self") => None,
        _ => Some(radius),
    }
}

/// Widest AoE footprint carried by a power's summoned pseudo-pet or ground patch.
///
/// Many AoE powers carry radius 0 on the parent because the real area lives on the summon: Burn is
/// a Self/SingleTarget shell over a radius-15 static object, and the rains are a Location parent
/// over a radius-20-to-25 patch. Scoring those off the parent makes them single-target and reports
/// their proc chance far too high: 238 Homecoming powers, 281 Rebirth, 233 Thunderspy. That
/// population is deliberately WIDER than the patch class in [`proc_patch_duration`] (179/219/182):
/// a pet that attacks on its own recharge still lends its footprint, it just doesn't lend a clock.
///
/// Arc is not read: the bin format stores none on a pseudo-pet ability, and every one of these
/// footprints is a sphere. `None` when the summon has no footprint at all.
pub fn pseudo_pet_proc_radius(def: &Power, db: &PowerDatabase) -> Option<f64> {
    let widest = pseudo_pet_abilities(def, db)
        .into_iter()
        .filter_map(ability_footprint)
        .fold(0.0_f64, f64::max);
    (widest > 0.0).then_some(widest)
}

/// Lifetime (s) of the summoned patch a power's PPM procs actually roll on, or `None` when they
/// roll on the cast like an ordinary power's.
///
/// The second half of [`pseudo_pet_proc_radius`]' job. That one borrows the patch's
/// RADIUS; this one recognises the patch also owns the CLOCK. A rain's parent is a Location
/// shell with no AoE of its own, and the pulsing power on the summon is an `Auto` with recharge 0,
/// so its procs fall back on the period the proc piece itself carries and the patch lives long
/// enough to see several of them. See [`proc_roll_schedule`] for the measurement.
///
/// The gate is narrow and the narrowing is the point. A parent with its own footprint rolls on the
/// cast, whatever rides along with it. Past that, every radius-bearing summoned ability must be
/// `Auto`, because a pulsing patch is what borrows the clock and a pet that attacks on its own
/// recharge is not one: Acid Mortar, Lightning Storm and Dark Servant all carry radiused `Click`
/// abilities and are excluded here, where Dark Servant's 240s lifetime would otherwise bill 24
/// rolls a cast.
///
/// Tornado IS in the class, and it's the one member worth naming because the beta's twin excludes
/// it in prose while including it in code. Both its abilities are authored `Auto` (`Tornado` at 7ft
/// and `Tornado_Fear` at 20ft), so the data calls it a pulser and this predicate follows the data.
/// Nothing has measured it; it's the first candidate if the class is ever tested again.
pub fn proc_patch_duration(def: &Power, db: &PowerDatabase) -> Option<f64> {
    if stat_or_default(def, "radius", 0.0) > 0.0 {
        return None;
    }
    let summon = power_summon(def)?;

    let mut saw_pulse = false;
    for ability in pseudo_pet_abilities(def, db) {
        if ability_footprint(ability).is_none() {
            continue;
        }
        if ability.get("type").and_then(Value::as_str) != Some("Auto") {
            return None;
        }
        saw_pulse = true;
    }
    if !saw_pulse {
        return None;
    }

    summon
        .get("duration")
        .and_then(Value::as_f64)
        .or_else(|| {
            summon
                .get("resolvedEntities")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|entity| entity.get("duration").and_then(Value::as_f64))
        })
        .filter(|duration| *duration > 0.0)
}

/// Rolls a patch of this lifetime gets: one at age 0, then one per period while it's still alive.
/// A tick due exactly at expiry doesn't fire, so a 15s rain at a 10s period gets 2 and a 10s Burn
/// patch gets 1.
///
/// `cycle` (recharge + cast) caps it, which matters only for the patches that outlive their own
/// cooldown (Faraday Cage at 240s, Lifegiving Spores at the 99999s that means "until recast").
/// Those are single-instance: recasting replaces the patch rather than stacking a second one, so a
/// cast is credited only with the rolls that land before the next one. Without the cap
/// Lifegiving Spores would claim ten thousand rolls per cast.
pub fn proc_rolls_in_patch(duration: f64, cycle: f64, period: f64) -> f64 {
    if period <= 0.0 {
        return 1.0;
    }
    let live = if cycle > 0.0 {
        duration.min(cycle)
    } else {
        duration
    };
    (live / period).ceil().max(1.0)
}

/// How many independent PPM rolls one activation gets, and what window each is scored against.
///
/// One place so that every PPM surface reads the same schedule: the Build-Up pass today, the
/// per-power proc rows and proc DPS when M5 lands them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProcRollSchedule {
    /// Seconds ONE roll is scored against, before any recharge slotting.
    pub window: f64,
    /// Cast time added to `window`; 0 when the window is the piece's own period.
    pub cast_time: f64,
    /// Independent rolls per activation. Above 1 only for a patch outliving one period.
    pub rolls: f64,
    /// Whether `window` is the piece's own period rather than the power's recharge, so no amount
    /// of recharge, slotted or global, can move it.
    pub fixed_period: bool,
}

/// The schedule a power's procs actually run on.
///
/// Measured in game 2026-08-05 on Defender Cold Domination Sleet (60s recharge, 2.03s cast, a
/// 15s patch at 20ft) with five 3.5 PPM procs slotted at once, over 26 clean casts (260 trials,
/// 37 firings). Firings landed at exactly two patch ages, 0s and 10s, never a third and never once
/// per 0.2s pulse; 14.2% per roll, Wilson 95% CI 10.5–19.0%. The piece's period WITH the area
/// factor predicts 17.9% (z = −1.56); dropping the area factor predicts 58.3% (z = −14.4); the
/// parent's 60s recharge predicts the 90% ceiling (z = −40.7). Both rivals are dead, and the second
/// of them is what this engine computed until this function existed.
///
/// `period` is the PROC PIECE's own `fActivatePeriod`, never a constant, the same field and the
/// same reason as [`calculate_auto_toggle_proc_chance`] (PPM-1). Every shipped PPM piece authors
/// 10.0 there, so the beta's twin hardcodes a 10; reading the field instead costs nothing and keeps
/// the value where the export owns it. `None` ⇒ the piece states no period, which only a patch host
/// needs to answer, and the caller reports that rather than standing a number in.
///
/// This function is the CLICK path's. An auto/toggle host is already scored on the piece's period
/// by [`calculate_auto_toggle_proc_chance`], so giving the schedule an arm for it would be a second
/// home for a rule that already has one.
fn proc_roll_schedule(
    def: &Power,
    db: &PowerDatabase,
    base_recharge: f64,
    cast_time: f64,
    period: Option<f64>,
) -> Option<ProcRollSchedule> {
    let Some(duration) = proc_patch_duration(def, db) else {
        return Some(ProcRollSchedule {
            window: base_recharge,
            cast_time,
            rolls: 1.0,
            fixed_period: false,
        });
    };
    let period = period?;
    Some(ProcRollSchedule {
        window: period,
        cast_time: 0.0,
        rolls: proc_rolls_in_patch(duration, base_recharge + cast_time, period),
        fixed_period: true,
    })
}

/// The power's PPM geometry as the contract states it, falling back to the summoned patch's
/// footprint when the parent carries none (see [`pseudo_pet_proc_radius`]).
///
/// `procsOnlyOnMainTarget` is emitted sparse-true (the converters write the key only when the
/// binary's `ProcMainTargetOnly` bool is set), so an absent key is the authored `false` and not a
/// dropped read. That's the emitter's own encoding, not a guess at an unstated axis.
fn proc_area(def: &Power, db: &PowerDatabase) -> ProcArea {
    let own_radius = stat_or_default(def, "radius", 0.0);
    // The parent's arc describes the parent's own footprint. A borrowed patch footprint is a
    // sphere and takes no arc from the shell it was summoned by; reading one across would apply a
    // cone the patch doesn't have.
    let (radius, arc_degrees) = match own_radius > 0.0 {
        true => {
            let raw_arc = def_num(def, "stats", "arc");
            // A radiused power with no authored arc is a sphere; `arc_to_degrees` maps the authored
            // 0 to 0, which would read as a degenerate cone, not the full circle the game uses.
            let arc = match arc_to_degrees(raw_arc.unwrap_or(0.0)) {
                0.0 => 360.0,
                degrees => degrees,
            };
            (own_radius, arc)
        }
        false => (pseudo_pet_proc_radius(def, db).unwrap_or(0.0), 360.0),
    };
    ProcArea {
        radius,
        arc_degrees,
        only_main_target: def
            .extra
            .get("procsOnlyOnMainTarget")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

// ============================================================
// Proc Rule of 5 (separate instance from set bonuses).
// ============================================================

/// The proc-tracked stat key for a category (beta `PROC_CATEGORY_TO_STAT`). `None`
/// ⇒ not Rule-of-5 tracked (e.g. KB protection), which includes any category absent
/// from the map.
fn proc_category_stat(category: &str) -> Option<&'static str> {
    match category {
        "Recovery" | "Endurance" => Some("recovery"),
        "Regeneration" | "Heal" => Some("regeneration"),
        "Absorb" => Some("absorb"),
        "Recharge" => Some("recharge"),
        "RunSpeed" => Some("runspeed"),
        "JumpSpeed" => Some("jumpspeed"),
        "FlySpeed" => Some("flyspeed"),
        "JumpHeight" => Some("jumpheight"),
        "Damage" => Some("damage"),
        _ => None,
    }
}

/// The enhancement aspect an always-on movement global scales with, or `None` for a
/// category that applies flat.
///
/// Only the MOVEMENT globals are enhanceable: they're ordinary buffs granted by a
/// slotted piece, so the host power's own movement enhancement multiplies them, where
/// an unresistable global like LotG's +Recharge doesn't scale with anything. Jump
/// enhancement raises jump SPEED and jump HEIGHT alike, so both map to `jump`.
fn movement_enhancement_aspect(category: &str) -> Option<&'static str> {
    match category {
        "RunSpeed" => Some("run"),
        "JumpSpeed" | "JumpHeight" => Some("jump"),
        "FlySpeed" => Some("fly"),
        _ => None,
    }
}

/// Per-stat, per-2dp-value bucket count (cap 5). Reuses the set-bonus `value_key`
/// (the load-bearing JS `toFixed(2)` replica) so proc and set caps bucket identically.
type ProcTracking = BTreeMap<&'static str, BTreeMap<String, u8>>;

/// Record one emitted bonus; returns `true` when it's within the first five for
/// its `(stat, value)` bucket (the beta `trackBonus` return).
fn track_proc_bonus(tracking: &mut ProcTracking, stat: &'static str, value: f64) -> bool {
    let count = tracking
        .entry(stat)
        .or_default()
        .entry(crate::set_bonuses::value_key(value))
        .or_insert(0);
    if *count < 5 {
        *count += 1;
        true
    } else {
        false
    }
}

// ============================================================
// Proc breakdown provenance (the beta `type:'proc'` dashboard sources).
// ============================================================

/// Which proc pass emitted a breakdown source. The mapper formats the label per kind
/// (the beta's `${set}: ${proc}` for always-on, `… (PPM)` for PPM, `… (in ${power})` for
/// Build-Up) and only the always-on kind carries a display power name for the over-cap ring
/// (the beta omits `powerName` on the PPM / Build-Up rows).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcSourceKind {
    AlwaysOn,
    Ppm,
    BuildUp,
}

/// One proc contribution to the dashboard breakdown, a `type:'proc'` source in the beta
/// breakdown map (`applySingleProcEffect` / PPM / Build-Up `addToBreakdown`). The engine emits
/// the facts + the camelCase `breakdown_key`; the mapper resolves the display name and formats
/// the label. `note` carries the beta's Endurance→recovery / Heal→regen reinterpretation tag
/// (`"end"` / `"hp"`, else empty) that becomes the " (+End)" / " (+HP)" label suffix.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ProcBreakdownSource {
    pub breakdown_key: String,
    /// The [`ProcEffect::category`] this contribution came from, the switch in
    /// [`CharacterState::disabled_proc_categories`] that governs it. Carried so a reader (and
    /// the gate) can attribute a row to the control that removes it; several categories share
    /// one `breakdown_key`, so the key can't answer for it.
    pub category: String,
    pub set_name: String,
    pub proc_name: String,
    pub value: f64,
    pub capped: bool,
    pub kind: ProcSourceKind,
    pub note: String,
    pub power_internal_name: String,
    pub power_set: String,
}

/// The identity of the proc piece being applied, borrowed while emitting sources so the
/// per-effect push sites stay cheap.
struct ProcSource<'a> {
    set_name: &'a str,
    proc_name: &'a str,
    power_internal_name: &'a str,
    power_set: &'a str,
}

impl ProcSource<'_> {
    /// Record one breakdown source for this proc under `breakdown_key`.
    #[allow(clippy::too_many_arguments)]
    fn emit(
        &self,
        out: &mut Vec<ProcBreakdownSource>,
        category: &str,
        breakdown_key: &str,
        value: f64,
        capped: bool,
        kind: ProcSourceKind,
        note: &str,
    ) {
        out.push(ProcBreakdownSource {
            breakdown_key: breakdown_key.to_string(),
            category: category.to_string(),
            set_name: self.set_name.to_string(),
            proc_name: self.proc_name.to_string(),
            value,
            capped,
            kind,
            note: note.to_string(),
            power_internal_name: self.power_internal_name.to_string(),
            power_set: self.power_set.to_string(),
        });
    }
}

// ============================================================
// Apply a single structured proc effect into the totals.
// ============================================================

const ALL_DEF_TYPES: [&str; 11] = [
    "melee", "ranged", "aoe", "smashing", "lethal", "fire", "cold", "energy", "negative",
    "psionic", "toxic",
];
const ALL_RES_TYPES: [&str; 8] = [
    "smashing", "lethal", "fire", "cold", "energy", "negative", "psionic", "toxic",
];
/// The six mez types a `MezResist` / `All` proc effect covers. `knockback` is NOT one of
/// them: the converter's all-six warrant (`MEZ_ATTRIBS.issubset`) is over the mez attribs
/// only, so expanding into knockback would claim a resistance the export never stated.
const ALL_MEZ_TYPES: [&str; 6] = ["hold", "stun", "immobilize", "sleep", "confuse", "fear"];

fn def_field<'a>(g: &'a mut GlobalBonuses, key: &str) -> Option<&'a mut f64> {
    Some(match key {
        "melee" => &mut g.defense_melee,
        "ranged" => &mut g.defense_ranged,
        "aoe" | "area" => &mut g.defense_aoe,
        "smashing" => &mut g.defense_smashing,
        "lethal" => &mut g.defense_lethal,
        "fire" => &mut g.defense_fire,
        "cold" => &mut g.defense_cold,
        "energy" => &mut g.defense_energy,
        "negative" => &mut g.defense_negative,
        "psionic" => &mut g.defense_psionic,
        "toxic" => &mut g.defense_toxic,
        _ => return None,
    })
}

fn res_field<'a>(g: &'a mut GlobalBonuses, key: &str) -> Option<&'a mut f64> {
    Some(match key {
        "smashing" => &mut g.resistance_smashing,
        "lethal" => &mut g.resistance_lethal,
        "fire" => &mut g.resistance_fire,
        "cold" => &mut g.resistance_cold,
        "energy" => &mut g.resistance_energy,
        "negative" => &mut g.resistance_negative,
        "psionic" => &mut g.resistance_psionic,
        "toxic" => &mut g.resistance_toxic,
        _ => return None,
    })
}

/// The camelCase dashboard breakdown key for a mez-resistance type, in the spelling
/// [`GlobalBonuses::add_mez_resistance`] accepts. `None` ⇒ not a mez type this calc models.
fn mez_breakdown_key(sub: &str) -> Option<&'static str> {
    Some(match sub {
        "hold" => "mezResistHold",
        "stun" => "mezResistStun",
        "immobilize" => "mezResistImmobilize",
        "sleep" => "mezResistSleep",
        "confuse" => "mezResistConfuse",
        "fear" => "mezResistFear",
        "knockback" => "mezResistKnockback",
        _ => return None,
    })
}

/// The camelCase dashboard breakdown key for a typed defense/resistance sub-key, the beta's
/// `specificDefMap` / `specificResMap` (`applySingleProcEffect`). `defense`/`resistance` picks
/// the family via the two prefixes.
fn typed_breakdown_key(family: &str, sub: &str) -> Option<&'static str> {
    let key = match sub {
        "melee" => "Melee",
        "ranged" => "Ranged",
        "aoe" | "area" => "AoE",
        "smashing" => "Smashing",
        "lethal" => "Lethal",
        "fire" => "Fire",
        "cold" => "Cold",
        "energy" => "Energy",
        "negative" => "Negative",
        "psionic" => "Psionic",
        "toxic" => "Toxic",
        _ => return None,
    };
    Some(match (family, key) {
        ("def", "Melee") => "defMelee",
        ("def", "Ranged") => "defRanged",
        ("def", "AoE") => "defAoE",
        ("def", "Smashing") => "defSmashing",
        ("def", "Lethal") => "defLethal",
        ("def", "Fire") => "defFire",
        ("def", "Cold") => "defCold",
        ("def", "Energy") => "defEnergy",
        ("def", "Negative") => "defNegative",
        ("def", "Psionic") => "defPsionic",
        ("def", "Toxic") => "defToxic",
        ("res", "Smashing") => "resSmashing",
        ("res", "Lethal") => "resLethal",
        ("res", "Fire") => "resFire",
        ("res", "Cold") => "resCold",
        ("res", "Energy") => "resEnergy",
        ("res", "Negative") => "resNegative",
        ("res", "Psionic") => "resPsionic",
        ("res", "Toxic") => "resToxic",
        // Positional res / defense has no matching key, dropped like the beta's map miss.
        _ => return None,
    })
}

/// What the router did with one proc effect. Every effect resolves to exactly one of
/// these: the switch below has no fallthrough, so a category or effect type the data
/// grows becomes a visible error rather than a number that quietly never arrives.
/// The `_ => {}` this enum replaces hid three real drops at once:
/// the `MezResist`/`All` collapse (MEZRES-1), `+Perception` globals landing in the
/// converter's `Special` bucket (PROC-PERCEPTION-1), and any slash-joined defense type
/// `_effect_type_for_defense` can emit. The coverage guard asserts the shipped data
/// never produces [`ProcRoute::Unreadable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcRoute {
    /// Spent into at least one [`GlobalBonuses`] field.
    Spent,
    /// Recognized, and deliberately not spent on the player dashboard. Carries why.
    NotSpent(&'static str),
    /// No arm claimed it. Carries what couldn't be read.
    Unreadable(String),
}

/// Route one proc effect (category + value) into the matching global(s), mirroring
/// the beta `applySingleProcEffect`. Stealth lands in `stealth_contribs` (additive).
/// `src`/`out` emit the `type:'proc'` dashboard breakdown source(s) alongside each apply,
/// exactly where the beta calls `addToBreakdown`.
#[allow(clippy::too_many_arguments)]
fn apply_single_proc_effect(
    category: &str,
    value: f64,
    value_max: Option<f64>,
    effect_type: Option<&str>,
    g: &mut GlobalBonuses,
    stealth_contribs: &mut Vec<StealthContribution>,
    src: &ProcSource,
    out: &mut Vec<ProcBreakdownSource>,
) -> ProcRoute {
    use ProcSourceKind::AlwaysOn;
    match category {
        "Recovery" => {
            g.recovery += value;
            src.emit(out, category, "recovery", value, false, AlwaysOn, "");
        }
        "Endurance" => {
            g.recovery += value;
            src.emit(out, category, "recovery", value, false, AlwaysOn, "end");
        }
        "Regeneration" => {
            g.regeneration += value;
            src.emit(out, category, "regeneration", value, false, AlwaysOn, "");
        }
        "Heal" => {
            g.regeneration += value;
            src.emit(out, category, "regeneration", value, false, AlwaysOn, "hp");
        }
        "MaxHP" => {
            g.max_hp += value;
            src.emit(out, category, "maxHP", value, false, AlwaysOn, "");
        }
        "Absorb" => {
            g.absorb += value;
            src.emit(out, category, "absorb", value, false, AlwaysOn, "");
        }
        "ToHit" => {
            g.to_hit += value;
            src.emit(out, category, "toHit", value, false, AlwaysOn, "");
        }
        "Recharge" => {
            g.recharge += value;
            src.emit(out, category, "recharge", value, false, AlwaysOn, "");
        }
        "Damage" => {
            g.damage += value;
            src.emit(out, category, "damage", value, false, AlwaysOn, "");
        }
        "RunSpeed" => {
            g.run_speed += value;
            src.emit(out, category, "runSpeed", value, false, AlwaysOn, "");
        }
        "JumpSpeed" => {
            g.jump_speed += value;
            src.emit(out, category, "jumpSpeed", value, false, AlwaysOn, "");
        }
        "FlySpeed" => {
            g.fly_speed += value;
            src.emit(out, category, "flySpeed", value, false, AlwaysOn, "");
        }
        "JumpHeight" => {
            g.jump_height += value;
            src.emit(out, category, "jumpHeight", value, false, AlwaysOn, "");
        }
        "MezResist" => {
            // `All` is warranted upstream, not assumed here: extract-proc-data.py emits it
            // only when the effect group carries all six mez attribs
            // (`MEZ_ATTRIBS.issubset`), the same all-six guard the set-bonus converter uses.
            // So this expands what the export states, and reads `All` the way the
            // `Defense` and `Resistance` arms below read their own
            // (DATA-GAP-REGISTER MEZRES-1).
            let et = effect_type.unwrap_or("").to_ascii_lowercase();
            if et == "all" {
                for t in ALL_MEZ_TYPES {
                    route_closed(g.add_mez_resistance(t, value), t);
                    if let Some(k) = mez_breakdown_key(t) {
                        src.emit(out, category, k, value, false, AlwaysOn, "");
                    }
                }
            } else if let Some(k) = mez_breakdown_key(&et) {
                route_closed(g.add_mez_resistance(&et, value), &et);
                src.emit(out, category, k, value, false, AlwaysOn, "");
            } else {
                return ProcRoute::Unreadable(format!("MezResist effect type {et:?}"));
            }
        }
        "SlowResistance" => {
            g.debuff_resist_slow += value;
            src.emit(
                out,
                category,
                "debuffResistSlow",
                value,
                false,
                AlwaysOn,
                "",
            );
        }
        "RechargeResistance" => {
            g.debuff_resist_recharge += value;
            src.emit(
                out,
                category,
                "debuffResistRecharge",
                value,
                false,
                AlwaysOn,
                "",
            );
        }
        "EnduranceDrainResistance" => {
            // One named effect covering both drain axes: the source auto power
            // (Set_Bonus.Challenge_Set_Bonus.Synapses_Agility) carries two
            // same-scale templates, on the Recovery and Endurance attribs, so
            // the category fans out to both debuff resists.
            g.debuff_resist_endurance += value;
            g.debuff_resist_recovery += value;
            src.emit(
                out,
                category,
                "debuffResistEndurance",
                value,
                false,
                AlwaysOn,
                "",
            );
            src.emit(
                out,
                category,
                "debuffResistRecovery",
                value,
                false,
                AlwaysOn,
                "",
            );
        }
        "KnockbackProtection" => {
            g.protection_knockback += value;
            src.emit(out, category, "protKnockback", value, false, AlwaysOn, "");
        }
        "Perception" => {
            // The same % global the power-side applier feeds (`scale × 100` into
            // `perception_radius`); the converter's bridge multiplier is the same
            // 100.0, so `value` lands directly. Rectified Reticle and Warp are the
            // shipping instances (DATA-GAP-REGISTER PROC-PERCEPTION-1).
            g.perception_radius += value;
            src.emit(
                out,
                category,
                "perceptionRadius",
                value,
                false,
                AlwaysOn,
                "",
            );
        }
        "Defense" => {
            let et = effect_type.unwrap_or("").to_ascii_lowercase();
            if et == "all" {
                for t in ALL_DEF_TYPES {
                    if let Some(f) = def_field(g, t) {
                        *f += value;
                    }
                    if let Some(k) = typed_breakdown_key("def", t) {
                        src.emit(out, category, k, value, false, AlwaysOn, "");
                    }
                }
            } else if let Some(f) = def_field(g, &et) {
                *f += value;
                if let Some(k) = typed_breakdown_key("def", &et) {
                    src.emit(out, category, k, value, false, AlwaysOn, "");
                }
            } else {
                // `_effect_type_for_defense` slash-joins a partial vector list
                // ("cold/energy") when a global covers some but not all defense types.
                // No such global ships today; if one does, it must be split upstream.
                return ProcRoute::Unreadable(format!("Defense effect type {et:?}"));
            }
        }
        "Resistance" => {
            let et = effect_type.unwrap_or("").to_ascii_lowercase();
            if et == "all" {
                for t in ALL_RES_TYPES {
                    if let Some(f) = res_field(g, t) {
                        *f += value;
                    }
                    if let Some(k) = typed_breakdown_key("res", t) {
                        src.emit(out, category, k, value, false, AlwaysOn, "");
                    }
                }
            } else if let Some(f) = res_field(g, &et) {
                *f += value;
                if let Some(k) = typed_breakdown_key("res", &et) {
                    src.emit(out, category, k, value, false, AlwaysOn, "");
                }
            } else {
                return ProcRoute::Unreadable(format!("Resistance effect type {et:?}"));
            }
        }
        "Stealth" => {
            // A stealth IO splits PvE/PvP across two effects: `{value}` (PvE) and
            // `{value, valueMax}` with value==valueMax (PvP). Guard the PvE side on
            // `value != valueMax` so the duplicated PvP magnitude never leaks into
            // PvE. Additive group (null stack key).
            let pvp = value_max.unwrap_or(0.0);
            let pve = match value_max {
                None => value,
                Some(vm) => {
                    if value != vm {
                        value
                    } else {
                        0.0
                    }
                }
            };
            stealth_contribs.push(StealthContribution {
                stack_key: None,
                pve,
                pvp,
                // Labelled with the piece, not its power: a stealth IO is what a player would
                // look for in the breakdown, and the same power can hold more than one.
                power_name: src.proc_name.to_string(),
            });
        }
        // Foe-facing payloads. They reach this router only from an always-on entry, and
        // an always-on debuff on the enemy is not a stat on the player's sheet.
        "Control" | "Debuff" => {
            return ProcRoute::NotSpent("foe-facing payload — not a player global")
        }
        // The converter's junk drawer: `_proc_effect_from_bridge` files anything it has no
        // proc category for under `Special` (crit-chance markers, knock-conversion flags,
        // pet-bonus stubs). Since PROC-PERCEPTION-1 moved the `+Perception` globals to their
        // own category, every shipped `Special` is a valueless marker, so this arm is reachable
        // only if data grows a valued `Special`, which `every_shipped_proc_effect_classifies`
        // turns into a decision.
        "Special" => {
            return ProcRoute::NotSpent("converter junk drawer — markers, no player global")
        }
        other => return ProcRoute::Unreadable(format!("proc category {other:?}")),
    }
    ProcRoute::Spent
}

// ============================================================
// The controls: which categories are gateable, and what each slotted piece offers.
// ============================================================

/// One proc effect category the build can switch off, as the proc data spells it.
///
/// The list is ASKED of [`apply_single_proc_effect`] rather than written down: a category is
/// offerable exactly when the router spends it, and what it feeds is the breakdown keys the
/// router emits for it. That's the whole vocabulary, so a category the proc data grows, or
/// a router arm someone adds, reaches the control surface with no second table to update.
///
/// The beta wrote that table by hand (`PROC_CATEGORY_TO_SETTING`, nine buckets over fourteen
/// category names, unknown ⇒ always enabled) against proc data carrying twenty-one. Its
/// "Disable All" therefore left Stealth, Absorb, MaxHP and the always-on +Damage globals
/// contributing, and its `BuildUp` key named a category the data never emits.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcCategory {
    /// The category token, as [`ProcEffect::category`] spells it.
    pub name: String,
    /// The dashboard breakdown keys effects in this category reach, deduped in router order.
    /// Empty for a category that contributes somewhere the breakdown doesn't name: Stealth
    /// resolves through the radius pass, not through a global.
    pub breakdown_keys: Vec<String>,
    /// Set names carrying it, deduped, for the row's example line.
    pub examples: Vec<String>,
    /// How many effects among the BUILD's slotted proc pieces fall under it. Zero means the
    /// switch is real but currently moves nothing, a fact about the build, not about the
    /// control, so it's reported rather than used to hide the row.
    pub slotted: usize,
    /// Whether the build has it switched on.
    pub enabled: bool,
    /// Why this category can't contribute to the player's dashboard at all, in the router's
    /// own words. `Some` ⇒ shown inert, never offered as a switch: a control that can't move a
    /// number reads as a broken one, and dropping the row would hide the reason a slotted piece
    /// shows up nowhere.
    ///
    /// Carries both of the router's non-spending verdicts, and the second is why this is a
    /// reason rather than a flag: [`ProcRoute::NotSpent`] is a category deliberately not on the
    /// player's sheet, while [`ProcRoute::Unreadable`] is one nothing can read, a gap the
    /// surface has to state rather than dress as a switch (Rule 1).
    pub inert_reason: Option<String>,
}

/// The magnitude the probe routes. Any non-zero value works, because the probe reads which keys
/// the router touches, never what it wrote. 1.0 keeps a debug dump legible.
const PROBE_VALUE: f64 = 1.0;

/// Every proc category the dataset ships, with what it feeds and whether this build allows it.
///
/// Two walks with different scopes, and the split is the point: the OFFER comes from the
/// dataset's whole proc database, so the surface is the same for a build with nothing slotted
/// yet, while `slotted` comes from the build's own pieces.
pub fn proc_categories(state: &CharacterState, db: &PowerDatabase) -> Vec<ProcCategory> {
    let mut found: Vec<ProcCategory> = Vec::new();

    for proc in db.procs.iter() {
        for eff in &proc.effects {
            let (route, keys) = probe_route(eff);
            let at = match found.iter().position(|c| c.name == eff.category) {
                Some(at) => at,
                None => {
                    found.push(ProcCategory {
                        name: eff.category.clone(),
                        breakdown_keys: Vec::new(),
                        examples: Vec::new(),
                        slotted: 0,
                        enabled: state.proc_category_enabled(&eff.category),
                        inert_reason: match route {
                            ProcRoute::Spent => None,
                            ProcRoute::NotSpent(why) => Some(why.to_string()),
                            ProcRoute::Unreadable(what) => Some(format!("cannot be read — {what}")),
                        },
                    });
                    found.len() - 1
                }
            };
            let category = &mut found[at];
            for key in keys {
                if !category.breakdown_keys.contains(&key) {
                    category.breakdown_keys.push(key);
                }
            }
            if !category.examples.contains(&proc.set_name) {
                category.examples.push(proc.set_name.clone());
            }
        }
    }

    for (sel, _) in selected_with_def(state, db) {
        for (_, _, io_name, set_name, _) in proc_pieces(&sel.slots) {
            let Some(proc) = db.procs.find(io_name, set_name) else {
                continue;
            };
            for eff in &proc.effects {
                if let Some(category) = found.iter_mut().find(|c| c.name == eff.category) {
                    category.slotted += 1;
                }
            }
        }
    }

    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// Route one effect into scratch state and report the verdict plus the breakdown keys it
/// touched. The accumulator is discarded: this asks the router what it WOULD do, which is the
/// only way the answer can't drift from what it does.
fn probe_route(eff: &ProcEffect) -> (ProcRoute, Vec<String>) {
    let mut scratch = GlobalBonuses::default();
    let mut stealth = Vec::new();
    let mut emitted = Vec::new();
    let route = apply_single_proc_effect(
        &eff.category,
        PROBE_VALUE,
        eff.value_max,
        eff.effect_type.as_deref(),
        &mut scratch,
        &mut stealth,
        &ProcSource {
            set_name: "",
            proc_name: "",
            power_internal_name: "",
            power_set: "",
        },
        &mut emitted,
    );
    let mut keys: Vec<String> = emitted.into_iter().map(|src| src.breakdown_key).collect();
    // Stealth is the one arm that spends somewhere the breakdown doesn't name at apply time:
    // it queues a radius contribution that `resolve_stealth_radius` commits after every source
    // is gathered, so no source is emitted here. Reading the queue keeps "what does this
    // category feed" answerable for it without a category → key table on the side.
    if !stealth.is_empty() {
        keys.extend(
            crate::stealth::STEALTH_RADIUS_KEYS
                .iter()
                .map(|key| (*key).to_string()),
        );
    }
    (route, keys)
}

/// One proc piece slotted in one power, and the controls its own effects call for.
///
/// Flat rather than an enum over control types because [`ProcOverride`] is flat and a piece can
/// carry both kinds at once (Reactive Defenses' scaling +Resistance beside a plain global),
/// while the override that governs them is one record per PIECE, not per effect.
///
/// It carries no claim about what the piece FEEDS, deliberately. The probe behind
/// [`proc_categories`] can say which global the router would spend a category into, and for
/// several pieces that claim would be false: a chance-gated Damage proc routes to `damage` in
/// the router and is filtered out by every pass that could reach it, so a row promising "feeds
/// Damage" would describe an arithmetic nobody runs. What a piece actually contributes is
/// measured off the breakdown the calc emitted for this build, like every other provenance
/// ledger here: a second description of the same arithmetic is free to drift from the first.
#[derive(Debug, Clone, PartialEq)]
pub struct SlottedProc {
    /// The [`CharacterState::proc_overrides`] key this row writes.
    pub key: String,
    pub slot_index: usize,
    pub set_name: String,
    pub io_name: String,
    /// Whether it contributes. `false` removes the piece from every proc pass.
    pub enabled: bool,
    /// Which knob is authoritative: `"auto"`, `"stacks"`, or `"hp"`.
    pub mode: String,
    /// The stack cap, when any effect self-stacks. `Some` ⇒ a stack pin is offerable.
    pub max_stacks: Option<u32>,
    /// The pinned stack count, when `mode == "stacks"`.
    pub stacks: Option<u32>,
    /// Whether any effect is HP-scaling ⇒ a %HP pin is offerable.
    pub scaling: bool,
    /// The pinned %HP, when `mode == "hp"`.
    pub hp_pct: Option<f64>,
    /// Categories among its effects that are being withheld by
    /// [`CharacterState::disabled_proc_categories`], a piece switched off from the OTHER
    /// control, two surfaces away, which a row has to say or it reads as broken.
    ///
    /// Empty whenever this piece carries an override of its own, because then it doesn't
    /// answer to the category switch at all: the finer control wins, in both directions. A row
    /// that reported the category anyway would be telling a contributing piece it was silenced.
    pub disabled_categories: Vec<String>,
    /// The proc data's one-line account of what the piece does, verbatim. Display only.
    pub mechanics: Option<String>,
    /// How often the piece fires in THIS power, by the same formula the totals score it with.
    pub roll: ProcRoll,
    /// The foe damage it deals, for a damage proc.
    pub damage: Option<ProcDamage>,
}

/// How often a slotted piece fires in the power holding it.
///
/// Every chance here comes off the same helpers the proc passes score with, so a row cannot
/// state a chance the totals did not use. The window is the power's base recharge over its own
/// slotted recharge only: global recharge (set bonuses, Hasten) never enters a proc's chance.
#[derive(Debug, Clone, PartialEq)]
pub enum ProcRoll {
    /// Granted by slotting it (`Global`, `Proc120s`); nothing is rolled.
    AlwaysOn,
    /// A click power: `chance` per roll, `rolls` independent rolls per activation. `rolls` is
    /// above 1 only for a summoned patch outliving one of the piece's periods.
    PerActivation {
        ppm: f64,
        chance: f64,
        rolls: f64,
        /// The window is the piece's own period, so recharge of any kind cannot move it.
        fixed_period: bool,
        working: ChanceWorking,
    },
    /// An auto or toggle power: `chance` per check, one check every `period` seconds.
    PerCheck { ppm: f64, chance: f64, period: f64 },
    /// The power is flagged `ProcAllowed: none`, so the piece never rolls against it. With
    /// `via_pet` the summon copies its slotting, and the piece fires off the pet instead.
    Never { via_pet: bool },
    /// The proc data names no PPM for this piece, so there is no chance to state.
    Unrated,
    /// The engine could not score it; the text says why.
    Unknown(String),
}

/// The terms a click power's chance was scored from, so a reader can redo the arithmetic.
///
/// `chance = ppm × (window + cast_time) / (60 × area_factor)`, then clamped.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChanceWorking {
    /// What the window starts from: the power's base recharge, or the piece's own period when
    /// the roll is on a fixed period.
    pub start: f64,
    /// The recharge strength the window was divided by (0.95 = +95%), as the divisor applied it:
    /// zero for a fixed period or a power that takes no recharge strength.
    pub slotted_recharge: f64,
    /// `start / (1 + slotted_recharge)`.
    pub window: f64,
    pub cast_time: f64,
    /// The AoE denominator; 1.0 for single target.
    pub area_factor: f64,
}

/// Score one slotted piece in its host power. See [`ProcRoll`].
#[allow(clippy::too_many_arguments)]
fn proc_roll(
    state: &CharacterState,
    db: &PowerDatabase,
    sel: &SelectedPower,
    def: &Power,
    proc: &ProcData,
    set_name: &str,
    io_name: &str,
    alpha: &AlphaEnhancement,
    recharge_bounds: Option<StrengthBounds>,
) -> ProcRoll {
    if proc.is_always_on() {
        return ProcRoll::AlwaysOn;
    }
    let Some(ppm) = proc.ppm else {
        return ProcRoll::Unrated;
    };
    let site = match proc_fires_in(def, proc, set_name, io_name) {
        Err(e) => return ProcRoll::Unknown(e.detail.to_string()),
        Ok(None) => {
            let via_pet = power_summon(def)
                .and_then(|summon| summon.get("copyBoosts"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            return ProcRoll::Never { via_pet };
        }
        Ok(Some(site)) => site,
    };
    let ptype = power_type_lower(def);
    if ptype == "auto" || ptype == "toggle" {
        // The auto/toggle pass scores on single-target geometry; see `apply_ppm_procs`.
        let Some(period) = proc.activate_period else {
            return ProcRoll::Unknown("no activate period in the proc data".to_string());
        };
        return ProcRoll::PerCheck {
            ppm,
            chance: calculate_auto_toggle_proc_chance(ppm, period, ProcArea::single_target()),
            period,
        };
    }
    let base_recharge = stat_or_default(def, "recharge", 4.0);
    let cast_time = stat_or_default(def, "castTime", 1.0);
    let area = roll_area(site.as_ref(), proc_area(def, db));
    let Some(schedule) =
        proc_roll_schedule(def, db, base_recharge, cast_time, proc.activate_period)
    else {
        return ProcRoll::Unknown(
            "no activate period in the proc data, and this power rolls on its summoned patch's \
             clock"
                .to_string(),
        );
    };
    let window = match schedule.fixed_period {
        true => schedule.window,
        false => {
            // Errors are dropped HERE, not lost: the same slots go through the same call in the
            // totals pass, which reports them. Failing the row on one would make it disagree
            // with the chance the totals scored, which carries on past them.
            let slotted_recharge = crate::apply::power_enhancement(
                &sel.slots,
                def,
                alpha,
                state.level as i32,
                &state.combat,
                db,
                &mut Vec::new(),
            )
            .get("recharge");
            proc_recharge_window(def, schedule.window, slotted_recharge, recharge_bounds)
        }
    };
    ProcRoll::PerActivation {
        ppm,
        chance: calculate_proc_chance(ppm, window, schedule.cast_time, area),
        rolls: schedule.rolls,
        fixed_period: schedule.fixed_period,
        working: ChanceWorking {
            start: schedule.window,
            slotted_recharge: match window > 0.0 {
                true => schedule.window / window - 1.0,
                false => 0.0,
            },
            window,
            cast_time: schedule.cast_time,
            area_factor: ppm_area_denominator(area),
        },
    }
}

/// What a foe-damage proc adds to the power holding it.
///
/// Proc damage is flat: no damage enhancement and no damage buff moves it, which is why it rides
/// beside the power's own damage rather than through [`crate::damage`].
#[derive(Debug, Clone, PartialEq)]
pub struct ProcDamage {
    pub damage_type: Option<String>,
    /// One proc's hit at the character's level.
    pub per_hit: f64,
    /// `per_hit × chance × rolls`, the average one activation gains. `None` where no
    /// per-activation figure exists: an auto or toggle host rolls on a clock, not on a cast,
    /// and an unrated or unscoreable piece has no chance to multiply by. Zero for a piece that
    /// never fires in this power.
    pub per_cast: Option<f64>,
}

/// The proc damage table. Every archetype ships the same one; the build's own is read anyway,
/// because that is the table the game resolves the proc's template against.
const PROC_DAMAGE_TABLE: &str = "Melee_ProcDamage";

/// The foe damage one proc hit deals at `level`, or `None` for a piece that deals none.
///
/// The proc data carries the damage at levels 1 and 50 (`value`, `value_max`): the template's
/// scale times the proc damage table at both ends. Between them it follows the table's own curve
/// rather than a straight line, and it reads the CHARACTER's level, never the piece's craft level.
/// A Build Up proc's `Damage` row is a self buff with a duration and no `value_max`, so it is not
/// one of these.
fn proc_hit_damage(
    proc: &ProcData,
    db: &PowerDatabase,
    archetype: &str,
    level: i32,
) -> Option<(f64, Option<String>)> {
    let eff = proc.effects.iter().find(|eff| {
        eff.category == "Damage" && eff.duration.is_none() && eff.value_max.is_some()
    })?;
    let (low, high) = (eff.value?, eff.value_max?);
    let table = |at: i32| {
        db.at_tables
            .get_table_value(archetype, PROC_DAMAGE_TABLE, at)
    };
    let damage = match (table(1), table(50), table(level)) {
        (Some(t1), Some(t50), Some(here)) if t50 != t1 => {
            let along = ((here - t1) / (t50 - t1)).clamp(0.0, 1.0);
            low + (high - low) * along
        }
        // No archetype yet, or a table that cannot place the level: the level-50 figure, which
        // is what every piece's own tooltip leads with.
        _ => high,
    };
    Some((damage, eff.effect_type.clone()))
}

/// The average proc damage one activation of a power gains from the pieces given — the ones
/// switched on, whose `Damage` category is not withheld in Proc settings.
pub fn proc_damage_per_cast(procs: &[SlottedProc]) -> f64 {
    procs
        .iter()
        .filter(|proc| proc.enabled && !proc.disabled_categories.iter().any(|c| c == "Damage"))
        .filter_map(|proc| proc.damage.as_ref()?.per_cast)
        .sum()
}

/// The proc pieces slotted in one power, in slot order.
///
/// Addressed by owning set plus internal name, not by internal name alone: `Build_Up` names
/// sixty-four different powers, so the set id is half the address. Empty for a power the build
/// doesn't hold: there's no slotting to control.
pub fn slotted_procs(
    state: &CharacterState,
    db: &PowerDatabase,
    powerset: &str,
    internal_name: &str,
) -> Vec<SlottedProc> {
    let Some(sel) = state
        .all_selected()
        .find(|sel| sel.powerset == powerset && sel.internal_name == internal_name)
    else {
        return Vec::new();
    };
    let Some(def) = gather::resolve_power(db, powerset, internal_name) else {
        return Vec::new();
    };
    let alpha =
        crate::incarnates::alpha_enhancement(&state.incarnates, state.combat.exemplar_level, db);
    let recharge_bounds = ReductionClamps::for_build(state, db).recharge;
    let archetype = state.archetype.id.as_deref().unwrap_or("");
    let level = state.level as i32;

    let mut rows = Vec::new();
    for (slot_index, _, io_name, set_name, is_proc) in proc_pieces(&sel.slots) {
        let Some(proc) = db.procs.find(io_name, set_name) else {
            continue;
        };
        if !is_proc && !looks_like_legacy_proc_slot(io_name, proc) {
            continue;
        }
        let over = proc_override(state, sel, slot_index);
        let mut disabled_categories: Vec<String> = Vec::new();
        if over.is_none() {
            for eff in &proc.effects {
                if !state.proc_category_enabled(&eff.category)
                    && !disabled_categories.contains(&eff.category)
                {
                    disabled_categories.push(eff.category.clone());
                }
            }
        }
        let roll = proc_roll(
            state,
            db,
            sel,
            def,
            proc,
            set_name,
            io_name,
            &alpha,
            recharge_bounds,
        );
        let damage = proc_hit_damage(proc, db, archetype, level).map(|(per_hit, damage_type)| {
            let per_cast = match &roll {
                ProcRoll::PerActivation { chance, rolls, .. } => Some(per_hit * chance * rolls),
                ProcRoll::Never { .. } => Some(0.0),
                _ => None,
            };
            ProcDamage {
                damage_type,
                per_hit,
                per_cast,
            }
        });
        rows.push(SlottedProc {
            key: proc_override_key(powerset, internal_name, slot_index),
            slot_index,
            set_name: set_name.to_string(),
            io_name: io_name.to_string(),
            enabled: over.is_none_or(|o| o.enabled),
            mode: over.map_or("auto", |o| o.mode.as_str()).to_string(),
            max_stacks: proc.effects.iter().find_map(|eff| eff.max_stacks),
            stacks: over.and_then(|o| o.stacks),
            scaling: proc.effects.iter().any(|eff| eff.scaling),
            hp_pct: over.and_then(|o| o.hp_pct),
            disabled_categories,
            mechanics: proc.mechanics.clone(),
            roll,
            damage,
        });
    }
    rows
}

// ============================================================
// Build iteration helper.
// ============================================================

/// `(selection, def)` for every selected power whose def resolves in this dataset.
/// Unresolvable powers are skipped here; the gather pass surfaces them.
fn selected_with_def<'a>(
    state: &'a CharacterState,
    db: &'a PowerDatabase,
) -> impl Iterator<Item = (&'a SelectedPower, &'a Power)> {
    state.all_selected().filter_map(move |sel| {
        gather::resolve_power(db, &sel.powerset, &sel.internal_name).map(|def| (sel, def))
    })
}

/// The IO-set proc pieces slotted in a power: `(slot_index, enhancement, io_name,
/// set_name, is_proc)`.
fn proc_pieces(
    slots: &[Option<Enhancement>],
) -> impl Iterator<Item = (usize, &Enhancement, &str, &str, bool)> {
    slots.iter().enumerate().filter_map(|(i, slot)| {
        let enh = slot.as_ref()?;
        let EnhancementKind::IoSet {
            set_name, is_proc, ..
        } = &enh.kind
        else {
            return None;
        };
        Some((i, enh, enh.name.as_str(), set_name.as_str(), *is_proc))
    })
}

/// The beta legacy safety net: accept a `proc:false` piece as a real proc when its
/// slot name still clearly identifies the proc entry (`looksLikeLegacyProcSlot`).
///
/// The beta matched two further names here — the placeholders its extractor emitted
/// when it could not derive a proc's effect. A piece is named from its boost power
/// now, so no piece carries either string, and an arm keyed on a name the data does
/// not contain can only ever fire on something it was not meant for.
fn looks_like_legacy_proc_slot(slot_name: &str, proc: &ProcData) -> bool {
    let slot = slot_name.to_ascii_lowercase();
    let io = proc.io_name.to_ascii_lowercase();
    if slot.is_empty() || io.is_empty() {
        return false;
    }
    slot == io || slot.contains(&io)
}

// ============================================================
// Pass entry.
// ============================================================

/// Apply the always-on/PPM, Proc120 and variable proc contributions into `g`, three of the
/// beta's four proc passes, in order. Stealth radii accumulate into `stealth_contribs` for the
/// later resolve. Both proc switches ride in on `state`, so no pass takes them as an argument.
///
/// The Build-Up pass is NOT here. It's [`apply_build_up_procs`], called separately and later,
/// because its click-proc window reads the build's FINAL global recharge (PPM-2) and the
/// incarnate pass still had recharge to add when this one runs. A caller that wants the whole
/// proc contribution has to invoke both.
pub fn apply_procs(
    state: &CharacterState,
    db: &PowerDatabase,
    g: &mut GlobalBonuses,
    stealth_contribs: &mut Vec<StealthContribution>,
    alpha: &AlphaEnhancement,
    errors: &mut Vec<CalcError>,
) -> Vec<ProcBreakdownSource> {
    let mut breakdown = Vec::new();
    apply_always_on_and_ppm(
        state,
        db,
        g,
        stealth_contribs,
        &mut breakdown,
        alpha,
        errors,
    );
    apply_variable_procs(state, db, g, stealth_contribs, &mut breakdown, errors);
    breakdown
}

/// Always-on globals + Proc120s (Rule-of-5 tracked), then the PPM procs in
/// auto/toggle powers. Beta `applyProcBonuses` + `applyPPMProcBonuses`.
#[allow(clippy::too_many_arguments)]
fn apply_always_on_and_ppm(
    state: &CharacterState,
    db: &PowerDatabase,
    g: &mut GlobalBonuses,
    stealth_contribs: &mut Vec<StealthContribution>,
    breakdown: &mut Vec<ProcBreakdownSource>,
    alpha: &AlphaEnhancement,
    errors: &mut Vec<CalcError>,
) {
    let mut tracking: ProcTracking = BTreeMap::new();

    for (sel, def) in selected_with_def(state, db) {
        let ptype = power_type_lower(def);
        let is_always_active = ptype == "auto" || (ptype == "toggle" && sel.is_active);

        for (slot_index, _, io_name, set_name, is_proc) in proc_pieces(&sel.slots) {
            let Some(proc) = db.procs.find(io_name, set_name) else {
                continue;
            };
            if !is_proc && !looks_like_legacy_proc_slot(io_name, proc) {
                continue;
            }
            // A proc the user switched off contributes nothing on ANY path: the beta gates the
            // always-on pass on the same override as the variable one.
            let over = proc_override(state, sel, slot_index);
            if over.is_some_and(|o| !o.enabled) {
                continue;
            }
            // `is_always_on` is `Global | Proc120s`, so the host's `procsAllowed` never applies
            // here (see [`proc_fires_in`]). These are granted by the piece being slotted, not by
            // the power firing, which is what that flag speaks about.
            if !proc.is_always_on() {
                continue;
            }
            // Global procs apply regardless of host activity; Proc120s need an
            // active auto/toggle host.
            if !(proc.proc_type == ProcType::Global || is_always_active) {
                continue;
            }
            let source = ProcSource {
                set_name,
                proc_name: io_name,
                power_internal_name: &sel.internal_name,
                power_set: &sel.powerset,
            };

            for eff in &proc.effects {
                // Variable procs (stacks / HP-scaling) are owned by
                // apply_variable_procs, so skip here to avoid double counting.
                if control_type(eff) != ControlType::Toggle {
                    continue;
                }
                // The piece's own switch wins where the build set one; the category switch is
                // the default for a piece the build has never touched.
                if over.is_none() && !state.proc_category_enabled(&eff.category) {
                    continue;
                }
                let Some(value) = eff.value else { continue };
                // Skip pet/ally buffs and chance-gated procs: no steady player stat.
                if eff.target.as_deref() == Some("pets") {
                    continue;
                }
                if eff.chance.is_some_and(|c| c < 1.0) {
                    continue;
                }
                // Movement globals are ENHANCEABLE, unlike every other always-on
                // global, which applies at face value. Thrust's +Run Speed scales with
                // the Run enhancement in its own slotting power ("will not suppress in
                // combat and can be enhanced"): 35% base with 26.5% Run slotted in the
                // same power reads 44.28% in game. Rule-of-5 tracks the ENHANCED value,
                // since that's what lands on the dashboard.
                let value = match movement_enhancement_aspect(&eff.category) {
                    Some(aspect) => {
                        let enh = crate::apply::power_enhancement(
                            &sel.slots,
                            def,
                            alpha,
                            state.level as i32,
                            &state.combat,
                            db,
                            errors,
                        );
                        value * (1.0 + enh.get(aspect))
                    }
                    None => value,
                };
                let allowed = match proc_category_stat(&eff.category) {
                    None => true, // not Rule-of-5 tracked (e.g. KB protection)
                    Some(stat) => track_proc_bonus(&mut tracking, stat, value),
                };
                if allowed {
                    let route = apply_single_proc_effect(
                        &eff.category,
                        value,
                        eff.value_max,
                        eff.effect_type.as_deref(),
                        g,
                        stealth_contribs,
                        &source,
                        breakdown,
                    );
                    if let ProcRoute::Unreadable(what) = route {
                        errors.push(CalcError::new(
                            "Proc",
                            format!("{set_name} \"{io_name}\": {what}"),
                        ));
                    }
                } else if let Some(stat) = proc_category_stat(&eff.category) {
                    // Rule-of-5 rejected: a capped breakdown row so the tooltip and over-cap
                    // ring show it (the beta's `else if (stat)` branch). The key is the tracked
                    // stat itself (matching the beta, incl. its lowercase `runspeed`).
                    source.emit(
                        breakdown,
                        &eff.category,
                        stat,
                        value,
                        true,
                        ProcSourceKind::AlwaysOn,
                        "",
                    );
                }
            }
        }
    }

    apply_ppm_procs(state, db, g, breakdown, errors);
}

/// PPM procs in active auto/toggle powers → recovery/regeneration (beta
/// `applyPPMProcBonuses`). The pet-power redirection gap (memory
/// `proc-petpower-gap`) lives in DAMAGE-proc DPS, not this recovery/regen path.
fn apply_ppm_procs(
    state: &CharacterState,
    db: &PowerDatabase,
    g: &mut GlobalBonuses,
    breakdown: &mut Vec<ProcBreakdownSource>,
    errors: &mut Vec<CalcError>,
) {
    for (sel, def) in selected_with_def(state, db) {
        let ptype = power_type_lower(def);
        let is_auto_or_toggle = ptype == "auto" || ptype == "toggle";
        let is_active = ptype == "auto" || (ptype == "toggle" && sel.is_active);
        if !is_auto_or_toggle || !is_active {
            continue;
        }

        for (slot_index, _enh, io_name, set_name, is_proc) in proc_pieces(&sel.slots) {
            if !is_proc {
                continue;
            }
            let Some(proc) = db.procs.find(io_name, set_name) else {
                continue;
            };
            let (ProcType::Proc, Some(ppm)) = (proc.proc_type, proc.ppm) else {
                continue;
            };
            // A site changes nothing here: an auto/toggle host rolls on the PIECE's own
            // activate period, not on any recharge, so the only question this pass asks the
            // routing is whether the piece fires at all. No delegating power is an auto or
            // a toggle today, so it never answers yes.
            match proc_fires_in(def, proc, set_name, io_name) {
                Err(e) => {
                    errors.push(e);
                    continue;
                }
                Ok(None) => continue,
                Ok(Some(_)) => {}
            }
            // The piece's own switch reaches this pass too. The beta gates only the always-on
            // and variable passes on it, so a Performance Shifter switched off in its slot went
            // on feeding recovery from here: invisible while nothing wrote an override, and a
            // control that half-works the moment one does.
            let over = proc_override(state, sel, slot_index);
            if over.is_some_and(|o| !o.enabled) {
                continue;
            }
            // The piece's own ActivatePeriod sets both the per-check chance and the check
            // rate. Without it there's no auto/toggle proc model, only a guess, so the
            // piece is reported and skipped rather than run against a stood-in number.
            let Some(activate_period) = proc.activate_period else {
                errors.push(CalcError::new(
                    "Proc",
                    format!("{set_name} \"{io_name}\": no activate period in the proc data"),
                ));
                continue;
            };
            // Single-target geometry regardless of the toggle's own radius, which is the
            // oracle's shape (`calculateAutoToggleProcsPerMinute` takes `radius = 0` by
            // default and this pass passes none). Not the same claim as PPM-3's flag: that
            // one says the game charges no area penalty, this one says the beta never asked.
            let procs_per_min = calculate_auto_toggle_procs_per_minute(
                ppm,
                activate_period,
                ProcArea::single_target(),
            );
            let source = ProcSource {
                set_name,
                proc_name: io_name,
                power_internal_name: &sel.internal_name,
                power_set: &sel.powerset,
            };

            for eff in &proc.effects {
                let Some(value) = eff.value else { continue };
                if over.is_none() && !state.proc_category_enabled(&eff.category) {
                    continue;
                }
                match eff.category.as_str() {
                    "Endurance" => {
                        let end_per_sec = (value * procs_per_min) / 60.0;
                        let recovery_pct = (end_per_sec / BASE_RECOVERY_RATE) * 100.0;
                        g.recovery += recovery_pct;
                        source.emit(
                            breakdown,
                            &eff.category,
                            "recovery",
                            recovery_pct,
                            false,
                            ProcSourceKind::Ppm,
                            "",
                        );
                    }
                    // PPM Heal grants are one-shot HP, NOT steady regen: the game
                    // excludes them from Combat Attributes. Skip (beta).
                    "Heal" => {}
                    "Recovery" => {
                        let recovery_val = (value * procs_per_min) / 60.0;
                        let recovery_pct = (recovery_val / BASE_RECOVERY_RATE) * 100.0;
                        g.recovery += recovery_pct;
                        source.emit(
                            breakdown,
                            &eff.category,
                            "recovery",
                            recovery_pct,
                            false,
                            ProcSourceKind::Ppm,
                            "",
                        );
                    }
                    "Regeneration" => {
                        let regen_pct = if eff.duration.is_some_and(|d| d > 0.0) {
                            // Stacking duration regen buff: value is per-stack %, not
                            // a one-shot grant. Steady-state ≈ per-stack × avg
                            // concurrent stacks (Little's law: arrivalRate × lifetime).
                            let avg_stacks = (procs_per_min / 60.0) * eff.duration.unwrap();
                            value * avg_stacks
                        } else {
                            let regen_val = (value * procs_per_min) / 60.0;
                            (regen_val / BASE_REGEN_RATE) * 100.0
                        };
                        g.regeneration += regen_pct;
                        source.emit(
                            breakdown,
                            &eff.category,
                            "regeneration",
                            regen_pct,
                            false,
                            ProcSourceKind::Ppm,
                            "",
                        );
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Variable procs: self-stacking buffs (Might of the Tanker) and HP-scaling
/// globals (Reactive Defenses). Beta `applyVariableProcBonuses`, default (auto)
/// mode. A non-Global variable proc only contributes when its host is in use:
/// auto always; toggle only while on; a click attack is assumed in-rotation.
fn apply_variable_procs(
    state: &CharacterState,
    db: &PowerDatabase,
    g: &mut GlobalBonuses,
    stealth_contribs: &mut Vec<StealthContribution>,
    breakdown: &mut Vec<ProcBreakdownSource>,
    errors: &mut Vec<CalcError>,
) {
    let archetype = state.archetype.id.as_deref().unwrap_or("");
    let level = i32::from(state.level);

    let resolve_magnitude = |raw: Option<f64>, scale_table: Option<&str>| -> f64 {
        let Some(raw) = raw else { return 0.0 };
        match scale_table {
            None => raw,
            Some(table) => {
                raw * db
                    .at_tables
                    .get_table_value(archetype, table, level)
                    .unwrap_or(0.0)
            }
        }
    };

    for (sel, def) in selected_with_def(state, db) {
        // A click host is in-rotation (never suppressed); a toggle host is
        // suppressed only while off; auto is always on. Global scaling procs are
        // always on regardless.
        let ptype = power_type_lower(def);
        let host_suppressed = ptype == "toggle" && !sel.is_active;

        for (slot_index, _enh, io_name, set_name, is_proc) in proc_pieces(&sel.slots) {
            if !is_proc {
                continue;
            }
            let Some(proc) = db.procs.find(io_name, set_name) else {
                continue;
            };
            // Two shipped pieces are `ProcType::Proc` with a stack control, the +Res(All)
            // chances, and those stacks are the proc firing, so the host's flag reaches them.
            // A site changes only WHETHER it fires: the contribution is read off the stack
            // count, and no window enters it.
            match proc_fires_in(def, proc, set_name, io_name) {
                Err(e) => {
                    errors.push(e);
                    continue;
                }
                Ok(None) => continue,
                Ok(Some(_)) => {}
            }

            for eff in &proc.effects {
                let control = control_type(eff);
                if control == ControlType::Toggle {
                    continue; // handled by the always-on pass
                }
                if proc.proc_type != ProcType::Global && host_suppressed {
                    continue;
                }
                let over = proc_override(state, sel, slot_index);
                if over.is_none() && !state.proc_category_enabled(&eff.category) {
                    continue;
                }
                let per_unit = resolve_magnitude(eff.value, eff.scale_table.as_deref());
                let cap_value = eff
                    .value_max
                    .map(|max| resolve_magnitude(Some(max), eff.scale_table.as_deref()));
                let contribution =
                    resolve_proc_contribution(control, per_unit, cap_value, eff.max_stacks, over);
                if contribution == 0.0 {
                    continue;
                }
                let source = ProcSource {
                    set_name,
                    proc_name: io_name,
                    power_internal_name: &sel.internal_name,
                    power_set: &sel.powerset,
                };
                let route = apply_single_proc_effect(
                    &eff.category,
                    contribution,
                    None,
                    eff.effect_type.as_deref(),
                    g,
                    stealth_contribs,
                    &source,
                    breakdown,
                );
                if let ProcRoute::Unreadable(what) = route {
                    errors.push(CalcError::new(
                        "Proc",
                        format!("{set_name} \"{io_name}\": {what}"),
                    ));
                }
            }
        }
    }
}

/// The Build-Up proc that won the single contribution, and the two categories its halves came
/// from. The winner is decided in the loop and emitted after it, so its identity has to
/// outlive the borrow of the selection it was found in.
struct BuildUpWinner {
    set_name: String,
    proc_name: String,
    power_internal_name: String,
    power_set: String,
    damage_category: String,
    /// Whether the winner's damage half is switched on. Which proc is best is the proc's own
    /// merit, not a display choice, so ranking is by expected damage either way and a
    /// suppressed damage half still decides who carries the ToHit one.
    damage_open: bool,
    /// Empty when the winner brought no ToHit half, which is also when nothing emits under it.
    to_hit_category: String,
}

/// Averaged Build-Up procs (Decimation, Gaussian's) in active click powers → the
/// single best expected +Damage/+ToHit contribution (the buff doesn't self-stack).
/// Beta `applyBuildUpProcBonuses`.
///
/// Split out of [`apply_procs`] and run late (after the incarnate pass and the what-if layer)
/// because it used to be the one proc pass that read the accumulator: `g.recharge` was the
/// `global` term of [`proc_recharge_window`], and Destiny Ageless would have been missing from it
/// at the proc pass's own position. **That reason is retired** — the window no longer reads global
/// recharge at all, so this pass no longer depends on where it sits. The position is kept because
/// everything it writes is an additive `+=` into damage/toHit and its breakdown rows already came
/// last, so moving it back would be churn with nothing to gain.
pub fn apply_build_up_procs(
    state: &CharacterState,
    db: &PowerDatabase,
    g: &mut GlobalBonuses,
    alpha: &AlphaEnhancement,
    errors: &mut Vec<CalcError>,
) -> Vec<ProcBreakdownSource> {
    let mut breakdown = Vec::new();
    let recharge_bounds = ReductionClamps::for_build(state, db).recharge;
    let mut best_damage = 0.0_f64;
    let mut best_to_hit = 0.0_f64;
    // The winning proc's identity, for the two breakdown rows (the beta's single best
    // Build-Up contribution). Owned strings, since the borrow of `sel` ends each loop.
    let mut best_source: Option<BuildUpWinner> = None;

    for (sel, def) in selected_with_def(state, db) {
        let ptype = power_type_lower(def);
        // Build-Up procs fire only in active click powers.
        if ptype == "auto" || ptype == "toggle" || !sel.is_active {
            continue;
        }
        let base_recharge = stat_or_default(def, "recharge", 4.0);
        let cast_time = stat_or_default(def, "castTime", 1.0);
        let area = proc_area(def, db);

        for (slot_index, _enh, io_name, set_name, is_proc) in proc_pieces(&sel.slots) {
            if !is_proc {
                continue;
            }
            let Some(proc) = db.procs.find(io_name, set_name) else {
                continue;
            };
            let (ProcType::Proc, Some(ppm)) = (proc.proc_type, proc.ppm) else {
                continue;
            };
            // The one pass that scores against a window, so a routed piece takes the site's
            // GEOMETRY here. The window is still this power's.
            let site = match proc_fires_in(def, proc, set_name, io_name) {
                Err(e) => {
                    errors.push(e);
                    continue;
                }
                Ok(None) => continue,
                Ok(Some(site)) => site,
            };
            // Reached through the same two switches as the other three passes. The beta gates
            // this pass on a `buildUp` setting key instead, a bucket naming a category its
            // proc data never emits, so the pass answered to a control nothing else did.
            let over = proc_override(state, sel, slot_index);
            if over.is_some_and(|o| !o.enabled) {
                continue;
            }
            let category_open =
                |eff: &ProcEffect| over.is_some() || state.proc_category_enabled(&eff.category);
            // A Build-Up proc is a self-buff Damage effect WITH a duration (a plain
            // damage proc carries value..valueMax and no duration), plus a ToHit buff.
            // SELF-buff is the operative word: Soulbound Allegiance is a pet-set piece
            // slotted in a summon power, so the boost is copied to the pet and its Build
            // Up lands on the PET (`target: "pets"`). This pass was the one proc pass
            // with no target filter, so that pet buff reached the player's dashboard as
            // a flat +90% damage (the PPM cap, off a 240s summon recharge).
            let is_self = |e: &ProcEffect| matches!(e.target.as_deref(), None | Some("self"));
            // The damage effect IDENTIFIES a Build-Up proc, so it's found unconditionally and
            // gated at its contribution instead. Gating the find would make switching off
            // Damage procs also drop Gaussian's +ToHit, which is not a damage contribution.
            // A switch has to remove exactly what it names.
            let Some(dmg) = proc
                .effects
                .iter()
                .find(|e| e.category == "Damage" && e.duration.is_some() && is_self(e))
            else {
                continue;
            };
            let to_hit = proc
                .effects
                .iter()
                .find(|e| e.category == "ToHit" && is_self(e) && category_open(e));

            // Resolved here rather than per power: the ED aggregation is only wanted for a
            // power that actually hosts a Build-Up proc, which is a handful in any build.
            let slotted_recharge = crate::apply::power_enhancement(
                &sel.slots,
                def,
                alpha,
                state.level as i32,
                &state.combat,
                db,
                errors,
            )
            .get("recharge");
            // Whether this power's procs roll on its recharge AT ALL is the schedule's call: a
            // summoned patch owns its own clock and its parent's recharge never enters
            // (PROC-PATCH-1). A site replaces the GEOMETRY only, since the shell's is not the
            // child's (Fault's shell is radius 0, its cone child 20ft at 55°). The window stays
            // this power's own recharge and cast: the roll is scored against the click the player
            // pressed, measured 2026-08-09 on two powers whose children recharge nothing like
            // their parents. No site summons a patch, so the patch branch stays the shell's own.
            let area = roll_area(site.as_ref(), area);
            let schedule =
                proc_roll_schedule(def, db, base_recharge, cast_time, proc.activate_period);
            let Some(schedule) = schedule else {
                errors.push(CalcError::new(
                    "Proc",
                    format!(
                        "{set_name} \"{io_name}\": no activate period in the proc data, and \
                         {} rolls on its summoned patch's clock",
                        sel.internal_name
                    ),
                ));
                continue;
            };
            // A patch that outlives one period buys several independent rolls. What two rolls of a
            // DURATION buff come to is not a thing this pass knows, since a second Build Up inside
            // one cast neither doubles the buff nor is a second independent chance at it, so the
            // case is reported rather than averaged into a plausible number. No shipped power
            // reaches it: of the patch hosts that can slot a Build-Up proc (0 Homecoming, 5
            // Rebirth, 5 Thunderspy) every one lives a single period or less.
            if schedule.rolls > 1.0 {
                errors.push(CalcError::new(
                    "Proc",
                    format!(
                        "{set_name} \"{io_name}\" in {}: a Build-Up proc rolling {} times per cast \
                         has no modelled combination",
                        sel.internal_name, schedule.rolls
                    ),
                ));
                continue;
            }
            let window = match schedule.fixed_period {
                // Passing the recharge terms through a fixed-period window would reproduce the
                // exact bug the schedule exists to fix, one level down.
                true => schedule.window,
                false => {
                    proc_recharge_window(def, schedule.window, slotted_recharge, recharge_bounds)
                }
            };
            let proc_chance = calculate_proc_chance(ppm, window, schedule.cast_time, area);
            let avg_damage = proc_chance * dmg.value.unwrap_or(0.0);
            let avg_to_hit = proc_chance * to_hit.and_then(|e| e.value).unwrap_or(0.0);
            if avg_damage > best_damage {
                best_damage = avg_damage;
                best_to_hit = avg_to_hit;
                best_source = Some(BuildUpWinner {
                    set_name: set_name.to_string(),
                    proc_name: io_name.to_string(),
                    power_internal_name: sel.internal_name.clone(),
                    power_set: sel.powerset.clone(),
                    damage_category: dmg.category.clone(),
                    damage_open: category_open(dmg),
                    to_hit_category: to_hit.map(|e| e.category.clone()).unwrap_or_default(),
                });
            }
        }
    }

    if let Some(won) = best_source {
        let source = ProcSource {
            set_name: &won.set_name,
            proc_name: &won.proc_name,
            power_internal_name: &won.power_internal_name,
            power_set: &won.power_set,
        };
        if best_damage > 0.0 && won.damage_open {
            g.damage += best_damage;
            source.emit(
                &mut breakdown,
                &won.damage_category,
                "damage",
                best_damage,
                false,
                ProcSourceKind::BuildUp,
                "",
            );
        }
        if best_to_hit > 0.0 {
            g.to_hit += best_to_hit;
            source.emit(
                &mut breakdown,
                &won.to_hit_category,
                "toHit",
                best_to_hit,
                false,
                ProcSourceKind::BuildUp,
                "",
            );
        }
    }
    breakdown
}
