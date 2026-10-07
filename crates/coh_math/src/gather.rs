//! Pass 0 (GATHER): the active, mode-resolved atom stream of a build.
//!
//! Two layers:
//!
//! * [`gather_base_atoms`], the M2 primitive: the unconditional base atoms of an
//!   already-resolved list of `&Power`, in power order then atom order (drop
//!   `gated == Some(true)`; chance-0 riders arrive pre-classified by the converter,
//!   see METHOD-1 in the gap register).
//! * [`gather_active_atoms`], the M3 full gather (`character-totals.ts` Step 4,
//!   `collectAllPowers` @3750 + `computeModeSuppression`). Resolves each selected
//!   power's [`Power`] def from the [`PowerDatabase`], keeps the *contributing* ones
//!   (auto powers, or toggles/clicks the user switched on), expands the active stance
//!   sub-powers, drops the mode-suppressed ones (Granite → the other Stone toggles),
//!   and skips `not_on_caster` atoms (pet/foe traps that never reach the caster's own
//!   totals). The result is the atom stream Passes 1–8 consume; per-atom self-directed
//!   routing and the active/toggle *nuances* (e.g. endurance cost counts toggles only)
//!   live in the apply pass.
//!
//! Skeptical-borrowing notes vs the beta `collectAllPowers`:
//! * Our `active_sub_power` stores the sub-power's `internalName` directly
//!   (identity/def split), so stance expansion needs no `STANCE_GROUPS` option table.
//!   The sub-power is resolved from the parent's set like any other def.
//! * `internalName` is NOT globally unique (`Build_Up` appears 64× across ATs), so EVERY
//!   partition is resolved within its named set: powersets by their `Powerset`, pools and
//!   epics by the aggregate id each power is tagged with (`coh_data::PartitionPower`). The flat
//!   scan survives only as a last-resort fallback for a selection whose set id no aggregate owns.
//!   It used to be the only pool/epic path, on the assumption that identities there don't
//!   collide; they do. 85-97 internal names per fork are published by more than one epic mastery
//!   at their own scales, so the unscoped scan answered with another archetype's copy.
//! * The beta's `collectAllPowers` SKIPS `fitness`/`archetype` inherents because it silos them
//!   into dedicated passes. The rebuild splits the difference BY MECHANIC (roadmap decision 4):
//!   fitness (Health/Stamina) is fully atomized (`Regeneration` / `Recovery` atoms), so gather
//!   keeps it and its buffs flow through the apply loop like any power. No double-count, because
//!   there's no parallel fitness pass. The ARCHETYPE inherents (Vigilance/Fury) are instead
//!   handled by the combat-context-driven Pass 3 ([`crate::inherents`]): their damage depends on
//!   team size / a meter and their atoms are gated, which the general apply loop can't resolve, so
//!   gather SKIPS them (`inherent_category == Archetype`). This skip became load-bearing once
//!   `convert-inherents.cjs` landed the archetype-inherent atoms in the contract (before that there
//!   was no def to resolve); it keeps Pass 3 their sole home. ([DATA-GAP-REGISTER] INHERENT-2.)

use crate::expr::{eval_bool, SourceContext};
use crate::totals::CalcError;
use coh_data::{
    caster_class_name, reaches_caster, AtomicEffect, CharacterState, EffectType, Enhancement,
    InherentCategory, Power, PowerDatabase,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// One active atom with its owning power, the unit Passes 1–8 consume.
#[derive(Debug, Clone, Copy)]
pub struct GatheredAtom<'a> {
    pub power: &'a Power,
    pub atom: &'a AtomicEffect,
}

/// The base (non-gated) atoms of the given powers, power order then atom order.
/// The primitive [`gather_active_atoms`] calls once the active set is resolved.
pub fn gather_base_atoms<'a>(powers: impl IntoIterator<Item = &'a Power>) -> Vec<GatheredAtom<'a>> {
    let mut out = Vec::new();
    for power in powers {
        for atom in &power.atoms {
            if atom.gated != Some(true) {
                out.push(GatheredAtom { power, atom });
            }
        }
    }
    out
}

/// The gather result: the contributing powers plus every selection that named a power
/// the dataset couldn't resolve. An unresolvable pick must reach the totals' error
/// channel. A selection silently contributing nothing is a soft-wrong total (Rule 1).
#[derive(Debug)]
pub struct GatheredPowers<'a> {
    pub powers: Vec<ActivePower<'a>>,
    pub unresolved: Vec<CalcError>,
    /// The build's active source modes/stances, for `<mode> source.Mode?` gates. Feeds
    /// [`active_conditional_powers`], which re-admits the mode-gated atoms this build satisfies.
    pub source_modes: HashSet<String>,
}

/// Full Pass 0 gather: resolve the contributing, mode-resolved atom stream of `state`
/// against `db`. Power order then atom order (the M2 invariant holds); `not_on_caster`
/// atoms are dropped (caster totals only). The second element carries the selections
/// that failed to resolve, for the caller's error channel.
pub fn gather_active_atoms<'a>(
    state: &'a CharacterState,
    db: &'a PowerDatabase,
) -> (Vec<GatheredAtom<'a>>, Vec<CalcError>) {
    let gathered = gather_active_powers(state, db);
    let mut unresolved = gathered.unresolved;
    let caster_class = caster_class_name(state, db);
    let atoms = gather_base_atoms(gathered.powers.iter().map(|a| a.def).collect::<Vec<_>>())
        .into_iter()
        .filter(|g| g.atom.not_on_caster != Some(true))
        .filter(|g| keep_for_archetype(g, caster_class, &mut unresolved))
        .collect();
    (atoms, unresolved)
}

/// Does this base atom belong to THIS build's archetype?
///
/// Most atoms carry no fork and are kept unconditionally. The ones that do are the two-armed
/// pool defences: Rebirth's Tough, Weave and Combat Jumping each ship a Kheldian arm and an
/// everyone-else arm. The arms partition the roster, so exactly one may survive: applying both
/// doubles the power, 3.0 S/L resistance on Tough where Homecoming and Thunderspy read 1.5,
/// which is the cross-fork oracle the gate uses (DATA-GAP-REGISTER AT-FORK-1).
///
/// A forked atom on a build whose class token the dataset doesn't state is neither kept
/// silently nor dropped silently: it reaches the error channel, because either answer would
/// be a number nothing in the data supports (Rule 1).
fn keep_for_archetype(
    gathered: &GatheredAtom,
    caster_class: Option<&str>,
    unresolved: &mut Vec<CalcError>,
) -> bool {
    let Some(fork) = gathered.atom.caster_archetypes.as_deref() else {
        return true;
    };
    let Some(class_name) = caster_class else {
        unresolved.push(CalcError::new(
            gathered.power.ident().to_string(),
            format!(
                "effect applies only to {fork}, and this build's archetype has no class name \
                 in the dataset — cannot tell whether it belongs to these totals"
            ),
        ));
        return false;
    };
    gathered.atom.applies_to_class(class_name)
}

/// The contributing, mode-resolved power list of `state`, the beta's post-suppression
/// `allPowers` (`character-totals.ts` Step 4.1). [`gather_active_atoms`] flattens this to
/// atoms; the per-power passes (Pass 1 strength onward) iterate the powers directly,
/// because their aggregation is per-power (a power's `specialBuff` max, its toggle
/// endurance cost) rather than per-atom.
pub fn gather_active_powers<'a>(
    state: &'a CharacterState,
    db: &'a PowerDatabase,
) -> GatheredPowers<'a> {
    let (powers, unresolved) = resolve_active_powers(state, db);
    // Collect modes BEFORE suppression: a suspended toggle is still running and still sets its
    // modes (mirrors `drop_mode_suppressed`'s pre-suppression `live` set).
    let source_modes = collect_source_modes(&powers);
    GatheredPowers {
        powers: drop_mode_suppressed(powers),
        unresolved,
        source_modes,
    }
}

/// The active source modes/stances for `<mode> source.Mode?` gates, collected from every active
/// power's `setsModes`. The gate tokens are `k`-prefixed (`kDefensiveAdaptation`) while `setsModes`
/// publishes the bare mode (`DefensiveAdaptation`), so both spellings are inserted. A build in
/// Defensive Adaptation must satisfy `kDefensiveAdaptation Source.Mode?`. (Efficient Adaptation sets
/// the legacy `RestedAdaptation`, matching its own `kRestedAdaptation` gate, the name reconciliation
/// the export already encodes; deriving the mode from the sub-power NAME would misfire there.)
fn collect_source_modes(powers: &[ActivePower]) -> HashSet<String> {
    let mut out = HashSet::new();
    for p in powers {
        for m in modes(p.def, "setsModes") {
            out.insert(m.to_string());
            out.insert(format!("k{m}"));
        }
    }
    out
}

/// The two combat-engagement modes, for `kEngaged Source.Mode?` and `kOutOfCombat Source.Mode?`.
/// No power's `setsModes` publishes either (they're combat engagement, not stances), so both are
/// bound from [`crate::CombatContext::in_combat`] rather than collected: the
/// [[attribmod-flag-bits]]-style runtime-state binding the export's own gate names, exactly as Fury
/// binds `kRage` from the meter. Both spellings of each are listed for the same reason
/// [`collect_source_modes`] emits both: the gate token is `k`-prefixed while a bare mode may appear
/// elsewhere.
///
/// The export states that they're exact complements, so one flag drives both. The auto-issued
/// `Engagement` inherent carries two `Set_Mode` templates under complementary gates: `Engaged`
/// while `Attacked … 8 <` or `AttackedByOther … 8 <`, `OutOfCombat` while both are `8 >=`.
const ENGAGED_MODES: [&str; 2] = ["kEngaged", "Engaged"];
const OUT_OF_COMBAT_MODES: [&str; 2] = ["kOutOfCombat", "OutOfCombat"];

/// Every mode a build satisfies right now: the stances its powers publish, plus the
/// combat-state pair [`ENGAGED_MODES`]/[`OUT_OF_COMBAT_MODES`] binds. Shared with the per-power
/// damage projection and the effective-power form gate, so a mode-gated damage atom, a mode-gated
/// resistance atom and a snipe's fast-form gate all answer to the same build state rather than to
/// lists that can drift apart.
pub fn live_modes(source_modes: &HashSet<String>, in_combat: bool) -> HashSet<String> {
    let mut modes = source_modes.clone();
    let engagement = if in_combat {
        ENGAGED_MODES
    } else {
        OUT_OF_COMBAT_MODES
    };
    modes.extend(engagement.iter().map(|m| m.to_string()));
    modes
}

/// The export attrib a meter-publishing atom carries, as
/// [`coh_data::AtomicEffect::meta_attrib`] spells it (lower-cased at ingest).
const METER_ATTRIB: &str = "meter";

/// Does this build's own powers publish the HIDE meter, the state
/// [`crate::CombatContext::hidden`] declares, and the one `kMeter source>` names?
///
/// `kMeter` is a SINGLE character attribute that ten mechanics drive: Hide, Placate,
/// Domination, Defiance, Opportunity, Fury/Rage, Primal Energy, Battle Euphoria and Pack
/// Mentality all publish `meter`. The game reuses one slot because a character has exactly
/// one meter mechanic, its archetype's, so what the meter MEANS is a fact about the build,
/// and `hidden` may answer it only for a build whose meter is the hide meter. A Beast
/// Mastery Mastermind's isn't: its `Cur.kMeter source> .1 * .05 + @Strength *` reads the
/// meter as a QUANTITY, and handing that program a value meaning "hidden" would fabricate
/// a number where it honestly refuses today.
///
/// The discriminator is what MAKES a meter the hide meter: attacking drops it. `Hide`
/// and the two Mask Presences publish their meter under a combat-suppression window
/// (`Attacked`/`Damaged` at 8–10s) and nothing else does (18 / 13 / 3 atoms over three
/// powers per fork). Domination, Defiance, Opportunity, Fury and Pack Mentality build
/// their meters up in combat, so none is suppressed. That window and
/// [`crate::CombatContext::in_combat`] encode the same fact, which is the convention
/// [`live_modes`] already follows for `kOutOfCombat`.
///
/// The attrib is load-bearing here. Shape alone (a `Self`-targeted, combat-suppressible
/// [`EffectType::Meta`] atom) matches 8 / 3 / 4 powers where this rule matches 3 / 3 / 3,
/// and the over-match isn't harmless: it takes in the Teleport family through
/// `designer_status` (Shadow Step, Teleport and the two Dwarf Steps; so any build holding
/// the Teleportation pool, and every Kheldian, would read as publishing a hide meter) plus
/// Shadow Cloak, and on Thunderspy it takes in `Primalists_Cloak` through `set_mode`. The
/// Primalist is the exact archetype whose scalar `cur.kMeter` programs this scoping exists
/// to protect. Rebirth over-matches nothing, which is why the gate asserts the widening
/// SOMEWHERE rather than per fork.
///
/// Reads the build's OWN powers rather than the archetype's name, so the Fortunata and
/// Night Widow branches that reach the meter through `Teamwork` answer yes without
/// being named, and a Stalker who somehow hasn't got Hide answers no. The qualifying
/// set is `Hide`, `FRT_Mask_Presence` and `NW_Mask_Presence`, identically on all three
/// forks (Homecoming lower-cases the two prefixes), pinned by
/// a gate that also holds the over-match, so dropping the attrib test reds with its
/// own diagnosis.
///
/// `Cloaking_Device` is NOT a publisher on any fork (it carries no `Meter` atom at all),
/// and Homecoming's plain `Mask_Presence` isn't one either. Both were named here before
/// the gate existed to check them.
pub fn publishes_hide_meter(state: &CharacterState, db: &PowerDatabase) -> bool {
    state.all_selected().any(|selection| {
        // `resolve_power`, not `PowerDatabase::find_power`: the latter searches `powersets`
        // only, so a pool, epic or inherent pick resolved to `None` and answered "publishes
        // nothing" without saying so. Harmless on today's corpus (the three publishers are all
        // powerset powers) and exactly the shape that outlives the corpus that made it safe.
        resolve_power(db, &selection.powerset, &selection.internal_name).is_some_and(|power| {
            power.atoms.iter().any(|atom| {
                atom.effect_type == Some(EffectType::Meta)
                    && atom.meta_attrib.as_deref() == Some(METER_ATTRIB)
                    && reaches_caster(atom, power)
                    && atom.suppressible == Some(true)
            })
        })
    })
}

/// Every power path this dataset's effect gates ask the CASTER about, as spelled by the gate.
///
/// The corpus is what makes [`owned_powers`] possible. A build can't enumerate its own
/// powers as dotted paths: the contract keeps a powerset's id (`sentinel/super-reflexes`) and
/// drops the raw category the gates spell (`Sentinel_Defense.Super_Reflexes.Master_Brawler`), so
/// there's no way to synthesize the key a gate will look up. Going the other way works: collect
/// the paths the gates DO ask about, then ask the build about each. The corpus is small, 73
/// distinct paths across the three forks, and every entry is answered by one structural
/// predicate.
/// Public so a gate can assert its COVERAGE directly. Nothing else calls it, and it isn't private
/// because its blind spots are invisible from behaviour: a path this walk misses is never
/// owned, which reads as a build that doesn't hold the power. `formVariants` is the case: three
/// Homecoming paths (the Crab and Bane armours, Water Jet's lockout) are named by no other
/// carrier, so omitting that key here would leave them permanently unowned with every projection
/// still returning a plausible number.
pub fn caster_owned_path_corpus(db: &PowerDatabase) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut collect = |expression: Option<&[Box<str>]>| {
        for path in crate::expr::caster_owned_paths(expression.unwrap_or_default()) {
            out.insert(path.to_string());
        }
    };
    for power in db.all_powers() {
        for atom in &power.atoms {
            collect(atom.requires_expression.as_deref());
        }
        // The form conditions too, all three mechanisms: a form selected by an ownership gate is
        // a different attack, so a missed path there swaps the wrong atom list rather than
        // dropping a rider (SNIPE-3, MODEVAR-1, CHAIN-1). `formVariants` is where most of them
        // are: it exists for the tables the other two detectors can't read, and omitting it here
        // would leave every one of those branches unselectable by any build while the bundle
        // carried the form.
        for key in ["quickSnipe", "modeVariants", "formVariants"] {
            let Some(node) = power.extra.get(key) else {
                continue;
            };
            let conditions: Vec<&Value> = match node {
                Value::Object(map) if key == "modeVariants" => map.values().collect(),
                Value::Array(list) => list.iter().collect(),
                single => vec![single],
            };
            for condition in conditions {
                collect(crate::expr::json_tokens(condition.get("condition")).as_deref());
            }
        }
    }
    out
}

/// What the build owns, keyed as [`SourceContext::owned_powers`] expects.
///
/// Every caster-ownership gate in the dataset resolved against this build's picks. Before this
/// existed the map was left empty, which the evaluator reads as a definite "not owned", so every
/// such gate answered false regardless of the build, and the effects behind them (Cross Punch's
/// bonus damage for holding Boxing or Kick, Brimstone Armor's rider, the Stalker ATO proc's
/// Build Up clause) were unreachable rather than merely off.
///
/// Two producers answer it, and neither can answer the other's half. The PICKS come first: a
/// path naming a power the build selected is owned because it's held. Transient combat state
/// (a granted charge, a combo-meter stack, a lockout) isn't a pick and no walk of the build's
/// selections can find it; the layer that knows is the conditional toggles, which
/// [`coh_data::caster_state::active_ownership_claims`] reads. A build that's declared three
/// tidal stacks owns three copies of `Tidal_Power`, and Water Jet's enhanced form becomes
/// selectable; a build that's declared nothing still answers a definite "not owned", which is
/// the honest default rather than a fabricated combat state.
///
/// The two compose by MAX, never by overwrite. The halves overlap on the paths that are both a
/// real pick and a declarable state (Sentinel Super Reflexes' `Master_Brawler` is one), and a
/// toggle left off must not unsay the pick that holds the power.
pub fn owned_powers(state: &CharacterState, db: &PowerDatabase) -> HashMap<String, f64> {
    let mut owned: HashMap<String, f64> = caster_owned_path_corpus(db)
        .into_iter()
        .filter_map(|path| {
            let count = coh_data::pick_rules::owned_power_count(state, &db.set_paths, &path);
            (count > 0.0).then(|| (crate::expr::normalize_power_path(&path), count))
        })
        .collect();
    for (path, count) in coh_data::caster_state::active_ownership_claims(state, db) {
        owned
            .entry(crate::expr::normalize_power_path(&path))
            .and_modify(|held| *held = held.max(count))
            .or_insert(count);
    }
    owned
}

/// The mode-gated conditional contributions active for this build's source modes, as slot-less
/// synthetic powers, the rebuild's analog of the beta `expandActiveConditionals`.
///
/// A `gated` atom carrying a `requires_expression` is a conditional the base gather drops; it belongs
/// to the totals only while its gate is satisfied (Bio Armor's Hardened Carapace grants extra
/// resistance ONLY in Defensive Adaptation). Here each such atom whose gate evaluates a DEFINITE true
/// against the build's `source_modes` is re-admitted, cloned with `gated` cleared into a synthetic
/// power per originating power, which the apply loop then treats as a base atom of a slot-less power.
///
/// `owned` is the build's caster-ownership answer ([`owned_powers`]): a gate here reads a mode
/// OR a held power (`Pool.Fighting.Kick source.ownPower?` is Cross Punch's bonus-damage clause),
/// and those atoms are `gated`, so this is the pass that re-admits them.
///
/// The re-admission is purely ADDITIVE: an Indeterminate or false gate is left dropped (matching
/// the [`crate::inherents`] `Ok(true)` convention), so no build without an active exotic mode
/// changes. It's also UNENHANCED: every mode-gated atom is `ignoreStrength`, and running the
/// synthetic through the apply loop with no slots, empty strength, and an inactive Alpha reproduces
/// that exactly. The synthetic drops the `effects` bag so a family absent from its atoms never falls
/// back to the base power's bag and double-counts (the atom `?? bag` seam). `not_on_caster` atoms are
/// excluded, as in [`gather_active_atoms`] (caster totals only).
///
/// Besides the powers' own `setsModes` stances, the caller's combat engagement binds one more mode:
/// `kOutOfCombat` is active exactly while `!in_combat` (see [`OUT_OF_COMBAT_MODES`]). It's the same
/// out-of-combat signal the suppressible-defense path already reads (`apply.rs`), so a build's
/// out-of-combat defense (Mask Presence, via `suppressible`) and its out-of-combat damage (Targeting
/// Drone's snipe buff, gated `kOutOfCombat Source.Mode?`, which has no non-gated copy and is otherwise
/// dropped) turn on together. One combat state, not two independent toggles. That is a deliberate
/// data-faithful choice over the beta, which drives the snipe buff from a separate default-off
/// `outofcombat` adjuster; both gates encode "out of combat," so the engine keys both on `in_combat`.
pub fn active_conditional_powers(
    powers: &[ActivePower],
    source_modes: &HashSet<String>,
    in_combat: bool,
    target_entity: &str,
    owned: &HashMap<String, f64>,
) -> Vec<(String, Power)> {
    let ctx = SourceContext {
        source_modes: live_modes(source_modes, in_combat),
        owned_powers: owned.clone(),
        // The PvE/PvP side the build declares, and nothing more about the target: a finisher's
        // self-buff forks on `enttype target>` (Sky Splitter's Perfection of Body resistance is
        // `enttype target> critter eq … source.ownPower? &&`), and without the side that whole
        // gate stayed Indeterminate, so the buff never reached the totals on a fork that writes
        // the PvE copy that way. The rank stays unstated — a self-totals pass has no one target,
        // and a gate that asks `arch target>` is still left out.
        target: Some(crate::expr::TargetIdentity {
            archetype_class: None,
            entity_type: target_entity.to_string(),
        }),
        ..Default::default()
    };
    let mut out = Vec::new();
    for ap in powers {
        let active_atoms: Vec<AtomicEffect> = ap
            .def
            .atoms
            .iter()
            .filter(|a| a.gated == Some(true) && a.not_on_caster != Some(true))
            .filter_map(|a| {
                let expr = a.requires_expression.as_deref().filter(|e| !e.is_empty())?;
                matches!(eval_bool(expr, &ctx), Ok(true)).then(|| {
                    let mut cleared = a.clone();
                    cleared.gated = None;
                    cleared
                })
            })
            .collect();
        if active_atoms.is_empty() {
            continue;
        }
        // `ap.def` is the power as the DATABASE holds it, and no fork has carried a
        // power-level `effects` bag since 2026-09-03, so the strip that stood here removed a key
        // that was never present.
        let extra = ap.def.extra.clone();
        // The parent's set travels with the synthetic power: these atoms are that armour's own,
        // re-admitted for the active mode, and a breakdown row has to name the power the player
        // can see in their build.
        out.push((
            ap.power_set.to_string(),
            Power {
                name: ap.def.name.clone(),
                internal_name: ap.def.internal_name.clone(),
                atoms: active_atoms,
                allowed_enhancements: ap.def.allowed_enhancements.clone(),
                allowed_set_categories: ap.def.allowed_set_categories.clone(),
                quick_snipe_atoms: ap.def.quick_snipe_atoms.clone(),
                mode_variant_atoms: ap.def.mode_variant_atoms.clone(),
                form_variant_atoms: ap.def.form_variant_atoms.clone(),
                extra,
            },
        ));
    }
    out
}

/// An active power's definition paired with the per-power calc inputs its SELECTION carries.
///
/// The def alone isn't enough once a pass needs a user input that lives on the pick rather
/// than the dataset. Pairing them here keeps that association intact instead of re-deriving it
/// from a name map. The beta's `targetsHitValues` is keyed by `internalName`, which is NOT
/// unique (Build_Up appears ×64 across archetypes), so it can't address one specific pick.
/// [[skeptical-borrowing]]
/// What kind of contributor supplied a bonus, the grouping the detailed-totals breakdown reads.
/// Only the kinds addressed as a POWER (an internal name in a powerset); set bonuses, procs and
/// the incarnate loadout carry their own provenance from their own passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
pub enum PowerSourceKind {
    /// A picked power the build has switched on (or an always-on auto), and the mode-gated
    /// stance effects that ride one; those are the same power's contribution, not a source of
    /// their own.
    ActivePower,
    /// An earned accolade. An ordinary auto-on Self power by the time it reaches the apply pass,
    /// so only the gather knows.
    Accolade,
    /// An archetype inherent (Vigilance, Fury). Never gathered: the gather skips these so their
    /// atoms cannot double-count, and Pass 3 ([`crate::inherents`]) is their sole applier. But
    /// they file the same per-power rows, so their source group is named here with the rest.
    Inherent,
}

#[derive(Debug, Clone, Copy)]
pub struct ActivePower<'a> {
    /// The dataset-owned definition, resolved from the `PowerDatabase`.
    pub def: &'a Power,
    /// Which kind of contributor this is, for the per-source breakdown. Declared at each gather
    /// site rather than inferred downstream: by the time the apply pass sees the list, an
    /// accolade and a toggle are both just auto-on Self powers, and nothing in the def tells
    /// them apart.
    pub kind: PowerSourceKind,
    /// The owning set's id. Carried because `internal_name` is NOT unique (`Build_Up` appears
    /// ×64 across archetypes), so the pair is the only thing that addresses one specific
    /// contributor in a breakdown row.
    pub power_set: &'a str,
    /// Targets hit / stacks active for this pick (`coh_math::stacking`). `None` = no input.
    /// An expanded stance sub-power inherits its parent selection's value: it's the same
    /// pick, and the beta likewise keys the parent power's slider.
    pub targets_hit: Option<u32>,
    /// This pick's positional enhancement slots (from the `SelectedPower`, not the def),
    /// the per-power input the apply loop's enhancement aggregation
    /// ([`crate::enhancement::calculate_power_enhancement_bonuses`]) reads. Empty when the
    /// power carries no slots. An expanded stance sub-power inherits its parent selection's
    /// slots (same pick), exactly as `targets_hit` does.
    pub slots: &'a [Option<Enhancement>],
}

fn power_type(p: &Power) -> Option<&str> {
    p.extra.get("powerType").and_then(Value::as_str)
}

/// Auto powers are always contributing; everything else needs an explicit toggle
/// (`character-totals.ts:1082`, and `mode-suppression.ts` `isActivePower`).
fn is_auto(p: &Power) -> bool {
    power_type(p).is_some_and(|t| t.eq_ignore_ascii_case("auto"))
}

/// The `setsModes` / `modesSuspended` string arrays a power carries in its `extra` bag.
fn modes<'a>(p: &'a Power, key: &str) -> impl Iterator<Item = &'a str> {
    p.extra
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
}

/// Resolve a selected power's def. See [`PowerDatabase::resolve_power`] for the lookup
/// order. Kept as a free function so the calc passes read `resolve_power(db, set, name)`
/// uniformly; the resolution itself lives with the database it walks.
pub(crate) fn resolve_power<'a>(
    db: &'a PowerDatabase,
    set_id: &str,
    name: &str,
) -> Option<&'a Power> {
    db.resolve_power(set_id, name)
}

/// The contributing power set: every selected power that's auto or toggled on, in
/// `all_selected` order, with each active parent's stance sub-power appended after the
/// real powers (mirrors `collectAllPowers`, whose stance block runs last).
fn resolve_active_powers<'a>(
    state: &'a CharacterState,
    db: &'a PowerDatabase,
) -> (Vec<ActivePower<'a>>, Vec<CalcError>) {
    let mut active: Vec<ActivePower<'a>> = Vec::new();
    let mut stances: Vec<ActivePower<'a>> = Vec::new();
    let mut unresolved: Vec<CalcError> = Vec::new();
    for selection in state.all_selected() {
        // Archetype inherents (Vigilance, Fury, …) are handled by Pass 3's derived,
        // combat-context-driven step (crate::inherents), NOT the general apply loop. That loop
        // drops gated atoms (gather_base_atoms below) and can't see team size, so gathering
        // them here would mis-count AND double-count against Pass 3. Now that the inherent
        // powerset ships in the contract, this skip is load-bearing (before extraction there
        // was no def to resolve). Mirrors the beta `collectAllPowers` archetype-inherent skip.
        // Fitness/prestige/basic inherents stay ordinary powers.
        if selection.inherent_category == Some(InherentCategory::Archetype) {
            continue;
        }
        let Some(power_def) = resolve_power(db, &selection.powerset, &selection.internal_name)
        else {
            unresolved.push(CalcError::new(
                "gather",
                format!(
                    "selected power {}/{} is not in this dataset — its contribution is missing from the totals",
                    selection.powerset, selection.internal_name
                ),
            ));
            continue;
        };
        let parent_contributes = is_auto(power_def) || selection.is_active;
        if parent_contributes {
            active.push(ActivePower {
                def: power_def,
                kind: PowerSourceKind::ActivePower,
                power_set: &selection.powerset,
                targets_hit: selection.targets_hit,
                slots: &selection.slots,
            });
        }
        // Stance expansion: the active sub-power's own base effects flow like any
        // toggle (Bio Offensive's -Res self-penalty, Kheldian forms). Our model
        // already carries the sub-power's identity, so resolve it in the parent's set.
        //
        // Deliberately OUTSIDE the parent's active gate. The stance is stored build-scoped on
        // the parent's `active_sub_power`, and the granted stance toggles are never persisted
        // as their own selections. The beta's `setActiveSubPower` writes the stance without
        // touching the parent's `isActive`, so gating on the parent dropped the stance of every
        // build whose owner picked a stance and left the parent toggle off. That's the same
        // rule the TS oracle applies (`legacy-totals.oracle.ts` materializes the active stance
        // as a synthetic active power regardless of the parent), and it's what shipped for the
        // whole pre-engine life of the planner.
        if let Some(sub) = selection.active_sub_power.as_deref() {
            match resolve_power(db, &selection.powerset, sub) {
                Some(sub_power_def) => stances.push(ActivePower {
                    def: sub_power_def,
                    kind: PowerSourceKind::ActivePower,
                    power_set: &selection.powerset,
                    targets_hit: selection.targets_hit,
                    slots: &selection.slots,
                }),
                None => unresolved.push(CalcError::new(
                    "gather",
                    format!(
                        "active stance {sub} of {}/{} is not in this dataset — its contribution is missing from the totals",
                        selection.powerset, selection.internal_name
                    ),
                )),
            }
        }
    }
    active.extend(stances);
    active.extend(resolve_accolades(state, db, &mut unresolved));
    (active, unresolved)
}

/// The build's earned accolades, as contributing powers.
///
/// An accolade is an ordinary auto-on Self power (`The_Atlas_Medallion` = `MaxEndurance 5`,
/// `Freedom_Phalanx_Reserve` = `MaxHP 1`), but it costs no power pick, so the build carries it
/// as a flat id list beside the powersets instead of as a [`SelectedPower`]. Nothing resolved
/// that list: `CharacterState::accolades` crossed into the engine unread, so every accolade
/// bonus was silently dropped from the totals (report 2026-07-26; the beta's pre-engine calc
/// applied them in its own Step 8).
///
/// Resolving them HERE, rather than in a bespoke pass, is the Fitness rule and not the
/// Vigilance one (see this module's header): their atoms are plain ungated `MaxHP` /
/// `MaxEndurance` buffs that [`crate::apply`] already reads off any power, so a parallel pass
/// would know nothing the apply loop doesn't, and would have to re-derive the scale→percent
/// conversion and the `ignoreStrength` split that the appliers own.
///
/// Only `Auto` members contribute. The set is taken whole from the game data, so it also holds
/// the click/travel accolades (Eye of the Magus, Long Range Teleport); those grant a timed buff
/// on use, and folding one permanently into the totals because it was *earned* would be wrong.
/// An id that resolves to nothing is reported. A dropped bonus must not be silent.
fn resolve_accolades<'a>(
    state: &'a CharacterState,
    db: &'a PowerDatabase,
    unresolved: &mut Vec<CalcError>,
) -> Vec<ActivePower<'a>> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for id in &state.accolades {
        // Ids arrive lower-cased (the planner stores the internal name folded); one accolade
        // is earned once, so a repeated id must not stack its buff twice.
        if !seen.insert(id.to_ascii_lowercase()) {
            continue;
        }
        match find_accolade(db, id) {
            Some((power_set, def)) => {
                if is_auto(def) {
                    out.push(ActivePower {
                        def,
                        kind: PowerSourceKind::Accolade,
                        power_set,
                        targets_hit: None,
                        slots: &[],
                    });
                }
            }
            None => unresolved.push(CalcError::new(
                "gather",
                format!(
                    "accolade {id} is not in this dataset — its bonus is missing from the totals"
                ),
            )),
        }
    }
    out
}

/// Find an accolade by its folded internal name, within the accolade-category powersets
/// ([`PowerDatabase::accolade_powers`], the same universe the picker derives its toggles
/// from).
fn find_accolade<'a>(db: &'a PowerDatabase, id: &str) -> Option<(&'a str, &'a Power)> {
    db.accolade_powers_with_set()
        .find(|(_, p)| p.ident().eq_ignore_ascii_case(id))
}

/// Drop the mode-suppressed powers (`computeModeSuppression`). Every power here is
/// already active, so the beta's `isActivePower` gate is satisfied by construction.
/// A suspended toggle is still *running* (it still sets its own modes), so live modes
/// are collected from the whole set before suppression is resolved (single pass, no
/// cascade). A power never suppresses itself (a mode it also sets).
fn drop_mode_suppressed<'a>(powers: Vec<ActivePower<'a>>) -> Vec<ActivePower<'a>> {
    let mut live: HashMap<&str, &str> = HashMap::new();
    for p in &powers {
        for m in modes(p.def, "setsModes") {
            live.entry(m).or_insert_with(|| p.def.name.as_str());
        }
    }
    if live.is_empty() {
        return powers;
    }
    let mut suppressed: HashSet<&str> = HashSet::new();
    for p in &powers {
        for m in modes(p.def, "modesSuspended") {
            let Some(&setter) = live.get(m) else { continue };
            if setter != p.def.name {
                suppressed.insert(p.def.ident());
                break;
            }
        }
    }
    powers
        .into_iter()
        .filter(|p| !suppressed.contains(p.def.ident()))
        .collect()
}
