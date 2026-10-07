//! Endurance-discount applier — the atom-native EndDisc buff (`+EnduranceDiscount`, the endurance
//! cost reduction of Conserve Power, Body Mastery, Force Affinity, …). Migrated from the bag (ATOM7
//! / DATA-GAP-REGISTER PASS2B-7): the typed `EnduranceDiscount` atoms already ship, and the beta
//! reads only `effects.enduranceDiscount` (character-totals.ts:1757) — it has no
//! `enduranceDiscountValue` atom reader — so this is the rebuild reading atoms where the frozen
//! oracle reads the bag. It stays behavior-preserving because the atom value equals the bag value
//! exactly for the whole corpus (census: HC 8/8 · Reb 9/9, zero phantoms, zero bag-only — the
//! `atom_confirmation` endurancediscount arm and the `endurance_discount_atom_bag_parity` guard
//! both pin it). The bag stays the `?? bag` fallback for the atom-less residue: all Thunderspy
//! (TSPY-3 `Unmapped`, zero `EnduranceDiscount` atoms) and any HC/Reb power with none.
//!
//! Mirrors the converter's `enduranceDiscount` gate on the atom side — the simplest in the ATOM
//! series. The converter routes the `EnduranceDiscount` combat-modifier attrib
//! (convert-powerset.cjs:4667) UNCONDITIONALLY: no aspect, debuff, or self sub-branch, just
//! `effects.enduranceDiscount = makeEffect()` (OVERWRITE / last-write-wins, `Math.abs` on scale).
//! So the reader takes the LAST non-gated `EnduranceDiscount` atom with NO aspect/debuff filter —
//! this is the load-bearing difference from range/perception (which exclude `aspect == Res` and the
//! debuff face): here an `aspect=Res` or foe-facing (`toWho=Target`) atom is written to the same
//! slot too, and the census confirms the unconditional mirror is phantom-free on all three datasets
//! (dropping any of those atoms would make its power bag-only). The endurance DEBUFF-RESISTANCE is a
//! DIFFERENT attrib — the `Endurance` resource with `aspect=resistance` → `debuffResistance.endurance`
//! (ATOM13) — so there is no aspect collision on `EnduranceDiscount` to gate against.
//!
//! The consumer's `> 0` gate is SEPARATE and stays at the call site, applied identically to whichever
//! source produced the value, so it does not affect the atom == bag slot equivalence this reader
//! rests on. (Because `makeEffect` abs's the scale, the only value the `> 0` gate drops is an exact
//! zero — but it is faithful to the beta and stays.)

use super::TypedValue;
use coh_data::{EffectType, Power};

/// `enduranceDiscount` — the EndDisc buff (Conserve Power, Body Mastery, Force Affinity). Resolved
/// `scale × 100` by the caller (AT-table resolved), gated `> 0`, accumulated into the canonical
/// `endurance` (EndDisc) accumulator; NOT enhanced. OVERWRITE fold: the LAST non-gated
/// `EnduranceDiscount` atom, with NO aspect/debuff filter (the converter's unconditional
/// `= makeEffect()`), `|scale|` mirroring `makeEffect`'s `Math.abs`. An absent scale becomes NaN,
/// not a silent zero (the mod-level `Math.abs(undefined)` convention). `per_target` is carried from
/// the chosen atom for shape faithfulness (EndDisc is not stacked in the corpus). `None` ⇒ fall back
/// to `bag.endurance_discount()`.
pub fn endurance_discount_value(power: &Power) -> Option<TypedValue> {
    // OVERWRITE fold: the LAST EnduranceDiscount atom, unconditionally (mirrors the converter's
    // gate-free `effects.enduranceDiscount = makeEffect()`).
    let last = super::base_atoms_of_type(power, EffectType::EnduranceDiscount)
        .into_iter()
        .next_back()?;
    Some(TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: last.per_target,
    })
}
