//! The game client's JSON build — what the CoH-to-Sidekick importer sends in an `/import#…` link.
//!
//! A different document from `/buildsave`'s text ([`crate::game_export`]), carrying the same
//! build: the character, then every power the client knows about with the boosts slotted in it.
//! The beta reads it in `utils/external-import/converter.ts` by reshaping it into its
//! `/buildsave` structure and handing that to the importer every other game document goes
//! through, and this module does the same thing for the same reason — the power and boost
//! resolution in [`crate::game_import`] is where a record is looked up against a dataset, and a
//! second copy of that for a second spelling of the same build would be two places for the
//! lookups to disagree.
//!
//! What this format carries that `/buildsave` does not is accolades and incarnates, so those two
//! come out alongside the reshaped export rather than inside it, for the caller to resolve.
//!
//! **Read, not trusted.** The link is a URL anyone can compose, so every field is checked on the
//! way in and a document that does not have the shape is refused with what was missing — the
//! beta's own refusal, `missing build.character or build.powers`, is the one kept here.

use crate::boost_index::{BoostEntry, BoostIndex};
use crate::game_export::{ExportHeader, ExportedEnhancement, ExportedPower, GameExport, Slot};
use crate::level::Level;
use serde::Deserialize;

/// The document as the client writes it. Only the fields this reader uses are declared; the
/// client's others (`ok`, the boost's own `categoryName`) are ignored rather than required.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientDocument {
    pub build: ClientBuild,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientBuild {
    pub character: ClientCharacter,
    pub powers: Vec<ClientPower>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClientCharacter {
    pub name: String,
    /// `class_scrapper` — the class record's name, lower-cased.
    pub archetype: String,
    pub origin: String,
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientPower {
    /// `scrapper_melee`, `pool`, `epic`, `inherent`, `fitness`, `incarnate`, `temporary_powers`,
    /// `prestige`.
    pub category_name: String,
    pub power_set_name: String,
    pub power_name: String,
    /// The level the power was bought at. `0` is the client's spelling for a power granted
    /// rather than bought, as in `/buildsave`.
    pub power_level_bought: u32,
    /// Slots bought on top of the one every power comes with.
    #[serde(default)]
    pub power_num_boosts_bought: u32,
    #[serde(default)]
    pub boosts: Vec<ClientBoost>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientBoost {
    /// Which slot: `null` for the slot the power came with, `1..=n` for the bought ones.
    pub idx: Option<u32>,
    /// The boost RECORD's name, `crafted_crushing_impact_b` — the game addresses a boost as
    /// `Boosts.<record>.<record>`, so the set slot holds the record itself (see the correction
    /// in [`crate::game_import`]'s module doc).
    pub power_set_name: String,
    /// The crafted level; `0` for an attuned piece, which has no level of its own.
    pub level: u32,
    /// Enhancement boosters, `1..=5`, or `null` for none.
    pub num_combines: Option<u32>,
}

/// What the link held, reshaped: the powers and slots as a `/buildsave` export, plus the two
/// things that format has no line for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientRead {
    pub export: GameExport,
    /// Accolade power names, as the client spelled them, for the caller to match against the
    /// dataset's roster.
    pub accolades: Vec<String>,
    /// `(slot, power)` for every incarnate the client listed — every one the character OWNS,
    /// not only the one slotted, so the caller picks.
    pub incarnates: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClientError {
    #[error("not a game-client build: {0}")]
    Shape(String),
    #[error("the build states level {0}, which is not a level")]
    Level(u32),
}

/// Whether a JSON text is a game-client build rather than a `.skif` or a Mids file: an object
/// with `build.character` and a `build.powers` array. Cheap enough to ask before choosing a
/// reader, and specific enough that no `.skif` answers yes — a `.skif`'s `build` holds an
/// `archetype`, never a `character`.
pub fn is_client_document(text: &str) -> bool {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return false;
    };
    let build = &value["build"];
    build["character"].is_object() && build["powers"].is_array()
}

pub fn parse(text: &str) -> Result<ClientDocument, ClientError> {
    serde_json::from_str(text).map_err(|error| {
        ClientError::Shape(format!(
            "{error} (expected build.character and build.powers)"
        ))
    })
}

/// Reshape the client's build into a `/buildsave` export plus its accolades and incarnates.
///
/// `index` is consulted for one thing only: an attuned piece. The client marks one with level
/// `0` and may name it by its crafted record, where `/buildsave` names the attuned record
/// itself — and [`crate::game_import::resolve_enhancement`] reads attunement off the RECORD.
/// So a level-0 piece whose record is a crafted set piece is swapped for its `Attuned_` twin,
/// when the index holds one. Nothing else is looked up here.
pub fn read(
    document: &ClientDocument,
    index: Option<&BoostIndex>,
) -> Result<ClientRead, ClientError> {
    let character = &document.build.character;
    let level = u8::try_from(character.level)
        .ok()
        .and_then(Level::new)
        .ok_or(ClientError::Level(character.level))?;

    let mut powers = Vec::new();
    let mut accolades = Vec::new();
    let mut incarnates = Vec::new();
    for power in &document.build.powers {
        match power.category_name.as_str() {
            // Prestige powers are cosmetic travel; the beta drops them too.
            "prestige" => {}
            "incarnate" => {
                incarnates.push((power.power_set_name.clone(), power.power_name.clone()))
            }
            // Temporary powers are not part of a build, except the accolades among them.
            "temporary_powers" => {
                if power.power_set_name.eq_ignore_ascii_case("accolades") {
                    accolades.push(power.power_name.clone());
                }
            }
            _ => powers.push(export_power(power, index)),
        }
    }

    Ok(ClientRead {
        export: GameExport {
            header: ExportHeader {
                character_name: character.name.clone(),
                level,
                origin: title_case(&character.origin),
                archetype: title_case(&character.archetype),
            },
            powers,
        },
        accolades,
        incarnates,
    })
}

/// One power, in `/buildsave`'s spelling: title-cased tokens, and one slot per slot the power
/// has, in order, with an empty slot wherever nothing is slotted.
fn export_power(power: &ClientPower, index: Option<&BoostIndex>) -> ExportedPower {
    let category = match power.category_name.as_str() {
        "pool" => "Pool".to_string(),
        "epic" => "Epic".to_string(),
        "inherent" | "fitness" => "Inherent".to_string(),
        other => title_case(other),
    };
    let bought = power.power_num_boosts_bought as usize;
    let mut slots = vec![Slot::Empty; 1 + bought];
    for boost in &power.boosts {
        let position = match boost.idx {
            None => 0,
            Some(idx) if (1..=bought as u32).contains(&idx) => idx as usize,
            // A slot index past what the power bought is a slot the build does not have. The
            // beta drops it; keeping it would add a slot the character never paid for.
            Some(_) => continue,
        };
        slots[position] = Slot::Filled(export_boost(boost, index));
    }
    ExportedPower {
        level: u8::try_from(power.power_level_bought)
            .ok()
            .and_then(Level::new),
        category,
        powerset: title_case(&power.power_set_name),
        power_name: title_case(&power.power_name),
        slots,
    }
}

fn export_boost(boost: &ClientBoost, index: Option<&BoostIndex>) -> ExportedEnhancement {
    let record = title_case(&boost.power_set_name);
    let attuned = boost.level == 0;
    let uid = match attuned {
        true => attuned_twin(&record, index).unwrap_or(record),
        false => record,
    };
    ExportedEnhancement {
        uid,
        // `/buildsave` prints `(1)` for an attuned piece — a count, not a level — and the
        // resolver ignores the level of an attuned record, so the same `1` stands in here.
        stated_level: if attuned { 1 } else { boost.level },
        boosters: boost.num_combines.filter(|&n| n > 0),
    }
}

/// The attuned record for a piece the client marked attuned, when the index holds one.
///
/// Swapped only when the name as sent is NOT already an attuned set piece: either a crafted set
/// piece, or a name the index does not carry at all. The second case matters because some sets
/// exist only attuned — Winter's Bite has no `Crafted_` record — and the Superior sets' crafted
/// and attuned names differ by more than the prefix, so a client spelling the crafted form of
/// one names nothing. Either way the attuned record is looked up, never assumed: a twin the
/// index does not hold leaves the name as sent, to be reported by the resolver.
fn attuned_twin(record: &str, index: Option<&BoostIndex>) -> Option<String> {
    let index = index?;
    let already_attuned = matches!(
        index.get(record),
        Some(BoostEntry::IoSet { attuned: true, .. })
    );
    let crafted_or_unknown = match index.get(record) {
        None => true,
        Some(BoostEntry::IoSet { attuned: false, .. }) => true,
        Some(_) => false,
    };
    if already_attuned || !crafted_or_unknown {
        return None;
    }
    let twin = if let Some(rest) = record.strip_prefix("Superior_Crafted_") {
        format!("Superior_Attuned_{rest}")
    } else {
        format!("Attuned_{}", record.strip_prefix("Crafted_")?)
    };
    matches!(
        index.get(&twin),
        Some(BoostEntry::IoSet { attuned: true, .. })
    )
    .then_some(twin)
}

/// `crafted_crushing_impact_b` → `Crafted_Crushing_Impact_B`: each underscore-separated word
/// capitalised, the rest left as it came. The client writes records lower-case and the
/// dataset's indexes are keyed the way the game's own files spell them.
fn title_case(token: &str) -> String {
    token
        .split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOCUMENT: &str = r#"{
      "ok": true,
      "build": {
        "character": { "name": "Test", "archetype": "class_scrapper", "origin": "magic", "level": 50 },
        "powers": [
          { "categoryName": "scrapper_melee", "powerSetName": "martial_arts", "powerName": "storm_kick",
            "powerLevelBought": 1, "powerNumBoostsBought": 2,
            "boosts": [
              { "idx": null, "categoryName": "boosts", "powerSetName": "crafted_crushing_impact_a", "boostName": "x", "level": 50, "numCombines": 5 },
              { "idx": 2, "categoryName": "boosts", "powerSetName": "crafted_accuracy", "boostName": "x", "level": 35, "numCombines": null },
              { "idx": 9, "categoryName": "boosts", "powerSetName": "crafted_accuracy", "boostName": "x", "level": 35, "numCombines": null }
            ] },
          { "categoryName": "fitness", "powerSetName": "fitness", "powerName": "health",
            "powerLevelBought": 0, "powerNumBoostsBought": 0, "boosts": [] },
          { "categoryName": "incarnate", "powerSetName": "alpha", "powerName": "cardiac_radial_paragon", "powerLevelBought": 0, "powerNumBoostsBought": 0, "boosts": [] },
          { "categoryName": "temporary_powers", "powerSetName": "accolades", "powerName": "atlas_medallion", "powerLevelBought": 0, "powerNumBoostsBought": 0, "boosts": [] },
          { "categoryName": "prestige", "powerSetName": "prestige", "powerName": "sprint", "powerLevelBought": 0, "powerNumBoostsBought": 0, "boosts": [] }
        ]
      }
    }"#;

    /// The link that failed on next.coh-sidekick.com on 2026-09-30 was this shape, handed to the
    /// `.skif` reader because it was JSON. Pins the shape as its own document, and the reshaping
    /// the beta's converter does, one field at a time.
    #[test]
    fn a_client_build_reads_as_the_export_it_describes() {
        assert!(is_client_document(DOCUMENT));
        assert!(!is_client_document(
            r#"{"version":4,"build":{"archetype":"scrapper"}}"#
        ));

        let read = read(&parse(DOCUMENT).unwrap(), None).unwrap();
        assert_eq!(read.export.header.archetype, "Class_Scrapper");
        assert_eq!(read.export.header.origin, "Magic");
        assert_eq!(read.accolades, vec!["atlas_medallion"]);
        assert_eq!(
            read.incarnates,
            vec![("alpha".to_string(), "cardiac_radial_paragon".to_string())]
        );

        // Prestige dropped; the power and the fitness inherent kept.
        assert_eq!(read.export.powers.len(), 2);
        let kick = &read.export.powers[0];
        assert_eq!(kick.category, "Scrapper_Melee");
        assert_eq!(kick.powerset, "Martial_Arts");
        assert_eq!(kick.power_name, "Storm_Kick");
        assert_eq!(kick.level, Level::new(1));
        // One base slot plus two bought; idx 9 is past what was bought and is dropped.
        assert_eq!(kick.slots.len(), 3);
        assert_eq!(
            kick.slots[0],
            Slot::Filled(ExportedEnhancement {
                uid: "Crafted_Crushing_Impact_A".to_string(),
                stated_level: 50,
                boosters: Some(5),
            })
        );
        assert_eq!(kick.slots[1], Slot::Empty);
        assert!(matches!(&kick.slots[2], Slot::Filled(e) if e.uid == "Crafted_Accuracy"));

        let health = &read.export.powers[1];
        assert_eq!(health.category, "Inherent");
        assert_eq!(health.level, None);
    }
}
