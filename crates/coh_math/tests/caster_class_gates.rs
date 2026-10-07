//! A damage gate that forks on the caster's archetype must answer for the build's own class.
//!
//! The converter stamps `caster_archetypes` only where the class is the gate's SOLE unknown. The
//! hit-time forks conjoin it with live target state — Kick's Stalker arm is
//! `kHeld target> 0 > kSleep target> 0 > || arch source> Class_Stalker eq && …` — so they reach
//! the wire unstamped, and before `SourceContext::caster_class` every build of every archetype saw
//! the Stalker, Corruptor and Controller arms as unresolved damage lines.
//!
//! A class answered as a name also has to survive `==`: before it was answered, no `==` check
//! ever saw two names, and once it was they all read as malformed.
//!
//! Two directions: a Blaster's projection names no other archetype's arm anywhere in the corpus,
//! and a Stalker's Kick still reports its own held-target arm (the control — an assertion that
//! passes by wiping every forked row would pass the first half too).

use coh_data::{DatasetId, PowerDatabase};
use coh_math::damage::resolve_power_damage;
use coh_math::expr::{SourceContext, TargetIdentity};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

fn context(db: &PowerDatabase, archetype: &str) -> SourceContext {
    SourceContext {
        target: Some(TargetIdentity {
            archetype_class: None,
            entity_type: "critter".to_string(),
        }),
        caster_class: Some(
            db.class_name_of(archetype)
                .unwrap_or_else(|| panic!("no class name for {archetype}"))
                .to_string(),
        ),
        ..Default::default()
    }
}

/// The `Class_*` tokens a gate compares `arch source>` against, by `eq` or by `==` — the game
/// reads a `==` between two names as `eq`, and one check in seven is spelled that way.
fn caster_classes_named(gate: &str) -> Vec<&str> {
    let tokens: Vec<&str> = gate.split_whitespace().collect();
    tokens
        .windows(4)
        .filter(|w| {
            w[0] == "arch" && w[1].eq_ignore_ascii_case("source>") && (w[3] == "eq" || w[3] == "==")
        })
        .map(|w| w[2])
        .collect()
}

#[test]
fn no_other_archetypes_arm_reaches_the_unresolved_lines() {
    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let ctx = context(&db, "blaster");
        let own = db.class_name_of("blaster").unwrap();
        let mut foreign = Vec::new();
        for power in db.all_powers() {
            let mut errors = Vec::new();
            let damage =
                resolve_power_damage(power, "blaster", 50, 0.0, 0.0, &ctx, &db, &mut errors);
            for row in &damage.unresolved {
                if row.reason.starts_with("gate is malformed") {
                    foreign.push(format!("{} — {} ({})", power.ident(), row.gate, row.reason));
                }
                let named = caster_classes_named(&row.gate);
                if !named.is_empty() && !named.iter().any(|c| c.eq_ignore_ascii_case(own)) {
                    foreign.push(format!("{} — {}", power.ident(), row.gate));
                }
            }
        }
        assert!(
            foreign.is_empty(),
            "{dataset:?}: {} unresolved rows name another archetype, e.g. {:?}",
            foreign.len(),
            &foreign[..foreign.len().min(5)]
        );
    }
}

#[test]
fn a_stalkers_kick_keeps_its_own_arm() {
    let db = load(DatasetId::Homecoming);
    let kick = db
        .all_powers()
        .find(|p| p.name == "Kick")
        .expect("Homecoming carries Kick");
    let mut errors = Vec::new();
    let damage = resolve_power_damage(
        kick,
        "stalker",
        50,
        0.0,
        0.0,
        &context(&db, "stalker"),
        &db,
        &mut errors,
    );
    assert!(
        damage
            .unresolved
            .iter()
            .any(|row| row.gate.contains("Class_Stalker") && row.gate.contains("kHeld")),
        "the Stalker's held-target arm vanished: {:?}",
        damage.unresolved
    );
}

/// A gate only the fight can answer still has an amount: Containment decides WHETHER a
/// Controller's Kick lands its second hit, not how big it is, and the line states that hit
/// rather than the gate text. Containment doubles, so it is the certain hit again.
#[test]
fn a_controllers_kick_states_what_containment_adds() {
    let db = load(DatasetId::Homecoming);
    let kick = db
        .all_powers()
        .find(|p| p.name == "Kick")
        .expect("Homecoming carries Kick");
    let mut errors = Vec::new();
    let damage = resolve_power_damage(
        kick,
        "controller",
        50,
        0.0,
        0.0,
        &context(&db, "controller"),
        &db,
        &mut errors,
    );
    let containment = damage
        .unresolved
        .iter()
        .find(|row| row.tags.iter().any(|tag| tag == "Containment"))
        .expect("a Controller's Kick carries a Containment line");
    let adds = containment
        .if_it_lands
        .as_ref()
        .expect("Containment's amount does not depend on its gate");
    assert!(
        (adds.total.base - damage.base).abs() < 1e-9,
        "Containment adds {} against a hit of {}",
        adds.total.base,
        damage.base
    );
}

/// Rebirth guards Force Bolt's base hit with `cur.kUntouchable target> 0 <=`. A target the
/// attack can hit is never Untouchable, so that hit is the certain one, not a situational line
/// beside a zero headline. Containment's twin still waits on the foe's mez state.
#[test]
fn rebirths_force_bolt_lands_its_base_hit() {
    let db = load(DatasetId::Rebirth);
    let bolt = db
        .all_powers()
        .find(|p| p.ident() == "Force_Bolt")
        .expect("Rebirth carries Force_Bolt");
    let mut errors = Vec::new();
    let damage = resolve_power_damage(
        bolt,
        "defender",
        50,
        0.0,
        0.0,
        &context(&db, "defender"),
        &db,
        &mut errors,
    );
    assert!(
        damage.base > 0.0 && damage.components.iter().any(|c| c.is_certain()),
        "the base hit is not certain: {damage:?}"
    );
    assert!(
        damage
            .unresolved
            .iter()
            .any(|row| row.gate.contains("kHeld")),
        "the Containment twin stopped waiting on mez state: {:?}",
        damage.unresolved
    );
}
