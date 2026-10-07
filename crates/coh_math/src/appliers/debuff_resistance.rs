//! `debuffResistanceValue` — per-type debuff RESISTANCE, migrated from the bag (ATOM13 /
//! DATA-GAP-REGISTER PASS2B-11). Each debuff-resistance type is a `<Attr> aspect=Res` atom
//! (the resisted attribute's own EffectType, resistance face); the converter routes it to
//! `effects.debuffResistance.<type> = makeEffect()` (OVERWRITE / last-write-wins). The frozen
//! oracle reads only the bag, so this is the rebuild reading atoms where the oracle reads the
//! bag — behavior-preserving because the atom (|scale|, table) equals the bag value exactly for
//! every routed type across the corpus (measured 0 bag-only / 0 phantom / 0 divergence on HC +
//! Rebirth; see the ATOM13 design spec and the `debuff_resistance_atom_bag_parity` guard). The
//! bag stays the `?? bag` fallback for the atom-less residue: **all** of Thunderspy (TSPY-3
//! `Unmapped`, zero aspect=Res debuff-resistance atoms).
//!
//! All TEN types `GlobalBonuses::add_debuff_resistance` routes are read. `accuracy` and `range`
//! joined the other eight with DEBUFFRES-1; before that the router dropped them, so reading them
//! here would have fed nothing.
//!
//! Per-type notes the census surfaced:
//! * **movement** collapses the sub-axes: `Movement/Run|Fly|Jump|JumpHeight aspect=Res` all route
//!   to the single `debuffResistance.movement` slot (converter last-write-wins), so the reader
//!   takes the LAST `Movement aspect=Res` atom regardless of subType.
//! * **endurance** is `Endurance aspect=Res`, NOT `EnduranceDiscount` — the endurance discount is
//!   a separate attrib/slot (ATOM7). A power can carry an `EnduranceDiscount aspect=Res` atom
//!   alongside; it must not feed endurance-DDR.
//! * **defense** is the ATOM14-bridged `Defense/All aspect=Res` (the `Base_Defense@Resistance`
//!   template); positional defense-DDR would also land here (last-write-wins), matching the
//!   converter's `debuffResistance.defense`.

use super::*;
use coh_data::{Aspect, EffectType, Power};

/// bag debuffResistance key → the EffectType whose `aspect=Res` atom encodes it, in the ten
/// `add_debuff_resistance` routes. Every atom shares `aspect=Res`; the resisted attribute's own
/// EffectType is the discriminator (movement's Run/Fly/Jump/JumpHeight subtypes all collapse
/// here), so no per-key aspect/subType gate beyond the EffectType is needed.
const ROUTED: [(&str, EffectType); 10] = [
    ("defense", EffectType::Defense),
    ("endurance", EffectType::Endurance),
    ("movement", EffectType::Movement),
    ("perception", EffectType::Perception),
    ("recharge", EffectType::RechargeTime),
    ("recovery", EffectType::Recovery),
    ("regeneration", EffectType::Regeneration),
    ("tohit", EffectType::ToHit),
    ("accuracy", EffectType::Accuracy),
    ("range", EffectType::Range),
];

/// Is this atom one of the ten debuff-resistance routes? The membership half of [`ROUTED`],
/// used by [`crate::stacking::StackFamily`] to keep its three variants a partition of the atom
/// stream rather than three overlapping claims on it.
///
/// `aspect: Res` alone is NOT the test, and that is the whole reason this exists: `Resistance`
/// is absent from `ROUTED` because a `Resistance|Res` atom is ordinary damage resistance, the
/// buff face of its own type. Treating every Res atom as debuff-resistance unstacked the
/// resistance half of Healing Flames, Reconstruction, Light Form and the Kuji-In pair.
pub(crate) fn is_debuff_resistance_atom(atom: &coh_data::AtomicEffect) -> bool {
    atom.aspect == Some(Aspect::Res)
        && ROUTED
            .iter()
            .any(|(_, effect_type)| atom.effect_type == Some(*effect_type))
}

/// The per-type debuff RESISTANCE this power contributes (`effects.debuffResistance`), keyed by
/// the lowercase bag type. `None` when no routed `<Attr> aspect=Res` atom exists (→ the bag
/// fallback). Value is `|scale|` on the atom's table (the converter's `makeEffect` = `Math.abs`),
/// no per-target (`makeEffect` drops it — a debuff-resistance is never a per-foe increment).
pub fn debuff_resistance_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    let mut out = Vec::new();
    for (key, effect_type) in ROUTED {
        // OVERWRITE fold (converter `effects.debuffResistance.<type> = makeEffect()`): the LAST
        // non-gated atom of this EffectType with the resistance face.
        let Some(atom) = base_atoms_of_type(power, effect_type)
            .into_iter()
            .rfind(|a| a.aspect == Some(Aspect::Res))
        else {
            continue;
        };
        let Some(scale) = atom.scale else { continue };
        out.push((
            key.to_string(),
            TypedValue {
                scale: scale.abs(),
                table: table_of(atom).map(Box::from),
                per_target: None,
            },
        ));
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
