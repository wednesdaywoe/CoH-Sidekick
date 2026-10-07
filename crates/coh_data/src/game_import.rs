//! The `/buildsave` export's enhancements, resolved against a dataset.
//!
//! [`crate::game_export`] reads the text; this module turns what it read into the slotted
//! pieces a build carries. The two are separate because they fail differently — a line that
//! will not parse is a broken file, and a record this fork has never heard of is a build to
//! report on.
//!
//! **The name is looked up, never parsed.** `Crafted_Hecatomb_A` is resolved by asking
//! [`crate::boost_index`] what that record IS, because the index exists to answer exactly this
//! question (its own doc opens on the `/buildsave` case). Reading the parts of the name instead
//! — an `Attuned_` prefix for attunement, a trailing `_A` for the piece, an origin word for the
//! tier — re-derives from spelling what the data already states, and every such rule is a place
//! the game can disagree with us silently. The beta's importer does parse the name — see its
//! `parseIOSetUid`, `parseSOEnhancement` and `hasIOSetPieceSuffix`.
//!
//! **This paragraph carried a false claim until 2026-09-08**, and it is left visible rather than
//! quietly deleted because it is the shape of mistake this module exists to avoid. It said the
//! beta's external-JSON front end "drops the piece suffix before the lookup even happens, which
//! resolves all 2,076 Homecoming set pieces to a set-level name that is not a record" — read off
//! `convertBoost`'s `uid = titleCase(boost.powerSetName)` on the assumption that a boost's
//! `powerSetName` names its SET. It does not. The game addresses a boost as
//! `Boosts.<record>.<record>`: all 2,739 keys under `exported_powers/boosts/*/index.json` are
//! `Boosts.<full record name>`, so the powerset slot holds `Crafted_Crushing_Impact_B` itself.
//! The front end passes the record name and the piece resolves. Verified by running a crafted
//! set piece and an origin enhancement through the beta's own `importExternalBuild`: two
//! imported, zero failed, no warnings. **Inferring a field's meaning from its name is the same
//! error as inferring an effect's from its attrib name**, which is Rule 0's territory.
//!
//! **A piece that will not resolve is returned, not dropped.** Every refusal names the record
//! and why, so the caller can retain it and report it (rule 8) rather than shipping a build
//! quietly smaller than the one the player saved.

use crate::boost_index::{BoostEntry, BoostIndex, OriginTier};
use crate::enhancements::EnhancementCatalog;
use crate::game_export::ExportedEnhancement;
use crate::level::Level;
use crate::skif::SkifEnhancement;

/// A slotted piece that could not be resolved, and the record it named.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedPiece {
    /// The record name as the export spelled it, kept verbatim so a report can be searched for
    /// it and so a re-export can still write what the player had.
    pub uid: String,
    pub refusal: PieceRefusal,
}

/// Why a record could not become a slotted piece.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PieceRefusal {
    #[error("this dataset's boost index carries no record of that name")]
    UnknownRecord,
    #[error("the boost index places this record in no family, so there is nothing to slot")]
    Unclassified,
    #[error("the record enhances {named:?}, of which this dataset's catalog carries {carried:?}; a generic piece needs exactly one")]
    Stat {
        named: Vec<String>,
        carried: Vec<String>,
    },
    #[error("the record is crafted at level {record}, but the export states level {stated}")]
    LevelDisagrees { record: i64, stated: u32 },
    #[error("the export states {stated} boosters, which is more than a level can hold")]
    BoosterOutOfRange { stated: u32 },
}

/// Resolve one exported record into the piece a build slots.
///
/// `character_level` is needed only for the origin and special families, whose slotted form is
/// a level RELATIVE to the character ([`crate::Enhancement`]'s `boost` field on those kinds)
/// while the export states an absolute one. The subtraction can leave the dataset's own
/// attenuation curve — a level-15 SO on a level-28 character is -13, where Homecoming's `below`
/// curve reaches -3 — and it is stored anyway. `coh_math::enhancement_level_multiplier` clamps
/// to the end of the curve by explicit contract, naming an imported save as the case it exists
/// for; clamping here instead would throw away what the file said and leave the build unable to
/// say what the player actually had.
pub fn resolve_enhancement(
    exported: &ExportedEnhancement,
    character_level: Level,
    index: &BoostIndex,
    catalog: &EnhancementCatalog,
) -> Result<SkifEnhancement, UnresolvedPiece> {
    let refuse = |refusal| UnresolvedPiece {
        uid: exported.uid.clone(),
        refusal,
    };
    let entry = index
        .get(&exported.uid)
        .ok_or_else(|| refuse(PieceRefusal::UnknownRecord))?;

    match entry {
        BoostEntry::IoSet {
            set,
            piece,
            attuned,
        } => Ok(SkifEnhancement::IoSet {
            set_id: set.clone(),
            piece_num: *piece,
            attuned: *attuned,
            // An attuned piece has no level of its own — it scales with the character — and the
            // client prints `1` for every one of them. Carrying that `1` as a level would slot
            // every attuned piece at level 1.
            level: (!*attuned).then(|| crafted_level(exported)).flatten(),
            booster: booster(exported).map_err(refuse)?,
        }),

        BoostEntry::CommonIo { level, stats } => {
            if let Some(record) = level {
                if *record != i64::from(exported.stated_level) {
                    return Err(refuse(PieceRefusal::LevelDisagrees {
                        record: *record,
                        stated: exported.stated_level,
                    }));
                }
            }
            Ok(SkifEnhancement::IoGeneric {
                stat: catalog_stat(stats, catalog).map_err(refuse)?,
                level: crafted_level(exported),
                booster: booster(exported).map_err(refuse)?,
            })
        }

        BoostEntry::Origin { tier, stats, .. } => Ok(SkifEnhancement::Origin {
            stat: catalog_stat(stats, catalog).map_err(refuse)?,
            tier: tier_name(*tier).to_string(),
            relative_level: relative_level(exported.stated_level, character_level),
        }),

        BoostEntry::Special { family, id } => Ok(SkifEnhancement::Special {
            category: family.clone(),
            id: id.clone(),
            relative_level: relative_level(exported.stated_level, character_level),
        }),

        BoostEntry::Unclassified => Err(refuse(PieceRefusal::Unclassified)),
    }
}

/// The tier as the build spells it. The index and the build agree on `TO`/`DO`/`SO`; this
/// exists so the agreement is one `match` the compiler checks rather than a shared literal.
fn tier_name(tier: OriginTier) -> &'static str {
    match tier {
        OriginTier::Training => "TO",
        OriginTier::Dual => "DO",
        OriginTier::Single => "SO",
    }
}

fn crafted_level(exported: &ExportedEnhancement) -> Option<Level> {
    Level::from_i64(i64::from(exported.stated_level))
}

fn booster(exported: &ExportedEnhancement) -> Result<u8, PieceRefusal> {
    match exported.boosters {
        None => Ok(0),
        Some(stated) => {
            u8::try_from(stated).map_err(|_| PieceRefusal::BoosterOutOfRange { stated })
        }
    }
}

/// The piece's level minus the character's, signed, saturating rather than wrapping.
///
/// No band is applied. The reach of the attenuation belongs to the dataset's curves and the
/// clamp belongs to the reader of those curves; a second clamp here would be a rule the export
/// never stated.
fn relative_level(stated: u32, character_level: Level) -> i8 {
    let difference = i64::from(stated) - i64::from(character_level.get());
    difference.clamp(i64::from(i8::MIN), i64::from(i8::MAX)) as i8
}

/// The one thing a generic piece enhances, in the catalog's own vocabulary.
///
/// A record may name more than the catalog carries: every `_Heal` record on Homecoming names
/// `["Healing", "Absorb"]` and the catalog's 26 common-IO types have no `Absorb`, so the
/// intersection is what decides. Requiring exactly one survivor means a record that names two
/// terms the catalog DOES carry is refused rather than silently resolved to whichever came
/// first — the slot holds one stat, so a second is a question, not a tie to break.
fn catalog_stat(named: &[String], catalog: &EnhancementCatalog) -> Result<String, PieceRefusal> {
    let carried: Vec<String> = named
        .iter()
        .filter(|stat| catalog.common_io_types.contains(stat))
        .cloned()
        .collect();
    match carried.as_slice() {
        [only] => Ok(only.clone()),
        _ => Err(PieceRefusal::Stat {
            named: named.to_vec(),
            carried,
        }),
    }
}

// ============================================================
// The build.
// ============================================================

use crate::build_sets::{archetype_for_class, bucket_of, set_powers, Bucket, Roles, SetLookup};
use crate::game_export::{ExportedPower, GameExport, Slot};
use crate::skif::{SkifBuild, SkifPower, SkifSelection};
use crate::{DatasetId, PowerDatabase};

/// Something the export named that this dataset has no record of.
///
/// Kept beside the build rather than dropped, because the conversion happens BEFORE
/// [`crate::skif::hydrate`] and anything refused here never reaches the list `hydrate` keeps
/// (rule 8). The two lists are joined by the caller into one report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportNote {
    /// What the export called the thing, so the note can be matched to a line in the file.
    pub context: String,
    pub detail: String,
}

/// An export turned into the shape [`crate::skif::hydrate`] reads, and everything that did not
/// survive the turning.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    pub build: SkifBuild,
    pub unresolved: Vec<ImportNote>,
}

/// Convert an export into a build, against `database`.
///
/// The powerset is resolved by asking the dataset which of its records carries the binary set
/// path the export prints — `Blaster_Support.Radiation_Manipulation` — rather than by slugging
/// the printed name. The two disagree: that set's id is `blaster/atomic-manipulation`, because
/// the id slugs the DISPLAY name and Homecoming renamed the set without renaming it in the
/// binary. A slug would miss it, and the beta keeps a hand-written alias table for the cases
/// anyone noticed.
pub fn to_skif_build(
    export: &GameExport,
    database: &PowerDatabase,
    dataset: DatasetId,
) -> Converted {
    let mut unresolved = Vec::new();
    let archetype = archetype_id(export, database, &mut unresolved);

    let mut build = SkifBuild {
        name: export.header.character_name.clone(),
        dataset: dataset.as_str().to_string(),
        archetype: archetype.clone(),
        origin: Some(export.header.origin.clone()),
        level: export.header.level.get(),
        primary: None,
        secondary: None,
        pools: Vec::new(),
        epic_pool: None,
        inherents: Vec::new(),
        accolades: Vec::new(),
        incarnates: Default::default(),
        slot_order: Vec::new(),
        stances: Default::default(),
        modes: Default::default(),
        power_state: Default::default(),
        proc_overrides: Default::default(),
        attack_chains: Default::default(),
        disabled_proc_categories: Default::default(),
        what_if_buffs: None,
    };

    let sets = SetLookup::of(database);
    let roles = archetype.as_deref().and_then(|id| Roles::of(id, database));

    // The powers this fork grants every character. Fitness is the case: the client files Swift,
    // Hurdle, Health and Stamina under a `Fitness` set, the dataset still carries the old Fitness
    // POOL of that name, and so the four resolved as pool picks — while the app grants the same
    // four as inherents, and every imported build drew them twice (seen 2026-09-30 on a vault
    // export). A granted power is an inherent wherever the export files it.
    let universal: Vec<String> = database
        .inherent_grants()
        .ok()
        .flatten()
        .map(|grants| grants.grants.into_iter().map(|g| g.internal_name).collect())
        .unwrap_or_default();
    let granted_as = |power: &ExportedPower, pick: &SkifPower| {
        universal
            .iter()
            .find(|name| {
                name.eq_ignore_ascii_case(&power.power_name)
                    || name.eq_ignore_ascii_case(&pick.internal_name)
            })
            .cloned()
    };

    for power in &export.powers {
        let set_id = set_id_of(power, &sets);
        let mut pick = to_pick(power, set_id.as_deref(), export, database, &mut unresolved);
        let bucket = set_id
            .as_deref()
            .and_then(|id| bucket_of(id, roles.as_ref(), database));
        if let Some(Bucket::Pool(_)) = bucket {
            if let Some(granted) = granted_as(power, &pick) {
                pick.internal_name = granted;
                build.inherents.push(pick);
                continue;
            }
        }
        match bucket {
            // An inherent this dataset has no power for, with nothing slotted in it, carries
            // nothing into the build: the inherent reconcile drops any entry no grant names on
            // load. The client lists several (`Stance`, `Engagement`, `Walk`, `Fitness_Fix`), and
            // reporting each as unresolved buried the notes that mattered. One that DOES carry
            // enhancements is kept, so the report still names what would be lost.
            Some(Bucket::Inherent)
                if pick.slots.iter().all(Option::is_none)
                    && database.find_granted_power(&pick.internal_name).is_none() => {}
            Some(Bucket::Inherent) => build.inherents.push(pick),
            Some(Bucket::Primary) => push(&mut build.primary, set_id.clone(), pick),
            Some(Bucket::Secondary) => push(&mut build.secondary, set_id.clone(), pick),
            Some(Bucket::Pool(id)) => {
                let slot = build.pools.iter_mut().find(|pool| pool.id == id);
                match slot {
                    Some(pool) => pool.powers.push(pick),
                    None => build.pools.push(SkifSelection {
                        id,
                        powers: vec![pick],
                    }),
                }
            }
            Some(Bucket::Epic(id)) => push(&mut build.epic_pool, Some(id), pick),
            None => unresolved.push(ImportNote {
                context: format!("{}.{}", power.category, power.powerset),
                detail: format!(
                    "no {} powerset, pool or epic pool carries that binary set path, so {:?} has no home in this build",
                    dataset.as_str(),
                    power.power_name
                ),
            }),
        }
    }
    Converted { build, unresolved }
}

/// Add a pick to a selection, creating it at the set the pick came from.
fn push(slot: &mut Option<SkifSelection>, id: Option<String>, pick: SkifPower) {
    match slot {
        Some(selection) => selection.powers.push(pick),
        None => {
            *slot = Some(SkifSelection {
                id: id.unwrap_or_default(),
                powers: vec![pick],
            })
        }
    }
}

/// The archetype id whose class token the header states.
///
/// Resolved through the dataset's own `className` rather than by stripping `Class_` and
/// lower-casing: the token is a field of the archetype record, so asking is one lookup and
/// re-spelling is a rule that has to keep being right.
fn archetype_id(
    export: &GameExport,
    database: &PowerDatabase,
    unresolved: &mut Vec<ImportNote>,
) -> Option<String> {
    let stated = &export.header.archetype;
    let found = archetype_for_class(stated, database);
    if found.is_none() {
        unresolved.push(ImportNote {
            context: stated.clone(),
            detail: "no archetype in this dataset states that class token".to_string(),
        });
    }
    found
}

fn set_id_of(power: &ExportedPower, sets: &SetLookup) -> Option<String> {
    sets.resolve(&power.category, &power.powerset)
        .map(str::to_string)
}

/// One power line and its slots, as a pick.
fn to_pick(
    power: &ExportedPower,
    set_id: Option<&str>,
    export: &GameExport,
    database: &PowerDatabase,
    unresolved: &mut Vec<ImportNote>,
) -> SkifPower {
    let slots = power
        .slots
        .iter()
        .map(|slot| to_slot(slot, power, export, database, unresolved))
        .collect();
    SkifPower {
        internal_name: dataset_ident(power, set_id, database),
        powerset: None,
        // `0` is the format's own spelling for granted-not-picked at both ends, so the fold
        // through `Option<Level>` and back is lossless.
        level: power.level.map_or(0, |level| level.get()),
        slots,
        is_active: false,
        active_sub_power: None,
    }
}

fn to_slot(
    slot: &Slot,
    power: &ExportedPower,
    export: &GameExport,
    database: &PowerDatabase,
    unresolved: &mut Vec<ImportNote>,
) -> Option<crate::skif::SkifEnhancement> {
    let exported = match slot {
        Slot::Empty => return None,
        Slot::Unreadable(line) => {
            unresolved.push(ImportNote {
                context: power.power_name.clone(),
                detail: format!("a slot line could not be read: {line:?}"),
            });
            return None;
        }
        Slot::Filled(exported) => exported,
    };
    let (Some(index), Some(catalog)) = (
        database.boost_index.as_ref(),
        database.enhancements.as_ref(),
    ) else {
        unresolved.push(ImportNote {
            context: exported.uid.clone(),
            detail: "this dataset ships no boost index, so no enhancement can be resolved"
                .to_string(),
        });
        return None;
    };
    match resolve_enhancement(exported, export.header.level, index, catalog) {
        Ok(piece) => Some(piece),
        Err(refused) => {
            unresolved.push(ImportNote {
                context: format!("{} in {}", refused.uid, power.power_name),
                detail: refused.refusal.to_string(),
            });
            None
        }
    }
}

/// The dataset's own spelling of the power the export names, within the set it belongs to.
///
/// Usually the export's token IS the ident, and it is returned unchanged — including when the
/// set did not resolve, so a power nothing could place still carries what the file called it.
///
/// Fitness is why the display name is tried at all. The game exports its four powers as
/// `Swift`, `Hurdle`, `Health`, `Stamina`; the dataset spells the first `Pool.Fitness.Quick`,
/// an internal name the game kept from before the power was renamed. Matching on ident alone
/// loses exactly that one power out of every build in the corpus, and loses it QUIETLY — the
/// pick survives, unresolved, and reads as a power this fork does not carry.
///
/// The display match is scoped to the resolved set, where names are unique. Across the dataset
/// they are not — every archetype's blast sets repeat each other's power names — so a global
/// display match would resolve to another archetype's power and say nothing.
fn dataset_ident(power: &ExportedPower, set_id: Option<&str>, database: &PowerDatabase) -> String {
    let named = power.power_name.as_str();
    let Some(set_id) = set_id else {
        return named.to_string();
    };
    let candidates = set_powers(database, set_id);
    if candidates
        .iter()
        .any(|candidate| candidate.ident() == named)
    {
        return named.to_string();
    }
    candidates
        .iter()
        .find(|candidate| candidate.name == named)
        .map_or_else(|| named.to_string(), |found| found.ident().to_string())
}
