//! The dashboard roster: which panels the user has built, what each is called, and which stats
//! each one holds in which order.
//!
//! This is the user data that used to be a compile-time constant. A stat's panel was
//! [`StatDef::section`](stat_registry::StatDef::section) — eight fixed groups, one grid surface
//! each, and the only thing the user chose was a flat visible/hidden set. So a build that lives
//! or dies on three numbers could not put those three numbers together: they sat in three
//! panels, each dragging a dozen neighbours, and the only way to quiet a neighbour was to switch
//! it off everywhere it appeared.
//!
//! A panel is a container the user fills now. Which means the roster owns two facts the app had
//! nowhere to put before — **which panel a stat is in**, and **in what order** — and one fact it
//! had in a different shape: *visible* is no longer a flag on a stat, it is the question
//! [`Dashboards::shows`] answers by looking for a home.
//!
//! **One home per stat.** [`Dashboards::place`] moves rather than copies, and the invariant is
//! enforced on the way in from storage as well as on the way through the UI, because a
//! hand-edited or half-written document is the case that would otherwise show one number twice
//! and let the two disagree the moment either was dragged.
//!
//! **What stayed a constant.** [`stat_registry::StatSection`] — the taxonomy — is untouched by
//! any of this. Six surfaces outside the dashboard group stats by family and none of them ever
//! wanted a panel; splitting the two meanings is what kept an arbitrary-panels change out of
//! them. A stat's family is still the registry's to state; only its PLACE became the user's.

use crate::grid::model::DashboardId;
use crate::grid::PanelKind;
use crate::panels::stat_registry::{self, StatDef};
use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

/// The longest panel name the roster will hold, in characters.
///
/// A name is not just a heading: it reaches an `aria-label`, a `title` attribute, the mobile
/// stack's header, a quickbar pill and `localStorage`. `.panel-title` ellipsizes, so a long name
/// costs the user an ellipsis rather than a broken layout — but nothing downstream bounds the
/// string, and a store is the wrong place to learn that. Counted in `chars` rather than bytes so
/// the cap means the same thing for every alphabet.
pub const NAME_MAX: usize = 32;

/// The name a roster with no saved state gives its one panel.
const DEFAULT_NAME: &str = "Dashboard";

/// One user-built panel: what it is called, and the stats it holds in the order it holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardPanel {
    pub id: DashboardId,
    pub name: String,
    /// Stat ids, resolved through [`stat_registry::by_id`] so the list is always a subset of the
    /// live vocabulary — the same guarantee `StatVisibility` got from holding `&'static str`.
    /// A `Vec` rather than a set, because the order is the user's and a set would not keep it.
    pub stats: Vec<&'static str>,
}

/// Every panel the user has, plus the counter that mints the next one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "StoredDashboards", into = "StoredDashboards")]
pub struct Dashboards {
    panels: Vec<DashboardPanel>,
    /// Monotonic. Never counts down, never re-mints. See [`DashboardId`].
    next_id: u32,
}

impl Default for Dashboards {
    fn default() -> Self {
        Dashboards::defaults()
    }
}

impl Dashboards {
    /// What a fresh install opens on: [`stat_registry::DEFAULT_VISIBLE`] split one panel per
    /// category, the way the original Sidekick's dashboard read (2026-10-04, user-directed).
    ///
    /// Only categories the default set fills get a panel, so this is Offense, Defense,
    /// Resistance and Survival with ids 1–4 — the ids `grid::model::DEFAULT_SURFACES` restates.
    /// A single panel was the default before; it put every row under one "Dashboard" heading,
    /// and a new user met the organizer before the dashboard read like the planner they knew.
    pub fn defaults() -> Dashboards {
        let mut dashboards = Dashboards {
            panels: Vec::new(),
            next_id: 1,
        };
        dashboards.regroup_by_category();
        dashboards
    }

    // An `empty()` constructor stood here, building a roster with no panels, and nothing called
    // it -- next door in `coh-sidekick-1.0` only its eight tests do. Deleted 2026-09-26; recover
    // from `git show 71a2b04f0:crates/app/src/panels/dashboards.rs`. Its doc argued that an empty
    // roster must stay REPRESENTABLE, so that a state the user reached is not silently swapped for
    // a default they did not ask for, and that argument is about the type rather than about the
    // constructor: `panels` is a `Vec`, so the state is still representable, and `remove_panel`
    // still reaches it if a surface ever lets the last panel go.

    pub fn panels(&self) -> &[DashboardPanel] {
        &self.panels
    }

    // `is_empty()` went with `empty()` on 2026-09-26: its allow said in as many words that it
    // "reads the state `empty()` above creates, and has the same non-caller". Callers that need
    // the answer have `panels().is_empty()`.

    pub fn ids(&self) -> impl Iterator<Item = DashboardId> + '_ {
        self.panels.iter().map(|panel| panel.id)
    }

    pub fn panel(&self, id: DashboardId) -> Option<&DashboardPanel> {
        self.panels.iter().find(|panel| panel.id == id)
    }

    /// The panel's name, or `None` for an id the roster does not have. The caller decides what
    /// an unknown id means — the grid draws nothing for it, the organizer skips it.
    pub fn name_of(&self, id: DashboardId) -> Option<&str> {
        self.panel(id).map(|panel| panel.name.as_str())
    }

    /// The stats in one panel, in the user's order. Empty for a panel with none AND for an id
    /// the roster does not have; the two are told apart by [`Self::panel`], and no caller that
    /// only wants to draw rows needs to.
    pub fn stats_in(&self, id: DashboardId) -> &[&'static str] {
        self.panel(id).map_or(&[], |panel| panel.stats.as_slice())
    }

    /// Is this stat on the dashboard anywhere? The successor to `StatVisibility::is_visible`,
    /// and the reason the exports needed no other change: "shown" was always the question they
    /// were asking, and a stat with a home is the answer now.
    pub fn shows(&self, id: &str) -> bool {
        self.home_of(id).is_some()
    }

    /// Which panel holds this stat, if any.
    pub fn home_of(&self, id: &str) -> Option<DashboardId> {
        self.panels
            .iter()
            .find(|panel| panel.stats.contains(&id))
            .map(|panel| panel.id)
    }

    /// How many of the vocabulary's stats are placed somewhere. What the organizer's summary
    /// counts.
    pub fn placed_count(&self) -> usize {
        self.panels.iter().map(|panel| panel.stats.len()).sum()
    }

    /// Put a stat in a panel at an index, taking it out of wherever it was.
    ///
    /// **Lift, then drop.** The index is read against the list with the stat already out of it,
    /// which is what makes one implementation serve both a move between panels and a reorder
    /// within one: in the cross-panel case nothing was lifted out of the target, so the index is
    /// just the index; in the same-panel case the lift shifts everything below the old position
    /// up, and the drop lands where the gap is.
    ///
    /// That is [`crate::grid::move_to_index`]'s convention, arrived at the same way — the mobile
    /// reorder menu's drop target is the row under the pointer, and by the time the drop
    /// commits, the dragged row is no longer among the rows being counted. The two agree, which
    /// is why dragging a stat feels the same as dragging a panel. The implementation to avoid is
    /// the one that inserts into the list BEFORE removing the old copy: it counts the dragged
    /// row as one of the rows it is being positioned against, and every downward move lands one
    /// short.
    ///
    /// Takes a `&'static StatDef` rather than an id so a stat outside the vocabulary cannot be
    /// placed at all — the invariant `StatVisibility` got from holding `&'static str`, kept.
    pub fn place(&mut self, stat: &'static StatDef, panel: DashboardId, index: usize) {
        self.unplace(stat.id);
        if let Some(target) = self.panels.iter_mut().find(|held| held.id == panel) {
            target.stats.insert(index.min(target.stats.len()), stat.id);
        }
    }

    /// Append a stat to the end of a panel — what clicking an unplaced pill does, where the
    /// user has named a panel but not a position in it.
    pub fn place_last(&mut self, stat: &'static StatDef, panel: DashboardId) {
        let end = self.panel(panel).map_or(0, |held| held.stats.len());
        self.place(stat, panel, end);
    }

    /// Take a stat off the dashboard. Sweeps every panel rather than the one
    /// [`Self::home_of`] names, so a document that slipped a stat into two panels is repaired
    /// by the first unplace rather than leaving the second copy behind.
    pub fn unplace(&mut self, id: &str) {
        for panel in &mut self.panels {
            panel.stats.retain(|held| *held != id);
        }
    }

    /// The first panel in roster order, or `None` for an emptied roster.
    ///
    /// What a surface that has to name a panel but has not been told which one falls back to —
    /// the browse grid's target before the user picks one. `Option` rather than a panel minted
    /// on demand, because a roster with no panels is a state the user can reach deliberately
    /// and quietly creating one behind them would undo it.
    pub fn first(&self) -> Option<DashboardId> {
        self.panels.first().map(|panel| panel.id)
    }

    /// Put a stat on the dashboard or take it off — the browse grid's pill click.
    ///
    /// Placing appends to the named panel rather than asking where, because a pill click says
    /// "show this" and not "show this HERE"; the position is what dragging is for.
    pub fn toggle(&mut self, stat: &'static StatDef, panel: DashboardId) {
        if self.shows(stat.id) {
            self.unplace(stat.id);
        } else {
            self.place_last(stat, panel);
        }
    }

    /// How many of these stats are on the dashboard — what a group's `n/total` counts.
    pub fn count_shown<'a>(&self, stats: impl IntoIterator<Item = &'a StatDef>) -> usize {
        stats.into_iter().filter(|stat| self.shows(stat.id)).count()
    }

    /// Show all of these, or hide all of them if they are already all shown.
    ///
    /// The asymmetry is the point and it predates the roster: a group header that only ever
    /// cleared would need a second control to refill, and one that only ever filled would have
    /// no way back. "All on unless already all on" is one control for both.
    pub fn toggle_all<'a>(
        &mut self,
        stats: impl IntoIterator<Item = &'a StatDef> + Clone,
        panel: DashboardId,
    ) {
        let all_shown = stats.clone().into_iter().all(|stat| self.shows(stat.id));
        for stat in stats {
            let stat: &'static StatDef =
                stat_registry::by_id(stat.id).expect("a StatDef is one of the registry's own");
            if all_shown {
                self.unplace(stat.id);
            } else if !self.shows(stat.id) {
                self.place_last(stat, panel);
            }
        }
    }

    /// Add an empty panel and answer its id.
    ///
    /// Named from the id rather than from the count, so two panels can never carry one name: a
    /// count-derived "Dashboard 2" collides with an existing "Dashboard 2" the moment anything
    /// has been deleted, and the name is what the user picks the panel out by.
    pub fn add_panel(&mut self) -> DashboardId {
        let id = DashboardId(self.next_id);
        self.next_id += 1;
        self.panels.push(DashboardPanel {
            id,
            name: format!("{DEFAULT_NAME} {}", id.0),
            stats: Vec::new(),
        });
        id
    }

    /// Delete a panel. Its stats become unplaced — they are not deleted, because a stat is not
    /// the panel's to own: it is a row in the vocabulary, and the only thing the user just said
    /// is that they do not want it HERE. Handing the orphans to a surviving panel would be the
    /// app deciding where they go instead.
    pub fn remove_panel(&mut self, id: DashboardId) {
        self.panels.retain(|panel| panel.id != id);
    }

    /// Rename a panel, trimmed and capped at [`NAME_MAX`] characters.
    ///
    /// An all-whitespace name is refused rather than stored: the name is the only thing a
    /// panel's header, its quickbar pill and its Display-menu row have to say what it is, and
    /// three surfaces reading blank is worse than the rename not taking.
    pub fn rename(&mut self, id: DashboardId, name: &str) {
        let trimmed: String = name.trim().chars().take(NAME_MAX).collect();
        if trimmed.is_empty() {
            return;
        }
        if let Some(panel) = self.panels.iter_mut().find(|panel| panel.id == id) {
            panel.name = trimmed;
        }
    }

    /// Replace every panel with one per category, each holding the stats already shown from
    /// that category — the old fixed-panel arrangement, one click away.
    ///
    /// Regroups what is shown rather than showing everything: which stats the user wants is a
    /// choice they already made, and only where those stats sit is being undone. Categories
    /// with nothing shown get no panel. A roster showing nothing regroups the defaults, so the
    /// button never answers with an empty dashboard.
    ///
    /// Stats keep the order they had across the old panels. The new panels get fresh ids rather
    /// than reusing the old ones, for the reason [`DashboardId`] gives.
    pub fn regroup_by_category(&mut self) {
        let mut shown: Vec<&'static str> = self
            .panels
            .iter()
            .flat_map(|panel| panel.stats.iter().copied())
            .collect();
        if shown.is_empty() {
            shown = stat_registry::DEFAULT_VISIBLE
                .into_iter()
                .filter_map(|id| Some(stat_registry::by_id(id)?.id))
                .collect();
        }

        self.panels.clear();
        for section in stat_registry::StatSection::ALL {
            let stats: Vec<&'static str> = shown
                .iter()
                .copied()
                .filter(|id| stat_registry::by_id(id).is_some_and(|stat| stat.section == section))
                .collect();
            if stats.is_empty() {
                continue;
            }
            let id = DashboardId(self.next_id);
            self.next_id += 1;
            self.panels.push(DashboardPanel {
                id,
                name: section.title().to_string(),
                stats,
            });
        }
    }

    // REORDERING THE ROSTER HAS NO WRITER, by decision on 2026-09-26. `move_panel(id, index)`
    // stood here, resorting `panels` through `crate::grid::move_to_index`; nothing had ever called
    // it, in any commit. It is recoverable from `git show 71a2b04f0:crates/app/src/panels/
    // dashboards.rs`.
    //
    // Panel order is kept in three places and this was the only one with no way in. The desktop
    // grid stores rectangles, so order does not arise; the mobile stack stores its own list
    // (`MobileOrder`, reordered by `reorder_menu` from the phone nav's Display menu, dashboards
    // included); and this roster's order is what [`surfaces`] reports, which sets the DEFAULT
    // mobile order and the order dashboards are listed in menus. Its biggest reader is Reset
    // layout (`quickbar::reset_arrangement`), which rebuilds both layouts from `surfaces` -- it
    // only reads, and was unaffected by the deletion. `move_to_index` itself is live and shared
    // with `mobile_order` and the stats organizer's drag.
    //
    // To make roster order user-settable, give the organizer (panels/stats_config.rs) a drag on
    // the CARD the way it already has one on the rows, and write through it.
}

/// Every surface that exists right now: the fixed five, with the user's panels between
/// Power Pools and Set Bonuses.
///
/// **This is what `PanelKind::ALL` used to be**, and the difference is that it has to be asked
/// rather than read. Every load path that completes a layout, every menu that lists what can be
/// shown, and the mobile stack's order all take this as `expected` — so the roster is the single
/// answer to "which surfaces are there", and a stored rectangle naming a surface it does not
/// contain is dropped rather than honoured.
///
/// The order is the old `ALL` order with the eight stat panels replaced in place by whatever
/// stands in their stead. It is not cosmetic: `hidden_panels` reports in it, so two callers
/// comparing lists compare the same list, and `MobileOrder::default_order` is literally it.
pub fn surfaces(dashboards: &Dashboards) -> Vec<PanelKind> {
    let mut roster = vec![PanelKind::Powers, PanelKind::Available, PanelKind::Pools];
    roster.extend(dashboards.ids().map(PanelKind::Dashboard));
    roster.push(PanelKind::SetBonuses);
    roster.push(PanelKind::Info);
    roster
}

/// What a surface is CALLED — the one answer every header, pill, menu row and `aria-label`
/// goes through.
///
/// A fixed surface answers from [`PanelKind::fixed_title`]; a dashboard answers with the name
/// the user typed. The fallback is for an id the roster does not have, which is a surface
/// mid-reconcile rather than a state anything should render — naming it after its id keeps a
/// half-torn-down frame legible instead of blank, and says enough to tell which one it was.
pub fn title_of(panel: PanelKind, dashboards: &Dashboards) -> String {
    if let PanelKind::Dashboard(id) = panel {
        return dashboards
            .name_of(id)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{DEFAULT_NAME} {}", id.0));
    }
    // Every other kind is fixed and `fixed_title` is exhaustive over them, so the fallback is
    // unreachable. It names the kind rather than answering `String::new()` anyway: there is no
    // arrangement of this function that should be able to produce a blank header, and an empty
    // string is the one answer that would be invisible in the place it appeared.
    panel
        .fixed_title()
        .map(str::to_string)
        .unwrap_or_else(|| format!("{panel:?}"))
}

/// The live roster, provided at the shell root so every dashboard panel, the organizer, and the
/// two exports read and write one set. A `Signal` rather than a `Memo`: the organizer owns the
/// edits, the panels observe them. A newtype so it never collides with another `Signal<_>` in
/// context.
#[derive(Clone, Copy, PartialEq)]
pub struct DashboardConfig(pub Signal<Dashboards>);

/// One panel as it is written to storage: ids as owned strings, because `&'static str` has
/// nowhere to borrow from on the way back in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDashboard {
    pub id: u32,
    pub name: String,
    pub stats: Vec<String>,
}

/// The roster as it is written to storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDashboards {
    pub panels: Vec<StoredDashboard>,
    #[serde(default)]
    pub next_id: u32,
}

/// Reading a stored roster IS the reconcile, which is why the conversion lives on the serde
/// boundary rather than in a method a load path could forget to call.
///
/// Five repairs, each for a document that is honest-but-stale or corrupt rather than a shape
/// the app would ever write:
///
/// - a stat id the registry retired in favour of named successors
///   ([`stat_registry::RETIRED`]) becomes those successors, in its place — the S/L, F/C and E/N
///   pair rows open as their six types rather than vanishing from a saved dashboard.
/// - any other stat id this build no longer has is dropped, and its panel is kept — the
///   retirement case `StatVisibility::from_ids` handled, now per panel. Losing the panel because
///   one row on it went stale would cost the user an arrangement to save a row.
/// - a stat named by two panels keeps its FIRST home, because one home is the invariant every
///   other surface is written against.
/// - a duplicate panel id is dropped, since two panels answering to one id would mean two
///   surfaces fighting over one grid rectangle.
/// - `next_id` is floored above every id present, so a hand-edited or truncated counter cannot
///   re-mint a live id.
impl From<StoredDashboards> for Dashboards {
    fn from(stored: StoredDashboards) -> Dashboards {
        let mut panels: Vec<DashboardPanel> = Vec::new();
        let mut placed: Vec<&'static str> = Vec::new();

        for panel in stored.panels {
            let id = DashboardId(panel.id);
            if panels.iter().any(|held| held.id == id) {
                continue;
            }
            let mut stats = Vec::new();
            let wanted_ids = panel.stats.iter().flat_map(|wanted| {
                stat_registry::successors_of(wanted)
                    .map_or_else(|| vec![wanted.as_str()], |successors| successors.to_vec())
            });
            for wanted in wanted_ids {
                let Some(stat) = stat_registry::by_id(wanted) else {
                    continue;
                };
                if placed.contains(&stat.id) {
                    continue;
                }
                placed.push(stat.id);
                stats.push(stat.id);
            }
            let name: String = panel.name.trim().chars().take(NAME_MAX).collect();
            panels.push(DashboardPanel {
                id,
                name: if name.is_empty() {
                    format!("{DEFAULT_NAME} {}", id.0)
                } else {
                    name
                },
                stats,
            });
        }

        let floor = panels.iter().map(|panel| panel.id.0 + 1).max().unwrap_or(1);
        Dashboards {
            panels,
            next_id: stored.next_id.max(floor),
        }
    }
}

impl From<Dashboards> for StoredDashboards {
    fn from(live: Dashboards) -> StoredDashboards {
        StoredDashboards {
            next_id: live.next_id,
            panels: live
                .panels
                .into_iter()
                .map(|panel| StoredDashboard {
                    id: panel.id.0,
                    name: panel.name,
                    stats: panel.stats.into_iter().map(str::to_string).collect(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::model::{GridItem, DEFAULT_SURFACES};

    /// The gate `grid::model::DEFAULT_SURFACES`' own doc has cited since it was written, from the
    /// one side that can see both lists. It did not exist until 2026-09-27.
    ///
    /// `DEFAULT_SURFACES` restates the fresh-install roster inside the grid, deliberately: the
    /// grid is kept ignorant of what a dashboard CONTAINS, so it must not reach into this module
    /// for the default. A restatement drifts, and this is the thing that says so — from here,
    /// because `panels::dashboards` may read the grid and the grid may not read it.
    #[test]
    fn the_authored_default_seats_exactly_the_default_roster() {
        let live = surfaces(&Dashboards::default());
        let mut restated = DEFAULT_SURFACES.to_vec();
        let mut actual = live.clone();
        restated.sort_by_key(|p| format!("{p:?}"));
        actual.sort_by_key(|p| format!("{p:?}"));
        assert_eq!(
            restated, actual,
            "grid::model::DEFAULT_SURFACES has drifted from panels::dashboards::surfaces"
        );

        // And the authored default seats every one of them, which is what makes the restatement
        // load-bearing rather than decorative: `default_layout` builds from `DEFAULT_SURFACES`,
        // so a drift there is a layout missing a surface the app has.
        let seated: Vec<_> = GridItem::default_layout()
            .iter()
            .map(|item| item.panel)
            .collect();
        for panel in &live {
            assert!(
                seated.contains(panel),
                "the authored default seats no {panel:?}, which the fresh roster has"
            );
        }
        assert_eq!(
            seated.len(),
            live.len(),
            "the authored default seats a surface the roster has not"
        );
    }

    #[test]
    fn regroup_by_category_keeps_what_is_shown_and_mints_fresh_ids() {
        let mut dashboards = Dashboards::defaults();
        let extra = dashboards.add_panel();
        let range = stat_registry::by_id("range_bonus").unwrap();
        dashboards.place_last(range, extra);
        let before = dashboards.placed_count();
        let old_ids: Vec<_> = dashboards.ids().collect();

        dashboards.regroup_by_category();

        assert_eq!(dashboards.placed_count(), before);
        let names: Vec<_> = dashboards
            .panels()
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["Offense", "Defense", "Resistance", "Survival"]);
        assert_eq!(
            dashboards.panels()[0].stats,
            [
                "damage",
                "accuracy",
                "tohit",
                "recharge",
                "level_shift",
                "range_bonus"
            ]
        );
        assert!(dashboards.ids().all(|id| !old_ids.contains(&id)));
    }

    #[test]
    fn defaults_split_the_default_set_one_panel_per_category() {
        let dashboards = Dashboards::defaults();
        let panels: Vec<_> = dashboards
            .panels()
            .iter()
            .map(|p| (p.id.0, p.name.as_str(), p.stats.len()))
            .collect();
        assert_eq!(
            panels,
            [
                (1, "Offense", 5),
                (2, "Defense", 2),
                (3, "Resistance", 6),
                (4, "Survival", 5)
            ]
        );
        assert_eq!(
            dashboards.placed_count(),
            stat_registry::DEFAULT_VISIBLE.len()
        );
    }

    /// A dashboard saved while the pair rows existed opens with each pair as its two types, in
    /// the pair's place — and a type the user had already placed elsewhere keeps that home.
    #[test]
    fn a_stored_pair_row_opens_as_its_two_types() {
        let stored = StoredDashboards {
            panels: vec![
                StoredDashboard {
                    id: 1,
                    name: "Tank".to_string(),
                    stats: ["defense_melee", "res_fc", "res_sl", "res_psionic"]
                        .map(String::from)
                        .to_vec(),
                },
                StoredDashboard {
                    id: 2,
                    name: "Other".to_string(),
                    stats: ["res_lethal", "res_en"].map(String::from).to_vec(),
                },
            ],
            next_id: 3,
        };
        let dashboards = Dashboards::from(stored);
        assert_eq!(
            dashboards.panels()[0].stats,
            [
                "defense_melee",
                "res_fire",
                "res_cold",
                "res_smashing",
                "res_lethal",
                "res_psionic"
            ]
        );
        assert_eq!(
            dashboards.panels()[1].stats,
            ["res_energy", "res_negative"],
            "res_lethal's first home was panel 1, via res_sl"
        );
    }

    #[test]
    fn regroup_by_category_of_nothing_shown_regroups_the_defaults() {
        let mut dashboards = Dashboards::defaults();
        let ids: Vec<_> = dashboards.ids().collect();
        for id in ids {
            dashboards.remove_panel(id);
        }

        dashboards.regroup_by_category();

        assert_eq!(
            dashboards.placed_count(),
            stat_registry::DEFAULT_VISIBLE.len()
        );
    }
}
