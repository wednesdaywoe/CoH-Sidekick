//! Proc / global-IO effect database — the typed view of the contract's
//! `proc-data` section (the beta's binary-sourced `PROC_DATABASE`, keyed by IO
//! name). The per-proc structured `effects` the totals pipeline needs for the
//! proc pass (`coh_math::procs`): always-on globals (LotG +Recharge, Steadfast
//! +Def), Proc120s (Numina, Miracle), PPM procs in auto/toggle (Performance
//! Shifter), Build-Up procs, and the variable procs (Reactive Defenses, Might of
//! the Tanker).
//!
//! Source: `contract/<dataset>/proc-data.json`, emitted verbatim from the beta's
//! `src/data/proc-data.ts` `PROC_DATABASE` by `emit-contract.cjs` (itself
//! binary-sourced via `scripts/extract-proc-data.py` — Rule 0: the rebuild reads
//! the export, never a hand table). Absent section ⇒ empty database; the
//! calc-side lookups then find nothing and contribute nothing.
//!
//! Data/calc split (D2): this struct is pure DATA. The proc math (PPM chance
//! formula, always-on/PPM/Build-Up/variable apply logic, Rule of 5) lives in
//! `coh_math::procs`.

use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// A proc's activation type. `Global`/`Proc120s` are always-on; `Proc` fires by
/// PPM (or, for Build-Up/variable procs, drives its own averaged model).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ProcType {
    Proc,
    Proc120s,
    Global,
}

/// One structured effect of a proc (the beta `ProcEffect`). Binary-sourced. A
/// proc carries a LIST of these (Aegis = Resistance + MezResist; Winter's Gift =
/// Slow + Recharge resist). `category` is the beta `ProcEffectCategory` string —
/// matched (not enum'd) by `coh_math::procs`, which owns the closed switch.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcEffect {
    /// Effect category (`Recovery`, `Defense`, `Resistance`, `Damage`, …).
    pub category: String,
    /// Value (percentage, flat, or feet for stealth).
    #[serde(default)]
    pub value: Option<f64>,
    /// Max value (damage ranges, stealth PvP radius, HP-scaling cap).
    #[serde(default)]
    pub value_max: Option<f64>,
    /// Damage/effect type (`Fire`, `All`, `Psionic`, …).
    #[serde(default)]
    pub effect_type: Option<String>,
    /// Duration in seconds (timed effects; drives the Build-Up / stacking model).
    #[serde(default)]
    pub duration: Option<f64>,
    /// Effect target. `pets` = buffs the player's pets (MM auras); `foe` =
    /// debuff/mez applied to the enemy. The player-dashboard path skips both.
    /// Omitted = self.
    #[serde(default)]
    pub target: Option<String>,
    /// Trigger chance when < 1 (chance-gated). The always-on path skips these.
    /// Omitted = always on.
    #[serde(default)]
    pub chance: Option<f64>,
    /// True when the value is an HP-scaling floor (Reactive Defenses 3%–12.9%):
    /// `value` is the floor (full HP), `value_max` the cap (near-0 HP).
    #[serde(default)]
    pub scaling: bool,
    /// Max concurrent stacks for a self-stacking buff proc (Might of the Tanker = 3).
    #[serde(default)]
    pub max_stacks: Option<u32>,
    /// AT modifier table for a "By the Slotted Power" effect whose magnitude is
    /// `value × getTableValue(archetype, scale_table, level)`.
    #[serde(default)]
    pub scale_table: Option<String>,
}

/// One proc / global IO entry (the beta `ProcData`). Only the fields the calc
/// reads are captured, plus `mechanics` for the tooltip; the other display-only fields
/// (`pvpNotes`, `pool`, …) are ignored (no `deny_unknown_fields`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcData {
    /// IO set this proc belongs to.
    pub set_name: String,
    /// Proc IO name.
    pub io_name: String,
    /// Procs Per Minute (`null` for Globals / Proc120s).
    #[serde(default)]
    pub ppm: Option<f64>,
    /// The export's one-line account of what the proc does
    /// (`"Buff(Build up (15% ToHit 100% Dam)) for 10s"`). Display only; the calc reads
    /// [`effects`](Self::effects).
    #[serde(default)]
    pub mechanics: Option<String>,
    /// The PIECE's own `fActivatePeriod`, binary-sourced. `CalculateModChance` multiplies
    /// PPM by `ptemplate->ppowBase->fActivatePeriod`, and a proc's templates are iterated
    /// straight off the boost — so this is the enhancement's field, never the host
    /// toggle's. It sets both the per-check chance and the check rate (60 / period).
    /// Absent only where the extractor could not resolve the piece's set in the binary.
    #[serde(default)]
    pub activate_period: Option<f64>,
    /// The PIECE's own `BoostsAllowed`, verbatim: one real boost type plus the five
    /// origins. The routing key for `procRollSites` — `CopyBoosts` filters by the
    /// destination's boost types, and no power's list names an origin, so the origins
    /// ride along harmlessly. Absent where the extractor resolved no piece.
    #[serde(default)]
    pub boosts_allowed: Option<Vec<String>>,
    /// Activation type.
    #[serde(rename = "type")]
    pub proc_type: ProcType,
    /// Structured, binary-sourced effects.
    #[serde(default)]
    pub effects: Vec<ProcEffect>,
}

impl ProcData {
    /// The beta `isProcAlwaysOn`: Global and Proc120s are always active.
    pub fn is_always_on(&self) -> bool {
        matches!(self.proc_type, ProcType::Global | ProcType::Proc120s)
    }
}

/// The proc database keyed by its PROC_DATABASE key (usually `"Set: IO Name"` or a
/// bare IO name). Wraps the fuzzy [`find`](ProcDatabase::find) lookup the beta's
/// `findProcData` performs.
#[derive(Debug, Clone, Default)]
pub struct ProcDatabase {
    entries: BTreeMap<String, ProcData>,
}

impl ProcDatabase {
    /// Build from the contract's `proc-data` section (`Record<key, ProcData>`).
    /// Absent ⇒ empty. An entry that fails to type surfaces loud (never silently
    /// dropped — Rule 1).
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(value) = section else {
            return Ok(Self::default());
        };
        let map = value
            .as_object()
            .ok_or_else(|| "proc-data section is not an object".to_string())?;
        let mut entries = BTreeMap::new();
        for (key, entry) in map {
            let data: ProcData = serde_json::from_value(entry.clone())
                .map_err(|e| format!("proc-data entry {key:?}: {e}"))?;
            entries.insert(key.clone(), data);
        }
        Ok(Self { entries })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every entry, in key order. The universe a surface offering per-category proc controls
    /// derives from — which categories exist is a property of the dataset, not of the build.
    pub fn iter(&self) -> impl Iterator<Item = &ProcData> {
        self.entries.values()
    }

    /// The entry for this piece, or nothing — for a display, which must not take
    /// [`find`](Self::find)'s set-name fallback and show a sibling piece's proc.
    ///
    /// The export names a proc in its own words, often not the piece's ("Recharge/Chance for
    /// Hold" is filed as "Chance for Hold", "Chance to Stun" as "Chance for Disorient"), so
    /// three readings, each one that cannot pick the wrong entry: the same name; a name the
    /// piece's ends with as its last `/` segment, when only one does; and, when the set has a
    /// single proc piece (`sole_proc_in_set`) and the export a single entry for the set, that
    /// pairing. Measured on Homecoming: 56 of 165 proc pieces by name alone, 165 with all three.
    pub fn find_for_piece(
        &self,
        piece_name: &str,
        set_name: &str,
        sole_proc_in_set: bool,
    ) -> Option<&ProcData> {
        let in_set: Vec<&ProcData> = self
            .entries
            .values()
            .filter(|data| data.set_name == set_name)
            .collect();
        if let Some(hit) = in_set.iter().find(|data| data.io_name == piece_name) {
            return Some(hit);
        }
        let suffixed: Vec<&&ProcData> = in_set
            .iter()
            .filter(|data| piece_name.ends_with(&format!("/{}", data.io_name)))
            .collect();
        if let [only] = suffixed.as_slice() {
            return Some(only);
        }
        match in_set.as_slice() {
            [only] if sole_proc_in_set && suffixed.is_empty() => Some(only),
            _ => None,
        }
    }

    /// Look up proc data with the beta `findProcData` fuzzy matching. A slotted
    /// proc always supplies both `enhancement_name` and `set_name`, so the
    /// set-aware paths dominate; the bare-name "first match" fallback (order-
    /// dependent in the beta) is unreachable when `set_name` is non-empty.
    pub fn find(&self, enhancement_name: &str, set_name: &str) -> Option<&ProcData> {
        // Most specific: "Set: IO Name".
        if !set_name.is_empty() {
            if let Some(hit) = self.entries.get(&format!("{set_name}: {enhancement_name}")) {
                return Some(hit);
            }
        }
        // Exact match on the bare key, but only accept when the set also matches
        // (or none was given).
        if let Some(exact) = self.entries.get(enhancement_name) {
            if set_name.is_empty() || exact.set_name == set_name {
                return Some(exact);
            }
        }
        // Set-aware IO-name scan: prefer a matching set.
        let mut first_match: Option<&ProcData> = None;
        for (key, data) in &self.entries {
            let name_matches =
                data.io_name == enhancement_name || key.ends_with(&format!(": {enhancement_name}"));
            if !name_matches {
                continue;
            }
            if !set_name.is_empty() && data.set_name == set_name {
                return Some(data);
            }
            if first_match.is_none() {
                first_match = Some(data);
            }
        }
        // A no-setName caller takes the first ioName match (moot for the calc).
        if set_name.is_empty() {
            if let Some(hit) = first_match {
                return Some(hit);
            }
        }
        // Fallback: match by set name (LotG "Defense/+Recharge" vs "Buff Recharge").
        if !set_name.is_empty() {
            if let Some(hit) = self.entries.values().find(|d| d.set_name == set_name) {
                return Some(hit);
            }
        }
        // Last resort: an ioName match even with a mismatched set.
        first_match
    }
}
