//! Per-power damage — the atoms an attack ships, each resolved against the target it names.
//!
//! An attack does not carry "a damage number". It carries several `Damage` atoms whose gates
//! disagree about who is being hit, and reading them is the whole job. Beheader's, verbatim:
//!
//! ```text
//! scale 1.00  Melee_Damage          prob 1.00  <critter>                    —
//! scale 0.45  Melee_Damage          prob 0.00  <critter>                    FieryEmbrace
//! scale 1.00  Melee_InherentDamage  prob 0.10  <critter, not a minion nor a player>
//!                                                                          CritLarge,ScrapperCrit_ST
//! scale 1.00  Melee_InherentDamage  prob 0.05  <critter, is a minion>       CritSmall,ScrapperCrit_ST
//! scale 1.26  Melee_PvPDamage       prob 1.00  <player>                     —
//! scale 1.26  Melee_PvPDamage       prob 0.05  <player>                     CritPlayer,ScrapperCrit_ST
//! ```
//!
//! Every one of those gates begins `enttype target> …`, and on four of the six rows that half is
//! INHERITED from the enclosing effect group rather than written on the atom's own — the whole
//! rank fork reads `enttype target> critter eq  arch target> Class_Minion_Grunt eq … !  &&`.
//!
//! The `InherentDamage` rows are the Scrapper critical hit, and the export states all of it:
//! the probability, the rank fork the two probabilities differ across, and — because
//! `melee_inherentdamage` is the same table as `melee_damage` — the fact that a crit is a second
//! full hit. The beta wrote those down instead (`averageBonusVsMinions` / `averageBonusVsHigher`,
//! `OPPORTUNITY_CRIT_MULTIPLIER = 0.40`, an `at === 'scrapper'` branch and five
//! `powersetId.startsWith(…)` checks). None of that is ported; there is no archetype name in this
//! module, and the mechanics fall out of the atoms on every fork at once (Rule 0).
//!
//! The tag column is the group's own authored `Tag`, which NAMES each of those mechanics, and it
//! reaches the reader on [`DamageComponent::tags`] unclassified — a component says
//! `CritLarge · ScrapperCrit_ST` in the game's own words rather than showing a bare 10%. It is
//! Homecoming-only — Parse6 (Rebirth, Thunderspy) has no effect group to hang one on — so a
//! surface that names a mechanic must say nothing rather than infer one on those forks.
//!
//! **A gate this context cannot answer is reported, never assumed.** With no target chosen every
//! damage atom is unresolved, and the caller renders that rather than a number — an attack
//! silently showing its ungated components only would read as a complete answer (Rule 1).

use crate::expr::{eval, eval_bool, EvalContext, EvalError, SourceContext, Value, CURRENT_TO_HIT};
use crate::projection::ThreeTier;
use crate::scaled::resolve_scaled_effect;
use crate::totals::CalcError;
use coh_data::{
    expression_text, lands_on_caster, AtomicEffect, EffectType, Power, PowerDatabase, PvMode,
};

/// How a resolved component applies — the three things the export's chance fields say, kept apart
/// because summing across them would be summing different kinds of claim.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub enum DamageApplication {
    /// `baseProbability == 1` — part of every hit that lands.
    Always,
    /// `0 < baseProbability < 1` — a chance of a further hit. Every hit-time archetype mechanic
    /// takes this shape (the crit, Scourge, Assassination), and none of them is folded into the
    /// damage: an averaged number is not any hit that ever lands.
    Chance(f64),
    /// A chance of zero on either field — present on the power but inert as it stands, waiting on
    /// something outside the attack. Every fork ships them (775 Homecoming, 735 Rebirth, 518
    /// Thunderspy), overwhelmingly Fire: the damage a Fiery Embrace-class buff adds to whatever
    /// attack is running. Two of those three counts read 0 until a parser gap was closed, and a
    /// fork claiming none of a mechanic every other fork has is the shape that bug wore.
    ///
    /// Reported rather than dropped, and never counted. The beta instead guessed at them with a
    /// threshold over the power's own type mix (`FIERY_EMBRACE_THRESHOLD = 0.20` — "Fire is under
    /// a fifth of the total, so it must be Fiery Embrace"), which is a heuristic over a name in
    /// all but spelling. What wakes one is not in the atom's NUMBERS, but it is in the group's
    /// [`AtomicEffect::tags`]: every one of those Fire components is tagged `FieryEmbrace`.
    ///
    /// The export writes "inert" in two places — the group's `Chance` and the template's
    /// `TickChance` — and [`application_of`] reads both, because both are the same roll.
    Dormant,
}

/// A component that lands over time rather than at once — the atom's `applicationPeriod`
/// saying it repeats, and its `duration` saying for how long.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct DamageOverTime {
    /// Seconds the effect runs for.
    pub duration: f64,
    /// Seconds between ticks.
    pub period: f64,
    /// Ticks the effect would land if every one of them did — the tick at t=0 included.
    pub nominal_ticks: f64,
    /// The per-tick apply chance, when the template rolls one. `None` is every tick landing.
    pub tick_chance: Option<f64>,
    /// Whether a missed tick ends the chain (see [`expected_ticks`]).
    pub cancel_on_miss: bool,
    /// Probability-weighted ticks — what a total multiplies the per-tick damage by. Equal to
    /// `nominal_ticks` when every tick is certain.
    pub expected_ticks: f64,
}

/// One damage atom, resolved. Kept per-atom rather than summed by type because the reasons two
/// atoms differ — a different table, how it applies, a gate — are what a reader needs to see;
/// the sum is one fold away and throws all of it out.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DamageComponent {
    /// The atom's `subType` — the damage type as the export names it (`Lethal`, `Fire`).
    pub damage_type: String,
    /// The AT modifier table this component read (`Melee_Damage`, `Melee_InherentDamage`).
    pub table: String,
    /// `scale × |table[level]|`, before enhancement or buffs. PER TICK for a
    /// [`Self::over_time`] component — the tick is what the scale describes.
    pub base: f64,
    /// Everything this component lands over its whole duration, at each enhancement tier: the
    /// per-tick [`Self::base`] times the ticks expected to land, then the build's strength.
    /// Resolved here rather than by a display surface, so no surface has to know that the cap
    /// binds the multiplier rather than the damage.
    pub total: ThreeTier,
    /// Whether this component always lands, may land, or is inert.
    pub application: DamageApplication,
    /// Set when this component ticks rather than landing at once.
    pub over_time: Option<DamageOverTime>,
    /// The gate that had to hold for this component to be here, if it had one. Carried so a
    /// reader can be told WHY a row appears against this target and not another.
    pub gate: Option<String>,
    /// The effect group's authored `Tag`s, verbatim and unclassified — where the game NAMES the
    /// mechanic this component belongs to (`CritLarge`, `ScrapperCrit_ST`, `StealthCrit`,
    /// `Containment`, `FieryEmbrace`).
    ///
    /// Nothing here decides which tags "are" mechanics: the export ships flavour labels
    /// (`Bleed`, `ColdDamage`) in the same field, and a list saying `CritLarge` counts while
    /// `ColdDamage` does not would be exactly the table of game proper nouns Rule 0 forbids.
    /// Every tag the group wrote is passed through and the reader sees the export's own words.
    ///
    /// Populated on every fork since COND-11, and the forks spell it differently. Homecoming
    /// tags fewer than a third of its groups; both Parse6 forks name nearly every AttribMod,
    /// because the field they carry it in is per-mod rather than per-group. Roughly nine in ten
    /// Parse6 names restate the mod's own table (`Ones`, `Res_Dmg`), which reads like corruption
    /// and isn't: Homecoming's own most common tag is `Damage`, and both Parse6 forks carry the
    /// same real vocabulary underneath (`FieryEmbrace`, `ColdDamage`, `CritPlayer`, `Defiance`).
    ///
    /// A name here is a label, never the mechanic. Containment is a gate on target mez state and
    /// fires wherever that gate does — of Homecoming's own 377 Containment twins only 210 carry
    /// the `Containment` tag and 134 carry no tag at all. Counting tags to decide whether a fork
    /// HAS a mechanic is the mistake PARSE6-3 records; count the gate.
    pub tags: Vec<String>,
    /// Set when this component's magnitude program ranges over a bounded circumstance register
    /// (Savage Leap's `distance`, the roll-quality snipes' `@ToHitRoll`): [`Self::base`] and
    /// [`Self::total`] hold the LOW endpoint, this holds the high one, and the endpoints are
    /// exact — each is the program's image of the register's own domain endpoint. `None` is a
    /// component whose value is one point, which every other component is.
    pub spread: Option<DamageSpread>,
}

/// The high end of a range-valued [`DamageComponent`], shaped like the low end it pairs with.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct DamageSpread {
    /// The high endpoint of [`DamageComponent::base`] — per tick for a ticking component.
    pub base: f64,
    /// The high endpoint of [`DamageComponent::total`], tier for tier.
    pub total: ThreeTier,
    /// The register the value ranges over, as the program names it (`distance`, `@ToHitRoll`) —
    /// the export's own word, so the row can say WHY it is a range without a translation table.
    pub over: String,
}

impl DamageComponent {
    /// Whether this component is part of every landed hit — the only kind the totals sum.
    pub fn is_certain(&self) -> bool {
        self.application == DamageApplication::Always
    }

    /// The chance of this component, when it is one.
    pub fn chance(&self) -> Option<f64> {
        match self.application {
            DamageApplication::Chance(probability) => Some(probability),
            _ => None,
        }
    }

    /// Everything this component lands over its whole duration, unenhanced — shorthand for
    /// `total.base`.
    pub fn total_base(&self) -> f64 {
        self.total.base
    }
}

/// Ticks an effect of `duration` lands at `period`, counting the tick at t=0: they fire at
/// 0, period, 2·period, … duration, so `duration / period + 1`.
///
/// The epsilon absorbs float32 noise in the period — the binary stores `0.20000000298` for an
/// authored 0.2, which makes `2 / 0.2` come out at 9.9999998 and drops the final tick through
/// the floor (the beta's `dotTickCount`, whose comment records Freeze Ray reading 10 ticks in
/// the planner against 11 in game). It is far above any float32 error and far below the gap to
/// a genuinely fractional ratio.
pub(crate) fn nominal_ticks(duration: f64, period: f64) -> f64 {
    if period <= 0.0 {
        return 1.0;
    }
    (duration / period + 1e-4).floor() + 1.0
}

/// The probability-weighted tick count — how many of `nominal` actually land.
///
/// A tick chance compounds one of two ways, and the template's `CancelOnMiss` flag says which:
/// with the flag the whole chain stops at the first miss, so tick k needs k consecutive hits
/// and the expectation is the geometric sum `Σ chance^k`; without it each tick rolls alone and
/// the expectation is `n × chance`. It is why the game's own tooltip reads lower than
/// n × per-tick: Flares' four 80% ticks average 0.8 + 0.64 + 0.512 + 0.4096 = 2.3616.
fn expected_ticks(nominal: f64, chance: Option<f64>, cancel_on_miss: bool) -> f64 {
    let Some(chance) = chance.filter(|c| *c > 0.0 && *c < 1.0) else {
        return nominal;
    };
    if !cancel_on_miss {
        return nominal * chance;
    }
    chance * (1.0 - chance.powf(nominal)) / (1.0 - chance)
}

/// A damage atom whose gate this context could not answer. Held rather than dropped: the
/// difference between "this attack has no PvP component" and "nobody said who is being hit" is
/// the difference between a fact and a blank.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct UnresolvedDamage {
    pub damage_type: String,
    pub gate: String,
    /// What the evaluator could not answer — an unmodelled reader, or a malformed program.
    pub reason: String,
    /// The group's authored `Tag`s, as on [`DamageComponent::tags`]. An unresolved component is
    /// where they earn the most: Containment's gate reads five mez states this context does not
    /// model, so the line the reader gets is the difference between "some Fire component is
    /// unresolved" and "the Containment one is".
    pub tags: Vec<String>,
    /// Whether the only thing missing is the target's RANK (`arch target>`) — the one gap the
    /// planner has a control for, so a surface can offer that control instead of the gate text.
    pub waits_on_rank: bool,
    /// The component as it lands when the gate does hold. The gate decides WHETHER it applies,
    /// not how much, so the amount is usually knowable while the condition is not — Containment's
    /// doubled hit is a number even with nobody saying the foe is held. `None` when the value
    /// needs more than the gate did (its own magnitude program, an unsettled roll).
    pub if_it_lands: Option<DamageComponent>,
}

/// Every damage component of one power against one target, at all three enhancement tiers.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize)]
pub struct PowerDamage {
    /// The components whose gate holds against this target, in the atoms' own order.
    pub components: Vec<DamageComponent>,
    /// The components whose gate this context could not answer (Rule 1 — shown, not dropped).
    pub unresolved: Vec<UnresolvedDamage>,
    /// Sum of every [`DamageApplication::Always`] component's total base damage — a ticking
    /// component contributing everything it lands over its duration, not one tick. Chance and
    /// dormant components are excluded: neither is part of the hit this number describes. A
    /// range-valued component contributes its LOW endpoint, so this is the hit's floor.
    pub base: f64,
    /// [`Self::base`] with this power's own slotted enhancement.
    pub enhanced: f64,
    /// [`Self::enhanced`] with the build's global damage buffs, capped.
    pub r#final: f64,
    /// The high end of the three sums, present exactly when a certain component carries a
    /// [`DamageComponent::spread`] — the hit then ranges, and stating only its floor would
    /// read as the whole answer.
    pub high: Option<ThreeTier>,
    /// Whether the archetype's damage-strength cap bound [`Self::r#final`].
    pub capped: bool,
}

impl PowerDamage {
    /// Whether this power deals damage at all — the discriminator between "no damage rows
    /// because it is not an attack" and "no damage rows because no target is chosen".
    pub fn is_empty(&self) -> bool {
        self.components.is_empty() && self.unresolved.is_empty()
    }
}

/// Resolve one power's damage against `target_context`, at `level` for `archetype`.
///
/// `enhancement` is this power's post-ED damage fraction and `global_damage` the build's damage
/// buffs; both fold into ONE strength multiplier (`1 + enh + buffs`), which is what the atoms'
/// `Abs` aspect means and what the beta does at `damage.ts:650`.
#[allow(clippy::too_many_arguments)]
pub fn resolve_power_damage(
    power: &Power,
    archetype: &str,
    level: i32,
    enhancement: f64,
    global_damage: f64,
    target_context: &SourceContext,
    database: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> PowerDamage {
    let mut damage = PowerDamage::default();

    // This power as THIS archetype has it. An epic-pool attack open to both Scrappers and
    // Stalkers carries each one's hit-time components, and showing the other's would read as
    // missing information rather than as somebody else's mechanic (AT-FORK-1). A fork the
    // stamp can't carry — one conjoined with live target state — is answered by the gate's own
    // `arch source>` read against `SourceContext::caster_class`.
    let narrowed = power.for_caster_class(database.class_name_of(archetype));
    let power: &Power = &narrowed;

    // ONE strength multiplier for the whole power, resolved before any component so every row and
    // the total agree by construction. The archetype's damage-strength cap binds the MULTIPLIER,
    // per power — not the global damage-buff total (beta `damage.ts:648`). An absent cap does not
    // clamp: a dataset that ships none has not said the damage is unlimited, but inventing a
    // ceiling would be worse.
    let enhanced_multiplier = 1.0 + enhancement;
    let strength = 1.0 + enhancement + global_damage;
    let cap = damage_cap(archetype, database).filter(|cap| strength > *cap);
    damage.capped = cap.is_some();
    let final_multiplier = cap.unwrap_or(strength);

    for atom in &power.atoms {
        if atom.effect_type != Some(EffectType::Damage) {
            continue;
        }
        // A self-directed Damage atom is a cost the caster pays, not damage this power deals.
        //
        // The ANCHOR reading on purpose, not `reaches_caster`, and for `perma.rs`'s reason: this
        // decides whether to DROP a component, so the burden of proof is reversed. An atom the
        // power happens to be able to land on its caster is still the damage the power deals to
        // everyone else, and reading the join here would delete a PBAoE's whole payload the
        // moment its `EntsAffected` named `Self`. Only an atom that names the caster outright is
        // a cost.
        if lands_on_caster(atom) {
            continue;
        }
        // Homecoming forks PvE/PvP two ways and an attack may use either: the group's `Requires`
        // (`enttype target> critter eq`, which [`gate_holds`] answers) or the group's own PvE/PvP
        // flag bit, which is a field and not an expression. Storm Blast is written the second way,
        // so before this its `Ranged_PvPDamage` components were summed into a PvE hit. Only 29
        // Homecoming damage atoms take flag form; on Rebirth and Thunderspy every one of the 3,139
        // / 2,793 PvP-flagged atoms ALSO carries the player gate, so reading the flag changes
        // nothing there and simply agrees with it.
        match pv_mode_holds(atom, target_context) {
            GateVerdict::Holds => {}
            GateVerdict::Fails => continue,
            GateVerdict::Unknown { reason, .. } => {
                damage.unresolved.push(UnresolvedDamage {
                    damage_type: sub_type_name(atom),
                    gate: format!("{:?}-only", atom.pv_mode.unwrap_or(PvMode::Any)),
                    reason,
                    tags: tags_of(atom),
                    waits_on_rank: false,
                    if_it_lands: None,
                });
                continue;
            }
        }
        // An unanswered gate still has its component valued below, so the line can state what
        // it adds when it applies; every exit from here on files `pending` in its own place.
        let pending = match gate_holds(atom, target_context) {
            GateVerdict::Holds => None,
            GateVerdict::Fails => continue,
            GateVerdict::Unknown {
                reason,
                waits_on_rank,
            } => Some(UnresolvedDamage {
                damage_type: sub_type_name(atom),
                gate: expression_text(atom.requires_expression.as_deref()),
                reason,
                tags: tags_of(atom),
                waits_on_rank,
                if_it_lands: None,
            }),
        };

        // An instant template that rolls a chance of its own — one this atom's `base_probability`
        // does not already carry — is a component this module cannot place. The periodic reading
        // is settled (a per-tick roll); the instant one is not, and Homecoming's carriers mix a
        // 0.998 to-hit artifact in with genuine per-template rolls. Folding either reading in
        // would ship a guess as a number, so it is reported (Rule 1).
        if let Some(chance) = instant_tick_chance(atom) {
            damage
                .unresolved
                .push(pending.unwrap_or_else(|| UnresolvedDamage {
                    damage_type: sub_type_name(atom),
                    gate: expression_text(atom.requires_expression.as_deref()),
                    reason: format!(
                    "the template rolls its own {chance} chance without ticking, and what that \
                     roll means is not settled across the forks"
                ),
                    tags: tags_of(atom),
                    waits_on_rank: false,
                    if_it_lands: None,
                }));
            continue;
        }

        let table = atom.modifier_table.as_deref();
        // The scale through its table, signed as the table stores it — which is the `@StdResult`
        // operand a magnitude program modifies, so it is computed before [`magnitude`] rather
        // than folded into it. The bare table value beside it is the program's `@Value`, needed
        // only when there is a program to read it.
        let std_result = resolve_scaled_effect(
            atom.scale.unwrap_or(0.0),
            table,
            archetype,
            level,
            database,
            errors,
        );
        let table_value = if atom.magnitude_expression.is_some() {
            resolve_scaled_effect(1.0, table, archetype, level, database, errors)
        } else {
            0.0
        };
        // The gate's own gap outranks a later one: it is the first thing the line can't say.
        let unresolved = |reason: String| {
            pending.clone().unwrap_or_else(|| UnresolvedDamage {
                damage_type: sub_type_name(atom),
                gate: expression_text(atom.requires_expression.as_deref()),
                reason,
                tags: tags_of(atom),
                waits_on_rank: false,
                if_it_lands: None,
            })
        };
        let Valuation {
            kind,
            strength_scales,
        } = match magnitude(atom, std_result, table_value, target_context) {
            Ok(valuation) => valuation,
            Err(reason) => {
                damage.unresolved.push(unresolved(reason));
                continue;
            }
        };

        let over_time = over_time_of(atom);
        let ticks = over_time.map_or(1.0, |over_time| over_time.expected_ticks);
        // A strength-inert value reads the same at every tier — scaling it would fabricate an
        // enhancement response the game does not have (see [`Valuation::strength_scales`]).
        let tiers = |per_tick: f64| {
            let total = per_tick * ticks;
            if strength_scales {
                ThreeTier {
                    base: total,
                    enhanced: total * enhanced_multiplier,
                    r#final: total * final_multiplier,
                }
            } else {
                ThreeTier {
                    base: total,
                    enhanced: total,
                    r#final: total,
                }
            }
        };

        // Damage tables store their values NEGATIVE — damage is a reduction of the target's hit
        // points, and the table says so in its sign. The magnitude is what a damage row means
        // (beta `calculateDamageWithATTable`'s `Math.abs`).
        let (base, application, spread) = match kind {
            ValuationKind::Point(value) => (value.abs(), application_of(atom), None),
            ValuationKind::Rolled { p, value } => {
                // The program re-evaluates per application. On a ticking template that is one
                // roll per tick OR one roll for the chain, and the game's delayed-eval path
                // makes the answer non-obvious — no shipped atom combines the two (every rand
                // carrier is instant, `base_probability` 1), so the combination is a decision
                // for the data that first ships it, not a default.
                if over_time.is_some() {
                    damage.unresolved.push(unresolved(format!(
                        "the magnitude program rolls a {p} chance and the template ticks; \
                         whether the roll repeats per tick is not settled"
                    )));
                    continue;
                }
                (
                    value.abs(),
                    rolled_application(application_of(atom), p),
                    None,
                )
            }
            ValuationKind::Ranged { lo, hi, over } => {
                let (low, high) = magnitude_span(lo, hi);
                (
                    low,
                    application_of(atom),
                    Some(DamageSpread {
                        base: high,
                        total: tiers(high),
                        over,
                    }),
                )
            }
        };

        let component = DamageComponent {
            damage_type: sub_type_name(atom),
            table: table.unwrap_or_default().to_string(),
            base,
            total: tiers(base),
            application,
            over_time,
            gate: atom
                .requires_expression
                .as_deref()
                .map(|tokens| expression_text(Some(tokens))),
            tags: tags_of(atom),
            spread,
        };
        match pending {
            Some(mut line) => {
                line.if_it_lands = Some(component);
                damage.unresolved.push(line);
            }
            None => damage.components.push(component),
        }
    }

    // Summed per component rather than as `base × multiplier`, because a strength-inert
    // component's tiers do not move with the rest of the hit.
    let certain = || damage.components.iter().filter(|c| c.is_certain());
    damage.base = certain().map(|c| c.total.base).sum();
    damage.enhanced = certain().map(|c| c.total.enhanced).sum();
    damage.r#final = certain().map(|c| c.total.r#final).sum();
    damage.high = certain().any(|c| c.spread.is_some()).then(|| {
        let high = |c: &DamageComponent| c.spread.as_ref().map_or(c.total, |spread| spread.total);
        ThreeTier {
            base: certain().map(|c| high(c).base).sum(),
            enhanced: certain().map(|c| high(c).enhanced).sum(),
            r#final: certain().map(|c| high(c).r#final).sum(),
        }
    });

    damage
}

/// The application after the magnitude program's own roll joins the atom's: independent rolls,
/// both of which must pass for the value to land. A chance the program prices at nothing is a
/// component the export ships inert, exactly as a zero in the chance fields is.
fn rolled_application(application: DamageApplication, p: f64) -> DamageApplication {
    if p <= 0.0 {
        return DamageApplication::Dormant;
    }
    if p >= 1.0 {
        return application;
    }
    match application {
        DamageApplication::Always => DamageApplication::Chance(p),
        DamageApplication::Chance(q) => DamageApplication::Chance(q * p),
        DamageApplication::Dormant => DamageApplication::Dormant,
    }
}

/// A damage magnitude is the absolute value of the (negative-stored) result; for a ranged one,
/// the ordered absolute values of its endpoints. A range spanning zero bottoms out at zero —
/// somewhere in the register's domain the component deals nothing.
fn magnitude_span(lo: f64, hi: f64) -> (f64, f64) {
    if lo >= 0.0 {
        (lo, hi)
    } else if hi <= 0.0 {
        (-hi, -lo)
    } else {
        (0.0, (-lo).max(hi))
    }
}

/// The combat registers a damage atom's `magnitude_expression` reads, each bound to what the
/// game's own evaluator binds it to (`attribmod.c` `mod_Fill` → `combateval_StoreAttribCalcInfo`
/// / `combateval_StoreToHitInfo`, and `eval.c`'s `Random`):
///
/// - `@StdResult` — `fFinal`: table × effectiveness × scale × strength. Supplied as
///   `std_result × strength`, where `std_result` is the scale×table half this resolver computes
///   and `strength` is the probe multiplier below.
/// - `@Value` — `fVal`, the AT table value at combat level, alone.
/// - `@Scale` — `ptemplate->fScale`, the atom's own scale. An atom stating none leaves it
///   unstated (absence is not a default).
/// - `@Effectiveness` — `fEffectiveness`, the target-level scaling. `1.0` here: the projection
///   is even-level everywhere (the purple patch is source-blocked), the same convention
///   `std_result` already embodies.
/// - `@Strength` — `fStr`, the caster's strength for the attrib. The PIPELINE owns strength
///   (the three tiers multiply it on after), so evaluation runs at `strength = 1.0` — and
///   [`magnitude`] proves the program is multiplicative in it by evaluating a second time at
///   `2.0`, refusing any program where doubling the strength carriers does not double (or
///   leave untouched) the result. `appliers/absorb.rs` binds `@Strength` to 1.0 on the same
///   reasoning.
/// - `@ToHit` — `fToHit`, the build's own to-hit chance, read from the same
///   [`CURRENT_TO_HIT`] attribute the fork snipes' form gate reads. Unsupplied → Indeterminate.
/// - `rand`, `@ToHitRoll` — uniform-[0,1) dies, resolved to [`Value::Die`] so a determinate
///   chance compared against one folds to its exact probability.
/// - `distance` — the source-target separation, a circumstance no build state pins. Resolved
///   to an unbounded [`Value::Range`]: the shipped programs saturate it with their own
///   `minmax`, and a program that does not leaves an infinite endpoint [`magnitude`] refuses.
///
/// Everything else — `source.TeamSize>`, `source.ownPowerNum?`, the target readers — is the
/// same build state the atom's GATE is evaluated against, so it delegates rather than carrying
/// a second, differently-populated copy.
struct Magnitude<'a> {
    build: &'a SourceContext,
    std_result: f64,
    /// The AT table value at this level, scale excluded — the game's `fVal`.
    table_value: f64,
    /// The atom's own scale, when it states one.
    scale: Option<f64>,
    /// The strength this evaluation runs at — 1.0 for the real read, 2.0 for the probe.
    strength: f64,
}

impl EvalContext for Magnitude<'_> {
    fn resolve(&self, reader: &str, operands: &[Value]) -> Result<Value, EvalError> {
        match reader {
            "@StdResult" => Ok(Value::Number(self.std_result * self.strength)),
            "@Value" => Ok(Value::Number(self.table_value)),
            "@Scale" => self
                .scale
                .map(Value::Number)
                .ok_or_else(|| EvalError::Indeterminate("@Scale (the atom states none)".into())),
            "@Effectiveness" => Ok(Value::Number(1.0)),
            "@Strength" => Ok(Value::Number(self.strength)),
            "@ToHit" => self
                .build
                .resolve("source>", &[Value::Symbol(CURRENT_TO_HIT.into())])
                .map_err(|_| EvalError::Indeterminate("@ToHit".into())),
            "rand" | "@ToHitRoll" => Ok(Value::Die(reader.into())),
            "distance" => Ok(Value::Range {
                lo: 0.0,
                hi: f64::INFINITY,
                over: "distance".into(),
            }),
            _ => self.build.resolve(reader, operands),
        }
    }
}

/// What one damage atom's program yields, before ticks and before strength.
#[derive(Debug)]
enum ValuationKind {
    /// One number — the shape a plain `scale × table` atom and a fully-determinate program share.
    Point(f64),
    /// `value` with probability `p` — the program compared a determinate chance against a die.
    Rolled { p: f64, value: f64 },
    /// The exact endpoints of a program ranging over a bounded circumstance, and the register's
    /// name it ranges over.
    Ranged { lo: f64, hi: f64, over: String },
}

#[derive(Debug)]
struct Valuation {
    kind: ValuationKind,
    /// Whether the three tiers may multiply strength onto this value. A program that reads
    /// neither `@StdResult` nor `@Strength` produces a magnitude the game never enhances —
    /// `@Scale Max.kHitPoints target> * negate` deals a fraction of the target's HP no slotting
    /// changes — and scaling it anyway would fabricate an enhancement response.
    strength_scales: bool,
}

/// Two probe reads agree when they are the same number at strength 1 and 2 — the program
/// ignores the strength registers.
fn agrees(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(1.0)
}

/// Two probe reads double when strength doubling doubles the result — the program is
/// multiplicative in the strength carriers, so tier multiplication composes exactly.
fn doubles(at_one: f64, at_two: f64) -> bool {
    agrees(at_two, 2.0 * at_one)
}

/// What one damage atom is worth.
///
/// `scale × table` is the answer only for an atom that states a plain magnitude. An atom whose
/// `magnitude_expression` is set is VALUED by that program, and its `scale` is the program's
/// `@StdResult` operand rather than its result — Homecoming's quick snipes ramp with the caster's
/// ToHit (`cur.kToHit source> 0.75 - 0.22 / -1.0 1.0 minmax … @StdResult *`) and split one shot
/// across two types by trailing `0.3 *` / `0.7 *`. Summing their raw scales instead reads a
/// two-type shot as two full shots.
///
/// The program runs TWICE, at strength 1 and strength 2, and the pair of reads is the proof of
/// how strength lands: doubled → multiplicative, the tiers' outer strength composes exactly;
/// unmoved → strength-inert, the tiers must not scale it; anything else → the program bends
/// strength in a way one number per tier cannot state, refused. The same comparison holds a
/// rolled probability and a range's endpoints to strength-independence.
///
/// `Err` when the program cannot be valued here — a register this context does not model, an
/// endpoint no program clamp bounded, a per-roll shape this resolver has no settled reading
/// for. The component is then REPORTED, never folded at its raw scale: an unvalued expression's
/// scale is an operand, and summing an operand as if it were the result is how these atoms came
/// to overstate 39 Homecoming / 134 Rebirth / 170 Thunderspy powers (DATA-GAP-REGISTER
/// MAGEXPR-1).
fn magnitude(
    atom: &AtomicEffect,
    std_result: f64,
    table_value: f64,
    build: &SourceContext,
) -> Result<Valuation, String> {
    let Some(expression) = atom.magnitude_expression.as_deref() else {
        return Ok(Valuation {
            kind: ValuationKind::Point(std_result),
            strength_scales: true,
        });
    };
    let read = |strength: f64| {
        let context = Magnitude {
            build,
            std_result,
            table_value,
            scale: atom.scale,
            strength,
        };
        eval(expression, &context)
    };
    let at_one = match read(1.0) {
        Ok(value) => value,
        Err(EvalError::Indeterminate(reader)) => {
            return Err(format!(
                "magnitude expression {expression:?} depends on {reader}, which this context \
                 does not model"
            ))
        }
        // Same reading as a malformed gate: a grammar gap rather than an unknown, and news.
        Err(EvalError::Malformed(problem)) => {
            return Err(format!(
                "magnitude expression {expression:?} is malformed: {problem}"
            ))
        }
    };
    let at_two = read(2.0).map_err(|e| {
        format!("magnitude expression {expression:?} fails only under the strength probe: {e:?}")
    })?;

    let strength_scales = |one: f64, two: f64| -> Result<bool, String> {
        if doubles(one, two) {
            Ok(true)
        } else if agrees(one, two) {
            Ok(false)
        } else {
            Err(format!(
                "magnitude expression {expression:?} is nonlinear in strength ({one} at 1× \
                 becomes {two} at 2×), which three point tiers cannot state"
            ))
        }
    };

    match (at_one, at_two) {
        (Value::Number(one), Value::Number(two)) => Ok(Valuation {
            strength_scales: strength_scales(one, two)?,
            kind: ValuationKind::Point(one),
        }),
        (
            Value::Chance { p, value },
            Value::Chance {
                p: p2,
                value: value2,
            },
        ) => {
            if !agrees(p, p2) {
                return Err(format!(
                    "magnitude expression {expression:?} rolls a strength-dependent chance \
                     ({p} at 1×, {p2} at 2×)"
                ));
            }
            Ok(Valuation {
                strength_scales: strength_scales(value, value2)?,
                kind: ValuationKind::Rolled { p, value },
            })
        }
        (
            Value::Range { lo, hi, over },
            Value::Range {
                lo: lo2, hi: hi2, ..
            },
        ) => {
            if !lo.is_finite() || !hi.is_finite() {
                return Err(format!(
                    "magnitude expression {expression:?} ranges unboundedly over {over}"
                ));
            }
            let scales = strength_scales(lo, lo2)?;
            if scales != strength_scales(hi, hi2)? {
                return Err(format!(
                    "magnitude expression {expression:?} scales one endpoint of its {over} \
                     range with strength and not the other"
                ));
            }
            Ok(Valuation {
                strength_scales: scales,
                kind: ValuationKind::Ranged {
                    lo,
                    hi,
                    over: over.into(),
                },
            })
        }
        // A bare symbol, die, or pass/fail roll as the whole magnitude ships nowhere; each
        // would be an authored oddity worth seeing, not a value to decode.
        (one @ (Value::Symbol(_) | Value::Die(_) | Value::Bernoulli(_)), _) => Err(format!(
            "magnitude expression {expression:?} resolves to {one:?}, not a value this \
             resolver can state"
        )),
        // The probe changed which SHAPE came back — strength decides whether the program
        // rolls or ranges, which no fixed component kind can state.
        (one, two) => Err(format!(
            "magnitude expression {expression:?} changes shape under the strength probe \
             ({one:?} at 1×, {two:?} at 2×)"
        )),
    }
}

/// The archetype's total damage-strength cap, as a multiplier.
fn damage_cap(archetype: &str, database: &PowerDatabase) -> Option<f64> {
    database
        .archetype_stats
        .get(archetype)
        .map(|caps| caps.damage_cap)
        .filter(|cap| *cap > 0.0)
}

/// The atom's damage type as the export names it. An atom with no `subType` is untyped damage,
/// which the export does ship — named as such rather than guessed at from the power's name (the
/// beta's `inferDamageType` reads the power NAME, which is the hardcode Rule 0 forbids).
fn sub_type_name(atom: &AtomicEffect) -> String {
    atom.sub_type
        .as_ref()
        .map(|sub_type| format!("{sub_type:?}"))
        .unwrap_or_else(|| "Untyped".to_string())
}

/// The group tags an atom carries, comma-joined on the wire and split back here. Empty for an
/// untagged atom and for every atom on a fork whose parser has no effect group to read one from.
///
/// The split is the only processing: no tag is renamed, reordered, filtered or looked up. The
/// export's `ScrapperCrit_ST` reaches the reader as `ScrapperCrit_ST`, because a table turning it
/// into "Critical hit (single target)" is a table of game proper nouns, which is what the beta's
/// five `is<Archetype>AttackPower` checks were (Rule 0).
fn tags_of(atom: &AtomicEffect) -> Vec<String> {
    atom.tags
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_string)
        .collect()
}

/// How an atom applies, from either chance it carries. An absent probability is `Always` — the
/// field says how often a present effect fires, and saying nothing is not saying "never".
///
/// A non-positive chance on EITHER field is `Dormant`, because both are the same roll: the source
/// rolls `fRand < fChance` with `fRand` drawn from `[0,1)`, which nothing at or below zero can
/// pass. Homecoming writes one of its mechanics that way — the Sentinel's Opportunity component is
/// `chance 1.0` on the group and `TickChance 0.0000` on the template, and 516 Homecoming damage
/// atoms carry it. Left unread, its nominal ticks summed into the certain hit (Sentinel Burst read
/// 92.49 where 66.07 is the hit).
///
/// Reading the two fields the same way is what makes the forks agree. Fire Sword's dormant
/// Fiery-Embrace pair is authored identically on all three, and each encodes it where its schema
/// has room: Homecoming and Thunderspy on the group's `Chance` and the template's `TickChance`,
/// Rebirth on the single `Chance` that serves as both.
fn application_of(atom: &AtomicEffect) -> DamageApplication {
    if atom.tick_chance.is_some_and(|chance| chance <= 0.0) {
        return DamageApplication::Dormant;
    }
    match atom.base_probability {
        None => DamageApplication::Always,
        Some(probability) if probability >= 1.0 => DamageApplication::Always,
        Some(probability) if probability <= 0.0 => DamageApplication::Dormant,
        Some(probability) if spends_its_roll_on_ticks(atom, probability) => {
            DamageApplication::Always
        }
        Some(probability) => DamageApplication::Chance(probability),
    }
}

/// Whether this atom's one roll is the per-tick roll [`over_time_of`] has already spent.
///
/// A periodic mod carrying the SAME probability in both slots carries it once: the fork has no
/// effect group, so the AttribMod's `Chance` is lifted into a synthetic group chance beside the
/// copy left on the template. Rolling it in both places discounts the component twice — a 0.8
/// per-tick DoT withheld from the total AND cut to 0.8 of its ticks. Fire Sword's burn is the
/// case: Homecoming and Thunderspy write it `Chance 1.0` + `TickChance 0.8`, and it must read the
/// same where one field says both.
///
/// The equality is what makes it decidable, so this needs no fork to be named — and only a fork
/// with one field can satisfy it. Rebirth templates do; no Homecoming or Thunderspy one does,
/// which `each_fork_rolls_its_own_way` pins.
fn spends_its_roll_on_ticks(atom: &AtomicEffect, probability: f64) -> bool {
    atom.application_period.is_some_and(|period| period > 0.0)
        && atom
            .tick_chance
            .is_some_and(|chance| same_authored_chance(chance, probability))
}

/// Whether two chances are the same authored number. The wire rounds a tick chance to two
/// decimals and a base probability not at all (`mapTickChance`, and the float32 noise it exists to
/// absorb — an authored 0.8 reaches the export as 0.800000011920929), so comparing them raw asks
/// the wrong question. This asks them at the coarser of the two precisions, which is the one that
/// survived.
fn same_authored_chance(a: f64, b: f64) -> bool {
    (a * 100.0).round() == (b * 100.0).round()
}

/// The tick shape of a periodic atom, or `None` for one that lands at once.
fn over_time_of(atom: &AtomicEffect) -> Option<DamageOverTime> {
    let period = atom.application_period.filter(|period| *period > 0.0)?;
    let duration = atom.duration.unwrap_or(0.0);
    let nominal = nominal_ticks(duration, period);
    let tick_chance = atom.tick_chance.filter(|chance| *chance < 1.0);
    let cancel_on_miss = atom.cancel_on_miss == Some(true);
    Some(DamageOverTime {
        duration,
        period,
        nominal_ticks: nominal,
        tick_chance,
        cancel_on_miss,
        expected_ticks: expected_ticks(nominal, tick_chance, cancel_on_miss),
    })
}

/// The chance an atom rolls per application while landing all at once — the case
/// [`resolve_power_damage`] refuses to interpret. `None` for a periodic atom (whose chance is
/// a per-tick roll [`over_time_of`] owns), for one that rolls nothing, for a zero
/// ([`application_of`] reads that as `Dormant`), and for one whose roll is the same roll its
/// `base_probability` already carries.
///
/// That last case is every Rebirth carrier. Parse6 has no effect group, so the AttribMod's single
/// `Chance` is both the application roll and the per-tick roll, and the parser emits it into both
/// slots — one field seen twice, not two rolls to compound.
fn instant_tick_chance(atom: &AtomicEffect) -> Option<f64> {
    if atom.application_period.is_some_and(|period| period > 0.0) {
        return None;
    }
    let chance = atom
        .tick_chance
        .filter(|chance| *chance > 0.0 && *chance < 1.0)?;
    let duplicates_base = atom
        .base_probability
        .is_some_and(|probability| same_authored_chance(probability, chance));
    (!duplicates_base).then_some(chance)
}

enum GateVerdict {
    Holds,
    Fails,
    /// `waits_on_rank`: unknown only because no target rank is chosen — see
    /// [`UnresolvedDamage::waits_on_rank`].
    Unknown {
        reason: String,
        waits_on_rank: bool,
    },
}

/// The entity type the export's `enttype target>` fork names for a player. The other side of the
/// fork is `critter`, but this is not a two-valued question — an unrecognised entity type is
/// neither, and answering it would be inventing the answer.
const PLAYER_ENTITY_TYPE: &str = "player";

/// Whether an atom's PvE/PvP FLAG admits this target. `Any` (the overwhelming majority) always
/// holds; the other two need to know whether the target is a player, which only a chosen target
/// can say.
fn pv_mode_holds(atom: &AtomicEffect, context: &SourceContext) -> GateVerdict {
    let mode = match atom.pv_mode {
        None | Some(PvMode::Any) => return GateVerdict::Holds,
        Some(mode) => mode,
    };
    let Some(target) = context.target.as_ref() else {
        return GateVerdict::Unknown {
            reason: "the atom applies in one of PvE/PvP only, and no target is chosen".to_string(),
            waits_on_rank: false,
        };
    };
    let is_player = target.entity_type.eq_ignore_ascii_case(PLAYER_ENTITY_TYPE);
    let holds = match mode {
        PvMode::PvP => is_player,
        PvMode::PvE => !is_player,
        PvMode::Any => true,
    };
    if holds {
        GateVerdict::Holds
    } else {
        GateVerdict::Fails
    }
}

/// Whether an atom's `requires` gate holds for this target. An ungated atom always holds.
fn gate_holds(atom: &AtomicEffect, context: &SourceContext) -> GateVerdict {
    let Some(gate) = atom
        .requires_expression
        .as_deref()
        .filter(|g| !g.is_empty())
    else {
        return GateVerdict::Holds;
    };
    match eval_bool(gate, context) {
        Ok(true) => GateVerdict::Holds,
        Ok(false) => GateVerdict::Fails,
        Err(EvalError::Indeterminate(reader)) if *reader == *crate::expr::RANK_READER => {
            GateVerdict::Unknown {
                reason: format!("depends on {reader}, and no target rank is chosen"),
                waits_on_rank: true,
            }
        }
        Err(EvalError::Indeterminate(reader)) => GateVerdict::Unknown {
            reason: format!("depends on {reader}, which this context does not model"),
            waits_on_rank: false,
        },
        // Malformed is a grammar gap, not an unknown target — no real gate reaches it,
        // so seeing one here is news and says so in the row.
        Err(EvalError::Malformed(problem)) => GateVerdict::Unknown {
            reason: format!("gate is malformed: {problem}"),
            waits_on_rank: false,
        },
    }
}
