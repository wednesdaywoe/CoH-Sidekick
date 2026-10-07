//! A granted power survives being saved and read back (GRANTLOCK-1).
//!
//! `.skif` stores neither `is_locked` nor `inherent_category`: the reader drops both on purpose
//! and names the load path's two reconciles as what re-derives them (`skif.rs`, rule 6). Neither
//! reconcile did. Both skipped an entry already under the grant's name instead of claiming it,
//! so a grant came back from a file as an ordinary pick and stayed one — and, on the inherents
//! list, the identity sync that runs FIRST dropped the unclaimed entry outright, so the grant
//! was re-created from its def with its toggle off and its enhancements gone.
//!
//! Two costs on the read side, so two legs. A powerset or pool grant lost its granted mark and
//! SPENT A POWER PICK, on all four forks — every pool that hands something out (Afterburner,
//! Jaunt, Double Jump, Translocation, Stomp, Turbo Boost, Athletics). An inherent grant lost the
//! user's toggle and slotting outright — eleven powers across four Thunderspy archetypes, Hide
//! among them, which is the bug report this gate was written for.
//!
//! The third leg pins the invariant the fix rests on — after the pair, nothing in the inherents
//! list is unclaimed — since that is what makes it safe for the identity sync to carry an
//! unstamped entry through instead of dropping it.
//!
//! The fourth leg is the SAVE side, which the round-trip turned up on the way: the writer chose
//! what to record from the slotting alone, so an inherent the user only switched on never
//! reached the file. Sprint is the everyday case; two of the Thunderspy grants take no slots at
//! all and so could never be saved on.
//!
//! Graded through the REAL codec (`skif::encode` then `skif::decode`) rather than a hand-cleared
//! `SelectedPower`, because what the reader drops is the thing under test: a probe that clears
//! the fields itself is only grading its own idea of the format. That choice is what found the
//! fourth leg — a hand-cleared probe reported the inherents half already fixed while a real save
//! was still dropping two of the eleven.

use coh_data::{
    ArchetypeSelection, CharacterState, DatasetId, Enhancement, InherentCategory, InherentGrant,
    Level, PoolSelection, PowerDatabase, SelectedPower,
};
use std::path::PathBuf;

fn load(dataset: DatasetId) -> PowerDatabase {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../contract")
        .join(dataset.as_str())
        .join("bundle.json.gz");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    PowerDatabase::from_gz_bytes(&bytes).unwrap_or_else(|e| panic!("load {dataset:?}: {e}"))
}

/// The two reconciles `build_io::adopt_as` runs on a load, in its order — the identity sync
/// first, the grant sync second. Inlined because the app crate has no lib target, and the ORDER
/// is half of what the gate is grading: reversing it hides the inherents half of the defect.
fn load_reconcile(state: &mut CharacterState, db: &PowerDatabase) {
    if state.archetype.id.is_some() {
        if let Ok(Some(grants)) = db.inherent_grants() {
            let archetype_inherent = archetype_inherent(state, db);
            let auto = db
                .leveling_schedule
                .as_ref()
                .map(|schedule| schedule.auto_granted_slot_levels.clone())
                .unwrap_or_default();
            state.inherents = coh_data::granted_inherents(
                &grants,
                archetype_inherent.as_ref(),
                &auto,
                state.level,
                &state.inherents,
            );
        }
    }
    coh_data::sync_granted_powers(state, db);
}

/// `app::inherents::archetype_inherent`, to the extent this gate needs it.
fn archetype_inherent(state: &CharacterState, db: &PowerDatabase) -> Option<InherentGrant> {
    let archetype_id = state.archetype.id.as_deref()?;
    let declared = db
        .archetypes()
        .ok()?
        .get(archetype_id)?
        .inherent
        .name
        .clone();
    let power = db.archetype_inherent(&declared)?;
    Some(InherentGrant {
        internal_name: power.ident().to_string(),
        name: power.name.clone(),
        category: InherentCategory::Archetype,
        max_slots: power.max_slots().unwrap_or(0),
    })
}

/// Save and read back through the real codec, then reconcile exactly as a load does.
fn round_trip(state: &CharacterState, db: &PowerDatabase) -> CharacterState {
    let text = coh_data::skif::encode(state, db).expect("the build encodes");
    let decoded = coh_data::skif::decode(&text, db).expect("its own file decodes");
    let mut back = decoded.build;
    load_reconcile(&mut back, db);
    back
}

fn some_archetype(db: &PowerDatabase) -> ArchetypeSelection {
    let catalog = db.archetypes().expect("archetype catalog");
    let first = catalog.all().first().expect("a fork has archetypes");
    ArchetypeSelection {
        id: Some(first.id.clone()),
        name: first.name.clone(),
    }
}

/// Every pool that hands a power out, with the picks that could enable it held.
fn granting_pools(db: &PowerDatabase) -> Vec<(String, Vec<SelectedPower>)> {
    let mut ids: Vec<String> = db
        .pool_powers
        .iter()
        .filter(|entry| entry.power.is_auto_issued())
        .map(|entry| entry.set_id.clone())
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|set_id| {
            let picks = db
                .pool_powers
                .iter()
                .filter(|entry| entry.set_id == set_id && !entry.power.is_auto_issued())
                .map(|entry| SelectedPower::picked(entry.power.ident(), &set_id, 1))
                .collect();
            (set_id, picks)
        })
        .collect()
}

/// A pool grant comes back a grant, and so goes on spending no power pick.
///
/// The pick budget is the cost that made this visible: `picked_powers` counts every UNLOCKED
/// selection, so a grant that lost its mark took one of the build's 24 picks and could block a
/// real one.
#[test]
fn a_pool_grant_comes_back_a_grant() {
    let mut graded = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        for (set_id, picks) in granting_pools(&db) {
            let mut state = CharacterState::empty(dataset);
            state.level = 50;
            state.archetype = some_archetype(&db);
            state.pools.push(PoolSelection {
                id: set_id.clone(),
                name: String::new(),
                powers: picks,
            });
            load_reconcile(&mut state, &db);

            let granted: Vec<String> = state.pools[0]
                .powers
                .iter()
                .filter(|power| power.is_locked)
                .map(|power| power.internal_name.clone())
                .collect();
            if granted.is_empty() {
                continue;
            }
            let picks_before = state.picked_powers().count();

            let back = round_trip(&state, &db);

            if back.picked_powers().count() != picks_before {
                failures.push(format!(
                    "{dataset:?} {set_id}: picks spent went {picks_before} -> {} across a read",
                    back.picked_powers().count()
                ));
            }
            for name in &granted {
                graded += 1;
                match back.pools[0]
                    .powers
                    .iter()
                    .find(|power| &power.internal_name == name)
                {
                    None => {
                        failures.push(format!("{dataset:?} {set_id}/{name}: gone after a read"))
                    }
                    Some(power) if !power.is_locked => failures.push(format!(
                        "{dataset:?} {set_id}/{name}: came back unlocked — a grant read as a pick"
                    )),
                    Some(_) => {}
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The defect was 28 pool/fork pairs; a floor well under that still catches a pass that
    // stops reaching the corpus, without pinning a number the forks may move.
    assert!(
        graded >= 20,
        "only {graded} pool grants graded — the sweep stopped finding them"
    );
    println!("{graded} pool grants survived a save and a read");
}

/// An inherent grant comes back with the user's toggle and the user's enhancements.
///
/// The worse half: because the identity sync runs first on the load path, an unclaimed entry
/// was DROPPED before the grant sync could carry anything from it, so the power was rebuilt
/// from its def — off, and empty.
#[test]
fn an_inherent_grant_keeps_its_toggle_and_its_slotting() {
    let mut graded = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let catalog = db.archetypes().expect("archetype catalog");
        for archetype in catalog.all() {
            let mut state = CharacterState::empty(dataset);
            state.level = 50;
            state.archetype = ArchetypeSelection {
                id: Some(archetype.id.clone()),
                name: archetype.name.clone(),
            };
            load_reconcile(&mut state, &db);

            // Switch on every grant this archetype gets and slot the ones that take slots.
            let granted: Vec<String> = state
                .inherents
                .iter()
                .filter(|power| power.inherent_category == Some(InherentCategory::Granted))
                .map(|power| power.internal_name.clone())
                .collect();
            if granted.is_empty() {
                continue;
            }
            for power in state.inherents.iter_mut() {
                if !granted.contains(&power.internal_name) {
                    continue;
                }
                power.is_active = true;
                if !power.slots.is_empty() {
                    power.slots[0] = Some(Enhancement::generic_io("Defense", Level::new(50), 0));
                }
            }
            let before: Vec<SelectedPower> = state
                .inherents
                .iter()
                .filter(|power| granted.contains(&power.internal_name))
                .cloned()
                .collect();

            let back = round_trip(&state, &db);

            for was in &before {
                graded += 1;
                let Some(now) = back
                    .inherents
                    .iter()
                    .find(|power| power.internal_name == was.internal_name)
                else {
                    failures.push(format!(
                        "{dataset:?} {}/{}: gone after a read",
                        archetype.id, was.internal_name
                    ));
                    continue;
                };
                let filled = |power: &SelectedPower| {
                    power.slots.iter().filter(|slot| slot.is_some()).count()
                };
                if now.is_active != was.is_active {
                    failures.push(format!(
                        "{dataset:?} {}/{}: toggle was {} and came back {}",
                        archetype.id, was.internal_name, was.is_active, now.is_active
                    ));
                }
                if filled(now) != filled(was) {
                    failures.push(format!(
                        "{dataset:?} {}/{}: {} enhancement(s) went in and {} came back",
                        archetype.id,
                        was.internal_name,
                        filled(was),
                        filled(now)
                    ));
                }
                if !now.is_locked {
                    failures.push(format!(
                        "{dataset:?} {}/{}: came back unlocked",
                        archetype.id, was.internal_name
                    ));
                }
                // One entry, not two: the identity sync now carries an unstamped entry
                // through, and a name it answers for itself would otherwise ride twice.
                let copies = back
                    .inherents
                    .iter()
                    .filter(|power| power.internal_name == was.internal_name)
                    .count();
                if copies != 1 {
                    failures.push(format!(
                        "{dataset:?} {}/{}: {copies} copies in the inherents list",
                        archetype.id, was.internal_name
                    ));
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The defect was 11 archetype/power pairs, all Thunderspy. The floor is below that and
    // above zero, so a fork that stops granting some of them does not turn the gate vacuous
    // silently.
    assert!(
        graded >= 8,
        "only {graded} inherent grants graded — the sweep stopped finding them"
    );
    println!("{graded} inherent grants kept their toggle and slotting across a read");
}

/// After the load pair, nothing in the inherents list is unclaimed.
///
/// This is the invariant that makes the fix safe. The identity sync now carries an entry with
/// no category through instead of dropping it, which is only sound because the grant sync that
/// always runs next either claims it or drops it. An unclaimed entry left in the list would be
/// invisible — `InherentsSection` groups by category and renders none of them — while still
/// reaching the totals.
#[test]
fn the_load_pair_leaves_nothing_unclaimed() {
    let mut graded = 0usize;

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let catalog = db.archetypes().expect("archetype catalog");
        for archetype in catalog.all() {
            let mut state = CharacterState::empty(dataset);
            state.level = 50;
            state.archetype = ArchetypeSelection {
                id: Some(archetype.id.clone()),
                name: archetype.name.clone(),
            };
            load_reconcile(&mut state, &db);
            let back = round_trip(&state, &db);
            for power in &back.inherents {
                graded += 1;
                assert!(
                    power.inherent_category.is_some(),
                    "{dataset:?} {}/{}: still unclaimed after the load pair",
                    archetype.id,
                    power.internal_name
                );
            }
        }
    }

    assert!(graded >= 400, "only {graded} inherents graded");
    println!("{graded} inherent entries all carried a category after a read");
}

/// An inherent switched on and never slotted is still written to the file.
///
/// The save-side half of GRANTLOCK-1, and the one that cost state without any grant being
/// involved: `is_modified_inherent` decided what to write from the SLOTTING alone, so an
/// inherent the user only switched on was left out of the file entirely and came back off.
/// Sprint is the everyday case — a build running with it on saved as a build with it off — and
/// the slotless grants (Hold Ground, Placate) could never be saved on at all.
///
/// Graded on every inherent the fork grants that takes no enhancement, and on one that does,
/// with NOTHING placed in it — so the slot terms cannot carry the leg and only the toggle can.
#[test]
fn an_inherent_switched_on_but_never_slotted_is_written() {
    let mut graded = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for dataset in DatasetId::ALL {
        let db = load(dataset);
        let catalog = db.archetypes().expect("archetype catalog");
        for archetype in catalog.all() {
            let mut state = CharacterState::empty(dataset);
            state.level = 50;
            state.archetype = ArchetypeSelection {
                id: Some(archetype.id.clone()),
                name: archetype.name.clone(),
            };
            load_reconcile(&mut state, &db);

            // Every inherent on, and not one enhancement placed anywhere.
            let on: Vec<String> = state
                .inherents
                .iter()
                .map(|power| power.internal_name.clone())
                .collect();
            for power in state.inherents.iter_mut() {
                power.is_active = true;
            }
            assert!(
                state
                    .inherents
                    .iter()
                    .all(|power| power.slots.iter().all(Option::is_none)),
                "the leg proves nothing if a slot is filled"
            );

            let back = round_trip(&state, &db);

            for name in &on {
                graded += 1;
                let copies: Vec<&SelectedPower> = back
                    .inherents
                    .iter()
                    .filter(|power| &power.internal_name == name)
                    .collect();
                match copies.as_slice() {
                    [] => failures.push(format!(
                        "{dataset:?} {}/{name}: dropped from the list by a save",
                        archetype.id
                    )),
                    [power] if !power.is_active => failures.push(format!(
                        "{dataset:?} {}/{name}: switched on, saved, came back off",
                        archetype.id
                    )),
                    [_] => {}
                    // EVERY inherent is on here, so every one is written and every one comes
                    // back with no category — which is the state the identity sync could
                    // double: it rebuilds the universal grants from their own names AND carries
                    // unstamped entries through, and a name in both lists would ride twice.
                    // Two copies of an inherent is two contributions to the totals.
                    many => failures.push(format!(
                        "{dataset:?} {}/{name}: {} copies in the inherents list",
                        archetype.id,
                        many.len()
                    )),
                }
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(graded >= 400, "only {graded} inherents graded");
    println!("{graded} inherents kept an unslotted toggle across a save");
}
