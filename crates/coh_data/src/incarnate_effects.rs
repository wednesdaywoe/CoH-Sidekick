//! Incarnate power effect tables — the typed view of the contract's `incarnate`
//! section (`GENERATED_ALPHA_EFFECTS`, `GENERATED_DESTINY_EFFECTS`, …). The
//! per-slot effect data the Rust totals pipeline needs for Pass 6 (incarnates).
//!
//! Ported (shape, not name) from the beta's `src/data/incarnate-effects.ts`
//! interfaces (`AlphaEffects`, `DestinyEffects`, `HybridEffects`, …). Unlike
//! regular powers, incarnate effects are NOT atomized — the beta reads these
//! bespoke flat per-power tables directly, so we mirror the flat tables rather
//! than routing incarnate data through the atom pipeline (there is nothing to
//! atomize). Each table is `Record<powerId, FixedShape>` keyed by the NORMALIZED
//! power id ([`normalize_incarnate_power_id`]).
//!
//! Data/calc split (D2): this struct is pure DATA. The apply logic (Destiny
//! time-decay resolution, Alpha-enhances-Destiny, Genesis-Fate scaling, the
//! flat-accumulator writes into `GlobalBonuses`) lives in `coh_math::incarnates`.
//!
//! Source: `contract/<dataset>/incarnate.json`, emitted verbatim from the beta's
//! generated `incarnate-effects.ts` by `emit-contract.cjs`. Absent section ⇒
//! empty tables (a hand-constructed `PowerDatabase` needs no incarnate data); the
//! calc-side lookups then find nothing and contribute nothing.
//!
//! Interface / Judgement contribute NOTHING to stat totals in the game (Interface
//! is enemy-debuff procs; Judgement is a click attack) — the beta's own
//! `applyIncarnateBonuses` never reads them into `GlobalBonuses`. They are typed
//! here for completeness (identity/display data a UI pass may consume later), not
//! because the calc reads them. Lore contributes ONLY its `level_shift`.

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// Alpha slot enhancement bonuses (percentage as decimal). One struct serves both
/// `GENERATED_ALPHA_EFFECTS` and `GENERATED_ALPHA_ED_BYPASS` (the ED-bypass table
/// has the same aspect union minus `level_shift`). For Pass 6 the calc-relevant
/// fields are `level_shift` (Alpha's own level shift) and the four
/// Alpha-enhances-Destiny aspects (`resistance`, `defense`, `heal`,
/// `endurance_modification`); the full aspect union additionally feeds the apply pass's
/// `combineWithAlphaED` ED split, mapped to enhancement aspects by `coh_math`'s
/// `alpha_enhancement` (INCARNATE-1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AlphaEffects {
    pub damage: Option<f64>,
    pub accuracy: Option<f64>,
    pub recharge: Option<f64>,
    pub endurance_reduction: Option<f64>,
    pub endurance_modification: Option<f64>,
    pub range: Option<f64>,
    pub heal: Option<f64>,
    pub defense: Option<f64>,
    pub resistance: Option<f64>,
    pub hold: Option<f64>,
    pub immobilize: Option<f64>,
    pub stun: Option<f64>,
    pub sleep: Option<f64>,
    pub fear: Option<f64>,
    pub confuse: Option<f64>,
    pub slow: Option<f64>,
    pub to_hit_debuff: Option<f64>,
    pub defense_debuff: Option<f64>,
    pub to_hit_buff: Option<f64>,
    pub taunt: Option<f64>,
    // No `intangible`: the `intangible_*` alpha_silent files are an HC slot REUSE whose
    // content is Absorb (they parse to a single `Absorb`@Strength template pair), and the
    // converter now names them `absorb`. A field here that nothing in
    // `alpha_effects_to_bonuses` spends is exactly how that boost went missing — parsed,
    // never applied. If a genuine Intangible alpha ever ships, add the field AND its `put`.
    pub run_speed: Option<f64>,
    pub jump_speed: Option<f64>,
    pub fly_speed: Option<f64>,
    pub absorb: Option<f64>,
    pub level_shift: Option<f64>,
}

/// Flat / peak Destiny effect values (`GENERATED_DESTINY_EFFECTS`). Percentage
/// fields are decimals (0.5 = +50%); `mez_protection`/`kb_protection` are raw
/// magnitude points; `level_shift` is a small integer count.
///
/// `endurance`, `debuff_resistance`, `status_resistance`, `heal_percent`,
/// `heal_scale`/`heal_table`, `initial_duration`/`total_duration` are DISPLAY-ONLY
/// — the beta's `applyIncarnateBonuses` never reads them into `GlobalBonuses`.
/// `endurance` in particular is Ageless's instant per-cast refill, NOT a sustained
/// buff; mapping it into max-endurance would be a bug. They are parsed for
/// completeness (and `heal_percent` is an Alpha-enhance write target, though it
/// too is never read downstream).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DestinyEffects {
    pub defense_all: Option<f64>,
    pub resistance_all: Option<f64>,
    pub debuff_resistance: Option<f64>,
    pub status_resistance: Option<f64>,
    pub heal_received: Option<f64>,
    pub heal_percent: Option<f64>,
    pub heal_scale: Option<f64>,
    pub heal_table: Option<String>,
    pub kb_protection: Option<f64>,
    pub run_speed: Option<f64>,
    pub recovery: Option<f64>,
    pub regeneration: Option<f64>,
    // `maxHP` is fully capitalized in the wire, which serde's camelCase would emit
    // as `maxHp` — pin the exact key.
    #[serde(rename = "maxHP")]
    pub max_hp: Option<f64>,
    pub max_endurance: Option<f64>,
    pub endurance: Option<f64>,
    pub recharge: Option<f64>,
    pub damage: Option<f64>,
    pub to_hit: Option<f64>,
    pub mez_protection: Option<f64>,
    pub level_shift: Option<f64>,
    pub initial_duration: Option<f64>,
    pub total_duration: Option<f64>,
}

impl DestinyEffects {
    /// Overwrite the numeric field named by a timeline stat key (the beta's
    /// dynamic `resolved[stat] = …`). Covers every key that appears in a
    /// `GENERATED_DESTINY_TIMELINE` across all datasets; an unrecognized key is
    /// ignored (a decaying stat with no matching field is never read downstream).
    fn set_timeline_stat(&mut self, key: &str, value: f64) {
        match key {
            "defenseAll" => self.defense_all = Some(value),
            "resistanceAll" => self.resistance_all = Some(value),
            "debuffResistance" => self.debuff_resistance = Some(value),
            "kbProtection" => self.kb_protection = Some(value),
            "recovery" => self.recovery = Some(value),
            "regeneration" => self.regeneration = Some(value),
            "maxHP" => self.max_hp = Some(value),
            "endurance" => self.endurance = Some(value),
            "recharge" => self.recharge = Some(value),
            "mezProtection" => self.mez_protection = Some(value),
            _ => {}
        }
    }
}

/// One decay tier of a diminishing Destiny buff: `value` (contribution) active
/// until `duration` seconds after cast. `duration == 0` marks an instant-only
/// tier (active at `t <= 0` only).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct DestinyTimelineTier {
    pub value: f64,
    pub duration: f64,
}

/// Hybrid three-layer effect model. The layers stay `HashMap<String, f64>` (not
/// fixed fields) because the beta's consumer (`applyHybridStatBlock`) iterates
/// them generically — the map keys are `GlobalBonuses` camelCase field names
/// (`damage`, `defMelee`, `resSmashing`, …) plus the special keys
/// `statusResistance`, `enduranceDiscount`, `defenseAll`, `resistanceAll`, and
/// `prot*`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HybridEffects {
    pub tree: String,
    /// Always-on just by equipping.
    pub passive: HashMap<String, f64>,
    /// Active when the slot is toggled on.
    pub front_loaded: HashMap<String, f64>,
    /// Stacks once per nearby enemy while the slot is toggled on, up to [`Self::max_targets`].
    /// Applied by `coh_math::incarnates`, which reads the foe count off the combat context.
    pub per_target: HashMap<String, f64>,
    /// The per-foe ceiling: how many enemies [`Self::per_target`] can stack against. Derived
    /// from the power's own `max_targets_hit` minus the caster's slot in it, not from the
    /// tooltip that used to state it (HYBRID-PT-1). 0 on every tree with no per-foe layer.
    pub max_targets: f64,
    pub duration: f64,
    pub recharge: f64,
}

/// The below-45 exemplar power a Genesis ability grants. Only the Fate-tree
/// `buff` kind feeds the dashboard (`stats.recharge`/`stats.recovery`); the other
/// kinds (attack/proc/summon) and any other `stats` key (`endurance`) are
/// display-only. Unknown fields (radius, recharge, mezProtection, …) are ignored.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenesisExemplarEffect {
    pub kind: String,
    pub stats: HashMap<String, f64>,
}

/// Genesis amplifier effect (Rebirth-only). Only `tree` + `tier_percent` (and the
/// Fate `exemplar_effect`) are calc-relevant: `fate` scales the Destiny block,
/// `socket` adds `tier_percent` to Max HP and Max Endurance. `verdict`/`data` are
/// display-only.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GenesisEffects {
    pub display_name: String,
    pub tree: String,
    pub enhances_slot: String,
    pub tier_percent: f64,
    #[serde(rename = "loreMaxHP")]
    pub lore_max_hp: Option<f64>,
    pub exemplar_power: Option<String>,
    pub exemplar_effect: Option<GenesisExemplarEffect>,
}

/// Interface proc effects — typed for completeness; NO field feeds `GlobalBonuses`
/// (enemy-debuff procs, not player stats).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct InterfaceEffects {
    pub debuff_type: Option<String>,
    pub debuff_magnitude: Option<f64>,
    pub debuff_duration: Option<f64>,
    pub dot_type: Option<String>,
    pub dot_damage: Option<f64>,
    pub dot_duration: Option<f64>,
    pub dot_table_name: Option<String>,
    pub proc_chance: Option<f64>,
}

/// Judgement click-attack effects — typed for completeness; NO field feeds
/// `GlobalBonuses` (a click attack, not a passive stat).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JudgementEffects {
    pub damage_type: String,
    pub effect_area: String,
    pub range: f64,
    pub radius: f64,
    pub arc: f64,
    pub max_targets: f64,
    pub activation_time: f64,
    pub recharge_time: f64,
    pub damage_scale: f64,
    pub table_name: String,
    pub secondary_effects: Vec<String>,
}

/// Lore pet-summon effects — only `level_shift` is calc-relevant.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LoreEffects {
    pub faction: String,
    pub pets: Vec<String>,
    pub duration: f64,
    pub recharge_time: f64,
    pub level_shift: f64,
}

/// The contract's `incarnate` section, fully typed. Each map is keyed by the
/// normalized power id (see [`normalize_incarnate_power_id`]).
#[derive(Debug, Default, Clone)]
pub struct IncarnateEffects {
    pub alpha_effects: HashMap<String, AlphaEffects>,
    pub alpha_ed_bypass: HashMap<String, AlphaEffects>,
    pub destiny_effects: HashMap<String, DestinyEffects>,
    pub destiny_timeline: HashMap<String, HashMap<String, Vec<DestinyTimelineTier>>>,
    pub destiny_boosts: HashMap<String, Vec<String>>,
    pub hybrid_effects: HashMap<String, HybridEffects>,
    pub genesis_effects: HashMap<String, GenesisEffects>,
    pub interface_effects: HashMap<String, InterfaceEffects>,
    pub judgement_effects: HashMap<String, JudgementEffects>,
    pub lore_effects: HashMap<String, LoreEffects>,
}

impl IncarnateEffects {
    /// Whether this dataset carries any effect data for the named slot (the
    /// picker's offered-slot gate — see
    /// [`crate::database::PowerDatabase::offered_incarnate_slots`]). An unknown
    /// slot id is `false`: a slot the effects schema doesn't know cannot
    /// contribute, so it is never offered.
    pub fn has_effects_for_slot(&self, slot_id: &str) -> bool {
        match slot_id {
            "alpha" => !self.alpha_effects.is_empty(),
            "judgement" => !self.judgement_effects.is_empty(),
            "interface" => !self.interface_effects.is_empty(),
            "destiny" => !self.destiny_effects.is_empty(),
            "lore" => !self.lore_effects.is_empty(),
            "hybrid" => !self.hybrid_effects.is_empty(),
            "genesis" => !self.genesis_effects.is_empty(),
            _ => false,
        }
    }

    /// Parse the contract's `incarnate` section (a map of `GENERATED_*` tables).
    ///
    /// Malformed ≠ absent (matching [`crate::at_tables`]/
    /// [`crate::purple_patch`]): an ABSENT section degrades to empty tables (a
    /// hand-constructed `PowerDatabase` needs no incarnate data — the lookups then
    /// find nothing), but a PRESENT table that doesn't parse is an error, never
    /// silently the default.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(IncarnateEffects::default());
        };
        Ok(IncarnateEffects {
            alpha_effects: parse_table(section, "GENERATED_ALPHA_EFFECTS")?,
            alpha_ed_bypass: parse_table(section, "GENERATED_ALPHA_ED_BYPASS")?,
            destiny_effects: parse_table(section, "GENERATED_DESTINY_EFFECTS")?,
            destiny_timeline: parse_table(section, "GENERATED_DESTINY_TIMELINE")?,
            destiny_boosts: parse_table(section, "GENERATED_DESTINY_BOOSTS")?,
            hybrid_effects: parse_table(section, "GENERATED_HYBRID_EFFECTS")?,
            genesis_effects: parse_table(section, "GENERATED_GENESIS_EFFECTS")?,
            interface_effects: parse_table(section, "GENERATED_INTERFACE_EFFECTS")?,
            judgement_effects: parse_table(section, "GENERATED_JUDGEMENT_EFFECTS")?,
            lore_effects: parse_table(section, "GENERATED_LORE_EFFECTS")?,
        })
    }

    /// Destiny effect values resolved at `time_sec` seconds after cast — the beta
    /// `getDestinyEffectsAtTime`. Each decaying stat is replaced by the sum of its
    /// timeline tiers still active at `time_sec` (`duration > time_sec`, or an
    /// instant tier at `time_sec <= 0`), rounded to 6 decimals to match the beta's
    /// float-drift guard; non-decaying stats pass through from the flat table.
    /// `None` when the power has no flat effect entry.
    pub fn destiny_effects_at_time(
        &self,
        normalized_id: &str,
        time_sec: f64,
    ) -> Option<DestinyEffects> {
        let mut effects = self.destiny_effects.get(normalized_id)?.clone();
        let Some(timeline) = self.destiny_timeline.get(normalized_id) else {
            return Some(effects);
        };
        for (stat, tiers) in timeline {
            let sum: f64 = tiers
                .iter()
                .filter(|t| t.duration > time_sec || (t.duration == 0.0 && time_sec <= 0.0))
                .map(|t| t.value)
                .sum();
            effects.set_timeline_stat(stat, round6(sum));
        }
        Some(effects)
    }

    /// The sustained-floor time (seconds after cast) — the beta
    /// `getDestinySustainedFloorTime`: the start of the final decay plateau (the
    /// second-largest distinct positive tier duration), or 0 when the power does
    /// not decay (fewer than two distinct expiries). The conservative value a
    /// perma-Destiny build sustains; Pass 6 resolves every Destiny power here.
    pub fn destiny_sustained_floor_time(&self, normalized_id: &str) -> f64 {
        let Some(timeline) = self.destiny_timeline.get(normalized_id) else {
            return 0.0;
        };
        let mut durations: Vec<f64> = timeline
            .values()
            .flat_map(|tiers| tiers.iter())
            .map(|t| t.duration)
            .filter(|&d| d > 0.0)
            .collect();
        durations.sort_by(|a, b| a.partial_cmp(b).expect("tier durations are finite"));
        durations.dedup();
        if durations.len() <= 1 {
            return 0.0;
        }
        durations[durations.len() - 2]
    }
}

/// Round to 6 decimals — the beta's `Math.round(x * 1e6) / 1e6` drift guard.
/// Applied to timeline tier sums and Alpha-enhance products so the Rust f64
/// matches the TS oracle bit-for-bit (the values are non-negative, where Rust's
/// round-half-away and JS's round-half-up agree).
pub(crate) fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Parse one `GENERATED_*` table into `HashMap<powerId, T>`. An absent table key
/// is an empty map (a dataset may not carry every slot); a present-but-malformed
/// table is an error naming the table.
fn parse_table<T: serde::de::DeserializeOwned>(
    section: &Value,
    key: &str,
) -> Result<HashMap<String, T>, String> {
    match section.get(key) {
        None => Ok(HashMap::new()),
        Some(val) => {
            serde_json::from_value(val.clone()).map_err(|e| format!("incarnate.{key}: {e}"))
        }
    }
}

/// Normalize a build's incarnate `power_name` (the beta `SelectedIncarnatePower.
/// powerId`, e.g. `"Incarnate.Alpha.Agility_Core_Paragon"`) into the generated
/// tables' key (`"agility_core_paragon"`). Ports the beta `normalizePowerId`:
/// lowercase, strip a leading `incarnate.<slot>.` prefix, then replace `.`, `-`,
/// and whitespace with `_`.
pub fn normalize_incarnate_power_id(power_id: &str) -> String {
    let lower = power_id.to_lowercase();
    let stripped = [
        "alpha",
        "judgement",
        "interface",
        "destiny",
        "lore",
        "hybrid",
        "genesis",
    ]
    .iter()
    .find_map(|slot| lower.strip_prefix(&format!("incarnate.{slot}.")))
    .unwrap_or(lower.as_str());
    stripped
        .chars()
        .map(|c| {
            if c == '.' || c == '-' || c.is_whitespace() {
                '_'
            } else {
                c
            }
        })
        .collect()
}
