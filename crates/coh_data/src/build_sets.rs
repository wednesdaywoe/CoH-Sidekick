//! Which of a build's five buckets a powerset belongs in, and how a binary set path finds it.
//!
//! Every importer has the same two questions to answer before it can place a pick: what is
//! this dataset's id for the set the file names, and which bucket of a build does that set
//! belong in. Both are read off the dataset — the set path a powerset states, and the
//! catalogue that carries the resolved id — never off the file's own category token, which
//! names the GAME's grouping rather than this build's (`Inherent` for Fitness, `Epic` for a
//! set the dataset files by archetype).
//!
//! Shared because the two readers that ask are the two that must agree: a `/buildsave` and a
//! `.mbd` describing the same character have to place its picks in the same buckets, and a
//! second copy of this rule is a place they can drift.

use crate::pick_rules::normalize;
use crate::{PowerDatabase, INHERENT_SET};

/// Where a power belongs in a build.
pub(crate) enum Bucket {
    Primary,
    Secondary,
    Pool(String),
    Epic(String),
    Inherent,
}

/// The archetype's own primary and secondary set ids, in the comparison spelling.
///
/// Branch sets are folded in beside the role they specialise, because a VEAT branch pick lives
/// in the base role's list and nothing else declares the branch was taken. The shape follows
/// [`crate::skif::hydrate`], which had the same problem first.
pub(crate) struct Roles {
    pub(crate) primary: Vec<String>,
    pub(crate) secondary: Vec<String>,
}

impl Roles {
    pub(crate) fn of(archetype_id: &str, database: &PowerDatabase) -> Option<Roles> {
        let catalog = database.archetypes().ok()?;
        let archetype = catalog
            .all()
            .iter()
            .find(|candidate| candidate.id == archetype_id)?;
        let mut primary: Vec<String> = archetype
            .primary_sets
            .iter()
            .map(|s| normalize(s))
            .collect();
        let mut secondary: Vec<String> = archetype
            .secondary_sets
            .iter()
            .map(|s| normalize(s))
            .collect();
        for branch in &archetype.branches {
            primary.extend(branch.primary_set.iter().map(|s| normalize(s)));
            secondary.extend(branch.secondary_set.iter().map(|s| normalize(s)));
        }
        Some(Roles { primary, secondary })
    }
}

/// Binary set path -> build id, for every set a build can hold.
///
/// The same records [`crate::pick_rules::SetPaths`] indexes, read the other way round and
/// keeping the id in its AUTHORED spelling. That index folds both halves to a comparison
/// spelling, which is right for matching a gate and wrong here: `tanker/dark_armor` is what the
/// fold produces and `tanker/dark-armor` is what a build stores, so handing the folded one to
/// `hydrate` resolves nothing. Only the path half is folded, because only it is matched against.
pub(crate) struct SetLookup {
    by_path: Vec<(String, String)>,
}

impl SetLookup {
    pub(crate) fn of(database: &PowerDatabase) -> SetLookup {
        let powersets = database
            .powersets
            .iter()
            .filter_map(|set| Some((normalize(set.set_path.as_ref()?), set.id.clone())));
        let pools = database
            .pool_catalog
            .pools
            .iter()
            .chain(database.pool_catalog.epics.iter())
            .filter_map(|pool| Some((normalize(pool.set_path.as_ref()?), pool.id.clone())));
        SetLookup {
            by_path: powersets.chain(pools).collect(),
        }
    }

    /// The build id for the set path `category.powerset` names.
    ///
    /// The exact path answers 215 of the `/buildsave` corpus's 222 power lines. The rest are
    /// Fitness, which the game exports under `Inherent` while the dataset models it as
    /// `Pool.Fitness` — the two disagree about the category, not about the set. So a miss falls
    /// back to the set-name half, and ONLY when exactly one record claims that name: 85 of the
    /// dataset's 234 set names are claimed by several categories at once (every blast set is
    /// four archetypes'), and picking one of those would slot another archetype's powerset
    /// without saying so.
    pub(crate) fn resolve(&self, category: &str, powerset: &str) -> Option<&str> {
        self.resolve_path(&format!("{category}.{powerset}"))
    }

    /// The same lookup for a path already spelled as one string, which is how a `.mbd` states
    /// it. The tail fallback is the `resolve` doc's, unchanged — a path with no `.` claims
    /// nothing, since the tail half compares against the segment after one.
    pub(crate) fn resolve_path(&self, path: &str) -> Option<&str> {
        let exact = normalize(path);
        if let Some((_, id)) = self.by_path.iter().find(|(path, _)| *path == exact) {
            return Some(id);
        }
        let tail = exact.split_once('.').map(|(_, name)| name.to_string())?;
        let mut claimants = self
            .by_path
            .iter()
            .filter(|(path, _)| path.split_once('.').is_some_and(|(_, name)| name == tail));
        let only = claimants.next()?;
        claimants.next().is_none().then_some(only.1.as_str())
    }
}

/// Which of a build's buckets the set `id` names belongs in.
///
/// Decided by where the resolved set is found, never by any token the file spells: the
/// catalogue that carries it is the dataset's own statement of what kind of set it is.
pub(crate) fn bucket_of(
    id: &str,
    roles: Option<&Roles>,
    database: &PowerDatabase,
) -> Option<Bucket> {
    let folded = normalize(id);
    if folded == normalize(INHERENT_SET) {
        return Some(Bucket::Inherent);
    }
    if database
        .pool_catalog
        .pools
        .iter()
        .any(|pool| normalize(&pool.id) == folded)
    {
        return Some(Bucket::Pool(id.to_string()));
    }
    if database
        .pool_catalog
        .epics
        .iter()
        .any(|epic| normalize(&epic.id) == folded)
    {
        return Some(Bucket::Epic(id.to_string()));
    }
    let roles = roles?;
    if roles.primary.contains(&folded) {
        return Some(Bucket::Primary);
    }
    roles
        .secondary
        .contains(&folded)
        .then_some(Bucket::Secondary)
}

/// Every power a set offers, whichever partition holds it.
///
/// A powerset keeps its own list; a pool or an epic pool keeps its powers in the partition
/// tables, one row per (set, power). Anything asking "what does this set hold" has to ask both,
/// and asking only the first is how a pool power reads as a set this dataset does not carry.
pub(crate) fn set_powers<'a>(database: &'a PowerDatabase, set_id: &str) -> Vec<&'a crate::Power> {
    if let Some(powerset) = database.find_powerset(set_id) {
        return powerset.powers.iter().collect();
    }
    database
        .pool_powers
        .iter()
        .chain(database.epic_powers.iter())
        .filter(|entry| entry.set_id == set_id)
        .map(|entry| &entry.power)
        .collect()
}

/// The archetype id whose class token a file states — `Class_Blaster` → `blaster`.
///
/// Resolved through the dataset's own `className` rather than by stripping `Class_` and
/// lower-casing: the token is a field of the archetype record, so asking is one lookup and
/// re-spelling is a rule that has to keep being right. Shared by both importers, which state
/// a miss in their own words because their note types differ.
pub(crate) fn archetype_for_class(class: &str, database: &PowerDatabase) -> Option<String> {
    let catalog = database.archetypes().ok()?;
    catalog
        .all()
        .iter()
        .find(|archetype| database.class_name_of(&archetype.id) == Some(class))
        .map(|archetype| archetype.id.clone())
}
