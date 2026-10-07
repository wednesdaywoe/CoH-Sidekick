//! `defenseBuffValue` / `defenseBuffSuppressibleValue`, ported from atom-query.ts.
//! Defense exercises two axes resistance doesn't: last-write-wins on colliding
//! same-type base buffs (Rebirth Hide), and gated `firstTargetExcluded` per-target
//! increments (Phalanx Fighting) that fold into the base slot without adding to N=1.

use super::*;
use coh_data::{reaches_caster, EffectType, Power, SubType};

/// The eleven standard defense globals: three positions + eight damage types.
pub(crate) const DEFENSE_STD_SUBTYPES: [SubType; 11] = [
    SubType::Melee,
    SubType::Ranged,
    SubType::AoE,
    SubType::Smashing,
    SubType::Lethal,
    SubType::Fire,
    SubType::Cold,
    SubType::Energy,
    SubType::Negative,
    SubType::Toxic,
    SubType::Psionic,
];

fn is_std(a: &coh_data::AtomicEffect) -> bool {
    a.sub_type
        .is_some_and(|s| DEFENSE_STD_SUBTYPES.contains(&s))
}

/// The `All` arm of the buff filter: a `Base_Defense` row at the attribute's live
/// value. The aspect is checked HERE, where the typed arm never needs to, because
/// `All` is the one defense subType whose other faces exist in bulk: `Defense/All`
/// at `Res` is defense-debuff-resistance (BRIDGE-1's reclassification — 229
/// Homecoming atoms), and at `Str` it is defense strength (Adrenal Booster).
/// Reading either as a defense value would ship Wolf Spider Armor's 0.3 as +30%
/// defense — DEFALL-1's census is the warrant that `Cur` alone is the buff face.
fn is_all_cur(a: &coh_data::AtomicEffect) -> bool {
    a.sub_type == Some(SubType::All) && a.aspect == Some(coh_data::Aspect::Cur)
}

/// One defense type's `{ scale, table, perTarget? }` (mirrors `defensePerTypeValue`).
fn defense_per_type_value(group: &[&coh_data::AtomicEffect], power: &Power) -> Option<TypedValue> {
    let base: Vec<_> = group.iter().copied().filter(|a| !is_gated(a)).collect();
    // A gated atom carrying a per-foe increment is Phalanx Fighting's: an untoggleable gate
    // (`target != source`) the conditional extractor never surfaces, so the converter's base
    // per-foe pass reads it and the bag's slot holds its increment. An atom the extractor DID
    // surface belongs to its `conditionalEffects` entry, where the extractor recomputes the
    // per-foe scaling in the entry's own scope — crediting it to the base is Evolving Armor
    // granting its Defensive-stance +Def in every stance. The stamp only reaches such an atom
    // since PERFOE-1's conditional half, which is why this clause could be written without the
    // `conditional_id` term and stay green.
    let gated_incr: Vec<_> = group
        .iter()
        .copied()
        .filter(|a| is_gated(a) && truthy_per_target(a) && a.conditional_id.is_none())
        .collect();
    if base.is_empty() && gated_incr.is_empty() {
        return None;
    }
    let base_incr: Vec<_> = base
        .iter()
        .copied()
        .filter(|a| truthy_per_target(a))
        .collect();
    let increments: Vec<_> = base_incr
        .iter()
        .copied()
        .chain(gated_incr.iter().copied())
        .collect();
    let table = preferred_table(&base, group).unwrap_or("");
    if !increments.is_empty() {
        let per_target = sum_distinct_abs(increments.iter(), |a| a.per_target.or(Some(0.0)));
        let bases: Vec<_> = base
            .iter()
            .copied()
            .filter(|a| !truthy_per_target(a))
            .collect();
        let self_incr: Vec<_> = base_incr
            .iter()
            .copied()
            .filter(|a| coh_data::reaches_caster(a, power))
            .collect();
        let scale = sum_distinct_abs(bases.iter(), |a| a.scale)
            + sum_distinct_abs(self_incr.iter(), |a| a.scale);
        return Some(TypedValue {
            scale,
            table: Some(Box::from(table)),
            per_target: Some(per_target),
        });
    }
    // No per-target increment → last-write-wins (the bag's direct slot assignment).
    let last = base[base.len() - 1];
    Some(TypedValue {
        scale: last.scale.map_or(f64::NAN, f64::abs),
        table: last.modifier_table.clone(),
        per_target: None,
    })
}

/// One half of the combat-suppression axis, per type. The group is drawn from ALL
/// atoms of the type (not just base) so Phalanx's gated increment is recoverable;
/// `defense_per_type_value` re-filters `gated` itself.
///
/// [`excludes_caster`] drops the rows the power hands to everyone but the caster
/// (Grant Cover's team defense). Phalanx Fighting carries the same `target ≠ source`
/// clause and survives, because its rows are aimed at `Self` — see that function.
///
/// A `Cur`-aspect `All` group ([`is_all_cur`]) lands on all eleven keys through
/// [`defense_total_keys`]. Its only corpus population is Personal Force Field on the
/// two Parse6 forks; the seven PvP-gated typed siblings those powers also carry are
/// `gated` with no per-target fold, so their groups reconstruct to nothing and the
/// `All` expansion cannot collide with a typed key today. If a fork ever ships a
/// power carrying both in one suppression half, the two entries co-exist in the
/// returned list and the apply site sums them — which is the game's arithmetic
/// (`Base_Defense` plus the typed row), not a collision.
fn defense_buff_by_type(
    power: &Power,
    want_suppressible: bool,
) -> Option<Vec<(String, TypedValue)>> {
    let atoms: Vec<_> = atoms_of_type(power, EffectType::Defense)
        .into_iter()
        .filter(|a| {
            (is_std(a) || is_all_cur(a))
                && !is_debuff_atom(a)
                && !excludes_caster(a)
                && (a.suppressible == Some(true)) == want_suppressible
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (sub, group) in by_sub_type(&atoms) {
        if let Some(v) = defense_per_type_value(&group, power) {
            // A reconstruction of exactly 0 with no per-target growth is not a real
            // buff and the bag surfaces nothing for it (scale-0 placeholder class).
            let pt_truthy = v.per_target.is_some_and(|x| x != 0.0);
            if v.scale != 0.0 || pt_truthy {
                for key in defense_total_keys(sub) {
                    out.push((key, v.clone()));
                }
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// The total keys one defense subType lands on. Eleven for `All` — the game's
/// `Base_Defense`, which is not a twelfth position but the same value on every one of
/// them. Homecoming's own data is the oracle for that reading: it states Personal Force
/// Field as eleven typed rows where the two Parse6 forks state the identical scale once
/// as `Base_Defense` — same power, same 7.5, same table, and the same description on both.
///
/// Both sides route through this: the debuff side for the two accolades' −0.1
/// `Base_Defense`, and the buff side for Personal Force Field, whose 7.5 the two
/// Parse6 forks state once as `Base_Defense` (DEFALL-1). The buff side admits `All`
/// only at `aspect: Cur` — see [`is_all_cur`] for why the other faces must stay out.
fn defense_total_keys(sub: Option<SubType>) -> Vec<String> {
    match sub {
        Some(SubType::All) => DEFENSE_STD_SUBTYPES
            .iter()
            .map(|s| s.as_wire().to_lowercase())
            .collect(),
        other => vec![other
            .map(|s| s.as_wire().to_lowercase())
            .unwrap_or_default()],
    }
}

/// Does this atom apply when the power fires, or is it a later phase of the same cast?
///
/// `delay` is the only thing that separates a power's own effect from its CRASH, and the
/// two are otherwise indistinguishable on every axis a consumer keys on: Rage states its
/// +damage and +ToHit for 120 seconds at delay 0 and its −20% defense for 10 seconds at
/// `Delay 120`, all `Self`, `IgnoreStrength` rows of one power. The totals dashboard reports the
/// sustained state, so the crash is not part of it — the same judgement
/// [`crate::appliers::damage::self_damage_debuff_value`] makes from a co-present buff, made
/// here from the datum instead.
///
/// The rule is any delay at all, and that is safe only because the family is narrow: half
/// the corpus's delayed templates are sub-second animation timing (7.5k of 14.7k), but no
/// self-directed defense debuff is. The two populations here are 0 and 30–120 seconds with
/// nothing in between, and
/// the corpus gate fails if a row ever lands between
/// them — a 0.25s-delayed self debuff wants a decision, not this rule applied to it.
fn starts_with_the_cast(a: &coh_data::AtomicEffect) -> bool {
    a.delay.is_none_or(|d| d <= 0.0)
}

/// The caster's own −defense penalty, keyed by lowercase defense type: the sibling
/// `resistance_self_debuff_value` has had since the migration and defense had not, so a
/// power that negates its own caster's defense reached no total (DEFDEBUFF-1).
///
/// The whole routed population is small and worth naming, because each member tests a
/// different clause: Thunderspy's Organic Armor states `Defense −500 × Melee_Buff_Def` on
/// all seven typed slots while Defensive Adaptation is up (mode-gated, so it arrives here
/// only through [`crate::gather::active_conditional_powers`], and saturating, so it lands
/// on the class floor ATTRMIN-1 exports rather than on a number); its DNA Siphon states
/// −0.5 on the same slots under the other stance; and the two accolades state −0.1 as
/// `Base_Defense`, which is why `All` routes to eleven keys here. Rage's −0.2 is NOT in it
/// — that one is the crash, see [`starts_with_the_cast`].
///
/// `aspect: Cur` is the attribute's live value. The `Res` face of the same attribute is
/// defense-debuff RESISTANCE, and it carries debuff rows of its own — 28 Homecoming atoms
/// state `Melee_Debuff_Res_Dmg` on `Defense/Res`, which lowers what the TARGET resists
/// −Def with, not their defense. None of them reaches a caster today, so this clause is
/// insurance rather than a live filter; it is here because the two faces are one attrib
/// name apart and the skill's first rule is not to discriminate by that name.
///
/// Value is `|scale|` per key, last-write-wins within a subType (the shape
/// `resistance_self_debuff_value` established); the apply site negates it. Unenhanced:
/// every member of the population is `IgnoreStrength`, and no enhancement makes a
/// self-penalty smaller.
pub fn defense_self_debuff_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    let atoms: Vec<_> = base_atoms_of_type(power, EffectType::Defense)
        .into_iter()
        .filter(|a| {
            a.aspect == Some(coh_data::Aspect::Cur)
                && is_debuff_atom(a)
                && starts_with_the_cast(a)
                && a.not_on_caster != Some(true)
                && reaches_caster(a, power)
        })
        .collect();
    if atoms.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for (sub, group) in by_sub_type(&atoms) {
        let last = group[group.len() - 1];
        for key in defense_total_keys(sub) {
            out.push((
                key,
                TypedValue {
                    scale: last.scale.map_or(f64::NAN, f64::abs),
                    table: last.modifier_table.clone(),
                    per_target: None,
                },
            ));
        }
    }
    Some(out)
}

/// The always-on per-type +defense buff (`effects.defenseBuff`).
pub fn defense_buff_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    defense_buff_by_type(power, false)
}

/// The combat-suppressed per-type +defense buff (`effects.defenseBuffSuppressible` —
/// Hide, Stealth, Cloaking Device), applied only out of combat.
pub fn defense_buff_suppressible_value(power: &Power) -> Option<Vec<(String, TypedValue)>> {
    defense_buff_by_type(power, true)
}
