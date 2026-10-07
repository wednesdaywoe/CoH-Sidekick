//! What the quickbar can hold, and which of it is pinned — the pure data half of the row.
//! No Dioxus and no I/O: [`crate::layout_store`] persists these values and
//! [`super`] renders them.
//!
//! The row holds two kinds of thing and the difference between them is *behaviour*, not
//! location: a panel docks onto the grid and holds a fill while it is there, a tool opens a
//! modal and never does. Both are pinnable, both sit on the same row, and which one an entry
//! is is answered by its variant rather than by a field beside it.

use crate::grid::model::PanelKind;

use crate::view::marks;
use dioxus::prelude::Element;
use serde::{Deserialize, Serialize};

/// One item the user can pin to the quickbar. The VARIANT is the kind: a panel always docks,
/// a tool always opens a modal, by construction — so the renderer branches on it without ever
/// reading a discriminator the stored data could lie about.
///
/// `{ id: String, kind: String }` is the shape this deliberately isn't. Two strings that have
/// to agree is a mapping every write site can get wrong, and the id half would be a second
/// copy of what [`PanelKind`] already is (its title, its group, its slug, its grid geometry).
/// The sum type makes both mistakes unrepresentable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuickBarItem {
    Panel(PanelKind),
    Tool(ToolId),
}

/// How an item behaves when it is clicked. Derived from the variant and never stored, so it
/// cannot drift from the thing it mirrors; it exists so the renderer's behaviour branch is
/// itself exhaustive and a third kind is a compile error rather than a silent default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickbarItemKind {
    /// Docks onto the grid and holds a fill while it is there.
    Panel,
    /// Opens a modal and holds nothing.
    Tool,
}

impl QuickBarItem {
    pub fn kind(&self) -> QuickbarItemKind {
        match self {
            QuickBarItem::Panel(_) => QuickbarItemKind::Panel,
            QuickBarItem::Tool(_) => QuickbarItemKind::Tool,
        }
    }

    /// The item's name, or `None` for a dashboard panel — whose name is the user's and is
    /// resolved through [`crate::panels::dashboards::title_of`] at the render site.
    ///
    /// This module is deliberately free of Dioxus and of I/O, so it has no roster to ask. The
    /// alternative was returning a placeholder for the one case it cannot answer, and a
    /// placeholder here would be the visible text of the quickbar pill as well as its tooltip
    /// — two panels reading "Dashboard" with no way to tell which pill is which.
    // Uncalled: every render site resolves its own text, the `Panel` arm through
    // `fixed_title()` and the `Dashboard` case through `dashboards::title_of` as the doc says.
    // Kept because the doc above is the argument for returning `Option` instead of a placeholder,
    // and that argument is about the quickbar's visible text, not about this accessor.
    #[allow(dead_code)]
    pub fn title(&self) -> Option<&'static str> {
        match self {
            QuickBarItem::Panel(panel) => panel.fixed_title(),
            QuickBarItem::Tool(tool) => Some(tool.title()),
        }
    }

    /// Unique across both kinds — a panel and a tool could otherwise share a slug and collide
    /// as render keys. The variant prefix is what guarantees it.
    pub fn slug(&self) -> String {
        match self {
            QuickBarItem::Panel(panel) => format!("panel-{}", panel.slug()),
            QuickBarItem::Tool(tool) => format!("tool-{}", tool.slug()),
        }
    }
}

/// A modal launcher the quickbar can hold. Mirrors [`PanelKind`]'s job for the other kind of
/// item: the identity, the label and the mark, with the open-signal and the modal host left to
/// the resolver in [`super::ToolLauncher`].
///
/// The hosts themselves are NOT resolved here and are not affected by pinning. A modal that is
/// open has to stay mounted whether or not its launcher is on the row, so every host is
/// mounted unconditionally at the shell root; unpinning moves the way IN, never the surface.
///
/// The organizer is deliberately absent, retired from this roster 2026-09-15 and moved to the
/// Display menu (`super::DashboardEntry`). It failed the membership rule this list already
/// states through [`super::ControlsAction`]'s doc — every launcher here answers a question
/// about the numbers on screen — because it does not answer a question about the numbers, it
/// decides which numbers are on the grid. A stored pin naming it retires cleanly through
/// `Retirable<T>` in [`crate::layout_store`]: it deserializes to `None` and drops, without
/// taking the rest of the pinned set with it.
///
/// The build's FILE acts are deliberately absent, and are no longer on this row at all: they
/// are entries in [`crate::main_menu`] (HM1), where a nested menu of them has somewhere to be.
/// None of them is a single-shot modal with an open flag a launcher could raise, so none was
/// ever a candidate [`ToolId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ToolId {
    Accolades,
    SetBonusFinder,
    DetailedTotals,
    AttackChain,
    WhatIf,
    ProcSources,
    EnhancementList,
    EnhancementTools,
    PowersetCompare,
    CompareSlotting,
    Controls,
}

impl ToolId {
    /// Every variant, in row order. The exhaustive `match` fails to compile when a variant is
    /// added, forcing this list to be extended with it — the same guard [`roster()`]
    /// carries, and for the same reason: a tool missing from here is a tool with no way in.
    pub const ALL: [ToolId; 11] = {
        match ToolId::Accolades {
            ToolId::Accolades
            | ToolId::SetBonusFinder
            | ToolId::DetailedTotals
            | ToolId::AttackChain
            | ToolId::WhatIf
            | ToolId::ProcSources
            | ToolId::EnhancementList
            | ToolId::EnhancementTools
            | ToolId::PowersetCompare
            | ToolId::CompareSlotting
            | ToolId::Controls => {}
        }
        [
            ToolId::Accolades,
            ToolId::SetBonusFinder,
            ToolId::DetailedTotals,
            ToolId::AttackChain,
            ToolId::WhatIf,
            ToolId::ProcSources,
            ToolId::EnhancementList,
            ToolId::EnhancementTools,
            ToolId::PowersetCompare,
            ToolId::CompareSlotting,
            ToolId::Controls,
        ]
    };

    /// The word on the button. One source for it, read by every launcher — a tool labelled one
    /// way on the row and another in the menu is two tools as far as anyone reading is
    /// concerned.
    ///
    /// That every launcher reads it is also what makes the roster *self-checking*, which is
    /// worth more than the consistency: the menu walks `ALL` in order and each row's label is
    /// rendered by whichever component [`super::ToolLauncher`] resolved to. A transposed arm
    /// would therefore print one label twice and drop another, so eleven distinct labels in
    /// declared order is a proof that no arm resolves to a neighbour's component. Hardcode a
    /// label back into a launcher and that proof quietly stops holding.
    pub fn title(&self) -> &'static str {
        match self {
            ToolId::Accolades => "Accolades",
            ToolId::SetBonusFinder => "Set Bonus Finder",
            ToolId::DetailedTotals => "Totals",
            ToolId::AttackChain => "Chains",
            ToolId::WhatIf => "What-if",
            ToolId::ProcSources => "Proc Sources",
            ToolId::EnhancementList => "Enhancement List",
            ToolId::EnhancementTools => "Enhancement Tools",
            ToolId::PowersetCompare => "Compare Powersets",
            ToolId::CompareSlotting => "Compare Slotting",
            ToolId::Controls => "Controls",
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            ToolId::Accolades => "accolades",
            ToolId::SetBonusFinder => "set-bonus-finder",
            ToolId::DetailedTotals => "detailed-totals",
            ToolId::AttackChain => "attack-chain",
            ToolId::WhatIf => "what-if",
            ToolId::ProcSources => "proc-sources",
            ToolId::EnhancementList => "enhancement-list",
            ToolId::EnhancementTools => "enhancement-tools",
            ToolId::PowersetCompare => "powerset-compare",
            ToolId::CompareSlotting => "compare-slotting",
            ToolId::Controls => "controls",
        }
    }

    /// The drawn mark, resolved at render rather than stored on the variant — an icon is a
    /// render-time function of the id, which is what keeps the glyph question (and the
    /// iconoir/heroicons choice behind it) out of this type entirely.
    pub fn mark(&self) -> Element {
        match self {
            ToolId::Accolades => marks::accolades(),
            ToolId::SetBonusFinder => marks::finder(),
            ToolId::DetailedTotals => marks::totals(),
            ToolId::AttackChain => marks::chains(),
            ToolId::WhatIf => marks::what_if(),
            ToolId::ProcSources => marks::procs(),
            ToolId::EnhancementList => marks::list(),
            ToolId::EnhancementTools => marks::lift(),
            ToolId::PowersetCompare => marks::columns(),
            ToolId::CompareSlotting => marks::slotting(),
            ToolId::Controls => marks::keyboard(),
        }
    }
}

/// Whether the roster changes subject between these two neighbours, and so wants a line drawn
/// between them in the menu.
///
/// Two axes, because the roster has two scales of grouping and one rule for both would lose
/// one of them. Across the kinds it is the kind that changes — tools then panels, one seam.
/// Within the panels it is [`PanelGroup`]: thirteen evenly-spaced rows are a list read to the
/// end of once and skimmed forever after, and the clusters give the eye somewhere to aim.
/// Reading the change off the neighbours rather than off a row index is what stops a new tool
/// or a new surface from silently moving a hardcoded seam.
pub fn seam_before(previous: QuickBarItem, item: QuickBarItem) -> bool {
    match (previous, item) {
        (QuickBarItem::Panel(before), QuickBarItem::Panel(panel)) => {
            before.group() != panel.group()
        }
        (before, item) => before.kind() != item.kind(),
    }
}

/// What a fresh install pins (decision 2026-08-24, user-chosen). Five items, each on the row
/// for a stated reason rather than because it fit.
///
/// **The four tools** are the ones a build is read with rather than edited with: what the
/// numbers are (Totals), what would grant a number the build is short of (Set Bonus Finder),
/// what the rotation does with them (Chains), and what it all costs to actually acquire
/// (Enhancement List). The other seven open from the menu, which is the discoverability trade
/// this default is making: a row of twenty-four is the mess the customising exists to end, and
/// an empty row teaches nobody that the row can be filled.
///
/// **The one panel** is exactly the one [`GridItem::default_layout`] ships OFF the grid. A
/// docked panel can always be dismissed from its own header ✕, so its pill is a convenience;
/// for a panel that starts hidden the pill is the only single click that brings it back, and
/// `default_layout`'s own reasoning for hiding it rather than folding it depends on that click
/// existing. Pinning it is what keeps that promise true.
///
/// It was three until the eight fixed stat panels retired (2026-09-14). The rule is unchanged —
/// pin what starts hidden — and the three it used to name were the mez panels, which started
/// hidden because no fresh build shows a row from them. Set Bonuses is now the only surface the
/// default ships off-grid. A dashboard panel is never pinned by default because it is never
/// hidden by default: the user made it, so it is on the grid.
///
/// There is no old pinning behaviour to preserve — pinning did not exist — so this is a fresh
/// default rather than a behaviour-preserving migration.
pub fn default_pins() -> Vec<QuickBarItem> {
    vec![
        QuickBarItem::Tool(ToolId::DetailedTotals),
        QuickBarItem::Tool(ToolId::SetBonusFinder),
        QuickBarItem::Tool(ToolId::AttackChain),
        QuickBarItem::Tool(ToolId::EnhancementList),
        QuickBarItem::Panel(PanelKind::SetBonuses),
    ]
}

/// Whether a restored pinned set is structurally sound: no item pinned twice.
///
/// That is the whole of it, and the shortness is the point. A saved grid layout is checked
/// against "every surface present exactly once", because a surface missing from the grid is a
/// surface with nowhere to be — which is why `load_desktop` reconciles a save BEFORE it
/// validates, appending whatever the save predates. A pinned set has no such completion to
/// perform: **absence is the unpinned state**, and appending a missing item would re-pin
/// something either the user unpinned or never asked for. So the load path validates without
/// reconciling, an empty list is valid, and the menu is where an item the row doesn't name is
/// found.
///
/// A duplicate is still corrupt — two pills for one surface would disagree the moment one was
/// clicked — and it takes the whole restored list down to the default, the same way an
/// invalid layout does.
pub fn validate_pins(pins: &[QuickBarItem]) -> bool {
    pins.iter()
        .enumerate()
        .all(|(index, item)| !pins[..index].contains(item))
}
