//! Perception applier — the atom-native `+Perception` radius buff. Migrated from the bag
//! (ATOM5 / DATA-GAP-REGISTER PASS2B-6): the typed `Perception` atoms already ship (HC 174 /
//! Reb 133), and the beta reads only `effects.perceptionBuff` (character-totals.ts:1971) — it has
//! no `perceptionBuffValue` atom reader — so this is the rebuild reading atoms where the frozen
//! oracle reads the bag. It stays behavior-preserving because the atom value equals the bag value
//! exactly for the whole corpus (census: HC 25/25 · Reb 20/20, zero phantoms, zero bag-only — the
//! `atom_confirmation` perception arm and the `perception_atom_bag_parity` guard both pin it). The
//! bag stays the `?? bag` fallback for the atom-less residue: all Thunderspy (TSPY-3 `Unmapped`,
//! zero `Perception` atoms) and any HC/Reb power with no `Perception` atom.
//!
//! Mirrors the converter's `perceptionBuff` gate on the atom side. The converter routes the
//! `perceptionradius` attrib (convert-powerset.cjs:4677) three ways: `aspect=resistance` →
//! `debuffResistance.perception` (ATOM13); `isDebuff || scale < 0` → `perceptionDebuff` (M4);
//! else → `effects.perceptionBuff = makeEffect()` (this). So the buff face is `aspect != Res` +
//! `!isDebuff`, and the fold is OVERWRITE / last-write-wins (a plain `= makeEffect()` assignment,
//! `Math.abs` on scale): the reader takes the LAST surviving atom, not a sum (stealth's family,
//! not recharge's SUM or mez-resist's ACCUMULATE — always read the converter's per-slot write).
//!
//! The gate is the converter-exact `aspect != Res`, NOT a tighter `== Cur`. The exact mirror
//! recognizes precisely the atoms the converter routes here, so a real buff can never be silently
//! missed — its only failure mode is the LOUD one: a mistyped atom without a bag entry becomes a
//! phantom → the `perception_atom_bag_parity` guard goes RED. Unlike ATOM1's `== Str` (a remedy for
//! 7 Thunderspy recharge phantoms from mis-decoded `RechargeTime` atoms), perception needs no such
//! remedy — Thunderspy carries ZERO typed `Perception` atoms, so `!= Res` is phantom-free on all
//! three datasets (census). The rule that covers both: use the converter-exact mirror unless it
//! fabricates phantoms, then tighten as a remedy.

use super::{base_atoms_of_type, is_debuff_atom, TypedValue};
use coh_data::{Aspect, EffectType, Power};

/// `perceptionBuff` — the +Perception radius buff (Tactics, Focused Accuracy, Clear Mind,
/// +Perception auras). Resolved `scale × 100` by the caller (AT-table resolved — a scale-direct
/// `*_Ones` table or a real `*_Res_Boolean` table), gated `> 0`, accumulated into
/// `perception_radius`; NOT enhanced and NOT stacked. OVERWRITE fold: the LAST non-`Res` non-debuff
/// `Perception` atom (the converter's `= makeEffect()`), `|scale|` mirroring `makeEffect`'s
/// `Math.abs` (a no-op for the positive buff face, kept for faithfulness). An absent scale becomes
/// NaN, not a silent zero (the mod-level `Math.abs(undefined)` convention). `per_target` is carried
/// from the chosen atom for shape faithfulness even though the current downstream ignores it
/// (perception is not stacked). `None` ⇒ fall back to `bag.perception_buff()`.
pub fn perception_buff_value(power: &Power) -> Option<TypedValue> {
    // OVERWRITE fold: the LAST non-`Res` (not the DDR face) non-debuff (not the M4 face) atom.
    let last = base_atoms_of_type(power, EffectType::Perception)
        .into_iter()
        .rfind(|a| a.aspect != Some(Aspect::Res) && !is_debuff_atom(a))?;
    Some(TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: last.per_target,
    })
}
