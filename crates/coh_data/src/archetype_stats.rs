//! Archetype cap tables — the per-AT ceilings Pass 8 (`finalize`) clamps the projected
//! display stats against: the resistance cap, the damage-strength cap, and the per-level
//! HP / HP-cap / absorb-cap tables. Ported (shape, not name) from the beta's `ArchetypeStats`
//! (`src/types/archetype.ts:21`) as generated into `archetype-stats.generated.ts`.
//!
//! Data/calc split (D2): this struct is pure DATA — the caps loaded from the contract's
//! `archetype-stats` section. The CLAMP math (resistance cap, absolute-HP cap) lives in
//! `coh_math::finalize`.
//!
//! **Two different things are called a defense cap, and only one of them is a clamp.** The
//! "defense softcap" is a purple-patch level-diff reference THRESHOLD defense legitimately
//! exceeds ([`crate::purple_patch::get_defense_softcap`]) — it is not here and never was.
//! [`ArchetypeCaps::defense_ceiling_table`] is the game's actual
//! `CLAMP_CUR(fDefenseType[..])` clamp, three to five times above that threshold, and it is
//! here (DATA-GAP-REGISTER CAPS-1).
//!
//! Source: the contract's `archetype-stats.<at> = { baseHP, maxHP, absorbCap, resistanceCap,
//! damageCap, rechargeFloor, rechargeCap, hpTable, hpCapTable, absorbCapTable, toHitBase,
//! toHitCapTable, regenerationBase, regenerationCapTable, recoveryBase, recoveryCapTable,
//! defenseCeilingTable, maxEnduranceTable, maxEnduranceCapTable }` section.
//! Absent ≠ malformed, like
//! [`crate::at_tables`] / [`crate::purple_patch`]: an ABSENT section degrades to empty
//! (every `get` misses — a hand-constructed `PowerDatabase` needs no cap data, and the
//! calc-side lookups fail loud rather than inventing a ceiling), but a PRESENT section
//! that fails to decode is a load error. Every field is required: a defaulted-in `0.0`
//! `resistanceCap` would silently clamp resistance to 0% (all 45 contract entries carry
//! every field, censused 2026-07-28).

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

/// One value per travel axis. Named fields rather than a keyed map with an axis enum of its
/// own: `coh_math::movement::MovementStat` already owns the axis vocabulary and selects the
/// field by exhaustive `match`, so a fifth axis breaks a build instead of missing a lookup.
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
pub struct MovementAxes {
    #[serde(rename = "runSpeed")]
    pub run_speed: f64,
    #[serde(rename = "flySpeed")]
    pub fly_speed: f64,
    #[serde(rename = "jumpSpeed")]
    pub jump_speed: f64,
    #[serde(rename = "jumpHeight")]
    pub jump_height: f64,
}

/// Per-level ceiling per travel axis (index = level − 1), same field layout as [`MovementAxes`].
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
pub struct MovementAxisTables {
    #[serde(rename = "runSpeed")]
    pub run_speed: Vec<f64>,
    #[serde(rename = "flySpeed")]
    pub fly_speed: Vec<f64>,
    #[serde(rename = "jumpSpeed")]
    pub jump_speed: Vec<f64>,
    #[serde(rename = "jumpHeight")]
    pub jump_height: Vec<f64>,
}

/// One archetype's caps. `resistance_cap` is a FRACTION (0.75 = 75%); `damage_cap` is a
/// MULTIPLIER (5.0 = the total damage-strength ceiling, i.e. +400% over the ×1 base). The
/// HP tables are indexed by `level - 1` (index 0 = level 1 … index 49 = level 50).
#[derive(Debug, Default, Clone, PartialEq, Deserialize)]
pub struct ArchetypeCaps {
    /// The game's own class token for this archetype (`"Class_Blaster"`).
    ///
    /// Carried because the effect gates compare against this spelling and nothing else:
    /// an archetype-forked atom names the classes it is base for in it
    /// ([`crate::AtomicEffect::caster_archetypes`]), so the gather needs the build's
    /// token to match. Reading it off the export rather than rebuilding it from the
    /// hyphenated id keeps a naming convention out of the code (AT-FORK-1).
    #[serde(default, rename = "className")]
    pub class_name: String,
    /// Level-50 base (unbuffed) HP — the fallback when [`Self::hp_table`] is absent.
    #[serde(rename = "baseHP")]
    pub base_hp: f64,
    /// Level-50 HP cap — the fallback when [`Self::hp_cap_table`] is absent.
    #[serde(rename = "maxHP")]
    pub max_hp: f64,
    /// Resistance cap as a FRACTION (0.75 = 75%). `finalize` multiplies by 100 to clamp the
    /// percentage-valued `res*` display stats.
    #[serde(rename = "resistanceCap")]
    pub resistance_cap: f64,
    /// Total damage-strength cap as a MULTIPLIER (e.g. 5.0). Applied PER POWER against
    /// `1 + enh + globalDamage + buffs` (beta `damage.ts:648`), not against the global
    /// damage-buff total — so `finalize` does NOT clamp the `damage` display stat with it;
    /// `coh_math::damage` binds each power's strength multiplier against it (RB5).
    #[serde(rename = "damageCap")]
    pub damage_cap: f64,
    /// RechargeTime net-strength FLOOR (`ClampStrength`: the lowest a recharge debuff can
    /// push the `1 + enh + global` divisor — 0.25 on every player class, i.e. a power slows
    /// to at most 4× its base recharge, the −75% floor).
    #[serde(rename = "rechargeFloor")]
    pub recharge_floor: f64,
    /// RechargeTime net-strength CAP (the highest buffs can push the divisor — 5.0 on every
    /// player class, the +400% recharge cap).
    #[serde(rename = "rechargeCap")]
    pub recharge_cap: f64,
    /// EnduranceDiscount net-strength FLOOR — 0.0001 on every class of every dataset, the same
    /// epsilon the server adds to the divisor at each consumption site (`fEnduranceCost /
    /// (fEnduranceDiscount + 0.0001f)`, `character_tick.c` / `character_combat_eval.c`).
    /// A divide guard, NOT a floor like recharge's: an endurance debuff is unbounded, so a
    /// power under one costs more, and nothing pins the divisor at 1.
    #[serde(rename = "enduranceFloor")]
    pub endurance_floor: f64,
    /// EnduranceDiscount net-strength CAP (the highest buffs can push the divisor — 5.0 on
    /// every class, the +400% endurance-discount cap).
    #[serde(rename = "enduranceCap")]
    pub endurance_cap: f64,
    /// Level-50 absorb ceiling — the fallback when [`Self::absorb_cap_table`] is absent.
    #[serde(rename = "absorbCap")]
    pub absorb_cap: f64,
    /// The unbuffed travel scales — AttribBase's own slots (1.0 run/jump/height, 1.5 fly on
    /// every player class of all three datasets). A "scale" is the multiplier the server hands
    /// the physics layer: `setSpeed` scales `BASE_PLAYER_FORWARDS_SPEED` (0.7 ft/tick × 30
    /// ticks/s = 21 ft/s) by it, and `setJumpHeight` takes the jump scale the same way. The
    /// unit projection is the display's business, not this struct's.
    #[serde(rename = "movementBase")]
    pub movement_base: MovementAxes,
    /// The travel FLOORS — AttribMin's own slots, the lower half of the pair `ClampCur` holds
    /// `attrCur` between. 0.1 run and fly, 0.0 jump speed and height on every player class of
    /// all three datasets, so a grounding power's saturating debuff (Granite Armor's
    /// `JumpHeight −500`, Rest's `−1000` on all four axes) leaves a crawl rather than a
    /// negative speed. Not derivable and not zero — the run/fly tenth is authored, and NPC
    /// classes author their own (a 1.95 run floor, a 50.0 jump height).
    /// DATA-GAP-REGISTER MOVEMIN-1.
    #[serde(rename = "movementFloor")]
    pub movement_floor: MovementAxes,
    /// The per-level travel ceilings — AttribMaxTable's rows, what `ClampCur` bounds `attrCur`
    /// against (`Common/entity/character_attribs.c`). Flat across levels on Homecoming; per-level
    /// on both forks, so exemplaring down genuinely lowers what a build can reach there.
    #[serde(rename = "movementCapTable")]
    pub movement_cap_table: MovementAxisTables,
    /// Base (unbuffed) HP per level (index = level − 1).
    #[serde(rename = "hpTable")]
    pub hp_table: Vec<f64>,
    /// HP cap per level (index = level − 1).
    #[serde(rename = "hpCapTable")]
    pub hp_cap_table: Vec<f64>,
    /// Absorb ceiling per level (index = level − 1) — the AttribMaxMax Absorb row the game
    /// clamps against. It runs close to [`Self::hp_table`] but is authored separately (HC's
    /// Brute caps 20 points under its base HP at level 1; every dataset caps the Dominator
    /// on a curve its HP never touches), so it is its own table, never derived from HP.
    #[serde(rename = "absorbCapTable")]
    pub absorb_cap_table: Vec<f64>,

    // ---- The attribute ceilings `ClampCur` bounds `attrCur` against, read from the class
    // binary's AttribMaxTable (DATA-GAP-REGISTER CAPS-1). Each ceiling is an ABSOLUTE
    // attribute value, so the percentage the dashboard shows is only recoverable against the
    // class's own base — which is why the bases ride with them. Indexed by `level - 1`, read
    // through [`at_level`] like the HP tables. ----
    /// Base ToHit — 0.75 on every player archetype of every dataset, the even-level base
    /// to-hit the purple patch also reports.
    #[serde(rename = "toHitBase")]
    pub to_hit_base: f64,
    /// ToHit ceiling per level: 0.95 at level 1 rising to 2.0035 at 50, uniform across all 45
    /// player archetypes. Per-level, so exemplaring down genuinely lowers it.
    #[serde(rename = "toHitCapTable")]
    pub to_hit_cap_table: Vec<f64>,
    /// Base regeneration multiplier — 0.25, or 0.30 on the Arachnos classes.
    #[serde(rename = "regenerationBase")]
    pub regeneration_base: f64,
    /// Regeneration ceiling per level. Against [`Self::regeneration_base`] this is the game's
    /// published cap — 3000% Scrapper/Stalker, 2500% Tanker/Brute, 2000% everyone else.
    #[serde(rename = "regenerationCapTable")]
    pub regeneration_cap_table: Vec<f64>,
    /// Base recovery multiplier — 1.0, or 1.05 on the Arachnos classes.
    #[serde(rename = "recoveryBase")]
    pub recovery_base: f64,
    /// Recovery ceiling per level — 750% Controller/Dominator/Mastermind, 625% Defender, 500%
    /// everyone else, against [`Self::recovery_base`].
    #[serde(rename = "recoveryCapTable")]
    pub recovery_cap_table: Vec<f64>,
    /// The real per-level defense clamp — `CLAMP_CUR(fDefenseType[..])`, ≈1.75-2.25 at level
    /// 50. NOT the defense softcap: that is a purple-patch level-diff THRESHOLD defense
    /// legitimately exceeds ([`crate::purple_patch::get_defense_softcap`]), and this clamp sits
    /// three to five times above it. One curve rather than one per damage type because the
    /// exporter proves every typed defense row the binary authors agrees with every other on
    /// the same archetype, and raises when two disagree.
    #[serde(rename = "defenseCeilingTable")]
    pub defense_ceiling_table: Vec<f64>,
    /// The other half of that clamp — AttribMin's typed-defense scalar, `−1.0` on every player
    /// archetype of all three datasets, so a debuff can drive defense to −100% and no further.
    /// Read for the same reason [`Self::movement_floor`] is: the game writes "your defense is
    /// negated" as a saturating magnitude rather than as a switch, and Thunderspy's Organic
    /// Armor states `Defense −500 × Melee_Buff_Def` on all seven typed slots while Defensive
    /// Adaptation is up — −50.0 on a Tanker at level 50, fifty times past this floor. Absent
    /// (`0.0`) is indistinguishable from a real zero floor here, which is why the EXPORTER
    /// decides authorship and refuses to write the slot it cannot prove.
    /// DATA-GAP-REGISTER ATTRMIN-1.
    #[serde(rename = "defenseFloor")]
    pub defense_floor: f64,
    /// Base max endurance per level — a flat 100 on every player archetype. HitPoints' shape:
    /// the base is itself an AttribMaxTable row, not an AttribBase scalar.
    #[serde(rename = "maxEnduranceTable")]
    pub max_endurance_table: Vec<f64>,
    /// Max-endurance ceiling per level — the AttribMaxMax row over
    /// [`Self::max_endurance_table`], 120 at level 1 rising to 365 at 50. This is how far
    /// +MaxEnd buffs can raise the pool, not the pool itself.
    #[serde(rename = "maxEnduranceCapTable")]
    pub max_endurance_cap_table: Vec<f64>,
}

impl ArchetypeCaps {
    /// Base (unbuffed) HP at `level`, ports beta `getBaselineHealth`: `idx =
    /// clamp(level-1, 0, 49)` into [`Self::hp_table`], falling back to the level-50 scalar
    /// [`Self::base_hp`] when the table is absent.
    pub fn base_hp_at_level(&self, level: i32) -> f64 {
        at_level(&self.hp_table, level).unwrap_or(self.base_hp)
    }

    /// HP cap at `level`, same clamp/fallback as [`Self::base_hp_at_level`] over
    /// [`Self::hp_cap_table`] / [`Self::max_hp`].
    pub fn hp_cap_at_level(&self, level: i32) -> f64 {
        at_level(&self.hp_cap_table, level).unwrap_or(self.max_hp)
    }

    /// Absorb ceiling at `level`, same clamp/fallback as [`Self::base_hp_at_level`] over
    /// [`Self::absorb_cap_table`] / [`Self::absorb_cap`].
    pub fn absorb_cap_at_level(&self, level: i32) -> f64 {
        at_level(&self.absorb_cap_table, level).unwrap_or(self.absorb_cap)
    }

    /// The ToHit BUFF ceiling at `level`, in PERCENTAGE POINTS — the units
    /// `CharacterStats::to_hit` is in. ToHit is additive (`base + buff/100`, see
    /// `coh_math::finalize::project_combat`), so the buff a build may carry is the attribute
    /// ceiling minus the base: 125.35 points at level 50.
    ///
    /// `None` when the table is absent, never a fabricated ceiling — the caller leaves the
    /// stat raw and says so, the same way it does without any caps at all.
    pub fn to_hit_buff_cap_at_level(&self, level: i32) -> Option<f64> {
        Some((at_level(&self.to_hit_cap_table, level)? - self.to_hit_base) * 100.0)
    }

    /// The regeneration BUFF ceiling at `level`, as a PERCENT over base — the units
    /// `CharacterStats::regeneration` is in. Regeneration is multiplicative over the class's
    /// own base, so the buff ceiling is `(cap / base − 1) × 100`: +2900% for a Scrapper, whose
    /// total therefore caps at the published 3000%.
    pub fn regeneration_buff_cap_at_level(&self, level: i32) -> Option<f64> {
        buff_cap_over_base(&self.regeneration_cap_table, self.regeneration_base, level)
    }

    /// The recovery BUFF ceiling at `level`, same units and arithmetic as
    /// [`Self::regeneration_buff_cap_at_level`] — +400% for the classes the game publishes at
    /// a 500% total.
    pub fn recovery_buff_cap_at_level(&self, level: i32) -> Option<f64> {
        buff_cap_over_base(&self.recovery_cap_table, self.recovery_base, level)
    }

    /// The defense ceiling at `level` as a PERCENT — the units the `def_*` stats are in.
    /// A real clamp, unrelated to the defense softcap threshold.
    pub fn defense_ceiling_at_level(&self, level: i32) -> Option<f64> {
        Some(at_level(&self.defense_ceiling_table, level)? * 100.0)
    }

    /// The defense floor as a PERCENT — the units the `def_*` stats are in, −100.0 on every
    /// player archetype. Unlike the ceiling this takes no level: `AttribMin` is a scalar, so
    /// exemplaring lowers what a build can reach but not how far a debuff can drive it.
    pub fn defense_floor_percent(&self) -> f64 {
        self.defense_floor * 100.0
    }

    /// `(base pool, ceiling)` max endurance at `level` in POINTS — the units
    /// `CharacterStats::max_end` is in. `None` unless BOTH tables carry the level, for the
    /// same reason [`Self::baseline_health`] reads its pair jointly: a base without its
    /// ceiling (or the reverse) cannot produce an honest clamped pool.
    pub fn max_endurance_at_level(&self, level: i32) -> Option<(f64, f64)> {
        Some((
            at_level(&self.max_endurance_table, level)?,
            at_level(&self.max_endurance_cap_table, level)?,
        ))
    }

    /// `(baseHealth, maxHealth)` at `level` — the faithful port of the beta `getBaselineHealth`
    /// ([stats.ts:115](../../../CoH-Sidekick/src/utils/calculations/stats.ts#L115)). Unlike
    /// [`Self::base_hp_at_level`] / [`Self::hp_cap_at_level`] (which fall back per-table
    /// independently), the beta uses the level-indexed tables **only when both carry the clamped
    /// index**, else falls back to *both* scalars together. Identical to the independent form at
    /// the L50 corpus (both tables shorter than idx 49 ⇒ scalars); the JOINT rule matters only
    /// off-corpus, so absorb-fraction resolution (which the gate grades against this exact
    /// baseline) stays bit-faithful at every level.
    pub fn baseline_health(&self, level: i32) -> (f64, f64) {
        let idx = (level - 1).clamp(0, 49) as usize;
        match (self.hp_table.get(idx), self.hp_cap_table.get(idx)) {
            (Some(&base), Some(&cap)) => (base, cap),
            _ => (self.base_hp, self.max_hp),
        }
    }
}

/// A multiplicative attribute's BUFF ceiling as a percent over its own base: `(cap/base − 1)
/// × 100`. `None` when the table misses the level or the base is not positive — a zero base
/// would divide, and the ceiling means nothing without it.
fn buff_cap_over_base(table: &[f64], base: f64, level: i32) -> Option<f64> {
    if base <= 0.0 {
        return None;
    }
    Some((at_level(table, level)? / base - 1.0) * 100.0)
}

/// `table[clamp(level-1, 0, 49)]`, or `None` when the table is empty (or too short at the
/// clamped index) so the caller falls back to the scalar. The `49` ceiling mirrors the
/// beta's `Math.min(49, level - 1)` — levels beyond 50 read the level-50 row.
pub fn at_level(table: &[f64], level: i32) -> Option<f64> {
    if table.is_empty() {
        return None;
    }
    let idx = (level - 1).clamp(0, 49) as usize;
    table.get(idx).copied()
}

/// One dataset's per-archetype caps: `archetype → caps`.
#[derive(Debug, Default, Clone)]
pub struct ArchetypeStats {
    per_archetype: HashMap<String, ArchetypeCaps>,
}

impl ArchetypeStats {
    /// Parse the contract's `archetype-stats` section (`{ <at>: { baseHP, … } }`).
    /// Absent → empty (every `get` misses, and the calc fails loud on the miss);
    /// present-but-broken — a non-object section, or an entry missing any cap field —
    /// is an error: a skipped entry or a defaulted-in `0.0` cap would ship a silently
    /// wrong clamp.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(ArchetypeStats::default());
        };
        let map = section
            .as_object()
            .ok_or("archetype-stats section is not an object")?;
        let mut per_archetype = HashMap::with_capacity(map.len());
        for (at_id, caps_val) in map {
            let caps = serde_json::from_value::<ArchetypeCaps>(caps_val.clone())
                .map_err(|decode_error| format!("archetype-stats {at_id:?}: {decode_error}"))?;
            per_archetype.insert(at_id.clone(), caps);
        }
        Ok(ArchetypeStats { per_archetype })
    }

    /// The caps for `archetype`, or `None` if absent (an unknown AT id, or an unloaded
    /// section). The caller decides — `finalize` fails loud rather than clamping to a
    /// fabricated ceiling.
    pub fn get(&self, archetype: &str) -> Option<&ArchetypeCaps> {
        self.per_archetype.get(archetype)
    }

    /// Every non-empty class token this section states, sorted.
    ///
    /// The roster an archetype-forked atom is filtered against
    /// (`AtomicEffect::caster_archetypes`), read here rather than through the archetype
    /// catalog because this is the map `class_name_of` already resolves through and it needs
    /// no section parse. The two rosters agreeing is a claim about the export, not a
    /// derivation, so `archetype_stats_roster_is_the_catalog_roster` holds them equal on all
    /// four forks — a stats entry the catalog does not list would add a phantom arm to the
    /// unanimity vote in `window_slots::bag_slots` and silently drop restored slots.
    ///
    /// Sorted because the vote takes its value from the first arm: unanimity makes the choice
    /// order-independent, and a stable order keeps a debug dump reproducible anyway.
    pub fn class_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .per_archetype
            .values()
            .map(|caps| caps.class_name.as_str())
            .filter(|name| !name.is_empty())
            .collect();
        names.sort_unstable();
        names
    }
}
