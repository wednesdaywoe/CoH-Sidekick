//! The pool and epic-pool *aggregates* — the identity a picker lists, as distinct from the
//! powers inside them.
//!
//! [`PowerDatabase`](crate::PowerDatabase) flattens both sections into
//! [`PartitionPower`](crate::database::PartitionPower) vectors tagged by `set_id`, which is
//! all the calc ever needs: a power's owner disambiguates it. A *picker* needs the other
//! half — what the aggregate is called, what it says about itself, and which powers it
//! offers in wire order — so this reader keeps that half instead of dropping it on the
//! floor when the section is consumed.
//!
//! Membership is deliberately stored as idents rather than [`Power`](crate::Power) values:
//! the flat partitions already own the defs, and duplicating them here would be a second
//! copy to keep in sync (the identity/definition split `character.rs` documents).

use serde_json::Value;

/// One pool or epic-pool aggregate.
///
/// The shared fields are the whole of a standard pool. `min_level` is the epic sections'
/// `minLevel`, absent on standard pools — which take the schedule's
/// [`pool_unlock_level`](crate::LevelingSchedule::pool_unlock_level) instead.
///
/// There is deliberately no `archetype` field even though the epic wire carries one: that
/// tag holds a SINGLE archetype id while an epic pool can serve several (the Arachnos
/// masteries serve both VEATs), and the powers' own `requires` expressions carry the full
/// gate. Reading the lossy tag is how the beta ended up hand-patching the Widow back in;
/// [`crate::pick_rules`] evaluates the sourced gate instead.
#[derive(Debug, Clone, PartialEq)]
pub struct PoolDef {
    /// Aggregate id (`"speed"`, `"arctic_mastery"`) — the `set_id` its powers carry.
    pub id: String,
    /// The pool's own name in the binary, fully qualified (`"Pool.Manipulation"`).
    ///
    /// Most pool ids already ARE the binary name, which is why the divergence hid: only
    /// Presence is keyed off its display name while every gate spells it `Pool.Manipulation`.
    /// [`crate::pick_rules`] matches set paths against this, never against the id.
    pub set_path: Option<String>,
    /// `SetBuyRequires` + `SetBuyRequiresFailedText` — the set-level gate and its refusal
    /// message. This is where "you can only have one Specialized power pool" lives: the five
    /// specialized pools each list the others' powers. Evaluated by
    /// [`crate::pick_rules::set_gate`].
    pub buy_requires: Vec<String>,
    pub buy_requires_failed: String,
    /// `SpecializeAt` + `SpecializeRequires` — Pool.Fitness is the only pool that branches.
    /// Zero marks a set as not a specialization set at all, and a non-zero value is 0-based:
    /// see [`crate::Powerset::specialize_at`], which documents both bases in play.
    pub specialize_at: u8,
    pub specialize_requires: Vec<String>,
    /// Display name (the wire's `displayName`, falling back to `name`).
    pub name: String,
    pub description: String,
    /// Icon filename, display only.
    pub icon: String,
    /// The epic wire's `minLevel`; `None` on standard pools.
    pub min_level: Option<u8>,
    /// Whether this aggregate is dormant — present in the bins but not released on this
    /// server. Dormant pools are dropped from the catalog at load (see `is_dormant`), so this
    /// field is only ever read during that filter and is `None` on the epic wire, which carries
    /// no such flag. Kept explicit rather than inferred so a future reader can tell a released
    /// pool from a dormant one without re-reading the raw wire.
    pub dormant: Option<bool>,
    /// The aggregate's powers by [`Power::ident`](crate::Power::ident), in wire order.
    pub power_idents: Vec<String>,
}

/// Both aggregate registries, in the order the picker lists them (name-ascending, which is
/// the only order the wire's object keys imply anything about).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PoolCatalog {
    pub pools: Vec<PoolDef>,
    pub epics: Vec<PoolDef>,
}

impl PoolCatalog {
    /// Read both aggregate registries from the raw `power-pools` / `epic-pools` sections.
    ///
    /// Called during the bundle load, BEFORE the sections are flattened into the partition
    /// vectors — the loader removes them from the retained `sections` map, so a lazy reader
    /// like [`PowerDatabase::archetypes`](crate::PowerDatabase::archetypes) has nothing left
    /// to read. Malformed is an error, never a silent empty catalog (Rule 1): an empty
    /// picker looks like a dataset with no pools rather than a decode fault.
    pub fn from_sections(pools: &Value, epics: &Value) -> Result<Self, String> {
        Ok(PoolCatalog {
            pools: read_registry(pools, "power-pools")?,
            epics: read_registry(epics, "epic-pools")?,
        })
    }

    /// The aggregate with this id, from either registry.
    pub fn find(&self, id: &str) -> Option<&PoolDef> {
        self.pools
            .iter()
            .chain(self.epics.iter())
            .find(|pool| pool.id == id)
    }
}

fn read_registry(section: &Value, label: &str) -> Result<Vec<PoolDef>, String> {
    let Value::Object(registry) = section else {
        return Err(format!("{label} section is not an object"));
    };
    let mut defs: Vec<PoolDef> = registry
        .iter()
        .filter(|(_, value)| !is_dormant(value))
        .map(|(key, value)| read_pool(key, value, label))
        .collect::<Result<_, _>>()?;
    defs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(defs)
}

/// A pool the server hasn't released carries `dormant: true` — its powers sit behind a
/// dev-only `accesslevel > 0` gate the client can't use. The game hides such a set, so the
/// catalog drops it too: a picker that listed Gadgetry or Utility Belt on Homecoming would
// offer pools the build can never take. Absent means released (the epic wire carries no flag),
// so only an explicit `true` filters, and a released pool with no flag survives.
fn is_dormant(value: &Value) -> bool {
    value.get("dormant").and_then(Value::as_bool) == Some(true)
}

fn read_pool(key: &str, value: &Value, label: &str) -> Result<PoolDef, String> {
    let Value::Object(map) = value else {
        return Err(format!("{label} entry {key:?} is not an object"));
    };
    let string = |field: &str| {
        map.get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let name = match map.get("displayName").and_then(Value::as_str) {
        Some(display) if !display.is_empty() => display.to_string(),
        _ => string("name"),
    };
    let powers = map
        .get("powers")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{label} entry {key:?} has no powers array"))?;

    Ok(PoolDef {
        id: map
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or(key)
            .to_string(),
        set_path: map
            .get("setPath")
            .and_then(Value::as_str)
            .map(str::to_string),
        buy_requires: string_list(map, "buyRequires"),
        buy_requires_failed: string("buyRequiresFailed"),
        specialize_at: map.get("specializeAt").and_then(Value::as_u64).unwrap_or(0) as u8,
        specialize_requires: string_list(map, "specializeRequires"),
        name,
        description: string("description"),
        icon: string("icon"),
        min_level: map
            .get("minLevel")
            .and_then(Value::as_u64)
            .map(|level| level as u8),
        dormant: map.get("dormant").and_then(Value::as_bool),
        power_idents: powers.iter().map(power_ident).collect(),
    })
}

fn string_list(map: &serde_json::Map<String, Value>, field: &str) -> Vec<String> {
    map.get(field)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// A pool power's [`Power::ident`](crate::Power::ident), derived the same way the loader's
/// `normalize_legacy_power` does: these sections reach the bundle in the converter's legacy
/// shape, where the identity lives only in the dotted `fullName`. Deriving it here rather
/// than re-parsing the whole power keeps the catalog to metadata.
fn power_ident(power: &Value) -> String {
    if let Some(internal) = power.get("internalName").and_then(Value::as_str) {
        return internal.to_string();
    }
    if let Some(full) = power.get("fullName").and_then(Value::as_str) {
        if let Some(last) = full.rsplit('.').next() {
            return last.to_string();
        }
    }
    power
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}
