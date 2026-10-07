//! ToHit appliers, ported from `toHitBuffValue` (atom-query.ts): the atom-native
//! `tohitBuff` / `tohitBuffUnenhanced` values. A ToHit atom lands here when its
//! aspect is neither `Res` (→ debuffResistance) nor `Str` (→ specialBuff strength),
//! it is a positive-scale non-debuff, and its `ignoreStrength` matches the
//! requested half (`false` → enhanceable, `true` → unenhanced).

use super::{base_atoms_of_type, per_target_value_of, TypedValue};
use coh_data::{Aspect, AtomicEffect, EffectType, Power};

fn to_hit_buff_by_half(power: &Power, want_ignore_strength: bool) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::ToHit)
        .into_iter()
        .filter(|a| {
            a.aspect != Some(Aspect::Res)
                && a.aspect != Some(Aspect::Str)
                && (a.ignore_strength == Some(true)) == want_ignore_strength
                && a.scale.is_some_and(|s| s > 0.0)
                && !a
                    .modifier_table
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("debuff")
        })
        .collect();
    per_target_value_of(&atoms, power)
}

/// `toHitBuffValue(power)` — the enhanceable `tohitBuff` half.
pub fn to_hit_buff_value(power: &Power) -> Option<TypedValue> {
    to_hit_buff_by_half(power, false)
}

/// `toHitBuffValue(power, { ignoreStrength: true })` — the `tohitBuffUnenhanced` half.
pub fn to_hit_buff_unenhanced_value(power: &Power) -> Option<TypedValue> {
    to_hit_buff_by_half(power, true)
}
