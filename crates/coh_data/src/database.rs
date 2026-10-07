//! Bundle loading: `contract/<dataset>/bundle.json.gz` → an owned `PowerDatabase`.

use crate::archetype_stats::ArchetypeStats;
use crate::at_tables::AtTables;
use crate::atom_wire;
use crate::boost_index::BoostIndex;
use crate::enhancement_curves::EnhancementCurves;
use crate::enhancements::EnhancementCatalog;
use crate::incarnate_catalog::{IncarnateCatalog, IncarnateSlotCatalog};
use crate::incarnate_crafting::IncarnateCrafting;
use crate::incarnate_effects::IncarnateEffects;
use crate::io_sets::IoSetCatalog;
use crate::leveling_schedule::LevelingSchedule;
use crate::mids_enh_names::MidsEnhNames;
use crate::mids_names::MidsNames;
use crate::mids_uids::MidsUids;
use crate::pick_rules::SetPaths;
use crate::pools::PoolCatalog;
use crate::power::{Power, Powerset};
use crate::proc_data::ProcDatabase;
use crate::purple_patch::PurplePatch;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::Read;

/// Which dataset (CoH server) a build or bundle targets. Serialized as the
/// lowercase wire string (`"homecoming"` …) so it matches the beta's `Build.serverId`
/// and keys the per-dataset persistence envelope (M3 item 10).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatasetId {
    Homecoming,
    Rebirth,
    Thunderspy,
    /// Homecoming's open beta — the Brainstorm server. Not a pre-release lane held back
    /// from users: Brainstorm is public and playable by anyone, so its data ships. It is
    /// re-pointed at each release cycle rather than retired, so this variant outlives any
    /// one patch and its CONTENT is what live is about to become.
    Brainstorm,
}

impl DatasetId {
    pub const ALL: [DatasetId; 4] = [
        DatasetId::Homecoming,
        DatasetId::Rebirth,
        DatasetId::Thunderspy,
        DatasetId::Brainstorm,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            DatasetId::Homecoming => "homecoming",
            DatasetId::Rebirth => "rebirth",
            DatasetId::Thunderspy => "thunderspy",
            DatasetId::Brainstorm => "brainstorm",
        }
    }

    /// Is this the Homecoming game, on either of its rings?
    ///
    /// Homecoming and Brainstorm are one game two shards apart, while Rebirth and
    /// Thunderspy are separate forks with their own content. A fact that follows from
    /// WHICH GAME the bytes are (Homecoming's Labyrinth accolades, its salvage table)
    /// holds for both rings; a fact about a fork does not. Written as a predicate because
    /// the alternative — `dataset == DatasetId::Homecoming` at each site — silently
    /// answers "no" for Brainstorm and drops the fork-shaped half of the claim.
    pub fn is_homecoming(&self) -> bool {
        matches!(self, DatasetId::Homecoming | DatasetId::Brainstorm)
    }

    /// Is this the shard whose content is still moving?
    ///
    /// Brainstorm is Homecoming's open beta — public and playable, which is why its data
    /// ships at all, but its numbers are what live is ABOUT to become rather than what live
    /// is. A planner reading it is planning against a moving target, and that is the one
    /// fact about a fork a reader needs surfaced rather than looked up.
    ///
    /// A predicate here rather than a table in the UI, for the reason [`from_wire`] records:
    /// a roster written twice is a roster that drifts, and the second copy is the one that
    /// never grows an arm when the roster does.
    ///
    /// [`from_wire`]: Self::from_wire
    pub fn is_open_beta(&self) -> bool {
        matches!(self, DatasetId::Brainstorm)
    }

    /// The name a user reads in the server picker.
    ///
    /// Distinct from [`as_str`], which is the wire id — it keys the contract directory,
    /// the export tree and every persisted build, so it must never move for a label's
    /// sake. "HC Brainstorm" names the SERVER a planner is planning against; "brainstorm"
    /// alone would not tell a Homecoming player which shard they were looking at.
    pub fn display_name(&self) -> &'static str {
        match self {
            DatasetId::Homecoming => "Homecoming",
            DatasetId::Rebirth => "Rebirth",
            DatasetId::Thunderspy => "Thunderspy",
            DatasetId::Brainstorm => "HC Brainstorm",
        }
    }

    /// The inverse of [`as_str`], derived from [`ALL`](Self::ALL) rather than written out.
    ///
    /// Two gate files hand-rolled this inverse as a `match` with a panicking fallthrough, and
    /// neither grew a Brainstorm arm when the roster did — a hand table is a roster, and a
    /// roster written twice is a roster that drifts. Searching `ALL` means the mapping cannot
    /// disagree with `as_str` or omit a variant, because there is only one list. Returns
    /// `Option` rather than defaulting: an unrecognized wire id is the caller's to report
    /// loudly, and picking a plausible dataset for it would replay the fold Rule 1 forbids.
    pub fn from_wire(s: &str) -> Option<DatasetId> {
        DatasetId::ALL.into_iter().find(|d| d.as_str() == s)
    }
}

/// The one bundle schema this decoder understands. Checked at every load edge —
/// `from_bundle_json` rejects a bundle claiming any other version, and
/// [`assert_schema_version`] additionally pins the atom tuple field ORDER (a tuple-tail
/// reorder keeps `schema: 1` but must not decode).
pub const SUPPORTED_SCHEMA: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("gzip: {0}")]
    Gzip(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Decode(String),
    #[error("manifest count mismatch: {0}")]
    CountMismatch(String),
}

#[derive(Debug, Deserialize)]
pub struct ManifestCounts {
    pub powersets: usize,
    pub powers: usize,
    pub atoms: usize,
}

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub dataset: String,
    pub schema: u32,
    pub counts: ManifestCounts,
    #[serde(default)]
    pub provenance: Value,
}

/// A pool or epic power together with the aggregate that owns it.
///
/// Both partitions are flat vectors, but an epic power's identity is unique only WITHIN its
/// mastery — every archetype's epic pool republishes the same internal names at its own
/// scales — so a lookup that ignores the set answers with another archetype's copy. The
/// powerset partition needs no such tag: its [`Powerset`] already owns its powers.
#[derive(Debug, Clone, PartialEq)]
pub struct PartitionPower {
    /// The `id` of the pool / epic aggregate this power was collected from.
    pub set_id: String,
    pub power: Power,
}

/// One dataset, fully owned in memory. ~40k atoms; loads in milliseconds.
pub struct PowerDatabase {
    pub manifest: Manifest,
    pub powersets: Vec<Powerset>,
    /// Pool + epic powers, collected from their aggregates — the SAME `Power` shape, each
    /// tagged with the aggregate that owns it ([`PartitionPower`]).
    pub pool_powers: Vec<PartitionPower>,
    pub epic_powers: Vec<PartitionPower>,
    /// The two partitions' aggregates — what each pool is CALLED and which powers it
    /// offers, which the flat vectors above drop. Only the picker reads it; the calc
    /// addresses powers by `set_id` + ident and never needs the aggregate.
    pub pool_catalog: PoolCatalog,
    /// Each holdable set's binary name, keyed by the id a build stores — the input
    /// [`crate::pick_rules`] matches a `requires` set path against. Derived from the records
    /// above at load, since it is a whole-dataset fact and the gates are read per render.
    pub set_paths: SetPaths,
    /// The BASIC and PRESTIGE inherents every character carries — Brawl, Sprint, Rest, Ninja
    /// Run, the prestige sprints. They live in the `levels` section (the beta's
    /// `BASIC_INHERENT_POWERS` / `PRESTIGE_SPRINT_POWERS`), not in any powerset or pool, so
    /// before this they resolved nowhere and a build running Sprint got none of its +100% run
    /// speed. Deduped against every other origin, so the fitness inherents (which ARE pool
    /// powers) are not doubled. Deliberately NOT in [`Self::all_powers`]: that feeds the
    /// manifest count check, and these are a lookup fallback, not new corpus.
    pub inherent_powers: Vec<Power>,
    /// Archetype modifier tables, parsed from the `at-tables` section. The scaled-effect
    /// resolver (`coh_math`) reads these to turn a `{ scale, table }` into a number
    /// (Pass 1 onward). The raw section is retained in [`sections`] too — this is the
    /// typed view, not a move.
    pub at_tables: AtTables,
    /// Combat level-difference scaling tables (base ToHit, combat modifier, defense
    /// softcap), parsed from the `purple-patch` section. Pass 8 (`finalize`) reads
    /// these via the `coh_math::purple_patch` lookups to project hit-chance / combat
    /// modifier / defense softcap. The raw section is retained in [`sections`] too.
    pub purple_patch: PurplePatch,
    /// Per-archetype cap tables (resistance cap, damage-strength cap, per-level HP / HP-cap
    /// tables), parsed from the `archetype-stats` section. Pass 8 (`finalize`) reads these
    /// to clamp the projected display stats (D4). The raw section is retained in [`sections`]
    /// too — this is the typed view, not a move.
    pub archetype_stats: ArchetypeStats,
    /// Per-dataset enhancement-curve data (ED thresholds, boost-type→schedule
    /// assignment, per-level strength curves, …), parsed from the
    /// `enhancement-curves` section. The `coh_math::enhancement` lookups take
    /// these as input. `None` when the section is absent (a hand-constructed
    /// `PowerDatabase`) — consumers surface that as an error, never a default.
    /// The raw section is retained in [`sections`] too.
    pub enhancement_curves: Option<EnhancementCurves>,
    /// Per-slot incarnate effect tables (Alpha/Destiny/Hybrid/Genesis/…), parsed
    /// from the `incarnate` section. Pass 6 (`coh_math::incarnates`) reads these
    /// to apply the Destiny/Hybrid/Genesis stat bonuses and the incarnate level
    /// shift. The raw section is retained in [`sections`] too — this is the typed
    /// view, not a move.
    pub incarnate_effects: IncarnateEffects,
    /// Per-slot incarnate pick lists (identity/display), parsed from the
    /// `incarnate-catalog` section — the dataset's own export indices, feeding
    /// the incarnate picker. Which slots are OFFERED is gated through
    /// [`Self::offered_incarnate_slots`], not stored here. The raw section is
    /// retained in [`sections`] too — this is the typed view, not a move.
    pub incarnate_catalog: IncarnateCatalog,
    pub incarnate_crafting: IncarnateCrafting,
    /// Per-dataset IO-set catalog (set/piece data), parsed from the `io-sets`
    /// section. The `coh_math::enhancement` aggregation reads this as the beta
    /// `getIOSet` lookup. `None` when the section is absent (a hand-constructed
    /// `PowerDatabase`) — consumers surface that as an error, never a default.
    /// The raw section is retained in [`sections`] too.
    pub io_sets: Option<IoSetCatalog>,
    /// Per-dataset boost index (the name the game client prints for an
    /// enhancement -> the section that describes it), parsed from the
    /// `boost-index` section. A game-client import resolves slotted
    /// enhancements through it. `None` when the section is absent (a
    /// hand-constructed `PowerDatabase`) — consumers surface that as an error,
    /// never a default. The raw section is retained in [`sections`] too.
    pub boost_index: Option<BoostIndex>,
    /// Mids Reborn's enhancement-UID namespace for this fork, parsed from the
    /// `mids-uids` section. The `.mbd` reader consults it AFTER [`boost_index`],
    /// for the spellings where Mids has drifted from the game and only Mids can
    /// say so. `None` when the section is absent (a hand-constructed
    /// `PowerDatabase`) — consumers surface that as an error, never a default.
    pub mids_uids: Option<MidsUids>,
    /// Mids' power-name namespace for this fork, both directions, parsed from the
    /// `mids-names` section. A `.mbd` names a power by internal name alone and that
    /// namespace has rotated under stable display names, so the reader joins through
    /// here rather than matching the name it was given (MBDIMPORT-2). `None` when the
    /// section is absent — consumers surface that as an error, never a default.
    pub mids_names: Option<MidsNames>,
    /// Mids' enhancement SHORT-NAME namespace and array order for this fork, parsed
    /// from the `mids-enh-names` section. Only the legacy `.mxd` reader needs it: that
    /// format names an enhancement by a short code in its post half and by an array
    /// index in its compressed half, and neither is a name the export owns. `None` when
    /// the section is absent — consumers surface that as an error, never a default.
    pub mids_enh_names: Option<MidsEnhNames>,
    /// Per-dataset enhancement catalog (the pickable generic-IO/origin/special
    /// families), parsed from the `enhancements` section. The slotting UI reads
    /// this to offer the enhancements a power's allow-lists accept. `None` when
    /// the section is absent (a hand-constructed `PowerDatabase`) — consumers
    /// surface that as an error, never a default. The raw section is retained in
    /// [`sections`] too.
    pub enhancements: Option<EnhancementCatalog>,
    /// Per-dataset proc / global-IO effect database, parsed from the `proc-data`
    /// section (the beta binary-sourced `PROC_DATABASE`). `coh_math::procs` reads
    /// this for the proc pass (always-on globals, PPM, Build-Up, variable procs).
    /// Empty when the section is absent. The raw section is retained in
    /// [`sections`] too.
    pub procs: ProcDatabase,
    /// Per-dataset level-gated slot/pick budget (the beta `SLOT_GRANTS` /
    /// `POWER_PICK_LEVELS`, sourced from `schedules.bin`), parsed from the
    /// `leveling-schedule` section. The slot-economy budget (`placed_budget_slots`,
    /// `total_slots_at_level`) reads this. `None` when the section is absent (a
    /// hand-constructed `PowerDatabase`) — consumers surface that as an error,
    /// never a default. The raw section is retained in [`sections`] too.
    pub leveling_schedule: Option<LevelingSchedule>,
    /// Supporting sections, raw until their consuming pass is ported
    /// (at-tables, archetypes, archetype-stats, pet-entities, levels).
    pub sections: Map<String, Value>,
}

impl PowerDatabase {
    pub fn from_gz_bytes(bytes: &[u8]) -> Result<Self, LoadError> {
        let mut json = String::new();
        GzDecoder::new(bytes).read_to_string(&mut json)?;
        Self::from_bundle_json(&json)
    }

    pub fn from_bundle_json(json: &str) -> Result<Self, LoadError> {
        let mut root: Map<String, Value> = serde_json::from_str(json)?;

        let manifest: Manifest = serde_json::from_value(
            root.remove("manifest")
                .ok_or_else(|| LoadError::Decode("bundle has no manifest".into()))?,
        )?;
        if manifest.schema != SUPPORTED_SCHEMA {
            return Err(LoadError::Decode(format!(
                "bundle declares schema {} but this decoder supports schema {SUPPORTED_SCHEMA}",
                manifest.schema
            )));
        }

        let powersets_value = root
            .remove("powersets")
            .ok_or_else(|| LoadError::Decode("bundle has no powersets".into()))?;
        let Value::Object(registry) = powersets_value else {
            return Err(LoadError::Decode("powersets is not an object".into()));
        };
        let mut powersets = Vec::with_capacity(registry.len());
        for (key, ps) in registry {
            powersets.push(Powerset::from_value(&key, ps).map_err(LoadError::Decode)?);
        }

        let pools_section = root
            .remove("power-pools")
            .ok_or_else(|| LoadError::Decode("bundle has no power-pools".into()))?;
        let epics_section = root
            .remove("epic-pools")
            .ok_or_else(|| LoadError::Decode("bundle has no epic-pools".into()))?;
        // The aggregates' own identity is read BEFORE the sections are flattened — the
        // partitions below keep only the powers, and both sections leave `sections` here,
        // so a later lazy reader would find nothing to read.
        let pool_catalog = PoolCatalog::from_sections(&pools_section, &epics_section)
            .map_err(LoadError::Decode)?;
        let pool_powers = collect_powers(pools_section).map_err(LoadError::Decode)?;
        let epic_powers = collect_powers(epics_section).map_err(LoadError::Decode)?;

        // The basic/prestige inherents ride the `levels` section as ordinary power-shaped nodes
        // (name + effects bag), which is exactly what `collect_powers` picks up. Everything else
        // in that section (schedules, requirement tables) is not power-shaped and is skipped.
        let mut inherent_powers: Vec<Power> = match root.get("levels") {
            Some(levels) => collect_powers_including_damage_only(levels.clone())
                .map_err(LoadError::Decode)?
                .into_iter()
                .map(|p| p.power)
                .collect(),
            None => Vec::new(),
        };
        {
            let existing: std::collections::HashSet<&str> = powersets
                .iter()
                .flat_map(|ps| ps.powers.iter())
                .chain(pool_powers.iter().map(|p| &p.power))
                .chain(epic_powers.iter().map(|p| &p.power))
                .map(Power::ident)
                .collect();
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            inherent_powers
                .retain(|p| !existing.contains(p.ident()) && seen.insert(p.ident().to_string()));
        }

        let at_tables = AtTables::from_section(root.get("at-tables")).map_err(LoadError::Decode)?;
        let purple_patch =
            PurplePatch::from_section(root.get("purple-patch")).map_err(LoadError::Decode)?;
        let archetype_stats =
            ArchetypeStats::from_section(root.get("archetype-stats")).map_err(LoadError::Decode)?;
        let boost_index =
            BoostIndex::from_section(root.get("boost-index")).map_err(LoadError::Decode)?;
        // The curves are read at the levels the boost index names, so the index
        // is parsed first and handed over (BOOST-6).
        let enhancement_curves = EnhancementCurves::from_section(
            root.get("enhancement-curves"),
            boost_index.as_ref().map_or(&[], BoostIndex::craft_levels),
        )
        .map_err(LoadError::Decode)?;
        let incarnate_effects =
            IncarnateEffects::from_section(root.get("incarnate")).map_err(LoadError::Decode)?;
        let incarnate_catalog = IncarnateCatalog::from_section(root.get("incarnate-catalog"))
            .map_err(LoadError::Decode)?;
        let incarnate_crafting = IncarnateCrafting::from_section(root.get("incarnate-crafting"))
            .map_err(LoadError::Decode)?;
        let io_sets = IoSetCatalog::from_section(root.get("io-sets")).map_err(LoadError::Decode)?;
        let mids_uids = MidsUids::from_section(root.get("mids-uids")).map_err(LoadError::Decode)?;
        let mids_names =
            MidsNames::from_section(root.get("mids-names")).map_err(LoadError::Decode)?;
        let mids_enh_names =
            MidsEnhNames::from_section(root.get("mids-enh-names")).map_err(LoadError::Decode)?;
        let enhancements = EnhancementCatalog::from_section(root.get("enhancements"))
            .map_err(LoadError::Decode)?;
        let procs = ProcDatabase::from_section(root.get("proc-data")).map_err(LoadError::Decode)?;
        let leveling_schedule = LevelingSchedule::from_section(root.get("leveling-schedule"))
            .map_err(LoadError::Decode)?;
        if let Some(curves) = &enhancement_curves {
            if curves.dataset != manifest.dataset {
                return Err(LoadError::Decode(format!(
                    "enhancement-curves section carries dataset id \"{}\" in a \"{}\" bundle",
                    curves.dataset, manifest.dataset
                )));
            }
        }
        if let Some(schedule) = &leveling_schedule {
            if schedule.dataset != manifest.dataset {
                return Err(LoadError::Decode(format!(
                    "leveling-schedule section carries dataset id \"{}\" in a \"{}\" bundle",
                    schedule.dataset, manifest.dataset
                )));
            }
        }

        let set_paths = SetPaths::of_dataset(&powersets, &pool_catalog);

        let db = PowerDatabase {
            manifest,
            powersets,
            pool_powers,
            epic_powers,
            pool_catalog,
            set_paths,
            inherent_powers,
            at_tables,
            purple_patch,
            archetype_stats,
            enhancement_curves,
            incarnate_effects,
            incarnate_catalog,
            incarnate_crafting,
            io_sets,
            boost_index,
            mids_uids,
            mids_names,
            mids_enh_names,
            enhancements,
            procs,
            leveling_schedule,
            sections: root,
        };
        db.verify_counts()?;
        Ok(db)
    }

    /// The incarnate slots the picker offers on THIS dataset: catalog slots whose
    /// matching effects table is non-empty. All three forks export a genesis
    /// powerset, but off-Rebirth it is dormant data the game never serves
    /// (GENESIS-1) — its effect tables are empty, which is the data-carried form
    /// of that dormancy, so the emptiness IS the gate (no dataset-name list).
    pub fn offered_incarnate_slots(&self) -> Vec<&IncarnateSlotCatalog> {
        self.incarnate_catalog
            .slots
            .iter()
            .filter(|slot| self.incarnate_effects.has_effects_for_slot(&slot.id))
            .collect()
    }

    /// Resolve a powerset-partition power by identity (powerset `id` + [`Power::ident`]).
    /// `None` when this dataset has no such powerset or power — e.g. a selection carried
    /// across a dataset switch.
    pub fn find_power(&self, powerset_id: &str, power_ident: &str) -> Option<&Power> {
        self.powersets
            .iter()
            .find(|ps| ps.id == powerset_id)?
            .powers
            .iter()
            .find(|p| p.ident() == power_ident)
    }

    /// The archetype catalog (names, primary/secondary set ids, inherents) parsed on
    /// demand from the retained `archetypes` section. The selection UI is the only
    /// consumer and memoizes the result, so this stays a lazy view rather than an eager
    /// `PowerDatabase` field. `Err` on a malformed section (Rule 1); an empty catalog on a
    /// hand-built database that carries no `archetypes` section.
    pub fn archetypes(&self) -> Result<crate::archetypes::Archetypes, String> {
        crate::archetypes::Archetypes::from_section(self.sections.get("archetypes"))
    }

    /// The powerset with this `id`, or `None`.
    pub fn find_powerset(&self, powerset_id: &str) -> Option<&Powerset> {
        self.powersets.iter().find(|ps| ps.id == powerset_id)
    }

    /// The game's own class token for an archetype id (`"blaster"` → `"Class_Blaster"`),
    /// or `None` when the dataset does not state one.
    ///
    /// The single place that mapping is made. An archetype-forked atom names the classes it
    /// belongs to in this spelling ([`crate::AtomicEffect::caster_archetypes`]), so every
    /// consumer that filters atoms for one build needs it — and rebuilding it from the
    /// hyphenated id at each site would be authoring a naming convention the export already
    /// states (AT-FORK-1).
    pub fn class_name_of(&self, archetype: &str) -> Option<&str> {
        let class_name = self.archetype_stats.get(archetype)?.class_name.as_str();
        (!class_name.is_empty()).then_some(class_name)
    }

    /// The typed id of the fork this bundle declares, or `None` when the manifest names one
    /// this build does not know — which is a bundle the caller must reject, never a dataset to
    /// guess at (Rule 1). Engine entry points take the fork as an argument instead, because a
    /// hand-built `PowerDatabase` states no manifest dataset at all.
    pub fn dataset_id(&self) -> Option<DatasetId> {
        DatasetId::from_wire(&self.manifest.dataset)
    }

    /// The dataset's player class roster, in the spelling an archetype-forked atom names
    /// (`Class_Blaster`, …) — the input `window_slots::bag_slots` votes over.
    ///
    /// Whole-dataset fact, so it is answered from the typed `archetype-stats` view rather
    /// than by parsing the `archetypes` section per call: the display bag asks this once per
    /// power per render.
    pub fn player_classes(&self) -> Vec<&str> {
        self.archetype_stats.class_names()
    }

    /// The inherent powers this dataset grants outright, parsed on demand from the retained
    /// `levels` section — a lazy view like [`Self::archetypes`], since only the build's
    /// grant step reads it. `Ok(None)` on a database with no `levels` section.
    pub fn inherent_grants(&self) -> Result<Option<crate::InherentGrants>, String> {
        crate::InherentGrants::from_section(self.sections.get("levels"))
    }

    /// The power that carries an archetype's own inherent, matched by the display NAME the
    /// archetype declares for it (`Archetype::inherent.name`) against the `Inherent`
    /// powerset the converter builds.
    ///
    /// Matching on the declared name is what keeps the archetype→inherent mapping in the
    /// data: the two sides are independent exports of the same fact, and one naming a power
    /// the other does not ship is a real dataset gap the caller must show (one fork declares
    /// an archetype inherent it has no power for). A positional or hand-written mapping
    /// would paper over exactly that.
    pub fn archetype_inherent(&self, inherent_name: &str) -> Option<&Power> {
        self.find_powerset(crate::INHERENT_SET)?
            .powers
            .iter()
            .find(|power| power.name == inherent_name)
    }

    /// Every power reachable from this dataset, one shape, any origin.
    pub fn all_powers(&self) -> impl Iterator<Item = &Power> {
        self.powersets
            .iter()
            .flat_map(|ps| ps.powers.iter())
            .chain(self.pool_powers.iter().map(|p| &p.power))
            .chain(self.epic_powers.iter().map(|p| &p.power))
    }

    /// A basic/prestige inherent (Sprint, Brawl, Rest, Ninja Run, the prestige sprints) by
    /// ident. The last-resort arm of the selected-power lookup: these carry no powerset of
    /// their own, and the build stores them all under the synthetic `Inherent` set.
    pub fn find_inherent_power(&self, power_ident: &str) -> Option<&Power> {
        self.inherent_powers
            .iter()
            .find(|p| p.ident() == power_ident)
    }

    /// A GRANTED inherent's def by ident alone, wherever this dataset keeps it.
    ///
    /// Granted inherents are the one selection with no real owning set — the build tags them
    /// all `Inherent` — so they cannot be resolved set-scoped like every other power. Three
    /// places hold them, and all three are needed: [`Self::inherent_powers`] for the basic and
    /// prestige ones, the synthetic `Inherent` powerset for the archetype inherents, and the
    /// pool partition for the fitness ones (which the loader drops from `inherent_powers`
    /// precisely because the legacy Fitness pool still publishes them).
    ///
    /// Ident alone is safe here in a way it is not for epic powers: these names are unique
    /// across the whole corpus, which is what lets the loader dedupe them in the first place.
    pub fn find_granted_power(&self, power_ident: &str) -> Option<&Power> {
        self.find_inherent_power(power_ident)
            .or_else(|| {
                self.find_powerset(crate::INHERENT_SET)?
                    .powers
                    .iter()
                    .find(|power| power.ident() == power_ident)
            })
            .or_else(|| {
                self.pool_powers
                    .iter()
                    .find(|entry| entry.power.ident() == power_ident)
                    .map(|entry| &entry.power)
            })
    }

    /// Resolve a SELECTED power's def — the lookup that answers "what did the build pick?"
    /// wherever this dataset keeps that power.
    ///
    /// Every set-scoped path is tried before any unscoped one: the named powerset, then the
    /// pool/epic aggregate with that same id. Epic masteries republish one another's internal
    /// names at their own scales, so an unscoped pool/epic match answers with a different
    /// archetype's copy of the power whenever the scoped lookup would have succeeded — which
    /// is why the scoped attempt comes first rather than last.
    ///
    /// The unscoped buckets remain as a fallback for the selections that carry a set id no
    /// aggregate owns (an inherent, or a pool power whose set was renamed away from its
    /// `fullName`), where an identity match is the only handle available. The basic and
    /// prestige inherents come last: they are in no powerset and no pool — the build stores
    /// them under the synthetic [`INHERENT_SET`](crate::INHERENT_SET) — so without that arm
    /// every one of them resolved nowhere and silently contributed zero.
    pub fn resolve_power(&self, set_id: &str, power_ident: &str) -> Option<&Power> {
        if let Some(power) = self.find_power(set_id, power_ident) {
            return Some(power);
        }
        if let Some(power) = self.find_partition_power(set_id, power_ident) {
            return Some(power);
        }
        self.pool_powers
            .iter()
            .chain(self.epic_powers.iter())
            .find(|entry| entry.power.ident() == power_ident)
            .map(|entry| &entry.power)
            .or_else(|| self.all_powers().find(|power| power.ident() == power_ident))
            .or_else(|| self.find_inherent_power(power_ident))
    }

    /// A pool or epic power by the aggregate that owns it plus its [`Power::ident`] — the
    /// set-scoped lookup an epic power needs, since the masteries share internal names.
    pub fn find_partition_power(&self, set_id: &str, power_ident: &str) -> Option<&Power> {
        self.pool_powers
            .iter()
            .chain(self.epic_powers.iter())
            .find(|p| p.set_id == set_id && p.power.ident() == power_ident)
            .map(|p| &p.power)
    }

    fn verify_counts(&self) -> Result<(), LoadError> {
        let powersets = self.powersets.len();
        let powers = self.all_powers().count();
        let atoms: usize = self.all_powers().map(|p| p.atoms.len()).sum();
        let c = &self.manifest.counts;
        if (powersets, powers, atoms) != (c.powersets, c.powers, c.atoms) {
            return Err(LoadError::CountMismatch(format!(
                "loaded ({powersets} powersets, {powers} powers, {atoms} atoms) \
                 vs manifest ({}, {}, {})",
                c.powersets, c.powers, c.atoms
            )));
        }
        Ok(())
    }
}

/// The walker's Power-shape rule, mirrored: a node with a string `name` and an `atoms` array
/// is a Power; pool/epic aggregates nest.
///
/// The bag used to be the other half of this rule, and a node carrying one without atoms is
/// now an ERROR rather than a Power ([`collect_into`]). That is the strip's last blocking
/// read: the predicate is what decided a node was loadable AT ALL, so leaving it counting a
/// key the bag strip is removing would have made the strip delete powers.
///
/// Pool and epic powers reach the bundle in the converter's LEGACY shape — no
/// `internalName`, no `stats`, and their execution stats under the pre-rename keys — so each
/// one is normalized on the way in ([`normalize_legacy_power`]).
fn collect_powers(value: Value) -> Result<Vec<PartitionPower>, String> {
    let mut out = Vec::new();
    collect_into(value, None, &mut out, false)?;
    Ok(out)
}

/// As [`collect_powers`], but a node carrying only `damage` (no `atoms`) also counts.
///
/// Used ONLY for the `levels` section. Brawl was the one power in the corpus authored as pure
/// damage, so the strict predicate skipped it and it resolved nowhere, which the engine then
/// reported as "not in this dataset" on every recalc of any build that has it (i.e. all of
/// them; Brawl is locked). It carries atoms now, and as of 2026-09-03 nothing in any fork's
/// `levels` matches on `damage` alone, so this arm is currently redundant. It stays because
/// `levels` is the one section that can afford it: inherent powers are excluded from
/// `all_powers()`, so widening here cannot desync the manifest count reconciliation the way
/// widening [`collect_powers`] would.
fn collect_powers_including_damage_only(value: Value) -> Result<Vec<PartitionPower>, String> {
    let mut out = Vec::new();
    collect_into(value, None, &mut out, true)?;
    Ok(out)
}

fn collect_into(
    value: Value,
    set_id: Option<&str>,
    out: &mut Vec<PartitionPower>,
    damage_only_counts: bool,
) -> Result<(), String> {
    match value {
        Value::Array(items) => {
            for v in items {
                collect_into(v, set_id, out, damage_only_counts)?;
            }
        }
        Value::Object(map) => {
            // An aggregate names itself; its powers and any aggregate nested under it belong
            // to that id until a deeper one renames the scope.
            let own_id = map.get("id").and_then(Value::as_str).map(str::to_string);
            let scope = own_id.as_deref().or(set_id).unwrap_or_default().to_string();

            let named = map.get("name").is_some_and(Value::is_string);
            let is_power = named
                && (map.get("atoms").is_some_and(Value::is_array)
                    || (damage_only_counts && map.get("damage").is_some_and(Value::is_array)));
            // Rule 1, at the seam the strip narrowed. The bag used to be an arm of this
            // predicate, so dropping it turned "loaded as a power" into "skipped" for any node
            // that carries a bag and no atoms — and a skipped pool power does not fail, it
            // just stops existing, which is the soft-wrong the mandate forbids. The measured
            // population is 0 on all four forks (bundle-wide, not just these sections), so
            // this errors on the empty set today and exists to stay that way.
            if named && !is_power && map.get("effects").is_some_and(Value::is_object) {
                let name = map.get("name").and_then(Value::as_str).unwrap_or("?");
                return Err(format!(
                    "{name}: carries an `effects` bag and no `atoms`, so the atom-shaped \
                     predicate skips it and the power would silently vanish. The bag is not a \
                     power shape any more — the converter owes this power its atoms."
                ));
            }
            if is_power {
                let mut normalized = map.clone();
                normalize_legacy_power(&mut normalized);
                out.push(PartitionPower {
                    set_id: scope.clone(),
                    power: Power::from_value(Value::Object(normalized))?,
                });
            }
            // Recurse even into a matched power: the JS walkers (emitter + fixture
            // sweep) do, so a power-shaped node nested inside another power counts on
            // both sides — diverging here would break the manifest reconciliation.
            for (_, v) in map {
                collect_into(v, Some(&scope), out, damage_only_counts)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// A toggle's tick interval when the wire omits it — the standard 0.5s tick.
const DEFAULT_ACTIVATE_PERIOD: f64 = 0.5;

/// Bring a legacy-shaped pool/epic power up to the powerset partition's shape.
///
/// The converter emits these two partitions in the pre-rename shape the beta's
/// `transformPoolPower` (`power-pools.ts`) adapts at runtime: the identity lives only in
/// `fullName`, and the execution stats sit under `endurance` / `activationTime` rather than
/// the `enduranceCost` / `castTime` the rest of the engine reads. Without this the partition
/// is doubly invisible — nothing can address a multi-word power by the `internalName` the
/// caller holds, and every projection reads through the renamed keys and finds nothing.
///
/// Applied only to the pool/epic sections, and only to fields the wire has not already
/// normalized, so a converter that starts emitting the modern shape silently stops needing
/// this rather than being overridden by it.
///
/// MBDIMPORT-17 took that exit for `internalName`: both pool emitters write the export's own
/// `name` now, so that branch is dead against a current contract. It stays for an older one,
/// where removing it would turn a missing identity into an empty one instead of a derived one.
/// What guards the field is outside here — `scripts/keys/mbdimport17-pool-internal-names.py`
/// reads the contract, not this function's output, so an emitter that stops writing the field
/// reds even though this would still cover for it.
fn normalize_legacy_power(power: &mut Map<String, Value>) {
    if !power.contains_key("internalName") {
        if let Some(ident) = legacy_internal_name(power) {
            power.insert("internalName".into(), Value::String(ident));
        }
    }

    // A toggle's wire endurance is per TICK; every other execution reader treats
    // `enduranceCost` on a pool power as the per-second drain the display shows, which is the
    // division `transformPoolPower` does at the same point.
    let is_toggle = power
        .get("powerType")
        .and_then(Value::as_str)
        .is_some_and(|kind| kind.eq_ignore_ascii_case("toggle"));

    let Some(Value::Object(effects)) = power.get_mut("effects") else {
        return;
    };
    if !effects.contains_key("enduranceCost") {
        if let Some(endurance) = effects.get("endurance").and_then(Value::as_f64) {
            let period = effects
                .get("activatePeriod")
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_ACTIVATE_PERIOD);
            let cost = if is_toggle {
                endurance / period
            } else {
                endurance
            };
            if let Some(cost) = serde_json::Number::from_f64(cost) {
                effects.insert("enduranceCost".into(), Value::Number(cost));
            }
        }
    }
    if !effects.contains_key("castTime") {
        if let Some(activation) = effects.get("activationTime").cloned() {
            effects.insert("castTime".into(), activation);
        }
    }
}

/// The identity `transformPoolPower` derives: the last dotted segment of `fullName` with
/// whitespace underscored, falling back to the display `name` the same way.
fn legacy_internal_name(power: &Map<String, Value>) -> Option<String> {
    let source = power
        .get("fullName")
        .and_then(Value::as_str)
        .and_then(|full| full.rsplit('.').next())
        .or_else(|| power.get("name").and_then(Value::as_str))?;
    Some(underscored(source))
}

/// The beta's `replace(/\s+/g, '_')`: every RUN of whitespace becomes one underscore,
/// leading and trailing runs included, so the two sides agree on every name shape.
fn underscored(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut previous_was_whitespace = false;
    for ch in value.chars() {
        if ch.is_whitespace() {
            if !previous_was_whitespace {
                out.push('_');
            }
            previous_was_whitespace = true;
            continue;
        }
        previous_was_whitespace = false;
        out.push(ch);
    }
    out
}

/// Parse `contract/schema-version.json` and assert the atom wire order matches the
/// decoder. Call once at startup with the contract you ship.
pub fn assert_schema_version(schema_json: &str) -> Result<(), LoadError> {
    #[derive(Deserialize)]
    struct SchemaVersion {
        schema: u32,
        #[serde(rename = "atomTupleFields")]
        atom_tuple_fields: Vec<String>,
    }
    let sv: SchemaVersion = serde_json::from_str(schema_json)?;
    if sv.schema != SUPPORTED_SCHEMA {
        return Err(LoadError::Decode(format!(
            "unsupported contract schema {}",
            sv.schema
        )));
    }
    atom_wire::assert_schema(&sv.atom_tuple_fields).map_err(LoadError::Decode)
}
