//! `mezProtectionValue` — the curated-armor mez PROTECTION slots (`effects.hold`/`stun`/
//! `immobilize`/`sleep`/`confuse`/`fear`), migrated from the bag (ATOM11 / DATA-GAP-REGISTER
//! PASS2B-10). Each slot is the `Mez/<subType>` atom with a non-resistance / non-strength face
//! (`aspect ∉ {Res, Str}` — the converter's else-branch at `convert-powerset.cjs:6686`); the
//! applier then credits it as protection when the winning atom's table is `Res_Boolean`, or when
//! it is protection-spelled on a non-foe power (MEZPROT-2), or for a self-KB `Melee_Ones`
//! (the knockback path — see below). The frozen oracle reads only the bag, so
//! this is the rebuild reading atoms where the oracle reads the bag.
//!
//! The reader reproduces the FULL `effects[mezType]` bag slot — the converter's **max-MAGNITUDE,
//! PvE-preferred** fold over ALL that subType's non-res/non-str Mez atoms (every `toWho`, every
//! table), not just the Res_Boolean ones. This is load-bearing: some T9 armors (Power Surge, Icy
//! Bastion) carry BOTH a Res_Boolean protection atom AND a higher-magnitude NON-Res_Boolean mez
//! atom that WINS the fold, so the bag slot is non-Res_Boolean and the applier drops it. A reader
//! scoped to Res_Boolean atoms would resurrect that dropped protection (a phantom); reproducing the
//! full fold makes the applier's existing `Res_Boolean` gate credit exactly what it credits from
//! the bag. Behavior-preserving — the atom slot equals the bag slot for every credited (Res_Boolean)
//! case across the corpus (measured 0 credited bag-only / 0 credited phantom / 0 credited divergence
//! on HC + Rebirth; see the ATOM11 design spec and the `mez_protection_atom_bag_parity` guard).
//!
//! **knockback / knockup / repel are atom-native too (ATOM15 / PASS2B-1)** — via the separate
//! [`kb_protection_value`], which reproduces the converter's ACCUMULATE fold (over `Mez` +
//! `MezResist`, the reason a Mez-only reader "diverged" on Quantum/Evasive Maneuvers + Bo Ryaku)
//! restricted to SELF-directed protection atoms, so a foe-attack's knockback is excluded (the
//! PASS2B-1 fix). `mez_protection_value` (the six-mez fold) still returns `None` for KB/KU/repel —
//! they are a distinct fold owned by `kb_protection_value`. Repel routes to its own total; the
//! KB/KU pair folds to `max` and shares one. The flat pool/epic `protection` object is
//! corpus-vacuous.
//!
//! Thunderspy once carried zero Mez atoms for these subtypes (TSPY-3 `Unmapped`), which made its
//! curated-armor protection a `?? bag` case. **That is no longer true** — TSPY-3 recovered the
//! typing block, and Thunderspy now states 534 ungated protection magnitudes of exactly this
//! shape, so this reader answers there like anywhere else. The note is kept rather than deleted
//! because the stale version outlived its cause by long enough to mislead: it was still asserting
//! a fork had no atom path while MEZPROT-1 was being diagnosed on that fork's atoms.

use super::*;
use coh_data::slot_value::ScaledMez;
use coh_data::{
    reaches_caster, Aspect, AtomicEffect, AttribType, EffectType, Power, PvMode, SubType,
};

/// The subtypes that fold into each protection field. Only `fear` has two — the converter's
/// `MEZ_TYPES` maps both `afraid` and `terrorized` to `fear`, so both `Mez` subtypes participate in
/// the single `effects.fear` fold. Every other type is one subtype. knockback/knockup/repel are
/// ABSENT (owned by the separate [`kb_protection_value`], a distinct fold), so this reader returns
/// `None`.
fn field_subtypes(field: &str) -> Option<&'static [SubType]> {
    Some(match field {
        "hold" => &[SubType::Held],
        "stun" => &[SubType::Stunned],
        "immobilize" => &[SubType::Immobilized],
        "sleep" => &[SubType::Sleep],
        "confuse" => &[SubType::Confused],
        "fear" => &[SubType::Terrorized, SubType::Afraid],
        _ => return None,
    })
}

/// The `effects.<field>` mez-protection slot this power contributes, from its `Mez/<subType>`
/// atoms, or `None` for a field the reader does not own (knockback/knockup) or when no qualifying
/// atom exists (→ the `?? bag` fallback). The value is the converter's max-magnitude / PvE-preferred
/// winner as `{|scale|, table}` — the applier's downstream `Res_Boolean` / self-KB gate and
/// `|scale| × getTableValue(50)` resolution are untouched (this migrates only the source of the
/// `{scale, table}` pair). A table-less winner yields `None` (`Bag::mez` likewise requires a table).
pub fn mez_protection_value(power: &Power, field: &str) -> Option<ScaledMez> {
    let subtypes = field_subtypes(field)?;
    // The converter's `effects[mezType]` fold (cvt:4330): iterate the subType's non-res/non-str Mez
    // atoms in order; keep the max-magnitude one, but a PvE (non-`pvp` table) atom always beats a
    // PvP one regardless of magnitude, and equal-magnitude ties keep the earlier atom.
    let mut best: Option<(f64, bool, &AtomicEffect)> = None; // (magnitude, isPvP, atom)
    for atom in base_atoms_of_type(power, EffectType::Mez)
        .into_iter()
        .filter(|a| {
            a.sub_type.is_some_and(|s| subtypes.contains(&s))
                && a.aspect != Some(Aspect::Res)
                && a.aspect != Some(Aspect::Str)
                // `guardThunderspyAppliedMez` deletes this very slot when the power's
                // `targets_affected` names no foe and the row is not protection-backed, and now
                // stamps the atoms behind it (TSPY-10). Protection-backed rows are never stamped
                // — the guard carves them out first — so this only ever drops the applied face
                // the bag already removed (Revive/Restore Essence self-roots, Hibernate, Hide).
                && a.not_on_caster != Some(true)
        })
    {
        let is_pvp = table_of(atom).is_some_and(|t| t.to_lowercase().contains("pvp"));
        let magnitude = atom.magnitude.filter(|m| *m != 0.0).unwrap_or(1.0); // JS `magnitude || 1`
        let take = match &best {
            None => true,
            Some((_, best_is_pvp, _)) if *best_is_pvp != is_pvp => *best_is_pvp, // prefer PvE
            Some((best_mag, _, _)) => magnitude > *best_mag,
        };
        if take {
            best = Some((magnitude, is_pvp, atom));
        }
    }
    let atom = best?.2;
    Some(ScaledMez {
        scale: atom.scale?.abs(),
        table: table_of(atom)?.to_string(),
        // MEZPROT-2: whether the fold winner is protection-spelled. The apply pass credits
        // the slot when the table is Res_Boolean OR (this flag AND the power does not affect
        // a foe). Mirrors the converter's `protectionBackedMezKeys` three-spelling test.
        is_protection: is_protection_spelled(atom),
    })
}

/// The converter's three-spelling protection test (`protectionBackedMezKeys` / TSPY-8): the
/// winner is protection rather than applied control when the sign sits in any of the three
/// slots it can — negative scale (the Res_Boolean armors, e.g. Fortification's −24),
/// negative MAGNITUDE (Duration-typed mez, where scale×the duration), or an Expression
/// magnitude computed at runtime (Inner Will).
fn is_protection_spelled(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.magnitude.is_some_and(|m| m < 0.0)
        || a.attrib_type == Some(AttribType::Expression)
}

/// The `effects.knockback` / `effects.knockup` / `effects.repel` PROTECTION slot, atom-native
/// (ATOM15 / PASS2B-1). `field ∈ {"knockback", "knockup", "repel"}`. Reproduces the converter's KB accumulate fold
/// (`convert-powerset.cjs:6704`), but restricted to **self-directed protection** atoms — the
/// converter's branches 2a (`Self` + aspect=Res + Res_Boolean) and 3 (`Self` + aspect≠Res). Branch 1
/// (`toWho ≠ Self`) is EXCLUDED: that is offensive foe-knockback (Battle Axe Gash's `0.67`), not
/// caster protection — the PASS2B-1 fix that retires the lossy `effectArea + powerType` proxy. Spans
/// BOTH `Mez` AND `MezResist` effectTypes: the converter keys on the resolved attrib string, so a
/// `MezResist/Knockback` protection atom counts (Quantum/Evasive Maneuvers carry ONLY that; a
/// `Mez`-only reader missed it — the ATOM11 KB deferral). PvP-twin atoms are dropped (the converter
/// excludes them upstream). Accumulate `|scale|` with reset-on-table-change, in emission order;
/// returns `{|scale|, table}` or `None`.
pub fn kb_protection_value(power: &Power, field: &str) -> Option<ScaledMez> {
    let subtype = match field {
        "knockback" => SubType::Knockback,
        "knockup" => SubType::Knockup,
        // The converter files repel under `KNOCKBACK_TYPES` alongside the other two
        // (`convert-powerset.cjs:3385`, branch at 6713), so its slot is this accumulate fold, not
        // the six-mez max fold above. It routes to its own total (`protection_repel`), not
        // knockback's.
        //
        // WHICH FOLD is read from the converter, NOT graded by the corpus: routing repel through
        // `mez_protection_value` instead leaves all 143 `totals_replay.rs` builds green. The five
        // that state `protRepel` carry ONE surviving repel atom each (Granite Armor's is a
        // `Self`/`Current`/`Melee_Ones` scale −10, and Rebirth's duplicate pair does not both
        // survive), so max and accumulate coincide on every one of them. The two folds also
        // differ in their filters — this one drops the PvP twin and foe-facing atoms, that one
        // prefers PvE and honours `not_on_caster` — and those differences are equally unmeasured
        // here. The converter is the evidence; the corpus only confirms the magnitude.
        "repel" => SubType::Repel,
        _ => return None,
    };
    let mut cur: Option<(f64, String)> = None; // (accumulated |scale|, table)
    for atom in power.atoms.iter().filter(|a| {
        !is_gated(a)
            && matches!(a.effect_type, Some(EffectType::Mez) | Some(EffectType::MezResist))
            && a.sub_type == Some(subtype)
            && reaches_caster(a, power) // branch 1 (foe) excluded → PASS2B-1
            && a.pv_mode != Some(PvMode::PvP) // the converter drops the PvP twin upstream
            && !is_kb_resistance(a) // branch 2b (aspect=Res non-Res_Boolean) is KB *resistance*, not protection
    }) {
        let Some(table) = table_of(atom).map(str::to_string) else {
            continue;
        };
        let scale = atom.scale.unwrap_or(0.0).abs(); // converter's `Math.abs(a.scale || 0)`
        match &mut cur {
            Some((acc, t)) if *t == table => *acc += scale,
            _ => cur = Some((scale, table)),
        }
    }
    cur.map(|(scale, table)| ScaledMez {
        scale,
        table,
        // KB/KU protection is self-directed by construction (the fold above excludes foe
        // branches), so it is always protection.
        is_protection: true,
    })
}

/// The converter's branch 2b: a `Self` aspect=Res KB/KU atom on a NON-`Res_Boolean` table routes to
/// `effects.mezResistance` (knockback *resistance*, a distinct slot), not the protection slot.
fn is_kb_resistance(a: &AtomicEffect) -> bool {
    a.aspect == Some(Aspect::Res)
        && !table_of(a).is_some_and(|t| t.to_ascii_lowercase().contains("res_boolean"))
}
