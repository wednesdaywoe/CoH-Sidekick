//! Per-power atom appliers, ported one-for-one from `src/data/core/atom-query.ts`.
//! Each returns what one power contributes to one effect family — the value the
//! oracle fixtures pin corpus-wide.
//!
//! TS-truthiness is load-bearing here and mirrored explicitly: `gated` counts only
//! when `Some(true)`, a `perTarget` of `0` is as absent as a missing one, and an
//! empty-string `modifierTable` is as absent as none. Absent scales propagate as NaN
//! exactly like `Math.abs(undefined)` (and serialize to `null`, as `JSON.stringify`
//! does), rather than being silently defaulted to zero.

pub mod absorb;
pub mod accuracy;
pub mod damage;
pub mod debuff_resistance;
pub mod defense;
pub mod endurance_discount;
pub mod hp_scaling_resource;
pub mod maxhp;
pub mod mez_protection;
pub mod mez_resistance;
pub mod movement;
pub mod perception;
pub mod range;
pub mod recharge;
pub mod resistance;
pub mod resources;
pub mod stealth;
pub mod taunt_placate;
pub mod to_hit;

use coh_data::{
    excludes_caster, reaches_caster, AtomicEffect, EffectType, Power, Stacking, SubType,
};
use std::collections::HashSet;

/// `{ scale, table, perTarget? }` — the per-type value object the TS appliers return.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedValue {
    pub scale: f64,
    /// `None` mirrors an absent TS `table` (dropped by JSON.stringify); `Some("")`
    /// mirrors the `?? ''` fallback, which serializes as an empty string.
    pub table: Option<Box<str>>,
    pub per_target: Option<f64>,
}

pub(crate) fn truthy_per_target(a: &AtomicEffect) -> bool {
    a.per_target.is_some_and(|v| v != 0.0)
}

pub(crate) fn is_gated(a: &AtomicEffect) -> bool {
    a.gated == Some(true)
}

pub(crate) fn table_of(a: &AtomicEffect) -> Option<&str> {
    a.modifier_table.as_deref().filter(|t| !t.is_empty())
}

/// The bag's `isDebuff`: negative scale, or a `*debuff*` table (a −resistance at
/// scale ≥ 0 on a debuff table still debuffs).
pub(crate) fn is_debuff_atom(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}

/// A row the client prints but never applies. Brainstorm's Disruption Strike states a
/// −2.5 resistance debuff at `AnyAffected` on a `["Self"]` power, which every recipient
/// test resolves as the caster (TARGETS-3), so untagged it lands as a phantom self
/// penalty on all 8 types. Rest's real −10 crash has that exact shape, and the group's
/// `DisplayOnly` tag is the only thing telling the two apart.
///
/// Asked token-wise against the comma-joined `tags` string, so a future `NotDisplayOnly`
/// can't match. Totals-only: `power.atoms` stays complete, since printing the row is the
/// whole reason it exists. Twin of `isDisplayOnly` in `src/data/core/atom-query.ts`.
pub(crate) fn is_display_only(a: &AtomicEffect) -> bool {
    a.tags
        .as_deref()
        .is_some_and(|t| format!(",{t},").contains(",DisplayOnly,"))
}

pub(crate) fn atoms_of_type(power: &Power, t: EffectType) -> Vec<&AtomicEffect> {
    power
        .atoms
        .iter()
        .filter(|a| a.effect_type == Some(t) && !is_display_only(a))
        .collect()
}

/// The power's always-on atoms of one type (`baseAtomsOfType`): the converter-stamped
/// `gated` flag is the runtime's only base-vs-conditional signal.
pub(crate) fn base_atoms_of_type(power: &Power, t: EffectType) -> Vec<&AtomicEffect> {
    power
        .atoms
        .iter()
        .filter(|a| a.effect_type == Some(t) && !is_gated(a) && !is_display_only(a))
        .collect()
}

/// Group by subType, first-seen order (mirrors JS `Map` insertion order).
pub(crate) fn by_sub_type<'a>(
    atoms: &[&'a AtomicEffect],
) -> Vec<(Option<SubType>, Vec<&'a AtomicEffect>)> {
    let mut out: Vec<(Option<SubType>, Vec<&'a AtomicEffect>)> = Vec::new();
    for a in atoms {
        match out.iter_mut().find(|(k, _)| *k == a.sub_type) {
            Some((_, bucket)) => bucket.push(a),
            None => out.push((a.sub_type, vec![a])),
        }
    }
    out
}

/// Σ of `|val(a)|` over atoms with a DISTINCT `|val|` (dedup the type/duration copies).
/// An absent value mirrors `Math.abs(undefined)`: NaN joins the sum and poisons it,
/// exactly as in TS — never a silent zero.
pub(crate) fn sum_distinct_abs<'a>(
    atoms: impl IntoIterator<Item = &'a &'a AtomicEffect>,
    val: impl Fn(&AtomicEffect) -> Option<f64>,
) -> f64 {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut sum = 0.0;
    for a in atoms {
        let v = val(a).map_or(f64::NAN, f64::abs);
        // NaN dedups against itself (JS Set SameValueZero); normalize its bits.
        let bits = if v.is_nan() {
            f64::NAN.to_bits()
        } else {
            v.to_bits()
        };
        if !seen.insert(bits) {
            continue;
        }
        sum += v;
    }
    sum
}

fn wire_or_empty<T: Copy>(v: Option<T>, f: impl Fn(T) -> &'static str) -> &'static str {
    v.map(f).unwrap_or("")
}

/// Atom identity minus `duration` — the duration-bucketing key. Absent enum fields
/// join as empty strings (JS `Array.join` renders `undefined` as ``).
fn durationless_key(a: &AtomicEffect) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{:.4}",
        wire_or_empty(a.effect_type, |v| v.as_wire()),
        wire_or_empty(a.sub_type, |v| v.as_wire()),
        wire_or_empty(a.pv_mode, |v| v.as_wire()),
        if a.resistible == Some(true) { "R" } else { "U" },
        wire_or_empty(a.to_who, |v| v.as_wire()),
        wire_or_empty(a.attrib_type, |v| v.as_wire()),
        wire_or_empty(a.aspect, |v| v.as_wire()),
        a.modifier_table.as_deref().unwrap_or("").to_lowercase(),
        a.scale.unwrap_or(f64::NAN),
    )
}

/// Bucket otherwise-identical atoms by duration, longest-lived first, stable key
/// tiebreak (mirrors `durationBuckets`).
pub(crate) fn duration_buckets<'a>(
    atoms: &[&'a AtomicEffect],
) -> Vec<(String, Option<f64>, Vec<&'a AtomicEffect>)> {
    let mut out: Vec<(String, Option<f64>, Vec<&'a AtomicEffect>)> = Vec::new();
    for a in atoms {
        let key = durationless_key(a);
        match out
            .iter_mut()
            .find(|(k, d, _)| *k == key && d.map(f64::to_bits) == a.duration.map(f64::to_bits))
        {
            Some((_, _, bucket)) => bucket.push(a),
            None => out.push((key, a.duration, vec![a])),
        }
    }
    out.sort_by(|x, y| {
        // JS comparator `y.duration - x.duration || key cmp`: a NaN difference
        // (either side absent) falls through to the key tiebreak.
        match (x.1, y.1) {
            (Some(a), Some(b)) => b
                .partial_cmp(&a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| x.0.cmp(&y.0)),
            _ => x.0.cmp(&y.0),
        }
    });
    out
}

/// The per-target `{ scale, perTarget }` of a same-slot atom group (mirrors
/// `perTargetFromGroup`): dedup-summed distinct increments, N=1 base plus only the
/// increments landing on the caster. `table` is `Option` because `damageBuffValue`
/// passes its dominant-table key through, which can be a genuinely absent table
/// (JSON.stringify then drops the key) — `perTargetValueOf` always passes `Some`
/// (its `?? ''` fallback).
pub(crate) fn per_target_from_group(
    atoms: &[&AtomicEffect],
    table: Option<&str>,
    power: &Power,
) -> Option<TypedValue> {
    let increments: Vec<&&AtomicEffect> = atoms.iter().filter(|a| truthy_per_target(a)).collect();
    if increments.is_empty() {
        return None;
    }
    let per_target = sum_distinct_abs(increments.iter().copied(), |a| a.per_target.or(Some(0.0)));
    let bases: Vec<&&AtomicEffect> = atoms.iter().filter(|a| !truthy_per_target(a)).collect();
    let self_increments: Vec<&&AtomicEffect> = increments
        .iter()
        .copied()
        .filter(|a| reaches_caster(a, power))
        .collect();
    let scale = sum_distinct_abs(bases.iter().copied(), |a| a.scale)
        + sum_distinct_abs(self_increments.iter().copied(), |a| a.scale);
    Some(TypedValue {
        scale,
        table: table.map(Box::from),
        per_target: Some(per_target),
    })
}

/// Reconstruct one slot's `{ scale, table, perTarget? }` from a same-slot atom group
/// (mirrors `perTargetValueOf`): per-target reconstruction when increments exist,
/// otherwise the longest-lived instance — never the overlap sum.
pub(crate) fn per_target_value_of(atoms: &[&AtomicEffect], power: &Power) -> Option<TypedValue> {
    if atoms.is_empty() {
        return None;
    }
    let table = atoms.iter().find_map(|a| table_of(a)).unwrap_or("");
    if let Some(v) = per_target_from_group(atoms, Some(table), power) {
        return Some(v);
    }
    let buckets = duration_buckets(atoms);
    // Only genuine same-slot DUPLICATES — same recipient, all `Replace` — collapse by
    // max (`foldResourceSlot`'s maxHP rule). A tie across different recipients is not a
    // duplicate (Thermal Radiation's Fire Shield buffs the target at 2 and the caster at
    // 1), so it keeps the bucket-order pick.
    let tied: Vec<&&AtomicEffect> = buckets
        .iter()
        .filter(|b| b.1.map(f64::to_bits) == buckets[0].1.map(f64::to_bits))
        .flat_map(|b| b.2.iter())
        .collect();
    let same_recipient = tied.iter().all(|a| a.to_who == tied[0].to_who);
    // A `Replace` base beside a `Stack` increment on the same recipient is the engine's
    // CO-APPLICATION idiom, not two spellings of one value — both land, so the value at
    // one target is their sum. Reached only when the converter did not stamp `perTarget`:
    // Memento Mori's +MaxHP (Replace 3 beside Stack 0.15, both 30s) arrives through a
    // redirect, and the redirect path deliberately withholds the per-foe stamp.
    let co_applied = same_recipient
        && tied.iter().any(|a| a.stacking == Some(Stacking::Replace))
        && tied.iter().any(|a| {
            matches!(
                a.stacking,
                Some(Stacking::Stack) | Some(Stacking::Continuous) | Some(Stacking::RefreshToCount)
            )
        });
    if co_applied {
        // Dedup by (|scale|, table) for the same reason `per_target_from_group` does: a
        // by-type buff repeats one value across N atoms.
        let mut distinct: Vec<(u64, Option<&str>)> = Vec::new();
        let mut sum = 0.0;
        for atom in &tied {
            let scale = atom.scale.map_or(f64::NAN, f64::abs);
            let key = (scale.to_bits(), table_of(atom));
            if !distinct.contains(&key) {
                distinct.push(key);
                sum += scale;
            }
        }
        return Some(TypedValue {
            scale: sum,
            table: Some(Box::from(table)),
            per_target: None,
        });
    }
    let duplicates = same_recipient && tied.iter().all(|a| a.stacking == Some(Stacking::Replace));
    let scale = if duplicates {
        tied.iter()
            .map(|a| a.scale.map_or(f64::NAN, f64::abs))
            .fold(f64::NEG_INFINITY, f64::max)
    } else {
        buckets[0].2[0].scale.map_or(f64::NAN, f64::abs)
    };
    Some(TypedValue {
        scale,
        table: Some(Box::from(table)),
        per_target: None,
    })
}

/// First atom carrying a truthy table, `base` preferred over the whole group
/// (the defense table-resolution rule).
pub(crate) fn preferred_table<'a>(
    base: &[&'a AtomicEffect],
    group: &[&'a AtomicEffect],
) -> Option<&'a str> {
    base.iter()
        .find_map(|a| table_of(a))
        .or_else(|| group.iter().find_map(|a| table_of(a)))
}
