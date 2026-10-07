//! PASS2B-2 regen/recovery Expression reader — recovers the HP-scaling regen/recovery magnitude
//! atom-native by EVALUATING the `Regeneration`/`Recovery`, `aspect=Current`, `type=Expression`
//! atom's `magnitude_expression`, replacing the bag's flat `+100%` placeholder (the scale×table
//! the `resource_buff_value` PUNT falls back to). The canonical case is **Gamma Boost**
//! (Radiation Armor), whose regen and recovery scale with the caster's current-HP%:
//!
//! - Regen — `75 kHitPoints% source> - 30 + 100 / @StdResult *` → `(105 − H)/100 × @StdResult`,
//!   rising as current-HP% (`H`) falls ("the lower your current health, the greater the regen").
//! - Recovery — `1.2 kHitPoints% source> * 100 / .3 * @StdResult *` → `1.2·H/100 × 0.3 × @StdResult`,
//!   rising with current-HP% ("the higher your current health, the greater the recovery").
//!
//! Unlike ATOM10's absorb fraction (whose `Max.kHitPoints` is build-time-known and neutralized to
//! unity), `kHitPoints%` is genuine runtime state the user supplies via `combat.hit_points_percent`
//! (default full health). `@StdResult` = the atom's own scale×table — resolved here, not
//! neutralized, because the expression multiplies it in to reach the final magnitude. The result is
//! the COMPLETE magnitude (already × @StdResult), so the caller adds it directly and must NOT re-run
//! `resolve_scaled_effect` on it.

use super::base_atoms_of_type;
use crate::expr::{eval, EvalContext, EvalError, Value};
use crate::scaled::resolve_scaled_effect;
use crate::totals::CalcError;
use coh_data::{
    reaches_caster, Aspect, AttribType, CombatContext, EffectType, Power, PowerDatabase,
};

/// Resolves the two readers an HP-scaling resource Expression names: `kHitPoints% source>` (the
/// caster's current-HP%, the runtime input) and `@StdResult` (the atom's own scale×table, the base
/// magnitude the expression modulates). Every other reader is Indeterminate, so only this shape
/// yields a number — an unrecognized reader falls back to the bag rather than a fabricated value
/// (Rule 1).
struct HitPointsProbe {
    hit_points_percent: f64,
    std_result: f64,
}

impl EvalContext for HitPointsProbe {
    fn resolve(&self, reader: &str, operands: &[Value]) -> Result<Value, EvalError> {
        match reader {
            "source>" | "Source>" => match operands.first() {
                Some(Value::Symbol(s)) if &**s == "kHitPoints%" => {
                    Ok(Value::Number(self.hit_points_percent))
                }
                _ => Err(EvalError::Indeterminate(reader.into())),
            },
            "@StdResult" => Ok(Value::Number(self.std_result)),
            _ => Err(EvalError::Indeterminate(reader.into())),
        }
    }
}

/// What the HP-scaling Expression contributes to one resource half.
#[derive(Debug, Clone, PartialEq)]
pub struct HpScalingResource {
    /// The FINAL magnitude — already × `@StdResult`, so the caller adds it directly and must
    /// never re-run `resolve_scaled_effect` on it.
    pub value: f64,
    /// The contributing atoms' `modifierTable`, for the caller's `Res_Boolean` guard. Carried
    /// because the Expression now admits itself (ATOM-BAG-5): with no bag slot behind it there
    /// is no other table for that guard to read.
    pub table: Option<String>,
}

/// The HP-scaling regen or recovery magnitude read atom-native, or `None` when this power has no
/// such atom (the bag placeholder then supplies the half, if it carries one).
///
/// Matches the atoms `resource_buff_value` PUNTs: BASE atoms of `effect_type`, `aspect=Current`,
/// `type=Expression`, self-directed (`toWho` Self/All), not caster-excluded. Sums all such atoms
/// (Gamma Boost carries exactly one per effect type). `None` when there is no such atom, its
/// `scale`/`magnitude_expression` is missing, or the expression falls outside the resolvable shape
/// (any Indeterminate reader) — which is how the Kheldian `endurancecost power.boosted>` toggles
/// and Fortify Pack's `Cur.kMeter` stay out of this reader entirely.
pub fn hp_scaling_resource_value(
    power: &Power,
    effect_type: EffectType,
    archetype: &str,
    level: i32,
    combat: &CombatContext,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> Option<HpScalingResource> {
    let mut total: Option<f64> = None;
    let mut table: Option<String> = None;
    for atom in base_atoms_of_type(power, effect_type) {
        if atom.aspect != Some(Aspect::Cur)
            || atom.attrib_type != Some(AttribType::Expression)
            || atom.not_on_caster == Some(true)
            || !reaches_caster(atom, power)
        {
            continue;
        }
        let std_result = resolve_scaled_effect(
            atom.scale?,
            atom.modifier_table.as_deref(),
            archetype,
            level,
            db,
            errors,
        );
        let probe = HitPointsProbe {
            hit_points_percent: combat.hit_points_percent,
            std_result,
        };
        let expression = atom
            .magnitude_expression
            .as_deref()
            .filter(|e| !e.is_empty())?;
        let value = match eval(expression, &probe) {
            Ok(Value::Number(n)) => n,
            _ => return None,
        };
        total = Some(total.unwrap_or(0.0) + value);
        table = atom.modifier_table.as_deref().map(str::to_string);
    }
    total.map(|value| HpScalingResource { value, table })
}
