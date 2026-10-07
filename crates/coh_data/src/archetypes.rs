//! Archetype catalog — the picker's data floor (M3 step 12a). Ported (shape, not name)
//! from the beta `archetypes` dataset section: `{ archetypes: { <id>: { name, side,
//! description, inherent, primarySets, secondarySets, stats } }, standardArchetypeIds,
//! epicArchetypeIds }`.
//!
//! This is a UI-facing catalog: which archetypes exist, in canonical order, and which
//! primary/secondary powersets each offers. The per-AT CAP tables (`resistanceCap`, HP
//! tables) that the calc clamps against live in the separate `archetype-stats` section
//! ([`crate::archetype_stats`]); this reader deliberately ignores the duplicated `stats`
//! block so there is one owner of caps.
//!
//! Parsed lazily from the retained `sections` bag ([`crate::PowerDatabase::archetypes`])
//! rather than eagerly into a `PowerDatabase` field: the selection UI is the only consumer,
//! so it memoizes one parse, and the calc-path construction sites stay untouched. Absent
//! section → an empty catalog (a hand-built database has no archetypes); a present section
//! that fails to decode, or that lists an id it does not define, is an error (Rule 1 — a
//! dropped archetype would silently vanish from the picker).

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

/// One archetype's picker-relevant identity: its name, lore, inherent, and the primary /
/// secondary powerset ids it can draw from.
#[derive(Debug, Clone, PartialEq)]
pub struct Archetype {
    /// Archetype id (the section-map key, e.g. `"blaster"`) — injected from the key, not a
    /// field of the wire value.
    pub id: String,
    /// Display name (`"Blaster"`).
    pub name: String,
    /// `"hero"` / `"villain"` / `"rogue"` — the alignment the AT belongs to.
    pub side: String,
    /// One-line lore blurb for the picker.
    pub description: String,
    /// The archetype inherent (Defiance, Fury, …) — name + description only; its calc
    /// contribution comes from the `Inherent` powerset via Pass 3, not from here.
    pub inherent: ArchetypeInherent,
    /// Primary powerset ids (e.g. `"blaster/fire-blast"`) — resolved to [`crate::Powerset`]s
    /// via [`crate::PowerDatabase`] when the picker renders them.
    pub primary_sets: Vec<String>,
    /// Secondary powerset ids.
    pub secondary_sets: Vec<String>,
    /// The VEAT branch specialisations, by branch id — Arachnos Soldier's Crab/Bane, Widow's
    /// Fortunata/Night Widow. Each names the powerset pair the specialisation adds.
    ///
    /// The branch sets are in neither [`Self::primary_sets`] nor [`Self::secondary_sets`], so
    /// this is the only structure naming them and anything resolving a VEAT's powers has to
    /// read it: a scoped lookup against the base set misses every one of them. A branch pick
    /// lives in the base ROLE's list, carrying the branch set's own id — which is also the
    /// only record that the branch was taken (beta-authored files instead filed it under the
    /// base set id, which `crate::skif` still resolves). Empty for the 20-odd archetypes that
    /// have no branches.
    pub branches: Vec<ArchetypeBranch>,
}

/// One VEAT branch — the powerset pair a specialisation swaps in.
///
/// No unlock level: each of the pair's powersets carries `SpecializeAt` from the binary and
/// the character level is one higher ([`crate::Powerset::specialize_at`]), so anything that
/// needs the level asks [`crate::set_gate`], which reads the record rather than a copy.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchetypeBranch {
    /// Branch id (the wire key), injected from it rather than read as a field.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub primary_set: Option<String>,
    #[serde(default)]
    pub secondary_set: Option<String>,
}

/// An archetype inherent's display fields (`inherent.ts` shape). Calc-inert here.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct ArchetypeInherent {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// The dataset's archetypes in canonical order (standard ATs first, then the epic/VEAT
/// ones), plus the epic-id set so the picker can group them.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Archetypes {
    ordered: Vec<Archetype>,
    epic_ids: HashSet<String>,
}

#[derive(Debug, Deserialize)]
struct ArchetypesWire {
    #[serde(default)]
    archetypes: serde_json::Map<String, Value>,
    #[serde(default, rename = "standardArchetypeIds")]
    standard_ids: Vec<String>,
    #[serde(default, rename = "epicArchetypeIds")]
    epic_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ArchetypeWire {
    name: String,
    #[serde(default)]
    side: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    inherent: ArchetypeInherent,
    #[serde(default, rename = "primarySets")]
    primary_sets: Vec<String>,
    #[serde(default, rename = "secondarySets")]
    secondary_sets: Vec<String>,
    #[serde(default)]
    branches: serde_json::Map<String, Value>,
}

impl Archetypes {
    /// Parse the `archetypes` section. Absent → empty (a hand-built database). The display
    /// order is `standardArchetypeIds` then `epicArchetypeIds`; if those lists are empty
    /// (an older/partial section) it falls back to the map's own key order. An id listed in
    /// either order list but absent from the `archetypes` map is a malformed section —
    /// an error, not a silently dropped archetype.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(Archetypes::default());
        };
        let wire: ArchetypesWire = serde_json::from_value(section.clone())
            .map_err(|e| format!("archetypes section: {e}"))?;

        let epic_ids: HashSet<String> = wire.epic_ids.iter().cloned().collect();

        // Canonical order: standard first, then epic. Fall back to map key order when the
        // section carries no order lists.
        let order: Vec<String> = if wire.standard_ids.is_empty() && wire.epic_ids.is_empty() {
            wire.archetypes.keys().cloned().collect()
        } else {
            wire.standard_ids
                .iter()
                .chain(wire.epic_ids.iter())
                .cloned()
                .collect()
        };

        let mut ordered = Vec::with_capacity(order.len());
        for id in order {
            let value = wire
                .archetypes
                .get(&id)
                .ok_or_else(|| format!("archetype id {id:?} is listed but not defined"))?;
            let at: ArchetypeWire = serde_json::from_value(value.clone())
                .map_err(|e| format!("archetype {id:?}: {e}"))?;
            let mut branches = Vec::with_capacity(at.branches.len());
            for (branch_id, value) in at.branches {
                let mut branch: ArchetypeBranch = serde_json::from_value(value)
                    .map_err(|e| format!("archetype {id:?} branch {branch_id:?}: {e}"))?;
                branch.id = branch_id;
                branches.push(branch);
            }
            ordered.push(Archetype {
                id,
                name: at.name,
                side: at.side,
                description: at.description,
                inherent: at.inherent,
                primary_sets: at.primary_sets,
                secondary_sets: at.secondary_sets,
                branches,
            });
        }
        Ok(Archetypes { ordered, epic_ids })
    }

    /// Every archetype in canonical (standard-then-epic) order.
    pub fn all(&self) -> &[Archetype] {
        &self.ordered
    }

    /// The archetype with this id, or `None`.
    pub fn get(&self, id: &str) -> Option<&Archetype> {
        self.ordered.iter().find(|a| a.id == id)
    }

    /// Whether `id` is an epic/VEAT archetype (Peacebringer, Warshade, the Arachnos VEATs).
    pub fn is_epic(&self, id: &str) -> bool {
        self.epic_ids.contains(id)
    }
}
