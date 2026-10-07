//! Atom-census for the ATOMIC-STATE-AUDIT confirmation backlog (ATOM10-13) and the ATOM1
//! recharge / ATOM2 mezResistance migrations.
//!
//! Measures, across all three contract bundles, what typed atoms the powers of each
//! bag-read family actually carry — the "measure, don't believe" method the audit doc
//! demands. Not a permanent test; a manual probe gated behind the `census-probe`
//! feature so it never compiles in ordinary `cargo test`/`clippy` builds:
//!   cargo run -q -p coh_data --features census-probe --example atom_confirmation -- <family>
//! where <family> is one of: absorb protection taunt debuffresist recharge mezresist stealth
//! elusivity perception range endurancediscount maxendurance accuracy  (default: all).
//! The `recharge` arm correlates `effects.rechargeBuff` / self `rechargeDebuff` with the
//! `RechargeTime Str` atoms the ATOM1 applier reads; the `mezresist` arm correlates
//! `effects.mezResistance` with the per-type `MezResist Res` atoms the ATOM2 applier reads
//! (bag-vs-atom value equivalence + phantoms, both directions). The `stealth` arm dumps the
//! `Stealth` atom shape distribution — the measurement that surfaced BRIDGE-2: pre-fix all
//! three stealth attribs collapsed to `Stealth`/no-subType, indistinguishable by every typed
//! field; post-fix they carry `RadiusPvE`/`RadiusPvP`/`Translucency` so the ATOM3 applier can
//! read the two radii apart.

use coh_data::atom::{Aspect, AtomicEffect, EffectType, Stacking, SubType};
use coh_data::{DatasetId, PowerDatabase};
use serde_json::Value;
use std::path::PathBuf;

fn load(ds: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(ds.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {ds:?}: {e}"))
}

fn bag(p: &coh_data::Power) -> Option<&serde_json::Map<String, Value>> {
    p.extra.get("effects").and_then(Value::as_object)
}

fn atom_desc(a: &AtomicEffect) -> String {
    format!(
        "type={:?} sub={:?} aspect={:?} toWho={:?} attrib={:?} scale={:?} table={:?}",
        a.effect_type.map(|e| e.as_wire()),
        a.sub_type.map(|s| s.as_wire()),
        a.aspect.map(|x| x.as_wire()),
        a.to_who.map(|x| x.as_wire()),
        a.attrib_type.map(|x| x.as_wire()),
        a.scale,
        a.modifier_table.as_deref(),
    )
}

fn table_kind(a: &AtomicEffect) -> &'static str {
    match a.modifier_table.as_deref() {
        None => "no-table",
        Some(t) => {
            let l = t.to_lowercase();
            if l.contains("ones") {
                "ones(scale-direct)"
            } else {
                "real-table"
            }
        }
    }
}

fn incr(map: &mut std::collections::BTreeMap<String, usize>, key: String) {
    *map.entry(key).or_insert(0) += 1;
}

// ---- ATOM10 absorb -----------------------------------------------------------------
fn absorb(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM10 absorb — {ds:?} ##########");
    let mut atom_by_kind: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize;
    let mut samples = 0usize;
    for p in db.all_powers() {
        let absorb_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Absorb))
            .collect();
        let has_bag = bag(p).and_then(|b| b.get("absorb")).is_some();
        for a in &absorb_atoms {
            incr(
                &mut atom_by_kind,
                format!(
                    "aspect={:?} {}",
                    a.aspect.map(|x| x.as_wire()),
                    table_kind(a)
                ),
            );
        }
        if has_bag {
            bag_present += 1;
            if absorb_atoms.is_empty() {
                bag_only += 1;
            } else {
                bag_and_atom += 1;
            }
        }
        if (has_bag || !absorb_atoms.is_empty()) && samples < 14 {
            samples += 1;
            let bagv = bag(p).and_then(|b| b.get("absorb")).map(|v| v.to_string());
            println!("  {}  bag.absorb={:?}", p.name, bagv);
            for a in &absorb_atoms {
                println!("      ATOM {}", atom_desc(a));
            }
        }
    }
    println!("  -- Absorb atom counts by (aspect, table-kind): {atom_by_kind:?}");
    println!(
        "  -- bag.absorb powers: {bag_present} (with-atom {bag_and_atom}, bag-only {bag_only})"
    );
}

// ---- ATOM11 mez protection (strict bidirectional) ----------------------------------
// The applier (apply.rs:586/619) reproduces two bag paths: (1) the flat `protection` object —
// corpus-vacuous (0 powers all datasets); (2) the curated-armor slots effects.hold/stun/
// immobilize/sleep/confuse/fear/knockback/knockup, credited when the table is `Res_Boolean` OR a
// self-KB `Melee_Ones` (Acrobatics), value `|scale| × getTableValue(level 50)`. The source swap
// happens BEFORE that gate, so the reader must reproduce the FULL `effects[field]` bag slot. The
// converter writes each slot with TWO different folds:
//   * 6 MEZ types (hold/stun/immobilize/sleep/confuse/fear): `effects[mezType]` folded by
//     max-MAGNITUDE, PvE-preferred, over Mez/<sub> atoms with aspect ∉ {resistance, strength}
//     (the else-branch at cvt:4330; the winner's {|scale|, table} is the slot).
//   * knockback/knockup: `effects[kb]` ACCUMULATE `+= |scale|` (reset on table change) over the KB
//     sub-branches (cvt:4347) — self aspect=Cur, self aspect=Res+Res_Boolean, and foe non-res
//     scale>0 (a foe-KB effect). A self aspect=Res NON-boolean KB routes to mezResistance (ATOM2),
//     NOT here.
// This census simulates BOTH folds and tests, per field and BOTH directions, atom == bag slot.
const MEZ6: [(&str, SubType); 6] = [
    ("hold", SubType::Held),
    ("stun", SubType::Stunned),
    ("immobilize", SubType::Immobilized),
    ("sleep", SubType::Sleep),
    ("confuse", SubType::Confused),
    ("fear", SubType::Terrorized),
];
const KB2: [(&str, SubType); 2] = [
    ("knockback", SubType::Knockback),
    ("knockup", SubType::Knockup),
];

fn tbl_lower(a: &AtomicEffect) -> String {
    a.modifier_table.as_deref().unwrap_or("").to_lowercase()
}
fn is_res_boolean_atom(a: &AtomicEffect) -> bool {
    tbl_lower(a).contains("res_boolean")
}
/// The 6-MEZ max-magnitude / PvE-preferred fold → (|scale|, table_lower) of the winning atom.
fn mez6_fold(p: &coh_data::Power, sub: SubType) -> Option<(f64, String)> {
    let mut best: Option<(f64, bool, f64, String)> = None; // (mag, isPvP, |scale|, table)
    for a in p.atoms.iter().filter(|a| {
        a.effect_type == Some(EffectType::Mez)
            && a.sub_type == Some(sub)
            && a.aspect != Some(Aspect::Res)
            && a.aspect != Some(Aspect::Str)
            && a.gated != Some(true)
    }) {
        let Some(scale) = a.scale else { continue };
        let table = tbl_lower(a);
        let mag = a.magnitude.filter(|m| *m != 0.0).unwrap_or(1.0); // JS `magnitude || 1`
        let is_pvp = table.contains("pvp");
        let take = match &best {
            None => true,
            Some((_, bpvp, _, _)) if *bpvp != is_pvp => *bpvp, // prefer PvE
            Some((bmag, _, _, _)) => mag > *bmag,
        };
        if take {
            best = Some((mag, is_pvp, scale.abs(), table));
        }
    }
    best.map(|(_, _, s, t)| (s, t))
}
/// The same max-mag/PvE fold, restricted to `Res_Boolean`-tabled atoms — the SCOPED reader the
/// migration uses. `PvE-prefer` is inert here (Res_Boolean is never a PvP table), so it reduces
/// to max-magnitude, first-wins on ties.
fn mez6_fold_res_boolean(p: &coh_data::Power, sub: SubType) -> Option<(f64, String)> {
    let mut best: Option<(f64, f64, String)> = None; // (mag, |scale|, table)
    for a in p.atoms.iter().filter(|a| {
        a.effect_type == Some(EffectType::Mez)
            && a.sub_type == Some(sub)
            && a.aspect != Some(Aspect::Res)
            && a.aspect != Some(Aspect::Str)
            && a.gated != Some(true)
            && is_res_boolean_atom(a)
    }) {
        let Some(scale) = a.scale else { continue };
        let mag = a.magnitude.filter(|m| *m != 0.0).unwrap_or(1.0);
        if best.as_ref().is_none_or(|(bmag, _, _)| mag > *bmag) {
            best = Some((mag, scale.abs(), tbl_lower(a)));
        }
    }
    best.map(|(_, s, t)| (s, t))
}
/// The KB accumulate fold (sum |scale| while the table holds, reset on change) → (scale, table).
fn kb_fold(p: &coh_data::Power, sub: SubType) -> Option<(f64, String)> {
    let mut cur: Option<(f64, String)> = None;
    for a in p.atoms.iter().filter(|a| {
        a.effect_type == Some(EffectType::Mez) && a.sub_type == Some(sub) && a.gated != Some(true)
    }) {
        let Some(scale) = a.scale else { continue };
        let is_self = a.to_who == Some(coh_data::atom::ToWho::Self_);
        let is_res = a.aspect == Some(Aspect::Res);
        let feeds = (!is_self && !is_res && scale > 0.0)
            || (is_self && is_res && is_res_boolean_atom(a))
            || (is_self && !is_res);
        if !feeds {
            continue;
        }
        let table = tbl_lower(a);
        match &mut cur {
            Some((s, t)) if *t == table => *s += scale.abs(),
            _ => cur = Some((scale.abs(), table)),
        }
    }
    cur
}
fn protection(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM11 mez protection — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut path1 = 0usize; // flat protection object (expect 0)
    let mut per_field: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    // Split bag-only / phantom by whether the CREDITED gate (Res_Boolean) would fire — a
    // non-Res_Boolean divergence is behavior-inert (the applier drops it), a Res_Boolean one is not.
    let mut bag_only: Vec<String> = Vec::new();
    let mut phantom: Vec<String> = Vec::new();
    let mut divergences: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        if bag(p).and_then(|b| b.get("protection")).is_some() {
            path1 += 1;
        }
        let rb = |t: &str| t.contains("res_boolean");
        // === 6 MEZ types: the SCOPED reader (Res_Boolean atoms only) vs the CREDITED bag value ===
        // The migration reads a Res_Boolean-scoped max-mag/PvE fold. Behavior-preserving IFF, per
        // field, the scoped value equals the applier's credited value = (the FULL-fold winner if it
        // is Res_Boolean, else nothing — the applier drops a non-Res_Boolean winner). The full fold
        // == the bag slot for credited cases (verified separately), so `full_v if Res_Boolean` is
        // the credited target. The failure to hunt: full winner NON-Res_Boolean (a foe-mez outweighs
        // the protection) while a Res_Boolean atom exists → the scoped reader would PHANTOM-credit.
        for (field, sub) in MEZ6 {
            let full_v = mez6_fold(p, sub); // max-mag over aspect∉{Res,Str}, any table
            let scoped_v = mez6_fold_res_boolean(p, sub); // same fold, Res_Boolean atoms only
            let target = full_v.filter(|(_, t)| rb(t)); // the applier's credited value
            let entry = per_field.entry(field.to_string()).or_insert((0, 0));
            match (&target, &scoped_v) {
                (Some((ts, tt)), Some((ss, st))) => {
                    entry.0 += 1;
                    if (ts - ss).abs() < 1e-9 && tt == st {
                        entry.1 += 1;
                    } else {
                        divergences.push(format!(
                            "  VALUE [{field}] {} credited=({ts},{tt}) scoped=({ss},{st}) CREDITED",
                            p.name
                        ));
                    }
                }
                (Some((ts, tt)), None) => {
                    entry.0 += 1;
                    bag_only.push(format!(
                        "  BAG-ONLY [{field}] {} credited=({ts},{tt}) CREDITED",
                        p.name
                    ));
                }
                (None, Some((ss, st))) => {
                    phantom.push(format!(
                        "  PHANTOM [{field}] {} scoped=({ss},{st}) CREDITED(gate-RED)",
                        p.name
                    ));
                }
                (None, None) => {}
            }
        }
        // === knockback/knockup census (historical) ===
        // This local `kb_fold` is the OLD Mez-only fold whose apparent divergence on 3 HC powers
        // (Quantum/Evasive Maneuvers, Bo Ryaku) deferred KB in ATOM11. ATOM15 (2026-07-19) RESOLVED it:
        // the real reader `coh_math::appliers::mez_protection::kb_protection_value` reads `Mez` +
        // `MezResist` self-directed atoms, reproducing the bag and fixing PASS2B-1. Kept here as the
        // historical census that motivated the coverage fix.
        for (field, sub) in KB2 {
            let atom_v = kb_fold(p, sub);
            let bag_v: Option<(f64, String)> = bag(p).and_then(|b| b.get(field)).and_then(|v| {
                let scale = v.get("scale").and_then(Value::as_f64)?;
                Some((
                    scale,
                    v.get("table")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase(),
                ))
            });
            let entry = per_field.entry(format!("KB:{field}")).or_insert((0, 0));
            match (&bag_v, &atom_v) {
                (Some((bs, bt)), Some((as_, at))) => {
                    entry.0 += 1;
                    if (bs - as_).abs() < 1e-9 && bt == at {
                        entry.1 += 1;
                    } else if rb(bt) || rb(at) {
                        divergences.push(format!(
                            "  VALUE [KB:{field}] {} bag=({bs},{bt}) atom=({as_},{at}) CREDITED",
                            p.name
                        ));
                    }
                }
                (Some((bs, bt)), None) if rb(bt) => {
                    entry.0 += 1;
                    bag_only.push(format!(
                        "  BAG-ONLY [KB:{field}] {} bag=({bs},{bt}) CREDITED",
                        p.name
                    ));
                }
                (Some(_), None) => entry.0 += 1,
                (None, Some((as_, at))) if rb(at) => {
                    phantom.push(format!(
                        "  PHANTOM [KB:{field}] {} atom=({as_},{at}) CREDITED(gate-RED)",
                        p.name
                    ));
                }
                _ => {}
            }
        }
    }
    println!("  -- path1 (flat protection object) powers: {path1}");
    println!("  -- per field (bag occurrences, atom==bag scale+table):");
    for (k, (n, m)) in &per_field {
        println!("       {k}: {m}/{n}");
    }
    let cred = |v: &[String]| v.iter().filter(|s| s.contains("CREDITED")).count();
    println!(
        "  -- BAG-ONLY: {} ({} CREDITED / Res_Boolean)",
        bag_only.len(),
        cred(&bag_only)
    );
    for m in bag_only.iter().filter(|s| s.contains("CREDITED")).take(40) {
        println!("{m}");
    }
    println!(
        "  -- PHANTOM: {} ({} CREDITED gate-RED)",
        phantom.len(),
        cred(&phantom)
    );
    for m in phantom.iter().filter(|s| s.contains("CREDITED")).take(40) {
        println!("{m}");
    }
    println!(
        "  -- DIVERGENCES: {} ({} CREDITED)",
        divergences.len(),
        cred(&divergences)
    );
    for m in divergences.iter().take(40) {
        println!("{m}");
    }
}

// ---- ATOM12 taunt/placate resistance (strict bidirectional) ------------------------
// The applier reads the TOP-LEVEL `effects.taunt` / `effects.placate` slots (apply.rs:741),
// crediting resistance ONLY when the slot is an OBJECT with a `Res_Boolean` table
// (`|scale| × getTableValue × 100`). The converter writes `effects[ctrlType] = makeEffect()`
// (CONTROL_TYPES, aspect != resistance — an aspect=resistance taunt/placate routes to
// `mezResistance` instead — OVERWRITE / last-write-wins) from a `Mez` atom whose subType is
// Taunt/Placate. Migration = read that atom's (|scale|, table) instead. This census tests, per
// slot and BOTH directions, that the atom equals the bag slot for EVERY object-form entry (not
// just the Res_Boolean ones — the source swap is applied before the downstream Res_Boolean gate,
// so the whole slot must match), and separately the Res_Boolean-credited subset.
fn taunt(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM12 taunt/placate resistance — {ds:?} ##########");
    for (slot, sub) in [("taunt", SubType::Taunt), ("placate", SubType::Placate)] {
        let mut seen: std::collections::BTreeSet<String> = Default::default();
        let mut matched = 0usize; // object-form bag entry with an exact atom
        let mut res_boolean = 0usize; // …of which credited (Res_Boolean table)
        let mut bag_only: Vec<String> = Vec::new();
        let mut phantom: Vec<String> = Vec::new();
        let mut divergences: Vec<String> = Vec::new();
        for p in db.all_powers() {
            let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
            if !seen.insert(id) {
                continue;
            }
            // bag: object-form effects.<slot> (a bare number is `typeof !== 'object'` → skipped).
            let bag_v: Option<(f64, String)> = bag(p)
                .and_then(|b| b.get(slot))
                .filter(|v| v.is_object())
                .and_then(|v| {
                    let scale = v.get("scale").and_then(Value::as_f64)?;
                    Some((
                        scale,
                        v.get("table")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_lowercase(),
                    ))
                });
            // atom: LAST non-gated Mez/<sub> atom, aspect != Res (OVERWRITE) → (|scale|, table).
            let atom_v: Option<(f64, String)> = p
                .atoms
                .iter()
                .filter(|a| {
                    a.effect_type == Some(EffectType::Mez)
                        && a.sub_type == Some(sub)
                        && a.aspect != Some(Aspect::Res)
                        && a.gated != Some(true)
                })
                .next_back()
                .and_then(|a| {
                    a.scale.map(|s| {
                        (
                            s.abs(),
                            a.modifier_table.as_deref().unwrap_or("").to_lowercase(),
                        )
                    })
                });
            match (&bag_v, &atom_v) {
                (Some((bs, bt)), Some((as_, at))) => {
                    if (bs - as_).abs() < 1e-9 && bt == at {
                        matched += 1;
                        if bt.contains("res_boolean") {
                            res_boolean += 1;
                        }
                    } else {
                        divergences.push(format!(
                            "  VALUE [{slot}] {} bag=({bs},{bt}) atom=({as_},{at})",
                            p.name
                        ));
                    }
                }
                (Some((bs, bt)), None) => {
                    bag_only.push(format!("  BAG-ONLY [{slot}] {} bag=({bs},{bt})", p.name));
                }
                (None, Some((as_, at))) => {
                    // A phantom is gate-RED only if Res_Boolean (else the downstream gate drops it).
                    let flag = if at.contains("res_boolean") {
                        " RES_BOOLEAN(gate-RED)"
                    } else {
                        ""
                    };
                    phantom.push(format!(
                        "  PHANTOM [{slot}] {} atom=({as_},{at}){flag}",
                        p.name
                    ));
                }
                (None, None) => {}
            }
        }
        println!(
            "  -- {slot}: object-form matched {matched} (of which Res_Boolean-credited {res_boolean}), bag-only {}, phantom {}, divergences {}",
            bag_only.len(),
            phantom.len(),
            divergences.len()
        );
        for m in bag_only
            .iter()
            .chain(phantom.iter())
            .chain(divergences.iter())
            .take(30)
        {
            println!("{m}");
        }
    }
}

// ---- ATOM13 debuffResistance (strict bidirectional) --------------------------------
// The applier reads `bag.debuffResistance` (apply.rs:693), routing 10 types via
// `add_debuff_resistance`: defense/endurance/movement/perception/recharge/recovery/
// regeneration/tohit, plus the accuracy/range pair DEBUFFRES-1 added. The converter writes
// each `effects.debuffResistance.<type> = makeEffect()` (OVERWRITE / last-write-wins) from a
// `<Attr> aspect=resistance` atom, decoded to a `<EffectType> aspect=Res` atom (the movement
// sub-axes Run/Fly/Jump/JumpHeight all collapse to `debuffResistance.movement`; endurance is
// `Endurance` NOT `EnduranceDiscount`; defense is the ATOM14-bridged `Defense/All aspect=Res`).
// Migration = read the LAST matching `<EffectType> aspect=Res` non-gated atom (|scale|, table).
// This census tests, per ROUTED type and BOTH directions, that the atom exactly equals the bag.
const DR_ROUTED: &[(&str, EffectType)] = &[
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
fn debuffresist(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM13 debuffResistance — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut per_type: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut bag_only: Vec<String> = Vec::new(); // routed bag type, no atom (keep bag fallback)
    let mut phantom: Vec<String> = Vec::new(); // routed atom value, no bag entry (gate-RED)
    let mut divergences: Vec<String> = Vec::new();
    // NON-routed RAW bag keys (accuracy/range/…) enumerated for visibility. They have no target
    // global; none reaches the router on the live path (the atom reader emits only routed types).
    let mut nonrouted: std::collections::BTreeMap<String, usize> = Default::default();
    for p in db.all_powers() {
        let idp = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(idp) {
            continue;
        }
        let bag_dr = bag(p)
            .and_then(|b| b.get("debuffResistance"))
            .and_then(Value::as_object);
        if let Some(o) = bag_dr {
            for k in o.keys() {
                let kl = k.to_ascii_lowercase();
                if !DR_ROUTED.iter().any(|(t, _)| *t == kl) {
                    incr(&mut nonrouted, kl);
                }
            }
        }
        for (ty, et) in DR_ROUTED {
            // atom: LAST non-gated <et> aspect=Res atom (converter OVERWRITE) → (|scale|, table_lower)
            let atom_v: Option<(f64, String)> = p
                .atoms
                .iter()
                .filter(|a| {
                    a.effect_type == Some(*et)
                        && a.aspect == Some(Aspect::Res)
                        && a.gated != Some(true)
                })
                .next_back()
                .and_then(|a| {
                    a.scale.map(|s| {
                        (
                            s.abs(),
                            a.modifier_table.as_deref().unwrap_or("").to_lowercase(),
                        )
                    })
                });
            let bag_v: Option<(f64, String)> = bag_dr.and_then(|o| o.get(*ty)).and_then(|v| {
                let scale = v
                    .get("scale")
                    .and_then(Value::as_f64)
                    .or_else(|| v.as_f64())?;
                Some((
                    scale,
                    v.get("table")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_lowercase(),
                ))
            });
            let entry = per_type.entry((*ty).to_string()).or_insert((0, 0));
            match (&bag_v, &atom_v) {
                (Some((bs, bt)), Some((as_, at))) => {
                    entry.0 += 1;
                    if (bs - as_).abs() < 1e-9 && bt == at {
                        entry.1 += 1;
                    } else {
                        divergences.push(format!(
                            "  VALUE [{ty}] {} bag=({bs},{bt}) atom=({as_},{at})",
                            p.name
                        ));
                    }
                }
                (Some((bs, bt)), None) => {
                    entry.0 += 1;
                    bag_only.push(format!("  BAG-ONLY [{ty}] {} bag=({bs},{bt})", p.name));
                }
                (None, Some((as_, at))) => {
                    phantom.push(format!("  PHANTOM [{ty}] {} atom=({as_},{at})", p.name));
                }
                (None, None) => {}
            }
        }
    }
    println!("  -- per routed bag-type (occurrences, atom==bag scale+table):");
    for (k, (n, m)) in &per_type {
        println!("       {k}: {m}/{n}");
    }
    println!("  -- NON-routed RAW bag keys (expected empty since DEBUFFRES-1): {nonrouted:?}");
    println!(
        "  -- routed BAG-ONLY (keep bag fallback): {}",
        bag_only.len()
    );
    for m in bag_only.iter().take(30) {
        println!("{m}");
    }
    println!("  -- routed PHANTOM (gate-RED risk): {}", phantom.len());
    for m in phantom.iter().take(30) {
        println!("{m}");
    }
    println!("  -- routed VALUE divergences: {}", divergences.len());
    for m in divergences.iter().take(30) {
        println!("{m}");
    }
}

// ---- ATOM1 recharge ----------------------------------------------------------------
// The applier currently reads `bag.rechargeBuff` (scale × 100, NO table resolution —
// extractScaleValue). The bag converter routes `rechargetime` attrib + aspect=strength +
// non-debuff + non-slow-table → effects.rechargeBuff (LAST-WRITE-WINS). Migration = read the
// matching `RechargeTime` Str non-debuff atom instead. This census tests the ONLY thing that
// keeps the oracle/totals gate green: does the atom scale EXACTLY equal the bag scale, for
// every corpus power, in BOTH directions (no bag-only powers, no atom-only phantoms)?
fn is_debuff_like(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}
fn recharge_buff_atoms(p: &coh_data::Power) -> Vec<&AtomicEffect> {
    // Mirror the bag's rechargeBuff gate on the atom side.
    p.atoms
        .iter()
        .filter(|a| a.effect_type == Some(EffectType::RechargeTime))
        .filter(|a| a.gated != Some(true))
        .filter(|a| a.aspect == Some(Aspect::Str))
        .filter(|a| !is_debuff_like(a))
        .filter(|a| {
            !a.modifier_table
                .as_deref()
                .is_some_and(|t| t.to_lowercase().contains("slow"))
        })
        .collect()
}
fn recharge(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM1 recharge — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value
    let mut atom_only = 0usize; // PHANTOM: applier→value where beta→0
    let mut sum_match = 0usize; // bag scale == Σ|atom.scale|
    let mut has_pertarget = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    for p in db.all_powers() {
        // Dedup by identity — powers appear once per archetype that grants them.
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let bag_v = bag(p).and_then(|b| b.get("rechargeBuff"));
        let bag_scale: Option<f64> = bag_v.and_then(|v| {
            v.as_f64()
                .or_else(|| v.get("scale").and_then(Value::as_f64))
        });
        let bag_pertarget: Option<f64> =
            bag_v.and_then(|v| v.get("perTarget").and_then(Value::as_f64));
        let atoms = recharge_buff_atoms(p);
        let has_bag = bag_v.is_some();
        let has_atom = !atoms.is_empty();
        if has_bag {
            bag_present += 1;
            if bag_pertarget.is_some() {
                has_pertarget += 1;
            }
            if has_atom {
                bag_and_atom += 1;
                let atom_sum: f64 = atoms.iter().filter_map(|a| a.scale).map(f64::abs).sum();
                let close = bag_scale.is_some_and(|b| (b - atom_sum).abs() < 1e-9);
                if close {
                    sum_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag_scale={bag_scale:?} bag_perTarget={bag_pertarget:?} Σatoms={atom_sum} atoms={:?}",
                        p.name,
                        atoms.iter().map(|a| (a.scale, a.per_target)).collect::<Vec<_>>()
                    ));
                }
            } else {
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag={bag_v:?}", p.name));
            }
        }
        if has_atom && !has_bag {
            atom_only += 1;
            divergences.push(format!(
                "  ATOM-ONLY {} atoms={:?}",
                p.name,
                atoms.iter().map(|a| a.scale).collect::<Vec<_>>()
            ));
        }
    }
    println!("  bag.rechargeBuff powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom-only (PHANTOM): {atom_only}   bag-with-perTarget: {has_pertarget}");
    println!("  Σ|atom.scale| == bag.scale among with-atom: {sum_match}/{bag_and_atom}");
    if !divergences.is_empty() {
        println!("  -- divergences ({}):", divergences.len());
        for m in &divergences {
            println!("{m}");
        }
    }

    // ---- self-directed rechargeDebuff crash (Granite Armor −65%) --------------------
    // Bag: effects.rechargeDebuff with toWho:Self; applier resolves table × −100.
    // Converter gate: rechargetime attrib + (isDebuff || scale<0 || slow-table), self-targeting
    // sets toWho:Self. Atom side must be the self-facing RechargeTime crash.
    let mut crash_seen: std::collections::BTreeSet<String> = Default::default();
    let mut crash_bag = 0usize;
    let mut crash_with_atom = 0usize;
    let mut crash_bag_only = 0usize;
    let mut crash_rows: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !crash_seen.insert(id) {
            continue;
        }
        let rd = bag(p).and_then(|b| b.get("rechargeDebuff"));
        let is_self_crash = rd.and_then(|v| v.get("toWho").and_then(Value::as_str)) == Some("Self");
        if !is_self_crash {
            continue;
        }
        crash_bag += 1;
        let bag_scale = rd.and_then(|v| {
            v.as_f64()
                .or_else(|| v.get("scale").and_then(Value::as_f64))
        });
        // self-facing RechargeTime atoms carrying the crash (debuff / negative / slow)
        let crash_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::RechargeTime))
            .filter(|a| a.to_who == Some(coh_data::atom::ToWho::Self_))
            .filter(|a| {
                is_debuff_like(a)
                    || a.modifier_table
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains("slow"))
            })
            .collect();
        if crash_atoms.is_empty() {
            crash_bag_only += 1;
            crash_rows.push(format!("  CRASH-BAG-ONLY {} bag={rd:?}", p.name));
        } else {
            crash_with_atom += 1;
            crash_rows.push(format!(
                "  CRASH {} bag_scale={bag_scale:?} atoms={:?}",
                p.name,
                crash_atoms
                    .iter()
                    .map(|a| (
                        a.scale,
                        a.aspect.map(|x| x.as_wire()),
                        a.modifier_table.as_deref()
                    ))
                    .collect::<Vec<_>>()
            ));
        }
    }
    // phantom crash: a self-facing RechargeTime Str debuff/slow atom in a power with NO bag self-crash.
    let mut crash_phantom = 0usize;
    let mut phantom_seen: std::collections::BTreeSet<String> = Default::default();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !phantom_seen.insert(id) {
            continue;
        }
        let has_bag_crash = bag(p)
            .and_then(|b| b.get("rechargeDebuff"))
            .and_then(|v| v.get("toWho").and_then(Value::as_str))
            == Some("Self");
        let atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::RechargeTime))
            .filter(|a| a.to_who == Some(coh_data::atom::ToWho::Self_))
            .filter(|a| a.aspect == Some(Aspect::Str))
            .filter(|a| a.gated != Some(true))
            .filter(|a| {
                is_debuff_like(a)
                    || a.modifier_table
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains("slow"))
            })
            .collect();
        if !atoms.is_empty() && !has_bag_crash {
            crash_phantom += 1;
            crash_rows.push(format!(
                "  CRASH-PHANTOM {} atoms={:?}",
                p.name,
                atoms.iter().map(|a| a.scale).collect::<Vec<_>>()
            ));
        }
    }
    println!("  self rechargeDebuff crash powers: {crash_bag}  (with-atom {crash_with_atom}, BAG-ONLY {crash_bag_only}, PHANTOM {crash_phantom})");
    for m in crash_rows.iter().take(30) {
        println!("{m}");
    }
}

// ---- ATOM2 mezResistance -----------------------------------------------------------
// Bag: `effects.mezResistance[type] = { scale: |scale|, table }` for type in
// hold/stun/sleep/immobilize/confuse/fear (converter MEZ_TYPES + aspect=resistance), PLUS a
// self-targeting non-Res_Boolean KB-resistance → mezResistance[knockback/knockup/repel]. The
// converter accumulates `+= |scale|` for a same-(type,table) run else last-write-wins. Applier
// (apply.rs:551): `resolveScaledEffect(scale, table) × 100` — Res_Boolean RESOLVES here.
// `add_mez_resistance` routes hold/stun/immobilize/sleep/confuse/fear/knockback plus taunt/placate
// (MEZRES-2), repel and teleport (MEZRES-3), and declares only knockup unspent. Teleport is split
// upstream at the reader, which withholds the carriers protecting the entity the power moved
// rather than its caster; the bag cannot serve it at all, since it drops `toWho`. Migration = read
// `MezResist aspect=Res` atoms (subType→type) instead. This census tests, per bag type and in BOTH
// directions, whether the atom sum equals the bag scale (the only thing that keeps the totals gate
// green).
fn mez_type_of(a: &AtomicEffect) -> Option<&'static str> {
    match a.sub_type? {
        SubType::Held => Some("hold"),
        SubType::Stunned => Some("stun"),
        SubType::Sleep => Some("sleep"),
        SubType::Confused => Some("confuse"),
        SubType::Terrorized => Some("fear"),
        SubType::Immobilized => Some("immobilize"),
        SubType::Knockback => Some("knockback"),
        SubType::Knockup => Some("knockup"),
        SubType::Repel => Some("repel"),
        _ => None,
    }
}
// The seven types this ATOM2 census covers (a phantom on one turns the gate RED). The router also
// routes taunt/placate, whose own parity is the ATOM12 census's, not this one's.
const ROUTED_MEZ_RES_TYPES: [&str; 7] = [
    "hold",
    "stun",
    "immobilize",
    "sleep",
    "confuse",
    "fear",
    "knockback",
];
fn mezresist(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM2 mezResistance — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    // aspect distribution of MezResist atoms (confirm the family is aspect=Res).
    let mut mezresist_aspect: std::collections::BTreeMap<String, usize> = Default::default();
    // subType distribution of aspect=Res MezResist atoms (confirm the subType→type map is total).
    let mut mezresist_res_sub: std::collections::BTreeMap<String, usize> = Default::default();
    // per bag-type: (occurrences, atom-sum-matches-bag), split routed vs dropped.
    let mut per_type: std::collections::BTreeMap<String, (usize, usize)> = Default::default();
    let mut bag_only: Vec<String> = Vec::new(); // routed bag type, no matching atom sum
    let mut phantom: Vec<String> = Vec::new(); // routed atom-type value, no bag entry
    let mut divergences: Vec<String> = Vec::new();
    let mut kb_rows: Vec<String> = Vec::new(); // how KB-resistance is atom-encoded
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        for a in p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::MezResist))
        {
            incr(
                &mut mezresist_aspect,
                format!("{:?}", a.aspect.map(|x| x.as_wire())),
            );
            if a.aspect == Some(Aspect::Res) {
                incr(
                    &mut mezresist_res_sub,
                    format!("{:?}", a.sub_type.map(|s| s.as_wire())),
                );
            }
        }
        // The atom reader's proposed value per mez-type, faithful to the converter's TWO folds:
        //   * MEZ types (hold/stun/sleep/immobilize/confuse/fear): the converter ACCUMULATES
        //     `+= |scale|` per same (type,table) → SUM |scale|. No toWho gate (converter MEZ path
        //     has none).
        //   * KB types (knockback/knockup/repel): the converter OVERWRITES (`= makeEffect()`,
        //     last-write-wins) and only for SELF-targeting non-boolean → single |scale|,
        //     gated toWho=Self (foe-facing KB-res on control powers is skipped: `!isSelfTargeting`).
        // Over BASE (non-gated) aspect=Res MezResist atoms. `atom_scales` retained for diagnosis;
        // `tables_per_type` proves the single-table assumption behind the MEZ sum.
        const KB_TYPES: [&str; 3] = ["knockback", "knockup", "repel"];
        let mut atom_scales: std::collections::BTreeMap<&'static str, Vec<f64>> =
            Default::default();
        let mut tables_per_type: std::collections::BTreeMap<
            &'static str,
            std::collections::BTreeSet<String>,
        > = Default::default();
        for a in p.atoms.iter().filter(|a| {
            a.effect_type == Some(EffectType::MezResist)
                && a.aspect == Some(Aspect::Res)
                && a.gated != Some(true)
        }) {
            let (Some(ty), Some(s)) = (mez_type_of(a), a.scale) else {
                continue;
            };
            let is_res_boolean = a
                .modifier_table
                .as_deref()
                .is_some_and(|t| t.to_lowercase().contains("res_boolean"));
            if KB_TYPES.contains(&ty)
                && (a.to_who != Some(coh_data::atom::ToWho::Self_) || is_res_boolean)
            {
                // foe-facing KB-resistance → converter skips (`!isSelfTargeting`); self-facing
                // Res_Boolean KB → converter routes to effects[kb] PROTECTION, not mezResistance.
                continue;
            }
            atom_scales.entry(ty).or_default().push(s.abs());
            tables_per_type
                .entry(ty)
                .or_default()
                .insert(a.modifier_table.as_deref().unwrap_or("").to_lowercase());
        }
        let atom_sum: std::collections::BTreeMap<&'static str, f64> = atom_scales
            .iter()
            .map(|(ty, v)| {
                let value = if KB_TYPES.contains(ty) {
                    // overwrite / single value (all equal in the corpus)
                    v.iter().cloned().fold(0.0_f64, f64::max)
                } else {
                    // accumulate: sum all |scale|
                    v.iter().sum()
                };
                (*ty, value)
            })
            .collect();
        for (ty, tabs) in &tables_per_type {
            if tabs.len() > 1 {
                divergences.push(format!("  MULTI-TABLE [{ty}] {} tables={tabs:?}", p.name));
            }
        }
        let bag_mez = bag(p)
            .and_then(|b| b.get("mezResistance"))
            .and_then(Value::as_object);
        // For a KB-resistance bag entry, dump every aspect=Res atom with a KB subType (any
        // EffectType) — the KB-resistance encoding is the migration's open question.
        if let Some(bm) = bag_mez {
            for kb in ["knockback", "knockup", "repel"] {
                if bm.get(kb).is_none() {
                    continue;
                }
                let kb_atoms: Vec<String> = p
                    .atoms
                    .iter()
                    .filter(|a| {
                        a.aspect == Some(Aspect::Res)
                            && matches!(
                                a.sub_type,
                                Some(SubType::Knockback)
                                    | Some(SubType::Knockup)
                                    | Some(SubType::Repel)
                            )
                    })
                    .map(atom_desc)
                    .collect();
                if kb_rows.len() < 20 {
                    kb_rows.push(format!(
                        "  [{kb}] {}  bag={}  kb-res-atoms={:?}",
                        p.name,
                        bm.get(kb).unwrap(),
                        kb_atoms
                    ));
                }
            }
        }
        // Per-type equivalence, both directions.
        let bag_types: std::collections::BTreeSet<String> = bag_mez
            .map(|o| o.keys().map(|k| k.to_ascii_lowercase()).collect())
            .unwrap_or_default();
        // bag→atom (occurrence + match, and bag-only)
        if let Some(bm) = bag_mez {
            for (k, v) in bm {
                let ty = k.to_ascii_lowercase();
                let bag_scale = v
                    .as_f64()
                    .or_else(|| v.get("scale").and_then(Value::as_f64))
                    .unwrap_or(0.0);
                let entry = per_type.entry(ty.clone()).or_insert((0, 0));
                entry.0 += 1;
                let asum = atom_sum.get(ty.as_str()).copied();
                let matched = asum.is_some_and(|s| (s - bag_scale).abs() < 1e-9);
                if matched {
                    entry.1 += 1;
                } else if ROUTED_MEZ_RES_TYPES.contains(&ty.as_str()) {
                    match asum {
                        None => bag_only.push(format!(
                            "  BAG-ONLY [{ty}] {} bag_scale={bag_scale}",
                            p.name
                        )),
                        Some(s) => divergences.push(format!(
                            "  VALUE [{ty}] {} bag_scale={bag_scale} atom_sum={s} distinct={:?}",
                            p.name,
                            atom_scales.get(ty.as_str())
                        )),
                    }
                }
            }
        }
        // atom→bag phantom: a routed atom-type value with no bag entry.
        for (ty, s) in &atom_sum {
            if ROUTED_MEZ_RES_TYPES.contains(ty) && !bag_types.contains(*ty) {
                phantom.push(format!("  PHANTOM [{ty}] {} atom_sum={s}", p.name));
            }
        }
    }
    println!("  -- MezResist atom aspect distribution: {mezresist_aspect:?}");
    println!("  -- aspect=Res MezResist subType distribution: {mezresist_res_sub:?}");
    println!("  -- per bag-type (occurrences, atom-sum == bag-scale):");
    for (k, (n, m)) in &per_type {
        let routed = if ROUTED_MEZ_RES_TYPES.contains(&k.as_str()) {
            "routed"
        } else {
            "DROPPED"
        };
        println!("       {k} ({routed}): {m}/{n}");
    }
    println!(
        "  -- routed BAG-ONLY (keep bag fallback): {}",
        bag_only.len()
    );
    for m in bag_only.iter().take(20) {
        println!("{m}");
    }
    println!("  -- routed PHANTOM (gate-RED risk): {}", phantom.len());
    for m in phantom.iter().take(20) {
        println!("{m}");
    }
    println!("  -- routed VALUE divergences: {}", divergences.len());
    for m in divergences.iter().take(20) {
        println!("{m}");
    }
    println!("  -- KB-resistance encoding samples:");
    for m in kb_rows.iter().take(20) {
        println!("{m}");
    }
}

// ---- ATOM3 stealth radius (bag-vs-atom value equivalence) --------------------------
// BRIDGE-2 typed the three stealth attribs `Stealth/{RadiusPvE,RadiusPvP,Translucency}`.
// This census proves the ATOM3 reader reproduces the bag's (pve, pvp, stackKey) EXACTLY,
// so pointing the applier at atoms is behavior-preserving (the totals/oracle gate reads the
// same bag, so any divergence turns it RED). Fold: the converter OVERWRITES
// (`effects.stealth[axis] = makeEffect()`, `convert-powerset.cjs:7137`), so the reader takes
// the LAST atom of each axis (last-write-wins). stackKey: the converter sets it from any
// STEALTH_TYPES atom whose stacking is Suppress and whose key is resolved (`:4692-4696`).
fn stealth_axis_of(a: &AtomicEffect) -> Option<&'static str> {
    match a.sub_type? {
        SubType::RadiusPvE => Some("pve"),
        SubType::RadiusPvP => Some("pvp"),
        SubType::Translucency => Some("translucency"),
        _ => None,
    }
}
// The bag's empty/sentinel-normalized suppress key (mirrors the retired bag's stealth() + converter :4693).
fn resolved_key(a: &AtomicEffect) -> Option<&str> {
    let k = a.stack_key.as_deref()?;
    if k.is_empty() || k == "4294967295" || k == "0" {
        return None;
    }
    Some(k)
}
fn stealth(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM3 stealth — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut shape: std::collections::BTreeMap<String, usize> = Default::default();
    // per axis: (bag-present occurrences, atom==bag matches)
    let mut per_axis: std::collections::BTreeMap<&'static str, (usize, usize)> = Default::default();
    let mut key_total = 0usize;
    let mut key_match = 0usize;
    let mut bag_only: Vec<String> = Vec::new(); // bag has axis, no atom for it
    let mut phantom: Vec<String> = Vec::new(); // atom axis value, no bag entry
    let mut divergences: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let bag_stealth = bag(p)
            .and_then(|b| b.get("stealth"))
            .and_then(Value::as_object);
        // Non-gated Stealth atoms (mirrors base_atoms_of_type).
        let stealth_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Stealth) && a.gated != Some(true))
            .collect();
        if bag_stealth.is_none() && stealth_atoms.is_empty() {
            continue;
        }
        for a in &stealth_atoms {
            incr(
                &mut shape,
                format!(
                    "sub={:?} stacking={:?}",
                    a.sub_type.map(|s| s.as_wire()),
                    a.stacking.map(|x| x.as_wire())
                ),
            );
        }
        // atom-derived: LAST atom of each axis wins (converter overwrite), value = |scale|
        // (converter `makeEffect` does `Math.abs`, :4088), as (|scale|, table).
        let atom_axis = |axis: &str| -> Option<(f64, Option<String>)> {
            stealth_atoms
                .iter()
                .rfind(|a| stealth_axis_of(a) == Some(axis))
                .and_then(|a| {
                    a.scale
                        .map(|s| (s.abs(), a.modifier_table.as_deref().map(str::to_string)))
                })
        };
        // atom-derived stackKey: last Suppress-stacking atom with a resolved key.
        let atom_key: Option<String> = stealth_atoms
            .iter()
            .filter(|a| a.stacking == Some(Stacking::Suppress))
            .filter_map(|a| resolved_key(a).map(str::to_string))
            .next_back();

        let bag_axis = |axis: &str| -> Option<(f64, Option<String>)> {
            let key = if axis == "pve" {
                "stealthPvE"
            } else {
                "stealthPvP"
            };
            let v = bag_stealth?.get(key)?;
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            let table = v.get("table").and_then(Value::as_str).map(str::to_string);
            Some((scale, table))
        };
        let bag_key: Option<String> = bag_stealth
            .and_then(|o| o.get("stackKey"))
            .and_then(Value::as_str)
            .filter(|k| !k.is_empty())
            .map(str::to_string);

        for axis in ["pve", "pvp"] {
            let b = bag_axis(axis);
            let a = atom_axis(axis);
            let entry = per_axis.entry(axis).or_insert((0, 0));
            match (&b, &a) {
                (Some((bs, bt)), Some((as_, at))) => {
                    entry.0 += 1;
                    if (bs - as_).abs() < 1e-9 && bt == at {
                        entry.1 += 1;
                    } else {
                        divergences.push(format!(
                            "  VALUE [{axis}] {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                            p.name
                        ));
                    }
                }
                (Some((bs, _)), None) => {
                    entry.0 += 1;
                    bag_only.push(format!("  BAG-ONLY [{axis}] {} bag_scale={bs}", p.name));
                }
                (None, Some((as_, _))) => {
                    phantom.push(format!("  PHANTOM [{axis}] {} atom_scale={as_}", p.name));
                }
                (None, None) => {}
            }
        }
        // stackKey correlation (only where the bag records a power at all).
        if bag_stealth.is_some() {
            key_total += 1;
            if bag_key == atom_key {
                key_match += 1;
            } else {
                divergences.push(format!(
                    "  KEY {} bag={bag_key:?} atom={atom_key:?}",
                    p.name
                ));
            }
        }
    }
    println!("  -- Stealth atom (subType, stacking) distribution:");
    for (k, n) in &shape {
        println!("       {n:>4}  {k}");
    }
    for (axis, (n, m)) in &per_axis {
        println!("  -- axis {axis}: {m}/{n} atom==bag (scale+table)");
    }
    println!("  -- stackKey: {key_match}/{key_total} atom==bag");
    println!("  -- BAG-ONLY (keep bag fallback): {}", bag_only.len());
    for m in bag_only.iter().take(20) {
        println!("{m}");
    }
    println!("  -- PHANTOM (gate-RED risk): {}", phantom.len());
    for m in phantom.iter().take(20) {
        println!("{m}");
    }
    println!("  -- DIVERGENCES: {}", divergences.len());
    for m in divergences.iter().take(30) {
        println!("{m}");
    }
}

// ---- ATOM4 elusivity — NOT a migration (data-bug finding) ---------------------------
// The audit hypothesized elusivity as a bag→atom migration (read the 268/195 `Elusivity`
// atoms). This census PROVED that wrong on two counts, so ATOM4 is resolved as a data bug,
// not a migration:
//   * The `Elusivity`-typed atoms are aspect=Str, positional (Melee/Ranged/AoE), 0.1 on
//     Melee_Ones — the real PvP-defense stat, a DIFFERENT mechanic the calc never consumes.
//     Reading them phantoms 24/29 defense powers AND is the wrong value.
//   * The only powers that ever carried an `effects.elusivity` bag entry were Foresight +
//     Widow-Teamwork Elude (HC only), and that entry was HAND-AUTHORED OVERRIDE hardcode
//     byte-copied from each power's own `debuffResistance.defense` (`Base_Defense@Resistance`,
//     the `Defense aspect=Res` atom) — so the calc summed it twice into debuff_resist_defense
//     (a 2× DDR double-count). The binary grants DDR ONCE; retired 2026-07-18.
// Post-retirement this census reads 0 bag-visible in every dataset. The phantom-quantification
// (Defense/aspect=Res atoms carry the SAME value with no bag entry — 31 HC / 34 Reb) is the
// proof that no typed atom field could ever have isolated the elusivity source: a bridge
// collision with no distinguishing signal. Kept as the standing evidence for the finding.
fn elusivity(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM4 elusivity — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut shape_bagvisible: std::collections::BTreeMap<String, usize> = Default::default();
    let mut shape_other: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bagvisible_samples: Vec<String> = Vec::new();
    let mut other_samples: Vec<String> = Vec::new();
    let mut n_bagvisible_powers = 0usize;
    let mut n_other_powers = 0usize;
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let elus_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Elusivity))
            .collect();
        if elus_atoms.is_empty() {
            continue;
        }
        let bag_elus = bag(p)
            .and_then(|b| b.get("elusivity"))
            .and_then(Value::as_object);
        let bag_visible = bag_elus.is_some_and(|o| !o.is_empty());
        if bag_visible {
            n_bagvisible_powers += 1;
        } else {
            n_other_powers += 1;
        }
        for a in &elus_atoms {
            let shape = format!(
                "aspect={:?} sub={:?} toWho={:?} gated={} table={}",
                a.aspect.map(|x| x.as_wire()),
                a.sub_type.map(|s| s.as_wire()),
                a.to_who.map(|x| x.as_wire()),
                if a.gated == Some(true) { "Y" } else { "n" },
                table_kind(a),
            );
            if bag_visible {
                incr(&mut shape_bagvisible, shape);
                if bagvisible_samples.len() < 20 {
                    bagvisible_samples.push(format!("  {} :: {}", p.name, atom_desc(a)));
                }
            } else {
                incr(&mut shape_other, shape);
                if other_samples.len() < 20 {
                    other_samples.push(format!("  {} :: {}", p.name, atom_desc(a)));
                }
            }
        }
    }
    // PHANTOM QUANTIFICATION: the bag `elusivity.all` VALUE is a Defense/All/aspect=Res
    // Res_Boolean atom (BRIDGE-1), byte-identical to plain defense-DDR. If the migration read
    // THAT atom as elusivity, every defense-DDR power with no elusivity bag entry phantoms.
    let mut ddr_atom_powers = 0usize; // powers with a base Defense/All/aspect=Res Res_Boolean atom
    let mut ddr_with_elus_bag = 0usize; // …of which also carry an elusivity bag entry
    {
        let mut seen2: std::collections::BTreeSet<String> = Default::default();
        for p in db.all_powers() {
            let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
            if !seen2.insert(id) {
                continue;
            }
            let has_ddr_atom = p.atoms.iter().any(|a| {
                a.effect_type == Some(EffectType::Defense)
                    && a.aspect == Some(Aspect::Res)
                    && a.gated != Some(true)
                    && a.modifier_table
                        .as_deref()
                        .is_some_and(|t| t.to_lowercase().contains("res_boolean"))
            });
            if !has_ddr_atom {
                continue;
            }
            ddr_atom_powers += 1;
            if bag(p)
                .and_then(|b| b.get("elusivity"))
                .and_then(Value::as_object)
                .is_some_and(|o| !o.is_empty())
            {
                ddr_with_elus_bag += 1;
            }
        }
    }
    println!(
        "  -- PHANTOM if reading Defense/aspect=Res as elusivity: {} DDR-atom powers, only {} have an elusivity bag → {} phantoms",
        ddr_atom_powers,
        ddr_with_elus_bag,
        ddr_atom_powers - ddr_with_elus_bag
    );
    println!("  -- powers with an Elusivity atom: {n_bagvisible_powers} bag-visible, {n_other_powers} atom-only (no bag)");
    println!("  -- Elusivity atom shape on BAG-VISIBLE powers (Foresight/Elude):");
    for (k, n) in &shape_bagvisible {
        println!("       {n:>4}  {k}");
    }
    for s in &bagvisible_samples {
        println!("{s}");
    }
    println!("  -- Elusivity atom shape on ATOM-ONLY powers (potential phantoms):");
    for (k, n) in &shape_other {
        println!("       {n:>4}  {k}");
    }
    for s in other_samples.iter().take(20) {
        println!("{s}");
    }
}

// ---- ATOM5 perception (bag-vs-atom value equivalence) ------------------------------
// The applier reads `bag.perceptionBuff` (apply.rs:394): `resolveScaledEffect(scale,table) × 100`,
// gated `value > 0`, into `perception_radius`. The converter routes the `perceptionradius` attrib
// (STEALTH_TYPES): aspect=resistance → debuffResistance.perception (ATOM13); isDebuff||scale<0 →
// perceptionDebuff (M4); else → effects.perceptionBuff = makeEffect() (OVERWRITE / last-write-wins,
// `Math.abs` on scale). Migration = read the matching `Perception` buff-face atom instead. This
// census tests the ONLY thing that keeps the totals gate green: does the atom (last, |scale|, table)
// EXACTLY equal the bag perceptionBuff (scale, table), both directions (bag-only + phantom)? It also
// dumps the aspect distribution so the reader gates on the SPECIFIC positive aspect (the ATOM1
// lesson: `!= Res` can admit mis-decoded phantom atoms; prefer the tight gate).
fn is_debuff_like_perc(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}
fn perception(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM5 perception — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    // aspect distribution of Perception atoms, split by the bag slot the power carries.
    let mut aspect_by_slot: std::collections::BTreeMap<String, usize> = Default::default();
    // (bag-present buff occurrences, atom==bag matches) for the converter-literal gate.
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value (keep bag fallback)
    let mut phantom_ne_res = 0usize; // PHANTOM under `aspect != Res` gate
    let mut phantom_str = 0usize; // PHANTOM under tight `aspect == Str` gate
    let mut value_match = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let b = bag(p);
        let bag_buff = b.and_then(|b| b.get("perceptionBuff"));
        let has_debuff = b.and_then(|b| b.get("perceptionDebuff")).is_some();
        let has_dr = b
            .and_then(|b| b.get("debuffResistance"))
            .and_then(|d| d.get("perception"))
            .is_some();
        let perc_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Perception) && a.gated != Some(true))
            .collect();
        // Aspect census, tagged with which bag slot(s) the power carries.
        let slot = if bag_buff.is_some() {
            "BUFF"
        } else if has_debuff {
            "debuff"
        } else if has_dr {
            "debuffRes"
        } else {
            "atom-only"
        };
        for a in &perc_atoms {
            incr(
                &mut aspect_by_slot,
                format!(
                    "[{slot}] aspect={:?} debuff={} {}",
                    a.aspect.map(|x| x.as_wire()),
                    if is_debuff_like_perc(a) { "Y" } else { "n" },
                    table_kind(a),
                ),
            );
        }
        // Reader simulation: converter-literal buff gate (aspect != Res, non-debuff), OVERWRITE
        // fold → the LAST such atom (|scale|, table).
        let buff_atom = perc_atoms
            .iter()
            .rfind(|a| a.aspect != Some(Aspect::Res) && !is_debuff_like_perc(a));
        let atom_v: Option<(f64, Option<String>, Option<&'static str>)> = buff_atom.and_then(|a| {
            a.scale.map(|s| {
                (
                    s.abs(),
                    a.modifier_table.as_deref().map(str::to_string),
                    a.aspect.map(|x| x.as_wire()),
                )
            })
        });
        let bag_v: Option<(f64, Option<String>)> = bag_buff.and_then(|v| {
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            Some((
                scale,
                v.get("table").and_then(Value::as_str).map(str::to_string),
            ))
        });
        // Tight `aspect == Str` variant — for the phantom comparison only.
        let buff_atom_str = perc_atoms
            .iter()
            .rfind(|a| a.aspect == Some(Aspect::Str) && !is_debuff_like_perc(a));
        match (&bag_v, &atom_v) {
            (Some((bs, bt)), Some((as_, at, asp))) => {
                bag_present += 1;
                bag_and_atom += 1;
                if (bs - as_).abs() < 1e-9 && bt == at {
                    value_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag=({bs},{bt:?}) atom=({as_},{at:?},aspect={asp:?})",
                        p.name
                    ));
                }
                if samples.len() < 12 {
                    samples.push(format!(
                        "  {} bag=({bs},{bt:?}) atom=({as_},{at:?},aspect={asp:?})",
                        p.name
                    ));
                }
            }
            (Some((bs, _)), None) => {
                bag_present += 1;
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag_scale={bs}", p.name));
            }
            (None, Some((as_, _, asp))) => {
                phantom_ne_res += 1;
                if buff_atom_str.is_some() {
                    phantom_str += 1;
                }
                divergences.push(format!(
                    "  PHANTOM {} atom_scale={as_} aspect={asp:?}",
                    p.name
                ));
            }
            (None, None) => {}
        }
    }
    println!("  -- Perception atom aspect/table by bag-slot:");
    for (k, n) in &aspect_by_slot {
        println!("       {n:>4}  {k}");
    }
    println!("  bag.perceptionBuff powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom==bag (scale+table) among with-atom: {value_match}/{bag_and_atom}");
    println!("  PHANTOM under `aspect != Res` gate: {phantom_ne_res}   under tight `aspect == Str`: {phantom_str}");
    println!("  -- samples:");
    for s in &samples {
        println!("{s}");
    }
    if !divergences.is_empty() {
        println!("  -- divergences/blockers ({}):", divergences.len());
        for m in divergences.iter().take(40) {
            println!("{m}");
        }
    }
}

// ---- ATOM6 range (bag-vs-atom value equivalence) -----------------------------------
// The applier reads `bag.rangeBuff` (apply.rs:434): `resolveScaledEffect(scale,table) × 100`,
// gated `value > 0` AND `is_self(power)` (the power's targetType — a SEPARATE consumer gate,
// applied identically to both sources, so it does NOT affect atom==bag slot equivalence). The
// converter routes the `range` attrib (convert-powerset.cjs:7074): aspect=resistance →
// debuffResistance.range (ATOM13); isDebuff||scale<0 → self rangeDebuff (M4) else foe-drop;
// else self-targeting (`_target === 'Self'` → toWho=Self) OR aspect=strength (ally/team +Range,
// Power of the Depths) → effects.rangeBuff = makeEffect() (OVERWRITE / last-write-wins, `Math.abs`
// on scale). Migration = read the matching `Range` buff-face atom instead. This census tests the
// ONLY thing that keeps the totals gate green: does the atom (last, |scale|, table) EXACTLY equal
// the bag rangeBuff (scale, table), both directions (bag-only + phantom)?
fn is_debuff_like_range(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}
fn range(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM6 range — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    // aspect/toWho distribution of Range buff-face atoms (which converter branch they hit).
    let mut buff_shape: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value (keep bag fallback)
    let mut phantom = 0usize; // PHANTOM: applier→value where beta→0 (gate-RED risk)
    let mut value_match = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let bag_buff = bag(p).and_then(|b| b.get("rangeBuff"));
        // Reader simulation: the converter's rangeBuff gate on non-gated Range atoms —
        //   aspect != Res  AND  !isDebuff  AND  (toWho == Self  OR  aspect == Str)
        // OVERWRITE fold → the LAST such atom (|scale|, table).
        let range_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Range) && a.gated != Some(true))
            .collect();
        let buff_atoms: Vec<&&AtomicEffect> = range_atoms
            .iter()
            .filter(|a| {
                a.aspect != Some(Aspect::Res)
                    && !is_debuff_like_range(a)
                    && (a.to_who == Some(coh_data::atom::ToWho::Self_)
                        || a.aspect == Some(Aspect::Str))
            })
            .collect();
        for a in &buff_atoms {
            incr(
                &mut buff_shape,
                format!(
                    "aspect={:?} toWho={:?} {}",
                    a.aspect.map(|x| x.as_wire()),
                    a.to_who.map(|x| x.as_wire()),
                    table_kind(a),
                ),
            );
        }
        let atom_v: Option<(f64, Option<String>)> = buff_atoms.iter().next_back().and_then(|a| {
            a.scale
                .map(|s| (s.abs(), a.modifier_table.as_deref().map(str::to_string)))
        });
        let bag_v: Option<(f64, Option<String>)> = bag_buff.and_then(|v| {
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            Some((
                scale,
                v.get("table").and_then(Value::as_str).map(str::to_string),
            ))
        });
        match (&bag_v, &atom_v) {
            (Some((bs, bt)), Some((as_, at))) => {
                bag_present += 1;
                bag_and_atom += 1;
                if (bs - as_).abs() < 1e-9 && bt == at {
                    value_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
                if samples.len() < 12 {
                    samples.push(format!(
                        "  {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
            }
            (Some((bs, _)), None) => {
                bag_present += 1;
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag_scale={bs}", p.name));
            }
            (None, Some((as_, _))) => {
                phantom += 1;
                divergences.push(format!("  PHANTOM {} atom_scale={as_}", p.name));
            }
            (None, None) => {}
        }
    }
    println!("  -- Range buff-face atom (aspect, toWho, table) distribution:");
    for (k, n) in &buff_shape {
        println!("       {n:>4}  {k}");
    }
    println!("  bag.rangeBuff powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom==bag (scale+table) among with-atom: {value_match}/{bag_and_atom}");
    println!("  PHANTOM (gate-RED risk): {phantom}");
    println!("  -- samples:");
    for s in &samples {
        println!("{s}");
    }
    if !divergences.is_empty() {
        println!("  -- divergences/blockers ({}):", divergences.len());
        for m in divergences.iter().take(40) {
            println!("{m}");
        }
    }
}

// ---- ATOM7 enduranceDiscount (bag-vs-atom value equivalence) ------------------------
// The applier reads `bag.enduranceDiscount` (apply.rs:428): `resolveScaledEffect(scale,table) ×
// 100`, gated `discount > 0` (a SEPARATE consumer gate, applied identically to both sources, so
// it does NOT affect atom==bag slot equivalence), routed to the canonical `endurance` (EndDisc)
// accumulator. The converter routes the `EnduranceDiscount` attrib (convert-powerset.cjs:6897)
// UNCONDITIONALLY — no aspect/debuff/self sub-branch — `effects.enduranceDiscount = makeEffect()`
// (OVERWRITE / last-write-wins, `Math.abs` on scale). The endurance debuff-resistance is a
// DIFFERENT attrib (`Endurance` resource, aspect=resistance → debuffResistance.endurance, ATOM13),
// so there is no aspect collision on `EnduranceDiscount`. Migration = read the `EnduranceDiscount`
// atom instead. This census tests the ONLY thing that keeps the totals gate green: does the atom
// (last, |scale|, table) EXACTLY equal the bag enduranceDiscount (scale, table), both directions?
fn endurance_discount(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM7 enduranceDiscount — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    // aspect/toWho distribution of EnduranceDiscount atoms (the converter is unconditional, so
    // this is diagnostic — it must NOT change the gate, only reveal the shape).
    let mut shape: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value (keep bag fallback)
    let mut phantom = 0usize; // PHANTOM: applier→value where beta→0 (gate-RED risk)
    let mut value_match = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let bag_slot = bag(p).and_then(|b| b.get("enduranceDiscount"));
        // Reader simulation: the converter's UNCONDITIONAL enduranceDiscount write on non-gated
        // EnduranceDiscount atoms. OVERWRITE fold → the LAST such atom (|scale|, table).
        let disc_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| {
                a.effect_type == Some(EffectType::EnduranceDiscount) && a.gated != Some(true)
            })
            .collect();
        for a in &disc_atoms {
            incr(
                &mut shape,
                format!(
                    "aspect={:?} toWho={:?} {}",
                    a.aspect.map(|x| x.as_wire()),
                    a.to_who.map(|x| x.as_wire()),
                    table_kind(a),
                ),
            );
        }
        let atom_v: Option<(f64, Option<String>)> = disc_atoms.iter().next_back().and_then(|a| {
            a.scale
                .map(|s| (s.abs(), a.modifier_table.as_deref().map(str::to_string)))
        });
        let bag_v: Option<(f64, Option<String>)> = bag_slot.and_then(|v| {
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            Some((
                scale,
                v.get("table").and_then(Value::as_str).map(str::to_string),
            ))
        });
        match (&bag_v, &atom_v) {
            (Some((bs, bt)), Some((as_, at))) => {
                bag_present += 1;
                bag_and_atom += 1;
                if (bs - as_).abs() < 1e-9 && bt == at {
                    value_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
                if samples.len() < 12 {
                    samples.push(format!(
                        "  {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
            }
            (Some((bs, _)), None) => {
                bag_present += 1;
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag_scale={bs}", p.name));
            }
            (None, Some((as_, _))) => {
                phantom += 1;
                divergences.push(format!("  PHANTOM {} atom_scale={as_}", p.name));
            }
            (None, None) => {}
        }
    }
    println!("  -- EnduranceDiscount atom (aspect, toWho, table) distribution:");
    for (k, n) in &shape {
        println!("       {n:>4}  {k}");
    }
    println!("  bag.enduranceDiscount powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom==bag (scale+table) among with-atom: {value_match}/{bag_and_atom}");
    println!("  PHANTOM (gate-RED risk): {phantom}");
    println!("  -- samples:");
    for s in &samples {
        println!("{s}");
    }
    if !divergences.is_empty() {
        println!("  -- divergences/blockers ({}):", divergences.len());
        for m in divergences.iter().take(40) {
            println!("{m}");
        }
    }
}

// ---- ATOM8 maxEndurance (bag-vs-atom value equivalence) ----------------------------
// The applier reads `bag.maxEndBuff` (apply.rs:397): `resolveScaledEffect(scale,table) ×
// enhMultiplier` (absolute endurance POINTS, NO ×100), accumulated into `maxEndurance`. The
// converter routes the `Endurance` RESOURCE attrib with `aspect === 'maximum'`
// (convert-powerset.cjs:6907): `addOrAccumulate('maxEndBuff')` — the RESOURCE-SUM fold
// (foldResourceSlot: Σ|scale| while the table holds, RESET on a table change, last-table-wins),
// NOT the OVERWRITE of range/perception/endDisc. UNLIKE regen/recovery there is NO
// enhanceable/unenhanceable twin: every max-end atom (IgnoreStrength or not) lands in the single
// `maxEndBuff` slot. The bridge folds `endurance@maximum` to `EffectType::MaxEndurance`
// (scripts/_atomic-effect.ts), so the census filters that type. Migration = read those atoms with the
// resource-sum fold, twin collapsed. This tests: does the atom sum EXACTLY equal the bag maxEndBuff?
fn max_endurance(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM8 maxEndurance — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut shape: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value (keep bag fallback)
    let mut phantom = 0usize; // PHANTOM: applier→value where beta→0 (gate-RED risk)
    let mut value_match = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let bag_slot = bag(p).and_then(|b| b.get("maxEndBuff"));
        // Reader simulation: MaxEndurance (= endurance@maximum, bridge-folded) non-gated, notOnCaster
        // != true. NO isDebuff filter — the converter's maxEndBuff branch (aspect==maximum) runs BEFORE
        // the isDebuff/drain branch, so a NEGATIVE max-end atom (Burnout −25) lands here too, abs'd to
        // +25 (the ATOM7 unconditional-branch lesson). Twin COLLAPSED (no ignore_strength split).
        // Expression → PUNT (bag). Fold = resource-sum (Σ|scale| reset-on-table-change).
        let is_debuff_like = |a: &AtomicEffect| {
            a.scale.is_some_and(|s| s < 0.0)
                || a.modifier_table
                    .as_deref()
                    .is_some_and(|t| t.to_lowercase().contains("debuff"))
        };
        let atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| {
                a.effect_type == Some(EffectType::MaxEndurance)
                    && a.gated != Some(true)
                    && a.not_on_caster != Some(true)
                    // Drop foe-facing debuffs (Soul Consumption's −1 Target drain) — not a caster
                    // buff. Self-debuffs (Burnout −25) and ally-Target buffs (Power of the Depths
                    // +20) are KEPT — the converter's standard foe-drop.
                    && !(a.to_who == Some(coh_data::atom::ToWho::Target) && is_debuff_like(a))
            })
            .collect();
        for a in &atoms {
            incr(
                &mut shape,
                format!(
                    "aspect={:?} toWho={:?} ignStr={:?} perTgt={:?} {} {}",
                    a.aspect.map(|x| x.as_wire()),
                    a.to_who.map(|x| x.as_wire()),
                    a.ignore_strength,
                    a.per_target.map(|v| v != 0.0),
                    if a.attrib_type == Some(coh_data::atom::AttribType::Expression) {
                        "EXPR"
                    } else {
                        ""
                    },
                    table_kind(a),
                ),
            );
        }
        let punt = atoms
            .iter()
            .any(|a| a.attrib_type == Some(coh_data::atom::AttribType::Expression));
        let atom_v: Option<(f64, Option<String>)> = if atoms.is_empty() || punt {
            None
        } else {
            // fold_resource_sum: Σ|scale| while table unchanged, reset on change, last-table-wins.
            let mut scale = 0.0f64;
            let mut table: Option<&str> = atoms[0].modifier_table.as_deref();
            for a in &atoms {
                if a.modifier_table.as_deref() == table {
                    scale += a.scale.map_or(f64::NAN, f64::abs);
                } else {
                    scale = a.scale.map_or(f64::NAN, f64::abs);
                    table = a.modifier_table.as_deref();
                }
            }
            Some((scale, table.map(str::to_string)))
        };
        let bag_v: Option<(f64, Option<String>)> = bag_slot.and_then(|v| {
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            Some((
                scale,
                v.get("table").and_then(Value::as_str).map(str::to_string),
            ))
        });
        match (&bag_v, &atom_v) {
            (Some((bs, bt)), Some((as_, at))) => {
                bag_present += 1;
                bag_and_atom += 1;
                if (bs - as_).abs() < 1e-9 && bt == at {
                    value_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
                if samples.len() < 12 {
                    samples.push(format!(
                        "  {} bag=({bs},{bt:?}) atom=({as_},{at:?})",
                        p.name
                    ));
                }
            }
            (Some((bs, _)), None) => {
                bag_present += 1;
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag_scale={bs}", p.name));
            }
            (None, Some((as_, _))) => {
                phantom += 1;
                divergences.push(format!("  PHANTOM {} atom_scale={as_}", p.name));
            }
            (None, None) => {}
        }
    }
    println!("  -- MaxEndurance atom (aspect, toWho, ignStr, perTgt, table) distribution:");
    for (k, n) in &shape {
        println!("       {n:>4}  {k}");
    }
    println!("  bag.maxEndBuff powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom==bag (scale+table) among with-atom: {value_match}/{bag_and_atom}");
    println!("  PHANTOM (gate-RED risk): {phantom}");
    println!("  -- samples:");
    for s in &samples {
        println!("{s}");
    }
    if !divergences.is_empty() {
        println!("  -- divergences/blockers ({}):", divergences.len());
        for m in divergences.iter().take(40) {
            println!("{m}");
        }
    }
}

// ---- ATOM9 accuracy (bag-vs-atom value equivalence) --------------------------------
// The applier reads `bag.accuracyBuff` (apply.rs:424): `resolveScaledEffect(scale,table) × 100`,
// via the `stack()` seam, into `accuracy`. The converter routes the `accuracy` combat-modifier
// attrib (convert-powerset.cjs:7037): aspect=resistance → debuffResistance.accuracy (ATOM13);
// isDebuff||scale<0 → accuracyDebuff (M4); else → effects.accuracyBuff = makeEffect() (OVERWRITE /
// last-write-wins, `Math.abs`). Accuracy is a STRENGTH-aspect stat by nature (cvt:5017), and the
// converter's specialBuff block explicitly excludes it (cvt:5020) so it falls through to accuracyBuff
// — so the buff-face atoms are aspect=Str, and the converter-exact gate is `aspect != Res` +
// `!isDebuff`, same as perception. This census tests atom==bag both directions, and compares the
// converter-exact `!= Res` gate against the tighter `== Str` for phantoms (the ATOM1/ATOM5 lesson).
fn is_debuff_like_acc(a: &AtomicEffect) -> bool {
    a.scale.is_some_and(|s| s < 0.0)
        || a.modifier_table
            .as_deref()
            .is_some_and(|t| t.to_lowercase().contains("debuff"))
}
fn accuracy(ds: DatasetId, db: &PowerDatabase) {
    println!("\n########## ATOM9 accuracy — {ds:?} ##########");
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    let mut aspect_by_slot: std::collections::BTreeMap<String, usize> = Default::default();
    let mut bag_present = 0usize;
    let mut bag_and_atom = 0usize;
    let mut bag_only = 0usize; // MIGRATION BLOCKER: applier→0 where beta→value (keep bag fallback)
    let mut phantom_ne_res = 0usize; // PHANTOM under `aspect != Res` gate
    let mut phantom_str = 0usize; // PHANTOM under tight `aspect == Str` gate
    let mut value_match = 0usize;
    let mut divergences: Vec<String> = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for p in db.all_powers() {
        let id = p.internal_name.clone().unwrap_or_else(|| p.name.clone());
        if !seen.insert(id) {
            continue;
        }
        let b = bag(p);
        let bag_buff = b.and_then(|b| b.get("accuracyBuff"));
        let has_debuff = b.and_then(|b| b.get("accuracyDebuff")).is_some();
        let has_dr = b
            .and_then(|b| b.get("debuffResistance"))
            .and_then(|d| d.get("accuracy"))
            .is_some();
        let acc_atoms: Vec<&AtomicEffect> = p
            .atoms
            .iter()
            .filter(|a| a.effect_type == Some(EffectType::Accuracy) && a.gated != Some(true))
            .collect();
        let slot = if bag_buff.is_some() {
            "BUFF"
        } else if has_debuff {
            "debuff"
        } else if has_dr {
            "debuffRes"
        } else {
            "atom-only"
        };
        for a in &acc_atoms {
            incr(
                &mut aspect_by_slot,
                format!(
                    "[{slot}] aspect={:?} debuff={} {}",
                    a.aspect.map(|x| x.as_wire()),
                    if is_debuff_like_acc(a) { "Y" } else { "n" },
                    table_kind(a),
                ),
            );
        }
        // Reader simulation: converter-literal buff gate (aspect != Res, non-debuff), OVERWRITE fold.
        let buff_atom = acc_atoms
            .iter()
            .rfind(|a| a.aspect != Some(Aspect::Res) && !is_debuff_like_acc(a));
        let atom_v: Option<(f64, Option<String>, Option<&'static str>)> = buff_atom.and_then(|a| {
            a.scale.map(|s| {
                (
                    s.abs(),
                    a.modifier_table.as_deref().map(str::to_string),
                    a.aspect.map(|x| x.as_wire()),
                )
            })
        });
        let bag_v: Option<(f64, Option<String>)> = bag_buff.and_then(|v| {
            let scale = v
                .get("scale")
                .and_then(Value::as_f64)
                .or_else(|| v.as_f64())?;
            Some((
                scale,
                v.get("table").and_then(Value::as_str).map(str::to_string),
            ))
        });
        let buff_atom_str = acc_atoms
            .iter()
            .rfind(|a| a.aspect == Some(Aspect::Str) && !is_debuff_like_acc(a));
        match (&bag_v, &atom_v) {
            (Some((bs, bt)), Some((as_, at, asp))) => {
                bag_present += 1;
                bag_and_atom += 1;
                if (bs - as_).abs() < 1e-9 && bt == at {
                    value_match += 1;
                } else {
                    divergences.push(format!(
                        "  VALUE {} bag=({bs},{bt:?}) atom=({as_},{at:?},aspect={asp:?})",
                        p.name
                    ));
                }
                if samples.len() < 12 {
                    samples.push(format!(
                        "  {} bag=({bs},{bt:?}) atom=({as_},{at:?},aspect={asp:?})",
                        p.name
                    ));
                }
            }
            (Some((bs, _)), None) => {
                bag_present += 1;
                bag_only += 1;
                divergences.push(format!("  BAG-ONLY {} bag_scale={bs}", p.name));
            }
            (None, Some((as_, _, asp))) => {
                phantom_ne_res += 1;
                if buff_atom_str.is_some() {
                    phantom_str += 1;
                }
                divergences.push(format!(
                    "  PHANTOM {} atom_scale={as_} aspect={asp:?}",
                    p.name
                ));
            }
            (None, None) => {}
        }
    }
    println!("  -- Accuracy atom aspect/table by bag-slot:");
    for (k, n) in &aspect_by_slot {
        println!("       {n:>4}  {k}");
    }
    println!("  bag.accuracyBuff powers (deduped): {bag_present}  (with-atom {bag_and_atom}, BAG-ONLY {bag_only})");
    println!("  atom==bag (scale+table) among with-atom: {value_match}/{bag_and_atom}");
    println!("  PHANTOM under `aspect != Res` gate: {phantom_ne_res}   under tight `aspect == Str`: {phantom_str}");
    println!("  -- samples:");
    for s in &samples {
        println!("{s}");
    }
    if !divergences.is_empty() {
        println!("  -- divergences/blockers ({}):", divergences.len());
        for m in divergences.iter().take(40) {
            println!("{m}");
        }
    }
}

fn main() {
    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    for ds in DatasetId::ALL {
        let db = load(ds);
        match which.as_str() {
            "absorb" => absorb(ds, &db),
            "protection" => protection(ds, &db),
            "taunt" => taunt(ds, &db),
            "debuffresist" => debuffresist(ds, &db),
            "recharge" => recharge(ds, &db),
            "mezresist" => mezresist(ds, &db),
            "stealth" => stealth(ds, &db),
            "elusivity" => elusivity(ds, &db),
            "perception" => perception(ds, &db),
            "range" => range(ds, &db),
            "endurancediscount" => endurance_discount(ds, &db),
            "maxendurance" => max_endurance(ds, &db),
            "accuracy" => accuracy(ds, &db),
            _ => {
                absorb(ds, &db);
                protection(ds, &db);
                taunt(ds, &db);
                debuffresist(ds, &db);
                recharge(ds, &db);
                mezresist(ds, &db);
                stealth(ds, &db);
                elusivity(ds, &db);
                perception(ds, &db);
                range(ds, &db);
                endurance_discount(ds, &db);
                max_endurance(ds, &db);
                accuracy(ds, &db);
            }
        }
    }
}
