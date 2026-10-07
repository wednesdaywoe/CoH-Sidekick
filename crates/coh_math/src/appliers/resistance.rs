//! `resistanceBuffValue` / `resistanceSelfDebuffValue`, ported from atom-query.ts.

use super::*;
use coh_data::{reaches_caster, Aspect, EffectType, Power, SubType};

/// The eight standard damage types the calc totals as `res<Type>` globals. Every
/// other subType (All, Kheldian/signature types, Heal, Special) is behavior-
/// irrelevant to the caster's resistance totals and excluded on both sides.
pub(crate) const RESIST_STD_SUBTYPES: [SubType; 8] = [
    SubType::Smashing,
    SubType::Lethal,
    SubType::Fire,
    SubType::Cold,
    SubType::Energy,
    SubType::Negative,
    SubType::Toxic,
    SubType::Psionic,
];

fn is_std(a: &coh_data::AtomicEffect) -> bool {
    a.sub_type.is_some_and(|s| RESIST_STD_SUBTYPES.contains(&s))
}

/// The per-damage-type +resistance BUFF this power contributes (`effects.resistance`),
/// keyed by lowercase type. `None` when no standard-type buff atom exists (bag fallback).
pub fn resistance_buff_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    let atoms: Vec<_> = base_atoms_of_type(power, EffectType::Resistance)
        .into_iter()
        .filter(|a| a.aspect == Some(Aspect::Res) && is_std(a) && !is_debuff_atom(a))
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (sub, group) in by_sub_type(&atoms) {
        if let Some(v) = per_target_value_of(&group, power) {
            out.push((
                sub.map(|s| s.as_wire().to_lowercase()).unwrap_or_default(),
                v,
            ));
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// The caster's own −resistance PENALTY (`effects.resistanceDebuff`'s self-directed
/// entries): last-write-wins per type, `|scale|`. `None` on no such atom (bag fallback).
///
/// [`reaches_caster`], not the anchor-only reading: Rest states its −resistance at
/// `AnyAffected` on a `["Self"]` power, so only the power resolves it as the caster's
/// (TARGETS-3).
///
pub fn resistance_self_debuff_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    let atoms: Vec<_> = base_atoms_of_type(power, EffectType::Resistance)
        .into_iter()
        .filter(|a| {
            a.aspect == Some(Aspect::Res)
                && is_std(a)
                && is_debuff_atom(a)
                && reaches_caster(a, power)
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (sub, group) in by_sub_type(&atoms) {
        let last = group[group.len() - 1];
        out.push((
            sub.map(|s| s.as_wire().to_lowercase()).unwrap_or_default(),
            TypedValue {
                scale: last.scale.map_or(f64::NAN, f64::abs),
                table: last.modifier_table.clone(),
                per_target: None,
            },
        ));
    }
    Some(out)
}

/// Whether each self −Resistance penalty type is RESISTIBLE — i.e. whether the caster's own
/// resistance to that type mitigates it (`effective = nominal × (1 − R)`). An
/// `IgnoreResistance` atom (`resistible: false`) applies flat instead.
///
/// A sibling of [`resistance_self_debuff_value`] rather than a wider return type: that
/// function's shape is what the TS-fixture oracle gate diffs against, and the flag is a
/// calc-side concern the fixtures do not carry. Absent on the atom means resistible — CoH
/// resists −Res by default.
pub fn resistance_self_debuff_resistible(power: &Power) -> Vec<(String, bool)> {
    let atoms: Vec<_> = base_atoms_of_type(power, EffectType::Resistance)
        .into_iter()
        .filter(|a| {
            a.aspect == Some(Aspect::Res)
                && is_std(a)
                && is_debuff_atom(a)
                && reaches_caster(a, power)
        })
        .collect();
    by_sub_type(&atoms)
        .into_iter()
        .map(|(sub, group)| {
            let last = group[group.len() - 1];
            (
                sub.map(|s| s.as_wire().to_lowercase()).unwrap_or_default(),
                last.resistible != Some(false),
            )
        })
        .collect()
}
