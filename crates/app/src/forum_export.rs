//! Forum export — the build as a post someone else can read.
//!
//! A `.skif` can only be opened by this planner. Sharing a build with the community means
//! pasting it into a forum thread, a subreddit or a Discord message, and the format that has
//! meant "a City of Heroes build" there for years is Mids Reborn's forum post: the powers in
//! level order, what is slotted in each, and what the slotting bought. This is that post,
//! rendered from the build the planner already holds.
//!
//! # Three formats, one document
//!
//! [`ForumFormat`] is the only thing that varies. Every section is built once as a sequence of
//! [`Line`]s that say what they ARE — a heading, a list item, a rule — and the format decides how
//! each is marked up. A format is therefore a dozen lines of markup vocabulary and cannot
//! disagree with another about the document's content; that is the shape the beta's
//! `forum-export.ts` also reached, and the reason it is preserved.
//!
//! # Everything shown is read from the engine, not recounted
//!
//! The set-bonus section is the case that matters. The beta counts pieces per power, looks the
//! set up, and lists every tier at or below that count — which ignores the Rule of 5 entirely, so
//! its post credits a build with bonuses the game refuses to grant it. Here the section is
//! [`crate::panels::powers::build_set_bonus_block`], the same builder behind the slot tooltip:
//! the descriptions the set data authors, and each bonus's Rule-of-5 verdict read from the
//! build's own [`coh_math::set_bonuses`] tracking. The post and the tooltip cannot disagree, and a
//! refused bonus is marked refused rather than advertised.
//!
//! **Which verdict, though, is the trap.** The flag to hand is the BUCKET's `capped`, and it is
//! true of every copy once a sixth exists — so a post keyed on it strikes out all six and says
//! the build gets none of a bonus it gets five of. That is the same lie pointing the other way.
//! The per-copy question has its own lookup
//! ([`coh_math::set_bonuses::bonus_instance_rejected`]), and only asserting the exact number of
//! marks tells the two readings apart.
//!
//! Totals are the same argument one layer up: every number is a [`StatDef::read`] off
//! [`CalculatedTotals`], the field the dashboard row itself shows.
//!
//! # What this deliberately does not print
//!
//! # Slot levels
//!
//! Each slotted piece is prefixed the way Mids writes it: `A:` for the slot that came with the
//! power, then the character level each later slot was granted at
//! ([`coh_data::slot_levels::slot_levels`]), and `?` for a slot the schedule has no grant for.

use crate::build_session::BuildSession;
use crate::modal::{Modal, ModalSize};
use crate::naming;
use crate::panels::dashboards::{DashboardConfig, Dashboards};
use crate::panels::powers::build_set_bonus_block;
use crate::panels::stat_registry::{self, StatDef, StatSection};
use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use coh_data::{CharacterState, Enhancement, EnhancementKind, SelectedPower};
use coh_math::CalculatedTotals;
use dioxus::prelude::*;

/// The markup vocabulary of one destination.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ForumFormat {
    /// No markup at all. Pastes intact anywhere, including places that would show the tags.
    Plain,
    /// phpBB / vBulletin — the Homecoming forums' own.
    BbCode,
    /// CommonMark — Reddit, Discord, and most modern forums.
    Markdown,
}

impl ForumFormat {
    /// Every format, in the order the picker offers them: safest first.
    pub const ALL: [ForumFormat; 3] = [
        ForumFormat::Plain,
        ForumFormat::BbCode,
        ForumFormat::Markdown,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ForumFormat::Plain => "Plain text",
            ForumFormat::BbCode => "BBCode",
            ForumFormat::Markdown => "Markdown",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            ForumFormat::Plain => "Pastes anywhere; no styling",
            ForumFormat::BbCode => "The Homecoming forums, and other phpBB boards",
            ForumFormat::Markdown => "Reddit, Discord, modern forums",
        }
    }

    /// A stable id, for the DOM and for a caller that persists the choice.
    pub fn id(self) -> &'static str {
        match self {
            ForumFormat::Plain => "plain",
            ForumFormat::BbCode => "bbcode",
            ForumFormat::Markdown => "markdown",
        }
    }

    fn bold(self, text: &str) -> String {
        match self {
            ForumFormat::Plain => text.to_string(),
            ForumFormat::BbCode => format!("[b]{text}[/b]"),
            ForumFormat::Markdown => format!("**{text}**"),
        }
    }

    fn italic(self, text: &str) -> String {
        match self {
            ForumFormat::Plain => text.to_string(),
            ForumFormat::BbCode => format!("[i]{text}[/i]"),
            ForumFormat::Markdown => format!("*{text}*"),
        }
    }

    /// A refused bonus: struck out where the destination can strike, and named where it cannot.
    ///
    /// Plain text has no strikethrough and the fact is load-bearing — a reader must not take a
    /// refused bonus for a granted one — so there it becomes words rather than losing the mark.
    fn refused(self, text: &str) -> String {
        match self {
            ForumFormat::Plain => format!("{text}  [refused — over the Rule of 5]"),
            ForumFormat::BbCode => format!("[s]{text}[/s]"),
            ForumFormat::Markdown => format!("~~{text}~~"),
        }
    }
}

/// Which optional sections the post carries. The powers list is not among them: a build post
/// without its powers is not a build post.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ForumSections {
    pub set_bonuses: bool,
    pub incarnates: bool,
    pub totals: bool,
}

impl Default for ForumSections {
    /// All on. Every section suppresses itself when empty, so a build with no incarnates costs
    /// nothing by having them enabled — and the default that shows the most is the one that needs
    /// no visit to the checkboxes.
    fn default() -> Self {
        ForumSections {
            set_bonuses: true,
            incarnates: true,
            totals: true,
        }
    }
}

// ============================================================
// The document.
// ============================================================

/// One line of the post, in terms of what it is rather than how it looks.
///
/// This is what keeps the three formats from drifting: a section emits these, and only
/// [`render_lines`] knows about brackets, asterisks or indentation.
#[derive(Clone, PartialEq, Debug)]
enum Line {
    /// A section's own name, or a row that heads the items under it.
    Heading(String),
    /// Emphasised running text.
    Note(String),
    /// A `label: value` pair, where the label carries the emphasis.
    Field { label: String, value: String },
    /// An item under the heading above it. Lists render as a RUN, because BBCode wraps one in
    /// tags a per-line renderer could neither open nor close.
    ///
    /// `refused` is carried here rather than spliced into the text, so that how a refusal is
    /// SHOWN stays the format's decision and not the section builder's.
    Item { text: String, refused: bool },
    /// A section break.
    Rule,
    /// Vertical space.
    Blank,
}

/// Render the document. The only place markup exists.
fn render_lines(format: ForumFormat, lines: &[Line]) -> String {
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 4);
    let mut in_list = false;
    for line in lines {
        let is_item = matches!(line, Line::Item { .. });
        if format == ForumFormat::BbCode && is_item != in_list {
            out.push(if is_item { "[list]" } else { "[/list]" }.to_string());
        }
        in_list = is_item;

        out.push(match line {
            Line::Heading(text) => format.bold(text),
            Line::Note(text) => format.italic(text),
            Line::Field { label, value } => {
                format!("{} {value}", format.bold(&format!("{label}:")))
            }
            Line::Item { text, refused } => {
                let text = if *refused {
                    format.refused(text)
                } else {
                    text.clone()
                };
                match format {
                    ForumFormat::Plain => format!("    {text}"),
                    ForumFormat::BbCode => format!("[*]{text}"),
                    ForumFormat::Markdown => format!("- {text}"),
                }
            }
            Line::Rule => match format {
                ForumFormat::Plain => "-".repeat(48),
                ForumFormat::BbCode => "[hr]".to_string(),
                ForumFormat::Markdown => "---".to_string(),
            },
            Line::Blank => String::new(),
        });
    }
    if in_list && format == ForumFormat::BbCode {
        out.push("[/list]".to_string());
    }

    // Sections end with a blank, so the document would too. A paste that arrives with three
    // empty lines at the end is one the user has to clean up; one closing newline is the
    // convention for a text file.
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out.push(String::new());
    out.join("\n")
}

/// Everything the renderer reads, gathered once so the section builders stay pure over it.
pub struct ForumInput<'a> {
    pub build: &'a CharacterState,
    pub database: &'a Db,
    pub totals: &'a CalculatedTotals,
    /// The dashboard's visible rows. The totals section shows what THIS dashboard shows rather
    /// than a second curated list, which would be one more stat vocabulary to keep in step.
    pub visibility: &'a Dashboards,
    /// Every slot's level, or `None` when the dataset has no levelling schedule to solve against.
    pub slot_levels: Option<&'a coh_data::slot_levels::SlotLevels>,
}

/// The build, as a forum post.
pub fn render(input: &ForumInput, format: ForumFormat, sections: ForumSections) -> String {
    let mut lines = header(input);
    lines.extend(powers(input));
    if sections.set_bonuses {
        lines.extend(set_bonus_section(input));
    }
    if sections.incarnates {
        lines.extend(incarnates(input));
    }
    if sections.totals {
        lines.extend(totals(input));
    }
    render_lines(format, &lines)
}

/// A section's opening: a break, its name, and the space under it.
fn section_head(title: &str) -> Vec<Line> {
    vec![
        Line::Rule,
        Line::Blank,
        Line::Heading(title.to_string()),
        Line::Blank,
    ]
}

// ============================================================
// Sections.
// ============================================================

/// Who the build is, and which game it is for.
///
/// The fork is stated, which the beta's post does not do. A Rebirth build pasted into a
/// Homecoming thread is not a build that thread can use, and the same power carries different
/// numbers between forks — so the one line that makes the rest legible is which fork produced it.
fn header(input: &ForumInput) -> Vec<Line> {
    let build = input.build;
    let mut lines = Vec::new();

    let name = build.name.trim();
    if !name.is_empty() {
        lines.push(Line::Heading(name.to_string()));
    }
    lines.push(Line::Heading(
        match naming::archetype_name(build, input.database) {
            Some(archetype) => format!("Level {} {archetype}", build.level),
            None => format!("Level {}", build.level),
        },
    ));
    lines.push(Line::Note(format!(
        "Planned with CoH Sidekick — {} data",
        build.dataset.as_str()
    )));
    lines.push(Line::Blank);

    let mut field = |label: String, value: Option<String>| {
        if let Some(value) = value {
            lines.push(Line::Field { label, value });
        }
    };
    field(
        "Primary powerset".to_string(),
        naming::powerset_name(&build.primary, input.database),
    );
    field(
        "Secondary powerset".to_string(),
        naming::powerset_name(&build.secondary, input.database),
    );
    for (index, pool) in build.pools.iter().enumerate() {
        field(
            format!("Power pool #{}", index + 1),
            Some(naming::pool_name(&pool.id, input.database)),
        );
    }
    if let Some(epic) = build.epic_pool.as_ref() {
        field(
            "Epic pool".to_string(),
            Some(naming::pool_name(&epic.id, input.database)),
        );
    }
    lines.push(Line::Blank);
    lines
}

/// Every power in the order the character took them, with what is slotted in each.
///
/// Inherents appear only where something is slotted in them. An unslotted inherent is identical
/// on every character of that archetype and says nothing about this build; a slotted one holds
/// the uniques a build is often planned around, and dropping those would drop the point.
fn powers(input: &ForumInput) -> Vec<Line> {
    let mut listed: Vec<&SelectedPower> = input
        .build
        .all_selected()
        .filter(|power| {
            power.inherent_category.is_none() || power.slots.iter().any(Option::is_some)
        })
        .collect();
    // A stable sort, so powers sharing a level keep the build's own order (primary, then
    // secondary, then pools) instead of an arbitrary one.
    listed.sort_by_key(|power| power.level);

    let mut lines = section_head("Powers");
    for power in listed {
        let name = naming::power_label(Some(input.database), &power.powerset, &power.internal_name);
        lines.push(Line::Heading(format!("Level {}: {name}", power.level)));
        let levels = input
            .slot_levels
            .and_then(|levels| coh_data::slot_levels::levels_of(levels, input.build, power));
        for (index, slotted) in power.slots.iter().enumerate() {
            let Some(slotted) = slotted else { continue };
            let prefix = match (index, levels) {
                (0, _) => "A".to_string(),
                (_, Some(levels)) => levels
                    .get(index)
                    .copied()
                    .flatten()
                    .map_or_else(|| "?".to_string(), |level| level.to_string()),
                (_, None) => "?".to_string(),
            };
            lines.push(Line::Item {
                text: format!("{prefix}: {}", enhancement_label(slotted)),
                refused: false,
            });
        }
        lines.push(Line::Blank);
    }
    lines
}

/// One slotted enhancement: its own name, plus what distinguishes this copy of it.
///
/// An unresolvable piece is named by its id rather than left blank. The codec keeps a slot whose
/// set this dataset does not carry (rule 8), and a blank line in a forum post reads as an empty
/// slot rather than as a piece the reader's own planner may well have.
fn enhancement_label(enhancement: &Enhancement) -> String {
    let name = naming::enhancement_name(enhancement);
    let named = match &enhancement.kind {
        EnhancementKind::IoSet { set_name, .. } if !set_name.is_empty() => {
            format!("{set_name}: {name}")
        }
        _ => name.to_string(),
    };

    let qualifiers = naming::enhancement_qualifiers(enhancement);
    if qualifiers.is_empty() {
        named
    } else {
        format!("{named} ({})", qualifiers.join(", "))
    }
}

/// What the slotting bought, per set per power.
///
/// Grouped this way rather than by stat because the totals section already gives the per-stat
/// view, and because this is the half a reader checks against their own slotting: four pieces of
/// that set, in that power, for these bonuses.
///
/// Only ACTIVE tiers are listed — a threshold the build has not reached is not a bonus it has.
fn set_bonus_section(input: &ForumInput) -> Vec<Line> {
    let Some(catalog) = input.database.io_sets.as_ref() else {
        return Vec::new();
    };
    let mut body = Vec::new();

    for power in input.build.all_selected() {
        // Set ids in slot order, each once: the block is per set, and a six-piece set would
        // otherwise be built (and printed) six times.
        let mut set_ids: Vec<&str> = Vec::new();
        for slot in power.slots.iter().flatten() {
            if let EnhancementKind::IoSet { set_id, .. } = &slot.kind {
                if !set_ids.contains(&set_id.as_str()) {
                    set_ids.push(set_id);
                }
            }
        }

        for set_id in set_ids {
            let Some(set) = catalog.get(set_id) else {
                continue;
            };
            let block = build_set_bonus_block(set, power, set_id, &input.totals.set_bonus_tracking);
            // A lone piece reaches no tier — the export prints only sets whose bonuses are
            // in play.
            if block.slotted < 2 {
                continue;
            }
            let effects: Vec<&crate::panels::powers::BonusEffectRow> = block
                .tiers
                .iter()
                .filter(|tier| tier.active)
                .flat_map(|tier| tier.effects.iter())
                .collect();
            if effects.is_empty() {
                continue;
            }

            let power_name =
                naming::power_label(Some(input.database), &power.powerset, &power.internal_name);
            let set_name = if set.name.is_empty() {
                set_id.to_string()
            } else {
                set.name.clone()
            };
            body.push(Line::Heading(format!(
                "{set_name} — {} of {} in {power_name}",
                block.slotted,
                set.pieces.len()
            )));
            for effect in effects {
                body.push(Line::Item {
                    text: effect.description.clone(),
                    refused: effect.refused,
                });
            }
            body.push(Line::Blank);
        }
    }

    if body.is_empty() {
        return Vec::new();
    }
    let mut lines = section_head("Set bonuses");
    lines.extend(body);
    lines
}

/// The incarnate loadout, slot by slot.
fn incarnates(input: &ForumInput) -> Vec<Line> {
    let mut items = Vec::new();
    for (slot_id, pick) in input.build.incarnates.occupied() {
        let catalog = input.database.incarnate_catalog.slot(slot_id);
        let found = catalog.and_then(|slot| slot.find_power(&pick.power_name));
        let slot_label = catalog
            .map(|slot| slot.display_name.clone())
            .unwrap_or_else(|| slot_id.to_string());
        let power = found
            .map(|power| power.display_name.clone())
            .unwrap_or_else(|| pick.power_name.clone());
        let tier = found
            .map(|power| format!(" ({})", power.tier().label()))
            .unwrap_or_default();
        // An equipped-but-inactive pick is out of the totals below, so it is listed AND said to
        // be off — dropping it would misreport the loadout, printing it plain would misreport
        // the numbers.
        let state = if pick.active { "" } else { " — switched off" };
        items.push(Line::Field {
            label: slot_label,
            value: format!("{power}{tier}{state}"),
        });
    }
    if items.is_empty() {
        return Vec::new();
    }

    let mut lines = section_head("Incarnates");
    lines.extend(items);
    lines.push(Line::Blank);
    lines
}

/// The dashboard, as text.
///
/// The rows are the ones the dashboard is showing — not a second list curated here, which would
/// be one more stat vocabulary to keep in step with [`stat_registry`]. Someone who hid Status
/// Resistance because their build has none is not asking for it in the post either.
fn totals(input: &ForumInput) -> Vec<Line> {
    let mut body = Vec::new();
    for section in StatSection::ALL {
        let rows: Vec<Line> = stat_registry::in_section(section)
            .filter(|stat| input.visibility.shows(stat.id))
            .filter_map(|stat| row(stat, input.totals))
            .collect();
        if rows.is_empty() {
            continue;
        }
        body.push(Line::Heading(section.title().to_string()));
        body.extend(rows);
        body.push(Line::Blank);
    }
    if body.is_empty() {
        return Vec::new();
    }

    let mut lines = section_head("Totals");
    lines.extend(body);
    lines
}

/// One stat row, or `None` where the build has nothing to say about it.
///
/// A zero is dropped rather than printed: forty `0%` lines bury the six numbers a reader came
/// for. This is a display filter and nothing else — the value shown is the engine's, unrounded
/// and unmodified.
fn row(stat: &'static StatDef, totals: &CalculatedTotals) -> Option<Line> {
    let value = (stat.read)(totals);
    (value != 0.0).then(|| Line::Field {
        label: stat.label.to_string(),
        value: stat.format.render(value),
    })
}

// ============================================================
// The modal.
// ============================================================

/// The export's open state, held at the shell root for the containment reason every modal here
/// shares: a `fixed` backdrop is contained by the grid's `transform`ed surfaces (see
/// [`crate::modal`]).
#[derive(Clone, Copy)]
pub struct ForumExportOpen(pub Signal<bool>);

/// Mounted by the shell above both layout roots. Renders nothing while closed.
#[component]
pub fn ForumExportHost(database: Option<Db>) -> Element {
    let mut open = use_context::<ForumExportOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        Modal {
            title: "Share to a forum".to_string(),
            // Wide, because the preview is the surface: a build post is 60-80 columns of
            // monospace and wrapping it would misrepresent what will be pasted.
            size: ModalSize::Xl,
            on_close: move |_| open.set(false),
            ForumExportBody { database }
        }
    }
}

#[component]
fn ForumExportBody(database: Option<Db>) -> Element {
    let session = use_context::<BuildSession>();
    let totals = use_context::<BuildTotals>().0;
    let config = use_context::<DashboardConfig>().0;

    let mut format = use_signal(|| ForumFormat::Plain);
    let mut sections = use_signal(ForumSections::default);
    let mut copied = use_signal(|| Option::<Result<(), String>>::None);

    let Some(database) = database else {
        return rsx! {
            div { class: "load-state", "Loading the dataset…" }
        };
    };

    // A memo, not a signal: the post is a pure function of the build, the totals, the visible
    // stats and the two controls — derived state that is STORED is state that can be stale.
    let post = use_memo({
        let database = database.clone();
        move || {
            let build = session.build.read();
            let slot_levels = database
                .leveling_schedule
                .as_ref()
                .map(|schedule| coh_data::slot_levels::slot_levels(schedule, &build));
            render(
                &ForumInput {
                    build: &build,
                    database: &database,
                    totals: &totals.read(),
                    visibility: &config.read(),
                    slot_levels: slot_levels.as_ref(),
                },
                format(),
                sections(),
            )
        }
    });

    rsx! {
        div { class: "forum-export",
            div { class: "forum-export__controls",
                fieldset { class: "forum-export__formats",
                    legend { class: "forum-export__legend", "Format" }
                    div { class: "forum-export__format-row",
                        for option in ForumFormat::ALL {
                            button {
                                key: "{option.id()}",
                                class: if format() == option { "seg active" } else { "seg" },
                                r#type: "button",
                                title: "{option.hint()}",
                                "aria-pressed": format() == option,
                                onclick: move |_| {
                                    format.set(option);
                                    copied.set(None);
                                },
                                "{option.label()}"
                            }
                        }
                    }
                    p { class: "forum-export__hint", "{format().hint()}" }
                }

                fieldset { class: "forum-export__sections",
                    legend { class: "forum-export__legend", "Include" }
                    SectionToggle {
                        label: "Set bonuses",
                        checked: sections().set_bonuses,
                        on_toggle: move |on| {
                            sections.write().set_bonuses = on;
                            copied.set(None);
                        },
                    }
                    SectionToggle {
                        label: "Incarnates",
                        checked: sections().incarnates,
                        on_toggle: move |on| {
                            sections.write().incarnates = on;
                            copied.set(None);
                        },
                    }
                    SectionToggle {
                        label: "Totals",
                        checked: sections().totals,
                        on_toggle: move |on| {
                            sections.write().totals = on;
                            copied.set(None);
                        },
                    }
                }
            }

            // Editable rather than read-only, and selectable either way: where both clipboard
            // mechanisms are refused, selecting the text by hand is the whole fallback, and a
            // `readonly` box is harder to select from on a touch screen. Nothing reads it back.
            textarea {
                class: "forum-export__preview mono",
                spellcheck: false,
                value: "{post}",
                "aria-label": "The post, as it will be pasted",
            }

            div { class: "forum-export__actions",
                match copied() {
                    Some(Ok(())) => rsx! {
                        span { class: "forum-export__status", "Copied — paste it into the thread." }
                    },
                    Some(Err(reason)) => rsx! {
                        span { class: "forum-export__status is-error",
                            "Nothing was copied: {reason}. Select the text above and copy it."
                        }
                    },
                    None => rsx! {
                        span { class: "forum-export__status is-quiet",
                            "{post.read().len()} characters"
                        }
                    },
                }
                button {
                    class: "seg is-primary",
                    r#type: "button",
                    onclick: move |_| async move {
                        copied.set(Some(crate::clipboard::copy(&post.read().clone()).await));
                    },
                    "Copy the post"
                }
            }
        }
    }
}

/// One include/exclude switch. A checkbox because these are independent — three of them are not
/// a choice between three things.
#[component]
fn SectionToggle(label: String, checked: bool, on_toggle: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "forum-export__toggle",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |evt| on_toggle.call(evt.checked()),
            }
            span { "{label}" }
        }
    }
}
