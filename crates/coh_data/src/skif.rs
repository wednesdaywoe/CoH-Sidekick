//! `.skif` v5 — the native build file: [`encode`] a [`CharacterState`] to it, [`decode`] one
//! back. This module implements the approved v5 schema and restates only what a reader of
//! the code needs.
//!
//! **Two zones, opposite failure rules** (draft rule 2). The STRUCTURAL zone — the envelope,
//! the version, the container shape, slot positions, the four enhancement `type` tags — is
//! owned by this schema, and an unknown value there is refused, because misreading structure
//! produces numbers rather than gaps. The VOCABULARY zone — dataset ids, powerset and power
//! ids, set ids, special categories, conditional ids, mode tokens, proc categories, incarnate
//! slot ids — is owned by the export, and an unknown value there is preserved verbatim and
//! REPORTED ([`Decoded::unresolved`]), never dropped and never applied. A build authored
//! against a newer dataset is not corruption.
//!
//! One vocabulary key is TRANSLATED rather than merely preserved, and the distinction is worth
//! keeping straight: a conditional id whose gate compares a stack count carries that count in
//! its id, and the threshold spelling of it changed once (COND-3). [`migrate_stances`] maps the
//! old key onto the new ones and reports having done so. It applies a value under a key the
//! file did not write, which nothing else here does, so it is confined to a respelling whose
//! inverse is derivable from the id alone — never a guess about what a key might have meant.
//!
//! **What travels** (draft rule 5): power choices, slots, enhancements, accolades, incarnates,
//! and *the state of what is running* — toggles, stances, forms, incarnate actives, the
//! caster-side per-power opt-ins, the proc switches. What does not: every reading condition
//! (Fury, team size, current HP, enemy level, exemplar level, Destiny scrub, from-Hide, the
//! chosen target, targets-hit, target-state conditionals). Those open at the planner's
//! default, which is what gives the file exactly one reading.
//!
//! **What is never stored** (draft rule 6): anything the reader can compute. Set-bonus
//! tallies, accolade values, granted/locked flags, inherent categories, inherent slot counts.
//! A stored copy can only agree with the computation or contradict it.
//!
//! The one exception worth naming: [`SelectedPower::inherent_slot_count`] is *not* in the file
//! but IS re-derived here, from the same [`crate::auto_granted_slot_count`] the grant pass
//! uses — because [`crate::granted_inherents`] separates user-placed slots from granted ones
//! by subtracting it, so a build restored with it at zero would gain a slot on every load.

use crate::character::{
    power_address, ArchetypeSelection, AttackChain, CharacterState, Enhancement, EnhancementKind,
    IncarnateSlot, PoolSelection, PowersetSelection, ProcOverride, SelectedPower, SlotLevelSource,
    SlotOrderEntry,
};
use crate::database::{DatasetId, PowerDatabase};
use crate::level::Level;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

pub mod legacy;

/// The only version this codec reads or writes. v4 falls through an unrecognized version into
/// a legacy full-`Build` path and mis-parses; v5 gets no tolerance at all (draft envelope).
pub const VERSION: u32 = 5;

/// The `version` a file declares, or `None` if it declares none — the one question a caller may
/// ask before committing to a reader.
///
/// This exists so routing between [`decode`] and [`legacy::decode_legacy`] is an explicit
/// decision made on a value, rather than a reader quietly widening to accept what it was handed.
/// That widening is the failure v4 shipped.
pub fn probe_version(text: &str) -> Option<u32> {
    let raw: Map<String, Value> = serde_json::from_str(text).ok()?;
    raw.get("version")
        .and_then(Value::as_u64)
        .map(|version| version as u32)
}

/// The fork a file's powers must be resolved against, or `None` if no loaded fork answers to
/// what it names.
///
/// The second question a caller may ask before committing to a reader, and it exists because
/// both readers take a [`PowerDatabase`] and neither can choose one: handed the wrong fork,
/// every powerset, power and set piece in the file resolves against definitions that do not
/// carry it, and rule 8's retain-and-report turns a perfectly good build into a page of
/// unresolved entries. An app holding one fork at a time has to load this one FIRST.
///
/// `None` is not "use the fork you have" — it means the file names a fork this build has never
/// heard of, and no switch can help. The caller hands it to a reader anyway and gets
/// [`SkifError::UnknownDataset`], because the refusal belongs to the decoder that can state it
/// precisely, not to a probe.
///
/// The version decides where to look, exactly as it does for the readers: v5 spells it
/// `build.dataset`, the legacy tier `build.serverId`, and a version that predates the field at
/// all takes [`legacy::ASSUMED_DATASET`] — read from there rather than restated, so the
/// assumption keeps one home.
pub fn probe_dataset(text: &str) -> Option<DatasetId> {
    let raw: Map<String, Value> = serde_json::from_str(text).ok()?;
    let version = raw.get("version").and_then(Value::as_u64)? as u32;
    let build = raw.get("build")?.as_object()?;
    let named = match version {
        VERSION => build.get("dataset").and_then(Value::as_str)?,
        _ if legacy::LEGACY_VERSIONS.contains(&version) => build
            .get("serverId")
            .and_then(Value::as_str)
            .unwrap_or(legacy::ASSUMED_DATASET),
        _ => return None,
    };
    DatasetId::ALL.into_iter().find(|id| id.as_str() == named)
}

// ============================================================
// The file.
// ============================================================

/// A `.skif` v5 file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkifFile {
    pub version: u32,
    /// What the build was authored against — the stamps that turn an unrecognized vocabulary
    /// key from a mystery into a diagnosis (draft rule 4). Absent = unstamped, which reads.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authored_against: Option<AuthoredAgainst>,
    /// Free-form and never read. Declared so a writer's annotations round-trip.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Value>,
    pub build: SkifBuild,
}

/// The dataset stamps ([`PowerDatabase::manifest`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthoredAgainst {
    pub dataset: String,
    pub contract_schema: u32,
    pub dataset_schema: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub export_commit: Option<String>,
}

/// The build itself. Every map is `BTreeMap`, so a file's key order is a function of the
/// build and not of iteration — two encodes of one build are the same bytes.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkifBuild {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    pub dataset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archetype: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    pub level: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<SkifSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<SkifSelection>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pools: Vec<SkifSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic_pool: Option<SkifSelection>,
    /// Sparse: only inherents the user has slotted or placed slots on. The roster itself is
    /// the dataset's (rule 6).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inherents: Vec<SkifPower>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub accolades: Vec<String>,
    /// Keyed by slot id, not seven named fields — so a fork's next slot needs no schema
    /// (draft rule 3, and `genesis` is the counter-example that earned it).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub incarnates: BTreeMap<String, SkifIncarnate>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slot_order: Vec<SkifSlotOrder>,
    /// Caster-wide conditional state — stances, Domination, ammo modes. Absent ⇒ the entry's
    /// own `defaultActive`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stances: BTreeMap<String, bool>,
    /// Caster modes (Kheldian forms). A map rather than v4's array so an explicitly-off mode
    /// is expressible.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub modes: BTreeMap<String, bool>,
    /// Per-power caster-side opt-ins, keyed `powerset:internalName:id`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub power_state: BTreeMap<String, bool>,
    /// Per-slotted-piece proc overrides, keyed `powerset:internalName:slotIndex`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub proc_overrides: BTreeMap<String, ProcOverride>,
    /// Proc categories the build switched OFF. Travels because it is a switch thrown on the
    /// character that gates real contributions in every proc pass — a reader without it reads
    /// HIGHER totals than the author authored.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_proc_categories: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attack_chains: Vec<AttackChain>,
    /// Declared so it can be REFUSED. A file carrying a what-if layer asserts capability the
    /// build does not have, so it is invalid rather than merely ignored — the one reading
    /// condition that fails a file instead of defaulting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub what_if_buffs: Option<Value>,
}

impl SkifBuild {
    /// Every power the file holds, across all five buckets. The buckets differ in how they
    /// RESOLVE, not in what a power is, so anything asking a question about the picks
    /// themselves asks it here.
    pub fn selections(&self) -> impl Iterator<Item = &SkifPower> {
        self.primary
            .iter()
            .chain(self.secondary.iter())
            .chain(self.pools.iter())
            .chain(self.epic_pool.iter())
            .flat_map(|selection| selection.powers.iter())
            .chain(self.inherents.iter())
    }
}

/// A powerset / pool selection. The v4 display `name` is dropped — it re-resolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkifSelection {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub powers: Vec<SkifPower>,
}

/// One picked power. `isLocked`, `inherentCategory`, `inherentSlotCount` and `targetsHit` are
/// absent by rules 5 and 6 — the first three re-derive from the dataset, the fourth is a claim
/// about the world rather than about the build.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkifPower {
    pub internal_name: String,
    /// The set this pick belongs to, written ONLY when it is not the set its list names.
    ///
    /// One case needs it and the format could not state it before: a VEAT branch pick lives in
    /// the base role's list under the BRANCH set's id, and that id is the only record that the
    /// branch was taken (nothing declares a branch — the game confers the set when a power is
    /// bought out of it). Without this the pick reloads as a base-set pick, the branch gate
    /// stops seeing it, and both branches are offered again to a build that has chosen.
    ///
    /// Absent means "the list's set", which is every other pick in every file — so nothing
    /// that was written before this existed changes shape, and re-encoding one is byte-stable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub powerset: Option<String>,
    /// The level the power was taken at; `0` ⇒ granted, so no pick level to record.
    pub level: u8,
    /// Positional, base slot at index 0, `null` = empty. No length cap in the format — the
    /// power's own `maxSlots` and the schedule's budget own it (rule 1).
    pub slots: Vec<Option<SkifEnhancement>>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_sub_power: Option<String>,
}

/// One incarnate pick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkifIncarnate {
    pub power: String,
    pub active: bool,
}

/// One slot-addition record for leveling mode.
///
/// A mirror of [`SlotOrderEntry`] rather than a reuse of it: that type is also the
/// `localStorage` shape, where the field names are Rust's, and a file a person may read or
/// hand-edit should not carry two naming conventions because of where a struct happens to
/// live.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkifSlotOrder {
    pub power_name: String,
    pub slot_index: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// Where `level` came from (MBDEXPORT-21). Absent on every v4 file and on any v5 written
    /// before the field existed, which reads as unstated — see [`SlotOrderEntry::level_source`]
    /// for why that is carried rather than defaulted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_source: Option<SlotLevelSource>,
}

impl From<&SlotOrderEntry> for SkifSlotOrder {
    fn from(entry: &SlotOrderEntry) -> Self {
        SkifSlotOrder {
            power_name: entry.power_name.clone(),
            slot_index: entry.slot_index,
            category: entry.category.clone(),
            level: entry.level,
            level_source: entry.level_source,
        }
    }
}

impl From<SkifSlotOrder> for SlotOrderEntry {
    fn from(entry: SkifSlotOrder) -> Self {
        SlotOrderEntry {
            power_name: entry.power_name,
            slot_index: entry.slot_index,
            category: entry.category,
            level: entry.level,
            level_source: entry.level_source,
        }
    }
}

/// A slotted enhancement, by kind. `type` is the one closed enum below the envelope — an
/// unknown tag is structural and refuses the file.
///
/// The substantive change from v4 is splitting its `boost`, which carried two different game
/// mechanics on two different curves in one field: `booster` is the Enhancement Booster
/// combine (unsigned), `relativeLevel` is the piece's level minus the character's (SIGNED,
/// and the negative half is real — an out-levelled SO is weaker).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum SkifEnhancement {
    #[serde(rename = "io-set")]
    IoSet {
        set_id: String,
        piece_num: u8,
        #[serde(default, skip_serializing_if = "is_false")]
        attuned: bool,
        /// Absent = defer to the reader's global IO level. `0` is not a second spelling of
        /// that — [`Level`] rejects it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Level>,
        #[serde(default, skip_serializing_if = "is_zero")]
        booster: u8,
    },
    #[serde(rename = "io-generic")]
    IoGeneric {
        stat: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        level: Option<Level>,
        #[serde(default, skip_serializing_if = "is_zero")]
        booster: u8,
    },
    Special {
        category: String,
        id: String,
        #[serde(default, skip_serializing_if = "is_zero_i8")]
        relative_level: i8,
    },
    Origin {
        stat: String,
        tier: String,
        #[serde(default, skip_serializing_if = "is_zero_i8")]
        relative_level: i8,
    },
}

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_zero(value: &u8) -> bool {
    *value == 0
}
fn is_zero_i8(value: &i8) -> bool {
    *value == 0
}

// ============================================================
// Failure.
// ============================================================

/// A refusal. Every arm is a STRUCTURAL problem — vocabulary the reader does not know becomes
/// an [`Unresolved`] entry instead, because the build is still readable without it.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SkifError {
    #[error("not a .skif file: {0}")]
    Malformed(String),
    #[error("unsupported .skif version {found:?} (this reader writes and reads v{VERSION})")]
    Version { found: Option<u32> },
    #[error("stamped as authored against {stamped:?} but the build declares {build:?}")]
    DatasetDisagrees { stamped: String, build: String },
    #[error("no loaded dataset answers to {0:?}")]
    UnknownDataset(String),
    #[error(
        "the file carries a what-if buff layer, which asserts capability the build does not have"
    )]
    WhatIfLayer,
    #[error("enhancement booster is negative ({0}) — the booster axis is unsigned; a signed level offset belongs on relativeLevel")]
    NegativeBooster(i8),
}

/// One thing the file named that this dataset does not carry. Mirrors the two fields of the
/// calc's own `CalcError` (`context` = where to look, `detail` = what could not be resolved),
/// because rule 8 puts an unresolved pick on the same fail-loud channel the calc uses —
/// `coh_math` sits above this crate, so the shape is mirrored rather than imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub context: String,
    pub detail: String,
}

/// A decoded file: the build, plus everything in it this dataset could not resolve.
///
/// The entries are RETAINED in the build, never dropped (rule 8) — v4 shrinks a build on
/// import, which is the worst available outcome because the number that comes out is
/// confidently wrong. A retained-but-unresolved enhancement carries its wire identity and an
/// empty definition, so it contributes nothing and re-exports unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct Decoded {
    pub build: CharacterState,
    pub unresolved: Vec<Unresolved>,
}

// ============================================================
// Encode.
// ============================================================

/// Write a build as a `.skif` v5 file, stamped with what `database` is.
pub fn encode(build: &CharacterState, database: &PowerDatabase) -> Result<String, SkifError> {
    let file = SkifFile {
        version: VERSION,
        authored_against: Some(AuthoredAgainst {
            dataset: build.dataset.as_str().to_string(),
            contract_schema: crate::database::SUPPORTED_SCHEMA,
            dataset_schema: database.manifest.schema,
            export_commit: database
                .manifest
                .provenance
                .get("betaCommit")
                .and_then(Value::as_str)
                .map(str::to_string),
        }),
        meta: None,
        build: slim(build)?,
    };
    serde_json::to_string_pretty(&file).map_err(|e| SkifError::Malformed(e.to_string()))
}

/// The build's file form, with no dataset stamps. Split out so the round-trip property under
/// test is the BUILD's, independent of which database was resident when it was written.
pub fn slim(build: &CharacterState) -> Result<SkifBuild, SkifError> {
    Ok(SkifBuild {
        name: build.name.clone(),
        dataset: build.dataset.as_str().to_string(),
        archetype: build.archetype.id.clone(),
        origin: build.origin.clone(),
        level: build.level,
        primary: slim_powerset(&build.primary)?,
        secondary: slim_powerset(&build.secondary)?,
        pools: build
            .pools
            .iter()
            .map(slim_pool)
            .collect::<Result<_, _>>()?,
        epic_pool: build.epic_pool.as_ref().map(slim_pool).transpose()?,
        inherents: build
            .inherents
            .iter()
            .filter(is_modified_inherent)
            // An inherent carries the synthetic INHERENT_SET, which is also the list it is
            // read back into, so none of them ever states a set of its own.
            .map(|power| slim_power(power, crate::INHERENT_SET))
            .collect::<Result<_, _>>()?,
        accolades: build.accolades.clone(),
        incarnates: slim_incarnates(build),
        slot_order: build.slot_order.iter().map(SkifSlotOrder::from).collect(),
        stances: build.combat.global_conditionals.clone(),
        modes: build
            .combat
            .active_modes
            .iter()
            .map(|mode| (mode.clone(), true))
            .collect(),
        power_state: build.combat.power_state.clone(),
        proc_overrides: build.proc_overrides.clone(),
        disabled_proc_categories: build.disabled_proc_categories.clone(),
        attack_chains: build.attack_chains.clone(),
        what_if_buffs: None,
    })
}

/// An inherent is written only when the user has authored something on it. The roster itself is
/// the dataset's, so writing an untouched inherent would be storing a derived fact (rule 6).
///
/// Three things count, and all three are the user's input rather than the dataset's answer:
/// a piece in a slot, slots placed beyond the base one, and the TOGGLE.
///
/// The toggle was missing until GRANTLOCK-1, and the omission is the sharper half of that
/// finding, because it cost the state on save rather than on read: an inherent switched on and
/// never slotted was not written at all, so it came back off. A Stalker's Hide, a Mastermind's
/// Hold Ground and a Stalker's Placate could never be saved on at all — their defs give them no
/// slots, so the slot terms can never fire for them — and Sprint, Ninja Run and every other
/// slottable inherent lost its switch unless a piece happened to be in it. `is_active` is build
/// state, not a derived fact, so rule 6 never covered it.
///
/// The slot terms are measured against the base slot alone rather than against
/// [`SelectedPower::inherent_slot_count`], because a fork's auto-granted slots are re-derived
/// on load: an inherent holding only its base and its granted slots, all empty, has nothing
/// authored in it.
fn is_modified_inherent(power: &&SelectedPower) -> bool {
    let placed = power.slots.len() > 1 + usize::from(power.inherent_slot_count);
    placed || power.slots.iter().any(Option::is_some) || power.is_active
}

fn slim_powerset(selection: &PowersetSelection) -> Result<Option<SkifSelection>, SkifError> {
    let Some(id) = selection.id.clone() else {
        return Ok(None);
    };
    let powers = selection
        .powers
        .iter()
        .map(|power| slim_power(power, &id))
        .collect::<Result<_, _>>()?;
    Ok(Some(SkifSelection { id, powers }))
}

fn slim_pool(pool: &PoolSelection) -> Result<SkifSelection, SkifError> {
    Ok(SkifSelection {
        id: pool.id.clone(),
        powers: pool
            .powers
            .iter()
            .map(|power| slim_power(power, &pool.id))
            .collect::<Result<_, _>>()?,
    })
}

fn slim_power(power: &SelectedPower, list_set: &str) -> Result<SkifPower, SkifError> {
    Ok(SkifPower {
        internal_name: power.internal_name.clone(),
        // Only a pick that belongs to some other set states one — see [`SkifPower::powerset`].
        powerset: (power.powerset != list_set).then(|| power.powerset.clone()),
        level: power.level,
        slots: power
            .slots
            .iter()
            .map(|slot| slot.as_ref().map(slim_enhancement).transpose())
            .collect::<Result<_, _>>()?,
        is_active: power.is_active,
        active_sub_power: power.active_sub_power.clone(),
    })
}

/// The booster axis is unsigned by nature and the relative-level axis is signed; the model
/// carries both in one `i8`, so this is where a value in the wrong axis has to fail rather
/// than be clamped into a plausible one.
fn booster(boost: i8) -> Result<u8, SkifError> {
    u8::try_from(boost).map_err(|_| SkifError::NegativeBooster(boost))
}

fn slim_enhancement(enhancement: &Enhancement) -> Result<SkifEnhancement, SkifError> {
    Ok(match &enhancement.kind {
        EnhancementKind::IoSet {
            set_id, piece_num, ..
        } => SkifEnhancement::IoSet {
            set_id: set_id.clone(),
            piece_num: *piece_num,
            attuned: enhancement.attuned,
            level: enhancement.level,
            booster: booster(enhancement.boost)?,
        },
        EnhancementKind::GenericIo { stat, .. } => SkifEnhancement::IoGeneric {
            stat: stat.clone(),
            level: enhancement.level,
            booster: booster(enhancement.boost)?,
        },
        EnhancementKind::Special { category, .. } => SkifEnhancement::Special {
            category: category.clone(),
            id: registry_id(&enhancement.id, category).to_string(),
            relative_level: enhancement.boost,
        },
        EnhancementKind::Origin { tier, stat, .. } => SkifEnhancement::Origin {
            stat: stat.clone(),
            tier: tier.clone(),
            relative_level: enhancement.boost,
        },
    })
}

/// A special's id in the model is `"<category>-<registry id>"` ([`Enhancement::special`]); the
/// file states the two separately, so the prefix comes off. A stored id that does not carry
/// the prefix is used whole — that is what the beta's own slimmer does, and inventing a strip
/// where there is no prefix would corrupt the id.
fn registry_id<'a>(id: &'a str, category: &str) -> &'a str {
    id.strip_prefix(category)
        .and_then(|rest| rest.strip_prefix('-'))
        .unwrap_or(id)
}

fn slim_incarnates(build: &CharacterState) -> BTreeMap<String, SkifIncarnate> {
    build
        .incarnates
        .occupied()
        .map(|(slot_id, pick)| {
            (
                slot_id.to_string(),
                SkifIncarnate {
                    power: pick.power_name.clone(),
                    active: pick.active,
                },
            )
        })
        .collect()
}

// ============================================================
// Decode.
// ============================================================

/// Read a `.skif` v5 file against `database`.
///
/// Refuses (structural): anything that is not JSON in this shape, any version but
/// [`VERSION`], a dataset stamp disagreeing with the build's own, a dataset id no fork
/// answers to, an unknown enhancement `type`, and a what-if layer.
///
/// Reports (vocabulary): a powerset, power, IO set, special or incarnate slot this dataset
/// does not carry. Those stay in the build and come back in [`Decoded::unresolved`].
pub fn decode(text: &str, database: &PowerDatabase) -> Result<Decoded, SkifError> {
    // Read the version before the shape: a v4 file parsed as v5 would report every field it
    // lacks as a shape problem, which buries the one fact the reader needs.
    let found = probe_version(text);
    if found != Some(VERSION) {
        return Err(SkifError::Version { found });
    }

    let file: SkifFile =
        serde_json::from_str(text).map_err(|e| SkifError::Malformed(e.to_string()))?;
    hydrate(file, database)
}

/// Rebuild a [`CharacterState`] from a parsed file. Split from [`decode`] so the round-trip
/// gate can drive it without a serialization hop.
pub fn hydrate(file: SkifFile, database: &PowerDatabase) -> Result<Decoded, SkifError> {
    if file.build.what_if_buffs.is_some() {
        return Err(SkifError::WhatIfLayer);
    }
    if let Some(stamp) = &file.authored_against {
        if stamp.dataset != file.build.dataset {
            return Err(SkifError::DatasetDisagrees {
                stamped: stamp.dataset.clone(),
                build: file.build.dataset.clone(),
            });
        }
    }
    let dataset = DatasetId::ALL
        .into_iter()
        .find(|id| id.as_str() == file.build.dataset)
        .ok_or_else(|| SkifError::UnknownDataset(file.build.dataset.clone()))?;

    let slim = file.build;
    let mut unresolved = Vec::new();
    let mut build = CharacterState::empty(dataset);

    build.name = slim.name;
    build.origin = slim.origin;
    build.level = slim.level;
    build.archetype = ArchetypeSelection {
        id: slim.archetype,
        // Display only, and the dataset owns it — resolved by the UI from the id, exactly as
        // it is for a build that was never in a file.
        name: String::new(),
    };

    let resolver = Resolver {
        database,
        branch_sets: branch_powersets(build.archetype.id.as_deref(), database),
    };
    build.primary = hydrate_powerset(slim.primary, &resolver, &mut unresolved);
    build.secondary = hydrate_powerset(slim.secondary, &resolver, &mut unresolved);
    build.pools = slim
        .pools
        .into_iter()
        .map(|pool| hydrate_pool(pool, &resolver, &mut unresolved))
        .collect();
    build.epic_pool = slim
        .epic_pool
        .map(|pool| hydrate_pool(pool, &resolver, &mut unresolved));

    let auto_granted = database
        .leveling_schedule
        .as_ref()
        .map(|schedule| schedule.auto_granted_slot_levels.clone())
        .unwrap_or_default();
    build.inherents = slim
        .inherents
        .into_iter()
        .map(|power| {
            let mut selection = hydrate_power(
                power,
                crate::INHERENT_SET,
                &resolver,
                &mut unresolved,
                Resolve::Granted,
            );
            selection.inherent_slot_count = crate::auto_granted_slot_count(
                &auto_granted,
                &selection.internal_name,
                build.level,
            );
            selection
        })
        .collect();

    build.accolades = slim.accolades;
    for (slot_id, pick) in slim.incarnates {
        let placed = build.incarnates.set(
            &slot_id,
            Some(IncarnateSlot {
                power_name: pick.power.clone(),
                active: pick.active,
            }),
        );
        if !placed {
            unresolved.push(Unresolved {
                context: format!("incarnate slot {slot_id:?}"),
                detail: format!(
                    "{:?} carries no such slot; the pick {:?} was kept out of the loadout",
                    dataset.as_str(),
                    pick.power
                ),
            });
        }
    }

    build.slot_order = slim
        .slot_order
        .into_iter()
        .map(SlotOrderEntry::from)
        .collect();
    build.proc_overrides = slim.proc_overrides;
    build.disabled_proc_categories = slim.disabled_proc_categories;
    build.attack_chains = slim.attack_chains;
    build.combat.global_conditionals = migrate_stances(slim.stances, database, &mut unresolved);
    build.combat.power_state = slim.power_state;
    build.combat.active_modes = slim
        .modes
        .into_iter()
        .filter_map(|(mode, on)| on.then_some(mode))
        .collect();

    Ok(Decoded { build, unresolved })
}

/// Move a stance key the dataset no longer spells onto the keys it does.
///
/// Conditional ids are VOCABULARY, owned by the export, and one family of them was respelled: a
/// gate that compares a stack count now carries that count in its id, so a threshold's `<stem>`
/// became `<stem>-<N>plus` (COND-3, 2026-08-07). `stances` is stored verbatim and read verbatim,
/// so without this a build saved before the respelling would load with the toggle silently off —
/// no error, no diagnosis, just a bonus the author had switched on and the reader does not show.
///
/// The rule is the respelling's own inverse, and names no power (Rule 0): a key the dataset does
/// not carry, whose `-<N>plus` descendants it does, hands its value to each of those. A key the
/// dataset still carries is left exactly alone, which is what makes this stable rather than a
/// standing rewrite — once no dataset spells the bare id, the rule can only fire on a file older
/// than the respelling.
///
/// Only the `plus` family, because only the threshold spelling moved. An exact `== N` gate has
/// carried its `-N` since long before this format existed, so a surviving `<stem>-<N>` sibling is
/// a state the old bare key never spoke for and must not be switched on by inheriting from it.
///
/// One old key can have several heirs, and that is the collapse this fixes: the bare
/// `kinetic_assault_impulse` displayed the union of its `>= 1` and `>= 5` bonuses, so the state
/// it recorded is BOTH successors on. A value the file STATES always outranks one it inherits,
/// whichever order the two are read in: a stated key is written unconditionally, an inherited
/// one only fills a gap.
fn migrate_stances(
    stances: BTreeMap<String, bool>,
    database: &PowerDatabase,
    unresolved: &mut Vec<Unresolved>,
) -> BTreeMap<String, bool> {
    // Nothing to translate, and reading the whole dataset's vocabulary to say so is the common
    // case: most builds never touch a caster-state toggle.
    if stances.is_empty() {
        return stances;
    }
    let declared = crate::caster_state::declared_conditional_ids(database);

    let mut out: BTreeMap<String, bool> = BTreeMap::new();
    for (id, on) in stances {
        if declared.contains(&id) {
            out.insert(id, on);
            continue;
        }
        let heirs: Vec<String> = declared
            .iter()
            .filter(|candidate| is_count_threshold_of(candidate, &id))
            .cloned()
            .collect();
        if heirs.is_empty() {
            unresolved.push(Unresolved {
                context: format!("conditional {id:?}"),
                detail: "this dataset declares no such conditional; the state was kept as written"
                    .to_string(),
            });
            out.insert(id, on);
            continue;
        }
        unresolved.push(Unresolved {
            context: format!("conditional {id:?}"),
            detail: format!(
                "respelled by stack count; the state moved onto {}",
                heirs.join(", ")
            ),
        });
        for heir in heirs {
            out.entry(heir).or_insert(on);
        }
    }
    out
}

/// Is `candidate` the id `stem` became when its gate's stack threshold was spelled into it —
/// `<stem>-<N>plus`, the one shape the converter's `_countState` writes for a `>=` or `>` gate?
fn is_count_threshold_of(candidate: &str, stem: &str) -> bool {
    let Some(digits) = candidate
        .strip_prefix(stem)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix("plus"))
    else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Which lookup answers "does this dataset carry the power" for a bucket. The five buckets
/// resolve through three different indices, and asking the wrong one reports a power that is
/// present as missing — which would make [`Decoded::unresolved`] useless by crying wolf.
///
/// `Granted` is the one that is not obvious. A build tags every granted inherent with the
/// synthetic [`crate::INHERENT_SET`], but the dataset keeps them in THREE places
/// ([`PowerDatabase::find_granted_power`]) — and the fitness ones live in the pool partition,
/// because the legacy Fitness pool still publishes them. Health and Stamina are the two most
/// commonly slotted powers in the game, so a lookup that missed the pool partition would
/// report them missing on almost every real build.
#[derive(Clone, Copy)]
enum Resolve {
    Powerset,
    Partition,
    Granted,
}

fn hydrate_powerset(
    slim: Option<SkifSelection>,
    resolver: &Resolver,
    unresolved: &mut Vec<Unresolved>,
) -> PowersetSelection {
    let Some(slim) = slim else {
        return PowersetSelection::default();
    };
    if resolver.database.find_powerset(&slim.id).is_none() {
        unresolved.push(Unresolved {
            context: format!("powerset {:?}", slim.id),
            detail: "this dataset carries no such powerset; its picks were kept unresolved"
                .to_string(),
        });
    }
    PowersetSelection {
        id: Some(slim.id.clone()),
        name: String::new(),
        powers: hydrate_powers(
            slim.powers,
            &slim.id,
            resolver,
            unresolved,
            Resolve::Powerset,
        ),
    }
}

fn hydrate_pool(
    slim: SkifSelection,
    resolver: &Resolver,
    unresolved: &mut Vec<Unresolved>,
) -> PoolSelection {
    PoolSelection {
        id: slim.id.clone(),
        name: String::new(),
        powers: hydrate_powers(
            slim.powers,
            &slim.id,
            resolver,
            unresolved,
            Resolve::Partition,
        ),
    }
}

fn hydrate_powers(
    powers: Vec<SkifPower>,
    powerset: &str,
    resolver: &Resolver,
    unresolved: &mut Vec<Unresolved>,
    resolve: Resolve,
) -> Vec<SelectedPower> {
    powers
        .into_iter()
        .map(|power| hydrate_power(power, powerset, resolver, unresolved, resolve))
        .collect()
}

fn hydrate_power(
    slim: SkifPower,
    list_set: &str,
    resolver: &Resolver,
    unresolved: &mut Vec<Unresolved>,
    resolve: Resolve,
) -> SelectedPower {
    // A pick that states its own set keeps it; every other pick belongs to the list it is in.
    let powerset = slim.powerset.unwrap_or_else(|| list_set.to_string());
    let internal_name = resolver.resolve_power_name(&slim.internal_name, &powerset, resolve);
    if internal_name.is_none() {
        unresolved.push(Unresolved {
            context: format!("power {}", power_address(&powerset, &slim.internal_name)),
            detail: "this dataset carries no such power; the pick was kept and marked".to_string(),
        });
    }
    SelectedPower {
        // The pick is kept under the name the FILE gave it when nothing resolves (rule 8): a
        // build that quietly shrinks on import is the worst outcome, because the number that
        // comes out is confidently wrong.
        internal_name: internal_name.unwrap_or(slim.internal_name),
        powerset,
        level: slim.level,
        slots: slim
            .slots
            .into_iter()
            .map(|slot| slot.map(|piece| hydrate_enhancement(piece, resolver.database, unresolved)))
            .collect(),
        is_active: slim.is_active,
        active_sub_power: slim.active_sub_power,
        // Re-derived by the load path's grant/inherent reconcile, never stored (rule 6).
        inherent_slot_count: 0,
        is_locked: false,
        inherent_category: None,
        // A reading condition, not build design (rule 5).
        targets_hit: None,
    }
}

/// The power's name as this dataset spells it, or `None` when it carries no such power.
///
/// Exact within the named set, then case-insensitive — real Homecoming patch drift renamed
/// `Tough_Hide` to `Tough_hide`. The v4 display-name fallback is deliberately NOT here: it is
/// a fuzzy match that can silently bind the wrong power, which is the failure rule 8 exists to
/// prevent. (`legacy` keeps a scoped version of it, because a v2 file carries no other
/// identity — see [`legacy::resolve_by_name`] for what makes that one safe.)
impl Resolver<'_> {
    fn resolve_power_name(
        &self,
        internal_name: &str,
        powerset: &str,
        resolve: Resolve,
    ) -> Option<String> {
        let database = self.database;
        // Eight granted inherents were renamed when they stopped being
        // hand-authored (see [`crate::inherent_aliases`]); a file saved under an
        // old name resolves through it. A no-op for every other name, and for
        // every resolve kind but `Granted` — no powerset power was renamed.
        let internal_name = match resolve {
            Resolve::Granted => crate::current_inherent_name(internal_name),
            _ => internal_name,
        };
        // The named set first, then this archetype's branch sets — never wider. `resolve_power`
        // would find a branch power too, through an UNSCOPED fallback that also answers a build
        // meaning one archetype's `Slice` with another's (nine copies ship).
        let scoped: Vec<&str> = std::iter::once(powerset)
            .chain(self.branch_sets.iter().map(String::as_str))
            .collect();
        let found = |ident: &str| -> Option<&crate::Power> {
            match resolve {
                Resolve::Powerset => scoped
                    .iter()
                    .find_map(|set| database.find_power(set, ident)),
                Resolve::Partition => database.find_partition_power(powerset, ident),
                Resolve::Granted => database.find_granted_power(ident),
            }
        };
        if let Some(power) = found(internal_name) {
            return Some(power.ident().to_string());
        }
        let wanted = internal_name.to_lowercase();
        let candidates: Box<dyn Iterator<Item = &crate::Power>> = match resolve {
            Resolve::Powerset => Box::new(
                scoped
                    .into_iter()
                    .filter_map(|set| database.find_powerset(set))
                    .flat_map(|set| set.powers.iter()),
            ),
            Resolve::Partition => Box::new(
                database
                    .pool_powers
                    .iter()
                    .chain(database.epic_powers.iter())
                    .filter(|entry| entry.set_id == powerset)
                    .map(|entry| &entry.power),
            ),
            // The same three places `find_granted_power` walks, in the same order.
            Resolve::Granted => Box::new(
                database
                    .inherent_powers
                    .iter()
                    .chain(
                        database
                            .find_powerset(crate::INHERENT_SET)
                            .into_iter()
                            .flat_map(|set| set.powers.iter()),
                    )
                    .chain(database.pool_powers.iter().map(|entry| &entry.power)),
            ),
        };
        candidates
            .filter(|power| power.ident().to_lowercase() == wanted)
            .map(|power| power.ident().to_string())
            .next()
    }
}

/// The dataset, plus the extra powersets a pick in THIS build may legitimately name.
///
/// The second half is the VEAT case and it is not an edge: a beta-authored build records a
/// branch pick under its archetype's BASE set id, so `Longfang` is stored under
/// `arachnos-soldier/arachnos-soldier` and lives in `arachnos-soldier/crab-spider-soldier`.
/// Ten of one corpus build's twenty-four picks are like that. A set-scoped lookup calls every
/// one of them missing. (A build this app writes files the pick under the branch set's own id,
/// which resolves without the widening — the widening is what keeps the older form readable.)
struct Resolver<'a> {
    database: &'a PowerDatabase,
    branch_sets: Vec<String>,
}

/// The VEAT branch powersets an archetype can draw from, which the export states outright
/// ([`crate::Archetype::branches`]) and which are in neither `primarySets` nor `secondarySets`.
///
/// Empty for the archetypes that have no branches, and empty when no archetype is chosen —
/// in both cases resolution stays exactly as scoped as it was.
pub(crate) fn branch_powersets(archetype: Option<&str>, database: &PowerDatabase) -> Vec<String> {
    let Some(archetype) = archetype else {
        return Vec::new();
    };
    let Ok(catalog) = database.archetypes() else {
        return Vec::new();
    };
    let Some(at) = catalog.get(archetype) else {
        return Vec::new();
    };
    at.branches
        .iter()
        .flat_map(|branch| [branch.primary_set.clone(), branch.secondary_set.clone()])
        .flatten()
        .collect()
}

/// Rebuild a slotted enhancement. The identity fields come off the wire; the DEFINITION
/// fields (a set's name, a piece's aspects, a special's aspect values) come from the dataset,
/// which is why a stale snapshot of them is not in the file at all (rule 6).
///
/// An unmatched set or special is retained with an EMPTY definition and reported rather than
/// dropped (rule 8). Empty is what makes that honest twice over: it contributes nothing to any
/// total, and it carries no fabricated name for a proc lookup to match on.
/// A stored booster the piece's own slotting rules refuse — a sub-50 craft level, an attuned
/// piece, a pure proc ([`Enhancement::takes_booster`]) — is read without it, and the drop is
/// reported, never silent: the state was written by the beta's pick path (which stamped the
/// global boost after the set-band clamp) or by hand, and honouring it would show a number
/// the game cannot reach.
fn reconcile_refused_booster(enhancement: &mut Enhancement, unresolved: &mut Vec<Unresolved>) {
    if !enhancement.holds_refused_booster() {
        return;
    }
    let context = match &enhancement.kind {
        EnhancementKind::IoSet {
            set_id, piece_num, ..
        } => format!("enhancement {set_id}#{piece_num}"),
        _ => format!("enhancement {}", enhancement.id),
    };
    unresolved.push(Unresolved {
        context,
        detail: format!(
            "a +{} booster on a piece that cannot take one (boosters combine only into an \
             unattuned, non-proc IO at level {}+); the piece was read without it",
            enhancement.boost,
            crate::BOOSTER_LEVEL_FLOOR,
        ),
    });
    enhancement.boost = 0;
}

fn hydrate_enhancement(
    slim: SkifEnhancement,
    database: &PowerDatabase,
    unresolved: &mut Vec<Unresolved>,
) -> Enhancement {
    match slim {
        SkifEnhancement::IoSet {
            set_id,
            piece_num,
            attuned,
            level,
            booster,
        } => {
            let found = database
                .io_sets
                .as_ref()
                .and_then(|catalog| catalog.get(&set_id))
                .and_then(|set| set.piece(piece_num).map(|piece| (set, piece)));
            let Some((set, piece)) = found else {
                unresolved.push(Unresolved {
                    context: format!("enhancement {set_id}#{piece_num}"),
                    detail: "this dataset carries no such set piece; the slot was kept and \
                             contributes nothing"
                        .to_string(),
                });
                let mut enhancement = Enhancement {
                    id: format!("{set_id}-{piece_num}"),
                    name: String::new(),
                    icon: String::new(),
                    level,
                    attuned,
                    boost: i8::try_from(booster).unwrap_or(i8::MAX),
                    kind: EnhancementKind::IoSet {
                        set_id,
                        set_name: String::new(),
                        piece_num,
                        aspects: Vec::new(),
                        is_proc: false,
                        is_unique: false,
                    },
                };
                reconcile_refused_booster(&mut enhancement, unresolved);
                return enhancement;
            };
            // The piece's 0-based position is the id suffix the model mints; `num` is its
            // 1-based key. Recovered by position rather than assumed to be `num - 1`, since a
            // set whose pieces are not contiguous would put them out of step.
            let piece_index = set
                .pieces
                .iter()
                .position(|candidate| candidate.num == piece_num)
                .unwrap_or(0);
            let mut enhancement = Enhancement {
                id: format!("{set_id}-{piece_index}"),
                name: piece.name.clone(),
                icon: String::new(),
                level,
                attuned,
                boost: i8::try_from(booster).unwrap_or(i8::MAX),
                kind: EnhancementKind::IoSet {
                    set_id,
                    set_name: set.name.clone(),
                    piece_num,
                    aspects: piece.aspects.clone(),
                    is_proc: piece.proc,
                    is_unique: piece.unique,
                },
            };
            reconcile_refused_booster(&mut enhancement, unresolved);
            enhancement
        }
        SkifEnhancement::IoGeneric {
            stat,
            level,
            booster,
        } => {
            // Constructed with the file's own boost restated after the fact, because
            // [`Enhancement::generic_io`] applies the floor itself — silently, which is right
            // at pick time and wrong here, where a dropped booster must be REPORTED.
            let mut enhancement = Enhancement::generic_io(stat, level, 0);
            enhancement.boost = i8::try_from(booster).unwrap_or(i8::MAX);
            reconcile_refused_booster(&mut enhancement, unresolved);
            enhancement
        }
        SkifEnhancement::Special {
            category,
            id,
            relative_level,
        } => {
            let def = database.enhancements.as_ref().and_then(|catalog| {
                catalog
                    .special_families()
                    .into_iter()
                    .find(|(_, tag, _)| *tag == category)
                    .and_then(|(_, _, registry)| registry.get(&id))
            });
            let Some(def) = def else {
                unresolved.push(Unresolved {
                    context: format!("enhancement {category}-{id}"),
                    detail: "this dataset carries no such special; the slot was kept and \
                             contributes nothing"
                        .to_string(),
                });
                return Enhancement {
                    id: format!("{category}-{id}"),
                    name: String::new(),
                    icon: String::new(),
                    level: None,
                    attuned: false,
                    boost: relative_level,
                    kind: EnhancementKind::Special {
                        category,
                        aspects: Vec::new(),
                    },
                };
            };
            Enhancement::special(&id, def, category, relative_level)
        }
        SkifEnhancement::Origin {
            stat,
            tier,
            relative_level,
        } => Enhancement::origin(stat, tier, None, relative_level),
    }
}
