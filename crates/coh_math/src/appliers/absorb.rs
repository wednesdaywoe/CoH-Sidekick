//! ATOM10 (fraction half) absorb reader — recovers the MaxHP-fraction absorb magnitude
//! atom-native by EVALUATING the `Absorb`/`aspect=Max`/`type=Expression` atom's
//! `magnitude_expression`, replacing the bag's converter-recovered `maxHPFraction`
//! (`parseAbsorbMaxHPFraction`, `convert-powerset.cjs`). This is the first atom reader that
//! consumes the expression VM (`crate::expr`) outside the Pass-3 inherents — the ATOM10
//! residual the register/audit tracked as "blocked on an Expression evaluator."
//!
//! The magnitude is `Max.kHitPoints source> C * [@Strength *]`: `C` fraction of the caster's
//! CURRENT Max HP, optionally scaled by +Absorb strength. Only `C` is atom-static — the max HP
//! and the strength are build/runtime state applied downstream (max HP post-pass in
//! [`crate::lib`] step 9.2, +Absorb strength by the apply-loop `enh_multiplier`), exactly as for
//! the bag-read fraction. So the reader NEUTRALIZES both `Max.kHitPoints` and `@Strength` to
//! unity and returns the bare coefficient `C`.

use super::{base_atoms_of_type, TypedValue};
use crate::expr::{eval, EvalContext, EvalError, Value};
use coh_data::{Aspect, AtomicEffect, AttribType, EffectType, Power, Stacking, ToWho};

/// Extracts the bare fraction from an absorb magnitude expression by neutralizing its two
/// static-shape readers to `1.0`: `Max.kHitPoints source>` (the caster's max HP, multiplied in at
/// step 9.2) and `@Strength` (the +Absorb strength, applied by the apply-loop `enh_multiplier`).
/// Every OTHER reader is Indeterminate — Rule 1, an unrecognized shape yields no number rather
/// than a fabricated one. Master Brawler's `100 kHitPoints% source> - kEndurance% source> + 200 /
/// @StdResult *` reads live HP and endurance and stays unrecovered here as it does in the
/// converter.
///
/// `@StdResult` is the exception, and only on a `_ones` table. The game computes it as
/// `table[level] × effectiveness × scale × strength` (`attribmod.c` `mod_Fill`, stored by
/// `combateval_StoreAttribCalcInfo`), and a `_ones` table is 1.0 at every level — so the level and
/// archetype this atom-static probe does not have DROP OUT, and the fraction is the template's own
/// scale. That is the same guard the converter's `parseAbsorbMaxHPFraction` applies, from the same
/// source reading (PROD6B-2a); on any other table `@StdResult` is not a bare fraction and stays
/// Indeterminate. Sentinel's Ablative Carapace (0.3) and Parasitic Leech (0.143) are the corpus.
struct FractionProbe {
    /// The atom's own scale — `@StdResult`'s value once a `_ones` table has cancelled the level
    /// term and `@Strength` has been neutralized.
    scale: Option<f64>,
    /// Whether the atom's table is a `_ones` table, which is what makes that cancellation sound.
    ones_table: bool,
}

impl EvalContext for FractionProbe {
    fn resolve(&self, reader: &str, operands: &[Value]) -> Result<Value, EvalError> {
        match reader {
            "source>" | "Source>" => match operands.first() {
                Some(Value::Symbol(s)) if &**s == "Max.kHitPoints" => Ok(Value::Number(1.0)),
                _ => Err(EvalError::Indeterminate(reader.into())),
            },
            "@Strength" => Ok(Value::Number(1.0)),
            "@StdResult" => match self.scale.filter(|_| self.ones_table) {
                Some(scale) => Ok(Value::Number(scale)),
                None => Err(EvalError::Indeterminate(reader.into())),
            },
            _ => Err(EvalError::Indeterminate(reader.into())),
        }
    }
}

/// The MaxHP-fraction absorb magnitude read atom-native, or `None` to fall back to the bag.
///
/// Mirrors the converter's `parseAbsorbMaxHPFraction` gate exactly: BASE atoms only (a gated
/// conditional — Bio Armor's Defensive-Adaptation +3.3% — is dropped, so the always-on 10%
/// stands alone, the same size-1 fraction set the converter requires), `Absorb`/`aspect=Max`/
/// `type=Expression`, one distinct fraction. `None` when there is no such base atom, an
/// expression falls outside the two shapes [`FractionProbe`] evaluates, or two base atoms carry
/// distinct fractions (the converter bails to `null` there too → bag).
pub fn absorb_max_hp_fraction_value(power: &Power) -> Option<f64> {
    let mut fraction: Option<f64> = None;
    for atom in base_atoms_of_type(power, EffectType::Absorb) {
        if !is_ceiling(atom) {
            continue;
        }
        // An unevaluable ceiling bails the WHOLE read rather than being skipped: a power whose
        // fraction the probe cannot recover has no fraction, and answering with a sibling's
        // would be a number this reader made up.
        let value = fraction_of(atom)?;
        match fraction {
            None => fraction = Some(value),
            Some(seen) if (seen - value).abs() < 1e-9 => {}
            Some(_) => return None,
        }
    }
    fraction
}

/// Is this the CEILING half — the `Absorb`/`aspect=Max`/`type=Expression` row whose magnitude
/// states the shield as a fraction of max HP? See [`states_max_hp_fraction`] for the pair.
fn is_ceiling(atom: &AtomicEffect) -> bool {
    atom.aspect == Some(Aspect::Max)
        && atom.attrib_type == Some(AttribType::Expression)
        && atom.not_on_caster != Some(true)
}

/// One ceiling atom's fraction, or `None` if it is not a ceiling or its expression falls outside
/// the shapes [`FractionProbe`] evaluates.
fn fraction_of(atom: &AtomicEffect) -> Option<f64> {
    if !is_ceiling(atom) {
        return None;
    }
    let probe = FractionProbe {
        scale: atom.scale,
        ones_table: states_max_hp_fraction(super::table_of(atom)),
    };
    match eval(atom.magnitude_expression.as_deref()?, &probe) {
        Ok(Value::Number(n)) => Some(n),
        _ => None,
    }
}

/// Does an absorb shield on this table state a FRACTION of the caster's Max HP, rather than
/// absolute HP off an AT table? A `_ones` table is the tell, and this is why.
///
/// **The game authors a MaxHP-fraction shield as a PAIR of AttribMods that are one shield.**
/// Thunderspy's Ablative Carapace is the full form:
///
/// ```text
/// Absorb  aspect Cur  type Magnitude   scale 0.3  Melee_Ones  duration 0
/// Absorb  aspect Max  type Expression  scale 1.0  Melee_Ones  duration 30
///         magnitude "Max.kHitPoints source> 0.3 * @Strength *"
/// ```
///
/// The `Max` row raises the absorb CEILING to 30% of the caster's max HP for thirty seconds; the
/// `Cur` row instantly FILLS it with the same 30%. They agree on every other field — table,
/// target, gate, stacking, `ignoreStrength` — and the game gives them two display messages for
/// the one event ("points of absorption" beside "points of absorption over time"). The bag fused
/// them into one slot because a slot has room for one number, and that fusion was right.
///
/// **The data proves the halves are not additive, from within one fork.** Homecoming authors
/// Parasitic Aura WITH the fill on Scrapper, Stalker and Tanker and WITHOUT it on Brute
/// (`raw defs/*/Bio_Organic_Armor/Parasitic_Aura.powers`), and Ablative Carapace without it on
/// every archetype. A Brute's Parasitic Aura is not a shield-less Parasitic Aura, so the fill
/// cannot be a second 10% — it is the same 10% stated a second way, and an archetype whose def
/// omits it gets the shield from the ceiling alone.
///
/// So the routing counts the pair ONCE: a `_ones`-table atom is not HP, it is the points half,
/// and [`absorb_max_hp_fraction_value`] reads the ceiling that states the same number.
///
/// The halves agreeing is what makes counting once right, so it is GRADED, not assumed —
/// `absorb_mode_gate_corpus::the_fraction_pair_states_one_shield`, over 45 pairs on four
/// datasets (3 / 15 / 24 / 3), zero disagreements. Where only one half exists it answers alone:
/// 12 / 0 / 9 / 12 powers ceiling-only, 0 / 6 / 8 / 4 points-only, the latter being the shields
/// whose ceiling expression reads the TARGET's max HP and stays Indeterminate. Counted per power
/// INSTANCE — the census this came from (`absorb_atom_census` §5b, deleted with the bag in
/// 54093d10a2) deduplicated by internal name, which read Homecoming's pair population as zero
/// because Brute's fill-less Parasitic Aura answered for all four archetypes.
///
/// A real AT table (`Melee_HealSelf`, `Ranged_Heal`) is the other form: absolute HP, no ceiling
/// sibling anywhere in the corpus, and Homecoming spells most of its shields that way.
pub fn states_max_hp_fraction(table: Option<&str>) -> bool {
    table.is_some_and(|t| t.to_ascii_lowercase().ends_with("_ones"))
}

/// The MaxHP-fraction shield's PER-FOE increment, read atom-native — the `maxHPFractionPerTarget`
/// companion slot, the last field of the `absorb` bag with no atom behind it (PROD6C-3j).
///
/// A fraction shield inside a foe-targeted AoE grows with the foes hit like every other self-buff
/// in its block, but it has no scale to increment: the ceiling atom's scale is the `1.0`
/// placeholder the magnitude Expression sits on, so an ordinary `perTarget` there would grow a
/// number that means nothing. Each foe re-applies the same Expression, so **the increment IS the
/// fraction**, which is why the converter carries it as a companion rather than as a `perTarget`
/// (`computeAoePerTargetPatches`, `convert-powerset.cjs:7908`).
///
/// Every input the converter uses rides the wire. Its AoE gate is the power's own
/// `effectArea` + `stats.maxTargets` (the unbounded `255` sentinel and a single-target power both
/// fail it), and its template gate is `target`, `aspect`, `type` and `stack` — `toWho`,
/// `aspect`, `attribType` and `stacking` on the atom. `Replace` is excluded deliberately: it is
/// the always-on base application, not a per-foe increment.
///
/// The recipient test is the converter's literal `Self`, NOT the wider [`lands_on_caster`]
/// (TARGETS-2). The widening was measured before being declined: it admits no further power in
/// any fork here, so taking it would be an unmeasured change to a two-power family rather than
/// the correction it was for stacking.
///
/// **Graded by two live second sides**, in `absorb_mode_gate_corpus`. The original grading —
/// `absorb_atom_census` §5e, per power against the shipped base slot, 2 / 1 / 1 agreeing — is
/// historical and cannot be re-run: atom1-13 deleted both the slot and the census. What replaced
/// it reaches the same claim from the wire.
///
/// - `the_stance_companion_states_the_per_foe_fraction` — the companion field itself, which only
///   its BASE copy left the contract: `conditionalEffects[].effects` is a different key and still
///   ships one. 2 / 0 / 0 / 2 entries, the Bio Armor Defensive Adaptation pair.
/// - `the_points_half_states_the_per_foe_fraction` — where a `_ones` shield writes a points half,
///   that half's own `perTarget` states the increment, stamped onto the template rather than
///   evaluated out of this Expression. 3 / 4 / 7 / 3 base powers.
///
/// Together they cover every POWER the reader answers on, though not every atom: Homecoming and
/// Brainstorm author Brute's Parasitic Aura and Sentinel's Parasitic Leech ceiling-only, so those
/// two base atoms carry no second side and are graded through their own stance copies instead.
///
/// **The area gate's narrow arms are inert, and no guard can hold them.** Returning `None`
/// outright reds both legs, and reading the atom's raw scale instead of the evaluated ceiling
/// reds both; but widening `max_targets <= 1.0 || == 255.0` to a bare `< 1.0` survives, because
/// no power on any fork carries a `Self`/stacking absorb Expression outside an AoE or Cone with a
/// bounded target count. Stated rather than counted as coverage.
pub fn absorb_max_hp_fraction_per_target(power: &Power) -> Option<f64> {
    let stats = crate::projection::extra_object(power, "stats");
    let max_targets = crate::projection::object_number(stats, "maxTargets")?;
    let area = power.extra.get("effectArea").and_then(|v| v.as_str());
    if !matches!(area, Some("AoE" | "Cone")) || max_targets <= 1.0 || max_targets == 255.0 {
        return None;
    }
    // First match, as the converter breaks on one: only one distinct fraction ever reaches the
    // slot, because a group carrying more is deferred before this point.
    base_atoms_of_type(power, EffectType::Absorb)
        .into_iter()
        .find_map(|atom| {
            (atom.to_who == Some(ToWho::Self_)
                && matches!(
                    atom.stacking,
                    Some(Stacking::Stack | Stacking::Continuous | Stacking::RefreshToCount)
                ))
            .then(|| fraction_of(atom))
            .flatten()
        })
}

/// The atoms the converter routes to `addOrAccumulate('absorb')` — the FLAT-HP half of the slot.
///
/// Two `continue`s in the converter's absorb arm (`convert-powerset.cjs:6984`) carve the other two
/// forms out: an `aspect=Maximum` + `type=Expression` atom is the MaxHP-fraction magnitude
/// [`absorb_max_hp_fraction_value`] reads, and an `aspect=Strength` atom is a `specialBuff` meta.
/// Everything else is flat, INCLUDING the `aspect=Maximum` + `type=Magnitude` shape — the
/// converter's test is on the pair, not on the aspect alone, and Frigid Shield's shield is that
/// shape.
fn flat_atoms(power: &Power) -> Vec<&AtomicEffect> {
    base_atoms_of_type(power, EffectType::Absorb)
        .into_iter()
        .filter(|a| a.aspect != Some(Aspect::Str))
        .filter(|a| {
            !(a.aspect == Some(Aspect::Max) && a.attrib_type == Some(AttribType::Expression))
        })
        .collect()
}

/// The FLAT-HP absorb magnitude read atom-native — the `{scale, table, perTarget}` triple of the
/// `absorb` bag slot, or `None` when the power routes no flat absorb atom (ATOM-BAG-3).
///
/// Two rules, and the second is the one the bag could not state.
///
/// **Fold in encounter order, restarting on a table change.** That is `foldResourceSlot`
/// (`convert-powerset.cjs:6026`) exactly: a different table opens a new accumulator rather than a
/// second bucket, so the value is the last contiguous same-table run. Reproduced rather than
/// improved on, because a reader that disagrees with the shipped slot for a second reason cannot
/// be graded against it.
///
/// **A mod re-applied on a schedule counts once.** Particle Shielding grants a 7.5% shield every
/// five seconds for thirty — seven templates identical but for their `delay`, each lasting five
/// seconds, stacking `Replace`. Its help says as much ("you will gain a small absorption shield
/// every few seconds"). The bag has no field for a delay, so the converter's fold summed all seven
/// into 52.5% and `absorbStackCount` (`:6554`) divided the total back down and recorded the count
/// as a STACK DEPTH — two compensating errors that leave the base value right and offer a ×7
/// slider for a shield that never doubles. Skipping the repeat here reaches the same base with
/// neither step, and no depth to carry.
///
/// The repeat test is whole-atom equality with [`AtomicEffect::delay`] cleared, not a hand-written
/// key: `absorbStackCount`'s own `allMatch` compares scale, table and target, and that is too
/// narrow twice over in the shipped corpus. Homecoming's Frigid Shield lists its shield as the
/// enhanceable/IgnoreStrength twin — two `Replace` templates identical on all three of those
/// fields and differing only in the flag — and both halves apply and SUM, the same idiom
/// `maxHPBuff`/`maxHPBuffUnenhanced` split into two slots for. Collapsing on `Replace` alone
/// halves it, and collapsing on scale/table/target alone does too.
///
/// Measured against the shipped slot per power by `absorb_atom_census`: 35 / 35 / 41 powers
/// agreed on scale, table AND `perTarget` across Homecoming, Rebirth and Thunderspy, with zero
/// disagreements and no power answering on one side only. That is the reading that justified
/// going atom-native, and it is HISTORICAL — the slot left the contract in atom1-13 and the
/// census was deleted with it, so the comparison has no second side left to make.
pub fn absorb_flat_value(power: &Power) -> Option<TypedValue> {
    let atoms = flat_atoms(power);
    let mut kept: Vec<&AtomicEffect> = Vec::new();
    for atom in atoms {
        let same_mod = |other: &&AtomicEffect| {
            let (mut a, mut b) = (atom.clone(), (*other).clone());
            a.delay = None;
            b.delay = None;
            a == b
        };
        if !kept.iter().any(same_mod) {
            kept.push(atom);
        }
    }

    let mut folded: Option<TypedValue> = None;
    for atom in kept {
        let table = super::table_of(atom).map(Box::from);
        let value = atom.scale.map_or(f64::NAN, f64::abs);
        match &mut folded {
            // `Math.abs` on an absent scale is NaN in the converter and NaN here; adding to it
            // keeps it NaN, which is the Rule 1 outcome — a visible break, not a silent zero.
            Some(v) if v.table == table => v.scale += value,
            _ => {
                folded = Some(TypedValue {
                    scale: value,
                    table,
                    per_target: None,
                })
            }
        }
    }
    // The AoE increment is stamped per atom and is uniform across the run; the fold has no sum to
    // make of it (`adjust_for_stacking` takes the per-target path off a single increment).
    if let Some(v) = &mut folded {
        v.per_target = flat_atoms(power)
            .iter()
            .find_map(|a| a.per_target.filter(|p| *p != 0.0));
    }
    folded
}
