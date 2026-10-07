//! The dataset contract, in Rust.
//!
//! `contract/<dataset>/bundle.json.gz` (emitted by `scripts/emit-contract.cjs` from
//! `pipeline/<dataset>/*.json`, `hand-data/`, `mids-tables/` and `exported_powers/`) is the
//! wire; this crate is the only place it is decoded.
//!
//! That parenthesis read "from the frozen TS oracle" until 2026-09-26, and had since the
//! emitter stopped pulling TypeScript through `tsx`: there is no `src/` in this repository
//! and every emitter input is a `JSON.parse`. The oracle survives only as the frozen
//! `fixtures/oracle/` records, which nothing regenerates and this crate never reads. Powers own
//! their atoms (`Vec<AtomicEffect>` per power — never a shared table), and every wire
//! discriminator decodes strictly: an unknown enum string is a load error, not a default
//! (GAME-DATA-PRINCIPLES §3: absence is absence, and a value we don't recognize is a
//! schema change we must hear about, not paper over).

pub mod accolades;
pub mod archetype_stats;
pub mod archetypes;
pub mod at_tables;
pub mod atom;
pub mod atom_wire;
pub mod boost_index;
pub(crate) mod build_sets;
pub mod build_store;
pub mod caster_state;
pub mod character;
pub mod client_build;
pub mod database;
pub mod enhancement_curves;
pub mod enhancements;
pub mod game_export;
pub mod game_import;
pub mod grant_edges;
pub mod granted_powers;
pub mod import_link;
pub mod incarnate_catalog;
pub mod incarnate_crafting;
pub mod incarnate_effects;
pub mod inherent_aliases;
pub mod inherent_grants;
pub mod io_sets;
pub mod level;
pub mod leveling_schedule;
pub mod mbd;
pub mod mbd_import;
pub mod mids_enh_names;
pub mod mids_names;
pub mod mids_uids;
pub mod mxd;
pub mod mxd_import;
pub mod pick_rules;
pub mod pools;
pub mod power;
pub mod proc_data;
pub mod purple_patch;
pub mod skif;
pub mod slot_levels;
pub mod slot_value;
pub mod slotting_rules;
pub mod target_classes;

pub use accolades::{AccoladeFaction, AccoladeToggle, ACCOLADE_CATEGORY};
pub use archetype_stats::{
    at_level, ArchetypeCaps, ArchetypeStats, MovementAxes, MovementAxisTables,
};
pub use archetypes::{Archetype, ArchetypeInherent, Archetypes};
pub use at_tables::{AtTables, TableScope};
pub use atom::{
    excludes_caster, expression_text, expression_tokens, lands_on_caster, reaches_caster,
    ApplicationType, Aspect, AtomicEffect, AttribType, EffectType, PvMode, Stacking, SubType,
    ToWho,
};

pub use boost_index::{BoostEntry, BoostIndex, OriginTier};
pub use build_store::StoredBuilds;
pub use caster_state::{
    castable_in_mode, caster_class_name, caster_modes, conditional_for_class, form_modes,
    global_mechanics, group_label, mode_label, picks_answer, set_global_mechanic, set_stance,
    stance_group_for, stance_groups, MechanicToggle, ModeToggle, StanceGroup, StanceOption,
};
pub use character::power_address;
pub use character::{
    ArchetypeSelection, AspectValue, AttackChain, CharacterState, CombatContext, ContentMode,
    Enhancement, EnhancementKind, IncarnateLoadout, IncarnateSlot, InherentCategory, IoSlotting,
    PoolSelection, PowersetSelection, ProcOverride, SelectedPower, SlotOrderEntry, ValidationError,
    BOOSTER_LEVEL_FLOOR, MAX_USER_SLOTS_PER_POWER,
};
pub use database::{DatasetId, LoadError, PowerDatabase};
pub use enhancement_curves::{
    BoostEffectiveness, EnhancementCurves, ExemplarHandicaps, OriginTiers, Schedule, ScheduleCurve,
    ScheduleCurves, TierScales,
};
pub use enhancements::{EnhancementCatalog, SpecialEnhancementDef};
pub use granted_powers::sync_granted_powers;
pub use incarnate_catalog::{
    IncarnateBranch, IncarnateCatalog, IncarnateCatalogPower, IncarnateSlotCatalog, IncarnateTier,
    IncarnateTreeRow, IncarnateTreeView, RareDepth, TREE_GRID_COLUMNS,
};
pub use incarnate_crafting::{
    BuyOption, CraftNode, CraftRecipe, CraftSalvage, IncarnateCrafting, RecipeFamily, SalvageRarity,
};
pub use incarnate_effects::{
    normalize_incarnate_power_id, AlphaEffects, DestinyEffects, DestinyTimelineTier,
    GenesisEffects, GenesisExemplarEffect, HybridEffects, IncarnateEffects, InterfaceEffects,
    JudgementEffects, LoreEffects,
};
pub use inherent_aliases::current_inherent_name;
pub use inherent_grants::{
    auto_granted_slot_count, granted_inherents, InherentGrant, InherentGrants, INHERENT_SET,
};
pub use io_sets::{IoSet, IoSetCatalog, IoSetPiece, SetBonus, SetBonusEffect};
pub use level::Level;
pub use leveling_schedule::{
    level_progress, placed_budget_slots, slots_remaining, LevelProgress, LevelingSchedule,
};
pub use mbd::{MbdEnhancement, MbdFile, MbdPowerEntry, MbdSlotEntry};
pub use mids_names::MidsNames;
pub use mids_uids::{MidsFamily, MidsSetPiece, MidsUidPrefix, MidsUids};
pub use pick_rules::{requires_met, set_gate, RequiresError, SetGate, SetPaths};
pub use pools::{PoolCatalog, PoolDef};
pub use power::{targets_name_foe, MechanicType, Power, Powerset};
pub use proc_data::{ProcData, ProcDatabase, ProcEffect, ProcType};
pub use purple_patch::PurplePatch;
pub use slot_value::{Absorb, MovementValue, Scaled, ScaledMez, Stealth};
pub use target_classes::{target_ranks, TargetRank};
