//! An untouched targets-hit slider must not delete a base the game gives unconditionally.
//!
//! A per-foe self-buff reaches its caster only through a target: its `EntsAffected` lists foes
//! alone, so with nobody in radius no effect block runs and an absent count is honestly zero
//! (PROD6B-2d). Phalanx Fighting is the other shape — `EntsAffected kLeaguemate, kCaster` over a
//! 12-foot sphere with `maxTargets 3`, an unconditional `Replace` base of 0.5 beside a
//! `target ≠ source` increment of 0.3. The caster fills the first of those three seats for as long
//! as the power is on, so N never reaches zero, and reading the untouched slider as "no foes"
//! deleted the 5% melee/ranged/AoE defence the game hands out with no ally in sight (PERFOE-3).
//!
//! Guarded Spin is the third shape, and it is a CLICK rather than an aura (PERFOE-4). Its
//! `EntsAffected` is `kFoe` alone, so the seat predicate above says nothing about it, and its
//! +Def(Melee, Lethal) is one `kStackType_Stack` mod aimed at the caster which the game applies
//! once per foe the cone lands on — so the per-foe growth is real. What is not real is the empty
//! count: `character_tick.c` refuses a queued power outright when the target entity is null or of
//! the wrong type, and a cone range-checks that same entity before firing. A foe-aimed power that
//! fired had a foe in front of it, so a build saying "I use this" is saying N ≥ 1, and the
//! reporter's Staff Fighting attack moved no defence whatever they did with its toggle.
//!
//! Four populations, and the middle two are why this is a gate and not an assertion:
//!
//! 1. **Floored on the caster's own seat** — [`caster_occupies_a_target_slot`]. Absent and one
//!    target must agree on every field; the base is not the slider's to delete.
//! 2. **Floored on the aim** — [`aim_guarantees_a_target`]. Same assertion, different warrant.
//!    The set is DECLARED by name below, and so is its row count: the predicate reads two fields
//!    no `per_target` power's value depends on, so widening or narrowing it has to change the
//!    list rather than quietly change a total.
//! 3. **Caster-affected, but the count is not an AoE entity count.** `per_target` also reaches
//!    atoms from the `Execute_Power` redirect branch, where the increment counts something else
//!    entirely — Reactive Regeneration's stacks count how recently you were hit, on a
//!    `SingleTarget` toggle whose `maxTargets` is absent. A floor there would assert a combat
//!    state rather than a seat in a sphere, so these keep the zero. The set is DECLARED by name
//!    below: widening the predicate moves a power out of it and reds, narrowing moves one in.
//! 4. **Foe-aimed by nothing, or aimed at the caster** — the control. If nothing there moved
//!    between absent and one target, the corpus this sweep walks reaches no field and its clean
//!    floored buckets would mean nothing.

use coh_data::{CharacterState, DatasetId, PoolSelection, Power, PowerDatabase, SelectedPower};
use coh_math::stacking::{
    aim_guarantees_a_target, caster_occupies_a_target_slot, per_target_count_cannot_be_zero,
};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::PathBuf;

/// The powers whose `targetsAffected` names the caster and whose `per_target` did NOT come from
/// the AoE pass, as `dataset/powerset/ident`. Each keeps the zero at an absent count.
///
/// Written out rather than derived so that a change to the predicate has to change this list too.
/// Reactive Regeneration is the whole population: the redirect branch stamps its four
/// `Melee_Ones` stacks (regen, and the regen/end/recovery debuff resistances) on a power with no
/// sphere at all.
const NOT_AN_AOE_COUNT: &[&str] = &[
    "homecoming/brute/regeneration/Instant_Regeneration",
    "homecoming/scrapper/regeneration/Instant_Regeneration",
    "homecoming/sentinel/regeneration/Instant_Regeneration",
    "homecoming/stalker/regeneration/Instant_Regeneration",
    "homecoming/tanker/regeneration/Instant_Healing",
    "brainstorm/brute/regeneration/Instant_Regeneration",
    "brainstorm/scrapper/regeneration/Instant_Regeneration",
    "brainstorm/sentinel/regeneration/Instant_Regeneration",
    "brainstorm/stalker/regeneration/Instant_Regeneration",
    "brainstorm/tanker/regeneration/Instant_Healing",
];

/// The powers whose per-foe count is floored because the game will not let them be used without
/// an entity in their sights — every one of them a Click aimed at a `Foe` over a bounded AoE or
/// cone, which is the only shape [`aim_guarantees_a_target`] admits in any of the four contracts.
/// Fulcrum Shift / Kinetic Transfer reach it through the redirect's sphere: a single-target
/// shell whose `KineticTransfer` hits up to 10 foes around the one it was aimed at
/// (`perTargetMaxTargets`), so a cast reached at least that one.
///
/// Idents rather than `dataset/powerset/ident`, with the placement count pinned beside them: the
/// same power is floored under up to seventeen archetype/fork pairs and the list is here to be
/// read, not to enumerate. 16 of the 26 reach a displayed field; the rest carry an `Endurance`
/// magnitude at duration 0, a `Meta` combat marker, or an increment the base pass drops as gated,
/// and float on the predicate without moving a number (the same way PERFOE-3's Phoenix Awakening
/// and Rejuvenate do). Cross Punch left the list when its never-true `0` group stopped minting a
/// conditional, the only thing that stamped a per-foe increment on it.
const AIM_FLOORED: &[&str] = &[
    "Aging_Touch",
    "Bitter_Freeze_Ray",
    "Charged_Brawl",
    "Dark_Pit",
    "Defensive_Sweep",
    "Devour_Psyche",
    "Electric_Fence",
    "Follow_Up",
    "Fulcrum_Shift",
    "Guarded_Spin",
    "Havoc_Punch",
    "High_Low",
    "Keening_Winds",
    "Kinetic_Transfer",
    "Moisture_Absorption",
    "Pale_Blade",
    "Parasitic_Leech",
    "Parry",
    "Placate",
    "Shadow_Maul",
    "Soul_Drain",
    "Special_2",
    "Stun",
    "Throw_Sand",
    "Thunder_Strike",
    "Thunderous_Blast",
];

/// How many `dataset/powerset/ident` rows the list above stands for.
const AIM_FLOORED_ROWS: usize = 80;

/// Every spelling of `targetType` the four contracts carry. [`Power::aim_requires_an_entity`]
/// answers from a declared vocabulary, and an unrecognised spelling would take its `false` arm
/// silently — which is the soft default CLAUDE.md Rule 1 exists to refuse. Pinned here so a fork
/// introducing one reds a test instead.
const TARGET_TYPE_VOCABULARY: &[&str] = &[
    "Ally (Alive)",
    "Any",
    "Dead Teammate",
    "DeadFoe",
    "Foe",
    "Location",
    "Own Pet (Alive)",
    "Self",
    "Teammate",
    "Teleport",
];

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn carries_per_target(power: &Power) -> bool {
    power
        .atoms
        .iter()
        .any(|a| a.per_target.is_some_and(|v| v != 0.0))
}

/// The totals a level-50 `archetype` holding exactly this one power, active, produces — the real
/// gather + apply passes, with `targets_hit` passed through as given.
fn totals(
    db: &PowerDatabase,
    dataset: DatasetId,
    ident: &str,
    container: &str,
    is_pool: bool,
    targets_hit: Option<u32>,
    archetype: &str,
) -> Value {
    let mut state = CharacterState::empty(dataset);
    let mut pick = SelectedPower::picked(ident.to_string(), container, 1);
    pick.is_active = true;
    pick.targets_hit = targets_hit;
    if is_pool {
        state.pools = vec![PoolSelection {
            id: container.to_string(),
            name: container.to_string(),
            powers: vec![pick],
        }];
    } else {
        state.primary.powers = vec![pick];
    }
    state.level = 50;
    state.archetype.id = Some(archetype.to_string());

    let powers = coh_math::gather::gather_active_powers(&state, db).powers;
    let mut g = coh_math::GlobalBonuses::default();
    let (mut stealth, mut absorb, mut movement, mut caps, mut res_debuffs) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    coh_math::apply::apply_active_power_bonuses(
        &powers,
        &mut g,
        archetype,
        50,
        &coh_math::strength::StrengthBuffs::default(),
        &state.combat,
        &coh_math::incarnates::AlphaEnhancement::default(),
        &mut stealth,
        &mut absorb,
        &mut movement,
        &mut caps,
        &mut res_debuffs,
        &mut Vec::new(),
        db,
    );
    serde_json::json!({
        "totals": serde_json::to_value(&g).expect("totals serialize"),
        "absorb": format!("{absorb:?}"),
        "movement": format!("{movement:?}"),
    })
}

/// The archetype the atom fork resolves against, per powerset — its own container prefix, which is
/// the archetype whose arm a forked atom names. Probing every archetype would multiply the sweep
/// by fifteen to re-run the same read.
fn archetype_of(container: &str) -> String {
    container.split('/').next().unwrap_or(container).to_string()
}

/// One archetype to probe a pool or epic power under. A pool power belongs to no archetype, so
/// there is no owning class to read off its container the way `archetype_of` reads a powerset's.
/// Taken from the dataset's own roster rather than named, so a fork whose archetypes are spelled
/// differently still probes something real.
fn pool_probe_archetype(db: &PowerDatabase) -> String {
    db.archetypes()
        .expect("archetype catalog")
        .all()
        .iter()
        .find(|at| db.class_name_of(&at.id).is_some())
        .map(|at| at.id.clone())
        .expect("a dataset with no classed archetype cannot be probed")
}

#[test]
fn an_untouched_slider_keeps_a_base_the_caster_always_gets() {
    let mut declared_seen: BTreeSet<String> = BTreeSet::new();
    let mut floored_total = 0usize;
    let mut foe_only_moved = 0usize;
    let mut aim_seen: BTreeSet<String> = BTreeSet::new();
    let mut aim_rows = 0usize;

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let mut floored_here = 0usize;
        // Pools and epics are walked beside the powersets, under a fixed probe archetype: they
        // carry no per-target power today, and the point of including them is that a future one
        // cannot slip past the declared exclusion by living outside the powerset tree.
        let pool_probe = pool_probe_archetype(&db);
        let subjects: Vec<(&Power, String, bool, String)> = db
            .powersets
            .iter()
            .flat_map(|ps| {
                ps.powers
                    .iter()
                    .map(move |p| (p, ps.id.clone(), false, archetype_of(&ps.id)))
            })
            .chain(
                db.pool_powers
                    .iter()
                    .chain(db.epic_powers.iter())
                    .map(|part| (&part.power, part.set_id.clone(), true, pool_probe.clone())),
            )
            .collect();
        {
            for (power, container, is_pool, archetype) in subjects {
                if !carries_per_target(power) {
                    continue;
                }
                if db.class_name_of(&archetype).is_none() {
                    continue;
                }
                let key = format!("{}/{}/{}", dataset.as_str(), container, power.ident());
                let absent = totals(
                    &db,
                    dataset,
                    power.ident(),
                    &container,
                    is_pool,
                    None,
                    &archetype,
                );
                let one = totals(
                    &db,
                    dataset,
                    power.ident(),
                    &container,
                    is_pool,
                    Some(1),
                    &archetype,
                );

                if caster_occupies_a_target_slot(power) {
                    floored_here += 1;
                    floored_total += 1;
                    assert_eq!(
                        absent, one,
                        "{key}: the caster holds one of this power's own target slots, so an \
                         absent count is its solo value — not a power that failed to fire"
                    );
                } else if aim_guarantees_a_target(power) {
                    aim_rows += 1;
                    aim_seen.insert(power.ident().to_string());
                    assert!(
                        AIM_FLOORED.contains(&power.ident()),
                        "{key}: its aim already rules out the empty count, so it is floored — but \
                         it is not in AIM_FLOORED. Adjudicate it and add it, or narrow the \
                         predicate; a floor nobody wrote down is a total nobody checked"
                    );
                    assert_eq!(
                        absent, one,
                        "{key}: the game refuses this power without an entity in its sights, so a \
                         build that uses it reached at least one — an absent count is its \
                         one-target value, not a power that fired into empty air (PERFOE-4)"
                    );
                } else if power.affects_caster() {
                    declared_seen.insert(key.clone());
                    assert!(
                        NOT_AN_AOE_COUNT.contains(&key.as_str()),
                        "{key}: names the caster in `targetsAffected` but its per-target count is \
                         not an AoE entity count, and it is not in NOT_AN_AOE_COUNT. Adjudicate it \
                         and add it, or widen the predicate — do not leave the reading unsaid"
                    );
                } else if absent != one {
                    foe_only_moved += 1;
                }
            }
        }
        assert!(
            floored_here > 0,
            "{}: no power floors its per-target count — Phalanx Fighting is on all four forks, so \
             the predicate has stopped matching anything",
            dataset.as_str()
        );
    }

    // Both directions on the declared exclusion: every name must still be reached, and the sweep
    // above already refused any that is not on the list.
    let declared: BTreeSet<String> = NOT_AN_AOE_COUNT.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        declared_seen, declared,
        "NOT_AN_AOE_COUNT no longer matches the corpus — a name here is not being reached, so the \
         predicate has widened (or the power is gone) and the exclusion is stale"
    );
    assert!(
        floored_total >= 12,
        "only {floored_total} floored powers across four forks — Phalanx Fighting alone is 3 per \
         fork"
    );
    assert!(
        foe_only_moved > 0,
        "no foe-only per-target power moved between an absent and a one-target count — this sweep \
         is measuring a corpus that reaches no field, and its clean floored bucket proves nothing"
    );

    // Both directions on the aim-floored declaration, the same way as the exclusion above: the
    // sweep already refused a power that is floored and unlisted, and this refuses a listed power
    // the predicate has stopped reaching.
    let aim_declared: BTreeSet<String> = AIM_FLOORED.iter().map(|s| (*s).to_string()).collect();
    assert_eq!(
        aim_seen, aim_declared,
        "AIM_FLOORED no longer matches the corpus — a name here is not being floored any more, so \
         the predicate has narrowed (or the power is gone) and a total has moved back to zero"
    );
    assert_eq!(
        aim_rows, AIM_FLOORED_ROWS,
        "the aim floor reaches {aim_rows} dataset/powerset/ident rows, not the {AIM_FLOORED_ROWS} \
         it was measured over. The named set is unchanged, so this is a change in PLACEMENT — a \
         fork granting the same power to another archetype, or withdrawing one. Re-census and \
         re-pin"
    );
}

/// `aim_requires_an_entity` reads a declared vocabulary, so the vocabulary has to be the whole of
/// what the data says. An unlisted spelling takes its `false` arm and silently keeps a zero.
#[test]
fn target_type_vocabulary_is_fully_declared() {
    let declared: BTreeSet<&str> = TARGET_TYPE_VOCABULARY.iter().copied().collect();
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let powers: Vec<&Power> = db
            .powersets
            .iter()
            .flat_map(|ps| ps.powers.iter())
            .chain(
                db.pool_powers
                    .iter()
                    .chain(db.epic_powers.iter())
                    .map(|part| &part.power),
            )
            .collect();
        for power in powers {
            if let Some(aim) = power.extra.get("targetType").and_then(Value::as_str) {
                seen.insert(aim.to_string());
            }
        }
        assert!(
            !seen.is_empty(),
            "{dataset:?}: no power states a targetType at all"
        );
        for aim in &seen {
            assert!(
                declared.contains(aim.as_str()),
                "{dataset:?}: targetType {aim:?} is not in TARGET_TYPE_VOCABULARY, so \
                 `Power::aim_requires_an_entity` is answering it from a default. Decide whether \
                 the game can fire such a power with nobody targeted and add it to one side"
            );
        }
    }
}

/// The floor is a claim about two fields, and the union is the only thing the apply sites ask.
#[test]
fn the_two_reasons_are_both_reasons() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let powers: Vec<&Power> = db
            .powersets
            .iter()
            .flat_map(|ps| ps.powers.iter())
            .chain(
                db.pool_powers
                    .iter()
                    .chain(db.epic_powers.iter())
                    .map(|part| &part.power),
            )
            .collect();
        let mut seats = 0usize;
        let mut aims = 0usize;
        for power in powers {
            let seat = caster_occupies_a_target_slot(power);
            let aim = aim_guarantees_a_target(power);
            assert_eq!(
                per_target_count_cannot_be_zero(power),
                seat || aim,
                "{dataset:?}/{}: the union the apply sites read has stopped being the union of \
                 the two reasons",
                power.ident()
            );
            if carries_per_target(power) && seat {
                seats += 1;
            }
            if carries_per_target(power) && aim {
                aims += 1;
            }
        }
        assert!(
            seats > 0 && aims > 0,
            "{dataset:?}: floored by seat {seats}, by aim {aims} — every fork carries both \
             (Phalanx Fighting and Guarded Spin), so a zero here means a predicate stopped matching"
        );
    }
}
