//! Display names, resolved from the ids a build actually stores.
//!
//! A build carries identity, not definitions — a powerset id, a power's internal name, a pool
//! id — and every display name beside them is a convenience the picker filled in. The `.skif`
//! codec does not store any of them (rule 6: anything the reader can compute is never stored),
//! so [`coh_data::skif::hydrate`] leaves those fields EMPTY and says outright that the UI
//! resolves them from the id.
//!
//! Which means: **a surface that reads a stored `name` is correct only for builds the user
//! picked in this session, and blank for every build that arrived from a file.** That defect has
//! now shipped twice — the header's identity chip read `· /`, and the Powers panel's pool and
//! epic group titles were empty headings — both from reading the field instead of the id. This
//! module is the one place that resolution lives, so a third surface cannot reinvent it wrong.
//!
//! **The id is the last fallback, never blank.** A dataset that does not carry something the
//! build names is a real state (a build carried across forks, rule 8's retain-and-report), and
//! `arachnos-soldier` is ugly where nothing at all looks broken.

use crate::shell::Db;
use coh_data::{CharacterState, Enhancement, EnhancementKind, PowerDatabase, PowersetSelection};

/// The archetype's name, or its id. `None` only when the build has no archetype at all.
pub fn archetype_name(build: &CharacterState, database: &PowerDatabase) -> Option<String> {
    let id = build.archetype.id.as_ref()?;
    Some(
        database
            .archetypes()
            .ok()
            .and_then(|catalog| catalog.get(id).map(|at| at.name.clone()))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| id.clone()),
    )
}

/// A primary/secondary powerset's name, or its id. `None` when the slot is unselected.
pub fn powerset_name(selection: &PowersetSelection, database: &PowerDatabase) -> Option<String> {
    let id = selection.id.as_ref()?;
    Some(
        database
            .find_powerset(id)
            .map(|powerset| powerset.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| id.clone()),
    )
}

/// A pool or epic pool's name, or its id.
///
/// Pools resolve through the pool catalog rather than [`PowerDatabase::find_powerset`]: a pool is
/// a partition of the dataset, not a powerset record, and the powersets list does not carry it.
pub fn pool_name(id: &str, database: &PowerDatabase) -> String {
    database
        .pool_catalog
        .find(id)
        .map(|pool| pool.name.clone())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| id.to_string())
}

/// A power's name, addressed the way a build addresses a pick: owning set plus internal name.
///
/// Falls back to the internal name rather than to a placeholder: an unresolvable power is still
/// a real contributor, and naming it badly beats dropping it or calling it "Unknown".
pub fn power_label(db: Option<&Db>, power_set: &str, internal_name: &str) -> String {
    db.and_then(|db| db.0.resolve_power(power_set, internal_name))
        .map(|power| power.name.clone())
        .unwrap_or_else(|| internal_name.to_string())
}

/// A slotted piece's own name — never blank.
///
/// The id is the fallback for the same reason it is everywhere else here: the codec keeps a
/// slot whose set this dataset does not carry (rule 8), and a nameless row reads as an empty
/// slot rather than as a piece the reader may well have.
pub fn enhancement_name(enhancement: &Enhancement) -> &str {
    if enhancement.name.is_empty() {
        enhancement.id.as_str()
    } else {
        enhancement.name.as_str()
    }
}

/// What distinguishes one copy of a piece from another copy of the same piece.
///
/// Shared, because two surfaces already print it — the forum post and the enhancement list —
/// and a build's post disagreeing with its own shopping list about a piece is the drift this
/// module exists to prevent.
///
/// **`boost` is spelled as the mechanic it IS, which depends on the kind.** The field carries a
/// booster combine on an IO and a signed relative level on an SO ([`Enhancement::boost`]), so a
/// bare number here would read as whichever one the reader had in mind.
pub fn enhancement_qualifiers(enhancement: &Enhancement) -> Vec<String> {
    let mut qualifiers: Vec<String> = Vec::new();
    if enhancement.attuned {
        qualifiers.push("attuned".to_string());
    } else if let Some(level) = enhancement.level {
        qualifiers.push(format!("level {}", level.get()));
    }
    match &enhancement.kind {
        EnhancementKind::IoSet { .. } | EnhancementKind::GenericIo { .. } => {
            if enhancement.boost > 0 {
                qualifiers.push(format!("+{} boosters", enhancement.boost));
            }
        }
        EnhancementKind::Special { .. } | EnhancementKind::Origin { .. } => {
            if enhancement.boost != 0 {
                qualifiers.push(format!("{:+} levels", enhancement.boost));
            }
        }
    }
    qualifiers
}
