//! Who the build is hitting — the target vocabulary, read off the gates that test it.
//!
//! A per-power damage projection cannot avoid this question. An attack does not ship "a damage
//! number": it ships several `Damage` atoms whose `requires` gates disagree about who is being
//! hit. Beheader's are typical —
//!
//! ```text
//! scale 1.00  Melee_Damage          prob 1.00  enttype target> critter eq
//! scale 1.00  Melee_InherentDamage  prob 0.10  <not a minion class, not a player>
//! scale 1.00  Melee_InherentDamage  prob 0.05  <is a minion class>
//! scale 1.26  Melee_PvPDamage       prob 1.00  enttype target> player eq
//! ```
//!
//! — so the rows a reader sees depend on a target, and the last two are the Scrapper critical
//! hit stating its own probability against its own rank fork. The beta answered the same fork by
//! writing the answer down (`averageBonusVsMinions` / `averageBonusVsHigher`, and PvP never
//! computed at all). This module reads it instead.
//!
//! **The tiers are the tokens' own second segment.** Every critter class the gates name has the
//! shape `Class_<Rank>_<Flavour>` — `Class_Minion_Grunt`, `Class_Lt_Sniper`,
//! `Class_Boss_Archvillain`. Splitting on that shape is what yields the rank list; no tier table
//! is authored here, and a fork that added a rank would surface it. The export ships no critter
//! catalogue of its own, so these gates are the only place the vocabulary exists.
//!
//! **What the shape alone cannot do is tell a critter from a player.** The same gates also name
//! the player archetypes, and two of those wear the critter shape exactly —
//! `Class_Arachnos_Soldier` would read as rank "Arachnos". So the discriminator is the export's
//! own archetype catalogue rather than the token: a class whose body folds to a known archetype
//! id is the caster's kind, not a target rank. That fold is the same one
//! [`crate::caster_state`] uses to match conditional ids, and for the same reason — the two
//! spellings (`Class_Arachnos_Soldier` / `arachnos-soldier`) are one identity written twice.

use crate::caster_state::fold;
use crate::database::PowerDatabase;
use std::collections::BTreeSet;

/// The `arch target>` reader, whose operand is the class token a gate compares against.
const TARGET_ARCH_READER: &str = "arch";
/// The reader token that marks a target-side entity read.
const TARGET_ENTITY_READER: &str = "target>";

/// One rank the corpus's gates distinguish, e.g. `Minion`. Held as the export's own segment
/// rather than an enum: a fork that ships a rank this build has never seen must appear in the
/// list, not fall into a catch-all (Rule 1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct TargetRank {
    /// The rank segment as the export spells it — `Minion`, `Lt`, `Boss`.
    pub segment: String,
    /// Every class token in this rank, in corpus order, e.g. `Class_Minion_Grunt`.
    pub classes: Vec<String>,
}

impl TargetRank {
    /// The class token to evaluate gates against when a reader picks this rank. Any member
    /// answers every `Class_<Rank>_*` gate identically for rank purposes, so this is the first
    /// in sorted order — a stable choice, not a preferred flavour.
    pub fn representative(&self) -> &str {
        &self.classes[0]
    }
}

/// The target ranks this dataset's gates distinguish, sorted by rank segment.
///
/// Scans every atom's `requires` gate for `arch target> <Class_…> eq` and groups the tokens by
/// their rank segment. Player-archetype tokens are excluded — they are the PvP identity, which
/// rides on `entity_type` instead.
///
/// `Err` when the archetype catalogue will not parse, rather than a shape-only best effort: the
/// catalogue is the only thing separating a critter rank from a player archetype, so without it
/// the list would offer "Arachnos" as a kind of enemy (Rule 1).
pub fn target_ranks(database: &PowerDatabase) -> Result<Vec<TargetRank>, String> {
    let player_kinds: BTreeSet<String> = database
        .archetypes()?
        .all()
        .iter()
        .map(|archetype| fold(&archetype.id))
        .collect();
    let mut by_segment: std::collections::BTreeMap<String, BTreeSet<String>> = Default::default();

    for power in database.all_powers() {
        for atom in &power.atoms {
            let Some(gate) = atom.requires_expression.as_deref() else {
                continue;
            };
            for token in target_class_tokens(gate) {
                let Some(segment) = rank_segment(&token, &player_kinds) else {
                    continue;
                };
                by_segment.entry(segment).or_default().insert(token);
            }
        }
    }

    Ok(by_segment
        .into_iter()
        .map(|(segment, classes)| TargetRank {
            segment,
            classes: classes.into_iter().collect(),
        })
        .collect())
}

/// The class tokens a gate compares a target's `arch` against. The gate is reverse-polish, so
/// the pattern is the three consecutive tokens `arch`, `target>`, `<Class_…>` followed by `eq`;
/// scanning for that triple is what keeps a source-side `$archetype @Class_Brute ==` (the AT
/// gate, a different reader entirely) out of the result.
fn target_class_tokens(tokens: &[Box<str>]) -> Vec<String> {
    tokens
        .windows(4)
        .filter(|w| {
            w[0].eq_ignore_ascii_case(TARGET_ARCH_READER)
                && w[1].eq_ignore_ascii_case(TARGET_ENTITY_READER)
                && w[3].eq_ignore_ascii_case("eq")
        })
        .map(|w| w[2].to_string())
        .collect()
}

/// The rank segment of a critter class token — the middle of `Class_<Rank>_<Flavour>`.
///
/// `None` for anything that is not a critter rank: a token with no flavour segment
/// (`Class_Scrapper`), and — the case the shape cannot catch — a token whose body names a player
/// archetype the export catalogues (`Class_Arachnos_Soldier` folds onto `arachnos-soldier`).
fn rank_segment(class_token: &str, player_kinds: &BTreeSet<String>) -> Option<String> {
    let body = class_token.strip_prefix("Class_")?;
    if player_kinds.contains(&fold(body)) {
        return None;
    }
    let (rank, flavour) = body.split_once('_')?;
    (!rank.is_empty() && !flavour.is_empty()).then(|| rank.to_string())
}
