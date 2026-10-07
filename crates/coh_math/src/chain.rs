//! Attack-chain scheduling and endurance simulation (RB5-c) — the pure timing model.
//!
//! No data dependencies: [`crate::chain_build`] derives [`ChainPower`]s from the live build's
//! projections and feeds them here. This module only does the *scheduling* (a single-animation
//! greedy packer, mirroring the in-game constraint that one power animates at a time and a
//! power's recharge does not begin until its cast animation finishes — so its solo repeat
//! period is cast + recharge) and the *endurance simulation* (continuous recovery − toggle
//! drain, minus the lump endurance cost paid at each cast — a spiky rotation can bottom out
//! even when its average net is positive). Ported from the beta's `attack-chain.ts`, which is
//! pure in exactly the same way.
//!
//! This is the ONE place a rolled damage component's probability is folded into a number.
//! [`crate::damage`] keeps `Chance(p)` apart from the certain total because an averaged
//! per-hit number is not any hit that ever lands — but a chain's DPS is an average over many
//! hits by definition, so `p × damage` is the honest contribution here (the RB5-a boundary,
//! user-set). The fold itself happens in [`crate::chain_build`]; this module receives the
//! already-averaged per-cast damage.
//!
//! The one type it borrows is [`StrengthBounds`], which lives beside `ThreeTier::reduction` —
//! the primitive the per-power display divides with — so the chain's recharge divisor and the
//! displayed one cannot drift apart. That is a type, not a data dependency: still no bin here.

use crate::projection::StrengthBounds;

/// How a power behaves in the chain — drives palette grouping and bar colour only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ChainPowerKind {
    Attack,
    Buff,
    Utility,
}

/// Damage that ticks AFTER the cast finishes, drawn as tick marks on the timeline. DoT that
/// lands during the cast is already folded into [`ChainPower::damage`].
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct ChainDot {
    /// Nominal tick count (every tick, whether or not it lands).
    pub ticks: f64,
    /// Seconds between ticks.
    pub period: f64,
    /// Enhanced damage per tick.
    pub per_tick: f64,
    /// Per-tick apply chance when the DoT rolls one. `None` = every tick lands.
    pub chance: Option<f64>,
    /// Whether a missed tick cancels the remaining chain (geometric decay).
    pub cancel_on_miss: bool,
    /// The component's own application roll (`Chance(p)`), gating the whole DoT. 1.0 for a
    /// certain component.
    pub application_chance: f64,
}

impl ChainDot {
    /// Probability that tick `t` (1-indexed) actually lands: the application roll times the
    /// per-tick chance — `chance^t` for cancel-on-miss (tick t needs every prior tick to have
    /// hit), a flat `chance` for independent ticks, 1 for unconditional ones.
    pub fn tick_probability(&self, t: u32) -> f64 {
        let per_tick = match self.chance {
            Some(chance) if chance > 0.0 && chance < 1.0 => {
                if self.cancel_on_miss {
                    chance.powi(t as i32)
                } else {
                    chance
                }
            }
            _ => 1.0,
        };
        self.application_chance * per_tick
    }
}

/// A self-buff or foe-debuff window drawn as a translucent band on the timeline: `Buff`
/// (Build Up / Aim / Soul Drain) shows which casts are buffed; `Debuff` (Touch of Fear −ToHit,
/// −Res attacks) shows when the debuff expires so a refresh can be timed rather than recast on
/// cooldown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum EffectWindowKind {
    Buff,
    Debuff,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct EffectWindow {
    pub kind: EffectWindowKind,
    /// Seconds the window lasts (flat — durations are not enhanceable).
    pub duration: f64,
}

/// One power available to the chain, with values already enhanced for the build.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChainPower {
    /// Stable id (`"<powerset>:<internal name>"`) — what a saved chain stores.
    pub id: String,
    pub name: String,
    pub kind: ChainPowerKind,
    /// ArcanaTime — the animation lock, seconds (also the recharge anchor).
    pub cast: f64,
    /// Raw base recharge (seconds), BEFORE enhancement/global — so the what-if
    /// global-recharge slider can be applied on top.
    pub base_recharge: f64,
    /// Slotted post-ED recharge enhancement as a fraction (0.95 = +95%).
    pub recharge_enhancement: f64,
    /// True for a power whose authored `StrengthsDisallowed` names `RechargeTime`: no
    /// recharge strength applies at all, slotted or global, so
    /// [`effective_recharge`] returns `base_recharge` unchanged.
    pub fixed_recharge: bool,
    /// Enhanced endurance cost per activation.
    pub endurance_cost: f64,
    /// Self endurance GAINED per activation (Dark Consumption-class click recovery),
    /// paid back at the cast. 0 = none.
    pub endurance_gain: f64,
    /// Averaged per-cast damage: certain components in full, rolled components at
    /// `p × damage`, in-cast DoT at its expected value. Dormant components contribute
    /// nothing (inert is inert even on average). Slotted damage procs at their average when the
    /// chain was asked to include them.
    pub damage: f64,
    /// After-cast DoT components, drawn as ticks (their damage is NOT in [`Self::damage`];
    /// the schedule truncates trailing ticks at the loop boundary).
    pub dots: Vec<ChainDot>,
    /// A buff/debuff window to draw, when the power carries one.
    pub effect_window: Option<EffectWindow>,
    /// Damage rows whose gate the build's combat context could not answer (chiefly: no
    /// target chosen). Carried so the modal can say the number is incomplete rather than
    /// quietly reading low (Rule 1).
    pub unresolved_damage: usize,
}

/// Effective recharge after slotted enhancement + (build + what-if) global recharge, with the
/// archetype's net-strength clamp applied to the divisor. The floor keeps a deep recharge
/// debuff from blowing the divisor past zero (a power slows to at most `base / floor`); the
/// cap is the +400% ceiling the beta never applied, so a slider past it now reads flat.
pub fn effective_recharge(
    power: &ChainPower,
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> f64 {
    if power.fixed_recharge {
        return power.base_recharge;
    }
    let net = 1.0 + power.recharge_enhancement + global_recharge_pct / 100.0;
    power.base_recharge / net.clamp(bounds.floor, bounds.cap)
}

/// How powers are ranked / weighted everywhere in the builder (palette order, colour
/// intensity, compactness weighting). All three are STABLE per-power values (no dependence on
/// how many times the power is cast this rotation), so a power's colour/rank doesn't shift as
/// the chain is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PowerMetric {
    /// Per-cast damage (direct + full DoT) — "how big is the hit".
    Damage,
    /// Damage ÷ cast time — damage per second of animation (the classic attack-chain metric
    /// for choosing fillers).
    DamagePerActivation,
    /// Damage ÷ (cast + recharge) — sustained single-power throughput.
    DamagePerSecond,
}

/// Per-cast nominal damage: the direct hit plus expected after-cast DoT damage, weighting
/// each tick by its apply chance so cancel-on-miss / chance-gated DoTs match the in-game
/// average.
pub fn nominal_damage(power: &ChainPower) -> f64 {
    let dot_total: f64 = power
        .dots
        .iter()
        .map(|dot| {
            (1..=dot.ticks as u32)
                .map(|t| dot.per_tick * dot.tick_probability(t))
                .sum::<f64>()
        })
        .sum();
    power.damage + dot_total
}

/// The ranking value for a power under the chosen metric.
pub fn power_metric_value(
    power: &ChainPower,
    metric: PowerMetric,
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> f64 {
    let damage = nominal_damage(power);
    match metric {
        PowerMetric::Damage => damage,
        PowerMetric::DamagePerActivation => {
            if power.cast > 0.0 {
                damage / power.cast
            } else {
                0.0
            }
        }
        PowerMetric::DamagePerSecond => {
            let period = power.cast + effective_recharge(power, global_recharge_pct, bounds);
            if period > 0.0 {
                damage / period
            } else {
                0.0
            }
        }
    }
}

/// A scheduled cast: `power_index` indexes into the `ChainPower` slice passed alongside;
/// `sequence_index` is the position in the pick-order sequence (for per-bar removal).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct Activation {
    pub power_index: usize,
    pub start: f64,
    pub end: f64,
    pub sequence_index: usize,
}

const EPS: f64 = 0.001;

fn overlaps_any(activations: &[Activation], start: f64, end: f64) -> bool {
    activations
        .iter()
        .any(|a| start < a.end - EPS && end > a.start + EPS)
}

/// Row → recharge lane, for a schedule whose rows include per-activation FORMS of one power
/// (RB5-d). The game runs one recharge timer per power whichever form fired, so Energy
/// Transfer's charged and uncharged rows must gate each other's readiness. The identity
/// mapping (`0..powers.len()`) is the ordinary one-row-per-power case, and the un-laned
/// entry points below build it themselves.
pub fn identity_lanes(count: usize) -> Vec<usize> {
    (0..count).collect()
}

/// Earliest start time at which row `row_index` can be cast: not before its recharge LANE is
/// ready, and not overlapping any other animation. Greedy. Recharge starts when a cast
/// FINISHES, not when it begins (CoH mechanic), so a prior cast at `[start, end]` makes the
/// lane ready again at `end + effective_recharge` — each prior cast's own row supplies that
/// recharge, since the form actually fired is what the game's timer runs on.
pub fn find_slot_in_lanes(
    powers: &[ChainPower],
    lanes: &[usize],
    activations: &[Activation],
    row_index: usize,
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> f64 {
    let power = &powers[row_index];
    let lane = lanes[row_index];
    let lane_ready_at = activations
        .iter()
        .filter(|a| lanes[a.power_index] == lane)
        .map(|a| a.end + effective_recharge(&powers[a.power_index], global_recharge_pct, bounds))
        .fold(0.0_f64, f64::max);

    let mut candidate_start = lane_ready_at;
    for _ in 0..500 {
        let candidate_end = candidate_start + power.cast;
        if !overlaps_any(activations, candidate_start, candidate_end) {
            return candidate_start;
        }
        let blocker_end = activations
            .iter()
            .filter(|a| a.start < candidate_end && a.end > candidate_start)
            .map(|a| a.end)
            .fold(f64::NEG_INFINITY, f64::max);
        if blocker_end == f64::NEG_INFINITY {
            break;
        }
        candidate_start = blocker_end.max(lane_ready_at);
    }
    candidate_start
}

/// [`find_slot_in_lanes`] with the identity lane map — the one-row-per-power case.
pub fn find_slot(
    powers: &[ChainPower],
    activations: &[Activation],
    power_index: usize,
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> f64 {
    find_slot_in_lanes(
        powers,
        &identity_lanes(powers.len()),
        activations,
        power_index,
        global_recharge_pct,
        bounds,
    )
}

/// Replay a pick-order sequence into scheduled activations, each tagged with its sequence
/// index so a single bar can be removed (filter the sequence by index and replay).
/// Deterministic: same sequence + same recharge ⇒ identical layout.
pub fn replay_chain(
    powers: &[ChainPower],
    sequence: &[usize],
    global_recharge_pct: f64,
    bounds: StrengthBounds,
) -> Vec<Activation> {
    let mut activations: Vec<Activation> = Vec::with_capacity(sequence.len());
    for (sequence_index, &power_index) in sequence.iter().enumerate() {
        if power_index >= powers.len() {
            continue;
        }
        let start = find_slot(
            powers,
            &activations,
            power_index,
            global_recharge_pct,
            bounds,
        );
        activations.push(Activation {
            power_index,
            start,
            end: start + powers[power_index].cast,
            sequence_index,
        });
        activations.sort_by(|a, b| a.start.total_cmp(&b.start));
    }
    activations
}

/// A stretch of the cycle where nothing is animating.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct DeadGap {
    pub start: f64,
    pub end: f64,
}

fn dead_gaps(activations: &[Activation], cycle_seconds: f64) -> Vec<DeadGap> {
    let mut sorted: Vec<&Activation> = activations.iter().collect();
    sorted.sort_by(|a, b| a.start.total_cmp(&b.start));
    let mut gaps = Vec::new();
    let mut cursor = 0.0_f64;
    for activation in sorted {
        if activation.start > cursor + EPS {
            gaps.push(DeadGap {
                start: cursor,
                end: activation.start,
            });
        }
        cursor = cursor.max(activation.end);
    }
    if cursor < cycle_seconds - EPS {
        gaps.push(DeadGap {
            start: cursor,
            end: cycle_seconds,
        });
    }
    gaps
}

/// Per-activation damage that lands within the cycle window. After-cast DoT ticks count only
/// if they fall before the loop closes, matching how a looping rotation truncates trailing
/// ticks.
fn activation_damage(powers: &[ChainPower], activation: &Activation, cycle_seconds: f64) -> f64 {
    let power = &powers[activation.power_index];
    let mut damage = power.damage;
    for dot in &power.dots {
        for t in 1..=dot.ticks as u32 {
            let tick_time = activation.start + power.cast + f64::from(t) * dot.period;
            if tick_time <= cycle_seconds + EPS {
                damage += dot.per_tick * dot.tick_probability(t);
            }
        }
    }
    damage
}

/// Character-level inputs to the endurance simulation.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct EnduranceParams {
    /// 100 + the build's +Max Endurance points.
    pub max_endurance: f64,
    /// Endurance recovered per second.
    pub recovery_per_second: f64,
    /// Endurance drained per second by the build's ACTIVE toggles.
    pub toggle_per_second: f64,
}

/// A vertex on the endurance-over-time track (for the area chart), endurance as a fraction
/// of the bar.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct EndurancePoint {
    pub time: f64,
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct EnduranceResult {
    /// recovery − toggles − attack spend, per second (steady-state average).
    pub net_per_second: f64,
    pub recovery_per_second: f64,
    pub toggle_per_second: f64,
    /// Attack endurance spent per second over the cycle (gross cost).
    pub attack_per_second: f64,
    /// Net endurance gained/lost per full loop. Analytic (counts the full click gain);
    /// `sustainable` / `stall_time` come from the clamped sim instead, which discards
    /// endurance wasted over the cap.
    pub per_loop_delta: f64,
    pub sustainable: bool,
    /// From a full bar: seconds until empty (`None` when sustainable).
    pub time_to_empty: Option<f64>,
    /// From a full bar: the first time endurance would hit 0 mid-cast and the chain stalls
    /// (`None` when it never stalls within the horizon).
    pub stall_time: Option<f64>,
    /// Piecewise-linear endurance track from a full bar, for the visualization.
    pub track: Vec<EndurancePoint>,
    /// How many loops the track spans.
    pub track_loops: u32,
}

/// Simulate endurance from a FULL bar across enough loops to reveal steady state or a stall.
/// Continuous slope = recovery − toggles; each activation subtracts its endurance cost (net
/// of any click gain) at its start. Endurance clamps to `[0, max]` — a burst recovery power
/// that overfills the cap wastes the overflow, which the analytic per-loop delta would
/// wrongly credit, so sustainability is read from the CLAMPED walk, not the average.
fn simulate_endurance(
    powers: &[ChainPower],
    activations: &[Activation],
    cycle_seconds: f64,
    params: EnduranceParams,
) -> EnduranceResult {
    let net_passive = params.recovery_per_second - params.toggle_per_second;
    let attack_per_cycle: f64 = activations
        .iter()
        .map(|a| powers[a.power_index].endurance_cost)
        .sum();
    let gain_per_cycle: f64 = activations
        .iter()
        .map(|a| powers[a.power_index].endurance_gain)
        .sum();
    let attack_per_second = if cycle_seconds > 0.0 {
        attack_per_cycle / cycle_seconds
    } else {
        0.0
    };
    let per_loop_delta = net_passive * cycle_seconds - attack_per_cycle + gain_per_cycle;
    let net_per_second = if cycle_seconds > 0.0 {
        per_loop_delta / cycle_seconds
    } else {
        net_passive
    };

    // Per-cycle endurance events in time order: cast start → cost net of gain, so a recovery
    // click is a refill spike.
    let mut events: Vec<(f64, f64)> = activations
        .iter()
        .map(|a| {
            let power = &powers[a.power_index];
            (a.start, power.endurance_cost - power.endurance_gain)
        })
        .collect();
    events.sort_by(|a, b| a.0.total_cmp(&b.0));

    const MAX_LOOPS: u32 = 30;
    let mut track = Vec::new();
    let mut endurance = params.max_endurance;
    let mut stall_time = None;
    let mut previous_boundary = params.max_endurance;
    let mut boundary_delta = 0.0;
    let push = |track: &mut Vec<EndurancePoint>, time: f64, value: f64| {
        track.push(EndurancePoint {
            time,
            fraction: (value / params.max_endurance).clamp(0.0, 1.0),
        });
    };

    push(&mut track, 0.0, endurance);
    let mut loops = 0;
    'outer: for loop_index in 0..MAX_LOOPS {
        let base = f64::from(loop_index) * cycle_seconds;
        let mut cursor = 0.0;
        for &(time, cost) in &events {
            // Recover continuously up to this cast.
            endurance = (endurance + net_passive * (time - cursor)).min(params.max_endurance);
            push(&mut track, base + time, endurance);
            // Pay the cast.
            endurance -= cost;
            if endurance < -EPS && stall_time.is_none() {
                stall_time = Some(base + time);
                push(&mut track, base + time, 0.0);
                break 'outer;
            }
            // Clamp both ways: a recovery click (negative cost) refills but caps at max.
            endurance = endurance.clamp(0.0, params.max_endurance);
            push(&mut track, base + time, endurance);
            cursor = time;
        }
        // Recover through the tail of the loop.
        endurance = (endurance + net_passive * (cycle_seconds - cursor)).min(params.max_endurance);
        push(&mut track, base + cycle_seconds, endurance);
        loops = loop_index + 1;
        boundary_delta = endurance - previous_boundary;
        // Steady state reached (boundary endurance stopped moving) → stop; keep at least 2
        // loops so the sawtooth shows the repeating pattern.
        if loop_index >= 1 && boundary_delta.abs() < EPS {
            break;
        }
        previous_boundary = endurance;
    }

    let sustainable = stall_time.is_none() && boundary_delta >= -EPS;
    let time_to_empty = if !sustainable && net_per_second < 0.0 {
        Some(params.max_endurance / -net_per_second)
    } else {
        None
    };

    EnduranceResult {
        net_per_second,
        recovery_per_second: params.recovery_per_second,
        toggle_per_second: params.toggle_per_second,
        attack_per_second,
        per_loop_delta,
        sustainable,
        time_to_empty,
        stall_time,
        track,
        track_loops: loops,
    }
}

/// Every derived stat for the current chain.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ChainResult {
    pub cycle_seconds: f64,
    pub total_damage: f64,
    pub damage_per_second: f64,
    /// Endurance spent per second (attacks only).
    pub endurance_per_second: f64,
    pub dead_time: f64,
    pub dead_gaps: Vec<DeadGap>,
    /// 0–100; share of the cycle spent animating (not idle).
    pub efficiency: f64,
    /// 0–100; damage-weighted recharge utilization — how close each damaging power comes to
    /// firing exactly on cooldown every loop (100 = perfectly compact; lower means a power
    /// sits "waiting" while another animation plays). `None` when the chain deals no damage.
    /// Distinct from efficiency: efficiency asks "are you ever idle", compactness asks "when
    /// a power is ready, do you fire it immediately".
    pub compactness: Option<f64>,
    pub endurance: Option<EnduranceResult>,
    /// Max per-activation damage in the chain — drives relative-damage colouring.
    pub max_damage: f64,
}

/// Compute every derived stat for the current chain. `None` for an empty chain.
pub fn compute_chain(
    powers: &[ChainPower],
    activations: &[Activation],
    global_recharge_pct: f64,
    bounds: StrengthBounds,
    endurance: Option<EnduranceParams>,
    metric: PowerMetric,
) -> Option<ChainResult> {
    compute_chain_in_lanes(
        powers,
        &identity_lanes(powers.len()),
        activations,
        global_recharge_pct,
        bounds,
        endurance,
        metric,
    )
}

/// [`compute_chain`] over a schedule whose rows include per-activation forms: rows sharing a
/// lane share one recharge timer, so the loop constraint binds per LANE (RB5-d). Damage,
/// endurance and the gaps already read each activation's own row and need no lane at all.
pub fn compute_chain_in_lanes(
    powers: &[ChainPower],
    lanes: &[usize],
    activations: &[Activation],
    global_recharge_pct: f64,
    bounds: StrengthBounds,
    endurance: Option<EnduranceParams>,
    metric: PowerMetric,
) -> Option<ChainResult> {
    if activations.is_empty() {
        return None;
    }

    let last_end = activations.iter().map(|a| a.end).fold(0.0_f64, f64::max);
    // The cycle isn't just where the last animation ends — when the loop repeats, every
    // power must have recharged since its last cast THIS loop. Recharge starts at cast-END,
    // so power P's last cast is ready again at last_end_P + recharge; the loop's next cast
    // of P sits at cycle + first_start_P, which must not precede that. The binding power
    // (usually a long-recharge opener cast once) can push the true cycle past the visible
    // casts — that boundary idle is real dead time. Grouped per LANE: a power's forms share
    // its one timer, and the recharge that binds is the lane's LAST cast's own.
    let mut cycle_seconds = last_end;
    let cast_lanes: std::collections::BTreeSet<usize> =
        activations.iter().map(|a| lanes[a.power_index]).collect();
    for lane in &cast_lanes {
        let mine: Vec<&Activation> = activations
            .iter()
            .filter(|a| lanes[a.power_index] == *lane)
            .collect();
        let first_start = mine.iter().map(|a| a.start).fold(f64::INFINITY, f64::min);
        let last = mine
            .iter()
            .max_by(|a, b| a.end.total_cmp(&b.end))
            .expect("lane groups are non-empty by construction");
        let need = last.end
            + effective_recharge(&powers[last.power_index], global_recharge_pct, bounds)
            - first_start;
        if need > cycle_seconds {
            cycle_seconds = need;
        }
    }

    let gaps = dead_gaps(activations, cycle_seconds);
    let dead_time: f64 = gaps.iter().map(|g| g.end - g.start).sum();
    let efficiency = if cycle_seconds > 0.0 {
        ((1.0 - dead_time / cycle_seconds) * 100.0).round()
    } else {
        0.0
    };

    let mut total_damage = 0.0;
    let mut max_damage = 0.0_f64;
    let mut endurance_per_cycle = 0.0;
    let mut cast_count: std::collections::BTreeMap<usize, u32> = std::collections::BTreeMap::new();
    for activation in activations {
        let damage = activation_damage(powers, activation, cycle_seconds);
        total_damage += damage;
        max_damage = max_damage.max(damage);
        endurance_per_cycle += powers[activation.power_index].endurance_cost;
        *cast_count.entry(activation.power_index).or_insert(0) += 1;
    }

    // Compactness: metric-weighted recharge utilization. A power's fastest solo repeat
    // period is cast + recharge, so in a cycle it could fire at most cycle / period times;
    // u = min(1, times_cast × period / cycle) — 1.0 means it fires exactly on cooldown every
    // loop. Weighting by the chosen metric keeps weak fillers and zero-value utility from
    // skewing it, and means slack on a high-value power (recoverable DPS) counts most.
    let mut weight_sum = 0.0;
    let mut weighted_utilization_sum = 0.0;
    for (&power_index, &count) in &cast_count {
        let weight = power_metric_value(&powers[power_index], metric, global_recharge_pct, bounds);
        if weight <= 0.0 {
            continue;
        }
        let period = powers[power_index].cast
            + effective_recharge(&powers[power_index], global_recharge_pct, bounds);
        let utilization = if cycle_seconds > 0.0 && period > 0.0 {
            (f64::from(count) * period / cycle_seconds).min(1.0)
        } else {
            1.0
        };
        weight_sum += weight;
        weighted_utilization_sum += weight * utilization;
    }
    let compactness = if weight_sum > 0.0 {
        Some((weighted_utilization_sum / weight_sum * 100.0).round())
    } else {
        None
    };

    Some(ChainResult {
        cycle_seconds,
        total_damage,
        damage_per_second: if cycle_seconds > 0.0 {
            total_damage / cycle_seconds
        } else {
            0.0
        },
        endurance_per_second: if cycle_seconds > 0.0 {
            endurance_per_cycle / cycle_seconds
        } else {
            0.0
        },
        dead_time,
        dead_gaps: gaps,
        efficiency,
        compactness,
        endurance: endurance
            .map(|params| simulate_endurance(powers, activations, cycle_seconds, params)),
        max_damage,
    })
}
