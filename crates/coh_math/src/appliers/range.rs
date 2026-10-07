//! Range applier — the atom-native `+Range` buff. Migrated from the bag (ATOM6 /
//! DATA-GAP-REGISTER PASS2B-8): the typed `Range` atoms already ship, and the beta reads only
//! `effects.rangeBuff` (character-totals.ts:1989) — it has no `rangeBuffValue` atom reader — so
//! this is the rebuild reading atoms where the frozen oracle reads the bag. It stays
//! behavior-preserving because the atom value equals the bag value exactly for the whole corpus
//! (census: HC 13/13 · Reb 1/1, zero phantoms, zero bag-only — the `atom_confirmation` range arm
//! and the `range_atom_bag_parity` guard both pin it). The bag stays the `?? bag` fallback for the
//! atom-less residue: all Thunderspy (TSPY-3 `Unmapped`, zero `Range` atoms) and any HC/Reb power
//! with no buff-face `Range` atom.
//!
//! Mirrors the converter's `rangeBuff` routing on the atom side. The converter routes the `range`
//! attrib (convert-powerset.cjs:4646) four ways: `aspect=resistance` → `debuffResistance.range`
//! (ATOM13); `isDebuff || scale < 0` → the self `rangeDebuff` (M4) or a dropped foe-side debuff;
//! else the caster buff `effects.rangeBuff = makeEffect()`. The fold is OVERWRITE /
//! last-write-wins (a plain `= makeEffect()` assignment, `Math.abs` on scale): the reader takes
//! the LAST surviving atom, not a sum (stealth/perception's family, not recharge's SUM).
//!
//! The converter writes that buff on two sub-branches — a self-targeting atom, OR an
//! `aspect=strength` ally/team `+Range` (Power of the Depths, `toWho == Target`) — and this
//! reader mirrored both until TARGETS-3. The second branch was standing in for a question about
//! the POWER: Power of the Depths is `["Friend", "Self"]`, so its team buff reaches the caster,
//! and the aspect had nothing to do with it. `reaches_caster` asks directly, so an identical
//! `aspect=Str` team buff on a power that leaves the caster out now reads `None` where the old
//! disjunct credited it.
//!
//! The consumer's `> 0` and `is_self(power)` gates are SEPARATE and stay at the call site: the
//! `is_self` gate reads the POWER's `targetType` (not the atom's `toWho`) and is applied
//! identically to whichever source — atom or bag — produced the value, so it does not affect the
//! atom == bag slot equivalence this reader rests on. It is load-bearing at consumption: the same
//! `rangeBuff` slot on a Foe-targeted snipe (Blazing Bolt, Moonbeam) is the per-power Fast Snipe
//! range bump, not a persistent caster buff, and must not feed the character Range total.

use super::{is_debuff_atom, TypedValue};
use coh_data::{reaches_caster, Aspect, EffectType, Power};

/// `rangeBuff` — the `+Range` buff (Boost Range, Aim's/snipe +Range, team `+Range`). Resolved
/// `scale × 100` by the caller (AT-table resolved), gated `> 0` AND `is_self(power)`, accumulated
/// into `range`; NOT enhanced. OVERWRITE fold: the LAST non-`Res` non-debuff atom that reaches
/// the caster (the converter's `= makeEffect()`), `|scale|` mirroring `makeEffect`'s
/// `Math.abs`. An absent scale becomes NaN, not a silent zero (the mod-level
/// `Math.abs(undefined)` convention). `per_target` is carried from the chosen atom for shape
/// faithfulness (range is not stacked in the corpus, but the caller's `adjustForStacking` seam
/// reads it). `None` ⇒ fall back to `bag.range_buff()`.
///
/// The recipient test used to be `toWho == Self` OR `aspect == Str`, and the second arm was a
/// stand-in for the question [`reaches_caster`] now answers: a team `+Range` buff is stated
/// `AnyAffected`, and the only thing separating the copy that reaches the caster from the one
/// that does not is the power's own targets (TARGETS-3). Reading the aspect instead credited
/// every team buff and no `aspect=Cur` one.
pub fn range_buff_value(power: &Power) -> Option<TypedValue> {
    // OVERWRITE fold: the LAST buff-face atom — not the DDR face (`aspect == Res`), not the M4
    // debuff face (`is_debuff_atom`), and one the caster is a recipient of.
    let last = super::base_atoms_of_type(power, EffectType::Range)
        .into_iter()
        .rfind(|a| {
            a.aspect != Some(Aspect::Res) && !is_debuff_atom(a) && reaches_caster(a, power)
        })?;
    Some(TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: last.per_target,
    })
}
