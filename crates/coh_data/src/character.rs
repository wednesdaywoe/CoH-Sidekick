//! `CharacterState` — the build model (identity + user choices), plus the
//! `Enhancement` union and the positional slot model.
//!
//! Skeptical-borrowing: the beta has **no**
//! `CharacterState` type. We mirror the *shapes* of `Build` (`build.ts:115`),
//! `SelectedPower` (`power.ts:1081`), the `Enhancement` union (`enhancement.ts:111`),
//! and `IncarnateBuildState` (`incarnate.ts:101`) — not the names, and not fields that
//! are pure display or trivially re-derivable.
//!
//! **Identity / definition split (why some beta fields are dropped here).**
//! Dataset-owned *definitions* resolve from the [`crate::PowerDatabase`] at
//! calc/display time; build-authored *selections* are stored inline. So a picked power
//! is stored as its identity ([`SelectedPower::internal_name`] + set + level + slots +
//! flags) and its heavyweight [`crate::Power`] def is looked up from the dataset when
//! gathering atoms — never snapshotted into the build. That kills the stale-def bug
//! class, keeps undo clones cheap, and makes serde trivial. Enhancements, by contrast,
//! are lightweight build-authored selections and are stored inline (as the beta does).
//! This is the same identity/definition split the item-10 slim/hydrate codec formalizes.
//!
//! No calc and no persistence live here — this is the pure model (decisions D1–D3 of
//! the M3 execution plan). `coh_math` imports it
//! for `recalculate`; the app mutates and persists it (D2 — the data crate is the
//! shared floor).

use crate::database::DatasetId;
use crate::level::Level;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// A power can carry at most 6 user slots (1 default + 5 added); fitness auto-grants
/// stack on top via [`SelectedPower::inherent_slot_count`].
pub const MAX_USER_SLOTS_PER_POWER: usize = 6;

/// The address a pick answers to anywhere a map is keyed by power: its owning set plus its
/// internal name.
///
/// An internal name alone is not an address, and the reason is not merely that `Build_Up`
/// names sixty-four powers across archetypes — **the collision happens inside a single
/// build**. `Conserve_Power` is offerable from both a secondary and an epic pool, so a key
/// missing the set reaches two different picks at once, and a control set on one silently
/// moves the other. Every per-power map (`proc_overrides`,
/// [`CombatContext::power_state`], chain entries) mints its key through here so the shape is
/// stated once.
pub fn power_address(powerset: &str, internal_name: &str) -> String {
    format!("{powerset}:{internal_name}")
}

// How many standard power pools a build may hold is a DATASET rule, not a constant: the
// leveling schedule exports `maxPowerPools` (4 on Homecoming/Rebirth, 5 on Thunderspy), and
// the pool picker and powers panel already resolve it from there. It is deliberately absent
// from [`CharacterState::validate`], which is dataset-free by contract and so cannot ask.

/// A complete character build. Mirrors the calc-relevant surface of the beta `Build`
/// (`build.ts:115-227`); display-only and cloud/library fields (`icon`, `vaultId`,
/// crafting checklists) are omitted — none feed totals, and the persistence codec
/// (item 10) re-derives what the UI needs. `attack_chains` is the exception among the
/// display-only fields: a saved rotation cannot be re-derived, so it travels with the
/// build like the beta's `attackChains`.
/// One slotted proc's control override ([`CharacterState::proc_overrides`]).
///
/// `mode` selects which knob is authoritative: `auto` takes the honest steady-state default
/// (one discrete stack for a stacking buff, the always-on floor for an HP-scaling one),
/// `stacks` pins the stack count, `hp` pins the %HP an HP-scaling proc is read at. An unknown
/// mode string is treated as `auto` — a saved build from a newer beta must degrade to the
/// default, never to zero.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcOverride {
    /// Master gate for this specific slotted proc. `false` removes its contribution entirely.
    pub enabled: bool,
    /// `"auto"` | `"stacks"` | `"hp"`.
    pub mode: String,
    /// Pinned stack count when `mode == "stacks"` (clamped to the effect's cap).
    #[serde(default)]
    pub stacks: Option<u32>,
    /// Pinned %HP when `mode == "hp"` (0..=100; 100 = full HP = the floor).
    #[serde(default, rename = "hpPct")]
    pub hp_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterState {
    /// User-facing build name.
    pub name: String,
    /// The character's origin — one of `"magic"`, `"mutation"`, `"natural"`, `"science"`,
    /// `"technology"` (lowercase; the app's Identity picker writes this vocabulary, matching
    /// the icon layer's existing `origin_frame_prefix`), or `None` when not set. A build fact
    /// like [`Self::name`] and [`Self::archetype`], not a preference: it is flavor-only (the
    /// calc never keys value on it, see [`Enhancement::origin`]) but it decides which DO/SO
    /// overlay frame an origin enhancement wears. `#[serde(default)]` so a pre-field saved
    /// build loads as `None` — the prior fixed behavior (every piece read the Natural frame).
    #[serde(default)]
    pub origin: Option<String>,
    /// The dataset this build targets (beta `serverId`). Keys the per-dataset
    /// persistence envelope and selects which definitions `hydrate` resolves against.
    pub dataset: DatasetId,
    /// Selected archetype (identity only; stats/inherent resolve from the dataset).
    pub archetype: ArchetypeSelection,
    /// Character level, 1..=50.
    pub level: u8,
    /// Primary powerset selection.
    pub primary: PowersetSelection,
    /// Secondary powerset selection.
    pub secondary: PowersetSelection,
    /// Standard power pools. The cap is the dataset's `maxPowerPools`, resolved where a
    /// pool is added rather than stored here.
    pub pools: Vec<PoolSelection>,
    /// Epic/patron pool.
    pub epic_pool: Option<PoolSelection>,
    /// Inherent powers (fitness, archetype inherent, prestige sprints, …). A flat list
    /// because inherents have no owning powerset — each carries its own `powerset` tag.
    pub inherents: Vec<SelectedPower>,
    /// Selected accolade ids (internal name, lower-cased). Resolved to the +MaxHP/+MaxEnd
    /// atoms in the contract's `Accolades` powerset — carried here, applied by the calc.
    pub accolades: Vec<String>,
    /// Incarnate loadout — the calc contribution (Pass 6) reads it via
    /// [`crate::incarnate_effects`] (WS16).
    pub incarnates: IncarnateLoadout,
    /// Per-slotted-proc control overrides, keyed `"<powerset>:<internalName>:<slot index>"`
    /// ([`power_address`] plus the slot) — the beta `Build.procOverrides`, written by the
    /// InfoPanel "Slotted Procs" block. An absent key means the runtime default (enabled +
    /// auto). Build identity, not UI context: it saves, loads and shares with the build.
    ///
    /// The beta keys this on the power's DISPLAY name, which cannot address one pick: see
    /// [`power_address`] for the within-build collision that breaks.
    #[serde(default)]
    pub proc_overrides: BTreeMap<String, ProcOverride>,
    /// Proc effect categories the build has switched OFF, spelled as the proc data spells
    /// them (`"Defense"`, `"MezResist"`, `"RunSpeed"`). Build identity: a category switched off
    /// is a binary switch the player threw on their own character, and it gates real
    /// contributions in every proc pass, so a reader without it reads HIGHER totals than the
    /// author authored. (Not the "reproduce the author's numbers" argument — that one is
    /// rejected, which is why Fury and team size do not travel. The test is whether the state
    /// was switched on the character or describes the encounter it is being read against.)
    ///
    /// Stored as the DISABLED set rather than an enabled map so that absence means
    /// contributing: a category the proc data grows lands on the dashboard by default instead
    /// of being silently withheld by a build saved before it existed (Rule 1). A fresh build
    /// therefore serializes nothing here.
    #[serde(default)]
    pub disabled_proc_categories: BTreeSet<String>,
    /// Chronological record of slot additions (leveling mode). Empty ⇒ respec mode,
    /// where slot levels are computed from power-pick order.
    pub slot_order: Vec<SlotOrderEntry>,
    /// Combat-context inputs to the totals loop: per-target/suppression drivers and the
    /// additive-AT-inherent inputs (Fury bar, Vigilance team size). Roadmap decision 4.
    pub combat: CombatContext,
    /// Saved attack chains (RB5-c). Display-only — nothing in the calc reads them — but
    /// build identity like [`Self::proc_overrides`]: they save, load and share with the
    /// build.
    #[serde(default)]
    pub attack_chains: Vec<AttackChain>,
    /// Incarnate crafting checklist: nodes marked "already crafted", keyed by the
    /// craft tree's parent-qualified node key
    /// ([`crate::incarnate_crafting::CraftNode::key`]). Display-only planner
    /// bookkeeping like [`Self::attack_chains`] — nothing in the calc reads it,
    /// but it persists with the build. Not (yet) carried by the `.skif` codec.
    #[serde(default)]
    pub crafting_obtained: BTreeSet<String>,
    /// Per-node salvage checkboxes in the crafting modal, keyed
    /// `"<node key>:<salvage id>"`. Same standing as [`Self::crafting_obtained`].
    #[serde(default)]
    pub crafting_salvage_checked: BTreeSet<String>,
    /// Shopping-list progress: salvage id → count acquired so far. Clamped to
    /// the needed count at read time, never at write, so an over-acquired entry
    /// survives the need shrinking and re-growing.
    #[serde(default)]
    pub shopping_acquired: BTreeMap<String, u32>,
}

/// A saved attack chain — the rotation builder's persisted state (RB5-c, beta
/// `AttackChain`, `build.ts:108`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttackChain {
    /// Stable id, minted at save time.
    pub id: String,
    /// User-facing chain name.
    pub name: String,
    /// Cast order as stable chain-power ids (`"<powerset>:<internal name>"`) — indices
    /// would break the moment the build's power set changes.
    pub powers: Vec<String>,
}

impl CharacterState {
    /// An empty build for `dataset`: no archetype, no powers, level 50 (the planner
    /// default — a fresh build is designed at cap and exemplared down), solo/out-of-combat.
    pub fn empty(dataset: DatasetId) -> Self {
        CharacterState {
            name: String::new(),
            origin: None,
            dataset,
            archetype: ArchetypeSelection::default(),
            level: 50,
            primary: PowersetSelection::default(),
            secondary: PowersetSelection::default(),
            pools: Vec::new(),
            epic_pool: None,
            inherents: Vec::new(),
            accolades: Vec::new(),
            incarnates: IncarnateLoadout::default(),
            proc_overrides: BTreeMap::new(),
            disabled_proc_categories: BTreeSet::new(),
            slot_order: Vec::new(),
            combat: CombatContext::default(),
            attack_chains: Vec::new(),
            crafting_obtained: BTreeSet::new(),
            crafting_salvage_checked: BTreeSet::new(),
            shopping_acquired: BTreeMap::new(),
        }
    }

    /// Whether proc effects of this category contribute to the build's totals.
    ///
    /// The one reader of [`Self::disabled_proc_categories`], so the absence-means-enabled
    /// convention is stated once rather than at each of the four proc passes.
    pub fn proc_category_enabled(&self, category: &str) -> bool {
        !self.disabled_proc_categories.contains(category)
    }

    /// Every selected power across primary, secondary, pools, epic, and inherents, in a
    /// stable order (primary → secondary → pools → epic → inherents). The calc's Pass 0
    /// gather walks this; UI panels render from it.
    pub fn all_selected(&self) -> impl Iterator<Item = &SelectedPower> {
        self.primary
            .powers
            .iter()
            .chain(self.secondary.powers.iter())
            .chain(self.pools.iter().flat_map(|p| p.powers.iter()))
            .chain(self.epic_pool.iter().flat_map(|p| p.powers.iter()))
            .chain(self.inherents.iter())
    }

    /// [`Self::all_selected`], to edit. Same buckets in the same order, beside its read-only
    /// twin so a bucket the model grows later is grown into both — a build-wide edit that
    /// reaches four of five buckets leaves a silently untouched pocket of the build, which is
    /// the shape of the beta defect this walk exists to avoid.
    pub fn all_selected_mut(&mut self) -> impl Iterator<Item = &mut SelectedPower> {
        self.primary
            .powers
            .iter_mut()
            .chain(self.secondary.powers.iter_mut())
            .chain(self.pools.iter_mut().flat_map(|p| p.powers.iter_mut()))
            .chain(self.epic_pool.iter_mut().flat_map(|p| p.powers.iter_mut()))
            .chain(self.inherents.iter_mut())
    }

    /// One selected power, addressed the way the rest of the app addresses a pick: by its
    /// owning set id plus its internal name. `None` when the build holds no such pick.
    ///
    /// Identity alone is not enough — `internalName` collides across archetypes (Build_Up
    /// appears ×64), so the set id is half the address. The inherent arm is what makes
    /// granted inherents reachable at all: they carry the synthetic
    /// [`INHERENT_SET`](crate::INHERENT_SET) rather than an owning powerset, so a lookup that
    /// stopped at the four picked buckets found nothing and every edit to one silently did
    /// nothing. (A power-gated grant needs no arm of its own — it lives in its owning
    /// bucket like any pick.)
    pub fn selected_power(&self, powerset: &str, internal_name: &str) -> Option<&SelectedPower> {
        let named = |power: &&SelectedPower| power.internal_name == internal_name;
        if self.primary.id.as_deref() == Some(powerset) {
            return self.primary.powers.iter().find(named);
        }
        if self.secondary.id.as_deref() == Some(powerset) {
            return self.secondary.powers.iter().find(named);
        }
        if let Some(pool) = self.pools.iter().find(|pool| pool.id == powerset) {
            return pool.powers.iter().find(named);
        }
        if let Some(epic) = self.epic_pool.iter().find(|epic| epic.id == powerset) {
            return epic.powers.iter().find(named);
        }
        if powerset == crate::INHERENT_SET {
            return self.inherents.iter().find(named);
        }
        // No bucket names this set, so the pick carries it on itself: a VEAT branch power,
        // which sits in a role list under its own set id.
        self.primary
            .powers
            .iter()
            .chain(&self.secondary.powers)
            .find(|power| power.powerset == powerset && named(power))
    }

    /// Mutable twin of [`selected_power`](Self::selected_power) — the one write handle every
    /// per-power edit (slotting, toggling, stance) resolves through.
    pub fn selected_power_mut(
        &mut self,
        powerset: &str,
        internal_name: &str,
    ) -> Option<&mut SelectedPower> {
        let named = |power: &&mut SelectedPower| power.internal_name == internal_name;
        if self.primary.id.as_deref() == Some(powerset) {
            return self.primary.powers.iter_mut().find(named);
        }
        if self.secondary.id.as_deref() == Some(powerset) {
            return self.secondary.powers.iter_mut().find(named);
        }
        if let Some(pool) = self.pools.iter_mut().find(|pool| pool.id == powerset) {
            return pool.powers.iter_mut().find(named);
        }
        if let Some(epic) = self.epic_pool.iter_mut().find(|epic| epic.id == powerset) {
            return epic.powers.iter_mut().find(named);
        }
        if powerset == crate::INHERENT_SET {
            return self.inherents.iter_mut().find(named);
        }
        // The VEAT branch pick, as in `selected_power`.
        self.primary
            .powers
            .iter_mut()
            .chain(&mut self.secondary.powers)
            .find(|power| power.powerset == powerset && named(power))
    }

    /// Every power that spends one of the level-gated power picks
    /// ([`crate::LevelingSchedule::pick_levels`]), in the same stable order — which is
    /// also the tie-break order when two powers share a pick level.
    ///
    /// Locked selections are excluded: the game grants them — the flat inherents and the
    /// power-gated grants a bucket materializes ([`crate::sync_granted_powers`]) — so they
    /// cost no pick. They still appear in [`all_selected`](Self::all_selected) because
    /// several carry enhancement slots.
    pub fn picked_powers(&self) -> impl Iterator<Item = &SelectedPower> {
        self.primary
            .powers
            .iter()
            .chain(self.secondary.powers.iter())
            .chain(self.pools.iter().flat_map(|p| p.powers.iter()))
            .chain(self.epic_pool.iter().flat_map(|p| p.powers.iter()))
            .filter(|power| !power.is_locked)
    }

    /// Structural validation independent of the dataset (a picked power's *existence* is
    /// the hydrate step's concern). Returns every violation found, not just the first, so
    /// a form can surface them together; `Ok(())` means structurally sound.
    pub fn validate(&self) -> Result<(), Vec<ValidationError>> {
        let mut errors = Vec::new();
        if !(1..=50).contains(&self.level) {
            errors.push(ValidationError::LevelOutOfRange { level: self.level });
        }
        if self.combat.vigilance_team_size == 0 {
            errors.push(ValidationError::EmptyTeamSize);
        }
        for power in self.all_selected() {
            let budget = MAX_USER_SLOTS_PER_POWER + power.inherent_slot_count as usize;
            if power.slots.len() > budget {
                errors.push(ValidationError::SlotBudgetExceeded {
                    power: power.internal_name.clone(),
                    slots: power.slots.len(),
                    budget,
                });
            }
            if power.level != 0 && !(1..=50).contains(&power.level) {
                errors.push(ValidationError::PowerLevelOutOfRange {
                    power: power.internal_name.clone(),
                    level: power.level,
                });
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

/// A structural problem found by [`CharacterState::validate`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationError {
    #[error("character level {level} out of range 1..=50")]
    LevelOutOfRange { level: u8 },
    #[error("power {power:?} has {slots} slots, budget {budget}")]
    SlotBudgetExceeded {
        power: String,
        slots: usize,
        budget: usize,
    },
    #[error("power {power:?} taken at level {level}, out of range 1..=50")]
    PowerLevelOutOfRange { power: String, level: u8 },
    #[error("Vigilance team size cannot be zero (solo = 1)")]
    EmptyTeamSize,
}

/// Selected archetype — identity only. The full stats block and inherent power resolve
/// from the dataset by `id` (beta `ArchetypeSelection` snapshots them; we don't, to keep
/// the dataset the single source of truth).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ArchetypeSelection {
    /// Archetype id (e.g. `"blaster"`); `None` on a fresh build.
    pub id: Option<String>,
    /// Display name, kept so the UI can render before a dataset lookup.
    pub name: String,
}

/// A primary/secondary powerset selection (`build.ts:15`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PowersetSelection {
    /// Powerset id (e.g. `"blaster/fire-blast"`); `None` when unselected.
    pub id: Option<String>,
    /// Display name.
    pub name: String,
    /// Powers picked from this set.
    pub powers: Vec<SelectedPower>,
}

/// A power-pool selection (`build.ts:28`). Distinct from [`PowersetSelection`] only in
/// that a pool always has an id once added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PoolSelection {
    /// Pool id (e.g. `"speed"`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Powers picked from this pool.
    pub powers: Vec<SelectedPower>,
}

/// A power picked into a build. Beta `SelectedPower extends Power` (`power.ts:1081`); we
/// store identity + build state and resolve the [`crate::Power`] def from the dataset (see
/// the module-level identity/definition split).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectedPower {
    /// The power's `internalName` — the identity `hydrate` resolves to a def.
    pub internal_name: String,
    /// The set this power belongs to (beta `powerSet`). Disambiguates same-named powers
    /// across sets and is the only owner info a flat inherent carries. The wire key stays
    /// `power_set` — the frozen fixture/contract format — while the field reads in the
    /// game's one-word vocabulary.
    #[serde(rename = "power_set")]
    pub powerset: String,
    /// Level the power was taken at (1..=50; 0 ⇒ unset/auto-granted).
    pub level: u8,
    /// Positional enhancement slots (decision D1). `None` = an empty slot; index is the
    /// slot number the slotting UI drops onto.
    pub slots: Vec<Option<Enhancement>>,
    /// Toggled on and contributing to totals (toggle/buff powers).
    pub is_active: bool,
    /// Active mutually-exclusive sub-power (Bio Adaptation, Kheldian/Primalist forms).
    pub active_sub_power: Option<String>,
    /// Trailing slots auto-granted by the game (fitness Health/Stamina grants) that do
    /// not count against the user's slot budget.
    pub inherent_slot_count: u8,
    /// User cannot remove this power (inherent powers).
    pub is_locked: bool,
    /// Inherent category, when this is an inherent power.
    pub inherent_category: Option<InherentCategory>,
    /// Targets hit / stacks active for this selection — the user input `coh_math::stacking`
    /// consumes (`adjustForStacking`). `None` = no input, which the two stacking paths read
    /// differently: 0 targets on the per-target path, 1 stack on the linear one (see
    /// `coh_math::stacking`). One value serves two mechanics, exactly as the beta's targets-hit slider
    /// does: for an AoE effect carrying `perTarget` it is the number of targets hit; for an
    /// effect in the power's `stacksLinear` it is the STACK COUNT (Build Up ×2).
    ///
    /// Stored PER SELECTION, not in a map keyed by `internalName` like the beta's
    /// `targetsHitValues` (`character-totals.ts:860`) — internalName is NOT unique (Build_Up
    /// appears ×64 across archetypes), so the beta's key cannot address one specific pick.
    /// [[skeptical-borrowing]]
    #[serde(default)]
    pub targets_hit: Option<u32>,
}

impl SelectedPower {
    /// This pick's [`power_address`] — what every per-power map keys on.
    pub fn address(&self) -> String {
        power_address(&self.powerset, &self.internal_name)
    }

    /// A freshly picked power: one empty base slot, not an inherent, toggled off.
    pub fn picked(
        internal_name: impl Into<String>,
        powerset: impl Into<String>,
        level: u8,
    ) -> Self {
        SelectedPower {
            internal_name: internal_name.into(),
            powerset: powerset.into(),
            level,
            slots: vec![None],
            is_active: false,
            active_sub_power: None,
            inherent_slot_count: 0,
            is_locked: false,
            inherent_category: None,
            targets_hit: None,
        }
    }
}

/// Category for inherent powers (`power.ts:1091`), plus [`Self::Granted`] — the
/// pick-gated grants the reconcile materializes into the inherents list
/// ([`crate::granted_powers`]), which the identity sync carries through untouched.
/// The category doubles as the ownership marker between those two writers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InherentCategory {
    Fitness,
    Basic,
    Prestige,
    Archetype,
    Granted,
}

/// An enhancement slotted into a power. Union of the beta's four enhancement kinds
/// (`enhancement.ts:111`), flattened: the common `BaseEnhancement` fields sit alongside a
/// `type`-tagged [`EnhancementKind`] payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Enhancement {
    /// Stable identifier.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Icon filename (display only).
    #[serde(default)]
    pub icon: String,
    /// Craft level, when the piece has one; `None` defers to the build's global
    /// IO level at calc time. The wire also spells that deferral as a legacy `0`
    /// (the beta's falsy-level idiom), which decodes to `None` here.
    #[serde(deserialize_with = "crate::level::deserialize_optional")]
    pub level: Option<Level>,
    /// Attuned (level-agnostic) piece.
    #[serde(default)]
    pub attuned: bool,
    /// Stored level offset. Which mechanic this IS depends on `kind`, because
    /// the two sit on different curves from different bins and no enhancement
    /// carries both (`coh_math`'s `enhancement_level_multiplier`):
    ///
    ///  - `IoSet` / `GenericIo` — Enhancement Booster level, 0..=5, off the
    ///    dataset's `boosters` combine curve.
    ///  - `Special` / `Origin` — RELATIVE LEVEL: the enhancement's level minus
    ///    the character's combat level, off the `above`/`below` curves. SIGNED,
    ///    and the negative half is a real, common state — an out-levelled SO is
    ///    weaker, by -10% per level on Homecoming.
    ///
    /// Signed for that reason: it was `u8`, which could not represent an
    /// under-level enhancement at all.
    #[serde(default)]
    pub boost: i8,
    /// Kind-specific payload, tagged by `type` on the wire.
    #[serde(flatten)]
    pub kind: EnhancementKind,
}

/// An Enhancement Booster combines only into a level-50+ IO in-game — sub-50 IOs
/// (including a set piece whose set tops out lower) cannot accept one, which the beta's
/// own tools state outright ("Attuned IOs and sub-50 IOs cannot accept boosters
/// in-game"). The export does not carry the rule — the catalog describes items, not the
/// combine economy — so it is a named game constant like the IO level band, and it lives
/// here rather than in the UI's `picker_defaults` family because its enforcing reader is
/// [`Enhancement::takes_booster`], upstream of the UI.
pub const BOOSTER_LEVEL_FLOOR: u8 = 50;

/// The floor test for one piece's stated craft level. An absent level defers to the
/// build's global craft level (the wire's `slot.level || globalIOLevel` idiom), which the
/// beta reads as 50 (`level ?? 50`) — refusing absence here would strip boosters off
/// every deferred-level generic IO, so absence passes.
fn booster_floor_met(level: Option<Level>) -> bool {
    level.is_none_or(|level| level.get() >= BOOSTER_LEVEL_FLOOR)
}

impl Enhancement {
    /// A generic ("common") IO enhancing one aspect, mirroring the beta
    /// `createGenericIOEnhancement`. `level` `None` defers to the build's global
    /// IO level at calc time (`coh_math`'s `slot.level || globalIOLevel`); `boost`
    /// is the catalyst booster level (0 = none), which the calc scales the value by.
    ///
    /// Generic IOs are never attuned (the picker's attunement toggle governs set
    /// pieces only, per the beta) — they always carry an explicit craft level.
    ///
    /// The kind's `value` carries no authored number: the calc derives a generic
    /// IO's magnitude from `stat`'s ED schedule and level (`coh_math::enhancement`
    /// reads `GenericIo { stat, .. }` and ignores the field), so a stored value
    /// would be a hand-kept duplicate of an export-owned number.
    pub fn generic_io(stat: impl Into<String>, level: Option<Level>, boost: u8) -> Self {
        let stat = stat.into();
        let id = match level {
            Some(level) => format!("generic-io-{stat}-{level}"),
            None => format!("generic-io-{stat}"),
        };
        let name = format!("{stat} IO");
        let boost = if booster_floor_met(level) { boost } else { 0 };
        Enhancement {
            id,
            name,
            icon: String::new(),
            level,
            attuned: false,
            // Booster axis: unsigned by nature, so the widening is lossless.
            boost: i8::try_from(boost).unwrap_or(i8::MAX),
            kind: EnhancementKind::GenericIo { stat, value: 0.0 },
        }
    }

    /// A piece of an IO set, mirroring the beta `createIOSetEnhancement`.
    /// `piece_index` is the piece's 0-based position within the set — the id suffix
    /// (`{set_id}-{piece_index}`) — distinct from `piece.num`, its 1-based slot key.
    ///
    /// The picker's [`IoSlotting`] defaults (attunement, craft level, booster) are
    /// stamped here per the beta's rules:
    /// - A set the export marks `attunedOnly` (ATO/event/reward sets) is attuned
    ///   however it is picked, as is any piece whose picker attunement toggle is
    ///   on. That used to read `set_max_level <= 1`, which is right for most of
    ///   the roster and misses the reward sets that keep a craft range — see
    ///   `IoSet::attuned_only`. Attuned pieces scale with character
    ///   level, so they carry no fixed craft level (`level = None`) — the same rule
    ///   `coh_math::enhancement` applies when it aggregates the piece.
    /// - A non-attuned piece's craft level is clamped into the set's own
    ///   `[min, max]` band (beta `Math.max(minLevel, Math.min(level, maxLevel))`).
    /// - Pure procs (a proc with no aspects) and attuned pieces don't scale with
    ///   boosters, so their booster level is dropped.
    /// - The booster floor reads the level the piece actually lands at, AFTER the
    ///   set-band clamp ([`BOOSTER_LEVEL_FLOOR`]). The beta enforced the floor only
    ///   in its bulk tools, not at pick time — so its pick path stamped the global
    ///   boost onto a piece the band had just clamped below 50, a state its own
    ///   tools call impossible (the v4 corpus holds one: Basilisk's Gaze +5 at 30).
    ///
    /// The stored `name` is the raw piece label, NOT the set name: it is the proc
    /// lookup key the calc engine matches on (the beta's `findProcData`), so resolving
    /// it to a display name here would break proc identification.
    pub fn io_set(
        set_id: impl Into<String>,
        set_name: impl Into<String>,
        piece_index: usize,
        piece: &crate::IoSetPiece,
        set_min_level: i64,
        set_max_level: i64,
        set_attuned_only: bool,
        slotting: IoSlotting,
    ) -> Self {
        let set_id = set_id.into();
        let attuned = slotting.attuned || set_attuned_only;
        let kind = EnhancementKind::IoSet {
            set_id: set_id.clone(),
            set_name: set_name.into(),
            piece_num: piece.num,
            aspects: piece.aspects.clone(),
            is_proc: piece.proc,
            is_unique: piece.unique,
        };
        let level = if attuned {
            None
        } else {
            // An attuned-only set took the branch above, so the clamp lands on a
            // real level here; a catalog row that made it `0` would defer to the
            // build's global IO level, as an absent one does.
            Level::from_i64(
                i64::from(slotting.io_level)
                    .min(set_max_level)
                    .max(set_min_level),
            )
        };
        let boost =
            if slotting.boost > 0 && !kind.is_pure_proc() && !attuned && booster_floor_met(level) {
                slotting.boost
            } else {
                0
            };
        Enhancement {
            id: format!("{set_id}-{piece_index}"),
            name: piece.name.clone(),
            icon: String::new(),
            level,
            attuned,
            // Booster axis: unsigned by nature, so the widening is lossless.
            boost: i8::try_from(boost).unwrap_or(i8::MAX),
            kind,
        }
    }

    /// Whether a catalyst could attune this piece. Set pieces only: a common IO, an SO and a
    /// Hamidon have no attunement state at all, so "not attuned" is a fact about them rather
    /// than a state left to change.
    pub fn takes_attunement(&self) -> bool {
        matches!(self.kind, EnhancementKind::IoSet { .. })
    }

    /// Whether this piece carries a craft level a build-wide re-level can write. An attuned
    /// piece does not: it scales with the character instead, which is why [`Self::io_set`]
    /// leaves its level `None`.
    pub fn takes_craft_level(&self) -> bool {
        !self.attuned
            && matches!(
                self.kind,
                EnhancementKind::IoSet { .. } | EnhancementKind::GenericIo { .. }
            )
    }

    /// Whether an Enhancement Booster can be combined into this piece — the rule
    /// [`Self::io_set`] stamps at pick time, asked of a piece that already exists. An attuned
    /// piece cannot take one, neither can a pure proc, which has no magnitude to scale, and
    /// neither can a piece crafted below [`BOOSTER_LEVEL_FLOOR`].
    pub fn takes_booster(&self) -> bool {
        self.takes_craft_level() && !self.kind.is_pure_proc() && booster_floor_met(self.level)
    }

    /// A stored booster the game refuses: the piece rides the booster axis (a set or generic
    /// IO — an Origin/Special `boost` is a signed relative level, a different mechanic on
    /// different curves) yet its own slotting rules no longer accept a combine
    /// ([`Self::takes_booster`]). Only state written by an older build or a foreign file can
    /// hold one — the constructors refuse the state — so a reader that meets it reconciles
    /// to zero rather than honouring a number the game cannot reach.
    pub fn holds_refused_booster(&self) -> bool {
        self.boost > 0
            && matches!(
                self.kind,
                EnhancementKind::IoSet { .. } | EnhancementKind::GenericIo { .. }
            )
            && !self.takes_booster()
    }

    /// A special enhancement (Hamidon/Titan/Hydra/D-Sync/Prestige), mirroring the
    /// beta `createSpecialEnhancement`. Each aspect carries its own authored
    /// percentage straight from the catalog def; the calc reads them directly, so
    /// nothing is derived here. `category` is the picker's family tag (`hamidon`,
    /// `d-sync`, …). Specials are never attuned and carry no craft level. `boost` is
    /// a signed RELATIVE LEVEL, not a booster combine, and is deliberately not
    /// clamped here: its domain belongs to the dataset's above/below curves and is
    /// resolved at read time by `coh_math`'s `enhancement_level_multiplier`.
    pub fn special(
        id: &str,
        def: &crate::SpecialEnhancementDef,
        category: impl Into<String>,
        boost: i8,
    ) -> Self {
        let category = category.into();
        Enhancement {
            id: format!("{category}-{id}"),
            name: def.name.clone(),
            icon: String::new(),
            level: None,
            attuned: false,
            boost,
            kind: EnhancementKind::Special {
                category,
                aspects: def.aspects.clone(),
            },
        }
    }

    /// An origin enhancement (TO/DO/SO), mirroring the beta `createOriginEnhancement`.
    /// The magnitude is derived by the calc from `tier` + the aspect's ED schedule
    /// (the export's origin-tier grid), so no authored value is stored — the `value`
    /// field is a placeholder the calc ignores (as with [`Self::generic_io`]). Only
    /// SO carries a character origin, and even then it's display/flavor only (the
    /// calc never keys value on it); `character_origin` is `None` until the build
    /// model tracks the character's origin. `boost` is a signed RELATIVE LEVEL —
    /// an SO under your combat level is WEAKER, and that half was previously
    /// unrepresentable. Unclamped for the same reason as [`Self::special`].
    pub fn origin(
        stat: impl Into<String>,
        tier: impl Into<String>,
        character_origin: Option<String>,
        boost: i8,
    ) -> Self {
        let stat = stat.into();
        let tier = tier.into();
        let origin = if tier == "SO" { character_origin } else { None };
        Enhancement {
            id: format!("origin-{tier}-{stat}"),
            name: format!("{stat} {tier}"),
            icon: String::new(),
            level: None,
            attuned: false,
            boost,
            kind: EnhancementKind::Origin {
                tier,
                origin,
                secondary_origin: None,
                stat,
                value: 0.0,
            },
        }
    }
}

/// The picker's global slotting defaults, stamped onto a set piece at pick time
/// (the beta's `{ attuned, level, boost }` options bag). `io_level` is the crafting
/// level the picker's Lv control holds; `boost` the catalyst booster level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IoSlotting {
    pub attuned: bool,
    pub io_level: u8,
    pub boost: u8,
}

/// The kind-specific half of an [`Enhancement`]. Enhancement stat/tier/origin values stay
/// as wire strings here — the enhancement-value curves in `coh_math` key off schedule
/// strings, not a stat enum, so a typed enum would add coupling without a consumer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum EnhancementKind {
    /// A piece of an IO set.
    #[serde(rename = "io-set")]
    IoSet {
        set_id: String,
        set_name: String,
        piece_num: u8,
        aspects: Vec<String>,
        is_proc: bool,
        is_unique: bool,
    },
    /// A common (generic) IO.
    #[serde(rename = "io-generic")]
    GenericIo { stat: String, value: f64 },
    /// A special enhancement (Hamidon/Titan/Hydra/D-Sync/prestige).
    Special {
        category: String,
        aspects: Vec<AspectValue>,
    },
    /// An origin enhancement (TO/DO/SO).
    Origin {
        tier: String,
        origin: Option<String>,
        secondary_origin: Option<String>,
        stat: String,
        value: f64,
    },
}

impl EnhancementKind {
    /// A proc carrying no aspects — nothing but a chance to fire, so there is no magnitude for
    /// a booster combine to scale. Stated once here because two places ask it: the picker as it
    /// stamps a piece's booster ([`Enhancement::io_set`]), and any later re-slotting, which has
    /// only the built piece to ask.
    pub fn is_pure_proc(&self) -> bool {
        matches!(
            self,
            EnhancementKind::IoSet {
                is_proc: true,
                aspects,
                ..
            } if aspects.is_empty()
        )
    }
}

/// One `(stat, value)` aspect of a special enhancement (`enhancement.ts:86`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AspectValue {
    pub stat: String,
    pub value: f64,
}

/// Incarnate loadout — one optional pick per slot (`incarnate.ts:101`). Pass 6
/// ([`crate::incarnate_effects`] + `coh_math::incarnates`) reads it for the Destiny/Hybrid/
/// Genesis stat bonuses and the level shift, and the apply pass reads the equipped Alpha's
/// ED-bypass enhancement of the build's other powers (`combine_with_alpha_ed`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IncarnateLoadout {
    pub alpha: Option<IncarnateSlot>,
    pub judgement: Option<IncarnateSlot>,
    pub interface: Option<IncarnateSlot>,
    pub destiny: Option<IncarnateSlot>,
    pub lore: Option<IncarnateSlot>,
    pub hybrid: Option<IncarnateSlot>,
    /// Rebirth-only; `None` on datasets without a Genesis slot.
    pub genesis: Option<IncarnateSlot>,
}

impl IncarnateLoadout {
    /// Every slot id this loadout models, in catalog order. Beside [`Self::slot_ref`]'s match
    /// rather than anywhere else, so the list and the fields it names cannot drift apart.
    pub const SLOT_IDS: [&'static str; 7] = [
        "alpha",
        "judgement",
        "interface",
        "destiny",
        "lore",
        "hybrid",
        "genesis",
    ];

    /// The filled slots, as `(slot id, pick)` in [`Self::SLOT_IDS`] order — what a serializer
    /// keyed by slot id writes, instead of seven named fields (a fork that grows a slot then
    /// costs no schema).
    pub fn occupied(&self) -> impl Iterator<Item = (&'static str, &IncarnateSlot)> {
        Self::SLOT_IDS
            .into_iter()
            .filter_map(|slot_id| Some((slot_id, self.get(slot_id)?)))
    }

    /// The pick in the slot named by the catalog's slot id (`alpha` … `genesis`).
    /// An unknown id is `None` — the same answer as an empty slot, since a slot
    /// this loadout doesn't model can hold nothing.
    pub fn get(&self, slot_id: &str) -> Option<&IncarnateSlot> {
        self.slot_ref(slot_id).and_then(|slot| slot.as_ref())
    }

    /// Write (or clear, with `None`) the pick in the named slot. `false` when the
    /// id names no slot — the caller surfaces that, never silently drops the pick.
    #[must_use]
    pub fn set(&mut self, slot_id: &str, pick: Option<IncarnateSlot>) -> bool {
        match self.slot_mut(slot_id) {
            Some(slot) => {
                *slot = pick;
                true
            }
            None => false,
        }
    }

    fn slot_ref(&self, slot_id: &str) -> Option<&Option<IncarnateSlot>> {
        match slot_id {
            "alpha" => Some(&self.alpha),
            "judgement" => Some(&self.judgement),
            "interface" => Some(&self.interface),
            "destiny" => Some(&self.destiny),
            "lore" => Some(&self.lore),
            "hybrid" => Some(&self.hybrid),
            "genesis" => Some(&self.genesis),
            _ => None,
        }
    }

    fn slot_mut(&mut self, slot_id: &str) -> Option<&mut Option<IncarnateSlot>> {
        match slot_id {
            "alpha" => Some(&mut self.alpha),
            "judgement" => Some(&mut self.judgement),
            "interface" => Some(&mut self.interface),
            "destiny" => Some(&mut self.destiny),
            "lore" => Some(&mut self.lore),
            "hybrid" => Some(&mut self.hybrid),
            "genesis" => Some(&mut self.genesis),
            _ => None,
        }
    }
}

/// A single incarnate pick — identity plus the dashboard toggle. The tree/tier/display
/// data resolves from the dataset; the effect values from [`crate::incarnate_effects`],
/// keyed by [`normalize_incarnate_power_id`](crate::incarnate_effects::normalize_incarnate_power_id)
/// of `power_name`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncarnateSlot {
    /// The incarnate power's `internalName` (normalized to the effect-table key at calc time).
    pub power_name: String,
    /// Toggled on for dashboard totals. Alpha/Destiny/Hybrid gate their stat contribution on
    /// it; Judgement/Lore/Interface don't feed player stats, so their toggle is cosmetic (the
    /// level shift is gated separately). Read by Pass 6 (`coh_math::incarnates`).
    pub active: bool,
}

/// One slot-addition record for leveling mode (`build.ts:182`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlotOrderEntry {
    /// The power (by `internalName`) the slot was added to.
    pub power_name: String,
    /// Which slot index (1+) this record added.
    pub slot_index: u8,
    /// Category disambiguator for same-named powers across categories.
    pub category: Option<String>,
    /// The grant-pool level this slot was assigned at (Mids-style freed-level return).
    pub level: Option<u8>,
    /// Where [`Self::level`] came from — the discriminator MBDEXPORT-21 is about.
    ///
    /// `authored` is a placement someone made: in the planner's own UI, or in the `.mbd` this
    /// build was imported from. `packed` is the respec solver's wholesale fill, which wears the
    /// same field and is not a chronology. `None` is UNSTATED, not a default: an entry written
    /// before this field existed, whose provenance is genuinely unknown.
    ///
    /// Carried here so a `.skif` round trip through this reader cannot quietly downgrade a
    /// `packed` level to an unstated one — an unstated level is CARRIED by the `.mbd` writer,
    /// so dropping the field would turn "the planner filled this in" back into "the author
    /// placed it here", which is the exact claim this row exists to stop making.
    pub level_source: Option<SlotLevelSource>,
}

/// How a [`SlotOrderEntry::level`] came to be. See that field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotLevelSource {
    /// A placement — made in the planner, or read out of a `.mbd`'s own record.
    Authored,
    /// The respec solver's wholesale fill. A legal packing, not a levelling history.
    Packed,
}

/// Content mode for the purple-patch defense-softcap lookup ([`coh_math::purple_patch`],
/// which re-exports this rather than defining it — the calc crate can't own it because
/// [`CombatContext`] here needs it too, and `coh_math` depends on `coh_data`, not the
/// reverse). Incarnate-trial content layers an empirical ToHit buff onto enemies
/// (`PurplePatch::incarnate_to_hit_buff_pct`), raising the practical softcap; standard
/// content does not. Modeled as an enum (not a bool or a string) so the two modes are the
/// only representable states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentMode {
    #[default]
    Standard,
    Incarnate,
}

/// Combat-context inputs to the totals loop. Defaults are the beta `CalculationOptions`
/// baseline: solo, out of combat, even-level, Fury 0, team of one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CombatContext {
    /// In combat — gates `suppressible` atoms.
    pub in_combat: bool,
    /// Target level relative to the character (0 = even-con). Feeds the purple-patch
    /// effective-level-diff lookups in Pass 8.
    pub enemy_level_offset: i32,
    /// Brute Fury bar, 0..=100 — the Fury additive-damage input (Pass 3).
    pub fury_level: f64,
    /// Defender Vigilance team size, ≥1 — the Vigilance additive-damage input (Pass 3).
    pub vigilance_team_size: u32,
    /// Current health as a percentage, 0..=100 — the `kHitPoints%` runtime input for the
    /// HP-scaling regen/recovery Expressions (Gamma Boost). A live game-state value the user
    /// supplies, not an exportable one; the default is full health. A pre-field saved build
    /// loads at full health rather than the `f64` zero (which would read as death's-door).
    #[serde(default = "full_health_percent")]
    pub hit_points_percent: f64,
    /// Exemplar level being previewed, or `None` when not exemplared (the build runs at its
    /// design `level`). A live dashboard input like `hit_points_percent` — what the user is
    /// previewing, not a persisted build-design fact. Its ONLY calc effect is incarnate
    /// suppression (Pass 6): below level 45 every incarnate contribution turns off except the
    /// Genesis-Fate exemplar buff. It does NOT re-scale AT tables elsewhere in the pipeline —
    /// full exemplar-wide rescaling is a separate, larger ticket (this crate resolves AT tables
    /// at the build level). A pre-field saved build loads as `None` (not exemplared).
    #[serde(default, deserialize_with = "crate::level::deserialize_optional")]
    pub exemplar_level: Option<Level>,
    /// Seconds after cast to evaluate the diminishing Destiny buff at (Pass 6). Another live
    /// dashboard scrub like `exemplar_level` — what the user is previewing, not a persisted
    /// build-design fact — and the beta `destinyTime` option. `None` resolves each Destiny
    /// power at its sustained-floor time (the conservative value a perma-Destiny build holds);
    /// `Some(t)` reads the timeline at exactly `t`. Only affects Destiny powers that decay. A
    /// pre-field saved build loads as `None` (the sustained floor, the prior fixed behavior).
    #[serde(default)]
    pub destiny_time: Option<f64>,
    /// How many foes are standing inside an active Melee Hybrid's sphere (Pass 6). The Melee
    /// line stacks a Regeneration + Resistance (Core) or Regeneration + Defense (Radial) buff
    /// once per nearby enemy, up to a ceiling the equipped tier states (4, 7 or 9), and that
    /// per-foe layer had no input to read until this field.
    ///
    /// A live dashboard scrub in the same category as [`Self::destiny_time`] — what the player
    /// is previewing, not a persisted build-design fact. `None`, which is what a saved build
    /// predating the field loads as, reads as no foes and applies nothing, exactly the behaviour
    /// the pass had while the layer was cut. That default is deliberate rather than convenient:
    /// a solo total should not quietly assume a crowd.
    ///
    /// Deliberately NOT the per-power [`SelectedPower::targets_hit`], though both count nearby
    /// enemies. That one is stored per selection because it addresses one pick's own AoE; the
    /// Hybrid is not a pick at all (the loadout stores a slot and a power id), and its sphere is
    /// its own 8–10 foot radius, so a build reading Invincibility at 3 foes is making no claim
    /// about what is inside the Hybrid's smaller circle.
    #[serde(default)]
    pub hybrid_targets_hit: Option<u32>,
    /// How many of the loadout's earned incarnate level shifts to read the build with, or
    /// `None` for all of them (Pass 6). A live dashboard input like `destiny_time`, and the
    /// control the beta's `incarnateLevelShiftActive` boolean was: a pre-field saved build loads
    /// as `None`, which is the prior fixed behaviour.
    ///
    /// It exists because the shift a build can claim is not a property of the build. A full
    /// Alpha + Destiny + Lore loadout has earned +3, but only incarnate-flagged content grants
    /// all three — everywhere else the same character fights at a smaller shift, and reading the
    /// build at +3 overstates every purple-patch number. Which content grants what is a game
    /// rule that appears nowhere in the export, so the planner cannot derive this and must not
    /// branch on it (Rule 0): the player says which reading they want.
    ///
    /// A CEILING, not a magnitude — [`coh_math::incarnates::apply_level_shift`] spends it down
    /// the earned grants and can never exceed them. Setting it does not conjure a shift the
    /// loadout has not earned, so a value above what is equipped simply reads as "all of them".
    #[serde(default)]
    pub incarnate_level_shift: Option<f64>,
    /// Standard or Incarnate-trial content, for the purple-patch defense-softcap lookup
    /// (Pass 8). Sits beside [`Self::incarnate_level_shift`] on the same criterion: which
    /// content the build is being read against is a live dashboard input the player says,
    /// not a build-design fact. Engine-complete since [`coh_math::purple_patch`] shipped —
    /// this field is what was missing between it and a control. `#[serde(default)]` so a
    /// pre-field saved build loads as [`ContentMode::Standard`], the prior fixed behavior.
    #[serde(default)]
    pub content_mode: ContentMode,
    /// The caster is hidden — the from-Hide opener state. Gates the mid-combat cast a power
    /// carries beside its slow from-Hide animation (PROD6C-3k): hidden shows the slow cast,
    /// not hidden shows the fast one. A live dashboard input like `destiny_time`, and a
    /// pre-field saved build loads as not hidden.
    #[serde(default)]
    pub hidden: bool,
    /// The character is on a PvP map — the `isPVPMap?` a set-bonus tier's `Requires` asks about
    /// (BONUS-REQ-1). A PvP set states a second, different bonus at each piece count that the
    /// game applies only here, so the two readings of one build genuinely differ. A live
    /// dashboard input like `hidden`, and a pre-field saved build loads as PvE.
    #[serde(default)]
    pub pvp: bool,
    /// Which `scope: "global"` conditional effects are on, by conditional id — the caster-state
    /// mechanics that share one state across every power (stances, Domination, ammo modes).
    /// Absent means the entry's own `defaultActive` decides. Read by the display projection
    /// (PROD6C-3k); the TOTALS pass models the same states through the powers' `setsModes`
    /// gates instead ([`crate::CharacterState`] Pass 0), so this changes no total.
    #[serde(default)]
    pub global_conditionals: BTreeMap<String, bool>,
    /// Which `scope: "per-power"` conditional effects are on, keyed `"<internalName>:<id>"` —
    /// the TARGET-state mechanics (a foe drowning, disintegrating, contaminated) that each
    /// power tracks for itself. Absent means `defaultActive` decides.
    ///
    /// Keyed on the internal name alone, which is the within-build collision [`power_address`]
    /// describes: a target state set on an epic `Conserve_Power` also lands on the secondary
    /// one. Qualifying it means threading the owning set through `power_adjusters` and
    /// `effective_power`, which take a `Power` — and a `Power` does not carry its set. Left as
    /// debt rather than half-done: unlike [`Self::power_state`] and
    /// [`CharacterState::proc_overrides`], this map does not travel in a saved build, so no
    /// file format depends on the fix.
    #[serde(default)]
    pub per_power_conditionals: BTreeMap<String, bool>,
    /// Per-power switches the player threw on their own character, keyed the same way — today
    /// the buff-pet aura opt-ins (`coh_math::buff_pets`).
    ///
    /// The caster-side half of what used to share [`Self::per_power_conditionals`] with target
    /// state. They are separate fields because the two answer to OPPOSITE rules at the file
    /// boundary: a switch on your own character travels with a shared build, a foe's drowning
    /// is world state and opens at the planner's default. One map cannot say which a key is, so
    /// a codec reading it would have to sniff the key — and any caster-side mechanic added
    /// later would silently fail to travel.
    #[serde(default)]
    pub power_state: BTreeMap<String, bool>,
    /// Caster modes the player has switched on — the keys a power's `modeVariants` table is
    /// indexed by (`Peacebringer_Blaster_Mode`, `HunterMode`, `FastMode`, `SeismicPower`).
    /// While a mode is live the game's PowerRedirector fires a different record entirely, so the
    /// projection describes that record instead of the base one (PROD6C-3l). Display only —
    /// slots stay on the base power, which every mode shares — and a pre-field saved build loads
    /// with no mode live.
    #[serde(default)]
    pub active_modes: BTreeSet<String>,
    /// Who the build is being read against, for the target-side gates a per-power damage
    /// projection cannot avoid ([`crate::target_classes`]). Holds the export's own class token
    /// (`Class_Minion_Grunt`), because that is what the gates compare with — a rank enum here
    /// would be authoring buckets the data already draws, and would draw them wrong on the fork
    /// whose crit gates exclude one of its own minion classes.
    ///
    /// `None` — the default, and what a saved build predating the field loads as — means no
    /// target chosen, leaving every target gate unresolved. That is the honest state for the
    /// TOTALS, which have no one target and never read this; only the per-power projection does.
    #[serde(default)]
    pub target_class: Option<String>,
    /// Whether the target is another player — the `enttype target>` fork every fork's attacks
    /// carry, a separate scale (and on Homecoming a separate table) from the PvE one. A live
    /// dashboard input like [`Self::target_class`]; a pre-field saved build loads as PvE.
    #[serde(default)]
    pub target_is_player: bool,
    /// The what-if TEAM-BUFF layer: how much of each stat to pretend a teammate is handing the
    /// build, keyed by the `GlobalBonuses` field name the buff lands in
    /// (`"damage"`, `"toHit"`, `"recharge"`, …) and valued in that field's own units.
    ///
    /// A PREVIEW INPUT, not build design — the same category as [`Self::destiny_time`] and
    /// [`Self::hit_points_percent`]. It is deliberately not persisted with a build: a shared
    /// build carrying a hidden +damage would present a simulated number as the build's own
    /// (decision 2026-08-01). A pre-field saved build loads with the layer empty.
    ///
    /// The vocabulary is the accumulator's, not a list written here: a key is whatever
    /// `GlobalBonuses::add_by_camel_name` routes, so a stat with no accumulator behind it
    /// cannot be named rather than being named and quietly doing nothing. The injection lands
    /// in the accumulators BEFORE projection, at the same point the build's own globals do, so
    /// every archetype ceiling binds against a what-if exactly as it binds against a real buff.
    #[serde(default)]
    pub what_if_buffs: BTreeMap<String, f64>,
}

/// Full health — the `hit_points_percent` default, both for a fresh [`CombatContext`] and for a
/// saved build that predates the field.
fn full_health_percent() -> f64 {
    100.0
}

impl Default for CombatContext {
    fn default() -> Self {
        CombatContext {
            in_combat: false,
            enemy_level_offset: 0,
            fury_level: 0.0,
            vigilance_team_size: 1,
            hit_points_percent: full_health_percent(),
            exemplar_level: None,
            destiny_time: None,
            hybrid_targets_hit: None,
            incarnate_level_shift: None,
            content_mode: ContentMode::Standard,
            hidden: false,
            pvp: false,
            global_conditionals: BTreeMap::new(),
            per_power_conditionals: BTreeMap::new(),
            power_state: BTreeMap::new(),
            active_modes: BTreeSet::new(),
            target_class: None,
            target_is_player: false,
            what_if_buffs: BTreeMap::new(),
        }
    }
}

impl CombatContext {
    /// The target these inputs describe, for [`coh_math`]'s expression contexts: the class token
    /// gates compare against, and the entity type spelled as the gates spell it.
    ///
    /// There is ALWAYS a target, because there is always an answer to the PvE/PvP fork — which
    /// side of it a build is planning against is the `target_is_player` switch, defaulted like
    /// every other combat input. The class is the half that can be unstated, and the `None` there
    /// is a rank nobody has chosen rather than an absent target: gates reading `enttype` are
    /// answered, gates reading `arch` stay Indeterminate and report it.
    ///
    /// Requiring both halves at once is what made a fresh build show its damage as unresolved on
    /// 612 of Homecoming's 1120 powers — the entity fork was answerable the whole time, and only
    /// the 137 rank-gated powers ever needed the rank.
    pub fn target_identity(&self) -> (Option<String>, &'static str) {
        (
            self.target_class.clone(),
            if self.target_is_player {
                "player"
            } else {
                "critter"
            },
        )
    }
}
