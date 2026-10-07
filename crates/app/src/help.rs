//! The Help modal — what each surface of this planner is for (the beta's `HelpModal`).
//!
//! Written against this tree the way [`crate::controls`] is: every topic names a surface that
//! exists in `crates/app`, and [`Surface::ALL`] is the roster it has to cover. The beta's 54
//! topics are not the starting point, and measuring them is why — against this tree roughly a
//! third of its surfaces go unmentioned (the attack chain, the what-if layer, the quickbar, PNG
//! export) and a fifth of its topics describe surfaces this tree deliberately replaced.
//!
//! **Staleness rather than content is this surface's defect**, which is what the roster is for.
//! A help topic is a comment with a bigger audience,
//! and the two things that keep one honest are tests here rather than anyone's memory:
//!
//! - [`every_surface_symbol_resolves`](tests::every_surface_symbol_resolves) reads this crate's
//!   own source and fails when a symbol a surface names is no longer in it. Renaming a module or
//!   a host component therefore reds a help test — the rename half of §7's sweep, with a machine
//!   doing the grepping.
//! - [`help_covers_every_surface`](tests::help_covers_every_surface) fails when a surface has no
//!   topic. That is the additive half, which no grep can see: a new panel renames nothing.
//!   [`Surface::ALL`] is anchored to [`ToolId`] and [`PanelKind`] by exhaustive matches, so a new
//!   tool or fixed panel is a compile error in this file before it is a missing topic.
//!
//! Both are tests, and neither breaks the build: help landing in the same commit as the feature
//! is the goal, not a gate.
//!
//! **The Controls sheet is reached, not restated.** Search runs over
//! [`crate::controls::reference_rows`] as well as over the topics, and a match there is drawn as
//! a way INTO the Controls modal. One source, two readers — a topic that re-stated a controls row
//! would be the second copy that drifts, and `help_reaches_the_controls_sheet_without_restating_it`
//! is what forbids it.

use crate::grid::model::PanelKind;
use crate::modal::{Modal, ModalSize};
use crate::quickbar::model::ToolId;
use dioxus::prelude::*;

/// The modal's open state, held at the shell root and provided as context. A bare flag: the
/// footer's Help action and the main menu's Help row both want the same whole guide.
#[derive(Clone, Copy)]
pub struct HelpOpen(pub Signal<bool>);

/// The shelf a topic sits on. Eight, and every one of them is a division of THIS tree — the
/// beta's nine included an Incarnates shelf with six topics and a Dashboard & Stats shelf with
/// ten, split that way because its stat panels were a fixed taxonomy. They are not any more.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Start,
    Powers,
    Slotting,
    Stats,
    Tools,
    Layout,
    Files,
    Settings,
}

impl Category {
    /// Every shelf, in reading order. The exhaustive `match` fails to compile when a variant is
    /// added, the guard [`ToolId::ALL`] carries and for the same reason: a shelf missing here is
    /// a shelf whose topics have no tab.
    pub const ALL: [Category; 8] = {
        match Category::Start {
            Category::Start
            | Category::Powers
            | Category::Slotting
            | Category::Stats
            | Category::Tools
            | Category::Layout
            | Category::Files
            | Category::Settings => {}
        }
        [
            Category::Start,
            Category::Powers,
            Category::Slotting,
            Category::Stats,
            Category::Tools,
            Category::Layout,
            Category::Files,
            Category::Settings,
        ]
    };

    pub fn label(self) -> &'static str {
        match self {
            Category::Start => "Getting started",
            Category::Powers => "Powers",
            Category::Slotting => "Slotting",
            Category::Stats => "Stats",
            Category::Tools => "Tools",
            Category::Layout => "Layout",
            Category::Files => "Files",
            Category::Settings => "Settings",
        }
    }

    /// The CSS suffix picking the shelf's hue, the same way `controls__title--{accent}` does.
    fn accent(self) -> &'static str {
        match self {
            Category::Start => "start",
            Category::Powers => "powers",
            Category::Slotting => "slotting",
            Category::Stats => "stats",
            Category::Tools => "tools",
            Category::Layout => "layout",
            Category::Files => "files",
            Category::Settings => "settings",
        }
    }
}

/// A thing in this app a user can be looking at, and the roster help has to cover.
///
/// **The variant is not the point; [`Surface::symbol`] is.** Each one names a module and an item
/// inside it that this crate actually has, and the test that resolves those is what puts help in
/// the path of a rename. A surface whose symbol stops resolving is a surface whose topics are
/// now describing something that moved.
///
/// Not every surface is pinnable and not every one is a panel, so this is a roster of its own
/// rather than a projection of [`ToolId`] or [`PanelKind`] — but it is anchored to both by
/// [`surface_of_tool`] and [`surface_of_panel`], whose exhaustive matches make a new tool or a
/// new fixed panel a compile error here.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Surface {
    // Getting started
    Identity,
    PowersetSelect,
    Dataset,
    LevelControl,
    LevelUp,
    // Powers
    AvailablePanel,
    PowersPanel,
    InfoPanel,
    PoolsPanel,
    PoolPicker,
    IncarnatePicker,
    IncarnateCrafting,
    Accolades,
    Inherents,
    PowersByLevel,
    PinnedPowers,
    // Slotting
    EnhancementPicker,
    SlotTooltip,
    SetBonusTotals,
    SetBonusFinder,
    EnhancementList,
    EnhancementTools,
    CompareSlotting,
    ProcSources,
    // Stats
    Dashboards,
    StatsConfig,
    DetailedTotals,
    BuildSettings,
    // Tools
    WhatIf,
    AttackChain,
    PowersetCompare,
    // Layout
    Grid,
    Quickbar,
    PanelVisibility,
    MobileStack,
    ReorderMenu,
    Controls,
    // Files
    BuildFile,
    BuildHandoff,
    PasteBuild,
    ForumExport,
    ExportImage,
    // Settings
    MainMenu,
    Theme,
    UiScale,
    RuleOfFive,
    History,
    AppInfo,
}

impl Surface {
    /// Every surface. The exhaustive `match` fails to compile when a variant is added, so the
    /// list cannot silently fall behind the enum — and `help_covers_every_surface` reads this
    /// array, so a surface added here with no topic reds rather than disappearing.
    #[allow(dead_code)] // read by the coverage and symbol tests; that is the whole job.
    pub const ALL: [Surface; 48] = {
        match Surface::Identity {
            Surface::Identity
            | Surface::PowersetSelect
            | Surface::Dataset
            | Surface::LevelControl
            | Surface::LevelUp
            | Surface::AvailablePanel
            | Surface::PowersPanel
            | Surface::InfoPanel
            | Surface::PoolsPanel
            | Surface::PoolPicker
            | Surface::IncarnatePicker
            | Surface::IncarnateCrafting
            | Surface::Accolades
            | Surface::Inherents
            | Surface::PowersByLevel
            | Surface::PinnedPowers
            | Surface::EnhancementPicker
            | Surface::SlotTooltip
            | Surface::SetBonusTotals
            | Surface::SetBonusFinder
            | Surface::EnhancementList
            | Surface::EnhancementTools
            | Surface::CompareSlotting
            | Surface::ProcSources
            | Surface::Dashboards
            | Surface::StatsConfig
            | Surface::DetailedTotals
            | Surface::BuildSettings
            | Surface::WhatIf
            | Surface::AttackChain
            | Surface::PowersetCompare
            | Surface::Grid
            | Surface::Quickbar
            | Surface::PanelVisibility
            | Surface::MobileStack
            | Surface::ReorderMenu
            | Surface::Controls
            | Surface::BuildFile
            | Surface::BuildHandoff
            | Surface::PasteBuild
            | Surface::ForumExport
            | Surface::ExportImage
            | Surface::MainMenu
            | Surface::Theme
            | Surface::UiScale
            | Surface::RuleOfFive
            | Surface::History
            | Surface::AppInfo => {}
        }
        [
            Surface::Identity,
            Surface::PowersetSelect,
            Surface::Dataset,
            Surface::LevelControl,
            Surface::LevelUp,
            Surface::AvailablePanel,
            Surface::PowersPanel,
            Surface::InfoPanel,
            Surface::PoolsPanel,
            Surface::PoolPicker,
            Surface::IncarnatePicker,
            Surface::IncarnateCrafting,
            Surface::Accolades,
            Surface::Inherents,
            Surface::PowersByLevel,
            Surface::PinnedPowers,
            Surface::EnhancementPicker,
            Surface::SlotTooltip,
            Surface::SetBonusTotals,
            Surface::SetBonusFinder,
            Surface::EnhancementList,
            Surface::EnhancementTools,
            Surface::CompareSlotting,
            Surface::ProcSources,
            Surface::Dashboards,
            Surface::StatsConfig,
            Surface::DetailedTotals,
            Surface::BuildSettings,
            Surface::WhatIf,
            Surface::AttackChain,
            Surface::PowersetCompare,
            Surface::Grid,
            Surface::Quickbar,
            Surface::PanelVisibility,
            Surface::MobileStack,
            Surface::ReorderMenu,
            Surface::Controls,
            Surface::BuildFile,
            Surface::BuildHandoff,
            Surface::PasteBuild,
            Surface::ForumExport,
            Surface::ExportImage,
            Surface::MainMenu,
            Surface::Theme,
            Surface::UiScale,
            Surface::RuleOfFive,
            Surface::History,
            Surface::AppInfo,
        ]
    };

    /// `module::path::Item` — a real item in a real file of this crate.
    ///
    /// Read only by `every_surface_symbol_resolves`, which is the whole job: the string exists to
    /// be resolved against the source tree, so that renaming `panels::what_if` or its host
    /// component fails a test in the file describing it. Nothing renders it; a Rust path is not
    /// something to show a user.
    #[allow(dead_code)]
    pub fn symbol(self) -> &'static str {
        match self {
            Surface::Identity => "panels::identity::IdentityPanel",
            Surface::PowersetSelect => "panels::powerset_select::PowersetSelect",
            Surface::Dataset => "data_source::load",
            Surface::LevelControl => "level_control::LevelControl",
            Surface::LevelUp => "level_control::LevelUpControl",
            Surface::AvailablePanel => "panels::powers::AvailablePanel",
            Surface::PowersPanel => "panels::powers::PowersPanel",
            Surface::InfoPanel => "panels::info::Info",
            Surface::PoolsPanel => "panels::powers::PoolsPanel",
            Surface::PoolPicker => "panels::pool_picker::PoolPickerHost",
            Surface::IncarnatePicker => "panels::incarnate_picker::IncarnatePickerHost",
            Surface::IncarnateCrafting => "panels::incarnate_crafting::IncarnateCraftingHost",
            Surface::Accolades => "panels::accolade_picker::AccoladePickerHost",
            Surface::Inherents => "panels::inherents_section::InherentsSection",
            Surface::PowersByLevel => "panels::powers_by_level::PowersByLevel",
            Surface::PinnedPowers => "pinned_powers::PinnedPowersStrip",
            Surface::EnhancementPicker => "panels::powers::EnhancementPickerHost",
            Surface::SlotTooltip => "panels::powers::SlotTooltipHost",
            Surface::SetBonusTotals => "panels::set_bonus_totals::SetBonusTotals",
            Surface::SetBonusFinder => "panels::set_bonus_finder::SetBonusFinderHost",
            Surface::EnhancementList => "enhancement_list::EnhancementListHost",
            Surface::EnhancementTools => "enhancement_tools::EnhancementToolsHost",
            Surface::CompareSlotting => "compare_slotting::CompareSlottingHost",
            Surface::ProcSources => "panels::proc_settings::ProcSettingsHost",
            Surface::Dashboards => "panels::dashboards::Dashboards",
            Surface::StatsConfig => "panels::stats_config::StatsConfigHost",
            Surface::DetailedTotals => "panels::detailed_totals::DetailedTotalsHost",
            Surface::BuildSettings => "panels::combat::Combat",
            Surface::WhatIf => "panels::what_if::WhatIfHost",
            Surface::AttackChain => "panels::attack_chain::AttackChainHost",
            Surface::PowersetCompare => "powerset_compare::PowersetCompareHost",
            Surface::Grid => "grid::view::GridContainer",
            Surface::Quickbar => "quickbar::Quickbar",
            Surface::PanelVisibility => "panel_visibility::PanelVisibilityHost",
            Surface::MobileStack => "mobile_order::MobileOrder",
            Surface::ReorderMenu => "reorder_menu::ReorderMenu",
            Surface::Controls => "controls::ControlsHost",
            Surface::BuildFile => "build_io::BuildFileEntries",
            Surface::BuildHandoff => "build_io::BuildHandoffEntries",
            Surface::PasteBuild => "build_io::PasteBuildHost",
            Surface::ForumExport => "forum_export::ForumExportHost",
            Surface::ExportImage => "export_image::ExportImageHost",
            Surface::MainMenu => "main_menu::FileMenu",
            Surface::Theme => "theme::apply_theme",
            Surface::UiScale => "ui_scale::MAX_PCT",
            Surface::RuleOfFive => "shell::RuleOfFiveAlert",
            Surface::History => "history::History",
            Surface::AppInfo => "app_info::ChangelogHost",
        }
    }
}

/// The surface behind a pinnable tool. Exhaustive, so a twelfth [`ToolId`] cannot be added
/// without deciding which surface of the roster it is — and therefore without
/// `help_covers_every_surface` asking for a topic about it.
///
/// [`Surface::StatsConfig`] left this map when the organizer was retired from the roster and
/// moved to the Display menu (`quickbar::DashboardEntry`). Its help topic is untouched: the
/// coverage check walks `Surface`, not this map, so a surface reachable by something other than
/// a pinnable tool is covered exactly as before.
#[allow(dead_code)] // an anchor, not a call site: its exhaustiveness is what the compiler checks.
fn surface_of_tool(tool: ToolId) -> Surface {
    match tool {
        ToolId::Accolades => Surface::Accolades,
        ToolId::SetBonusFinder => Surface::SetBonusFinder,
        ToolId::DetailedTotals => Surface::DetailedTotals,
        ToolId::AttackChain => Surface::AttackChain,
        ToolId::WhatIf => Surface::WhatIf,
        ToolId::ProcSources => Surface::ProcSources,
        ToolId::EnhancementList => Surface::EnhancementList,
        ToolId::EnhancementTools => Surface::EnhancementTools,
        ToolId::PowersetCompare => Surface::PowersetCompare,
        ToolId::CompareSlotting => Surface::CompareSlotting,
        ToolId::Controls => Surface::Controls,
    }
}

/// The surface behind a grid panel, on the same terms. Every dashboard panel answers
/// [`Surface::Dashboards`], because what a dashboard CONTAINS is the user's and a topic cannot
/// be written about a panel nobody has built yet — which is the whole content change this
/// category went through (see [`crate::panels::dashboards`]).
#[allow(dead_code)] // likewise.
fn surface_of_panel(panel: PanelKind) -> Surface {
    match panel {
        PanelKind::Powers => Surface::PowersPanel,
        PanelKind::Available => Surface::AvailablePanel,
        PanelKind::Pools => Surface::PoolsPanel,
        PanelKind::SetBonuses => Surface::SetBonusTotals,
        PanelKind::Info => Surface::InfoPanel,
        PanelKind::Dashboard(_) => Surface::Dashboards,
    }
}

/// One entry in the guide.
///
/// `body` reaches the user verbatim, so it is written as sentences rather than as a label.
/// `keywords` are the words somebody would type that the body does not contain — the game's
/// vocabulary, the beta's names for things, and the wrong-but-common word.
pub struct Topic {
    /// Stable across rewrites: it is the deep-link handle and the key the list renders by.
    pub id: &'static str,
    pub title: &'static str,
    pub category: Category,
    /// The surface this describes. The whole staleness mechanism hangs off this field, and
    /// nothing renders it — a topic that had to SAY which surface it was about would be a topic
    /// nobody could write plainly.
    #[allow(dead_code)]
    pub surface: Surface,
    pub body: &'static str,
    pub keywords: &'static [&'static str],
}

/// The guide, in reading order.
pub const TOPICS: &[Topic] = &[
    // ---- Getting started ----------------------------------------------------------------
    Topic {
        id: "start-archetype",
        title: "Pick an archetype",
        category: Category::Start,
        surface: Surface::Identity,
        body: "The identity panel is where a build starts: the archetype decides which powersets \
               you can take, which scaling tables every number is read off, and where your caps \
               sit. Changing it clears every selection that depended on it, so it is the first \
               choice rather than a late one.",
        keywords: &[
            "AT",
            "class",
            "archetype",
            "scrapper",
            "blaster",
            "tanker",
            "new build",
        ],
    },
    Topic {
        id: "start-powersets",
        title: "Choose a primary and secondary",
        category: Category::Start,
        surface: Surface::PowersetSelect,
        body: "Each archetype offers its own primary and secondary lists, and the two dropdowns \
               only ever show sets that archetype can actually take. Some sets branch part-way \
               up, and the branch is offered where it unlocks rather than at the top.",
        keywords: &[
            "primary",
            "secondary",
            "powerset",
            "set",
            "branch",
            "choose",
        ],
    },
    Topic {
        id: "start-dataset",
        title: "Which game the build is for",
        category: Category::Start,
        surface: Surface::Dataset,
        body: "Sidekick reads each server's own exported data, and the forks disagree: a power \
               can have different numbers, or not exist, depending on which one you are planning \
               for. The dataset badge in the identity panel says which is loaded, and switching \
               it reloads every number in the app from that fork's export.",
        keywords: &[
            "homecoming",
            "thunderspy",
            "rebirth",
            "server",
            "fork",
            "dataset",
            "shard",
        ],
    },
    Topic {
        id: "start-level",
        title: "The build level",
        category: Category::Start,
        surface: Surface::LevelControl,
        body: "The level in the header is what the whole build is projected at. It decides how \
               many power picks and slots you have, how far enhancements scale, and which \
               numbers the info panel and the dashboards show.",
        keywords: &["level", "50", "exemplar", "slider", "budget", "picks"],
    },
    Topic {
        id: "start-level-up",
        title: "Level-up mode",
        category: Category::Start,
        surface: Surface::LevelUp,
        body: "Off, you plan the finished build and can spend picks and slots in any order — the \
               respec view. On, the build is gated to the level on screen: anything you could not \
               have taken yet is unavailable, so you see the character the way you would actually \
               level it.",
        keywords: &[
            "level up",
            "levelup",
            "respec",
            "gated",
            "progression",
            "leveling",
        ],
    },
    // ---- Powers -------------------------------------------------------------------------
    Topic {
        id: "powers-available",
        title: "Picking powers",
        category: Category::Powers,
        surface: Surface::AvailablePanel,
        body: "The Available panel lists a powerset whole, in the order it unlocks — including \
               the powers you have already taken, which are marked with their pick level on a \
               filled badge rather than dropped from the list. Click a power to take it; click \
               one you already hold to give it back, which also returns its slots and \
               enhancements (one undo restores them). A power you cannot take yet says why — \
               the level it needs, or the prerequisite it is behind — rather than being left \
               out of the list.",
        keywords: &[
            "available",
            "pick",
            "add",
            "take",
            "remove",
            "drop",
            "unlock",
            "requires",
            "prerequisite",
        ],
    },
    Topic {
        id: "powers-loadout",
        title: "The loadout",
        category: Category::Powers,
        surface: Surface::PowersPanel,
        body: "The Powers panel is the build itself: every power you have taken, as a card with \
               its enhancement slots. Powers that buff you rather than a target carry an on/off \
               control, because a toggle that is off contributes nothing and the totals have to \
               agree with that.",
        keywords: &[
            "loadout", "picked", "cards", "toggle", "on", "off", "remove", "drop",
        ],
    },
    Topic {
        id: "powers-info",
        title: "Reading a power",
        category: Category::Powers,
        surface: Surface::InfoPanel,
        body: "The Info panel shows one power in full: what it does, at the build's level, with \
               the build's slotting and buffs already applied. It reads the same projection the \
               dashboards do, so a number here and a number there can never disagree.",
        keywords: &[
            "info",
            "tooltip",
            "hover",
            "details",
            "damage",
            "recharge",
            "endurance",
        ],
    },
    Topic {
        id: "powers-pools-panel",
        title: "The Power Pools panel",
        category: Category::Powers,
        surface: Surface::PoolsPanel,
        body: "Power Pools is its own panel: the two buttons that choose which pools the build \
               holds, over the same numbered rows the Available panel draws for your powersets. \
               It sits under Available by default and can be dragged, resized or closed like any \
               other panel — the pools grow by a whole list every time you take one, which is why \
               they get a rectangle you control rather than a share of somebody else's.",
        keywords: &[
            "pools panel",
            "power pools",
            "panel",
            "epic pool",
            "patron pool",
            "layout",
        ],
    },
    Topic {
        id: "powers-pools",
        title: "Pool and epic powers",
        category: Category::Powers,
        surface: Surface::PoolPicker,
        body: "Beyond the primary and secondary, a build can take from the shared power pools, \
               and from one epic or patron pool that unlocks late. The picker offers the pools \
               the build can still take and says what each costs you in picks.",
        keywords: &[
            "pool",
            "epic",
            "patron",
            "ancillary",
            "fitness",
            "speed",
            "leaping",
            "hasten",
        ],
    },
    Topic {
        id: "powers-incarnates",
        title: "Incarnate slots",
        category: Category::Powers,
        surface: Surface::IncarnatePicker,
        body: "At 50 the incarnate trees open, and each slot takes one ability from its own tree. \
               The picker lists what the dataset actually ships for that slot, so a fork with a \
               different tree offers a different list rather than a hardcoded one.",
        keywords: &[
            "incarnate",
            "alpha",
            "judgement",
            "interface",
            "lore",
            "destiny",
            "hybrid",
            "50",
        ],
    },
    Topic {
        id: "powers-incarnate-crafting",
        title: "What an incarnate costs to craft",
        category: Category::Powers,
        surface: Surface::IncarnateCrafting,
        body: "Every incarnate ability is the top of a craft tree, and the tiers beneath it have \
               to be built first. This is that tree for each of your picks, read from the game's \
               own recipes, with the salvage it all adds up to as one list you can tick off.",
        keywords: &[
            "craft",
            "recipe",
            "salvage",
            "shard",
            "thread",
            "tier",
            "very rare",
            "checklist",
        ],
    },
    Topic {
        id: "powers-accolades",
        title: "Accolades",
        category: Category::Powers,
        surface: Surface::Accolades,
        body: "Accolades are permanent rewards that buff the character, and a build planned \
               without them reads low. Switch on the ones you have and their effects fold into \
               the totals. Some only apply in a particular mode, and a row that does says so.",
        keywords: &[
            "accolade",
            "badge",
            "portal jockey",
            "atlas medallion",
            "reward",
            "permanent",
        ],
    },
    Topic {
        id: "powers-inherents",
        title: "Inherent powers",
        category: Category::Powers,
        surface: Surface::Inherents,
        body: "Every character is handed powers it never picks — the archetype's own inherent, \
               Brawl, Sprint, Health and Stamina. They sit in the Powers panel beside the picks, \
               marked apart because they spend none of your picks, and the ones the game lets you \
               slot take enhancements exactly like any other power.",
        keywords: &[
            "inherent", "brawl", "sprint", "rest", "prestige", "fitness", "stamina",
        ],
    },
    Topic {
        id: "powers-by-level",
        title: "The build in level order",
        category: Category::Powers,
        surface: Surface::PowersByLevel,
        body: "The Powers panel has a second arrangement: the same cards laid out over the levels \
               the game grants picks at, rather than grouped by the powerset they came from. It \
               is the view that answers whether a build is playable on the way up rather than \
               only at the end. The levels come from the dataset's own schedule.",
        keywords: &[
            "by level",
            "order",
            "plan",
            "levelling",
            "progression",
            "when",
        ],
    },
    Topic {
        id: "powers-pinned",
        title: "Tracking a power toward perma",
        category: Category::Powers,
        surface: Surface::PinnedPowers,
        body: "Track, on a power's card in the Info panel, puts it on a strip across the top with \
               how close its recharge is to covering its own duration. It is for the two or three \
               powers a build is really built around, so you can watch them while you slot \
               something else. Tracking is a view, not part of the build: it is not saved.",
        keywords: &[
            "perma", "pin", "pinned", "track", "hasten", "recharge", "uptime", "strip",
        ],
    },
    // ---- Slotting -----------------------------------------------------------------------
    Topic {
        id: "slot-picker",
        title: "Slotting an enhancement",
        category: Category::Slotting,
        surface: Surface::EnhancementPicker,
        body: "Opening a slot offers only what that power will actually accept, because the game \
               data says which categories each power allows. The tabs divide it into plain IOs, \
               IO sets, specials and origin enhancements, and the header's level, attunement and \
               booster settings are stamped onto whatever you pick next.",
        keywords: &[
            "enhancement",
            "IO",
            "SO",
            "DO",
            "set",
            "slot",
            "attuned",
            "booster",
            "catalyst",
        ],
    },
    Topic {
        id: "slot-budget",
        title: "Slots are a budget",
        category: Category::Slotting,
        surface: Surface::PowersPanel,
        body: "A build has a fixed number of enhancement slots and a fixed number of power picks, \
               and the counters say how many of each are left. Adding slots to a power and taking \
               them back is one of the most-repeated acts in planning, so it is a drag rather \
               than a click each.",
        keywords: &[
            "budget",
            "slots",
            "picks",
            "counter",
            "remaining",
            "67",
            "24",
        ],
    },
    Topic {
        id: "slot-tooltip",
        title: "What a slotted piece is doing",
        category: Category::Slotting,
        surface: Surface::SlotTooltip,
        body: "A filled slot will tell you what it is worth: the piece's level, which aspects of \
               the power it enhances and by how much after diminishing returns, any procs it \
               carries, and which tiers of its set the build has earned.",
        keywords: &[
            "tooltip",
            "hover",
            "aspect",
            "ED",
            "diminishing",
            "proc",
            "tier",
            "bonus",
        ],
    },
    Topic {
        id: "slot-set-bonuses",
        title: "What your sets are granting",
        category: Category::Slotting,
        surface: Surface::SetBonusTotals,
        body: "The Set Bonuses panel collects everything the build's IO sets grant — every set \
               tier and every always-on global on a unique piece (Luck of the Gambler's +Recharge, \
               Steadfast Protection's +Def) — one row per stat they feed, with the sources behind \
               each. It is the surface for the question you ask while spending slots rather than \
               while reading a number.",
        keywords: &[
            "set bonus",
            "bonuses",
            "IO set",
            "granted",
            "totals",
            "sources",
        ],
    },
    Topic {
        id: "slot-finder",
        title: "Finding a set that grants what you want",
        category: Category::Slotting,
        surface: Surface::SetBonusFinder,
        body: "The Set Bonus Finder runs the other way round from the planner: instead of asking \
               what a set gives you, it searches every bonus in the dataset by the effect it \
               grants. It is how you answer \"what would get me more recharge\" before anything \
               is slotted.",
        keywords: &[
            "finder",
            "lookup",
            "search",
            "recharge",
            "defense",
            "which set",
            "bonus",
        ],
    },
    Topic {
        id: "slot-shopping",
        title: "The build as a shopping list",
        category: Category::Slotting,
        surface: Surface::EnhancementList,
        body: "Every piece the build is asking for, grouped by what you would actually buy — two \
               copies at different levels or different boosts are different purchases and get \
               their own lines. The catalysts and boosters the slotting implies are counted \
               exactly, not estimated.",
        keywords: &[
            "shopping", "list", "buy", "acquire", "auction", "catalyst", "booster", "count",
        ],
    },
    Topic {
        id: "slot-tools",
        title: "One edit across every slot",
        category: Category::Slotting,
        surface: Surface::EnhancementTools,
        body: "Finishing a build is mechanical: lift everything to level 50, attune what can be \
               attuned, boost what can be boosted. Enhancement Tools does each of those axes \
               across the whole build in one act, and puts it all back in one undo. Each axis is \
               opt-in, so an axis you leave off is untouched.",
        keywords: &[
            "maximize", "bulk", "all", "lift", "attune", "boost", "level 50", "at once",
        ],
    },
    Topic {
        id: "slot-compare",
        title: "Comparing two slottings",
        category: Category::Slotting,
        surface: Surface::CompareSlotting,
        body: "Compare Slotting holds several slottings of one power side by side and shows what \
               each would do to the live build — not to an abstract version of the power. Pick \
               the row you want and it applies in one commit.",
        keywords: &[
            "compare",
            "slotting",
            "alternative",
            "side by side",
            "what if",
            "swap",
        ],
    },
    Topic {
        id: "slot-procs",
        title: "Which procs count",
        category: Category::Slotting,
        surface: Surface::ProcSources,
        body: "A proc only fires some of the time, so whether it belongs in a total is a \
               judgement rather than a fact. Proc Sources lists the categories the build's procs \
               actually spend and lets you switch any of them out of the totals. The setting \
               travels with the build, because it moves every number.",
        keywords: &[
            "proc",
            "chance",
            "PPM",
            "global",
            "damage proc",
            "exclude",
            "disable",
        ],
    },
    // ---- Stats --------------------------------------------------------------------------
    Topic {
        id: "stats-panel-is-a-container",
        title: "A dashboard panel is a container you fill",
        category: Category::Stats,
        surface: Surface::Dashboards,
        body: "There is no fixed set of stat panels. You make the panels, name them, and put the \
               stats you are steering by into them — so a build that lives on three numbers can \
               have one small panel holding exactly those three. A stat lives in one panel at a \
               time; moving it somewhere else moves it rather than copying it.",
        keywords: &[
            "dashboard",
            "panel",
            "stats",
            "group",
            "offense",
            "defense",
            "survival",
            "custom",
        ],
    },
    Topic {
        id: "stats-build-a-panel",
        title: "Building and filling a panel",
        category: Category::Stats,
        surface: Surface::StatsConfig,
        body: "Configure Stats is where panels are made, named, reordered and filled. The whole \
               stat vocabulary is below, grouped by family; drop a stat into a panel and it \
               appears there immediately, because the grid is right behind the modal and the \
               decision is worth making by looking.",
        keywords: &[
            "configure",
            "organize",
            "add stat",
            "rename",
            "reorder",
            "hide",
            "show",
            "reset",
        ],
    },
    Topic {
        id: "stats-detailed",
        title: "Why a number is what it is",
        category: Category::Stats,
        surface: Surface::DetailedTotals,
        body: "A dashboard says what your defense is; Totals says where every point of it came \
               from, grouped by source. A contribution that is real but did not apply — a set \
               bonus the Rule of 5 refused, a travel buff that lost its group — keeps its row and \
               is marked, because a build paying for something it is not getting should be told.",
        keywords: &[
            "totals",
            "detailed",
            "breakdown",
            "sources",
            "why",
            "ledger",
            "residual",
        ],
    },
    Topic {
        id: "stats-build-settings",
        title: "The conditions the numbers assume",
        category: Category::Stats,
        surface: Surface::BuildSettings,
        body: "Every number in the app is computed under some assumption: who you are fighting, \
               how many of them, whether you are exemplared, which optional mechanics are on. \
               Build Settings is where those live, and the header says when they have drifted \
               from the defaults so a surprising number has somewhere to be explained.",
        keywords: &[
            "combat",
            "settings",
            "target",
            "enemy",
            "exemplar",
            "arcanatime",
            "assumptions",
            "drift",
        ],
    },
    // ---- Tools --------------------------------------------------------------------------
    Topic {
        id: "tools-what-if",
        title: "Simulating a teammate's buffs",
        category: Category::Tools,
        surface: Surface::WhatIf,
        body: "Most builds are played on teams, and a build that looks short of a target solo may \
               not be. What-if injects the buffs someone else would be handing you and lets every \
               consequence fall out — the caps binding, a snipe going fast, the rotation getting \
               quicker — rather than adjusting one number in isolation.",
        keywords: &[
            "what if",
            "team",
            "buff",
            "kinetics",
            "speed boost",
            "simulate",
            "external",
        ],
    },
    Topic {
        id: "tools-chains",
        title: "Building an attack chain",
        category: Category::Tools,
        surface: Surface::AttackChain,
        body: "Chains lays a rotation out on a timeline: what fires when, where you are waiting \
               on recharge, what the endurance bar is doing, and what the whole loop is worth per \
               second. Every figure is the same projection the Info panel shows, so the rotation \
               cannot disagree with the powers in it. Chains save with the build.",
        keywords: &[
            "chain",
            "rotation",
            "attack chain",
            "DPS",
            "timeline",
            "recharge",
            "gap",
            "loop",
        ],
    },
    Topic {
        id: "tools-powerset-compare",
        title: "Comparing two powersets",
        category: Category::Tools,
        surface: Surface::PowersetCompare,
        body: "Two powersets side by side, powers paired by what they do, one metric at a time — \
               for the choice you make before there is a build to read. Both sides are projected \
               by the same engine the planner uses, unslotted, at the level and against the target \
               already on screen.",
        keywords: &[
            "compare",
            "powerset",
            "versus",
            "which set",
            "pick",
            "metric",
            "unslotted",
        ],
    },
    // ---- Layout -------------------------------------------------------------------------
    Topic {
        id: "layout-grid",
        title: "Arranging the planner",
        category: Category::Layout,
        surface: Surface::Grid,
        body: "On a desktop the panels sit on a free grid: drag a header to move a panel, the \
               corner to resize it, the fold control to collapse it to its title bar. The \
               arrangement is saved, and it is saved per column count, so a narrower window keeps \
               its own layout rather than wrecking the wide one.",
        keywords: &[
            "grid", "drag", "resize", "move", "fold", "collapse", "arrange", "layout", "reset",
        ],
    },
    Topic {
        id: "layout-quickbar",
        title: "The row of pins",
        category: Category::Layout,
        surface: Surface::Quickbar,
        body: "The quickbar holds whatever you pinned and nothing else. A pinned tool opens its \
               modal; a pinned panel docks and undocks from the grid. Things are pinned from the \
               Tools and Display menus in the header, each of which has a pin beside every row.",
        keywords: &[
            "quickbar",
            "pin",
            "unpin",
            "shortcut",
            "toolbar",
            "tools menu",
            "display menu",
        ],
    },
    Topic {
        id: "layout-panels",
        title: "Taking a panel off the grid",
        category: Category::Layout,
        surface: Surface::PanelVisibility,
        body: "Folding a panel says not right now; hiding one says not in this build, and the \
               grid closes over it. The Panels list is every surface with its state, and hiding \
               holds across the desktop grid and the mobile stack because it is a property of the \
               panel rather than of an arrangement.",
        keywords: &[
            "hide",
            "show",
            "panels",
            "visibility",
            "remove panel",
            "empty grid",
        ],
    },
    Topic {
        id: "layout-mobile",
        title: "On a phone",
        category: Category::Layout,
        surface: Surface::MobileStack,
        body: "Below 900px the grid becomes one column and the panels stack in an order of their \
               own, independent of where they sit on the desktop grid. Controls grow to a \
               finger-sized target on a touch pointer without the text changing size.",
        keywords: &[
            "mobile", "phone", "touch", "stack", "column", "narrow", "tablet",
        ],
    },
    Topic {
        id: "layout-reorder",
        title: "Reordering the stack",
        category: Category::Layout,
        surface: Surface::ReorderMenu,
        body: "Reorder, in the Display menu, is how the mobile stack is arranged: drag a row, or \
               move it with the up and down controls if a drag is awkward.",
        keywords: &[
            "reorder",
            "order",
            "stack",
            "move up",
            "move down",
            "drag",
            "arrange",
        ],
    },
    Topic {
        id: "layout-controls",
        title: "Every gesture in one sheet",
        category: Category::Layout,
        surface: Surface::Controls,
        body: "Controls is the interaction reference: each act the planner responds to, split \
               into the desktop and touch halves and the acts common to both. Searching here \
               reaches it too — a match from that sheet appears below the topics with a way \
               straight into it.",
        keywords: &[
            "controls",
            "keyboard",
            "mouse",
            "shortcut",
            "gesture",
            "click",
            "drag",
            "reference",
        ],
    },
    // ---- Files --------------------------------------------------------------------------
    Topic {
        id: "files-save-open",
        title: "Saving and opening a build",
        category: Category::Files,
        surface: Surface::BuildFile,
        body: "A build saves as a .skif file this planner reads back exactly, and Open file also \
               reads Mids' current .mbd builds, the older .mxd posts, and the .txt the game's own \
               /buildsave writes. Opening one authored on another fork is a dataset switch before \
               it is an import, so you are asked which fork to read it against rather than being \
               handed numbers its author would not recognise — except for a .mxd, which names no \
               fork at all and is read against whatever is loaded, with anything it could not \
               resolve on the receipt.",
        keywords: &[
            "save",
            "open",
            "load",
            "file",
            "skif",
            "mbd",
            "mxd",
            "mids",
            "buildsave",
            "import",
            "new build",
        ],
    },
    Topic {
        id: "files-handoff",
        title: "Two ways to hand a build over",
        category: Category::Files,
        surface: Surface::BuildHandoff,
        body: "To somebody who also runs Sidekick, send the .skif: it is plain text, so it \
               survives a chat window and pastes back in whole. To everybody else, the menu's \
               handoff group renders the build as something that does not need this planner to \
               read — a forum post, or a picture.",
        keywords: &[
            "share",
            "send",
            "give",
            "export",
            "handoff",
            "someone else",
            "friend",
        ],
    },
    Topic {
        id: "files-paste",
        title: "Pasting a build in",
        category: Category::Files,
        surface: Surface::PasteBuild,
        body: "Paste takes a build someone sent you — a share link, or the text of a .skif, a \
               Mids .mbd or .mxd, or a /buildsave pasted whole — and reads it in. A .mxd is the \
               whole forum post, data block and all: paste it as you found it. If it refuses, it \
               says what was wrong while the box is still open, rather than making you start \
               again.",
        keywords: &[
            "paste",
            "import",
            "link",
            "received",
            "clipboard",
            "receipt",
        ],
    },
    Topic {
        id: "files-forum",
        title: "Posting a build to a forum",
        category: Category::Files,
        surface: Surface::ForumExport,
        body: "The format that has meant \"a City of Heroes build\" on forums for years is the \
               Mids post, and this renders one: powers in level order, what is in each slot, and \
               what the slotting bought. The set bonuses in the post are the ones the build \
               actually receives, Rule of 5 included.",
        keywords: &[
            "forum", "post", "bbcode", "markdown", "reddit", "discord", "share", "mids",
        ],
    },
    Topic {
        id: "files-image",
        title: "A picture of the build",
        category: Category::Files,
        surface: Surface::ExportImage,
        body: "Exports the build as a PNG poster — the powers as tiles, the stats as bars, the \
               set bonuses listed — for somewhere that will not take text. The build itself rides \
               along inside the file's metadata, though nothing here reads one back out yet, so \
               send the .skif as well if the other person plans to open it.",
        keywords: &[
            "image",
            "png",
            "screenshot",
            "picture",
            "poster",
            "download",
            "export",
        ],
    },
    // ---- Settings -----------------------------------------------------------------------
    Topic {
        id: "settings-menu",
        title: "Where everything else is",
        category: Category::Settings,
        surface: Surface::MainMenu,
        body: "Three menus, split by what they are about. FILE, at the left of the header, holds \
               everything the app does to your build as a file, plus the account that can hold \
               one; inside it the order is by consequence, running from the acts that replace \
               your build wholesale to the ones that hand a copy to somebody. OPTIONS, at the \
               right, holds the preferences that change no build at all — theme, alerts, UI \
               scale and crash reporting. HELP, beside it, is the app talking about itself: this \
               guide, feedback, what changed, what this is, and how to support it. On a phone all \
               three live under Menu in the bar at the bottom. Anything not built yet is shown disabled \
               with its reason rather than left out.",
        keywords: &[
            "menu",
            "hamburger",
            "options",
            "settings",
            "file",
            "help",
            "where is",
            "disabled",
        ],
    },
    Topic {
        id: "settings-theme",
        title: "Themes",
        category: Category::Settings,
        surface: Surface::Theme,
        body: "The planner ships several themes and remembers the one you chose. It is purely \
               presentation: no number anywhere reads it.",
        keywords: &[
            "theme",
            "colour",
            "color",
            "dark",
            "light",
            "appearance",
            "skin",
        ],
    },
    Topic {
        id: "settings-scale",
        title: "Making the app bigger or smaller",
        category: Category::Settings,
        surface: Surface::UiScale,
        body: "UI scale grows or shrinks the whole interface in steps. It is chrome only — it \
               changes how much fits on screen, never what anything computes.",
        keywords: &[
            "scale",
            "zoom",
            "font size",
            "bigger",
            "smaller",
            "dense",
            "text size",
        ],
    },
    Topic {
        id: "settings-rule-of-five",
        title: "The Rule of 5 warning",
        category: Category::Settings,
        surface: Surface::RuleOfFive,
        body: "The game grants at most five instances of any one set bonus, and a sixth is simply \
               ignored — slots spent for nothing. The planner warns when a build crosses that \
               line. The banner's close button hides it until reload; the toggle in the menu \
               turns the warning off for good.",
        keywords: &[
            "rule of 5",
            "rule of five",
            "cap",
            "six",
            "wasted",
            "bonus",
            "warning",
            "alert",
        ],
    },
    Topic {
        id: "settings-undo",
        title: "Undoing a change",
        category: Category::Settings,
        surface: Surface::History,
        body: "Every edit to the build is one step, including the ones made in bulk: a drag that \
               adds six slots, or a tool that re-levels forty enhancements, each go back in one. \
               The keys and buttons that drive it are listed in Controls.",
        keywords: &[
            "undo", "redo", "back", "revert", "mistake", "history", "step",
        ],
    },
    Topic {
        id: "settings-about",
        title: "What this is and what changed",
        category: Category::Settings,
        surface: Surface::AppInfo,
        body: "About says what the planner is and where its data comes from; the changelog says \
               what this version shipped. Both are in the menu, beside this guide.",
        keywords: &[
            "about",
            "changelog",
            "version",
            "credits",
            "what's new",
            "release",
            "data source",
        ],
    },
];

/// How well `topic` answers one search term. Title beats keyword beats body, so a topic ABOUT
/// the word ranks above one that merely says it in passing.
fn score(topic: &Topic, term: &str) -> u32 {
    let mut total = 0;
    if topic.title.to_lowercase().contains(term) {
        total += 8;
    }
    if topic
        .keywords
        .iter()
        .any(|word| word.to_lowercase().contains(term))
    {
        total += 4;
    }
    if topic.body.to_lowercase().contains(term) {
        total += 1;
    }
    total
}

/// The topics matching `query`, best first; every topic in reading order when it is blank.
///
/// Terms are ANDed, because a second word is a user narrowing rather than widening. Ties keep
/// reading order — `sort_by_key` is stable — so an unhelpfully generic query still reads as the
/// guide rather than as a shuffle of it.
///
/// The order is load-bearing: a search renders the hits flat, in exactly this sequence. Grouping
/// them back under their shelves discards the ranking, which is what the first cut did.
pub fn search(query: &str) -> Vec<&'static Topic> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    if terms.is_empty() {
        return TOPICS.iter().collect();
    }
    let mut hits: Vec<(u32, &'static Topic)> = TOPICS
        .iter()
        .filter_map(|topic| {
            let mut total = 0;
            for term in &terms {
                match score(topic, term) {
                    0 => return None,
                    points => total += points,
                }
            }
            Some((total, topic))
        })
        .collect();
    hits.sort_by_key(|(points, _)| std::cmp::Reverse(*points));
    hits.into_iter().map(|(_, topic)| topic).collect()
}

/// The Controls sheet's rows matching `query` — the second half of what a search here reaches.
///
/// Read from [`crate::controls`] rather than copied into a topic, so the guide can point at a
/// gesture without becoming a second place that describes it.
fn controls_matching(query: &str) -> Vec<(&'static str, &'static crate::controls::Row)> {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_lowercase())
        .collect();
    if terms.is_empty() {
        return Vec::new();
    }
    crate::controls::reference_rows()
        .filter(|(section, row)| {
            terms.iter().all(|term| {
                section.to_lowercase().contains(term)
                    || row.action.to_lowercase().contains(term)
                    || row.description.to_lowercase().contains(term)
            })
        })
        .collect()
}

/// The one Help modal, hosted at the shell root like every other overlay — a `fixed` backdrop
/// rendered inside a grid surface would be contained by its `transform` (see [`crate::modal`]).
///
/// The host only gates; the body mounts and unmounts with `open`, so the query and the active
/// tab reset on every open rather than persisting a search nobody remembers running.
#[component]
pub fn HelpHost() -> Element {
    let mut open = use_context::<HelpOpen>().0;
    if !open() {
        return rsx! {};
    }
    rsx! {
        HelpBody { on_close: move |_| open.set(false) }
    }
}

#[component]
fn HelpBody(on_close: EventHandler<()>) -> Element {
    let mut query = use_signal(String::new);
    let mut active = use_signal(|| Option::<Category>::None);
    let mut controls_open = use_context::<crate::controls::ControlsOpen>().0;

    let text = query();
    let searching = text.split_whitespace().next().is_some();
    let hits = search(&text);
    let shown: Vec<&'static Topic> = hits
        .into_iter()
        .filter(|topic| active().is_none_or(|category| topic.category == category))
        .collect();
    let control_hits = controls_matching(&text);
    let found = shown.len() + control_hits.len();

    rsx! {
        Modal {
            title: "Help".to_string(),
            size: ModalSize::Xl,
            on_close,
            div { class: "help",
                // Search and tabs stick as one block. The guide is longer than a screen, and a
                // filter you have to scroll back up to change is a filter you stop using.
                div { class: "help__head",
                    div { class: "help__search",
                        input {
                            class: "help__query",
                            r#type: "search",
                            value: "{text}",
                            placeholder: "Search help",
                            "aria-label": "Search help topics",
                            oninput: move |evt| query.set(evt.value()),
                        }
                        if searching {
                            span { class: "help__count mono",
                                if found == 1 { "1 result" } else { "{found} results" }
                            }
                        }
                    }

                    div { class: "help__tabs", role: "group", "aria-label": "Help categories",
                        button {
                            class: if active().is_none() { "seg active" } else { "seg" },
                            r#type: "button",
                            "aria-pressed": active().is_none(),
                            onclick: move |_| active.set(None),
                            "All"
                        }
                        for category in Category::ALL {
                            button {
                                key: "{category.accent()}",
                                class: if active() == Some(category) { "seg active" } else { "seg" },
                                r#type: "button",
                                "aria-pressed": active() == Some(category),
                                onclick: move |_| active.set(Some(category)),
                                "{category.label()}"
                            }
                        }
                    }
                }

                if shown.is_empty() && control_hits.is_empty() {
                    p { class: "help__empty",
                        "Nothing here matches that. The Controls sheet covers the gestures, and the Discord in the footer covers the rest."
                    }
                }

                // Searching flattens the shelves. Regrouping ranked hits by category would put
                // the best match wherever its shelf happens to fall — the run that found this
                // had the exact-title hit rendering third, under two topics that merely say the
                // word — so the ranking is only visible as an order if the order is the ranking.
                if searching {
                    section { class: "help__section",
                        for topic in shown.iter().copied() {
                            details {
                                key: "{topic.id}",
                                class: "help__topic",
                                open: true,
                                summary { class: "help__topic-title",
                                    "{topic.title}"
                                    span { class: "help__topic-shelf", "{topic.category.label()}" }
                                }
                                p { class: "help__topic-body", "{topic.body}" }
                            }
                        }
                    }
                } else {
                    for category in Category::ALL {
                        {
                            let group: Vec<&'static Topic> = shown
                                .iter()
                                .copied()
                                .filter(|topic| topic.category == category)
                                .collect();
                            if group.is_empty() {
                                rsx! {}
                            } else {
                                rsx! {
                                    section { key: "{category.accent()}", class: "help__section",
                                        h3 { class: "help__title help__title--{category.accent()}", "{category.label()}" }
                                        for topic in group {
                                            details {
                                                key: "{topic.id}",
                                                class: "help__topic",
                                                summary { class: "help__topic-title", "{topic.title}" }
                                                p { class: "help__topic-body", "{topic.body}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                if !control_hits.is_empty() {
                    section { class: "help__section",
                        h3 { class: "help__title help__title--controls", "From the Controls sheet" }
                        ul { class: "help__controls",
                            for (section_title , row) in control_hits.iter().copied() {
                                li { key: "{section_title}-{row.action}", class: "help__control",
                                    span { class: "help__control-action", "{row.action}" }
                                    span { class: "help__control-dash", "—" }
                                    span { "{row.description}" }
                                }
                            }
                        }
                        button {
                            class: "seg",
                            r#type: "button",
                            onclick: move |_| {
                                controls_open.set(true);
                                on_close.call(());
                            },
                            "Open Controls"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether `needle` appears in `haystack` as a whole Rust identifier rather than as a
    /// fragment of a longer one. `Info` must not match inside `InfoPanel`.
    fn names_item(haystack: &str, needle: &str) -> bool {
        let ident = |c: char| c.is_alphanumeric() || c == '_';
        haystack.match_indices(needle).any(|(i, _)| {
            let before = haystack[..i].chars().next_back();
            let after = haystack[i + needle.len()..].chars().next();
            !before.is_some_and(ident) && !after.is_some_and(ident)
        })
    }

    /// The file a `module::path` names, as this crate lays modules out: `a::b` is `src/a/b.rs`,
    /// or `src/a/b/mod.rs` where the module has children.
    fn module_file(path: &[&str]) -> Option<std::path::PathBuf> {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let flat = src.join(format!("{}.rs", path.join("/")));
        if flat.is_file() {
            return Some(flat);
        }
        let nested = src.join(path.join("/")).join("mod.rs");
        nested.is_file().then_some(nested)
    }

    /// The rename half of this file's staleness mechanism, described in the module header since
    /// the file was written and never written until 2026-09-27. Every [`Surface::symbol`] names a
    /// real item in a real file of this crate, so renaming a module or a host component reds a
    /// help test — with a machine doing the grepping.
    #[test]
    fn every_surface_symbol_resolves() {
        let mut broken = Vec::new();
        for surface in Surface::ALL {
            let symbol = surface.symbol();
            let parts: Vec<&str> = symbol.split("::").collect();
            let (item, module) = match parts.split_last() {
                Some((item, module)) if !module.is_empty() => (*item, module),
                _ => {
                    broken.push(format!("{symbol}: not a `module::path::Item`"));
                    continue;
                }
            };
            let Some(file) = module_file(module) else {
                broken.push(format!(
                    "{symbol}: no file for module `{}`",
                    module.join("::")
                ));
                continue;
            };
            let src = std::fs::read_to_string(&file).expect("a module file this crate owns");
            if !names_item(&src, item) {
                broken.push(format!("{symbol}: `{item}` is not in {}", file.display()));
            }
        }
        assert!(
            broken.is_empty(),
            "help symbols that no longer resolve:\n  {}",
            broken.join("\n  ")
        );
    }

    /// The additive half, which no grep can see: a new panel renames nothing, so nothing goes
    /// red when it arrives with no topic. Also described in the header since the file was written
    /// and never written until 2026-09-27.
    #[test]
    fn help_covers_every_surface() {
        let uncovered: Vec<_> = Surface::ALL
            .into_iter()
            .filter(|surface| !TOPICS.iter().any(|topic| topic.surface == *surface))
            .map(|surface| format!("{surface:?} ({})", surface.symbol()))
            .collect();
        assert!(
            uncovered.is_empty(),
            "{} of {} surfaces have no help topic:\n  {}",
            uncovered.len(),
            Surface::ALL.len(),
            uncovered.join("\n  ")
        );
    }

    /// The Controls sheet is REACHED, not restated — the third claim the module header makes and
    /// the third it never proved.
    ///
    /// Two halves, because the sentence has two. Reached: a term that lives only in a controls
    /// row comes back from `controls_matching`, so search does run over both sources. Not
    /// restated: no topic body carries a row's description verbatim. The second half catches
    /// literal copying and nothing subtler, which is the honest limit — a paraphrase that drifts
    /// is a thing a reader catches and a string comparison cannot.
    #[test]
    fn help_reaches_the_controls_sheet_without_restating_it() {
        let rows: Vec<_> = crate::controls::reference_rows().collect();
        assert!(!rows.is_empty(), "the controls sheet has no rows to reach");

        let copied: Vec<_> = TOPICS
            .iter()
            .flat_map(|topic| {
                rows.iter()
                    .filter(move |(_, row)| topic.body.contains(row.description))
                    .map(move |(section, row)| {
                        format!("topic `{}` restates {section}/{}", topic.id, row.action)
                    })
            })
            .collect();
        assert!(copied.is_empty(), "{}", copied.join("\n  "));

        let (_, first) = rows[0];
        let hits = controls_matching(first.action);
        assert!(
            hits.iter().any(|(_, row)| row.action == first.action),
            "searching `{}` does not reach the controls sheet",
            first.action
        );
    }
}
