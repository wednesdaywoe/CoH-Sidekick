//! `mezResistanceValue` — per-type mez RESISTANCE (duration reduction), migrated from the bag
//! (ATOM2 / DATA-GAP-REGISTER PASS2B-13). The typed `MezResist` atoms already ship (HC 542 /
//! Reb 504, **all `aspect=Res`**), and the beta reads only the bag, so this is the rebuild
//! reading atoms where the frozen oracle reads the bag. It stays behavior-preserving because the
//! atom fold equals the bag value exactly for the whole corpus (measured: every routed type
//! matched, 0 phantoms, 0 divergences on HC + Rebirth — see the ATOM2 design spec). The bag stays
//! the `?? bag` fallback for the atom-less residue: **all** of Thunderspy (TSPY-3 `Unmapped`).
//!
//! The reader mirrors the converter's `mezResistance` routing (`convert-powerset.cjs:6675`)
//! on the atom side, and — crucially — the converter's **two different folds**:
//!
//! * **MEZ types** (hold/stun/sleep/immobilize/confuse/fear): the converter ACCUMULATES
//!   (`mezResistance[type].scale += |scale|` per same table), so the value is `Σ |scale|`. No
//!   `toWho` gate (the converter's MEZ path has none; a foe-facing hold rides a `Mez` atom, not
//!   `MezResist`, so none phantom).
//! * **KB types** (knockback/knockup/repel): the converter OVERWRITES (`= makeEffect()`,
//!   last-write-wins), so the value is the single `|scale|` (measured uniform per power, so `max`
//!   == last-write-wins). Only self-facing, non-`Res_Boolean` KB is mez RESISTANCE — a foe-facing
//!   KB-res on a control power the converter drops (`!isSelfTargeting`), and a self-facing
//!   `Res_Boolean` KB-res rides the mez-PROTECTION path (`effects[kb]`), not `mezResistance`.
//!
//! `add_mez_resistance` (the caller) routes hold/stun/immobilize/sleep/confuse/fear/**knockback**
//! and **repel**, plus **taunt/placate** and **teleport**, and declares knockup unspent — this
//! reader still emits knockup (faithful mirror; the decision happens once, at the router).
//!
//! **Teleport is the one key filtered HERE rather than at the router** (MEZRES-3), because only
//! here is the atom's target still visible. About half the corpus's `MezResist`/`Teleport` atoms
//! protect somebody else — Wormhole and Shadow Slip author one beside the `Mez/Teleport` that
//! moved the foe, so it is the victim's post-yank immunity at scale 100, and Increase Density's is
//! the ally's. Emitting the key and vetoing at the router is not open to us: the router sees a
//! lowercase string, not an atom. Nor is the KB branch's `toWho == Self` filter enough — Static
//! Shield authors `Target kTarget` on a power whose `EntsAffected` is `kCaster`, where "target" IS
//! the caster, so that filter would drop a real 100%. [`teleport_protects_caster`] carries the
//! rule and the two shapes that rule out anything simpler.
//!
//! Taunt/placate RESISTANCE has TWO disjoint encodings in the export: the `effects.taunt|placate`
//! slot (an `aspect != Res` `Mez` atom — the ATOM12 family, read by
//! [`super::taunt_placate::taunt_placate_value`]) and this `mezResistance` slot (an `aspect=Res`
//! `MezResist` atom). No corpus power carries both — measured across all three datasets and
//! pinned by the route sweep — so the two paths sum without double-counting.

use super::*;
use coh_data::{reaches_caster, Aspect, AtomicEffect, EffectType, Power, SubType};

/// subType → the lowercase mez-type key `GlobalBonuses::add_mez_resistance` routes by. The six
/// MEZ types, the three KB types, and the taunt/placate pair; every other subType has no key
/// and is excluded.
///
/// Taunt and Placate are here because this slot is the ONLY encoding some powers use for them
/// (MEZRES-2). Excluding them here dropped the value one step upstream of the router, where the
/// router's own declared-unspent table could not see it — Tactical Training: Vengeance's six mez
/// types were credited while its taunt and placate, same scale and same table, were not.
fn mez_resist_key(sub: SubType) -> Option<&'static str> {
    match sub {
        SubType::Held => Some("hold"),
        SubType::Stunned => Some("stun"),
        SubType::Sleep => Some("sleep"),
        SubType::Confused => Some("confuse"),
        SubType::Terrorized => Some("fear"),
        SubType::Immobilized => Some("immobilize"),
        SubType::Knockback => Some("knockback"),
        SubType::Knockup => Some("knockup"),
        SubType::Repel => Some("repel"),
        SubType::Taunt => Some("taunt"),
        SubType::Placate => Some("placate"),
        SubType::Teleport => Some("teleport"),
        _ => None,
    }
}

/// Does this `MezResist`/`Teleport` atom protect the CASTER, or the entity the power just
/// moved?
///
/// Both are spelled the same way. A power that teleports somebody grants its victim 15
/// seconds of teleport immunity so they cannot be chain-yanked, and that immunity rides a
/// `toWho: Target` atom — the identical shape Static Shield uses for the caster's own
/// protection, because the game writes both as `AnyAffected`, "whoever this power affects".
///
/// [`Power::targets_affected`] is the only field that resolves it, which is why routing this
/// key waited on the contract carrying it. Two shapes prove no simpler rule works:
///
/// * **`targetType` cannot answer it.** Shadow Slip and Fold Space are `targetType: "Self"`
///   PBAoEs that yank foes; their `targets_affected` is `["Foe"]`, which is what excludes them.
/// * **"does the power also teleport somebody" cannot either.** Increase Density moves nobody
///   and still must be excluded — its protection is the ALLY's (`["Friend"]`).
///
/// Measured over every `Teleport`/`Resistance` template in the raw export — 43 Homecoming, 35
/// Rebirth, 30 Thunderspy — this rule returns the right answer for all of them.
///
/// This was the first site to ask the question and it hand-rolled the join. TARGETS-3 gave
/// every site the same predicate, so the rule now lives in [`reaches_caster`] and this is a
/// name for what the teleport key needs it for. The route sweep still re-derives the verdict
/// from the two raw inputs rather than calling either, so a change here reds there.
fn teleport_protects_caster(atom: &AtomicEffect, power: &Power) -> bool {
    reaches_caster(atom, power)
}

/// The knockback family — the converter routes it through the OVERWRITE fold with the
/// self-facing / non-`Res_Boolean` gate, unlike the six accumulate-fold MEZ types.
fn is_knockback(sub: SubType) -> bool {
    matches!(sub, SubType::Knockback | SubType::Knockup | SubType::Repel)
}

fn is_res_boolean(a: &AtomicEffect) -> bool {
    a.modifier_table
        .as_deref()
        .is_some_and(|t| t.to_lowercase().contains("res_boolean"))
}

/// The per-mez-type +resistance this power contributes (`effects.mezResistance`), keyed by the
/// lowercase mez key. `None` when no qualifying `MezResist` atom exists (→ the bag fallback).
pub fn mez_resistance_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    let atoms: Vec<&AtomicEffect> = base_atoms_of_type(power, EffectType::MezResist)
        .into_iter()
        .filter(|a| a.aspect == Some(Aspect::Res))
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (sub, group) in by_sub_type(&atoms) {
        let Some(sub) = sub else { continue };
        let Some(key) = mez_resist_key(sub) else {
            continue;
        };
        let kb = is_knockback(sub);
        // KB is mez RESISTANCE only when self-facing and NOT on a `Res_Boolean` table: a
        // foe-facing KB-res (control powers pinning the target) the converter drops, and a
        // self-facing `Res_Boolean` KB-res rides the mez-PROTECTION path (`effects[kb]`).
        //
        // Teleport takes the converter's no-gate MEZ path for its fold, but needs a target
        // gate of its own that no other key does — see `teleport_protects_caster`.
        let group: Vec<&AtomicEffect> = if kb {
            group
                .into_iter()
                .filter(|a| reaches_caster(a, power) && !is_res_boolean(a))
                .collect()
        } else if sub == SubType::Teleport {
            group
                .into_iter()
                .filter(|a| teleport_protects_caster(a, power))
                .collect()
        } else {
            group
        };
        if group.is_empty() {
            continue;
        }
        let scale = if kb {
            // OVERWRITE fold (converter `= makeEffect()`): the single stored value. KB-res atoms
            // are uniform per power (measured), so `max` == last-write-wins.
            group
                .iter()
                .filter_map(|a| a.scale)
                .map(f64::abs)
                .fold(0.0_f64, f64::max)
        } else {
            // ACCUMULATE fold (converter `+= |scale|` per same table): `Σ |scale|`. The corpus is
            // single-table per type, so the cross-table last-wins subtlety never bites.
            group.iter().filter_map(|a| a.scale).map(f64::abs).sum()
        };
        let table = group.iter().find_map(|a| table_of(a)).map(Box::from);
        out.push((
            key.to_string(),
            TypedValue {
                scale,
                table,
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
