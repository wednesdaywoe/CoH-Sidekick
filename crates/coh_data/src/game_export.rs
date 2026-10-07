//! The in-game `/buildsave` export, read back into structures.
//!
//! A running City of Heroes client writes a build as text: a header line naming the character,
//! a profile section listing every power in pick order with one line per enhancement slot, and
//! a badge section after it. This module turns that text into structures and stops there. It
//! resolves nothing against a dataset, because the two jobs fail differently — text that will
//! not parse is a broken file, while a power this fork has never heard of is a build to report
//! on. Keeping them apart means a separator typo cannot reach the resolver's unresolved list,
//! and this parser can be graded against the corpus with no [`crate::PowerDatabase`] loaded.
//!
//! **What the parentheses hold is not decidable here.** `Crafted_Hecatomb_A (50)` states a
//! crafted level; `Attuned_Decimation_A (1)` states a count, because an attuned piece has no
//! level of its own and the client prints `1` for every one of them. Which kind a record is
//! belongs to the boost index, so the number is carried verbatim under a name that does not
//! claim to know, and the meaning is decided where the index is in scope.
//!
//! **An unreadable slot line is retained, never emptied.** `EMPTY` is a slot the player left
//! empty; a line this parser cannot read is a slot holding something it failed to describe.
//! Folding the second into the first loses an enhancement and reports a build smaller than the
//! one the player saved — the silent shape rule 8 exists to refuse. [`Slot::Unreadable`] keeps
//! the line so a reader can say what it could not read.

use crate::level::Level;

/// A build as the game client wrote it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameExport {
    pub header: ExportHeader,
    /// Every power in the profile, in the order the client listed them.
    pub powers: Vec<ExportedPower>,
}

/// The header line: who this build belongs to.
///
/// `origin` and `archetype` are carried as the client spelled them (`Magic`, `Class_Tanker`)
/// rather than resolved to ids, for the same reason the parenthesised number is: the mapping is
/// a dataset question and this module holds no dataset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportHeader {
    pub character_name: String,
    pub level: Level,
    pub origin: String,
    pub archetype: String,
}

/// One power line and the slot lines under it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedPower {
    /// The level the power was taken at, or `None` where the client printed `0` — its spelling
    /// for a power granted rather than picked. [`Level`] rejects `0` so the two cannot be
    /// confused downstream.
    pub level: Option<Level>,
    /// `Inherent`, `Pool`, `Epic`, `Tanker_Defense`, … — the client's own category token.
    pub category: String,
    pub powerset: String,
    pub power_name: String,
    /// The slots in the order the client printed them, which is slot order.
    pub slots: Vec<Slot>,
}

/// One enhancement slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Slot {
    /// The client printed `EMPTY`: a slot the player has not filled.
    Empty,
    Filled(ExportedEnhancement),
    /// A slot line this parser could not read, kept verbatim so it can be reported. Distinct
    /// from [`Slot::Empty`] on purpose — see the module doc.
    Unreadable(String),
}

/// An enhancement as the export names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedEnhancement {
    /// The boost record's own name — `Crafted_Hecatomb_A`, `Attuned_Decimation_A`,
    /// `Crafted_Accuracy`. This is the key `boost-index.json` is keyed by, so it is carried
    /// whole: the piece suffix that separates `Crafted_Hecatomb_A` from `Crafted_Hecatomb_B` is
    /// part of the name, not decoration on it.
    pub uid: String,
    /// The number in parentheses, verbatim. A crafted level for a crafted piece, and a count
    /// for an attuned one — the boost record decides which, and this module does not hold it.
    pub stated_level: u32,
    /// The `+N` boosters, where the export printed any.
    pub boosters: Option<u32>,
}

/// Why a text could not be read as an export at all.
///
/// These are file-level defects, not build-level ones: nothing here describes a power or piece
/// that failed to resolve, because resolution happens downstream against a dataset.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    #[error("no header line — expected `Name: Level N Origin Class_Archetype`")]
    NoHeader,
    #[error("the header states level {stated}, which is not a level")]
    HeaderLevel { stated: u32 },
    #[error("no character profile — expected a `------` rule to open it")]
    NoProfile,
    #[error("a slot line appeared before any power: {line:?}")]
    OrphanSlot { line: String },
    #[error("a line in the profile is neither a power nor a slot: {line:?}")]
    UnreadableProfileLine { line: String },
}

/// Read `text` as a `/buildsave` export.
pub fn parse(text: &str) -> Result<GameExport, ExportError> {
    let lines: Vec<&str> = text.lines().map(str::trim_end).collect();
    Ok(GameExport {
        header: parse_header(&lines)?,
        powers: parse_profile(&lines)?,
    })
}

/// A rule line — the `------------------` that opens and closes each section.
///
/// Matched by composition rather than by length: the client draws it as wide as the section
/// title, so pinning a width here would break on a section this corpus does not contain.
fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|c| c == '-')
}

/// Whether a line is indented, which is how the format marks a slot as belonging to the power
/// above it. The client writes a tab; two spaces is accepted because a build pasted through a
/// chat client or a forum arrives with the tab expanded.
fn is_indented(line: &str) -> bool {
    line.starts_with('\t') || line.starts_with("  ")
}

/// `Charnel: Level 50 Magic Class_Tanker`
///
/// Read from the first line that parses rather than from line 1, because the client emits a
/// leading blank on some exports and a paste can add more.
fn parse_header(lines: &[&str]) -> Result<ExportHeader, ExportError> {
    lines
        .iter()
        .find_map(|line| split_header(line))
        .ok_or(ExportError::NoHeader)?
        .into_header()
}

/// The header's four fields, still untyped. Separated from [`parse_header`] so a line that is
/// simply not the header (every other line in the file) is distinguishable from the header
/// being present and stating something impossible — the first is skipped, the second is an
/// error naming what it stated.
struct HeaderFields<'a> {
    character_name: &'a str,
    level: u32,
    origin: &'a str,
    archetype: &'a str,
}

impl HeaderFields<'_> {
    fn into_header(self) -> Result<ExportHeader, ExportError> {
        let level = u8::try_from(self.level)
            .ok()
            .and_then(Level::new)
            .ok_or(ExportError::HeaderLevel { stated: self.level })?;
        Ok(ExportHeader {
            character_name: self.character_name.to_string(),
            level,
            origin: self.origin.to_string(),
            archetype: self.archetype.to_string(),
        })
    }
}

fn split_header(line: &str) -> Option<HeaderFields<'_>> {
    // The name may itself contain a colon, so the split is on the LAST colon that leaves a
    // well-formed tail rather than the first — a character called "Dr: Vahz" is a legal name.
    let (name, tail) = line.rmatch_indices(':').find_map(|(at, _)| {
        let tail = line[at + 1..].trim();
        header_tail(tail).map(|fields| (line[..at].trim(), fields))
    })?;
    let (level, origin, archetype) = tail;
    (!name.is_empty()).then_some(HeaderFields {
        character_name: name,
        level,
        origin,
        archetype,
    })
}

/// `Level 50 Magic Class_Tanker` — the part after the name.
fn header_tail(tail: &str) -> Option<(u32, &str, &str)> {
    let mut words = tail.split_whitespace();
    if !words.next()?.eq_ignore_ascii_case("Level") {
        return None;
    }
    let level = words.next()?.parse::<u32>().ok()?;
    let origin = words.next()?;
    let archetype = words.next()?;
    // A fifth word means this is not the header line, whatever else it looks like.
    words.next().is_none().then_some((level, origin, archetype))
}

/// The powers between the profile's opening rule and the one that closes it.
///
/// Bounded by the closing rule rather than by end-of-file because the export continues past it
/// — Homecoming writes a badge section next, and every file in the corpus has one. Reading to
/// the end would take badge names for powers.
fn parse_profile(lines: &[&str]) -> Result<Vec<ExportedPower>, ExportError> {
    let opened = lines
        .iter()
        .position(|line| is_rule(line))
        .ok_or(ExportError::NoProfile)?;
    let body = &lines[opened + 1..];
    let closed = body.iter().position(|line| is_rule(line));
    let body = &body[..closed.unwrap_or(body.len())];

    let mut powers: Vec<ExportedPower> = Vec::new();
    for line in body {
        if line.trim().is_empty() {
            continue;
        }
        if is_indented(line) {
            let slot = parse_slot(line.trim());
            let power = powers.last_mut().ok_or_else(|| ExportError::OrphanSlot {
                line: line.trim().to_string(),
            })?;
            power.slots.push(slot);
            continue;
        }
        let power = parse_power_line(line).ok_or_else(|| ExportError::UnreadableProfileLine {
            line: line.trim().to_string(),
        })?;
        powers.push(power);
    }
    Ok(powers)
}

/// `Level 1: Inherent Inherent Brawl`
fn parse_power_line(line: &str) -> Option<ExportedPower> {
    let (level_part, rest) = line.split_once(':')?;
    let mut level_words = level_part.split_whitespace();
    if !level_words.next()?.eq_ignore_ascii_case("Level") {
        return None;
    }
    let level = level_words.next()?.parse::<u8>().ok()?;
    if level_words.next().is_some() {
        return None;
    }
    let mut fields = rest.split_whitespace();
    let (category, powerset, power_name) = (fields.next()?, fields.next()?, fields.next()?);
    if fields.next().is_some() {
        return None;
    }
    Some(ExportedPower {
        // `0` is the client's spelling for granted-not-picked; `Level::new` folds it to `None`.
        level: Level::new(level),
        category: category.to_string(),
        powerset: powerset.to_string(),
        power_name: power_name.to_string(),
        slots: Vec::new(),
    })
}

/// `EMPTY`, `Crafted_Hecatomb_A (50)`, or `Crafted_Armageddon_A (50+5)`.
///
/// Returns a [`Slot`] rather than an `Option` because there is no third answer: a line that is
/// not `EMPTY` and does not parse is a slot whose contents could not be read, and saying so is
/// the point.
fn parse_slot(trimmed: &str) -> Slot {
    if trimmed == "EMPTY" {
        return Slot::Empty;
    }
    match split_slot(trimmed) {
        Some(enhancement) => Slot::Filled(enhancement),
        None => Slot::Unreadable(trimmed.to_string()),
    }
}

fn split_slot(trimmed: &str) -> Option<ExportedEnhancement> {
    let (uid, tail) = trimmed.split_once('(')?;
    let uid = uid.trim();
    // A uid is one token. Two would mean the split landed inside something this parser has not
    // been shown, and guessing which half is the name is how a wrong record gets resolved.
    if uid.is_empty() || uid.split_whitespace().count() != 1 {
        return None;
    }
    let inner = tail.strip_suffix(')')?;
    let (stated_level, boosters) = match inner.split_once('+') {
        Some((level, boosters)) => (level, Some(boosters.trim().parse::<u32>().ok()?)),
        None => (inner, None),
    };
    Some(ExportedEnhancement {
        uid: uid.to_string(),
        stated_level: stated_level.trim().parse::<u32>().ok()?,
        boosters,
    })
}
