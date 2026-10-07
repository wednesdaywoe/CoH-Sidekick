//! Attack-chain data wiring (RB5-c): the live build's projections as [`ChainPower`]s.
//!
//! The only part of the chain that touches build/data; pure scheduling lives in
//! [`crate::chain`]. Every number here is read from [`PowerProjection`], the same resolve the
//! info panel renders, so the chain's numbers match the rest of the app by construction, not by
//! parallel derivation (the beta re-ran its calc functions here; the projection IS that run).
//!
//! This is where a rolled component's probability is folded in. A chain's DPS averages over
//! many hits by definition, so a `Chance(p)` component contributes `p × damage`, the one
//! consumer the RB5-a boundary allows to average. Dormant components stay at zero (an inert
//! effect is inert on average too), and a component whose gate the combat context couldn't
//! answer contributes NOTHING but is counted on [`ChainPower::unresolved_damage`], so the modal
//! can say the number is incomplete instead of quietly reading low (with no target chosen,
//! that's every target-gated row).
//!
//! No alternate-form tables. The beta wired Energy Transfer's fast cast, the fast snipe's
//! ToHit window and Assassin's Strike's from-Hide opener through hardcoded power-name tables
//! (`POWER_FORMS` / `CHARGE_GRANTS`, which Rule 0 forbids porting). Here a power enters the
//! chain as [`crate::effective::effective_power`] resolves it under the build's CURRENT combat
//! context, the same snipe form the info panel shows, because the projection already runs that
//! transform.
//!
//! The snipe's ToHit window is reached that way now, not by a table: the forks gate their
//! fast form on `cur.kToHit source> .97 >=`, the export carries that condition, and the
//! projection evaluates it against the build's own to-hit. So a to-hit team buff switches the
//! form and moves the rotation, and the chain offers a control for it wherever some scheduled
//! power's gate reads to-hit (CHAIN-1's third bullet). Still missing is per-cast switching
//! WITHIN one rotation: a charge spent by a later cast, a from-Hide opener. NOT for want of
//! wire data, since the grant, the spend and the selecting condition all ship (Total Focus's
//! `Grant_Power` on `…Energy_Store`, Stun's and Barrage's `Revoke_Power` on it, Energy Transfer's
//! own redirect conditions). The FORM ships too as of 2026-08-07: `formVariants` carries the
//! ownership-selected pair the interrupt-keyed and `Source.Mode?` detectors couldn't read, and
//! `effective::with_form_variant` selects it. A build can DECLARE the charge as of 2026-08-07,
//! because the conditional toggles stamp what state they assert and `gather::owned_powers` folds
//! it in, so the charged branch is reachable and the combat toggles are the lever for it. What
//! no declaration can express is a charge banked and spent WITHIN one rotation, the per-cast
//! switching this file still doesn't model (DATA-GAP-REGISTER CHAIN-1, corrected 2026-08-07).

use crate::chain::{
    find_slot_in_lanes, identity_lanes, Activation, ChainDot, ChainPower, ChainPowerKind,
    EffectWindow, EffectWindowKind, EnduranceParams,
};
use crate::chain_walk::{self, WalkState};
use crate::damage::DamageApplication;
use crate::finalize::CalculatedTotals;
use crate::projection::{PowerProjection, ReductionClamps, StrengthBounds};
use crate::window_slots::Fallback;
use coh_data::grant_edges::GrantOp;
use coh_data::{CharacterState, Power, PowerDatabase, SelectedPower};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};

/// Self-buff registry keys whose presence marks an offensive click buff (Build Up / Aim /
/// Soul Drain / Follow Up), the beta's `SELF_BUFF_KEYS`. Registry vocabulary, not game proper
/// nouns.
///
/// Not `perma::SELF_BUFF_KEYS`, which shares the name and is twenty-odd keys wide; these three
/// are clean of foe-directed values on every fork (a foe −ToHit routes to `tohitDebuff`, a slot
/// of its own), so presence really does name the caster here. The wide set carries no such
/// guarantee. Keep the two apart before borrowing a claim from either.
const SELF_BUFF_KEYS: [&str; 3] = ["damageBuff", "tohitBuff", "rechargeBuff"];

/// Foe-debuff registry keys that warrant a duration window on the timeline, the beta's curated
/// `FOE_DEBUFF_KEYS`. Deliberately a subset of the registry's `category: 'debuff'` entries:
/// `enduranceCrash` is a self-penalty crash and `enduranceDrain` / `specialDebuff` are instant
/// or odd-shaped, none a maintained foe debuff.
const FOE_DEBUFF_KEYS: [&str; 11] = [
    "tohitDebuff",
    "defenseDebuff",
    "resistanceDebuff",
    "damageDebuff",
    "regenDebuff",
    "recoveryDebuff",
    "rechargeDebuff",
    "accuracyDebuff",
    "slow",
    "threatDebuff",
    "perceptionDebuff",
];

/// Everything the Attack-Chain modal needs, derived once per recalculation.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChainInputs {
    /// Chain candidates: the click powers with a cast time that the build can cast in the
    /// requested form, in bucket order.
    pub powers: Vec<ChainPower>,
    /// The archetype's recharge net-strength clamp. `None` when no archetype is chosen or the
    /// dataset ships no caps for it. The modal reports that rather than the engine inventing
    /// bounds (Rule 1).
    pub bounds: Option<StrengthBounds>,
    /// The endurance simulation's character-level inputs. `None` for the same reason
    /// [`Self::bounds`] is: without the archetype's exported endurance pool there's no bar to
    /// drain, and inventing a 100-point one would be a hardcode of a value the export owns.
    pub endurance: Option<EnduranceParams>,
    /// The build's global recharge percent, as the totals report it. That INCLUDES whatever the
    /// what-if team-buff layer injected, because the injection lands in the accumulator before
    /// projection ([`crate::what_if`]). The chain therefore folds this in once and adds nothing
    /// on top; a surface that wants the un-simulated figure subtracts the layer's own
    /// `recharge` entry rather than reading a second field.
    pub build_global_recharge_pct: f64,
    /// Whether each power's damage carries its slotted damage procs' average. Kept on the inputs
    /// so the per-cast walk re-derives its rows the same way the base rows were derived.
    pub include_proc_damage: bool,
    /// The what-if stats that actually move a chain number, each with the value the chain is
    /// computing with now and the archetype's own ceiling for it. That's everything a
    /// chain-side control needs, so no surface has to know which stats those are. See
    /// [`chain_sensitive_buffs`].
    pub what_if: Vec<ChainWhatIf>,
}

/// Derive the chain inputs from the finalized totals. Walks the four picked buckets
/// (primary / secondary / pools / epic; inherents can't sit in an attack chain, matching the
/// beta's candidate set) and keeps Click powers with a cast time.
///
/// `form` is the caster mode the chain is being built inside, from [`coh_data::form_modes`];
/// `None` is the build's default form. The totals must have been computed for the same mode,
/// since a form suspends the human toggles and redirects the attacks it replaces. Pass a state
/// whose `combat.active_modes` holds `form` to both.
///
/// `include_proc_damage` adds each power's average slotted damage-proc damage to its per-cast
/// damage ([`crate::procs::proc_damage_per_cast`]); it is the reader's choice, not the build's.
pub fn chain_inputs(
    state: &CharacterState,
    totals: &CalculatedTotals,
    db: &PowerDatabase,
    form: Option<&str>,
    include_proc_damage: bool,
) -> ChainInputs {
    let forms = coh_data::form_modes(state, db);
    let mut powers = Vec::new();
    let mut reach = ChainReach::default();
    let mut add_bucket = |selections: &[SelectedPower]| {
        for selection in selections {
            let Some(projection) = totals.power_projection.iter().find(|p| {
                p.power_set == selection.powerset
                    && p.power_internal_name == selection.internal_name
            }) else {
                continue;
            };
            let Some(def) =
                crate::gather::resolve_power(db, &selection.powerset, &selection.internal_name)
            else {
                continue;
            };
            // Costing no pick says nothing about whether a power can be cast in a rotation:
            // the Kheldian form attacks are granted and ARE the rotation in their form. What
            // separates them from the free riders a travel power hands out (Double Jump,
            // Translocation, Jaunt) is that the game lets you slot them. The free riders
            // carry an empty `boostsAllowed` in the export.
            if selection.is_locked && !def.accepts_enhancements() {
                continue;
            }
            if !coh_data::castable_in_mode(def, form, &forms) {
                continue;
            }
            if let Some(mut power) = chain_power(selection, projection, def, state.dataset) {
                if include_proc_damage {
                    let procs = crate::procs::slotted_procs(
                        state,
                        db,
                        &selection.powerset,
                        &selection.internal_name,
                    );
                    power.damage += crate::procs::proc_damage_per_cast(&procs);
                }
                // Asked of the powers that actually got SCHEDULED: a snipe the build holds but
                // can't cast in this form reaches no number here, so it earns no control.
                reach.to_hit_selects_a_fast_form |=
                    crate::effective::fast_form_reads_caster_to_hit(def);
                powers.push(power);
            }
        }
    };
    add_bucket(&state.primary.powers);
    add_bucket(&state.secondary.powers);
    for pool in &state.pools {
        add_bucket(&pool.powers);
    }
    if let Some(epic) = &state.epic_pool {
        add_bucket(&epic.powers);
    }

    let bounds = ReductionClamps::for_build(state, db).recharge;
    let caps = state
        .archetype
        .id
        .as_deref()
        .and_then(|archetype| db.archetype_stats.get(archetype));

    ChainInputs {
        powers,
        bounds,
        endurance: endurance_params(totals),
        build_global_recharge_pct: totals.bonuses.recharge,
        include_proc_damage,
        what_if: chain_what_if(
            totals,
            caps,
            i32::from(state.level),
            &state.combat.what_if_buffs,
            reach,
        ),
    }
}

/// The what-if stats a chain's numbers move with, and how to read each one.
///
/// The list is MEASURED, not asserted: `chain_sensitivity_gate` perturbs every key
/// [`crate::what_if::vocabulary`] offers and holds this table to exactly the set that changes a
/// chain number. So a chain that starts consuming a new stat, the day the ToHit-gated snipe
/// form reaches it (DATA-GAP-REGISTER CHAIN-1), fails the gate until the stat is added here,
/// and a stat that stops mattering can't linger as a control that does nothing.
///
/// The three accessors travel together deliberately. A bare list of names would let a stat
/// arrive without a way to read its current value or its ceiling, and the surface would fill
/// the gap with a hardcoded 100-point bar or a made-up slider range: the exact hardcode Rule 0
/// forbids, since the export owns both numbers.
///
/// A stat's reach can also depend on the BUILD rather than being universal, and
/// [`Self::reaches`] carries that. See [`ChainReach`].
struct ChainBuff {
    /// The `GlobalBonuses` field name, the what-if layer's whole vocabulary.
    stat: &'static str,
    /// What the chain is computing with right now, in the layer's units. Already simulated: the
    /// injection landed before projection, so this INCLUDES the layer's own entry.
    current: fn(&CalculatedTotals, Option<&coh_data::ArchetypeCaps>, i32) -> f64,
    /// The RAW accumulator total for the same stat: build plus layer, before any ceiling.
    /// Subtracting the layer's entry from [`Self::current`] instead would report nonsense the
    /// moment a ceiling binds: a +5000% recovery what-if against a +400% ceiling would have the
    /// surface reading "Build −4600%".
    accumulated: fn(&CalculatedTotals) -> f64,
    /// The archetype's ceiling for this stat in the same units, or `None` when the dataset
    /// ships no caps for the build's class.
    ceiling: fn(Option<&coh_data::ArchetypeCaps>, i32) -> Option<f64>,
    /// Whether this stat reaches THIS build's chain at all. Most reach every chain and answer
    /// `true` unconditionally, since a rotation always has damage to deal, activations to pay
    /// for and recharge to wait on.
    reaches: fn(ChainReach) -> bool,
}

/// What a chain candidate walk found that decides a stat's reach beyond the universal ones.
///
/// One field so far, and it's measured off the powers in hand rather than assumed from the
/// dataset: a build whose rotation holds no to-hit-gated fast form gets no to-hit control,
/// whether that's because the fork authors its snipes on a different gate or because this
/// particular build never picked one.
#[derive(Debug, Clone, Copy, Default)]
struct ChainReach {
    /// Some scheduled power's fast form is selected by the caster's own to-hit, so moving
    /// to-hit can switch the form and with it every number the rotation reads off that power
    /// ([`crate::effective::fast_form_reads_caster_to_hit`]).
    to_hit_selects_a_fast_form: bool,
}

/// A reduction aspect's strength cap as a BUFF percentage: the game clamps `1 + enh + global`
/// to `cap`, so the most a global can ever be worth is `(cap − 1) × 100`.
fn strength_cap_as_buff_percent(cap: f64) -> f64 {
    (cap - 1.0) * 100.0
}

const CHAIN_BUFFS: &[ChainBuff] = &[
    ChainBuff {
        stat: "damage",
        current: |totals, _, _| totals.bonuses.damage,
        accumulated: |totals| totals.bonuses.damage,
        ceiling: |caps, _| caps.map(|caps| strength_cap_as_buff_percent(caps.damage_cap)),
        reaches: |_| true,
    },
    ChainBuff {
        // The endurance-DISCOUNT accumulator (`endrdx`), which divides every activation cost.
        stat: "endurance",
        current: |totals, _, _| totals.bonuses.endurance,
        accumulated: |totals| totals.bonuses.endurance,
        ceiling: |caps, _| caps.map(|caps| strength_cap_as_buff_percent(caps.endurance_cap)),
        reaches: |_| true,
    },
    ChainBuff {
        // Absolute endurance points, not a percentage, so its "current" is the pool the
        // simulation drains, measured over the archetype's own base pool.
        stat: "maxEndurance",
        current: |totals, caps, level| {
            let base = caps
                .and_then(|caps| caps.max_endurance_at_level(level))
                .map(|(base, _)| base);
            match base {
                Some(base) => totals.stats.max_endurance_absolute - base,
                None => totals.bonuses.max_endurance,
            }
        },
        accumulated: |totals| totals.bonuses.max_endurance,
        ceiling: |caps, level| {
            caps.and_then(|caps| caps.max_endurance_at_level(level))
                .map(|(base, cap)| cap - base)
        },
        reaches: |_| true,
    },
    ChainBuff {
        stat: "recharge",
        current: |totals, _, _| totals.bonuses.recharge,
        accumulated: |totals| totals.bonuses.recharge,
        ceiling: |caps, _| caps.map(|caps| strength_cap_as_buff_percent(caps.recharge_cap)),
        reaches: |_| true,
    },
    ChainBuff {
        stat: "recovery",
        // The CLAMPED figure, because that's what the endurance simulation runs on.
        current: |totals, _, _| totals.stats.recovery,
        accumulated: |totals| totals.bonuses.recovery,
        ceiling: |caps, level| caps.and_then(|caps| caps.recovery_buff_cap_at_level(level)),
        reaches: |_| true,
    },
    ChainBuff {
        // Reaches a chain only through a fast form the caster's to-hit selects: the form swaps
        // in its own cast time, damage and atoms, so the rotation's timing and DPS both move.
        // Nothing else in a chain reads to-hit. The schedule never rolls to hit, and damage is
        // scheduled at its full value (RB5-a's user-set boundary).
        stat: "toHit",
        // UNCLAMPED, because that's the figure the fast-form gate is evaluated against
        // (`crate::projection`'s `gate_context` reads `AttribBase` + this buff, matching the
        // beta's `permanentToHit`). A control reporting the clamped one would name a value the
        // chain isn't using.
        current: |totals, _, _| totals.bonuses.to_hit,
        accumulated: |totals| totals.bonuses.to_hit,
        ceiling: |caps, level| caps.and_then(|caps| caps.to_hit_buff_cap_at_level(level)),
        reaches: |reach| reach.to_hit_selects_a_fast_form,
    },
];

/// One chain-side what-if control's data: the stat, where it stands, and where it stops.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ChainWhatIf {
    pub stat: &'static str,
    /// The value the chain is computing with, in the layer's units, layer included and any
    /// ceiling applied.
    pub current: f64,
    /// What the build reaches WITHOUT the layer, measured off the raw accumulator, so it stays
    /// the build's own figure even when [`Self::current`] is sitting on a ceiling.
    pub from_build: f64,
    /// The archetype's ceiling for this stat, or `None` when the dataset ships none. A control
    /// then spans no bounded range rather than a guessed one.
    pub ceiling: Option<f64>,
}

/// Every what-if stat a chain's numbers can move with: the SUPERSET, for a surface that needs
/// to know it can render a control for each.
///
/// Which of them a given build actually offers is [`ChainInputs::what_if`], and it can be
/// narrower: a stat whose reach depends on the build ([`ChainReach`]) drops out when nothing in
/// the rotation reaches it. A surface that renders this list directly would show a control that
/// changes nothing.
pub fn chain_sensitive_buffs() -> Vec<&'static str> {
    CHAIN_BUFFS.iter().map(|buff| buff.stat).collect()
}

fn chain_what_if(
    totals: &CalculatedTotals,
    caps: Option<&coh_data::ArchetypeCaps>,
    level: i32,
    layer: &std::collections::BTreeMap<String, f64>,
    reach: ChainReach,
) -> Vec<ChainWhatIf> {
    CHAIN_BUFFS
        .iter()
        .filter(|buff| (buff.reaches)(reach))
        .map(|buff| ChainWhatIf {
            stat: buff.stat,
            current: (buff.current)(totals, caps, level),
            from_build: (buff.accumulated)(totals) - layer.get(buff.stat).copied().unwrap_or(0.0),
            ceiling: (buff.ceiling)(caps, level),
        })
        .collect()
}

/// One selection as a chain candidate, or `None` when it can't sit in a chain (not a Click,
/// or no cast time to schedule).
fn chain_power(
    selection: &SelectedPower,
    projection: &PowerProjection,
    def: &Power,
    dataset: coh_data::DatasetId,
) -> Option<ChainPower> {
    if !power_type_is(def, "click") {
        return None;
    }
    let cast = projection.arcana_time?;
    // The raw (pre-ArcanaTime) cast time decides whether a DoT ticks during the swing or
    // lingers after it. The beta compares the DoT's duration against it with a 50ms slack.
    let raw_cast = projection.cast_time.map(|tier| tier.base).unwrap_or(0.0);

    // `StrengthsDisallowed` drops slotted and global recharge, `GlobalStrengthsDisallowed` only
    // the global; slotted recharge still shortens a Kuji-In Rin.
    let recharge_locked = crate::perma::disallows_recharge(def, "strengthsDisallowed");
    let ignores_global_recharge =
        recharge_locked || crate::perma::disallows_recharge(def, "globalStrengthsDisallowed");
    let recharge_enhancement = if recharge_locked {
        0.0
    } else {
        projection
            .enhancement_bonuses
            .get("recharge")
            .copied()
            .unwrap_or(0.0)
    };

    let mut damage = 0.0;
    let mut dots = Vec::new();
    for component in &projection.damage.components {
        let application_chance = match component.application {
            DamageApplication::Always => 1.0,
            DamageApplication::Chance(probability) => probability,
            DamageApplication::Dormant => continue,
        };
        match component.over_time {
            Some(over_time) if over_time.duration > raw_cast + 0.05 => {
                // Lingers past the animation: drawn as ticks, truncated at the loop
                // boundary by the schedule. The per-tick enhanced value is the total with
                // the expectation divided back out.
                if over_time.expected_ticks > 0.0 {
                    dots.push(ChainDot {
                        ticks: over_time.nominal_ticks,
                        period: over_time.period,
                        per_tick: component.total.r#final / over_time.expected_ticks,
                        chance: over_time.tick_chance,
                        cancel_on_miss: over_time.cancel_on_miss,
                        application_chance,
                    });
                }
            }
            // In-cast DoT and instant components fold into the per-cast damage at their
            // expected value, the honest average a chain's DPS is made of.
            _ => damage += application_chance * component.total.r#final,
        }
    }

    let (self_buff, foe_debuff) = atom_windows(def, dataset);
    let effect_window = if self_buff > 0.0 {
        Some(EffectWindow {
            kind: EffectWindowKind::Buff,
            duration: self_buff,
        })
    } else if foe_debuff > 0.0 {
        Some(EffectWindow {
            kind: EffectWindowKind::Debuff,
            duration: foe_debuff,
        })
    } else {
        None
    };

    // A power whose damage rows are all unresolved is still an attack. With no target chosen
    // that's every target-gated attack, and classifying it "utility" would silently demote
    // the whole palette (Rule 1: the gap is reported, not re-labelled).
    let is_attack = !projection.damage.is_empty() || !dots.is_empty();
    let kind = if is_attack {
        ChainPowerKind::Attack
    } else if self_buff > 0.0 {
        ChainPowerKind::Buff
    } else {
        ChainPowerKind::Utility
    };

    let endurance_gain = projection
        .granted_magnitudes
        .iter()
        .find(|row| row.effect_key == "enduranceGain")
        .map(|row| row.value.r#final.max(0.0))
        .unwrap_or(0.0);

    Some(ChainPower {
        id: selection.address(),
        name: display_name(def),
        kind,
        cast,
        base_recharge: projection.recharge.map(|tier| tier.base).unwrap_or(0.0),
        recharge_enhancement,
        ignores_global_recharge,
        endurance_cost: projection
            .endurance_cost
            .map(|tier| tier.r#final)
            .unwrap_or(0.0),
        endurance_gain,
        damage,
        dots,
        effect_window,
        unresolved_damage: projection.damage.unresolved.len(),
    })
}

/// The character endurance parameters for the sustainability sim, all three read off the
/// totals the build already computed ([`crate::projection::toggle_endurance_total`] fills the
/// drain at Step 9.7). The chain held its own copy of that sum while no build-wide pass
/// existed; reading the graded field instead is what keeps a rotation's sustainability answer
/// and the dashboard's Net End row from being two numbers that can disagree.
/// The endurance bar and the rate it refills at, read from the FINALIZED stats rather than the
/// raw accumulator.
///
/// The pool is the archetype's own exported base plus its buffs, clamped to the exported
/// per-level pool ceiling (CAPS-1). Every shipped class authors a flat 100-point base today, so
/// what changes a number here is the CLAMP, but the base is read from the export anyway,
/// because a literal 100 is a value the export owns. The recovery percentage is likewise the
/// clamped one, so the sim can't run on a rate the dashboard shows bound to a ceiling.
///
/// `None` when the pool is unknown (no archetype yet, or a dataset shipping no caps for it):
/// there's no bar to drain, and the modal says so rather than draining an invented one.
fn endurance_params(totals: &CalculatedTotals) -> Option<EnduranceParams> {
    let max_endurance = totals.stats.max_endurance_absolute;
    if max_endurance <= 0.0 {
        return None;
    }
    Some(EnduranceParams {
        max_endurance,
        // Base recovery refills the bar in 60s; +Recovery scales the rate.
        recovery_per_second: (max_endurance / 60.0) * (1.0 + totals.stats.recovery / 100.0),
        toggle_per_second: totals.bonuses.toggle_end_cost,
    })
}

fn power_type_is(def: &Power, kind: &str) -> bool {
    def.extra
        .get("powerType")
        .and_then(Value::as_str)
        .is_some_and(|t| t.eq_ignore_ascii_case(kind))
}

fn display_name(def: &Power) -> String {
    def.name.clone()
}

/// The chain's effect windows read from the power's atoms instead of the effects bag:
/// `(self_buff_window, foe_debuff_window)`, composed exactly as [`chain_power`] composes the
/// bag pair — the foe window is asked only when no self-buff window and no self-directed
/// penalty claims the power. [`crate::window_slots`] is the per-key converter mirror behind
/// both answers; `chain_window_atom_bag_parity` holds this to the bag it replaces across every
/// click power of all three bundles.
pub fn atom_windows(def: &Power, dataset: coh_data::DatasetId) -> (f64, f64) {
    let slots = crate::window_slots::window_slots(def, dataset);
    let self_buff = slots.window_duration(&SELF_BUFF_KEYS, &[Fallback::BuffDuration]);
    let foe_debuff = if self_buff > 0.0 || slots.has_self_directed_penalty() {
        0.0
    } else {
        slots.window_duration(
            &FOE_DEBUFF_KEYS,
            &[Fallback::EffectDuration, Fallback::BuffDuration],
        )
    };
    (self_buff, foe_debuff)
}

/// A working index-sequence → the stable id list a saved chain stores. Storing indices would
/// break the moment the build's power set changes; ids survive reload, export and share, and a
/// saved chain gracefully drops any power no longer picked.
pub fn sequence_to_ids(powers: &[ChainPower], sequence: &[usize]) -> Vec<String> {
    sequence
        .iter()
        .filter_map(|&index| powers.get(index).map(|power| power.id.clone()))
        .collect()
}

/// A saved chain's id list → an index-sequence for the CURRENT build's chain powers.
/// Matches each id exactly, then falls back to its internal-name half (a pool reshuffle
/// changes the set id, not the power); ids with no surviving power are dropped.
pub fn ids_to_sequence(powers: &[ChainPower], ids: &[String]) -> Vec<usize> {
    let name_of = |id: &str| id.rsplit(':').next().unwrap_or(id).to_string();
    ids.iter()
        .filter_map(|id| {
            powers.iter().position(|power| &power.id == id).or_else(|| {
                let name = name_of(id);
                powers.iter().position(|power| name_of(&power.id) == name)
            })
        })
        .collect()
}

// ============================================================================
// RB5-d — the per-activation schedule
// ============================================================================

/// One position's walk annotations, for the modal's per-activation markers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CastNote {
    /// The position's state selected a row other than the base one — an alternate form.
    pub form_changed: bool,
    /// The hide meter's state when this cast STARTED. `None` for a build with no meter axis.
    pub hidden: Option<bool>,
    /// Grant paths this cast banked (its applied grant edges).
    pub banked: Vec<String>,
    /// Grant paths this cast spent (its applied revoke edges).
    pub spent: Vec<String>,
}

/// A packed rotation resolved per activation (RB5-d): each cast scheduled with the form its
/// POSITION selects, not the form the whole-rotation toggles select once.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ScheduledChain {
    /// The base candidate rows first (index-compatible with [`ChainInputs::powers`]), then one
    /// appended row per alternate form some activation resolved to.
    pub powers: Vec<ChainPower>,
    /// Row → recharge lane (the base row's index): a power's forms share its one timer.
    pub lanes: Vec<usize>,
    /// The schedule. `power_index` points into [`Self::powers`], so every downstream consumer
    /// (damage, endurance, the timeline) reads the form actually cast with no further lookup.
    pub activations: Vec<Activation>,
    /// Per SEQUENCE POSITION (not per sorted activation — join on
    /// [`Activation::sequence_index`]): what the walk did there.
    pub notes: Vec<Option<CastNote>>,
    /// The meter clock could not be read (a stale bundle): the meter axis was not modelled,
    /// and the modal says so beside the numbers rather than quietly scheduling without it.
    pub meter_error: Option<String>,
    /// Grant-edge decode failures and context-resolve gaps, verbatim (Rule 1).
    pub edge_errors: Vec<String>,
    /// Edges whose condition the position context could not answer (nothing applied).
    pub edges_indeterminate: usize,
    /// Edges rolling below certainty (nothing applied; a ledger holds no fractional charge).
    pub edges_probabilistic: usize,
}

/// A position context's identity, for memoizing the per-context resolve. The overlay rides as
/// bit-exact pairs: two positions whose ledgers agree produce identical keys, and no rounding
/// can merge two that differ.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CtxKey {
    hidden: Option<bool>,
    overlay: Vec<(String, u64)>,
}

impl CtxKey {
    fn new(hidden: Option<bool>, overlay: &BTreeMap<String, f64>) -> Self {
        CtxKey {
            hidden,
            overlay: overlay
                .iter()
                .map(|(path, count)| (path.clone(), count.to_bits()))
                .collect(),
        }
    }

    fn overlay_map(&self) -> HashMap<String, f64> {
        self.overlay
            .iter()
            .map(|(path, bits)| (path.clone(), f64::from_bits(*bits)))
            .collect()
    }
}

/// Memoized per-context candidate rows. The base context answers from the rows already in
/// hand; any other context is a full [`crate::recalculate_in_cast_context`] run through the
/// same [`chain_inputs`] derivation, so a form row's numbers come from the one resolve the
/// info panel renders, never a second calculator. A rotation visits a handful of contexts.
struct FormBank<'a> {
    state: &'a CharacterState,
    db: &'a PowerDatabase,
    form: Option<&'a str>,
    include_proc_damage: bool,
    base_key: CtxKey,
    base_rows: &'a [ChainPower],
    resolved: HashMap<CtxKey, Vec<ChainPower>>,
}

impl<'a> FormBank<'a> {
    fn row_for(&mut self, key: &CtxKey, base_index: usize) -> Option<ChainPower> {
        if *key == self.base_key {
            return self.base_rows.get(base_index).cloned();
        }
        let state = self.state;
        let db = self.db;
        let form = self.form;
        let include_proc_damage = self.include_proc_damage;
        let rows = self.resolved.entry(key.clone()).or_insert_with(|| {
            let mut context_state = state.clone();
            if let Some(hidden) = key.hidden {
                context_state.combat.hidden = hidden;
            }
            let overlay = key.overlay_map();
            let totals = crate::recalculate_in_cast_context(&context_state, db, &[], &overlay);
            chain_inputs(&context_state, &totals, db, form, include_proc_damage).powers
        });
        let id = &self.base_rows.get(base_index)?.id;
        rows.iter().find(|row| &row.id == id).cloned()
    }
}

/// The defs behind the chain candidates, keyed by the candidate id, for the walk's grant-edge
/// and meter-atom reads. Walks the same four buckets [`chain_inputs`] walks.
fn candidate_defs<'a>(state: &CharacterState, db: &'a PowerDatabase) -> HashMap<String, &'a Power> {
    let mut defs = HashMap::new();
    let mut add_bucket = |selections: &[SelectedPower]| {
        for selection in selections {
            if let Some(def) =
                crate::gather::resolve_power(db, &selection.powerset, &selection.internal_name)
            {
                defs.insert(selection.address(), def);
            }
        }
    };
    add_bucket(&state.primary.powers);
    add_bucket(&state.secondary.powers);
    for pool in &state.pools {
        add_bucket(&pool.powers);
    }
    if let Some(epic) = &state.epic_pool {
        add_bucket(&epic.powers);
    }
    defs
}

/// Schedule a pick-order sequence with per-cast state (RB5-d): the walk banks and spends
/// grant edges, runs the hide-meter clock, and resolves each activation's FORM against the
/// state at its own position — a charge Total Focus banked selects Energy Transfer's charged
/// branch two casts later, the opener resolves from Hide, and a post-Placate cast resolves
/// inside the granted window.
///
/// `state` must be the same (form-adjusted) state `base` was derived from, and `form` the
/// same chosen form. The feedback loop the stream doc names — form choice moves cast times,
/// cast times move the meter windows — is closed by resolving at the candidate start, then
/// re-slotting with the resolved cast and re-resolving once if the start moved; state changes
/// only at cast boundaries, so the fixed point lands within the bounded retries.
pub fn schedule_chain(
    state: &CharacterState,
    db: &PowerDatabase,
    base: &ChainInputs,
    form: Option<&str>,
    sequence: &[usize],
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> ScheduledChain {
    let base_len = base.powers.len();
    let mut powers = base.powers.clone();
    let mut lanes = identity_lanes(base_len);
    let mut activations: Vec<Activation> = Vec::with_capacity(sequence.len());
    let mut notes: Vec<Option<CastNote>> = vec![None; sequence.len()];
    let mut edge_errors = Vec::new();
    let mut edges_indeterminate = 0;
    let mut edges_probabilistic = 0;

    let (meter, meter_error) = match chain_walk::hide_meter_clock(state, db) {
        Ok(clock) => (clock, None),
        Err(problem) => (None, Some(problem)),
    };
    let declared_hidden = state.combat.hidden;
    let mut walk = WalkState::new(meter);

    let defs = candidate_defs(state, db);
    let base_owned = crate::gather::owned_powers(state, db);
    let source_modes = crate::gather::gather_active_powers(state, db).source_modes;
    let (archetype_class, entity_type) = state.combat.target_identity();
    let target = Some(crate::expr::TargetIdentity {
        archetype_class,
        entity_type: entity_type.to_string(),
    });

    let base_key = CtxKey::new(meter.map(|_| declared_hidden), &BTreeMap::new());
    let mut bank = FormBank {
        state,
        db,
        form,
        include_proc_damage: base.include_proc_damage,
        base_key,
        base_rows: &base.powers,
        resolved: HashMap::new(),
    };

    for (sequence_index, &base_index) in sequence.iter().enumerate() {
        if base_index >= base_len {
            continue;
        }
        let mut row = base_index;
        let mut start = find_slot_in_lanes(
            &powers,
            &lanes,
            &activations,
            row,
            global_recharge_pct,
            bounds,
        );
        for _ in 0..3 {
            let overlay = walk.owned_overlay_at(start);
            let key = CtxKey::new(walk.hidden_at(start, declared_hidden), &overlay);
            let resolved = match bank.row_for(&key, base_index) {
                Some(resolved) => resolved,
                None => {
                    // The context resolve dropped the power (it can happen only if a form
                    // makes it uncastable). Reported, and the base row stands.
                    edge_errors.push(format!(
                        "{}: absent from its position's context resolve; scheduled as the base form",
                        base.powers[base_index].id
                    ));
                    base.powers[base_index].clone()
                }
            };
            let new_row = if resolved == powers[base_index] {
                base_index
            } else if let Some(existing) = powers[base_len..]
                .iter()
                .position(|candidate| *candidate == resolved)
            {
                base_len + existing
            } else {
                powers.push(resolved);
                lanes.push(base_index);
                powers.len() - 1
            };
            let new_start = find_slot_in_lanes(
                &powers,
                &lanes,
                &activations,
                new_row,
                global_recharge_pct,
                bounds,
            );
            let settled = new_row == row && (new_start - start).abs() < 1e-9;
            row = new_row;
            start = new_start;
            if settled {
                break;
            }
        }
        let end = start + powers[row].cast;
        let hidden_at_start = walk.hidden_at(start, declared_hidden);
        activations.push(Activation {
            power_index: row,
            start,
            end,
            sequence_index,
        });
        activations.sort_by(|a, b| a.start.total_cmp(&b.start));

        // The cast's own writes, at its end (where the game anchors recharge and grants):
        // grant edges and a granted re-hide window first, then the attack event — a cast
        // that both banks and attacks banks into the state its own attack then suppresses.
        let mut banked = Vec::new();
        let mut spent = Vec::new();
        if let Some(def) = defs.get(&base.powers[base_index].id) {
            let env = chain_walk::EdgeEnv {
                base_owned: base_owned.clone(),
                source_modes: source_modes.clone(),
                in_combat: state.combat.in_combat,
                hidden: walk.hidden_at(end, declared_hidden),
                target: target.clone(),
            };
            let mut applied = Vec::new();
            match walk.apply_cast_edges(def, end, &env, &mut applied) {
                Ok(outcome) => {
                    edges_indeterminate += outcome.indeterminate;
                    edges_probabilistic += outcome.probabilistic;
                }
                Err(problem) => edge_errors.push(problem),
            }
            for (op, path) in applied {
                match op {
                    GrantOp::Grant => banked.push(path),
                    GrantOp::Revoke => spent.push(path),
                }
            }
            if let Some(window) = chain_walk::self_meter_window(def) {
                walk.note_self_meter_grant(end, window);
            }
        }
        if powers[row].kind == ChainPowerKind::Attack {
            walk.note_attack(end);
        }
        notes[sequence_index] = Some(CastNote {
            form_changed: row != base_index,
            hidden: hidden_at_start,
            banked,
            spent,
        });
    }

    ScheduledChain {
        powers,
        lanes,
        activations,
        notes,
        meter_error,
        edge_errors,
        edges_indeterminate,
        edges_probabilistic,
    }
}
