//! The v2/v3/v4 legacy tier — beta-authored `.skif` files, read through an explicit door.
//!
//! **Not a fallthrough, and the distinction is the point.** v4's own reader falls through an
//! unrecognized version into a full-`Build` path and mis-parses; the draft calls that the single
//! most dangerous line in the format. So [`super::decode`] stays strictly v5 and refuses
//! everything else, and a caller that wants a legacy file asks for one by name. Routing is the
//! caller's decision, made on [`super::probe_version`], not a silent widening of the v5 reader.
//!
//! **One decoder for all three versions**, because the beta has one: `importBuild` routes
//! `version === 2 | 3 | 4` into a single `hydrateBuild`, and the version differences are
//! narrow enough to state here —
//!
//! - **v4** adds `serverId`. v2/v3 have none, so they are Homecoming by the beta's own default,
//!   and that assumption is REPORTED rather than made quietly (it is the one place this tier
//!   invents an answer).
//! - **v3** adds `internalName` to a power. v2 has only the display name, so v2 is the one
//!   version where a display-name match is the only handle there is — see [`resolve_by_name`].
//! - **v1** is a whole `Build` object rather than this shape, and is refused.
//!
//! **The migration is a shape mapping, and then the v5 reader.** Everything here produces a
//! [`SkifBuild`] and hands it to [`super::hydrate`], so a legacy file resolves its powers, its
//! set pieces and its specials through exactly the rules a v5 file does — including rule 8's
//! retain-and-report. Only the things v5 has no slot for are handled here.

use super::{
    AuthoredAgainst, Decoded, SkifBuild, SkifEnhancement, SkifError, SkifFile, SkifIncarnate,
    SkifPower, SkifSelection, SkifSlotOrder, Unresolved,
};
use crate::character::{power_address, AttackChain, ProcOverride};
use crate::database::PowerDatabase;
use crate::level::Level;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// The versions this tier reads. v1 is a different shape entirely; v5 is [`super::decode`]'s.
pub const LEGACY_VERSIONS: [u32; 3] = [2, 3, 4];

/// The dataset a version too old to name one is assumed to target — the beta's own default
/// (`hydrateBuild`: "older exports predate the multi-dataset migration"). Assumed, and said so.
///
/// Visible to the parent module so [`super::probe_dataset`] can answer for a v2/v3 file with
/// the same assumption this tier will make when it reads one, rather than a second copy that
/// could disagree with it.
pub(super) const ASSUMED_DATASET: &str = "homecoming";

// ============================================================
// The wire shape.
// ============================================================

#[derive(Debug, Deserialize)]
struct LegacyFile {
    version: u32,
    build: LegacyBuild,
}

/// Deliberately tolerant: every field defaults and unknown ones are ignored, because a legacy
/// file is a fixed artifact this reader has to accept as it finds it. The strictness lives one
/// step later, where the resulting build meets the dataset.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyBuild {
    name: String,
    /// v4 only. Absent on v2/v3.
    server_id: Option<String>,
    archetype: Option<LegacyArchetype>,
    level: Option<u8>,
    primary: Option<LegacySelection>,
    secondary: Option<LegacySelection>,
    pools: Vec<LegacySelection>,
    epic_pool: Option<LegacySelection>,
    inherents: Vec<LegacyPower>,
    accolades: Vec<Value>,
    incarnates: BTreeMap<String, Value>,
    slot_order: Vec<SkifSlotOrder>,
    attack_chains: Vec<AttackChain>,
    proc_overrides: BTreeMap<String, ProcOverride>,
    active_modes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LegacyArchetype {
    id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct LegacySelection {
    id: Option<String>,
    #[serde(default)]
    powers: Vec<LegacyPower>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct LegacyPower {
    /// The display name. Required in v2, where it is the only identity there is.
    name: String,
    /// v3 onward.
    internal_name: Option<String>,
    level: u8,
    slots: Vec<Option<LegacyEnhancement>>,
    is_active: bool,
    active_sub_power: Option<String>,
}

/// One slotted piece. The single `boost` is what v5 splits into `booster` and `relativeLevel`:
/// v4 carries two different mechanics on two different curves in one field, and which one it is
/// depends entirely on `type`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum LegacyEnhancement {
    #[serde(rename = "io-set")]
    IoSet {
        #[serde(rename = "setId")]
        set_id: String,
        #[serde(rename = "pieceNum")]
        piece_num: u8,
        #[serde(default)]
        attuned: bool,
        #[serde(default)]
        level: Option<i64>,
        #[serde(default)]
        boost: i8,
    },
    #[serde(rename = "io-generic")]
    IoGeneric {
        stat: String,
        #[serde(default)]
        level: Option<i64>,
        #[serde(default)]
        boost: i8,
    },
    Special {
        id: String,
        category: String,
        #[serde(default)]
        boost: i8,
    },
    Origin {
        stat: String,
        tier: String,
        #[serde(default)]
        boost: i8,
    },
}

// ============================================================
// Decode.
// ============================================================

/// Read a beta-authored v2/v3/v4 `.skif`.
///
/// Refuses a version this tier does not own — including v5, which has its own reader, and v1,
/// which is a whole `Build` object rather than the slim shape.
pub fn decode_legacy(text: &str, database: &PowerDatabase) -> Result<Decoded, SkifError> {
    // The version comes first, before the shape — a v5 file read as v4 fails on the fields it
    // spells differently, and reporting THAT would bury the one fact the caller needs.
    let found = super::probe_version(text);
    if !found.is_some_and(|version| LEGACY_VERSIONS.contains(&version)) {
        return Err(SkifError::Version { found });
    }
    let file: LegacyFile =
        serde_json::from_str(text).map_err(|e| SkifError::Malformed(e.to_string()))?;

    let mut notes = Vec::new();
    let build = to_v5(file.build, file.version, database, &mut notes);
    let dataset = build.dataset.clone();

    let mut decoded = super::hydrate(
        SkifFile {
            version: super::VERSION,
            // Stamped as the file's own dataset so `hydrate`'s agreement check is a tautology
            // here rather than a second chance to disagree — a legacy file has no stamp, and
            // inventing one that could contradict the build would be manufacturing a refusal.
            authored_against: Some(AuthoredAgainst {
                dataset,
                contract_schema: crate::database::SUPPORTED_SCHEMA,
                dataset_schema: database.manifest.schema,
                export_commit: None,
            }),
            meta: None,
            build,
        },
        database,
    )?;

    // After hydration, because it is the only point where a pick's display name is knowable:
    // the key names one, and only the resolved def carries it.
    remap_proc_overrides(&mut decoded, database, &mut notes);

    notes.append(&mut decoded.unresolved);
    decoded.unresolved = notes;
    Ok(decoded)
}

fn to_v5(
    legacy: LegacyBuild,
    version: u32,
    database: &PowerDatabase,
    notes: &mut Vec<Unresolved>,
) -> SkifBuild {
    let dataset = match legacy.server_id {
        Some(id) => id,
        None => {
            notes.push(Unresolved {
                context: format!("v{version} file"),
                detail: format!(
                    "carries no dataset — versions before 4 predate multi-dataset support, so it \
                     is being read as {ASSUMED_DATASET}. If it was authored on a fork, its \
                     fork-only powers and enhancements will report as missing."
                ),
            });
            ASSUMED_DATASET.to_string()
        }
    };

    let primary_id = legacy.primary.as_ref().and_then(|s| s.id.clone());
    let secondary_id = legacy.secondary.as_ref().and_then(|s| s.id.clone());
    let archetype = legacy.archetype.and_then(|a| a.id);
    let branch_sets = super::branch_powersets(archetype.as_deref(), database);

    SkifBuild {
        name: legacy.name,
        dataset,
        // The beta defaults an absent level to 50, and so does an empty build here.
        level: legacy.level.unwrap_or(50),
        primary: selection(legacy.primary, database, &branch_sets),
        secondary: selection(legacy.secondary, database, &branch_sets),
        pools: legacy
            .pools
            .into_iter()
            .filter_map(|pool| selection(Some(pool), database, &branch_sets))
            .collect(),
        epic_pool: selection(legacy.epic_pool, database, &branch_sets),
        inherents: legacy
            .inherents
            .into_iter()
            .map(|power| slim_power(power, crate::INHERENT_SET, database, &branch_sets))
            .collect(),
        archetype,
        // No legacy version tracked a character origin — every enhancement in these files
        // read the Natural overlay frame, and that stays true reading them back in.
        origin: None,
        // v4 stores each accolade verbatim with a frozen `bonuses[]` snapshot; only the id
        // survives, because the export owns those values (rule 6).
        accolades: legacy.accolades.iter().filter_map(accolade_id).collect(),
        incarnates: incarnates(legacy.incarnates, database, notes),
        slot_order: legacy.slot_order,
        // v4 has no caster-wide conditional state at all — stances resolve to their own
        // `defaultActive`, which is what an absent key means in v5 too.
        stances: BTreeMap::new(),
        modes: legacy
            .active_modes
            .into_iter()
            .map(|mode| (mode, true))
            .collect(),
        // Neither the buff-pet opt-in nor the proc-category switches exist in v4: both are
        // rebuild mechanics. An absent opt-in is off and an absent category is contributing,
        // so a legacy build lands on the same defaults a fresh one does.
        power_state: BTreeMap::new(),
        proc_overrides: legacy.proc_overrides,
        disabled_proc_categories: Default::default(),
        attack_chains: attack_chains(legacy.attack_chains, primary_id, secondary_id),
        what_if_buffs: None,
    }
}

/// An accolade id, from either spelling: v4 writes the whole object, older code wrote a bare id.
///
/// Two ids are renamed on the way through. They predate the beta's game-internal-name
/// convention, and this tier is the only door they can still arrive by — the planner has
/// written the current spelling since 2026-03-21, so no v5 file carries them. Left alone,
/// [`crate::skif::hydrate`] hands the engine an id `resolve_accolades` cannot find and the
/// build loses its +5 Max End and +10% Max HP to a reported error, which is the right
/// failure for an id nobody knows and the wrong one for an id we do (DATA-GAP ACCOLADE-3).
///
/// The table is CLOSED, not a best effort: the beta's id vocabulary has had exactly two
/// shapes, the four of 2026-01-20 and the eight of 2026-03-21 that renamed two of them, so
/// these are the whole delta. It is also not a Rule 0 hardcode — no fork's export spells an
/// accolade either way; these are ids the beta's own serializer wrote, and the table records
/// its storage history rather than a game fact. Its TS twin is `normalizeAccoladeIds`.
fn accolade_id(entry: &Value) -> Option<String> {
    let id = match entry {
        Value::String(id) => id.clone(),
        Value::Object(map) => map.get("id").and_then(Value::as_str).map(str::to_string)?,
        _ => return None,
    };
    Some(match id.as_str() {
        "atlas_medallion" => "the_atlas_medallion".to_string(),
        "freedom_phalanx" => "freedom_phalanx_reserve".to_string(),
        _ => id,
    })
}

fn selection(
    legacy: Option<LegacySelection>,
    database: &PowerDatabase,
    branch_sets: &[String],
) -> Option<SkifSelection> {
    let legacy = legacy?;
    let id = legacy.id?;
    Some(SkifSelection {
        powers: legacy
            .powers
            .into_iter()
            .map(|power| slim_power(power, &id, database, branch_sets))
            .collect(),
        id,
    })
}

fn slim_power(
    legacy: LegacyPower,
    powerset: &str,
    database: &PowerDatabase,
    branch_sets: &[String],
) -> SkifPower {
    SkifPower {
        // A legacy writer files every pick under the list's own set — including a VEAT branch
        // pick, which it records under the BASE set (see `skif::Resolver`). Stating nothing
        // here preserves that rather than inventing a set the file never named.
        powerset: None,
        internal_name: legacy
            .internal_name
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| resolve_by_name(&legacy.name, powerset, database, branch_sets)),
        level: legacy.level,
        slots: legacy
            .slots
            .into_iter()
            .map(|s| s.map(enhancement))
            .collect(),
        is_active: legacy.is_active,
        active_sub_power: legacy.active_sub_power,
    }
}

/// A v2 power's internal name, recovered from the only identity it carries.
///
/// **This is the display-name match v5's rule 8 deletes, and it is confined here on purpose.**
/// A v2 file predates `internalName` entirely, so refusing the match would refuse the version;
/// what makes it safe is that it is SCOPED — to the named powerset, plus this archetype's own
/// branch sets, and nothing else. Rule 8's objection is to matching across the whole corpus,
/// which is a different thing.
///
/// It is also not merely a convenience. Two real Homecoming renames in the corpus moved the
/// DISPLAY name while the internal one stayed: `Long_Jump` displays as "Super Jump" and
/// `Invisibility` as "Infiltration". Underscoring the display name would bind `Super_Jump` and
/// `Infiltration` — different powers — so a lookup against the dataset is the only thing that
/// recovers the pick the file meant.
///
/// The last resort is still the beta's own: spaces to underscores. It cannot bind the wrong
/// power — it either names one or names nothing — and naming nothing is retained and reported
/// by [`super::hydrate`].
fn resolve_by_name(
    display_name: &str,
    powerset: &str,
    database: &PowerDatabase,
    branch_sets: &[String],
) -> String {
    let wanted = display_name.to_lowercase();
    let named = |set: &str| -> Option<String> {
        let scoped = database
            .find_powerset(set)
            .into_iter()
            .flat_map(|powerset| powerset.powers.iter())
            .chain(
                database
                    .pool_powers
                    .iter()
                    .chain(database.epic_powers.iter())
                    .filter(|entry| entry.set_id == set)
                    .map(|entry| &entry.power),
            )
            .find(|power| power.name.to_lowercase() == wanted)?;
        Some(scoped.ident().to_string())
    };
    if powerset == crate::INHERENT_SET {
        if let Some(found) = database
            .inherent_powers
            .iter()
            .find(|power| power.name.to_lowercase() == wanted)
        {
            return found.ident().to_string();
        }
    }
    named(powerset)
        .or_else(|| branch_sets.iter().find_map(|set| named(set)))
        .unwrap_or_else(|| display_name.replace(' ', "_"))
}

/// v4's single `boost` field, split onto the axis its `type` says it is. The two are different
/// mechanics on different curves and no piece carries both.
fn enhancement(legacy: LegacyEnhancement) -> SkifEnhancement {
    match legacy {
        LegacyEnhancement::IoSet {
            set_id,
            piece_num,
            attuned,
            level,
            boost,
        } => SkifEnhancement::IoSet {
            set_id,
            piece_num,
            attuned,
            level: level.and_then(Level::from_i64),
            booster: boost.max(0) as u8,
        },
        LegacyEnhancement::IoGeneric { stat, level, boost } => SkifEnhancement::IoGeneric {
            stat,
            level: level.and_then(Level::from_i64),
            booster: boost.max(0) as u8,
        },
        LegacyEnhancement::Special {
            id,
            category,
            boost,
        } => SkifEnhancement::Special {
            category,
            id,
            relative_level: boost,
        },
        LegacyEnhancement::Origin { stat, tier, boost } => SkifEnhancement::Origin {
            stat,
            tier,
            relative_level: boost,
        },
    }
}

/// v4's incarnate block, keyed by slot id already — what it carries beyond identity is display
/// data the catalog owns (`displayName`, `icon`, `tier`, `treeId`, `treeName`), which rule 6
/// drops.
///
/// The identity is resolved through the catalog rather than by stripping the `Incarnate.<Slot>.`
/// prefix off `powerName`: the catalog states both spellings, so matching it is exact where
/// string surgery would be a guess about a naming convention.
///
/// `active` has no v4 field — it lived in the beta's `uiStore`, whose default is ON, so an
/// imported build's incarnates contribute exactly as they did to the author.
fn incarnates(
    legacy: BTreeMap<String, Value>,
    database: &PowerDatabase,
    notes: &mut Vec<Unresolved>,
) -> BTreeMap<String, SkifIncarnate> {
    let mut out = BTreeMap::new();
    for (slot_id, entry) in legacy {
        let Some(entry) = entry.as_object() else {
            continue;
        };
        let full_name = entry.get("powerName").and_then(Value::as_str);
        let power_id = entry.get("powerId").and_then(Value::as_str);
        let catalog = database
            .incarnate_catalog
            .slots
            .iter()
            .find(|slot| slot.id == slot_id);

        let resolved = catalog.and_then(|slot| {
            slot.powers
                .iter()
                .find(|power| Some(power.full_name.as_str()) == full_name)
                .or_else(|| power_id.and_then(|id| slot.find_power(id)))
        });

        if let Some(power) = resolved {
            out.insert(
                slot_id,
                SkifIncarnate {
                    power: power.internal_name.clone(),
                    active: true,
                },
            );
            continue;
        }

        // Retained under the name the file gave it, not dropped (rule 8) — this is a live case,
        // not a hypothetical: Homecoming ships 54 Judgement and 189 Lore powers where both forks
        // ship 45 and 180, so a build carried across forks holds picks the fork has never heard
        // of. Dropping one would lose it from the re-export too, and carrying it back to
        // Homecoming would no longer restore it.
        //
        // The name is the full id's tail, which is the internal name — string surgery only
        // because the catalog, the thing that could answer properly, is what has just said no.
        let Some(named) = full_name.or(power_id) else {
            continue;
        };
        let internal = named.rsplit('.').next().unwrap_or(named);
        notes.push(Unresolved {
            context: format!("incarnate slot {slot_id:?}"),
            detail: format!(
                "this dataset's catalog carries no {named:?}; the pick was kept and marked, so \
                 it survives a re-export"
            ),
        });
        out.insert(
            slot_id,
            SkifIncarnate {
                power: internal.to_string(),
                active: true,
            },
        );
    }
    out
}

/// v4 addresses a chain entry by BUCKET (`primary:Fire_Blast`), which cannot name a pool by
/// name and cannot tell two pools apart. The rebuild addresses by powerset, so the two buckets
/// that name one are translated and the rest are left for the pass below to resolve against the
/// build.
///
/// Chains are display-only — nothing in the calc reads them — so an entry that cannot be
/// translated is carried through verbatim rather than dropped: it will simply not match a power
/// when the chain is next opened, which is visible, where a missing row is not.
fn attack_chains(
    chains: Vec<AttackChain>,
    primary_id: Option<String>,
    secondary_id: Option<String>,
) -> Vec<AttackChain> {
    chains
        .into_iter()
        .map(|chain| AttackChain {
            powers: chain
                .powers
                .iter()
                .map(|entry| match entry.split_once(':') {
                    Some(("primary", ident)) => match &primary_id {
                        Some(set) => power_address(set, ident),
                        None => entry.clone(),
                    },
                    Some(("secondary", ident)) => match &secondary_id {
                        Some(set) => power_address(set, ident),
                        None => entry.clone(),
                    },
                    _ => entry.clone(),
                })
                .collect(),
            ..chain
        })
        .collect()
}

/// Re-address v4's proc overrides, which are keyed on the power's DISPLAY name plus the slot
/// index (`procOverrideKey(power.name, …)`).
///
/// A display name does not address a pick — that is the whole reason the rebuild re-keyed these
/// — so the translation is done by resolving each of the build's own picks to its def and
/// matching the name. **Exactly one match translates; zero or several are reported and
/// dropped**, because several IS the collision the address exists to fix and picking one would
/// be the silent mis-binding.
///
/// Worth knowing before trusting this: only one real build in `fixtures/skif/v4` carries proc
/// overrides at all, because the beta writes an entry only when a control is TOUCHED — an
/// untouched row is `"auto"` and produces no key. Its two are the exactly-one case.
///
/// The several case has no observed instance and is only barely reachable: across all three
/// forks, a single build can hold two picks sharing a display name in four ways — one on
/// Homecoming (a Corruptor with Thermal Radiation carries `Fire_Shield`, and so does the Fire
/// Mastery epic it can take), three on Thunderspy, none on Rebirth. That one Homecoming build is
/// hand-written in the gate, since no exported file has it.
fn remap_proc_overrides(
    decoded: &mut Decoded,
    database: &PowerDatabase,
    notes: &mut Vec<Unresolved>,
) {
    if decoded.build.proc_overrides.is_empty() {
        return;
    }
    let addresses: Vec<(String, String)> = decoded
        .build
        .all_selected()
        .filter_map(|selection| {
            let def = database.resolve_power(&selection.powerset, &selection.internal_name)?;
            Some((def.name.clone(), selection.address()))
        })
        .collect();

    let mut remapped = BTreeMap::new();
    for (key, over) in std::mem::take(&mut decoded.build.proc_overrides) {
        let Some((display_name, slot_index)) = key.rsplit_once(':') else {
            notes.push(Unresolved {
                context: format!("proc override {key:?}"),
                detail: "the key names no slot; the override was dropped".to_string(),
            });
            continue;
        };
        let mut matches = addresses
            .iter()
            .filter(|(name, _)| name == display_name)
            .map(|(_, address)| address);
        match (matches.next(), matches.next()) {
            (Some(address), None) => {
                remapped.insert(format!("{address}:{slot_index}"), over);
            }
            (Some(_), Some(_)) => notes.push(Unresolved {
                context: format!("proc override {key:?}"),
                detail: format!(
                    "{display_name:?} names more than one pick in this build, and a display name \
                     cannot say which; the override was dropped rather than bound to a guess"
                ),
            }),
            (None, _) => notes.push(Unresolved {
                context: format!("proc override {key:?}"),
                detail: format!(
                    "no pick in this build resolves to a power named {display_name:?}; the \
                     override was dropped"
                ),
            }),
        }
    }
    decoded.build.proc_overrides = remapped;
}
