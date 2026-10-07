//! `tauntPlacateValue` — the top-level `effects.taunt` / `effects.placate` slots, migrated from
//! the bag (ATOM12 / DATA-GAP-REGISTER PASS2B-14). These are the beta's separate "Additional mez
//! resistance" pair (`character-totals.ts:1907`/`:1927`): taunt/placate RESISTANCE, credited only
//! when the slot carries a `Res_Boolean` table. The converter writes `effects[ctrlType] =
//! makeEffect()` (CONTROL_TYPES, aspect != resistance — an aspect=resistance taunt/placate routes
//! to `mezResistance` instead — OVERWRITE / last-write-wins) from a `Mez` atom whose subType is
//! Taunt/Placate. Migration = read that atom's `{|scale|, table}` instead.
//!
//! Behavior-preserving: the atom value equals the bag slot exactly across the corpus (measured 0
//! phantom / 0 divergence on all three datasets; 0 bag-only bar two HC taunt residuals on a
//! non-`Res_Boolean` table — uncredited, served by `?? bag`; see the ATOM12 design spec and the
//! `taunt_placate_atom_bag_parity` guard). UNLIKE ATOM5-8, **Thunderspy carries real Taunt/Placate
//! `Mez` atoms** (10 taunt / 1 placate), so it is not a blanket TSPY-3 residual.
//!
//! Returns the same [`ScaledMez`] `Bag::mez` returns, so the applier wires it as a literal
//! `taunt_placate_value(power, sub) ?? bag.mez(slot)` and the downstream `Res_Boolean` gate +
//! `|scale| × getTableValue(level) × 100` resolution is untouched — this migrates only the SOURCE
//! of the `{scale, table}` pair. The value is the converter's `Math.abs(scale)`, matching the
//! bag's stored magnitude (the raw atom scale is negative — a −30 still grants +30 resistance).

use super::*;
use coh_data::slot_value::ScaledMez;
use coh_data::{Aspect, EffectType, Power, SubType};

/// The `effects.taunt` / `effects.placate` slot this power carries, from the `Mez/<sub>` atom
/// (subType Taunt or Placate). `None` when no qualifying atom exists (→ the `?? bag` fallback), or
/// when the winning atom is table-less (`Bag::mez` likewise requires a table — the curated-armor
/// path keys its Res_Boolean decision on the table name).
pub fn taunt_placate_value(power: &Power, sub: SubType) -> Option<ScaledMez> {
    // The converter's CONTROL fold (TAUNT-1): the power's own row beats a redirect-collected
    // one (`owner_targets` presence is the collection provenance — a collector stamps it
    // exactly when it pulls a template out of another power's file), and among rows of one
    // provenance the LAST non-gated Mez atom of this subType with a non-resistance face wins
    // (aspect=resistance routes to mezResistance). Before TAUNT-1 this was a plain last-write
    // fold, which always showed the redirect-collected inherent punchvoke row.
    let rows: Vec<_> = base_atoms_of_type(power, EffectType::Mez)
        .into_iter()
        .filter(|a| a.sub_type == Some(sub) && a.aspect != Some(Aspect::Res))
        .collect();
    let atom = rows
        .iter()
        .rfind(|a| a.owner_targets.as_deref().is_none_or(|t| t.is_empty()))
        .or(rows.last())?;
    Some(ScaledMez {
        scale: atom.scale?.abs(),
        table: table_of(atom)?.to_string(),
        is_protection: false,
    })
}
