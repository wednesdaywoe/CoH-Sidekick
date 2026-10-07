//! Purple Patch — the combat level-difference scaling tables (base ToHit, combat
//! modifier, defense softcap). Ported (shape, not name) from the beta's per-dataset
//! `datasets/<id>/purple-patch.ts`. Pass 8 (`finalize`) reads these to project the
//! hit-chance / combat-modifier / defense-softcap display stats.
//!
//! Data/calc split (D2): this struct is pure DATA — the tables loaded from the
//! contract's `purple-patch` section. The LOOKUPS (`get_base_to_hit`,
//! `get_combat_modifier`, `get_defense_softcap` — clamped index math) live in
//! `coh_math::purple_patch` (D5). (This differs from [`crate::at_tables`], whose
//! `get_table_value` lookup lives here because it carries table-name normalization;
//! the purple-patch lookups are pure combat-math clamps, so D5 placed them in the
//! calc crate.)
//!
//! Source: the contract's `purple-patch.json` section, emitted by `emit-contract.cjs`
//! by SAMPLING the beta's own `getBaseToHit`/`getCombatModifier`/`getDefenseSoftcap`
//! across their reachable domains (no hand-transcription). The tables are per-dataset
//! in the contract, mirroring the beta's `getActiveDataset().purplePatch` facade —
//! though at this snapshot Rebirth and Thunderspy re-export Homecoming's tables, so the
//! three emitted sections coincide byte-for-byte. Absent section ⇒ empty tables (a
//! hand-constructed `PowerDatabase` needs no purple-patch data); the calc-side lookups
//! return `None` on an empty table rather than inventing a fallback.

use serde::Deserialize;
use serde_json::Value;

/// One dataset's purple-patch scaling tables. Every field is indexed by an absolute
/// (unsigned) level-difference magnitude; the signed-`levelDiff` → branch/index logic
/// lives in the `coh_math` lookups. Values match the beta's tables exactly (they are
/// emitted by sampling the beta's lookups).
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PurplePatch {
    /// Base ToHit when you are 0..=4 levels ABOVE the target (index = levels above).
    pub base_to_hit_above: Vec<f64>,
    /// Base ToHit when you are 0..=7 levels BELOW the target (index = levels below).
    pub base_to_hit_below: Vec<f64>,
    /// Combat modifier (damage/debuff/mez scaling) when 0..=49 levels ABOVE target.
    pub combat_mod_above: Vec<f64>,
    /// Combat modifier when 0..=12 levels BELOW target.
    pub combat_mod_below: Vec<f64>,
    /// Practical defense softcap by enemy level offset (index = levels the enemy is
    /// above the player; negative/zero offsets clamp to index 0).
    pub defense_softcap_by_level_diff: Vec<f64>,
    /// Empirical ToHit buff layered onto enemies in Incarnate-trial content, added to
    /// the level-diff softcap when the content mode is Incarnate.
    pub incarnate_to_hit_buff_pct: f64,
}

impl PurplePatch {
    /// Parse the contract's `purple-patch` section. Malformed ≠ absent (matching
    /// [`crate::at_tables`]): an ABSENT section degrades to empty
    /// tables (the lookups then return `None` — a hand-constructed `PowerDatabase`
    /// needs no purple-patch data), but a PRESENT section that doesn't parse is an
    /// error, never silently the default.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(PurplePatch::default());
        };
        serde_json::from_value(section.clone()).map_err(|e| format!("purple-patch section: {e}"))
    }
}
