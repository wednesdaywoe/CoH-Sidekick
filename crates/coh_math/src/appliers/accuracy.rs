//! Accuracy applier — the atom-native `+Accuracy` buff (Focused Accuracy, Targeting Drone, Eagle
//! Eye, Combat Training: Offensive). Migrated from the bag (ATOM9 / DATA-GAP-REGISTER PASS2B-5): the
//! typed `Accuracy` atoms already ship, and the beta reads only `effects.accuracyBuff`
//! (character-totals.ts:1179) — it has no `accuracyBuffValue` atom reader — so this is the rebuild
//! reading atoms where the frozen oracle reads the bag. It stays behavior-preserving because the atom
//! value equals the bag value exactly for the whole corpus (census: HC 7/7 · Reb 4/4 · **tspy 2/2**,
//! zero phantoms, zero bag-only — the `atom_confirmation` accuracy arm and the
//! `accuracy_atom_bag_parity` guard both pin it).
//!
//! **Unlike ATOM5-8, Thunderspy carries real `Accuracy` atoms** (Conditioning, `aspect=Unspecified`),
//! so this is the first family where tspy is atom-fed, not a TSPY-3 `?? bag` residual. The bag stays
//! the fallback only for a power with no buff-face `Accuracy` atom (any residue on any dataset).
//!
//! Mirrors the converter's `accuracyBuff` gate on the atom side. The converter routes the `accuracy`
//! combat-modifier attrib (convert-powerset.cjs:7037): `aspect === 'resistance'` →
//! `debuffAccuracy`/`debuffResistance.accuracy` (ATOM13); `isDebuff || scale < 0` → `accuracyDebuff`
//! (M4); else the caster buff `effects.accuracyBuff = makeEffect()`. Accuracy is a STRENGTH-aspect
//! stat by nature (convert-powerset.cjs:7562) and the converter's `specialBuff` block explicitly
//! excludes it there so it falls through here — so the buff face is `aspect=Str` in most of the
//! NOT all: Rebirth's Whirlwind carries an `aspect=Cur` accuracy buff. The gate is therefore the
//! converter-exact `aspect != Res` + `!isDebuff`, NOT a tighter `aspect == Str`: tightening would drop
//! Whirlwind (`Cur`) to `?? bag` (the Rule 1 smell), and the `aspect != Res` guard is what keeps a
//! `debuffResistance.accuracy` DDR atom (`aspect=Res`, positive scale on a `Res_Boolean` table, so NOT
//! `is_debuff`) from phantoming into the buff. Fold = OVERWRITE / last-write-wins (`= makeEffect()`,
//! `Math.abs` on scale) — the range/perception/stealth family, not recharge's SUM.
//!
//! The consumer's `stack()` (`adjustForStacking`) and `× 100` stay at the call site, applied
//! identically to whichever source produced the value.

use super::{is_debuff_atom, TypedValue};
use coh_data::{Aspect, EffectType, Power};

/// `accuracyBuff` — the `+Accuracy` buff. Resolved `scale × 100` by the caller (AT-table resolved,
/// via the `adjustForStacking` seam), accumulated into `accuracy`; NOT enhanced (accuracy
/// enhancements boost the attack roll, not a +Accuracy buff power). OVERWRITE fold: the LAST non-`Res`
/// non-debuff `Accuracy` atom, `|scale|` mirroring `makeEffect`'s `Math.abs`. An absent scale becomes
/// NaN, not a silent zero. `per_target` is carried from the chosen atom for shape faithfulness.
/// `None` ⇒ fall back to `bag.accuracy_buff()`.
pub fn accuracy_buff_value(power: &Power) -> Option<TypedValue> {
    let last = super::base_atoms_of_type(power, EffectType::Accuracy)
        .into_iter()
        .rfind(|a| a.aspect != Some(Aspect::Res) && !is_debuff_atom(a))?;
    Some(TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: last.per_target,
    })
}
