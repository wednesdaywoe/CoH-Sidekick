//! `resolveStealthRadius` (`character-totals.ts:928`) — the stealth-radius commit.
//!
//! Stealth is the first family in the port whose total is NOT a running accumulation. Every
//! other Pass 2 family adds into `GlobalBonuses` as its power is walked; stealth cannot,
//! because a source's contribution depends on the OTHER sources: powers sharing a binary
//! suppress group (`stack_key`) do not stack — only the largest radius in the group applies.
//! So the apply loop only COLLECTS a [`StealthContribution`] per power, and this module
//! commits the grouped total once every source is known.
//!
//! The two axes (PvE / PvP) resolve independently: a power can win its group on one axis and
//! lose it on the other.
//!
//! # Why the commit is deferred past the apply loop
//!
//! The beta resolves at `character-totals.ts:4294` — after the three `applyActivePowerBonuses`
//! calls (base, conditional, buff-pet) AND after the proc passes, which push contributions of
//! their own (`:2607`). Procs are M4 here, so the M3 contribution set is active-powers-only;
//! the synthetic corpus carries zero procs by design, so the M3 resolve reproduces the full TS
//! output exactly. M4's procs plug in as additional contributions with no change to this
//! function — which is why it takes a slice rather than reading the powers itself.
//!
//! # Ordering is load-bearing for exact-f64
//!
//! The beta sums group winners by JS `Map` iteration order — first-insertion order of each
//! `stackKey` among the contributions — then adds the ungrouped ones in contribution order.
//! Float addition is not associative, so a `HashMap` here could sum the same winners into a
//! different f64 and red the gate. [`resolve_stealth_radius`] preserves insertion order with a
//! `Vec` (the group count is a handful; linear search is cheaper than hashing anyway).

/// One stealth-radius source, gathered during the apply loop and resolved with every other
/// source by [`resolve_stealth_radius`].
///
/// `stack_key` is the binary suppress group (`None` = stacks additively). Radii are in FEET.
/// The beta pushes a contribution only when at least one axis is positive
/// (`character-totals.ts:1941`), so a zero/zero source never reaches the resolve.
#[derive(Debug, Clone, PartialEq)]
pub struct StealthContribution {
    pub stack_key: Option<String>,
    /// PvE radius; 0.0 when this source has no PvE component.
    pub pve: f64,
    /// PvP radius; 0.0 when this source has no PvP component.
    pub pvp: f64,
    /// The DISPLAY name of whatever supplied this radius — the power for a walk contribution,
    /// the proc piece for one pushed by the proc pass. Carried directly rather than resolved
    /// from a (set, internal name) pair, for [`crate::movement::MovementContribution`]'s reason:
    /// a contribution can come from a synthetic with no selection of its own to resolve against.
    pub power_name: String,
}

/// One stealth source as the detailed breakdown shows it — the radius it WOULD contribute on
/// this axis, plus whether it lost its suppress group.
///
/// `superseded` is the stealth spelling of [`crate::movement::MovementBreakdownSource`]'s
/// `suppressed`, and it is likewise NOT `capped`: losing a suppress group is ordinary game
/// mechanics, while `capped` drives the Rule-of-5 warning ring. The row keeps its own radius so
/// the tooltip can say what it would have contributed had it won.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct StealthBreakdownSource {
    pub breakdown_key: String,
    pub value: f64,
    pub superseded: bool,
    pub power_name: String,
}

/// One axis of the grouped total: the sum of each suppress group's LARGEST radius, plus every
/// ungrouped radius. Non-positive radii are skipped on both paths — a source positive on the
/// other axis contributes nothing here (the beta's `v <= 0` guard), and a negative radius can
/// neither win a group nor subtract from the total.
fn resolve_axis(
    contributions: &[StealthContribution],
    axis: fn(&StealthContribution) -> f64,
) -> f64 {
    // (key, largest radius seen) in first-insertion order — see the module doc on f64 ordering.
    let group_winners = group_winners(contributions, axis);

    let mut total = 0.0;
    for (_, best) in &group_winners {
        total += best; // one winner per suppress group
    }
    for contribution in contributions {
        let radius = axis(contribution);
        if radius > 0.0 && contribution.stack_key.is_none() {
            total += radius; // ungrouped sources stack
        }
    }
    total
}

/// The two breakdown keys a stealth contribution resolves into, PvE first.
///
/// Named here rather than at each reader because a stealth source is queued during the apply
/// walk and only committed by [`resolve_stealth_radius`] afterwards — so a surface asking what
/// a stealth contributor feeds cannot read it off an emitted source the way every other
/// contribution allows.
pub const STEALTH_RADIUS_KEYS: [&str; 2] = ["stealthRadiusPvE", "stealthRadiusPvP"];

/// Commit the gathered contributions to the two stealth-radius totals, with the per-source rows
/// behind them.
///
/// The beta ASSIGNS rather than accumulates (`global[key] = total`), which is why this returns
/// the totals instead of taking `&mut GlobalBonuses`: calling it twice must not double a
/// radius, and a return value makes that structurally impossible.
pub fn resolve_stealth_radius(contributions: &[StealthContribution]) -> StealthTotals {
    StealthTotals {
        pve: resolve_axis(contributions, |c| c.pve),
        pvp: resolve_axis(contributions, |c| c.pvp),
        breakdown: [
            (
                STEALTH_RADIUS_KEYS[0],
                (|c: &StealthContribution| c.pve) as fn(&StealthContribution) -> f64,
            ),
            (STEALTH_RADIUS_KEYS[1], |c: &StealthContribution| c.pvp),
        ]
        .into_iter()
        .flat_map(|(key, axis)| axis_breakdown(contributions, key, axis))
        .collect(),
    }
}

/// The two radii plus the rows they were resolved from.
pub struct StealthTotals {
    pub pve: f64,
    pub pvp: f64,
    pub breakdown: Vec<StealthBreakdownSource>,
}

impl StealthTotals {
    /// The two radii alone, for the callers and tests that assert on the totals rather than on
    /// their provenance.
    pub fn radii(&self) -> (f64, f64) {
        (self.pve, self.pvp)
    }
}

/// One row per source that offered a positive radius on this axis, flagged with whether it
/// actually reached the total. Mirrors the movement resolve's own breakdown pass: within a
/// suppress group only the largest radius applies, so every other member is `superseded`.
///
/// Only positive radii produce rows, matching what [`resolve_axis`] counts — a source positive
/// on the other axis alone contributes nothing here and a row for it would read as a zero
/// contribution rather than as an absent one.
fn axis_breakdown(
    contributions: &[StealthContribution],
    breakdown_key: &str,
    axis: fn(&StealthContribution) -> f64,
) -> Vec<StealthBreakdownSource> {
    let winners = group_winners(contributions, axis);
    // Which contribution IS each group's winner, by position. Identity, not value: two sources
    // tied at a group's maximum are both "not less than the best", so a value compare would
    // leave both rows live and the breakdown would sum to more than the total it explains
    // (`resolve_axis` adds each group exactly once). The winner is the FIRST to reach the
    // maximum, matching `group_winners`, whose `>` never displaces an equal incumbent.
    let winning_index = |key: &str, best: f64| {
        contributions
            .iter()
            .position(|c| c.stack_key.as_deref() == Some(key) && axis(c) == best)
    };
    contributions
        .iter()
        .enumerate()
        .filter(|(_, c)| axis(c) > 0.0)
        .map(|(index, c)| StealthBreakdownSource {
            breakdown_key: breakdown_key.to_string(),
            value: axis(c),
            // An ungrouped source always applies; a grouped one applies only if it is its
            // group's winner.
            superseded: match c.stack_key.as_deref() {
                Some(key) => winners
                    .iter()
                    .find(|(k, _)| *k == key)
                    .is_some_and(|(_, best)| winning_index(key, *best) != Some(index)),
                None => false,
            },
            power_name: c.power_name.clone(),
        })
        .collect()
}

/// Each suppress group's largest radius on one axis, in first-insertion order. Shared by the
/// total and its breakdown so a row can never disagree with the sum about who won.
fn group_winners(
    contributions: &[StealthContribution],
    axis: fn(&StealthContribution) -> f64,
) -> Vec<(&str, f64)> {
    let mut winners: Vec<(&str, f64)> = Vec::new();
    for contribution in contributions {
        let radius = axis(contribution);
        let Some(key) = contribution.stack_key.as_deref().filter(|_| radius > 0.0) else {
            continue;
        };
        match winners.iter_mut().find(|(k, _)| *k == key) {
            Some((_, best)) if radius > *best => *best = radius,
            Some(_) => {}
            None => winners.push((key, radius)),
        }
    }
    winners
}
