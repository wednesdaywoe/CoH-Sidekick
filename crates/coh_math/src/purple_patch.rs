//! Purple-patch lookups — the calc half of the level-difference scaling tables. The
//! DATA (the tables) lives in [`coh_data::PurplePatch`], loaded from the contract; these
//! are the clamped index lookups over it (D5: "coh_math ports only the lookups"). Pass 8
//! (`finalize`) reads them to project the hit-chance / combat-modifier / defense-softcap
//! display stats.
//!
//! Ports the beta's per-dataset `getBaseToHit` / `getCombatModifier` / `getDefenseSoftcap`
//! (`datasets/<id>/purple-patch.ts`) exactly, including the sign convention: `level_diff`
//! is signed, POSITIVE = the target is higher (you are BELOW it), NEGATIVE or zero = you
//! are above or equal. Each returns `None` when its table is empty (an unloaded
//! `PowerDatabase`) rather than inventing a fallback — the caller decides (Rule 1).

use coh_data::PurplePatch;

/// Content mode for the defense-softcap lookup. Lives in `coh_data` now, not here —
/// [`coh_data::CombatContext`] carries it as a live dashboard input, and `coh_math`
/// depends on `coh_data`, not the reverse. Re-exported so every existing
/// `purple_patch::ContentMode` call site here keeps resolving unchanged.
pub use coh_data::ContentMode;

/// Base ToHit chance for a signed `level_diff`. Ports beta `getBaseToHit`: the ABOVE
/// table indexed by `min(-level_diff, len-1)` when `level_diff <= 0`, else the BELOW
/// table indexed by `min(level_diff, len-1)`.
///
/// The negation saturates because `level_diff` descends from `combat.enemy_level_offset`,
/// an unbounded `i32` off the wire: `-i32::MIN` has no `i32` to be, which panics in a
/// checked build and wraps back to `i32::MIN` in the release wasm — a negative index that
/// `clamped_index` then reads as an enormous `usize` and clamps to the LAST row, the
/// opposite end of the table from where a maximally-below-level target belongs.
pub fn get_base_to_hit(purple_patch: &PurplePatch, level_diff: i32) -> Option<f64> {
    if level_diff <= 0 {
        clamped_index(&purple_patch.base_to_hit_above, level_diff.saturating_neg())
    } else {
        clamped_index(&purple_patch.base_to_hit_below, level_diff)
    }
}

/// Chance to hit, as a fraction: `clamp(clamp(base_to_hit + to_hit_pct/100) × accuracy)`, both
/// clamps `[0.05, 0.95]`. `to_hit_pct` is the ToHit buff in percentage points; `accuracy` is the
/// multiplier (`1.0` = unenhanced, unbuffed). One formula for the build-wide figure and the
/// per-power one, which differ only in which accuracy they pass.
pub fn hit_chance(base_to_hit: f64, to_hit_pct: f64, accuracy: f64) -> f64 {
    let final_to_hit = (base_to_hit + to_hit_pct / 100.0).clamp(0.05, 0.95);
    (final_to_hit * accuracy).clamp(0.05, 0.95)
}

/// Combat modifier (damage / debuff strength / mez duration scaling) for a signed
/// `level_diff`. Ports beta `getCombatModifier`, same branch/clamp shape as
/// [`get_base_to_hit`].
pub fn get_combat_modifier(purple_patch: &PurplePatch, level_diff: i32) -> Option<f64> {
    if level_diff <= 0 {
        clamped_index(&purple_patch.combat_mod_above, level_diff.saturating_neg())
    } else {
        clamped_index(&purple_patch.combat_mod_below, level_diff)
    }
}

/// Practical defense softcap for an enemy `level_diff` and content mode. Ports beta
/// `getDefenseSoftcap`: index `max(0, min(level_diff, len-1))` into the softcap table
/// (negative/zero offsets floor to index 0), plus the incarnate ToHit buff in Incarnate
/// mode.
pub fn get_defense_softcap(
    purple_patch: &PurplePatch,
    level_diff: i32,
    content_mode: ContentMode,
) -> Option<f64> {
    // Negative/zero offsets floor to index 0 (beta `Math.max(0, Math.min(...))`).
    let base = clamped_index(
        &purple_patch.defense_softcap_by_level_diff,
        level_diff.max(0),
    )?;
    Some(match content_mode {
        ContentMode::Standard => base,
        ContentMode::Incarnate => base + purple_patch.incarnate_to_hit_buff_pct,
    })
}

/// Index into `table` at `min(index, len-1)`, matching the beta's `Math.min(index,
/// len-1)`; `None` when the table is empty (an unloaded `PowerDatabase`). `index` is
/// always non-negative at the call sites (a magnitude, not a signed diff).
fn clamped_index(table: &[f64], index: i32) -> Option<f64> {
    let last = table.len().checked_sub(1)?;
    Some(table[(index as usize).min(last)])
}

#[cfg(test)]
mod tests {
    use super::hit_chance;

    #[test]
    fn hit_chance_scales_accuracy_against_the_level_gap() {
        // An even-level foe: 75% base ToHit × 1.2 accuracy.
        assert!((hit_chance(0.75, 0.0, 1.2) - 0.90).abs() < 1e-9);
        // The same power against a +4: 39% base ToHit × 1.2.
        assert!((hit_chance(0.39, 0.0, 1.2) - 0.468).abs() < 1e-9);
        // ToHit adds before accuracy multiplies.
        assert!((hit_chance(0.39, 10.0, 1.2) - 0.588).abs() < 1e-9);
        // Both clamps: ToHit floors at 5%, and the product caps at 95%.
        assert!((hit_chance(0.08, -50.0, 1.0) - 0.05).abs() < 1e-9);
        assert!((hit_chance(0.75, 0.0, 2.0) - 0.95).abs() < 1e-9);
    }
}
