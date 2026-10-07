//! The build's set bonuses, in one surface (the beta `SetBonusPopup`).
//!
//! The dashboard says what a number is; the detailed sheet says what is making it, stat by stat.
//! Neither answers the question a slotting session actually runs on — *what am I getting out of
//! my sets* — because reaching it there means expanding seventy stats and filtering each one's
//! sources by eye. This is that answer, grouped the way the dashboard groups everything else.
//!
//! # Always-on proc globals are here too
//!
//! A unique piece's global (Luck of the Gambler's +7.5% Recharge, Steadfast Protection's +3%
//! Def, Performance Shifter's +End) is *not* a set bonus — it has its own Rule-of-5 pool and
//! never enters `set_bonus_tracking` — but a reader slotting five LotG pieces is asking exactly
//! the question this surface answers, and the beta folded those rows in for the same reason.
//! They ride `proc_breakdown`, the engine's `type: 'proc'` provenance, and are folded into the
//! row for the stat they move. A Build-Up proc's row is a probability-weighted average of a
//! *timed* self-buff rather than a steady global, so it keeps its place in the detailed sheet
//! and is deliberately left out here.
//!
//! # A panel, not the beta's floating window
//!
//! The beta made this a draggable window because its info panel could not leave its column, so a
//! second readable surface had to float. Every surface here is already a free-grid panel — the
//! reason the pop-out info panel was retracted (2026-07-27) — so this is one too, which makes it
//! placeable beside the Powers panel while slotting and costs nothing while hidden.
//!
//! # Which number a row shows
//!
//! Rows are dashboard stats, and each one's value is read the way the engine reads it, in two
//! steps rather than one:
//!
//! - **Within an accumulator field, sum.** [`coh_math::set_bonuses::breakdown_sources_for_key`]
//!   is the engine's own "what did set bonuses put into this field" — already routed (knockback
//!   protection's ×0.01 scale, the recharge-debuff fan-out into Slow), so `debuffResistSlow`
//!   correctly carries both the bonuses that name Slow and the ones that name Recharge Debuff.
//!   The proc globals for the same field (see above) are summed alongside them.
//! - **Across the fields one row projects from, take the max.** Every row reads one field today.
//!   A row reading a pair would read it the way [`coh_math::finalize`] projects its `def_sl`:
//!   `max(defSmashing, defLethal)`, the larger half — not the sum, which would double every
//!   typed defense bonus, since the calc mirrors each one onto both halves of its pair.
//!
//! Neither step is a rule invented here: both are what the engine does with the same numbers one
//! layer down.
//!
//! # Rejected bonuses keep their row
//!
//! A Rule-of-5 rejection is shown struck through and excluded from the total, the same treatment
//! (and the same reason) as in the detailed sheet: a build paying six slots for a bonus that
//! grants five should be told, and the count is invisible without a line that names it.

use crate::naming::power_label;
use crate::panels::stat_registry::{self, StatDef, StatSection};
use crate::panels::stats::BuildTotals;
use crate::shell::Db;
use coh_math::procs::ProcSourceKind;
use coh_math::set_bonuses;
use coh_math::CalculatedTotals;
use dioxus::prelude::*;

/// One contributing source, as a row under a stat: an IO set's tier or an always-on proc global.
#[derive(Clone, PartialEq, Debug)]
pub struct BonusSource {
    /// The row's left-hand label: the IO set and the tier that fired ("Luck of the Gambler
    /// 3pc") for a set bonus, or the set and piece ("Luck of the Gambler: Defense/Increased
    /// Global Recharge Speed") for an always-on proc global.
    pub set: String,
    /// The set without its tier or piece — "Luck of the Gambler". The unit the "from N sets"
    /// summary counts, and the reason it is carried rather than parsed back off `set`.
    pub set_name: String,
    /// The power holding the pieces.
    pub power: String,
    pub value: f64,
    /// Past the Rule of 5's fifth identical copy: held, and granting nothing.
    pub rejected: bool,
}

/// One dashboard stat's set-bonus + always-on proc-global contribution.
#[derive(Clone, PartialEq, Debug)]
pub struct StatRoll {
    pub stat: &'static StatDef,
    /// The sum of the accepted sources — what this stat's total actually carries.
    pub total: f64,
    pub sources: Vec<BonusSource>,
}

impl StatRoll {
    /// How many held bonuses the Rule of 5 refused. Zero for almost every row; the exception is
    /// the whole reason this surface marks anything.
    pub fn wasted(&self) -> usize {
        self.sources.iter().filter(|source| source.rejected).count()
    }
}

/// Roll the build's set-bonus tracking, plus the always-on proc globals, up into one entry per
/// contributing dashboard stat, in registry order. A stat with neither is absent rather than
/// zero — this surface is a list of what the build HAS, and seventy zero rows would bury the six
/// that matter.
pub fn rolls(totals: &CalculatedTotals, db: Option<&Db>) -> Vec<StatRoll> {
    stat_registry::ALL
        .iter()
        .filter_map(|stat| roll_stat(totals, db, stat))
        .collect()
}

/// One stat's roll, or `None` when nothing feeds it.
///
/// The max across the stat's fields is what makes a pair read once (see the module docs); ties go
/// to the first field, which is the same one the projection would pick.
fn roll_stat(
    totals: &CalculatedTotals,
    db: Option<&Db>,
    stat: &'static StatDef,
) -> Option<StatRoll> {
    let mut best: Option<StatRoll> = None;
    for key in stat.breakdown_keys {
        let mut sources: Vec<BonusSource> =
            set_bonuses::breakdown_sources_for_key(&totals.set_bonus_tracking, key)
                .into_iter()
                .map(|source| BonusSource {
                    set: format!("{} {}pc", source.set_name, source.pieces),
                    set_name: source.set_name,
                    power: power_label(db, &source.power_set, &source.power_internal_name),
                    value: source.value,
                    rejected: source.rejected,
                })
                .collect();
        // The always-on / chance-based proc globals the set-bonus pass never sees. Rejected
        // (`capped`) rows are kept as sources so the R5 mark appears on the row they moved, the
        // same treatment the set-bonus rows get — and the same fact `rule_of_five_rejections`
        // already counts. Proc globals keep their own R5 pool, so these do not merge with a
        // set bonus of equal value below; they are simply reported side by side.
        for source in totals
            .proc_breakdown
            .iter()
            .filter(|source| source.breakdown_key == *key && source.kind != ProcSourceKind::BuildUp)
        {
            sources.push(BonusSource {
                set: format!("{}: {}", source.set_name, source.proc_name),
                set_name: source.set_name.clone(),
                power: power_label(db, &source.power_set, &source.power_internal_name),
                value: source.value,
                rejected: source.capped,
            });
        }
        if sources.is_empty() {
            continue;
        }
        let total: f64 = sources
            .iter()
            .filter(|source| !source.rejected)
            .map(|source| source.value)
            .sum();
        if best.as_ref().is_none_or(|best| total > best.total) {
            best = Some(StatRoll {
                stat,
                total,
                sources,
            });
        }
    }

    best
}

#[component]
pub fn SetBonusTotals(database: Db) -> Element {
    let totals = use_context::<BuildTotals>().0;
    // One read of the shared build memo, the dashboard's compute-once contract. Nothing here
    // recalculates; every number below is the engine's own, re-grouped.
    let totals = totals.read();
    let rolls = rolls(&totals, Some(&database));

    if rolls.is_empty() {
        return rsx! {
            div { class: "set-bonuses",
                p { class: "set-bonuses__empty",
                    "No set bonuses yet."
                }
            }
        };
    }

    // The engine's own count, not a sum of the rows' marks: one refused tier marks every stat it
    // would have granted, so summing the marks would report a single wasted bonus as seven — and
    // the shell banner, which counts refused BONUSES, would sit above this contradicting it.
    // The marks answer "which stats", this answers "how many bonuses".
    let refused = totals.rule_of_five_rejections();
    let sets = distinct_sets(&rolls);

    rsx! {
        div { class: "set-bonuses",
            p { class: "set-bonuses__summary",
                "{rolls.len()} stat{plural(rolls.len())} from {sets} set{plural(sets)}"
                if refused > 0 {
                    span { class: "set-bonuses__wasted-count",
                        " · {refused} bonus{plural_es(refused)} over the Rule of 5"
                    }
                }
            }
            for section in StatSection::ALL {
                {
                    let in_section: Vec<StatRoll> = rolls
                        .iter()
                        .filter(|roll| roll.stat.section == section)
                        .cloned()
                        .collect();
                    // A section the build earns nothing in draws no heading — the alternative is
                    // eight headings over six rows.
                    rsx! {
                        if !in_section.is_empty() {
                            section { class: "set-bonuses__group",
                                h3 { class: "set-bonuses__group-name", "{section.title()}" }
                                for roll in in_section {
                                    StatRollRow { key: "{roll.stat.id}", roll }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One stat, expandable to the sets behind it.
#[component]
fn StatRollRow(roll: StatRoll) -> Element {
    let mut expanded = use_signal(|| false);
    let is_open = expanded();
    let wasted = roll.wasted();

    rsx! {
        div { class: "set-bonus-stat",
            button {
                class: if wasted > 0 {
                    "set-bonus-stat__row has-wasted"
                } else {
                    "set-bonus-stat__row"
                },
                r#type: "button",
                "aria-expanded": is_open.to_string(),
                onclick: move |_| expanded.toggle(),
                span {
                    class: if is_open {
                        "set-bonus-stat__caret"
                    } else {
                        "set-bonus-stat__caret caret--folded"
                    },
                    "▼"
                }
                span { class: "set-bonus-stat__label", "{roll.stat.label}" }
                if wasted > 0 {
                    span {
                        class: "set-bonus-stat__wasted",
                        title: "{wasted} held bonus{plural_es(wasted)} the Rule of 5 refused",
                        "⚠"
                    }
                }
                span { class: "set-bonus-stat__value",
                    "{roll.stat.ledger_format.render_delta(roll.total)}"
                }
            }
            if is_open {
                div { class: "set-bonus-stat__sources",
                    for (index, source) in roll.sources.iter().enumerate() {
                        div {
                            key: "{index}",
                            class: if source.rejected {
                                "set-bonus-source is-rejected"
                            } else {
                                "set-bonus-source"
                            },
                            span { class: "set-bonus-source__set", "{source.set}" }
                            span { class: "set-bonus-source__power", "{source.power}" }
                            span { class: "set-bonus-source__value",
                                if source.rejected {
                                    "Rule of 5"
                                } else {
                                    "{roll.stat.ledger_format.render_delta(source.value)}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// How many distinct sets are contributing — counted over each source's `set_name`, so two tiers
/// of one set count once and the same set slotted in two powers counts once, which is what "from
/// N sets" means to a reader. A proc global counts its own set too: five LotG pieces are one set
/// however many tiers and globals they carry.
fn distinct_sets(rolls: &[StatRoll]) -> usize {
    rolls
        .iter()
        .flat_map(|roll| roll.sources.iter())
        .map(|source| source.set_name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len()
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "s"
    }
}

fn plural_es(count: usize) -> &'static str {
    if count == 1 {
        ""
    } else {
        "es"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use coh_math::procs::{ProcBreakdownSource, ProcSourceKind};

    /// The user report: five Luck of the Gambler +7.5% Recharge pieces reach the recharge total,
    /// but the panel read only `set_bonus_tracking` and so never listed them. They are proc
    /// globals, not set bonuses — the engine files them under `proc_breakdown` — and folding them
    /// in is what this test pins.
    fn lotg(value: f64, capped: bool) -> ProcBreakdownSource {
        ProcBreakdownSource {
            breakdown_key: "recharge".to_string(),
            category: "Recharge".to_string(),
            set_name: "Luck of the Gambler".to_string(),
            proc_name: "Defense/Increased Global Recharge Speed".to_string(),
            value,
            capped,
            kind: ProcSourceKind::AlwaysOn,
            note: String::new(),
            power_internal_name: "Stealth".to_string(),
            power_set: "invisibility".to_string(),
        }
    }

    fn recharge_roll(totals: &CalculatedTotals) -> StatRoll {
        rolls(totals, None)
            .into_iter()
            .find(|roll| roll.stat.id == "recharge")
            .expect("a recharge row for the slotted global")
    }

    #[test]
    fn a_lotg_global_reaches_the_recharge_row() {
        let totals = CalculatedTotals {
            proc_breakdown: vec![lotg(7.5, false)],
            ..Default::default()
        };

        let roll = recharge_roll(&totals);
        assert_eq!(roll.total, 7.5);
        assert_eq!(roll.sources.len(), 1);
        assert_eq!(
            roll.sources[0].set,
            "Luck of the Gambler: Defense/Increased Global Recharge Speed"
        );
        assert_eq!(roll.sources[0].power, "Stealth");
        assert!(!roll.sources[0].rejected);
        assert_eq!(roll.wasted(), 0);
    }

    #[test]
    fn five_lotg_globals_sum_to_37_5_and_a_sixth_is_rejected_not_summed() {
        let totals = CalculatedTotals {
            proc_breakdown: vec![
                lotg(7.5, false),
                lotg(7.5, false),
                lotg(7.5, false),
                lotg(7.5, false),
                lotg(7.5, false),
                lotg(7.5, true),
            ],
            ..Default::default()
        };

        let roll = recharge_roll(&totals);
        assert_eq!(
            roll.total, 37.5,
            "the fifth copy is the last one that counts"
        );
        assert_eq!(
            roll.wasted(),
            1,
            "the sixth is shown struck through, not dropped"
        );
    }

    #[test]
    fn a_build_up_proc_is_not_a_set_bonus_row() {
        let totals = CalculatedTotals {
            proc_breakdown: vec![ProcBreakdownSource {
                breakdown_key: "damage".to_string(),
                category: "Damage".to_string(),
                set_name: "Decimation".to_string(),
                proc_name: "Chance for Build Up".to_string(),
                value: 12.0,
                capped: false,
                kind: ProcSourceKind::BuildUp,
                note: String::new(),
                power_internal_name: "Zapp".to_string(),
                power_set: "electrical-blast".to_string(),
            }],
            ..Default::default()
        };

        assert!(
            rolls(&totals, None)
                .iter()
                .all(|roll| roll.stat.id != "damage"),
            "a timed, probability-weighted Build-Up contribution is not a set-bonus row"
        );
    }

    #[test]
    fn the_summary_counts_a_proc_globals_set() {
        let totals = CalculatedTotals {
            proc_breakdown: vec![lotg(7.5, false)],
            ..Default::default()
        };

        assert_eq!(distinct_sets(&rolls(&totals, None)), 1);
    }
}
