//! ATOM3 stealth-radius reader — the atom-native source of a power's `(pve, pvp, stack_key)`
//! stealth contribution. Migrated from the bag (DATA-GAP-REGISTER PASS2B-16 / ATOM3), unblocked
//! by BRIDGE-2: the bridge now types the two
//! radii `Stealth/RadiusPvE` and `Stealth/RadiusPvP` (and the visual `Stealth/Translucency`),
//! so the axes are separable atom-side. Before BRIDGE-2 all three collapsed to `Stealth`/no-subType
//! and the applier could only read the bag.
//!
//! It returns the SAME [`coh_data::slot_value::Stealth`] shape `Bag::stealth()` returns, with the
//! downstream gather/resolve unchanged.
//!
//! Two converter facts it mirrors (`convert-powerset.cjs`):
//!   * **Overwrite, not accumulate.** `effects.stealth[axis] = makeEffect()` is a plain
//!     assignment, so a power with several templates of one axis keeps the LAST — the reader takes
//!     the last atom of each axis.
//!   * **Absolute value.** `makeEffect` stores `Math.abs(scale)`, so a foe-facing stealth-DEBUFF
//!     (Arctic Air −35) lands as its magnitude — the reader takes `|scale|`.
//!
//! `stack_key` (the binary suppress group) is set from any Stealth-family atom whose stacking is
//! `Suppress` and whose key is resolved (non-empty, not the `0`/`4294967295` sentinels).
//! Translucency contributes only the key, never a radius (the bag drops its value).
//!
//! # Only the `Cur` face is a radius (ATOM-BAG-7)
//!
//! `StealthRadius_PVE`/`_PVP` carries three faces in the export, and until ATOM-BAG-7 this reader
//! and the converter both took `|scale|` off whichever one it found:
//!
//!   * `Cur` — the radius the power grants, in feet. The 116/72/49 rows every stealth power is.
//!   * `Str` — a multiplier on the caster's OWN stealth radius. Its whole population is
//!     Assassin's Strike: `−1` on `Melee_Ones`, `Self`, 8 seconds, `IgnoreStrength` — the
//!     post-strike reveal, one row per shipped variant (HC 1, Rebirth 17, Thunderspy 16). Read as
//!     a radius it inverted into a **+1 ft stealth buff on every Stalker who picked an Assassin's
//!     Strike**, on both axes. Mids types the same rows `Enhancement`/`aspect Str` — a strength
//!     modifier, not a stealth effect — and there is no stealth-strength channel here (`Stealth`
//!     is not a [`crate::strength::special_key`]), so the rows are carried and unmodelled.
//!   * `Max` — a CAP on stealth radius. Outside the player corpus (Warburg's negative-stealth
//!     temp sets it to 0); no cap channel either.
//!
//! The face set is pinned in both directions, so a fourth face arrives as
//! a red gate rather than as a silently dropped or silently inverted row.

use super::{is_gated, table_of};
use coh_data::slot_value::{Scaled, Stealth};
use coh_data::{Aspect, AtomicEffect, EffectType, Power, Stacking, SubType};

/// The bag's empty/sentinel-normalized suppress key: an unresolved key (`""`, `"0"`,
/// `"4294967295"`) means "no group" (`convert-powerset.cjs:5703`, mirrored in `Bag::stealth`).
fn resolved_key(a: &AtomicEffect) -> Option<&str> {
    let key = a.stack_key.as_deref()?;
    if key.is_empty() || key == "0" || key == "4294967295" {
        return None;
    }
    Some(key)
}

/// The atom-native stealth contribution for one power.
///
/// `None` when the power carries no stealth-radius atom on the `Cur` face — a translucency-only
/// power, or one whose only stealth rows are the `Str`-face reveal (§ the module doc).
pub fn stealth_contribution(power: &Power) -> Option<Stealth> {
    // Non-gated Stealth-family atoms on the radius face, in template order (`baseAtomsOfType`).
    // Order is load-bearing: the converter's last-write-wins keeps the LAST atom of each axis.
    let atoms: Vec<&AtomicEffect> = power
        .atoms
        .iter()
        .filter(|a| {
            a.effect_type == Some(EffectType::Stealth)
                && a.aspect == Some(Aspect::Cur)
                && !is_gated(a)
        })
        .collect();

    // The last atom of one axis wins; its value is `|scale|` on the atom's own table.
    let radius = |axis: SubType| -> Option<Scaled> {
        atoms
            .iter()
            .rfind(|a| a.sub_type == Some(axis))
            .and_then(|a| {
                a.scale.map(|scale| Scaled {
                    scale: scale.abs(),
                    table: table_of(a).map(str::to_string),
                    per_target: None,
                })
            })
    };
    let pve = radius(SubType::RadiusPvE);
    let pvp = radius(SubType::RadiusPvP);
    if pve.is_none() && pvp.is_none() {
        return None; // translucency-only / no radius → the bag has none either
    }

    // stack_key: the last Suppress-stacking Stealth atom that carries a resolved key.
    let stack_key = atoms
        .iter()
        .filter(|a| a.stacking == Some(Stacking::Suppress))
        .filter_map(|a| resolved_key(a).map(String::from))
        .next_back();

    Some(Stealth {
        stack_key,
        pve,
        pvp,
    })
}
