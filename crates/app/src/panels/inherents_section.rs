//! The granted-inherents section of the Powers panel — the powers the game hands the
//! character, shown beside the ones they picked.
//!
//! Where it sits depends on the [`PowersLayout`](crate::panels::powers::PowersLayout), because
//! inherents fit the two arrangements differently. The by-level ladder has one cell per pick
//! the character earns and an inherent spends no pick, so there it is a section BELOW the grid.
//! The by-powerset layout has a track for the sets a build didn't choose either — the epic pool
//! — so there the inherents ride in that fourth track (`stacked`), which is both where they are
//! true and what keeps them on screen: the section used to sit below the fold of a panel the
//! free grid lets you shrink, and a granted power nobody can find reads as a missing one.
//!
//! Cards are [`PickedPowerCard`](crate::panels::powers::PickedPowerCard) — the same card the
//! picked powers use — so a slottable inherent (Health, Stamina, Brawl) slots exactly like
//! any other power, and the locked ones simply render without a remove control.

use crate::build_session::BuildSession;
use crate::panels::power_group::PowerGroup;
use crate::panels::powers::PickedPowerCard;
use crate::shell::Db;
use coh_data::InherentCategory;
use dioxus::prelude::*;

/// The categories in display order, most build-specific first: the archetype's own inherent
/// leads, then the fitness powers that carry slots, then the universal basics, then the
/// prestige travel powers. Exhaustive over [`InherentCategory`] by construction — a new
/// variant fails to compile in [`category_title`] below, so it cannot be silently dropped
/// from the display.
const CATEGORY_ORDER: [InherentCategory; 5] = [
    InherentCategory::Archetype,
    InherentCategory::Granted,
    InherentCategory::Fitness,
    InherentCategory::Basic,
    InherentCategory::Prestige,
];

/// A category's column heading. The `match` is exhaustive with no `_` arm on purpose.
fn category_title(category: InherentCategory) -> &'static str {
    match category {
        InherentCategory::Archetype => "Archetype",
        InherentCategory::Granted => "Granted",
        InherentCategory::Fitness => "Fitness",
        InherentCategory::Basic => "Basic",
        InherentCategory::Prestige => "Prestige",
    }
}

/// Whether a category's group opens folded.
///
/// Basic and Prestige hold the powers every character is handed and almost no build plans
/// around — Brawl, Rest, the Sprints — so they start folded and give the room to the two a
/// build does work with. The archetype's own inherent and the fitness powers stay open: both
/// carry powers a build plans around (fitness takes slots, the archetype inherent drives the
/// AT's whole mechanic), and the beta's own bug report for folding those away was players
/// concluding the power was missing entirely.
///
/// Exhaustive with no `_` arm, so a new [`InherentCategory`] has to state its own answer.
fn category_starts_collapsed(category: InherentCategory) -> bool {
    match category {
        // Granted holds the pick-gated grants — the Kheldian form attacks a build slots
        // and plans around, so it opens like the other two plannable groups.
        InherentCategory::Archetype | InherentCategory::Granted | InherentCategory::Fitness => {
            false
        }
        InherentCategory::Basic | InherentCategory::Prestige => true,
    }
}

/// The inherents section: one group per category that has powers, plus a fault line when
/// the archetype declares an inherent this dataset ships no power for.
///
/// `stacked` runs the categories down one column instead of across their own grid — the form
/// the section takes inside the by-powerset layout's fourth track, which is already one column
/// wide.
#[component]
pub fn InherentsSection(database: Db, stacked: bool) -> Element {
    let session = use_context::<BuildSession>();
    let build = session.build;
    let mut collapsed = use_signal(|| false);

    let inherents = build.read().inherents.clone();

    // The archetype names an inherent; the dataset may or may not ship a power for it. The
    // corpus has one absence and it is Thunderspy's Primalist, which is the export's answer
    // rather than a defect: that fork grants and spends Primal Energy from the archetype's
    // own powersets, and files only a meter and a dampen under the name (INHERENT-10).
    //
    // Shown as a marked row either way, because from here the two are the same lookup coming
    // back empty — the verdict lives on the TS side, which the contract does not carry. So
    // the row states the absence and stops there. It used to add "so it contributes nothing
    // to the build", which is exactly the part this lookup cannot know and is false for the
    // one case in the corpus.
    let declared = crate::inherents::declared_inherent_name(&build.read(), &database);
    let resolved = crate::inherents::archetype_inherent(&build.read(), &database);
    let missing_archetype_inherent = declared.filter(|_| resolved.is_none());

    if inherents.is_empty() && missing_archetype_inherent.is_none() {
        return rsx! {};
    }

    let columns: Vec<(InherentCategory, Vec<coh_data::SelectedPower>)> = CATEGORY_ORDER
        .iter()
        .map(|&category| {
            let powers: Vec<coh_data::SelectedPower> = inherents
                .iter()
                .filter(|power| power.inherent_category == Some(category))
                .cloned()
                .collect();
            (category, powers)
        })
        .filter(|(_, powers)| !powers.is_empty())
        .collect();

    rsx! {
        section { class: if stacked { "inherents inherents--stacked" } else { "inherents" },
            button {
                class: "inherents__header",
                "aria-expanded": !collapsed(),
                onclick: move |_| collapsed.toggle(),
                span {
                    class: if collapsed() {
                        "inherents__caret caret--folded"
                    } else {
                        "inherents__caret"
                    },
                    "▼"
                }
                span { class: "inherents__title", "Inherents" }
                span { class: "inherents__count", "{inherents.len()}" }
                span { class: "inherents__note", "granted — no power picks" }
            }

            if let Some(name) = missing_archetype_inherent {
                div { class: "load-state error",
                    "This dataset ships no inherent power named {name}, so this row carries "
                    "no numbers. Whatever the mechanic does comes from elsewhere in the build."
                }
            }

            if !collapsed() {
                div { class: if stacked { "inherents__stack" } else { "power-columns" },
                    for (category, powers) in columns.iter() {
                        PowerGroup {
                            key: "inherent-{category_title(*category)}",
                            title: category_title(*category).to_string(),
                            count: powers.len(),
                            starts_collapsed: category_starts_collapsed(*category),
                            for (index, power) in powers.iter().enumerate() {
                                PickedPowerCard {
                                    key: "inherent-{category_title(*category)}-{index}",
                                    database: database.clone(),
                                    power: power.clone(),
                                    powerset_id: coh_data::INHERENT_SET.to_string(),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
