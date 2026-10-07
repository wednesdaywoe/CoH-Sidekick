//! `grantEdges` — the caster-state writes CASTING a power applies (RB5-d).
//!
//! Distinct from [`crate::granted_powers`], which is the BUILD-time mechanism (auto-issue: the
//! game hands a power over for holding a powerset at a level). An edge here fires at
//! activation: Total Focus banks `Redirects.Energy_Melee.Energy_Store` on the caster, Stun and
//! Barrage revoke it, and Energy Transfer's redirect selects a form by owning it. The read half
//! of that state already ships (`formVariants` conditions, the conditional toggles' `ownedPower`
//! claims, `maxTargetsExpression`) and resolves through `owned_powers`; this module reads the
//! WRITE half the converter stamps (`extractGrantEdges`), which is what lets a per-cast walk
//! over a rotation know which cast banks a charge and which spends it.
//!
//! Decay is part of the edge because the game makes it part of the grant: a granted power is
//! removed `lifetime` wall-clock seconds after the grant and `lifetime_in_game` in-play seconds
//! (`power_CheckUsageLimits`), fields the parser discarded until LIFETIME-1. An edge with no
//! `expires` states the granted record authors no limit — the converter refuses to emit an edge
//! whose target record it cannot read, precisely so that absence stays a statement.
//!
//! Reading is `Result`, not filter-and-skip: this key is converter-emitted and gated
//! (`audit-grant-edges.cjs` holds the shipped stamp deep-equal to the extractor), so a shape
//! this reader does not recognize is drift, and folding it to a default would ship a wrong
//! rotation as authoritative (Rule 1; the STACK-3 lesson).

use crate::Power;
use serde_json::Value;

/// Whether the cast adds or removes copies of the named power on the caster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantOp {
    Grant,
    Revoke,
}

/// One caster-state write, as the converter stamped it.
#[derive(Debug, Clone, PartialEq)]
pub struct GrantEdge {
    pub op: GrantOp,
    /// The granted/revoked power's dotted export path, verbatim
    /// (`Redirects.Energy_Melee.Energy_Store`) — the same spelling the ownership gates read.
    pub path: String,
    /// Copies added or removed. The game defaults an absent params count to 1; the converter
    /// resolves that default, so it is always present here.
    pub count: f64,
    /// The effect group chain's composed gate, verbatim tokens for `coh_math`'s expr VM.
    /// `None` = unconditional.
    pub condition: Option<Box<[Box<str>]>>,
    /// The group chain's composed roll. 1.0 = certain.
    pub chance: f64,
    /// Seconds after activation the edge applies (group chain + template delay). 0 = at cast.
    pub delay_seconds: f64,
    /// The granted record's own `lifetime`: wall-clock seconds from grant to removal.
    /// `None` = the record authors no wall-clock limit.
    pub expires: Option<f64>,
    /// The granted record's `lifetime_in_game` (in-play seconds), the other decay clock.
    pub expires_in_game: Option<f64>,
    /// The granted record's `num_allowed` — the stack ceiling ownership counts test against.
    pub max_count: Option<f64>,
}

/// The power's stamped edges, in emission order. `Ok(vec![])` when the power carries none.
pub fn grant_edges(power: &Power) -> Result<Vec<GrantEdge>, String> {
    let Some(raw) = power.extra.get("grantEdges") else {
        return Ok(Vec::new());
    };
    let name = power.internal_name.as_deref().unwrap_or(&power.name);
    let list = raw
        .as_array()
        .ok_or_else(|| format!("{name}: grantEdges is not an array"))?;
    list.iter()
        .enumerate()
        .map(|(index, entry)| {
            decode_edge(entry).map_err(|e| format!("{name} grantEdges[{index}]: {e}"))
        })
        .collect()
}

fn decode_edge(entry: &Value) -> Result<GrantEdge, String> {
    let object = entry.as_object().ok_or("not an object")?;
    let op = match object.get("op").and_then(Value::as_str) {
        Some("grant") => GrantOp::Grant,
        Some("revoke") => GrantOp::Revoke,
        other => return Err(format!("unrecognized op {other:?}")),
    };
    let path = object
        .get("path")
        .and_then(Value::as_str)
        .ok_or("missing path")?
        .to_string();
    let count = object
        .get("count")
        .and_then(Value::as_f64)
        .ok_or("missing count")?;
    let condition = match object.get("condition") {
        None => None,
        Some(value) => Some(
            value
                .as_array()
                .ok_or("condition is not a token array")?
                .iter()
                .map(|token| {
                    token
                        .as_str()
                        .map(Box::from)
                        .ok_or_else(|| format!("non-string condition token {token:?}"))
                })
                .collect::<Result<Box<[Box<str>]>, String>>()?,
        ),
    };
    let optional = |key: &str| -> Result<Option<f64>, String> {
        match object.get(key) {
            None => Ok(None),
            Some(value) => value
                .as_f64()
                .map(Some)
                .ok_or_else(|| format!("{key} is not a number")),
        }
    };
    Ok(GrantEdge {
        op,
        path,
        count,
        condition,
        chance: optional("chance")?.unwrap_or(1.0),
        delay_seconds: optional("delaySeconds")?.unwrap_or(0.0),
        expires: optional("expires")?,
        expires_in_game: optional("expiresInGame")?,
        max_count: optional("maxCount")?,
    })
}
