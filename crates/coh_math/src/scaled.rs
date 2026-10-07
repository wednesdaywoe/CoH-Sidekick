//! `resolve_scaled_effect` — turn a `{ scale, table }` scaled effect into a number by
//! multiplying the scale through the archetype modifier table. Ported from
//! `character-totals.ts` `resolveScaledEffect`.
//!
//! The M1/M2 appliers pinned the *shape* (`{ scale, table }`); this is where the totals
//! pipeline first resolves it. The `_ones` shortcut and the table-less 0.10 fallback are
//! the calc half of the AT-table split (the lookup itself lives in `coh_data::AtTables`, D2).
//!
//! Faithful to the TS branch order for an OBJECT input (which every atom-derived scaled
//! effect is — an atom always carries a `scale` and a `modifier_table`). In order:
//! `table` present and ends `_ones` → `scale × 1.0` (Ones tables are constant 1.0 for all
//! ATs; skip the lookup); else `table` present and the lookup hits → `scale × value`; else
//! table absent/empty → `scale × 0.10`, the beta's "unknown table" default rate.
//!
//! Where this deliberately DIVERGES from the beta: a **named table that misses the
//! lookup** (a typo'd/dropped AT table, an unknown archetype) used to take the same 0.10
//! fallback — a soft-wrong number shipped as authoritative. That case now contributes
//! nothing and pushes a [`CalcError`] into the caller's sink instead (Rule 1); the
//! corpus never reaches it (every named table resolves — the totals gate pins this).
//! The 0.10 fallback survives only for the genuinely table-less case the beta also
//! resolves that way.
//!
//! The bare-number ScalarOrScaled case (`typeof effect === 'number'`) is not modelled
//! here: no M3 pass feeds `resolve_scaled_effect` a table-less scalar (bag numbers like
//! `tohitBuff` are already resolved). Add it when a pass needs it.

use crate::totals::CalcError;
use coh_data::{PowerDatabase, TableScope};

/// The beta's "unknown table" default rate: a scaled effect whose table is
/// absent resolves at `scale × 0.10`.
const UNKNOWN_TABLE_DEFAULT_RATE: f64 = 0.10;

/// Resolve `scale × table[level]` for `archetype`, with the beta's `_ones` shortcut and
/// table-less 0.10 fallback. `table` is the atom's `modifier_table` (empty string treated
/// as absent, matching the TS `if (effect.table)` truthiness). A NAMED table that misses
/// the lookup pushes a [`CalcError`] into `errors` and contributes `0.0`.
pub fn resolve_scaled_effect(
    scale: f64,
    table: Option<&str>,
    archetype: &str,
    level: i32,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> f64 {
    resolve_scaled_effect_for(
        scale,
        table,
        TableScope::Archetype(archetype),
        level,
        db,
        errors,
    )
}

/// [`resolve_scaled_effect`] against an explicit class. A pseudo-pet's rows resolve against the
/// PET's own class rather than the summoner's — see [`TableScope`] — so a caller holding a pet's
/// effect states which class its table is read under (ENT-10).
pub fn resolve_scaled_effect_for(
    scale: f64,
    table: Option<&str>,
    scope: TableScope<'_>,
    level: i32,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> f64 {
    let Some(t) = table.filter(|t| !t.is_empty()) else {
        return scale * UNKNOWN_TABLE_DEFAULT_RATE;
    };
    if t.to_ascii_lowercase().ends_with("_ones") {
        return scale;
    }
    // No class selected: every table is keyed by one, so NONE of them can resolve — that is
    // the absence of a selection, not a missing table. It is the state the planner holds
    // before the user picks an archetype, and the auto-granted inherents (Ninja/Beast/Athletic
    // Run, Swift, Hurdle, Health) are already in the build there, so erroring made a fresh
    // planner log one failure per atom per recalc. A named table that misses for a REAL class
    // still fails loud below — that one is a data gap.
    let class = match scope {
        TableScope::Archetype(archetype) => archetype,
        TableScope::Pet(pet_class) => pet_class,
    };
    if class.is_empty() {
        return 0.0;
    }
    match db.at_tables.value(scope, t, level) {
        Some(value) => scale * value,
        None => {
            errors.push(CalcError::new(
                "scaled-effect",
                format!("named table {t:?} has no value for {scope:?} at level {level}"),
            ));
            0.0
        }
    }
}
