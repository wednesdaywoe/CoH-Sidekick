//! Recharge appliers — the atom-native `rechargeBuff` speed buff and its self-directed
//! `rechargeDebuff` crash. Migrated from the bag (ATOM1 / DATA-GAP-REGISTER PASS2B-3): the
//! typed `RechargeTime` atoms already ship, and the beta reads only the bag, so this is the
//! rebuild reading atoms where the oracle reads the bag. It stays behavior-preserving because
//! the atom value equals the bag value exactly for the whole corpus (measured: buff HC 28/28 ·
//! Reb 20/20, crash 2/2 both, zero phantoms — see the ATOM1 design spec). The bag stays as the
//! `?? bag` fallback for the atom-less residue (HC Entropic Aura's pure per-target increment,
//! and every Thunderspy recharge power — TSPY-3 `Unmapped`).
//!
//! Both readers mirror the converter's `rechargeBuff`/`rechargeDebuff` gate on the atom side.
//! The **sign** partitions them: a buff is `scale ≥ 0`, the crash is `scale < 0` (or a slow
//! table), so the two filters are disjoint on the same power.
//!
//! The buff gate is `aspect == Str` **specifically**, not the converter's literal
//! `aspect != resistance`: Thunderspy carries `RechargeTime` atoms with a mis-decoded non-`Str`
//! aspect and no bag entry (a TSPY-3 typing artifact), and `!= Res` would fabricate a recharge
//! total from them.
//!
//! `== Str` was believed to be phantom-free, and it is not: 23 Thunderspy powers carry a
//! Str-aspect `RechargeTime` row that the converter's `guardThunderspyOnesBuffs` deliberately
//! deletes from the bag (its shortHelp does not advertise `+Rech`), so this reader credited a
//! buff the oracle had already judged a parse artifact — up to +120% on one Blaster secondary,
//! Temporal Manipulation shipping six such powers at +20% each. The guard now stamps the atoms
//! it drops and the `not_on_caster` filter below honours it (TSPY-10). Thunderspy is NOT "left
//! on the bag": it has no bag rows here at all, so the stamp is the only thing separating a
//! real +recharge from the artifact.

use super::{base_atoms_of_type, is_debuff_atom, table_of, TypedValue};
use coh_data::{reaches_caster, Aspect, AtomicEffect, EffectType, Power};

/// The converter's slow-table guard (`table.toLowerCase().includes('slow')`) — a positive-scale
/// `RechargeTime` on a slow table is a debuff, routed to `rechargeDebuff`, not `rechargeBuff`.
fn is_slow_table(a: &AtomicEffect) -> bool {
    a.modifier_table
        .as_deref()
        .is_some_and(|t| t.to_lowercase().contains("slow"))
}

/// Fold the matching atoms to one `TypedValue`.
///
/// The fold sums the DISTINCT buff-values in the slot, not every row. Two rows that agree on
/// magnitude, table, duration, stacking and per-target but differ only in recipient are the SAME
/// `+recharge` buff broadcast to more than one target — Conduit of Pain's `+50%` to an ally and to
/// the caster, both `Ranged_Ones`/`Stack`/60s at `0.5`. Counting both sums the buff twice
/// (`1.0` for a `+50%`), which is DATA-GAP-REGISTER TSPY-11. Dedup to one representative per
/// distinct row — the identity key deliberately excludes `to_who` — and sum those, so Conduit reads
/// `0.5`.
///
/// A genuine multi-magnitude slot still sums: Beta Decay's `0.10` (`Replace`) and `0.025`
/// (`Continuous`) are both `Self` but differ in magnitude and stacking, so they carry distinct
/// keys and both count. The bag's last-write-wins collapsed equal rows to one; this reproduces
/// that over the whole corpus, and the TS oracle (`rechargeBuffValue`) agrees byte-for-byte.
fn fold(atoms: &[&AtomicEffect]) -> TypedValue {
    use std::collections::HashSet;
    // (|scale|, table, duration, stacking, |per_target|) — every field that says WHICH buff this
    // is, minus the recipient. Formatted with Debug (round-trip-safe for f64, `None` a distinct
    // spelling) into a String key, so an absent value stays a distinct class rather than a NaN
    // that would neither dedup nor sum cleanly.
    let mut seen: HashSet<String> = HashSet::new();
    let mut scale = 0.0;
    for a in atoms {
        let key = format!(
            "{:?}",
            (
                a.scale.map(f64::abs),
                a.modifier_table.as_deref(),
                a.duration.map(f64::abs),
                a.stacking,
                a.per_target.map(f64::abs),
            )
        );
        if seen.insert(key) {
            if let Some(s) = a.scale {
                scale += s.abs();
            }
        }
    }
    let table = atoms.iter().find_map(|a| table_of(a)).map(Box::from);
    let per_target_sum: f64 = atoms
        .iter()
        .filter_map(|a| a.per_target)
        .filter(|v| *v != 0.0)
        .sum();
    let per_target = (per_target_sum != 0.0).then_some(per_target_sum);
    TypedValue {
        scale,
        table,
        per_target,
    }
}

/// `rechargeBuff` — the +recharge speed buff (Hasten, Quickness, Speed Boost). Read directly as
/// `scale × 100` by the caller (no AT-table resolution — recharge buffs carry the final
/// fraction on a `*_Ones` table). NOT enhanced by Recharge enhancements (those cut a power's own
/// recharge time, not a +recharge buff). `None` ⇒ fall back to `bag.recharge_buff()`.
pub fn recharge_buff_value(power: &Power) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::RechargeTime)
        .into_iter()
        .filter(|a| a.aspect == Some(Aspect::Str))
        // `base_atoms_of_type` filters `gated` and `displayOnly` but NOT `not_on_caster` — each
        // applier states that itself (resources, defense, absorb all do). Omitting it here is
        // what let the 23 Thunderspy artifacts through; `gather` honours the flag but never
        // sees this read, which is per-power.
        .filter(|a| a.not_on_caster != Some(true))
        .filter(|a| !is_debuff_atom(a))
        .filter(|a| !is_slow_table(a))
        .collect();
    if atoms.is_empty() {
        return None;
    }
    Some(fold(&atoms))
}

/// The self-directed `rechargeDebuff` crash (Granite Armor −65%, Reaction Time −40%). Self-facing
/// only (`toWho == Self`) — a foe-facing `rechargeDebuff` is an enemy debuff, never the caller's
/// total. Unlike the buff, the caller resolves this through the AT table and subtracts
/// (`resolveScaledEffect(|scale|, table) × −100`). `None` ⇒ fall back to
/// `bag.self_recharge_debuff()`.
pub fn recharge_self_debuff_value(power: &Power) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::RechargeTime)
        .into_iter()
        .filter(|a| reaches_caster(a, power))
        .filter(|a| a.aspect == Some(Aspect::Str))
        .filter(|a| is_debuff_atom(a) || is_slow_table(a))
        .collect();
    if atoms.is_empty() {
        return None;
    }
    Some(fold(&atoms))
}
