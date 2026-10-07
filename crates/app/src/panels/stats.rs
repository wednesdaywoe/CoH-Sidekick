//! Stats — the whole-build dashboard: [`coh_math::recalculate`] over the live `CharacterState`,
//! projected to [`coh_math::CharacterStats`] (the single capped truth, D4). Reads
//! `CalculatedTotals` only; the per-power effect detail lives in the Info panel.
//!
//! **One component, however many panels.** Every dashboard panel is a
//! [`PanelKind::Dashboard`](crate::grid::PanelKind::Dashboard) carrying nothing but an id, so it
//! can be dragged, resized and placed on its own — a build that lives or dies on three numbers
//! can put those three numbers in one small surface and leave everything else off the grid.
//! They all read the one [`BuildTotals`] context, so splitting the dashboard across panels costs
//! no extra recalculation: compute once, render many.
//!
//! This used to be eight components over eight fixed groups, and the eight were a consequence
//! of where membership was decided rather than a judgement about how a build reads: a stat's
//! panel was a const field on its registry entry, so every group the registry named had to be a
//! surface, whether the user wanted it or not.
//!
//! No panel here knows a stat. Which rows a panel holds and in what order is the user's, held
//! in [`crate::panels::dashboards`]; what each row is called, its colour, its unit and how to
//! read it off the engine are the registry's. This component owns neither — it looks up ids and
//! draws them.
//!
//! **Colour never carries meaning alone.** Hue says which stat FAMILY a number belongs to, so a
//! build's defense numbers can be found at a glance; it says nothing about good or bad. The two
//! things that are good-or-bad — a value at its ceiling, and a total the Rule of 5 shrank — are
//! drawn as a dotted underline and a warning ring, both legible with no colour vision at all.
//!
//! Fail-loud errors do NOT live here. `bonuses.errors` surface in the shell's status strip
//! ([`crate::shell::StatusStrip`]), outside every panel, because an error routed into a panel is
//! only as visible as that panel's placement — and Rule 1 wants it visible unconditionally.

use crate::grid::model::DashboardId;
use crate::panels::dashboards::DashboardConfig;
use crate::panels::stat_registry;
use crate::panels::stats_config::StatsConfigOpen;
use coh_math::CalculatedTotals;
use dioxus::prelude::*;

/// Context handle for the one whole-build recompute, provided at the shell root over the loaded
/// dataset + live build. Every stat panel and the power-card slot tooltip (set-bonus Rule-of-5
/// provenance) read it, so `recalculate` — the full pass pipeline — runs once per edit, never
/// once per reader. A newtype so it never collides with another `Memo<_>` in context.
#[derive(Clone, Copy, PartialEq)]
pub struct BuildTotals(pub Memo<CalculatedTotals>);

/// One dashboard panel's body: the stats the user put in it, in the order they put them,
/// each coloured by family and marked with whichever status cues apply. The panel's *name* is
/// its own title in the grid chrome, so it is never repeated inside the body.
///
/// Takes a [`DashboardId`] where it used to take a `StatSection`, and the difference is the
/// whole change: a section decided its own membership at compile time, so this component read
/// the registry and filtered by a visible flag. A panel's membership is the roster's to state,
/// so it reads the roster — which is also what makes the ORDER the user's, a thing a filter
/// over a static list could not express.
#[component]
pub fn StatGroup(panel: DashboardId) -> Element {
    let totals = use_context::<BuildTotals>().0;
    let dashboards = use_context::<DashboardConfig>().0;
    let mut config_open = use_context::<StatsConfigOpen>().0;

    // Both reads resolve to owned values before `rsx!`, so no borrow of the shared totals memo
    // or the config is held across the render (keep logic above `rsx!`).
    let rows: Vec<_> = {
        let totals = totals.read();
        let dashboards = dashboards.read();
        dashboards
            .stats_in(panel)
            .iter()
            .filter_map(|id| stat_registry::by_id(id))
            .map(|stat| (stat, stat.resolve(&totals)))
            .collect()
    };

    rsx! {
        div { class: "stats",
            if rows.is_empty() {
                // An empty panel is no longer an anomaly — it is what a panel the user just
                // made looks like before they fill it — so this is a prompt rather than an
                // explanation. It still opens the one thing that fills it, scrolled to this
                // panel's own card.
                button {
                    class: "stats-empty",
                    r#type: "button",
                    onclick: move |_| {
                        config_open.set(Some(
                            crate::panels::stats_config::OrganizerTarget::Panel(panel),
                        ))
                    },
                    "Empty — add stats"
                }
            }
            for (stat, resolved) in rows {
                div { class: "stat-row",
                    // The label is the half a narrow surface truncates (see `.stat-label`), and
                    // CSS cannot tell us whether it did — so the full name is carried here
                    // unconditionally rather than only when it is clipped.
                    span { class: "stat-label", title: "{stat.label}", "{stat.label}" }
                    // The hairline between the name and the number. A real element rather than a
                    // pseudo on the label, because `.stat-label` earns its ellipsis from being a
                    // plain text box — making it a flex container to hang a leader off would
                    // cost the truncation that keeps a long stat name out of the value column.
                    span { class: "leader" }
                    span {
                        class: "stat-value mono",
                        class: if resolved.at_cap { "is-at-cap" },
                        class: if resolved.rule_of_five_capped { "is-rule-of-five-capped" },
                        class: if resolved.simulated { "is-simulated" },
                        style: "--stat-hue: {stat.family.token()};",
                        // Both cues are decoration; the title is what carries them to a screen
                        // reader and to anyone who does not read a hue as meaning anything.
                        title: cue_description(&resolved),
                        "{resolved.text}"
                        // A text mark, not only a hue: the whole point is that a SCREENSHOT of
                        // this dashboard cannot pass as the build's own numbers, and a screenshot
                        // carries no tooltip and no colour vocabulary.
                        if resolved.simulated {
                            span { class: "stat-simulated", title: "Includes a simulated team buff", "sim" }
                        }
                    }
                }
            }
        }
    }
}

/// What the row's tooltip says: the stat's own note (what the number was projected from, for the
/// stats that show a projection) followed by what the status cues mean. Empty when the value
/// carries neither, so no row grows a pointless empty tooltip.
fn cue_description(resolved: &stat_registry::ResolvedStat) -> String {
    let mut notes = Vec::new();
    if !resolved.note.is_empty() {
        notes.push(resolved.note.as_str());
    }
    if resolved.at_cap {
        notes.push("At or above its cap.");
    }
    if resolved.rule_of_five_capped {
        notes.push("A contributing set bonus was rejected by the Rule of 5.");
    }
    if resolved.simulated {
        notes.push("SIMULATED — includes a what-if team buff, not the build's own number.");
    }
    notes.join(" ")
}
