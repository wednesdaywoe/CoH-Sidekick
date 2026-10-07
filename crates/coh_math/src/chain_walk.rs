//! RB5-d — the per-cast state over a packed rotation: what a position in the sequence owns
//! and whether its meter is up, everything [`crate::chain_build::schedule_chain`] resolves a
//! cast's form against.
//!
//! Two mechanics need position state and both are data, not name tables. A charge banked and
//! spent within one rotation rides the powers' own `grantEdges` (Total Focus grants
//! `Redirects.Energy_Melee.Energy_Store`, Energy Transfer's redirect selects on owning it,
//! Stun and Barrage revoke it), and this module keeps the ledger: a grant lives from its cast
//! until spent or until the granted record's own `expires` (LIFETIME-1's fields). The
//! from-Hide position rule rides the meter atoms: the build's hide-meter publisher states the
//! in-rotation clock (Hide's `suppress_events`: attacking drops the meter for the atom's
//! `suppress_seconds`, so a gapped rotation re-hides), and a rotation power carrying its own
//! Self meter grant states the post-cast window (Placate's 10s, cancelled by the atom's
//! `cancel_events` the moment the caster acts).
//!
//! The event model is deliberately the caster's own actions and nothing else: a damaging cast
//! fires the attack event at its cast END (where the game anchors recharge), and the incoming
//! half of the suppress/cancel vocabularies (`Damaged`, `MissionObjectClick`,
//! `PseudoPetAttacked`) cannot occur in a positionless solo rotation, so the walk never
//! consults it. The meter axis exists only for a build whose powers publish the hide meter,
//! the same scope [`crate::projection`]'s gate context binds it under — `kMeter` is one
//! attribute ten mechanics drive, and a Dominator's meter does not mean "hidden".

use crate::expr::{eval_bool, normalize_power_path, SourceContext, HIDE_METER};
use coh_data::grant_edges::{grant_edges, GrantOp};
use coh_data::{AtomicEffect, CharacterState, EffectType, Power, PowerDatabase, PvMode, ToWho};
use std::collections::{BTreeMap, HashMap, HashSet};

/// The build's in-rotation meter clock, read off its hide-meter publisher's meter atom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterClock {
    /// The atom's `suppress_seconds`: how long an attack keeps the meter down.
    pub window: f64,
}

/// The hide-meter clock, for a build that publishes one. `Ok(None)` = no publisher, so the
/// walk models no meter axis at all. `Err` = a publisher whose meter atom carries no suppress
/// window, which is a stale bundle (the window ships as of RB5-d) — reported loud rather than
/// silently modelling an unsuppressable meter (Rule 1).
pub fn hide_meter_clock(
    state: &CharacterState,
    db: &PowerDatabase,
) -> Result<Option<MeterClock>, String> {
    if !crate::gather::publishes_hide_meter(state, db) {
        return Ok(None);
    }
    for selection in state.all_selected() {
        let Some(power) =
            crate::gather::resolve_power(db, &selection.powerset, &selection.internal_name)
        else {
            continue;
        };
        for atom in &power.atoms {
            if is_meter(atom)
                && atom.suppressible == Some(true)
                && coh_data::reaches_caster(atom, power)
            {
                return match atom.suppress_seconds {
                    Some(window) => Ok(Some(MeterClock { window })),
                    None => Err(format!(
                        "{}: hide-meter atom carries no suppress window — the bundle predates \
                         the RB5-d suppress-event export; regenerate it",
                        power.internal_name.as_deref().unwrap_or(&power.name)
                    )),
                };
            }
        }
    }
    // publishes_hide_meter found a publisher above; not reaching it here means the two walks
    // disagree, which is a defect in THIS function, not a data state.
    Err("hide-meter publisher found by publishes_hide_meter but not by the clock walk".into())
}

fn is_meter(atom: &AtomicEffect) -> bool {
    atom.effect_type == Some(EffectType::Meta) && atom.meta_attrib.as_deref() == Some("meter")
}

/// A rotation power's own Self meter grant — Placate's post-cast re-hide window.
///
/// The predicate is the atom shape, not a name: a Self-targeted meter atom with a positive
/// magnitude, a duration, a cancel-event list and no suppress tail of its own (the continuous
/// toggle meter carries one; the granted window does not). PvP-only rows are skipped — the
/// rotation is the PvE one, and Placate authors a shorter PvP twin beside this.
pub fn self_meter_window(power: &Power) -> Option<f64> {
    power
        .atoms
        .iter()
        .filter(|atom| {
            is_meter(atom)
                && atom.to_who == Some(ToWho::Self_)
                && atom.pv_mode != Some(PvMode::PvP)
                && atom.suppress_events.is_none()
                && atom.cancel_events.is_some()
                && atom.duration.is_some_and(|d| d > 0.0)
                && (atom.magnitude.unwrap_or(0.0) > 0.0 || atom.scale.unwrap_or(0.0) > 0.0)
        })
        .filter_map(|atom| atom.duration)
        .fold(None, |best: Option<f64>, d| {
            Some(best.map_or(d, |b| b.max(d)))
        })
}

/// One live grant in the position ledger.
#[derive(Debug, Clone, PartialEq)]
struct ActiveGrant {
    path: String,
    count: f64,
    /// Wall-clock expiry on the rotation timeline, from the edge's `expires` (or its in-game
    /// twin — the two clocks coincide inside a combat rotation). `None` = no authored limit.
    expires_at: Option<f64>,
}

/// What a cast's grant edges did — the Rule-1 counters the modal reports beside the numbers.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize)]
pub struct EdgeOutcome {
    pub granted: usize,
    pub revoked: usize,
    /// Edges whose condition this context could not answer: applied nothing, reported.
    pub indeterminate: usize,
    /// Edges rolling a chance below 1: a discrete ledger holds no fraction of a charge, so
    /// they apply nothing and are reported instead.
    pub probabilistic: usize,
}

/// The mutable per-cast state: the grant ledger and the meter clocks.
#[derive(Debug, Clone)]
pub struct WalkState {
    grants: Vec<ActiveGrant>,
    /// The meter model, present only for a hide-meter build.
    meter: Option<MeterClock>,
    /// The meter is down until here (attacks push it out).
    suppressed_until: f64,
    /// A granted re-hide window is up until here (Placate), cancelled by the next attack.
    granted_until: Option<f64>,
}

impl WalkState {
    pub fn new(meter: Option<MeterClock>) -> Self {
        WalkState {
            grants: Vec::new(),
            meter,
            suppressed_until: f64::NEG_INFINITY,
            granted_until: None,
        }
    }

    /// The position's ownership overlay: every unexpired grant, summed per path.
    pub fn owned_overlay_at(&self, time: f64) -> BTreeMap<String, f64> {
        let mut overlay: BTreeMap<String, f64> = BTreeMap::new();
        for grant in &self.grants {
            if grant.expires_at.is_none_or(|expiry| time < expiry) {
                *overlay.entry(grant.path.clone()).or_insert(0.0) += grant.count;
            }
        }
        overlay
    }

    /// Whether the hide meter is up at `time`. `None` for a build with no meter axis, so the
    /// caller leaves `state.combat.hidden` alone rather than overriding it with a model the
    /// build's data never stated.
    pub fn hidden_at(&self, time: f64, declared_hidden: bool) -> Option<bool> {
        self.meter?;
        let re_hidden = declared_hidden && time >= self.suppressed_until;
        let granted = self.granted_until.is_some_and(|until| time < until);
        // The declared toggle also covers the opener: before the first attack nothing has
        // suppressed the meter, so `suppressed_until` is still -inf and the declared state
        // stands.
        Some(re_hidden || granted)
    }

    /// A damaging cast landed at `time` (its cast end): the attack event. Drops the meter for
    /// the clock's window and cancels a granted re-hide window.
    pub fn note_attack(&mut self, time: f64) {
        if let Some(clock) = self.meter {
            self.suppressed_until = self.suppressed_until.max(time + clock.window);
            if let Some(until) = self.granted_until {
                if time < until {
                    self.granted_until = Some(time);
                }
            }
        }
    }

    /// The cast granted its own re-hide window (Placate's Self meter atom) at `time`.
    pub fn note_self_meter_grant(&mut self, time: f64, duration: f64) {
        if self.meter.is_some() {
            let until = time + duration;
            self.granted_until = Some(self.granted_until.map_or(until, |g| g.max(until)));
        }
    }

    /// Apply a cast's grant edges at `time` (its cast end), each condition answered against
    /// the position's own context — rebuilt per EDGE from the live ledger, because the edges
    /// of one cast are sequential: Total Focus banks through a hidden-state branch and then
    /// its `!own` fallback branch must SEE that bank, or one cast grants twice. `Ok(false)`
    /// is a correct non-application; only an unanswerable condition or a sub-certain roll is
    /// reported.
    pub fn apply_cast_edges(
        &mut self,
        power: &Power,
        time: f64,
        env: &EdgeEnv,
        applied_paths: &mut Vec<(GrantOp, String)>,
    ) -> Result<EdgeOutcome, String> {
        let mut outcome = EdgeOutcome::default();
        for edge in grant_edges(power)? {
            if edge.chance < 1.0 {
                outcome.probabilistic += 1;
                continue;
            }
            if let Some(condition) = edge.condition.as_deref() {
                let context = edge_context(env, &self.owned_overlay_at(time));
                match eval_bool(condition, &context) {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => {
                        outcome.indeterminate += 1;
                        continue;
                    }
                }
            }
            let path = normalize_power_path(&edge.path);
            let effective_time = time + edge.delay_seconds;
            match edge.op {
                GrantOp::Grant => {
                    let held: f64 = self
                        .grants
                        .iter()
                        .filter(|grant| {
                            grant.path == path
                                && grant
                                    .expires_at
                                    .is_none_or(|expiry| effective_time < expiry)
                        })
                        .map(|grant| grant.count)
                        .sum();
                    let ceiling = edge.max_count.unwrap_or(f64::INFINITY);
                    let count = edge.count.min((ceiling - held).max(0.0));
                    if count <= 0.0 {
                        continue;
                    }
                    self.grants.push(ActiveGrant {
                        path: path.clone(),
                        count,
                        expires_at: edge
                            .expires
                            .or(edge.expires_in_game)
                            .map(|lifetime| effective_time + lifetime),
                    });
                    outcome.granted += 1;
                    applied_paths.push((GrantOp::Grant, path));
                }
                GrantOp::Revoke => {
                    let mut remaining = edge.count;
                    // Spend the soonest-expiring copies first, the order that leaves the
                    // longest-lived state standing.
                    self.grants.sort_by(|a, b| {
                        a.expires_at
                            .unwrap_or(f64::INFINITY)
                            .total_cmp(&b.expires_at.unwrap_or(f64::INFINITY))
                    });
                    for grant in &mut self.grants {
                        if remaining <= 0.0 {
                            break;
                        }
                        if grant.path != path
                            || grant
                                .expires_at
                                .is_some_and(|expiry| effective_time >= expiry)
                        {
                            continue;
                        }
                        let taken = grant.count.min(remaining);
                        grant.count -= taken;
                        remaining -= taken;
                    }
                    self.grants.retain(|grant| grant.count > 0.0);
                    outcome.revoked += 1;
                    applied_paths.push((GrantOp::Revoke, path));
                }
            }
        }
        Ok(outcome)
    }
}

/// The position-independent half of an edge condition's context: everything except the live
/// ledger, which changes edge to edge within one cast.
#[derive(Debug, Clone, Default)]
pub struct EdgeEnv {
    /// The build's static ownership ([`crate::gather::owned_powers`]).
    pub base_owned: HashMap<String, f64>,
    /// The build's source modes (the gather's), made live per `in_combat`.
    pub source_modes: HashSet<String>,
    pub in_combat: bool,
    /// The position's meter state. `None` = no meter axis for this build.
    pub hidden: Option<bool>,
    /// Who the rotation is hitting — Total Focus's bank branches fork on
    /// `enttype target> critter eq`, so the chosen target answers them.
    pub target: Option<crate::expr::TargetIdentity>,
}

/// The context one edge condition is answered against: the static ownership with the live
/// ledger merged in by MAX (an overlay must not unsay a pick), the live modes, the meter and
/// the target. Narrower than the projection's gate context on purpose — anything a condition
/// reads beyond what's bound here answers Indeterminate and is REPORTED, never guessed.
pub fn edge_context(env: &EdgeEnv, overlay: &BTreeMap<String, f64>) -> SourceContext {
    let mut owned = env.base_owned.clone();
    for (path, count) in overlay {
        owned
            .entry(path.clone())
            .and_modify(|held| *held = held.max(*count))
            .or_insert(*count);
    }
    let mut source_attributes = HashMap::new();
    if let Some(hidden) = env.hidden {
        source_attributes.insert(HIDE_METER.to_string(), if hidden { 1.0 } else { 0.0 });
    }
    SourceContext {
        owned_powers: owned,
        source_modes: crate::gather::live_modes(&env.source_modes, env.in_combat),
        source_attributes,
        target: env.target.clone(),
        ..Default::default()
    }
}
