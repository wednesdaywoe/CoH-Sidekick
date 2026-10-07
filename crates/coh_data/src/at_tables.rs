//! Archetype Modifier Tables — the per-AT, per-level scale multipliers a scaled effect
//! resolves against (`{ scale, table } → scale × table[level]`). The first data the
//! totals pipeline needs beyond the per-power appliers: M1/M2 pinned the raw
//! `{ scale, table }` shape; Pass 1 (strength) is the first pass to turn it into a number.
//!
//! Ported from the beta's per-dataset `getTableValue` (`datasets/<id>/at-tables.ts`),
//! which reads `at.tables[name][level-1]` with a chain of name normalizations. The lookup
//! (name-normalization + level clamp) is data access and lives here; the *calc* half
//! (`scale × value`, the `_ones` shortcut, the table-less 0.10 fallback) is `resolveScaledEffect`
//! in `coh_math` — the data/calc split of D2.
//!
//! Source: the contract's `at-tables.archetypes.<at>.tables.<name_lc> = f64[]` section
//! (level = index + 1; HC carries 105 entries, Rebirth/Thunderspy 50 — the clamp absorbs
//! the difference). Absent section ⇒ empty tables ⇒ every lookup misses (the calc-side
//! fallback then applies), so a manually-constructed `PowerDatabase` needs no table data.
//!
//! `at-tables.pets.<class>.tables` has the same shape and is parsed the same way: a pet is a
//! second character and its magnitudes resolve against its OWN class (see [`TableScope`]).

use serde_json::Value;
use std::collections::HashMap;

/// Which class's modifier tables a scaled effect resolves against.
///
/// A power's own rows resolve against the build's archetype. A pseudo-pet's rows resolve
/// against the PET's class: the client hands `power_AddEffects` the pet's
/// `characterClassName` as the class its magnitudes read (`uiPowerInfo.c`
/// `power_AddPetEffects` → `modGetMagnitudeAndDuration` → `class_GetNamedTableValue`), and
/// the summoner's class travels alongside as `creatorClass`, which is consulted only when
/// evaluating a template's `Requires`. Inheriting the summoner's SLOTTING — `CopyBoosts`, and
/// the `copyCreatorMods` that copies the creator's strength mods onto the pet
/// (`character_pet.c`) — is a different fact from resolving against the summoner's TABLES,
/// and reading the first as the second is what ENT-10 was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableScope<'a> {
    /// The build's archetype, spelled as `at-tables.archetypes` keys it.
    Archetype(&'a str),
    /// A pet's own character class, spelled as the pet entity's `characterClass` states it.
    Pet(&'a str),
}

/// One dataset's modifier tables: `class → table-name (lowercase) → values`, held once for
/// the player archetypes and once for the pet classes. Values are indexed by `level - 1`.
#[derive(Debug, Default, Clone)]
pub struct AtTables {
    archetypes: HashMap<String, HashMap<String, Vec<f64>>>,
    pets: HashMap<String, HashMap<String, Vec<f64>>>,
}

impl AtTables {
    /// Parse the contract's `at-tables` section
    /// (`{ archetypes: { <at>: { tables: {...} } }, pets: { <class>: { tables: {...} } } }`).
    ///
    /// Malformed ≠ absent: an ABSENT section degrades to empty tables
    /// (every lookup misses, the calc-side fallback applies — a manually-constructed
    /// `PowerDatabase` needs no table data), but a PRESENT section that doesn't parse is
    /// an error. Compacting a non-numeric entry would shift every later level's row —
    /// plausible wrong numbers shipped as authoritative.
    ///
    /// `pets` is optional where `archetypes` is required: a hand-built section for a test
    /// that never summons carries no pet classes, and a bundle that names none is a dataset
    /// with no pets rather than a broken one.
    pub fn from_section(section: Option<&Value>) -> Result<Self, String> {
        let Some(section) = section else {
            return Ok(AtTables::default());
        };
        let archetype_map = section
            .get("archetypes")
            .and_then(Value::as_object)
            .ok_or("at-tables section has no archetypes object")?;
        let archetypes = parse_class_tables(archetype_map, "archetype")?;

        let pets = match section.get("pets") {
            None | Some(Value::Null) => HashMap::new(),
            Some(pets) => {
                let map = pets
                    .as_object()
                    .ok_or("at-tables pets is not an object of classes")?;
                parse_class_tables(map, "pet class")?
            }
        };
        Ok(AtTables { archetypes, pets })
    }

    /// The modifier value for `(scope, table_name, level)` — the archetype tables or the pet
    /// tables, by what the row itself says it resolves against.
    pub fn value(&self, scope: TableScope<'_>, table_name: &str, level: i32) -> Option<f64> {
        match scope {
            TableScope::Archetype(archetype) => self.get_table_value(archetype, table_name, level),
            TableScope::Pet(class) => self.get_pet_table_value(class, table_name, level),
        }
    }

    /// The pet-class modifier value for `(pet_class, table_name, level)`, resolved by the same
    /// normalization as [`AtTables::get_table_value`] — the beta `getPetTableValue` reads the
    /// pet tables through the same name rules.
    pub fn get_pet_table_value(
        &self,
        pet_class: &str,
        table_name: &str,
        level: i32,
    ) -> Option<f64> {
        lookup(self.pets.get(pet_class)?, table_name, level)
    }

    /// The AT modifier value for `(archetype, table_name, level)`, or `None` if the
    /// archetype or table is absent. Ports the beta `getTableValue` normalization exactly:
    /// lowercase the name, then on a miss try (in order) stripping a `self`/`other`/`target`
    /// suffix, aliasing `_tempdamage`/`_incarnateprocdamage` → `_damage`, and `_dam` →
    /// `_dmg`. Level is 1-based and clamped into the table (`index = clamp(level-1, 0,
    /// len-1)`), mirroring the beta's `Math.max(0, Math.min(len-1, level-1))`.
    pub fn get_table_value(&self, archetype: &str, table_name: &str, level: i32) -> Option<f64> {
        lookup(self.archetypes.get(archetype)?, table_name, level)
    }
}

/// Parse one `{ <class>: { tables: { <name>: f64[] } } }` map. `kind` names the half being
/// read so a malformed entry says which one it came from.
fn parse_class_tables(
    classes: &serde_json::Map<String, Value>,
    kind: &str,
) -> Result<HashMap<String, HashMap<String, Vec<f64>>>, String> {
    let mut out: HashMap<String, HashMap<String, Vec<f64>>> = HashMap::new();
    for (class_id, class_val) in classes {
        let tables = class_val
            .get("tables")
            .and_then(Value::as_object)
            .ok_or_else(|| format!("at-tables {kind} {class_id:?} has no tables object"))?;
        let mut parsed: HashMap<String, Vec<f64>> = HashMap::with_capacity(tables.len());
        for (name, values) in tables {
            let arr = values
                .as_array()
                .ok_or_else(|| format!("at-tables {class_id}.{name} is not an array"))?;
            let vals = arr
                .iter()
                .map(|v| {
                    v.as_f64().ok_or_else(|| {
                        format!("at-tables {class_id}.{name}: non-numeric entry {v}")
                    })
                })
                .collect::<Result<Vec<f64>, String>>()?;
            // The name is already lowercase in the contract; lowercase defensively so
            // lookups (which lowercase the query) always match.
            parsed.insert(name.to_lowercase(), vals);
        }
        out.insert(class_id.clone(), parsed);
    }
    Ok(out)
}

/// One class's table lookup: the beta's name normalization, then the level clamp.
fn lookup(tables: &HashMap<String, Vec<f64>>, table_name: &str, level: i32) -> Option<f64> {
    let key = table_name.to_lowercase();

    let table = tables
        .get(&key)
        .or_else(|| {
            // Suffixed power-data names (e.g. "ranged_healself") map to base tables.
            let stripped = strip_target_suffix(&key);
            tables.get(stripped)
        })
        .or_else(|| {
            // Temp/incarnate damage tables alias to base damage.
            let aliased = key
                .replace("_tempdamage", "_damage")
                .replace("_incarnateprocdamage", "_damage");
            if aliased != key {
                tables.get(&aliased)
            } else {
                None
            }
        })
        .or_else(|| {
            // The game's "_dam" spelling aliases to the extracted "_dmg" key.
            if let Some(base) = key.strip_suffix("_dam") {
                tables.get(&format!("{base}_dmg"))
            } else {
                None
            }
        })?;

    if table.is_empty() {
        return None;
    }
    let last = table.len() as i32 - 1;
    let index = (level - 1).clamp(0, last) as usize;
    table.get(index).copied()
}

/// Strip a trailing `self`/`other`/`target` (the beta's `/self$|other$|target$/`) — a
/// single trailing occurrence, matching the JS regex without the `_` the caller might
/// expect (the beta strips the bare word, e.g. `ranged_healself` → `ranged_heal`).
fn strip_target_suffix(key: &str) -> &str {
    for suffix in ["self", "other", "target"] {
        if let Some(base) = key.strip_suffix(suffix) {
            return base;
        }
    }
    key
}
