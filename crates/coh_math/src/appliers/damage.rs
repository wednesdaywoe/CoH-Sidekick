//! Damage-buff applier, ported from `damageBuffValue` (atom-query.ts): the
//! atom-native `damageBuff` value (Build Up, Assault, Soul Drain, AAO, Fulcrum
//! Shift). A +damage buff is not scalar — it explodes into one atom per damage
//! type (8–13 identical-scale siblings), so four axes are handled: damage-type
//! collapse (dedup by distinct |scale|), dominant table (drops off-table riders
//! like a blaster's `Melee_Ones` Defiance increment), per-target N=1 via `toWho`
//! (AAO's Self increment folds into N=1; Fulcrum Shift's Target increment does
//! not), and the non-uniform primary (the group covering the MOST damage types,
//! ties → longest duration → largest scale).
//!
//! A fifth axis is a REJECTION rather than a reconciliation: a `Defiance`-tagged
//! atom is dropped outright — see [`is_defiance`].

use super::{is_gated, per_target_from_group, TypedValue};
use coh_data::{reaches_caster, Aspect, AtomicEffect, EffectType, Power, SubType};

/// True when this atom belongs to a `Defiance`-tagged effect group — the Blaster
/// inherent, which the game data names on the group itself.
///
/// Defiance is a PER-CAST transient: every Blaster attack grants a few seconds of
/// self +Damage, so what a Blaster actually has is a rotation-dependent ramp, not a
/// sustained total. The totals dashboard reports the sustained value, which is why
/// [`crate::inherents`] does not model Defiance at all and why the beta's
/// `TRANSIENT_UNMODELED_ADJUSTERS` leaves the same class of combat state
/// (Storm's clear skies, Dual Pistols ammo, Staff perfection) out.
///
/// It reached the total anyway, because the converter routes the tagged group into
/// the ordinary `damageBuff` slot like any Build Up: End of Time (+5.4%) and Future
/// Pain (+11%) read as +16.4% global damage on a Blaster with no +Damage set bonus
/// anywhere, permanently, at one stack, for whichever attacks happened to be flagged
/// active — a number matching no game state. Reported 2026-08-05.
///
/// Dropped here rather than at the converter because the atom is REAL and the power
/// info panel is right to show it; only the caster's sustained totals must ignore it,
/// exactly as [`AtomicEffect::not_on_caster`](coh_data::AtomicEffect::not_on_caster)
/// is stamped rather than dropped. 115 Homecoming powers carry a Defiance atom; on 31
/// of them it IS the whole `damageBuff`, and the one power that mixes it with a genuine
/// +Damage buff (Soul Drain, whose `Melee_Ones` rider sits beside a `Melee_Buff_Dmg`
/// per-foe increment) is already settled by the dominant-table filter in
/// [`damage_buff_value`]. So this takes nothing else with it — Build Up, Aim, Soul
/// Drain, AAO and Fulcrum Shift are untouched.
///
/// Homecoming-only by the same schema fact that governs every tag: Parse6 has no group
/// to hang one on, so a Rebirth/Thunderspy Defiance rider is caught only by that
/// dominant-table filter.
pub(crate) fn is_defiance(a: &AtomicEffect) -> bool {
    a.tags
        .as_deref()
        .is_some_and(|t| t.split(',').any(|tag| tag.trim() == "Defiance"))
}

/// True when this power carries `DamageBuff` strength atoms and EVERY one of them is
/// Defiance — i.e. [`damage_buff_value`] found the slot and rejected all of it.
///
/// The apply walk needs this because its `?? effects.damageBuff` bag fallback would
/// otherwise undo the rejection: the bag slot holds the same Defiance value, and an
/// empty atom read is exactly the "atom-less legacy power" signal that fallback exists
/// to serve. Distinguishing "no +damage buff here" from "a +damage buff we decline to
/// count" is the whole job.
pub fn damage_buff_is_defiance_only(power: &Power) -> bool {
    let mut any = false;
    for a in power.atoms.iter().filter(|a| is_damage_buff_atom(a)) {
        if !is_defiance(a) {
            return false;
        }
        any = true;
    }
    any
}

/// The caster's own −damage crash (Granite Armor −30%, Bio Defensive Adaptation −25%), or
/// `None` when this power has no such atom.
///
/// The converter writes ONE `effects.damageDebuff` slot per power and tags it `toWho: 'Self'`
/// only when the atom that wrote it last was self-targeting (`convert-powerset.cjs:5705`). So
/// the mirror is last-write-wins over EVERY `_dmg` strength debuff, self or foe, with the self
/// test after the fold rather than as a filter before it. A power carrying a self crash and then
/// a foe −damage debuff ends up with a foe-tagged slot the calc reads as nothing on the caster,
/// and filtering first would resurrect the crash the converter dropped. No corpus power has that
/// ordering today — swapping the two passes the parity guard — so this follows the converter's
/// rule rather than the distribution that currently makes both forms agree.
///
/// Value is `|scale|` (`makeEffect`'s `Math.abs`); the apply site negates it.
pub fn self_damage_debuff_value(power: &Power) -> Option<TypedValue> {
    let last = power.atoms.iter().rfind(|a| {
        !is_gated(a)
            && a.effect_type == Some(EffectType::DamageBuff)
            && a.aspect == Some(Aspect::Str)
            && is_damage_debuff(a)
    })?;
    reaches_caster(last, power).then(|| TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: None,
    })
}

/// The converter's `isDebuff` for a damage-strength atom: a negative scale, or a table that
/// names itself a debuff (`convert-powerset.cjs:5581`). Deliberately not
/// [`super::is_debuff_atom`] — that helper is the same rule, but this one is a mirror of one
/// converter line and must not drift with it.
fn is_damage_debuff(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}

/// The `DamageBuff` strength-atom test both entry points share, so the set
/// [`damage_buff_value`] reads and the set [`damage_buff_is_defiance_only`] judges
/// cannot drift apart.
fn is_damage_buff_atom(a: &AtomicEffect) -> bool {
    !is_gated(a)
        && a.effect_type == Some(EffectType::DamageBuff)
        && a.aspect == Some(Aspect::Str)
        && a.scale.is_some_and(|s| s > 0.0)
        && !a
            .modifier_table
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("debuff")
}

/// One `(|scale|, duration)` group of same-value damage-type siblings.
struct Group {
    types: Vec<Option<SubType>>,
    duration: Option<f64>,
    scale: f64,
}

/// NaN-normalized bits so NaN groups with itself (JS string keys stringify every
/// NaN identically).
fn bits(v: f64) -> u64 {
    if v.is_nan() {
        f64::NAN.to_bits()
    } else {
        v.to_bits()
    }
}

pub fn damage_buff_value(power: &Power) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = power
        .atoms
        .iter()
        .filter(|a| is_damage_buff_atom(a) && !is_defiance(a))
        .collect();
    if atoms.is_empty() {
        return None;
    }

    // Dominant table: the one carrying the most total |scale|. An absent table is
    // its own key (the JS Map keys `undefined` distinctly), insertion order with
    // strict `>` means the first-seen table wins ties.
    let mut table_weight: Vec<(Option<&str>, f64)> = Vec::new();
    for a in &atoms {
        let key = a.modifier_table.as_deref();
        let w = a.scale.map_or(f64::NAN, f64::abs);
        match table_weight.iter_mut().find(|(k, _)| *k == key) {
            Some((_, acc)) => *acc += w,
            None => table_weight.push((key, w)),
        }
    }
    let mut table: Option<&str> = None;
    let mut best = f64::NEG_INFINITY;
    for (t, w) in &table_weight {
        if *w > best {
            best = *w;
            table = *t;
        }
    }
    let atoms: Vec<&AtomicEffect> = atoms
        .into_iter()
        .filter(|a| a.modifier_table.as_deref() == table)
        .collect();

    // Per-target increments present → shared reconstruction (dedup + toWho N=1).
    if let Some(v) = per_target_from_group(&atoms, table, power) {
        return Some(v);
    }

    // No per-target increment: group by (|scale|, duration) in insertion order;
    // the headline is the group shared by the most damage types.
    let mut groups: Vec<((u64, Option<u64>), Group)> = Vec::new();
    for a in &atoms {
        let s = a.scale.map_or(f64::NAN, f64::abs);
        let key = (bits(s), a.duration.map(bits));
        let g = match groups.iter_mut().position(|(k, _)| *k == key) {
            Some(i) => &mut groups[i].1,
            None => {
                groups.push((
                    key,
                    Group {
                        types: Vec::new(),
                        duration: a.duration,
                        scale: s,
                    },
                ));
                &mut groups.last_mut().unwrap().1
            }
        };
        if !g.types.contains(&a.sub_type) {
            g.types.push(a.sub_type);
        }
    }
    // JS initial primary: { types: ∅, duration: -1, scale: 0 }. The duration
    // tie-breaks mirror JS exactly: `>` is false when either side is absent/NaN,
    // `===` holds for undefined===undefined but never NaN===NaN.
    let mut p_types = 0usize;
    let mut p_duration: Option<f64> = Some(-1.0);
    let mut p_scale = 0.0f64;
    for (_, g) in &groups {
        let dur_gt = matches!((g.duration, p_duration), (Some(a), Some(b)) if a > b);
        let dur_eq = match (g.duration, p_duration) {
            (Some(a), Some(b)) => a == b,
            (None, None) => true,
            _ => false,
        };
        if g.types.len() > p_types
            || (g.types.len() == p_types && dur_gt)
            || (g.types.len() == p_types && dur_eq && g.scale > p_scale)
        {
            p_types = g.types.len();
            p_duration = g.duration;
            p_scale = g.scale;
        }
    }
    Some(TypedValue {
        scale: p_scale,
        table: table.map(Box::from),
        per_target: None,
    })
}
