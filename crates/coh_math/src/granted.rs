//! PROD6B-2: per-power granted buff/debuff magnitudes.
//!
//! 6B-1 projected a power's execution stats (recharge / endurance / accuracy / cast /
//! range) because the engine already computed them. The magnitudes a power *grants* are a
//! different problem: the accumulator only ever resolved the families that touch the
//! character's own totals, so the offensive half (mez duration, knockback distance, the
//! `-ToHit`/`-Def`/`-Regen` debuff aspects) had no reader anywhere in the engine. This
//! module is that reader. It walks a power's DISPLAY bag ([`display_effects`], PROD6C-3a),
//! resolves each registered key against its AT modifier table, and wraps the result in the
//! same three-tier the rest of the projection uses.
//!
//! It reads the bag rather than the atoms on purpose: the bag is the exact object the
//! beta's `RegistryEffectsDisplay` renders from, so a value resolved here is comparable
//! row-for-row against the display it's replacing (PROD6C). The per-key rules come from
//! [`crate::effect_registry`], i.e. `hand-data/effect-registry.json`, the same file the beta
//! reads, so neither side owns a private copy of them.
//!
//! Fidelity: this ports the beta `getEffectBaseValue` + `calcEffectThreeTier` +
//! `expandByTypeEntries` + `expandProtectionEntries` + the mez-duration and
//! knockback-distance resolution that lived inline in the component's render. The beta
//! `powerProjectionParity` test grades every row against `resolvePowerMagnitudes`, the
//! pure function extracted from that component.

use crate::effect_registry::{self, EffectCategory, EffectDisplayConfig, EffectFormat};
use crate::enhancement::EnhancementBonuses;
use crate::projection::{extra_object, is_reduction_aspect, ReductionClamps, ThreeTier};
use crate::totals::GlobalBonuses;
use coh_data::atom::AttribType;
use coh_data::{DatasetId, Power, PowerDatabase, TableScope};
use serde_json::{Map, Value};
use std::collections::{HashSet, VecDeque};

/// Table-base fallback rate for a scaled effect whose table is absent or unknown: the beta
/// `TABLE_BASE_VALUES['default']`.
///
/// The beta also carries seven NAMED fallback rates for specific tables. All of them are
/// unreachable: the four `*_res_dmg` entries are 0.10, identical to this default (only two of
/// them are missing from any fork's AT tables, since Rebirth/Thunderspy lack
/// `melee_debuff_res_dmg`/`ranged_debuff_res_dmg`, and those two therefore resolve to the same
/// number either way), and the `*_buff_def` / `*_debuff_def` / `*_slow` entries name tables
/// present for every archetype in all three forks, so the AT lookup always wins.
const UNKNOWN_TABLE_RATE: f64 = 0.10;

/// `*_Ones` tables (`Melee_Ones`, `Ranged_Ones`) are a constant 1.0 for every archetype
/// and level, signalling "the scale IS the value".
const ONES_TABLE_SUFFIX: &str = "_ones";

/// Display-bag field holding the `scale × table` terms one key's value is the SUM of, written
/// by [`pseudo_pet_effects`] when a pseudo-pet's rows for that key name more than one AT table.
///
/// A value carrying it has no top-level `scale` or `table`, because there's no honest single
/// pair to state: a scale means nothing except against its own table, so a reader that took one
/// would read a number in no unit at all.
pub const SCALE_TERMS_KEY: &str = "scaleTerms";

/// Display-bag field naming the pet character class a value's tables resolve against, written
/// by [`pseudo_pet_effects`] on every row it merges into a summoning power's bag.
///
/// A value without it resolves against the build's archetype, as the power's own rows do. A
/// pseudo-pet's rows are a second character's: the client resolves them against
/// `pDef->characterClassName` and passes the summoner alongside as `creatorClass` for the
/// `Requires` evaluation alone (`uiPowerInfo.c` `power_AddPetEffects`). The mark travels ON
/// the row rather than being decided by the reader, because one bag holds both kinds, and
/// after ENT-8's split one KEY can hold rows from two pets of different classes (ENT-10).
pub const PET_CLASS_KEY: &str = "petClass";

/// The class one display value's tables resolve against: the pet class the value names, else
/// the caller's scope (the build's archetype, in every non-pet case).
fn scope_of<'a>(value: &'a Value, outer: TableScope<'a>) -> TableScope<'a> {
    match value.get(PET_CLASS_KEY).and_then(Value::as_str) {
        Some(class) => TableScope::Pet(class),
        None => outer,
    }
}

/// The primary damage types an expanded by-type row must cover before it collapses to a
/// single "(All)" row.
const ALL_PRIMARY_DAMAGE_TYPES: [&str; 8] = [
    "smashing", "lethal", "fire", "cold", "energy", "negative", "psionic", "toxic",
];

/// What quantity a granted magnitude's three-tier carries. A mez row displays a fixed
/// magnitude beside an enhanceable duration, so the two travel together rather than as a
/// nullable field pair that could disagree.
///
/// The four mez members are one decision, taken off the atom's `attribType` (MEZDUR-1). That
/// field states which of the AttribMod's two numbers `scale × table` computes, and nothing
/// else on the row does: a `Duration` mez and a `Magnitude` mez are identical on effect type,
/// sub-type, sign, aspect, recipient and table, and differ only in what the product MEANS.
/// This used to be inferred from the table NAME — a `res_boolean` sniff — which was wrong in
/// both directions at once: it read every applied mez's duration as its magnitude, and read
/// every protection row off a `*_Ones` table as the def compiler's unscaled `Magnitude 1.0`
/// placeholder.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GrantedQuantity {
    /// The tier carries the effect's own value in its `format` unit.
    Value,
    /// The tier carries seconds; `magnitude` is the mez rank the effect grabs.
    MezDuration { magnitude: f64 },
    /// The tier carries the mez rank itself — applied magnitude for a foe-facing row,
    /// protection points for the negative-scale ones. The duration is the template's own
    /// and rides in the bag's `durations`, so there is no second number on the row.
    MezMagnitude,
    /// A mez valued by a stack-machine program rather than by its scale: Inner Will's six
    /// break-free templates read whatever is mezzing you right now (`mod.kHeld source> 1 +
    /// 2 30 minmax negate`). No static tier states it, so the row shows that it varies
    /// instead of printing a number the data does not carry.
    MezExpression,
    /// A mez the engine reads off the template's own two numbers, with no table involved:
    /// `mod_Fill`'s `kModType_Constant` arm (`Common/entity/attribmod.c:1074`) assigns
    /// `ptemplate->fMagnitude` and `ptemplate->fDuration` straight across, and the client's
    /// own power-info window does the same (`modGetMagnitudeAndDuration`,
    /// `Game/src/UI/uiPowerInfo.c:130`) after computing `scale × table` and discarding it.
    ///
    /// So this is neither of the resolvable quantities: the product that gives `MezDuration`
    /// and `MezMagnitude` their value is, for a `Constant` row, the one number the game
    /// throws away. It is also not `MezUnstated` — the discriminator arrived and said
    /// exactly this — nor `MezExpression`, which varies where a constant by definition does
    /// not.
    ///
    /// The population is empty and the gate keeps it that way. The 4,746 `Constant`
    /// templates in the HC export sit on meta attribs (`Set_Mode` 2,636, `Set_Costume`
    /// 1,125, `Power_Redirect`, `Grant_Power`, `Token_Add`, …), and they DO become atoms —
    /// 1,296 of them across the three shipped datasets, every one an `effectType: Meta` row
    /// carrying a `set_mode` / `unset_mode` / `set_costume` mechanic. What none of them is,
    /// is a mez: no `Constant` template lands on a mez attrib in any fork, which is the
    /// narrower claim this member actually rests on and the one the census pins. (ATTRTYPE-1
    /// asserted the broader "never become atoms" and was wrong; the contract bundles read
    /// zero because they predate the stamp, not because the rows are absent.) Whoever trips
    /// that gate has the implementation
    /// already: the value is the template's flat magnitude, unenhanceable, because
    /// `mod_Fill` never multiplies a `Constant` by Strength. It is not written here because
    /// the converter does not hand `mag` across untouched — MEZFACE-1 spends a negative one
    /// as a protection spelling — so resolving the number without a real row to check it
    /// against would publish a guess.
    MezConstant,
    /// The mez value's `attribType` never reached the display bag, so nothing says which
    /// number `scale × table` is. Not a state the game has — a converter regression, which
    /// `mez_attrib_type_stamped` fails on and the row admits to rather than guessing.
    MezUnstated,
    /// The tier carries a distance (knockback / knockup / repel).
    Distance,
}

/// One resolved granted magnitude: a display row's worth of already-resolved numbers.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct GrantedMagnitude {
    /// Row key: the registry effect key, suffixed with the type for an expanded row
    /// (`resistance_fire`, `protection_hold`) exactly as the beta keys its rows.
    pub row_key: String,
    /// The registry key this row resolved through, un-suffixed.
    pub effect_key: String,
    /// Row label. Usually the config's label; an expanded row carries its type label and
    /// the percent-form absorb row carries a qualified one.
    pub label: String,
    pub category: EffectCategory,
    /// Unit of `value`. Normally the config's format; the absorb row overrides it when the
    /// authored value is a fraction of Max HP rather than an HP amount.
    pub format: EffectFormat,
    pub priority: Option<f64>,
    pub value: ThreeTier,
    pub quantity: GrantedQuantity,
    /// Abbreviated type summary for a COLLAPSED by-type row (`All`, `SLF`), when the
    /// authored value was a by-type object the row didn't expand.
    pub by_type_label: Option<String>,
    /// How long this row's effect lasts, in seconds, when the display bag records one for its
    /// key.
    ///
    /// A mez's magnitude and its duration are different quantities (MEZDUR-1) and only one of
    /// them can be the tier. `MezMagnitude` puts the rank in `value`, and the seconds used to
    /// be looked up separately in the bag's `durations` map — which worked while the display
    /// resolved its own bag and stopped the moment the row became the whole answer. The number
    /// travels on the row now, so a consumer that has the row has both (ENGLAG-2).
    ///
    /// `None` on a [`GrantedQuantity::MezDuration`] row, whose tier already IS the seconds —
    /// see [`recorded_duration`], because the number the map holds for one of those is not the
    /// number the game uses. `None` too where the bag records nothing, which includes every
    /// EXPANDED row: `durations` is keyed by bag key and an expansion is keyed
    /// `{effect_key}_{type_key}`, so the pre-strip display never found one for those either.
    /// Restating that as a decision rather than widening it — a per-type row inheriting its
    /// parent's duration would be a new annotation, not a restored one.
    pub duration: Option<f64>,
    /// The source template had AllowStrength off, so neither slotting nor a +Strength buff
    /// reaches this row and all three tiers are the base value. The game's own power-info
    /// window says the same thing in words, "Ignores Buffs and Enhancements", so the fact
    /// travels with the row rather than being inferred from three equal numbers (ENT-4).
    pub ignores_strength: bool,
}

// ---------------------------------------------------------------------------
// authored-value readers (the beta's `getScaleValue` / `isMezEffect` shapes)
// ---------------------------------------------------------------------------

/// The beta `getScaleValue`: a bare number is its own scale; a scaled effect carries one.
fn scale_of(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    value.get("scale").and_then(Value::as_f64)
}

fn table_of(value: &Value) -> Option<&str> {
    value.get("table").and_then(Value::as_str)
}

/// The beta `isMezEffect`: an object with a numeric `mag`.
fn mez_magnitude(value: &Value) -> Option<f64> {
    value.get("mag").and_then(Value::as_f64)
}

/// The `attribType` the converters stamp on a mez bag value from the atom it projects
/// (MEZDUR-1) — the one field saying which of the AttribMod's two numbers `scale × table`
/// computes.
///
/// `None` for an absent field and for an unrecognized spelling alike, because both are the
/// same defect from here: the discriminator did not arrive. Neither falls back to a guess —
/// the caller renders [`GrantedQuantity::MezUnstated`], and the corpus gate keeps the
/// population at zero.
fn mez_attrib_type(value: &Value) -> Option<AttribType> {
    value
        .get("attribType")
        .and_then(Value::as_str)
        .and_then(|wire| wire.parse().ok())
}

fn is_by_type_object(value: &Value) -> bool {
    value.as_object().is_some_and(|object| {
        object
            .keys()
            .any(|key| effect_registry::is_by_type_key(key))
    })
}

/// The beta `getByTypeFirstValue` (registry version): the FIRST entry only, either a number or
/// an object carrying a `scale`. Anything else collapses to nothing. `serde_json`'s
/// `preserve_order` keeps the bag in authored order, so "first" means the same entry it
/// does in JS.
fn by_type_first_value(value: &Value) -> Option<&Value> {
    let first = value.as_object()?.values().next()?;
    // A term-carrying entry counts: a pet's `slow.jumpHeight` states two rows off two tables
    // (Toxic Tarantula's `0.2 @Ranged_Slow` and `500 @Ranged_Ones`), so the entry inside a
    // by-type object is exactly the shape a top-level value can be. Rejecting it collapsed the
    // whole key to nothing and the row vanished (ENT-8).
    if first.is_number() || first.get("scale").is_some() || first.get(SCALE_TERMS_KEY).is_some() {
        return Some(first);
    }
    None
}

/// The beta `getByTypeAbbreviations`: `All` when the value covers every primary damage
/// type, else each type's initial-or-abbreviation concatenated.
fn by_type_abbreviation(value: &Value) -> Option<String> {
    let object = value.as_object()?;
    let keys: Vec<String> = object.keys().map(|key| key.to_lowercase()).collect();
    if ALL_PRIMARY_DAMAGE_TYPES
        .iter()
        .all(|damage_type| keys.iter().any(|key| key == damage_type))
    {
        return Some("All".to_string());
    }
    Some(
        keys.iter()
            .map(|key| abbreviate_type(key))
            .collect::<Vec<_>>()
            .concat(),
    )
}

/// One type's abbreviation for a collapsed by-type summary: the beta's `typeAbbrev` map,
/// falling back to the uppercased first character.
fn abbreviate_type(lowercased_key: &str) -> String {
    match lowercased_key {
        "smashing" => "S".to_string(),
        "lethal" => "L".to_string(),
        "fire" => "F".to_string(),
        "cold" => "C".to_string(),
        "energy" => "E".to_string(),
        "negative" => "N".to_string(),
        "psionic" => "P".to_string(),
        "toxic" => "T".to_string(),
        "melee" => "Mel".to_string(),
        "ranged" => "Rng".to_string(),
        "aoe" => "AoE".to_string(),
        "run" | "runspeed" => "Run".to_string(),
        "fly" | "flyspeed" => "Fly".to_string(),
        "jump" | "jumpspeed" => "Jmp".to_string(),
        "jumpheight" => "JmpH".to_string(),
        other => other
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// table resolution
// ---------------------------------------------------------------------------

/// The AT modifier value for a table at the display level, or `None` when the archetype or
/// table is absent (the beta `getTableValue`, which returns `undefined` on a miss).
fn table_value(db: &PowerDatabase, scope: TableScope<'_>, table: &str, level: i32) -> Option<f64> {
    db.at_tables.value(scope, table, level)
}

/// The beta `getTableBaseValue`. The per-scale base rate for a table name: 1.0 for a
/// `*_Ones` table, the AT-specific value when the tables carry the name, else the
/// [`UNKNOWN_TABLE_RATE`] fallback.
fn table_base_value(
    db: &PowerDatabase,
    scope: TableScope<'_>,
    table: Option<&str>,
    level: i32,
) -> f64 {
    let Some(table) = table else {
        return UNKNOWN_TABLE_RATE;
    };
    let key = table.to_lowercase();
    if key.ends_with(ONES_TABLE_SUFFIX) {
        return 1.0;
    }
    table_value(db, scope, &key, level).unwrap_or(UNKNOWN_TABLE_RATE)
}

/// The beta `calculateResistancePercent`: a bare number passes through as an already-final
/// percent; a scaled effect is `scale × table-base`. Returns a FRACTION.
fn resistance_fraction(
    db: &PowerDatabase,
    scope: TableScope<'_>,
    value: &Value,
    level: i32,
) -> f64 {
    if let Some(number) = value.as_f64() {
        return number;
    }
    let Some(scale) = scale_of(value) else {
        return 0.0;
    };
    let scope = scope_of(value, scope);
    scale * table_base_value(db, scope, table_of(value), level)
}

// ---------------------------------------------------------------------------
// three-tier composition
// ---------------------------------------------------------------------------

/// The beta `GLOBAL_BONUS_ASPECT_MAP` (`powerDisplayUtils.ts`): the enhancement aspects a
/// build-wide global bonus feeds into a per-power "final" column, paired with the
/// accumulator field carrying it. The six mez entries are keyed by EFFECT key rather than
/// enhancement aspect because that's how the mez-duration rows read them. An aspect
/// absent here has no global contribution.
const GLOBAL_BONUS_ASPECTS: &[(&str, &str)] = &[
    ("damage", "damage"),
    ("accuracy", "accuracy"),
    ("recharge", "recharge"),
    ("endurance", "endurance"),
    ("range", "range"),
    // NB `tohit` is deliberately absent. `toHit` is the character's ToHit TOTAL, the sum of
    // every +ToHit buff running with the buff's own contribution included, and the only row
    // reading the `tohit` aspect is `tohitBuff`. Feeding the total in as a strength multiplier
    // made a ToHit buff enhance itself: two stacks of Rage (+40% ToHit) rendered its own
    // +ToHit row at 40 × 1.40. What scales a ToHit buff is ToHit enhancement plus ToHit
    // STRENGTH (Power Boost), so the strength half arrives via [`STRENGTH_ASPECTS`] instead.
    ("heal", "healOther"),
    // A knockback row's quantity is a DISTANCE, and knockback strength is what scales it. That's
    // structurally the same coupling as the six mez keys below, whose quantity is a duration.
    // Only `knockback` is mapped: the corpus grants no knockup or repel strength, and inventing
    // the other two would be a guess dressed as symmetry.
    ("knockback", "knockbackStrength"),
    ("immobilize", "immobilizeDuration"),
    ("hold", "holdDuration"),
    ("stun", "stunDuration"),
    ("sleep", "sleepDuration"),
    ("confuse", "confuseDuration"),
    ("fear", "terrorDuration"),
];

/// The active +Strength self-buffs (Power Boost family) that reach a per-power display row,
/// paired with the accumulator field carrying them: the beta InfoPanel's
/// `globalBonusesWithStrength` augmentation, now single-sourced here (PROD6C).
///
/// Strength is a non-ED multiplier on the caster's OWN matching output, so it lands in the
/// "final" column exactly like a build-wide global. Only the families whose rows have
/// no other global coupling are here: `defense`, `absorb` and the six mez keys appear in no
/// [`GLOBAL_BONUS_ASPECTS`] entry at all, and heal Strength is additive with the +Heal set
/// bonus already mapped there. `damage` is deliberately absent. The family grants no damage
/// Strength, so folding it would double-count against the mapped +Damage global. `tohit` was
/// absent too, on the reading that "the accumulator has already folded ToHit Strength into the
/// `toHit` field these rows read". But that field is the ToHit TOTAL, not a Strength, and
/// using a total as a multiplier is what let a +ToHit buff scale by its own output.
/// [`GLOBAL_BONUS_ASPECTS`] no longer carries `tohit`; the Strength half belongs here.
///
/// `absorb` is keyed separately from `heal` because the server reads Strength at the mod's own
/// attrib offset (`attribmod.c` `mod_Fill`). Power Boost happens to list `Heal_Dmg` and
/// `Absorb` in one template, but every +Heal set bonus targets `Heal_Dmg` alone and the corpus
/// carries hundreds of heal-only and absorb-only Strength templates, so the two keys diverge.
/// The absorb rows reach this entry through the registry's `strengthAspect`.
///
/// Unlike [`GLOBAL_BONUS_ASPECTS`] these fields are stored as FRACTIONS, not percentages
/// (`crate::strength`), so they add without the /100.
const STRENGTH_ASPECTS: &[(&str, &str)] = &[
    ("defense", "strengthDefense"),
    ("heal", "strengthHeal"),
    ("absorb", "strengthAbsorb"),
    ("tohit", "strengthToHit"),
    ("immobilize", "strengthMez"),
    ("hold", "strengthMez"),
    ("stun", "strengthMez"),
    ("sleep", "strengthMez"),
    ("confuse", "strengthMez"),
    ("fear", "strengthMez"),
];

/// The build-wide global bonus for an aspect, as a fraction: the beta
/// `convertGlobalBonusesToAspects` (each dashboard percent over 100), then `|| 0`, plus the
/// matching +Strength self-buff ([`STRENGTH_ASPECTS`]).
pub fn global_for_aspect(g: &GlobalBonuses, aspect: &str) -> f64 {
    let global = GLOBAL_BONUS_ASPECTS
        .iter()
        .find(|(mapped, _)| *mapped == aspect)
        .and_then(|(_, field)| g.get(field))
        .map(|percent| percent / 100.0)
        .unwrap_or(0.0);
    let strength = STRENGTH_ASPECTS
        .iter()
        .find(|(mapped, _)| *mapped == aspect)
        .and_then(|(_, field)| g.get(field))
        .unwrap_or(0.0);
    global + strength
}

/// Compose a three-tier for a base value under a named aspect.
fn tier_for_aspect(
    aspect: &str,
    base: f64,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    clamps: ReductionClamps,
) -> ThreeTier {
    tier_for_aspects(aspect, aspect, base, enhancement, g, clamps)
}

/// The general form: the slotted bonus reads `enhancement_aspect`, the build-wide global and
/// +Strength read `strength_aspect`. They differ only where one boost enhances several attribs
/// (see [`crate::effect_registry::EffectDisplayConfig::strength_aspect`]).
fn tier_for_aspects(
    enhancement_aspect: &str,
    strength_aspect: &str,
    base: f64,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    clamps: ReductionClamps,
) -> ThreeTier {
    let slotted = enhancement.get(enhancement_aspect);
    let global = global_for_aspect(g, strength_aspect);
    if is_reduction_aspect(enhancement_aspect) {
        ThreeTier::reduction(base, slotted, global, clamps.for_aspect(enhancement_aspect))
    } else {
        ThreeTier::multiplicative(base, slotted, global)
    }
}

/// Whether an authored effect value declares that the caster's Strength never reaches it.
///
/// The converters stamp this from the source template's `IgnoreStrength` flag on every route
/// a value can arrive by (the parent power, the pseudo-pet redirect list and the pet entity
/// table), because the server reads Strength per template (`attribmod.c` `mod_Fill`: the
/// `f *= fStr` multiply is inside an `if(ptemplate->bAllowStrength …)`), not per power. Two
/// halves of one toggle can therefore differ, and Dark Servant's Darkest Night is the case
/// that does: its −ToHit is enhanceable and its −Damage is not (ENT-4).
fn ignores_strength(value: &Value) -> bool {
    value.get("ignoreStrength").and_then(Value::as_bool) == Some(true)
}

/// The authored entry a row's BASE was actually read from.
///
/// A `canBeByType` key the config doesn't expand (`slow`, `damageDebuff`) collapses to its FIRST
/// entry ([`effect_base_value`] does the same), so the row shows one axis's number under a summary
/// label like `-Speed (FlyJmpHJmpRun)`. The mark has to follow that same entry: Ice Arrow's slow
/// marks three of its four axes, and reading `ignoreStrength` off the outer object, which never
/// carries one, reported the whole row enhanceable.
fn collapsed_source<'a>(value: &'a Value, config: &EffectDisplayConfig) -> &'a Value {
    if config.can_be_by_type && is_by_type_object(value) {
        if let Some(first) = by_type_first_value(value) {
            return first;
        }
    }
    value
}

/// The beta `calcEffectThreeTier`: an effect with no enhancement aspect is flat across all
/// three tiers.
fn tier_for_config(
    config: &EffectDisplayConfig,
    base: f64,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    clamps: ReductionClamps,
) -> ThreeTier {
    match config.enhancement_aspect.as_deref() {
        None => ThreeTier::flat(base),
        Some(aspect) => {
            let strength = config.strength_aspect.as_deref().unwrap_or(aspect);
            tier_for_aspects(aspect, strength, base, enhancement, g, clamps)
        }
    }
}

// ---------------------------------------------------------------------------
// base-value resolution: the beta `getEffectBaseValue`
// ---------------------------------------------------------------------------

/// Resolve an authored effect value to its displayable base quantity.
///
/// A value carrying [`SCALE_TERMS_KEY`] resolves each term through its OWN table and sums the
/// resolved magnitudes, because that's the only order in which they're addable: the game
/// computes every AttribMod's magnitude as `scale × table[class][level]` at the template's own
/// table (`attribmod.c` `mod_Fill`, and `uiPowerInfo.c` `modGetMagnitudeAndDuration` for the
/// display), and where it reduces several templates to one number it accumulates the resolved
/// magnitudes (`getTotalDamage`). Adding the scales first is the operation the game performs
/// nowhere, and it's ENT-8. A term that resolves to nothing contributes nothing, exactly as a
/// lone unresolvable value produces no row.
fn effect_base_value(
    value: &Value,
    config: &EffectDisplayConfig,
    scope: TableScope<'_>,
    db: &PowerDatabase,
    level: i32,
    buff_debuff_modifier: f64,
) -> Option<f64> {
    // A by-type value the config doesn't expand collapses to its first entry, and that entry
    // can itself carry terms, so the collapse happens here rather than inside
    // [`term_base_value`]. A term is never a by-type object, and reading the terms off the
    // uncollapsed object would find none.
    let value = if config.can_be_by_type && is_by_type_object(value) {
        by_type_first_value(value)?
    } else {
        value
    };
    let Some(terms) = value.get(SCALE_TERMS_KEY).and_then(Value::as_array) else {
        return term_base_value(value, config, scope, db, level, buff_debuff_modifier);
    };
    let resolved: Vec<f64> = terms
        .iter()
        .filter_map(|term| term_base_value(term, config, scope, db, level, buff_debuff_modifier))
        .collect();
    (!resolved.is_empty()).then(|| resolved.iter().sum())
}

/// One `scale × table` term's displayable base quantity.
///
/// `buff_debuff_modifier` is a multiplier applied ONLY on the table-less fallback paths.
/// The engine always passes 1.0, and nothing supplies another value: the beta used to scale
/// it by archetype for Defender/Controller primaries, a rule keyed on archetype NAMES that
/// could never come into the engine (Rule 0), and PROD6B-2b deleted it after measuring
/// that it matched no row that reaches these paths. The fallbacks themselves are
/// NOT unreachable: ~6-7k rows per fork land here, all of them the `accuracy` key on pool
/// and epic-pool powers, whose set ids the deleted rule never matched.
fn term_base_value(
    value: &Value,
    config: &EffectDisplayConfig,
    scope: TableScope<'_>,
    db: &PowerDatabase,
    level: i32,
    buff_debuff_modifier: f64,
) -> Option<f64> {
    // Each term names the class it resolves against, because one key's terms can come from
    // two pets of different classes (ENT-10).
    let scope = scope_of(value, scope);
    // Every table read resolves at the character's level, the way the game resolves one:
    // the client's power-info window reads `class_GetNamedTableValue(pclass, table, iLevel)`
    // with `iLevel` taken from `e->pchar->iLevel` / `iCombatLevel`, and the runtime uses
    // `iEffCombatLevel` (`attribmod.c` `mod_Fill`). The beta pinned 50 here for the mez /
    // buff-debuff / percent reads while resolving by-type and heal/absorb at the build level;
    // ~70% of the AT tables vary by level, so the pin overstated a level-10 magnitude by
    // roughly 2× (PROD6B-2c).
    let at_table = |table: &str| table_value(db, scope, table, level);

    if config.format == EffectFormat::Mag {
        if let Some(number) = value.as_f64() {
            return Some(number);
        }
        if mez_magnitude(value).is_some() {
            // A mez states ONE resolved number either way — `|scale × table|`, in seconds
            // when the atom says `Duration` and in magnitude points when it says
            // `Magnitude`. What the flat `mag` beside it means depends on the same field,
            // so it is never the value here: on a `Duration` row it is the rank the mez
            // grabs, and on a `Magnitude` row it is the def compiler's unscaled 1.0.
            return match mez_attrib_type(value) {
                Some(AttribType::Duration) | Some(AttribType::Magnitude) => {
                    let (scale, table) = (scale_of(value)?, table_of(value)?);
                    Some((scale * at_table(table)?).abs())
                }
                // Expression-valued, constant-valued and unstated rows have no resolvable
                // base; the row is built from the quantity alone. `Constant` belongs on this
                // side for a reason the other two do not share: the game computes
                // `scale × table` for it and then discards the result, taking both numbers
                // off the template instead (`attribmod.c:1074`, `uiPowerInfo.c:130`). So the
                // product above is not this row's value — it is the number the engine threw
                // away.
                Some(AttribType::Expression) | Some(AttribType::Constant) | None => None,
            };
        }
        // No `mag` (knockback / knockup / repel): the table resolves a distance.
        if let (Some(scale), Some(table)) = (scale_of(value), table_of(value)) {
            if let Some(resolved) = at_table(table) {
                return Some((scale * resolved).abs());
            }
        }
        return scale_of(value);
    }

    if let Some(face) = config.calculation {
        let scale = scale_of(value);
        // A flat-percent-per-scale effect ignores the AT-table reference its data carries.
        if let (Some(flat), Some(scale)) = (config.flat_percent_per_scale, scale) {
            return Some((scale * flat).abs());
        }
        if let (Some(scale), Some(table)) = (scale, table_of(value)) {
            if let Some(resolved) = at_table(table) {
                return Some((scale * resolved).abs() * 100.0);
            }
        }
        // Table-less fallback: the canonical 10%-per-scale (buff) / 5% (debuff) rule.
        let fraction = match scale {
            Some(scale) if scale != 0.0 => scale * face.base_rate() * buff_debuff_modifier,
            _ => 0.0,
        };
        return Some(fraction * 100.0);
    }

    let scale = scale_of(value)?;

    if config.format == EffectFormat::Percent {
        if let Some(table) = table_of(value) {
            if let Some(resolved) = at_table(table) {
                return Some((scale * resolved).abs() * 100.0);
            }
        }
        return Some(scale * config.percent_multiplier() * buff_debuff_modifier);
    }

    // Heal / absorb amounts resolve their scale through the table into an HP amount.
    if config.value_from_table {
        if let Some(table) = table_of(value) {
            if let Some(resolved) = table_value(db, scope, table, level) {
                return Some(scale * resolved);
            }
        }
    }

    Some(scale)
}

/// The percent form of a Max-HP-fraction effect: an authored `maxHPFraction`, or a scale on
/// a `*_Ones` table (both mean "this fraction of the target's Max HP"). `None` when the
/// value is a real amount resolved through a heal table.
fn max_hp_fraction_percent(value: &Value) -> Option<f64> {
    if let Some(fraction) = value.get("maxHPFraction").and_then(Value::as_f64) {
        return Some(fraction * 100.0);
    }
    let table = table_of(value)?;
    if !table.to_lowercase().ends_with(ONES_TABLE_SUFFIX) {
        return None;
    }
    Some(scale_of(value)? * 100.0)
}

// ---------------------------------------------------------------------------
// the display bag: the beta `buildDisplayEffects`
// ---------------------------------------------------------------------------

/// The display bag's SEED, projected from the power's atoms.
///
/// This used to clone the power's authored wire bag, which made it the load-bearing site of the
/// writer-side strip: ~60% of every bag-carrying power's display keys arrived here and nowhere
/// else. [`crate::window_slots::bag_slots`] is the atom-side mirror of the converter's own
/// projection, held to the authored bag key-for-key and value-for-value by
/// `display_slot_presence_atom_bag_parity` / `display_slot_value_atom_bag_parity` over every
/// power of all four bundles; `display_seed_atom_native` then grades this function's whole
/// output, and holds a non-summoning bag carrier to EXACT equality with and without the wire.
///
/// Three things the mirror carries as fields rather than slots are written back as the ordinary
/// bag keys they are: the `durations` map, and the `buffDuration` / `effectDuration`
/// power-level fallbacks.
///
/// **`summon` is still the wire bag's, and it is why the bag cannot leave the contract.** Its
/// value is `extractSummon`'s, built from the template's pet parameters outside the projection
/// this mirrors, so [`crate::window_slots::WindowSlots`] states the key without a value and no
/// atom on the summoner says what it holds (ENT-14, 17 powers). Reading it here keeps the
/// function honest about the gap instead of dropping a registered row on the surfaces that
/// render it. Every other key the old seed owned and the atoms do not state — the execution
/// stats, the `damage` array, `effectArea` — has a `stats` or top-level home, and the overlay
/// below already read it from there and overwrote the seed's copy.
fn atom_seed(power: &Power, dataset: DatasetId, db: &PowerDatabase) -> Map<String, Value> {
    let classes = db.player_classes();
    let slots = crate::window_slots::bag_slots(power, dataset, &classes);

    let mut bag = slots.values;
    if !slots.durations.is_empty() {
        let durations = slots
            .durations
            .iter()
            .filter_map(|(key, d)| {
                serde_json::Number::from_f64(*d).map(|n| ((*key).to_owned(), Value::Number(n)))
            })
            .collect();
        bag.insert("durations".to_owned(), Value::Object(durations));
    }
    insert_number(&mut bag, "buffDuration", slots.buff_duration);
    insert_number(&mut bag, "effectDuration", slots.effect_duration);

    // Off the power's own `summon`, not a bag slot. The display bag keeps the key — its
    // consumers ask for `summon` and the shape is unchanged — but the source is now the one
    // address the converter writes, so this row stops depending on whether a strip left the
    // power an `effects` object (ENT-22).
    if let Some(summon) = power.summon() {
        bag.insert("summon".to_owned(), summon.clone());
    }
    sort_keys_deep(bag)
}

/// `_sortKeysDeep` (`scripts/convert-powerset.cjs:5933`), the converter's "canonical emit order"
/// pass over the finished bag.
///
/// Object order is load-bearing on both sides of the wire and always has been — the workspace
/// takes serde_json's `preserve_order` for exactly that reason — and two readers here consume it:
/// [`collapsed_source`] takes the FIRST entry of a by-type value to decide the collapsed row's
/// `ignoreStrength` mark, and the display renders rows in bag order with nothing sorting them
/// afterwards. So a mirror that reproduces the bag's contents in a different order is not the
/// same bag.
///
/// It bit immediately. Thunderspy's eight Dark Nova / Black Dwarf attacks author `slow` over four
/// axes where only `jumpHeight` is `IgnoreStrength`; the atoms arrive in template order
/// (jumpHeight first) and the shipped bag is sorted (flySpeed first), so the collapsed row's mark
/// flipped true. `display_slot_value_atom_bag_parity` could not see it, because `serde_json`'s
/// map equality under `preserve_order` compares as a set of pairs and ignores order.
fn sort_keys_deep(value: Map<String, Value>) -> Map<String, Value> {
    let mut sorted: Vec<(String, Value)> = value.into_iter().collect();
    sorted.sort_by(|(a, _), (b, _)| a.cmp(b));
    sorted
        .into_iter()
        .map(|(key, value)| (key, sort_value(value)))
        .collect()
}

fn sort_value(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(sort_keys_deep(map)),
        Value::Array(items) => Value::Array(items.into_iter().map(sort_value).collect()),
        other => other,
    }
}

/// Build the bag the surfaces resolve, from the power's atoms (PROD6C-3a).
///
/// The projected bag isn't what `RegistryEffectsDisplay` renders: InfoPanel and
/// PowerInfoTooltip each built their own bag at the render edge, now single-sourced beta-side
/// as `buildDisplayEffects`, which this mirrors. Four transforms, all pure functions of the
/// power object: the merged execution `stats`, a heal authored inside the `damage` array, a
/// summon's duration standing in for a missing buff duration, and the flattened `movement`
/// object.
///
/// The seed under those transforms is [`atom_seed`], not the wire `effects` object. The beta
/// still starts from the authored bag, so this is the one place the two sides differ by
/// construction rather than by defect — and `display_seed_atom_native` is what holds the
/// difference to zero on every non-summoning bag carrier of the frozen corpus.
///
/// Two further transforms run on that built bag, in the order the surfaces apply them:
/// `targets_hit`, the power's own stacking-slider input (PROD6C-3b), then the pseudo-pet merge
/// (PROD6C-3c).
///
/// `power` is the EFFECTIVE power ([`crate::effective`], PROD6C-3k), meaning the snipe's fast form
/// and the merged mode-gated contributions. `stacking_source` is the power the build holds, which
/// is what the slider reads: the surfaces offer the slider for the base power and scale the merged
/// bag with it. The one transform still outside both is the Kheldian / Primalist form redirect,
/// whose variant powers the bundle doesn't carry.
pub fn display_effects(
    power: &Power,
    stacking_source: &Power,
    targets_hit: Option<u32>,
    dataset: DatasetId,
    db: &PowerDatabase,
) -> Map<String, Value> {
    use crate::projection::{base_endurance_cost, object_number, truthy_stat};

    let mut bag = atom_seed(power, dataset, db);

    // What the build's live conditionals add, stamped on the effective power by
    // [`crate::effective`]. The seed above cannot hold it: every conditional's atoms are `gated`
    // on all four forks and the bag mirror's subset drops gated rows, so an active control
    // contributed nothing to this bag between the seed's swap to the atoms and this line. It
    // sits here, before the stats-first overlay, because that is where the merged bag it
    // replaces used to sit — an execution stat still answers from `stats` over a conditional's
    // copy of it, exactly as it did when the whole merged bag was the seed.
    if let Some(delta) = extra_object(power, crate::effective::CONDITIONAL_DELTA_KEY) {
        for (key, value) in delta {
            bag.insert(key.clone(), value.clone());
        }
    }

    // Execution stats live on `stats` for primary/secondary powers and in the bag itself for
    // pool/epic ones. These are the same stats-first reads the projection's own fields use, so
    // one rule serves both. A zero is no stat, leaving any authored bag value standing.
    insert_number(&mut bag, "enduranceCost", base_endurance_cost(power));
    for key in [
        "recharge",
        "accuracy",
        "range",
        "castTime",
        "radius",
        "maxTargets",
    ] {
        insert_number(&mut bag, key, truthy_stat(power, key, key));
    }

    // Both partitions author the arc in the binary's radians; the registry row is degrees.
    let raw_arc = object_number(extra_object(power, "stats"), "arc")
        .or_else(|| object_number(extra_object(power, "effects"), "arc"));
    insert_number(&mut bag, "arc", raw_arc.map(crate::procs::arc_to_degrees));

    if !is_truthy(bag.get("healing")) {
        let damage = power
            .extra
            .get("damage")
            .or_else(|| extra_object(power, "effects").and_then(|e| e.get("damage")));
        if let Some(healing) = damage.and_then(healing_from_damage) {
            bag.insert("healing".to_string(), healing);
        }
    }

    // A summon with no duration effect of its own displays the pet's lifespan.
    let has_duration = is_truthy(bag.get("buffDuration")) || is_truthy(bag.get("effectDuration"));
    if !has_duration {
        let duration = bag.get("summon").and_then(|summon| summon.get("duration"));
        if is_truthy(duration) {
            let duration = duration.cloned().unwrap_or(Value::Null);
            bag.insert("buffDuration".to_string(), duration);
        }
    }

    // The movement buffs (Super Jump, Fly, Sprint) as the registry's per-axis keys.
    //
    // Read from the ATOMS, because an axis can carry two of them: Sprint's run speed is
    // `RunningSpeed 0.5 Melee_Ones` plus its `IgnoreStrength` twin, and the game's own
    // monitor shows both, the second tagged "Ignores buffs and enhancements". The bag's
    // nested `movement` object holds one value per axis and so can only ever show one —
    // it is the `?? bag` fallback here for the same reason it is one in the apply pass.
    // The `<key>Unenhanced` spelling is what the ENT-6 split-row walk below reads.
    {
        let entries = crate::appliers::movement::movement_buff_value(power)
            .map(|v| {
                v.into_iter()
                    .map(|(axis, m)| {
                        let mut o = serde_json::Map::new();
                        o.insert("scale".into(), Value::from(m.scale));
                        if let Some(table) = m.table.as_deref() {
                            o.insert("table".into(), Value::from(table));
                        }
                        (axis.to_string(), m.ignore_strength, Value::Object(o))
                    })
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                let movement = bag.get("movement").and_then(Value::as_object)?;
                Some(
                    movement
                        .iter()
                        .map(|(axis, value)| (axis.clone(), false, value.clone()))
                        .collect(),
                )
            })
            .unwrap_or_default();
        let axis_counts: std::collections::HashMap<&str, usize> =
            entries
                .iter()
                .fold(std::collections::HashMap::new(), |mut m, (a, _, _)| {
                    *m.entry(a.as_str()).or_default() += 1;
                    m
                });
        for (axis, ignore_strength, value) in &entries {
            let (axis, ignore_strength, value) = (axis, *ignore_strength, value);
            let flat_key = match axis.as_str() {
                "flySpeed" => "fly",
                "runSpeed" => "runSpeed",
                "jumpSpeed" => "jumpSpeed",
                "jumpHeight" => "jumpHeight",
                // The twin's own `<axis>Unenhanced` split slots (FLYPOOL-1), reachable
                // only on the bag-fallback arm — the atom arm derives its split from
                // `ignore_strength` below and never names these axes.
                "flySpeedUnenhanced" => "flyUnenhanced",
                "runSpeedUnenhanced" => "runSpeedUnenhanced",
                "jumpSpeedUnenhanced" => "jumpSpeedUnenhanced",
                "jumpHeightUnenhanced" => "jumpHeightUnenhanced",
                _ => continue,
            };
            if !is_truthy(Some(value)) {
                continue;
            }
            // Paired axes only, matching the apply pass: a LONE `IgnoreStrength`
            // entry keeps the plain key, because the totals still enhance it and a
            // row marked "ignores buffs and enhancements" beside a number that
            // moved would be worse than either reading. MOVEMAP-1.
            let paired = axis_counts.get(axis.as_str()).copied().unwrap_or(0) > 1;
            let key = if ignore_strength && paired {
                format!("{flat_key}Unenhanced")
            } else {
                flat_key.to_string()
            };
            bag.insert(key, value.clone());
        }
    }

    crate::stacking::adjust_display_bag(&mut bag, stacking_source, targets_hit);

    // The pseudo-pet debuffs merge UNDER the power's own bag, and after the slider, which is the
    // order both surfaces spread them in (PROD6C-3c).
    let mut merged = pseudo_pet_effects(power, db);
    if merged.is_empty() {
        return bag;
    }
    for (key, value) in bag {
        merged.insert(key, value);
    }
    merged
}

/// Where one pet effect type goes, out of the three consumers a pet's kit can reach.
///
/// A pet effect type is a string on the wire, so a consumer can only route the names it knows
/// and drops the rest without a word. That silence hid `RegenDebuff` until ENT-7 went looking
/// for it, and then six more types behind it: a summoning power showed and contributed nothing
/// for its pet's knockback, taunt, endurance drain, −Recovery or heal (ENT-9). Every type both
/// converters can emit is therefore named HERE, with [`PetEffectRoute::Undisplayed`] a stated
/// verdict rather than a fallthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PetEffectRoute {
    /// Merges into the summoning power's display bag as a `{scale, table}` row, under the key a
    /// PARENT power carrying the same mechanic already writes, so a pet's −Recharge and a
    /// parent power's land in one slot rather than two vocabularies.
    Scalar(&'static str),
    /// Merges as a control row `{mag, scale, table}`, the shape a directly-applied control
    /// effect uses, so the registry resolves magnitude and duration the same way for both.
    Control(&'static str),
    /// Folded into the player's totals by [`crate::buff_pets::apply_buff_pet_auras`] rather than
    /// into a display bag: these are the ally buffs a "floaty" buff-pet projects.
    AllyAura,
    /// The non-applied FACE of an applied type: a pet's mez protection, its mez resistance or its
    /// debuff resistance. Folded into the summoner's totals by the same pass as [`Self::AllyAura`],
    /// through the [`FacedRoute`] this carries.
    ///
    /// The route isn't the whole decision, and this is the axis a type name cannot carry: measured
    /// over the build-reachable corpus, the faced rows split 100 summoner-facing to 50 foe-facing on
    /// Homecoming, 10 to 78 on Rebirth and 15 to 42 on Thunderspy, with the SAME type names at the
    /// same aspect on both sides. Force Field Generator's bubble and Faraday Cage's whole kit are
    /// the summoner's; Singularity's post-hold knock immunity is the mob's. Every row is authored
    /// `target: AnyAffected`, so only the pet POWER's `targetsAffected` tells them apart, and the fold
    /// gates on it before consulting this at all (ENT-12).
    Faced(FacedRoute),
    /// The pet's OWN stat sheet: a row off a `target: Self` template, read by the surface that
    /// shows what the pet IS and by nothing here.
    ///
    /// Distinct from [`Self::Faced`], which it superficially resembles: a faced row is addressed
    /// to somebody else (`AnyAffected`) and the whole difficulty is deciding WHO, which is why
    /// that route carries a [`FacedRoute`] and gates on `targetsAffected`. These rows name their
    /// recipient outright, and it's the pet, so there's no recipient question and therefore no
    /// fold: the summoner's totals must never absorb them, which is what the `Self…` prefix on
    /// the type names exists to keep true. `DefenseBuff` and `SelfDefense` are the same mechanic
    /// on two different characters, and only the name keeps a pet's 45% off the player's sheet.
    ///
    /// Distinct from [`Self::Undisplayed`] too, and not a synonym for "the engine ignores it":
    /// these have a consumer, it's just not a Rust one. `pet_effect_routes` asserts no reachable
    /// type is `Undisplayed`, so folding them there would say something false about them.
    OwnStat,
    /// Emitted by a converter and deliberately not consumed.
    ///
    /// After ENT-12 step 2 the corpus's one occupant is Homecoming's `HealResist`, and step 3
    /// re-measured why: not merely that its carriers are foe-facing, but that no build reaches one
    /// at all. All three are Homecoming's (Rebirth and Thunderspy ship the name nowhere), all three
    /// are `["Foe"]`-facing, and none is reachable: `IncarnatePets_Lore_Knives_LT` and its `_Buff`
    /// twin are commandable, so the walk stops at them by design, and `Pets_Arachnos_Corrosive_Blast`
    /// is non-commandable but named by no shipped summon and reached by no `createsEntities` chain.
    /// So `GlobalBonuses::heal_received` modelling the same `Res(Heal)` mechanic for Incandescence
    /// Destiny settles nothing here: which direction a POSITIVE `Res(Heal)` on a foe moves the healing
    /// that foe receives is unsettled, and there's no reachable row to settle it against even if it
    /// were. Reachability is graded rather than asserted
    /// (`pet_faced_fold::heal_resist_reaches_no_build`), because a fork moving the type onto a
    /// summonable pet is exactly when somebody has to answer the direction question.
    ///
    /// The variant also catches a face of a known applied type the corpus doesn't ship
    /// (`SlowResist`, `TauntProtection`), which is the point of it being a verdict: a face that
    /// arrives must be routed deliberately, not read as the effect it protects from.
    Undisplayed,
}

/// Where one faced row's value lands in [`GlobalBonuses`], and in which unit.
///
/// Three facts rather than one key, because the three families don't share a router OR a unit:
/// protection is a flat magnitude and the two resistances are percentages, and the field a router
/// picks isn't always derivable from the key it takes (`add_debuff_resistance("tohit")` writes
/// `debuff_resist_to_hit`, `add_mez_protection("knockup")` writes `protection_knockback`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FacedRoute {
    pub family: FacedFamily,
    /// The LOWERCASE type key [`FacedFamily`]'s router takes.
    pub type_key: &'static str,
    /// The camelCase [`GlobalBonuses`] field a breakdown row for this face is labelled with,
    /// which is the field the router WRITES, not a restatement of [`Self::type_key`]. `None` for a
    /// key the router declares unspent, where there's no field to name.
    pub breakdown_key: Option<&'static str>,
}

/// The [`GlobalBonuses`] router one faced family goes through, which also fixes its unit.
///
/// Both are the parent power route's, so a pet's protection and a power's own land in one slot in
/// one unit ([`crate::apply`] Pass 2b).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacedFamily {
    /// [`GlobalBonuses::add_mez_protection`]: a flat MAGNITUDE, `scale × table[level]`, no
    /// `× 100`.
    Protection,
    /// [`GlobalBonuses::add_mez_resistance`]: a PERCENTAGE, `scale × table[level] × 100`.
    MezResistance,
    /// [`GlobalBonuses::add_debuff_resistance`]: a PERCENTAGE, same unit as
    /// [`Self::MezResistance`].
    DebuffResistance,
}

/// Every faced type name either converter ships, and where it goes.
///
/// The 24 names are measured over both routes and all three forks, not derived from the applied
/// vocabulary: a face exists only where a template actually carries that aspect, so of the 29 applied
/// types 8 have both faces, 8 have only `Resist`, and 13 have neither. `HealResist` is the one shipped
/// name with no entry here. See [`PetEffectRoute::Undisplayed`] for why.
///
/// Every `type_key` is the one the SAME router already takes from a parent power carrying the same
/// mechanic, so Force Field Generator's hold protection and an armor set's land in `protection_hold`
/// together rather than in two vocabularies (the ENT-9 shape).
///
/// What the reachable corpus exercises is a smaller set than the table. Of the 23, 18 sit on a pet
/// some power summons and 16 of those reach a summoner. `KnockbackResist` and `KnockupResist` are
/// reachable but occur only foe-facing. The other 5 (the mez `Hold`, `Stun`, `Immobilize` and
/// `Sleep` `Resist` names, plus `TauntResist`) are declared with no reachable carrier at all,
/// because their destination is the parent route's own and their mapping is the same mechanical
/// one. A fork moving one onto a reachable ally aura should fold it rather than drop it.
/// A separate fold restates the whole table so a wrong entry cannot agree with itself.
const PET_FACE_ROUTES: &[(&str, FacedRoute)] = &[
    face(
        "HoldProtection",
        FacedFamily::Protection,
        "hold",
        "protHold",
    ),
    face(
        "StunProtection",
        FacedFamily::Protection,
        "stun",
        "protStun",
    ),
    face(
        "ImmobilizeProtection",
        FacedFamily::Protection,
        "immobilize",
        "protImmobilize",
    ),
    face(
        "SleepProtection",
        FacedFamily::Protection,
        "sleep",
        "protSleep",
    ),
    face(
        "ConfuseProtection",
        FacedFamily::Protection,
        "confuse",
        "protConfuse",
    ),
    face(
        "FearProtection",
        FacedFamily::Protection,
        "fear",
        "protFear",
    ),
    // Knockback and knockup protection are ONE physical stat: `add_mez_protection` routes both to
    // `protection_knockback`, and the corpus grants them only as a pair at an equal scale and table
    // (every one of the pet table's `KnockupProtection` rows has a `KnockbackProtection` beside it,
    // all three forks). The fold maxes the pair before adding, exactly as the parent route does for
    // a power's own. Summing would double Faraday Cage's KB protection.
    face(
        "KnockbackProtection",
        FacedFamily::Protection,
        "knockback",
        "protKnockback",
    ),
    face(
        "KnockupProtection",
        FacedFamily::Protection,
        "knockup",
        "protKnockback",
    ),
    face(
        "HoldResist",
        FacedFamily::MezResistance,
        "hold",
        "mezResistHold",
    ),
    face(
        "StunResist",
        FacedFamily::MezResistance,
        "stun",
        "mezResistStun",
    ),
    face(
        "ImmobilizeResist",
        FacedFamily::MezResistance,
        "immobilize",
        "mezResistImmobilize",
    ),
    face(
        "SleepResist",
        FacedFamily::MezResistance,
        "sleep",
        "mezResistSleep",
    ),
    face(
        "ConfuseResist",
        FacedFamily::MezResistance,
        "confuse",
        "mezResistConfuse",
    ),
    face(
        "FearResist",
        FacedFamily::MezResistance,
        "fear",
        "mezResistFear",
    ),
    face(
        "KnockbackResist",
        FacedFamily::MezResistance,
        "knockback",
        "mezResistKnockback",
    ),
    // Routed to the key `add_mez_resistance` declares UNSPENT rather than skipped here, so the
    // decision lives at the router with its warrant, the same reason `apply` still emits knockup
    // from a power's own bag. The warrant extends to pet rows on measurement: every `KnockupResist`
    // in the pet table is paired with a `KnockbackResist` at an equal scale and table, so the
    // knockback key already carries the pair's value.
    face_unspent("KnockupResist", FacedFamily::MezResistance, "knockup"),
    face(
        "TauntResist",
        FacedFamily::MezResistance,
        "taunt",
        "mezResistTaunt",
    ),
    face(
        "ToHitDebuffResist",
        FacedFamily::DebuffResistance,
        "tohit",
        "debuffResistToHit",
    ),
    face(
        "RegenDebuffResist",
        FacedFamily::DebuffResistance,
        "regeneration",
        "debuffResistRegeneration",
    ),
    face(
        "RechargeDebuffResist",
        FacedFamily::DebuffResistance,
        "recharge",
        "debuffResistRecharge",
    ),
    face(
        "RecoveryDebuffResist",
        FacedFamily::DebuffResistance,
        "recovery",
        "debuffResistRecovery",
    ),
    face(
        "EndDrainResist",
        FacedFamily::DebuffResistance,
        "endurance",
        "debuffResistEndurance",
    ),
    face(
        "DefenseDebuffResist",
        FacedFamily::DebuffResistance,
        "defense",
        "debuffResistDefense",
    ),
];

const fn face(
    pet_type: &'static str,
    family: FacedFamily,
    type_key: &'static str,
    breakdown_key: &'static str,
) -> (&'static str, FacedRoute) {
    (
        pet_type,
        FacedRoute {
            family,
            type_key,
            breakdown_key: Some(breakdown_key),
        },
    )
}

const fn face_unspent(
    pet_type: &'static str,
    family: FacedFamily,
    type_key: &'static str,
) -> (&'static str, FacedRoute) {
    (
        pet_type,
        FacedRoute {
            family,
            type_key,
            breakdown_key: None,
        },
    )
}

/// Every effect type either pet converter can emit, and where it goes.
///
/// Every display key here is the one `convert-powerset.cjs` writes when a PARENT power carries
/// the same mechanic, so the two routes land in one slot rather than growing a parallel
/// vocabulary, which is what lets Rebirth and Thunderspy deliver through a pet (Siphon Power's
/// ally +damage, Fulcrum Shift's foe −damage, Transference's ally endurance) what Homecoming
/// carries on the parent, and be read the same way (ENT-3).
///
/// A [`PetEffectRoute::Scalar`] entry describes the common case and guarantees nothing about
/// enhanceability: a row may carry `ignoreStrength`, and then it lands in the same key with
/// that mark, which [`resolve_granted_magnitudes`] renders flat across all three tiers. That's
/// the game's own rule for such a template: shown, never boosted (ENT-4). The parent route
/// follows it too, so neither route can overstate what the other understates.
///
/// `Knockback`, `Knockup` and `Taunt` are `Scalar` and not `Control` because the parent route
/// writes all three with no magnitude, and the registry's `mag` format then reads them as a
/// distance rather than a duration. The pet rows do carry a `magnitude` the parent's don't; it's
/// the template's own field, no consumer of these keys reads it, and [`merged_contributions`]
/// builds the merged value from scale and table alone, so it stays out of the number rather
/// than being invented into it (ENT-9).
const PET_EFFECT_ROUTES: &[(&str, PetEffectRoute)] = &[
    ("Slow", PetEffectRoute::Scalar("slow")),
    ("DefenseDebuff", PetEffectRoute::Scalar("defenseDebuff")),
    ("ToHitDebuff", PetEffectRoute::Scalar("tohitDebuff")),
    (
        "ResistanceDebuff",
        PetEffectRoute::Scalar("resistanceDebuff"),
    ),
    ("DamageDebuff", PetEffectRoute::Scalar("damageDebuff")),
    ("DamageBuff", PetEffectRoute::Scalar("damageBuff")),
    ("EnduranceGain", PetEffectRoute::Scalar("enduranceGain")),
    ("RegenDebuff", PetEffectRoute::Scalar("regenDebuff")),
    ("RechargeDebuff", PetEffectRoute::Scalar("rechargeDebuff")),
    (
        "MovementCapDebuff",
        PetEffectRoute::Scalar("movementCapDebuff"),
    ),
    ("Knockback", PetEffectRoute::Scalar("knockback")),
    ("Knockup", PetEffectRoute::Scalar("knockup")),
    ("Taunt", PetEffectRoute::Scalar("taunt")),
    ("EndDrain", PetEffectRoute::Scalar("enduranceDrain")),
    ("RecoveryDebuff", PetEffectRoute::Scalar("recoveryDebuff")),
    ("Heal", PetEffectRoute::Scalar("healing")),
    ("Hold", PetEffectRoute::Control("hold")),
    ("Stun", PetEffectRoute::Control("stun")),
    ("Sleep", PetEffectRoute::Control("sleep")),
    ("Fear", PetEffectRoute::Control("fear")),
    ("Confuse", PetEffectRoute::Control("confuse")),
    ("Immobilize", PetEffectRoute::Control("immobilize")),
    ("DefenseBuff", PetEffectRoute::AllyAura),
    ("ResistanceBuff", PetEffectRoute::AllyAura),
    ("Absorb", PetEffectRoute::AllyAura),
    ("RegenBuff", PetEffectRoute::AllyAura),
    ("RecoveryBuff", PetEffectRoute::AllyAura),
    ("ToHitBuff", PetEffectRoute::AllyAura),
    ("RechargeBuff", PetEffectRoute::AllyAura),
    ("SelfResistance", PetEffectRoute::OwnStat),
    ("SelfDefense", PetEffectRoute::OwnStat),
    ("SelfMezProtection", PetEffectRoute::OwnStat),
    ("SelfMezResistance", PetEffectRoute::OwnStat),
    ("SelfDebuffResistance", PetEffectRoute::OwnStat),
];

/// The route one effect type takes, or `None` for a type this vocabulary doesn't know.
///
/// A non-applied FACE resolves in two steps: [`PET_FACE_ROUTES`] for the 23 names that reach a
/// total, then the suffix rule that BUILDS a face name for the residue. Both converters publish the
/// resistance face of an applied type as `<Type>Resist` and the negative-Current face as
/// `<Type>Protection`, and those suffixes are where the inline route's own `EndDrainResist` /
/// `HoldProtection` names came from. A suffix over an UNKNOWN base is still `None`, so a typo can't
/// enter as "a protection of something", and a face of a KNOWN base that no table claims is
/// [`PetEffectRoute::Undisplayed`], a stated verdict, never the applied effect.
///
/// `None` is a gate failure rather than a runtime one: no caller here has an error channel to
/// carry it, so the enforcement is `convert-pet-entities.cjs` throwing where a type is born and
/// a corpus sweep for the other direction. Public so that
/// gate grades this function and not a second copy of the rule.
pub fn pet_effect_route(effect_type: &str) -> Option<PetEffectRoute> {
    if let Some(route) = PET_EFFECT_ROUTES
        .iter()
        .find(|(pet_type, _)| *pet_type == effect_type)
        .map(|(_, route)| *route)
    {
        return Some(route);
    }
    if let Some(faced) = PET_FACE_ROUTES
        .iter()
        .find(|(pet_type, _)| *pet_type == effect_type)
        .map(|(_, route)| *route)
    {
        return Some(PetEffectRoute::Faced(faced));
    }
    let applied = effect_type
        .strip_suffix("Protection")
        .or_else(|| effect_type.strip_suffix("Resist"))?;
    PET_EFFECT_ROUTES
        .iter()
        .any(|(pet_type, _)| *pet_type == applied)
        .then_some(PetEffectRoute::Undisplayed)
}

/// Whether a type is one of the ally-buff auras [`crate::buff_pets`] folds into the totals.
pub fn is_ally_aura(effect_type: &str) -> bool {
    pet_effect_route(effect_type) == Some(PetEffectRoute::AllyAura)
}

/// The faced family and unit a type folds through, or `None` if it isn't a faced type.
pub fn faced_route(effect_type: &str) -> Option<FacedRoute> {
    match pet_effect_route(effect_type)? {
        PetEffectRoute::Faced(route) => Some(route),
        _ => None,
    }
}

/// Whether [`crate::buff_pets`] folds a row of this type into the summoner's totals: an ally-buff
/// aura, or the protection/resistance face of an applied type.
///
/// One predicate over one vocabulary, asked by the walk itself, so the two families cannot drift
/// into "a type the fold visits but keys nothing" the way ENT-9's did.
pub fn folds_into_summoner_totals(effect_type: &str) -> bool {
    matches!(
        pet_effect_route(effect_type),
        Some(PetEffectRoute::AllyAura | PetEffectRoute::Faced(_))
    )
}

/// Whether a pet ABILITY's recipients include the summoner, read off the pet power's
/// `targetsAffected`.
///
/// The polarity inverts against a player power, and that's the whole difficulty. The pet is the
/// caster, so `Self` (the export's name for `kTargetType_Caster`) is the pet ALONE and the summoner
/// arrives as somebody else's token. Every faced row is authored `target: AnyAffected`, so the atom's
/// own recipient axis discriminates nothing and this field is the only thing separating the bubble a
/// pet puts over its summoner from the knock immunity it hands the foe it just held (ENT-12).
///
/// Each token is classified from the game's own comment on `TargetType`
/// (`Common/entity/powers.h:287`), not from the name:
///
/// - `Friend`: "anyone alive on the same side as the caster except the caster". The summoner is on
///   the pet's side, so YES.
/// - `Teammate`: "any living teammate and their pets except the caster". YES.
/// - `MyOwner`: "the target is the owner of the caster (all the way back up)". The summoner
///   exactly. YES.
/// - `Any`: "any living entity which isn't dead". YES.
/// - `Self`: `kTargetType_Caster`, "the caster, dead or alive", so here the PET. NO.
/// - `Foe`, `DeadFoe`: a different side from the caster. NO.
/// - `DeadPlayerFriend`: "any dead players on the same side". A sheet is read alive. NO.
///
/// Those eight are the whole vocabulary the pet corpus ships (measured, both routes, all three
/// forks); `Friend`, `Self`, `Foe` and `DeadFoe` are the four that occur on a row the fold walks. An
/// unlisted token answers `false`, which is the under-reporting direction and the one this entry's
/// severity is bounded in. A corpus sweep makes a ninth token red a gate
/// rather than silently dropping a buff.
///
/// An ability stating NO recipient is also `false`: absence is not a defaulted recipient, and
/// crediting the summoner on absence is the fabricated-discriminator trap. No shipped ability is in
/// that state (every one of 1410 / 1274 / 1312 states one), which is itself gated by
/// `every_protection_row_states_its_recipient`.
pub fn reaches_summoner(ability: &Value) -> bool {
    ability
        .get("targetsAffected")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .any(|who| matches!(who, "Friend" | "Teammate" | "MyOwner" | "Any"))
}

/// Which slot of the summoning power's display bag one pet effect type merges into.
///
/// [`None`] is a decision here, not a fallthrough. The two extractors this replaces each ended in
/// `_ => None`, so three of the five routes reached the display path through a wildcard and a
/// sixth route would have joined them silently, the shape that hid `RegenDebuff` and then six
/// types behind it (ENT-9). The match below is exhaustive, so a new [`PetEffectRoute`] variant
/// breaks the build until somebody decides what the card says about it.
#[derive(Clone, Copy)]
enum PetDisplaySlot {
    /// A `{scale, table}` row, merged additively on a [`PetBagSlot`] collision.
    Scalar(&'static str),
    /// A `{mag, scale, table}` control row; the strongest instance wins rather than summing.
    Control(&'static str),
}

fn display_slot(effect_type: Option<&str>) -> Option<PetDisplaySlot> {
    match pet_effect_route(effect_type?)? {
        PetEffectRoute::Scalar(key) => Some(PetDisplaySlot::Scalar(key)),
        PetEffectRoute::Control(key) => Some(PetDisplaySlot::Control(key)),
        // An ally buff and a summoner-facing face are the summoner's TOTALS, not the summoning
        // power's card: [`crate::buff_pets`] folds both behind the per-pet opt-in, and writing them
        // here too would show the same contribution twice in two vocabularies.
        //
        // A `Faced` row addressed to somebody ELSE reaches neither, and that's ENT-12 step 3's
        // verdict, the one decision here about the row rather than the type. It's the foe's own
        // stat: a mez'd target's knock immunity. It stays out because the PARENT route already drops
        // the identical rider (a foe-facing `aspect=Resistance` KB row hits
        // `convert-powerset.cjs`'s `!isSelfTargeting` guard on 120 / 85 / 77 powers: Freeze Ray,
        // Electron Shackles, Wide Area Web Grenade), and because giving the pet route a key its
        // twin doesn't write is ENT-9's defect from the other side. Measured over the reachable
        // corpus the whole population is that one rider: `KnockbackResist`, `KnockupResist` and
        // their two `Protection` faces, 50 / 78 / 38 rows over 20 / 23 / 12 summons, with 27 of the
        // 28 carrying abilities applying a control on the same ability. The fold gate holds
        // the silence and the twin's drop together, so the two routes cannot part.
        PetEffectRoute::AllyAura | PetEffectRoute::Faced(_) => None,
        // The pet's own sheet, not the summoning power's card. A summon's card says what casting
        // it does to the world; a pet's 45% smashing resistance is a fact about the pet, and
        // writing it under `resistanceBuff` here would read as the SUMMONER gaining it, the
        // leak [`PetEffectRoute::OwnStat`] names.
        PetEffectRoute::OwnStat => None,
        // Emitted and deliberately unread, so there's nothing for a card to say either. The
        // corpus's one occupant is `HealResist`; see [`PetEffectRoute::Undisplayed`].
        PetEffectRoute::Undisplayed => None,
    }
}

/// The pseudo-pet effects fragment the surfaces merge into a summon power's display bag: the
/// beta `synthesizePseudoPetEffects` (PROD6C-3c, transform (7)).
///
/// Powers like Glue Arrow deliver their enhanceable debuffs through a non-commandable
/// pseudo-pet and carry nothing on the parent, so without this the player's enhancements never
/// reach them. The pet inherits the summoner's enhancements (`CopyBoosts`), which is what makes
/// applying the SUMMONER's slotting to them correct, and it's a different fact from which
/// class the magnitudes RESOLVE against, which is the pet's own. Every row an entity supplies
/// is therefore stamped with that entity's class ([`PET_CLASS_KEY`]) on the way into the bag,
/// and every table read downstream uses the class on the row it's reading (ENT-10).
///
/// Only NON-commandable entities qualify. A commandable pet (Mastermind henchmen, Lore) keeps
/// its own Summons block. Additive debuffs combine on a [`PetBagSlot`] collision
/// ([`merged_contributions`]); control keeps the single strongest instance, and only when it's
/// reliably applied (a sub-1.0 chance would read as a guaranteed mez here).
///
/// Public so the merge-table gate grades the refusal itself and not only its arithmetic:
/// whether two rows were added or kept apart is invisible in the resolved number whenever their
/// tables hold the same value, which is most of the corpus.
pub fn pseudo_pet_effects(power: &Power, db: &PowerDatabase) -> Map<String, Value> {
    let mut enhanceable: Vec<(PetBagSlot, Vec<PetContribution>)> = Vec::new();
    let mut mez = Map::new();

    let Some(summon) = power.summon() else {
        return Map::new();
    };

    // `pet_class` is the entity's own `characterClass`, or `None` for a row from the inline
    // block below, whose shells have no class to state (see that branch).
    let mut add_effect = |effect: &Value, pet_class: Option<&str>| {
        let effect_type = effect.get("type").and_then(Value::as_str);
        let scale = effect.get("scale").and_then(Value::as_f64);
        let table = effect.get("table").and_then(Value::as_str);

        let slot = display_slot(effect_type);

        if let (Some(PetDisplaySlot::Scalar(key)), Some(scale), Some(table)) = (slot, scale, table)
        {
            let slot = PetBagSlot {
                key,
                axis: effect
                    .get("axis")
                    .and_then(Value::as_str)
                    .map(str::to_string),
            };
            let contributions = match enhanceable.iter().position(|(held, _)| *held == slot) {
                Some(index) => &mut enhanceable[index].1,
                None => {
                    enhanceable.push((slot, Vec::new()));
                    &mut enhanceable.last_mut().expect("just pushed").1
                }
            };
            contributions.push(PetContribution {
                scale,
                table: table.to_string(),
                pet_class: pet_class.map(str::to_string),
                ignores_strength: ignores_strength(effect),
            });
        }

        let magnitude = effect.get("magnitude").and_then(Value::as_f64);
        let chance = effect.get("chance").and_then(Value::as_f64);
        let (Some(PetDisplaySlot::Control(key)), Some(magnitude), Some(scale), Some(table)) =
            (slot, magnitude, scale, table)
        else {
            return;
        };
        if chance.is_some_and(|chance| chance < 1.0) {
            return;
        }
        // Control doesn't sum into one bigger number: higher magnitude wins, then the longer
        // duration (scale stands in for it, since the true duration needs the AT table).
        let previous = mez.get(key);
        let previous_magnitude = previous.and_then(|v| v.get("mag")).and_then(Value::as_f64);
        let previous_scale = previous
            .and_then(|v| v.get("scale"))
            .and_then(Value::as_f64);
        let stronger = match previous_magnitude {
            None => true,
            Some(previous_magnitude) => {
                magnitude > previous_magnitude
                    || (magnitude == previous_magnitude && scale > previous_scale.unwrap_or(0.0))
            }
        };
        if stronger {
            let mut winner =
                serde_json::json!({ "mag": magnitude, "scale": scale, "table": table });
            // The pet row's own discriminator, forwarded rather than re-derived: a merged
            // control row is read by the same `mag`-format path a parent power's is, and
            // without it every summoned mez resolves as unstated (MEZDUR-1).
            if let Some(attrib_type) = effect.get("attribType") {
                winner["attribType"] = attrib_type.clone();
            }
            if let Some(to_who) = effect.get("toWho") {
                winner["toWho"] = to_who.clone();
            }
            if let Some(class) = pet_class {
                winner[PET_CLASS_KEY] = Value::String(class.to_string());
            }
            if ignores_strength(effect) {
                winner["ignoreStrength"] = Value::Bool(true);
            }
            mez.insert(key.to_string(), winner);
        }
    };

    // Real entity-backed pseudo-pets (Glue Arrow's sticky patch, the rains, the location holds).
    let mut from_entity_table = false;
    for entity_name in summon_entity_names(summon) {
        for entity in summoned_entity_chain(db, &entity_name) {
            let pet_class = entity.get("characterClass").and_then(Value::as_str);
            for effect in entity_ability_effects(entity) {
                from_entity_table = true;
                add_effect(effect, pet_class);
            }
        }
    }

    // Synthesized location pseudo-pets (Storm Cell, Category Five) carry their entity inline,
    // `IgnoreStrength` templates included. Those used to be skipped here on the reading that
    // an unenhanceable effect was "informational". But this merge is the only place a
    // pseudo-pet's kit reaches the summoning power at all, so skipping showed nothing rather
    // than showing something unboostable, and it made this branch disagree with both the
    // entity-table branch above and the parent-power route (ENT-4).
    //
    // The inline block stands in for a pet the entity table has no record of, so it's read
    // only when the table produced nothing. Homecoming's Sentinel Whirlpool is the one power in
    // any fork carrying both for the SAME pet, and its two blocks are identical row for row
    // (the two converter routes share one classification), so walking both would add the pet to
    // itself and double every number it publishes (ENT-8).
    //
    // These rows carry NO pet class, and that's the data rather than a gap in it: the shells
    // they come from (`PL_StaticObject`, `PL_FightPreferMelee`, `Pet_NoCollision`,
    // `PL_Untargetable_FightPreferRanged`) have no `villaindef.bin` record at all, so there's
    // no `characterClassName` for the game to resolve them against either. `power_AddPetEffects`
    // opens with `villainFindByName(pTemplate->pchEntityDef)` and finds nothing. They resolve
    // against the summoner's archetype, which is also what the converter measured in game for
    // Storm Cell and Category Five (ENT-10).
    if !from_entity_table {
        for resolved in summon
            .get("resolvedEntities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            for effect in entity_ability_effects(resolved) {
                add_effect(effect, None);
            }
        }
    }

    let mut merged: Map<String, Value> = Map::new();
    for (slot, contributions) in enhanceable {
        let Some(value) = merged_contributions(&contributions) else {
            continue;
        };
        let Some(axis) = slot.axis else {
            merged.insert(slot.key.to_string(), value);
            continue;
        };
        // A key is axed on every row or on none (both converters stamp an axis on `Slow` and
        // `MovementCapDebuff` and on nothing else), so the entry is an object whenever this
        // arm runs. If the two shapes ever did meet in one key, dropping is the conservative
        // answer: it can't publish an axis value under a flat reader, which is the shape
        // confusion this split exists to prevent.
        if let Value::Object(axes) = merged
            .entry(slot.key.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
        {
            axes.insert(axis, value);
        }
    }
    merged.append(&mut mez);
    merged
}

/// The bag slot one pet row lands in: a display key, and the movement axis within it when the
/// key is one the registry marks `canBeByType`.
///
/// The axis is a discriminator, not decoration. Homecoming's Blizzard slows fly by 0.6 and run,
/// jump and jump-height by 0.5 off one table, and Caltrops states run 0.8 beside a jump-height
/// 500. One number per key cannot hold either, and summing them produces 500.8. The
/// parent-power route has written `slow[axis]` since ENT-5; this is the pet route arriving at
/// the same shape, which is also the shape the display already collapses to its first entry.
#[derive(PartialEq, Eq)]
struct PetBagSlot {
    key: &'static str,
    axis: Option<String>,
}

/// One pet row's contribution to a display key.
///
/// Kept whole rather than folded into a running scale, because a scale states nothing on its
/// own: it's a multiplier on ITS table AT ITS CLASS, and the merge has neither a class nor a
/// level to spend a table against.
struct PetContribution {
    scale: f64,
    table: String,
    /// The summoned entity's own character class, absent for a row from the inline
    /// `resolvedEntities` block (whose shells have no villain def, so no class).
    pet_class: Option<String>,
    ignores_strength: bool,
}

/// The display value one bag key's pet contributions carry.
///
/// Rows naming one table add as scales. Rows naming DIFFERENT tables travel as separate
/// [`SCALE_TERMS_KEY`] terms for [`effect_base_value`] to resolve and sum, because adding their
/// scales mixes units: Homecoming's Summon Tarantula published `slow = 500.2` from a `0.2` on
/// `Ranged_Slow` plus a `500` on `Ranged_Ones`, a number in no unit at all (ENT-8).
///
/// Sameness is a case-insensitive exact name AND the same pet class, because a table name is
/// only half of a table: `class_GetNamedTableValue(pclass, name, level)` takes both, and one
/// name holds different values under different classes. Homecoming's Soul Extraction is the
/// corpus's one case (six `Ranged_Debuff_ToHit` rows off three henchman classes, whose values
/// differ), and summing their scales would resolve all six at whichever class arrived first
/// (ENT-10). The name half is the game's own identity: `classes.c` looks the template's table
/// up in a case-insensitive stash and aliases nothing, so Rebirth's Dark Servant, which spells
/// one table `Ranged_DeBuff_ToHit` on three rows and `Ranged_Debuff_ToHit` on a fourth, stays
/// one sum. Splitting a pair that's really one table+class would cost nothing anyway: the
/// terms resolve identically and add back to the same number.
///
/// The `ignoreStrength` mark is all-or-nothing over the whole key either way. One displayed
/// number can make only one claim about whether the summoner's slotting reaches it, and a mixed
/// key drops the mark and reads as enhanceable, which is what it was before the mark existed
/// (ENT-4).
fn merged_contributions(contributions: &[PetContribution]) -> Option<Value> {
    let (first, rest) = contributions.split_first()?;
    let unenhanceable = contributions
        .iter()
        .all(|contribution| contribution.ignores_strength);

    let one_source = rest.iter().all(|contribution| {
        contribution.table.eq_ignore_ascii_case(&first.table)
            && contribution.pet_class == first.pet_class
    });

    let term = |contribution: &PetContribution| {
        let mut term = serde_json::json!({
            "scale": contribution.scale,
            "table": contribution.table,
        });
        if let Some(class) = &contribution.pet_class {
            term[PET_CLASS_KEY] = Value::String(class.clone());
        }
        term
    };

    let mut merged = match one_source {
        true => {
            let scale: f64 = contributions.iter().map(|c| c.scale).sum();
            let mut summed = term(first);
            summed["scale"] = serde_json::json!(scale);
            summed
        }
        false => {
            let terms: Vec<Value> = contributions.iter().map(term).collect();
            serde_json::json!({ SCALE_TERMS_KEY: terms })
        }
    };
    if unenhanceable {
        merged["ignoreStrength"] = Value::Bool(true);
    }
    Some(merged)
}

/// The entity names a summon effect points at: the multi-entity list when it carries one,
/// else its single entity.
///
/// Public so the summon-resolution gate grades the names the consumers actually look up rather
/// than a second copy of this rule that could drift out of step with them.
pub fn summon_entity_names(summon: &Value) -> Vec<String> {
    if let Some(entities) = summon.get("entities").and_then(Value::as_array) {
        if !entities.is_empty() {
            return entities
                .iter()
                .filter_map(|entry| entry.get("entity").and_then(Value::as_str))
                .map(str::to_string)
                .collect();
        }
    }
    summon
        .get("entity")
        .and_then(Value::as_str)
        .map(str::to_string)
        .into_iter()
        .collect()
}

fn pet_entity<'a>(db: &'a PowerDatabase, name: &str) -> Option<&'a Value> {
    db.sections.get("pet-entities")?.get("entities")?.get(name)
}

/// The pseudo-pets one summon reference actually delivers: the named entity, then the ones its
/// own powers create in place (`createsEntities`), transitively.
///
/// A pet's payload can be one summon deeper. Poison Trap's pet carries a Self_Destruct and a
/// self-resistance and nothing else; the choke, the vomit and the −Recovery live in the gas
/// cloud that Self_Destruct leaves behind as the trap dies, so a walk that stopped at the named
/// entity showed the power doing nothing at all (ENT-3 step 4).
///
/// Only in-place summons are followed. The converter has already applied that filter (an
/// `EntCreate` at `target: AnyAffected` spawns one copy per foe hit and never reaches
/// `createsEntities`), because a chain like Jolting Chain's Jump1 → Jump2 → Jump3 is three
/// entities on three different targets, and summing them into one bag would report the whole
/// chain as landing on one.
///
/// Commandability is checked HERE rather than by the callers, so the root and its descendants
/// answer to one rule: a commandable pet is the player's own directable combat pet, it keeps
/// its own Summons block instead of folding into the summoning power, and that's as true of
/// one a pet calls as of one the player summons. Its subtree is not descended into either.
///
/// Unresolvable names are skipped. That's a fact about the export's scope: it carries only
/// player-facing pet entities, so a pet that calls an NPC (Fire Imps, Dust Devils) names one
/// that isn't there. A summon-chain gate asserts no player-summonable pet reaches one,
/// which is what keeps this skip from hiding a live loss.
/// Public so that gate grades the walk the consumers actually make rather than a
/// second copy of the rule that could drift out of step with it.
pub fn summoned_entity_chain<'a>(db: &'a PowerDatabase, root: &str) -> Vec<&'a Value> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<String> = VecDeque::from([root.to_string()]);

    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(entity) = pet_entity(db, &name) else {
            continue;
        };
        if entity.get("commandable").and_then(Value::as_bool) == Some(true) {
            continue;
        }
        out.push(entity);
        for child in entity
            .get("createsEntities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
        {
            queue.push_back(child.to_string());
        }
    }
    out
}

pub(crate) fn entity_abilities(entity: &Value) -> impl Iterator<Item = &Value> {
    entity
        .get("abilities")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

pub(crate) fn ability_effects(ability: &Value) -> impl Iterator<Item = &Value> {
    ability
        .get("effects")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// Every effect row an entity's abilities carry, flattened.
///
/// The ability the row came from is lost here, so a caller that needs a fact about the pet's POWER
/// rather than about one row (its recipients, above all) walks [`entity_abilities`] instead.
pub(crate) fn entity_ability_effects(entity: &Value) -> impl Iterator<Item = &Value> {
    entity_abilities(entity).flat_map(ability_effects)
}

/// The JS truthiness the surfaces' own `&&` guards apply: absent, `null` and `0` all mean "no
/// value here", so a zero leaves the transform's own value standing.
fn is_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(value) => value.as_f64() != Some(0.0),
    }
}

fn insert_number(bag: &mut Map<String, Value>, key: &str, value: Option<f64>) {
    if let Some(number) = value.and_then(serde_json::Number::from_f64) {
        bag.insert(key.to_string(), Value::Number(number));
    }
}

/// The beta `extractHealingFromDamage`: a heal authored as a `type: "Heal"` entry of the
/// `damage` array (Life Drain, Reconstruction) rather than as a `healing` effect. Entries are
/// summed only across the FIRST entry's table, because mixing tables would be
/// unitless-incorrect, and the sum tracks the `ignoreStrength` portion the enhancement must
/// not reach.
fn healing_from_damage(damage: &Value) -> Option<Value> {
    let entries: Vec<&Value> = match damage {
        Value::Array(entries) => entries.iter().collect(),
        Value::Object(_) => vec![damage],
        _ => return None,
    };
    let heals: Vec<&Value> = entries
        .into_iter()
        .filter(|entry| entry.get("type").and_then(Value::as_str) == Some("Heal"))
        .collect();
    let table = heals.first()?.get("table").cloned();

    let mut scale = 0.0;
    let mut unenhanced_scale = 0.0;
    for heal in heals {
        if heal.get("table").cloned() != table {
            continue;
        }
        let Some(entry_scale) = heal.get("scale").and_then(Value::as_f64) else {
            continue;
        };
        let ticks = match (
            heal.get("duration").and_then(Value::as_f64),
            heal.get("tickRate").and_then(Value::as_f64),
        ) {
            (Some(dur), Some(rate)) if dur > 0.0 && rate > 0.0 => {
                crate::damage::nominal_ticks(dur, rate)
            }
            _ => 1.0,
        };
        scale += entry_scale * ticks;
        if heal.get("ignoreStrength").and_then(Value::as_bool) == Some(true) {
            unenhanced_scale += entry_scale * ticks;
        }
    }
    if scale == 0.0 && unenhanced_scale == 0.0 {
        return None;
    }

    let mut healing = Map::new();
    healing.insert("scale".to_string(), serde_json::json!(scale));
    if let Some(table) = table {
        healing.insert("table".to_string(), table);
    }
    if unenhanced_scale != 0.0 {
        healing.insert(
            "unenhancedScale".to_string(),
            serde_json::json!(unenhanced_scale),
        );
    }
    Some(Value::Object(healing))
}

// ---------------------------------------------------------------------------
// the walk
// ---------------------------------------------------------------------------

/// Resolve every registered effect in a power's DISPLAY bag ([`display_effects`]) into a
/// granted magnitude row. A `<key>Unenhanced` split slot resolves through its base key's
/// registration as an IgnoreStrength row (ENT-6). Other unregistered keys (the bag also
/// carries `durations`, `maxStacks`, `onlyAffectsSelf`, … bookkeeping) and zero-valued
/// effects produce no row, matching the display's own skips.
#[allow(clippy::too_many_arguments)]
pub fn resolve_granted_magnitudes(
    power: &Power,
    stacking_source: &Power,
    archetype: &str,
    level: i32,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    db: &PowerDatabase,
    targets_hit: Option<u32>,
    dataset: DatasetId,
) -> Vec<GrantedMagnitude> {
    let effects = display_effects(power, stacking_source, targets_hit, dataset, db);
    resolve_display_bag(&effects, power, archetype, level, enhancement, g, db)
}

/// The duration the display bag records for one key, in seconds, where that number is the
/// EFFECTIVE one.
///
/// `durations` is a bookkeeping key of the bag rather than a registered effect, so the walk
/// skips it as a row and reads it here instead. Non-positive is absent: `WindowSlots` only ever
/// records `> 0.0`, and an authored bag reaching a hand-written test can carry anything.
///
/// **A `Duration`-valued mez is excluded, and that is the whole subtlety.** `mod_Fill` computes
/// `scale x table` and assigns it as the duration for `kModType_Duration`, discarding the
/// template's own `fDuration` — which is what the map records. The two are not the same number:
/// Power Surge's hold resolves to 17.88s and its template says 30. So on those rows the tier
/// already IS the effective duration, and publishing the recorded one beside it would state the
/// number the game throws away, which is MEZDUR-1's defect wearing a new field name. Every
/// other `attribType` takes `fDuration` straight across, so there the recorded value is the
/// effective one — which is why a `-Recharge lasts 15s` annotation was always right.
fn recorded_duration(
    effects: &Map<String, Value>,
    bag_key: &str,
    quantity: GrantedQuantity,
) -> Option<f64> {
    if matches!(quantity, GrantedQuantity::MezDuration { .. }) {
        return None;
    }
    effects
        .get("durations")?
        .get(bag_key)?
        .as_f64()
        .filter(|d| *d > 0.0)
}

/// The walk itself, over a bag the caller states.
///
/// Split out of [`resolve_granted_magnitudes`] when the display seed moved to the atoms: the
/// per-key resolution rules (the registry lookup, the by-type expansion, the mez quantity, the
/// three-tier, the ENT-6 split slot) are one subject and how the bag was BUILT is another, and
/// a unit test of the first should not have to satisfy the second. The tests that hand it a bag
/// grade the rules on inputs the corpus does not happen to contain — two enhanceabilities inside
/// one by-type value, a suffixed key whose base is unregistered — and every one of them sits
/// beside a corpus-wide test in the same file that grades real data.
///
/// `power` is still read, for the mez FACE label: a protection-spelled non-foe row reads as
/// protection and a self-directed one as the caster's own root, and that pair is the power's,
/// not the slot's.
#[allow(clippy::too_many_arguments)]
pub fn resolve_display_bag(
    effects: &Map<String, Value>,
    power: &Power,
    archetype: &str,
    level: i32,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    db: &PowerDatabase,
) -> Vec<GrantedMagnitude> {
    let clamps = ReductionClamps::from_caps(db.archetype_stats.get(archetype));
    // The build's own scope. A merged pseudo-pet row overrides it with the class the row
    // itself names, one table read at a time (ENT-10).
    let scope = TableScope::Archetype(archetype);

    let mut rows = Vec::new();
    for (bag_key, value) in effects {
        if value.is_null() {
            continue;
        }
        // A `<key>Unenhanced` slot is the converter's IgnoreStrength verdict in key form: the
        // split predates the mark travelling on the value (ENT-4), and two of its shapes still
        // ship with no mark at all (the per-foe aura rows `_remapUnenhancedPatchKeys` moves, and
        // Sprint's second run template). The half resolves through its base key's config and
        // renders flat (the same treatment a marked value under the base key gets), so the
        // registry keeps one entry per family and a power with both halves shows both rows, the
        // way the game's own monitor does, one tagged "Ignores buffs and enhancements" (ENT-6).
        let (config, effect_key, from_split_slot) = match effect_registry::lookup(bag_key) {
            Some(config) => (config, bag_key.as_str(), false),
            None => {
                let split = bag_key
                    .strip_suffix("Unenhanced")
                    .and_then(|base| effect_registry::lookup(base).map(|config| (base, config)));
                let Some((base_key, config)) = split else {
                    continue;
                };
                (config, base_key, true)
            }
        };

        // No split family expands today; if one ever ships a by-type half, the generic path's
        // first-entry collapse keeps the row carrying the slot's mark, where the expansion
        // reads each entry's own (absent) one. A non-expandable value falls through to the
        // generic path, as in the beta.
        if !from_split_slot
            && config.expand_by_type
            && value.is_object()
            && push_expanded_rows(
                &mut rows,
                effect_key,
                value,
                config,
                scope,
                level,
                enhancement,
                g,
                clamps,
                db,
            )
        {
            continue;
        }

        // An effect authored as a fraction of Max HP reports a percent, not an amount, so
        // its form is settled before the generic resolution (a value carrying ONLY a
        // `maxHPFraction` has no scale for the generic path to read).
        let percent_form = config
            .max_hp_fraction_percent_form
            .then(|| max_hp_fraction_percent(value))
            .flatten();

        let quantity = mez_quantity(value, config);
        // A row whose value the data does not state still gets a row. It carries no number —
        // the surface says so in words — because the alternative shapes are both worse: a
        // fabricated one is the soft-wrong this gap was filed for, and dropping the row is
        // the silent omission ENT-6 measured, where a power's card simply lacked an effect
        // the game shows.
        if matches!(
            quantity,
            GrantedQuantity::MezExpression
                | GrantedQuantity::MezConstant
                | GrantedQuantity::MezUnstated
        ) {
            rows.push(GrantedMagnitude {
                row_key: bag_key.clone(),
                effect_key: effect_key.to_string(),
                label: mez_face_label(power, effect_key, value, &config.label),
                category: config.category,
                format: config.format,
                priority: config.priority,
                value: ThreeTier::flat(0.0),
                quantity,
                by_type_label: None,
                duration: recorded_duration(effects, bag_key, quantity),
                // The template's own mark, not a stand-in for "shows no number". The row
                // withholds its enhanced columns because `quantity` says the value is
                // unresolvable; claiming IgnoreStrength here would state something about the
                // source template that the source template does not say.
                ignores_strength: ignores_strength(collapsed_source(value, config)),
            });
            continue;
        }

        let (base, label, format) = match percent_form {
            Some(percent) => (
                percent,
                format!("{} (% Max HP)", config.label),
                EffectFormat::Percent,
            ),
            None => match effect_base_value(value, config, scope, db, level, 1.0) {
                Some(base) => (base, config.label.clone(), config.format),
                None => continue,
            },
        };
        // The mez FACE rides the label: a protection-spelled non-foe row reads as protection,
        // a self-directed one as the caster's own root, off the same pair the applier credits
        // (MEZFACE-1).
        let label = mez_face_label(power, effect_key, value, &label);
        if base == 0.0 {
            continue;
        }

        let unenhanceable = from_split_slot || ignores_strength(collapsed_source(value, config));
        let tier = match quantity {
            _ if unenhanceable => ThreeTier::flat(base),
            // A mez and a knockback's distance are scaled by the bonus named by the EFFECT
            // key (a hold reads the `hold` bonus), not by the config's enhancement aspect,
            // which `mag`-format effects don't declare. The coupling holds for both mez
            // quantities: the server applies Strength at the mod's own attrib offset before
            // deciding which number the product becomes (`attribmod.c` `mod_Fill`), so a
            // `Magnitude` mez takes it on the magnitude exactly as a `Duration` one takes it
            // on the seconds.
            GrantedQuantity::MezDuration { .. }
            | GrantedQuantity::MezMagnitude
            | GrantedQuantity::Distance => {
                tier_for_aspect(effect_key, base, enhancement, g, clamps)
            }
            GrantedQuantity::MezExpression
            | GrantedQuantity::MezConstant
            | GrantedQuantity::MezUnstated => {
                unreachable!("an unresolvable mez pushed its row above and continued")
            }
            GrantedQuantity::Value => tier_for_config(config, base, enhancement, g, clamps),
        };

        rows.push(GrantedMagnitude {
            row_key: bag_key.clone(),
            effect_key: effect_key.to_string(),
            label,
            category: config.category,
            format,
            priority: config.priority,
            value: tier,
            quantity,
            by_type_label: (config.can_be_by_type && is_by_type_object(value))
                .then(|| by_type_abbreviation(value))
                .flatten(),
            duration: recorded_duration(effects, bag_key, quantity),
            ignores_strength: unenhanceable,
        });
    }

    rows
}

/// Classify a `mag`-format effect off the discriminator the atom carries: a mez states its
/// own `attribType`, and a `{ scale, table }` with no `mag` at all is a distance.
///
/// A pure function of the value and its registry config — no table read. The classification
/// used to depend on one (`table_value(...) > 0` stood in for "this is a duration"), which
/// made the row's unit hostage to whether the AT happened to carry the table.
fn mez_quantity(value: &Value, config: &EffectDisplayConfig) -> GrantedQuantity {
    if config.format != EffectFormat::Mag {
        return GrantedQuantity::Value;
    }
    let Some(magnitude) = mez_magnitude(value) else {
        return if scale_of(value).is_some() && table_of(value).is_some() {
            GrantedQuantity::Distance
        } else {
            GrantedQuantity::Value
        };
    };
    match mez_attrib_type(value) {
        Some(AttribType::Duration) => GrantedQuantity::MezDuration { magnitude },
        Some(AttribType::Magnitude) => GrantedQuantity::MezMagnitude,
        Some(AttribType::Expression) => GrantedQuantity::MezExpression,
        Some(AttribType::Constant) => GrantedQuantity::MezConstant,
        None => GrantedQuantity::MezUnstated,
    }
}

/// The FACE a mez row renders as (MEZFACE-1), off the same discriminator pair the applier
/// credits — the converter's protection spelling (negative scale, TSPY-8's first; negative
/// magnitude, its second; an `Expression`-valued row) on a power that affects no FOE, plus
/// the row's own recipient. The bag used to state neither axis (the scale was abs-es and no
/// `toWho` rode it), so a display reading the FACE had to sniff `res_boolean` table names,
/// which missed every `*_Ones` armor and could never see a self-root at all.
///
/// `Prot` wins over the self mark: a protection-spelled non-foe row is caster-facing by
/// definition (that is what the pair says), so it states the group, not the recipient.
fn mez_face_label(power: &Power, effect_key: &str, value: &Value, label: &str) -> String {
    const MEZ: [&str; 6] = ["hold", "stun", "immobilize", "sleep", "confuse", "fear"];
    if !MEZ.contains(&effect_key) {
        return label.to_string();
    }
    let spelled = value
        .get("scale")
        .and_then(Value::as_f64)
        .is_some_and(|s| s < 0.0)
        || value
            .get("mag")
            .and_then(Value::as_f64)
            .is_some_and(|m| m < 0.0)
        || value.get("attribType").and_then(Value::as_str) == Some("Expression");
    if spelled && !power.affects_foe() {
        return format!("{label} Prot");
    }
    if value.get("toWho").and_then(Value::as_str) == Some("Self") {
        return format!("{label} (Self)");
    }
    label.to_string()
}

/// Expand a by-type / protection value into one row per type. Returns `false` when the
/// value is not expandable, so the caller falls through to the generic path.
#[allow(clippy::too_many_arguments)]
fn push_expanded_rows(
    rows: &mut Vec<GrantedMagnitude>,
    effect_key: &str,
    value: &Value,
    config: &EffectDisplayConfig,
    scope: TableScope<'_>,
    level: i32,
    enhancement: &EnhancementBonuses,
    g: &GlobalBonuses,
    clamps: ReductionClamps,
    db: &PowerDatabase,
) -> bool {
    let push = |rows: &mut Vec<GrantedMagnitude>,
                type_key: &str,
                label: String,
                tier: ThreeTier,
                unenhanceable: bool| {
        rows.push(GrantedMagnitude {
            row_key: format!("{effect_key}_{type_key}"),
            effect_key: effect_key.to_string(),
            label,
            category: config.category,
            format: config.format,
            priority: config.priority,
            value: tier,
            quantity: GrantedQuantity::Value,
            by_type_label: None,
            // Keyed `{effect_key}_{type_key}`, which `durations` never holds — see the field.
            duration: None,
            ignores_strength: unenhanceable,
        });
    };

    // Mez protection: authored as plain magnitudes per mez type, never enhanceable.
    if config.format == EffectFormat::Mag {
        let Some(object) = value.as_object() else {
            return false;
        };
        for (type_key, entry) in object {
            let Some(magnitude) = entry.as_f64().filter(|m| *m != 0.0) else {
                continue;
            };
            let label = format!("{}: {}", config.label, effect_registry::mez_label(type_key));
            push(rows, type_key, label, ThreeTier::flat(magnitude), false);
        }
        return true;
    }

    if is_by_type_object(value) {
        for (type_key, label, percent, unenhanceable) in
            expand_by_type_entries(value, &config.label, scope, db, level)
        {
            if percent == 0.0 {
                continue;
            }
            let tier = match unenhanceable {
                true => ThreeTier::flat(percent),
                false => tier_for_config(config, percent, enhancement, g, clamps),
            };
            push(rows, &type_key, label, tier, unenhanceable);
        }
        return true;
    }

    // A scalar value on an effect declared to resolve through the table-base path.
    if config.scalar_from_table_percent {
        let percent = resistance_fraction(db, scope, value, level) * 100.0;
        if percent != 0.0 {
            push(
                rows,
                "_all",
                config.label.clone(),
                ThreeTier::flat(percent),
                ignores_strength(value),
            );
        }
        return true;
    }

    false
}

/// The beta `expandByTypeEntries`: resolve each non-zero type to a percent, collapsing to
/// one `(All)` row when every value agrees AND the value covers all primary damage types
/// (or names `all` outright).
///
/// Each entry carries its own enhanceability, because a by-type value is a bag of independent
/// templates: Venom Grenade's eight −Resistance types are all `IgnoreStrength`, but nothing
/// makes that a property of the key. The collapse therefore needs the marks to agree as well
/// as the numbers: one `(All)` row cannot say two things about whether slotting reaches it.
fn expand_by_type_entries(
    value: &Value,
    label_prefix: &str,
    scope: TableScope<'_>,
    db: &PowerDatabase,
    level: i32,
) -> Vec<(String, String, f64, bool)> {
    let Some(object) = value.as_object() else {
        return Vec::new();
    };
    let resolved: Vec<(String, f64, bool)> = object
        .iter()
        .filter(|(_, entry)| !entry.is_null() && entry.as_f64() != Some(0.0))
        .map(|(type_key, entry)| {
            (
                type_key.clone(),
                resistance_fraction(db, scope, entry, level) * 100.0,
                ignores_strength(entry),
            )
        })
        .collect();
    if resolved.is_empty() {
        return Vec::new();
    }

    let all_same = resolved
        .iter()
        .all(|(_, percent, _)| (percent - resolved[0].1).abs() < 0.001);
    let all_agree_on_strength = resolved
        .iter()
        .all(|(_, _, unenhanceable)| *unenhanceable == resolved[0].2);
    let has_all_key = resolved.iter().any(|(type_key, _, _)| type_key == "all");
    let covers_all_primary = has_all_key
        || ALL_PRIMARY_DAMAGE_TYPES.iter().all(|damage_type| {
            resolved
                .iter()
                .any(|(type_key, _, _)| type_key == damage_type)
        });
    if all_same && all_agree_on_strength && covers_all_primary {
        return vec![(
            "_all".to_string(),
            format!("{label_prefix} (All)"),
            resolved[0].1,
            resolved[0].2,
        )];
    }

    resolved
        .into_iter()
        .map(|(type_key, percent, unenhanceable)| {
            let label = format!("{label_prefix}: {}", effect_registry::type_label(&type_key));
            (type_key, label, percent, unenhanceable)
        })
        .collect()
}
