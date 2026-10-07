//! Buff-pet fold — the port of the beta's `expandBuffPetAuras` / `buffPetAuraEffects`
//! (`legacy-totals.oracle.ts`, Step 7.2) plus the shared detection in
//! `utils/calculations/buff-pet-auras.ts`, extended to the protection and resistance FACES the
//! beta had no reader for at all (ENT-12).
//!
//! A buff-pet is a summonable, non-commandable drone whose purpose is to project a persistent
//! ally buff onto the caster and team — Traps' Force Field Generator (+Defense and mez
//! protection), Electrical Affinity's Faraday Cage (mez protection and debuff resistance and
//! nothing else), Traps' Triage Beacon (+Regeneration). The buff lives on the PET ENTITY's
//! abilities, never on the summoning power, so the per-power walk that produces the totals cannot
//! see it: without this pass a build running Force Field Generator shows none of its ~13%
//! defense, and one running Faraday Cage shows nothing whatsoever — its parent power carries a
//! single `EntCreate` atom and an empty effects bag.
//!
//! **Who receives it is a separate question from what it is, and it is asked FIRST.** A pet's
//! `HoldProtection` row and the one Singularity hands the foe it just held are the same type at
//! the same aspect; only the pet POWER's recipients tell them apart
//! ([`crate::granted::reaches_summoner`]), so the walk gates on that before consulting the
//! vocabulary. For the seven ally-aura types the gate is a guard rather than a fix — every
//! build-reachable aura row is `Friend`-facing on all three forks (29 / 48 / 18 rows) — but the
//! `["Foe"]` rows it excludes are real: Tech Lab's `RecoveryBuff` 4.0 would have read as +400%
//! recovery had a power summoned it.
//!
//! OPT-IN, per pet. The fold is gated on the summoning power's
//! `<powerset>:<internalName>:buffpet` entry in [`coh_data::CombatContext::power_state`]
//! ([`buff_pet_toggle_key`]), off by default — a build that has not enabled a pet produces
//! nothing here and its totals are unchanged. That toggle, NOT the parent's active state, is
//! the gate: a click summon has no persistent `is_active`.
//!
//! [`buff_pet_sources`] is the read behind the control that writes it, and it walks what this
//! folds — so the opt-in is offered exactly where there is something to fold, and it can name
//! the totals it will move.
//!
//! Values take the summoning power's enhancement when the summon is `CopyBoosts`, which every
//! buff-pet on Homecoming is (Force Field Generator, Triage Beacon, Faraday Cage, Spirit Tree,
//! Prismatic Shield): the game hands the pet the summoner's slotting, so three Defense IOs in
//! Prismatic Shield raise the shield's defense. The beta resolved these base with slot-less
//! synthetic powers, and so did this pass until Prismatic Shield was reported ignoring its
//! enhancements. Each row is enhanced by the aspect the parent route uses for the same row
//! (see [`enh_multiplier`]). No strength: `CopyCreatorMods` would carry it, and it is not modelled
//! here.
//!
//! The AT table is read at the PET's own character class where the entity states one and at the
//! build's archetype where it does not, which is not a choice about tidiness: `Res_Boolean`
//! differs enough between the two that Force Field Generator's mag-20 hold protection is 6.92 on
//! `minion_pets` and 8.65 read as a Defender (ENT-10).

use crate::apply::{defense_enh, power_enhancement};
use crate::enhancement::EnhancementBonuses;
use crate::granted::{faced_route, FacedFamily, FacedRoute};
use crate::incarnates::AlphaEnhancement;
use crate::scaled::resolve_scaled_effect_for;
use crate::totals::{route_closed, CalcError, GlobalBonuses, TypeRoute};
use coh_data::{CharacterState, Power, PowerDatabase, TableScope};
use serde_json::Value;
use std::collections::HashSet;

/// Per-power conditional sub-id for a summon's "count this buff-pet's aura" toggle, keyed
/// `<power internal name>:<this>`. Mirrors the beta `BUFF_PET_TOGGLE_ID`.
const BUFF_PET_TOGGLE_ID: &str = "buffpet";

/// The [`coh_data::CombatContext::power_state`] key a summon's buff-pet toggle is stored
/// under. The fold and its control resolve it through this one function, so the opt-in a
/// surface writes is by construction the one the totals read.
pub fn buff_pet_toggle_key(powerset: &str, power_internal_name: &str) -> String {
    let address = coh_data::power_address(powerset, power_internal_name);
    format!("{address}:{BUFF_PET_TOGGLE_ID}")
}

/// One contribution a buff-pet makes to the dashboard breakdown — the beta's per-pet active-power
/// row. The engine emits the facts and the camelCase `breakdown_key`; the output mapper resolves the
/// summoning power's display name (which is what the row is labelled with, exactly as the beta labels
/// its synthetic pet power).
///
/// One row per key the pet writes, ally aura and faced alike, EXCEPT the knockback pair: knockback
/// and knockup protection are one stat, so they arrive as a single `protKnockback` row carrying the
/// pair's max rather than as two the reader would have to know not to add.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct BuffPetBreakdownSource {
    pub breakdown_key: String,
    pub value: f64,
    pub power_internal_name: String,
    pub power_set: String,
}

/// Fold every toggled-on buff-pet's ally buffs into `g`, returning the breakdown rows.
///
/// Walks `all_selected()` rather than the gathered active powers: the gate is the per-pet
/// toggle, and a click summon is never "active".
pub fn apply_buff_pet_auras(
    state: &CharacterState,
    db: &PowerDatabase,
    alpha: &AlphaEnhancement,
    g: &mut GlobalBonuses,
    errors: &mut Vec<CalcError>,
) -> Vec<BuffPetBreakdownSource> {
    let archetype = state.archetype.id.as_deref().unwrap_or("");
    let level = state.level as i32;
    let mut out = Vec::new();

    for selection in state.all_selected() {
        let toggle_key = buff_pet_toggle_key(&selection.powerset, &selection.internal_name);
        if state.combat.power_state.get(&toggle_key) != Some(&true) {
            continue;
        }
        let Some(power) =
            crate::gather::resolve_power(db, &selection.powerset, &selection.internal_name)
        else {
            continue;
        };
        // The summoner's slotting reaches the pet only through `CopyBoosts`; without it the pet's
        // aura runs on nothing but its own base.
        let copies_boosts = power
            .summon()
            .and_then(|summon| summon.get("copyBoosts"))
            .and_then(Value::as_bool)
            == Some(true);
        let enh = if copies_boosts {
            power_enhancement(
                &selection.slots,
                power,
                alpha,
                level,
                &state.combat,
                db,
                errors,
            )
        } else {
            EnhancementBonuses::default()
        };

        // Knockback and knockup protection are one stat granted as a pair, so they fold to `max`
        // before reaching `g` — the same fold the parent route applies across a single power's own
        // protection slots. Accumulated over the WHOLE summoning power, which on this corpus is the
        // same thing as per-ability: no power spreads ally KB/KU protection across two abilities
        // (measured, all three forks), so the pair is always Faraday Cage's or EMP Arrow's single
        // row pair.
        let mut knockback_protection = 0.0_f64;
        let mut row = |breakdown_key: &str, value: f64| {
            out.push(BuffPetBreakdownSource {
                breakdown_key: breakdown_key.to_string(),
                value,
                power_internal_name: selection.internal_name.clone(),
                power_set: selection.powerset.clone(),
            });
        };

        each_folded_row(power, db, |_pet, pet_class, effect| {
            let scope = scope(pet_class, archetype);
            let kind = effect.get("type").and_then(Value::as_str);
            if let Some(faced) = kind.and_then(faced_route) {
                let value = faced_value(faced.family, effect, scope, level, db, errors)
                    * enh_multiplier(effect, Some(faced), &enh);
                if value == 0.0 {
                    return;
                }
                if faced.family == FacedFamily::Protection
                    && matches!(faced.type_key, "knockback" | "knockup")
                {
                    knockback_protection = knockback_protection.max(value);
                    return;
                }
                if let Some(breakdown_key) = route_faced(g, faced, value, kind, errors) {
                    row(breakdown_key, value);
                }
                return;
            }
            let value =
                aura_value(effect, scope, level, db, errors) * enh_multiplier(effect, None, &enh);
            if value == 0.0 {
                return;
            }
            for breakdown_key in aura_keys(effect) {
                route_closed(g.add_by_camel_name(breakdown_key, value), breakdown_key);
                row(breakdown_key, value);
            }
        });

        if knockback_protection > 0.0 {
            route_closed(
                g.add_mez_protection("knockback", knockback_protection),
                "knockback",
            );
            row("protKnockback", knockback_protection);
        }
    }
    out
}

/// Send one faced value through its family's router, returning the breakdown key to label the row
/// with — or `None` when there is no row to write.
///
/// The three [`TypeRoute`] verdicts are answered separately rather than through
/// [`route_closed`], because two of them are reachable here. `Unspent` is a real outcome:
/// `KnockupResist` routes to the key `add_mez_resistance` declares unspent (the knockback key
/// already carries the pair's value), so it contributes nothing AND names no field — the decision
/// lives at the router with its warrant, and this call site does not re-state it. `Unknown` cannot
/// happen while [`crate::granted::FacedRoute`]'s keys match the routers, so it is surfaced as a
/// [`CalcError`] rather than swallowed: a key that stops routing is a contribution that silently
/// never arrives, which is Rule 1's whole subject.
fn route_faced(
    g: &mut GlobalBonuses,
    faced: FacedRoute,
    value: f64,
    kind: Option<&str>,
    errors: &mut Vec<CalcError>,
) -> Option<&'static str> {
    let route = match faced.family {
        FacedFamily::Protection => g.add_mez_protection(faced.type_key, value),
        FacedFamily::MezResistance => g.add_mez_resistance(faced.type_key, value),
        FacedFamily::DebuffResistance => g.add_debuff_resistance(faced.type_key, value),
    };
    match route {
        TypeRoute::Routed => faced.breakdown_key,
        TypeRoute::Unspent(_) => None,
        TypeRoute::Unknown => {
            errors.push(CalcError::new(
                "buff-pet face",
                format!(
                    "pet effect type {:?} routes to {:?}, which its family's router does not take",
                    kind.unwrap_or("<untyped>"),
                    faced.type_key,
                ),
            ));
            None
        }
    }
}

/// One faced row's value in its family's unit — the parent power route's own arithmetic
/// ([`crate::apply`] Pass 2b), so a pet's protection and a power's own are the same kind of number
/// in the same slot.
///
/// Base only; the caller applies [`enh_multiplier`].
///
/// **The scale is read as stated, not `|scale|`.** Both converters normalize a face to a positive
/// scale (0 of the corpus's 920 faced rows is negative) because a NEGATIVE resistance-aspect row is
/// published as ENT-12 step 1's `vulnerability` face and never wears one of these names. An `abs`
/// here would therefore guard nothing today and would silently republish a converter regression as a
/// buff — the exact defect step 1 closed. A negative subtracts, visibly.
///
/// **No `Res_Boolean` gate either, and the absence is a decision.** The parent route needs one
/// because a bag `knockback` slot holds both self-protection and a foe-facing knockback ATTACK, and
/// the table name is its proxy for which. Here the converter has already forked the face and the
/// recipient is already answered, so carrying the gate across would only drop real ally protection:
/// Faraday Cage's KB/KU 10 rides `Ranged_Ones`.
fn faced_value(
    family: FacedFamily,
    effect: &Value,
    scope: TableScope<'_>,
    level: i32,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> f64 {
    let scale = effect.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
    let table = effect.get("table").and_then(Value::as_str);
    let resolved = resolve_scaled_effect_for(scale, table, scope, level, db, errors);
    match family {
        // A flat protection MAGNITUDE — mag 6.92 on Force Field Generator, the same units an armor
        // set's protection lands in. No `× 100`.
        FacedFamily::Protection => resolved,
        FacedFamily::MezResistance | FacedFamily::DebuffResistance => resolved * 100.0,
    }
}

/// The enhancement multiplier on one folded row: the aspect the parent power route enhances the
/// same row by ([`crate::apply`]), so a slotted buff-pet and a slotted armor agree on what a
/// Defense IO does. `enh` is empty unless the summon is `CopyBoosts`, and an `ignoreStrength` row
/// stays flat as it does on the parent.
///
/// Protection is unenhanced, as the parent's `Res_Boolean` armor protection is. Recharge buffs are
/// unenhanced, as the parent reads them scale-directly. Of the debuff resistances only defense
/// takes an enhancement, by Defense, and taunt/placate resistance takes none (both as the parent).
fn enh_multiplier(effect: &Value, faced: Option<FacedRoute>, enh: &EnhancementBonuses) -> f64 {
    if effect.get("ignoreStrength").and_then(Value::as_bool) == Some(true) {
        return 1.0;
    }
    let bonus = match faced {
        Some(faced) => match (faced.family, faced.type_key) {
            (FacedFamily::Protection, _) => 0.0,
            (FacedFamily::MezResistance, "taunt" | "placate") => 0.0,
            (FacedFamily::MezResistance, ty) => enh.get(ty),
            (FacedFamily::DebuffResistance, "defense") => enh.get("defense"),
            (FacedFamily::DebuffResistance, _) => 0.0,
        },
        None => match effect.get("type").and_then(Value::as_str) {
            Some("DefenseBuff") => defense_enh(enh),
            Some("ResistanceBuff") => enh.get("resistance"),
            Some("RegenBuff") => enh.get("heal"),
            Some("RecoveryBuff") => enh.get("enduranceMod"),
            Some("ToHitBuff") => enh.get("tohit"),
            Some("Absorb") => enh.get("absorb"),
            _ => 0.0,
        },
    };
    1.0 + bonus
}

/// One buff-pet `power` summons, and the totals its ally buffs reach.
///
/// The read behind the opt-in control: it walks what [`apply_buff_pet_auras`] folds, through the
/// same traversal and the same key vocabulary, so a control offered here is a control that moves
/// exactly those numbers. Empty for a power that summons nothing, summons a commandable pet, or
/// summons one carrying no ally buff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuffPetSource {
    /// The pet's own display name, as the entity states it.
    pub pet: String,
    /// The [`GlobalBonuses`] keys its ally buffs write, in walk order and deduped.
    pub breakdown_keys: Vec<String>,
}

pub fn buff_pet_sources(power: &Power, db: &PowerDatabase) -> Vec<BuffPetSource> {
    let mut sources: Vec<BuffPetSource> = Vec::new();
    each_folded_row(power, db, |pet, _pet_class, effect| {
        let source = match sources.iter_mut().find(|source| source.pet == pet) {
            Some(existing) => existing,
            None => {
                sources.push(BuffPetSource {
                    pet: pet.to_string(),
                    breakdown_keys: Vec::new(),
                });
                sources.last_mut().expect("just pushed")
            }
        };
        let faced = effect
            .get("type")
            .and_then(Value::as_str)
            .and_then(faced_route);
        // A faced row names one field; an aura names one per sub-type it lists. The `None`
        // breakdown key of a router-unspent face names nothing, which is what stops the control
        // promising a total that will not move.
        let keys: Vec<&str> = match faced {
            Some(faced) => faced.breakdown_key.into_iter().collect(),
            None => aura_keys(effect),
        };
        for key in keys {
            if !source.breakdown_keys.iter().any(|held| held == key) {
                source.breakdown_keys.push(key.to_string());
            }
        }
    });
    sources.retain(|source| !source.breakdown_keys.is_empty());
    sources
}

/// Visit every row the pets `power` summons contribute to the SUMMONER's totals — the ally-buff
/// auras and the protection/resistance faces — deduped.
///
/// The dedupe spans the entity's abilities AND the summon's entities, on the beta's own
/// [`aura_key`] — a pet whose bubble is authored on two abilities must not count twice.
fn each_folded_row(
    power: &Power,
    db: &PowerDatabase,
    mut visit: impl FnMut(&str, Option<&str>, &Value),
) {
    let Some(summon) = power.summon() else {
        return;
    };

    let mut seen: HashSet<String> = HashSet::new();
    // Which types reach the summoner's totals is asked of the one vocabulary that names every pet
    // effect type, rather than of a second list beside it: the beta kept a `BUFF_PET_AURA_TYPES`
    // array here, and two lists over one vocabulary is how a type ends up folded by one consumer and
    // dropped by the other (ENT-9).
    // Returns whether it folded anything, which is what the entity/inline precedence below is
    // decided on.
    let mut visit_ability = |pet: &str, pet_class: Option<&str>, ability: &Value| {
        // Whose buff it is, before what it is. Every faced row is authored `target: AnyAffected`,
        // so the recipient is a fact about the pet's POWER and nothing on the row itself can carry
        // it — see [`crate::granted::reaches_summoner`].
        if !crate::granted::reaches_summoner(ability) {
            return false;
        }
        let mut folded = false;
        for effect in ability
            .get("effects")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(kind) = effect.get("type").and_then(Value::as_str) else {
                continue;
            };
            if !crate::granted::folds_into_summoner_totals(kind) {
                continue;
            }
            if !seen.insert(aura_key(effect)) {
                continue;
            }
            visit(pet, pet_class, effect);
            folded = true;
        }
        folded
    };

    let mut from_entity_table = false;
    for entity_name in crate::granted::summon_entity_names(summon) {
        // The whole chain, not just the named entity: a pet's payload can sit in an entity its
        // own Self_Destruct leaves behind (ENT-3 step 4). The commandable skip that used to be
        // written out here lives in the walk now, so a pet a pet calls answers the same rule.
        //
        // No in-place child in any of the three forks carries a folded type today, so this walk
        // returns exactly the root's rows and the fold's numbers are unchanged — it shares the
        // chain so that stays true by construction if one ever does, rather than by nobody
        // remembering this call site.
        for entity in crate::granted::summoned_entity_chain(db, &entity_name) {
            let pet = entity
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or(&entity_name);
            let pet_class = entity.get("characterClass").and_then(Value::as_str);
            for ability in crate::granted::entity_abilities(entity) {
                from_entity_table |= visit_ability(pet, pet_class, ability);
            }
        }
    }

    // Synthesized location pseudo-pets carry their entity inline, and this is where Faraday Cage,
    // EMP Arrow and Force Bubble live — the reason those three reached nothing before ENT-12 step 2
    // was not their types (Faraday Cage's ally `ResistanceBuff` routes `AllyAura` correctly) but
    // this walk, which stopped at the entity table while `granted::pseudo_pet_effects` did not.
    //
    // Read only when the table folded nothing. The precedence is ENT-8's: Sentinel Whirlpool
    // carries both blocks for the SAME pet, identical row for row, and walking both would add the
    // pet to itself and double every number it publishes.
    //
    // The test is what THIS walk folded, not whether the table's abilities carry effects at all,
    // and the difference is a live case (ENT-17). Spirit Tree's entity is real — a taunt and the
    // tree's own resistances — while the +Regen aura it exists to project lives only in the
    // redirect the converter synthesizes inline. A presence test reads that entity as the answer
    // and declines the block holding the payload, so the aura reaches no total. Whirlpool is
    // unaffected either way: its table rows ARE folded, so the inline block stays skipped, and
    // `seen` would dedupe them even if it weren't.
    //
    // These shells have no `villaindef.bin` record and therefore no character class, so their
    // tables resolve against the summoner's archetype — the data, not a gap in it (ENT-10).
    if !from_entity_table {
        for resolved in summon
            .get("resolvedEntities")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let pet = resolved
                .get("displayName")
                .and_then(Value::as_str)
                .unwrap_or("");
            for ability in crate::granted::entity_abilities(resolved) {
                visit_ability(pet, None, ability);
            }
        }
    }
}

/// The [`GlobalBonuses`] keys one aura effect writes. A by-type buff writes one per type it
/// names; everything else writes a single scalar.
///
/// Public so the route gate can grade it per ROW. Which types are walked and which keys
/// each writes are two separate decisions, so a type can be an ally aura and still write nothing
/// — and a pet whose bubble also grants defense hides that from any check that only asks whether
/// the PET reached a key (ENT-9).
pub fn aura_keys(effect: &Value) -> Vec<&'static str> {
    match effect.get("type").and_then(Value::as_str) {
        Some("DefenseBuff") => type_list(effect, "defenseTypes")
            .iter()
            .filter_map(|ty| defense_key(ty))
            .collect(),
        Some("ResistanceBuff") => type_list(effect, "resistanceTypes")
            .iter()
            .filter_map(|ty| resistance_key(ty))
            .collect(),
        Some("Absorb") => vec!["absorb"],
        Some("RegenBuff") => vec!["regeneration"],
        Some("RecoveryBuff") => vec!["recovery"],
        Some("ToHitBuff") => vec!["toHit"],
        Some("RechargeBuff") => vec!["recharge"],
        _ => Vec::new(),
    }
}

/// The class an aura's table is read under: the pet's own, or the build's archetype when the
/// entity states none.
fn scope<'a>(pet_class: Option<&'a str>, archetype: &'a str) -> TableScope<'a> {
    match pet_class {
        Some(class) => TableScope::Pet(class),
        None => TableScope::Archetype(archetype),
    }
}

/// One aura effect's value in its keys' display unit. Every key an effect writes carries the
/// same value — a bubble granting melee and ranged defense grants the same percentage to both.
fn aura_value(
    effect: &Value,
    scope: TableScope<'_>,
    level: i32,
    db: &PowerDatabase,
    errors: &mut Vec<CalcError>,
) -> f64 {
    let scale = effect.get("scale").and_then(Value::as_f64).unwrap_or(0.0);
    let table = effect.get("table").and_then(Value::as_str);
    let mut resolved = || resolve_scaled_effect_for(scale, table, scope, level, db, errors);
    match effect.get("type").and_then(Value::as_str) {
        Some("DefenseBuff" | "ResistanceBuff" | "RegenBuff" | "RecoveryBuff" | "ToHitBuff") => {
            resolved() * 100.0
        }
        // Flat-HP absorb off a Heal table: the game folds a pet's aspect=Maximum Absorb to a
        // flat amount, and the MaxHP-fraction form is an Expression the pet parser does not
        // carry — so this never routes through the absorb-fraction resolve.
        Some("Absorb") => resolved(),
        // A recharge buff carries its final fraction on a `*_Ones` table, so it is read
        // scale-directly × 100 with no AT-table resolution — the same rule the per-power apply
        // pass uses.
        Some("RechargeBuff") => scale * 100.0,
        _ => 0.0,
    }
}

/// The beta `auraKey` — type + scale + table + the joined sub-type list.
fn aura_key(effect: &Value) -> String {
    let subs = if effect.get("defenseTypes").is_some() {
        type_list(effect, "defenseTypes")
    } else {
        type_list(effect, "resistanceTypes")
    };
    format!(
        "{}|{}|{}|{}",
        effect.get("type").and_then(Value::as_str).unwrap_or(""),
        effect
            .get("scale")
            .and_then(Value::as_f64)
            .map(|s| s.to_string())
            .unwrap_or_default(),
        effect.get("table").and_then(Value::as_str).unwrap_or(""),
        subs.join(","),
    )
}

fn type_list(effect: &Value, field: &str) -> Vec<String> {
    effect
        .get(field)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default()
}

/// Lowercase defense type → the camelCase `GlobalBonuses` field the beta breakdown keys on.
fn defense_key(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "melee" => "defMelee",
        "ranged" => "defRanged",
        "aoe" => "defAoE",
        "smashing" => "defSmashing",
        "lethal" => "defLethal",
        "fire" => "defFire",
        "cold" => "defCold",
        "energy" => "defEnergy",
        "negative" => "defNegative",
        "psionic" => "defPsionic",
        "toxic" => "defToxic",
        _ => return None,
    })
}

fn resistance_key(ty: &str) -> Option<&'static str> {
    Some(match ty {
        "smashing" => "resSmashing",
        "lethal" => "resLethal",
        "fire" => "resFire",
        "cold" => "resCold",
        "energy" => "resEnergy",
        "negative" => "resNegative",
        "psionic" => "resPsionic",
        "toxic" => "resToxic",
        _ => return None,
    })
}
