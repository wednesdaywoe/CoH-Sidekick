//! The quickbar (the beta's `DashboardActionButtons`): one row of whatever the user pinned,
//! and — at its trailing edge, under the brand — the app's own five actions
//! ([`crate::app_actions`]), which are not pinnable and are the band's other half of the
//! sentence its CSS already states: row 1 is the build's controls, row 2 is the app's. Where things get PINNED moved up to row 1 with HM4/HM5 — the Tools and
//! Display menus there each offer their own half of the roster with a pin toggle beside every
//! row — so this row is pure pins and the way back to a hidden surface is the pill on the row
//! itself.
//!
//! An action lives here rather than the main menu because the roster is what you *do* to the
//! build on screen; [`crate::main_menu`] keeps what the app is set up as and what a build does
//! as a FILE.
//!
//! **The row holds two kinds of thing and does not sort them.** A tool opens a modal; a panel
//! docks onto the grid. That is the entire difference, it is carried by
//! [`QuickBarItem`]'s variant, and it shows up as behaviour rather than as position: a pinned
//! panel holds a fill while it is docked and draws the app's dashed empty-holder edge while it
//! is not, and a tool launcher never does either. So "where do I find Totals" has one answer
//! regardless of how Totals renders, which the old two-band split could not give.
//!
//! Membership and visibility are separate stores on purpose, and the pill is the only place
//! they meet. Whether a panel has a pill at all is the pinned set's business
//! ([`crate::layout_store::persist_quickbar`]); whether it is on the grid is
//! [`GridItem::hidden`]'s, unchanged. Clicking a pill docks and undocks, exactly as it did
//! when the row was fixed; the hover ✕ and the row-1 menus' pin toggles are the only things
//! that touch membership. Folding "pinned" into `GridItem` would have given one write path
//! two answers.
//!
//! Tools carry a mark and panels do not, which is the one channel still doing work a
//! behaviour ought to do. Panel glyphs are the icon pass's job (iconoir vs heroicons, and the
//! three ids with no naturally distinct glyph); until it lands, the mark reads as a kind cue
//! it is not meant to keep.

pub mod model;

use crate::build_session::BuildSession;
use crate::grid::model::{GridItem, PanelGroup, PanelKind};
use crate::layout_store;
use crate::mobile_order::MobileOrder;
use crate::panel_visibility::set_panel_hidden;
use crate::panels;
use crate::popover::{Popover, PopoverOpen};
use crate::view::marks;
use dioxus::prelude::*;
use model::{QuickBarItem, QuickbarItemKind, ToolId};

/// Pin `item` to the end of the row or take it off, persisting the result. The one write path
/// for membership — the hover ✕ and the menu's pin toggle both come through here, so the
/// ordered list and the stored copy of it can never disagree.
///
/// Appending rather than inserting in roster order is what makes the `Vec` an ordered list
/// instead of a set with extra steps: a pin lands where the user last looked for it, and
/// drag-reorder writes into the same list without the two ever needing to be reconciled.
fn set_pinned(mut pins: Signal<Vec<QuickBarItem>>, item: QuickBarItem, pinned: bool) {
    let mut next = pins.peek().clone();
    match pinned {
        // Guarded rather than assumed: a double-click on the pin toggle is one user gesture
        // and two events, and the second one must not be able to author the duplicate that
        // `validate_pins` would throw the whole row away for on the next load.
        true if !next.contains(&item) => next.push(item),
        true => return,
        false => next.retain(|entry| *entry != item),
    }
    layout_store::persist_quickbar(&next);
    pins.set(next);
}

/// The row, mounted by the shell between the header and the layout roots. Renders in every
/// viewport: a pinned pill is the way back to a hidden surface, and the mobile stack hides
/// surfaces through the same flag the grid does.
#[component]
pub fn Quickbar(
    /// What the user pinned, in their order. The shell owns it so it survives the grid
    /// remounting on a dataset switch, exactly as the layout does.
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    /// `None` while the bundle loads — the launchers whose surfaces need definitions say so
    /// rather than pretending to be available.
    database: Option<crate::shell::Db>,
) -> Element {
    let pinned = pins.read().clone();

    rsx! {
        div { class: "quickbar",
            div { class: "quickbar-band",
                div {
                    class: "quickbar-items",
                    role: "toolbar",
                    "aria-label": "Quickbar",
                    if pinned.is_empty() {
                        // An empty row would otherwise read as a failed render rather than as
                        // a choice the user made, and the way back out of it is the Tools and
                        // Display menus up in row 1 (HM4/HM5), which are where things get
                        // pinned from now on.
                        span { class: "quickbar-empty", "Nothing pinned — add from Tools or Display." }
                    }
                    for item in pinned.iter().copied() {
                        PinnedItem {
                            key: "{item.slug()}",
                            item,
                            pins,
                            layout,
                            database: database.clone(),
                        }
                    }
                }
                // The app's own five, at the trailing edge under the brand (2026-09-15,
                // user-directed). A sibling of the pinned row rather than an entry in it:
                // these are not pinnable and never were, so putting them IN the row would
                // mean five items the hover ✕ has no answer for. The pinned row's `flex: 1`
                // is what seats them right without either side naming a width.
                crate::app_actions::AppActions {}
            }
        }
    }
}

/// One pinned entry: the item's own control, plus the ✕ that takes it off the row.
///
/// The ✕ is a sibling of the control rather than a child of it — a button inside a button is
/// invalid, and the two do genuinely different things (one uses the item, one removes it), so
/// each gets its own hit target and its own accessible name. It appears on hover and on
/// keyboard focus, so the gesture is reachable without a pointer.
#[component]
fn PinnedItem(
    item: QuickBarItem,
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    database: Option<crate::shell::Db>,
) -> Element {
    let title = use_item_title(item);
    rsx! {
        div { class: "quickbar-pinned",
            ItemControl { item, layout, database }
            button {
                class: "quickbar-unpin",
                r#type: "button",
                "aria-label": "Unpin {title}",
                title: "Unpin {title}",
                onclick: move |_| set_pinned(pins, item, false),
                {marks::close()}
            }
        }
    }
}

/// The item's own control, resolved from the variant. This is the behaviour branch the kind
/// exists for: a panel gets the pill that docks it, a tool gets the button that opens it, and
/// a third kind of item would fail to compile here rather than render as whichever arm a
/// catch-all happened to name.
#[component]
fn ItemControl(
    item: QuickBarItem,
    layout: Signal<Vec<GridItem>>,
    database: Option<crate::shell::Db>,
) -> Element {
    match item {
        QuickBarItem::Panel(panel) => rsx! { PanelPill { panel, layout } },
        QuickBarItem::Tool(tool) => rsx! { ToolLauncher { tool, database } },
    }
}

/// QB4's resolver: each [`ToolId`] to the component that raises its open signal.
///
/// A `match` to eleven existing components rather than a table of setters, because each of
/// those components already reads its own open flag out of context — and a hook read cannot be
/// done from an event handler or from a table. Swapping the whole component is also what makes
/// the resolver safe under reorder: Dioxus unmounts one component type and mounts another, so
/// no instance ever changes which context it reads.
///
/// The modal HOSTS are not here and do not move. Every one of them is mounted unconditionally
/// at the shell root, above both layout roots, so a modal stays open while its launcher is
/// being unpinned out from under it. Pinning owns the way in, never the surface.
#[component]
fn ToolLauncher(tool: ToolId, database: Option<crate::shell::Db>) -> Element {
    match tool {
        ToolId::Accolades => rsx! { AccoladesAction {} },
        ToolId::SetBonusFinder => rsx! { SetBonusFinderAction {} },
        ToolId::DetailedTotals => rsx! { DetailedTotalsAction {} },
        ToolId::AttackChain => rsx! { panels::attack_chain::AttackChainButton {} },
        ToolId::WhatIf => rsx! { panels::what_if::WhatIfButton {} },
        ToolId::ProcSources => rsx! { ProcSourcesAction {} },
        ToolId::EnhancementList => rsx! { EnhancementListAction {} },
        ToolId::EnhancementTools => rsx! { EnhancementToolsAction { database } },
        ToolId::PowersetCompare => rsx! { PowersetCompareAction {} },
        ToolId::CompareSlotting => rsx! { CompareSlottingAction {} },
        ToolId::Controls => rsx! { ControlsAction {} },
    }
}

/// What the app can OPEN, and the pin toggle for each (HM4: the roster's tool half, lifted
/// off row 2 and given its own row-1 menu). The row is curated to pins, so "what can I open"
/// no longer has an answer on screen — that question used to be the More menu, and now this
/// is. A tool added to [`ToolId`] appears here without any edit to this component.
#[component]
pub fn ToolsMenu(
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    database: Option<crate::shell::Db>,
) -> Element {
    // The roster's tool half: what the user can OPEN. Each row's own control launches the
    // modal and the pin beside it puts the launcher on the quickbar (HM4).
    let tools: Vec<QuickBarItem> = ToolId::ALL.into_iter().map(QuickBarItem::Tool).collect();

    rsx! {
        Popover {
            label: "Tools".to_string(),
            title: "Tools".to_string(),
            modifier: "popover--tools".to_string(),
            div { class: "quickbar-menu", role: "menu",
                div { class: "quickbar-menu-head",
                    span { "Tools" }
                    span { class: "quickbar-menu-hint", "pin to the quickbar" }
                }
                for (index, item) in tools.iter().copied().enumerate() {
                    RosterRow {
                        key: "{item.slug()}",
                        item,
                        opens_cluster: index > 0 && {
                            let tools = &tools;
                            model::seam_before(tools[index - 1], tools[index])
                        },
                        pins,
                        layout,
                        database: database.clone(),
                    }
                }
            }
        }
    }
}

/// What is on screen: the panel checklist (each surface dock its own pill and pin it from
/// here) plus the two arrangement acts — reset and reorder — that used to be loose buttons on
/// the row (HM5). Membership and visibility stay separate stores exactly as the quickbar
/// fixes them: the pill here toggles [`GridItem::hidden`], the pin toggles the quickbar, and
/// this menu is simply the one place both are offered for the same surface.
#[component]
pub fn DisplayMenu(
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    mut reorder_open: Signal<bool>,
    reset_confirm_open: Signal<bool>,
) -> Element {
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let roster = crate::panels::dashboards::surfaces(&dashboards.read());
    // Popped-out surfaces are hidden, but this label counts what the user can put BACK, and a
    // surface in its own window is not off the grid in that sense — it is somewhere else.
    let popped = use_context::<crate::panel_popout::PoppedOut>();
    let hidden = crate::panel_visibility::hidden_panels(&layout.read(), &roster)
        .into_iter()
        .filter(|panel| !popped.holds(*panel))
        .count();
    let label = if hidden == 0 {
        "Display".to_string()
    } else {
        format!("Display · {hidden} hidden")
    };
    // The Reorder row raises a full-screen overlay over the menu itself, and the Reset layout
    // row raises the confirm modal — both close the menu on the way in, because a modal over
    // a popover stacks an unreachable menu behind the thing it answers (the rule the tool
    // launchers' doc states). Both closes happen in child components, because `PopoverOpen`
    // only exists around the Popover's children — this body runs before that provider mounts.
    let panels: Vec<QuickBarItem> = PanelGroup::ALL
        .into_iter()
        .flat_map(|group| group.panels(&roster))
        .map(QuickBarItem::Panel)
        .collect();

    rsx! {
        Popover {
            label,
            title: "Display".to_string(),
            modifier: "popover--more".to_string(),
            div { class: "quickbar-menu", role: "menu",
                div { class: "quickbar-menu-head",
                    span { "Panels" }
                    span { class: "quickbar-menu-hint", "pin to the quickbar" }
                }
                for (index, item) in panels.iter().copied().enumerate() {
                    RosterRow {
                        key: "{item.slug()}",
                        item,
                        opens_cluster: index > 0 && model::seam_before(panels[index - 1], panels[index]),
                        pins,
                        layout,
                        database: None,
                    }
                }

                DashboardEntry {}
                ResetLayoutEntry { reset_confirm_open }
                ReorderEntry { reorder_open }
            }
        }
    }
}

/// The Display menu's Dashboard row: the way into the organizer, moved here from the Tools
/// roster 2026-09-15.
///
/// **It was in the wrong menu, by the rule Tools already states.** [`ControlsAction`]'s doc
/// says every launcher there answers a question about the numbers on screen, and names Controls
/// as the one exception. `ToolId::ConfigureStats` was a second exception nobody named: it does
/// not answer a question about the numbers, it decides which numbers are on the grid at all.
/// That is this menu's subject — it already holds the panel checklist and the two arrangement
/// acts, and "which stats exist" is the same question one level in.
///
/// **Labelled for the room rather than the old door.** The modal is titled "Dashboard"; the
/// launcher said "Configure Stats", which is the vocabulary from when it was a stat-SELECTION
/// modal (see [`crate::panels::stats_config`]'s module doc). The modal was renamed when a panel
/// became a container the user fills; the launcher was not, so someone hunting for the word the
/// modal calls itself found nothing.
///
/// **This is the door that survives an empty roster**, which is why the placement matters more
/// than the label. The three other ways in — a dashboard's gear, `StatGroup`'s "Empty — add
/// stats" prompt, and a popped-out window's gear ([`crate::panel_popout`]) — all require a
/// dashboard to EXIST, so deleting the last one closes all three in the same instant. Opening
/// at `OrganizerTarget::All` is what lets this one keep working with nothing to deep-link to;
/// that target was added for exactly this case, and this row is the other half of it.
///
/// Split out like [`ResetLayoutEntry`] and [`ReorderEntry`], for their reason: raising a modal
/// closes the menu on the way in, and that needs [`PopoverOpen`], which only exists around the
/// Popover's children.
#[component]
fn DashboardEntry() -> Element {
    let mut menu_open = use_context::<PopoverOpen>().0;
    let mut open = use_context::<panels::stats_config::StatsConfigOpen>().0;

    rsx! {
        div { class: "quickbar-menu-row opens-cluster",
            button {
                class: "quickbar-action",
                r#type: "button",
                title: "Choose which stats each dashboard panel shows",
                onclick: move |_| {
                    menu_open.set(false);
                    open.set(Some(panels::stats_config::OrganizerTarget::All));
                },
                "Dashboard"
            }
        }
    }
}

/// The Display menu's Reset layout row, split out for the same reason [`ReorderEntry`] is:
/// raising the confirm closes the menu on the way in, and that needs [`PopoverOpen`], which
/// only exists around the Popover's children.
#[component]
fn ResetLayoutEntry(mut reset_confirm_open: Signal<bool>) -> Element {
    let mut menu_open = use_context::<PopoverOpen>().0;

    rsx! {
        div { class: "quickbar-menu-row",
            button {
                class: "quickbar-action",
                r#type: "button",
                title: "Restore the default panel arrangement",
                onclick: move |_| {
                    menu_open.set(false);
                    reset_confirm_open.set(true);
                },
                "Reset layout"
            }
        }
    }
}

/// The confirm behind the Display menu's Reset layout row, mounted at the shell root like
/// every other overlay: a `fixed` backdrop is contained by the grid's `transform`ed
/// surfaces, so it cannot render inside the menu (see [`crate::modal`]).
///
/// The reset is the one destructive act in the app with neither a seat wide enough to state
/// its consequence in nor a Ctrl+Z behind it: the layout is persisted, not committed through
/// the build's undo, so a mistaken click is gone for good.
#[component]
pub fn ResetLayoutConfirm(
    mut open: Signal<bool>,
    layout: Signal<Vec<GridItem>>,
    mobile_order: Signal<MobileOrder>,
) -> Element {
    let grid_columns = use_context::<crate::grid::view::GridColumns>().0;
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let roster = crate::panels::dashboards::surfaces(&dashboards.read());
    if !open() {
        return rsx! {};
    }
    rsx! {
        crate::confirm::ConfirmModal {
            title: "Reset layout".to_string(),
            message: "Restore the default panel arrangement on both the desktop grid and the mobile stack? It is saved over your current arrangement, and the layout has no undo.".to_string(),
            confirm_label: "Reset".to_string(),
            on_confirm: {
                let roster = roster.clone();
                move |_| {
                    open.set(false);
                    reset_arrangement(layout, mobile_order, grid_columns, roster.clone())
                }
            },
            on_cancel: move |_| open.set(false),
        }
    }
}

/// The Display menu's Reorder row, split out because closing the menu on the way in needs
/// [`PopoverOpen`], which only exists around the Popover's children — a hook in
/// `DisplayMenu`'s own body would panic at mount, before the provider is up.
#[component]
fn ReorderEntry(mut reorder_open: Signal<bool>) -> Element {
    let mut menu_open = use_context::<PopoverOpen>().0;

    rsx! {
        div { class: "quickbar-menu-row",
            button {
                class: "quickbar-action",
                r#type: "button",
                title: "Reorder the stack below 900px",
                onclick: move |_| {
                    menu_open.set(false);
                    reorder_open.set(true);
                },
                "Reorder"
            }
        }
    }
}

/// Restore both layouts to their defaults, through the same hand the loose Reset button used
/// to reach (moved to the Display menu in HM5, behind a confirm since the layout is
/// irreversible): window-fitted desktop defaults and the default mobile order, persisted so
/// the reset survives a reload the way a drag does.
fn reset_arrangement(
    mut layout: Signal<Vec<GridItem>>,
    mut order: Signal<MobileOrder>,
    mut grid_columns: Signal<u32>,
    roster: Vec<PanelKind>,
) {
    dioxus::prelude::spawn(async move {
        let space = crate::layout_store::measure_grid_space().await;
        let config = match space {
            Some(space) => crate::grid::model::GridConfig::for_width(space.width),
            None => crate::grid::model::GridConfig::default(),
        };
        let mut items = match space {
            Some(space) => crate::grid::model::GridItem::default_layout_for(
                config.columns,
                crate::grid::model::column_rows_for(space.height, &config),
                &roster,
            ),
            None => crate::grid::model::GridItem::default_layout_for(
                crate::grid::model::GridConfig::default().columns,
                crate::grid::model::DEFAULT_COLUMN_ROWS,
                &roster,
            ),
        };
        // A reset rebuilds the authored default, which has no memory of a surface being out in
        // its own window — without this it re-admits one whose window still has it, and the
        // dashboard is drawn twice.
        use_context::<crate::panel_popout::PoppedOut>().reapply(&mut items);
        let stack = MobileOrder::default_order(&roster);
        grid_columns.set(config.columns);
        crate::layout_store::persist_desktop(&items, config.columns);
        crate::layout_store::persist_mobile(&stack);
        crate::layout_store::mark_layout_authored();
        layout.set(items);
        order.set(stack);
    });
}

/// One roster row: the item's own control, and the pin toggle beside it.
///
/// What to call a quickbar item.
///
/// A hook rather than a plain fn — it reads the roster out of context — so it must be called
/// unconditionally at the top of a component, like any other. A tool answers from its own enum;
/// a dashboard panel answers with the name the user typed, which [`QuickBarItem::title`] cannot
/// know because its module has no Dioxus in it.
fn use_item_title(item: QuickBarItem) -> String {
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    match item {
        QuickBarItem::Panel(panel) => {
            crate::panels::dashboards::title_of(panel, &dashboards.read())
        }
        QuickBarItem::Tool(tool) => tool.title().to_string(),
    }
}

/// Whether using the control closes the menu is decided by the kind, and the two answers are
/// both right. Launching a tool puts a modal over this popover, so leaving it open would
/// stack an unreachable menu behind the thing you just opened. Docking a panel changes the
/// grid *behind* the menu, where the result is visible immediately and the next thing you
/// want is usually another panel — so that one stays open and the popover's own ✕, Escape or
/// an outside click ends it.
#[component]
fn RosterRow(
    item: QuickBarItem,
    /// Whether this row starts a new run of the roster — a different kind, or a different
    /// cluster of surfaces. It draws its own rule rather than one being placed before it,
    /// because a keyed row has to be the first node in the loop's block.
    opens_cluster: bool,
    pins: Signal<Vec<QuickBarItem>>,
    layout: Signal<Vec<GridItem>>,
    database: Option<crate::shell::Db>,
) -> Element {
    let mut open = use_context::<PopoverOpen>().0;
    let title = use_item_title(item);
    let pinned = pins.read().contains(&item);
    let closes = item.kind() == QuickbarItemKind::Tool;

    rsx! {
        div {
            class: if opens_cluster { "quickbar-menu-row opens-cluster" } else { "quickbar-menu-row" },
            // The click is caught on the way up from whichever control the arm below rendered,
            // so the close rule lives in one place rather than in eleven launchers that are
            // also used on the row, where nothing should close.
            div {
                class: "quickbar-menu-control",
                onclick: move |_| {
                    if closes {
                        open.set(false);
                    }
                },
                ItemControl { item, layout, database }
            }
            button {
                class: if pinned { "quickbar-menu-pin is-pinned" } else { "quickbar-menu-pin" },
                r#type: "button",
                "aria-pressed": pinned,
                "aria-label": if pinned { "Unpin {title}" } else { "Pin {title}" },
                title: if pinned { "Unpin {title}" } else { "Pin {title} to the quickbar" },
                onclick: move |_| set_pinned(pins, item, !pinned),
                {marks::pin(pinned)}
            }
        }
    }
}

/// One surface's pill. Reads its own state off the layout rather than taking it as a prop, so
/// a hide committed anywhere — this row, a panel's ✕, the visibility modal — lands here with no
/// second copy of the truth to keep in sync.
///
/// On is a plain word with no chrome at all, off is dashed and faint. That way round because
/// most surfaces are on at once, and Arclight — the system's "this control is doing something"
/// — spent on every docked pill at rest says nothing about any of them. Dashed already means
/// *absent* everywhere else in the app (an empty enhancement slot, an empty incarnate socket),
/// so an off pill reads as a gap in the row, which is exactly what a hidden panel is.
///
/// The dashed edge is drawn at the seam colour rather than a live one because "hidden" must
/// not outrank every surface that isn't.
///
/// This is the whole of what a pin's *fill* means, and why the kinds needed no badge to tell
/// them apart: a docked panel is a control with state behind it, a tool launcher is not.
#[component]
fn PanelPill(panel: PanelKind, layout: Signal<Vec<GridItem>>) -> Element {
    let columns = use_context::<crate::grid::view::GridColumns>().0();
    let dashboards = use_context::<crate::panels::dashboards::DashboardConfig>().0;
    let title = crate::panels::dashboards::title_of(panel, &dashboards.read());
    let hidden = layout
        .read()
        .iter()
        .any(|item| item.panel == panel && item.hidden);
    // Tracked, so the pill restates itself the moment the surface leaves or returns.
    let popped = use_context::<crate::panel_popout::PoppedOut>()
        .0
        .read()
        .contains(&panel);

    rsx! {
        button {
            class: if popped {
                "quickbar-pill is-popped"
            } else if hidden {
                "quickbar-pill is-off"
            } else {
                "quickbar-pill"
            },
            r#type: "button",
            // Inert while the surface is out in its own window, for the reason the visibility
            // modal's row states: the grid is not where it is, so offering it back would draw
            // it twice.
            disabled: popped,
            "aria-pressed": !hidden,
            title: if popped {
                "{title} is out in its own window"
            } else if hidden {
                "Put {title} back on the grid"
            } else {
                "Take {title} off the grid"
            },
            onclick: move |_| set_panel_hidden(layout, columns, panel, !hidden),
            "{title}"
        }
    }
}

/// Opens the accolade picker. The Powers panel's accolade strip is the readout — which
/// accolades this build claims — and this is the way in from anywhere else, which is the
/// division the beta's quickbar drew too.
#[component]
fn AccoladesAction() -> Element {
    let mut open = use_context::<panels::accolade_picker::AccoladePickerOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Permanent Accolades bonuses",
            onclick: move |_| open.set(true),
            {ToolId::Accolades.mark()}
            span { "{ToolId::Accolades.title()}" }
        }
    }
}

/// Opens the set-bonus finder. It is the one tool here that runs the other way: the detailed
/// sheet and the proc controls both start from a build, while this starts from the effect a
/// build is being planned around and finds the sets that grant it.
#[component]
fn SetBonusFinderAction() -> Element {
    let mut open = use_context::<panels::set_bonus_finder::SetBonusFinderOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Look up which IO sets grant a particular stat",
            onclick: move |_| open.set(true),
            {ToolId::SetBonusFinder.mark()}
            span { "{ToolId::SetBonusFinder.title()}" }
        }
    }
}

/// Opens the detailed stat sheet — every stat in the vocabulary with the sources behind it.
/// It answers the question the dashboard raises: the panels say what a number is, this says
/// what is making it.
#[component]
fn DetailedTotalsAction() -> Element {
    let mut open = use_context::<panels::detailed_totals::DetailedTotalsOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Every stat with the sources behind it",
            onclick: move |_| open.set(true),
            {ToolId::DetailedTotals.mark()}
            span { "{ToolId::DetailedTotals.title()}" }
        }
    }
}

/// Opens the per-category proc controls. Beside the detailed sheet because it answers the same
/// question from the other end: the sheet says which procs are making a number, this decides
/// which of them are allowed to.
///
/// The trigger carries the disabled count rather than a bare label: a category switched off
/// weeks ago goes on quietly withholding a contribution, and the number is the only standing
/// evidence of it.
#[component]
fn ProcSourcesAction() -> Element {
    let mut open = use_context::<panels::proc_settings::ProcSettingsOpen>().0;
    let session = use_context::<BuildSession>();
    let disabled = session.build.read().disabled_proc_categories.len();

    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Which proc categories contribute to totals",
            onclick: move |_| open.set(true),
            {ToolId::ProcSources.mark()}
            if disabled == 0 {
                span { "{ToolId::ProcSources.title()}" }
            } else {
                span { "{ToolId::ProcSources.title()} · {disabled} off" }
            }
        }
    }
}

/// Opens the enhancement list. Every other tool here reads the build as a set of numbers; this
/// one reads it as a set of things to go and acquire, which is the last question asked of a
/// finished build and the first one asked of a half-finished one.
#[component]
fn EnhancementListAction() -> Element {
    let mut open = use_context::<crate::enhancement_list::EnhancementListOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Every enhancement in this build, with catalysts and boosters",
            onclick: move |_| open.set(true),
            {ToolId::EnhancementList.mark()}
            span { "{ToolId::EnhancementList.title()}" }
        }
    }
}

/// Opens the enhancement tools. The only tool here that WRITES the build — every other one
/// reads it — so it sits directly after the list, which is the surface that shows what the
/// writing costs.
///
/// It carries the count of under-levelled pieces for the reason `ProcSourcesAction` carries its
/// own: a build finished at 47 reads as finished. Every number on the dashboard is a little low
/// and nothing anywhere says why, which is a state a build can sit in for months — it took a
/// diff against the live game to find one. The slot pips mark the pieces; this marks the build,
/// on the button that fixes it.
#[component]
fn EnhancementToolsAction(database: Option<crate::shell::Db>) -> Element {
    let mut open = use_context::<crate::enhancement_tools::EnhancementToolsOpen>().0;
    let session = use_context::<BuildSession>();
    // No dataset yet means no set bands to measure against, so the count is absent rather than
    // zero — "nothing to raise" and "cannot tell" are different things to say.
    let under = database
        .as_ref()
        .map(|db| crate::enhancement_tools::under_level_count(&session.build.read(), db));

    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Set craft level, attunement and boosters across the whole build",
            onclick: move |_| open.set(true),
            {ToolId::EnhancementTools.mark()}
            match under {
                Some(n) if n > 0 => rsx! { span { "{ToolId::EnhancementTools.title()} · {n} low" } },
                _ => rsx! { span { "{ToolId::EnhancementTools.title()}" } },
            }
        }
    }
}

/// Opens the powerset comparison. The only tool here that reads a build the user does not
/// have: every other one answers "what is this build doing", and this one answers "what would
/// a different one".
#[component]
fn PowersetCompareAction() -> Element {
    let mut open = use_context::<crate::powerset_compare::PowersetCompareOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Compare two sets' powers side by side, paired by what they do",
            onclick: move |_| open.set(true),
            {ToolId::PowersetCompare.mark()}
            span { "{ToolId::PowersetCompare.title()}" }
        }
    }
}

/// Opens Compare Slotting. Beside the powerset comparison because they ask the same kind of
/// question at two scales: that one reads sets the build doesn't own, this one reads slottings
/// one of its powers could carry. Reopens on whatever power it last compared; the modal's own
/// selector picks the first one.
#[component]
fn CompareSlottingAction() -> Element {
    let mut store = use_context::<crate::compare_slotting::CompareSlottingStore>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Compare alternative enhancement configurations for one power, side by side",
            onclick: move |_| store.write().open(),
            {ToolId::CompareSlotting.mark()}
            span { "{ToolId::CompareSlotting.title()}" }
        }
    }
}

/// Opens the Controls modal — the interaction reference, the one tool here that reads the
/// app rather than the build: every other launcher answers a question about the numbers on
/// screen, this one answers "how do I do that".
#[component]
fn ControlsAction() -> Element {
    let mut open = use_context::<crate::controls::ControlsOpen>().0;
    rsx! {
        button {
            class: "quickbar-action",
            r#type: "button",
            title: "Keyboard, mouse and touch control reference",
            onclick: move |_| open.set(true),
            {ToolId::Controls.mark()}
            span { "{ToolId::Controls.title()}" }
        }
    }
}
