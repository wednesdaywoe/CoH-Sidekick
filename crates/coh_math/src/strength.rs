//! Pass 1 — STRENGTH: the active +Strength self-buffs (Power Boost, Power Build Up,
//! Gather Shadows, Adrenal Booster, …). Ported from `character-totals.ts`
//! `collectStrengthBuffs` (@856), which accumulates the caster's per-aspect Strength
//! fractions into `GlobalBonuses.strength*`. These are non-ED multipliers on the caster's
//! OWN matching output, additive with enhancement strength, consumed by Pass 2 apply —
//! and they add nothing on their own ("twice nothing is nothing").
//!
//! **Atom-native, ahead of the beta (design decision 2026-07-16).** The beta reads the
//! transitional bag `effects.specialBuff`; it has no atom-native `specialBuff` applier.
//! We build one — the M3 execution-plan step-6 family list pointedly excludes
//! `specialBuff`, i.e. it is expected atom-native by Pass 1, and the atom stream already
//! carries the split cleanly: a +Strength effect is `aspect: Str, toWho: Self`, its
//! `effect_type`/`sub_type` naming the aspect. We reconstruct the bag's `specialBuff` map
//! from those atoms (last-write-wins per key, exactly the bag's single-entry-per-key
//! shape) and run the beta's identical routing over it. The totals gate compares this
//! Rust-atom-native reconstruction against the TS bag path end-to-end.
//!
//! **ED / enhancement strength deferred.** This pass is the GLOBAL +Strength only. The
//! per-power enhancement-derived strength (`calculatePowerEnhancementBonuses`, RB1 curves
//! plus IO ED) is applied inside Pass 2 (M3 step 5), where it modifies defense/to_hit/etc.
//! output — it is not a `strength*` field. The Alpha-vs-IO ED split (`combineWithAlphaED`)
//! layers on top of that per-power aggregation in Pass 2 when an Alpha incarnate is active
//! ([`crate::enhancement::combine_with_alpha_ed`], INCARNATE-1).
//!
//! **Stacking deferred.** The beta's `adjustForStacking` (perTarget / `stacksLinear` /
//! `maxStacks`) is a no-op under the default combat context (`targetsHit` undefined ⇒
//! value returned unchanged), which is all M3's synthetic gate exercises. The
//! slot-name-keyed stacking meta is bag-only and lands with Pass 2b (step 6, the one
//! carry-over that blocks deleting the bag); recorded here as the seam.

use crate::gather::ActivePower;
use crate::scaled::resolve_scaled_effect;
use crate::stacking::{adjust_for_stacking, StackFamily};
use crate::totals::CalcError;
use coh_data::slot_value::Scaled;
use coh_data::{reaches_caster, Aspect, AtomicEffect, EffectType, Power, PowerDatabase, SubType};
use std::collections::HashMap;

/// Per-aspect Strength fractions (the beta `StrengthBuffs`). `defense` and `mez` are the
/// per-power MAX across their sub-keys (the binary just enumerates every position/type);
/// the rest add across the map and across powers.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct StrengthBuffs {
    pub defense: f64,
    pub to_hit: f64,
    pub heal: f64,
    pub absorb: f64,
    pub end_mod: f64,
    pub movement: f64,
    pub mez: f64,
}

/// specialBuff keys that are defense Strength (all positions + all types + `defense`
/// itself, the `All`-subtype atom). The beta's `STRENGTH_DEFENSE_KEYS`.
const DEFENSE_KEYS: &[&str] = &[
    "defense", "melee", "ranged", "aoe", "smashing", "lethal", "fire", "cold", "energy",
    "negative", "psionic", "toxic",
];
/// specialBuff keys that are mez Strength (boosts both magnitude and duration). The beta's
/// `STRENGTH_MEZ_KEYS`.
const MEZ_KEYS: &[&str] = &["hold", "stun", "sleep", "confuse", "fear", "immobilize"];

/// The `specialBuff` key for a +Strength atom (`aspect: Str, toWho: Self`), or `None` if
/// this atom is not a strength contribution the bag would record. Mirrors the bag's
/// atom→`specialBuff` routing: the effect type names the aspect, and for the enumerated
/// `Enhancement` atoms the sub-type names the defense position/type or mez kind (`All` →
/// the aggregate `defense` key).
pub(crate) fn special_key(atom: &AtomicEffect) -> Option<&'static str> {
    match atom.effect_type? {
        EffectType::Heal => Some("heal"),
        EffectType::Absorb => Some("absorb"),
        EffectType::Endurance => Some("endurance"),
        EffectType::Movement => Some("movement"),
        EffectType::ToHit => Some("tohit"),
        EffectType::Enhancement => match atom.sub_type? {
            SubType::Melee => Some("melee"),
            SubType::Ranged => Some("ranged"),
            SubType::AoE => Some("aoe"),
            SubType::Smashing => Some("smashing"),
            SubType::Lethal => Some("lethal"),
            SubType::Fire => Some("fire"),
            SubType::Cold => Some("cold"),
            SubType::Energy => Some("energy"),
            SubType::Negative => Some("negative"),
            SubType::Psionic => Some("psionic"),
            SubType::Toxic => Some("toxic"),
            SubType::All => Some("defense"),
            SubType::Held => Some("hold"),
            SubType::Stunned => Some("stun"),
            SubType::Sleep => Some("sleep"),
            SubType::Confused => Some("confuse"),
            SubType::Terrorized => Some("fear"),
            SubType::Immobilized => Some("immobilize"),
            _ => None,
        },
        _ => None,
    }
}

/// Does this atom reach a `specialBuff` key at all? The membership half of [`special_key`],
/// used by [`crate::stacking::StackFamily::SpecialBuff`] to ask which of a power's atoms its
/// stacking question is about. An `aspect: Str` atom on a type with no key here is a strength
/// meta-template that no `specialBuff` consumer reads.
pub(crate) fn is_special_buff_atom(atom: &AtomicEffect) -> bool {
    atom.aspect == Some(Aspect::Str) && special_key(atom).is_some()
}

/// Reconstruct a power's `effects.specialBuff` map atom-native: `key → (scale, table)`,
/// over its `aspect: Str, toWho: Self` atoms, last-write-wins per key (the bag stores one
/// entry per key; the atoms for a key are uniform, so which one wins is immaterial).
/// `None` when the power carries no +Strength atoms (→ contributes nothing).
///
/// Base atoms only, the same scope [`crate::gather::gather_base_atoms`] gives every other
/// family. A mode-gated +Strength atom belongs to the build only while its stance is up, and
/// [`crate::gather::active_conditional_powers`] is the pass that re-admits one; reading it
/// here credited it in every stance. Bio Armor's Athletic Regulation is the whole population
/// — a Rested-Adaptation `movement` strength paid out under Defensive and Offensive too
/// (2 atoms on Homecoming and Brainstorm, 0 on Rebirth and Thunderspy).
fn special_buff_map(power: &Power) -> Option<HashMap<&'static str, (f64, Option<&str>)>> {
    let mut map: HashMap<&'static str, (f64, Option<&str>)> = HashMap::new();
    for atom in &power.atoms {
        if atom.gated == Some(true) {
            continue;
        }
        if atom.aspect != Some(Aspect::Str) || !reaches_caster(atom, power) {
            continue;
        }
        let (Some(key), Some(scale)) = (special_key(atom), atom.scale) else {
            continue;
        };
        // Abs the scale, mirroring the converter's `makeEffect` (`Math.abs(scale)`):
        // a specialBuff stores magnitude, and the sign of the effect lives in the
        // modifier table, not the scale. A self movement SLOW (Reaction Time's
        // JumpHeight, scale -0.7 on Melee_Slow, whose table is negative) must
        // reconstruct as +0.7 so it resolves to -0.7 <= 0 and credits nothing —
        // NOT as raw -0.7, whose double-negative with the table fabricates a
        // +movement-strength buff the game never grants (STRENGTH-1).
        map.insert(key, (scale.abs(), atom.modifier_table.as_deref()));
    }
    if map.is_empty() {
        None
    } else {
        Some(map)
    }
}

/// Collect the build's active +Strength self-buffs into per-aspect fractions. `powers` is
/// the mode-resolved active list ([`crate::gather::gather_active_powers`]); `archetype`
/// and `level` drive the AT-table resolution of each scaled `specialBuff` value.
///
/// The auto/active gate is already applied by gather. The beta additionally skips whole
/// non-self-targeted powers (a legacy foe -Special stored as a positive `specialBuff` on a
/// Foe power); the atom-native path is stricter and needs no power-level gate — a foe
/// debuff's atoms are `toWho: Target`, which [`special_buff_map`] already excludes.
pub fn collect_strength_buffs(
    powers: &[ActivePower],
    archetype: &str,
    level: i32,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> StrengthBuffs {
    let mut strength_buffs = StrengthBuffs::default();
    let caster_class = db.class_name_of(archetype);
    for active in powers {
        // Narrowed to this build's archetype: the Scrapper and Stalker arms of a shared
        // epic attack each carry their own `DamageBuff|Str|Self`, and folding both in
        // credits a build with another archetype's inherent (AT-FORK-1).
        let narrowed = active.def.for_caster_class(caster_class);
        let power: &Power = &narrowed;
        let Some(map) = special_buff_map(power) else {
            continue;
        };
        // The beta runs the specialBuff value through adjustForStacking (character-totals.ts:882)
        // before resolving it: a Build Up-class power stacks its +Strength fraction with the
        // cast. Read off the same `aspect: Str` atoms the map above is built from, so source and
        // stacking metadata now come from one place (ATOM-BAG-2) rather than the map from atoms
        // and its multiplier from the bag's `stacksLinear`.
        let stack_cap = StackFamily::SpecialBuff.cap(power);
        // Within one power the defense (and mez) sub-keys are uniform, so take the
        // representative (max) rather than summing the ~12 enumerated keys.
        let mut defense_max = 0.0_f64;
        let mut mez_max = 0.0_f64;
        for (key, (scale, table)) in map {
            let stacked = adjust_for_stacking(
                &Scaled {
                    scale,
                    table: table.map(String::from),
                    per_target: None,
                },
                active.targets_hit,
                stack_cap.is_some(),
                stack_cap,
                crate::stacking::target_count(power),
            );
            let fraction = resolve_scaled_effect(
                stacked.scale,
                stacked.table.as_deref(),
                archetype,
                level,
                db,
                errors,
            );
            if fraction <= 0.0 {
                continue;
            }
            if DEFENSE_KEYS.contains(&key) {
                defense_max = defense_max.max(fraction);
            } else if MEZ_KEYS.contains(&key) {
                mez_max = mez_max.max(fraction);
            } else if key == "tohit" {
                strength_buffs.to_hit += fraction;
            } else if key == "heal" {
                strength_buffs.heal += fraction;
            } else if key == "absorb" {
                strength_buffs.absorb += fraction;
            } else if key == "endurance" {
                strength_buffs.end_mod += fraction;
            } else if key == "movement" {
                strength_buffs.movement += fraction;
            }
        }
        strength_buffs.defense += defense_max;
        strength_buffs.mez += mez_max;
    }
    strength_buffs
}
