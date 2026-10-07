//! `Power` / `Powerset` — the contract's power shape. One shape for powerset, pool, and
//! epic powers (the five-converter legacy is invisible here).
//!
//! Atoms decode strictly into typed `AtomicEffect`s. Everything else the contract
//! carries (the transitional `effects` bag, stats, display fields) is preserved as raw
//! JSON in `extra` — quarantined, reachable, and shrinking as atomization completes.
//! Nothing is dropped: the roundtrip test proves counts against the manifest.

use crate::atom::{AtomicEffect, EffectType, SubType};
use crate::atom_wire::{decode_atom, AtomDecodeError};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct PowerWire {
    name: String,
    #[serde(rename = "internalName")]
    internal_name: Option<String>,
    #[serde(default)]
    atoms: Vec<Vec<Value>>,
    #[serde(default, rename = "allowedEnhancements")]
    allowed_enhancements: Option<Vec<String>>,
    #[serde(default, rename = "allowedSetCategories")]
    allowed_set_categories: Option<Vec<String>>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Power {
    pub name: String,
    pub internal_name: Option<String>,
    /// Owned per power — never normalized into a shared table (REBUILD-PLAN §2).
    pub atoms: Vec<AtomicEffect>,
    /// The enhancement categories this power accepts (the beta `EnhancementStatType[]`,
    /// values like `"Damage"`, `"Recharge"`, `"Defense Debuff"`). Gates which Alpha
    /// incarnate aspects reach the power in [`coh_math`]'s `combineWithAlphaED` /
    /// `filterAlphaByAllowedEnhancements`. `None` when the wire omits the field —
    /// 127 pseudo-pet/temp powers (Meteor, Category Five) carry no `allowedEnhancements`
    /// at all, and the beta's gate treats absent (accept every aspect) as DISTINCT from
    /// an empty list (accept none), so the distinction is preserved rather than
    /// collapsed to an empty `Vec` (undefined ≠ discarded).
    pub allowed_enhancements: Option<Vec<String>>,
    /// The IO-set categories this power accepts (the beta `allowedSetCategories`,
    /// values like `"Ranged Damage"`, `"Holds"`, `"Blaster Archetype Sets"` — the
    /// archetype's ATO category is already baked in per power by the converter). The
    /// picker's IO-Set tab keeps only the dataset's sets whose `type` is in this list
    /// ([`crate::IoSetCatalog::sets_for_power`]). `None` when the wire omits the field;
    /// a power with no set categories accepts no IO sets, so `None` and an empty list
    /// behave identically here — the distinction is kept only as provenance (undefined
    /// ≠ discarded).
    pub allowed_set_categories: Option<Vec<String>>,
    /// The FAST form's atoms, for an interruptible power that ships one (`extra["quickSnipe"]`).
    ///
    /// [`coh_math`]'s effective-power transform swaps this whole list into [`Self::atoms`] when
    /// the build satisfies the form's carried gate — the fast shot is a different attack, not the
    /// slow one cast quicker. It decodes here, beside the base list and through the same strict
    /// path, so a malformed tuple is a load error rather than a silent slow-damage swap; the
    /// transform itself has no error channel to report one through.
    ///
    /// Empty exactly when the power ships no fast form: a `quickSnipe` with no atoms is rejected
    /// at load ([`PowerDecodeError::FastFormWithoutAtoms`]), because that shape is how the fast
    /// cast came to be displayed beside the slow form's charged damage (SNIPE-3).
    pub quick_snipe_atoms: Vec<AtomicEffect>,
    /// Each mode variant's atoms, keyed by the caster mode that selects it — the atom half of
    /// `extra["modeVariants"]`, whose other keys stay raw JSON there.
    ///
    /// Same arrangement and same reason as [`Self::quick_snipe_atoms`]: while a live mode
    /// redirects the power, [`coh_math`]'s effective-power transform swaps this list into
    /// [`Self::atoms`], because the variant is a different record and not the base cast under a
    /// different name. Carrying only the variant's display fields left the projection resolving
    /// the BASE damage behind the variant's stats — Homecoming's Stalagmite states 2.92 under
    /// Seismic Power against a base 0.75, Titan Weapons' Crushing Blow 1.32 against 1.64, and the
    /// forks differ in both directions (MODEVAR-1).
    ///
    /// A `modeVariants` entry with no atoms is rejected at load
    /// ([`PowerDecodeError::ModeVariantWithoutAtoms`]) — every one of the 133 variant records
    /// across the three forks yields a non-empty list, so an empty one is a broken bundle rather
    /// than a variant that does nothing.
    pub mode_variant_atoms: HashMap<String, Vec<AtomicEffect>>,
    /// Each redirect-selected form's atoms, in the order `extra["formVariants"]` lists them —
    /// the atom half of the third form mechanism, whose other keys stay raw JSON there.
    ///
    /// Positional rather than keyed because the selector is the entry's own `condition`, not a
    /// name: the game's redirector walks the table in order and fires the first branch that
    /// holds, and two branches can name the same mode or the same power in different
    /// combinations (Rebirth's Teleport has three, differing only in which pair of Teleportation
    /// picks the build holds). A map would need a synthetic key and would lose the order the
    /// tie-break depends on.
    ///
    /// Same reason as [`Self::quick_snipe_atoms`] and [`Self::mode_variant_atoms`] for carrying
    /// atoms at all: the variant is a DIFFERENT record, so the projection must resolve its damage
    /// from its own list. An entry with no atoms is rejected at load
    /// ([`PowerDecodeError::FormVariantWithoutAtoms`]) — the converter skips such a branch rather
    /// than emitting it, so an empty one means a stale or hand-edited bundle.
    pub form_variant_atoms: Vec<Vec<AtomicEffect>>,
    /// Everything else on the wire, including the transitional `effects` bag.
    pub extra: Map<String, Value>,
}

/// The wire's `mechanicType` vocabulary — the four ways a power exists outside the
/// ordinary pick economy. See [`Power::mechanic_type`] for what each derives from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MechanicType {
    /// An auto-granted child toggle (ammo types, stance forms, adaptations) — already
    /// `autoIssue`, so the grant machinery owns it.
    ChildToggle,
    /// The visible parent of a mechanic (Swap Ammo, the stance parents): a real pick.
    ParentMechanic,
    /// A hidden set-mechanic passive the game grants and revokes (Seismic Shockwaves);
    /// never in any pick list.
    HiddenPassive,
    /// A hidden auto power (`ShowInInventory Never`, no info window); never in any
    /// pick list.
    HiddenAuto,
}

#[derive(Debug, thiserror::Error)]
pub enum PowerDecodeError {
    #[error("power {power:?}, {list} atom {atom_index}: {source}")]
    Atom {
        power: String,
        /// Which of the power's atom lists the tuple came from.
        list: &'static str,
        atom_index: usize,
        #[source]
        source: AtomDecodeError,
    },
    #[error(
        "power {power:?}: quickSnipe carries no atoms — the fast form would show its own cast \
         beside the slow form's damage (DATA-GAP-REGISTER SNIPE-3)"
    )]
    FastFormWithoutAtoms { power: String },
    #[error("power {power:?}: quickSnipe.atoms is not a list of wire tuples: {problem}")]
    MalformedFastFormAtoms { power: String, problem: String },
    #[error(
        "power {power:?}: modeVariants.{mode} carries no atoms — the variant would show its own \
         cast and area over the base power's damage (DATA-GAP-REGISTER MODEVAR-1)"
    )]
    ModeVariantWithoutAtoms { power: String, mode: String },
    #[error("power {power:?}: modeVariants.{mode}.atoms is not a list of wire tuples: {problem}")]
    MalformedModeVariantAtoms {
        power: String,
        mode: String,
        problem: String,
    },
    #[error(
        "power {power:?}: formVariants[{index}] carries no atoms — the variant would show its \
         own cast and area over the base power's damage (DATA-GAP-REGISTER CHAIN-1)"
    )]
    FormVariantWithoutAtoms { power: String, index: usize },
    #[error(
        "power {power:?}: formVariants[{index}].atoms is not a list of wire tuples: {problem}"
    )]
    MalformedFormVariantAtoms {
        power: String,
        index: usize,
        problem: String,
    },
    #[error(
        "power {power:?}: formVariants[{index}] carries no condition — nothing could select it"
    )]
    FormVariantWithoutCondition { power: String, index: usize },
    #[error(
        "power {power:?}: `requires` is not a list of tokens — a gate is a token array on the \
         wire, and a joined one cannot be re-split (DATA-GAP-REGISTER COND-8)"
    )]
    RequiresNotTokens { power: String },
}

/// Decode one wire atom list, naming the power and the list in any error.
fn decode_atoms(
    tuples: &[Vec<Value>],
    power: &str,
    list: &'static str,
) -> Result<Vec<AtomicEffect>, PowerDecodeError> {
    tuples
        .iter()
        .enumerate()
        .map(|(atom_index, tuple)| {
            decode_atom(tuple).map_err(|source| PowerDecodeError::Atom {
                power: power.to_string(),
                list,
                atom_index,
                source,
            })
        })
        .collect()
}

impl TryFrom<PowerWire> for Power {
    type Error = PowerDecodeError;

    fn try_from(wire: PowerWire) -> Result<Self, Self::Error> {
        let ident = wire.internal_name.as_deref().unwrap_or(&wire.name);
        let atoms = decode_atoms(&wire.atoms, ident, "atoms")?;

        // A `quickSnipe` with no atom list at all is the SNIPE-3 shape and is rejected; one whose
        // list is present but EMPTY would say the fast form deals nothing, which no fork's data
        // says, so it is rejected by the same check rather than shipped as a zero.
        let quick_snipe_atoms = match wire.extra.get("quickSnipe") {
            None => Vec::new(),
            Some(form) => {
                let Some(wire_atoms) = form.get("atoms").filter(|atoms| !atoms.is_null()) else {
                    return Err(PowerDecodeError::FastFormWithoutAtoms {
                        power: ident.to_string(),
                    });
                };
                let tuples: Vec<Vec<Value>> =
                    serde_json::from_value(wire_atoms.clone()).map_err(|problem| {
                        PowerDecodeError::MalformedFastFormAtoms {
                            power: ident.to_string(),
                            problem: problem.to_string(),
                        }
                    })?;
                if tuples.is_empty() {
                    return Err(PowerDecodeError::FastFormWithoutAtoms {
                        power: ident.to_string(),
                    });
                }
                decode_atoms(&tuples, ident, "quickSnipe.atoms")?
            }
        };

        let mut mode_variant_atoms = HashMap::new();
        if let Some(variants) = wire.extra.get("modeVariants").and_then(Value::as_object) {
            for (mode, variant) in variants {
                let tuples: Vec<Vec<Value>> = match variant.get("atoms") {
                    None => {
                        return Err(PowerDecodeError::ModeVariantWithoutAtoms {
                            power: ident.to_string(),
                            mode: mode.clone(),
                        })
                    }
                    Some(atoms) => serde_json::from_value(atoms.clone()).map_err(|problem| {
                        PowerDecodeError::MalformedModeVariantAtoms {
                            power: ident.to_string(),
                            mode: mode.clone(),
                            problem: problem.to_string(),
                        }
                    })?,
                };
                if tuples.is_empty() {
                    return Err(PowerDecodeError::ModeVariantWithoutAtoms {
                        power: ident.to_string(),
                        mode: mode.clone(),
                    });
                }
                mode_variant_atoms.insert(
                    mode.clone(),
                    decode_atoms(&tuples, ident, "modeVariants.atoms")?,
                );
            }
        }

        // A gate that is present but not a token list would read as "no gate" everywhere
        // downstream, which is the silently-wrong shape Rule 1 exists to refuse.
        if wire
            .extra
            .get("requires")
            .is_some_and(|value| crate::expression_tokens(Some(value)).is_none())
        {
            return Err(PowerDecodeError::RequiresNotTokens {
                power: ident.to_string(),
            });
        }

        let mut form_variant_atoms = Vec::new();
        if let Some(variants) = wire.extra.get("formVariants").and_then(Value::as_array) {
            for (index, variant) in variants.iter().enumerate() {
                // The condition IS the selector — a variant without one could never be chosen,
                // and would sit in the bundle looking like a form the planner supports.
                if crate::expression_tokens(variant.get("condition")).is_none_or(|c| c.is_empty()) {
                    return Err(PowerDecodeError::FormVariantWithoutCondition {
                        power: ident.to_string(),
                        index,
                    });
                }
                let tuples: Vec<Vec<Value>> = match variant.get("atoms") {
                    None => {
                        return Err(PowerDecodeError::FormVariantWithoutAtoms {
                            power: ident.to_string(),
                            index,
                        })
                    }
                    Some(atoms) => serde_json::from_value(atoms.clone()).map_err(|problem| {
                        PowerDecodeError::MalformedFormVariantAtoms {
                            power: ident.to_string(),
                            index,
                            problem: problem.to_string(),
                        }
                    })?,
                };
                if tuples.is_empty() {
                    return Err(PowerDecodeError::FormVariantWithoutAtoms {
                        power: ident.to_string(),
                        index,
                    });
                }
                form_variant_atoms.push(decode_atoms(&tuples, ident, "formVariants.atoms")?);
            }
        }

        Ok(Power {
            name: wire.name,
            internal_name: wire.internal_name,
            atoms,
            allowed_enhancements: wire.allowed_enhancements,
            allowed_set_categories: wire.allowed_set_categories,
            quick_snipe_atoms,
            mode_variant_atoms,
            form_variant_atoms,
            extra: wire.extra,
        })
    }
}

/// Does a recipient list name an enemy? The one foe vocabulary this repo keeps, the
/// converter's `TSPY_MEZ_FOE_TARGETS` (`{Foe, DeadFoe, DeadOrAliveFoe, Any}`).
///
/// Asked of a list rather than of a power because two different lists answer it. A power's
/// own [`targets_affected`](Power::targets_affected) is the usual one, and
/// [`AtomicEffect::owner_targets`](crate::AtomicEffect::owner_targets) replaces it for an
/// atom a collector pulled out of another power's file — the shell's aim says nothing about
/// whom the executed power's rows land on (TARGETS-3).
///
/// An empty list names nobody, so it is not a foe: unknown is not an enemy, and the readers
/// that consult this use a foe answer to VETO.
#[must_use]
pub fn targets_name_foe<S: AsRef<str>>(targets: &[S]) -> bool {
    #[rustfmt::skip]
    const FOE: [&str; 4] = ["Foe", "DeadFoe", "DeadOrAliveFoe", "Any"];
    targets.iter().any(|t| FOE.contains(&t.as_ref()))
}

impl Power {
    pub fn from_value(value: Value) -> Result<Self, String> {
        let wire: PowerWire = serde_json::from_value(value).map_err(|e| e.to_string())?;
        Power::try_from(wire).map_err(|e| e.to_string())
    }

    /// The power's resolution identity: `internalName` if present, else `name`.
    ///
    /// Pool and epic powers carry no `internalName` on the wire; the bundle loader derives one
    /// from `fullName` before this is ever read, so the `name` fallback is reached only by a
    /// hand-constructed `Power`. That derivation matters: 48-54 pool/epic powers per fork are
    /// named one thing and identified as another (the game renamed them and kept the original
    /// internal name), and every caller addresses them by the identity, not the display name.
    pub fn ident(&self) -> &str {
        self.internal_name.as_deref().unwrap_or(&self.name)
    }

    /// `EntsAffected` — the entity categories this power's effects can land on
    /// (`["Self"]`, `["Foe"]`, `["Friend", "Self"]`, …), as the export states them.
    ///
    /// This resolves the pronoun in an atom whose [`ToWho`](crate::ToWho) is `Target`. The
    /// game writes that as `AnyAffected`, meaning "whoever this power affects" rather than
    /// naming anybody, so the identical spelling means the caster on Static Shield
    /// (`["Self"]`) and the yanked foe on Wormhole (`["Foe"]`). The power's `targetType`
    /// cannot stand in: a PBAoE that teleports foes is `targetType: "Self"` as well
    /// (Shadow Slip), and only this field says the effects land on the foes.
    ///
    /// `None` when the wire omits it, which stays distinct from an authored empty list —
    /// the converters emit nothing rather than `[]` when the export states nothing.
    pub fn targets_affected(&self) -> Option<Vec<&str>> {
        Some(
            self.extra
                .get("targetsAffected")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .collect(),
        )
    }

    /// Is `self` among this power's [`targets_affected`](Self::targets_affected) — i.e. does
    /// an atom aimed at "the target" land on the caster?
    ///
    /// `false` when the source list is absent: no reason is not a license to credit the caster,
    /// and a route that guessed the other would fabricate a total.
    pub fn affects_caster(&self) -> bool {
        self.targets_affected()
            .is_some_and(|targets| targets.contains(&"Self"))
    }

    /// Must the game have an entity in the caster's sights before this power can be used at
    /// all — so that "used, and reached nobody" is not a state it can be in?
    ///
    /// This asks `targetType`, the AIMING field, and it is the one question that field answers
    /// better than [`targets_affected`](Self::targets_affected) does. The two are different
    /// facts and the doc on `targets_affected` warns against the substitution in the other
    /// direction: a PBAoE that teleports foes is aimed at `Self` while its effects land on
    /// `Foe`. Here the aim is the point. `character_tick.c`'s queued-activation check refuses
    /// the power outright when the target entity is null or not of the required type
    /// (`PowerTargetNotAffected`, `FloatNoTarget`), and a cone additionally range-checks that
    /// same entity before it fires — so a foe-aimed power that fired had a foe in front of it.
    ///
    /// The vocabulary is declared rather than derived, and it is the whole of what the four
    /// forks' contracts carry (`target_type_vocabulary` in `per_target_floor.rs` pins that, so a
    /// fork adding a spelling reds a test instead of quietly taking the `false` arm):
    ///
    /// * needs an entity — `Foe`, `DeadFoe`, `Ally (Alive)`, `Teammate`, `Dead Teammate`,
    ///   `Own Pet (Alive)`, `Any`.
    /// * needs none — `Self` (the caster is always there) and `Location` / `Teleport` (a point
    ///   on the ground, which no entity has to occupy).
    ///
    /// `false` when the field is absent, on the same reading as [`affects_caster`](Self::affects_caster):
    /// no reason is not a licence, and the readers use a `true` here to RAISE a floor.
    #[must_use]
    pub fn aim_requires_an_entity(&self) -> bool {
        #[rustfmt::skip]
        const NEEDS_AN_ENTITY: [&str; 7] = [
            "Foe", "DeadFoe", "Ally (Alive)", "Teammate", "Dead Teammate", "Own Pet (Alive)",
            "Any",
        ];
        self.extra
            .get("targetType")
            .and_then(Value::as_str)
            .is_some_and(|aim| NEEDS_AN_ENTITY.contains(&aim))
    }

    /// Does this power affect a FOE? The converter's `TSPY_MEZ_FOE_TARGETS` vocabulary
    /// (`convert-powerset.cjs`) — the applied-control/self-buff discriminator `guardThunderspyAppliedMez`
    /// keys on. A genuine control always affects a foe (even a PBAoE nuke cast on Self —
    /// Psychic Wail, EMP Pulse — lists `Foe`), while a pure self-buff affects only
    /// `Self`/`Leaguemate`.
    ///
    /// `false` when the list is absent or empty (unknown is not a foe — the protection path
    /// keeps such powers). Mirrors `TSPY_MEZ_FOE_TARGETS = {Foe, DeadFoe, DeadOrAliveFoe, Any}`.
    pub fn affects_foe(&self) -> bool {
        self.targets_affected()
            .is_some_and(|targets| targets_name_foe(&targets))
    }

    /// The character level at which this power unlocks — the level badge the picker's
    /// numbered rows show, and the level term of the grant rule. The wire's `available` is
    /// 0-indexed (0 ⇒ available from level 1), so the character level is `available + 1`; a
    /// power whose wire omits `available` is treated as available from level 1. Sourced from
    /// the export (`extra["available"]`, still carried raw in the bag), never hardcoded per
    /// powerset.
    ///
    /// A negative `available` states no level requirement at all — `piAvailable[i] - iLevel
    /// <= 0` at every level (`powers.c:545`) — which reads back as level 1.
    pub fn unlock_level(&self) -> u8 {
        self.available()
            .filter(|available| *available >= 0)
            .map_or(1, |available| (available + 1) as u8)
    }

    /// Does the game hand this power out rather than offer it as a pick?
    ///
    /// `autoIssue` is the game's own marker. `character_GrantAutoIssuePowers`
    /// (`Common/entity/character_base.c:1952`) buys the power for the character when
    /// `piAvailable[j] <= iLevel && bAutoIssue && !OwnsPower && IsAllowedToHavePower`, and
    /// `powers_load.c:950` forces AutoIssue ⟹ Free, so what it hands over costs no pick.
    ///
    /// This is the marker ALONE. Whether the build actually holds the power adds the level
    /// and `requires` terms, which belong to the build rather than the def and live in
    /// [`crate::granted_powers`]. The marker on its own is enough to answer the picker's
    /// question — a power the game issues is never bought, at any level.
    ///
    /// Before the parser named the field, this read `available < 0` as a proxy for it. The
    /// proxy is a strict subset: no power in any fork carries a negative `available` without
    /// `autoIssue`, an invariant the granted-powers corpus pins along with the field's
    /// presence, so an export that stopped emitting it fails there rather than silently
    /// emptying every grant.
    pub fn is_auto_issued(&self) -> bool {
        self.extra
            .get("autoIssue")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The def's `Free` flag — this power costs no power pick.
    ///
    /// `character_CountPowersBought` (`character_level.c:447`) skips a free power when it
    /// tallies what the build has spent, so this is the game's own answer to the picker's
    /// question: a free power is handed over, a non-free one is bought. AutoIssue implies it
    /// (`powers_load.c:950`), so [`is_auto_issued`](Self::is_auto_issued) is a subset.
    ///
    /// Absence is `false` — the parse-table default, and the overwhelming case.
    pub fn is_free(&self) -> bool {
        self.extra
            .get("free")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// The raw 0-indexed `available` field, still carried in the bag.
    fn available(&self) -> Option<i64> {
        self.extra.get("available").and_then(Value::as_i64)
    }

    /// The pet this power summons — entity name(s) and count, lifespan, display name, the
    /// redirect powers a location pseudo-pet stands for, and the inline `resolvedEntities`
    /// payload where the entity table has no record. `None` when the power summons nothing.
    ///
    /// A converter VERDICT, not a projection of the atoms, and deliberately so. Which
    /// `EntCreate` row is the power's window (ENT-14), how repeated rows aggregate into
    /// `entities` with counts, which P-hash resolves to which sibling, which redirect list
    /// names a real entity (ENT-16), and whether a tier-gated summon rebuilds at all — every
    /// one of those is decided with the whole power in view, in `convertPower`. Re-deriving
    /// them here would be a second implementation of rules that took four register entries to
    /// settle, and the epic, pool, inherent and accolade partitions would each need a third.
    ///
    /// It reached its readers through the `effects` bag until the atom1-13 strip retired the
    /// bag out from under it and 400+ summoners per fork went dark on all four forks — no pet
    /// rows on the display, and the buff-pet aura fold at zero (ENT-22). This accessor is the
    /// one address it has now, so a future move is one edit and not a census.
    pub fn summon(&self) -> Option<&Value> {
        self.extra.get("summon")
    }

    /// The converter's set-mechanic classification (`extra["mechanicType"]`) — how this
    /// power exists outside the ordinary pick economy, when it does.
    ///
    /// The verdict derives from the def's own flags (`ShowInManage kFalse` + `MaxBoosts 0`
    /// and friends — SHOWFLAGS-1), which is what separates a set-mechanic grant the game
    /// hands out and takes back (Seismic Shockwaves, Bio Armor's Adaptation, Staff
    /// Mastery) from a real auto-power pick like Mental Training, which keeps
    /// `ShowInManage` true and stays in the pick list.
    ///
    /// `None` when the wire omits the field: an ordinary power, the overwhelming case.
    /// An unrecognized value is an `Err`, never a silent `None` — a new converter
    /// vocabulary word must surface at the call site rather than quietly re-entering the
    /// pick list; the picker turns the `Err` into a visible
    /// [`Unreadable`](crate::pick_rules) row rather than a guess.
    pub fn mechanic_type(&self) -> Result<Option<MechanicType>, String> {
        let Some(value) = self.extra.get("mechanicType") else {
            return Ok(None);
        };
        let Some(name) = value.as_str() else {
            return Err(format!(
                "power {:?}: mechanicType is not a string: {value}",
                self.ident()
            ));
        };
        match name {
            "childToggle" => Ok(Some(MechanicType::ChildToggle)),
            "parentMechanic" => Ok(Some(MechanicType::ParentMechanic)),
            "hiddenPassive" => Ok(Some(MechanicType::HiddenPassive)),
            "hiddenAuto" => Ok(Some(MechanicType::HiddenAuto)),
            other => Err(format!(
                "power {:?}: unrecognized mechanicType {other:?}",
                self.ident()
            )),
        }
    }

    /// How many enhancement slots this power accepts, per the export's own `maxSlots` — the
    /// per-power ceiling, distinct from the build-wide budget the schedule sets.
    ///
    /// Most powers say 6, but not all: several say 4, and the powers the game grants without
    /// slotting say 0. `None` is the wire omitting the field (the pseudo-pet and temp powers,
    /// none of them pickable), which the caller reads as "no stated ceiling" rather than as
    /// zero — absent is not the same as none.
    ///
    /// A power that accepts no enhancement CATEGORY takes no slots either, whatever this
    /// says: [`accepts_enhancements`](Self::accepts_enhancements) is the other half of the
    /// answer, and it is the one that covers the archetype inherents, whose `maxSlots` the
    /// wire omits but whose `allowedEnhancements` is an explicit empty list.
    pub fn max_slots(&self) -> Option<u8> {
        self.extra
            .get("maxSlots")
            .and_then(Value::as_u64)
            .map(|slots| slots as u8)
    }

    /// Can anything be slotted into this power at all?
    ///
    /// `false` only for an EXPLICIT empty `allowedEnhancements` — the export's way of saying
    /// "accepts no enhancement category", which is how the archetype inherents are marked.
    /// An ABSENT list means the opposite (no stated restriction), the same distinction the
    /// Alpha-aspect gate already preserves.
    pub fn accepts_enhancements(&self) -> bool {
        self.allowed_enhancements
            .as_ref()
            .is_none_or(|allowed| !allowed.is_empty())
    }

    /// The power's damage-type SET for display (chips) — sorted, distinct.
    ///
    /// A power-level classification, NOT per-atom: HC/Rebirth atoms carry the element
    /// (`Damage` + `SubType`), so this derives from them directly; Thunderspy damage
    /// atoms are `Unmapped` (per-template element is absent from that binary), so it
    /// falls back to the converter-supplied `damageTypes` field the same way appliers
    /// fall back `atom ?? bag`. `Special`/`Heal` and `base_probability == 0` riders
    /// (the Fiery Embrace chance-0 rider, METHOD-1) are excluded so they never phantom
    /// a type onto an attack that doesn't deal it.
    ///
    /// A malformed `damageTypes` field (non-array, non-string entry, unrecognized type
    /// name) is an error, never silently dropped — malformed ≠ absent; the UI turns
    /// the `Err` into a visible marker for this power.
    pub fn damage_types(&self) -> Result<Vec<SubType>, String> {
        let mut from_atoms: Vec<SubType> = self
            .atoms
            .iter()
            .filter(|a| {
                a.effect_type == Some(EffectType::Damage)
                    && !a.is_gated()
                    && a.base_probability != Some(0.0)
            })
            .filter_map(|a| a.sub_type)
            .filter(|s| !matches!(s, SubType::Special))
            .collect();
        from_atoms.sort_by_key(|s| s.as_wire());
        from_atoms.dedup();
        if !from_atoms.is_empty() {
            return Ok(from_atoms);
        }
        let Some(field) = self.extra.get("damageTypes") else {
            return Ok(Vec::new());
        };
        let arr = field
            .as_array()
            .ok_or_else(|| format!("power {:?}: damageTypes is not an array", self.ident()))?;
        let mut set = Vec::with_capacity(arr.len());
        for entry in arr {
            let s = entry.as_str().ok_or_else(|| {
                format!(
                    "power {:?}: damageTypes entry {entry} is not a string",
                    self.ident()
                )
            })?;
            let sub_type = s
                .parse()
                .map_err(|_| format!("power {:?}: unrecognized damage type {s:?}", self.ident()))?;
            set.push(sub_type);
        }
        set.sort_by_key(|s: &SubType| s.as_wire());
        set.dedup();
        Ok(set)
    }
}

impl Power {
    /// This power as a caster of `class_name` actually has it — its archetype-forked atoms
    /// narrowed to that archetype's arm.
    ///
    /// Borrowed (free) for the overwhelming majority of powers, which carry no fork at all.
    /// Owned only where one does: Rebirth's Tough, Weave and Combat Jumping each ship a
    /// Kheldian arm and an everyone-else arm, and a build gets exactly one
    /// ([`crate::AtomicEffect::caster_archetypes`], DATA-GAP-REGISTER AT-FORK-1).
    ///
    /// `None` — a build with no archetype chosen, or one the dataset states no class token
    /// for — keeps every atom: the widest reading, chosen so this primitive never silently
    /// drops data. The gather is where that case reaches the error channel (Rule 1).
    ///
    /// This is the primitive every consumer that reads `power.atoms` for ONE build goes
    /// through. Filtering the gathered atom stream alone would not do it: the apply pass
    /// iterates powers and re-reads `power.atoms` per family, so the narrowing has to live
    /// on the power the appliers are handed.
    pub fn for_caster_class<'a>(&'a self, class_name: Option<&str>) -> std::borrow::Cow<'a, Power> {
        let Some(class_name) = class_name else {
            return std::borrow::Cow::Borrowed(self);
        };
        if self.atoms.iter().all(|a| a.applies_to_class(class_name)) {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut narrowed = self.clone();
        narrowed.atoms.retain(|a| a.applies_to_class(class_name));
        std::borrow::Cow::Owned(narrowed)
    }

    /// This power as the single-valued `effects` BAG sees it — every archetype-forked atom
    /// dropped, every other one kept.
    ///
    /// The bag holds one value per slot with nowhere to record "for a Scrapper only", so the
    /// converter keeps a fork out of it entirely and lets the atom stream carry it instead
    /// (AT-FORK-1). The atom↔bag parity guards need this view to compare like with like:
    /// without it Rebirth's Acrobatics reads as a phantom, an atom value the bag "lost",
    /// when in truth neither side is wrong and only one of them can express a fork.
    pub fn as_bag_view(&self) -> std::borrow::Cow<'_, Power> {
        if self.atoms.iter().all(|a| a.caster_archetypes.is_none()) {
            return std::borrow::Cow::Borrowed(self);
        }
        let mut narrowed = self.clone();
        narrowed.atoms.retain(|a| a.caster_archetypes.is_none());
        std::borrow::Cow::Owned(narrowed)
    }

    /// The value every archetype agrees on, reading this power through `read` once per class
    /// with the fork resolved the way [`Self::for_caster_class`] resolves it for a build.
    ///
    /// `None` when the power carries no fork at all, when `class_names` is empty, or when any
    /// two archetypes disagree — the three cases where there is no single answer to state.
    ///
    /// This is the atom-side twin of the converter's `_addUnanimousForkedSlots`. A fork often
    /// changes nothing a bag slot can see: Rebirth Combat Jumping forks only to hang a hover
    /// clause on the Kheldian arm, and both arms buff defense .25. The converter holds the
    /// archetype roster, so it resolves that and states the slot; [`Self::as_bag_view`] cannot,
    /// so it abstains. Neither is wrong, but the atom↔bag parity guards need a way to tell
    /// "the bag resolved a fork I can't see" from "the bag has a value I lost" — before this
    /// existed both sides fell silent on a fork and the guards scored that as a match.
    pub fn unanimous_across<T: PartialEq>(
        &self,
        class_names: &[String],
        read: impl Fn(&Power) -> Option<T>,
    ) -> Option<T> {
        if self.atoms.iter().all(|a| a.caster_archetypes.is_none()) {
            return None;
        }
        let mut agreed: Option<T> = None;
        for class_name in class_names {
            let value = read(&self.for_caster_class(Some(class_name)))?;
            match &agreed {
                Some(seen) if *seen != value => return None,
                Some(_) => {}
                None => agreed = Some(value),
            }
        }
        agreed
    }
}

#[derive(Debug, Clone)]
pub struct Powerset {
    pub id: String,
    /// The set's own name in the binary, fully qualified (`"Brute_Defense.Bio_Organic_Armor"`).
    ///
    /// [`id`](Self::id) slugs the DISPLAY name, and six sets across the three forks spell the
    /// two differently — Bio Armor is `Bio_Organic_Armor`, Presence is `Manipulation`, Spines
    /// is `Quills`. A `requires` set path is written the binary way, so this is what
    /// [`crate::pick_rules`] matches one against; `None` means no gate can name this set.
    pub set_path: Option<String>,
    /// `SetBuyRequires` + `SetBuyRequiresFailedText` — the set-level gate and the message the
    /// game shows when it refuses. Evaluated by [`crate::pick_rules::set_gate`].
    pub buy_requires: Vec<String>,
    pub buy_requires_failed: String,
    /// `SpecializeAt` + `SpecializeRequires` — the level a set branches at and the gate on
    /// taking THIS branch.
    ///
    /// Zero is not a level: it is what marks a set as NOT a specialization set at all
    /// (`baseset_IsSpecialization`). A non-zero value is on the raw 0-based `iLevel` scale,
    /// like [`Power::unlock_level`]'s `available` and unlike the expression language's
    /// `level`, so the character level is one higher — the VEAT branches read 23 here and
    /// unlock at 24. [`crate::pick_rules::set_gate`] does that conversion; nothing else
    /// should re-derive it.
    pub specialize_at: u8,
    pub specialize_requires: Vec<String>,
    pub name: String,
    pub archetype: Option<String>,
    pub category: Option<String>,
    pub powers: Vec<Power>,
    pub extra: Map<String, Value>,
}

#[derive(Debug, Deserialize)]
struct PowersetWire {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "setPath")]
    set_path: Option<String>,
    #[serde(default, rename = "buyRequires")]
    buy_requires: Vec<String>,
    #[serde(default, rename = "buyRequiresFailed")]
    buy_requires_failed: String,
    #[serde(default, rename = "specializeAt")]
    specialize_at: u8,
    #[serde(default, rename = "specializeRequires")]
    specialize_requires: Vec<String>,
    name: String,
    archetype: Option<String>,
    category: Option<String>,
    #[serde(default)]
    powers: Vec<Value>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl Powerset {
    pub fn from_value(registry_key: &str, value: Value) -> Result<Self, String> {
        let wire: PowersetWire = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let powers = wire
            .powers
            .into_iter()
            .map(Power::from_value)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Powerset {
            id: wire.id.unwrap_or_else(|| registry_key.to_string()),
            set_path: wire.set_path,
            buy_requires: wire.buy_requires,
            buy_requires_failed: wire.buy_requires_failed,
            specialize_at: wire.specialize_at,
            specialize_requires: wire.specialize_requires,
            name: wire.name,
            archetype: wire.archetype,
            category: wire.category,
            powers,
            extra: wire.extra,
        })
    }
}
