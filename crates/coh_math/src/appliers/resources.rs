//! Regen/Recovery appliers, ported from `resourceBuffValue` (atom-query.ts): the
//! atom-native `regenBuff` / `regenBuffUnenhanced` / `recoveryBuff` /
//! `recoveryBuffUnenhanced` values. The hardest resource slots:
//!
//! - `notOnCaster` atoms are skipped (Thunderspy's resource target-trap — the bag
//!   deletes the slot, the caster's total must too).
//! - Any `Expression`-typed resource atom PUNTS the whole slot to the bag (the
//!   converter's tick-chance-0 drop is not re-derivable from the wire).
//! - Per-target increments always route to the ENHANCEABLE slot regardless of
//!   their own `ignoreStrength` (Reactive Regeneration's IgnoreStrength pseudo-pet
//!   increment still lands in `regenBuff`, never the twin).
//! - N=1 counts only NON-IgnoreStrength self-increments — the atom-derivable
//!   discriminator between Consume/Devour Psyche (counted) and Reactive
//!   Regeneration (not counted).
//! - Flat atoms fold by `fold_resource_sum`: RAW SUM while the table holds, RESET
//!   on a table change (unlike defense/resistance last-write-wins and maxHP's
//!   Replace collapse). Burst/tail pairs overlap and sum (Icy Bastion +10).

use super::{
    base_atoms_of_type, is_debuff_atom, sum_distinct_abs, table_of, truthy_per_target, TypedValue,
};
use coh_data::{reaches_caster, Aspect, AtomicEffect, AttribType, EffectType, Power, ToWho};

/// The converter's `foldResourceSlot` SUM semantics: Σ|scale| while the table is
/// unchanged (Option-equal — two absent tables match), reset on change,
/// last-table-wins.
fn fold_resource_sum(atoms: &[&&AtomicEffect]) -> TypedValue {
    let mut scale = 0.0f64;
    let mut table: Option<&str> = atoms[0].modifier_table.as_deref();
    for a in atoms {
        if a.modifier_table.as_deref() == table {
            scale += a.scale.map_or(f64::NAN, f64::abs);
        } else {
            scale = a.scale.map_or(f64::NAN, f64::abs);
            table = a.modifier_table.as_deref();
        }
    }
    TypedValue {
        scale,
        table: table.map(Box::from),
        per_target: None,
    }
}

fn resource_buff_value(
    power: &Power,
    effect_type: EffectType,
    want_ignore_strength: bool,
) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, effect_type)
        .into_iter()
        .filter(|a| {
            a.aspect != Some(Aspect::Res) && !is_debuff_atom(a) && a.not_on_caster != Some(true)
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    // PUNT: the Expression + tick-chance-0 drop is not re-derivable.
    if atoms
        .iter()
        .any(|a| a.attrib_type == Some(AttribType::Expression))
    {
        return None;
    }

    let increments: Vec<&&AtomicEffect> = atoms.iter().filter(|a| truthy_per_target(a)).collect();
    // The flat base must LAND ON THE CASTER to count toward his own totals. Without this the
    // reader credits him with a foe debuff or an ally buff written on a power he merely owns:
    // Temporal Bomb's `Location` -Recovery patch read as +37.5% recovery, Thunderspy's Rally The
    // Militia (a pet-only buff) as +50% regen. `is_ally_only` in the apply loop catches only the
    // powers whose WHOLE targetType is an ally; it has nothing to say about one foe-facing row on
    // a power that also touches the caster, which is the recipient question the atom answers
    // directly (TARGETS-2/3). The per-target increments are deliberately NOT filtered here — a
    // per-foe increment is collected FROM foes and lands on the caster, and the increment path
    // below applies `reaches_caster` itself where it credits him at one target.
    let flat: Vec<&&AtomicEffect> = atoms
        .iter()
        .filter(|a| !truthy_per_target(a) && reaches_caster(a, power))
        .collect();

    // Per-target increments follow the FLAT BASE's slot, not their own
    // `ignoreStrength` — the converter patches whichever slot the base occupies
    // (`_remapUnenhancedPatchKeys`). That is the enhanceable one for every Homecoming
    // and Rebirth power, and for a base-less increment; Thunderspy's Rise to the
    // Challenge is the first with an IgnoreStrength base, so its increment belongs to
    // the twin (bag: `regenBuffUnenhanced` 1.25 +0.25/foe, no `regenBuff`).
    let base_is_unenhanced =
        !flat.is_empty() && flat.iter().all(|a| a.ignore_strength == Some(true));
    if want_ignore_strength == base_is_unenhanced && !increments.is_empty() {
        let table = increments
            .iter()
            .find_map(|a| table_of(a))
            .or_else(|| flat.iter().find_map(|a| table_of(a)))
            .unwrap_or("");
        let per_target =
            sum_distinct_abs(increments.iter().copied(), |a| a.per_target.or(Some(0.0)));
        // At one target: the flat base, plus only those self-increments belonging to
        // THIS slot. What disqualifies Reactive Regeneration's increment is not that it
        // is IgnoreStrength but that its base is not — it is a pseudo-pet buff riding an
        // enhanceable base, so it does not count at one target.
        let self_increments: Vec<&&AtomicEffect> = increments
            .iter()
            .copied()
            .filter(|a| {
                reaches_caster(a, power)
                    && (a.ignore_strength == Some(true)) == want_ignore_strength
            })
            .collect();
        let flat_mine: Vec<&&AtomicEffect> = flat
            .iter()
            .copied()
            .filter(|a| (a.ignore_strength == Some(true)) == want_ignore_strength)
            .collect();
        let scale = sum_distinct_abs(flat_mine.iter().copied(), |a| a.scale)
            + sum_distinct_abs(self_increments.iter().copied(), |a| a.scale);
        return Some(TypedValue {
            scale,
            table: Some(Box::from(table)),
            per_target: Some(per_target),
        });
    }

    let mine: Vec<&&AtomicEffect> = flat
        .iter()
        .copied()
        .filter(|a| (a.ignore_strength == Some(true)) == want_ignore_strength)
        .collect();
    if mine.is_empty() {
        return None;
    }
    Some(fold_resource_sum(&mine))
}

/// `regenBuffValue(power)` — the enhanceable `regenBuff` half.
pub fn regen_buff_value(power: &Power) -> Option<TypedValue> {
    resource_buff_value(power, EffectType::Regeneration, false)
}

/// `regenBuffValue(power, { ignoreStrength: true })` — the `regenBuffUnenhanced` half.
pub fn regen_buff_unenhanced_value(power: &Power) -> Option<TypedValue> {
    resource_buff_value(power, EffectType::Regeneration, true)
}

/// `recoveryBuffValue(power)` — the enhanceable `recoveryBuff` half.
pub fn recovery_buff_value(power: &Power) -> Option<TypedValue> {
    resource_buff_value(power, EffectType::Recovery, false)
}

/// `recoveryBuffValue(power, { ignoreStrength: true })` — the `recoveryBuffUnenhanced` half.
pub fn recovery_buff_unenhanced_value(power: &Power) -> Option<TypedValue> {
    resource_buff_value(power, EffectType::Recovery, true)
}

/// `maxEndBuff` — the +Max Endurance buff (ATOM8): Superior Conditioning, Physical Perfection,
/// Power of the Depths' team +MaxEnd, Burnout's self −MaxEnd. The caller resolves `scale ×
/// enhMultiplier` (absolute endurance POINTS, NO ×100) and accumulates into `maxEndurance`.
///
/// Three ways this differs from `resource_buff_value` (regen/recovery), all measured (census HC 6/6
/// · Reb 3/3 · tspy 0, 0 bag-only/0 phantom):
///
/// 1. **No enhanceable/unenhanceable twin.** The converter routes `endurance@maximum` (bridge-folded
///    to `EffectType::MaxEndurance`) to the single `maxEndBuff` slot regardless of `ignoreStrength`
///    (no twin split), so every surviving atom is summed — there is no `want_ignore_strength`
///    parameter.
/// 2. **Self-debuffs are KEPT (no `!is_debuff` filter).** The converter's `aspect === 'maximum'`
///    branch runs BEFORE the `isDebuff`/drain branch, so a NEGATIVE self max-end atom (Burnout −25 →
///    `makeEffect` `Math.abs` → +25) still lands in `maxEndBuff`. Applying regen/recovery's
///    `!is_debuff_atom` gate would wrongly drop it (the ATOM7 unconditional-branch lesson).
/// 3. **The one exclusion is a FOE-facing debuff** (`toWho == Target` AND `is_debuff` — Soul
///    Consumption's −1 foe max-end drain): not a caster buff, dropped for caster-stat purposes (the
///    converter's standard foe-drop). Its ally-Target +buff (Power of the Depths +20) is `toWho ==
///    Target` but positive, so it is KEPT.
///
/// Fold = the shared resource-sum (Σ|scale| while the table holds, reset on a table change). `None` ⇒
/// fall back to `bag.max_end_buff()` (all Thunderspy — zero `MaxEndurance` atoms, TSPY-3 — and any
/// residue).
pub fn max_endurance_buff_value(power: &Power) -> Option<TypedValue> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::MaxEndurance)
        .into_iter()
        .filter(|a| {
            a.not_on_caster != Some(true) && !(a.to_who == Some(ToWho::Target) && is_debuff_atom(a))
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    // PUNT: the Expression + tick-chance-0 drop is not re-derivable (mirrors `resource_buff_value`;
    // corpus-vacuous for maxEnd — no Expression max-end atoms — kept for faithfulness).
    if atoms
        .iter()
        .any(|a| a.attrib_type == Some(AttribType::Expression))
    {
        return None;
    }
    let refs: Vec<&&AtomicEffect> = atoms.iter().collect();
    Some(fold_resource_sum(&refs))
}
