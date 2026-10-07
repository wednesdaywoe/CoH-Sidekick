//! MaxHP appliers, ported from `maxHPBuffValue` (atom-query.ts): the atom-native
//! `maxHPBuff` / `maxHPBuffUnenhanced` values. A MaxHP atom is a +MaxHP buff when
//! its aspect is `Max` (a non-Max HitPoints atom is a HEAL, not this slot), it is
//! not a debuff (a −MaxHP is skipped), and its `ignoreStrength` matches the
//! requested half — the twins co-apply and SUM at totals time so the +Healing
//! strength multiplier hits only the enhanceable half.

use super::{base_atoms_of_type, is_debuff_atom, per_target_value_of, TypedValue};
use coh_data::{Aspect, AtomicEffect, EffectType, Power};

fn max_hp_buff_by_half(power: &Power, want_ignore_strength: bool) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::MaxHp)
        .into_iter()
        .filter(|a| {
            a.aspect == Some(Aspect::Max)
                && (a.ignore_strength == Some(true)) == want_ignore_strength
                && !is_debuff_atom(a)
        })
        .collect();
    per_target_value_of(&atoms, power)
}

/// `maxHPBuffValue(power)` — the enhanceable `maxHPBuff` half.
pub fn max_hp_buff_value(power: &Power) -> Option<TypedValue> {
    max_hp_buff_by_half(power, false)
}

/// `maxHPBuffValue(power, { ignoreStrength: true })` — the `maxHPBuffUnenhanced` half.
pub fn max_hp_buff_unenhanced_value(power: &Power) -> Option<TypedValue> {
    max_hp_buff_by_half(power, true)
}
