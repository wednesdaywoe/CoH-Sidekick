//! Pass 6: incarnate stat bonuses (Destiny, Hybrid, Genesis, and the incarnate
//! level shift).
//!
//! A targeted, build-state-driven pass (the shape of [`crate::inherents`], not the
//! generic atom loop): it reads the equipped [`IncarnateLoadout`] and the typed
//! [`coh_data::IncarnateEffects`] tables, and accumulates the flat per-slot stat
//! bonuses into [`GlobalBonuses`], the same accumulator every other pass writes,
//! mirroring the beta's `applyIncarnateBonuses`. Incarnate effects are NOT atoms
//! (the beta reads these bespoke flat tables directly), so gather never sees them
//! and there's no double-count risk.
//!
//! **What lands here:** Destiny (resolved at its sustained-floor decay time, then
//! optionally Alpha-enhanced and Genesis-Fate-scaled), Hybrid (passive always,
//! frontLoaded and perTarget when toggled on), Genesis Socket (→ Max HP / Max Endurance), and the
//! Alpha/Destiny/Lore level shift. Below level 45 (exemplar suppression) every
//! contribution turns off EXCEPT the Genesis-Fate exemplar buff.
//!
//! **Provenance.** Every contributor above is bracketed by an accumulator snapshot, and what moved
//! between the two IS its contribution, filed as [`IncarnateBreakdownSource`] rows for the
//! detailed-totals breakdown's `incarnate` group. Measured rather than reported for the same reason
//! the apply walk's rows are ([`crate::apply::PowerBreakdownSource`]): a slot that also described
//! itself would be a second account of the same arithmetic, free to drift from the first.
//!
//! **Documented no-ops (authored facts, not gaps):** Interface (enemy-debuff procs)
//! and Judgement (a click attack) contribute nothing to player stat totals in the
//! game, and the beta never reads them into `GlobalBonuses` either. Lore contributes
//! only its level shift.
//!
//! **What does NOT land here: Alpha's enhancement of other powers.** Alpha's main
//! contribution is a "virtual enhancement" injected into every regular power's
//! slotted-IO ED aggregation (the beta's `combineWithAlphaED`, a pre/post-ED split).
//! That injection lives in [`crate::apply`], which selects
//! [`crate::enhancement::combine_with_alpha_ed`] /
//! [`crate::enhancement::filter_alpha_by_allowed_enhancements`] over the plain aggregation
//! whenever an active Alpha is equipped. This pass supplies that path its inputs:
//! [`alpha_enhancement`] maps the equipped Alpha's `AlphaEffects` and its ED-bypass table to
//! the per-aspect [`crate::enhancement::EnhancementBonuses`] the split needs. What Alpha lands
//! DIRECTLY here is its own level shift and its enhancement of a Destiny power's flat effects
//! (flat-value × flat-value, not the ED hook).

use crate::enhancement::EnhancementBonuses;
use crate::totals::{route_closed, CalcError, GlobalBonuses, TypeRoute};
use coh_data::{
    normalize_incarnate_power_id, AlphaEffects, DestinyEffects, GenesisEffects, IncarnateEffects,
    IncarnateLoadout, Level, PowerDatabase,
};

/// Combat level below which all incarnate abilities suppress (except the Genesis-Fate
/// exemplar buff). The beta `INCARNATE_MIN_LEVEL`, a game rule ported not invented.
const INCARNATE_MIN_LEVEL: Level = Level::constant(45);

/// The eleven standard +Defense globals (three positions + eight damage types),
/// the expansion targets of a `defenseAll` buff.
const DEFENSE_TYPES: [&str; 11] = [
    "melee", "ranged", "aoe", "smashing", "lethal", "fire", "cold", "energy", "negative",
    "psionic", "toxic",
];

/// The eight standard +Resistance globals, the expansion targets of `resistanceAll`.
const RESISTANCE_TYPES: [&str; 8] = [
    "smashing", "lethal", "fire", "cold", "energy", "negative", "psionic", "toxic",
];

/// The six status types a `mezProtection` / `statusResistance` buff spreads across
/// (knockback is its own field).
const MEZ_STATUS_TYPES: [&str; 6] = ["hold", "stun", "immobilize", "sleep", "confuse", "fear"];

/// The slot ids the provenance rows carry: the catalog's own vocabulary
/// ([`coh_data::IncarnateSlotCatalog::id`], which is also the loadout's `get`/`set` key), so a
/// breakdown row addresses the same slot the incarnate UI does.
const ALPHA_SLOT: &str = "alpha";
const DESTINY_SLOT: &str = "destiny";
const LORE_SLOT: &str = "lore";
const HYBRID_SLOT: &str = "hybrid";
const GENESIS_SLOT: &str = "genesis";

/// One incarnate contributor's effect on one breakdown key. Pass 6's half of the detailed-totals
/// provenance, the counterpart to the apply walk's [`crate::apply::PowerBreakdownSource`].
///
/// A separate row type because an incarnate isn't addressed as a power: the loadout stores a slot
/// and a power id with no owning powerset, and the same equipped power can contribute twice in
/// ways the player must be able to tell apart (its stat block and its level shift; a Fate Genesis's
/// normal amplification and its below-45 exemplar buff).
///
/// Measured exactly like the per-power rows: each contributor is bracketed by an accumulator
/// snapshot and [`GlobalBonuses::deltas_since`] reads off what moved. A row therefore cannot
/// disagree with the total it helped make, however the Destiny time-resolve, Alpha enhance and
/// Fate scale combined to produce it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct IncarnateBreakdownSource {
    /// The beta camelCase field name ([`GlobalBonuses::BREAKDOWN_KEYS`]).
    pub breakdown_key: String,
    /// The slot id in the catalog's vocabulary (`alpha` … `genesis`), the loadout's own
    /// addressing and how the player finds the contributor in the incarnate UI.
    pub slot: String,
    /// The equipped power's `internalName`, as the loadout stores it. The display name the
    /// breakdown shows is the incarnate catalog's, resolved by the output mapper: this pass
    /// reads the id-keyed effect tables, never the catalog.
    pub power_name: String,
    /// The below-45 Genesis-Fate exemplar buff, which is a DIFFERENT contribution of the same
    /// equipped power (the only one that survives exemplar suppression). The beta labels it
    /// `<name> (exemplar)` for exactly that reason.
    pub exemplar: bool,
    /// How much this contributor moved that field, signed.
    pub value: f64,
}

/// File one incarnate contributor's measured deltas. A slot that moved nothing produces no rows.
fn record_incarnate(
    before: &GlobalBonuses,
    g: &GlobalBonuses,
    slot: &str,
    power_name: &str,
    exemplar: bool,
    out: &mut Vec<IncarnateBreakdownSource>,
) {
    out.extend(
        g.deltas_since(before)
            .into_iter()
            .map(|(key, value)| IncarnateBreakdownSource {
                breakdown_key: key.to_string(),
                slot: slot.to_string(),
                power_name: power_name.to_string(),
                exemplar,
                value,
            }),
    );
}

/// True when incarnate abilities are suppressed: the build is exemplared below level
/// 45. `None` (not exemplared) never suppresses. Ports the beta `areIncarnatesSuppressed`.
fn incarnates_suppressed(exemplar_level: Option<Level>) -> bool {
    exemplar_level.is_some_and(|lvl| lvl < INCARNATE_MIN_LEVEL)
}

/// Round to 6 decimals, the beta's `Math.round(x * 1e6) / 1e6` drift guard. Applied
/// to the Alpha-enhance product so the Rust f64 matches the TS oracle bit-for-bit
/// (values here are non-negative, where Rust's round-half-away and JS's round-half-up
/// agree).
fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

/// Map an `AlphaEffects` table (either the buff or its ED-bypass twin) to the per-aspect
/// [`EnhancementBonuses`] the apply-pass ED split reads, the beta
/// `mapAlphaEffectsToEnhancementBonuses`. Two keys are renamed to the aggregation's aspect
/// vocabulary: `enduranceReduction → endurance` and `enduranceModification → enduranceMod`
/// (writing the long form dropped the bonus silently in the beta's gate). The movement keys
/// (`runSpeed`/`jumpSpeed`/`flySpeed`) are carried verbatim as the beta writes them. They do
/// NOT match the aggregation's `run`/`fly`/`jump` vocabulary, so a gated power never accepts
/// them; inert in practice (no Alpha buffs movement), preserved for a faithful mapping.
fn alpha_effects_to_bonuses(alpha: &AlphaEffects) -> EnhancementBonuses {
    let mut pairs: Vec<(&'static str, f64)> = Vec::new();
    let mut put = |key: &'static str, value: Option<f64>| {
        if let Some(v) = value {
            pairs.push((key, v));
        }
    };
    put("damage", alpha.damage);
    put("accuracy", alpha.accuracy);
    put("recharge", alpha.recharge);
    put("endurance", alpha.endurance_reduction);
    put("enduranceMod", alpha.endurance_modification);
    put("range", alpha.range);
    put("heal", alpha.heal);
    put("defense", alpha.defense);
    put("resistance", alpha.resistance);
    put("hold", alpha.hold);
    put("stun", alpha.stun);
    put("immobilize", alpha.immobilize);
    put("sleep", alpha.sleep);
    put("fear", alpha.fear);
    put("confuse", alpha.confuse);
    put("slow", alpha.slow);
    put("tohitDebuff", alpha.to_hit_debuff);
    put("defenseDebuff", alpha.defense_debuff);
    put("tohit", alpha.to_hit_buff);
    put("taunt", alpha.taunt);
    put("runSpeed", alpha.run_speed);
    put("jumpSpeed", alpha.jump_speed);
    put("flySpeed", alpha.fly_speed);
    put("absorb", alpha.absorb);
    EnhancementBonuses::from_aspect_values(pairs)
}

/// The equipped Alpha's virtual-enhancement inputs for the apply-pass ED split
/// ([`crate::enhancement::combine_with_alpha_ed`] / `filter_alpha_by_allowed_enhancements`).
/// `active` is false, and both maps empty, unless an Alpha is equipped, toggled on, and
/// unsuppressed; [`crate::apply`] routes through the split only when `active`, leaving the
/// plain aggregation untouched for every non-Alpha build.
#[derive(Debug, Clone, Default)]
pub struct AlphaEnhancement {
    /// Whether an equipped, active, unsuppressed Alpha drives the ED split this build.
    pub active: bool,
    /// Alpha's total per-aspect buff (the beta `getAlphaEnhancementBonuses`).
    pub bonuses: EnhancementBonuses,
    /// The per-aspect slice that bypasses ED (the beta `getAlphaEdBypassBonuses`, read from
    /// the silent-grant BoostIgnoreDiminishing / `Ones` templates).
    pub ed_bypass: EnhancementBonuses,
}

/// Resolve the equipped Alpha into its enhancement inputs. The Rust of the beta's
/// `getAlphaEnhancementBonuses` + `getAlphaEdBypassBonuses`, sharing their active/suppression
/// gate (`activeAlphaEffects`): below level 45 (suppressed), or with no Alpha equipped, or with
/// the Alpha toggle off, it yields the inactive default. An equipped+active Alpha whose power id
/// is absent from the tables still reports `active` (empty maps) so the apply pass uses the ED
/// split's build-level rule uniformly, matching the beta (which always calls `combineWithAlphaED`).
pub fn alpha_enhancement(
    incarnates: &IncarnateLoadout,
    exemplar_level: Option<Level>,
    db: &PowerDatabase,
) -> AlphaEnhancement {
    if incarnates_suppressed(exemplar_level) {
        return AlphaEnhancement::default();
    }
    let Some(alpha) = incarnates.alpha.as_ref().filter(|a| a.active) else {
        return AlphaEnhancement::default();
    };
    let fx = &db.incarnate_effects;
    let id = normalize_incarnate_power_id(&alpha.power_name);
    AlphaEnhancement {
        active: true,
        bonuses: fx
            .alpha_effects
            .get(&id)
            .map(alpha_effects_to_bonuses)
            .unwrap_or_default(),
        ed_bypass: fx
            .alpha_ed_bypass
            .get(&id)
            .map(alpha_effects_to_bonuses)
            .unwrap_or_default(),
    }
}

/// The active Genesis amplifier (Rebirth-only), or `None`. Genesis is toggleable, so
/// it contributes only when equipped AND its slot toggle is on. Ports `getActiveGenesis`.
fn active_genesis<'a>(
    incarnates: &IncarnateLoadout,
    fx: &'a IncarnateEffects,
) -> Option<&'a GenesisEffects> {
    let slot = incarnates.genesis.as_ref()?;
    if !slot.active {
        return None;
    }
    fx.genesis_effects
        .get(&normalize_incarnate_power_id(&slot.power_name))
}

/// Enhance a Destiny buff by the equipped Alpha's enhancement, gated by the power's
/// accepted boost categories. Ports the beta `applyAlphaToDestiny`: a fixed four-row rule
/// (`Res_Damage→resistance→resistanceAll`, `Buff_Defense→defense→defenseAll`,
/// `Heal→heal→healPercent`, `Recovery→enduranceModification→recovery`), each a
/// straight `× (1 + aspect)` with NO ED modelled (a Destiny click buff carries no
/// slotted enhancements, so the Alpha value sits below the ED knee). Returns the
/// effects unchanged when no Alpha, no accepted boosts, or no matching aspect.
fn apply_alpha_to_destiny(
    mut effects: DestinyEffects,
    boosts_allowed: &[String],
    alpha: Option<&AlphaEffects>,
) -> DestinyEffects {
    let Some(alpha) = alpha else { return effects };
    if boosts_allowed.is_empty() {
        return effects;
    }
    let allows = |boost: &str| boosts_allowed.iter().any(|b| b == boost);
    // Res_Damage → resistance → resistanceAll
    enhance_field(
        &mut effects.resistance_all,
        alpha.resistance,
        allows("Res_Damage"),
    );
    // Buff_Defense → defense → defenseAll
    enhance_field(
        &mut effects.defense_all,
        alpha.defense,
        allows("Buff_Defense"),
    );
    // Heal → heal → healPercent (a dead sink downstream, ported for oracle fidelity).
    enhance_field(&mut effects.heal_percent, alpha.heal, allows("Heal"));
    // Recovery → enduranceModification → recovery
    enhance_field(
        &mut effects.recovery,
        alpha.endurance_modification,
        allows("Recovery"),
    );
    effects
}

/// One Alpha-enhance row: `dest *= 1 + enh` (round6'd) when the boost is allowed, the
/// Alpha aspect is present and nonzero, and the Destiny stat exists. Matches the beta's
/// `if (!enh || typeof base !== 'number') continue` guard (a 0 enhancement is a no-op).
fn enhance_field(dest: &mut Option<f64>, alpha_aspect: Option<f64>, allowed: bool) {
    if !allowed {
        return;
    }
    let (Some(enh), Some(base)) = (alpha_aspect, *dest) else {
        return;
    };
    if enh == 0.0 {
        return;
    }
    *dest = Some(round6(base * (1.0 + enh)));
}

/// Scale a Destiny effect block by `factor`. Ports the beta `scaleDestinyEffects` (Fate
/// Genesis amplification). Every numeric stat scales; the level shift and the duration
/// metadata are left untouched (they aren't amplifiable). No rounding (matching the
/// beta, which rounds only the Alpha-enhance product).
fn scale_destiny_effects(effects: &mut DestinyEffects, factor: f64) {
    for v in [
        &mut effects.defense_all,
        &mut effects.resistance_all,
        &mut effects.debuff_resistance,
        &mut effects.status_resistance,
        &mut effects.heal_received,
        &mut effects.heal_percent,
        &mut effects.heal_scale,
        &mut effects.kb_protection,
        &mut effects.run_speed,
        &mut effects.recovery,
        &mut effects.regeneration,
        &mut effects.max_hp,
        &mut effects.max_endurance,
        &mut effects.endurance,
        &mut effects.recharge,
        &mut effects.damage,
        &mut effects.to_hit,
        &mut effects.mez_protection,
    ]
    .into_iter()
    .flatten()
    {
        *v *= factor;
    }
}

/// The below-45 Genesis-Fate exemplar buff, the ONE incarnate contribution that
/// survives suppression. Ports `applyGenesisExemplarBuff`: a Fate-tree `buff`-kind
/// exemplar effect adds its `recharge` / `recovery` stats (× 100) to the totals. Its
/// `endurance` stat and mez protection stay display-only (never read here).
fn apply_genesis_exemplar_buff(
    g: &mut GlobalBonuses,
    incarnates: &IncarnateLoadout,
    fx: &IncarnateEffects,
    breakdown: &mut Vec<IncarnateBreakdownSource>,
) {
    let (Some(slot), Some(genesis)) = (incarnates.genesis.as_ref(), active_genesis(incarnates, fx))
    else {
        return;
    };
    if genesis.tree != "fate" {
        return;
    }
    let Some(ex) = &genesis.exemplar_effect else {
        return;
    };
    if ex.kind != "buff" {
        return;
    }
    let before = g.clone();
    if let Some(&recharge) = ex.stats.get("recharge").filter(|&&v| v != 0.0) {
        g.recharge += recharge * 100.0;
    }
    if let Some(&recovery) = ex.stats.get("recovery").filter(|&&v| v != 0.0) {
        g.recovery += recovery * 100.0;
    }
    record_incarnate(&before, g, GENESIS_SLOT, &slot.power_name, true, breakdown);
}

/// Genesis Socket (Rebirth), the only tree with a direct player-stat effect: +Max HP and
/// +Max Endurance at the tier %. Fate is handled via the Destiny scale; Verdict/Data are
/// display-only.
fn apply_genesis_socket(
    g: &mut GlobalBonuses,
    incarnates: &IncarnateLoadout,
    fx: &IncarnateEffects,
    breakdown: &mut Vec<IncarnateBreakdownSource>,
) {
    let (Some(slot), Some(genesis)) = (incarnates.genesis.as_ref(), active_genesis(incarnates, fx))
    else {
        return;
    };
    if genesis.tree != "socket" {
        return;
    }
    let before = g.clone();
    let value = genesis.tier_percent * 100.0;
    g.max_hp += value;
    g.max_endurance += value;
    record_incarnate(&before, g, GENESIS_SLOT, &slot.power_name, false, breakdown);
}

/// Route one incarnate stat-block key by its `GlobalBonuses` field name, failing loud when no
/// field owns it.
///
/// The calc's single most open coupling: the key is arbitrary text from the incarnate
/// effects data, matched against a field name. Every shipped key routes today (measured across
/// all three datasets' Hybrid blocks), so an unrecognized one means the data grew a stat this
/// calc has no home for. Rule 1 wants that visible, not a bonus that silently never arrives.
fn surface_stat_block_key(g: &mut GlobalBonuses, key: &str, value: f64) {
    if g.add_by_camel_name(key, value) == TypeRoute::Unknown {
        g.errors.push(CalcError::new(
            "Incarnate",
            format!("hybrid stat-block key {key:?} names no total"),
        ));
    }
}

/// Apply one Hybrid effect layer (passive, frontLoaded or perTarget). Ports the beta
/// `applyHybridStatBlock`.
/// Special keys: `statusResistance` spreads (× 100) across the six mez-resistance types;
/// `enduranceDiscount` (× 100) feeds the canonical EndDisc bucket (`g.endurance`);
/// `prot*` keys add raw magnitude (NOT × 100); `defenseAll`/`resistanceAll` expand
/// (× 100); every other key is a `GlobalBonuses` field name added (× 100) generically.
///
/// `stacks` is how many times the layer lands — 1 for the two always-once layers, and the
/// foe count for perTarget, whose row IS one foe's worth ([`apply_incarnate_bonuses`]). It
/// multiplies the stored decimal rather than the routed value because every branch below is
/// linear in it, including the raw-magnitude `prot*` one: three foes' worth of a mag-2
/// protection is mag 6, not 6%.
fn apply_hybrid_stat_block(
    g: &mut GlobalBonuses,
    layer: &std::collections::HashMap<String, f64>,
    stacks: f64,
) {
    for (stat, &stored) in layer {
        let decimal = stored * stacks;
        match stat.as_str() {
            "statusResistance" => {
                let value = decimal * 100.0;
                for mez in MEZ_STATUS_TYPES {
                    route_closed(g.add_mez_resistance(mez, value), mez);
                }
            }
            "enduranceDiscount" => g.endurance += decimal * 100.0,
            "defenseAll" => {
                let value = decimal * 100.0;
                for t in DEFENSE_TYPES {
                    route_closed(g.add_defense(t, value), t);
                }
            }
            "resistanceAll" => {
                let value = decimal * 100.0;
                for t in RESISTANCE_TYPES {
                    route_closed(g.add_resistance(t, value), t);
                }
            }
            // Mez-protection keys are raw magnitude, not a percentage.
            key if key.starts_with("prot") => {
                surface_stat_block_key(g, key, decimal);
            }
            // Every other key names a GlobalBonuses field directly (× 100).
            key => {
                surface_stat_block_key(g, key, decimal * 100.0);
            }
        }
    }
}

/// Accumulate an equipped, active Destiny power's stat bonuses into `g`, resolved at
/// the power's sustained-floor decay time, then optionally Alpha-enhanced and
/// Genesis-Fate-scaled. Ports the Destiny block of `applyIncarnateBonuses`.
fn apply_destiny(
    g: &mut GlobalBonuses,
    incarnates: &IncarnateLoadout,
    fx: &IncarnateEffects,
    fate_multiplier: f64,
    destiny_time: Option<f64>,
    breakdown: &mut Vec<IncarnateBreakdownSource>,
) {
    let Some(destiny) = incarnates.destiny.as_ref().filter(|s| s.active) else {
        return;
    };
    let destiny_id = normalize_incarnate_power_id(&destiny.power_name);
    let before = g.clone();

    // `destiny_time` scrubs the diminishing buff (the beta `destinyTime` option). `None`
    // resolves at the sustained-floor time, the value a perma-Destiny build holds and the
    // beta's default `destinyTime: null` path; `Some(t)` reads the timeline at `t`.
    let time = destiny_time.unwrap_or_else(|| fx.destiny_sustained_floor_time(&destiny_id));
    let Some(mut effects) = fx.destiny_effects_at_time(&destiny_id, time) else {
        return;
    };

    // Alpha enhances Destiny buffs the game says accept its aspects, gated by the
    // power's boosts_allowed. Only when the Alpha slot is equipped AND active.
    if let Some(alpha) = incarnates.alpha.as_ref().filter(|s| s.active) {
        let boosts_allowed = fx
            .destiny_boosts
            .get(&destiny_id)
            .cloned()
            .unwrap_or_default();
        let alpha_effects = fx
            .alpha_effects
            .get(&normalize_incarnate_power_id(&alpha.power_name));
        effects = apply_alpha_to_destiny(effects, &boosts_allowed, alpha_effects);
    }

    // Fate Genesis (Rebirth) amplifies the whole Destiny block by its tier %.
    if fate_multiplier > 0.0 {
        scale_destiny_effects(&mut effects, 1.0 + fate_multiplier);
    }

    if let Some(v) = effects.defense_all {
        let value = v * 100.0;
        for t in DEFENSE_TYPES {
            route_closed(g.add_defense(t, value), t);
        }
    }
    if let Some(v) = effects.resistance_all {
        let value = v * 100.0;
        for t in RESISTANCE_TYPES {
            route_closed(g.add_resistance(t, value), t);
        }
    }
    if let Some(v) = effects.regeneration {
        g.regeneration += v * 100.0;
    }
    if let Some(v) = effects.recovery {
        g.recovery += v * 100.0;
    }
    if let Some(v) = effects.damage {
        g.damage += v * 100.0;
    }
    if let Some(v) = effects.to_hit {
        g.to_hit += v * 100.0;
    }
    if let Some(v) = effects.recharge {
        g.recharge += v * 100.0;
    }
    if let Some(v) = effects.max_hp {
        g.max_hp += v * 100.0;
    }
    if let Some(v) = effects.max_endurance {
        g.max_endurance += v * 100.0;
    }
    if let Some(v) = effects.heal_received {
        g.heal_received += v * 100.0;
    }
    // Mez protection (Clarion) is a flat magnitude to all six status types, NOT × 100.
    if let Some(mag) = effects.mez_protection {
        for mez in MEZ_STATUS_TYPES {
            route_closed(g.add_mez_protection(mez, mag), mez);
        }
    }
    // Knockback/Knockup protection (Clarion) gets its own total, raw magnitude.
    if let Some(kb) = effects.kb_protection {
        route_closed(g.add_mez_protection("knockback", kb), "knockback");
    }
    // Run/Jump/Fly speed (Incandescence Radial) is one buff over all three movement axes.
    if let Some(v) = effects.run_speed {
        let value = v * 100.0;
        g.run_speed += value;
        g.fly_speed += value;
        g.jump_height += value;
    }

    // One contributor, however the time-resolve / Alpha enhance / Fate scale combined above.
    record_incarnate(
        &before,
        g,
        DESTINY_SLOT,
        &destiny.power_name,
        false,
        breakdown,
    );
}

/// One equipped slot's level-shift grant: which slot, what is in it, and how far it shifts.
///
/// Public because two surfaces need the SAME list: this pass spends it, and the Combat panel's
/// level-shift control reads it to know how far the build's own loadout can shift (its ceiling)
/// and to name the slots behind that ceiling. Two derivations of "what did the loadout earn"
/// would let the control offer a step the calc refuses to spend.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelShiftGrant {
    /// The catalog slot id ([`ALPHA_SLOT`] and friends), as the breakdown rows spell it.
    pub slot: &'static str,
    /// The equipped power's `internalName`, as the loadout stores it.
    pub power_name: String,
    /// How far this slot shifts. Never zero — a slot granting nothing is absent from the list.
    pub shift: f64,
}

/// The level shifts the equipped loadout has earned, in spend order.
///
/// Order is load-bearing, not cosmetic: [`apply_level_shift`] spends a user-set ceiling down
/// this list, so the first entry is the shift that survives the tightest ceiling. Alpha leads
/// because it is the slot the game unlocks first and the one a player reading their build at a
/// single shift is holding.
///
/// Alpha's and Destiny's `level_shift` is optional in the tables and Lore's is a plain `f64`;
/// all three are filtered on non-zero, so "no shift" and "a stated zero" collapse the same way
/// they always did here.
pub fn level_shift_grants(
    incarnates: &IncarnateLoadout,
    fx: &IncarnateEffects,
) -> Vec<LevelShiftGrant> {
    let alpha = incarnates.alpha.as_ref().and_then(|slot| {
        let shift = fx
            .alpha_effects
            .get(&normalize_incarnate_power_id(&slot.power_name))
            .and_then(|a| a.level_shift)?;
        Some((ALPHA_SLOT, &slot.power_name, shift))
    });
    let destiny = incarnates.destiny.as_ref().and_then(|slot| {
        let shift = fx
            .destiny_effects
            .get(&normalize_incarnate_power_id(&slot.power_name))
            .and_then(|d| d.level_shift)?;
        Some((DESTINY_SLOT, &slot.power_name, shift))
    });
    let lore = incarnates.lore.as_ref().and_then(|slot| {
        let shift = fx
            .lore_effects
            .get(&normalize_incarnate_power_id(&slot.power_name))
            .map(|l| l.level_shift)?;
        Some((LORE_SLOT, &slot.power_name, shift))
    });

    [alpha, destiny, lore]
        .into_iter()
        .flatten()
        .filter(|&(_, _, shift)| shift != 0.0)
        .map(|(slot, power_name, shift)| LevelShiftGrant {
            slot,
            power_name: power_name.clone(),
            shift,
        })
        .collect()
}

/// Sum the incarnate level shift from Alpha, Destiny, and Lore into `g.level_shift`, spending
/// at most `ceiling`. NEVER gated by the per-slot stat toggles; equipping the slot is enough.
/// Destiny reads its FLAT table value, not the time-resolved / enhanced one. Ports the
/// level-shift block of `applyIncarnateBonuses`, plus the ceiling the beta lacked.
///
/// `ceiling` is [`coh_data::CombatContext::incarnate_level_shift`]: `None` spends every earned
/// shift (the prior fixed behaviour), `Some(n)` spends `n` of them. It is spent DOWN
/// [`level_shift_grants`] rather than subtracted from the sum, so each slot's provenance row
/// still states what that slot actually contributed and the rows still add up to
/// `g.level_shift`. A slot the ceiling leaves nothing for files no row, exactly as an
/// unequipped one does — the breakdown says which shifts are being read, not which were earned.
fn apply_level_shift(
    g: &mut GlobalBonuses,
    incarnates: &IncarnateLoadout,
    fx: &IncarnateEffects,
    ceiling: Option<f64>,
    breakdown: &mut Vec<IncarnateBreakdownSource>,
) {
    let mut remaining = ceiling.unwrap_or(f64::INFINITY).max(0.0);
    for grant in level_shift_grants(incarnates, fx) {
        let spend = grant.shift.min(remaining);
        if spend <= 0.0 {
            break;
        }
        remaining -= spend;
        let before = g.clone();
        g.level_shift += spend;
        record_incarnate(&before, g, grant.slot, &grant.power_name, false, breakdown);
    }
}

/// Pass 6: apply the incarnate stat bonuses into `g`. Called once from
/// `recalculate` after the inherents pass, before the Pass-8 combat projection (the
/// level shift it writes feeds `effective_level_diff`). Ports `applyIncarnateBonuses`.
///
/// Returns this pass's provenance rows ([`IncarnateBreakdownSource`]), one per (contributor,
/// breakdown key): the equipped Destiny, Hybrid and Genesis Socket stat blocks, each slot's level
/// shift, and the below-45 Genesis-Fate exemplar buff. Empty when nothing is equipped or every
/// contribution suppressed.
pub fn apply_incarnate_bonuses(
    g: &mut GlobalBonuses,
    incarnates: &IncarnateLoadout,
    exemplar_level: Option<Level>,
    destiny_time: Option<f64>,
    hybrid_targets_hit: Option<u32>,
    level_shift_ceiling: Option<f64>,
    db: &PowerDatabase,
) -> Vec<IncarnateBreakdownSource> {
    let fx = &db.incarnate_effects;
    let mut breakdown = Vec::new();

    // Exemplared below 45: every normal incarnate contribution is off. The ONLY thing
    // that survives is a Fate Genesis's exemplar buff.
    if incarnates_suppressed(exemplar_level) {
        apply_genesis_exemplar_buff(g, incarnates, fx, &mut breakdown);
        return breakdown;
    }

    // Alpha's enhancement of OTHER powers is modeled in the apply pass (the ED split fed by
    // [`alpha_enhancement`]); its own level shift and Destiny enhancement land below.

    // Fate Genesis amplifies the Destiny block; resolve it up front.
    let fate_multiplier = match active_genesis(incarnates, fx) {
        Some(genesis) if genesis.tree == "fate" => genesis.tier_percent,
        _ => 0.0,
    };

    apply_destiny(
        g,
        incarnates,
        fx,
        fate_multiplier,
        destiny_time,
        &mut breakdown,
    );

    // Hybrid: passive is always-on (equipping alone grants it); frontLoaded and perTarget only
    // when the slot is toggled on — the per-foe rows live in the toggle's own effect groups,
    // behind a `target != source` gate, so nothing stacks off a toggle that is not running.
    // All three layers are ONE contributor: the beta labels them with the same equipped power,
    // and a player reads "my Hybrid gives me X", not a passive row beside a front-loaded one.
    if let Some(hybrid) = &incarnates.hybrid {
        if let Some(effects) = fx
            .hybrid_effects
            .get(&normalize_incarnate_power_id(&hybrid.power_name))
        {
            let before = g.clone();
            apply_hybrid_stat_block(g, &effects.passive, 1.0);
            if hybrid.active {
                apply_hybrid_stat_block(g, &effects.front_loaded, 1.0);
                // One row per foe in the sphere, up to the power's own ceiling. The clamp is
                // the data's (`maxTargets`, the foe capacity of the AoE), never a number
                // written here: the Melee line caps at 4, 7 or 9 by tier. A build with no
                // foe count stated reads as zero foes, the same absence the per-power
                // stacking slider takes ([`crate::stacking`]) — a solo total should not
                // assume a crowd.
                let foes = f64::from(hybrid_targets_hit.unwrap_or(0)).min(effects.max_targets);
                if foes > 0.0 {
                    apply_hybrid_stat_block(g, &effects.per_target, foes);
                }
            }
            record_incarnate(
                &before,
                g,
                HYBRID_SLOT,
                &hybrid.power_name,
                false,
                &mut breakdown,
            );
        }
    }

    apply_genesis_socket(g, incarnates, fx, &mut breakdown);

    // Interface is enemy-debuff procs, not player stats. Nothing to add (authored fact).

    // Level shift (Alpha / Destiny / Lore T3+), gated only by the level-shift flag.
    apply_level_shift(g, incarnates, fx, level_shift_ceiling, &mut breakdown);

    breakdown
}
