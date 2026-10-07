//! A `.mxd` rewritten as the document the `.mbd` reader already knows how to read.
//!
//! **This module resolves names and nothing else.** It does not build a build: it hands
//! [`crate::mbd_import::to_skif_build`] an [`MbdFile`], and every hard question after that —
//! which bucket a set belongs in, which UID names which boost record, what level the build is,
//! whether the enhancements reconcile — is answered once, by the reader that already answers it
//! for `.mbd`. A second importer growing its own copy of those answers is how the two halves of
//! the Mids surface drifted apart before (MBDEXPORT-4), and the `.mxd` is the third door onto
//! the same namespace.
//!
//! What has to happen here is the part a `.mbd` never needs, because a `.mbd` states internal
//! names and UIDs and a `.mxd` states neither:
//!
//! - **a power arrives as a DISPLAY name** and nothing else. The compressed half's `nIDPower` is
//!   an index into a Mids powers array that has been reshuffled wholesale between the versions
//!   the corpus spans, so it is not a second opinion — index 299 is War Mace's Pulverize in a
//!   2019 file and `Temporal_Healing` in the database vendored here. So the display name is
//!   resolved against OUR data, in the powersets the compressed half names, and then spelled
//!   BACK the way a `.mbd` would spell it, through the same reverse table the `.mbd` writer uses.
//!   That round trip is what keeps the rotated names right: Mids' Stalker Willpower
//!   `Reconstruction` is our `Resurgence`, and handing the reader our spelling of a rotated name
//!   would have it refuse the pick as rotated away (MBDIMPORT-2's refusal, fired at the wrong
//!   thing).
//! - **an enhancement arrives as a short code**, resolved through [`MidsEnhNames`], which is
//!   where the compressed half's index IS worth something: set membership survives Mids
//!   reordering its array even where a piece's name does not.
//!
//! **Which powerset a pick belongs to is not in the file.** The compressed half names the
//! build's eight powersets and then names powers by an index alone, so the only thing that
//! places a pick is finding its display name among the powers of the sets the build listed. A
//! name two of those sets both carry is declined rather than guessed — the same rule, at the
//! same door, as MBDIMPORT-18's.

use crate::build_sets::{archetype_for_class, set_powers, SetLookup};
use crate::mbd::{MbdBuiltWith, MbdEnhancement, MbdFile, MbdPowerEntry, MbdSlotEntry};
use crate::mbd_import::{fold_separators, relative_level_token, ImportNote};
use crate::mids_enh_names::{MidsEnhKind, MidsEnhNames, MidsEnhRefusal, MidsEnhRoute};
use crate::mids_names::MidsNames;
use crate::mxd::{MxdFile, MxdProseEnhancement, MxdSlot};
use crate::{Power, PowerDatabase};
use serde_json::Value;

/// The group a `.mbd` writes for powers the game grants rather than sells, and the one this
/// reader writes for any power it found outside the build's own sets.
///
/// Stated here because the `.mxd` states no group at all, and the reader downstream keys on it:
/// it is the one group token [`crate::mbd_import`] reads rather than resolves, precisely because
/// `Inherent.Inherent` names no set a build holds.
const INHERENT_GROUP_PATH: &str = "Inherent.Inherent";

/// Mids' grade tokens by ordinal — `eEnhGrade`, which is what the compressed half's second byte
/// holds for a Hamidon or an origin piece.
///
/// The ordinals are read off the corpus against Mids' own `.mbd` spelling of the same records:
/// `3` stands against `Grade: SingleO` on 5,538 corpus slots and `0` against `Grade: None` on
/// 123, matching the 91%/9% split those two take across the 2,611 special slots of the 2,187
/// `.mbd` files beside them.
const GRADE: [&str; 4] = ["None", "TrainingO", "DualO", "SingleO"];

/// What a `.mxd` read accounted for, before the `.mbd` reader has seen any of it.
///
/// Every count is against the FILE, on MBDIMPORT-5's terms: an entry this module declines still
/// carried slots and enhancements, and a count that only the success path reaches is a count
/// that loses them. The three route counts exist because the residual pass is a population to
/// watch rather than a mechanism to trust — a file where it carries most of the slots is a file
/// written by a Mids whose database this one no longer resembles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MxdSummary {
    /// Powerset paths the compressed half names, blanks not counted.
    pub powersets_in_file: usize,
    /// Of those, the ones this dataset has a set for. A path that does not resolve takes every
    /// power in it with it, so the two counts diverging explains a whole run of unnamed powers
    /// at once — which a per-power tally never does.
    pub powersets_resolved: usize,
    /// Entries in the compressed half that name a power.
    pub powers_in_file: usize,
    /// Entries whose display name reached exactly one power in this dataset.
    pub powers_named: usize,
    /// Entries whose display name reached none, or more than one.
    pub powers_unnamed: usize,
    pub enhancements_in_file: usize,
    pub enhancements_named: usize,
    pub enhancements_unnamed: usize,
    /// Named by the post's code alone.
    pub by_code: usize,
    /// Named by a code Mids gives to two records, with the file's own index choosing.
    pub by_code_and_index: usize,
    /// Named by the index, inside the set the code named — a Mids piece rename.
    pub by_index_within_set: usize,
}

impl MxdSummary {
    /// Does the read account for every power and every enhancement the file holds?
    pub fn reconciles(&self) -> bool {
        self.powers_named + self.powers_unnamed == self.powers_in_file
            && self.enhancements_named + self.enhancements_unnamed == self.enhancements_in_file
    }
}

/// A `.mxd` read into the `.mbd` reader's shape, with everything it could not name.
#[derive(Debug, Clone, PartialEq)]
pub struct MxdReading {
    pub file: MbdFile,
    pub notes: Vec<ImportNote>,
    pub summary: MxdSummary,
}

/// Rewrite a `.mxd` as a `.mbd`, against `database`.
///
/// The result is handed straight to [`crate::mbd_import::to_skif_build`]; nothing here decides
/// anything that reader decides.
pub fn to_mbd_file(file: &MxdFile, database: &PowerDatabase) -> MxdReading {
    let mut notes = Vec::new();
    let mut summary = MxdSummary::default();

    let Some(prose) = &file.prose else {
        // The post half is the only thing in the document that NAMES anything. A file carrying
        // only the compressed block is a real document — the corpus has one — and it is also a
        // document nothing can be resolved out of, which is a fact to state rather than an empty
        // build to hand over.
        notes.push(ImportNote {
            context: file.binary.character_name.clone(),
            detail: "this file carries only Mids' data block and not the post above it, and the \
                     block names powers and enhancements by index into the Mids database that \
                     wrote it — so there is nothing here that can be resolved against this \
                     dataset"
                .to_string(),
        });
        return MxdReading {
            file: skeleton(file, Vec::new()),
            notes,
            summary,
        };
    };

    let names = database.mids_names.as_ref();
    let enh_names = database.mids_enh_names.as_ref();
    if enh_names.is_none() {
        notes.push(ImportNote {
            context: "this dataset".to_string(),
            detail: "ships no Mids enhancement-name table, so no slotted piece in this file can \
                     be named"
                .to_string(),
        });
    }
    let homes = Homes::open(
        &file.binary.class_name,
        &file.binary.powersets,
        database,
        names,
    );
    // Distinct paths, because a pool named twice is one pool on both sides of this comparison —
    // counting the duplicate here and not in `powersets_resolved` would make every build that
    // re-picked a pool look like one with an unresolvable set.
    let mut distinct: Vec<&str> = file
        .binary
        .powersets
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .collect();
    distinct.sort_unstable();
    distinct.dedup();
    summary.powersets_in_file = distinct.len();
    summary.powersets_resolved = homes.declared.len();

    // **Two passes, because the second door needs the first door's answers.** An internal-name
    // claimant that another pick already holds is a refusal (see [`Homes::find_internal`]), and
    // the pick that holds it can be a LATER row — the corpus Stalker Ninjitsu takes `Blinding
    // Powder` at 35 and `Smoke Flash` at 49. Resolving in one pass would let row order decide
    // which of the two the build keeps.
    let resolved = resolve(&homes, file, prose, database, &mut notes);

    let mut entries = Vec::new();
    for ((record, row), home) in file.binary.powers().zip(&prose.rows).zip(resolved) {
        summary.powers_in_file += 1;
        summary.enhancements_in_file += record
            .slots
            .iter()
            .filter(|slot| slot.enhancement.is_some())
            .count();

        let Some(home) = home else {
            summary.powers_unnamed += 1;
            summary.enhancements_unnamed += record
                .slots
                .iter()
                .filter(|slot| slot.enhancement.is_some())
                .count();
            continue;
        };
        summary.powers_named += 1;

        let slot_entries = record
            .slots
            .iter()
            .zip(&row.enhancements)
            .map(|(slot, spelled)| {
                slot_entry(
                    slot,
                    spelled,
                    &row.power,
                    enh_names,
                    &mut notes,
                    &mut summary,
                )
            })
            .collect();

        entries.push(MbdPowerEntry {
            power_name: home.mids_name(names),
            // The compressed half is 0-based and a `.mbd` is not. An entry Mids files at no
            // level — every accolade in the corpus — is written 0, which is the level a `.mbd`
            // gives an entry it does not place.
            level: i32::from(record.level) + 1,
            stat_include: record.stat_include,
            proc_include: record.proc_include,
            variable_value: record.variable_value,
            inherent_slots_used: 0,
            sub_power_entries: Value::Null,
            slot_entries,
        });
    }

    MxdReading {
        file: skeleton(file, entries),
        notes,
        summary,
    }
}

/// Every entry of the file placed, in the file's own order, with a note for each refusal.
///
/// The display join runs over the whole file first and the internal-name join only over what it
/// left, so no entry can be taken by the second door while the first door still wants it, and no
/// row's position decides anything. See [`Homes::find_internal`] for why both halves of that
/// matter.
fn resolve(
    homes: &Homes,
    file: &MxdFile,
    prose: &crate::mxd::MxdProse,
    database: &PowerDatabase,
    notes: &mut Vec<ImportNote>,
) -> Vec<Option<Home>> {
    let rows: Vec<(&crate::mxd::MxdPowerRecord, &str)> = file
        .binary
        .powers()
        .zip(&prose.rows)
        .map(|(record, row)| (record, row.power.as_str()))
        .collect();

    // A row the author never touched is not worth a note whether or not this dataset has a record
    // of it. Mids lists a build's mode and combo meters — `Quick Form`, `Combo Level 1`, `Energy
    // Focus`, `Momentum` — as powers, and 1,590 of the corpus's unplaced entries are those. A
    // fail-loud channel crying wolf on every file is one nobody reads, which is the rule the
    // `.mbd` reader already applies to its own granted rows.
    let mut refusals: Vec<Option<(String, String, bool)>> = vec![None; rows.len()];
    let mut placed: Vec<Option<Home>> = Vec::with_capacity(rows.len());
    for (index, (record, display)) in rows.iter().enumerate() {
        match homes.find_displayed(display, database) {
            Ok(home) => placed.push(home),
            Err(detail) => {
                refusals[index] = Some(((*display).to_string(), detail, carries_something(record)));
                placed.push(None);
            }
        }
    }

    let held: Vec<String> = placed
        .iter()
        .flatten()
        .map(|home| home.ours.clone())
        .collect();
    // Two unplaced entries reaching for one power is the same question as one entry reaching for a
    // power already held, so both arms of it refuse rather than letting the earlier row win.
    let mut wanted: Vec<String> = Vec::new();
    for (index, (_, display)) in rows.iter().enumerate() {
        if placed[index].is_some() || refusals[index].is_some() {
            continue;
        }
        if let Ok(Some(home)) = homes.find_internal(display, database, &held) {
            wanted.push(home.ours);
        }
    }

    for (index, (record, display)) in rows.iter().enumerate() {
        if placed[index].is_some() || refusals[index].is_some() {
            continue;
        }
        match homes.find_internal(display, database, &held) {
            Ok(Some(home))
                if wanted
                    .iter()
                    .filter(|held| held.eq_ignore_ascii_case(&home.ours))
                    .count()
                    == 1 =>
            {
                placed[index] = Some(home);
            }
            Ok(Some(home)) => {
                refusals[index] = Some((
                    (*display).to_string(),
                    format!(
                        "two of this file's picks reach the power this dataset stores as \
                         {:?}, and the file says only the name",
                        home.ours,
                    ),
                    carries_something(record),
                ));
            }
            Ok(None) => {
                refusals[index] = Some((
                    (*display).to_string(),
                    format!(
                        "no power in this build's own sets, and none of this dataset's granted, \
                         accolade or incarnate rosters, is displayed {display:?}, and none of \
                         those sets stores a power under that name either. Mids renames powers \
                         between releases, so a file written by an older one can name a power \
                         this dataset spells differently"
                    ),
                    carries_something(record),
                ));
            }
            Err(detail) => {
                refusals[index] = Some(((*display).to_string(), detail, carries_something(record)));
            }
        }
    }

    for refusal in refusals.into_iter().flatten() {
        let (context, detail, carries) = refusal;
        if carries {
            notes.push(ImportNote { context, detail });
        }
    }
    placed
}

/// The document around the entries: everything the compressed half states outright.
fn skeleton(file: &MxdFile, power_entries: Vec<MbdPowerEntry>) -> MbdFile {
    let binary = &file.binary;
    let stated_level = power_entries
        .iter()
        .flat_map(|entry| {
            std::iter::once(entry.level).chain(entry.slot_entries.iter().map(|slot| slot.level))
        })
        .max()
        .unwrap_or(1)
        - 1;
    MbdFile {
        built_with: MbdBuiltWith {
            // Provenance, and provenance only — nothing may branch on it, on the same terms as
            // `.mbd`'s own `App`. The post's first line is what Mids wrote about itself.
            app: file
                .prose
                .as_ref()
                .and_then(|prose| prose.header.first().cloned())
                .unwrap_or_else(|| format!("Mids .mxd format {:.2}", binary.format_version)),
            version: format!("{:.2}", binary.format_version),
            // **A `.mxd` names no fork.** There is no field for one: the format predates the
            // forks, and the compressed half states an archetype class and eight powerset paths
            // and stops. So the file reads against whatever dataset is loaded, which is the same
            // answer a `/buildsave` gets and for the same reason.
            database: String::new(),
            database_version: String::new(),
        },
        level: stated_level.to_string(),
        class: binary.class_name.clone(),
        origin: binary.origin.clone(),
        alignment: binary.alignment.to_string(),
        name: binary.character_name.clone(),
        comment: String::new(),
        power_sets: binary.powersets.clone(),
        last_power: binary.last_power,
        power_entries,
    }
}

// ============================================================
// Where a power lives.
// ============================================================

/// One power found in this dataset, and the Mids group path that will lead the `.mbd` reader
/// back to it.
struct Home {
    /// The Mids path for the group the power sits in — verbatim from the compressed half where
    /// the power is in one of the build's own sets, and the group a `.mbd` would use otherwise.
    group_path: String,
    /// Our internal name for the power.
    ours: String,
}

impl Home {
    /// The three-segment name a `.mbd` states, spelled the way Mids spells it.
    ///
    /// The reverse table is the load-bearing half. Handing the reader OUR name for a power the
    /// set rotated would have it refuse the pick as rotated away — the refusal MBDIMPORT-2 built
    /// for a name Mids means and we do not have, fired at a name we do have.
    fn mids_name(&self, names: Option<&MidsNames>) -> String {
        let key = match names {
            Some(names) => names.powerset_key(&self.group_path),
            None => self.group_path.to_lowercase(),
        };
        let spelled = names
            .and_then(|names| names.mids_power_name(&key, &self.ours))
            .unwrap_or(&self.ours);
        format!("{}.{spelled}", self.group_path)
    }
}

/// The places a power in this file could be, opened once for the whole read.
struct Homes {
    /// The build's own powersets: the Mids path the file states, and our id for it.
    declared: Vec<(String, String)>,
    /// The role sets a BRANCH archetype holds underneath its branch, as our own path and id.
    ///
    /// **A VEAT's file names its branch sets and not the sets under them.** An Arachnos Soldier
    /// who specialised into Bane Spider declares `Bane_Spider_Soldier` and
    /// `Bane_Spider_Training`, and then lists `Wolf Spider Armor` and `Combat Training:
    /// Defensive` — powers of the base sets, taken before the branch existed, which appear in no
    /// set the file names. Seven of the corpus Bane's forty-one entries are those. `.skif`'s
    /// legacy reader met the mirror image of this and fixed it the same way: by reading the
    /// archetype's own structure rather than the file's.
    ///
    /// Empty for the twenty-odd archetypes with no branches, so the extra scope costs a
    /// non-VEAT nothing — and, more to the point, cannot make one of its fifteen primary sets
    /// answer for another.
    beneath_the_branch: Vec<(String, String)>,
}

impl Homes {
    fn open(
        class_name: &str,
        powersets: &[String],
        database: &PowerDatabase,
        names: Option<&MidsNames>,
    ) -> Self {
        let sets = SetLookup::of(database);
        let mut declared: Vec<(String, String)> = Vec::new();
        for path in powersets.iter().filter(|path| !path.trim().is_empty()) {
            let key = match names {
                Some(names) => names.powerset_key(path),
                None => path.to_lowercase(),
            };
            let Some(id) = sets.resolve_path(&key) else {
                continue;
            };
            // **A pool named twice is one pool.** Mids writes the same path into two of the
            // build's eight positions where an author re-picked one, and the corpus has 7 such
            // files — enough to make every power of that pool look like a name two of the
            // build's sets both carry, which is the shape this reader declines. The `.mbd`
            // reader dedupes the same list for the same reason, one selection later.
            if declared.iter().any(|(_, seen)| seen == id) {
                continue;
            }
            declared.push((path.clone(), id.to_string()));
        }
        Self {
            declared,
            beneath_the_branch: branch_role_sets(class_name, database),
        }
    }

    /// The one power in this dataset this DISPLAY name reaches.
    ///
    /// The order is the order the file's own structure implies: a build's own sets first, then
    /// the three homes a granted entry can be in. Within a stage, two claimants is a question
    /// rather than a tie to break — `Err` carries what to say about it, and `Ok(None)` means
    /// nothing this reader looks at displays the name at all.
    fn find_displayed(
        &self,
        display: &str,
        database: &PowerDatabase,
    ) -> Result<Option<Home>, String> {
        let mut claimed = claimants(&self.declared, display, database);
        if claimed.is_empty() {
            claimed = claimants(&self.beneath_the_branch, display, database);
        }
        match claimed.as_slice() {
            [(path, power)] => {
                return Ok(Some(Home {
                    group_path: (*path).to_string(),
                    ours: power.ident().to_string(),
                }))
            }
            [] => {}
            several => {
                let sets: Vec<&str> = several.iter().map(|(path, _)| *path).collect();
                return Err(format!(
                    "this build's {} both carry a power displayed {display:?}, and the file \
                     says only the name — which of them the author picked is not in it",
                    sets.join(" and "),
                ));
            }
        }

        Ok(self
            .incarnate(display, database)
            .or_else(|| accolade(display, database))
            .or_else(|| granted(display, database)))
    }

    /// The one power of this build's own sets whose INTERNAL name is `display`.
    ///
    /// **Second door, and only for a name nothing displays.** A power the game renames keeps its
    /// internal name, and the label Mids held before the rename is that internal name — Homecoming
    /// displays `Dull_Pain` as `Second Wind` and Electrical Blast's `Aim` as `Charge Up`, so a
    /// `.mxd` written before those renames names a power this dataset still carries, under the
    /// name it still stores. 281 of the 292 entries MXDIMPORT-1 counted are this.
    ///
    /// **It runs last, and that order is measured rather than chosen.** 433 corpus entries have a
    /// name that BOTH doors answer, and they disagree on every one: Shield Defense's three
    /// displays are a cyclic permutation of its three internal names, so `Active Defense` reaches
    /// one power by display and a different one by ident. Mids' own name table settles which is
    /// meant — it maps Mids' `active_defense` to our `Battle_Agility`, the display join's answer —
    /// so the display join keeps every entry it already had and this door sees only what it could
    /// not reach.
    ///
    /// **`placed` is the other half of the rule.** An internal name is not an identity across a
    /// revamp: Homecoming reused Stalker Ninjitsu's `Smoke_Flash` record for `Bo Ryaku` and its
    /// `Blinding_Powder` record for `Smoke Flash`, so a file that picked the old set's *two*
    /// powers has one pick with nowhere to go. Where the claimant is a power another entry already
    /// holds, the answer is a refusal and not a build that carries one power twice.
    fn find_internal(
        &self,
        display: &str,
        database: &PowerDatabase,
        placed: &[String],
    ) -> Result<Option<Home>, String> {
        let wanted = fold_separators(display);
        let claimed: Vec<(&str, &Power)> = self
            .declared
            .iter()
            .chain(&self.beneath_the_branch)
            .flat_map(|(path, set_id)| {
                set_powers(database, set_id)
                    .into_iter()
                    .filter(|power| fold_separators(power.ident()) == wanted)
                    .map(|power| (path.as_str(), power))
                    .collect::<Vec<_>>()
            })
            .collect();
        match claimed.as_slice() {
            [] => Ok(None),
            [(path, power)] => {
                if placed
                    .iter()
                    .any(|seen| seen.eq_ignore_ascii_case(power.ident()))
                {
                    return Err(format!(
                        "this dataset stores a power of this build's {path} under the name \
                         {display:?} and displays it {:?} — but another of this file's picks \
                         already took it, so the two cannot both be that power",
                        power.name,
                    ));
                }
                Ok(Some(Home {
                    group_path: (*path).to_string(),
                    ours: power.ident().to_string(),
                }))
            }
            several => {
                let sets: Vec<&str> = several.iter().map(|(path, _)| *path).collect();
                Err(format!(
                    "this build's {} both store a power under the name {display:?}, and the file \
                     says only the name — which of them the author picked is not in it",
                    sets.join(" and "),
                ))
            }
        }
    }

    fn incarnate(&self, display: &str, database: &PowerDatabase) -> Option<Home> {
        database.incarnate_catalog.slots.iter().find_map(|slot| {
            let power = slot
                .powers
                .iter()
                .find(|power| power.display_name.eq_ignore_ascii_case(display))?;
            Some(Home {
                group_path: slot.key.clone(),
                ours: power.internal_name.clone(),
            })
        })
    }
}

/// Every power of these sets displayed this way, with the path that leads back to it.
fn claimants<'a>(
    scope: &'a [(String, String)],
    display: &str,
    database: &'a PowerDatabase,
) -> Vec<(&'a str, &'a Power)> {
    scope
        .iter()
        .flat_map(|(path, set_id)| {
            set_powers(database, set_id)
                .into_iter()
                .filter(|power| power.name.eq_ignore_ascii_case(display))
                .map(|power| (path.as_str(), power))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The role sets a branch archetype holds under its branch, as our own path and id.
///
/// Our path rather than Mids' because the file never named these: it is the archetype's
/// structure that says they are there. [`crate::mbd_import`]'s path lookup folds either
/// spelling to the same key, so a path of ours resolves exactly as one of Mids' would.
fn branch_role_sets(class_name: &str, database: &PowerDatabase) -> Vec<(String, String)> {
    let Some(archetype) = archetype_for_class(class_name, database).and_then(|id| {
        database
            .archetypes()
            .ok()
            .and_then(|all| all.get(&id).cloned())
    }) else {
        return Vec::new();
    };
    if archetype.branches.is_empty() {
        return Vec::new();
    }
    archetype
        .primary_sets
        .iter()
        .chain(&archetype.secondary_sets)
        .filter_map(|set_id| {
            let path = database.find_powerset(set_id)?.set_path.clone()?;
            Some((path, set_id.clone()))
        })
        .collect()
}

/// An accolade by display name, under the path the `.mbd` reader resolves accolades through.
fn accolade(display: &str, database: &PowerDatabase) -> Option<Home> {
    let (set_id, power) = database
        .accolade_powers_with_set()
        .find(|(_, power)| power.name.eq_ignore_ascii_case(display))?;
    let path = database
        .find_powerset(set_id)
        .and_then(|set| set.set_path.clone())
        .unwrap_or_else(|| set_id.to_string());
    Some(Home {
        group_path: path,
        ours: power.ident().to_string(),
    })
}

/// A granted inherent by display name, in the three places this dataset keeps them.
///
/// The display-name twin of [`PowerDatabase::find_granted_power`], and it walks the same three
/// rosters for that function's reason: a granted inherent has no owning set, so there is nothing
/// to scope the lookup by. Two claimants is a refusal rather than a first-wins, because the
/// rosters overlap — Mids files a Kheldian form's attacks here too.
fn granted(display: &str, database: &PowerDatabase) -> Option<Home> {
    let synthetic = database
        .find_powerset(crate::INHERENT_SET)
        .map(|set| set.powers.as_slice())
        .unwrap_or_default();
    let claimants: Vec<&Power> = database
        .inherent_powers
        .iter()
        .chain(synthetic)
        .chain(database.pool_powers.iter().map(|entry| &entry.power))
        .filter(|power| power.name.eq_ignore_ascii_case(display))
        .collect();

    let first = claimants.first()?;
    // **The three rosters overlap, and mostly the overlap is one power** — the loader keeps the
    // Fitness four in the pool partition while the legacy Fitness pool publishes them too.
    if claimants
        .iter()
        .all(|power| power.ident().eq_ignore_ascii_case(first.ident()))
    {
        return Some(Home {
            group_path: INHERENT_GROUP_PATH.to_string(),
            ours: first.ident().to_string(),
        });
    }

    // Where they are two different powers, the one whose INTERNAL name is the display name is
    // the one Mids means, and it is the one the `.mbd` door already reaches: Homecoming's
    // `Swift` is both the standalone inherent and the Fitness pool's `Quick`, and a `.mbd`
    // naming `Inherent.Fitness.Swift` resolves to the former by ident. The identity test is the
    // name map generator's own, so the two doors agree about what counts as one name.
    let wanted = fold_separators(display);
    let mut identical = claimants
        .iter()
        .filter(|power| fold_separators(power.ident()) == wanted);
    let only = identical.next()?;
    identical.next().is_none().then(|| Home {
        group_path: INHERENT_GROUP_PATH.to_string(),
        ours: only.ident().to_string(),
    })
}

/// Is there anything on this entry the dataset does not already re-derive?
///
/// The `.mxd` twin of the `.mbd` reader's own rule, and it decides only whether an entry this
/// dataset cannot place is worth REPORTING. `StatInclude` is deliberately not part of it here:
/// Mids sets it on the mode and combo meters it lists as powers, so reading it would put every
/// one of them back on the receipt.
fn carries_something(record: &crate::mxd::MxdPowerRecord) -> bool {
    record.slots.len() > 1 || record.slots.iter().any(|slot| slot.enhancement.is_some())
}

// ============================================================
// The census behind MXDIMPORT-1.
// ============================================================

/// One entry a read could not place, with the scope that was searched and what was left in it.
///
/// This is the measurement MXDIMPORT-1 asks for and not a second resolver: it opens the same
/// [`Homes`] the read opens and asks it the same question, so what it calls unplaced is what the
/// read calls unplaced, by construction. What it adds is the two things a note cannot carry — the
/// level the file files the pick at, and which powers of the searched scope no other entry in
/// the same file claimed.
#[derive(Debug, Clone, PartialEq)]
pub struct UnplacedEntry {
    /// The display name the post half states.
    pub display: String,
    /// The character level the compressed half files the pick at, 1-based as a `.mbd` states it.
    pub level: i32,
    /// Would the read REPORT this entry, or drop it on [`carries_something`]'s rule? The 33
    /// distinct names MXDIMPORT-1 counts are the reported ones.
    pub reported: bool,
    /// The build's own sets, as our ids, in the order [`Homes::find`] searches them.
    pub scope: Vec<String>,
    /// Powers of that scope that no entry in this file placed: our set id, our display name, and
    /// the level the set makes it available at.
    ///
    /// The leftovers on OUR side of the join. A name that has exactly one of these at its own
    /// level, in every file it appears in, is a forced pairing in MBDIMPORT-16's sense; one with
    /// none is a name no join can reach from here.
    pub residual: Vec<(String, String, u8)>,
    /// Powers of that scope whose INTERNAL name is this display name, separators folded: our set
    /// id, our display name, and the level the set makes it available at.
    ///
    /// The candidate join. A power the game renames keeps its internal name, and the display
    /// Mids held before the rename is that internal name — so a name this dataset no longer
    /// displays may still be one it stores. Measured here rather than used: what matters is
    /// whether it lands on exactly one power per entry, and on the right one.
    pub by_internal: Vec<(String, String, u8)>,
    /// Powerset paths the file declares that this dataset has no set for.
    ///
    /// A path that does not resolve takes every power in it with it, so a miss in a file that has
    /// one is not evidence about names at all — it is the roster gap, arriving one power at a
    /// time. Separating the two is the first cut the census makes.
    pub unresolved_paths: Vec<String>,
    /// The enhancement codes the post half slots into this entry, `Empty` included.
    ///
    /// The file's own oracle on what the entry IS. Mids will not seat a Confuse set in a power
    /// whose allow-list has no Confuse, so the codes say what the power accepts, and a candidate
    /// that accepts none of them is the wrong power however its name reads.
    pub slotted: Vec<String>,
}

/// What one file's read left unplaced, and what the entries it DID place rested on.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UnplacedReport {
    pub entries: Vec<UnplacedEntry>,
    /// Entries the display join placed where a DIFFERENT power of the same scope carries that
    /// name internally: the name as written, the set and power the display join took, and the
    /// set and power the internal-name join would have taken.
    ///
    /// The population at risk from adding an internal-name join: where both joins answer and
    /// disagree, whichever runs first silently wins, and neither is loud. Zero is the answer that
    /// makes the order a free choice; anything else has to be adjudicated before the order is.
    pub display_join_contended: Vec<(String, String, String)>,
    /// Entries whose internal-name claimant is a power another entry of the same file already
    /// placed — a join that would hand the build the same power twice.
    pub internal_join_collides: usize,
    /// Powerset paths the file declares that this dataset has no set for.
    ///
    /// Reported per FILE and not per miss, because a path that does not resolve is a fact about
    /// the roster whether or not a power in it happened to be picked.
    pub unresolved_paths: Vec<String>,
}

/// Every entry of `file` that this dataset cannot place, with the scope and the leftovers.
///
/// Reads nothing the [`to_mbd_file`] read does not read, and decides nothing it does not decide.
pub fn unplaced(file: &MxdFile, database: &PowerDatabase) -> UnplacedReport {
    let Some(prose) = &file.prose else {
        return UnplacedReport::default();
    };
    let names = database.mids_names.as_ref();
    let homes = Homes::open(
        &file.binary.class_name,
        &file.binary.powersets,
        database,
        names,
    );

    // The reader's own answers, so what the census calls unplaced is what the read calls
    // unplaced, by construction rather than by a second copy of the rule.
    let mut sink = Vec::new();
    let resolved = resolve(&homes, file, prose, database, &mut sink);

    let mut claimed: Vec<String> = Vec::new();
    let mut misses: Vec<(String, i32, bool, Vec<String>)> = Vec::new();
    let mut display_join_contended = Vec::new();
    for ((record, row), home) in file.binary.powers().zip(&prose.rows).zip(&resolved) {
        match home {
            Some(home) => {
                // Measured against the DISPLAY join alone: an entry the internal join placed is
                // not a contest, it is the second door answering where the first said nothing.
                if matches!(homes.find_displayed(&row.power, database), Ok(Some(_))) {
                    let rivals: Vec<String> = internal_claimants(&homes, &row.power, database)
                        .iter()
                        .filter(|(_, power)| !power.ident().eq_ignore_ascii_case(&home.ours))
                        .map(|(set, power)| format!("{set} :: {}", power.name))
                        .collect();
                    if !rivals.is_empty() {
                        display_join_contended.push((
                            row.power.clone(),
                            home.ours.clone(),
                            rivals.join(" | "),
                        ));
                    }
                }
                claimed.push(home.ours.to_lowercase());
            }
            None => misses.push((
                row.power.clone(),
                i32::from(record.level) + 1,
                carries_something(record),
                row.enhancements
                    .iter()
                    .map(|e| e.code.clone())
                    .collect::<Vec<_>>(),
            )),
        }
    }

    let scope: Vec<String> = homes
        .declared
        .iter()
        .chain(&homes.beneath_the_branch)
        .map(|(_, id)| id.clone())
        .collect();
    // Resolved-ness and not membership in `declared`, which dedupes: a pool the author re-picked
    // appears twice and is one entry there, and calling its second mention unresolved would put
    // seven files in the roster-gap bucket that belong in neither.
    let sets = SetLookup::of(database);
    let mut unresolved_paths: Vec<String> = file
        .binary
        .powersets
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .filter(|path| {
            let key = match names {
                Some(names) => names.powerset_key(path),
                None => path.to_lowercase(),
            };
            sets.resolve_path(&key).is_none()
        })
        .map(str::to_string)
        .collect();
    unresolved_paths.sort();
    unresolved_paths.dedup();
    let residual: Vec<(String, String, u8)> = scope
        .iter()
        .flat_map(|id| {
            set_powers(database, id)
                .into_iter()
                .filter(|power| {
                    !claimed
                        .iter()
                        .any(|seen| *seen == power.ident().to_lowercase())
                })
                .map(|power| (id.clone(), power.name.clone(), power.unlock_level()))
                .collect::<Vec<_>>()
        })
        .collect();

    let mut internal_join_collides = 0usize;
    let entries = misses
        .into_iter()
        .map(|(display, level, reported, slotted)| {
            let by_internal: Vec<(String, String, u8)> =
                internal_claimants(&homes, &display, database)
                    .into_iter()
                    .map(|(path, power)| {
                        (path.to_string(), power.name.clone(), power.unlock_level())
                    })
                    .collect();
            if by_internal.len() == 1
                && internal_claimants(&homes, &display, database)
                    .iter()
                    .any(|(_, power)| {
                        claimed
                            .iter()
                            .any(|seen| *seen == power.ident().to_lowercase())
                    })
            {
                internal_join_collides += 1;
            }
            UnplacedEntry {
                display,
                level,
                reported,
                scope: scope.clone(),
                residual: residual.clone(),
                by_internal,
                unresolved_paths: unresolved_paths.clone(),
                slotted,
            }
        })
        .collect();
    UnplacedReport {
        entries,
        display_join_contended,
        internal_join_collides,
        unresolved_paths,
    }
}

/// Powers of the build's own scope whose INTERNAL name is `display`, separators folded.
///
/// Keyed on our set id rather than Mids' path, because the census reports against our rosters.
fn internal_claimants<'a>(
    homes: &'a Homes,
    display: &str,
    database: &'a PowerDatabase,
) -> Vec<(&'a str, &'a Power)> {
    let wanted = fold_separators(display);
    homes
        .declared
        .iter()
        .chain(&homes.beneath_the_branch)
        .flat_map(|(_, set_id)| {
            set_powers(database, set_id)
                .into_iter()
                .filter(|power| fold_separators(power.ident()) == wanted)
                .map(|power| (set_id.as_str(), power))
                .collect::<Vec<_>>()
        })
        .collect()
}

// ============================================================
// What is in a slot.
// ============================================================

fn slot_entry(
    slot: &MxdSlot,
    spelled: &MxdProseEnhancement,
    power: &str,
    names: Option<&MidsEnhNames>,
    notes: &mut Vec<ImportNote>,
    summary: &mut MxdSummary,
) -> MbdSlotEntry {
    let level = i32::from(slot.level) + 1;
    let empty = MbdSlotEntry {
        level,
        is_inherent: false,
        enhancement: None,
        flipped_enhancement: Value::Null,
    };
    let Some(reference) = &slot.enhancement else {
        return empty;
    };
    let Some(names) = names else {
        summary.enhancements_unnamed += 1;
        return empty;
    };

    match names.resolve(&spelled.code, Some(reference.index)) {
        Ok((record, route)) => {
            summary.enhancements_named += 1;
            match route {
                MidsEnhRoute::Code => summary.by_code += 1,
                MidsEnhRoute::CodeDisambiguatedByIndex => summary.by_code_and_index += 1,
                MidsEnhRoute::IndexWithinNamedSet => summary.by_index_within_set += 1,
            }
            MbdSlotEntry {
                enhancement: Some(enhancement(reference, record.kind, record.uid.clone())),
                ..empty
            }
        }
        Err(refusal) => {
            summary.enhancements_unnamed += 1;
            notes.push(ImportNote {
                context: format!("{power} — {}", spelled.code),
                detail: refusal_detail(&refusal, &spelled.code),
            });
            empty
        }
    }
}

/// One slotted piece in the spelling a `.mbd` states, out of the two bytes the compressed half
/// writes and the kind of record they belong to.
///
/// **The two bytes mean different things on different records**, which is why the kind decides
/// and not a guess — see [`crate::mxd::MxdEnhancementRef::fields`] for the measurement. An
/// invention states its crafted level and its booster count; a Hamidon or an origin piece states
/// a relative level and a grade, and states no crafted level at all, which is why the level
/// below is the one Mids' own `.mbd` writes for those records rather than a reading of a byte
/// that is not there.
fn enhancement(
    reference: &crate::mxd::MxdEnhancementRef,
    kind: MidsEnhKind,
    uid: String,
) -> MbdEnhancement {
    match kind {
        MidsEnhKind::Set | MidsEnhKind::Generic => {
            let (crafted_level, relative) = reference.as_invention();
            MbdEnhancement {
                uid,
                grade: GRADE[0].to_string(),
                io_level: i32::from(crafted_level),
                relative_level: relative_level_token(relative)
                    .unwrap_or_default()
                    .to_string(),
                obtained: false,
            }
        }
        _ => {
            let (relative, grade) = reference.as_graded();
            MbdEnhancement {
                uid,
                grade: GRADE
                    .get(usize::from(grade))
                    .copied()
                    .unwrap_or_default()
                    .to_string(),
                // Mids' own `.mbd` writes 1 for every special and origin record on all 2,187
                // corpus files. The compressed half states no crafted level for one — the byte
                // that would hold it holds the relative level instead — so this is the spelling
                // rather than a reading.
                io_level: 1,
                relative_level: relative_level_token(relative)
                    .unwrap_or_default()
                    .to_string(),
                obtained: false,
            }
        }
    }
}

/// A refusal in words the receipt can carry, naming the code the file used.
fn refusal_detail(refusal: &MidsEnhRefusal, code: &str) -> String {
    match refusal {
        MidsEnhRefusal::Unknown => format!(
            "the Mids enhancement database this dataset carries has nothing called {code:?}, and \
             the file's own index does not land in a set that code names. Mids renames pieces \
             between releases, so an older file can spell a piece this database no longer does"
        ),
        MidsEnhRefusal::Ambiguous(count) => format!(
            "Mids gives {code:?} to {count} different records and the file's index names neither, \
             so which one the author slotted is not in the file"
        ),
        MidsEnhRefusal::SetDisagrees(set) => format!(
            "the post calls this {code:?} and the data block's index lands in {set:?}; the two \
             halves of the file name different sets, so neither is evidence for the other"
        ),
    }
}
